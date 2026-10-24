use anyhow::{Context, Result, bail};
use eframe::egui;
use kakera_avg::{
    app::PlayerApp, assets::PreparedAssets, audio::PreparedAudio, loader, save::SaveStore,
};
use std::path::PathBuf;

struct Options {
    game: PathBuf,
    check: bool,
    no_audio: bool,
    save_directory: Option<PathBuf>,
    cubism_core: Option<PathBuf>,
    no_live2d: bool,
}

fn options() -> Result<Option<Options>> {
    let mut game = None;
    let mut cubism_core = None;
    let mut no_live2d = false;
    let mut check = false;
    let mut no_audio = false;
    let mut save_directory = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--cubism-core" {
            cubism_core = Some(PathBuf::from(
                args.next().context("--cubism-core 需要 DLL 路径")?,
            ));
        } else if arg == "--no-live2d" {
            no_live2d = true;
        } else if arg == "--game" {
            game = Some(PathBuf::from(args.next().context("--game 需要游戏目录")?));
        } else if arg == "--save-dir" {
            save_directory = Some(PathBuf::from(args.next().context("--save-dir 需要目录")?));
        } else if arg == "--no-audio" {
            no_audio = true;
        } else if arg == "--check" {
            check = true;
        } else if arg == "--help" || arg == "-h" {
            println!(
                "Kakera AVG v0.6.0\n\n用法：kakera-avg [--game <游戏目录>] [--check] [--no-audio] [--save-dir <目录>]\n\n--check  校验剧情/UI XML、JS 注册/初始条件、图片、字体、音频、视频和 Live2D 后退出，无需设备\n--no-audio  禁用音频输出，仍校验音频文件\n--cubism-core  指定受信任的 Windows x64 Cubism Core DLL\n--no-live2d  显式使用 XML fallback 静态立绘\n--save-dir  指定隔离的存档/设置/CG 进度目录（调试或便携使用）\n默认先查找程序旁的 games/demo，再查找源码样例目录。XML 和 JS 在启动时读取，编辑后重启生效。"
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
    Ok(Some(Options {
        game,
        check,
        no_audio,
        save_directory,
        cubism_core,
        no_live2d,
    }))
}

fn run() -> Result<()> {
    let Some(options) = options()? else {
        return Ok(());
    };
    let source = kakera_avg::content::DirectorySource::new(&options.game)?;
    let (loaded, logic) = loader::load_game(&source)?;
    let prepared = PreparedAssets::load_from(&loaded.story, &source)?;
    if let Some(interface) = &loaded.interface {
        prepared.validate_text(&format!("{}UI 文本溢出", interface.text()))?;
    }
    let mut prepared_audio = PreparedAudio::load_from(&loaded.story, &source)?;
    let media = kakera_avg::media::PreparedMedia::load(&loaded.story, &source)?;
    for (id, asset) in &media.videos {
        if let Some(audio) = &asset.audio {
            prepared_audio.insert_video(id, audio.clone());
        }
    }
    let core = options
        .cubism_core
        .or_else(|| std::env::var_os("CUBISM_CORE").map(PathBuf::from))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.join("Live2DCubismCore.dll")))
                .filter(|p| p.is_file())
        })
        .or_else(|| {
            // A fixed host dependency, never a library path supplied by game content.
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("runtime/live2d/windows-x64/Live2DCubismCore.dll");
            path.is_file().then_some(path)
        });
    let live2d = kakera_avg::live2d::PreparedLive2D::load(
        &loaded.story,
        &source,
        core.as_deref(),
        options.no_live2d,
    )?;
    if let Some(plan) = loader::initial_ui_plan(&loaded, &logic)? {
        live2d.validate_commands(&loaded.story.frames[&loaded.story.entry], &plan.live2d)?;
    }
    for warning in &loaded.warnings {
        eprintln!("警告：{warning}");
    }
    if options.check {
        println!(
            "校验通过：{}，{} 个场景，入口 {}，{} 个音频缓存，{} 个变量，{} 个 JS 文件，{} 个 XML UI 页面",
            loaded.story.id,
            loaded.story.frames.len(),
            loaded.story.entry,
            prepared_audio.len(),
            loaded.story.variables.len(),
            loaded.scripts.len(),
            loaded.interface.as_ref().map_or(0, |ui| ui.screens.len())
        );
        println!(
            "媒体校验：{} 个视频，{} 个 Live2D 模型，Core {}",
            media.videos.len(),
            live2d.models.len(),
            live2d
                .core
                .as_ref()
                .map(|c| format!("{:08x}", unsafe { (c.version)() }))
                .unwrap_or_else(|| "未启用".into())
        );
        return Ok(());
    }
    let saves = match options.save_directory {
        Some(directory) => SaveStore::at(directory),
        None => SaveStore::for_story(&loaded.story)?,
    };
    println!("存档目录：{}", saves.directory().display());
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
        Box::new(move |cc| {
            Ok(Box::new(PlayerApp::new(
                cc,
                loaded,
                kakera_avg::app::RuntimeAssets {
                    images: prepared,
                    audio: prepared_audio,
                    media,
                    live2d,
                },
                saves,
                options.no_audio,
                logic,
            )?))
        }),
    )
    .map_err(|e| anyhow::anyhow!("启动窗口失败：{e}"))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Kakera 启动失败：{error:#}");
        std::process::exit(1);
    }
}
