use crate::model::{Session, Story};
use anyhow::{Context, Result, ensure};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub story_id: String,
    pub content_version: u32,
    pub frame_id: String,
}

impl Snapshot {
    pub fn capture(story: &Story, session: &Session) -> Result<Self> {
        ensure!(
            story.frame(session).is_some(),
            "无法保存未知场景：{}",
            session.frame_id
        );
        Ok(Self {
            schema_version: 1,
            story_id: story.id.clone(),
            content_version: story.content_version,
            frame_id: session.frame_id.clone(),
        })
    }

    pub fn restore(self, story: &Story) -> Result<Session> {
        ensure!(
            self.schema_version == 1,
            "不支持的存档 schema：{}",
            self.schema_version
        );
        ensure!(
            self.story_id == story.id,
            "存档属于其他故事：{}",
            self.story_id
        );
        ensure!(
            self.content_version == story.content_version,
            "存档内容版本与当前故事不兼容"
        );
        ensure!(
            story.frames.contains_key(&self.frame_id),
            "存档场景不存在：{}",
            self.frame_id
        );
        Ok(Session {
            frame_id: self.frame_id,
        })
    }
}

pub struct SaveStore {
    path: PathBuf,
}

impl SaveStore {
    pub fn for_story(story: &Story) -> Result<Self> {
        // The loader limits story IDs to a single safe ASCII path component.
        let dirs =
            ProjectDirs::from("org", "kakera", "kakera-avg").context("无法定位用户数据目录")?;
        Ok(Self::at(
            dirs.data_local_dir().join(&story.id).join("save.json"),
        ))
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save(&self, story: &Story, session: &Session) -> Result<()> {
        let snapshot = Snapshot::capture(story, session)?;
        let data = serde_json::to_vec_pretty(&snapshot)?;
        let parent = self.path.parent().context("存档路径缺少父目录")?;
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建存档目录 {}", parent.display()))?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).context("创建同目录存档临时文件")?;
        temp.write_all(&data).context("写入存档临时文件")?;
        temp.as_file().sync_all().context("同步存档临时文件")?;
        // persist atomically replaces the destination (MoveFileExW on Windows).
        // On failure its owned temporary file is dropped; the old save is never deleted.
        temp.persist(&self.path)
            .with_context(|| format!("替换存档 {}", self.path.display()))?;
        Ok(())
    }

    pub fn load(&self, story: &Story) -> Result<Session> {
        let data = std::fs::read(&self.path)
            .with_context(|| format!("读取存档 {}", self.path.display()))?;
        let snapshot: Snapshot = serde_json::from_slice(&data).context("存档 JSON 格式错误")?;
        snapshot.restore(story)
    }
}
