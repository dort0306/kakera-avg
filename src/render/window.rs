//! Windows egui host presented directly through D3D11, including CPU/WARP fallback.
use super::{
    Renderer, Shared,
    d3d11::{Device, Draw, Pixels, Shader, Texture, Vertex},
};
use anyhow::{Context, Result, ensure};
use eframe::egui::{self, TextureId};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    collections::BTreeMap,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Dxgi::{Common::DXGI_FORMAT_UNKNOWN, *},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Fullscreen, Window, WindowButtons, WindowId},
};

struct Painter {
    device: Rc<Device>,
    chain: IDXGISwapChain,
    target: Option<Texture>,
    managed: BTreeMap<TextureId, (image::RgbaImage, Rc<Texture>)>,
    captures: crate::capture::Shared,
    latest_copy: Option<Rc<Texture>>,
    game_copy: Option<Rc<Texture>>,
}
impl Painter {
    fn new(device: Rc<Device>, window: &Window, captures: crate::capture::Shared) -> Result<Self> {
        let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
            anyhow::bail!("需要 Win32 窗口");
        };
        let size = window.inner_size();
        let chain = device.swapchain(
            HWND(handle.hwnd.get() as *mut _),
            [size.width.max(1), size.height.max(1)],
        )?;
        let target = Some(device.backbuffer(&chain)?);
        Ok(Self {
            device,
            chain,
            target,
            managed: BTreeMap::new(),
            captures,
            latest_copy: None,
            game_copy: None,
        })
    }
    fn resize(&mut self, size: [u32; 2]) -> Result<()> {
        if size.contains(&0) {
            return Ok(());
        }
        self.target = None;
        unsafe {
            self.device.context.ClearState();
            self.chain.ResizeBuffers(
                0,
                size[0],
                size[1],
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
        }
        self.target = Some(self.device.backbuffer(&self.chain)?);
        Ok(())
    }
    fn record_capture(&mut self, revision: u64, game: bool) {
        let result = (|| -> Result<()> {
            let target = self.target.as_ref().context("截图 backbuffer")?;
            ensure!(
                target.size[0] * target.size[1] <= 16 * 1024 * 1024,
                "窗口截图缓存超过每幅 64MiB 预算"
            );
            if self
                .latest_copy
                .as_ref()
                .is_some_and(|t| t.size != target.size)
            {
                self.captures.borrow_mut().latest = None;
                self.latest_copy = None;
            }
            if self.latest_copy.is_none() {
                self.latest_copy = Some(self.device.texture(target.size, None, false)?);
            }
            let texture = self
                .latest_copy
                .as_ref()
                .expect("capture allocated")
                .clone();
            unsafe {
                self.device.context.CopyResource(&texture.raw, &target.raw);
            }
            let device = self.device.clone();
            self.captures.borrow_mut().latest = Some(crate::capture::Frame {
                revision,
                read: Rc::new(move || device.readback_texture(&texture)),
            });
            if game {
                if self
                    .game_copy
                    .as_ref()
                    .is_some_and(|t| t.size != target.size)
                {
                    self.captures.borrow_mut().game = None;
                    self.game_copy = None;
                }
                if self.game_copy.is_none() {
                    self.game_copy = Some(self.device.texture(target.size, None, false)?);
                }
                let texture = self.game_copy.as_ref().expect("capture allocated").clone();
                unsafe {
                    self.device.context.CopyResource(&texture.raw, &target.raw);
                }
                let device = self.device.clone();
                self.captures.borrow_mut().game = Some(crate::capture::Frame {
                    revision,
                    read: Rc::new(move || device.readback_texture(&texture)),
                });
            }
            self.captures.borrow_mut().error = None;
            Ok(())
        })();
        if let Err(error) = result {
            self.captures.borrow_mut().error = Some(format!("{error:#}"));
        }
    }
    fn draw(
        &mut self,
        ctx: &egui::Context,
        output: egui::FullOutput,
        capture: (u64, bool),
    ) -> Result<()> {
        for (id, delta) in &output.textures_delta.set {
            let egui::ImageData::Color(image) = &delta.image;
            let data: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            let patch =
                image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, data)
                    .context("egui texture")?;
            let data = if let Some([x, y]) = delta.pos {
                let original = &self.managed.get(id).context("缺少待更新纹理")?.0;
                ensure!(
                    x + image.size[0] <= original.width() as usize
                        && y + image.size[1] <= original.height() as usize,
                    "UI 纹理补丁越界"
                );
                let mut merged = original.clone();
                image::imageops::replace(&mut merged, &patch, x as i64, y as i64);
                merged
            } else {
                patch
            };
            let texture = self.device.texture(
                [data.width() as usize, data.height() as usize],
                Some(data.as_raw()),
                false,
            )?;
            self.managed.insert(*id, (data, texture));
        }
        let target = self.target.as_ref().context("backbuffer")?;
        self.device.clear(target);
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        ensure!(primitives.len() <= 10000, "UI primitive 预算");
        let points = [
            target.size[0] as f32 / output.pixels_per_point,
            target.size[1] as f32 / output.pixels_per_point,
        ];
        for primitive in primitives {
            let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
                anyhow::bail!("D3D11 host 不支持任意绘制回调");
            };
            if mesh.indices.is_empty() {
                continue;
            }
            let texture = match mesh.texture_id {
                TextureId::Managed(_) => self.managed.get(&mesh.texture_id).map(|v| v.1.clone()),
                TextureId::User(id) => self.device.lookup(id),
            }
            .context("UI GPU 纹理已失效")?;
            let vertices: Vec<_> = mesh
                .vertices
                .iter()
                .map(|v| Vertex {
                    pos: [
                        2.0 * v.pos.x / points[0] - 1.0,
                        1.0 - 2.0 * v.pos.y / points[1],
                    ],
                    uv: [v.uv.x, v.uv.y],
                    color: v.color.to_array().map(|c| c as f32 / 255.0),
                })
                .collect();
            let clip = primitive.clip_rect;
            let pixel = output.pixels_per_point;
            let clip = RECT {
                left: (clip.min.x * pixel).floor().max(0.0) as i32,
                top: (clip.min.y * pixel).floor().max(0.0) as i32,
                right: (clip.max.x * pixel).ceil().min(target.size[0] as f32) as i32,
                bottom: (clip.max.y * pixel).ceil().min(target.size[1] as f32) as i32,
            };
            if clip.right <= clip.left || clip.bottom <= clip.top {
                continue;
            }
            self.device.draw(Draw {
                target,
                vertices: &vertices,
                indices: &mesh.indices,
                source: &texture,
                mask: None,
                shader: Shader::Ui,
                params: [[0.0; 4]; 5],
                blend: 0,
                cull: false,
                clip: Some(clip),
            })?;
        }
        self.record_capture(capture.0, capture.1);
        unsafe {
            self.chain.Present(1, DXGI_PRESENT(0)).ok()?;
        }
        for id in output.textures_delta.free {
            self.managed.remove(&id);
        }
        Ok(())
    }
}
type Builder = Box<dyn FnOnce(&egui::Context, Shared) -> Result<crate::app::PlayerApp>>;
struct Host {
    fullscreen: bool,
    windowed: Option<(
        winit::dpi::PhysicalSize<u32>,
        Option<winit::dpi::PhysicalPosition<i32>>,
    )>,
    captures: crate::capture::Shared,
    window: Option<Window>,
    input: Option<egui_winit::State>,
    ctx: egui::Context,
    app: Option<crate::app::PlayerApp>,
    painter: Option<Painter>,
    device: Rc<Device>,
    renderer: Shared,
    build: Option<Builder>,
    next: Instant,
    error: Option<anyhow::Error>,
}
impl Host {
    fn sync_window_mode(&mut self) -> Result<()> {
        let enabled = self.app.as_ref().context("app")?.fullscreen();
        if self.fullscreen == enabled {
            return Ok(());
        }
        let window = self.window.as_ref().context("window")?;
        let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
            anyhow::bail!("需要 Win32 窗口");
        };
        let hwnd = HWND(handle.hwnd.get() as *mut _);
        // Suspend size constraints before changing decorations/monitor coverage.
        crate::window_mode::native::fullscreen(hwnd, true)?;
        if enabled {
            self.windowed = Some((window.inner_size(), window.outer_position().ok()));
            window.set_fullscreen(Some(Fullscreen::Borderless(window.current_monitor())));
        } else {
            window.set_fullscreen(None);
            if let Some((size, position)) = self.windowed.take() {
                let [width, height] = crate::window_mode::size(size.width, window.scale_factor());
                let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(width, height));
                if let Some(position) = position {
                    window.set_outer_position(position);
                }
            }
            crate::window_mode::native::fullscreen(hwnd, false)?;
        }
        self.fullscreen = enabled;
        *self.captures.borrow_mut() = Default::default();
        window.request_redraw();
        Ok(())
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        if let Some(app) = &mut self.app {
            app.shutdown();
        }
        event_loop.exit();
    }
    fn redraw(&mut self) -> Result<()> {
        let window = self.window.as_ref().context("window")?;
        if window.inner_size().width == 0 || window.inner_size().height == 0 {
            return Ok(());
        }
        let input = self.input.as_mut().context("input")?;
        let raw = input.take_egui_input(window);
        let app = self.app.as_mut().context("app")?;
        let output = self.ctx.run(raw, |ctx| app.update_ui(ctx));
        input.handle_platform_output(window, output.platform_output.clone());
        self.painter
            .as_mut()
            .context("painter")?
            .draw(&self.ctx, output, app.painted_capture())?;
        self.sync_window_mode()
    }
    fn fallback(&mut self, reason: String) -> Result<()> {
        ensure!(!self.device.software, "WARP 呈现失败，无法继续：{reason}");
        self.renderer
            .borrow_mut()
            .fallback(format!("D3D11 设备/呈现失败，切换 WARP + CPU：{reason}"));
        let device = Device::new(true)?;
        *self.captures.borrow_mut() = Default::default();
        let mut painter = Painter::new(
            device.clone(),
            self.window.as_ref().context("window")?,
            self.captures.clone(),
        )?;
        if let Some(old) = &self.painter {
            for (id, (data, _)) in &old.managed {
                let texture = device.texture(
                    [data.width() as usize, data.height() as usize],
                    Some(data.as_raw()),
                    false,
                )?;
                painter.managed.insert(*id, (data.clone(), texture));
            }
        }
        self.device = device;
        self.painter = Some(painter);
        self.app
            .as_mut()
            .context("app")?
            .rebuild_cpu_presentation()?;
        Ok(())
    }
}
impl ApplicationHandler for Host {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<()> {
            let window = event_loop.create_window(
                Window::default_attributes()
                    .with_title("Kakera AVG · D3D11")
                    .with_enabled_buttons(WindowButtons::CLOSE | WindowButtons::MINIMIZE)
                    .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
                    .with_min_inner_size(winit::dpi::LogicalSize::new(640.0, 360.0)),
            )?;
            let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() else {
                anyhow::bail!("需要 Win32 窗口");
            };
            crate::window_mode::native::install(HWND(handle.hwnd.get() as *mut _))?;
            let input = egui_winit::State::new(
                self.ctx.clone(),
                egui::ViewportId::ROOT,
                &window,
                Some(window.scale_factor() as f32),
                window.theme(),
                Some(8192),
            );
            let painter = Painter::new(self.device.clone(), &window, self.captures.clone())?;
            let mut app =
                self.build.take().context("app builder")?(&self.ctx, self.renderer.clone())?;
            app.attach_capture(self.captures.clone());
            self.window = Some(window);
            self.input = Some(input);
            self.painter = Some(painter);
            self.app = Some(app);
            self.sync_window_mode()?;
            Ok(())
        })();
        if let Err(e) = result {
            self.fail(event_loop, e);
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let (Some(input), Some(window)) = (&mut self.input, &self.window) {
            let _ = input.on_window_event(window, &event);
        }
        let result = match event {
            WindowEvent::CloseRequested => {
                if let Some(app) = &mut self.app {
                    app.shutdown();
                }
                event_loop.exit();
                Ok(())
            }
            WindowEvent::Resized(size) => self
                .painter
                .as_mut()
                .map_or(Ok(()), |p| p.resize([size.width, size.height])),
            WindowEvent::RedrawRequested => self.redraw(),
            _ => Ok(()),
        };
        if let Err(e) = result
            && let Err(e) = self.fallback(format!("{e:#}"))
        {
            self.fail(event_loop, e);
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
            self.next = now
                + Duration::from_millis(if self.window.as_ref().is_some_and(Window::has_focus) {
                    16
                } else {
                    100
                });
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next));
    }
}
pub fn run(
    mode: &str,
    build: impl FnOnce(&egui::Context, Shared) -> Result<crate::app::PlayerApp> + 'static,
) -> Result<()> {
    let device = match Device::new(false) {
        Ok(d) => d,
        Err(e) if mode != "d3d11" => {
            eprintln!("硬件 D3D11 不可用，使用 WARP/CPU：{e:#}");
            Device::new(true)?
        }
        Err(e) => return Err(e),
    };
    let renderer = if mode == "cpu" || device.software {
        Renderer::cpu()
    } else {
        Renderer::with_gpu(Box::new(Pixels::new(device.clone())))
    };
    let mut host = Host {
        fullscreen: false,
        windowed: None,
        captures: Default::default(),
        window: None,
        input: None,
        ctx: egui::Context::default(),
        app: None,
        painter: None,
        device,
        renderer,
        build: Some(Box::new(build)),
        next: Instant::now(),
        error: None,
    };
    EventLoop::new()?.run_app(&mut host)?;
    if let Some(error) = host.error {
        return Err(error);
    }
    Ok(())
}
