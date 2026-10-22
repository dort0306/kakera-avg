use kakera_avg::{assets::PreparedAssets, loader::load_story};
use std::path::Path;

#[test]
fn shipped_demo_has_decodable_images_and_chinese_glyphs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let loaded = load_story(&root).unwrap();
    assert!(loaded.warnings.is_empty());
    PreparedAssets::load(&loaded.story).unwrap();
}

#[test]
fn missing_resource_error_identifies_id_and_path() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("story.xml"),
        include_str!("../doc/rust-mvc-example/story.xml"),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("flow.xml"),
        include_str!("../doc/rust-mvc-example/flow.xml"),
    )
    .unwrap();
    let error = load_story(directory.path()).err().unwrap().to_string();
    assert!(error.contains("bg.room"), "{error}");
    assert!(error.contains("room.png"), "{error}");
}

#[test]
fn corrupt_image_is_rejected_before_opening_window() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let mut story = load_story(&root).unwrap().story;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("broken.png");
    std::fs::write(&path, "not a png").unwrap();
    story.assets.get_mut("bg.room").unwrap().path = path;
    let error = PreparedAssets::load(&story).err().unwrap();
    assert!(format!("{error:#}").contains("bg.room"));
}
