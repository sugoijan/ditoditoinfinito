//! Actual GPU HUD: interrupted life changes, transparent combo gaps, and
//! plain/sRGB/encoded targets. Optional PNGs for inspecting the same frames.
use ddi_desktop::gpu;
use ddi_engine::frame::{Frame, ReceptorState};
use ddi_render::{RenderOptions, Renderer, SkinBlending};

const SIZE: [u32; 2] = [1280, 720];

fn frame(life: f32, combo: u32) -> Frame {
    Frame {
        song_time: 0.0,
        beat: 0.0,
        note_motion: Default::default(),
        roll_window_seconds: 0.0,
        lanes: 4,
        receptors: vec![
            ReceptorState {
                pressed: false,
                flash: None,
                holding: None
            };
            4
        ],
        notes: vec![],
        judgement: None,
        holds: Default::default(),
        combo,
        score: ddi_engine::rules::ScoreView {
            percent: Some(0.9876),
            ..Default::default()
        },
        life,
        danger: false,
        failed: false,
        finished: false,
        visible_range: 8.0,
        appearance: ddi_engine::appearance::Appearance::Visible,
        cover: Default::default(),
    }
}

fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    format: wgpu::TextureFormat,
    size: [u32; 2],
    frame: &Frame,
    opts: RenderOptions,
) -> Vec<u8> {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("HUD check"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.render(
        device,
        queue,
        &mut encoder,
        &view,
        None,
        size[0],
        size[1],
        frame,
        &ddi_chart::Layout::dance_single(),
        &ddi_engine::rules::presets::itg().names,
        opts,
        None,
    );
    let stride = (size[0] * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(stride * size[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        tex.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(size[1]),
            },
        },
        tex.size(),
    );
    queue.submit(Some(encoder.finish()));
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    buffer
        .slice(..)
        .get_mapped_range()
        .unwrap()
        .chunks(stride as usize)
        .flat_map(|row| &row[..(size[0] * 4) as usize])
        .copied()
        .collect()
}

fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * SIZE[0] + x) * 4) as usize;
    image[i..i + 3].try_into().unwrap()
}

fn snapshot(name: &str, size: [u32; 2], pixels: &[u8]) {
    if let Some(path) = std::env::var_os("DDI_HUD_SNAPSHOTS") {
        let path = std::path::Path::new(&path);
        std::fs::create_dir_all(path).unwrap();
        image::save_buffer(
            path.join(format!("{name}.png")),
            pixels,
            size[0],
            size[1],
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
fn accepted_hud_draws_through_all_color_paths() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    for (index, (format, blending)) in [
        (wgpu::TextureFormat::Rgba8Unorm, SkinBlending::Scene),
        (wgpu::TextureFormat::Rgba8UnormSrgb, SkinBlending::Scene),
        (
            wgpu::TextureFormat::Rgba8Unorm,
            SkinBlending::Encoded(wgpu::TextureFormat::Rgba16Float),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut renderer =
            Renderer::new(&device, &queue, format, blending, &[ddi_render::HUD_FONT]);
        let opts = RenderOptions {
            reduced_motion: true,
            ..Default::default()
        };
        let baseline = draw(
            &device,
            &queue,
            &mut renderer,
            format,
            SIZE,
            &frame(0.8, 0),
            opts,
        );
        let green = pixel(&baseline, 681, 26);
        let mut f = frame(0.5, 100);
        let red = draw(&device, &queue, &mut renderer, format, SIZE, &f, opts);
        let r = pixel(&red, 681, 26);
        assert!(r[0] > r[1] * 2 && r[0] > r[2] * 2, "red {r:?}");
        snapshot(&format!("{index}-damage-100"), SIZE, &red);
        // A white digit's left edge has a real background-colored gap
        // before the colored ring, including on the encoded float path.
        let white = |p: [u8; 3]| {
            p.into_iter()
                .zip([242, 238, 233])
                .all(|(a, b)| a.abs_diff(b) <= 2)
        };
        let mut gap = false;
        for y in 365..420 {
            if let Some(x) = (570..680).find(|&x| white(pixel(&red, x, y))) {
                let bg = pixel(&baseline, x, y);
                let clear = (2..6).any(|d| pixel(&red, x - d, y) == bg);
                let ring = (5..11).any(|d| {
                    let p = pixel(&red, x - d, y);
                    p != bg && !white(p) && p.iter().any(|&c| c > 80)
                });
                gap |= clear && ring;
            }
        }
        assert!(gap, "transparent combo gap, target {index}");
        // Same frozen turbulence, recolored at the exact same pixel.
        renderer.reset_hud();
        draw(
            &device,
            &queue,
            &mut renderer,
            format,
            SIZE,
            &frame(0.5, 100),
            opts,
        );
        f.life = 0.8;
        let opts = RenderOptions {
            hud_time: Some(0.01),
            ..RenderOptions::default()
        };
        let blue = draw(&device, &queue, &mut renderer, format, SIZE, &f, opts);
        let b = pixel(&blue, 681, 26);
        assert!(b[2] > b[0] * 2, "blue {b:?}");
        let green_mix = (f32::from(green[1]) - 92.0) / 103.0;
        let red_mix = (f32::from(r[0]) - 138.0) / 83.0;
        assert!(
            (green_mix - red_mix).abs() < 0.03,
            "shared field {green_mix} {red_mix}"
        );
        snapshot(&format!("{index}-recovery"), SIZE, &blue);
        for combo in [99, 200, 300, 900, 1000, 1200] {
            renderer.reset_hud();
            let pixels = draw(
                &device,
                &queue,
                &mut renderer,
                format,
                SIZE,
                &frame(1.0, combo),
                RenderOptions {
                    reduced_motion: true,
                    ..Default::default()
                },
            );
            snapshot(&format!("{index}-full-{combo}"), SIZE, &pixels);
        }
        let small = [320, 640];
        let pixels = draw(
            &device,
            &queue,
            &mut renderer,
            format,
            small,
            &frame(0.0, u32::MAX),
            RenderOptions {
                reverse: true,
                reduced_motion: true,
                ..Default::default()
            },
        );
        snapshot(&format!("{index}-narrow-reverse"), small, &pixels);
        let preview = draw(
            &device,
            &queue,
            &mut renderer,
            format,
            SIZE,
            &frame(1.0, 1000),
            RenderOptions {
                preview: Some(ddi_render::Preview {
                    geometry: ddi_render::FieldGeometry::new(1280.0, 720.0, 4.0, false),
                    receptors: false,
                }),
                ..Default::default()
            },
        );
        assert!(
            preview.as_chunks::<4>().0.iter().all(|p| p[..] == preview[..4]),
            "previews omit the HUD"
        );
        renderer.skin.background = wgpu::Color {
            r: 0.4,
            g: 0.5,
            b: 0.6,
            a: 1.0,
        };
        let mut bright = frame(0.2, 1000);
        bright.danger = true;
        bright.judgement = Some(ddi_engine::frame::JudgementFlash {
            judgement: ddi_engine::rules::Judgement::W1,
            delta_seconds: -0.012,
            age: 0.2,
            combo_at: 1000,
            w0: None,
        });
        let pixels = draw(
            &device,
            &queue,
            &mut renderer,
            format,
            SIZE,
            &bright,
            RenderOptions {
                show_deltas: true,
                reduced_motion: true,
                ..Default::default()
            },
        );
        snapshot(&format!("{index}-bright-warning"), SIZE, &pixels);
    }
}
