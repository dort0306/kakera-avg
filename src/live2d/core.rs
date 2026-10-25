//! Narrow dynamic bindings to the user-supplied Cubism Core ABI (SDK 5-r.5).
use anyhow::{Context, Result, ensure};
use libloading::Library;
use std::{
    ffi::{CStr, c_char, c_void},
    path::Path,
    sync::Arc,
};
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct V4 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
type Ptr = *mut c_void;
type CPtr = *const c_void;
macro_rules! api {
    ($($field:ident:$ty:ty=>$name:literal),*$(,)?)=>{
        pub struct Core {$(pub $field:$ty,)* _library:Library}
        impl Core {
            pub fn load(path:&Path)->Result<Arc<Self>> {
                // SAFETY: the native runtime is explicitly selected by the host, never game JS.
                let library=unsafe{Library::new(path)}.with_context(||format!("Cubism Core DLL: {}",path.display()))?;
                $(let $field=unsafe{*library.get::<$ty>(concat!($name,"\0").as_bytes())?};)*
                Ok(Arc::new(Self{$($field,)*_library:library}))
            }
        }
    }
}
api! {
    version:unsafe extern "C" fn()->u32=>"csmGetVersion",
    consistent:unsafe extern "C" fn(Ptr,u32)->i32=>"csmHasMocConsistency",
    revive:unsafe extern "C" fn(Ptr,u32)->Ptr=>"csmReviveMocInPlace",
    model_size:unsafe extern "C" fn(CPtr)->u32=>"csmGetSizeofModel",
    initialize:unsafe extern "C" fn(CPtr,Ptr,u32)->Ptr=>"csmInitializeModelInPlace",
    update:unsafe extern "C" fn(Ptr)->()=>"csmUpdateModel",
    reset:unsafe extern "C" fn(Ptr)->()=>"csmResetDrawableDynamicFlags",
    canvas:unsafe extern "C" fn(CPtr,*mut V2,*mut V2,*mut f32)->()=>"csmReadCanvasInfo",
    parameter_count:unsafe extern "C" fn(CPtr)->i32=>"csmGetParameterCount",
    parameter_ids:unsafe extern "C" fn(CPtr)->*const *const c_char=>"csmGetParameterIds",
    minimum:unsafe extern "C" fn(CPtr)->*const f32=>"csmGetParameterMinimumValues",
    maximum:unsafe extern "C" fn(CPtr)->*const f32=>"csmGetParameterMaximumValues",
    defaults:unsafe extern "C" fn(CPtr)->*const f32=>"csmGetParameterDefaultValues",
    values:unsafe extern "C" fn(Ptr)->*mut f32=>"csmGetParameterValues",
    part_count:unsafe extern "C" fn(CPtr)->i32=>"csmGetPartCount",
    part_ids:unsafe extern "C" fn(CPtr)->*const *const c_char=>"csmGetPartIds",
    part_opacity:unsafe extern "C" fn(Ptr)->*mut f32=>"csmGetPartOpacities",
    count:unsafe extern "C" fn(CPtr)->i32=>"csmGetDrawableCount",
    flags:unsafe extern "C" fn(CPtr)->*const u8=>"csmGetDrawableConstantFlags",
    dynamic:unsafe extern "C" fn(CPtr)->*const u8=>"csmGetDrawableDynamicFlags",
    blends:unsafe extern "C" fn(CPtr)->*const i32=>"csmGetDrawableBlendModes",
    textures:unsafe extern "C" fn(CPtr)->*const i32=>"csmGetDrawableTextureIndices",
    orders:unsafe extern "C" fn(CPtr)->*const i32=>"csmGetRenderOrders",
    opacity:unsafe extern "C" fn(CPtr)->*const f32=>"csmGetDrawableOpacities",
    mask_counts:unsafe extern "C" fn(CPtr)->*const i32=>"csmGetDrawableMaskCounts",
    masks:unsafe extern "C" fn(CPtr)->*const *const i32=>"csmGetDrawableMasks",
    vertex_counts:unsafe extern "C" fn(CPtr)->*const i32=>"csmGetDrawableVertexCounts",
    vertices:unsafe extern "C" fn(CPtr)->*const *const V2=>"csmGetDrawableVertexPositions",
    uvs:unsafe extern "C" fn(CPtr)->*const *const V2=>"csmGetDrawableVertexUvs",
    index_counts:unsafe extern "C" fn(CPtr)->*const i32=>"csmGetDrawableIndexCounts",
    indices:unsafe extern "C" fn(CPtr)->*const *const u16=>"csmGetDrawableIndices",
    multiply:unsafe extern "C" fn(CPtr)->*const V4=>"csmGetDrawableMultiplyColors",
    screen:unsafe extern "C" fn(CPtr)->*const V4=>"csmGetDrawableScreenColors",
    offscreen_count:unsafe extern "C" fn(CPtr)->i32=>"csmGetOffscreenCount",
}
#[repr(C, align(64))]
#[derive(Clone)]
struct Block([u8; 64]);
struct Memory(Vec<Block>);
impl Memory {
    fn new(size: usize) -> Result<Self> {
        ensure!(
            size > 0 && size <= 64 * 1024 * 1024,
            "Cubism 内存预算超过 64MiB"
        );
        Ok(Self(vec![Block([0; 64]); size.div_ceil(64)]))
    }
    fn ptr(&mut self) -> Ptr {
        self.0.as_mut_ptr().cast()
    }
}
#[derive(Clone, Debug)]
pub struct Parameter {
    pub id: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}
#[derive(Clone)]
pub struct Drawable {
    pub vertices: Vec<V2>,
    pub uvs: Vec<V2>,
    pub indices: Vec<u16>,
    pub masks: Vec<usize>,
    pub texture: usize,
    pub flags: u8,
    pub visible: bool,
    pub opacity: f32,
    pub order: i32,
    pub multiply: V4,
    pub screen: V4,
}
pub struct Model {
    core: Arc<Core>,
    ptr: Ptr,
    _moc: Memory,
    _memory: Memory,
    pub parameters: Vec<Parameter>,
    pub parts: Vec<String>,
    pub canvas: [f32; 2],
    pub origin: [f32; 2],
    pub ppu: f32,
}
unsafe fn slice<'a, T>(ptr: *const T, count: usize) -> Result<&'a [T]> {
    if count == 0 {
        return Ok(&[]);
    }
    ensure!(!ptr.is_null(), "Cubism 返回空数组");
    Ok(unsafe { std::slice::from_raw_parts(ptr, count) })
}
fn count(n: i32, max: usize) -> Result<usize> {
    ensure!(n >= 0 && n as usize <= max, "Cubism 数量超出预算");
    Ok(n as usize)
}
unsafe fn ids(ptr: *const *const c_char, n: usize) -> Result<Vec<String>> {
    let mut result = Vec::with_capacity(n);
    for p in unsafe { slice(ptr, n)? } {
        ensure!(!p.is_null(), "Cubism ID 为空");
        let id = unsafe { CStr::from_ptr(*p) }.to_str()?;
        ensure!(id.len() <= 256, "Cubism ID 过长");
        result.push(id.into());
    }
    Ok(result)
}
impl Model {
    pub fn new(core: Arc<Core>, bytes: &[u8], textures: usize) -> Result<Self> {
        unsafe {
            let mut moc = Memory::new(bytes.len())?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), moc.ptr().cast(), bytes.len());
            ensure!(
                (core.consistent)(moc.ptr(), bytes.len() as u32) == 1,
                "moc3 一致性校验失败"
            );
            let revived = (core.revive)(moc.ptr(), bytes.len() as u32);
            ensure!(!revived.is_null(), "无法加载 moc3");
            let size = (core.model_size)(revived);
            let mut memory = Memory::new(size as usize)?;
            let ptr = (core.initialize)(revived, memory.ptr(), size);
            ensure!(!ptr.is_null(), "无法初始化 Cubism 模型");
            ensure!(
                (core.offscreen_count)(ptr) == 0,
                "不支持 Cubism 5.3 离屏分组混合，请导出普通网格模型"
            );
            let n = count((core.parameter_count)(ptr), 4096)?;
            let names = ids((core.parameter_ids)(ptr), n)?;
            let mins = slice((core.minimum)(ptr), n)?;
            let maxs = slice((core.maximum)(ptr), n)?;
            let defs = slice((core.defaults)(ptr), n)?;
            let parameters = names
                .into_iter()
                .enumerate()
                .map(|(i, id)| Parameter {
                    id,
                    min: mins[i],
                    max: maxs[i],
                    default: defs[i],
                })
                .collect();
            let parts = ids((core.part_ids)(ptr), count((core.part_count)(ptr), 4096)?)?;
            let mut canvas = V2::default();
            let mut origin = V2::default();
            let mut ppu = 0.0;
            (core.canvas)(ptr, &mut canvas, &mut origin, &mut ppu);
            ensure!(
                [canvas.x, canvas.y, origin.x, origin.y, ppu]
                    .iter()
                    .all(|v| v.is_finite())
                    && canvas.x > 0.0
                    && canvas.y > 0.0
                    && ppu > 0.0,
                "Cubism 画布无效"
            );
            let model = Self {
                core,
                ptr,
                _moc: moc,
                _memory: memory,
                parameters,
                parts,
                canvas: [canvas.x, canvas.y],
                origin: [origin.x, origin.y],
                ppu,
            };
            model.drawables(textures)?;
            Ok(model)
        }
    }
    pub fn update(
        &mut self,
        values: &std::collections::BTreeMap<String, f32>,
        parts: &std::collections::BTreeMap<String, f32>,
    ) -> Result<()> {
        unsafe {
            let pointer = (self.core.values)(self.ptr);
            ensure!(
                !pointer.is_null() || self.parameters.is_empty(),
                "Cubism 参数指针为空"
            );
            for (i, p) in self.parameters.iter().enumerate() {
                let v = values.get(&p.id).copied().unwrap_or(p.default);
                ensure!(v.is_finite(), "Cubism 参数非有限数");
                *pointer.add(i) = v.clamp(p.min, p.max);
            }
            let pointer = (self.core.part_opacity)(self.ptr);
            ensure!(
                !pointer.is_null() || self.parts.is_empty(),
                "Cubism 部件指针为空"
            );
            for (i, id) in self.parts.iter().enumerate() {
                *pointer.add(i) = parts.get(id).copied().unwrap_or(1.0).clamp(0.0, 1.0);
            }
            (self.core.reset)(self.ptr);
            (self.core.update)(self.ptr);
            Ok(())
        }
    }
    pub fn drawables(&self, textures: usize) -> Result<Vec<Drawable>> {
        unsafe {
            let c = &self.core;
            let p = self.ptr;
            let n = count((c.count)(p), 2048)?;
            let flags = slice((c.flags)(p), n)?;
            let dyns = slice((c.dynamic)(p), n)?;
            let tex = slice((c.textures)(p), n)?;
            let blends = slice((c.blends)(p), n)?;
            let orders = slice((c.orders)(p), n)?;
            let opacity = slice((c.opacity)(p), n)?;
            let counts = slice((c.vertex_counts)(p), n)?;
            let vertices = slice((c.vertices)(p), n)?;
            let uvs = slice((c.uvs)(p), n)?;
            let index_counts = slice((c.index_counts)(p), n)?;
            let indices = slice((c.indices)(p), n)?;
            let mask_counts = slice((c.mask_counts)(p), n)?;
            let masks = slice((c.masks)(p), n)?;
            let multiply = slice((c.multiply)(p), n)?;
            let screen = slice((c.screen)(p), n)?;
            let mut result = Vec::with_capacity(n);
            let mut total = 0;
            for i in 0..n {
                ensure!(
                    (0..=2).contains(&blends[i]),
                    "不支持 Cubism 高级混合模式 {}",
                    blends[i]
                );
                ensure!(
                    tex[i] >= 0 && (tex[i] as usize) < textures,
                    "Cubism 贴图索引越界"
                );
                let len = count(counts[i], 65535)?;
                let index_len = count(index_counts[i], 196605)?;
                total += index_len;
                ensure!(
                    total <= 1_000_000 && index_len % 3 == 0,
                    "Cubism 网格预算或三角形无效"
                );
                let positions = slice(vertices[i], len)?.to_vec();
                let uv = slice(uvs[i], len)?.to_vec();
                let idx = slice(indices[i], index_len)?.to_vec();
                ensure!(
                    positions.iter().chain(&uv).all(|v| v.x.is_finite()
                        && v.y.is_finite()
                        && v.x.abs() < 1e6
                        && v.y.abs() < 1e6)
                        && idx.iter().all(|v| (*v as usize) < len),
                    "Cubism 网格顶点无效"
                );
                let mask = slice(masks[i], count(mask_counts[i], 2048)?)?;
                ensure!(
                    mask.iter().all(|m| *m >= 0 && (*m as usize) < n),
                    "Cubism 蒙版索引无效"
                );
                result.push(Drawable {
                    vertices: positions,
                    uvs: uv,
                    indices: idx,
                    masks: mask.iter().map(|m| *m as usize).collect(),
                    texture: tex[i] as usize,
                    flags: flags[i],
                    visible: dyns[i] & 1 != 0,
                    opacity: opacity[i],
                    order: orders[i],
                    multiply: multiply[i],
                    screen: screen[i],
                });
            }
            Ok(result)
        }
    }
}
