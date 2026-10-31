#![cfg_attr(windows, windows_subsystem = "windows")]
use kakera_avg::i18n;

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
                "{}",
                crate::i18n::format("host-main-0001", &[])
            );
            options.project = Some(PathBuf::from(
                args.next().context(crate::i18n::text("host-main-0021"))?,
            ));
        } else if arg == "--package" {
            ensure!(
                options.project.is_none() && options.package.is_none(),
                "{}",
                crate::i18n::format("host-main-0002", &[])
            );
            options.package = Some(PathBuf::from(
                args.next().context(crate::i18n::text("host-main-0022"))?,
            ));
        } else if arg == "--renderer" {
            options.renderer = args
                .next()
                .context(crate::i18n::text("host-main-0023"))?
                .into_string()
                .map_err(|_| anyhow::anyhow!("{}", crate::i18n::format("host-main-0003", &[])))?;
            ensure!(
                ["auto", "d3d11", "cpu"].contains(&options.renderer.as_str()),
                "{}",
                crate::i18n::format("host-main-0004", &[])
            );
        } else if arg == "--cubism-core" {
            options.load.core = Some(PathBuf::from(
                args.next().context(crate::i18n::text("host-main-0024"))?,
            ));
        } else if arg == "--no-live2d" {
            options.load.no_live2d = true;
        } else if arg == "--save-dir" {
            options.saves = Some(PathBuf::from(
                args.next().context(crate::i18n::text("host-main-0025"))?,
            ));
        } else if arg == "--debug-listen" {
            ensure!(
                options.debug_listen.is_none(),
                "{}",
                crate::i18n::format("host-main-0005", &[])
            );
            options.debug_listen = Some(
                args.next()
                    .context(crate::i18n::text("host-main-0026"))?
                    .to_string_lossy()
                    .parse()?,
            );
        } else if arg == "--debug-token-file" {
            ensure!(
                options.debug_token.is_none(),
                "{}",
                crate::i18n::format("host-main-0006", &[])
            );
            options.debug_token = Some(PathBuf::from(
                args.next().context(crate::i18n::text("host-main-0027"))?,
            ));
        } else if arg == "--check" {
            options.check = true;
        } else if arg == "--no-audio" {
            options.no_audio = true;
        } else if arg == "--help" || arg == "-h" {
            println!(
                "{}",
                crate::i18n::format(
                    "host-main-0007",
                    &[("arg0", env!("CARGO_PKG_VERSION").to_string())]
                )
            );
            return Ok(None);
        } else {
            anyhow::bail!(
                "{}",
                crate::i18n::format(
                    "host-main-0008",
                    &[("arg0", format!("{}", arg.to_string_lossy()))]
                )
            );
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
            "{}",
            crate::i18n::format("host-main-0009", &[])
        );
        ensure!(
            options.debug_listen.unwrap().ip().is_loopback(),
            "{}",
            crate::i18n::format("host-main-0010", &[])
        );
    }
    if options.project.is_none() && options.package.is_none() {
        let packaged = std::env::current_exe()?
            .parent()
            .context(crate::i18n::text("host-main-0028"))?
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
        println!(
            "{}",
            crate::i18n::format("host-main-0011", &[("arg0", format!("{}", path.display()))])
        );
        Some(StagedProject::capture(path)?)
    } else {
        None
    };
    let archive = if stage.is_none() {
        let path = options
            .package
            .as_ref()
            .context(crate::i18n::text("host-main-0029"))?;
        println!(
            "{}",
            crate::i18n::format("host-main-0012", &[("arg0", format!("{}", path.display()))])
        );
        let archive = Archive::open(path)?;
        println!(
            "{}",
            crate::i18n::format(
                "host-main-0013",
                &[("arg0", format!("{}", archive.version()))]
            )
        );
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
        archive
            .as_ref()
            .context(crate::i18n::text("host-main-0030"))?
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
            "{}",
            crate::i18n::format(
                "host-main-0014",
                &[
                    ("arg0", story.id.to_string()),
                    ("arg1", format!("{}", story.frames.len())),
                    ("arg2", format!("{}", story.variables.len())),
                    ("arg3", format!("{}", bundle.loaded.scripts.len())),
                    ("arg4", format!("{}", bundle.loaded.interface.screens.len()))
                ]
            )
        );
        println!(
            "{}",
            crate::i18n::format(
                "host-main-0015",
                &[
                    ("arg0", format!("{}", bundle.assets.media.videos.len())),
                    ("arg1", format!("{}", bundle.assets.live2d.models.len()))
                ]
            )
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
    println!(
        "{}",
        crate::i18n::format(
            "host-main-0016",
            &[("arg0", format!("{}", saves.directory().display()))]
        )
    );
    let window_title =
        kakera_avg::interface::host::State::new(&bundle.loaded.interface).window_title;
    let debug = options
        .debug_listen
        .map(|address| -> Result<_> {
            let token = kakera_avg::debug::token_file(
                options
                    .debug_token
                    .as_deref()
                    .context(crate::i18n::text("host-main-0031"))?,
            )?;
            let server = kakera_avg::debug::Server::bind(address, token)?;
            println!(
                "{}",
                crate::i18n::format("host-main-0017", &[("arg0", server.address.to_string())])
            );
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
            "{}",
            crate::i18n::format("host-main-0018", &[])
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
        .map_err(|e| {
            anyhow::anyhow!(
                "{}",
                crate::i18n::format("host-main-0019", &[("arg0", e.to_string())])
            )
        })
    }
}
fn main() {
    let dialog = launch_console::initialize();
    if let Err(error) = run() {
        eprintln!(
            "{}",
            crate::i18n::format("host-main-0020", &[("arg0", format!("{:#}", error))])
        );
        if dialog {
            launch_console::error_dialog(&crate::i18n::diagnostics(&format!("{error:#}")));
        }
        std::process::exit(1);
    }
}
