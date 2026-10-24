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
    let n = v.as_f64().context("动作数据需要数值")? as f32;
    ensure!(n.is_finite() && n.abs() <= 1e6, "动作数值越界");
    Ok(n)
}
impl Motion {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 8 * 1024 * 1024, "motion3 超过 8MiB");
        let v: Value = serde_json::from_slice(bytes)?;
        ensure!(v["Version"] == 3, "需要 motion3 Version 3");
        let duration = number(&v["Meta"]["Duration"])?;
        ensure!(duration > 0.0 && duration <= 300.0, "动作时长须在 0–300 秒");
        let list = v["Curves"].as_array().context("动作缺少 Curves")?;
        ensure!(list.len() <= 4096, "动作曲线过多");
        let mut curves = Vec::new();
        let mut budget = 0;
        for c in list {
            let target = c["Target"].as_str().context("曲线 Target")?.to_owned();
            ensure!(
                ["Parameter", "PartOpacity", "Model"].contains(&target.as_str()),
                "未知曲线目标"
            );
            let id = c["Id"].as_str().context("曲线 Id")?.to_owned();
            ensure!(id.len() <= 256, "曲线 ID 过长");
            if target == "Model" {
                ensure!(
                    ["Opacity", "EyeBlink", "LipSync"].contains(&id.as_str()),
                    "未知模型动作目标 {id}"
                );
            }
            let values: Vec<f32> = c["Segments"]
                .as_array()
                .context("曲线缺少 Segments")?
                .iter()
                .map(number)
                .collect::<Result<_>>()?;
            ensure!(values.len() >= 2, "曲线缺少起点");
            let mut start = [values[0], values[1]];
            let initial = start[1];
            let mut pos = 2;
            let mut segments = Vec::new();
            while pos < values.len() {
                budget += 1;
                ensure!(budget <= 100000, "动作片段预算耗尽");
                let tag = values[pos];
                pos += 1;
                let len = if tag == 1.0 { 6 } else { 2 };
                ensure!(pos + len <= values.len(), "动作片段不完整");
                let end = values[pos + len - 2];
                ensure!(
                    start[0] >= 0.0 && end > start[0] && end <= duration + 0.01,
                    "动作时间不单调或越界"
                );
                let seg = match tag {
                    0.0 => Segment::Linear([start[0], start[1], end, values[pos + 1]]),
                    1.0 => {
                        ensure!(
                            (start[0]..=end).contains(&values[pos])
                                && (start[0]..=end).contains(&values[pos + 2]),
                            "Bezier 控制点时间越界"
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
                    _ => anyhow::bail!("未知动作片段类型 {tag}"),
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
        ensure!(bytes.len() <= 1024 * 1024, "表情文件超过 1MiB");
        let v: Value = serde_json::from_slice(bytes)?;
        let entries = v["Parameters"].as_array().context("表情缺少 Parameters")?;
        ensure!(entries.len() <= 4096, "表情参数过多");
        let mut result = Vec::new();
        for p in entries {
            let id = p["Id"].as_str().context("表情 Id")?.to_owned();
            let value = number(&p["Value"])?;
            let blend = p["Blend"].as_str().unwrap_or("Add").to_owned();
            ensure!(
                ["Add", "Multiply", "Overwrite"].contains(&blend.as_str()),
                "未知表情混合"
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
