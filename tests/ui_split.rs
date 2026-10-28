use anyhow::Result;
use kakera_avg::{
    content::{ContentSource, DirectorySource},
    interface::{
        self,
        host::{self, Profile, State},
    },
    loader,
    package::{self, Archive, Snapshot, StagedProject},
    runtime::{Bundle, LoadOptions},
    settings::Settings,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Memory(BTreeMap<String, Vec<u8>>);
impl ContentSource for Memory {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        kakera_avg::content::validate_path(path)?;
        self.0
            .get(path.to_str().unwrap())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing {}", path.display()))
    }
}
const STORY: &str = r#"<story schema="8" id="ui-split" content-version="8" entry="start"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/><image id="bg" src="bg.png"/></assets><characters/><visuals/><frames><frame id="start"><text>开始。</text></frame></frames><next end="true"/></story>"#;
const ROOT: &str = r#"<ui schema="3" initial="hud" title-bind="ui.title">
  <include src="ui/pages.xml"/>
  <include src="ui/state.xml"/>
  <gallery><cg id="room" image="bg" title="房间"/></gallery>
</ui>"#;
const HUD: &str = r#"<ui schema="3">
  <!-- A complete page, with bindings declared in another file. -->
  <screen id="hud" modal="false" pause="false">
    <scene id="scene" width="1280" height="720"/>
    <label id="line" width="700" height="50" bind="ui.message"/>
    <button id="edit" width="100" height="40" text="修改 &amp; 标题" event="edit"/>
  </screen>
</ui>"#;
const STATE: &str = r#"<ui schema="3">
  <variables>
    <string id="title" initial="中文 &amp; Game"/>
    <string id="message" initial="分幕 &lt;提示&gt;"/>
  </variables>
  <gallery><cg id="extra" image="bg" title="另页"/></gallery>
</ui>"#;
fn source() -> Memory {
    Memory([
        ("story.xml", STORY), ("ui.xml", ROOT), ("ui/hud.xml", HUD), ("ui/state.xml", STATE),
        ("ui/pages.xml", r#"<ui schema="3"><include src="ui/hud.xml"/><screen id="settings"><button id="close" width="100" height="40" text="关闭" event="close"/></screen></ui>"#),
        ("story.js", "on.end('start');on.ui('edit',ctx=>ctx.setUi('title',ctx.value));on.ui('close',ctx=>ctx.close());"),
    ].into_iter().map(|(p,s)|(p.into(),s.as_bytes().to_vec())).collect())
}
fn replace(source: &mut Memory, path: &str, value: &str) {
    source.0.insert(path.into(), value.as_bytes().to_vec());
}
fn failure(source: &Memory) -> String {
    format!(
        "{:#}",
        loader::load_game(source).err().expect("must reject")
    )
}
fn write_game(root: &Path, memory: &Memory) -> Result<()> {
    for (path, bytes) in &memory.0 {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap())?;
        std::fs::write(file, bytes)?;
    }
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
        root.join("font.otf"),
    )?;
    image::RgbaImage::new(4, 4).save(root.join("bg.png"))?;
    Ok(())
}
fn options() -> LoadOptions {
    LoadOptions {
        core: None,
        no_live2d: true,
    }
}

#[test]
fn nested_includes_resolve_global_references_locations_and_real_js() -> Result<()> {
    let source = source();
    let compiled = interface::source::compile(&source, Path::new("ui.xml"))?;
    assert_eq!(
        compiled.files,
        ["ui.xml", "ui/pages.xml", "ui/hud.xml", "ui/state.xml"]
    );
    assert!(!compiled.xml.contains("<include"));
    let (loaded, logic) = loader::load_game(&source)?;
    let definition = &loaded.interface;
    assert_eq!(definition.screens.len(), 2);
    assert_eq!(definition.gallery.len(), 2);
    assert_eq!(
        definition.screens["hud"].nodes[1].location,
        "ui/hud.xml:5:5"
    );
    assert!(definition.text().contains("分幕 <提示>"));
    let state = State::new(definition);
    assert_eq!(state.window_title, "中文 & Game");
    let commands = logic.ui_event(
        &interface::Event {
            sound: None,
            route: None,
            name: "edit".into(),
            item: json!(null),
            value: json!("新的分幕标题"),
        },
        &loaded.story.start(),
        &state.variables,
        &json!({}),
    )?;
    let plan = host::prepare(
        definition,
        &state,
        &Settings::default(),
        &Profile::new("ui-split", definition),
        commands,
        false,
    )?;
    assert_eq!(plan.state.window_title, "新的分幕标题");
    let mut flat = source.clone();
    for file in compiled.files.iter().skip(1) {
        flat.0.remove(file);
    }
    replace(&mut flat, "ui.xml", &compiled.xml);
    let (published, _) = loader::load_game(&flat)?;
    assert_eq!(published.interface.text(), definition.text());
    assert_eq!(published.interface.events(), definition.events());
    assert_eq!(
        interface::source::compile(&flat, Path::new("ui.xml"))?.xml,
        compiled.xml
    );
    Ok(())
}

#[test]
fn invalid_cross_file_ui_keeps_author_file_and_line_diagnostics() {
    for (file, original, changed, position) in [
        ("ui/hud.xml", HUD, "bind=\"ui.message\"", "ui/hud.xml:5:5:"),
        (
            "ui/state.xml",
            STATE,
            "initial=\"分幕 &lt;提示&gt;\"",
            "ui/state.xml:4:5:",
        ),
    ] {
        let mut s = source();
        replace(&mut s, file, &original.replace(changed, "unknown=\"bad\""));
        let error = failure(&s);
        assert!(error.starts_with(position), "{error}");
    }
    for (file, text, detail) in [
        (
            "ui/pages.xml",
            r#"<ui schema="3"><include src="ui/hud.xml"/><screen id="hud"/></ui>"#,
            "重复屏幕",
        ),
        (
            "ui/pages.xml",
            r#"<ui schema="3"><include src="ui/hud.xml"/><screen id="settings"><label id="line" width="10" height="10"/></screen></ui>"#,
            "重复 UI ID",
        ),
        (
            "ui/state.xml",
            r#"<ui schema="3"><variables><string id="title" initial="x"/><string id="title" initial="y"/></variables></ui>"#,
            "重复 ID",
        ),
        (
            "ui/state.xml",
            r#"<ui schema="3"><variables/><variables/></ui>"#,
            "重复 <variables>",
        ),
        (
            "ui/state.xml",
            r#"<ui schema="3"><gallery/><gallery/></ui>"#,
            "重复 <gallery>",
        ),
        (
            "ui/state.xml",
            r#"<ui schema="3"><gallery><cg id="room" image="bg" title="重复"/></gallery></ui>"#,
            "重复 CG",
        ),
    ] {
        let mut s = source();
        replace(&mut s, file, text);
        let error = failure(&s);
        let location = if detail == "重复 CG" {
            "ui.xml:"
        } else {
            file
        };
        assert!(
            error.contains(location) && error.contains(detail),
            "{error}"
        );
    }
    let mut s = source();
    replace(
        &mut s,
        "ui/hud.xml",
        &HUD.replace("bind=\"ui.message\"", "bind=\"ui.missing\""),
    );
    assert!(failure(&s).starts_with("ui/hud.xml:5:5:"));
    s.0.insert("ui/hud.xml".into(), vec![255]);
    assert!(failure(&s).contains("ui/hud.xml 不是 UTF-8"));
    replace(
        &mut s,
        "ui/hud.xml",
        "<ui schema=\"3\">\n<screen></wrong>\n</ui>",
    );
    let error = failure(&s);
    assert!(error.contains("ui/hud.xml:2:"), "{error}");
}

#[test]
fn include_paths_roots_repetition_and_schema_are_strict() {
    for include in [
        "<include/>",
        "<include src=\"\"/>",
        "<include src=\"../a.xml\"/>",
        "<include src=\"/a.xml\"/>",
        "<include src=\"ui\\a.xml\"/>",
        "<include src=\"ui//a.xml\"/>",
        "<include src=\"a.js\"/>",
        "<include src=\"missing.xml\"/>",
        "<include src=\"ui.xml\"/>",
        "<include src=\"UI.xml\"/>",
        "<include src=\"ui/hud.xml\" extra=\"x\"/>",
        "<include src=\"ui/hud.xml\">bad</include>",
        "<include src=\"ui/hud.xml\"><screen/></include>",
        "<include src=\"ui/hud.xml\"/><include src=\"ui/hud.xml\"/>",
        "<include src=\"ui/hud.xml\"/><include src=\"UI/hud.xml\"/>",
    ] {
        let mut s = source();
        replace(
            &mut s,
            "ui.xml",
            &format!("<ui schema=\"3\" initial=\"hud\">\n{include}</ui>"),
        );
        let error = failure(&s);
        assert!(error.contains("ui.xml:2:"), "{include}: {error}");
    }
    for root in [
        "<ui schema=\"2\"/>",
        "<ui schema=\"3\" initial=\"hud\"/>",
        "<ui schema=\"3\" title=\"x\"/>",
        "<ui schema=\"3\"><unknown/></ui>",
        "<ui schema=\"3\">bad</ui>",
        "<ui schema=\"3\" xmlns=\"urn:test\"/>",
        "<ui schema=\"3\"><variables xmlns:p=\"urn:test\"/></ui>",
    ] {
        let mut s = source();
        replace(&mut s, "ui/state.xml", root);
        assert!(failure(&s).contains("ui/state.xml:"));
    }
    let mut s = source();
    replace(
        &mut s,
        "ui/hud.xml",
        &HUD.replace(
            "<scene id=\"scene\" width=\"1280\" height=\"720\"/>",
            "<include src=\"ui/state.xml\"/>",
        ),
    );
    assert!(failure(&s).starts_with("ui/hud.xml:4:5:"));
    replace(
        &mut s,
        "ui.xml",
        &ROOT.replace("schema=\"3\"", "schema=\"2\""),
    );
    assert!(failure(&s).starts_with("ui.xml:1:1:"));
}

#[test]
fn include_and_merged_runtime_budgets_cannot_be_bypassed() -> Result<()> {
    let mut s = source();
    let links = (0..127)
        .map(|i| format!("<include src=\"ui/empty{i}.xml\"/>"))
        .collect::<String>();
    replace(
        &mut s,
        "ui.xml",
        &format!(
            "<ui schema=\"3\" initial=\"hud\">{links}<screen id=\"hud\" modal=\"false\"/></ui>"
        ),
    );
    for i in 0..128 {
        replace(&mut s, &format!("ui/empty{i}.xml"), "<ui schema=\"3\"/>");
    }
    assert_eq!(
        interface::source::compile(&s, Path::new("ui.xml"))?
            .files
            .len(),
        128
    );
    let entry = String::from_utf8(s.read(Path::new("ui.xml"))?)?
        .replace("</ui>", "<include src=\"ui/empty127.xml\"/></ui>");
    replace(&mut s, "ui.xml", &entry);
    assert!(failure(&s).contains("超过 128"));
    let mut s = source();
    replace(
        &mut s,
        "ui.xml",
        r#"<ui schema="3" initial="hud"><include src="ui/1.xml"/></ui>"#,
    );
    for i in 1..16 {
        replace(
            &mut s,
            &format!("ui/{i}.xml"),
            &format!("<ui schema=\"3\"><include src=\"ui/{}.xml\"/></ui>", i + 1),
        );
    }
    replace(
        &mut s,
        "ui/15.xml",
        r#"<ui schema="3"><screen id="hud" modal="false"/></ui>"#,
    );
    loader::load_game(&s)?;
    replace(
        &mut s,
        "ui/15.xml",
        r#"<ui schema="3"><include src="ui/16.xml"/></ui>"#,
    );
    assert!(failure(&s).contains("超过 16 层"));
    let mut s = source();
    replace(
        &mut s,
        "ui/hud.xml",
        &format!(
            "<ui schema=\"3\"><!--{}--></ui>",
            "x".repeat(interface::source::MAX_XML_BYTES)
        ),
    );
    assert!(failure(&s).contains("累计超过 1MiB"));
    for (count, tag, expected) in [
        (1025, "nodes", "1024"),
        (65, "screens", "1–64"),
        (129, "variables", "128"),
        (257, "gallery", "256"),
    ] {
        let mut s = source();
        let items = (0..count)
            .map(|i| match tag {
                "nodes" => format!("<label id=\"n{i}\" width=\"10\" height=\"10\"/>"),
                "screens" => format!("<screen id=\"p{i}\" modal=\"false\"/>"),
                "variables" => format!("<bool id=\"v{i}\" initial=\"false\"/>"),
                _ => format!("<cg id=\"g{i}\" image=\"bg\" title=\"图\"/>"),
            })
            .collect::<Vec<_>>();
        let wrap = |items: &[String]| match tag {
            "nodes" => format!(
                "<screen id=\"p{}\" modal=\"false\">{}</screen>",
                items.len(),
                items.concat()
            ),
            "variables" | "gallery" => format!("<{tag}>{}</{tag}>", items.concat()),
            _ => items.concat(),
        };
        let half = count / 2;
        replace(
            &mut s,
            "ui.xml",
            r#"<ui schema="3" initial="p0"><include src="ui/a.xml"/><include src="ui/b.xml"/></ui>"#,
        );
        replace(
            &mut s,
            "ui/a.xml",
            &format!("<ui schema=\"3\">{}</ui>", wrap(&items[..half])),
        );
        replace(
            &mut s,
            "ui/b.xml",
            &format!("<ui schema=\"3\">{}</ui>", wrap(&items[half..])),
        );
        let error = failure(&s);
        assert!(error.contains(expected), "{tag}: {error}");
    }
    Ok(())
}

#[test]
fn both_real_packers_flatten_only_ui_dependencies_without_rewriting_sources() -> Result<()> {
    let root = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    let s = source();
    write_game(root.path(), &s)?;
    std::fs::write(
        root.path().join("notes.xml"),
        "<notes>keep explicitly</notes>",
    )?;
    std::fs::write(
        root.path().join("kakera.package.json"),
        r#"{"schema":1,"include":["notes.xml"]}"#,
    )?;
    for format in [1, 2] {
        let path = out.path().join(format!("ui-{format}.kpak"));
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_kakera-pack"))
            .arg(root.path())
            .arg(&path)
            .args([
                "--format",
                &format.to_string(),
                "--level",
                "1",
                "--no-live2d",
            ])
            .output()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let archive = Archive::open(&path)?;
        archive.verify_all()?;
        let bundle = Bundle::load(&archive, &options())?;
        assert_eq!(bundle.loaded.interface.screens.len(), 2);
        assert_eq!(
            interface::source::compile(&archive, Path::new("ui.xml"))?.files,
            ["ui.xml"]
        );
        for fragment in ["ui/pages.xml", "ui/hud.xml", "ui/state.xml"] {
            assert!(archive.read(Path::new(fragment)).is_err());
        }
        assert_eq!(
            archive.read(Path::new("story.js"))?,
            s.read(Path::new("story.js"))?
        );
        assert_eq!(
            archive.read(Path::new("notes.xml"))?,
            b"<notes>keep explicitly</notes>"
        );
        let first = std::fs::read(&path)?;
        drop(archive);
        drop(bundle);
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_kakera-pack"))
            .arg(root.path())
            .arg(&path)
            .args([
                "--format",
                &format.to_string(),
                "--level",
                "1",
                "--no-live2d",
                "--force",
            ])
            .output()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(std::fs::read(path)?, first);
    }
    for (path, bytes) in &s.0 {
        assert_eq!(std::fs::read(root.path().join(path))?, *bytes);
    }
    // An existing custom entry remains there; an unrelated root ui.xml is never overwritten.
    let mut custom = s.clone();
    custom.0.remove("ui.xml");
    replace(&mut custom, "ui/root.xml", ROOT);
    replace(
        &mut custom,
        "story.xml",
        &STORY.replace("src=\"ui.xml\"", "src=\"ui/root.xml\""),
    );
    replace(&mut custom, "ui.xml", "<notes>unrelated XML</notes>");
    let other = tempfile::tempdir()?;
    write_game(other.path(), &custom)?;
    let snapshot = Snapshot::directory(other.path())?;
    let (flat, _) = snapshot.for_publication()?;
    assert_eq!(
        flat.read(Path::new("ui.xml"))?,
        b"<notes>unrelated XML</notes>"
    );
    assert_eq!(
        interface::source::compile(&flat, Path::new("ui/root.xml"))?.files,
        ["ui/root.xml"]
    );
    assert_eq!(
        Bundle::load(&flat, &options())?
            .loaded
            .interface
            .screens
            .len(),
        2
    );
    let mut stage = StagedProject::capture(other.path())?;
    stage.for_publication()?;
    assert_eq!(
        stage.read(Path::new("ui/root.xml"))?,
        flat.read(Path::new("ui/root.xml"))?
    );
    stage.ensure_unchanged()?;
    Ok(())
}

#[test]
fn changing_ui_fragment_after_snapshot_cannot_replace_an_existing_package() -> Result<()> {
    let root = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    write_game(root.path(), &source())?;
    let selection = package::selection::Selection::discover(root.path(), &options())?;
    assert!(selection.paths.contains(Path::new("ui/hud.xml")));
    let mut stage =
        StagedProject::capture_selected(root.path(), selection.paths.iter().cloned().collect())?;
    selection.validate_candidate(&stage, &options())?;
    stage.for_publication()?;
    std::fs::write(root.path().join("ui/hud.xml"), HUD.replace("修改", "改变"))?;
    let path = out.path().join("old.kpak");
    std::fs::write(&path, b"OLD")?;
    assert!(
        package::write_streaming(&stage, &path, 1, true, |archive| {
            Bundle::load(archive, &options())?;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(std::fs::read(path)?, b"OLD");
    Ok(())
}

#[test]
fn real_project_watcher_rejects_bad_ui_fragment_and_recovers_after_repair() -> Result<()> {
    let root = tempfile::tempdir()?;
    write_game(root.path(), &source())?;
    let current = Bundle::load(&DirectorySource::new(root.path())?, &options())?;
    let session = current.loaded.story.start();
    let old = session.clone();
    let mut monitor = kakera_avg::development::ProjectMonitor::new(root.path(), options())?;
    let wait = |monitor: &mut kakera_avg::development::ProjectMonitor| -> Result<Bundle> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(result) = monitor.poll() {
                return result;
            }
            anyhow::ensure!(Instant::now() < deadline, "watch timeout");
            std::thread::sleep(Duration::from_millis(25));
        }
    };
    std::fs::write(
        root.path().join("ui/state.xml"),
        STATE.replace("中文", "更新"),
    )?;
    let candidate = wait(&mut monitor)?;
    assert_eq!(
        State::new(&candidate.loaded.interface).window_title,
        "更新 & Game"
    );
    std::fs::write(root.path().join("ui/hud.xml"), "<ui schema=\"3\"><broken/>")?;
    assert!(format!("{:#}", wait(&mut monitor).err().unwrap()).contains("ui/hud.xml"));
    assert_eq!(session, old);
    assert_eq!(
        State::new(&current.loaded.interface).window_title,
        "中文 & Game"
    );
    std::fs::write(root.path().join("ui/hud.xml"), HUD)?;
    let candidate = wait(&mut monitor)?;
    assert_eq!(
        kakera_avg::development::compatible_session(
            &current.loaded.story,
            &session,
            &candidate.loaded.story,
            &candidate.logic
        )?,
        session
    );
    assert_eq!(candidate.loaded.interface.screens.len(), 2);
    Ok(())
}
