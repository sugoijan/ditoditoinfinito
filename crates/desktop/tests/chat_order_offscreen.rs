//! The chat layer's place in the frame (the maintainer, 2026-11-21: "having
//! the layer in front of the dancing hand (but of course always behind the
//! notes field)"), drawn on this machine's GPU through the renderer a play
//! uses (Options' field preview's, the demo chart): a stand-in for the
//! dancing hand (an opaque red picture as the field's underlay, where the
//! hand's picture goes in a solo play, over the whole frame, and in a stage,
//! in the band behind the field) and a stand-in for the chat's layer (a
//! green square, as `ddi_shell::chat` composites it). The chat shows over
//! the hand; the receptors and notes stay over the chat; the field filter's
//! band still dims it in the lanes. Skipped where there is no GPU adapter
//! (CI). `DDI_OFFSCREEN_PPM=<dir>` keeps the pictures.

mod common;
use ddi_desktop::gpu;
use ddi_render::field_preview::{FieldPreview, FieldPreviewOptions};
use ddi_render::{FieldGeometry, field_span};

const FONT: &[u8] = include_bytes!("../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf");

const SIZE: [u32; 2] = [256, 480];

fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture, mut encoder: wgpu::CommandEncoder) -> Vec<u8> {
    let [w, h] = [texture.width(), texture.height()];
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
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
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    slice.get_mapped_range().expect("mapped").to_vec()
}

fn px(p: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE[0] + x) * 4) as usize;
    [p[i], p[i + 1], p[i + 2], p[i + 3]]
}

/// A texture of `size` filled with `rgba` inside `rect` (pixels), clear
/// elsewhere.
fn filled(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture, rect: [u32; 4], rgba: [u8; 4]) {
    let [w, h] = [texture.width(), texture.height()];
    let [x0, y0, x1, y1] = rect;
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for y in y0..y1.min(h) {
        for x in x0..x1.min(w) {
            let i = ((y * w + x) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&rgba);
        }
    }
    let _ = device;
    queue.write_texture(
        texture.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        texture.size(),
    );
}

/// The preview's frame at demo second `at` with a stand-in hand (over
/// `hand`, `[x, y, w, h]` target pixels; none: no hand) and a stand-in
/// chat (a green square over `chat`, `[x0, y0, x1, y1]`; none: no chat).
fn frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    preview: &mut FieldPreview,
    hand: Option<[f32; 4]>,
    chat: Option<[u32; 4]>,
    name: &str,
) -> Vec<u8> {
    let layer = chat.map(|rect| {
        let t = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test-chat"),
            size: wgpu::Extent3d {
                width: SIZE[0],
                height: SIZE[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        filled(device, queue, &t, rect, [0, 255, 0, 255]);
        t
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    for _ in 0..2 {
        preview.draw_with_underlay(device, queue, &mut encoder, SIZE, 100.0, |r, d, q, e, size, _| {
            match hand {
                Some(rect) => {
                    let t = r.create_noteskin_texture(d, size[0], size[1], false);
                    filled(d, q, &t, [0, 0, size[0], size[1]], [255, 0, 0, 255]);
                    r.set_field_underlay(d, e, Some((&t, rect, 1.0)));
                }
                None => r.set_field_underlay(d, e, None),
            }
            r.set_chat_layer(d, layer.as_ref().map(|t| (t, 1.0)));
        });
    }
    let pixels = read(device, queue, preview.texture().expect("drawn"), encoder);
    common::shot(&format!("chat-order-{name}.ppm"), &pixels, SIZE);
    pixels
}

/// Over the hand, under the field: in a solo play (the hand over the
/// whole frame) and in a stage (the hand in the band behind the field),
/// the chat's pixels show where the hand's would (outside the lanes, as
/// the layer is; inside them dimmed by the filter's band, still over the
/// hand), and the receptors' lit pixels stay as without the chat.
#[test]
fn the_chat_is_over_the_hand_and_under_the_field() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let mut preview = FieldPreview::new(&device, &queue, Some(wgpu::TextureFormat::Rgba16Float), &[FONT]);
    let options = FieldPreviewOptions {
        background: 0.0,
        field_filter: 0.5,
        ..FieldPreviewOptions::default()
    };
    preview.set_options(options.clone());
    preview.seek(2.0);
    let layout = options.layout();
    let geo = FieldGeometry::new(SIZE[0] as f32, SIZE[1] as f32, field_span(&layout), false);
    let [w, h] = SIZE.map(|v| v as f32);
    for (name, hand) in [
        ("solo", [0.0, 0.0, w, h]),
        // A stage's place: the band past the receptors' row, the field's
        // width (`ddi_shell::play_view::behind_field`'s shape).
        ("stage", [geo.left - 10.0, geo.receptor_y + geo.arrow, geo.arrow * 4.0 + 20.0, h - geo.receptor_y - geo.arrow]),
    ] {
        // The whole frame's chat: every pixel of the layer green.
        let all = [0, 0, SIZE[0], SIZE[1]];
        let bare = frame(&device, &queue, &mut preview, None, None, &format!("{name}-bare"));
        let hand_only = frame(&device, &queue, &mut preview, Some(hand), None, &format!("{name}-hand"));
        let both = frame(&device, &queue, &mut preview, Some(hand), Some(all), &format!("{name}-hand-chat"));
        // Inside the hand's picture, outside the lanes (the filter's band
        // and the field): the chat's green over the hand's red.
        let x = (hand[0].max(0.0) + 2.0) as u32;
        let y = (hand[1] + hand[3] - 4.0).min(h - 2.0) as u32;
        let (r0, c0) = (px(&hand_only, x, y), px(&both, x, y));
        assert!(r0[0] > 200 && r0[1] < 40, "{name}: the hand's red at {x},{y}: {r0:?}");
        assert!(c0[1] > 200 && c0[0] < 40, "{name}: the chat over the hand at {x},{y}: {c0:?}");
        // Inside the lanes over the hand, below the receptors: the
        // filter's band dims the chat (darker than outside), still green.
        let lx = (geo.left + geo.arrow * 2.0) as u32;
        let ly = (h - 6.0) as u32;
        let lane = px(&both, lx, ly);
        assert!(lane[1] > lane[0] && lane[1] < c0[1], "{name}: the band dims the chat: {lane:?} against {c0:?}");
        // The receptors' lit pixels (bright in the bare frame) stay as
        // they were: the field is over the chat.
        let top = geo.receptor_y - geo.arrow / 2.0;
        let mut lit = 0;
        let mut kept = 0;
        for y in top.max(0.0) as u32..(top + geo.arrow) as u32 {
            for x in geo.left as u32..(geo.left + geo.arrow * 4.0) as u32 {
                let p = px(&bare, x, y);
                if p[3] > 250 && u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) > 600 {
                    lit += 1;
                    let q = px(&both, x, y);
                    if (0..3).all(|k| p[k].abs_diff(q[k]) < 24) {
                        kept += 1;
                    }
                }
            }
        }
        assert!(lit > 20, "{name}: receptors drawn: {lit}");
        assert!(kept * 10 >= lit * 9, "{name}: the receptors over the chat: {kept} of {lit}");
    }
}

/// A frame's cost with the chat over the hand, against the chat off: the
/// main thread's milliseconds to record a frame of the preview's renderer
/// (means over 300 frames) and the scene's passes. `cargo test -p
/// ddi-desktop --test chat_order_offscreen -- --ignored --nocapture`.
#[test]
#[ignore]
fn cost() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let mut preview = FieldPreview::new(&device, &queue, Some(wgpu::TextureFormat::Rgba16Float), &[FONT]);
    preview.set_options(FieldPreviewOptions {
        background: 0.0,
        field_filter: 0.5,
        ..FieldPreviewOptions::default()
    });
    let [w, h] = SIZE.map(|v| v as f32);
    let hand_texture = std::cell::RefCell::new(None::<wgpu::Texture>);
    let chat = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test-chat"),
        size: wgpu::Extent3d { width: SIZE[0], height: SIZE[1], depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    filled(&device, &queue, &chat, [0, 0, 64, 64], [0, 255, 0, 255]);
    for (name, with_chat) in [("hand, chat off", false), ("hand, chat on", true), ("hand, chat off", false), ("hand, chat on", true)] {
        let mut ms = Vec::new();
        for k in 0..300 {
            let mut encoder = device.create_command_encoder(&Default::default());
            let t = std::time::Instant::now();
            preview.draw_with_underlay(&device, &queue, &mut encoder, SIZE, 100.0 + f64::from(k) / 60.0, |r, d, q, e, size, _| {
                let mut kept = hand_texture.borrow_mut();
                let t = kept.get_or_insert_with(|| {
                    let t = r.create_noteskin_texture(d, size[0], size[1], false);
                    filled(d, q, &t, [0, 0, size[0], size[1]], [255, 0, 0, 255]);
                    t
                });
                r.set_field_underlay(d, e, Some((t, [0.0, 0.0, w, h], 1.0)));
                r.set_chat_layer(d, with_chat.then_some((&chat, 1.0)));
            });
            ms.push(t.elapsed().as_secs_f64() * 1000.0);
            queue.submit(Some(encoder.finish()));
            if k % 30 == 0 {
                let _ = device.poll(wgpu::PollType::wait_indefinitely());
            }
        }
        let stats = preview.renderer_mut().frame_stats();
        eprintln!(
            "{name}: mean {:.3} ms; passes {}, conversions {}",
            ms.iter().skip(20).sum::<f64>() / (ms.len() - 20) as f64,
            stats.passes,
            stats.conversions
        );
    }
}
