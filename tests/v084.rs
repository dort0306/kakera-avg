use anyhow::Result;
use kakera_avg::{
    content::{ContentSource, DirectorySource},
    interface::{
        UiRoute,
        host::{self, Profile, State},
    },
    loader,
    package::{self, Archive, Snapshot, StagedProject},
    settings::Settings,
};
use std::{collections::BTreeMap, path::Path};
struct Memory(BTreeMap<String, Vec<u8>>);
impl ContentSource for Memory {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        self.0
            .get(path.to_str().unwrap())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing"))
    }
}
const STORY: &str = r#"<story schema="8" id="routes" content-version="8" entry="start"><interface src="ui.xml"/><scripts><script src="main.js"/></scripts><assets><font id="font" src="font.otf"/></assets><characters/><visuals/><frames><frame id="start"><text>开始</text></frame></frames><next end="true"/></story>"#;
fn source(route: &str) -> Memory {
    Memory([("story.xml",STORY.to_owned()),("main.js","on.end('start');on.ui('legacy',ctx=>ctx.close());".into()),("ui.xml",format!(r#"<ui schema="3" initial="hud"><screen id="hud" modal="false" pause="false"><button id="go" width="100" height="40" {route}/></screen><include src="ui/menu.xml"/></ui>"#)),("ui/menu.xml",r#"<ui schema="3"><screen id="menu"><button id="back" width="100" height="40" route="close"/></screen><screen id="other" modal="false" pause="false"/></ui>"#.into())].into_iter().map(|(p,s)|(p.into(),s.into_bytes())).collect())
}
#[test]
fn strict_routes_and_cross_file_targets() -> Result<()> {
    let (loaded, _) = loader::load_game(&source("route=\"open:menu\""))?;
    assert_eq!(
        loaded.interface.screens["hud"].nodes[0].route,
        Some(UiRoute::Open("menu".into()))
    );
    for route in [
        "route=\"\"",
        "route=\"open:\"",
        "route=\"open:missing\"",
        "route=\"jump:menu\"",
        "route=\"open: menu\"",
        "route=\"close\" event=\"legacy\"",
    ] {
        let err = loader::load_game(&source(route)).err().expect("reject");
        assert!(format!("{err:#}").contains("ui.xml:1:"), "{err:#}");
    }
    let mut s = source("route=\"open:menu\"");
    s.0.insert("ui/menu.xml".into(),br#"<ui schema="3"><screen id="menu"><button id="back" width="100" height="40" route="open:missing"/></screen></ui>"#.to_vec());
    assert!(format!("{:#}", loader::load_game(&s).err().unwrap()).starts_with("ui/menu.xml:1:"));
    Ok(())
}
#[test]
fn route_commands_use_existing_atomic_stack_rules() -> Result<()> {
    let (loaded, _) = loader::load_game(&source("route=\"open:menu\""))?;
    let d = &loaded.interface;
    let state = State::new(d);
    let settings = Settings::default();
    let profile = Profile::new("routes", d);
    let plan =
        |s: &State, r: UiRoute| host::prepare(d, s, &settings, &profile, vec![r.command()], false);
    let open = plan(&state, UiRoute::Open("menu".into()))?;
    assert_eq!(open.state.stack, ["hud", "menu"]);
    assert!(open.state.paused(d));
    assert!(plan(&open.state, UiRoute::Open("menu".into())).is_err());
    assert_eq!(open.state.stack, ["hud", "menu"]);
    assert_eq!(plan(&open.state, UiRoute::Close)?.state, state);
    assert!(plan(&state, UiRoute::Close).is_err());
    assert_eq!(
        plan(&open.state, UiRoute::Open("other".into()))?
            .state
            .stack,
        ["other"]
    );
    assert_eq!(state.stack, ["hud"]);
    Ok(())
}
#[test]
fn both_packages_keep_routes_and_flatten_ui() -> Result<()> {
    let root = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    for (path, bytes) in source("route=\"open:menu\"").0 {
        let p = root.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(p, bytes)?;
    }
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
        root.path().join("font.otf"),
    )?;
    let (snapshot, _) = Snapshot::directory(root.path())?.for_publication()?;
    let p = out.path().join("one.kpak");
    package::write(&p, &snapshot.encode(1)?, false)?;
    let mut stage = StagedProject::capture(root.path())?;
    stage.for_publication()?;
    let q = out.path().join("two.kpak");
    package::write_streaming(&stage, &q, 1, false, |a| {
        loader::load_game(a)?;
        Ok(())
    })?;
    for p in [p, q] {
        let a = Archive::open(&p)?;
        let (loaded, _) = loader::load_game(&a)?;
        assert_eq!(
            loaded.interface.screens["menu"].nodes[0].route,
            Some(UiRoute::Close)
        );
        assert!(a.read(Path::new("ui/menu.xml")).is_err());
    }
    assert!(
        String::from_utf8(DirectorySource::new(root.path())?.read(Path::new("ui.xml"))?)?
            .contains("include")
    );
    Ok(())
}
