use anyhow::Result;
use kakera_avg::{
    content::ContentSource,
    controller::{self, Intent},
    loader,
    package::Snapshot,
    story_chain,
};
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
            .ok_or_else(|| anyhow::anyhow!("缺失 {}", path.display()))
    }
}
const ENTRY: &str = r#"<story schema="8" id="chain" content-version="1" entry="opening">
<interface src="ui.xml"/><scripts><script src="scripts/main.js"/></scripts>
<assets><font id="font" src="font.otf"/><image id="bg" src="room.png"/></assets>
<variables><int id="trust" initial="0" min="0" max="3"/></variables>
<characters/><visuals/>
<frames><frame id="opening" visual="room"><text>第一章 &amp; 开始</text></frame></frames>
<next src="chapters/one.xml"/>
</story>"#;
const ONE: &str = r#"<story schema="8">
  <scripts><script src="scripts/end.js"/></scripts>
  <frames>
    <frame id="ending" visual="room"><text><![CDATA[第二章 <结束>]]></text></frame>
  </frames>
  <next src="chapters/two.xml"/>
</story>"#;
const TWO: &str = r#"<story schema="8"><visuals><visual id="room" background="bg"/></visuals><next end="true"/></story>"#;
fn source() -> Memory {
    Memory([
        ("story.xml", ENTRY), ("chapters/one.xml", ONE), ("chapters/two.xml", TWO),
        ("scripts/main.js", "on.advance('opening',ctx=>{ctx.add('trust',1);return 'ending';});"),
        ("scripts/end.js", "on.end('ending');"),
        ("ui.xml", r#"<ui schema="3" width="1280" height="720" initial="hud"><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/></screen></ui>"#),
    ].into_iter().map(|(p,s)| (p.to_owned(),s.as_bytes().to_vec())).collect())
}
fn replace(source: &mut Memory, path: &str, text: &str) {
    source.0.insert(path.to_owned(), text.as_bytes().to_vec());
}
fn fail(source: &Memory) -> String {
    format!(
        "{:#}",
        loader::load_game(source)
            .err()
            .expect("must reject invalid chain")
    )
}

#[test]
fn chain_preserves_cross_file_references_scripts_entities_and_save_replay() -> Result<()> {
    let source = source();
    let compiled = story_chain::compile(&source)?;
    assert_eq!(
        compiled.files,
        ["story.xml", "chapters/one.xml", "chapters/two.xml"]
    );
    let (loaded, logic) = loader::load_game(&source)?;
    assert_eq!(loaded.story.frames["opening"].text, "第一章 & 开始");
    assert_eq!(loaded.story.frames["ending"].text, "第二章 <结束>");
    assert_eq!(
        loaded.scripts,
        ["scripts/main.js", "scripts/end.js"].map(std::path::PathBuf::from)
    );
    let session = controller::transition_with(
        &loaded.story,
        &loaded.story.start(),
        Intent::Advance,
        &logic,
    )
    .map_err(anyhow::Error::msg)?
    .session;
    assert_eq!(session.frame_id, "ending");
    let save = kakera_avg::save::Snapshot::capture_with(&loaded.story, &session, &logic)?;
    let mut flattened = source.clone();
    flattened.0.remove("chapters/one.xml");
    flattened.0.remove("chapters/two.xml");
    replace(&mut flattened, "story.xml", &compiled.xml);
    let (published, published_logic) = loader::load_game(&flattened)?;
    assert_eq!(
        save.restore_with(&published.story, &published_logic)?,
        session
    );
    assert_eq!(published.story.frames.len(), 2);
    Ok(())
}

#[test]
fn links_are_explicit_strict_root_relative_and_bounded() {
    for next in [
        "",
        "<next/>",
        "<next end=\"false\"/>",
        "<next end=\"TRUE\"/>",
        "<next src=\"chapters/two.xml\" end=\"true\"/>",
        "<next src=\"\"/>",
        "<next src=\"../two.xml\"/>",
        "<next src=\"/two.xml\"/>",
        "<next src=\"a\\b.xml\"/>",
        "<next src=\"two.js\"/>",
        "<next end=\"true\"/><next end=\"true\"/>",
        "<next end=\"true\" other=\"1\"/>",
        "<next end=\"true\">bad</next>",
        "<next end=\"true\"><x/></next>",
    ] {
        let mut s = source();
        replace(
            &mut s,
            "chapters/two.xml",
            &format!(
                "<story schema=\"8\"><visuals><visual id=\"room\" background=\"bg\"/></visuals>{next}</story>"
            ),
        );
        let error = fail(&s);
        assert!(error.contains("chapters/two.xml:"), "{next}: {error}");
    }
    let mut s = source();
    replace(
        &mut s,
        "chapters/two.xml",
        "<story schema=\"8\"><next src=\"story.xml\"/></story>",
    );
    assert!(fail(&s).contains("循环"));
    replace(
        &mut s,
        "chapters/two.xml",
        "<story schema=\"8\"><next src=\"STORY.xml\"/></story>",
    );
    assert!(fail(&s).contains("大小写重复"));
    s.0.remove("chapters/two.xml");
    assert!(fail(&s).contains("chapters/two.xml"));
}

#[test]
fn source_diagnostics_identify_original_unicode_line_and_conflicting_ids() {
    let mut s = source();
    let unicode = ONE.replace("</text>", "</text><unknown/>");
    let doc = roxmltree::Document::parse(&unicode).unwrap();
    let node = doc
        .descendants()
        .find(|n| n.has_tag_name("unknown"))
        .unwrap();
    let position = doc.text_pos_at(node.range().start);
    replace(&mut s, "chapters/one.xml", &unicode);
    assert!(fail(&s).starts_with(&format!(
        "chapters/one.xml:{}:{}:",
        position.row, position.col
    )));
    replace(
        &mut s,
        "chapters/one.xml",
        &ONE.replace("id=\"ending\"", "id=\"opening\""),
    );
    let error = fail(&s);
    assert!(error.starts_with("chapters/one.xml:4:5:"), "{error}");
    assert!(error.contains("重复 ID"));
    replace(
        &mut s,
        "chapters/one.xml",
        &ONE.replace("<text>", "<text typo=\"1\">"),
    );
    assert!(fail(&s).starts_with("chapters/one.xml:4:"));
    replace(
        &mut s,
        "chapters/one.xml",
        &ONE.replace("visual=\"room\"", "visual=\"missing\""),
    );
    let error = fail(&s);
    assert!(error.starts_with("chapters/one.xml:4:5:"), "{error}");
    for xml in [
        "<story schema=\"6\"><next end=\"true\"/></story>",
        "<story schema=\"8\" id=\"duplicate\"><next end=\"true\"/></story>",
        "<story schema=\"8\"><interface src=\"ui.xml\"/><next end=\"true\"/></story>",
        "<story schema=\"8\"><frames/><frames/><next end=\"true\"/></story>",
        "<story schema=\"8\"><visuals typo=\"1\"/><next end=\"true\"/></story>",
        "<story schema=\"8\"><unknown/><next end=\"true\"/></story>",
        "<story schema=\"8\"><frames>",
    ] {
        let mut s = source();
        replace(&mut s, "chapters/one.xml", xml);
        assert!(fail(&s).contains("chapters/one.xml"));
    }
}

#[test]
fn chain_limits_prevent_unbounded_files_and_xml() {
    let mut s = source();
    for i in 1..=story_chain::MAX_FILES {
        let path = if i == 1 {
            "chapters/one.xml".to_owned()
        } else {
            format!("chapters/{i}.xml")
        };
        replace(
            &mut s,
            &path,
            &format!(
                "<story schema=\"8\"><next src=\"chapters/{}.xml\"/></story>",
                i + 1
            ),
        );
    }
    assert!(fail(&s).contains("超过 128"));
    let mut s = source();
    replace(
        &mut s,
        "chapters/one.xml",
        &format!(
            "<story schema=\"8\"><!--{}--><next end=\"true\"/></story>",
            "a".repeat(story_chain::MAX_XML_BYTES)
        ),
    );
    assert!(fail(&s).contains("累计超过 16MiB"));
    let mut s = source();
    s.0.insert("chapters/one.xml".into(), vec![255]);
    assert!(fail(&s).contains("UTF-8"));
}

#[test]
fn publishing_compiles_only_reachable_story_files_and_is_self_contained() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let mut s = source();
    replace(
        &mut s,
        "notes.xml",
        "<notes>ordinary file, not a chapter</notes>",
    );
    for (path, bytes) in &s.0 {
        let p = temp.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(p, bytes)?;
    }
    let snapshot = Snapshot::directory(temp.path())?;
    let (published, count) = snapshot.for_publication()?;
    assert_eq!(count, 3);
    assert_eq!(published.count(), snapshot.count() - 2);
    assert!(published.read(Path::new("chapters/one.xml")).is_err());
    assert!(published.read(Path::new("chapters/two.xml")).is_err());
    assert_eq!(
        published.read(Path::new("ui.xml"))?,
        s.read(Path::new("ui.xml"))?
    );
    assert_eq!(
        published.read(Path::new("notes.xml"))?,
        s.read(Path::new("notes.xml"))?
    );
    let bytes = published.encode(1)?;
    assert_eq!(bytes, snapshot.for_publication()?.0.encode(1)?);
    assert_eq!(
        std::fs::read(temp.path().join("story.xml"))?,
        ENTRY.as_bytes()
    );
    let decoded = Snapshot::decode(&bytes)?;
    let (loaded, _) = loader::load_game(&decoded)?;
    assert_eq!(loaded.story.frames.len(), 2);
    assert_eq!(story_chain::compile(&decoded)?.files, ["story.xml"]);
    Ok(())
}

#[test]
fn packer_rejects_bad_chapter_before_overwriting_existing_package() -> Result<()> {
    let game = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    let mut source = source();
    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    source.0.insert(
        "font.otf".into(),
        std::fs::read(demo.join("assets/fonts/chinese.otf"))?,
    );
    source.0.insert(
        "room.png".into(),
        std::fs::read(demo.join("assets/bg/room.png"))?,
    );
    for (path, bytes) in &source.0 {
        let path = game.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, bytes)?;
    }
    let package = destination.path().join("game.kpak");
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_kakera-pack"))
            .arg(game.path())
            .arg(&package)
            .args(["--level", "1", "--force"])
            .output()
    };
    let good = run()?;
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    let original = std::fs::read(&package)?;
    let packed = kakera_avg::package::Archive::open(&package)?;
    assert_eq!(packed.version(), 2);
    assert!(packed.read(Path::new("chapters/one.xml")).is_err());
    assert_eq!(loader::load_game(&packed)?.0.story.frames.len(), 2);
    std::fs::write(
        game.path().join("chapters/two.xml"),
        TWO.replace("end=\"true\"", "end=\"false\""),
    )?;
    let bad = run()?;
    assert_eq!(bad.status.code(), Some(1));
    assert!(String::from_utf8(bad.stderr)?.contains("chapters/two.xml:"));
    assert_eq!(std::fs::read(package)?, original);
    Ok(())
}

#[test]
fn real_chapter_notifications_keep_old_session_on_error_then_recover() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let mut s = source();
    s.0.insert(
        "font.otf".into(),
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
        )?,
    );
    s.0.insert(
        "room.png".into(),
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/bg/room.png"))?,
    );
    for (path, bytes) in &s.0 {
        let p = temp.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(p, bytes)?;
    }
    let (old, _) = loader::load_game(&s)?;
    let session = old.story.start();
    let mut monitor = kakera_avg::development::ProjectMonitor::new(
        temp.path(),
        kakera_avg::runtime::LoadOptions {
            core: None,
            no_live2d: true,
        },
    )?;
    let wait=|monitor:&mut kakera_avg::development::ProjectMonitor| -> Result<kakera_avg::runtime::Bundle> {
        let until=Instant::now()+Duration::from_secs(15);
        loop { if let Some(result)=monitor.poll(){return result;} anyhow::ensure!(Instant::now()<until,"watch timeout");std::thread::sleep(Duration::from_millis(25)); }
    };
    let path = temp.path().join("chapters/one.xml");
    std::fs::write(&path, ONE.replace("第二章", "已热更新"))?;
    let next = wait(&mut monitor)?;
    assert!(next.loaded.story.frames["ending"].text.contains("已热更新"));
    assert_eq!(
        kakera_avg::development::compatible_session(
            &old.story,
            &session,
            &next.loaded.story,
            &next.logic
        )?,
        session
    );
    std::fs::write(&path, ONE.replace("<next src=\"chapters/two.xml\"/>", ""))?;
    assert!(wait(&mut monitor).is_err());
    assert_eq!(session, old.story.start());
    std::fs::write(&path, ONE)?;
    assert_eq!(wait(&mut monitor)?.loaded.story.frames.len(), 2);
    Ok(())
}

#[cfg(windows)]
#[test]
fn gui_player_keeps_cli_output_and_failure_exit_codes_without_allocating_a_console() -> Result<()> {
    let player = env!("CARGO_BIN_EXE_kakera-avg");
    let packer = env!("CARGO_BIN_EXE_kakera-pack");
    let subsystem = |path: &str| -> Result<u16> {
        let pe = std::fs::read(path)?;
        let offset = u32::from_le_bytes(pe[60..64].try_into()?) as usize;
        anyhow::ensure!(&pe[offset..offset + 4] == b"PE\0\0", "PE signature");
        Ok(u16::from_le_bytes(pe[offset + 92..offset + 94].try_into()?))
    };
    assert_eq!(subsystem(player)?, 2, "player must be GUI subsystem");
    assert_eq!(subsystem(packer)?, 3, "packer must remain a console tool");
    let help = std::process::Command::new(player).arg("--help").output()?;
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout)?.contains(env!("CARGO_PKG_VERSION")));
    let missing = tempfile::tempdir()?.path().join("missing.kpak");
    let failure = std::process::Command::new(player)
        .args(["--package", missing.to_str().unwrap(), "--check"])
        .output()?;
    assert_eq!(failure.status.code(), Some(1));
    assert!(String::from_utf8(failure.stderr)?.contains("Kakera 启动失败"));
    Ok(())
}
