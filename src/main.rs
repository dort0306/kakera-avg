use anyhow::{Context, Result, bail};
use eframe::egui;
use kakera_avg::{app::PlayerApp, assets::PreparedAssets, loader, save::SaveStore};
use std::path::PathBuf;

struct Options {
    game: PathBuf,
    check: bool,
}

fn options() -> Result<Option<Options>> {
    let mut game = None;
    let mut check = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--game" {
            game = Some(PathBuf::from(args.next().context("--game 需要游戏目录")?));
        } else if arg == "--check" {
            check = true;
        } else if arg == "--help" || arg == "-h" {
            println!(
                "Kakera AVG\n\n用法：kakera-avg [--game <游戏目录>] [--check]\n\n--check  校验 XML、图片和字体后退出，无需窗口\n默认先查找程序旁的 games/demo，再查找源码样例目录。"
            );
            return Ok(None);
        } else {
            bail!(
                "未知参数：{}（使用 --help 查看用法）",
                arg.to_string_lossy()
            );
        }
    }
    let game = game.unwrap_or_else(|| {
        let packaged = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|p| p.join("games/demo")));
        packaged
            .filter(|path| path.join("story.xml").is_file())
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("games/demo"))
    });
    Ok(Some(Options { game, check }))
}

fn run() -> Result<()> {
    let Some(options) = options()? else {
        return Ok(());
    };
    let loaded = loader::load_story(&options.game)?;
    let prepared = PreparedAssets::load(&loaded.story)?;
    for warning in &loaded.warnings {
        eprintln!("警告：{warning}");
    }
    if options.check {
        println!(
            "校验通过：{}，{} 个场景，入口 {}",
            loaded.story.id,
            loaded.story.frames.len(),
            loaded.story.entry
        );
        return Ok(());
    }
    let saves = SaveStore::for_story(&loaded.story)?;
    println!("存档位置：{}", saves.path().display());
    let native = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([640.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Kakera AVG",
        native,
        Box::new(move |cc| Ok(Box::new(PlayerApp::new(cc, loaded, prepared, saves)))),
    )
    .map_err(|e| anyhow::anyhow!("启动窗口失败：{e}"))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Kakera 启动失败：{error:#}");
        std::process::exit(1);
    }
}
