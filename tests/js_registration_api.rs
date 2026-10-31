//! New `frame(id).advance/choice/end/enter/exit` + `ui(id).bind(event, callback)`
//! registration contract: routing, structured target validation and isolation.
use kakera_avg::{
    content::{ContentSource, validate_path},
    controller::{Intent, transition_with},
    interface::{
        Event, EventCategory, UiEventKey,
        host::{Command, UiValue},
    },
    loader::load_game,
    model::{Logic, Story, Variables},
    script::ScriptLogic,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const STORY: &str = r#"<story schema="8" id="js-api" content-version="11" entry="opening">
  <scripts><script src="scripts/routes.js"/><script src="scripts/ui.js"/></scripts>
  <interface src="ui.xml"/>
  <assets><font id="font" src="font.otf"/></assets><characters/><visuals/>
  <variables><int id="n" initial="0" min="0" max="9"/></variables>
  <frames>
    <frame id="opening"><text>开场。</text></frame>
    <frame id="question"><text>选择。</text><options><option id="go">前进</option><option id="stay">留下</option></options></frame>
    <frame id="walk"><text>行走。</text></frame>
    <frame id="ending"><text>结束。</text></frame>
  </frames>
  <next end="true"/>
</story>"#;

const UI: &str = r#"<ui schema="3" initial="hud">
  <variables><string id="status" initial=""/></variables>
  <screen id="hud" modal="false" pause="false" background-click="tap" background-right-click="menu">
    <scene id="scene" width="1280" height="720"/>
    <button id="advanceButton" width="120" height="40" text="继续" event="advance"/>
    <button id="stayButton" width="120" height="40" text="留下" event="stay"/>
    <button id="closeA" width="60" height="20" text="关闭" event="close"/>
    <button id="closeB" width="60" height="20" text="关闭" event="close"/>
    <repeat id="rows" width="300" height="120" source="choices" columns="1" item-width="100" item-height="20">
      <button id="rowButton" width="100" height="20" bind="item.text" event="choose"/>
    </repeat>
    <label id="caption" width="100" height="20" text="说明"/>
  </screen>
  <screen id="panel" modal="true"><label id="panelText" width="100" height="20" text="面板"/></screen>
</ui>"#;

const ROUTES: &str = r#"frame('opening').advance('question');
frame('question').choice('go', 'walk');
frame('question').choice('stay', ctx => { ctx.add('n', 1n); return 'ending'; }, ctx => ctx.vars.n < 3n);
frame('walk').advance(() => 'ending');
frame('ending').end();"#;

const HANDLERS: &str = r#"function closePage(ctx) { ctx.close(); }
ui('advanceButton').bind('advance', ctx => ctx.advance());
ui('stayButton').bind('stay', ctx => ctx.choose('stay'));
ui('closeA').bind('close', closePage);
ui('closeB').bind('close', ctx => ctx.setUi('status', 'b'));
ui('rowButton').bind('choose', ctx => ctx.setUi('status', ctx.item.text));
ui('hud').bind('tap', ctx => ctx.setUi('status', 'tap'));
ui('hud').bind('menu', ctx => ctx.setUi('status', 'menu'));
ui('panel').bind('screen.open', ctx => ctx.setUi('status', 'opened'));
ui('panel').bind('screen.close', ctx => ctx.setUi('status', 'closed'));
ui('$game').bind('game.enter', ctx => ctx.setUi('status', ctx.game.frameId));
ui('$host').bind('shortcut-captured', ctx => ctx.setUi('status', ctx.value));"#;

struct Game(BTreeMap<PathBuf, Vec<u8>>);
impl ContentSource for Game {
    fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        validate_path(path)?;
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing {}", path.display()))
    }
}
fn source_with(ui: &str, routes: &str, handlers: &str) -> Game {
    Game(
        [
            ("story.xml", STORY),
            ("ui.xml", ui),
            ("scripts/routes.js", routes),
            ("scripts/ui.js", handlers),
        ]
        .into_iter()
        .map(|(path, text)| (PathBuf::from(path), text.as_bytes().to_vec()))
        .collect(),
    )
}
fn source(routes: &str, handlers: &str) -> Game {
    source_with(UI, routes, handlers)
}
fn load_ok(routes: &str, handlers: &str) -> (Story, ScriptLogic) {
    load_game(&source(routes, handlers))
        .map(|(loaded, logic)| (loaded.story, logic))
        .unwrap()
}
fn failure(routes: &str, handlers: &str) -> String {
    format!(
        "{:#}",
        load_game(&source(routes, handlers))
            .err()
            .expect("must fail")
    )
    .replace(['\u{2068}', '\u{2069}'], "")
}
fn routes_failure(routes: &str) -> String {
    failure(routes, HANDLERS)
}
fn game(routes: &str) -> (Story, ScriptLogic) {
    load_ok(routes, HANDLERS)
}
fn variables() -> Variables {
    game(ROUTES).0.start().variables
}
fn dispatch(
    logic: &ScriptLogic,
    story: &Story,
    target: &str,
    event: &str,
    item: serde_json::Value,
) -> Vec<Command> {
    let mut dispatched = Event::ui(target, event);
    dispatched.item = item;
    logic
        .ui_event(
            &dispatched,
            &story.start(),
            &variables(),
            &json!({"game":{}}),
        )
        .unwrap()
}
fn dispatched_text(commands: &[Command]) -> &str {
    match commands {
        [
            Command::SetUi {
                value: UiValue::String(value),
                ..
            },
        ] => value,
        other => panic!("unexpected commands {other:?}"),
    }
}

#[test]
fn new_entries_route_both_target_forms_and_never_exit_the_host() {
    let (story, logic) = game(ROUTES);
    let question = transition_with(&story, &story.start(), Intent::Advance, &logic)
        .unwrap()
        .session;
    assert_eq!(question.frame_id, "question");
    assert_eq!(
        logic.options(&story, &question).unwrap(),
        vec!["go".to_owned(), "stay".to_owned()]
    );
    // A plain string target needs no variable update and still validates normally.
    let walk = transition_with(&story, &question, Intent::Choose("go".into()), &logic)
        .unwrap()
        .session;
    assert_eq!(walk.frame_id, "walk");
    let ending = transition_with(&story, &walk, Intent::Advance, &logic)
        .unwrap()
        .session;
    assert_eq!(ending.frame_id, "ending");
    // `end()` only declares an End route; it never quits the process.
    assert!(
        !transition_with(&story, &ending, Intent::Advance, &logic)
            .unwrap()
            .entered
    );
    // A function target with a guard applies its update plan and can be replayed.
    let stayed = transition_with(&story, &question, Intent::Choose("stay".into()), &logic)
        .unwrap()
        .session;
    assert_eq!(stayed.frame_id, "ending");
    assert_eq!(stayed.variables["n"].to_string(), "1");
    // The XML option set is still authoritative: a hidden/unknown id cannot be chosen.
    assert!(transition_with(&story, &question, Intent::Choose("missing".into()), &logic).is_err());
}

#[test]
fn static_and_function_targets_are_semantically_equivalent() {
    let functions = ROUTES
        .replace("advance('question')", "advance(() => 'question')")
        .replace("choice('go', 'walk')", "choice('go', () => 'walk')");
    let (plain_story, plain_logic) = game(ROUTES);
    let (function_story, function_logic) = game(&functions);
    assert_eq!(plain_story.routes, function_story.routes);
    let mut plain = plain_story.start();
    let mut function = function_story.start();
    for intent in [Intent::Advance, Intent::Choose("go".into())] {
        let a = transition_with(&plain_story, &plain, intent.clone(), &plain_logic).unwrap();
        let b = transition_with(&function_story, &function, intent, &function_logic).unwrap();
        assert_eq!(a.session, b.session);
        assert_eq!(a.entered, b.entered);
        assert_eq!(a.session.history, b.session.history);
        plain = a.session;
        function = b.session;
    }
    assert_eq!(plain.frame_id, "walk");
    assert_eq!(function.frame_id, "walk");
}

#[test]
fn obsolete_global_registration_entries_are_rejected_with_the_author_file() {
    for line in [
        "on.advance('opening', () => 'question');",
        "on.choice('question', 'go', () => 'walk');",
        "on.end('ending');",
        "on.ui('advance', ctx => ctx.advance());",
        "on.frameEnter('opening', ctx => ctx.close());",
        "on.frameExit('opening', ctx => ctx.close());",
        "on.screenOpen('panel', ctx => ctx.close());",
        "on.screenClose('panel', ctx => ctx.close());",
    ] {
        for routes in [format!("{ROUTES}\n{line}"), format!("{line}\n{ROUTES}")] {
            let message = routes_failure(&routes);
            assert!(message.contains("routes.js"), "{line}: {message}");
        }
    }
}

#[test]
fn selector_targets_arguments_and_routes_are_strict() {
    for body in [
        "frame('');",
        "frame(1);",
        "frame(null);",
        "ui('');",
        "frame('opening').advance(1);",
        "frame('opening').advance(null);",
        "frame('opening').advance([]);",
        "frame('opening').advance(new String('question'));",
        "frame('opening').advance('');",
        "frame('question').choice('go', 'walk', true);",
        "frame('question').choice('go', 3);",
        "frame('opening').advance('question');frame('opening').advance('question');",
        "frame('opening').advance('question');frame('opening').end();",
        "frame('question').choice('go', 'walk');frame('question').choice('go', 'walk');",
        "frame('opening').advance('ghost');",
        "frame('opening').advance('question');frame('question').choice('extra', 'walk');",
    ] {
        let message = routes_failure(&format!("{ROUTES}\n{body}"));
        assert!(message.contains("routes.js"), "{body}: {message}");
    }
    // A frame selector alone registers nothing; the declared routes stay authoritative.
    let (story, logic) = game(&format!("{ROUTES}\nframe('opening');frame('opening');"));
    assert_eq!(story.routes["opening"], kakera_avg::model::Route::Advance);
    assert_eq!(
        transition_with(&story, &story.start(), Intent::Advance, &logic)
            .unwrap()
            .session
            .frame_id,
        "question"
    );
}

#[test]
fn callbacks_cannot_register_and_globals_stay_frozen() {
    let registering = ROUTES.replace(
        "frame('question').choice('go', 'walk');",
        "frame('question').choice('go', ctx => { frame('walk').end(); return 'walk'; });",
    );
    let (story, logic) = game(&registering);
    let question = transition_with(&story, &story.start(), Intent::Advance, &logic)
        .unwrap()
        .session;
    let error = transition_with(&story, &question, Intent::Choose("go".into()), &logic)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("回调执行期间不能注册触发器"), "{error}");
    for body in [
        "globalThis.frame = 1;",
        "globalThis.ui = 1;",
        "Object.defineProperty(globalThis, 'frame', {value: 1});",
        "const replaced = frame('opening'); replaced.advance = 1;",
    ] {
        let message = routes_failure(&format!("{ROUTES}\n{body}"));
        assert!(message.contains("routes.js"), "{body}: {message}");
    }
    // A destructured selector method keeps pointing at its own target.
    let destructured = format!(
        "{}\nconst {{ advance }} = frame('walk');\nadvance(() => 'ending');",
        ROUTES.replace("frame('walk').advance(() => 'ending');", "")
    );
    let (story, logic) = game(&destructured);
    let question = transition_with(&story, &story.start(), Intent::Advance, &logic)
        .unwrap()
        .session;
    let walk = transition_with(&story, &question, Intent::Choose("go".into()), &logic)
        .unwrap()
        .session;
    assert_eq!(walk.frame_id, "walk");
    assert_eq!(
        transition_with(&story, &walk, Intent::Advance, &logic)
            .unwrap()
            .session
            .frame_id,
        "ending"
    );
}

#[test]
fn ui_bindings_are_structured_keys_with_exact_xml_targets() {
    let (story, logic) = game(ROUTES);
    let row = dispatch(
        &logic,
        &story,
        "rowButton",
        "choose",
        json!({"text":"前进"}),
    );
    assert_eq!(dispatched_text(&row), "前进");
    // Same event name, different targets: each target keeps its own callback.
    assert!(matches!(
        dispatch(&logic, &story, "closeA", "close", json!(null)).as_slice(),
        [Command::Close]
    ));
    assert_eq!(
        dispatched_text(&dispatch(&logic, &story, "closeB", "close", json!(null))),
        "b"
    );
    // Screen background callbacks resolve through the same `(target, event)` key.
    assert_eq!(
        dispatched_text(&dispatch(&logic, &story, "hud", "tap", json!(null))),
        "tap"
    );
    // A repeat row passes its real item, never the template's first row.
    assert_eq!(
        dispatched_text(&dispatch(
            &logic,
            &story,
            "rowButton",
            "choose",
            json!({"text":"留下"})
        )),
        "留下"
    );
}

#[test]
fn ui_registration_targets_and_lifecycle_events_are_validated() {
    let extra = |line: &str| format!("{HANDLERS}\n{line}");
    for (handlers, expected) in [
        (
            extra("ui('closeA').bind('close', ctx => ctx.close());"),
            "重复 UI 回调",
        ),
        (
            extra("ui('ghost').bind('close', ctx => ctx.close());"),
            "JS 注册目标不存在",
        ),
        (
            extra("ui('caption').bind('close', ctx => ctx.close());"),
            "JS 注册目标或事件与 XML 不一致",
        ),
        (
            extra("ui('advanceButton').bind('click', ctx => ctx.close());"),
            "JS 注册目标或事件与 XML 不一致",
        ),
        (
            extra("ui('panel').bind('screen.open', ctx => ctx.close());"),
            "重复 UI 回调",
        ),
        (
            extra("ui('advanceButton').bind('screen.open', ctx => ctx.close());"),
            "只能注册在页面目标上",
        ),
        (
            extra("ui('$game').bind('close', ctx => ctx.close());"),
            "game.enter",
        ),
        (
            extra("ui('$host').bind('game.enter', ctx => ctx.close());"),
            "保留事件名",
        ),
        (
            extra("ui('closeA').bind('frame.enter:opening', ctx => ctx.close());"),
            "保留事件名",
        ),
        (
            HANDLERS.replace(
                "ui('closeB').bind('close', ctx => ctx.setUi('status', 'b'));",
                "",
            ),
            "XML 事件缺少唯一注册",
        ),
    ] {
        let message = failure(ROUTES, &handlers);
        assert!(message.contains(expected), "{expected} not in: {message}");
    }
}

#[test]
fn reserved_virtual_ids_and_cross_namespace_collisions_are_rejected() {
    for (ui, expected) in [
        (
            UI.replace(
                r#"<screen id="panel" modal="true">"#,
                r#"<screen id="panel" modal="true"><button id="$game" width="10" height="10" text="假" event="close"/>"#,
            ),
            "固定虚拟目标",
        ),
        (
            UI.replace(r#"<screen id="panel" modal="true">"#, r#"<screen id="closeA" modal="true">"#),
            "页面与控件不能共用同一 ID",
        ),
    ] {
        let message = format!(
            "{:#}",
            load_game(&source_with(&ui, ROUTES, HANDLERS))
                .err()
                .expect("must fail")
        )
        .replace(['\u{2068}', '\u{2069}'], "");
        assert!(message.contains(expected), "{expected} not in: {message}");
    }
}

#[test]
fn lifecycle_and_virtual_targets_are_explicit_categories() {
    let routes = format!(
        "{}\nframe('opening').enter(ctx => ctx.setUi('status', 'enter'));\nframe('walk').exit(ctx => ctx.close());",
        ROUTES
    );
    let decoded = load_game(&source(&routes, HANDLERS)).unwrap();
    let definition = &decoded.0.interface;
    for slot in [
        "frame.enter:opening",
        "frame.exit:walk",
        "screen.open:panel",
        "screen.close:panel",
    ] {
        assert!(definition.lifecycle.contains_key(slot), "{slot}");
    }
    assert!(definition.enter);
    assert!(definition.host_handlers.contains("shortcut-captured"));
    let enter = definition.lifecycle["frame.enter:opening"].clone();
    assert_eq!(enter.category, EventCategory::FrameEnter);
    assert_eq!(enter.target, "opening");
    assert!(enter.event.is_empty());
    let screen = definition.lifecycle["screen.open:panel"].clone();
    assert_eq!(screen.category, EventCategory::Ui);
    assert_eq!(screen.event, "screen.open");
    assert_eq!(UiEventKey::game().category, EventCategory::Game);
    // A lifecycle callback keeps its UI command context: story variable updates fail.
    let forbidden = format!(
        "{}\nframe('opening').enter(ctx => ctx.add('n', 1n));",
        ROUTES
    );
    let (story, logic) = load_ok(&forbidden, HANDLERS);
    let definition = load_game(&source(&forbidden, HANDLERS))
        .unwrap()
        .0
        .interface;
    let error = logic
        .ui_event(
            &Event {
                sound: None,
                route: None,
                key: definition.lifecycle["frame.enter:opening"].clone(),
                item: serde_json::Value::Null,
                value: serde_json::Value::Null,
            },
            &story.start(),
            &variables(),
            &json!({}),
        )
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("scripts/routes.js"),
        "{error:#}"
    );
}

#[test]
fn ids_with_separators_never_collide_in_the_registration_key() {
    let ui = UI.replace(
        r#"<button id="advanceButton" width="120" height="40" text="继续" event="advance"/>"#,
        r#"<button id="advanceButton" width="120" height="40" text="继续" event="advance"/><button id="a" width="10" height="10" text="甲" event="b:c"/><button id="a:b" width="10" height="10" text="乙" event="c"/>"#,
    );
    let handlers = HANDLERS.replace(
        "ui('advanceButton').bind('advance', ctx => ctx.advance());",
        "ui('a').bind('b:c', ctx => ctx.setUi('status', 'first'));\nui('a:b').bind('c', ctx => ctx.setUi('status', 'second'));\nui('advanceButton').bind('advance', ctx => ctx.advance());",
    );
    let (story, logic) = load_game(&source_with(&ui, ROUTES, &handlers))
        .map(|(loaded, logic)| (loaded.story, logic))
        .unwrap();
    for (target, event, expected) in [("a", "b:c", "first"), ("a:b", "c", "second")] {
        assert_eq!(
            dispatched_text(&dispatch(&logic, &story, target, event, json!(null))),
            expected,
            "{target}/{event}"
        );
    }
}
