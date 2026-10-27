use anyhow::{Context, Result, ensure};
use kakera_avg::{
    package::{self, Snapshot, StagedProject},
    runtime::{Bundle, LoadOptions},
};
use std::path::PathBuf;
fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let Some(first) = args.next() else {
        anyhow::bail!(
            "用法：kakera-pack <工程目录> <输出.kpak> [--format 1|2] [--force] [--level 1..22] [--cubism-core DLL] [--no-live2d]"
        )
    };
    let root = PathBuf::from(first).canonicalize()?;
    let output = PathBuf::from(args.next().context("缺少输出路径")?);
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
            level = args.next().context("缺少级别")?.to_string_lossy().parse()?;
        } else if arg == "--format" {
            format = args
                .next()
                .context("缺少资源包格式")?
                .to_string_lossy()
                .parse()?;
            ensure!([1, 2].contains(&format), "资源包格式只能 1/2");
        } else if arg == "--cubism-core" {
            options.core = Some(PathBuf::from(args.next().context("缺少 DLL 路径")?));
        } else if arg == "--no-live2d" {
            options.no_live2d = true;
        } else {
            anyhow::bail!("未知参数 {arg:?}");
        }
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(parent)?;
    ensure!(
        !parent.canonicalize()?.starts_with(&root),
        "输出包必须位于工程目录之外"
    );
    ensure!(
        force || !output.exists(),
        "输出已存在，使用 --force 才能原子覆盖"
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
            "校验期间工程改变，请重新打包"
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
    println!("剧本合并：{chapters} 个 XML → story.xml");
    println!(
        "KPAK {format} 打包完成：{}\n{} 文件，原始 {} 字节，资源包 {} 字节（{:.1}%）",
        output.display(),
        count,
        expanded,
        packed,
        100.0 * packed as f64 / expanded.max(1) as f64
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("打包失败：{error:#}");
        std::process::exit(1);
    }
}
