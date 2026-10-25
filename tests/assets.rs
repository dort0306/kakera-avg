mod common;
use kakera_avg::{assets::PreparedAssets, content::ContentSource, loader};
#[test]
fn shipped_demo_has_decodable_images_and_chinese_glyphs() {
    let source = common::demo();
    let (loaded, _) = loader::load_game(&source).unwrap();
    PreparedAssets::load_from(&loaded.story, &source).unwrap();
}
#[test]
fn missing_resource_error_identifies_id_and_path() {
    let (loaded, _) = common::game();
    let source = common::source(common::STORY, common::SCRIPT);
    let text = format!(
        "{:#}",
        PreparedAssets::load_from(&loaded.story, &source)
            .err()
            .unwrap()
    );
    assert!(
        text.contains("bg.room") && text.contains("room.png"),
        "{text}"
    );
}
#[test]
fn corrupt_image_is_rejected_before_opening_window() {
    let (loaded, _) = common::game();
    let demo = common::demo();
    let mut source = common::source(common::STORY, common::SCRIPT);
    for asset in loaded.story.assets.values() {
        source
            .0
            .insert(asset.path.clone(), demo.read(&asset.path).unwrap());
    }
    source
        .0
        .insert("assets/bg/room.png".into(), b"not png".to_vec());
    assert!(
        format!(
            "{:#}",
            PreparedAssets::load_from(&loaded.story, &source)
                .err()
                .unwrap()
        )
        .contains("bg.room")
    );
}
