//! Declarative, bounded UI presentation data. Randomness never reaches story logic.
use crate::{
    loader::{check, elements, error, required, single},
    model::{AssetKind, Story},
};
use anyhow::{Context, Result, ensure};
use roxmltree::Node;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[derive(Clone, Debug)]
pub struct Clip {
    pub asset: String,
    pub actor: Option<String>,
    pub weight: u32,
    pub enabled: bool,
}
#[derive(Clone, Debug)]
pub enum Step {
    Fade {
        target: String,
        from: f32,
        to: f32,
        duration: f32,
    },
    Video(String),
    Sound(String),
}
#[derive(Clone, Debug)]
pub enum Finish {
    Return,
    Open(String),
    Close,
}
#[derive(Clone, Debug)]
pub struct Sequence {
    pub steps: Vec<Step>,
    pub finish: Finish,
    pub skippable: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub sounds: BTreeMap<String, Vec<Clip>>,
    pub sequences: BTreeMap<String, Sequence>,
    pub audio: BTreeSet<String>,
    pub actors: BTreeSet<String>,
    pub ownership: BTreeMap<String, Option<String>>,
}
#[derive(Clone, Debug)]
pub enum Action {
    Sound(String),
    Sequence(String),
    Enabled(bool),
    Actor(String, bool),
    Exclude(String, bool),
}
fn boolean(file: &str, n: Node<'_, '_>, key: &str, default: bool) -> Result<bool> {
    match n.attribute(key) {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(error(file, n, format!("{key} 仅接受 true/false"))),
    }
}
fn identifier<'a, 'input: 'a>(file: &str, n: Node<'a, 'input>) -> Result<&'a str> {
    let id = required(file, n, "id")?;
    if id.len() > 128 || id.chars().any(char::is_whitespace) {
        return Err(error(file, n, "演出/音效组 ID 最多 128 字节且不能含空白"));
    }
    Ok(id)
}
impl Catalog {
    pub fn parse(
        root: Node<'_, '_>,
        file: &str,
        story: &Story,
        screens: &BTreeMap<String, super::Screen>,
    ) -> Result<Self> {
        let mut c = Self {
            audio: story
                .assets
                .iter()
                .filter(|(_, a)| a.kind == AssetKind::Audio)
                .map(|(id, _)| id.clone())
                .collect(),
            actors: story.characters.keys().cloned().collect(),
            ..Default::default()
        };
        if !elements(root).any(|n| n.has_tag_name("ui-media")) {
            return Ok(c);
        }
        let group = single(file, root, "ui-media")?;
        check(file, group, "ui-media", &[], &["sound", "sequence"], false)?;
        for n in elements(group).filter(|n| n.has_tag_name("sound")) {
            check(file, n, "sound", &["id"], &["clip"], false)?;
            let id = identifier(file, n)?;
            if c.audio.contains(id) {
                return Err(error(file, n, "sound ID 不能与音频资源 ID 重名"));
            }
            let mut clips = Vec::new();
            let mut seen = BTreeSet::new();
            for clip in elements(n) {
                check(
                    file,
                    clip,
                    "clip",
                    &["asset", "actor", "weight", "enabled"],
                    &[],
                    false,
                )?;
                let asset = required(file, clip, "asset")?.to_owned();
                if !c.audio.contains(&asset) || !seen.insert(asset.clone()) {
                    return Err(error(file, clip, "音效资源缺失、类型错误或候选重复"));
                }
                let actor = clip.attribute("actor").map(str::to_owned);
                if actor.as_ref().is_some_and(|a| !c.actors.contains(a)) {
                    return Err(error(file, clip, "音效角色未声明"));
                }
                if let Some(old) = c.ownership.insert(asset.clone(), actor.clone())
                    && old != actor
                {
                    return Err(error(file, clip, "同一资源的角色归属必须一致"));
                }
                let weight = clip
                    .attribute("weight")
                    .unwrap_or("1")
                    .parse::<u32>()
                    .map_err(|_| error(file, clip, "weight 必须为整数"))?;
                if !(1..=1000).contains(&weight) {
                    return Err(error(file, clip, "weight 必须为 1–1000"));
                }
                clips.push(Clip {
                    asset,
                    actor,
                    weight,
                    enabled: boolean(file, clip, "enabled", true)?,
                });
                if clips.len() > 64 {
                    return Err(error(file, n, "候选超过 64"));
                }
            }
            if clips.is_empty()
                || c.sounds.insert(id.into(), clips).is_some()
                || c.sounds.len() > 64
            {
                return Err(error(file, n, "音效组为空、重复或超过 64"));
            }
        }
        let mut targets = BTreeSet::new();
        fn nodes(items: &[super::Node], repeat: bool, ids: &mut BTreeSet<String>) {
            for n in items {
                let repeated = repeat || matches!(n.kind, super::Kind::Repeat { .. });
                if !repeated {
                    ids.insert(n.id.clone());
                }
                nodes(&n.children, repeated, ids);
            }
        }
        for s in screens.values() {
            nodes(&s.nodes, false, &mut targets);
        }
        for n in elements(group).filter(|n| n.has_tag_name("sequence")) {
            check(
                file,
                n,
                "sequence",
                &["id", "finish", "skippable"],
                &["fade", "video", "sound"],
                false,
            )?;
            let id = identifier(file, n)?;
            let finish = match n.attribute("finish").unwrap_or("return") {
                "return" => Finish::Return,
                "close" => Finish::Close,
                s => {
                    let target = s
                        .strip_prefix("open:")
                        .filter(|s| screens.contains_key(*s))
                        .ok_or_else(|| error(file, n, "finish 需要 return/close/open:有效页面"))?;
                    Finish::Open(target.into())
                }
            };
            let mut steps = Vec::new();
            let mut seconds = 0.;
            for step in elements(n) {
                match step.tag_name().name() {
                    "fade" => {
                        check(
                            file,
                            step,
                            "fade",
                            &["target", "from", "to", "duration"],
                            &[],
                            false,
                        )?;
                        let target = required(file, step, "target")?;
                        if !targets.contains(target) {
                            return Err(error(file, step, "动画目标缺失或位于 repeat 内"));
                        }
                        let number = |key: &str, default: &str| -> Result<f32> {
                            step.attribute(key)
                                .unwrap_or(default)
                                .parse::<f32>()
                                .map_err(|_| error(file, step, format!("{key} 非数值")))
                        };
                        let from = number("from", "0")?;
                        let to = number("to", "1")?;
                        let ms = number("duration", "400")?;
                        if !from.is_finite()
                            || !to.is_finite()
                            || !ms.is_finite()
                            || !(0.0..=1.0).contains(&from)
                            || !(0.0..=1.0).contains(&to)
                            || !(1.0..=10000.0).contains(&ms)
                        {
                            return Err(error(file, step, "fade 透明度/时长越界"));
                        }
                        seconds += ms / 1000.;
                        steps.push(Step::Fade {
                            target: target.into(),
                            from,
                            to,
                            duration: ms / 1000.,
                        });
                    }
                    "video" => {
                        check(file, step, "video", &["asset"], &[], false)?;
                        let asset = required(file, step, "asset")?;
                        if !story
                            .assets
                            .get(asset)
                            .is_some_and(|a| a.kind == AssetKind::Video)
                        {
                            return Err(error(file, step, "视频资源不存在或类型错误"));
                        }
                        steps.push(Step::Video(asset.into()));
                    }
                    "sound" => {
                        check(file, step, "sound", &["ref"], &[], false)?;
                        let sound = required(file, step, "ref")?;
                        if !c.sound_exists(sound) {
                            return Err(error(file, step, "音效引用不存在"));
                        }
                        steps.push(Step::Sound(sound.into()));
                    }
                    _ => unreachable!(),
                }
                if steps.len() > 64 || seconds > 600. {
                    return Err(error(file, n, "演出超过步骤/时长预算"));
                }
            }
            if steps.is_empty()
                || c.sequences
                    .insert(
                        id.into(),
                        Sequence {
                            steps,
                            finish,
                            skippable: boolean(file, n, "skippable", true)?,
                        },
                    )
                    .is_some()
                || c.sequences.len() > 64
            {
                return Err(error(file, n, "演出为空、重复或超过 64"));
            }
        }
        Ok(c)
    }
    pub fn sound_exists(&self, id: &str) -> bool {
        self.audio.contains(id) || self.sounds.contains_key(id)
    }
    pub fn allowed(&self, asset: &str, prefs: &Preferences) -> bool {
        prefs.enabled
            && self
                .sounds
                .values()
                .flatten()
                .filter(|c| c.asset == asset)
                .all(|c| c.enabled)
            && !prefs.excluded.contains(asset)
            && self
                .ownership
                .get(asset)
                .and_then(|a| a.as_ref())
                .is_none_or(|a| !prefs.disabled_actors.contains(a))
    }
    pub fn choose_with(
        &self,
        id: &str,
        prefs: &Preferences,
        mut random: impl FnMut(u64) -> Result<u64>,
    ) -> Result<Option<String>> {
        ensure!(self.sound_exists(id), "未知 UI 音效：{id}");
        if let Some(clips) = self.sounds.get(id) {
            let valid: Vec<_> = clips
                .iter()
                .filter(|c| c.enabled && self.allowed(&c.asset, prefs))
                .collect();
            let total: u64 = valid.iter().map(|c| u64::from(c.weight)).sum();
            if total == 0 {
                return Ok(None);
            }
            let mut choice = random(total)?;
            ensure!(choice < total, "随机抽取越界");
            for c in valid {
                if choice < u64::from(c.weight) {
                    return Ok(Some(c.asset.clone()));
                }
                choice -= u64::from(c.weight);
            }
            unreachable!()
        }
        Ok(self.allowed(id, prefs).then(|| id.into()))
    }
    pub fn choose(&self, id: &str, prefs: &Preferences) -> Result<Option<String>> {
        self.choose_with(id, prefs, |limit| {
            let threshold = limit.wrapping_neg() % limit;
            loop {
                let mut bytes = [0; 8];
                getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("随机源失败：{e}"))?;
                let n = u64::from_le_bytes(bytes);
                if n >= threshold {
                    return Ok(n % limit);
                }
            }
        })
    }
    pub fn preference(&self, prefs: &mut Preferences, action: &Action) -> Result<bool> {
        match action {
            Action::Enabled(value) => prefs.enabled = *value,
            Action::Actor(actor, enabled) => {
                ensure!(self.actors.contains(actor), "未知音效角色");
                if *enabled {
                    prefs.disabled_actors.remove(actor);
                } else {
                    prefs.disabled_actors.insert(actor.clone());
                }
            }
            Action::Exclude(asset, excluded) => {
                ensure!(self.audio.contains(asset), "未知排除音效");
                if *excluded {
                    prefs.excluded.insert(asset.clone());
                } else {
                    prefs.excluded.remove(asset);
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub schema: u32,
    pub story_id: String,
    pub enabled: bool,
    pub disabled_actors: BTreeSet<String>,
    pub excluded: BTreeSet<String>,
}
impl Preferences {
    pub fn new(story_id: &str) -> Self {
        Self {
            schema: 1,
            story_id: story_id.into(),
            enabled: true,
            disabled_actors: BTreeSet::new(),
            excluded: BTreeSet::new(),
        }
    }
    pub fn load(path: &Path, story_id: &str) -> Result<Self> {
        let p: Self = serde_json::from_slice(&crate::save::read_json_bytes(path)?)?;
        p.validate(story_id)?;
        Ok(p)
    }
    fn validate(&self, story_id: &str) -> Result<()> {
        ensure!(
            self.schema == 1 && self.story_id == story_id,
            "UI 音效偏好版本/故事不符"
        );
        ensure!(
            self.disabled_actors.len() <= 128 && self.excluded.len() <= 10000,
            "UI 音效偏好超过预算"
        );
        ensure!(
            self.disabled_actors
                .iter()
                .chain(&self.excluded)
                .all(|s| !s.is_empty() && s.len() <= 1024),
            "偏好 ID 无效"
        );
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate(&self.story_id)?;
        crate::save::atomic_json(path, self).context("保存 UI 音效偏好")
    }
}
