//! Colors as written: the play's HUD text (a judgement word, the combo,
//! the timing error) on each kind of target the renderer draws to (an
//! sRGB view, as WebGPU and desktop; a plain view; the encoded float path
//! of the options' field preview), and the theme's accent on the 2D
//! pass. Read back from the GPU. Skipped where there is no GPU adapter
//! (CI).

use std::collections::HashMap;
use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_engine::frame::{Frame, JudgementFlash, ReceptorState};
use ddi_engine::rules::{Judgement, presets};
use ddi_render::scene::{W1_OUTSIDE_W0, judgement_color};
use ddi_render::{RenderOptions, Renderer, SkinBlending};
use ddi_ui::UiEvent;
use ddi_ui::dev_widgets::FoldFn;

const FONT: &[u8] = include_bytes!("../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf");
const SIZE: [u32; 2] = [256, 480];

fn read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    tex: &wgpu::Texture,
    mut enc: wgpu::CommandEncoder,
) -> Vec<u8> {
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(SIZE[0] * 4 * SIZE[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        tex.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE[0] * 4),
                rows_per_image: Some(SIZE[1]),
            },
        },
        tex.size(),
    );
    queue.submit(Some(enc.finish()));
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    buf.slice(..).get_mapped_range().expect("mapped").to_vec()
}

/// The color most pixels have among those `near` (each channel within
/// 40), and whether it is within 2 of `want`.
fn shows(pixels: &[u8], want: [u8; 3]) -> (bool, Option<[u8; 3]>) {
    let mut c: HashMap<[u8; 3], usize> = HashMap::new();
    for p in pixels.chunks(4) {
        let px = [p[0], p[1], p[2]];
        if px.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 40) {
            *c.entry(px).or_default() += 1;
        }
    }
    let top = c.into_iter().max_by_key(|e| e.1).map(|e| e.0);
    (
        top.is_some_and(|t| t.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 2)),
        top,
    )
}

fn bytes(c: [f32; 4]) -> [u8; 3] {
    [c[0], c[1], c[2]].map(|v| (v * 255.0).round() as u8)
}

#[test]
fn hud_and_menu_colors_draw_as_written() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let layout = ddi_chart::Layout::dance_single();
    let names = presets::itg().names;
    // ITG FA+'s white Fantastic (a W1 outside its W0 window) last.
    for (j, w0) in [
        (Judgement::W1, None),
        (Judgement::W4, None),
        (Judgement::Miss, None),
        (Judgement::W1, Some(false)),
    ] {
        let frame = Frame {
            song_time: 0.0,
            beat: 0.0,
            lanes: 4,
            receptors: vec![
                ReceptorState {
                    pressed: false,
                    flash: None,
                    holding: None,
                };
                4
            ],
            notes: Vec::new(),
            judgement: Some(JudgementFlash {
                judgement: j,
                delta_seconds: 0.0,
                age: 0.3,
                combo_at: 0,
                w0,
            }),
            holds: Default::default(),
            combo: 0,
            score: Default::default(),
            life: 0.5,
            danger: false,
            failed: false,
            finished: false,
            visible_range: 8.0,
            appearance: ddi_engine::appearance::Appearance::Visible,
            cover: Default::default(),
        };
        for (format, blending) in [
            (wgpu::TextureFormat::Rgba8UnormSrgb, SkinBlending::Scene),
            (wgpu::TextureFormat::Rgba8Unorm, SkinBlending::Scene),
            (
                wgpu::TextureFormat::Rgba8Unorm,
                SkinBlending::Encoded(wgpu::TextureFormat::Rgba16Float),
            ),
        ] {
            let mut r = Renderer::new(&device, &queue, format, blending, &[FONT]);
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: SIZE[0],
                    height: SIZE[1],
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
            let mut enc = device.create_command_encoder(&Default::default());
            r.render(
                &device,
                &queue,
                &mut enc,
                &view,
                None,
                SIZE[0],
                SIZE[1],
                &frame,
                &layout,
                &names,
                RenderOptions::default(),
                None,
            );
            let px = read(&device, &queue, &tex, enc);
            let want = bytes(if w0 == Some(false) {
                W1_OUTSIDE_W0
            } else {
                judgement_color(j)
            });
            let (ok, top) = shows(&px, want);
            assert!(
                ok,
                "{j:?} on {format:?} {blending:?}: {top:?}, want {want:?}"
            );
        }
    }

    // The theme's accent on song select's 2D pass (the selected chart's
    // frame, the style tab).
    let mut gpu = Gpu::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    let fold: FoldFn = Rc::new(|s: &str| s.to_string());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("select", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [1280.0, 720.0],
        ratio: 1.0,
    });
    for _ in 0..60 {
        ui.update(1.0 / 60.0);
    }
    let px = gpu.draw_offscreen(&mut ui, [1280, 720], wgpu::TextureFormat::Rgba8Unorm);
    let (ok, top) = shows(&px, [0xff, 0x6e, 0xa0]);
    assert!(ok, "accent: {top:?}");
}
