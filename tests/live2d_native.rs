//! Requires an explicit local SDK; copyrighted sample assets stay outside the repository.
use anyhow::{Context, Result};
use kakera_avg::{
    content::DirectorySource,
    live2d::{Live2DPlayer, PreparedLive2D, core::Model},
    loader,
};
use std::path::PathBuf;
#[test]
#[ignore = "set CUBISM_SDK and LIVE2D_TEST_GAME to locally generated SDK game; then run --ignored"]
fn real_cubism_model_deforms_and_composites_with_motion_and_validates_commands() -> Result<()> {
    let sdk = PathBuf::from(std::env::var_os("CUBISM_SDK").context("CUBISM_SDK")?);
    let game = PathBuf::from(std::env::var_os("LIVE2D_TEST_GAME").context("LIVE2D_TEST_GAME")?);
    let source = DirectorySource::new(&game)?;
    let (loaded, _) = loader::load_game(&source)?;
    let prepared = PreparedLive2D::load(
        &loaded.story,
        &source,
        Some(&sdk.join("Core/dll/windows/x86_64/Live2DCubismCore.dll")),
        false,
    )?;
    let asset = &prepared.models["haru"];
    let mut raw = Model::new(
        prepared.core.as_ref().unwrap().clone(),
        &asset.moc,
        asset.textures.len(),
    )?;
    let mut parameters = std::collections::BTreeMap::new();
    let parts = Default::default();
    raw.update(&parameters, &parts)?;
    let before = raw.drawables(asset.textures.len())?;
    let p = raw
        .parameters
        .iter()
        .find(|p| p.id == "ParamAngleX")
        .context("ParamAngleX")?;
    parameters.insert(p.id.clone(), p.max);
    raw.update(&parameters, &parts)?;
    let after = raw.drawables(asset.textures.len())?;
    assert!(before.iter().zip(&after).any(|(a, b)| {
        a.vertices
            .iter()
            .zip(&b.vertices)
            .any(|(a, b)| (a.x - b.x).abs() > 0.00001)
    }));
    let frame = &loaded.story.frames["live2d.stage"];
    let player = Live2DPlayer::prepare(frame, &prepared)?;
    let image = player.images.values().next().unwrap();
    assert!(image.pixels.iter().filter(|p| p.a() > 0).count() > 10000);
    // The bundled demo includes independent expression and XML-parameter scenes.
    let expression = Live2DPlayer::prepare(&loaded.story.frames["live2d.expression"], &prepared)?;
    let angle = Live2DPlayer::prepare(&loaded.story.frames["live2d.angle"], &prepared)?;
    assert_ne!(
        image.pixels,
        expression.images.values().next().unwrap().pixels
    );
    assert_ne!(image.pixels, angle.images.values().next().unwrap().pixels);
    assert!(
        prepared
            .validate_commands(
                frame,
                &[(kakera_avg::model::Slot::Center, "TapBody:0".into())]
            )
            .is_ok()
    );
    assert!(
        prepared
            .validate_commands(
                frame,
                &[(kakera_avg::model::Slot::Center, "Unknown:0".into())]
            )
            .is_err()
    );
    assert!(
        prepared
            .validate_commands(
                &loaded.story.frames["opening"],
                &[(kakera_avg::model::Slot::Center, "Idle:0".into())]
            )
            .is_err()
    );
    Ok(())
}
