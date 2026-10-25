use anyhow::Result;
use kakera_avg::{
    content::{ContentSource, DirectorySource},
    package::{self, Archive, CHUNK_SIZE, Snapshot, StagedProject, StreamArchive},
    runtime::{Bundle, LoadOptions},
};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};

fn fixture(root: &Path) -> Result<Vec<u8>> {
    std::fs::write(root.join("story.xml"), "entry")?;
    let bytes: Vec<_> = (0..CHUNK_SIZE * 2 + 37).map(|i| (i % 251) as u8).collect();
    std::fs::write(root.join("one.bin"), &bytes)?;
    std::fs::write(root.join("two.bin"), &bytes)?;
    std::fs::write(root.join("empty.bin"), [])?;
    Ok(bytes)
}
fn pack(root: &Path, output: &Path) -> Result<u64> {
    package::write_streaming(&StagedProject::capture(root)?, output, 1, false, |_| Ok(()))
}

#[test]
fn chunks_are_deterministic_deduplicated_and_streams_are_independent() -> Result<()> {
    let root = tempfile::tempdir()?;
    let output = tempfile::tempdir()?;
    let expected = fixture(root.path())?;
    let first = output.path().join("first.kpak");
    let second = output.path().join("second.kpak");
    pack(root.path(), &first)?;
    pack(root.path(), &second)?;
    assert_eq!(std::fs::read(&first)?, std::fs::read(&second)?);
    let archive = StreamArchive::open(&first)?;
    assert_eq!(archive.count(), 4);
    assert_eq!(archive.chunk_count(), 4); // Three data chunks shared by two members, one entry.
    assert!(archive.read(Path::new("empty.bin"))?.is_empty());
    assert_eq!(archive.read(Path::new("one.bin"))?, expected);
    assert!(archive.open(Path::new("../one.bin")).is_err());
    assert!(archive.open(Path::new("missing")).is_err());
    let mut a = archive.open(Path::new("one.bin"))?;
    let mut b = archive.open(Path::new("two.bin"))?;
    assert_eq!(a.read(&mut [])?, 0);
    let mut prefix = vec![0u8; CHUNK_SIZE + 11];
    a.read_exact(&mut prefix)?;
    assert_eq!(prefix, expected[..CHUNK_SIZE + 11]);
    let mut all = Vec::new();
    b.read_to_end(&mut all)?;
    assert_eq!(all, expected);
    let mut rest = Vec::new();
    a.read_to_end(&mut rest)?;
    assert_eq!(rest, expected[CHUNK_SIZE + 11..]);
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let archive = archive.clone();
            std::thread::spawn(move || archive.read(Path::new("two.bin")).unwrap())
        })
        .collect();
    for thread in threads {
        assert_eq!(thread.join().unwrap(), expected);
    }
    archive.verify_all()?;
    let dispatch = Archive::open(&first)?;
    assert_eq!(dispatch.version(), 2);
    dispatch.verify_all()?;
    Ok(())
}

fn index_offset(bytes: &[u8]) -> usize {
    u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize
}
fn repair_digest(bytes: &mut [u8]) {
    let offset = index_offset(bytes);
    let hash = Sha256::digest(&bytes[offset..]);
    bytes[48..80].copy_from_slice(&hash);
}
// Locate fields of a small independently specified binary fixture for corruption tests.
fn positions(bytes: &[u8]) -> (Vec<(usize, usize, usize)>, usize) {
    let mut p = index_offset(bytes);
    let count = u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
    p += 4;
    let mut members = Vec::new();
    for _ in 0..count {
        let len = u16::from_le_bytes(bytes[p..p + 2].try_into().unwrap()) as usize;
        p += 2;
        let name = p;
        p += len;
        let length = p;
        p += 8;
        let references = u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()) as usize;
        p += 4;
        members.push((name, length, p));
        p += references * 4;
    }
    (members, p + 4)
}

#[test]
fn open_validates_only_metadata_and_access_or_full_check_detects_payload_damage() -> Result<()> {
    let root = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    fixture(root.path())?;
    let path = out.path().join("game.kpak");
    pack(root.path(), &path)?;
    let mut bytes = std::fs::read(&path)?;
    bytes[80] ^= 1; // A binary member's block, not story.xml.
    std::fs::write(&path, bytes)?;
    let archive = StreamArchive::open(&path)?;
    assert_eq!(archive.read(Path::new("story.xml"))?, b"entry");
    let mut stream = archive.open(Path::new("one.bin"))?;
    assert!(stream.read(&mut [0u8; 1]).is_err());
    assert!(stream.read(&mut [0u8; 1]).is_err());
    assert!(archive.verify_all().is_err());
    Ok(())
}

#[test]
fn hostile_headers_paths_indices_lengths_and_layouts_are_rejected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    fixture(root.path())?;
    let path = out.path().join("game.kpak");
    pack(root.path(), &path)?;
    let good = std::fs::read(&path)?;
    let (members, chunks) = positions(&good);
    let mut cases = Vec::new();
    for position in [0, 8, 12, 16, 24, 32, 40, 44, 48] {
        let mut bytes = good.clone();
        bytes[position] ^= 128;
        cases.push(bytes);
    }
    for len in [0, 7, 79, good.len() - 1] {
        cases.push(good[..len].to_vec());
    }
    let mut tail = good.clone();
    tail.push(0);
    cases.push(tail);
    for (field, value) in [
        (16, u64::MAX),
        (24, 128 * 1024 * 1024 + 1),
        (32, 1u64 << 40 | 1),
    ] {
        let mut bytes = good.clone();
        bytes[field..field + 8].copy_from_slice(&value.to_le_bytes());
        cases.push(bytes);
    }
    for (field, replacement) in [
        (members[0].0, b"../xx.bin".as_slice()), // empty.bin has nine bytes.
        (members[1].0, b"One.bin".as_slice()),   // case collision with changed two.bin below.
    ] {
        let mut bytes = good.clone();
        bytes[field..field + replacement.len()].copy_from_slice(replacement);
        if field == members[1].0 {
            bytes[members[3].0..members[3].0 + 7].copy_from_slice(b"one.bin");
        }
        repair_digest(&mut bytes);
        cases.push(bytes);
    }
    let mut bytes = good.clone();
    bytes[members[1].2..members[1].2 + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    repair_digest(&mut bytes);
    cases.push(bytes);
    let mut bytes = good.clone();
    bytes[members[1].1..members[1].1 + 8].copy_from_slice(&((64u64 << 30) + 1).to_le_bytes());
    repair_digest(&mut bytes);
    cases.push(bytes);
    for (at, raw) in [
        (chunks, 81u64.to_le_bytes().to_vec()),
        (chunks + 8, 0u32.to_le_bytes().to_vec()),
        (chunks + 12, (CHUNK_SIZE as u32 + 1).to_le_bytes().to_vec()),
    ] {
        let mut bytes = good.clone();
        bytes[at..at + raw.len()].copy_from_slice(&raw);
        repair_digest(&mut bytes);
        cases.push(bytes);
    }
    let mut bytes = good.clone();
    let i = index_offset(&bytes);
    bytes[i..i + 4].copy_from_slice(&100001u32.to_le_bytes());
    repair_digest(&mut bytes);
    cases.push(bytes);
    let mut bytes = good.clone();
    bytes[chunks - 4..chunks].copy_from_slice(&1048577u32.to_le_bytes());
    repair_digest(&mut bytes);
    cases.push(bytes);
    // Keep lengths valid but orphan the first full-size block.
    let mut bytes = good.clone();
    for member in [1, 3] {
        bytes[members[member].2..members[member].2 + 4].copy_from_slice(&1u32.to_le_bytes());
    }
    repair_digest(&mut bytes);
    cases.push(bytes);
    let mut bytes = good.clone();
    bytes[chunks + 48..chunks + 56].copy_from_slice(&80u64.to_le_bytes());
    repair_digest(&mut bytes);
    cases.push(bytes);
    for (i, bytes) in cases.iter().enumerate() {
        std::fs::write(&path, bytes)?;
        assert!(StreamArchive::open(&path).is_err(), "hostile case {i}");
    }
    Ok(())
}

fn single(packed: &[u8], raw: u32, digest: [u8; 32]) -> Vec<u8> {
    let mut index = 1u32.to_le_bytes().to_vec();
    index.extend(9u16.to_le_bytes());
    index.extend(b"story.xml");
    index.extend(u64::from(raw).to_le_bytes());
    index.extend(1u32.to_le_bytes());
    index.extend(0u32.to_le_bytes());
    index.extend(1u32.to_le_bytes());
    index.extend(80u64.to_le_bytes());
    index.extend((packed.len() as u32).to_le_bytes());
    index.extend(raw.to_le_bytes());
    index.extend(digest);
    let mut bytes = b"KPAK\r\n\x1a\0".to_vec();
    bytes.extend(2u32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend((80u64 + packed.len() as u64).to_le_bytes());
    bytes.extend((index.len() as u64).to_le_bytes());
    bytes.extend((80u64 + packed.len() as u64 + index.len() as u64).to_le_bytes());
    bytes.extend((CHUNK_SIZE as u32).to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(Sha256::digest(&index));
    bytes.extend(packed);
    bytes.extend(index);
    bytes
}
#[test]
fn payload_rejects_trailing_frames_wrong_digests_and_inflation_before_delivery() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("test.kpak");
    let text = b"entry";
    let packed = zstd::bulk::compress(text, 1)?;
    let mut trailing = packed.clone();
    trailing.extend(&packed);
    for (packed, raw, hash) in [
        (trailing, 5, Sha256::digest(text).into()),
        (packed.clone(), 5, [0; 32]),
        (
            zstd::bulk::compress(&vec![0; CHUNK_SIZE + 1], 1)?,
            5,
            Sha256::digest(text).into(),
        ),
        (packed, 6, Sha256::digest(text).into()),
    ] {
        std::fs::write(&path, single(&packed, raw, hash))?;
        let archive = StreamArchive::open(&path)?;
        assert!(archive.read(Path::new("story.xml")).is_err());
    }
    Ok(())
}

#[test]
fn kpak1_remains_readable_and_unknown_versions_never_fall_back() -> Result<()> {
    let root = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    std::fs::write(root.path().join("story.xml"), b"entry")?;
    let bytes = Snapshot::directory(root.path())?.encode(1)?;
    let path = out.path().join("old.kpak");
    std::fs::write(&path, bytes)?;
    let archive = Archive::open(&path)?;
    assert_eq!(archive.version(), 1);
    assert_eq!(archive.read(Path::new("story.xml"))?, b"entry");
    archive.verify_all()?;
    drop(archive);
    let mut bytes = std::fs::read(&path)?;
    bytes[8..12].copy_from_slice(&3u32.to_le_bytes());
    std::fs::write(&path, bytes)?;
    assert!(Archive::open(&path).is_err());
    Ok(())
}

#[test]
fn staging_and_atomic_publication_preserve_source_and_old_output_on_failure() -> Result<()> {
    let root = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    fixture(root.path())?;
    std::fs::write(root.path().join("read.json"), b"private")?;
    let stage = StagedProject::capture(root.path())?;
    assert_eq!(stage.count(), 4);
    assert!(stage.read(Path::new("read.json")).is_err());
    let path = out.path().join("game.kpak");
    std::fs::write(&path, b"old")?;
    assert!(package::write_streaming(&stage, &path, 1, false, |_| Ok(())).is_err());
    assert!(
        package::write_streaming(&stage, &path, 1, true, |_| anyhow::bail!(
            "candidate rejected"
        ))
        .is_err()
    );
    assert_eq!(std::fs::read(&path)?, b"old");
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)?;
        assert!(package::write_streaming(&stage, &path, 1, true, |_| Ok(())).is_err());
        drop(locked);
        assert_eq!(std::fs::read(&path)?, b"old");
    }
    std::fs::write(root.path().join("story.xml"), b"changed")?;
    assert_eq!(stage.read(Path::new("story.xml"))?, b"entry");
    assert!(stage.ensure_unchanged().is_err());
    assert!(package::write_streaming(&stage, &path, 1, true, |_| Ok(())).is_err());
    assert_eq!(std::fs::read(&path)?, b"old");
    assert_eq!(std::fs::read_dir(out.path())?.count(), 1);
    assert!(
        package::write_streaming(&stage, &root.path().join("inside.kpak"), 1, false, |_| Ok(
            ()
        ))
        .is_err()
    );
    Ok(())
}

#[test]
fn real_demo_and_media_load_from_kpak2_with_compiled_chapters() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let out = tempfile::tempdir()?;
    let mut stage = StagedProject::capture(&root)?;
    assert_eq!(stage.for_publication()?, 5);
    for name in ["story.xml", "actors.xml", "media.xml", "visuals.xml"] {
        assert!(stage.read(&Path::new("chapters").join(name)).is_err());
    }
    stage.ensure_unchanged()?;
    let path = out.path().join("demo.kpak");
    let options = LoadOptions {
        core: kakera_avg::runtime::default_core(),
        no_live2d: false,
    };
    package::write_streaming(&stage, &path, 1, false, |archive| {
        let bundle = Bundle::load(archive, &options)?;
        assert_eq!(bundle.loaded.story.frames.len(), 30);
        assert_eq!(bundle.assets.media.videos.len(), 2);
        assert_eq!(bundle.assets.live2d.models.len(), 1);
        for video in bundle.assets.media.videos.values() {
            assert!(video.size[0] > 0 && video.size[1] > 0);
            assert!(!video.first.pixels.is_empty());
        }
        Ok(())
    })?;
    let archive = Archive::open(&path)?;
    assert_eq!(archive.version(), 2);
    Ok(())
}

#[test]
fn pack_cli_defaults_to_two_and_explicit_one_stays_available() -> Result<()> {
    let root = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
        root.path().join("font.otf"),
    )?;
    std::fs::write(
        root.path().join("story.xml"),
        r#"<story schema="8" id="pack-cli" content-version="1" entry="opening"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/></assets><characters/><visuals/><frames><frame id="opening"><text>入口</text></frame></frames><next end="true"/></story>"#,
    )?;
    std::fs::write(root.path().join("story.js"), "on.end('opening');")?;
    std::fs::write(
        root.path().join("ui.xml"),
        r#"<ui schema="3" initial="hud"><screen id="hud" modal="false"/></ui>"#,
    )?;
    for version in [2, 1] {
        let path = out.path().join(format!("format-{version}.kpak"));
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_kakera-pack"));
        command
            .arg(root.path())
            .arg(&path)
            .args(["--level", "1", "--no-live2d"]);
        if version == 1 {
            command.args(["--format", "1"]);
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(Archive::open(&path)?.version(), version);
        let refused = command.output()?;
        assert!(!refused.status.success());
    }
    let bad = std::process::Command::new(env!("CARGO_BIN_EXE_kakera-pack"))
        .arg(root.path())
        .arg(out.path().join("bad.kpak"))
        .args(["--format", "3"])
        .output()?;
    assert!(!bad.status.success());
    assert!(!out.path().join("bad.kpak").exists());
    Ok(())
}

#[test]
#[ignore = "explicit large-content acceptance: copies and hashes 600MiB with bounded memory"]
fn large_content_exceeds_kpak1_limits_without_whole_file_or_package_buffers() -> Result<()> {
    let root = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    std::fs::write(root.path().join("story.xml"), b"entry")?;
    let mut large = std::fs::File::create(root.path().join("large.bin"))?;
    let length = 600u64 * 1024 * 1024 + 37;
    // Independently compressed blocks must remain physically large as well.
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut random = 0x192837465abcdefu64;
    for word in buffer.as_chunks_mut::<8>().0 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        word.copy_from_slice(&random.to_le_bytes());
    }
    for id in 0..600u64 {
        buffer[..8].copy_from_slice(&id.to_le_bytes());
        large.write_all(&buffer)?;
    }
    let mut tail = [0u8; 37];
    tail[36] = 7;
    large.write_all(&tail)?;
    drop(buffer);
    drop(large);
    let stage = StagedProject::capture(root.path())?;
    assert!(stage.expanded_bytes() > package::MAX_TOTAL as u64);
    assert!(
        DirectorySource::new(root.path())?
            .read(Path::new("large.bin"))
            .is_err()
    );
    let path = out.path().join("large.kpak");
    let packed = package::write_streaming(&stage, &path, 1, false, |_| Ok(()))?;
    let archive = StreamArchive::open(&path)?;
    assert!(archive.read(Path::new("large.bin")).is_err());
    assert!(packed > package::MAX_TOTAL as u64);
    assert_eq!(
        archive.copy_to(Path::new("large.bin"), &mut std::io::sink())?,
        length
    );
    let mut stream = archive.open(Path::new("large.bin"))?;
    std::io::copy(&mut stream.by_ref().take(length - 1), &mut std::io::sink())?;
    let mut last = [0];
    stream.read_exact(&mut last)?;
    assert_eq!(last, [7]);
    println!(
        "large logical={length}, packed={packed}, unique chunks={}",
        archive.chunk_count()
    );
    Ok(())
}
