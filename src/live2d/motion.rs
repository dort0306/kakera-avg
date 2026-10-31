use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone)]
pub enum Segment {
    Linear([f32; 4]),
    Bezier([f32; 8]),
    Step([f32; 4]),
    Inverse([f32; 4]),
}
impl Segment {
    fn end(&self) -> f32 {
        match self {
            Self::Bezier(v) => v[6],
            Self::Linear(v) | Self::Step(v) | Self::Inverse(v) => v[2],
        }
    }
    fn value(&self, time: f32) -> f32 {
        fn cubic(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
            let u = 1.0 - t;
            u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
        }
        match self {
            Self::Linear(v) => {
                v[1] + (v[3] - v[1]) * ((time - v[0]) / (v[2] - v[0])).clamp(0.0, 1.0)
            }
            Self::Step(v) => {
                if time >= v[2] {
                    v[3]
                } else {
                    v[1]
                }
            }
            Self::Inverse(v) => {
                if time <= v[0] {
                    v[1]
                } else {
                    v[3]
                }
            }
            Self::Bezier(v) => {
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..18 {
                    let mid = (lo + hi) * 0.5;
                    if cubic(v[0], v[2], v[4], v[6], mid) < time {
                        lo = mid
                    } else {
                        hi = mid
                    }
                }
                cubic(v[1], v[3], v[5], v[7], (lo + hi) * 0.5)
            }
        }
    }
}
#[derive(Clone)]
pub struct Curve {
    pub target: String,
    pub id: String,
    pub segments: Vec<Segment>,
    pub initial: f32,
}
impl Curve {
    pub fn value(&self, t: f32) -> f32 {
        self.segments
            .iter()
            .find(|s| t < s.end())
            .or_else(|| self.segments.last())
            .map(|s| s.value(t))
            .unwrap_or(self.initial)
    }
}
#[derive(Clone)]
pub struct Motion {
    pub duration: f32,
    pub looping: bool,
    pub curves: Vec<Curve>,
}
fn number(v: &Value) -> Result<f32> {
    let n = v
        .as_f64()
        .context(crate::i18n::text("host-live2d-motion-0018"))? as f32;
    ensure!(
        n.is_finite() && n.abs() <= 1e6,
        "{}",
        crate::i18n::format("host-live2d-motion-0001", &[])
    );
    Ok(n)
}
impl Motion {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= 8 * 1024 * 1024,
            "{}",
            crate::i18n::format("host-live2d-motion-0002", &[])
        );
        let v: Value = serde_json::from_slice(bytes)?;
        ensure!(
            v["Version"] == 3,
            "{}",
            crate::i18n::format("host-live2d-motion-0003", &[])
        );
        let duration = number(&v["Meta"]["Duration"])?;
        ensure!(
            duration > 0.0 && duration <= 300.0,
            "{}",
            crate::i18n::format("host-live2d-motion-0004", &[])
        );
        let list = v["Curves"]
            .as_array()
            .context(crate::i18n::text("host-live2d-motion-0019"))?;
        ensure!(
            list.len() <= 4096,
            "{}",
            crate::i18n::format("host-live2d-motion-0005", &[])
        );
        let mut curves = Vec::new();
        let mut budget = 0;
        for c in list {
            let target = c["Target"]
                .as_str()
                .context(crate::i18n::text("host-live2d-motion-0020"))?
                .to_owned();
            ensure!(
                ["Parameter", "PartOpacity", "Model"].contains(&target.as_str()),
                "{}",
                crate::i18n::format("host-live2d-motion-0006", &[])
            );
            let id = c["Id"]
                .as_str()
                .context(crate::i18n::text("host-live2d-motion-0021"))?
                .to_owned();
            ensure!(
                id.len() <= 256,
                "{}",
                crate::i18n::format("host-live2d-motion-0007", &[])
            );
            if target == "Model" {
                ensure!(
                    ["Opacity", "EyeBlink", "LipSync"].contains(&id.as_str()),
                    "{}",
                    crate::i18n::format("host-live2d-motion-0008", &[("arg0", id.to_string())])
                );
            }
            let values: Vec<f32> = c["Segments"]
                .as_array()
                .context(crate::i18n::text("host-live2d-motion-0022"))?
                .iter()
                .map(number)
                .collect::<Result<_>>()?;
            ensure!(
                values.len() >= 2,
                "{}",
                crate::i18n::format("host-live2d-motion-0009", &[])
            );
            let mut start = [values[0], values[1]];
            let initial = start[1];
            let mut pos = 2;
            let mut segments = Vec::new();
            while pos < values.len() {
                budget += 1;
                ensure!(
                    budget <= 100000,
                    "{}",
                    crate::i18n::format("host-live2d-motion-0010", &[])
                );
                let tag = values[pos];
                pos += 1;
                let len = if tag == 1.0 { 6 } else { 2 };
                ensure!(
                    pos + len <= values.len(),
                    "{}",
                    crate::i18n::format("host-live2d-motion-0011", &[])
                );
                let end = values[pos + len - 2];
                ensure!(
                    start[0] >= 0.0 && end > start[0] && end <= duration + 0.01,
                    "{}",
                    crate::i18n::format("host-live2d-motion-0012", &[])
                );
                let seg = match tag {
                    0.0 => Segment::Linear([start[0], start[1], end, values[pos + 1]]),
                    1.0 => {
                        ensure!(
                            (start[0]..=end).contains(&values[pos])
                                && (start[0]..=end).contains(&values[pos + 2]),
                            "{}",
                            crate::i18n::format("host-live2d-motion-0013", &[])
                        );
                        Segment::Bezier([
                            start[0],
                            start[1],
                            values[pos],
                            values[pos + 1],
                            values[pos + 2],
                            values[pos + 3],
                            end,
                            values[pos + 5],
                        ])
                    }
                    2.0 => Segment::Step([start[0], start[1], end, values[pos + 1]]),
                    3.0 => Segment::Inverse([start[0], start[1], end, values[pos + 1]]),
                    _ => anyhow::bail!(
                        "{}",
                        crate::i18n::format(
                            "host-live2d-motion-0014",
                            &[("arg0", tag.to_string())]
                        )
                    ),
                };
                start = [end, values[pos + len - 1]];
                pos += len;
                segments.push(seg);
            }
            curves.push(Curve {
                target,
                id,
                segments,
                initial,
            });
        }
        Ok(Self {
            duration,
            looping: v["Meta"]["Loop"].as_bool().unwrap_or(false),
            curves,
        })
    }
    pub fn apply(
        &self,
        time: f32,
        values: &mut BTreeMap<String, f32>,
        parts: &mut BTreeMap<String, f32>,
        groups: &BTreeMap<String, Vec<String>>,
    ) -> f32 {
        let t = if self.looping {
            time % self.duration
        } else {
            time.min(self.duration)
        };
        let mut opacity = 1.0;
        for c in &self.curves {
            let value = c.value(t);
            match c.target.as_str() {
                "Parameter" => {
                    values.insert(c.id.clone(), value);
                }
                "PartOpacity" => {
                    parts.insert(c.id.clone(), value);
                }
                "Model" if c.id == "Opacity" => opacity = value,
                "Model" => {
                    if let Some(ids) = groups.get(&c.id) {
                        for id in ids {
                            let base = values.get(id).copied().unwrap_or(if c.id == "EyeBlink" {
                                1.0
                            } else {
                                0.0
                            });
                            values.insert(
                                id.clone(),
                                if c.id == "EyeBlink" {
                                    base * value
                                } else {
                                    base + value
                                },
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        opacity.clamp(0.0, 1.0)
    }
}
#[derive(Clone)]
pub struct Expression(pub Vec<(String, f32, String)>);
impl Expression {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= 1024 * 1024,
            "{}",
            crate::i18n::format("host-live2d-motion-0015", &[])
        );
        let v: Value = serde_json::from_slice(bytes)?;
        let entries = v["Parameters"]
            .as_array()
            .context(crate::i18n::text("host-live2d-motion-0023"))?;
        ensure!(
            entries.len() <= 4096,
            "{}",
            crate::i18n::format("host-live2d-motion-0016", &[])
        );
        let mut result = Vec::new();
        for p in entries {
            let id = p["Id"]
                .as_str()
                .context(crate::i18n::text("host-live2d-motion-0024"))?
                .to_owned();
            let value = number(&p["Value"])?;
            let blend = p["Blend"].as_str().unwrap_or("Add").to_owned();
            ensure!(
                ["Add", "Multiply", "Overwrite"].contains(&blend.as_str()),
                "{}",
                crate::i18n::format("host-live2d-motion-0017", &[])
            );
            result.push((id, value, blend));
        }
        Ok(Self(result))
    }
    pub fn apply(&self, values: &mut BTreeMap<String, f32>) {
        for (id, v, blend) in &self.0 {
            let base = values.get(id).copied().unwrap_or(0.0);
            values.insert(
                id.clone(),
                match blend.as_str() {
                    "Multiply" => base * v,
                    "Overwrite" => *v,
                    _ => base + v,
                },
            );
        }
    }
}
