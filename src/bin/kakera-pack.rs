use anyhow::{Context, Result, ensure};
use kakera_avg::i18n;
use kakera_avg::{
    package::{self, Snapshot, StagedProject},
    runtime::{Bundle, LoadOptions},
};
use std::path::PathBuf;
fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let Some(first) = args.next() else {
        anyhow::bail!("{}", crate::i18n::format("host-bin-kakera-pack-0001", &[]))
    };
    let root = PathBuf::from(first).canonicalize()?;
    let output = PathBuf::from(
        args.next()
            .context(crate::i18n::text("host-bin-kakera-pack-0010"))?,
    );
    let mut force = false;
    let mut level = 19;
    let mut format = 2;
    let mut options = LoadOptions {
        core: kakera_avg::runtime::default_core(),
        no_live2d: false,
    };
    while let Some(arg) = args.next() {
        if arg == "--force" {
            force = true;
        } else if arg == "--level" {
            level = args
                .next()
                .context(crate::i18n::text("host-bin-kakera-pack-0011"))?
                .to_string_lossy()
                .parse()?;
        } else if arg == "--format" {
            format = args
                .next()
                .context(crate::i18n::text("host-bin-kakera-pack-0012"))?
                .to_string_lossy()
                .parse()?;
            ensure!(
                [1, 2].contains(&format),
                "{}",
                crate::i18n::format("host-bin-kakera-pack-0002", &[])
            );
        } else if arg == "--cubism-core" {
            options.core = Some(PathBuf::from(
                args.next()
                    .context(crate::i18n::text("host-bin-kakera-pack-0013"))?,
            ));
        } else if arg == "--no-live2d" {
            options.no_live2d = true;
        } else {
            anyhow::bail!(
                "{}",
                crate::i18n::format(
                    "host-bin-kakera-pack-0003",
                    &[("arg0", format!("{:?}", arg))]
                )
            );
        }
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(parent)?;
    ensure!(
        !parent.canonicalize()?.starts_with(&root),
        "{}",
        crate::i18n::format("host-bin-kakera-pack-0004", &[])
    );
    ensure!(
        force || !output.exists(),
        "{}",
        crate::i18n::format("host-bin-kakera-pack-0005", &[])
    );
    let selection = package::selection::Selection::discover(&root, &options)?;
    let paths = selection.paths.iter().cloned().collect::<Vec<_>>();
    let (chapters, count, expanded, packed) = if format == 2 {
        let mut stage = StagedProject::capture_selected(&root, paths.clone())?;
        selection.validate_candidate(&stage, &options)?;
        let chapters = stage.for_publication()?;
        let packed = package::write_streaming(&stage, &output, level, force, |archive| {
            Bundle::load(archive, &options)?;
            selection.ensure_unchanged()?;
            Ok(())
        })?;
        (chapters, stage.count(), stage.expanded_bytes(), packed)
    } else {
        let snapshot = Snapshot::directory_selected(&root, &paths)?;
        selection.validate_candidate(&snapshot, &options)?;
        let (published, chapters) = snapshot.for_publication()?;
        let bytes = published.encode(level)?;
        let verified = Snapshot::decode(&bytes)?;
        Bundle::load(&verified, &options)?;
        ensure!(
            snapshot.fingerprint() == Snapshot::directory_selected(&root, &paths)?.fingerprint(),
            "{}",
            crate::i18n::format("host-bin-kakera-pack-0006", &[])
        );
        selection.ensure_unchanged()?;
        package::write(&output, &bytes, force)?;
        (
            chapters,
            published.count(),
            published.expanded_bytes() as u64,
            bytes.len() as u64,
        )
    };
    println!(
        "{}",
        crate::i18n::format(
            "host-bin-kakera-pack-0007",
            &[("arg0", chapters.to_string())]
        )
    );
    println!(
        "{}",
        crate::i18n::format(
            "host-bin-kakera-pack-0008",
            &[
                ("arg0", format.to_string()),
                ("arg1", format!("{}", output.display())),
                ("arg2", count.to_string()),
                ("arg3", expanded.to_string()),
                ("arg4", packed.to_string()),
                (
                    "arg5",
                    format!("{:.1}", 100.0 * packed as f64 / expanded.max(1) as f64)
                )
            ]
        )
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!(
            "{}",
            crate::i18n::format(
                "host-bin-kakera-pack-0009",
                &[("arg0", format!("{:#}", error))]
            )
        );
        std::process::exit(1);
    }
}
