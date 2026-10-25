//! D3D11 device, native images, shader filters and Cubism mesh/mask compositing.
use super::{GpuImage, Image, ModelDraw, PixelBackend};
use crate::model::effects::Filter;
use anyhow::{Context, Result, ensure};
use eframe::egui::{ColorImage, TextureId};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::atomic::{AtomicU64, Ordering},
};
use windows::{
    Win32::{
        Foundation::{HMODULE, RECT},
        Graphics::{
            Direct3D::Fxc::*,
            Direct3D::*,
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
    },
    core::{Interface, PCSTR, s},
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}
pub struct Texture {
    pub raw: ID3D11Texture2D,
    pub srv: ID3D11ShaderResourceView,
    pub target: Option<ID3D11RenderTargetView>,
    pub size: [usize; 2],
}
pub struct Device {
    pub raw: ID3D11Device,
    pub context: ID3D11DeviceContext,
    vertex: ID3D11VertexShader,
    ui: ID3D11PixelShader,
    model: ID3D11PixelShader,
    filter: ID3D11PixelShader,
    layout: ID3D11InputLayout,
    constants: ID3D11Buffer,
    blends: Vec<ID3D11BlendState>,
    rasters: Vec<ID3D11RasterizerState>,
    samplers: Vec<ID3D11SamplerState>,
    native: RefCell<BTreeMap<u64, Weak<Texture>>>,
    pub software: bool,
}
pub enum Shader {
    Ui,
    Model,
    Filter,
}
pub struct Draw<'a> {
    pub target: &'a Texture,
    pub vertices: &'a [Vertex],
    pub indices: &'a [u32],
    pub source: &'a Texture,
    pub mask: Option<&'a Texture>,
    pub shader: Shader,
    pub params: [[f32; 4]; 5],
    pub blend: usize,
    pub cull: bool,
    pub clip: Option<RECT>,
}
fn bytes<T: Copy>(values: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}
fn compile(entry: PCSTR, target: PCSTR) -> Result<Vec<u8>> {
    let source = include_bytes!("shaders.hlsl");
    let mut blob = None;
    let mut error = None;
    unsafe {
        let result = D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            s!("kakera.hlsl"),
            None,
            None,
            entry,
            target,
            D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut blob,
            Some(&mut error),
        );
        if let Err(e) = result {
            let message = error
                .map(|b: ID3DBlob| {
                    String::from_utf8_lossy(std::slice::from_raw_parts(
                        b.GetBufferPointer().cast(),
                        b.GetBufferSize(),
                    ))
                    .into_owned()
                })
                .unwrap_or_default();
            anyhow::bail!("HLSL 编译失败：{e} {message}");
        }
        let blob = blob.context("shader blob")?;
        Ok(
            std::slice::from_raw_parts(blob.GetBufferPointer().cast(), blob.GetBufferSize())
                .to_vec(),
        )
    }
}
impl Device {
    pub fn new(software: bool) -> Result<Rc<Self>> {
        unsafe {
            let mut raw = None;
            let mut context = None;
            let mut level = D3D_FEATURE_LEVEL::default();
            D3D11CreateDevice(
                None,
                if software {
                    D3D_DRIVER_TYPE_WARP
                } else {
                    D3D_DRIVER_TYPE_HARDWARE
                },
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[
                    D3D_FEATURE_LEVEL_11_0,
                    D3D_FEATURE_LEVEL_10_1,
                    D3D_FEATURE_LEVEL_10_0,
                ]),
                D3D11_SDK_VERSION,
                Some(&mut raw),
                Some(&mut level),
                Some(&mut context),
            )?;
            let raw = raw.context("D3D11 device")?;
            let context = context.context("D3D11 context")?;
            let code = compile(s!("vs_main"), s!("vs_4_0"))?;
            let mut vertex = None;
            raw.CreateVertexShader(&code, None, Some(&mut vertex))?;
            let mut layout = None;
            raw.CreateInputLayout(
                &[
                    D3D11_INPUT_ELEMENT_DESC {
                        SemanticName: s!("POSITION"),
                        Format: DXGI_FORMAT_R32G32_FLOAT,
                        ..Default::default()
                    },
                    D3D11_INPUT_ELEMENT_DESC {
                        SemanticName: s!("TEXCOORD"),
                        Format: DXGI_FORMAT_R32G32_FLOAT,
                        AlignedByteOffset: 8,
                        ..Default::default()
                    },
                    D3D11_INPUT_ELEMENT_DESC {
                        SemanticName: s!("COLOR"),
                        Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                        AlignedByteOffset: 16,
                        ..Default::default()
                    },
                ],
                &code,
                Some(&mut layout),
            )?;
            let ps = |entry| -> Result<ID3D11PixelShader> {
                let code = compile(entry, s!("ps_4_0"))?;
                let mut shader = None;
                raw.CreatePixelShader(&code, None, Some(&mut shader))?;
                shader.context("pixel shader")
            };
            let ui = ps(s!("ps_ui"))?;
            let model = ps(s!("ps_model"))?;
            let filter = ps(s!("ps_filter"))?;
            let mut constants = None;
            raw.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: 80,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut constants),
            )?;
            let mut blends = Vec::new();
            for i in 0..3 {
                let mut desc = D3D11_BLEND_DESC::default();
                desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                    BlendEnable: true.into(),
                    SrcBlend: if i == 2 {
                        D3D11_BLEND_DEST_COLOR
                    } else {
                        D3D11_BLEND_ONE
                    },
                    DestBlend: if i == 1 {
                        D3D11_BLEND_ONE
                    } else {
                        D3D11_BLEND_INV_SRC_ALPHA
                    },
                    BlendOp: D3D11_BLEND_OP_ADD,
                    SrcBlendAlpha: if i == 0 {
                        D3D11_BLEND_ONE
                    } else {
                        D3D11_BLEND_ZERO
                    },
                    DestBlendAlpha: if i == 0 {
                        D3D11_BLEND_INV_SRC_ALPHA
                    } else {
                        D3D11_BLEND_ONE
                    },
                    BlendOpAlpha: D3D11_BLEND_OP_ADD,
                    RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
                };
                let mut state = None;
                raw.CreateBlendState(&desc, Some(&mut state))?;
                blends.push(state.context("blend")?);
            }
            let mut rasters = Vec::new();
            for cull in [false, true] {
                let mut state = None;
                raw.CreateRasterizerState(
                    &D3D11_RASTERIZER_DESC {
                        FillMode: D3D11_FILL_SOLID,
                        CullMode: if cull {
                            D3D11_CULL_BACK
                        } else {
                            D3D11_CULL_NONE
                        },
                        FrontCounterClockwise: true.into(),
                        DepthClipEnable: true.into(),
                        ScissorEnable: true.into(),
                        ..Default::default()
                    },
                    Some(&mut state),
                )?;
                rasters.push(state.context("raster")?);
            }
            let mut samplers = Vec::new();
            for filter in [
                D3D11_FILTER_MIN_MAG_MIP_POINT,
                D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            ] {
                let mut state = None;
                raw.CreateSamplerState(
                    &D3D11_SAMPLER_DESC {
                        Filter: filter,
                        AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                        AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                        AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                        MaxLOD: f32::MAX,
                        ComparisonFunc: D3D11_COMPARISON_NEVER,
                        ..Default::default()
                    },
                    Some(&mut state),
                )?;
                samplers.push(state.context("sampler")?);
            }
            println!(
                "渲染设备：D3D11 {}，feature level {:x}",
                if software {
                    "WARP（软件）"
                } else {
                    "硬件"
                },
                level.0
            );
            Ok(Rc::new(Self {
                raw,
                context,
                vertex: vertex.context("vertex shader")?,
                ui,
                model,
                filter,
                layout: layout.context("layout")?,
                constants: constants.context("constants")?,
                blends,
                rasters,
                samplers,
                native: RefCell::new(BTreeMap::new()),
                software,
            }))
        }
    }
    pub fn texture(
        &self,
        size: [usize; 2],
        data: Option<&[u8]>,
        target: bool,
    ) -> Result<Rc<Texture>> {
        unsafe {
            ensure!(
                size.iter().all(|v| *v > 0 && *v <= 8192)
                    && size[0] * size[1] * 4 <= 256 * 1024 * 1024,
                "GPU 图片尺寸/预算错误"
            );
            if let Some(data) = data {
                ensure!(data.len() == size[0] * size[1] * 4, "GPU 图片字节数错误");
            }
            let desc = D3D11_TEXTURE2D_DESC {
                Width: size[0] as u32,
                Height: size[1] as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_SHADER_RESOURCE.0
                    | if target {
                        D3D11_BIND_RENDER_TARGET.0
                    } else {
                        0
                    }) as u32,
                ..Default::default()
            };
            let initial = data.map(|d| D3D11_SUBRESOURCE_DATA {
                pSysMem: d.as_ptr().cast(),
                SysMemPitch: (size[0] * 4) as u32,
                ..Default::default()
            });
            let mut raw = None;
            self.raw.CreateTexture2D(
                &desc,
                initial.as_ref().map(|p| p as *const _),
                Some(&mut raw),
            )?;
            let raw = raw.context("texture")?;
            let mut srv = None;
            self.raw
                .CreateShaderResourceView(&raw, None, Some(&mut srv))?;
            let mut rtv = None;
            if target {
                self.raw
                    .CreateRenderTargetView(&raw, None, Some(&mut rtv))?;
            }
            Ok(Rc::new(Texture {
                raw,
                srv: srv.context("shader resource")?,
                target: rtv,
                size,
            }))
        }
    }
    pub fn native(&self, texture: Rc<Texture>) -> Image {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let size = texture.size;
        let mut registry = self.native.borrow_mut();
        registry.retain(|_, v| v.strong_count() > 0);
        registry.insert(id, Rc::downgrade(&texture));
        Image::Gpu(GpuImage {
            id: TextureId::User(id),
            size,
            allocation: texture,
        })
    }
    pub fn lookup(&self, id: u64) -> Option<Rc<Texture>> {
        self.native.borrow().get(&id).and_then(Weak::upgrade)
    }
    pub fn clear(&self, target: &Texture) {
        unsafe {
            self.context
                .ClearRenderTargetView(target.target.as_ref().expect("render target"), &[0.0; 4]);
        }
    }
    fn buffer<T: Copy>(&self, data: &[T], bind: D3D11_BIND_FLAG) -> Result<ID3D11Buffer> {
        unsafe {
            let data = bytes(data);
            ensure!(
                !data.is_empty() && data.len() <= 128 * 1024 * 1024,
                "GPU buffer 预算"
            );
            let mut buffer = None;
            self.raw.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: data.len() as u32,
                    Usage: D3D11_USAGE_IMMUTABLE,
                    BindFlags: bind.0 as u32,
                    ..Default::default()
                },
                Some(&D3D11_SUBRESOURCE_DATA {
                    pSysMem: data.as_ptr().cast(),
                    ..Default::default()
                }),
                Some(&mut buffer),
            )?;
            buffer.context("buffer")
        }
    }
    pub fn draw(&self, draw: Draw<'_>) -> Result<()> {
        unsafe {
            if draw.indices.is_empty() {
                return Ok(());
            }
            ensure!(
                draw.indices.len() <= 6_000_000
                    && draw.vertices.len() <= 2_000_000
                    && draw
                        .indices
                        .iter()
                        .all(|i| (*i as usize) < draw.vertices.len()),
                "GPU mesh 预算/索引错误"
            );
            let vb = self.buffer(draw.vertices, D3D11_BIND_VERTEX_BUFFER)?;
            let ib = self.buffer(draw.indices, D3D11_BIND_INDEX_BUFFER)?;
            let ctx = &self.context;
            ctx.PSSetShaderResources(0, Some(&[None, None]));
            ctx.OMSetRenderTargets(Some(std::slice::from_ref(&draw.target.target)), None);
            ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: draw.target.size[0] as f32,
                Height: draw.target.size[1] as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
                ..Default::default()
            }]));
            ctx.RSSetState(&self.rasters[usize::from(draw.cull)]);
            ctx.RSSetScissorRects(Some(&[draw.clip.unwrap_or(RECT {
                left: 0,
                top: 0,
                right: draw.target.size[0] as i32,
                bottom: draw.target.size[1] as i32,
            })]));
            ctx.IASetInputLayout(&self.layout);
            ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.IASetVertexBuffers(0, 1, Some(&Some(vb)), Some(&32), Some(&0));
            ctx.IASetIndexBuffer(&ib, DXGI_FORMAT_R32_UINT, 0);
            ctx.VSSetShader(&self.vertex, None);
            ctx.PSSetShader(
                match draw.shader {
                    Shader::Ui => &self.ui,
                    Shader::Model => &self.model,
                    Shader::Filter => &self.filter,
                },
                None,
            );
            ctx.UpdateSubresource(&self.constants, 0, None, draw.params.as_ptr().cast(), 0, 0);
            ctx.PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
            ctx.PSSetShaderResources(
                0,
                Some(&[
                    Some(draw.source.srv.clone()),
                    draw.mask.map(|t| t.srv.clone()),
                ]),
            );
            ctx.PSSetSamplers(
                0,
                Some(&[Some(
                    self.samplers[usize::from(matches!(draw.shader, Shader::Ui))].clone(),
                )]),
            );
            ctx.OMSetBlendState(&self.blends[draw.blend.min(2)], None, u32::MAX);
            ctx.OMSetDepthStencilState(None, 0);
            ctx.DrawIndexed(draw.indices.len() as u32, 0, 0);
            ctx.PSSetShaderResources(0, Some(&[None, None]));
            self.raw.GetDeviceRemovedReason()?;
            Ok(())
        }
    }
    pub fn readback(&self, image: &Image) -> Result<ColorImage> {
        if let Image::Cpu(image) = image {
            return Ok(image.clone());
        }
        let Image::Gpu(image) = image else {
            unreachable!()
        };
        let texture = image
            .allocation
            .downcast_ref::<Texture>()
            .context("后端图片类型错误")?;
        self.readback_texture(texture)
    }
    pub fn readback_texture(&self, texture: &Texture) -> Result<ColorImage> {
        unsafe {
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.raw.GetDesc(&mut desc);
            desc.BindFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            desc.Usage = D3D11_USAGE_STAGING;
            let mut staging = None;
            self.raw.CreateTexture2D(&desc, None, Some(&mut staging))?;
            let staging = staging.context("staging")?;
            self.context.CopyResource(&staging, &texture.raw);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let mut data = Vec::with_capacity(texture.size[0] * texture.size[1] * 4);
            for y in 0..texture.size[1] {
                data.extend_from_slice(std::slice::from_raw_parts(
                    (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                    texture.size[0] * 4,
                ));
            }
            self.context.Unmap(&staging, 0);
            Ok(ColorImage::from_rgba_premultiplied(texture.size, &data))
        }
    }
    pub fn swapchain(
        &self,
        hwnd: windows::Win32::Foundation::HWND,
        size: [u32; 2],
    ) -> Result<IDXGISwapChain> {
        unsafe {
            let dxgi: IDXGIDevice = self.raw.cast()?;
            let factory: IDXGIFactory = dxgi.GetAdapter()?.GetParent()?;
            let mut chain = None;
            factory
                .CreateSwapChain(
                    &self.raw,
                    &DXGI_SWAP_CHAIN_DESC {
                        BufferDesc: DXGI_MODE_DESC {
                            Width: size[0],
                            Height: size[1],
                            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                            ..Default::default()
                        },
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                        BufferCount: 2,
                        OutputWindow: hwnd,
                        Windowed: true.into(),
                        SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
                        ..Default::default()
                    },
                    &mut chain,
                )
                .ok()?;
            factory.MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER)?;
            chain.context("swapchain")
        }
    }
    pub fn backbuffer(&self, chain: &IDXGISwapChain) -> Result<Texture> {
        unsafe {
            let raw: ID3D11Texture2D = chain.GetBuffer(0)?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            raw.GetDesc(&mut desc);
            let mut target = None;
            self.raw
                .CreateRenderTargetView(&raw, None, Some(&mut target))?;
            // The backbuffer is never sampled; a tiny placeholder supplies the unused SRV field.
            let placeholder = self.texture([1, 1], Some(&[255; 4]), false)?;
            Ok(Texture {
                raw,
                srv: placeholder.srv.clone(),
                target,
                size: [desc.Width as usize, desc.Height as usize],
            })
        }
    }
}
fn quad() -> [Vertex; 4] {
    [
        Vertex {
            pos: [-1.0, 1.0],
            uv: [0.0, 0.0],
            color: [1.0; 4],
        },
        Vertex {
            pos: [1.0, 1.0],
            uv: [1.0, 0.0],
            color: [1.0; 4],
        },
        Vertex {
            pos: [1.0, -1.0],
            uv: [1.0, 1.0],
            color: [1.0; 4],
        },
        Vertex {
            pos: [-1.0, -1.0],
            uv: [0.0, 1.0],
            color: [1.0; 4],
        },
    ]
}
struct CachedSource {
    texture: Rc<Texture>,
    size: [u32; 2],
    bytes: usize,
}
pub struct Pixels {
    pub device: Rc<Device>,
    /// Keyed by stable model identity and texture slot, never by an image address.
    sources: BTreeMap<(u64, usize), CachedSource>,
    source_bytes: usize,
}
impl Pixels {
    pub fn new(device: Rc<Device>) -> Self {
        Self {
            device,
            sources: BTreeMap::new(),
            source_bytes: 0,
        }
    }
}
impl PixelBackend for Pixels {
    fn name(&self) -> &str {
        "D3D11 GPU"
    }
    fn clear(&mut self) {
        self.sources.clear();
        self.source_bytes = 0;
    }
    fn filter(&mut self, source: &image::RgbaImage, filter: Filter) -> Result<Image> {
        let size = [source.width() as usize, source.height() as usize];
        let input = self.device.texture(size, Some(source.as_raw()), false)?;
        let output = self.device.texture(size, None, true)?;
        self.device.clear(&output);
        let (kind, strength) = match filter {
            Filter::Grayscale => (1.0, 0.0),
            Filter::Sepia => (2.0, 0.0),
            Filter::Blur(v) => (3.0, f32::from(v)),
            Filter::Pixelate(v) => (4.0, f32::from(v)),
            Filter::None => (5.0, 0.0),
        };
        let pass = |source: &Texture, target: &Texture, vertical: f32| -> Result<()> {
            let mut params = [[0.0; 4]; 5];
            params[3] = [kind, strength, vertical, 0.0];
            params[4] = [size[0] as f32, size[1] as f32, 0.0, 0.0];
            self.device.draw(Draw {
                target,
                vertices: &quad(),
                indices: &[0, 1, 2, 0, 2, 3],
                source,
                mask: None,
                shader: Shader::Filter,
                params,
                blend: 0,
                cull: false,
                clip: None,
            })
        };
        if matches!(filter, Filter::Blur(_)) {
            let middle = self.device.texture(size, None, true)?;
            self.device.clear(&middle);
            pass(&input, &middle, 0.0)?;
            pass(&middle, &output, 1.0)?;
        } else {
            pass(&input, &output, 0.0)?;
        }
        Ok(self.device.native(output))
    }
    fn model(&mut self, draw: ModelDraw<'_>) -> Result<Image> {
        let side = 1024.0;
        let scale = side / draw.canvas[0].max(draw.canvas[1]);
        let size = [
            (draw.canvas[0] * scale).round().max(1.0) as usize,
            (draw.canvas[1] * scale).round().max(1.0) as usize,
        ];
        let mut textures = Vec::new();
        for (index, image) in draw.textures.iter().enumerate() {
            let key = (draw.model, index);
            let size = [image.width(), image.height()];
            let reusable = self
                .sources
                .get(&key)
                .is_some_and(|cached| cached.size == size);
            if !reusable {
                if let Some(previous) = self.sources.remove(&key) {
                    self.source_bytes -= previous.bytes;
                }
                let bytes = image.as_raw().len();
                let total = self.source_bytes + bytes;
                ensure!(total <= 256 * 1024 * 1024, "模型 GPU 贴图超过 256MiB");
                let texture = self.device.texture(
                    [image.width() as usize, image.height() as usize],
                    Some(image.as_raw()),
                    false,
                )?;
                self.source_bytes = total;
                self.sources.insert(
                    key,
                    CachedSource {
                        texture,
                        size,
                        bytes,
                    },
                );
            }
            textures.push(self.sources[&key].texture.clone());
        }
        let vertices: Vec<Vec<Vertex>> = draw
            .meshes
            .iter()
            .map(|mesh| {
                mesh.vertices
                    .iter()
                    .zip(&mesh.uvs)
                    .map(|(p, uv)| Vertex {
                        pos: [
                            2.0 * (p.x * draw.ppu + draw.origin[0]) / draw.canvas[0] - 1.0,
                            2.0 * (p.y * draw.ppu + draw.origin[1]) / draw.canvas[1] - 1.0,
                        ],
                        uv: [uv.x, uv.y],
                        color: [1.0; 4],
                    })
                    .collect()
            })
            .collect();
        let indices: Vec<Vec<u32>> = draw
            .meshes
            .iter()
            .map(|m| m.indices.iter().map(|i| u32::from(*i)).collect())
            .collect();
        let mut masks = BTreeMap::<Vec<usize>, Rc<Texture>>::new();
        for mesh in &draw.meshes {
            if mesh.masks.is_empty() || masks.contains_key(&mesh.masks) {
                continue;
            }
            ensure!(masks.len() < 32, "GPU 蒙版超过 32 组");
            let target = self.device.texture(size, None, true)?;
            self.device.clear(&target);
            for index in &mesh.masks {
                let source = draw.meshes.get(*index).context("蒙版引用越界")?;
                let mut params = [[0.0; 4]; 5];
                params[2] = [1.0, 0.0, 1.0, 0.0];
                params[4] = [size[0] as f32, size[1] as f32, 0.0, 0.0];
                self.device.draw(Draw {
                    target: &target,
                    vertices: &vertices[*index],
                    indices: &indices[*index],
                    source: textures.get(source.texture).context("贴图引用")?,
                    mask: None,
                    shader: Shader::Model,
                    params,
                    blend: 0,
                    cull: source.flags & 4 == 0,
                    clip: None,
                })?;
            }
            masks.insert(mesh.masks.clone(), target);
        }
        let target = self.device.texture(size, None, true)?;
        self.device.clear(&target);
        let mut order: Vec<_> = (0..draw.meshes.len()).collect();
        order.sort_by_key(|i| draw.meshes[*i].order);
        for i in order {
            let mesh = &draw.meshes[i];
            if !mesh.visible || mesh.opacity <= 0.0 {
                continue;
            }
            let mask = masks.get(&mesh.masks);
            let mut params = [[0.0; 4]; 5];
            params[0] = [mesh.multiply.r, mesh.multiply.g, mesh.multiply.b, 1.0];
            params[1] = [mesh.screen.r, mesh.screen.g, mesh.screen.b, 0.0];
            params[2] = [
                mesh.opacity * draw.opacity,
                if mask.is_none() {
                    0.0
                } else if mesh.flags & 8 != 0 {
                    2.0
                } else {
                    1.0
                },
                0.0,
                0.0,
            ];
            params[4] = [size[0] as f32, size[1] as f32, 0.0, 0.0];
            self.device.draw(Draw {
                target: &target,
                vertices: &vertices[i],
                indices: &indices[i],
                source: textures.get(mesh.texture).context("模型贴图越界")?,
                mask: mask.map(|m| &**m),
                shader: Shader::Model,
                params,
                blend: if mesh.flags & 1 != 0 {
                    1
                } else if mesh.flags & 2 != 0 {
                    2
                } else {
                    0
                },
                cull: mesh.flags & 4 == 0,
                clip: None,
            })?;
        }
        Ok(self.device.native(target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live2d::core::{Drawable, V2, V4};

    fn quad() -> Drawable {
        Drawable {
            vertices: vec![
                V2 { x: -1.0, y: -1.0 },
                V2 { x: 1.0, y: -1.0 },
                V2 { x: 1.0, y: 1.0 },
                V2 { x: -1.0, y: 1.0 },
            ],
            uvs: vec![
                V2 { x: 0.0, y: 0.0 },
                V2 { x: 1.0, y: 0.0 },
                V2 { x: 1.0, y: 1.0 },
                V2 { x: 0.0, y: 1.0 },
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            masks: Vec::new(),
            texture: 0,
            flags: 0,
            visible: true,
            opacity: 1.0,
            order: 1,
            multiply: V4 {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            screen: V4 {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            },
        }
    }

    #[test]
    fn model_texture_cache_uses_stable_ids_not_allocations() -> Result<()> {
        let mut gpu = Pixels::new(Device::new(true)?);
        let textures = vec![image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([9, 8, 7, 255]),
        )];
        let draw = |model: u64| ModelDraw {
            meshes: vec![quad()],
            model,
            textures: &textures,
            canvas: [2.0, 2.0],
            origin: [1.0, 1.0],
            ppu: 1.0,
            opacity: 1.0,
        };
        gpu.model(draw(1))?;
        assert_eq!(gpu.sources.len(), 1);
        // One identity reuses its cached GPU texture across frames.
        gpu.model(draw(1))?;
        assert_eq!(gpu.sources.len(), 1);
        // A different identity must not alias the same image allocation.
        gpu.model(draw(2))?;
        assert_eq!(gpu.sources.len(), 2);
        assert_eq!(gpu.source_bytes, textures[0].as_raw().len() * 2);
        Ok(())
    }
}
