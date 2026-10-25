//! Indexed KPAK 2, with bounded, independently verified Zstandard chunks.
use super::{Cursor, MAGIC, Snapshot, StagedProject};
use crate::content::{ContentSource, MAX_READ_BYTES, MAX_STREAM_BYTES, validate_path};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{Arc, Mutex},
};

const HEADER: usize = 80;
pub const CHUNK_SIZE: usize = 1024 * 1024;
pub(super) const MAX_FILES: usize = 100000;
pub(super) const MAX_TOTAL: u64 = 1024 * 1024 * 1024 * 1024;
const MAX_CHUNKS: usize = 1048576;
const MAX_INDEX: usize = 128 * 1024 * 1024;
const MAX_PACKED_CHUNK: u32 = (2 * CHUNK_SIZE) as u32;

/// Only the package container is compatible; both versions use current story/UI/save.
pub enum Archive {
    Solid(Snapshot),
    Streaming(StreamArchive),
}
impl Archive {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_file(path).with_context(|| format!("资源包 {}", path.display()))
    }
    fn open_file(path: &Path) -> Result<Self> {
        let mut file = open_file(path)?;
        let mut header = [0u8; 12];
        file.read_exact(&mut header)?;
        ensure!(&header[..8] == MAGIC, "KPAK magic 错误");
        match u32::from_le_bytes(header[8..12].try_into()?) {
            1 => {
                // Decode the same opened object, even if the path is atomically replaced.
                file.rewind()?;
                let mut bytes = Vec::new();
                file.take((super::MAX_TOTAL + 65) as u64)
                    .read_to_end(&mut bytes)?;
                Ok(Self::Solid(Snapshot::decode(&bytes)?))
            }
            2 => Ok(Self::Streaming(StreamArchive::from_file(file)?)),
            version => anyhow::bail!("不支持的 KPAK 版本：{version}"),
        }
    }
    pub fn version(&self) -> u32 {
        match self {
            Self::Solid(_) => 1,
            Self::Streaming(_) => 2,
        }
    }
    pub fn verify_all(&self) -> Result<()> {
        match self {
            Self::Solid(_) => Ok(()),
            Self::Streaming(a) => a.verify_all(),
        }
    }
}
impl ContentSource for Archive {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        match self {
            Self::Solid(a) => a.read(path),
            Self::Streaming(a) => a.read(path),
        }
    }
    fn open(&self, path: &Path) -> Result<Box<dyn Read + '_>> {
        match self {
            Self::Solid(a) => a.open(path),
            Self::Streaming(a) => a.open(path),
        }
    }
}
fn open_file(path: &Path) -> Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Allow atomic replacement, but prevent in-place edits of the held package.
        options.share_mode(1 | 4);
    }
    Ok(options.open(path)?)
}

#[derive(Clone)]
struct Chunk {
    offset: u64,
    packed: u32,
    raw: u32,
    hash: [u8; 32],
}
struct Member {
    length: u64,
    chunks: Vec<u32>,
}
struct Index {
    files: BTreeMap<String, Arc<Member>>,
    chunks: Vec<Chunk>,
}
#[derive(Clone)]
pub struct StreamArchive {
    file: Arc<Mutex<File>>,
    index: Arc<Index>,
}
impl StreamArchive {
    pub fn open(path: &Path) -> Result<Self> {
        Self::from_file(open_file(path)?).with_context(|| format!("KPAK 2 {}", path.display()))
    }
    fn from_file(mut file: File) -> Result<Self> {
        let actual = file.metadata()?.len();
        ensure!(
            (HEADER as u64..=MAX_TOTAL).contains(&actual),
            "KPAK 2 文件长度错误"
        );
        file.rewind()?;
        let mut header = [0u8; HEADER];
        file.read_exact(&mut header)?;
        let u32at = |p| u32::from_le_bytes(header[p..p + 4].try_into().unwrap());
        let u64at = |p| u64::from_le_bytes(header[p..p + 8].try_into().unwrap());
        ensure!(
            &header[..8] == MAGIC
                && u32at(8) == 2
                && u32at(12) == 0
                && u32at(40) == CHUNK_SIZE as u32
                && u32at(44) == 0,
            "KPAK 2 头/版本/flags 错误"
        );
        let offset = u64at(16);
        let length = u64at(24);
        ensure!(
            length <= MAX_INDEX as u64
                && offset >= HEADER as u64
                && offset.checked_add(length) == Some(actual)
                && u64at(32) == actual,
            "KPAK 2 索引长度/偏移或尾随数据错误"
        );
        let mut bytes = vec![0u8; length as usize];
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut bytes)?;
        ensure!(
            Sha256::digest(&bytes)[..] == header[48..80],
            "KPAK 2 索引摘要错误"
        );
        let mut cursor = Cursor {
            bytes: &bytes,
            pos: 0,
        };
        let count = cursor.u32()? as usize;
        ensure!(count > 0 && count <= MAX_FILES, "KPAK 2 文件数错误");
        let mut files = BTreeMap::new();
        let mut case = BTreeSet::new();
        let mut previous = String::new();
        let mut total = 0u64;
        for _ in 0..count {
            let len = cursor.u16()? as usize;
            ensure!((1..=1024).contains(&len), "KPAK 2 路径长度错误");
            let name = std::str::from_utf8(cursor.take(len)?)?.to_owned();
            validate_path(Path::new(&name))?;
            ensure!(
                name > previous && case.insert(name.to_lowercase()),
                "KPAK 2 路径排序/碰撞错误"
            );
            previous = name.clone();
            let length = cursor.u64()?;
            let references = cursor.u32()? as usize;
            ensure!(
                length <= MAX_STREAM_BYTES
                    && references as u64 == length.div_ceil(CHUNK_SIZE as u64),
                "KPAK 2 文件长度/块引用数量错误"
            );
            total = total.checked_add(length).context("KPAK 2 总量溢出")?;
            ensure!(total <= MAX_TOTAL, "KPAK 2 逻辑总量超过 1TiB");
            // Validate bytes before allocating the reference table.
            let raw = cursor.take(references * 4)?;
            let chunks = raw
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| u32::from_le_bytes(*b))
                .collect();
            files.insert(name, Arc::new(Member { length, chunks }));
        }
        let count = cursor.u32()? as usize;
        ensure!(count <= MAX_CHUNKS, "KPAK 2 块数超过预算");
        let table = cursor.take(count * 48)?;
        ensure!(cursor.pos == bytes.len(), "KPAK 2 索引尾随数据");
        let mut chunks = Vec::with_capacity(count);
        let mut next = HEADER as u64;
        for bytes in table.as_chunks::<48>().0 {
            let chunk = Chunk {
                offset: u64::from_le_bytes(bytes[..8].try_into()?),
                packed: u32::from_le_bytes(bytes[8..12].try_into()?),
                raw: u32::from_le_bytes(bytes[12..16].try_into()?),
                hash: bytes[16..48].try_into()?,
            };
            ensure!(
                chunk.offset == next
                    && (1..=MAX_PACKED_CHUNK).contains(&chunk.packed)
                    && (1..=CHUNK_SIZE as u32).contains(&chunk.raw),
                "KPAK 2 块布局/预算错误"
            );
            next = next
                .checked_add(u64::from(chunk.packed))
                .context("KPAK 2 块偏移溢出")?;
            ensure!(next <= offset, "KPAK 2 块越过索引");
            chunks.push(chunk);
        }
        ensure!(
            next == offset && files.contains_key("story.xml"),
            "KPAK 2 孤立数据或缺少 story.xml"
        );
        let mut used = vec![false; chunks.len()];
        for member in files.values() {
            for (i, id) in member.chunks.iter().enumerate() {
                let chunk = chunks.get(*id as usize).context("KPAK 2 非法块引用")?;
                let expected =
                    (member.length - i as u64 * CHUNK_SIZE as u64).min(CHUNK_SIZE as u64);
                ensure!(
                    u64::from(chunk.raw) == expected,
                    "KPAK 2 文件与块长度不匹配"
                );
                used[*id as usize] = true;
            }
        }
        ensure!(used.iter().all(|used| *used), "KPAK 2 孤立块");
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            index: Arc::new(Index { files, chunks }),
        })
    }
    fn chunk(&self, id: u32) -> Result<Vec<u8>> {
        let chunk = &self.index.chunks[id as usize];
        let mut file = self
            .file
            .lock()
            .map_err(|_| anyhow::anyhow!("KPAK 2 文件锁损坏"))?;
        let packed = read_chunk(&mut file, chunk)?;
        drop(file);
        decode_chunk(&packed, chunk)
    }
    pub fn verify_all(&self) -> Result<()> {
        for id in 0..self.index.chunks.len() {
            self.chunk(id as u32)?;
        }
        Ok(())
    }
    pub fn count(&self) -> usize {
        self.index.files.len()
    }
    pub fn chunk_count(&self) -> usize {
        self.index.chunks.len()
    }
    fn member(&self, path: &Path) -> Result<Arc<Member>> {
        validate_path(path)?;
        self.index
            .files
            .get(path.to_str().context("路径必须 UTF-8")?)
            .cloned()
            .with_context(|| format!("包中缺少 {}", path.display()))
    }
}
fn read_chunk(file: &mut File, chunk: &Chunk) -> Result<Vec<u8>> {
    let mut bytes = vec![0u8; chunk.packed as usize];
    file.seek(SeekFrom::Start(chunk.offset))?;
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}
fn decode_chunk(bytes: &[u8], chunk: &Chunk) -> Result<Vec<u8>> {
    ensure!(
        bytes.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]),
        "KPAK 2 非标准 Zstandard 帧"
    );
    let size = zstd::zstd_safe::find_frame_compressed_size(bytes)
        .map_err(|e| anyhow::anyhow!("KPAK 2 Zstandard 帧：{e:?}"))?;
    ensure!(size == bytes.len(), "KPAK 2 块含尾随帧/数据");
    let content_size = zstd::zstd_safe::get_frame_content_size(bytes)
        .map_err(|e| anyhow::anyhow!("KPAK 2 Zstandard 长度：{e:?}"))?;
    ensure!(
        content_size == Some(u64::from(chunk.raw)),
        "KPAK 2 帧原文长度错误"
    );
    let mut decoder = zstd::bulk::Decompressor::new()?;
    decoder.window_log_max(20)?;
    let data = decoder.decompress(bytes, chunk.raw as usize)?;
    ensure!(
        data.len() == chunk.raw as usize && Sha256::digest(&data)[..] == chunk.hash,
        "KPAK 2 块摘要/长度错误"
    );
    Ok(data)
}
impl ContentSource for StreamArchive {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        let member = self.member(path)?;
        ensure!(
            member.length <= MAX_READ_BYTES,
            "内容超过 128MiB，请使用流式入口：{}",
            path.display()
        );
        let mut bytes = Vec::with_capacity(member.length as usize);
        self.open(path)?
            .read_to_end(&mut bytes)
            .with_context(|| format!("KPAK 2 {}", path.display()))?;
        Ok(bytes)
    }
    fn open(&self, path: &Path) -> Result<Box<dyn Read + '_>> {
        Ok(Box::new(StreamReader {
            archive: self.clone(),
            member: self.member(path)?,
            next: 0,
            bytes: Vec::new(),
            position: 0,
            failed: false,
        }))
    }
}
struct StreamReader {
    archive: StreamArchive,
    member: Arc<Member>,
    next: usize,
    bytes: Vec<u8>,
    position: usize,
    failed: bool,
}
impl Read for StreamReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.failed {
            return Err(std::io::Error::other("KPAK 2 流已失败"));
        }
        if self.position == self.bytes.len() {
            let Some(id) = self.member.chunks.get(self.next) else {
                return Ok(0);
            };
            // Release the previous plaintext before decompressing another chunk.
            self.bytes = Vec::new();
            self.bytes = match self.archive.chunk(*id) {
                Ok(bytes) => bytes,
                Err(e) => {
                    self.failed = true;
                    return Err(std::io::Error::other(format!("KPAK 2 块 {id}: {e:#}")));
                }
            };
            self.next += 1;
            self.position = 0;
        }
        let count = output.len().min(self.bytes.len() - self.position);
        output[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

/// Validate the entire candidate and source before a same-directory atomic commit.
pub fn write_streaming(
    stage: &StagedProject,
    path: &Path,
    level: i32,
    overwrite: bool,
    validate: impl FnOnce(&StreamArchive) -> Result<()>,
) -> Result<u64> {
    ensure!((1..=22).contains(&level), "压缩级别须在 1–22");
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    ensure!(
        !parent.canonicalize()?.starts_with(stage.original_root()),
        "输出包必须位于工程目录之外"
    );
    ensure!(
        overwrite || !path.exists(),
        "输出已存在，使用 --force 才能覆盖"
    );
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let length = encode(stage, temp.as_file_mut(), level)?;
    temp.as_file().sync_all()?;
    let archive = StreamArchive::from_file(temp.reopen()?)?;
    archive.verify_all()?;
    validate(&archive)?;
    drop(archive);
    stage.ensure_unchanged()?;
    if overwrite {
        temp.persist(path).map_err(|e| e.error)?;
    } else {
        temp.persist_noclobber(path).map_err(|e| e.error)?;
    }
    Ok(length)
}

fn encode(stage: &StagedProject, file: &mut File, level: i32) -> Result<u64> {
    file.write_all(&[0; HEADER])?;
    let mut compressor = zstd::bulk::Compressor::new(level)?;
    compressor.set_parameter(zstd::zstd_safe::CParameter::WindowLog(20))?;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut chunks = Vec::<Chunk>::new();
    let mut hashes = BTreeMap::<[u8; 32], u32>::new();
    let mut index = Vec::new();
    index.extend((stage.files.len() as u32).to_le_bytes());
    let mut offset = HEADER as u64;
    for (path, entry) in &stage.files {
        index.extend((path.len() as u16).to_le_bytes());
        index.extend(path.as_bytes());
        index.extend(entry.length.to_le_bytes());
        index.extend((entry.length.div_ceil(CHUNK_SIZE as u64) as u32).to_le_bytes());
        let mut input = stage.open(Path::new(path))?;
        let mut remaining = entry.length;
        let mut hash_file = Sha256::new();
        while remaining > 0 {
            let length = remaining.min(CHUNK_SIZE as u64) as usize;
            input.read_exact(&mut buffer[..length])?;
            let bytes = &buffer[..length];
            hash_file.update(bytes);
            let hash: [u8; 32] = Sha256::digest(bytes).into();
            let id = if let Some(id) = hashes.get(&hash) {
                let chunk = &chunks[*id as usize];
                ensure!(
                    decode_chunk(&read_chunk(file, chunk)?, chunk)? == bytes,
                    "KPAK 2 内容摘要碰撞"
                );
                *id
            } else {
                ensure!(chunks.len() < MAX_CHUNKS, "KPAK 2 块数超过预算");
                let packed = compressor.compress(bytes)?;
                ensure!(
                    packed.len() <= MAX_PACKED_CHUNK as usize,
                    "KPAK 2 压缩块超预算"
                );
                let id = chunks.len() as u32;
                file.seek(SeekFrom::Start(offset))?;
                file.write_all(&packed)?;
                chunks.push(Chunk {
                    offset,
                    packed: packed.len() as u32,
                    raw: length as u32,
                    hash,
                });
                hashes.insert(hash, id);
                offset += packed.len() as u64;
                ensure!(offset <= MAX_TOTAL, "KPAK 2 包超过 1TiB");
                id
            };
            index.extend(id.to_le_bytes());
            remaining -= length as u64;
            ensure!(index.len() <= MAX_INDEX, "KPAK 2 索引超过 128MiB");
        }
        ensure!(
            input.read(&mut [0u8; 1])? == 0 && hash_file.finalize()[..] == entry.hash,
            "KPAK 2 暂存文件改变：{path}"
        );
    }
    ensure!(
        index.len() + 4 + chunks.len() * 48 <= MAX_INDEX,
        "KPAK 2 索引超过 128MiB"
    );
    index.extend((chunks.len() as u32).to_le_bytes());
    for chunk in chunks {
        index.extend(chunk.offset.to_le_bytes());
        index.extend(chunk.packed.to_le_bytes());
        index.extend(chunk.raw.to_le_bytes());
        index.extend(chunk.hash);
    }
    ensure!(index.len() <= MAX_INDEX, "KPAK 2 索引超过 128MiB");
    let length = offset
        .checked_add(index.len() as u64)
        .context("KPAK 2 包长度溢出")?;
    ensure!(length <= MAX_TOTAL, "KPAK 2 包超过 1TiB");
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&index)?;
    let mut header = Vec::with_capacity(HEADER);
    header.extend(MAGIC);
    header.extend(2u32.to_le_bytes());
    header.extend(0u32.to_le_bytes());
    header.extend(offset.to_le_bytes());
    header.extend((index.len() as u64).to_le_bytes());
    header.extend(length.to_le_bytes());
    header.extend((CHUNK_SIZE as u32).to_le_bytes());
    header.extend(0u32.to_le_bytes());
    header.extend(Sha256::digest(&index));
    file.rewind()?;
    file.write_all(&header)?;
    Ok(length)
}
