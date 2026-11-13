//! DDI's judgment words in the player's language (round 4,
//! `docs/plans/ddi-ruleset.md`): the play's HUD draws a Japanese word in
//! the language's bold face (embedded on desktop) where the HUD's Latin
//! face has no glyph, in the tier's color, and what that costs a frame
//! against English. Read back from the GPU; skipped without an adapter.

use ddi_desktop::gpu;
use ddi_engine::frame::{Frame, JudgementFlash, ReceptorState};
use ddi_engine::rules::{Judgement, ddi};
use ddi_render::{RenderOptions, Renderer, SkinBlending};

const FONT: &[u8] = include_bytes!("../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf");
const SIZE: [u32; 2] = [256, 480];

fn frame(j: Judgement, delta: f64) -> Frame {
    Frame {
        song_time: 0.0,
        note_motion: Default::default(),
        roll_window_seconds: 0.0,
        beat: 0.0,
        lanes: 4,
        receptors: vec![
            ReceptorState {
                pressed: false,
                mine: None,
                flash: None,
                holding: None,
            };
            4
        ],
        notes: Vec::new(),
        judgement: Some(JudgementFlash {
            judgement: j,
            delta_seconds: delta,
            age: 0.3,
            combo_at: 0,
            w0: Some(false),
            boo: false,
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
    }
}

fn read(device: &wgpu::Device, queue: &wgpu::Queue, tex: &wgpu::Texture) -> Vec<u8> {
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(SIZE[0] * 4 * SIZE[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
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

#[test]
fn a_japanese_judgment_word_is_drawn_in_its_tier_s_color() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let layout = ddi_chart::Layout::dance_single();
    let names = ddi::names();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
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
    // The words a play would show, early and late, every tier.
    let words: Vec<Frame> = [
        Judgement::W1,
        Judgement::W2,
        Judgement::W3,
        Judgement::W4,
        Judgement::W5,
    ]
    .into_iter()
    .flat_map(|j| [frame(j, -0.01), frame(j, 0.01)])
    .chain([frame(Judgement::Miss, 0.0)])
    .collect();
    let mut cost = Vec::new();
    for tag in ["en", "ja"] {
        assert!(ddi_l10n::force(tag));
        let mut r = Renderer::new(&device, &queue, format, SkinBlending::Scene, &[FONT]);
        // The yellow of a W2's word, Glorious's.
        let mut draw = |f: &Frame| {
            let mut enc = device.create_command_encoder(&Default::default());
            r.render(
                &device,
                &queue,
                &mut enc,
                &view,
                None,
                SIZE[0],
                SIZE[1],
                f,
                &layout,
                &names,
                RenderOptions::default(),
                None,
            );
            queue.submit(Some(enc.finish()));
        };
        draw(&frame(Judgement::W2, 0.01));
        let px = read(&device, &queue, &tex);
        let want = ddi_engine::rules::colors::DDI.tiers[1].map(|v| (v * 255.0).round() as u8);
        let lit = px
            .chunks(4)
            .filter(|p| p[..3].iter().zip(&want).all(|(a, b)| a.abs_diff(*b) <= 3))
            .count();
        assert!(lit > 150, "{tag}: {lit} pixels of the word's yellow");
        // A frame's cost: each word in turn (a new word every frame, more
        // than any chart asks), the first pass shaping and rasterizing,
        // the rest cached as in play.
        for f in &words {
            draw(f);
        }
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let n = 600;
        let start = std::time::Instant::now();
        for i in 0..n {
            draw(&words[i % words.len()]);
        }
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        cost.push((tag, start.elapsed().as_secs_f64() * 1000.0 / n as f64));
        ddi_l10n::unforce();
    }
    ddi_l10n::choose(Some("en"));
    eprintln!("a frame of the HUD with a judgment word, CPU and GPU: {cost:?} ms");
}
