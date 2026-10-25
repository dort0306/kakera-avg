//! Auxiliary per-story read marks for Ctrl fast-forward. Never part of a save slot.
use crate::save::{atomic_json, read_json_bytes};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

const MAX_FRAMES: usize = 100_000;
const MAX_ID: usize = 256;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema_version: u32,
    story_id: String,
    frames: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub struct ReadHistory {
    story_id: String,
    frames: BTreeSet<String>,
    dirty: bool,
}

impl ReadHistory {
    pub fn new(story_id: &str) -> Self {
        Self {
            story_id: story_id.to_owned(),
            frames: BTreeSet::new(),
            dirty: false,
        }
    }

    pub fn load(path: &Path, story_id: &str) -> Result<Self> {
        let file: File = serde_json::from_slice(&read_json_bytes(path)?)
            .with_context(|| format!("读取已读进度 {}", path.display()))?;
        ensure!(
            file.schema_version == 1,
            "不支持的已读进度 schema：{}",
            file.schema_version
        );
        ensure!(
            file.story_id == story_id,
            "已读进度属于其他故事：{}",
            file.story_id
        );
        ensure!(
            file.frames.len() <= MAX_FRAMES,
            "已读进度超过 {MAX_FRAMES} 条"
        );
        ensure!(
            file.frames.iter().all(|id| valid_id(id)),
            "已读进度含非法 frame ID"
        );
        Ok(Self {
            story_id: story_id.to_owned(),
            frames: file.frames,
            dirty: false,
        })
    }

    pub fn contains(&self, frame_id: &str) -> bool {
        self.frames.contains(frame_id)
    }

    /// Returns whether the mark was new. Invalid ids and marks beyond the count budget
    /// are ignored; existing marks (including unknown frame ids) are never evicted.
    pub fn mark(&mut self, frame_id: &str) -> bool {
        if !valid_id(frame_id)
            || self.frames.len() >= MAX_FRAMES
            || !self.frames.insert(frame_id.to_owned())
        {
            return false;
        }
        self.dirty = true;
        true
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn save(&mut self, path: &Path) -> Result<()> {
        ensure!(
            self.frames.len() <= MAX_FRAMES,
            "已读进度超过 {MAX_FRAMES} 条，未改写旧文件"
        );
        ensure!(
            self.frames.iter().all(|id| valid_id(id)),
            "已读进度含非法 frame ID，未改写旧文件"
        );
        atomic_json(
            path,
            &File {
                schema_version: 1,
                story_id: self.story_id.clone(),
                frames: self.frames.clone(),
            },
        )?;
        self.dirty = false;
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_keeps_marks_and_reports_new_ones() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("read.json");
        let mut read = ReadHistory::new("demo");
        assert!(!read.contains("opening"));
        assert!(read.mark("opening"));
        assert!(!read.mark("opening"));
        assert!(read.dirty());
        read.save(&path)?;
        assert!(!read.dirty());
        let loaded = ReadHistory::load(&path, "demo")?;
        assert!(loaded.contains("opening"));
        assert!(!loaded.contains("question"));
        assert!(!loaded.dirty());
        // An empty id is not a frame and must not enter the file.
        assert!(!read.mark(""));
        Ok(())
    }

    #[test]
    fn rejects_invalid_identity_schema_and_fields() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("read.json");
        let cases = [
            r#"{"schema_version":2,"story_id":"demo","frames":[]}"#,
            r#"{"schema_version":1,"story_id":"other","frames":[]}"#,
            r#"{"schema_version":1,"story_id":"demo","frames":[],"extra":1}"#,
            r#"{"schema_version":1,"story_id":"demo","frames":[""]}"#,
            r#"{"schema_version":1,"story_id":"demo"}"#,
        ];
        for text in cases {
            std::fs::write(&path, text)?;
            assert!(
                ReadHistory::load(&path, "demo").is_err(),
                "must reject {text}"
            );
        }
        Ok(())
    }

    #[test]
    fn failed_write_keeps_marks_dirty_for_retry() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut read = ReadHistory::new("demo");
        read.mark("opening");
        // A directory is not a writable file target, so the atomic replace must fail.
        let blocked = directory.path().join("blocked");
        std::fs::create_dir(&blocked)?;
        assert!(read.save(&blocked).is_err());
        assert!(read.dirty());
        let path = directory.path().join("read.json");
        read.save(&path)?;
        assert!(ReadHistory::load(&path, "demo")?.contains("opening"));
        Ok(())
    }

    #[test]
    fn count_limit_preserves_unknown_marks_and_always_roundtrips() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("read.json");
        let mut read = ReadHistory::new("demo");
        assert!(read.mark("removed.chapter"));
        for index in 0..MAX_FRAMES - 1 {
            assert!(read.mark(&index.to_string()));
        }
        read.save(&path)?;
        let original = std::fs::read(&path)?;
        let mut loaded = ReadHistory::load(&path, "demo")?;
        assert!(loaded.contains("removed.chapter"));
        assert!(!loaded.mark("one.too.many"));
        assert!(!loaded.mark("0"));
        assert!(!loaded.dirty());
        loaded.save(&path)?;
        assert_eq!(std::fs::read(&path)?, original);
        assert_eq!(ReadHistory::load(&path, "demo")?.frames.len(), MAX_FRAMES);
        Ok(())
    }

    #[test]
    fn json_budget_and_id_limits_keep_the_previous_file_and_dirty_marks() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("read.json");
        let mut read = ReadHistory::new("demo");
        read.mark("opening");
        read.save(&path)?;
        let original = std::fs::read(&path)?;
        assert!(!read.mark(&"x".repeat(MAX_ID + 1)));
        assert!(!read.dirty());
        for index in 0..9000 {
            assert!(read.mark(&format!("{index:06}{}", "x".repeat(MAX_ID - 6))));
        }
        assert!(read.save(&path).is_err());
        assert!(read.dirty());
        assert_eq!(std::fs::read(&path)?, original);
        assert!(ReadHistory::load(&path, "demo")?.contains("opening"));
        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn locked_read_file_survives_failed_atomic_replace_and_retries() -> Result<()> {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("read.json");
        let mut read = ReadHistory::new("demo");
        read.mark("opening");
        read.save(&path)?;
        let original = std::fs::read(&path)?;
        read.mark("second");
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)?;
        assert!(read.save(&path).is_err());
        assert!(read.dirty());
        drop(lock);
        assert_eq!(std::fs::read(&path)?, original);
        read.save(&path)?;
        assert!(!read.dirty());
        let loaded = ReadHistory::load(&path, "demo")?;
        assert!(loaded.contains("opening") && loaded.contains("second"));
        Ok(())
    }
}
