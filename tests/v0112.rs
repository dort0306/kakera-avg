//! v0.11.2: image/panel normalization, JS composition tags and generic repeat paging.
use kakera_avg::{
    content::{ContentSource, validate_path},
    interface::{
        self, Data, Definition, Kind, data,
        host::{self, Command, Profile, State, UiValue},
        pagination,
    },
    loader::{self, LoadedStory},
    model::{Session, VariableValue, Variables},
    save::{SaveStore, SlotId},
    script::ScriptLogic,
    settings::Settings,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

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
const STORY_HEAD: &str = r#"<story schema="8" id="v0112" content-version="11" entry="opening">
  <scripts><script src="scripts/ui.js"/></scripts>
  <interface src="ui.xml"/>
  <assets><font id="font" src="font.otf"/><image id="art1" src="art1.png"/><image id="art2" src="art2.png"/><image id="art3" src="art3.png"/><image id="art4" src="art4.png"/><image id="art5" src="art5.png"/></assets>
  <characters/><visuals/>
  <frames><frame id="opening"><text>开场</text></frame></frames>
  <next end="true"/>
</story>"#;
fn source(ui: &str, script: &str) -> Game {
    Game(
        [
            ("story.xml", STORY_HEAD),
            ("ui.xml", ui),
            ("scripts/ui.js", script),
        ]
        .into_iter()
        .map(|(path, text)| (PathBuf::from(path), text.as_bytes().to_vec()))
        .collect(),
    )
}
fn load(ui: &str, script: &str) -> anyhow::Result<(LoadedStory, ScriptLogic)> {
    loader::load_game(&source(ui, script))
}
fn error(ui: &str, script: &str) -> String {
    let cause = match load(ui, script) {
        Ok(_) => panic!("invalid content must be rejected"),
        Err(cause) => cause,
    };
    kakera_avg::i18n::diagnostics(&format!("{cause:#}"))
}

const BASE_ROUTE: &str = "frame('opening').end();\n";

fn data_of(def: &Definition, story: &kakera_avg::model::Story, state: &State) -> Data {
    let session = story.start();
    let profile = Profile::new(&story.id, def);
    Data::build(data::Input {
        definition: def,
        story,
        session: &session,
        available: &[],
        state,
        profile: &profile,
        slots: &[],
        settings: &Settings::default(),
        complete: true,
        slot_pages: &[],
        autosaves: &[],
    })
}
fn find<'a>(nodes: &'a [interface::Node], id: &str) -> Option<&'a interface::Node> {
    for node in nodes {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find(&node.children, id) {
            return Some(found);
        }
    }
    None
}

const CARD_UI: &str = r#"<ui schema="3" initial="hud" width="1280" height="720">
  <variables><bool id="open" initial="true"/><string id="status" initial=""/><int id="page" initial="1" min="1" max="9"/></variables>
  <screen id="hud" modal="false" pause="false">
    <x-card id="help" x="40" y="60" width="640" height="400" title="帮助" expanded="ui.open">
      <button id="authorButton" width="120" height="32" text="作者按钮" event="go" />
    </x-card>
  </screen>
</ui>"#;

const CARD_JS: &str = "frame('opening').end();\ncomponent.register('x-card', {  props: {
    title: { type: 'string', required: true },
    expanded: { type: 'binding', valueType: 'bool', required: true }
  },
  expand({ props, children }) {
    return {
      tag: 'panel',
      attrs: { id: '$root', width: 600, height: 360 },
      children: [
        { tag: 'label', attrs: { id: 'title', width: 560, height: 40, text: props.title } },
        { tag: 'panel', attrs: { id: 'body', y: 50, width: 600, height: 310, visible: props.expanded }, children },
        { tag: 'button', attrs: { id: 'ownButton', x: 0, y: 320, width: 80, height: 32, text: '内部', event: 'go' } }
      ]
    };
  }
});
event.register('go', ctx => ctx.setUi('status', 'ok'));
";
#[test]
fn component_expands_with_instance_layout_local_ids_and_author_children() {
    let (loaded, logic) = load(CARD_UI, CARD_JS).unwrap();
    let def = &loaded.interface;
    let screen = &def.screens["hud"];
    let root = find(&screen.nodes, "help").expect("instance id maps to the expanded root");
    assert!(matches!(root.kind, Kind::Panel));
    assert_eq!(root.rect, [40.0, 60.0, 640.0, 400.0]);
    let title = find(&screen.nodes, "help.title").expect("generated local id is prefixed");
    assert_eq!(title.text, "帮助");
    let body = find(&screen.nodes, "help.body").unwrap();
    assert_eq!(body.visible, "ui.open");
    let authored = find(&screen.nodes, "help.authorButton").expect("author child keeps order");
    assert_eq!(authored.text, "作者按钮");
    let own = find(&screen.nodes, "help.ownButton").unwrap();
    assert_eq!(own.text, "内部");
    // Both the directly emitted event attribute and the independent definition resolve.
    assert_eq!(own.event.as_deref(), Some("go"));
    assert!(def.host_handlers.contains("go"));
    let _ = logic;
}

#[test]
fn component_bindings_may_target_expanded_nodes_by_full_id() {
    let script = format!(
        "{}event.register('go', ctx => ctx.setUi('status', 'bound'));\nui('help.ownButton').bind('go');",
        BASE_ROUTE
    );
    let js = CARD_JS
        .replacen("frame('opening').end();\n", "", 1)
        .replace(
            "event.register('go', ctx => ctx.setUi('status', 'ok'));",
            "",
        );
    let (loaded, logic) = load(CARD_UI, &format!("{js}{script}")).unwrap();
    let state = State::new(&loaded.interface);
    let session = loaded.story.start();
    let data = Data::build(data::Input {
        definition: &loaded.interface,
        story: &loaded.story,
        session: &session,
        available: &[],
        state: &state,
        profile: &Profile::new(&loaded.story.id, &loaded.interface),
        slots: &[],
        settings: &Settings::default(),
        complete: true,
        slot_pages: &[],
        autosaves: &[],
    });
    let key = interface::UiEventKey::ui("help.ownButton", "go");
    let commands = logic
        .ui_event(
            &interface::Event {
                sound: None,
                route: None,
                key,
                item: Value::Null,
                value: Value::Null,
            },
            &session,
            &state.variables,
            &data.values,
        )
        .unwrap();
    assert!(matches!(commands.first(), Some(Command::SetUi { name, .. }) if name == "status"));
}

#[test]
fn component_nesting_and_forward_references_are_supported() {
    let js = r#"frame('opening').end();
component.register('x-outer', {
  props: { label: { type: 'string', default: '外层' } },
  expand({ props, children }) {
    return { tag: 'panel', attrs: { id: '$root', width: 400, height: 300 }, children: [
      { tag: 'x-inner', attrs: { id: 'inner', width: 200, height: 100, text: props.label } },
      { tag: 'panel', attrs: { id: 'slot', width: 100, height: 100 }, children }
    ] };
  }
});
component.register('x-inner', {
  props: { text: { type: 'string', required: true } },
  expand({ props }) {
    return { tag: 'panel', attrs: { id: '$root', width: 200, height: 100 }, children: [
      { tag: 'label', attrs: { id: 'text', width: 180, height: 30, text: props.text } }
    ] };
  }
});
"#;
    let ui = r#"<ui schema="3" initial="hud"><variables><string id="status" initial=""/></variables>
      <screen id="hud" modal="false" pause="false">
        <x-outer id="nest"><label id="leaf" width="80" height="30" text="叶子" /></x-outer>
      </screen></ui>"#;
    let (loaded, _) = load(ui, js).unwrap();
    let nodes = &loaded.interface.screens["hud"].nodes;
    assert!(
        find(nodes, "nest.inner.text").is_some(),
        "nested component is prefixed twice"
    );
    assert_eq!(find(nodes, "nest.inner.text").unwrap().text, "外层");
    assert_eq!(find(nodes, "nest.leaf").unwrap().text, "叶子");
}

#[test]
fn invalid_component_contracts_are_rejected_with_instance_context() {
    let ui = r#"<ui schema="3" initial="hud"><variables><bool id="open" initial="true"/></variables>
      <screen id="hud" modal="false" pause="false"><x-card id="card" width="100" height="100" title="t" expanded="ui.open"><label id="inside" width="40" height="20" text="子" /></x-card></screen></ui>"#;
    // Dropping author children is refused.
    let drop = r#"frame('opening').end();
component.register('x-card', { props: { title: { type: 'string', required: true }, expanded: { type: 'binding', valueType: 'bool', required: true } },
  expand({ props }) { return { tag: 'panel', attrs: { id: '$root', width: 100, height: 100 } }; } });"#;
    let message = error(ui, drop);
    assert!(
        message.contains("x-card") || message.contains("作者子节点"),
        "{message}"
    );
    // Wrong root type is refused.
    let bad_root = r#"frame('opening').end();
component.register('x-card', { props: { title: { type: 'string', required: true }, expanded: { type: 'binding', valueType: 'bool', required: true } },
  expand({ props }) { return { tag: 'label', attrs: { id: '$root', width: 100, height: 100, text: 'x' } }; } });"#;
    let message = error(ui, bad_root);
    assert!(
        message.contains("panel") || message.contains("nine-patch"),
        "{message}"
    );
    // Unknown instance attribute is refused.
    let unknown = r#"frame('opening').end();
component.register('x-card', { props: { title: { type: 'string', required: true }, expanded: { type: 'binding', valueType: 'bool', required: true } },
  expand({ props, children }) { return { tag: 'panel', attrs: { id: '$root', width: 100, height: 100 }, children: [] }; } });"#;
    let with_extra = ui.replace(
        "expanded=\"ui.open\"",
        "expanded=\"ui.open\" colour=\"red\"",
    );
    let message = error(&with_extra, unknown);
    assert!(
        message.contains("colour") || message.contains("未声明"),
        "{message}"
    );
}

#[test]
fn duplicate_or_reserved_component_registrations_are_rejected() {
    let ui = r#"<ui schema="3" initial="hud"><screen id="hud" modal="false" pause="false"/></ui>"#;
    let duplicate = r#"frame('opening').end();
component.register('x-a', { props: {}, expand: () => ({tag:'panel', attrs:{id:'$root',width:10,height:10}}) });
component.register('x-a', { props: {}, expand: () => ({tag:'panel', attrs:{id:'$root',width:10,height:10}}) });"#;
    let message = error(ui, duplicate);
    assert!(
        message.contains("x-a") || message.contains("重复"),
        "{message}"
    );
    let reserved = r#"frame('opening').end();
component.register('panel', { props: {}, expand: () => ({tag:'panel', attrs:{id:'$root',width:10,height:10}}) });"#;
    let message = error(ui, reserved);
    assert!(
        message.contains("x-") || message.contains("组件标签"),
        "{message}"
    );
    let bad_prop = r#"frame('opening').end();
component.register('x-a', { props: { width: { type: 'string', default: 'x' } }, expand: () => ({tag:'panel', attrs:{id:'$root',width:10,height:10}}) });"#;
    let message = error(ui, bad_prop);
    assert!(
        message.contains("width") || message.contains("保留"),
        "{message}"
    );
}

#[test]
fn unused_component_and_empty_ui_are_valid() {
    let ui = r#"<ui schema="3" initial="hud"><screen id="hud" modal="false" pause="false"><label id="caption" width="80" height="30" text="说明"/></screen></ui>"#;
    let js = format!(
        "{BASE_ROUTE}component.register('x-unused', {{ props: {{}}, expand: () => ({{tag:'panel', attrs:{{id:'$root',width:10,height:10}}}}) }});"
    );
    assert!(load(ui, &js).is_ok());
}

#[test]
fn image_and_panel_share_one_internal_kind_but_keep_their_authoring_rules() {
    let ui = r##"<ui schema="3" initial="hud"><variables><string id="pic" initial="art1"/></variables>
      <screen id="hud" modal="false" pause="false">
        <image id="plain" width="100" height="100" image="art1" fit="contain" />
        <panel id="pictured" width="100" height="100" image-bind="ui.pic" />
        <panel id="colorOnly" width="100" height="100" color="#112233FF" />
      </screen></ui>"##;
    let (loaded, _) = load(ui, BASE_ROUTE).unwrap();
    let nodes = &loaded.interface.screens["hud"].nodes;
    for id in ["plain", "pictured", "colorOnly"] {
        assert!(matches!(find(nodes, id).unwrap().kind, Kind::Panel), "{id}");
    }
    // image keeps its stricter authoring contract.
    let child = ui.replace(
        "fit=\"contain\" />",
        "fit=\"contain\"><label id=\"inner\" width=\"10\" height=\"10\" text=\"x\"/></image>",
    );
    assert!(load(&child, BASE_ROUTE).is_err());
    let radius = ui.replace("fit=\"contain\" />", "fit=\"contain\" radius=\"8\" />");
    assert!(load(&radius, BASE_ROUTE).is_err());
    let missing = ui.replace(" image=\"art1\"", "");
    assert!(load(&missing, BASE_ROUTE).is_err());
}

const GALLERY_UI: &str = r#"<ui schema="3" initial="hud">
  <variables><int id="galleryPage" initial="1" min="1" max="3"/><string id="status" initial=""/></variables>
  <gallery>
    <cg id="cg1" image="art1" title="第一张"/>
    <cg id="cg2" image="art2" title="第二张"/>
    <cg id="cg3" image="art3" title="第三张"/>
    <cg id="cg4" image="art4" title="第四张"/>
    <cg id="cg5" image="art5" title="第五张"/>
  </gallery>
  <screen id="hud" modal="false" pause="false">
    <repeat id="galleryCards" x="40" y="100" width="900" height="300" source="gallery" columns="2" item-width="300" item-height="200" page-size="2" page-bind="ui.galleryPage">
      <label id="galleryCardTitle" width="280" height="32" bind="item.title" />
    </repeat>
    <label id="galleryPager" width="300" height="32" bind="repeat.galleryCards.pageText"/>
    <button id="prev" width="80" height="30" text="上一页" enabled="repeat.galleryCards.canPrev" event="prev"/>
    <button id="next" width="80" height="30" text="下一页" enabled="repeat.galleryCards.canNext" event="next"/>
  </screen>
</ui>"#;
const GALLERY_JS: &str = r#"frame('opening').end();
event.register('prev', ctx => { const p = ctx.repeats.galleryCards; if (p.canPrev) ctx.setUi('galleryPage', p.page - 1); });
event.register('next', ctx => { const p = ctx.repeats.galleryCards; if (p.canNext) ctx.setUi('galleryPage', p.page + 1); });"#;

#[test]
fn paginated_repeat_slices_each_page_and_exposes_metadata() {
    let (loaded, _) = load(GALLERY_UI, GALLERY_JS).unwrap();
    let def = &loaded.interface;
    let mut state = State::new(def);
    for (page, expected) in [
        (1, vec!["第一张", "第二张"]),
        (2, vec!["第三张", "第四张"]),
        (3, vec!["第五张"]),
    ] {
        state
            .variables
            .insert("galleryPage".into(), VariableValue::Int(page));
        let data = data_of(def, &loaded.story, &state);
        let rows = &data.lists["repeat:galleryCards"];
        assert_eq!(
            rows.iter()
                .map(|row| row["title"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected,
            "page {page}"
        );
        let meta = &data.values["repeat"]["galleryCards"];
        assert_eq!(meta["page"], json!(page));
        assert_eq!(meta["pages"], json!(3));
        assert_eq!(meta["pageSize"], json!(2));
        assert_eq!(meta["total"], json!(5));
        assert_eq!(meta["canPrev"], json!(page > 1));
        assert_eq!(meta["canNext"], json!(page < 3));
        assert!(meta["pageText"].as_str().unwrap().contains('/'));
        assert_eq!(meta["start"], json!((page - 1) * 2 + 1));
        assert_eq!(meta["end"], json!((page * 2).min(5)));
        assert!(data.pages.is_empty());
    }
    state
        .variables
        .insert("galleryPage".into(), VariableValue::Int(3));
    let data = data_of(def, &loaded.story, &state);
    assert!(
        data.values["repeat"]["galleryCards"]["pageText"]
            .as_str()
            .unwrap()
            .contains('3')
    );
}

#[test]
fn source_shrink_clamps_the_page_and_reports_normalization() {
    let (loaded, _) = load(GALLERY_UI, GALLERY_JS).unwrap();
    let def = &loaded.interface;
    let mut state = State::new(def);
    state
        .variables
        .insert("galleryPage".into(), VariableValue::Int(3));
    // Only one gallery entry exists once the others are removed from the definition.
    let mut short = def.clone();
    let gallery: BTreeMap<_, _> = def
        .gallery
        .iter()
        .take(1)
        .map(|(id, cg)| (id.clone(), cg.clone()))
        .collect();
    short.gallery = gallery;
    let data = data_of(&short, &loaded.story, &state);
    assert_eq!(data.values["repeat"]["galleryCards"]["page"], json!(1));
    assert_eq!(data.pages["galleryPage"], 1);
    assert_eq!(data.lists["repeat:galleryCards"].len(), 1);
}

#[test]
fn repeat_metadata_bindings_require_a_paginated_repeat() {
    let bogus = GALLERY_UI.replace(
        "bind=\"repeat.galleryCards.pageText\"",
        "bind=\"repeat.missing.pageText\"",
    );
    assert!(load(&bogus, GALLERY_JS).is_err());
    let plain = GALLERY_UI.replace(" page-size=\"2\" page-bind=\"ui.galleryPage\"", "");
    assert!(load(&plain, GALLERY_JS).is_err());
    let unknown_field = GALLERY_UI.replace(
        "bind=\"repeat.galleryCards.pageText\"",
        "bind=\"repeat.galleryCards.unknown\"",
    );
    assert!(load(&unknown_field, GALLERY_JS).is_err());
}

#[test]
fn pagination_declaration_errors_are_rejected() {
    let missing_bind = GALLERY_UI.replace(" page-bind=\"ui.galleryPage\"", "");
    assert!(load(&missing_bind, GALLERY_JS).is_err());
    let no_min = GALLERY_UI.replace(" min=\"1\"", "");
    assert!(load(&no_min, GALLERY_JS).is_err());
    let small_max = GALLERY_UI.replace("max=\"3\"", "max=\"2\"");
    assert!(load(&small_max, GALLERY_JS).is_err());
    let initial_high = GALLERY_UI
        .replace("initial=\"1\"", "initial=\"3\"")
        .replace("page-size=\"2\"", "page-size=\"3\"");
    assert!(load(&initial_high, GALLERY_JS).is_err());
    // Two repeats may not share one page variable.
    let shared = GALLERY_UI.replace(
        "</screen>",
        r#"<repeat id="secondGallery" width="300" height="200" source="gallery" columns="1" item-width="100" item-height="20" page-size="2" page-bind="ui.galleryPage"><label id="secondTitle" width="90" height="20" bind="item.title"/></repeat></screen>"#,
    );
    assert!(load(&shared, GALLERY_JS).is_err());
}

#[test]
fn explicit_page_writes_are_limited_to_the_current_page_count() {
    let (loaded, _) = load(GALLERY_UI, GALLERY_JS).unwrap();
    let def = &loaded.interface;
    let state = State::new(def);
    let profile = Profile::new(&loaded.story.id, def);
    let settings = Settings::default();
    let data = data_of(def, &loaded.story, &state);
    assert_eq!(data.page_limits["galleryPage"], 3);
    let prepare = |page: i64| {
        host::prepare_with_pages(
            def,
            &state,
            &settings,
            &profile,
            vec![Command::SetUi {
                name: "galleryPage".into(),
                value: UiValue::Int(page.to_string()),
            }],
            false,
            Some(&data.page_limits),
        )
    };
    assert!(prepare(3).is_ok());
    assert!(prepare(4).is_err());
    assert!(prepare(0).is_err());
}

#[test]
fn js_reads_repeat_metadata_as_a_snapshot() {
    let (loaded, logic) = load(GALLERY_UI, GALLERY_JS).unwrap();
    let def = &loaded.interface;
    let mut state = State::new(def);
    state
        .variables
        .insert("galleryPage".into(), VariableValue::Int(2));
    let data = data_of(def, &loaded.story, &state);
    let session = loaded.story.start();
    let commands = logic
        .ui_event(
            &interface::Event::ui("next", "next"),
            &session,
            &state.variables,
            &data.values,
        )
        .unwrap();
    assert!(
        matches!(commands.first(), Some(Command::SetUi { name, value })
            if name == "galleryPage" && matches!(value, UiValue::Int(v) if v == "3"))
    );
    let commands = logic
        .ui_event(
            &interface::Event::ui("prev", "prev"),
            &session,
            &state.variables,
            &data.values,
        )
        .unwrap();
    assert!(
        matches!(commands.first(), Some(Command::SetUi { name, value })
            if name == "galleryPage" && matches!(value, UiValue::Int(v) if v == "1"))
    );
}

#[test]
fn paginated_slots_read_only_the_requested_range() -> anyhow::Result<()> {
    let ui = r#"<ui schema="3" initial="hud">
      <variables><int id="slotPage" initial="20" min="1" max="20"/></variables>
      <screen id="hud" modal="false" pause="false">
        <repeat id="loadSlots" width="1000" height="400" source="slots" columns="2" item-width="400" item-height="120" page-size="10" page-bind="ui.slotPage">
          <label id="slotTitle" width="300" height="24" bind="item.title"/>
          <label id="slotPreview" width="360" height="60" bind="item.excerpt"/>
          <image id="slotImage" width="128" height="72" image-bind="item.image" fit="contain" visible="item.hasScreenshot"/>
        </repeat>
      </screen></ui>"#;
    let (loaded, logic) = load(ui, BASE_ROUTE).unwrap();
    let story = &loaded.story;
    let session = story.start();
    let dir = tempfile::tempdir()?;
    let saves = SaveStore::at(dir.path().to_owned());
    for number in 191..=200 {
        saves.save_with(SlotId::new(number)?, story, &session, &logic)?;
    }
    // A corrupt slot outside the requested page must not be read.
    std::fs::write(dir.path().join("slot-1.json"), b"{not json")?;
    let slots = saves.list_range(story, 191, 200);
    assert_eq!(slots.len(), 10);
    let mut state = State::new(&loaded.interface);
    let _ = state
        .variables
        .insert("slotPage".into(), VariableValue::Int(20));
    let data = Data::build(data::Input {
        definition: &loaded.interface,
        story,
        session: &session,
        available: &[],
        state: &state,
        profile: &Profile::new(&story.id, &loaded.interface),
        slots: &[],
        settings: &Settings::default(),
        complete: true,
        slot_pages: &[("loadSlots".into(), slots)],
        autosaves: &[],
    });
    let rows = &data.lists["repeat:loadSlots"];
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0]["slot"], json!(191));
    assert_eq!(rows[9]["slot"], json!(200));
    assert_eq!(data.values["repeat"]["loadSlots"]["total"], json!(200));
    assert_eq!(data.values["repeat"]["loadSlots"]["start"], json!(191));
    assert_eq!(data.values["repeat"]["loadSlots"]["end"], json!(200));
    assert_eq!(
        pagination::slot_requests(&loaded.interface, &state.variables),
        vec![("loadSlots".to_string(), 191, 200)]
    );
    Ok(())
}

#[test]
fn paginated_slots_do_not_touch_saves_page() {
    let ui = r#"<ui schema="3" initial="hud">
      <variables><int id="slotPage" initial="1" min="1" max="20"/></variables>
      <screen id="hud" modal="false" pause="false">
        <repeat id="loadSlots" width="400" height="200" source="slots" columns="1" item-width="100" item-height="20" page-size="10" page-bind="ui.slotPage">
          <label id="slotTitle" width="90" height="20" bind="item.title"/>
        </repeat>
      </screen></ui>"#;
    let (loaded, _) = load(ui, BASE_ROUTE).unwrap();
    let mut state = State::new(&loaded.interface);
    state.slot_page = 7;
    let data = data_of(&loaded.interface, &loaded.story, &state);
    assert_eq!(data.values["saves"]["page"], json!(7));
    assert_eq!(data.values["repeat"]["loadSlots"]["page"], json!(1));
    assert!(data.lists["repeat:loadSlots"].is_empty());
}

#[test]
fn page_variable_types_and_bounds_are_strict() {
    let wrong_type = GALLERY_UI.replace(
        "<int id=\"galleryPage\" initial=\"1\" min=\"1\" max=\"3\"/>",
        "<string id=\"galleryPage\" initial=\"1\"/>",
    );
    assert!(load(&wrong_type, GALLERY_JS).is_err());
    let wrong_bind = GALLERY_UI.replace("page-bind=\"ui.galleryPage\"", "page-bind=\"ui.other\"");
    assert!(load(&wrong_bind, GALLERY_JS).is_err());
    let page_size_zero = GALLERY_UI.replace("page-size=\"2\"", "page-size=\"0\"");
    assert!(load(&page_size_zero, GALLERY_JS).is_err());
    let page_size_odd = GALLERY_UI.replace("page-size=\"2\"", "page-size=\"2.5\"");
    assert!(load(&page_size_odd, GALLERY_JS).is_err());
}

#[test]
fn unused_helpers_stay_consistent() {
    let _: fn(&Variables) -> Value = data::values;
    let _ = Session::at("opening");
}
