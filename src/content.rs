//! Owned bytes for small assets; bounded streams for large resources.
use anyhow::{Context, Result, ensure};
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub trait ContentSource {
    fn read(&self, path: &Path) -> Result<Vec<u8>>;

    fn open(&self, path: &Path) -> Result<Box<dyn Read + '_>> {
        Ok(Box::new(std::io::Cursor::new(self.read(path)?)))
    }

    fn copy_to(&self, path: &Path, output: &mut dyn Write) -> Result<u64> {
        let bytes = std::io::copy(&mut self.open(path)?.take(MAX_STREAM_BYTES + 1), output)?;
        ensure!(
            bytes <= MAX_STREAM_BYTES,
            "{}",
            crate::i18n::format(
                "host-content-0001",
                &[("arg0", format!("{}", path.display()))]
            )
        );
        Ok(bytes)
    }
}

pub const MAX_READ_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_STREAM_BYTES: u64 = 64 * 1024 * 1024 * 1024;

pub fn validate_path(path: &Path) -> Result<()> {
    let text = path.to_string_lossy();
    ensure!(
        !text.is_empty()
            && !text.contains(['\\', ':', '<', '>', '|', '?', '*'])
            && !text.chars().any(char::is_control)
            && text.split('/').all(|part| !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' ']))
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "{}",
        crate::i18n::format(
            "host-content-0002",
            &[("arg0", format!("{}", path.display()))]
        )
    );
    Ok(())
}

pub fn text(source: &dyn ContentSource, path: &Path) -> Result<String> {
    String::from_utf8(source.read(path)?).with_context(|| {
        crate::i18n::format(
            "host-content-0003",
            &[("arg0", format!("{}", path.display()))],
        )
    })
}

pub struct DirectorySource {
    root: PathBuf,
}

impl DirectorySource {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            root: root.canonicalize().with_context(|| {
                crate::i18n::format(
                    "host-content-0004",
                    &[("arg0", format!("{}", root.display()))],
                )
            })?,
        })
    }
}

impl ContentSource for DirectorySource {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        let file = self.file(path)?;
        ensure!(
            file.metadata()?.len() <= MAX_READ_BYTES,
            "{}",
            crate::i18n::format(
                "host-content-0005",
                &[("arg0", format!("{}", path.display()))]
            )
        );
        let mut bytes = Vec::new();
        file.take(MAX_READ_BYTES + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_READ_BYTES,
            "{}",
            crate::i18n::format(
                "host-content-0006",
                &[("arg0", format!("{}", path.display()))]
            )
        );
        Ok(bytes)
    }

    fn open(&self, path: &Path) -> Result<Box<dyn Read + '_>> {
        Ok(Box::new(self.file(path)?))
    }
}

impl DirectorySource {
    fn file(&self, path: &Path) -> Result<std::fs::File> {
        validate_path(path)?;
        let resolved = self.root.join(path).canonicalize().with_context(|| {
            crate::i18n::format(
                "host-content-0007",
                &[("arg0", format!("{}", path.display()))],
            )
        })?;
        ensure!(
            resolved.starts_with(&self.root),
            "{}",
            crate::i18n::format(
                "host-content-0008",
                &[("arg0", format!("{}", path.display()))]
            )
        );
        ensure!(
            resolved.is_file(),
            "{}",
            crate::i18n::format(
                "host-content-0009",
                &[("arg0", format!("{}", path.display()))]
            )
        );
        let file = std::fs::File::open(&resolved)?;
        ensure!(
            file.metadata()?.len() <= MAX_STREAM_BYTES,
            "{}",
            crate::i18n::format(
                "host-content-0010",
                &[("arg0", format!("{}", path.display()))]
            )
        );
        Ok(file)
    }
}
