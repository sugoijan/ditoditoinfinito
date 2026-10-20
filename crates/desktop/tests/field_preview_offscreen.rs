//! Options' field preview (`ddi_render::field_preview`) drawn on this
//! machine's GPU, under a few option sets: Reverse moves the receptors
//! to the bottom, Hidden blanks the notes near them, a Mirror turn swaps
//! the lanes' notes, the background's brightness and the field filter
//! show; and the 2D pass draws the preview's texture as an image.
//! Skipped where there is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<dir>`
//! keeps the pictures.

use ddi_desktop::gpu;
use ddi_engine::appearance::Appearance;
use ddi_engine::transform::Turn;
use ddi_render::field_preview::{FieldPreview, FieldPreviewOptions, SOURCE};
use ddi_render::texture_cache::TextureKey;
use ddi_render::{DrawList, FieldGeometry, Painter, field_span};

const FONT: &[u8] = include_bytes!("../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf");

/// Rows of 1024 bytes, as copies want them.
const SIZE: [u32; 2] = [256, 480];

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Gpu {
    fn new() -> Option<Gpu> {
        let instance = gpu::instance();
        let adapter = gpu::adapter(&instance, None)?;
        let (device, queue) = gpu::device(&adapter).ok()?;
        Some(Gpu { device, queue })
    }

    /// `texture`'s RGBA rows.
    fn read(&self, texture: &wgpu::Texture, encoder: wgpu::CommandEncoder) -> Vec<u8> {
        let mut encoder = encoder;
        let [w, h] = [texture.width(), texture.height()];
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(w * 4 * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
            },
            texture.size(),
        );
        self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        slice.get_mapped_range().expect("mapped").to_vec()
    }

    /// The preview under `options` at demo second `at`: its pixels.
    fn frame(&self, preview: &mut FieldPreview, options: &FieldPreviewOptions, at: f64) -> Vec<u8> {
        preview.set_options(options.clone());
        preview.seek(at);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        // Two frames at the same time: the second as a play's would be.
        for _ in 0..2 {
            preview.draw(&self.device, &self.queue, &mut encoder, SIZE, 100.0);
        }
        let pixels = self.read(preview.texture().expect("drawn"), encoder);
        keep(
            &pixels,
            SIZE,
            &format!("{at}-{:?}", options.play.scroll.reverse),
        );
        pixels
    }
}

fn keep(pixels: &[u8], size: [u32; 2], name: &str) {
    let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") else {
        return;
    };
    let mut ppm = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
    ppm.extend(pixels.chunks(4).flat_map(|p| [p[0], p[1], p[2]]));
    let name: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), ppm);
}

fn lum(pixels: &[u8], x: u32, y: u32) -> f32 {
    let i = ((y * SIZE[0] + x) * 4) as usize;
    (f32::from(pixels[i]) + f32::from(pixels[i + 1]) + f32::from(pixels[i + 2])) / 765.0
}

/// Lit pixels (over `floor`) in columns `x0..x1`, rows `y0..y1`.
fn lit(pixels: &[u8], [x0, x1]: [f32; 2], [y0, y1]: [f32; 2], floor: f32) -> usize {
    let (x0, x1) = (x0.max(0.0) as u32, (x1.min(SIZE[0] as f32)) as u32);
    let (y0, y1) = (y0.max(0.0) as u32, (y1.min(SIZE[1] as f32)) as u32);
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| lum(pixels, x, y) > floor)
        .count()
}

#[test]
fn the_preview_follows_the_options() {
    let Some(gpu) = Gpu::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = (&gpu.device, &gpu.queue);
    let mut preview = FieldPreview::new(
        device,
        queue,
        Some(wgpu::TextureFormat::Rgba16Float),
        &[FONT],
    );
    // No background, the notes hidden: receptors and the HUD alone.
    let bare = FieldPreviewOptions {
        background: 0.0,
        play: ddi_engine::player::PlayOptions {
            appearance: Appearance::Stealth,
            ..Default::default()
        },
        ..FieldPreviewOptions::default()
    };
    let layout = bare.layout();
    let geo = FieldGeometry::new(SIZE[0] as f32, SIZE[1] as f32, field_span(&layout), false);
    let field = [geo.left, geo.left + geo.arrow * 4.0];
    let a = geo.arrow;
    // Where the receptors' outlines are: rows below the life bar.
    let centre = |p: &[u8]| {
        let (mut sum, mut n) = (0.0, 0.0);
        for y in (a as u32)..SIZE[1] {
            let row = lit(p, field, [y as f32, y as f32 + 1.0], 0.2) as f32;
            sum += row * y as f32;
            n += row;
        }
        sum / n.max(1.0)
    };
    let down = gpu.frame(&mut preview, &bare, 0.5);
    let receptors = centre(&down);
    assert!(
        (receptors - geo.receptor_y).abs() < a * 0.5,
        "{receptors} {}",
        geo.receptor_y
    );
    let mut reverse = bare.clone();
    reverse.play.scroll.reverse = true;
    let up = gpu.frame(&mut preview, &reverse, 0.5);
    let receptors = centre(&up);
    let bottom = SIZE[1] as f32 - a * 1.5;
    assert!((receptors - bottom).abs() < a * 0.5, "{receptors} {bottom}");

    // Notes shown, mid-phrase: Hidden blanks them near the receptors, not
    // further down.
    let shown = FieldPreviewOptions {
        background: 0.0,
        ..FieldPreviewOptions::default()
    };
    let at = 3.3;
    let near = [geo.receptor_y + a * 0.6, geo.receptor_y + a * 2.6];
    let far = [geo.receptor_y + a * 4.5, SIZE[1] as f32];
    let visible = gpu.frame(&mut preview, &shown, at);
    let mut hidden = shown.clone();
    hidden.play.appearance = Appearance::Hidden;
    let hid = gpu.frame(&mut preview, &hidden, at);
    let (v_near, h_near) = (lit(&visible, field, near, 0.2), lit(&hid, field, near, 0.2));
    let (v_far, h_far) = (lit(&visible, field, far, 0.2), lit(&hid, field, far, 0.2));
    assert!(v_near > 200, "notes near the receptors: {v_near}");
    assert!(h_near * 4 < v_near, "Hidden: {h_near} of {v_near}");
    assert!(
        h_far.abs_diff(v_far) * 5 < v_far.max(1),
        "far: {h_far} {v_far}"
    );

    // Mirror: each lane's notes are the opposite lane's.
    let lanes = |p: &[u8]| -> Vec<usize> {
        (0..4)
            .map(|l| {
                let x = geo.left + a * l as f32;
                lit(
                    p,
                    [x, x + a],
                    [geo.receptor_y + a * 0.6, SIZE[1] as f32],
                    0.2,
                )
            })
            .collect()
    };
    let mut mirror = shown.clone();
    mirror.play.transform.turn = Turn::Mirror;
    let plain = lanes(&visible);
    let turned = lanes(&gpu.frame(&mut preview, &mirror, at));
    let flipped: Vec<usize> = plain.iter().rev().copied().collect();
    assert_ne!(plain, flipped, "the lanes differ: {plain:?}");
    for (t, f) in turned.iter().zip(&flipped) {
        assert!(
            t.abs_diff(*f) * 4 <= (*f).max(40),
            "{turned:?} against {flipped:?}"
        );
    }

    // The background shows at its brightness, darker under the filter.
    let mut plate = |o: &FieldPreviewOptions| {
        let p = gpu.frame(&mut preview, o, 0.5);
        // Off the field: the left margin, below the receptors.
        let (mut sum, mut n) = (0.0, 0.0);
        for y in (SIZE[1] / 2)..SIZE[1] - 10 {
            for x in 2..(geo.left as u32 - 4) {
                sum += lum(&p, x, y);
                n += 1.0;
            }
        }
        // And between the lanes' notes, where the filter is.
        let lane = lum(&p, (geo.left + a * 0.05) as u32, SIZE[1] - 8);
        (sum / n, lane)
    };
    let mut lit_bg = bare.clone();
    lit_bg.background = 1.0;
    let (bright, under) = plate(&lit_bg);
    lit_bg.background = 0.3;
    let (dim, _) = plate(&lit_bg);
    assert!(bright > 0.15 && dim < bright * 0.6, "{bright} {dim}");
    lit_bg.background = 1.0;
    lit_bg.field_filter = 0.8;
    let (beside, filtered) = plate(&lit_bg);
    assert!((beside - bright).abs() < 0.02, "{beside} {bright}");
    assert!(filtered < under * 0.5, "{filtered} {under}");

    // The 2D pass draws the texture under its source, as an image.
    let mut painter = Painter::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    let id = painter.texture(TextureKey::drawn(SOURCE, 300.0));
    painter.set_external_image(device, SOURCE, preview.texture());
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut list = DrawList::new();
    list.image([0.0, 0.0, SIZE[0] as f32, SIZE[1] as f32], id, [1.0; 4]);
    let mut encoder = device.create_command_encoder(&Default::default());
    preview.set_options(lit_bg.clone());
    preview.draw(device, queue, &mut encoder, SIZE, 100.0);
    painter.draw(
        device,
        queue,
        &mut encoder,
        &view,
        SIZE,
        1.0,
        Some(wgpu::Color::BLACK),
        &list,
        &mut |_, _| {},
    );
    let drawn = gpu.read(&target, encoder);
    let mut encoder = device.create_command_encoder(&Default::default());
    preview.draw(device, queue, &mut encoder, SIZE, 100.0);
    let own = gpu.read(preview.texture().expect("drawn"), encoder);
    let differing = drawn
        .chunks(4)
        .zip(own.chunks(4))
        .filter(|(a, b)| {
            a.iter()
                .zip(b.iter())
                .take(3)
                .any(|(x, y)| x.abs_diff(*y) > 2)
        })
        .count();
    assert!(differing < 50, "{differing} pixels differ");
    assert!(
        painter.stats().textures.loading == 0,
        "never asked of the platform"
    );
}

/// Pixels of `color` (each channel within 40) in columns `x0..x1`, rows
/// `y0..y1`.
fn tinted(pixels: &[u8], color: [f32; 4], [x0, x1]: [f32; 2], [y0, y1]: [f32; 2]) -> usize {
    let (x0, x1) = (x0.max(0.0) as u32, (x1.min(SIZE[0] as f32)) as u32);
    let (y0, y1) = (y0.max(0.0) as u32, (y1.min(SIZE[1] as f32)) as u32);
    let want = color.map(|c| (c * 255.0) as i32);
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let i = ((y * SIZE[0] + x) * 4) as usize;
            (0..3).all(|k| (i32::from(pixels[i + k]) - want[k]).abs() <= 40)
        })
        .count()
}

/// The demo keeps every other hold and drops the rest: O.K. shows in
/// gold a little under a kept hold's receptor, N.G. in red under a
/// dropped one's, and under Reverse a little over it.
#[test]
fn kept_and_dropped_holds_show_ok_and_ng_over_their_lanes() {
    use ddi_engine::demo::Demo;
    use ddi_render::field_preview::BPM;
    use ddi_render::scene::{HELD_COLOR, LET_GO_COLOR};
    let Some(gpu) = Gpu::new() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let mut preview = FieldPreview::new(
        &gpu.device,
        &gpu.queue,
        Some(wgpu::TextureFormat::Rgba16Float),
        &[FONT],
    );
    for reverse in [false, true] {
        let mut options = FieldPreviewOptions {
            background: 0.0,
            ..FieldPreviewOptions::default()
        };
        options.play.scroll.reverse = reverse;
        // When each shows (a few tenths of a second in), from the same
        // demo the preview plays.
        let mut demo =
            Demo::with_options("dance-single", BPM, options.play, &options.rules).expect("demo");
        let (mut ok, mut ng) = (None, None);
        for i in 0..(demo.length() * 60.0) as u32 {
            let t = f64::from(i) / 60.0;
            for f in demo.step(t).frame.holds.flashes {
                let slot = if f.held { &mut ok } else { &mut ng };
                if slot.is_none() && (0.35..0.45).contains(&f.age) {
                    *slot = Some((t, f.lane));
                }
            }
        }
        let geo = FieldGeometry::new(
            SIZE[0] as f32,
            SIZE[1] as f32,
            field_span(&options.layout()),
            reverse,
        );
        let a = geo.arrow;
        // 54 of an arrow's 64 past the receptor, towards the middle.
        let y = geo.receptor_y + geo.direction * a * 54.0 / 64.0;
        for (seen, color, name) in [(ok, HELD_COLOR, "O.K."), (ng, LET_GO_COLOR, "N.G.")] {
            let (t, lane) = seen.unwrap_or_else(|| panic!("no {name} in the demo"));
            let pixels = gpu.frame(&mut preview, &options, t);
            let x = geo.left + a * f32::from(lane);
            let here = tinted(&pixels, color, [x, x + a], [y - a * 0.4, y + a * 0.4]);
            assert!(
                here > 20,
                "{name} reverse {reverse}: {here} pixels at lane {lane}"
            );
            // Nowhere near the receptors' other side.
            let behind = geo.receptor_y - geo.direction * a * 0.8;
            let away = tinted(
                &pixels,
                color,
                [x, x + a],
                [behind - a * 0.3, behind + a * 0.3],
            );
            assert!(away < here / 4, "{name} reverse {reverse}: {away} behind");
        }
    }
}
