//! Author-declared palette slots and the independent player color overrides file.
//!
//! Colors are stored as canonical `#RRGGBBAA` strings. `overrides` is a sparse table by
//! design: a missing slot means the author default, not a partially migrated old file.
use crate::color::Rgba;
use anyhow::{Context, Result, ensure};
use serde::de::{Error as DeError, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

pub const MAX_SLOTS: usize = 32;
pub const MAX_FILE_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Colors {
    pub overrides: BTreeMap<String, Rgba>,
}
impl Colors {
    pub fn resolve(
        &self,
        palette: &BTreeMap<String, super::PaletteSlot>,
    ) -> Result<BTreeMap<String, Rgba>> {
        ensure!(
            self.overrides.keys().all(|id| palette.contains_key(id)),
            "配色覆盖包含未声明的槽位"
        );
        Ok(palette
            .iter()
            .map(|(id, slot)| {
                (
                    id.clone(),
                    self.overrides.get(id).copied().unwrap_or(slot.initial),
                )
            })
            .collect())
    }
    pub fn set(&mut self, slot: &str, value: Option<Rgba>) {
        match value {
            Some(color) => {
                self.overrides.insert(slot.to_owned(), color);
            }
            None => {
                self.overrides.remove(slot);
            }
        }
    }
}

/// `overrides` uses a custom map visitor so duplicate JSON keys are rejected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overrides(pub BTreeMap<String, Rgba>);
impl Serialize for Overrides {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (id, color) in &self.0 {
            map.serialize_entry(id, &color.format())?;
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for Overrides {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapVisitor;
        impl<'de> Visitor<'de> for MapVisitor {
            type Value = Overrides;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("配色槽位对象")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut output = BTreeMap::new();
                let mut keys = std::collections::BTreeSet::new();
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    if !keys.insert(key.clone()) {
                        return Err(A::Error::custom(format!("重复配色槽位：{key}")));
                    }
                    let color = Rgba::parse(&value)
                        .map_err(|reason| A::Error::custom(format!("槽位 {key}：{reason}")))?;
                    if color.format() != value {
                        return Err(A::Error::custom(format!(
                            "槽位 {key} 的值必须是规范九字符大写 RGBA"
                        )));
                    }
                    output.insert(key, color);
                }
                Ok(Overrides(output))
            }
        }
        deserializer.deserialize_map(MapVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorsFile {
    pub schema_version: u32,
    pub story_id: String,
    pub overrides: Overrides,
}
impl ColorsFile {
    pub fn load_bytes(
        bytes: &[u8],
        story_id: &str,
        palette: &BTreeMap<String, super::PaletteSlot>,
    ) -> Result<Colors> {
        ensure!(
            bytes.len() as u64 <= MAX_FILE_BYTES,
            "配色偏好超过 {MAX_FILE_BYTES} 字节"
        );
        let file: Self = serde_json::from_slice(bytes).context("配色偏好 JSON 无效")?;
        ensure!(file.schema_version == 1, "配色偏好 schema 必须为 1");
        ensure!(file.story_id == story_id, "配色偏好的故事 ID 不匹配");
        ensure!(
            file.overrides.0.len() <= MAX_SLOTS,
            "配色覆盖超过 {MAX_SLOTS} 个槽位"
        );
        for id in file.overrides.0.keys() {
            ensure!(palette.contains_key(id), "配色覆盖引用未声明槽位：{id}");
        }
        Ok(Colors {
            overrides: file.overrides.0,
        })
    }
    pub fn to_bytes(story_id: &str, colors: &Colors) -> Result<Vec<u8>> {
        let file = Self {
            schema_version: 1,
            story_id: story_id.to_owned(),
            overrides: Overrides(colors.overrides.clone()),
        };
        let bytes = serde_json::to_vec_pretty(&file)?;
        ensure!(
            bytes.len() as u64 <= MAX_FILE_BYTES,
            "配色偏好超过 {MAX_FILE_BYTES} 字节"
        );
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::PaletteSlot;

    fn palette() -> BTreeMap<String, PaletteSlot> {
        [
            (
                "dialogue_background".to_owned(),
                PaletteSlot {
                    initial: Rgba::parse("#10253BFF").unwrap(),
                    location: "state.xml:1:1".into(),
                },
            ),
            (
                "dialogue_text".to_owned(),
                PaletteSlot {
                    initial: Rgba::parse("#F4EAD8FF").unwrap(),
                    location: "state.xml:2:1".into(),
                },
            ),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn overrides_are_sparse_and_strict() {
        let colors = ColorsFile::load_bytes(
            br##"{"schema_version":1,"story_id":"s","overrides":{"dialogue_text":"#FF0000FF"}}"##,
            "s",
            &palette(),
        )
        .unwrap();
        assert_eq!(colors.overrides.len(), 1);
        let resolved = colors.resolve(&palette()).unwrap();
        assert_eq!(resolved["dialogue_text"].format(), "#FF0000FF");
        assert_eq!(resolved["dialogue_background"].format(), "#10253BFF");
        let bytes = ColorsFile::to_bytes("s", &colors).unwrap();
        assert_eq!(
            ColorsFile::load_bytes(&bytes, "s", &palette()).unwrap(),
            colors
        );
        assert!(ColorsFile::load_bytes(&bytes, "other", &palette()).is_err());
        assert!(
            ColorsFile::load_bytes(
                br##"{"schema_version":1,"story_id":"s","overrides":{"other":"#FF0000FF"}}"##,
                "s",
                &palette()
            )
            .is_err()
        );
        assert!(ColorsFile::load_bytes(
            br##"{"schema_version":1,"story_id":"s","overrides":{"dialogue_text":"#ff0000ff"}}"##,
            "s",
            &palette()
        )
        .is_err());
        assert!(ColorsFile::load_bytes(
            br##"{"schema_version":1,"story_id":"s","overrides":{"dialogue_text":"#FF0000FF","dialogue_text":"#00FF00FF"}}"##,
            "s",
            &palette()
        )
        .is_err());
        assert!(
            ColorsFile::load_bytes(
                br##"{"schema_version":1,"story_id":"s","overrides":{},"extra":1}"##,
                "s",
                &palette()
            )
            .is_err()
        );
    }
}
