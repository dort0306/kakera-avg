//! Real Cubism Core deformation plus bounded motion/expression and software compositing.
use crate::{
    assets::AssetCache,
    content::{self, ContentSource},
    model::{AssetKind, Frame, Live2DPose, Slot, Story},
};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use std::{collections::BTreeMap, path::Path, sync::Arc};
pub mod core;
pub mod motion;
pub(crate) mod raster;
pub struct ModelAsset {
    /// Process-unique identity so GPU texture caches never key on image addresses.
    pub id: u64,
    pub moc: Vec<u8>,
    pub textures: Vec<image::RgbaImage>,
    pub motions: BTreeMap<String, motion::Motion>,
    pub expressions: BTreeMap<String, motion::Expression>,
    pub groups: BTreeMap<String, Vec<String>>,
    pub pose_groups: Vec<Vec<(String, Vec<String>)>>,
}
static NEXT_MODEL_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn relative(base: &Path, text: &str) -> Result<std::path::PathBuf> {
    let p = Path::new(text);
    content::validate_path(p)?;
    // PathBuf::join inserts backslashes on Windows; content paths stay platform neutral.
    let base = base.to_string_lossy().replace('\\', "/");
    let p = std::path::PathBuf::from(if base.is_empty() {
        text.to_owned()
    } else {
        format!("{base}/{text}")
    });
    content::validate_path(&p)?;
    Ok(p)
}
fn json(source: &dyn ContentSource, path: &Path) -> Result<serde_json::Value> {
    let bytes = source.read(path)?;
    ensure!(bytes.len() <= 8 * 1024 * 1024, "模型 JSON 超过 8MiB");
    serde_json::from_slice(&bytes).with_context(|| path.display().to_string())
}
impl ModelAsset {
    pub fn load(source: &dyn ContentSource, path: &Path) -> Result<Self> {
        let value = json(source, path)?;
        ensure!(value["Version"] == 3, "需要 model3 Version 3");
        let base = path.parent().context("model3 路径")?;
        let refs = &value["FileReferences"];
        let moc = source.read(&relative(base, refs["Moc"].as_str().context("缺少 Moc")?)?)?;
        let mut textures = Vec::new();
        let mut bytes = 0usize;
        for v in refs["Textures"].as_array().context("缺少 Textures")? {
            ensure!(textures.len() < 16, "模型贴图超过 16 张");
            let path = relative(base, v.as_str().context("贴图路径")?)?;
            let mut reader = image::ImageReader::new(std::io::Cursor::new(source.read(&path)?))
                .with_guessed_format()?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(8192);
            limits.max_image_height = Some(8192);
            reader.limits(limits);
            let image = reader.decode()?.to_rgba8();
            bytes += image.as_raw().len();
            ensure!(bytes <= 128 * 1024 * 1024, "模型贴图超过 128MiB");
            ensure!(image.width() > 0 && image.height() > 0, "空贴图");
            textures.push(image);
        }
        ensure!(!textures.is_empty(), "模型没有贴图");
        let mut motions = BTreeMap::new();
        let mut segment_budget = 0usize;
        let mut curve_budget = 0usize;
        if let Some(groups) = refs["Motions"].as_object() {
            for (group, entries) in groups {
                for (index, entry) in entries
                    .as_array()
                    .context("Motions 列表")?
                    .iter()
                    .enumerate()
                {
                    ensure!(motions.len() < 256, "模型动作超过 256 项");
                    let path = relative(base, entry["File"].as_str().context("动作 File")?)?;
                    let motion = motion::Motion::parse(&source.read(&path)?)
                        .with_context(|| path.display().to_string())?;
                    curve_budget += motion.curves.len();
                    segment_budget += motion
                        .curves
                        .iter()
                        .map(|c| c.segments.len())
                        .sum::<usize>();
                    ensure!(
                        curve_budget <= 16384 && segment_budget <= 200000,
                        "模型动作总曲线/片段预算耗尽"
                    );
                    if let Some(sound) = entry["Sound"].as_str() {
                        source.read(&relative(base, sound)?)?;
                    }
                    motions.insert(format!("{group}:{index}"), motion);
                }
            }
        }
        let mut expressions = BTreeMap::new();
        if let Some(list) = refs["Expressions"].as_array() {
            for entry in list {
                ensure!(expressions.len() < 128, "表情超过 128 项");
                let id = entry["Name"].as_str().context("表情 Name")?.to_owned();
                let path = relative(base, entry["File"].as_str().context("表情 File")?)?;
                let expression = motion::Expression::parse(&source.read(&path)?)
                    .with_context(|| path.display().to_string())?;
                ensure!(
                    expressions
                        .values()
                        .map(|e: &motion::Expression| e.0.len())
                        .sum::<usize>()
                        + expression.0.len()
                        <= 8192,
                    "模型表情参数总量超过 8192"
                );
                ensure!(expressions.insert(id, expression).is_none(), "表情重名");
            }
        }
        let mut groups = BTreeMap::new();
        if let Some(list) = value["Groups"].as_array() {
            for group in list {
                if group["Target"] == "Parameter" {
                    let name = group["Name"].as_str().context("模型 Group Name")?.into();
                    let ids = group["Ids"]
                        .as_array()
                        .context("模型 Group Ids")?
                        .iter()
                        .map(|v| v.as_str().map(str::to_owned).context("Group Id"))
                        .collect::<Result<Vec<_>>>()?;
                    ensure!(
                        groups.len() < 32
                            && ids.len() <= 4096
                            && ids.iter().all(|s| s.len() <= 256),
                        "模型参数组超出预算"
                    );
                    groups.insert(name, ids);
                }
            }
        }
        let mut pose_groups = Vec::new();
        if let Some(pose) = refs["Pose"].as_str() {
            let pose = json(source, &relative(base, pose)?)?;
            if let Some(list) = pose["Groups"].as_array() {
                for group in list {
                    ensure!(pose_groups.len() < 128, "Pose 组超过 128 项");
                    let mut parts = Vec::new();
                    for p in group.as_array().context("Pose group")? {
                        let id = p["Id"].as_str().context("Pose Id")?.into();
                        let links: Vec<String> = p["Link"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(str::to_owned))
                                    .collect()
                            })
                            .unwrap_or_default();
                        ensure!(
                            parts.len() < 128 && links.len() <= 128,
                            "Pose 部件/链接超过 128 项"
                        );
                        parts.push((id, links));
                    }
                    pose_groups.push(parts);
                }
            }
        }
        for key in ["Physics", "DisplayInfo", "UserData"] {
            if let Some(path) = refs[key].as_str() {
                json(source, &relative(base, path)?)?;
            }
        }
        Ok(Self {
            id: NEXT_MODEL_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            moc,
            textures,
            motions,
            expressions,
            groups,
            pose_groups,
        })
    }
}
#[derive(Default)]
pub struct PreparedLive2D {
    pub models: BTreeMap<String, Arc<ModelAsset>>,
    pub core: Option<Arc<core::Core>>,
    pub disabled: bool,
}
impl PreparedLive2D {
    pub fn load(
        story: &Story,
        source: &dyn ContentSource,
        dll: Option<&Path>,
        disabled: bool,
    ) -> Result<Self> {
        let mut models = BTreeMap::new();
        let mut cache_bytes = 0usize;
        for (id, a) in &story.assets {
            if a.kind == AssetKind::Live2D {
                ensure!(models.len() < 8, "Live2D 模型超过 8 项");
                let model =
                    ModelAsset::load(source, &a.path).with_context(|| format!("Live2D {id}"))?;
                cache_bytes += model.moc.len()
                    + model
                        .textures
                        .iter()
                        .map(|t| t.as_raw().len())
                        .sum::<usize>();
                ensure!(
                    cache_bytes <= 256 * 1024 * 1024,
                    "Live2D moc3/贴图总缓存超过 256MiB"
                );
                models.insert(id.clone(), Arc::new(model));
            }
        }
        let core = if !models.is_empty() && !disabled {
            Some(core::Core::load(dll.context("模型需要 Cubism Core；使用 --cubism-core <DLL路径> 或将 DLL 放在程序旁；--no-live2d 可显式使用 fallback")?)?)
        } else {
            None
        };
        let result = Self {
            models,
            core,
            disabled,
        };
        if !disabled {
            for (id, a) in &result.models {
                let raw = core::Model::new(
                    result.core.as_ref().context("Cubism Core")?.clone(),
                    &a.moc,
                    a.textures.len(),
                )?;
                for frame in story.frames.values() {
                    for p in &frame.visual.portraits {
                        if let Some(pose) = &p.live2d
                            && pose.model == *id
                        {
                            result.validate_pose(pose, &raw)?;
                        }
                    }
                }
            }
        }
        Ok(result)
    }
    fn validate_pose(&self, pose: &Live2DPose, raw: &core::Model) -> Result<()> {
        let asset = self.models.get(&pose.model).context("未知 Live2D 模型")?;
        if let Some(m) = &pose.motion {
            ensure!(asset.motions.contains_key(m), "未知 Live2D 动作 {m}");
        }
        if let Some(e) = &pose.expression {
            ensure!(asset.expressions.contains_key(e), "未知 Live2D 表情 {e}");
        }
        for (id, v) in &pose.parameters {
            let p = raw
                .parameters
                .iter()
                .find(|p| p.id == *id)
                .with_context(|| format!("未知 Live2D 参数 {id}"))?;
            ensure!(
                (p.min..=p.max).contains(v),
                "Live2D 参数 {id} 超出 {}–{}",
                p.min,
                p.max
            );
        }
        Ok(())
    }
    pub fn validate_commands(&self, frame: &Frame, commands: &[(Slot, String)]) -> Result<()> {
        for (slot, motion) in commands {
            let p = frame
                .visual
                .portraits
                .iter()
                .find(|p| p.slot == *slot)
                .and_then(|p| p.live2d.as_ref())
                .context("目标槽没有 Live2D 立绘")?;
            ensure!(
                self.models
                    .get(&p.model)
                    .is_some_and(|a| a.motions.contains_key(motion)),
                "模型没有动作 {motion}"
            );
        }
        Ok(())
    }
}
struct Instance {
    raw: core::Model,
    asset: Arc<ModelAsset>,
    pose: Live2DPose,
    age: f32,
    override_motion: Option<(String, f32)>,
    dirty: bool,
    accum: f32,
}
impl Instance {
    fn image(&mut self, renderer: &crate::render::Shared) -> Result<crate::render::Image> {
        let mut values: BTreeMap<_, _> = self
            .raw
            .parameters
            .iter()
            .map(|p| (p.id.clone(), p.default))
            .collect();
        let mut parts = BTreeMap::new();
        for group in &self.asset.pose_groups {
            for (i, (id, _)) in group.iter().enumerate() {
                values
                    .entry(id.clone())
                    .or_insert(if i == 0 { 1.0 } else { 0.0 });
            }
        }
        values.extend(self.pose.parameters.clone());
        let (motion, age) = self
            .override_motion
            .as_ref()
            .map(|(m, t)| (Some(m), *t))
            .unwrap_or((self.pose.motion.as_ref(), self.age));
        let opacity = motion
            .and_then(|m| self.asset.motions.get(m))
            .map(|m| m.apply(age, &mut values, &mut parts, &self.asset.groups))
            .unwrap_or(1.0);
        if let Some(expression) = self
            .pose
            .expression
            .as_ref()
            .and_then(|e| self.asset.expressions.get(e))
        {
            expression.apply(&mut values);
        }
        for group in &self.asset.pose_groups {
            let active = group
                .iter()
                .position(|(id, _)| values.get(id).is_some_and(|v| *v > 0.5))
                .unwrap_or(0);
            for (i, (id, links)) in group.iter().enumerate() {
                let v = if i == active { 1.0 } else { 0.0 };
                parts.entry(id.clone()).or_insert(v);
                for link in links {
                    parts.insert(link.clone(), v);
                }
            }
        }
        self.raw.update(&values, &parts)?;
        renderer.borrow_mut().model(crate::render::ModelDraw {
            meshes: self.raw.drawables(self.asset.textures.len())?,
            model: self.asset.id,
            textures: &self.asset.textures,
            canvas: self.raw.canvas,
            origin: self.raw.origin,
            ppu: self.raw.ppu,
            opacity,
        })
    }
}
pub struct Live2DPlayer {
    instances: BTreeMap<Slot, Instance>,
    pub images: BTreeMap<Slot, crate::render::Image>,
    renderer: crate::render::Shared,
}
pub fn texture_id(slot: Slot) -> &'static str {
    match slot {
        Slot::Left => "$live2d:left",
        Slot::Center => "$live2d:center",
        Slot::Right => "$live2d:right",
    }
}
pub fn old_texture_id(slot: Slot) -> &'static str {
    match slot {
        Slot::Left => "$live2d:old:left",
        Slot::Center => "$live2d:old:center",
        Slot::Right => "$live2d:old:right",
    }
}
impl Live2DPlayer {
    pub fn prepare(frame: &Frame, prepared: &PreparedLive2D) -> Result<Self> {
        Self::prepare_with(frame, prepared, crate::render::Renderer::cpu())
    }
    pub fn prepare_with(
        frame: &Frame,
        prepared: &PreparedLive2D,
        renderer: crate::render::Shared,
    ) -> Result<Self> {
        let mut result = Self {
            renderer,
            ..Self::default()
        };
        if prepared.disabled {
            return Ok(result);
        }
        for p in &frame.visual.portraits {
            if let Some(pose) = &p.live2d {
                let asset = prepared
                    .models
                    .get(&pose.model)
                    .context("Live2D 资源缺失")?
                    .clone();
                let raw = core::Model::new(
                    prepared
                        .core
                        .as_ref()
                        .context("Cubism Core 不可用")?
                        .clone(),
                    &asset.moc,
                    asset.textures.len(),
                )?;
                prepared.validate_pose(pose, &raw)?;
                let mut instance = Instance {
                    raw,
                    asset,
                    pose: pose.clone(),
                    age: 0.0,
                    override_motion: None,
                    dirty: true,
                    accum: 0.0,
                };
                result
                    .images
                    .insert(p.slot, instance.image(&result.renderer)?);
                result.instances.insert(p.slot, instance);
            }
        }
        Ok(result)
    }
    pub fn play(&mut self, slot: Slot, motion: String) {
        if let Some(i) = self.instances.get_mut(&slot) {
            i.override_motion = Some((motion, 0.0));
            i.dirty = true;
        }
    }
    pub fn tick(
        &mut self,
        seconds: f32,
        ctx: &egui::Context,
        assets: &mut AssetCache,
    ) -> Result<()> {
        for (slot, image) in &self.images {
            if !assets.has_stream(texture_id(*slot)) {
                assets.update_render(ctx, texture_id(*slot), image.clone());
            }
        }
        for (slot, i) in &mut self.instances {
            i.age += seconds.max(0.0);
            i.accum += seconds.max(0.0);
            if let Some((id, age)) = &mut i.override_motion {
                *age += seconds.max(0.0);
                if *age >= i.asset.motions[id].duration {
                    i.override_motion = None;
                }
            }
            if i.dirty
                || i.accum
                    >= 1.0
                        / if self.renderer.borrow().is_gpu() {
                            60.0
                        } else {
                            30.0
                        }
            {
                let image = i.image(&self.renderer)?;
                assets.update_render(ctx, texture_id(*slot), image.clone());
                self.images.insert(*slot, image);
                i.dirty = false;
                i.accum = 0.0;
            }
        }
        Ok(())
    }
    pub fn active(&self) -> bool {
        !self.instances.is_empty()
    }
}

impl Default for Live2DPlayer {
    fn default() -> Self {
        Self {
            instances: Default::default(),
            images: Default::default(),
            renderer: crate::render::Renderer::cpu(),
        }
    }
}
