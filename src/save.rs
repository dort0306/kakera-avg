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
            "{}",
            crate::i18n::format("host-save-0001", &[("arg0", SLOT_COUNT.to_string())])
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
                .context(crate::i18n::text("host-save-0013"))?
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
            "{}",
            crate::i18n::format(
                "host-save-0002",
                &[("arg0", self.schema_version.to_string())]
            )
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
    ensure!(
        id == story.id,
        "{}",
        crate::i18n::format("host-save-0003", &[("arg0", id.to_string())])
    );
    ensure!(
        content_version == story.content_version,
        "{}",
        crate::i18n::format("host-save-0004", &[])
    );
    Ok(())
}

fn parse_snapshot(data: &[u8], story: &Story, logic: &dyn Logic) -> Result<Snapshot> {
    let snapshot: Snapshot =
        serde_json::from_slice(data).context(crate::i18n::text("host-save-0014"))?;
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
        let dirs = ProjectDirs::from("org", "kakera", "kakera-avg")
            .context(crate::i18n::text("host-save-0015"))?;
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
        ensure!(
            (1..=AUTO_COUNT).contains(&slot),
            "{}",
            crate::i18n::format("host-save-0005", &[])
        );
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
        snapshot.generation = generation
            .checked_add(1)
            .context(crate::i18n::text("host-save-0016"))?;
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
        self.auto_metadata_with(story, &|id| story.frames[id].text.clone())
    }
    pub fn auto_metadata_with(
        &self,
        story: &Story,
        text: &dyn Fn(&str) -> String,
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
                        text(&s.frame_id),
                        true,
                        s.generation,
                        crate::save_preview::saved_at(s.saved_at_unix_ms),
                        has_image,
                    )
                }
                _ => (
                    crate::i18n::text("host-save-0017").into(),
                    false,
                    0,
                    String::new(),
                    false,
                ),
            };
            infos.push(serde_json::json!({"slot":slot,"title":crate::i18n::format("host-save-0006",&[("arg0",format!("{}",slot))]),"excerpt":crate::save_preview::excerpt(&preview),"preview":preview,"exists":true,"loadable":loadable,"generation":generation,"savedAt":saved_at,"hasScreenshot":has_image,"image":if has_image {crate::save_preview::image_id(slot,true)} else {String::new()}}));
        }
        infos.sort_by_key(|v| std::cmp::Reverse(v["generation"].as_u64().unwrap_or(0)));
        (infos, images)
    }
    pub fn list_page(&self, story: &Story, page: u16) -> Result<Vec<SlotInfo>> {
        ensure!(
            (1..=PAGE_COUNT).contains(&page),
            "{}",
            crate::i18n::format("host-save-0007", &[])
        );
        Ok(((page - 1) * PAGE_SIZE + 1..=page * PAGE_SIZE)
            .map(|number| self.slot_info(story, SlotId::new(number).expect("bounded page")))
            .collect())
    }
    /// Load metadata for an explicit one-based slot range. Only these slots are read;
    /// the other 200 slots stay untouched for paginated `slots` repeats.
    pub fn list_range(&self, story: &Story, start: u16, end: u16) -> Vec<SlotInfo> {
        let start = start.clamp(1, SLOT_COUNT);
        let end = end.clamp(start, SLOT_COUNT);
        (start..=end)
            .map(|number| self.slot_info(story, SlotId::new(number).expect("clamped slot")))
            .collect()
    }
    fn slot_info(&self, story: &Story, slot: SlotId) -> SlotInfo {
        let path = self.slot_path(slot);
        let state = if !path.exists() {
            SlotState::Empty
        } else {
            match read_json_bytes(&path)
                .and_then(|data| serde_json::from_slice::<Snapshot>(&data).map_err(Into::into))
            {
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
                _ => SlotState::Invalid(crate::i18n::text("host-save-0018").into()),
            }
        };
        SlotInfo { slot, state }
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
    let data = read_bytes_limited(path, MAX_JSON_BYTES)?;
    Ok(data)
}

pub(crate) fn read_bytes_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).with_context(|| {
        crate::i18n::format("host-save-0008", &[("arg0", format!("{}", path.display()))])
    })?;
    let mut data = Vec::new();
    file.take(limit + 1).read_to_end(&mut data)?;
    ensure!(
        data.len() as u64 <= limit,
        "{}",
        crate::i18n::format(
            "host-save-0009",
            &[
                ("arg0", limit.to_string()),
                ("arg1", format!("{}", path.display()))
            ]
        )
    );
    Ok(data)
}

pub(crate) fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    atomic_json_limited(path, value, MAX_JSON_BYTES)
}

pub(crate) fn atomic_json_limited(path: &Path, value: &impl Serialize, limit: u64) -> Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    ensure!(
        data.len() as u64 <= limit,
        "{}",
        crate::i18n::format("host-save-0010", &[("arg0", limit.to_string())])
    );
    let parent = path.parent().context(crate::i18n::text("host-save-0019"))?;
    std::fs::create_dir_all(parent).with_context(|| {
        crate::i18n::format(
            "host-save-0011",
            &[("arg0", format!("{}", parent.display()))],
        )
    })?;
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).context(crate::i18n::text("host-save-0020"))?;
    temp.write_all(&data)
        .context(crate::i18n::text("host-save-0021"))?;
    temp.as_file()
        .sync_all()
        .context(crate::i18n::text("host-save-0022"))?;
    temp.persist(path).with_context(|| {
        crate::i18n::format("host-save-0012", &[("arg0", format!("{}", path.display()))])
    })?;
    Ok(())
}
