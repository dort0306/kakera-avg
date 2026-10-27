//! Publication dependencies are observed through the same content boundary as the player.
use crate::{
    content::{ContentSource, DirectorySource, validate_path},
    runtime::{Bundle, LoadOptions},
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
};

pub const MANIFEST: &str = "kakera.package.json";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    include: Vec<String>,
}
struct Recording<'a> {
    source: &'a dyn ContentSource,
    paths: RefCell<BTreeSet<PathBuf>>,
}
impl ContentSource for Recording<'_> {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        self.paths.borrow_mut().insert(path.to_owned());
        self.source.read(path)
    }
    fn open(&self, path: &Path) -> Result<Box<dyn Read + '_>> {
        self.paths.borrow_mut().insert(path.to_owned());
        self.source.open(path)
    }
}
fn dependencies(source: &dyn ContentSource, options: &LoadOptions) -> Result<BTreeSet<PathBuf>> {
    let recording = Recording {
        source,
        paths: RefCell::new(BTreeSet::new()),
    };
    Bundle::load(&recording, options)?;
    Ok(recording.paths.into_inner())
}
fn manifest(root: &Path) -> Result<Option<Vec<u8>>> {
    let path = root.join(MANIFEST);
    if !path.try_exists()? {
        return Ok(None);
    }
    validate_selected(root, &[PathBuf::from(MANIFEST)])?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "{MANIFEST} 超过 1MiB");
    Ok(Some(bytes))
}
fn extras(root: &Path, bytes: Option<&[u8]>) -> Result<BTreeSet<PathBuf>> {
    fn licenses(root: &Path, path: &Path, output: &mut BTreeSet<PathBuf>) -> Result<()> {
        if !path.try_exists()? {
            return Ok(());
        }
        let kind = std::fs::symlink_metadata(path)?.file_type();
        ensure!(
            !kind.is_symlink(),
            "许可证目录不允许符号链接：{}",
            path.display()
        );
        if kind.is_dir() {
            for entry in std::fs::read_dir(path)? {
                licenses(root, &entry?.path(), output)?;
            }
        } else {
            ensure!(kind.is_file(), "许可证不是普通文件");
            output.insert(PathBuf::from(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
            ));
            ensure!(output.len() <= 100000, "许可证文件超过预算");
        }
        Ok(())
    }
    let mut paths = BTreeSet::new();
    for dir in ["assets/licenses", "licenses"] {
        licenses(root, &root.join(dir), &mut paths)?;
    }
    if let Some(bytes) = bytes {
        let parsed: Manifest = serde_json::from_slice(bytes).context(MANIFEST)?;
        ensure!(
            parsed.schema == 1 && parsed.include.len() <= 100000,
            "{MANIFEST} schema/条数无效"
        );
        let mut seen = BTreeSet::new();
        for name in parsed.include {
            let path = PathBuf::from(&name);
            validate_path(&path)?;
            ensure!(name.len() <= 1024 && name != MANIFEST, "附加文件路径无效");
            ensure!(
                seen.insert(name.to_lowercase()),
                "附加文件重复/大小写冲突：{name}"
            );
            paths.insert(path);
        }
    }
    validate_selected(root, &paths.iter().cloned().collect::<Vec<_>>())?;
    Ok(paths)
}
pub(super) fn validate_selected(root: &Path, paths: &[PathBuf]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for path in paths {
        validate_path(path)?;
        let name = path.to_str().context("路径必须 UTF-8")?;
        ensure!(
            name.len() <= 1024 && seen.insert(name.to_lowercase()),
            "路径过长/大小写冲突：{name}"
        );
        let mut current = root.to_owned();
        for part in path.components() {
            current.push(part.as_os_str());
            let kind = std::fs::symlink_metadata(&current)?.file_type();
            ensure!(
                !super::excluded(
                    part.as_os_str().to_str().context("路径必须 UTF-8")?,
                    kind.is_dir()
                ),
                "发布依赖/附加文件使用保留名称：{name}"
            );
            ensure!(!kind.is_symlink(), "不允许符号链接：{name}");
        }
        ensure!(current.is_file(), "附加/依赖项不是普通文件：{name}");
    }
    Ok(())
}
pub struct Selection {
    pub paths: BTreeSet<PathBuf>,
    required: BTreeSet<PathBuf>,
    extra: BTreeSet<PathBuf>,
    manifest_hash: Option<[u8; 32]>,
    root: PathBuf,
}
impl Selection {
    pub fn discover(root: &Path, options: &LoadOptions) -> Result<Self> {
        let root = root.canonicalize()?;
        let bytes = manifest(&root)?;
        let extra = extras(&root, bytes.as_deref())?;
        let required = dependencies(&DirectorySource::new(&root)?, options)?;
        let mut paths = required.union(&extra).cloned().collect::<BTreeSet<_>>();
        if bytes.is_some() {
            paths.insert(MANIFEST.into());
        }
        ensure!(paths.len() <= 100000, "发布文件超过 100000");
        validate_selected(&root, &paths.iter().cloned().collect::<Vec<_>>())?;
        Ok(Self {
            paths,
            required,
            extra,
            manifest_hash: bytes.map(|v| Sha256::digest(v).into()),
            root,
        })
    }
    pub fn validate_candidate(
        &self,
        source: &dyn ContentSource,
        options: &LoadOptions,
    ) -> Result<()> {
        ensure!(
            dependencies(source, options)? == self.required,
            "依赖发现期间工程改变，请重试"
        );
        self.ensure_unchanged()
    }
    pub fn ensure_unchanged(&self) -> Result<()> {
        let bytes = manifest(&self.root)?;
        ensure!(
            bytes.as_ref().map(|v| <[u8; 32]>::from(Sha256::digest(v))) == self.manifest_hash
                && extras(&self.root, bytes.as_deref())? == self.extra,
            "发布清单/许可证列表改变，请重试"
        );
        Ok(())
    }
}
