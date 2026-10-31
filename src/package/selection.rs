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
    ensure!(
        bytes.len() <= 1024 * 1024,
        "{}",
        crate::i18n::format(
            "host-package-selection-0001",
            &[("arg0", MANIFEST.to_string())]
        )
    );
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
            "{}",
            crate::i18n::format(
                "host-package-selection-0002",
                &[("arg0", format!("{}", path.display()))]
            )
        );
        if kind.is_dir() {
            for entry in std::fs::read_dir(path)? {
                licenses(root, &entry?.path(), output)?;
            }
        } else {
            ensure!(
                kind.is_file(),
                "{}",
                crate::i18n::format("host-package-selection-0003", &[])
            );
            output.insert(PathBuf::from(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
            ));
            ensure!(
                output.len() <= 100000,
                "{}",
                crate::i18n::format("host-package-selection-0004", &[])
            );
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
            "{}",
            crate::i18n::format(
                "host-package-selection-0005",
                &[("arg0", MANIFEST.to_string())]
            )
        );
        let mut seen = BTreeSet::new();
        for name in parsed.include {
            let path = PathBuf::from(&name);
            validate_path(&path)?;
            ensure!(
                name.len() <= 1024 && name != MANIFEST,
                "{}",
                crate::i18n::format("host-package-selection-0006", &[])
            );
            ensure!(
                seen.insert(name.to_lowercase()),
                "{}",
                crate::i18n::format("host-package-selection-0007", &[("arg0", name.to_string())])
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
        let name = path
            .to_str()
            .context(crate::i18n::text("host-package-selection-0015"))?;
        ensure!(
            name.len() <= 1024 && seen.insert(name.to_lowercase()),
            "{}",
            crate::i18n::format("host-package-selection-0008", &[("arg0", name.to_string())])
        );
        let mut current = root.to_owned();
        for part in path.components() {
            current.push(part.as_os_str());
            let kind = std::fs::symlink_metadata(&current)?.file_type();
            ensure!(
                !super::excluded(
                    part.as_os_str()
                        .to_str()
                        .context(crate::i18n::text("host-package-selection-0016"))?,
                    kind.is_dir()
                ),
                "{}",
                crate::i18n::format("host-package-selection-0009", &[("arg0", name.to_string())])
            );
            ensure!(
                !kind.is_symlink(),
                "{}",
                crate::i18n::format("host-package-selection-0010", &[("arg0", name.to_string())])
            );
        }
        ensure!(
            current.is_file(),
            "{}",
            crate::i18n::format("host-package-selection-0011", &[("arg0", name.to_string())])
        );
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
        ensure!(
            paths.len() <= 100000,
            "{}",
            crate::i18n::format("host-package-selection-0012", &[])
        );
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
            "{}",
            crate::i18n::format("host-package-selection-0013", &[])
        );
        self.ensure_unchanged()
    }
    pub fn ensure_unchanged(&self) -> Result<()> {
        let bytes = manifest(&self.root)?;
        ensure!(
            bytes.as_ref().map(|v| <[u8; 32]>::from(Sha256::digest(v))) == self.manifest_hash
                && extras(&self.root, bytes.as_deref())? == self.extra,
            "{}",
            crate::i18n::format("host-package-selection-0014", &[])
        );
        Ok(())
    }
}
