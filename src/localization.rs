//! Author-declared language catalogs and pure independent channel selection.
use crate::{
    content::ContentSource,
    i18n::{Argument, Messages},
    loader::{check, elements, error, required, single},
    model::{AssetKind, Frame, Story, VariableValue, Variables},
};
use anyhow::{Context, Result, ensure};
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    sync::Arc,
};

pub fn canonical(value: &str) -> Result<String> {
    ensure!(
        !value.is_empty() && value.len() <= 64 && !value.contains('_'),
        "{}",
        crate::i18n::format("host-i18n-detail-0022", &[("arg0", value.to_string())])
    );
    let parsed: unic_langid::LanguageIdentifier = value.parse()?;
    let spelling = parsed.to_string();
    ensure!(
        parsed.language.as_str() != "und" && spelling == value,
        "{}",
        crate::i18n::format("host-i18n-detail-0023", &[("arg0", value.to_string())])
    );
    Ok(value.into())
}
pub fn normalize_system(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .take(32)
        .filter_map(|v| {
            v.replace('_', "-")
                .parse::<unic_langid::LanguageIdentifier>()
                .ok()
        })
        .map(|id| id.to_string())
        .filter(|id| id != "und" && seen.insert(id.clone()))
        .collect()
}
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requests {
    #[serde(deserialize_with = "Option::deserialize")]
    pub text: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub voice: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub schema_version: u32,
    pub story_id: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub text: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub voice: Option<String>,
}
impl Preferences {
    pub fn load(bytes: &[u8], story: &str) -> Result<Requests> {
        ensure!(
            bytes.len() <= 16384,
            "{}",
            crate::i18n::text("host-i18n-detail-0024")
        );
        let file: Self = serde_json::from_slice(bytes)?;
        ensure!(
            file.schema_version == 1 && file.story_id == story,
            "{}",
            crate::i18n::text("host-i18n-detail-0025")
        );
        for id in [&file.text, &file.voice].into_iter().flatten() {
            canonical(id)?;
        }
        Ok(Requests {
            text: file.text,
            voice: file.voice,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Selection {
    pub requests: Requests,
    pub system: Vec<String>,
    pub text: Option<String>,
    pub voice: Option<String>,
    pub text_source: String,
    pub voice_source: String,
}
pub fn select(
    request: Option<&str>,
    system: &[String],
    available: &BTreeSet<String>,
    default: Option<&str>,
) -> (Option<String>, String) {
    let Some(default) = default else {
        return (None, "disabled".into());
    };
    if let Some(request) = request {
        return if available.contains(request) {
            (Some(request.into()), "manual".into())
        } else {
            (Some(default.into()), "default".into())
        };
    }
    if let Some(id) = system.iter().find(|id| available.contains(*id)) {
        return (Some(id.clone()), "system".into());
    }
    (Some(default.into()), "default".into())
}
#[derive(Clone, Debug)]
pub struct TextLocale {
    pub label: String,
    pub fonts: Vec<String>,
    pub messages: Messages,
}
#[derive(Clone, Debug)]
pub struct VoiceLocale {
    pub label: String,
    pub clips: BTreeMap<String, Option<String>>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Frame(String),
    Choice(String, String),
    Character(String),
    Variable(String),
    Gallery(String),
}
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub texts: BTreeMap<String, TextLocale>,
    pub voices: BTreeMap<String, VoiceLocale>,
    pub default_text: String,
    pub default_voice: Option<String>,
    pub targets: BTreeMap<Target, String>,
}
fn source_text(source: &dyn ContentSource, path: &str, limit: usize) -> Result<String> {
    crate::content::validate_path(Path::new(path))?;
    let mut bytes = Vec::new();
    source
        .open(Path::new(path))?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "{}",
        crate::i18n::format("host-i18n-detail-0026", &[("arg0", path.to_string())])
    );
    String::from_utf8(bytes).with_context(|| format!("{path}: UTF-8"))
}
impl Catalog {
    pub fn enabled(&self) -> bool {
        !self.texts.is_empty()
    }
    pub fn from_story(xml: &str, source: &dyn ContentSource) -> Result<Self> {
        let doc = Document::parse(xml)?;
        let root = doc.root_element();
        let Some(config) = elements(root).find(|n| n.has_tag_name("localization")) else {
            return Ok(Self::default());
        };
        single("story.xml", root, "localization")?;
        check(
            "story.xml",
            config,
            "localization",
            &["enabled", "src"],
            &[],
            false,
        )?;
        match required("story.xml", config, "enabled")? {
            "false" => {
                if config.attribute("src").is_some() {
                    return Err(error(
                        "story.xml",
                        config,
                        crate::i18n::text("host-i18n-detail-0027"),
                    ));
                }
                return Ok(Self::default());
            }
            "true" => {}
            _ => {
                return Err(error(
                    "story.xml",
                    config,
                    crate::i18n::text("host-i18n-detail-0050"),
                ));
            }
        }
        let file = required("story.xml", config, "src")?;
        let text = source_text(source, file, 1024 * 1024)?;
        let locale_doc = Document::parse(&text).with_context(|| file.to_string())?;
        let local = locale_doc.root_element();
        (|| -> Result<Self> {
            check(
                file,
                local,
                "localization",
                &["schema", "default-text", "default-voice"],
                &["texts", "voices"],
                false,
            )?;
            ensure!(
                required(file, local, "schema")? == "1",
                "{}",
                crate::i18n::format("host-i18n-detail-0028", &[("arg0", file.to_string())])
            );
            let mut result = Self {
                default_text: canonical(required(file, local, "default-text")?)?,
                default_voice: local
                    .attribute("default-voice")
                    .map(canonical)
                    .transpose()?,
                ..Self::default()
            };
            let mut total = 0usize;
            let mut files = 0usize;
            let mut entries = 0usize;
            let mut edges = 0usize;
            let mut cache_bytes = 0usize;
            for (name, voice) in [("texts", false), ("voices", true)] {
                let group = single(file, local, name)?;
                check(file, group, name, &[], &["locale"], false)?;
                for locale in elements(group) {
                    check(
                        file,
                        locale,
                        "locale",
                        &["id", "label"],
                        if voice {
                            &["clip"]
                        } else {
                            &["messages", "font"]
                        },
                        false,
                    )?;
                    let id = canonical(required(file, locale, "id")?)?;
                    let label = required(file, locale, "label")?.to_string();
                    ensure!(
                        label.chars().count() <= 64,
                        "{}",
                        crate::i18n::format("host-i18n-detail-0029", &[("arg0", file.to_string())])
                    );
                    if voice {
                        let mut clips = BTreeMap::new();
                        for clip in elements(locale) {
                            check(file, clip, "clip", &["key", "asset", "silent"], &[], false)?;
                            let key = required(file, clip, "key")?.to_string();
                            let asset = match (clip.attribute("asset"), clip.attribute("silent")) {
                                (Some(asset), None) => Some(asset.to_string()),
                                (None, Some("true")) => None,
                                _ => {
                                    return Err(error(
                                        file,
                                        clip,
                                        crate::i18n::text("host-i18n-detail-0051"),
                                    ));
                                }
                            };
                            ensure!(
                                clips.insert(key.clone(), asset).is_none(),
                                "{}",
                                crate::i18n::format(
                                    "host-i18n-detail-0030",
                                    &[("arg0", file.to_string()), ("arg1", key.to_string())]
                                )
                            );
                        }
                        ensure!(
                            result.voices.len() < 32
                                && result
                                    .voices
                                    .insert(id, VoiceLocale { label, clips })
                                    .is_none(),
                            "{}",
                            crate::i18n::format(
                                "host-i18n-detail-0031",
                                &[("arg0", file.to_string())]
                            )
                        );
                    } else {
                        let mut sources = Vec::new();
                        let mut fonts = Vec::new();
                        let mut paths = BTreeSet::new();
                        for child in elements(locale) {
                            if child.has_tag_name("messages") {
                                check(file, child, "messages", &["src"], &[], false)?;
                                let path = required(file, child, "src")?;
                                ensure!(
                                    paths.insert(path.to_lowercase()) && sources.len() < 16,
                                    "{file}: duplicate/excessive FTL files"
                                );
                                let text = source_text(source, path, 1024 * 1024)?;
                                total = total.saturating_add(text.len());
                                files += 1;
                                ensure!(
                                    files <= 128 && total <= 16 * 1024 * 1024,
                                    "{}",
                                    crate::i18n::text("host-i18n-detail-0032")
                                );
                                sources.push((path.into(), text));
                            } else {
                                check(file, child, "font", &["ref"], &[], false)?;
                                let font = required(file, child, "ref")?.to_string();
                                ensure!(
                                    !fonts.contains(&font) && fonts.len() < 16,
                                    "{}",
                                    crate::i18n::format(
                                        "host-i18n-detail-0033",
                                        &[("arg0", file.to_string())]
                                    )
                                );
                                fonts.push(font);
                            }
                        }
                        ensure!(
                            !sources.is_empty() && !fonts.is_empty(),
                            "{}",
                            crate::i18n::format(
                                "host-i18n-detail-0034",
                                &[("arg0", file.to_string())]
                            )
                        );
                        let messages = Messages::new(&id, &sources)
                            .with_context(|| format!("{file}: {id}"))?;
                        entries = entries.saturating_add(messages.entry_count());
                        edges = edges.saturating_add(messages.reference_count());
                        cache_bytes = cache_bytes.saturating_add(messages.cache_bytes());
                        ensure!(
                            entries <= 50000 && edges <= 100000 && cache_bytes <= 64 * 1024 * 1024,
                            "{}",
                            crate::i18n::format(
                                "host-i18n-detail-0035",
                                &[("arg0", file.to_string())]
                            )
                        );
                        ensure!(
                            result.texts.len() < 32
                                && result
                                    .texts
                                    .insert(
                                        id,
                                        TextLocale {
                                            label,
                                            fonts,
                                            messages
                                        }
                                    )
                                    .is_none(),
                            "{}",
                            crate::i18n::format(
                                "host-i18n-detail-0036",
                                &[("arg0", file.to_string())]
                            )
                        );
                    }
                }
            }
            ensure!(
                result.texts.contains_key(&result.default_text),
                "{}",
                crate::i18n::format("host-i18n-detail-0037", &[("arg0", file.to_string())])
            );
            ensure!(
                match &result.default_voice {
                    Some(id) => result.voices.contains_key(id),
                    None => result.voices.is_empty(),
                },
                "{}",
                crate::i18n::format("host-i18n-detail-0038", &[("arg0", file.to_string())])
            );
            for node in doc.descendants().filter(Node::is_element) {
                let target = match node.tag_name().name() {
                    "text" if node.attribute("key").is_some() => Some((
                        Target::Frame(frame_ref(node.parent_element().expect("text parent"))?),
                        "key",
                    )),
                    "option" if node.attribute("text-key").is_some() => {
                        let frame = node
                            .ancestors()
                            .find(|n| n.has_tag_name("frame"))
                            .context("option frame")?;
                        Some((
                            Target::Choice(
                                frame_ref(frame)?,
                                required("story.xml", node, "id")?.into(),
                            ),
                            "text-key",
                        ))
                    }
                    "character" if node.attribute("name-key").is_some() => Some((
                        Target::Character(required("story.xml", node, "id")?.into()),
                        "name-key",
                    )),
                    "int" | "bool" | "string" if node.attribute("label-key").is_some() => Some((
                        Target::Variable(required("story.xml", node, "id")?.into()),
                        "label-key",
                    )),
                    _ => None,
                };
                if let Some((target, attribute)) = target {
                    let key = required("story.xml", node, attribute)?;
                    result.targets.insert(target, key.into());
                }
            }
            Ok(result)
        })()
        .map_err(|cause| error(file, local, cause))
    }
    pub fn selection(&self, requests: Requests, system: Vec<String>) -> Selection {
        let (text, text_source) = select(
            requests.text.as_deref(),
            &system,
            &self.texts.keys().cloned().collect(),
            self.enabled().then_some(self.default_text.as_str()),
        );
        let (voice, voice_source) = select(
            requests.voice.as_deref(),
            &system,
            &self.voices.keys().cloned().collect(),
            self.default_voice.as_deref(),
        );
        Selection {
            requests,
            system,
            text,
            voice,
            text_source,
            voice_source,
        }
    }
    pub fn messages(&self, locale: Option<&str>, key: &str) -> Result<&Messages> {
        let fallback = &self
            .texts
            .get(&self.default_text)
            .context(crate::i18n::text("host-i18n-detail-0052"))?
            .messages;
        let messages = locale
            .and_then(|id| self.texts.get(id))
            .map(|v| &v.messages)
            .unwrap_or(fallback);
        let messages = if messages.contains(key) {
            messages
        } else {
            fallback
        };
        ensure!(
            messages.contains(key),
            "{}",
            crate::i18n::format("host-i18n-detail-0039", &[("arg0", key.to_string())])
        );
        Ok(messages)
    }
    pub fn static_text(&self, key: &str) -> Result<String> {
        let value = self.messages(None, key)?.format(key, &BTreeMap::new())?;
        for id in self.texts.keys() {
            let text = self
                .messages(Some(id), key)?
                .format(key, &BTreeMap::new())?;
            ensure!(
                !text.trim().is_empty(),
                "{}",
                crate::i18n::format(
                    "host-static-empty",
                    &[("locale", id.clone()), ("key", key.into())]
                )
            );
        }
        Ok(value)
    }
    pub fn text(&self, target: &Target, locale: Option<&str>, literal: &str) -> String {
        self.targets
            .get(target)
            .map(|key| {
                self.messages(locale, key)
                    .and_then(|m| m.format(key, &BTreeMap::new()))
                    .expect("validated static translation")
            })
            .unwrap_or_else(|| literal.into())
    }
    pub fn frame_text(&self, frame: &Frame, locale: Option<&str>) -> String {
        self.text(
            &Target::Frame(frame.reference().into()),
            locale,
            &frame.text,
        )
    }
    pub fn cue(&self, frame: &Frame, selection: &Selection) -> crate::model::AudioCue {
        let mut cue = frame.audio.clone();
        if let Some(key) = &cue.voice
            && let Some(default) = self
                .default_voice
                .as_ref()
                .and_then(|id| self.voices.get(id))
        {
            cue.voice = self
                .voices
                .get(selection.voice.as_deref().unwrap_or(""))
                .and_then(|v| v.clips.get(key))
                .or_else(|| default.clips.get(key))
                .expect("validated voice mapping")
                .clone();
        }
        cue
    }
    pub fn validate(&self, story: &Story) -> Result<()> {
        let keys: BTreeSet<_> = story
            .frames
            .values()
            .filter_map(|f| f.audio.voice.as_deref())
            .collect();
        for locale in self.texts.values() {
            for id in &locale.fonts {
                ensure!(
                    story
                        .assets
                        .get(id)
                        .is_some_and(|a| a.kind == AssetKind::Font),
                    "{}",
                    crate::i18n::format("host-i18n-detail-0040", &[("arg0", id.to_string())])
                );
            }
        }
        for (id, locale) in &self.voices {
            for (key, asset) in &locale.clips {
                ensure!(
                    keys.contains(key.as_str()),
                    "{}",
                    crate::i18n::format("host-i18n-detail-0041", &[("arg0", key.to_string())])
                );
                if let Some(asset) = asset {
                    ensure!(
                        story
                            .assets
                            .get(asset)
                            .is_some_and(|a| a.kind == AssetKind::Audio),
                        "{}",
                        crate::i18n::format(
                            "host-i18n-detail-0042",
                            &[("arg0", asset.to_string())]
                        )
                    );
                }
            }
            if Some(id) == self.default_voice.as_ref() {
                ensure!(
                    keys.iter().all(|key| locale.clips.contains_key(*key)),
                    "{}",
                    crate::i18n::text("host-i18n-detail-0043")
                );
            }
        }
        Ok(())
    }
}
fn frame_ref(node: Node<'_, '_>) -> Result<String> {
    Ok(node
        .attribute("name")
        .or_else(|| node.attribute("id"))
        .context("frame reference")?
        .into())
}

#[derive(Clone, Debug)]
pub enum ArgSource {
    Literal(String),
    Binding(String),
}
#[derive(Clone, Debug)]
pub struct TextBinding {
    pub key: String,
    pub bound: bool,
    pub arguments: BTreeMap<String, ArgSource>,
    pub location: String,
}
pub fn binding(
    file: &str,
    node: Node<'_, '_>,
    prefix: &str,
    catalog: &Catalog,
) -> Result<Option<TextBinding>> {
    let key_name = format!("{prefix}-key");
    let bound_name = format!("{prefix}-key-bind");
    let (key, bound) = match (
        node.attribute(key_name.as_str()),
        node.attribute(bound_name.as_str()),
    ) {
        (None, None) => {
            ensure!(
                !elements(node).any(|n| n.has_tag_name("text-arg")),
                "{}",
                crate::i18n::format("host-i18n-detail-0044", &[("arg0", file.to_string())])
            );
            return Ok(None);
        }
        (Some(k), None) => (k.to_string(), false),
        (None, Some(k)) => (
            k.strip_prefix("ui.")
                .context(crate::i18n::text("host-i18n-detail-0053"))?
                .to_string(),
            true,
        ),
        _ => {
            return Err(error(
                file,
                node,
                crate::i18n::text("host-i18n-detail-0054"),
            ));
        }
    };
    ensure!(
        catalog.enabled(),
        "{}",
        crate::i18n::format("host-i18n-detail-0045", &[("arg0", file.to_string())])
    );
    ensure!(
        node.attribute(prefix).is_none()
            && (node.has_tag_name("toggle")
                || node
                    .attribute(if prefix == "text" {
                        "bind"
                    } else {
                        "title-bind"
                    })
                    .is_none()),
        "{}",
        crate::i18n::format("host-i18n-detail-0046", &[("arg0", file.to_string())])
    );
    let mut arguments = BTreeMap::new();
    for arg in elements(node).filter(|n| n.has_tag_name("text-arg")) {
        check(
            file,
            arg,
            "text-arg",
            &["name", "value", "bind"],
            &[],
            false,
        )?;
        let name = required(file, arg, "name")?.to_string();
        let value = match (arg.attribute("value"), arg.attribute("bind")) {
            (Some(s), None) => ArgSource::Literal(s.into()),
            (None, Some(s)) => ArgSource::Binding(s.into()),
            _ => return Err(error(file, arg, crate::i18n::text("host-i18n-detail-0055"))),
        };
        ensure!(
            arguments.len() < 16 && arguments.insert(name, value).is_none(),
            "{}",
            crate::i18n::format("host-i18n-detail-0047", &[("arg0", file.to_string())])
        );
    }
    let p = node.document().text_pos_at(node.range().start);
    Ok(Some(TextBinding {
        key,
        bound,
        arguments,
        location: format!("{file}:{}:{}", p.row, p.col),
    }))
}
impl TextBinding {
    pub fn resolve(
        &self,
        catalog: &Catalog,
        selection: &Selection,
        variables: &Variables,
        numbers: &BTreeMap<String, i64>,
    ) -> Result<String> {
        let get = |path: &str| -> Result<Argument> {
            if let Some(id) = path.strip_prefix("ui.") {
                return Argument::from_value(
                    variables
                        .get(id)
                        .context(crate::i18n::text("host-i18n-detail-0056"))?,
                );
            }
            if let Some(n) = numbers.get(path) {
                return Ok(Argument::Number(*n));
            }
            let value = match path {
                "locale.text" => selection.text.as_deref().unwrap_or(""),
                "locale.voice" => selection.voice.as_deref().unwrap_or(""),
                _ => anyhow::bail!(
                    "{}",
                    crate::i18n::format("host-i18n-detail-0048", &[("arg0", path.to_string())])
                ),
            };
            Ok(Argument::Text(value.into()))
        };
        let key = if self.bound {
            match variables.get(&self.key) {
                Some(VariableValue::String(s)) => s.as_str(),
                _ => anyhow::bail!("{}", crate::i18n::text("host-i18n-detail-0049")),
            }
        } else {
            &self.key
        };
        let args = self
            .arguments
            .iter()
            .map(|(name, value)| {
                Ok((
                    name.clone(),
                    match value {
                        ArgSource::Literal(s) => Argument::Text(s.clone()),
                        ArgSource::Binding(path) => get(path)?,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        for id in catalog.texts.keys() {
            catalog
                .messages(Some(id), key)?
                .format(key, &args)
                .with_context(|| format!("{}: {id}/{key}", self.location))?;
        }
        catalog
            .messages(selection.text.as_deref(), key)?
            .format(key, &args)
            .with_context(|| self.location.clone())
    }
}

#[cfg(windows)]
pub fn system_languages() -> Result<Vec<String>> {
    use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let mut count = 0u32;
    let mut length = 0u32;
    unsafe {
        if GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            windows::core::PWSTR::null(),
            &mut length,
        )
        .is_err()
            || length > 32768
        {
            anyhow::bail!("GetUserPreferredUILanguages size query failed");
        }
        let mut buffer = vec![0u16; length as usize];
        if GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
        .is_err()
        {
            anyhow::bail!("GetUserPreferredUILanguages read failed");
        }
        Ok(normalize_system(
            buffer
                .split(|ch| *ch == 0)
                .filter(|s| !s.is_empty())
                .take(32)
                .map(String::from_utf16_lossy),
        ))
    }
}
#[cfg(not(windows))]
pub fn system_languages() -> Result<Vec<String>> {
    Ok(Vec::new())
}

pub type SharedCatalog = Arc<Catalog>;

pub fn project_data(
    data: &mut crate::interface::Data,
    def: &crate::interface::Definition,
    state: &crate::interface::host::State,
    story: &Story,
    session: &crate::model::Session,
    slots: &[crate::save::SlotInfo],
) {
    use serde_json::json;
    let catalog = &def.localization;
    let selected = &state.languages;
    let locale = selected.text.as_deref();
    let info = json!({"enabled":catalog.enabled(),"textRequested":selected.requests.text,"voiceRequested":selected.requests.voice,"text":selected.text,"voice":selected.voice,"textSource":selected.text_source,"voiceSource":selected.voice_source});
    data.values["localeInfo"] = info;
    data.values["locale"] = json!({"enabled":catalog.enabled(),"textRequested":selected.requests.text.as_deref().unwrap_or(""),"voiceRequested":selected.requests.voice.as_deref().unwrap_or(""),"text":selected.text.as_deref().unwrap_or(""),"voice":selected.voice.as_deref().unwrap_or(""),"textSource":selected.text_source,"voiceSource":selected.voice_source});
    data.lists.insert("textLocales".into(),catalog.texts.iter().map(|(id,l)|json!({"id":id,"label":l.label,"selected":Some(id)==selected.text.as_ref(),"default":*id==catalog.default_text})).collect());
    data.lists.insert("voiceLocales".into(),catalog.voices.iter().map(|(id,l)|json!({"id":id,"label":l.label,"selected":Some(id)==selected.voice.as_ref(),"default":Some(id)==catalog.default_voice.as_ref()})).collect());
    if !catalog.enabled() {
        return;
    }
    let frame = &story.frames[&session.frame_id];
    data.values["game"]["text"] = json!(catalog.frame_text(frame, locale));
    if let Some(id) = &frame.speaker {
        data.values["game"]["speaker"] = json!(catalog.text(
            &Target::Character(id.clone()),
            locale,
            &story.characters[id]
        ));
    }
    if let Some(choices) = data.lists.get_mut("choices") {
        for item in choices {
            if let Some(id) = item["id"].as_str()
                && let Some(choice) = frame.options.iter().find(|c| c.id == id)
            {
                item["text"] = json!(catalog.text(
                    &Target::Choice(frame.reference().into(), id.into()),
                    locale,
                    &choice.text
                ));
            }
        }
    }
    if let Some(history) = data.lists.get_mut("history") {
        for (item, entry) in history.iter_mut().zip(&session.history) {
            let frame = &story.frames[&entry.frame_id];
            item["text"] = json!(catalog.frame_text(frame, locale));
            if let Some(id) = &frame.speaker {
                item["speaker"] = json!(catalog.text(
                    &Target::Character(id.clone()),
                    locale,
                    &story.characters[id]
                ));
            }
            if let Some(choice) = entry
                .choice_id
                .as_ref()
                .and_then(|id| frame.options.iter().find(|c| &c.id == id))
            {
                item["choice"] = json!(crate::i18n::format_literal(
                    "host-history-choice",
                    &[(
                        "text",
                        catalog.text(
                            &Target::Choice(frame.reference().into(), choice.id.clone()),
                            locale,
                            &choice.text
                        )
                    )]
                ));
            }
        }
    }
    if let Some(items) = data.lists.get_mut("variables") {
        for item in items {
            if let Some(id) = item["id"].as_str() {
                item["label"] = json!(catalog.text(
                    &Target::Variable(id.into()),
                    locale,
                    &story.variables[id].label
                ));
            }
        }
    }
    if let Some(items) = data.lists.get_mut("gallery") {
        for item in items {
            if let Some(id) = item["id"].as_str() {
                item["title"] = json!(catalog.text(
                    &Target::Gallery(id.into()),
                    locale,
                    &def.gallery[id].title
                ));
            }
        }
    }
    if let Some(items) = data.lists.get_mut("slots") {
        for (item, slot) in items.iter_mut().zip(slots) {
            if let crate::save::SlotState::Ready { frame_id, .. } = &slot.state {
                let text = catalog.frame_text(&story.frames[frame_id], locale);
                item["excerpt"] = json!(crate::save_preview::excerpt(&text));
                item["preview"] = json!(text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channels_match_independently_with_explicit_manual_precedence() {
        let texts = ["zh-CN".into(), "en-US".into()].into_iter().collect();
        let voices = ["zh-CN".into(), "ja-JP".into()].into_iter().collect();
        let system = normalize_system(["EN_us".into(), "ja-JP".into(), "en-US".into()]);
        assert_eq!(system, vec!["en-US", "ja-JP"]);
        assert_eq!(
            select(None, &system, &texts, Some("zh-CN")),
            (Some("en-US".into()), "system".into())
        );
        assert_eq!(
            select(None, &system, &voices, Some("ja-JP")),
            (Some("ja-JP".into()), "system".into())
        );
        assert_eq!(
            select(Some("zh-CN"), &system, &texts, Some("zh-CN")).1,
            "manual"
        );
        assert_eq!(
            select(Some("fr-FR"), &system, &texts, Some("zh-CN")),
            (Some("zh-CN".into()), "default".into())
        );
        assert_eq!(
            select(None, &["zh-TW".into()], &texts, Some("zh-CN")).1,
            "default"
        );
        assert_eq!(select(None, &system, &BTreeSet::new(), None).1, "disabled");
    }
    #[test]
    fn preference_json_is_complete_strict_and_independent() {
        let good = br#"{"schema_version":1,"story_id":"test","text":null,"voice":"ja-JP"}"#;
        assert_eq!(
            Preferences::load(good, "test").unwrap().voice.as_deref(),
            Some("ja-JP")
        );
        for bytes in [
            br#"{"schema_version":1,"story_id":"test","text":null}"#.as_slice(),
            br#"{"schema_version":1,"story_id":"test","text":null,"text":"en-US","voice":null}"#,
            br#"{"schema_version":1,"story_id":"test","text":"en_us","voice":null}"#,
        ] {
            assert!(Preferences::load(bytes, "test").is_err());
        }
        assert!(Preferences::load(good, "other").is_err());
        assert!(Preferences::load(&vec![b' '; 16385], "test").is_err());
    }
}
