#![cfg_attr(windows, windows_subsystem = "windows")]

mod launch_console;
use anyhow::{Context, Result, ensure};
use kakera_avg::{
    app::PlayerApp,
    content::ContentSource,
    development::ProjectMonitor,
    package::{Archive, StagedProject},
    runtime::{Bundle, LoadOptions},
    save::SaveStore,
};
use std::path::PathBuf;
struct Options {
    project: Option<PathBuf>,
    package: Option<PathBuf>,
    check: bool,
    no_audio: bool,
    saves: Option<PathBuf>,
    load: LoadOptions,
    renderer: String,
    debug_listen: Option<std::net::SocketAddr>,
    debug_token: Option<PathBuf>,
}
fn options() -> Result<Option<Options>> {
    let mut options = Options {
        project: None,
        package: None,
        check: false,
        no_audio: false,
        saves: None,
        load: LoadOptions {
            core: None,
            no_live2d: false,
        },
        renderer: "auto".into(),
        debug_listen: None,
        debug_token: None,
    };
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--project" {
            ensure!(
                options.project.is_none() && options.package.is_none(),
                "只能选择一个工程或资源包"
            );
            options.project = Some(PathBuf::from(args.next().context("缺少工程目录")?));
        } else if arg == "--package" {
            ensure!(
                options.project.is_none() && options.package.is_none(),
                "只能选择一个工程或资源包"
            );
            options.package = Some(PathBuf::from(args.next().context("缺少包路径")?));
        } else if arg == "--renderer" {
            options.renderer = args
                .next()
                .context("缺少渲染后端")?
                .into_string()
                .map_err(|_| anyhow::anyhow!("后端名称 UTF-8"))?;
            ensure!(
                ["auto", "d3d11", "cpu"].contains(&options.renderer.as_str()),
                "后端只能 auto/d3d11/cpu"
            );
        } else if arg == "--cubism-core" {
            options.load.core = Some(PathBuf::from(args.next().context("缺少 DLL 路径")?));
        } else if arg == "--no-live2d" {
            options.load.no_live2d = true;
        } else if arg == "--save-dir" {
            options.saves = Some(PathBuf::from(args.next().context("缺少存档目录")?));
        } else if arg == "--debug-listen" {
            ensure!(options.debug_listen.is_none(), "重复 debug-listen");
            options.debug_listen = Some(
                args.next()
                    .context("缺少调试监听地址")?
                    .to_string_lossy()
                    .parse()?,
            );
        } else if arg == "--debug-token-file" {
            ensure!(options.debug_token.is_none(), "重复 debug-token-file");
            options.debug_token = Some(PathBuf::from(args.next().context("缺少令牌文件")?));
        } else if arg == "--check" {
            options.check = true;
        } else if arg == "--no-audio" {
            options.no_audio = true;
        } else if arg == "--help" || arg == "-h" {
            println!(
                "Kakera AVG v{}\n用法：kakera-avg [--project <目录> | --package <game.kpak>] [--renderer auto|d3d11|cpu] [--check]\n--project 工程状态：监控文件并自动验证更新\n--package 发布状态：只读 KPAK 1/2，不监控散文件\n--check 完整校验包/内容/资源后退出；默认不打开窗口或 GPU，显式 --renderer d3d11 可同时验证设备\n--no-audio 禁用音频设备输出\n--no-live2d 显式使用静态 fallback\n--cubism-core <DLL> 指定受信任的 Cubism Core\n--save-dir <目录> 使用隔离用户数据\n--debug-listen <loopback:port> 工程调试 API，初始暂停，须配 --debug-token-file 与 --save-dir\n--debug-token-file <文件> 32–128 字符调试令牌；仅显式 --project 可用\n默认读取 exe 旁 game.kpak；源码启动若没有包则使用 games/demo 工程。",
                env!("CARGO_PKG_VERSION")
            );
            return Ok(None);
        } else {
            anyhow::bail!("未知参数：{}", arg.to_string_lossy());
        }
    }
    if options.debug_listen.is_some() || options.debug_token.is_some() {
        ensure!(
            options.project.is_some()
                && options.package.is_none()
                && !options.check
                && options.saves.is_some()
                && options.debug_listen.is_some()
                && options.debug_token.is_some(),
            "调试要求显式 --project、--save-dir、--debug-listen、--debug-token-file，禁止发布/--check"
        );
        ensure!(
            options.debug_listen.unwrap().ip().is_loopback(),
            "调试仅支持 loopback 地址"
        );
    }
    if options.project.is_none() && options.package.is_none() {
        let packaged = std::env::current_exe()?
            .parent()
            .context("程序目录")?
            .join("game.kpak");
        if packaged.is_file() {
            options.package = Some(packaged);
        } else {
            options.project = Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("games/demo"));
        }
    }
    if options.load.core.is_none() {
        options.load.core = if options.package.is_some() {
            kakera_avg::runtime::host_core()
        } else {
            kakera_avg::runtime::default_core()
        };
    }
    Ok(Some(options))
}
fn run() -> Result<()> {
    let Some(options) = options()? else {
        return Ok(());
    };
    let monitor = if !options.check {
        options
            .project
            .as_ref()
            .map(|p| ProjectMonitor::new(p, options.load.clone()))
            .transpose()?
    } else {
        None
    };
    let stage = if let Some(path) = &options.project {
        println!("运行状态：工程（实时监控） {}", path.display());
        Some(StagedProject::capture(path)?)
    } else {
        None
    };
    let archive = if stage.is_none() {
        let path = options.package.as_ref().context("包路径")?;
        println!("运行状态：发布（只读资源包） {}", path.display());
        let archive = Archive::open(path)?;
        println!("资源包格式：KPAK {}", archive.version());
        if options.check {
            archive.verify_all()?;
        }
        Some(archive)
    } else {
        None
    };
    let source: &dyn ContentSource = if let Some(stage) = &stage {
        stage
    } else {
        archive.as_ref().context("包来源")?
    };
    let bundle = Bundle::load(source, &options.load)?;
    if let Some(stage) = &stage {
        stage.ensure_unchanged()?;
    }
    // Prepared resources own their bytes/temp files; release the source snapshot now.
    drop(stage);
    drop(archive);
    if options.check {
        let story = &bundle.loaded.story;
        println!(
            "校验通过：{}，{} 个场景，{} 个变量，{} 个 JS 文件，{} 个 XML UI 页面",
            story.id,
            story.frames.len(),
            story.variables.len(),
            bundle.loaded.scripts.len(),
            bundle.loaded.interface.screens.len()
        );
        println!(
            "媒体校验：{} 个视频，{} 个 Live2D 模型",
            bundle.assets.media.videos.len(),
            bundle.assets.live2d.models.len()
        );
        #[cfg(windows)]
        if options.renderer == "d3d11" {
            let _ = kakera_avg::render::d3d11::Device::new(false)?;
        }
        return Ok(());
    }
    let saves = match options.saves {
        Some(path) => SaveStore::at(path),
        None => SaveStore::for_story(&bundle.loaded.story)?,
    };
    println!("存档目录：{}", saves.directory().display());
    let window_title =
        kakera_avg::interface::host::State::new(&bundle.loaded.interface).window_title;
    let debug = options
        .debug_listen
        .map(|address| -> Result<_> {
            let token = kakera_avg::debug::token_file(
                options.debug_token.as_deref().context("调试令牌文件")?,
            )?;
            let server = kakera_avg::debug::Server::bind(address, token)?;
            println!("调试 API：{}（JSON Lines v1，初始暂停）", server.address);
            Ok(server)
        })
        .transpose()?;
    let build = move |ctx: &eframe::egui::Context,
                      renderer: kakera_avg::render::Shared|
          -> Result<PlayerApp> {
        let mut app = PlayerApp::new_context(
            ctx,
            bundle.loaded,
            bundle.assets,
            saves,
            options.no_audio,
            bundle.logic,
            renderer,
        )?;
        if let Some(monitor) = monitor {
            app.monitor_project(monitor);
        }
        if let Some(debug) = debug {
            app.attach_debug(debug);
        }
        Ok(app)
    };
    #[cfg(windows)]
    {
        kakera_avg::render::window::run(&options.renderer, window_title, build)
    }
    #[cfg(not(windows))]
    {
        ensure!(
            options.renderer != "d3d11",
            "此平台尚未实现 D3D11；使用 CPU 呈现或后续 Vulkan/Metal 后端"
        );
        eframe::run_native(
            &window_title,
            eframe::NativeOptions::default(),
            Box::new(move |cc| {
                Ok(Box::new(build(
                    &cc.egui_ctx,
                    kakera_avg::render::Renderer::cpu(),
                )?))
            }),
        )
        .map_err(|e| anyhow::anyhow!("窗口失败：{e}"))
    }
}
fn main() {
    let dialog = launch_console::initialize();
    if let Err(error) = run() {
        eprintln!("Kakera 启动失败：{error:#}");
        if dialog {
            launch_console::error_dialog(&format!("{error:#}"));
        }
        std::process::exit(1);
    }
}
