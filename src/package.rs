//! KPAK 1 solid archives and KPAK 2 independently streamed chunks.
pub mod selection;
mod staged;
mod streaming;
use crate::content::{ContentSource, DirectorySource, validate_path};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
pub use staged::StagedProject;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
pub use streaming::{Archive, CHUNK_SIZE, StreamArchive, write_streaming};

const MAGIC: &[u8; 8] = b"KPAK\r\n\x1a\0";
const HEADER: usize = 64;
pub const MAX_TOTAL: usize = 512 * 1024 * 1024;
const MAX_FILES: usize = 10000;
const MAX_FILE: usize = 128 * 1024 * 1024;

#[derive(Clone)]
pub struct Snapshot {
    data: Arc<Vec<u8>>,
    files: BTreeMap<String, (usize, usize)>,
}
impl ContentSource for Snapshot {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        validate_path(path)?;
        let (offset, length) = self
            .files
            .get(path.to_str().context("路径必须 UTF-8")?)
            .with_context(|| format!("包中缺少 {}", path.display()))?;
        Ok(self.data[*offset..offset + length].to_vec())
    }
}
pub(crate) fn excluded(name: &str, directory: bool) -> bool {
    (directory
        && matches!(
            name,
            ".git" | ".idea" | ".vscode" | "target" | "saves" | "node_modules"
        ))
        || name.ends_with(".tmp")
        || name.ends_with('~')
        || matches!(
            name,
            "settings.json" | "profile.json" | "save.json" | "read.json"
        )
        || (name.ends_with(".json") && (name.starts_with("slot-") || name.starts_with("auto-")))
}
pub fn files(root: &Path) -> Result<Vec<PathBuf>> {
    files_with_limit(root, MAX_FILES)
}
fn files_with_limit(root: &Path, limit: usize) -> Result<Vec<PathBuf>> {
    fn visit(root: &Path, dir: &Path, result: &mut Vec<PathBuf>, limit: usize) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("工程文件名必须 UTF-8"))?;
            if excluded(&name, kind.is_dir()) {
                continue;
            }
            ensure!(
                !kind.is_symlink(),
                "工程不允许符号链接：{}",
                entry.path().display()
            );
            if kind.is_dir() {
                visit(root, &entry.path(), result, limit)?;
            } else {
                ensure!(kind.is_file(), "工程包含特殊文件");
                let path = PathBuf::from(
                    entry
                        .path()
                        .strip_prefix(root)?
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
                validate_path(&path)?;
                result.push(path);
                ensure!(result.len() <= limit, "工程文件超过 {limit}");
            }
        }
        Ok(())
    }
    let root = root.canonicalize()?;
    let mut result = Vec::new();
    visit(&root, &root, &mut result, limit)?;
    result.sort();
    let mut case = BTreeSet::new();
    for p in &result {
        ensure!(
            case.insert(p.to_string_lossy().to_lowercase()),
            "路径大小写碰撞：{}",
            p.display()
        );
    }
    Ok(result)
}
impl Snapshot {
    pub fn directory(root: &Path) -> Result<Self> {
        Self::directory_selected(root, &files(root)?)
    }
    pub fn directory_selected(root: &Path, paths: &[PathBuf]) -> Result<Self> {
        ensure!(paths.len() <= MAX_FILES, "工程文件超过 {MAX_FILES}");
        selection::validate_selected(root, paths)?;
        let source = DirectorySource::new(root)?;
        let mut data = Vec::new();
        let mut table = BTreeMap::new();
        for path in paths {
            let bytes = source.read(path)?;
            ensure!(data.len() + bytes.len() <= MAX_TOTAL, "工程展开超过 512MiB");
            table.insert(
                path.to_str().context("UTF-8")?.to_owned(),
                (data.len(), bytes.len()),
            );
            data.extend(bytes);
        }
        ensure!(table.contains_key("story.xml"), "工程缺少 story.xml");
        Ok(Self {
            data: Arc::new(data),
            files: table,
        })
    }
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        for (path, (offset, len)) in &self.files {
            hash.update((path.len() as u32).to_le_bytes());
            hash.update(path.as_bytes());
            hash.update((*len as u64).to_le_bytes());
            hash.update(&self.data[*offset..offset + len]);
        }
        hash.finalize().into()
    }
    /// Publish only the compiled entry XML; retain unrelated UI/content files.
    pub fn for_publication(&self) -> Result<(Self, usize)> {
        let compiled = crate::story_chain::compile(self)?;
        let xml = compiled.publication_xml()?;
        let mut data = Vec::new();
        let mut files = BTreeMap::new();
        let chapters: BTreeSet<_> = compiled.files.iter().map(String::as_str).collect();
        for (path, (offset, len)) in &self.files {
            if path == selection::MANIFEST
                || (path != "story.xml" && chapters.contains(path.as_str()))
            {
                continue;
            }
            let bytes = if path == "story.xml" {
                xml.as_bytes()
            } else {
                &self.data[*offset..offset + len]
            };
            ensure!(bytes.len() <= MAX_FILE, "合并后的单文件超过 128MiB");
            ensure!(data.len() + bytes.len() <= MAX_TOTAL, "发布内容超过 512MiB");
            files.insert(path.clone(), (data.len(), bytes.len()));
            data.extend(bytes);
        }
        Ok((
            Self {
                data: Arc::new(data),
                files,
            },
            compiled.files.len(),
        ))
    }
    pub fn count(&self) -> usize {
        self.files.len()
    }
    pub fn expanded_bytes(&self) -> usize {
        self.files.values().map(|(_, len)| len).sum()
    }
    pub fn encode(&self, level: i32) -> Result<Vec<u8>> {
        ensure!((1..=22).contains(&level), "压缩级别须在 1–22");
        let mut payload = Vec::new();
        payload.extend((self.files.len() as u32).to_le_bytes());
        let mut blocks = Vec::<Vec<u8>>::new();
        let mut hashes = BTreeMap::<[u8; 32], usize>::new();
        for (path, (offset, len)) in &self.files {
            let bytes = &self.data[*offset..offset + len];
            let hash: [u8; 32] = Sha256::digest(bytes).into();
            let block = if let Some(index) = hashes.get(&hash) {
                ensure!(blocks[*index] == bytes, "内容摘要碰撞");
                *index
            } else {
                let index = blocks.len();
                blocks.push(bytes.to_vec());
                hashes.insert(hash, index);
                index
            };
            ensure!(path.len() <= 1024, "路径超过 1024 字节");
            payload.extend((path.len() as u16).to_le_bytes());
            payload.extend(path.as_bytes());
            payload.extend((block as u32).to_le_bytes());
        }
        payload.extend((blocks.len() as u32).to_le_bytes());
        for bytes in blocks {
            payload.extend((bytes.len() as u64).to_le_bytes());
            payload.extend(bytes);
        }
        ensure!(payload.len() <= MAX_TOTAL, "包展开超过 512MiB");
        let compressed = zstd::bulk::compress(&payload, level)?;
        let mut result = Vec::with_capacity(HEADER + compressed.len());
        result.extend(MAGIC);
        result.extend(1u32.to_le_bytes());
        result.extend(0u32.to_le_bytes());
        result.extend((compressed.len() as u64).to_le_bytes());
        result.extend((payload.len() as u64).to_le_bytes());
        result.extend(Sha256::digest(&payload));
        result.extend(compressed);
        Ok(result)
    }
    pub fn open(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take((MAX_TOTAL + HEADER + 1) as u64)
            .read_to_end(&mut bytes)?;
        Self::decode(&bytes).with_context(|| format!("资源包 {}", path.display()))
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() >= HEADER && &bytes[..8] == MAGIC,
            "KPAK magic/头错误"
        );
        let u32at = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        let u64at = |i| u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
        ensure!(u32at(8) == 1 && u32at(12) == 0, "不支持的 KPAK 版本/flags");
        let packed = usize::try_from(u64at(16))?;
        let expanded = usize::try_from(u64at(24))?;
        ensure!(
            packed <= MAX_TOTAL && expanded <= MAX_TOTAL && bytes.len() == HEADER + packed,
            "资源包长度或预算错误"
        );
        let frame_size = zstd::zstd_safe::find_frame_compressed_size(&bytes[HEADER..])
            .map_err(|e| anyhow::anyhow!("zstd 帧错误：{e:?}"))?;
        ensure!(frame_size == packed, "包包含尾随压缩数据");
        let payload = zstd::bulk::decompress(&bytes[HEADER..], expanded)?;
        ensure!(
            payload.len() == expanded && Sha256::digest(&payload)[..] == bytes[32..64],
            "包摘要/展开长度不匹配"
        );
        let mut cursor = Cursor {
            bytes: &payload,
            pos: 0,
        };
        let count = cursor.u32()? as usize;
        ensure!(count > 0 && count <= MAX_FILES, "文件数错误");
        let mut names = Vec::new();
        let mut case = BTreeSet::new();
        for _ in 0..count {
            let length = cursor.u16()? as usize;
            ensure!(length > 0 && length <= 1024, "路径长度错误");
            let name = std::str::from_utf8(cursor.take(length)?)?.to_owned();
            validate_path(Path::new(&name))?;
            ensure!(
                names
                    .last()
                    .is_none_or(|(prev, _): &(String, usize)| prev < &name)
                    && case.insert(name.to_lowercase()),
                "路径重复、大小写碰撞或排序错误"
            );
            names.push((name, cursor.u32()? as usize));
        }
        let blocks = cursor.u32()? as usize;
        ensure!(blocks > 0 && blocks <= count, "块数错误");
        let mut spans = Vec::new();
        for _ in 0..blocks {
            let len = usize::try_from(cursor.u64()?)?;
            ensure!(len <= MAX_FILE, "单文件超过 128MiB");
            let offset = cursor.pos;
            cursor.take(len)?;
            spans.push((offset, len));
        }
        ensure!(cursor.pos == payload.len(), "包包含尾随数据");
        let mut files = BTreeMap::new();
        let mut used = BTreeSet::new();
        for (name, index) in names {
            files.insert(name, *spans.get(index).context("非法块引用")?);
            used.insert(index);
        }
        ensure!(
            files.values().map(|(_, len)| *len as u64).sum::<u64>() <= MAX_TOTAL as u64,
            "逻辑展开总量超预算"
        );
        ensure!(
            used.len() == blocks && files.contains_key("story.xml"),
            "存在孤立块或缺少 story.xml"
        );
        Ok(Self {
            data: Arc::new(payload),
            files,
        })
    }
}
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(len).context("包长度溢出")?;
        let result = self.bytes.get(self.pos..end).context("资源包截断")?;
        self.pos = end;
        Ok(result)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into()?))
    }
}

pub fn write(path: &Path, bytes: &[u8], overwrite: bool) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    if overwrite {
        temp.persist(path).map_err(|e| e.error)?;
    } else {
        temp.persist_noclobber(path).map_err(|e| e.error)?;
    }
    Ok(())
}
