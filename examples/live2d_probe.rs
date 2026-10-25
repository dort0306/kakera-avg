//! Local diagnostic: cargo run --release --example live2d_probe -- <game> <Core.dll> <output.png>
use anyhow::{Context, Result};
use kakera_avg::{
    content::DirectorySource,
    live2d::{Live2DPlayer, PreparedLive2D},
    loader,
};
use std::path::Path;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(args.len() == 3, "需要 game、Core DLL 和输出 PNG 路径");
    let source = DirectorySource::new(Path::new(&args[0]))?;
    let (loaded, _) = loader::load_game(&source)?;
    let prepared = PreparedLive2D::load(&loaded.story, &source, Some(Path::new(&args[1])), false)?;
    let frame = loaded
        .story
        .frames
        .values()
        .find(|f| f.visual.portraits.iter().any(|p| p.live2d.is_some()))
        .context("没有 Live2D 场景")?;
    let start = std::time::Instant::now();
    let player = Live2DPlayer::prepare(frame, &prepared)?;
    let kakera_avg::render::Image::Cpu(image) =
        player.images.values().next().context("没有模型图像")?
    else {
        anyhow::bail!("expected CPU image");
    };
    let bytes: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|p| p.to_srgba_unmultiplied())
        .collect();
    image::save_buffer(
        Path::new(&args[2]),
        &bytes,
        image.size[0] as u32,
        image.size[1] as u32,
        image::ColorType::Rgba8,
    )?;
    println!(
        "{}: {:?}, {} 非透明像素，首次变形/合成 {:?}",
        frame.id,
        image.size,
        image.pixels.iter().filter(|p| p.a() > 0).count(),
        start.elapsed()
    );
    Ok(())
}
