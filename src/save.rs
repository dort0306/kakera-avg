use crate::model::{HistoryEntry, Session, Story};
use anyhow::{Context, Result, ensure};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const SLOT_COUNT: u8 = 6;
const MAX_JSON_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotId(u8);

impl SlotId {
    pub fn new(number: u8) -> Result<Self> {
        ensure!(
            (1..=SLOT_COUNT).contains(&number),
            "槽号必须在 1–{SLOT_COUNT} 之间"
        );
        Ok(Self(number))
    }
    pub fn number(self) -> u8 {
        self.0
    }
    pub fn all() -> impl Iterator<Item = Self> {
        (1..=SLOT_COUNT).map(Self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub story_id: String,
    pub content_version: u32,
    pub frame_id: String,
    pub history: Vec<HistoryEntry>,
    pub saved_at_unix_ms: u64,
}

impl Snapshot {
    pub fn capture(story: &Story, session: &Session) -> Result<Self> {
        story
            .validate_session(session)
            .map_err(anyhow::Error::msg)?;
        Ok(Self {
            schema_version: 2,
            story_id: story.id.clone(),
            content_version: story.content_version,
            frame_id: session.frame_id.clone(),
            history: session.history.clone(),
            saved_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("系统时间早于 Unix epoch")?
                .as_millis()
                .try_into()?,
        })
    }

    pub fn restore(&self, story: &Story) -> Result<Session> {
        ensure!(
            self.schema_version == 2,
            "不支持的槽位存档 schema：{}",
            self.schema_version
        );
        validate_identity(story, &self.story_id, self.content_version)?;
        let session = Session {
            frame_id: self.frame_id.clone(),
            history: self.history.clone(),
        };
        story
            .validate_session(&session)
            .map_err(anyhow::Error::msg)?;
        Ok(session)
    }
}

fn validate_identity(story: &Story, id: &str, content_version: u32) -> Result<()> {
    ensure!(id == story.id, "存档属于其他故事：{id}");
    ensure!(
        content_version == story.content_version,
        "存档内容版本与当前故事不兼容"
    );
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySnapshot {
    schema_version: u32,
    story_id: String,
    content_version: u32,
    frame_id: String,
}

pub struct SlotInfo {
    pub slot: SlotId,
    pub state: SlotState,
}

pub enum SlotState {
    Empty,
    Ready {
        frame_id: String,
        preview: String,
        saved_at_unix_ms: u64,
    },
    Invalid(String),
}

pub struct SaveStore {
    directory: PathBuf,
}

impl SaveStore {
    pub fn for_story(story: &Story) -> Result<Self> {
        let dirs =
            ProjectDirs::from("org", "kakera", "kakera-avg").context("无法定位用户数据目录")?;
        Ok(Self::at(dirs.data_local_dir().join(&story.id)))
    }
    pub fn at(directory: PathBuf) -> Self {
        Self { directory }
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn slot_path(&self, slot: SlotId) -> PathBuf {
        self.directory.join(format!("slot-{}.json", slot.0))
    }
    pub fn legacy_path(&self) -> PathBuf {
        self.directory.join("save.json")
    }
    pub fn settings_path(&self) -> PathBuf {
        self.directory.join("settings.json")
    }

    pub fn save(&self, slot: SlotId, story: &Story, session: &Session) -> Result<()> {
        atomic_json(&self.slot_path(slot), &Snapshot::capture(story, session)?)
    }

    fn snapshot(&self, slot: SlotId, story: &Story) -> Result<Snapshot> {
        let data = read_json_bytes(&self.slot_path(slot))?;
        let snapshot: Snapshot = serde_json::from_slice(&data).context("槽位存档 JSON 格式错误")?;
        snapshot.restore(story)?;
        Ok(snapshot)
    }

    pub fn load(&self, slot: SlotId, story: &Story) -> Result<Session> {
        self.snapshot(slot, story)?.restore(story)
    }

    pub fn list(&self, story: &Story) -> Vec<SlotInfo> {
        SlotId::all()
            .map(|slot| {
                let state = if !self.slot_path(slot).exists() {
                    SlotState::Empty
                } else {
                    match self.snapshot(slot, story) {
                        Ok(snapshot) => SlotState::Ready {
                            preview: story.frames[&snapshot.frame_id].text.clone(),
                            frame_id: snapshot.frame_id,
                            saved_at_unix_ms: snapshot.saved_at_unix_ms,
                        },
                        Err(error) => SlotState::Invalid(format!("{error:#}")),
                    }
                };
                SlotInfo { slot, state }
            })
            .collect()
    }

    pub fn import_legacy(&self, story: &Story) -> Result<Session> {
        let data = read_json_bytes(&self.legacy_path())?;
        let old: LegacySnapshot =
            serde_json::from_slice(&data).context("v0.1 存档 JSON 格式错误")?;
        ensure!(old.schema_version == 1, "旧档入口只接受 schema 1");
        validate_identity(story, &old.story_id, old.content_version)?;
        let session = Session::at(old.frame_id);
        story
            .validate_session(&session)
            .map_err(anyhow::Error::msg)?;
        Ok(session)
    }
}

pub(crate) fn read_json_bytes(path: &Path) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).with_context(|| format!("读取 {}", path.display()))?;
    let mut data = Vec::new();
    file.take(MAX_JSON_BYTES + 1).read_to_end(&mut data)?;
    ensure!(
        data.len() as u64 <= MAX_JSON_BYTES,
        "JSON 文件超过 1MiB：{}",
        path.display()
    );
    Ok(data)
}

pub(crate) fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    ensure!(
        data.len() as u64 <= MAX_JSON_BYTES,
        "JSON 内容超过 1MiB，未改写旧文件"
    );
    let parent = path.parent().context("保存路径缺少父目录")?;
    std::fs::create_dir_all(parent).with_context(|| format!("创建目录 {}", parent.display()))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).context("创建同目录临时文件")?;
    temp.write_all(&data).context("写入临时文件")?;
    temp.as_file().sync_all().context("同步临时文件")?;
    temp.persist(path)
        .with_context(|| format!("原子替换 {}", path.display()))?;
    Ok(())
}
