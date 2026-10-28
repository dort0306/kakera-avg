use crate::model::{HistoryEntry, Logic, Session, Story, Variables};
use anyhow::{Context, Result, ensure};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const SLOT_COUNT: u16 = 200;
pub const PAGE_SIZE: u16 = 10;
pub const PAGE_COUNT: u16 = SLOT_COUNT / PAGE_SIZE;
pub const AUTO_COUNT: u16 = 10;
const MAX_JSON_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotId(u16);

impl SlotId {
    pub fn new(number: u16) -> Result<Self> {
        ensure!(
            (1..=SLOT_COUNT).contains(&number),
            "槽号必须在 1–{SLOT_COUNT} 之间"
        );
        Ok(Self(number))
    }
    pub fn number(self) -> u16 {
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
    pub variables: Variables,
    pub history_base_variables: Variables,
    pub saved_at_unix_ms: u64,
    pub generation: u64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub screenshot: Option<crate::save_preview::Screenshot>,
}

impl Snapshot {
    pub fn capture_with(story: &Story, session: &Session, logic: &dyn Logic) -> Result<Self> {
        story
            .validate_session_with(session, logic)
            .map_err(anyhow::Error::msg)?;
        Self::capture_committed(story, session)
    }
    pub(crate) fn capture_committed(story: &Story, session: &Session) -> Result<Self> {
        Ok(Self {
            schema_version: 9,
            story_id: story.id.clone(),
            content_version: story.content_version,
            frame_id: session.frame_id.clone(),
            history: session.history.clone(),
            variables: session.variables.clone(),
            history_base_variables: session.history_base_variables.clone(),
            generation: 0,
            screenshot: None,
            saved_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("系统时间早于 Unix epoch")?
                .as_millis()
                .try_into()?,
        })
    }

    pub fn restore_with(&self, story: &Story, logic: &dyn Logic) -> Result<Session> {
        if let Some(image) = &self.screenshot {
            image.decode()?;
        }
        ensure!(
            self.schema_version == 9,
            "不支持的槽位存档 schema：{}",
            self.schema_version
        );
        validate_identity(story, &self.story_id, self.content_version)?;
        let session = Session {
            frame_id: self.frame_id.clone(),
            history: self.history.clone(),
            variables: self.variables.clone(),
            history_base_variables: self.history_base_variables.clone(),
        };
        story
            .validate_session_with(&session, logic)
            .map_err(anyhow::Error::msg)?;
        Ok(session)
    }
}

fn valid_screenshot(snapshot: &Snapshot) -> bool {
    snapshot
        .screenshot
        .as_ref()
        .is_none_or(|image| image.decode().is_ok())
}

fn validate_identity(story: &Story, id: &str, content_version: u32) -> Result<()> {
    ensure!(id == story.id, "存档属于其他故事：{id}");
    ensure!(
        content_version == story.content_version,
        "存档内容版本与当前故事不兼容"
    );
    Ok(())
}

fn parse_snapshot(data: &[u8], story: &Story, logic: &dyn Logic) -> Result<Snapshot> {
    let snapshot: Snapshot =
        serde_json::from_slice(data).context("需要 v0.9.0 schema 9 存档，请重新开始旧版本故事")?;
    snapshot.restore_with(story, logic)?;
    Ok(snapshot)
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
        screenshot: Option<crate::save_preview::Screenshot>,
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
    pub fn auto_path(&self, slot: u16) -> Result<PathBuf> {
        ensure!((1..=AUTO_COUNT).contains(&slot), "自动槽须在 1–10");
        Ok(self.directory.join(format!("auto-{slot}.json")))
    }
    pub(crate) fn autosave_committed(&self, story: &Story, session: &Session) -> Result<()> {
        self.autosave_snapshot(Snapshot::capture_committed(story, session)?)
    }
    pub(crate) fn autosave_snapshot(&self, mut snapshot: Snapshot) -> Result<()> {
        let snapshot_identity = (snapshot.story_id.clone(), snapshot.content_version);
        let mut generation = 0u64;
        for slot in 1..=AUTO_COUNT {
            if let Ok(data) = read_json_bytes(&self.auto_path(slot)?)
                && let Ok(snapshot) = serde_json::from_slice::<Snapshot>(&data)
                && snapshot.schema_version == 9
                && snapshot.story_id == snapshot_identity.0
                && snapshot.content_version == snapshot_identity.1
            {
                generation = generation.max(snapshot.generation);
            }
        }
        snapshot.generation = generation.checked_add(1).context("自动存档序号溢出")?;
        let slot = ((snapshot.generation - 1) % u64::from(AUTO_COUNT) + 1) as u16;
        self.write_snapshot(&self.auto_path(slot)?, &snapshot)
    }
    pub fn autosave_with(&self, story: &Story, session: &Session, logic: &dyn Logic) -> Result<()> {
        story
            .validate_session_with(session, logic)
            .map_err(anyhow::Error::msg)?;
        self.autosave_committed(story, session)
    }
    pub fn load_auto(&self, slot: u16, story: &Story, logic: &dyn Logic) -> Result<Session> {
        parse_snapshot(&read_json_bytes(&self.auto_path(slot)?)?, story, logic)?
            .restore_with(story, logic)
    }
    pub fn auto_list(&self, story: &Story) -> Vec<serde_json::Value> {
        self.auto_metadata(story).0
    }
    pub fn auto_metadata(
        &self,
        story: &Story,
    ) -> (
        Vec<serde_json::Value>,
        Vec<(String, crate::save_preview::Screenshot)>,
    ) {
        let mut infos = Vec::new();
        let mut images = Vec::new();
        for slot in 1..=AUTO_COUNT {
            let path = self.auto_path(slot).expect("bounded slot");
            if !path.exists() {
                continue;
            }
            let result = read_json_bytes(&path)
                .and_then(|data| serde_json::from_slice::<Snapshot>(&data).map_err(Into::into));
            let (preview, loadable, generation, saved_at, has_image) = match result {
                Ok(s)
                    if s.schema_version == 9
                        && s.story_id == story.id
                        && s.content_version == story.content_version
                        && story.frames.contains_key(&s.frame_id)
                        && valid_screenshot(&s) =>
                {
                    let has_image = s.screenshot.is_some();
                    if let Some(image) = s.screenshot {
                        images.push((crate::save_preview::image_id(slot, true), image));
                    }
                    (
                        story.frames[&s.frame_id].text.clone(),
                        true,
                        s.generation,
                        crate::save_preview::saved_at(s.saved_at_unix_ms),
                        has_image,
                    )
                }
                _ => (
                    "损坏或不兼容自动存档".into(),
                    false,
                    0,
                    String::new(),
                    false,
                ),
            };
            infos.push(serde_json::json!({"slot":slot,"title":format!("自动槽 {slot}"),"excerpt":crate::save_preview::excerpt(&preview),"preview":preview,"exists":true,"loadable":loadable,"generation":generation,"savedAt":saved_at,"hasScreenshot":has_image,"image":if has_image {crate::save_preview::image_id(slot,true)} else {String::new()}}));
        }
        infos.sort_by_key(|v| std::cmp::Reverse(v["generation"].as_u64().unwrap_or(0)));
        (infos, images)
    }
    pub fn list_page(&self, story: &Story, page: u16) -> Result<Vec<SlotInfo>> {
        ensure!((1..=PAGE_COUNT).contains(&page), "存档页须在 1–20");
        Ok(((page - 1) * PAGE_SIZE + 1..=page * PAGE_SIZE)
            .map(|number| {
                let slot = SlotId::new(number).expect("bounded page");
                let path = self.slot_path(slot);
                let state = if !path.exists() {
                    SlotState::Empty
                } else {
                    match read_json_bytes(&path).and_then(|data| {
                        serde_json::from_slice::<Snapshot>(&data).map_err(Into::into)
                    }) {
                        Ok(s)
                            if s.schema_version == 9
                                && s.story_id == story.id
                                && s.content_version == story.content_version
                                && story.frames.contains_key(&s.frame_id)
                                && valid_screenshot(&s) =>
                        {
                            SlotState::Ready {
                                preview: story.frames[&s.frame_id].text.clone(),
                                frame_id: s.frame_id,
                                saved_at_unix_ms: s.saved_at_unix_ms,
                                screenshot: s.screenshot,
                            }
                        }
                        _ => SlotState::Invalid("无效或旧版存档；加载时还会验证完整历史".into()),
                    }
                };
                SlotInfo { slot, state }
            })
            .collect())
    }
    pub fn settings_path(&self) -> PathBuf {
        self.directory.join("settings.json")
    }
    /// Auxiliary read marks for Ctrl fast-forward, independent of any story slot.
    pub fn read_path(&self) -> PathBuf {
        self.directory.join("read.json")
    }

    pub fn save_with(
        &self,
        slot: SlotId,
        story: &Story,
        session: &Session,
        logic: &dyn Logic,
    ) -> Result<()> {
        atomic_json(
            &self.slot_path(slot),
            &Snapshot::capture_with(story, session, logic)?,
        )
    }
    pub(crate) fn write_snapshot(&self, path: &Path, snapshot: &Snapshot) -> Result<()> {
        if let Some(image) = &snapshot.screenshot {
            image.decode()?;
        }
        atomic_json(path, snapshot)
    }

    fn snapshot(&self, slot: SlotId, story: &Story, logic: &dyn Logic) -> Result<Snapshot> {
        let data = read_json_bytes(&self.slot_path(slot))?;
        parse_snapshot(&data, story, logic)
    }

    pub fn load_with(&self, slot: SlotId, story: &Story, logic: &dyn Logic) -> Result<Session> {
        self.snapshot(slot, story, logic)?
            .restore_with(story, logic)
    }
}

pub(crate) fn read_json_bytes(path: &Path) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).with_context(|| format!("读取 {}", path.display()))?;
    let mut data = Vec::new();
    file.take(MAX_JSON_BYTES + 1).read_to_end(&mut data)?;
    ensure!(
        data.len() as u64 <= MAX_JSON_BYTES,
        "JSON 文件超过 2MiB：{}",
        path.display()
    );
    Ok(data)
}

pub(crate) fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    ensure!(
        data.len() as u64 <= MAX_JSON_BYTES,
        "JSON 内容超过 2MiB，未改写旧文件"
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
