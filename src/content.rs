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
            "流式内容超过 64GiB：{}",
            path.display()
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
        "内容路径必须为游戏内相对路径：{}",
        path.display()
    );
    Ok(())
}

pub fn text(source: &dyn ContentSource, path: &Path) -> Result<String> {
    String::from_utf8(source.read(path)?).with_context(|| format!("{} 不是 UTF-8", path.display()))
}

pub struct DirectorySource {
    root: PathBuf,
}

impl DirectorySource {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            root: root
                .canonicalize()
                .with_context(|| format!("游戏目录 {}", root.display()))?,
        })
    }
}

impl ContentSource for DirectorySource {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        let file = self.file(path)?;
        ensure!(
            file.metadata()?.len() <= MAX_READ_BYTES,
            "内容超过 128MiB：{}",
            path.display()
        );
        let mut bytes = Vec::new();
        file.take(MAX_READ_BYTES + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_READ_BYTES,
            "内容超过 128MiB：{}",
            path.display()
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
        let resolved = self
            .root
            .join(path)
            .canonicalize()
            .with_context(|| format!("读取内容 {}", path.display()))?;
        ensure!(
            resolved.starts_with(&self.root),
            "内容路径越出游戏目录：{}",
            path.display()
        );
        ensure!(resolved.is_file(), "内容不是普通文件：{}", path.display());
        let file = std::fs::File::open(&resolved)?;
        ensure!(
            file.metadata()?.len() <= MAX_STREAM_BYTES,
            "内容超过 64GiB：{}",
            path.display()
        );
        Ok(file)
    }
}
