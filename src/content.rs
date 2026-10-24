//! Byte-oriented game content boundary. A future archive implements this same interface.
use anyhow::{Context, Result, ensure};
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
};

pub trait ContentSource {
    fn read(&self, path: &Path) -> Result<Vec<u8>>;
}

pub fn validate_path(path: &Path) -> Result<()> {
    let text = path.to_string_lossy();
    ensure!(
        !text.is_empty()
            && !text.contains(['\\', ':'])
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
        // Bound individual source bytes before allocating; decoders apply their own limits too.
        const LIMIT: u64 = 128 * 1024 * 1024;
        let mut bytes = Vec::new();
        std::fs::File::open(&resolved)?
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= LIMIT,
            "内容超过 128MiB：{}",
            path.display()
        );
        Ok(bytes)
    }
}
