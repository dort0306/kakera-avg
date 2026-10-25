use anyhow::Result;
use kakera_avg::{
    content::ContentSource,
    loader,
    package::{self, Snapshot},
    runtime::{Bundle, LoadOptions},
};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    time::{Duration, Instant},
};
#[test]
fn archive_is_deterministic_deduplicated_bounded_and_keeps_old_file_on_failed_write() -> Result<()>
{
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("story.xml"), "abc".repeat(10000))?;
    std::fs::write(root.path().join("copy.txt"), "abc".repeat(10000))?;
    let source = Snapshot::directory(root.path())?;
    let encoded = source.encode(3)?;
    assert_eq!(encoded, source.encode(3)?);
    assert!(encoded.len() < 1000);
    let decoded = Snapshot::decode(&encoded)?;
    assert_eq!(
        decoded.read(Path::new("story.xml"))?,
        source.read(Path::new("story.xml"))?
    );
    assert_eq!(decoded.fingerprint(), source.fingerprint());
    assert!(decoded.read(Path::new("../copy.txt")).is_err());
    let expanded = u64::from_le_bytes(encoded[24..32].try_into()?);
    assert!(expanded < 40000);
    let destination = tempfile::tempdir()?;
    let path = destination.path().join("game.kpak");
    package::write(&path, &encoded, false)?;
    assert!(package::write(&path, b"new", false).is_err());
    assert_eq!(std::fs::read(&path)?, encoded);
    let blocked = destination.path().join("directory.kpak");
    std::fs::create_dir(&blocked)?;
    std::fs::write(blocked.join("marker"), "old")?;
    assert!(package::write(&blocked, &encoded, true).is_err());
    assert_eq!(std::fs::read_to_string(blocked.join("marker"))?, "old");
    package::write(&path, &encoded, true)?;
    assert_eq!(Snapshot::open(&path)?.fingerprint(), source.fingerprint());
    Ok(())
}
fn raw(payload: &[u8]) -> Result<Vec<u8>> {
    let compressed = zstd::bulk::compress(payload, 1)?;
    let mut bytes = b"KPAK\r\n\x1a\0".to_vec();
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend((compressed.len() as u64).to_le_bytes());
    bytes.extend((payload.len() as u64).to_le_bytes());
    bytes.extend(Sha256::digest(payload));
    bytes.extend(compressed);
    Ok(bytes)
}
#[test]
fn archive_rejects_corruption_trailing_frames_traversal_collisions_and_bombs() -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("story.xml"), "hello")?;
    let bytes = Snapshot::directory(root.path())?.encode(1)?;
    for index in [0, 8, 12, 16, 24, 32, bytes.len() - 1] {
        let mut bad = bytes.clone();
        bad[index] ^= 128;
        assert!(Snapshot::decode(&bad).is_err(), "byte {index}");
    }
    for len in [0, 7, 63, bytes.len() - 1] {
        assert!(Snapshot::decode(&bytes[..len]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.extend([0; 3]);
    assert!(Snapshot::decode(&trailing).is_err());
    let mut bomb = bytes.clone();
    bomb[24..32].copy_from_slice(&((package::MAX_TOTAL as u64) + 1).to_le_bytes());
    assert!(Snapshot::decode(&bomb).is_err());
    for names in [
        vec!["../story.xml"],
        vec!["/story.xml"],
        vec!["a//b"],
        vec!["a/./b"],
        vec!["C:story.xml"],
        vec!["story.xml ", "story.xml"],
        vec!["STORY.xml", "story.xml"],
        vec!["story.xml", "story.xml"],
    ] {
        let mut payload = (names.len() as u32).to_le_bytes().to_vec();
        for name in names {
            payload.extend((name.len() as u16).to_le_bytes());
            payload.extend(name.as_bytes());
            payload.extend(0u32.to_le_bytes());
        }
        payload.extend(1u32.to_le_bytes());
        payload.extend(0u64.to_le_bytes());
        assert!(Snapshot::decode(&raw(&payload)?).is_err());
    }
    Ok(())
}
#[test]
fn real_demo_loads_all_assets_from_package_without_directory_access() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let source = Snapshot::directory(&root)?;
    let packed = Snapshot::decode(&source.encode(1)?)?;
    let options = LoadOptions {
        core: kakera_avg::runtime::default_core(),
        no_live2d: false,
    };
    let bundle = Bundle::load(&packed, &options)?;
    let (directory, logic) = loader::load_game(&source)?;
    assert_eq!(bundle.loaded.story.start(), directory.story.start());
    assert_eq!(bundle.loaded.story.frames.len(), 30);
    assert_eq!(bundle.assets.media.videos.len(), 2);
    assert_eq!(bundle.assets.live2d.models.len(), 1);
    assert_eq!(bundle.logic.ui_handlers()?, logic.ui_handlers()?);
    Ok(())
}
#[test]
fn project_snapshot_excludes_user_data_and_detects_content_changes() -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("story.xml"), "content")?;
    std::fs::write(root.path().join("slot-1.json"), "private")?;
    std::fs::create_dir(root.path().join("target"))?;
    std::fs::write(root.path().join("target/cache"), "cache")?;
    let snapshot = Snapshot::directory(root.path())?;
    assert_eq!(snapshot.count(), 1);
    std::fs::write(root.path().join("story.xml"), "changed")?;
    assert_ne!(
        snapshot.fingerprint(),
        Snapshot::directory(root.path())?.fingerprint()
    );
    Ok(())
}
#[test]
fn project_notifications_reload_valid_content_and_reject_invalid_script_without_mutating_session()
-> Result<()> {
    // A real minimal editable game, with only the assets required by validation.
    let temp = tempfile::tempdir()?;
    let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    std::fs::create_dir(temp.path().join("scripts"))?;
    std::fs::create_dir(temp.path().join("assets"))?;
    std::fs::copy(
        original.join("assets/fonts/chinese.otf"),
        temp.path().join("assets/font.otf"),
    )?;
    let xml = r#"<story schema="8" id="reload-test" content-version="1" entry="opening"><scripts><script src="scripts/story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="assets/font.otf"/></assets><characters/><visuals/><frames><frame id="opening"><text>初始文字</text></frame></frames><next end="true"/></story>"#;
    std::fs::write(temp.path().join("story.xml"), xml)?;
    std::fs::write(
        temp.path().join("ui.xml"),
        r#"<ui schema="3" width="1280" height="720" initial="hud"><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/></screen></ui>"#,
    )?;
    std::fs::write(temp.path().join("scripts/story.js"), "on.end('opening');")?;
    let options = LoadOptions {
        core: None,
        no_live2d: true,
    };
    let bundle = Bundle::load(&Snapshot::directory(temp.path())?, &options)?;
    let current = bundle.loaded.story;
    let session = current.start();
    let old = session.clone();
    let mut monitor = kakera_avg::development::ProjectMonitor::new(temp.path(), options)?;
    let wait = |monitor: &mut kakera_avg::development::ProjectMonitor| -> Result<Bundle> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(result) = monitor.poll() {
                return result;
            }
            anyhow::ensure!(Instant::now() < deadline, "watch timeout");
            std::thread::sleep(Duration::from_millis(25));
        }
    };
    std::fs::write(
        temp.path().join("story.xml"),
        xml.replace("初始文字", "实时修改"),
    )?;
    let candidate = wait(&mut monitor)?;
    assert_eq!(candidate.loaded.story.frames["opening"].text, "实时修改");
    assert_eq!(
        kakera_avg::development::compatible_session(
            &current,
            &session,
            &candidate.loaded.story,
            &candidate.logic
        )?,
        session
    );
    std::fs::write(temp.path().join("scripts/story.js"), "syntax !!!")?;
    assert!(wait(&mut monitor).is_err());
    assert_eq!(session, old);
    std::fs::write(temp.path().join("scripts/new.js"), "on.end('opening');")?;
    std::fs::rename(
        temp.path().join("scripts/new.js"),
        temp.path().join("scripts/story.js"),
    )?;
    let candidate = wait(&mut monitor)?;
    let mut incompatible = candidate.loaded.story;
    incompatible.content_version += 1;
    assert!(
        kakera_avg::development::compatible_session(
            &current,
            &session,
            &incompatible,
            &candidate.logic
        )
        .is_err()
    );
    assert_eq!(session, old);
    Ok(())
}
