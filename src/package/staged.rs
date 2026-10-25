//! A private disk snapshot keeps large project candidates consistent without whole-game RAM.
use super::{
    files_with_limit,
    streaming::{MAX_FILES, MAX_TOTAL},
};
use crate::content::{ContentSource, DirectorySource, MAX_STREAM_BYTES};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub(super) struct Entry {
    pub length: u64,
    pub hash: [u8; 32],
}

pub struct StagedProject {
    source: DirectorySource,
    _directory: tempfile::TempDir,
    original: PathBuf,
    fingerprint: [u8; 32],
    pub(super) files: BTreeMap<String, Entry>,
}

fn scan(mut input: impl Read, mut output: Option<&mut dyn Write>) -> Result<Entry> {
    let mut hash = Sha256::new();
    let mut length = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let count = match input.read(&mut buffer) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        length += count as u64;
        ensure!(length <= MAX_STREAM_BYTES, "单文件超过 64GiB");
        hash.update(&buffer[..count]);
        if let Some(output) = output.as_deref_mut() {
            output.write_all(&buffer[..count])?;
        }
    }
    Ok(Entry {
        length,
        hash: hash.finalize().into(),
    })
}

fn fingerprint(files: &BTreeMap<String, Entry>) -> [u8; 32] {
    let mut hash = Sha256::new();
    for (path, entry) in files {
        hash.update((path.len() as u32).to_le_bytes());
        hash.update(path.as_bytes());
        hash.update(entry.length.to_le_bytes());
        hash.update(entry.hash);
    }
    hash.finalize().into()
}

impl StagedProject {
    pub fn capture(root: &Path) -> Result<Self> {
        let original = root.canonicalize()?;
        let source = DirectorySource::new(&original)?;
        let directory = tempfile::tempdir()?;
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        for path in files_with_limit(&original, MAX_FILES)? {
            let name = path.to_str().context("路径必须 UTF-8")?.to_owned();
            ensure!(name.len() <= 1024, "路径超过 1024 字节");
            let target = directory.path().join(&path);
            std::fs::create_dir_all(target.parent().context("暂存目录")?)?;
            let mut file = std::fs::File::create(target)?;
            let entry = scan(source.open(&path)?, Some(&mut file))
                .with_context(|| format!("暂存 {}", path.display()))?;
            total += entry.length;
            ensure!(total <= MAX_TOTAL, "工程逻辑总量超过 1TiB");
            files.insert(name, entry);
        }
        ensure!(files.contains_key("story.xml"), "工程缺少 story.xml");
        Ok(Self {
            source: DirectorySource::new(directory.path())?,
            _directory: directory,
            original,
            fingerprint: fingerprint(&files),
            files,
        })
    }

    pub fn ensure_unchanged(&self) -> Result<()> {
        let source = DirectorySource::new(&self.original)?;
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        for path in files_with_limit(&self.original, MAX_FILES)? {
            let name = path.to_str().context("路径必须 UTF-8")?.to_owned();
            ensure!(name.len() <= 1024, "路径超过 1024 字节");
            let entry = scan(source.open(&path)?, None)?;
            total += entry.length;
            ensure!(total <= MAX_TOTAL, "工程逻辑总量超过 1TiB");
            files.insert(name, entry);
        }
        ensure!(
            fingerprint(&files) == self.fingerprint,
            "校验期间工程改变，请重试"
        );
        Ok(())
    }

    pub fn for_publication(&mut self) -> Result<usize> {
        let compiled = crate::story_chain::compile(self)?;
        for path in &compiled.files {
            if path != "story.xml" {
                self.files.remove(path);
                std::fs::remove_file(self._directory.path().join(path))?;
            }
        }
        let bytes = compiled.xml.as_bytes();
        std::fs::write(self._directory.path().join("story.xml"), bytes)?;
        self.files.insert(
            "story.xml".into(),
            Entry {
                length: bytes.len() as u64,
                hash: Sha256::digest(bytes).into(),
            },
        );
        ensure!(self.expanded_bytes() <= MAX_TOTAL, "发布内容超过 1TiB");
        Ok(compiled.files.len())
    }

    pub fn count(&self) -> usize {
        self.files.len()
    }
    pub fn expanded_bytes(&self) -> u64 {
        self.files.values().map(|e| e.length).sum()
    }
    pub(super) fn original_root(&self) -> &Path {
        &self.original
    }
}

impl ContentSource for StagedProject {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        self.source.read(path)
    }
    fn open(&self, path: &Path) -> Result<Box<dyn Read + '_>> {
        self.source.open(path)
    }
}
