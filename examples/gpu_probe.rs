//! Headless D3D11 diagnostic; readback is for inspection, never the normal player path.
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    use kakera_avg::{
        content::DirectorySource,
        live2d::{Live2DPlayer, PreparedLive2D},
        render::{
            Renderer,
            d3d11::{Device, Pixels},
        },
    };
    let output = std::env::args_os().nth(1).context("需要输出 PNG 路径")?;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let source = DirectorySource::new(&root)?;
    let (loaded, _) = kakera_avg::loader::load_game(&source)?;
    let models = PreparedLive2D::load(
        &loaded.story,
        &source,
        kakera_avg::runtime::default_core().as_deref(),
        false,
    )?;
    let device = Device::new(false)?;
    let renderer = Renderer::with_gpu(Box::new(Pixels::new(device.clone())));
    let player = Live2DPlayer::prepare_with(
        &loaded.story.frames["live2d.stage"],
        &models,
        renderer.clone(),
    )?;
    ensure!(renderer.borrow().is_gpu(), "意外回退 CPU");
    let image = device.readback(player.images.values().next().context("模型输出")?)?;
    let bytes: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|p| p.to_srgba_unmultiplied())
        .collect();
    image::save_buffer(
        std::path::Path::new(&output),
        &bytes,
        image.size[0] as u32,
        image.size[1] as u32,
        image::ColorType::Rgba8,
    )?;
    println!(
        "D3D11 原生模型输出 {:?} -> {}",
        image.size,
        std::path::Path::new(&output).display()
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("D3D11 probe requires Windows");
}
