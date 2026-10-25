//! What a noteskin's frame costs: a dense field of taps, holds, rolls,
//! lifts and mines in every lane, drawn by the actual loader and textured
//! renderer for a number of frames, with no window or sound. Prints the
//! quads, runs and mask clears a frame takes, the pixels those clears
//! cover, and the times to build, prepare and draw a frame (the last the
//! wait for the GPU, so a relative measure). With an output path, the
//! last frame is saved there, to compare two builds' pictures.
//! `cargo run -p ddi-desktop --no-default-features --release --example
//! noteskin_cost -- [SKIN] [OUTPUT.png]` (a bundled skin's name or a
//! skin's folder, default Infinito;
//! `DDI_COST_FRAMES`, `DDI_COST_REVERSE`, `DDI_COST_ONLY=taps|long`,
//! `DDI_COST_NO_ATLAS`; scratch DDI_HOME, GPU required).
use std::time::Instant;

use ddi_chart::{Layout, Quantization};
use ddi_engine::frame::{NoteSprite, SpriteKind};
use ddi_noteskin::lua::Env;
use ddi_render::textured::{Shape, TexQuad, TexturedPipeline};
use ddi_render::{FieldGeometry, noteskin::NoteLayer, stencil::Mask};

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// The notes of a dense field at `beat`: in each lane a hold, a roll, a
/// lift, taps and a mine, scrolling up at one arrow a beat.
fn notes(lanes: usize, beat: f64) -> Vec<NoteSprite> {
    let mut out = Vec::new();
    let note = |index: usize, lane: usize, row: f64, kind: SpriteKind| NoteSprite {
        note_index: index,
        lane: lane as u8,
        y: (row - beat) as f32,
        kind,
        quantization: Quantization::N4,
        beat_frac: row.fract() as f32,
        color: None,
        alpha: 1.0,
        glow: 0.0,
        beat: row as f32,
        hold_life: 1.0,
    };
    let rows = beat.floor();
    for lane in 0..lanes {
        // A pattern of eight beats per lane, repeated so the field stays
        // full while it scrolls.
        for cycle in 0..3 {
            let base = rows - rows.rem_euclid(8.0) + 8.0 * cycle as f64 + (lane % 4) as f64 * 0.5;
            let index = |n: usize| lane * 1000 + cycle * 10 + n;
            let length = 2.0 + (lane % 3) as f64;
            let hold = |tail_y: f32| SpriteKind::HoldHead {
                tail_y,
                active: false,
                dropped: false,
            };
            let roll = |tail_y: f32| SpriteKind::RollHead {
                tail_y,
                active: false,
                dropped: false,
            };
            let row = base;
            out.push(note(
                index(0),
                lane,
                row,
                hold((row + length - beat) as f32),
            ));
            let row = base + 3.5;
            out.push(note(index(1), lane, row, roll((row + 2.0 - beat) as f32)));
            out.push(note(index(2), lane, base + 6.0, SpriteKind::Tap));
            out.push(note(index(3), lane, base + 6.5, SpriteKind::Lift));
            out.push(note(index(4), lane, base + 7.0, SpriteKind::Mine));
            out.push(note(index(5), lane, base + 7.5, SpriteKind::Tap));
        }
    }
    // `DDI_COST_ONLY=taps|long`: only taps (and lifts, mines) or only
    // long notes, to see what each costs.
    match std::env::var("DDI_COST_ONLY").as_deref() {
        Ok("taps") => out.retain(|n| {
            !matches!(
                n.kind,
                SpriteKind::HoldHead { .. } | SpriteKind::RollHead { .. }
            )
        }),
        Ok("long") => out.retain(|n| {
            matches!(
                n.kind,
                SpriteKind::HoldHead { .. } | SpriteKind::RollHead { .. }
            )
        }),
        _ => {}
    }
    out.retain(|n| {
        let end = match n.kind {
            SpriteKind::HoldHead { tail_y, .. } | SpriteKind::RollHead { tail_y, .. } => tail_y,
            _ => n.y,
        };
        end > -1.0 && n.y < 12.0
    });
    out
}

/// The pixels of the target a mask clear covers.
fn clear_pixels(q: &TexQuad, w: f32, h: f32) -> f64 {
    let Shape::Quad { transform: m, .. } = q.shape else {
        return 0.0;
    };
    let hx = (m[0].abs() + m[2].abs()) * 0.5;
    let hy = (m[1].abs() + m[3].abs()) * 0.5;
    let x = ((m[4] + hx).min(w) - (m[4] - hx).max(0.0)).max(0.0);
    let y = ((m[5] + hy).min(h) - (m[5] - hy).max(0.0)).max(0.0);
    f64::from(x) * f64::from(y)
}

fn percentile(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let name = args.next().unwrap_or_else(|| "Infinito".into());
    let output = args.next();
    let frames: usize = env("DDI_COST_FRAMES", 240);
    let reverse = std::env::var_os("DDI_COST_REVERSE").is_some();
    let instance = ddi_desktop::gpu::instance();
    let adapter = ddi_desktop::gpu::adapter(&instance, None).ok_or("GPU required")?;
    let (device, queue) = ddi_desktop::gpu::device(&adapter)?;
    let mut layout = Layout::builtin("danoni-9B").unwrap();
    let mut center = Layout::builtin("pump-single").unwrap().lanes[2].clone();
    center.column = 9.0;
    layout.lanes.push(center);
    // A bundled skin's name, or a skin's folder.
    let dir = if name.contains('/') {
        std::path::PathBuf::from(&name)
    } else {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/noteskins/dance")
            .join(&name)
    };
    let name = dir
        .file_name()
        .map_or(name.clone(), |n| n.to_string_lossy().into_owned());
    let custom = ddi_desktop::library::noteskins::DiskSkin {
        game: "dance".into(),
        name: name.clone(),
        dir,
        bundled: false,
        variant_of: None,
    };
    let key = custom.key();
    let mut skins = vec![custom];
    skins.extend(ddi_desktop::library::noteskins::bundled());
    let mut ready =
        ddi_desktop::play::skins::prepare_authoring(&key, &layout, Env::default(), &skins)
            .ok_or("skin load failed")?;
    if let Some(e) = ready.load_error {
        return Err(e.into());
    }
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut pipeline = TexturedPipeline::new(&device, format, None, false);
    ready.play.set_srgb(true);
    for (i, image) in ready.images.iter().enumerate() {
        if let Some(image) = image {
            let [w, h] = image.size;
            let texture = pipeline.create_texture(&device, w, h, true, false);
            queue.write_texture(
                texture.as_image_copy(),
                &image.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                texture.size(),
            );
            let id = pipeline.add_texture(&device, &texture);
            ready.play.set_texture(i, id);
        }
    }
    let (w, h) = (1920u32, 1080u32);
    let geo = FieldGeometry {
        arrow: 96.0,
        left: 480.0,
        receptor_y: if reverse { 920.0 } else { 160.0 },
        direction: if reverse { -1.0 } else { 1.0 },
        width: w as f32,
        height: h as f32,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cost target"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut stencil = ddi_render::stencil::Attachment::default();
    let stencil_view = stencil.view(&device, [w, h]).clone();
    let mut demo = ddi_engine::demo::Demo::new("dance-single", 120.0, 1.0).ok_or("demo")?;
    let mut frame = demo.step(0.0).frame;
    frame.lanes = layout.lanes.len() as u8;
    frame.roll_window_seconds = 0.35;
    let (mut build, mut prepare, mut draw) = (Vec::new(), Vec::new(), Vec::new());
    let (mut quads_total, mut runs_total, mut clears_total, mut clear_px_total) = (0, 0, 0, 0.0);
    let mut quads = Vec::new();
    for n in 0..frames {
        // Two beats a second at 60 frames a second.
        frame.song_time = n as f64 / 60.0;
        frame.beat = frame.song_time * 2.0;
        frame.notes = notes(layout.lanes.len(), frame.beat);
        let began = Instant::now();
        ready.play.update(&frame, std::iter::empty(), false);
        quads.clear();
        ready.play.receptors(&frame, &layout, &geo, &mut quads);
        ready.play.holds(&frame, &layout, &geo, &mut quads);
        ready.play.notes(&frame, &layout, &geo, &mut quads);
        ready.play.explosions(&frame, &layout, &geo, &mut quads);
        let built = Instant::now();
        pipeline.name_skin_materials(&device, ready.play.skin_materials(), &mut quads);
        pipeline.set_materials(ddi_render::material::MaterialFrame {
            params: ready.play.materials(),
            beat: frame.beat as f32,
            seconds: frame.song_time as f32,
            linear: true,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        // As the renderer does: a skin's parts packed into atlas pages,
        // before the frame is prepared.
        if std::env::var_os("DDI_COST_NO_ATLAS").is_none() {
            pipeline.pack(&device, &mut encoder, ready.play.atlas_textures());
        }
        pipeline.prepare(&device, &queue, w as f32, h as f32, &quads);
        let prepared = Instant::now();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cost"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0185,
                            g: 0.0203,
                            b: 0.0383,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(ddi_render::stencil::Attachment::pass(
                    &stencil_view,
                    true,
                )),
                ..Default::default()
            });
            pipeline.draw(&mut pass, 0..quads.len() as u32, false, true);
        }
        queue.submit(Some(encoder.finish()));
        device.poll(wgpu::PollType::wait_indefinitely())?;
        let drawn = Instant::now();
        // The first frames make pipelines and warm caches.
        if n >= 10 {
            build.push((built - began).as_secs_f64() * 1e3);
            prepare.push((prepared - built).as_secs_f64() * 1e3);
            draw.push((drawn - prepared).as_secs_f64() * 1e3);
            let stats = pipeline.stats();
            quads_total += stats.quads as u64;
            runs_total += stats.runs as u64;
            clears_total += stats.stencil_clears as u64;
            clear_px_total += quads
                .iter()
                .filter(|q| q.mask == Mask::Clear)
                .map(|q| clear_pixels(q, w as f32, h as f32))
                .sum::<f64>();
        }
    }
    let counted = build.len() as f64;
    let stats = pipeline.stats();
    println!(
        "{name}, {} lanes, {w}x{h}{}, {} frames",
        layout.lanes.len(),
        if reverse { ", reverse" } else { "" },
        build.len()
    );
    println!(
        "per frame: {:.0} quads, {:.0} runs, {:.1} mask clears covering {:.1} Mpx",
        quads_total as f64 / counted,
        runs_total as f64 / counted,
        clears_total as f64 / counted,
        clear_px_total / counted / 1e6,
    );
    for (what, v) in [
        ("build", &mut build),
        ("prepare", &mut prepare),
        ("draw (GPU wait)", &mut draw),
    ] {
        println!(
            "{what}: median {:.3} ms, p95 {:.3} ms",
            percentile(v, 0.5),
            percentile(v, 0.95)
        );
    }
    println!("pipelines made during frames: {}", stats.pipelines_made);
    if let Some(output) = output {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cost readback"),
            size: (w * h * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
            },
            target.size(),
        );
        queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely())?;
        let data = slice.get_mapped_range()?;
        image::save_buffer(output, &data, w, h, image::ColorType::Rgba8)?;
    }
    Ok(())
}
