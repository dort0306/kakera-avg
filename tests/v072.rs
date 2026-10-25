use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use eframe::egui::{Color32, ColorImage};
use kakera_avg::{
    content::DirectorySource,
    loader,
    save::{SaveStore, SlotId, SlotState, Snapshot},
    save_preview::{self, Screenshot},
};
use std::path::Path;

#[test]
fn screenshots_are_bounded_png_with_stable_aspect_and_reject_corruption() -> Result<()> {
    let image = Screenshot::capture(ColorImage::filled([32, 32], Color32::GREEN))?;
    let png = image.decode()?;
    assert_eq!(png.dimensions(), (320, 180));
    assert_eq!(png.get_pixel(160, 90).0, [0, 255, 0, 255]);
    assert_eq!(png.get_pixel(0, 0).0, [0, 0, 0, 255]);
    for broken in [
        "%not base64%".to_owned(),
        STANDARD.encode(b"not png"),
        "a".repeat(400_000),
    ] {
        let mut bad = image.clone();
        bad.png_base64 = broken;
        assert!(bad.decode().is_err());
    }
    let mut bad = image.clone();
    bad.width = 1024;
    assert!(bad.decode().is_err());
    let mut bad = image.clone();
    let mut bytes = STANDARD.decode(&bad.png_base64)?;
    bytes.extend([0; 4]);
    bad.png_base64 = STANDARD.encode(bytes);
    assert!(bad.decode().is_err());
    let mut bad = image.clone();
    let mut bytes = STANDARD.decode(&bad.png_base64)?;
    bytes[16..20].copy_from_slice(&4096u32.to_be_bytes());
    bad.png_base64 = STANDARD.encode(bytes);
    assert!(bad.decode().is_err());
    assert!(Screenshot::capture(ColorImage::filled([0, 1], Color32::BLACK)).is_err());
    assert_eq!(save_preview::saved_at(u64::MAX), "无效时间");
    assert_eq!(save_preview::saved_at(1_792_000_000_000).len(), 19);
    Ok(())
}

#[test]
fn screenshot_metadata_roundtrips_with_explicit_null_and_page_isolation() -> Result<()> {
    let source = DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo"))?;
    let (loaded, logic) = loader::load_game(&source)?;
    let story = &loaded.story;
    let directory = tempfile::tempdir()?;
    let store = SaveStore::at(directory.path().to_owned());
    let slot = SlotId::new(200)?;
    store.save_with(slot, story, &story.start(), &logic)?;
    let mut snapshot: Snapshot = serde_json::from_slice(&std::fs::read(store.slot_path(slot))?)?;
    assert!(snapshot.screenshot.is_none());
    snapshot.screenshot = Some(Screenshot::capture(ColorImage::filled(
        [16, 9],
        Color32::BLUE,
    ))?);
    std::fs::write(store.slot_path(slot), serde_json::to_vec(&snapshot)?)?;
    assert_eq!(store.load_with(slot, story, &logic)?, story.start());
    let infos = store.list_page(story, 20)?;
    assert!(
        matches!(&infos[9].state,SlotState::Ready{screenshot:Some(_),saved_at_unix_ms,..} if *saved_at_unix_ms>0)
    );
    std::fs::write(store.slot_path(SlotId::new(1)?), b"invalid unrelated page")?;
    assert!(matches!(
        &store.list_page(story, 20)?[9].state,
        SlotState::Ready { .. }
    ));
    snapshot.screenshot.as_mut().unwrap().png_base64 = "bad".into();
    std::fs::write(store.slot_path(slot), serde_json::to_vec(&snapshot)?)?;
    assert!(store.load_with(slot, story, &logic).is_err());
    assert!(matches!(
        &store.list_page(story, 20)?[9].state,
        SlotState::Invalid(_)
    ));
    Ok(())
}

#[cfg(windows)]
#[test]
fn real_d3d11_client_copy_readback_produces_png() -> Result<()> {
    for software in [false, true] {
        let device = kakera_avg::render::d3d11::Device::new(software)
            .or_else(|_| kakera_avg::render::d3d11::Device::new(true))?;
        let source = device.texture([16, 9], Some(&[12, 34, 210, 255].repeat(16 * 9)), false)?;
        let copy = device.texture([16, 9], None, false)?;
        unsafe {
            device.context.CopyResource(&copy.raw, &source.raw);
        }
        let screenshot = Screenshot::capture(device.readback_texture(&copy)?)?;
        assert_eq!(
            screenshot.decode()?.get_pixel(160, 90).0,
            [12, 34, 210, 255]
        );
    }
    Ok(())
}
