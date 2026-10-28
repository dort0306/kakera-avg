use kakera_avg::{
    content::{ContentSource, DirectorySource, validate_path},
    controller::{Intent, transition_with},
    loader::load_game,
    model::{HistoryEntry, Logic, MAX_HISTORY, Session, Story, VariableValue as V},
    save::{SaveStore, SlotId, Snapshot},
    script::ScriptLogic,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const STORY: &str = include_str!("fixtures/game/story.xml");
const SCRIPT: &str = include_str!("fixtures/game/story.js");
const ENDINGS: &str = include_str!("fixtures/game/endings.js");
struct Memory(BTreeMap<PathBuf, Vec<u8>>);
impl ContentSource for Memory {
    fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        validate_path(path)?;
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing {}", path.display()))
    }
}
fn source(story: &str, script: &str) -> Memory {
    Memory(
        [
            ("story.xml", story),
            ("ui.xml", include_str!("fixtures/game/ui.xml")),
            ("scripts/story.js", script),
            ("scripts/endings.js", ENDINGS),
        ]
        .into_iter()
        .map(|(path, text)| (PathBuf::from(path), text.as_bytes().to_vec()))
        .collect(),
    )
}
fn game(script: &str) -> (Story, ScriptLogic) {
    let (loaded, logic) = load_game(&source(STORY, script)).unwrap();
    (loaded.story, logic)
}
fn act(story: &Story, logic: &dyn Logic, session: &Session, choice: Option<&str>) -> Session {
    transition_with(
        story,
        session,
        choice.map_or(Intent::Advance, |id| Intent::Choose(id.into())),
        logic,
    )
    .unwrap()
    .session
}
fn walk(story: &Story, logic: &dyn Logic, session: &Session) -> Session {
    act(
        story,
        logic,
        &act(story, logic, session, Some("accept")),
        None,
    )
}

#[test]
fn real_quickjs_callbacks_unlock_choices_and_reach_three_endings() {
    let (story, logic) = game(SCRIPT);
    let question = act(&story, &logic, &story.start(), None);
    assert_eq!(
        logic.options(&story, &question).unwrap(),
        vec!["accept", "refuse"]
    );
    assert!(transition_with(&story, &question, Intent::Choose("invite".into()), &logic).is_err());
    let refused = act(
        &story,
        &logic,
        &act(&story, &logic, &question, Some("refuse")),
        None,
    );
    assert_eq!(refused.frame_id, "refuse");
    assert!(
        !transition_with(&story, &refused, Intent::Advance, &logic)
            .unwrap()
            .entered
    );
    let once = walk(&story, &logic, &question);
    let friendly = act(
        &story,
        &logic,
        &act(&story, &logic, &once, Some("refuse")),
        None,
    );
    assert_eq!(friendly.frame_id, "friendly");
    let twice = walk(&story, &logic, &once);
    assert_eq!(
        logic.options(&story, &twice).unwrap(),
        vec!["accept", "refuse", "invite"]
    );
    let accepted = act(
        &story,
        &logic,
        &act(&story, &logic, &twice, Some("invite")),
        None,
    );
    assert_eq!(accepted.frame_id, "accept");
    assert_eq!(accepted.variables["mood"], V::String("开心".into()));
    let capped = walk(&story, &logic, &twice);
    assert_eq!(
        logic.options(&story, &capped).unwrap(),
        vec!["refuse", "invite"]
    );
    assert_eq!(story.start().variables["trust"], V::Int(0));
}

#[test]
fn script_updates_fail_atomically_and_destination_guards_are_checked_before_commit() {
    for body in [
        "ctx.set('met',true);ctx.add('trust',4);return 'walk';",
        "ctx.set('met',true);ctx.add('trust',9223372036854775807n);ctx.add('trust',1);return 'walk';",
        "ctx.set('unknown',true);return 'walk';",
        "ctx.set('met',1);return 'walk';",
        "ctx.set('mood','x'.repeat(4097));return 'walk';",
        "ctx.add('trust',1);throw new Error('callback failed');",
        "ctx.add('trust',1);return 'missing';",
        "return Promise.resolve('walk');",
        "Promise.resolve().then(()=>ctx.add('trust',1));return 'walk';",
        "for(let i=0;i<129;i++)ctx.add('trust',0);return 'walk';",
        "ctx.vars.met=true;return 'walk';",
        "if(ctx.vars.trsut)return 'walk';return 'walk';",
    ] {
        let start = SCRIPT.find("  ctx.add").unwrap();
        let end = SCRIPT[start..].find("}, ctx =>").unwrap() + start;
        let code = format!("{}{}{}", &SCRIPT[..start], body, &SCRIPT[end..]);
        let (story, logic) = game(&code);
        let question = act(&story, &logic, &story.start(), None);
        let original = question.clone();
        assert!(
            transition_with(&story, &question, Intent::Choose("accept".into()), &logic).is_err(),
            "{body}"
        );
        assert_eq!(question, original);
    }
    // A guard error at the destination must also roll back the preceding advance.
    let code = SCRIPT.replace(
        "ctx => ctx.vars.trust < 3n",
        "ctx => {throw new Error('bad guard');}",
    );
    let (story, logic) = game(&code);
    assert!(transition_with(&story, &story.start(), Intent::Advance, &logic).is_err());
}

#[test]
fn registration_and_syntax_errors_identify_external_js_files() {
    for code in [
        SCRIPT.replace("on.end(\"accept\");", ""),
        format!("{SCRIPT}\non.end('missing');"),
        format!("{SCRIPT}\non.end('accept');"),
        SCRIPT.replace("\"invite\", ctx =>", "\"missing-option\", ctx =>"),
        SCRIPT.replace(
            "on.advance(\"opening\", () => \"question\");",
            "on.end('opening'); on.advance('opening',()=> 'question');",
        ),
        format!("{SCRIPT}\nfunction broken( {{"),
    ] {
        let error = load_game(&source(STORY, &code)).err().unwrap();
        let message = format!("{error:#}");
        assert!(
            message.contains("scripts/story.js") || message.contains("JS 缺少"),
            "{message}"
        );
        if !message.contains("缺少") {
            assert!(message.contains("scripts/story.js"), "{message}");
        }
    }
    let code = SCRIPT.replace("ctx => ctx.vars.trust < 3n", "ctx => 1");
    let (story, logic) = game(&code);
    let error = transition_with(&story, &story.start(), Intent::Advance, &logic)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("bool") && error.contains("scripts/story.js"),
        "{error}"
    );
}

#[test]
fn callbacks_are_isolated_and_guards_cannot_write_or_register() {
    let code = format!(
        "let counter=0;\n{}",
        SCRIPT.replace(
            "() => \"question\"",
            "() => (++counter === 1 ? 'question' : 'missing')"
        )
    );
    let (story, logic) = game(&code);
    let a = act(&story, &logic, &story.start(), None);
    let b = act(&story, &logic, &story.start(), None);
    assert_eq!(a, b);
    story
        .validate_session_with(&walk(&story, &logic, &a), &logic)
        .unwrap();
    let code = format!(
        "let seen=0;\n{}",
        SCRIPT
            .replace("ctx => ctx.vars.trust < 3n", "ctx => ++seen === 1")
            .replace(
                "ctx => ctx.vars.trust >= 2n && ctx.vars.met",
                "ctx => ++seen === 1"
            )
    );
    let (story, logic) = game(&code);
    let question = act(&story, &logic, &story.start(), None);
    let visible = logic.options(&story, &question).unwrap();
    assert_eq!(visible, vec!["accept", "refuse", "invite"]);
    for id in visible {
        assert!(transition_with(&story, &question, Intent::Choose(id), &logic).is_ok());
    }
    for guard in [
        "ctx => {ctx.add('trust',1);return true;}",
        "ctx => {on.end('opening');return true;}",
    ] {
        let code = SCRIPT.replace("ctx => ctx.vars.trust < 3n", guard);
        let (story, logic) = game(&code);
        assert!(transition_with(&story, &story.start(), Intent::Advance, &logic).is_err());
    }
}

#[test]
fn i64_values_cross_the_js_boundary_without_number_rounding() {
    let xml = STORY.replace(
        "initial=\"0\" min=\"0\" max=\"3\"",
        "initial=\"9007199254740993\"",
    );
    let code=SCRIPT.replace("() => \"question\"","ctx => {if(ctx.vars.trust !== 9007199254740993n)throw new Error('precision');ctx.add('trust',1);return 'question';}");
    let (loaded, logic) = load_game(&source(&xml, &code)).unwrap();
    let result = act(&loaded.story, &logic, &loaded.story.start(), None);
    assert_eq!(result.variables["trust"], V::Int(9_007_199_254_740_994));
    for value in [
        "9007199254740993",
        "1.5",
        "NaN",
        "Infinity",
        "9223372036854775808n",
    ] {
        let code = SCRIPT.replace(
            "() => \"question\"",
            &format!("ctx => {{ctx.set('trust',{value});return 'question';}}"),
        );
        let (loaded, logic) = load_game(&source(&xml, &code)).unwrap();
        assert!(
            transition_with(
                &loaded.story,
                &loaded.story.start(),
                Intent::Advance,
                &logic
            )
            .is_err()
        );
    }
}

#[test]
fn execution_limits_stop_loops_recursion_and_excessive_allocations() {
    for body in [
        "while(true){}",
        "function recurse(){return recurse()+1;}recurse();",
        "let data=[];while(true)data.push('x'.repeat(100000));",
    ] {
        let code = SCRIPT.replace("() => \"question\"", &format!("() => {{{body}}}"));
        let (story, logic) = game(&code);
        let start = Instant::now();
        let error = transition_with(&story, &story.start(), Intent::Advance, &logic)
            .err()
            .unwrap();
        assert!(start.elapsed() < Duration::from_secs(3), "{error}");
    }
    let start = Instant::now();
    assert!(load_game(&source(STORY, "while(true){}")).is_err());
    assert!(start.elapsed() < Duration::from_secs(3));
}

#[test]
fn callbacks_do_not_have_host_io_time_random_or_module_capabilities() {
    for body in [
        "Date.now();",
        "Math.random();",
        "require('fs');",
        "fetch('https://example.com');",
        "new Worker('x');",
    ] {
        let code = SCRIPT.replace(
            "() => \"question\"",
            &format!("() => {{{body}return 'question';}}"),
        );
        let (story, logic) = game(&code);
        assert!(transition_with(&story, &story.start(), Intent::Advance, &logic).is_err());
    }
}

#[test]
fn script_history_saves_restore_and_reject_inconsistent_state() {
    let (story, logic) = game(SCRIPT);
    let question = act(&story, &logic, &story.start(), None);
    let twice = walk(&story, &logic, &walk(&story, &logic, &question));
    let dir = tempfile::tempdir().unwrap();
    let slot = SlotId::new(1).unwrap();
    SaveStore::at(dir.path().into())
        .save_with(slot, &story, &twice, &logic)
        .unwrap();
    let restored = SaveStore::at(dir.path().into())
        .load_with(slot, &story, &logic)
        .unwrap();
    assert_eq!(restored, twice);
    let mut bad = Snapshot::capture_with(&story, &twice, &logic).unwrap();
    bad.variables.insert("trust".into(), V::Int(1));
    assert!(bad.restore_with(&story, &logic).is_err());
    let mut bad = Snapshot::capture_with(&story, &twice, &logic).unwrap();
    bad.history[1].choice_id = Some("invite".into());
    assert!(bad.restore_with(&story, &logic).is_err());
    let mut old = Snapshot::capture_with(&story, &twice, &logic).unwrap();
    old.content_version = 2;
    assert!(old.restore_with(&story, &logic).is_err());
}

#[test]
fn script_self_loop_advances_the_trimmed_history_checkpoint() {
    let xml = STORY.replace("max=\"3\"", "max=\"2000\"");
    let code = SCRIPT.replace(
        "() => \"question\"",
        "ctx => {ctx.add('trust',1);return 'opening';}",
    );
    let (loaded, logic) = load_game(&source(&xml, &code)).unwrap();
    let story = loaded.story;
    let mut session = story.start();
    session.history = vec![
        HistoryEntry {
            resume_to: None,
            frame_id: "opening".into(),
            choice_id: None
        };
        MAX_HISTORY
    ];
    session
        .variables
        .insert("trust".into(), V::Int((MAX_HISTORY - 1) as i64));
    let result = transition_with(&story, &session, Intent::Advance, &logic).unwrap();
    assert!(result.entered);
    assert_eq!(result.session.history.len(), MAX_HISTORY);
    assert_eq!(result.session.history_base_variables["trust"], V::Int(1));
    let snapshot = Snapshot::capture_with(&story, &result.session, &logic).unwrap();
    assert_eq!(
        snapshot.restore_with(&story, &logic).unwrap(),
        result.session
    );
}

#[test]
fn content_is_read_at_load_time_and_memory_source_needs_no_flow_or_real_paths() {
    let mut memory = source(STORY, SCRIPT);
    let (first, first_logic) = load_game(&memory).unwrap();
    memory.0.insert(
        PathBuf::from("scripts/story.js"),
        SCRIPT
            .replace("() => \"question\"", "() => 'refuse'")
            .into_bytes(),
    );
    let (second, second_logic) = load_game(&memory).unwrap();
    assert_eq!(
        act(&first.story, &first_logic, &first.story.start(), None).frame_id,
        "question"
    );
    assert_eq!(
        act(&second.story, &second_logic, &second.story.start(), None).frame_id,
        "refuse"
    );
    memory.0.insert(
        PathBuf::from("story.xml"),
        STORY.replace("午后的房间里", "清晨的房间里").into_bytes(),
    );
    let (third, _) = load_game(&memory).unwrap();
    assert!(third.story.frames["opening"].text.contains("清晨"));
    assert!(!memory.0.contains_key(Path::new("flow.xml")));
    let directory =
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap();
    let (disk, disk_logic) = load_game(&directory).unwrap();
    assert_eq!(
        act(&disk.story, &disk_logic, &disk.story.start(), None),
        act(&first.story, &first_logic, &first.story.start(), None)
    );
}

#[test]
fn content_paths_and_script_xml_are_strict() {
    for path in [
        "",
        "../secret",
        "D:/secret",
        "/secret",
        "assets/../../secret",
        "assets\\secret",
        "a:stream",
    ] {
        assert!(validate_path(Path::new(path)).is_err(), "{path}");
    }
    for (from, to) in [
        ("scripts/story.js", "../story.js"),
        ("scripts/story.js", "scripts/story.txt"),
        (
            "<script src=\"scripts/story.js\" />",
            "<script src=\"scripts/story.js\" unknown=\"x\" />",
        ),
        (
            "<script src=\"scripts/story.js\" />",
            "<script src=\"scripts/endings.js\" />",
        ),
        ("entry=\"opening\"", "entry=\"missing\""),
    ] {
        let error = load_game(&source(&STORY.replace(from, to), SCRIPT))
            .err()
            .unwrap();
        assert!(format!("{error:#}").contains("story.xml:"), "{error:#}");
    }
}

#[test]
fn every_runtime_asset_can_be_decoded_from_a_non_filesystem_source() {
    let directory =
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap();
    let mut memory = source(STORY, SCRIPT);
    let (loaded, _) = load_game(&memory).unwrap();
    for asset in loaded.story.assets.values() {
        memory
            .0
            .insert(asset.path.clone(), directory.read(&asset.path).unwrap());
    }
    kakera_avg::assets::PreparedAssets::load_from(&loaded.story, &memory).unwrap();
    let audio = kakera_avg::audio::PreparedAudio::load_from(&loaded.story, &memory).unwrap();
    assert_eq!(audio.len(), 3);
}
