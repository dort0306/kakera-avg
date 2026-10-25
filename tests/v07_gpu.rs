#![cfg(windows)]
use anyhow::Result;
use image::{Rgba, RgbaImage};
use kakera_avg::{
    live2d::core::{Drawable, V2, V4},
    model::effects::Filter,
    render::{
        self, ModelDraw, PixelBackend,
        d3d11::{Device, Pixels},
    },
};
fn device() -> Result<std::rc::Rc<Device>> {
    Device::new(false).or_else(|_| Device::new(true))
}
fn quad(texture: usize) -> Drawable {
    Drawable {
        vertices: vec![
            V2 { x: -1., y: -1. },
            V2 { x: 1., y: -1. },
            V2 { x: 1., y: 1. },
            V2 { x: -1., y: 1. },
        ],
        uvs: vec![
            V2 { x: 0., y: 0. },
            V2 { x: 1., y: 0. },
            V2 { x: 1., y: 1. },
            V2 { x: 0., y: 1. },
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        masks: vec![],
        texture,
        flags: 0,
        visible: true,
        opacity: 1.,
        order: 1,
        multiply: V4 {
            r: 1.,
            g: 1.,
            b: 1.,
            a: 1.,
        },
        screen: V4 {
            r: 0.,
            g: 0.,
            b: 0.,
            a: 0.,
        },
    }
}
#[test]
fn d3d11_filters_run_real_shaders_and_preserve_premultiplied_transparency() -> Result<()> {
    let device = device()?;
    let mut gpu = Pixels::new(device.clone());
    let source = RgbaImage::from_fn(8, 8, |x, y| {
        Rgba([
            (x * 30) as u8,
            (y * 30) as u8,
            100,
            if x < 4 { 255 } else { 128 },
        ])
    });
    for filter in [Filter::Grayscale, Filter::Sepia, Filter::Pixelate(2)] {
        let actual = device.readback(&gpu.filter(&source, filter)?)?;
        let expected = kakera_avg::assets::filter_image(&source, filter);
        let expected = eframe::egui::ColorImage::from_rgba_unmultiplied([8, 8], expected.as_raw());
        let max = actual
            .pixels
            .iter()
            .zip(&expected.pixels)
            .flat_map(|(a, b)| {
                a.to_array()
                    .into_iter()
                    .zip(b.to_array())
                    .map(|(a, b)| a.abs_diff(b))
            })
            .max()
            .unwrap();
        assert!(max <= 3, "{filter:?}: maximum channel difference {max}");
    }
    let source = RgbaImage::from_fn(8, 8, |x, _| {
        if x < 4 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 255, 0, 0])
        }
    });
    let blurred = device.readback(&gpu.filter(&source, Filter::Blur(2))?)?;
    assert!(blurred.pixels.iter().any(|p| p.a() > 0 && p.a() < 255));
    for pixel in blurred.pixels {
        assert!(pixel.g() <= 1 && pixel.b() <= 1);
        assert!(pixel.r().abs_diff(pixel.a()) <= 1);
    }
    Ok(())
}
#[test]
fn d3d11_masks_culling_blending_and_color_match_cpu_centers() -> Result<()> {
    let device = device()?;
    let mut gpu = Pixels::new(device.clone());
    let textures = vec![
        RgbaImage::from_pixel(2, 2, Rgba([255; 4])),
        RgbaImage::from_pixel(2, 2, Rgba([200, 80, 40, 180])),
    ];
    for mode in 0..6 {
        let mut source = quad(0);
        source.visible = false;
        source.opacity = 0.;
        let mut back = quad(0);
        back.order = 0;
        let mut target = quad(1);
        target.masks = vec![0];
        target.flags = match mode {
            1 => 8,
            2 => 1,
            3 => 2,
            _ => 0,
        };
        if mode == 4 {
            target.multiply.r = 0.5;
            target.screen.b = 0.3;
            target.opacity = 0.5;
        }
        if mode == 5 {
            target.flags = 4;
        }
        let meshes = vec![source, back, target];
        let draw = || ModelDraw {
            meshes: meshes.clone(),
            model: 1,
            textures: &textures,
            canvas: [2., 2.],
            origin: [1., 1.],
            ppu: 1.,
            opacity: 1.,
        };
        let actual = device.readback(&gpu.model(draw())?)?;
        let render::Image::Cpu(expected) = render::Renderer::cpu().borrow_mut().model(draw())?
        else {
            unreachable!()
        };
        let a = actual.pixels[actual.size[0] * (actual.size[1] / 2) + actual.size[0] / 2];
        let b = expected.pixels[expected.size[0] * (expected.size[1] / 2) + expected.size[0] / 2];
        assert!(
            a.to_array()
                .into_iter()
                .zip(b.to_array())
                .all(|(a, b)| a.abs_diff(b) <= 3),
            "mode {mode}: GPU {a:?} CPU {b:?}"
        );
    }
    Ok(())
}
#[test]
fn failed_gpu_operation_switches_to_real_cpu_fallback() -> Result<()> {
    struct Failed;
    impl PixelBackend for Failed {
        fn name(&self) -> &str {
            "failed"
        }
        fn clear(&mut self) {}
        fn filter(&mut self, _: &RgbaImage, _: Filter) -> Result<render::Image> {
            anyhow::bail!("injected failure")
        }
        fn model(&mut self, _: ModelDraw<'_>) -> Result<render::Image> {
            anyhow::bail!("failure")
        }
    }
    let renderer = render::Renderer::with_gpu(Box::new(Failed));
    let image = renderer.borrow_mut().filter(
        &RgbaImage::from_pixel(2, 2, Rgba([200, 0, 0, 255])),
        Filter::Grayscale,
    );
    assert!(matches!(image, render::Image::Cpu(_)));
    assert_eq!(renderer.borrow().name(), "CPU fallback");
    assert!(renderer.borrow().warning.is_some());
    Ok(())
}

#[test]
fn real_haru_uses_native_gpu_output_and_preserves_cpu_silhouette() -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let source = kakera_avg::content::DirectorySource::new(&root)?;
    let (loaded, _) = kakera_avg::loader::load_game(&source)?;
    let prepared = kakera_avg::live2d::PreparedLive2D::load(
        &loaded.story,
        &source,
        kakera_avg::runtime::default_core().as_deref(),
        false,
    )?;
    let device = device()?;
    let renderer = render::Renderer::with_gpu(Box::new(Pixels::new(device.clone())));
    let frame = &loaded.story.frames["live2d.stage"];
    let player =
        kakera_avg::live2d::Live2DPlayer::prepare_with(frame, &prepared, renderer.clone())?;
    let native = player.images.values().next().unwrap();
    assert!(matches!(native, render::Image::Gpu(_)));
    assert!(renderer.borrow().is_gpu());
    assert!(renderer.borrow().warning.is_none());
    let actual = device.readback(native)?;
    assert_eq!(*actual.size.iter().max().unwrap(), 1024);
    let cpu = kakera_avg::live2d::Live2DPlayer::prepare(frame, &prepared)?;
    let render::Image::Cpu(reference) = cpu.images.values().next().unwrap() else {
        unreachable!()
    };
    let a = actual.pixels.iter().filter(|p| p.a() > 128).count() as f64
        / (actual.size[0] * actual.size[1]) as f64;
    let b = reference.pixels.iter().filter(|p| p.a() > 128).count() as f64
        / (reference.size[0] * reference.size[1]) as f64;
    assert!(
        a > 0.05 && (a - b).abs() < 0.02,
        "silhouette coverage GPU {a} CPU {b}"
    );
    Ok(())
}
