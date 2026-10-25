//! A skin's own material drawn by the play's renderer lines up with its
//! part's art: a material painting a gradient in the component's
//! coordinates against resvg drawing the same gradient as SVG, inside a
//! circle part the material is clipped to. Skipped without a GPU adapter
//! (CI).

mod common;

use ddi_chart::{Layout, Quantization};
use ddi_engine::frame::{NoteSprite, SpriteKind};
use ddi_noteskin::lua::Env;
use ddi_render::FieldGeometry;
use ddi_render::noteskin::NoteLayer;
use ddi_render::textured::TexturedPipeline;

/// The drawing's width in pixels.
const W: usize = 128;

/// The gradient's ends, in the component's coordinates and as colors.
const FROM: [f32; 2] = [-30.0, -4.0];
const TO: [f32; 2] = [30.0, 10.0];
const COLORS: [&str; 2] = ["#ff2040", "#20a0ff"];

/// The part: a circle of the component, its art white.
const PART: &str = "<circle r=\"30\" fill=\"#fff\"/>";

/// Paints the gradient from `FROM` to `TO` (the parameters' first two:
/// its ends; the next two: its colors), as SVG's `linearGradient` does
/// in sRGB.
const WGSL: &str = "fn paint(m: Material) -> vec4<f32> {
    let a = m.params[0].xy;
    let d = m.params[0].zw - a;
    let t = clamp(dot(m.pos - a, d) / dot(d, d), 0.0, 1.0);
    return vec4<f32>(mix(m.params[1].rgb, m.params[2].rgb, t), 1.0);
}
";

fn rgb(hex: &str) -> [f32; 3] {
    let v = u32::from_str_radix(&hex[1..], 16).unwrap();
    [16, 8, 0].map(|s| ((v >> s) & 0xff) as f32 / 255.0)
}

/// A skin whose tap note is a component of one part, `PART`, painted by
/// `WGSL`.
fn skin() -> std::path::PathBuf {
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"96\" height=\"96\"><defs>\
         <symbol id=\"arrow\" viewBox=\"-48 -48 96 96\"><g id=\"fill\">{PART}</g></symbol></defs></svg>"
    );
    let [c0, c1] = COLORS.map(rgb);
    let params = format!(
        "{{ {{{}, {}, {}, {}}}, {{{}, {}, {}, 1}}, {{{}, {}, {}, 1}} }}",
        FROM[0], FROM[1], TO[0], TO[1], c0[0], c0[1], c0[2], c1[0], c1[1], c1[2]
    );
    let lua = format!(
        "local ret = ... or {{}}\n\
         local oldRedir, oldLoad = ret.Redir, ret.Load\n\
         ret.Redir = function(b, e) if string.lower(e) == 'tap note' then return b, e end return oldRedir(b, e) end\n\
         ret.Load = function()\n\
           if string.lower(Var 'Element') == 'tap note' then\n\
             return Def.Component {{ File = 'arrow.svg', Id = 'arrow', Parts = {{ fill = {{\n\
               Material = Def.Material {{ File = 'paint.wgsl', Params = {params} }},\n\
             }} }} }}\n\
           end\n\
           return oldLoad()\n\
         end\n\
         return ret\n"
    );
    let dir = std::env::temp_dir().join(format!("ddi-material-art-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("arrow.svg"), svg).unwrap();
    std::fs::write(dir.join("NoteSkin.lua"), lua).unwrap();
    std::fs::write(dir.join("paint.wgsl"), WGSL).unwrap();
    std::fs::write(
        dir.join("metrics.ini"),
        "[Global]\nFallbackNoteSkin=Peters-DDR\n",
    )
    .unwrap();
    dir
}

/// The same picture as SVG: the gradient filling the part, the
/// component's coordinates centered in 96 pixels.
fn reference() -> String {
    let circle = PART.replace("fill=\"#fff\"", "fill=\"url(#g)\"");
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"96\" height=\"96\" viewBox=\"-48 -48 96 96\">\
         <defs><linearGradient id=\"g\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">\
         <stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></linearGradient></defs>{circle}</svg>",
        FROM[0], FROM[1], TO[0], TO[1], COLORS[0], COLORS[1]
    )
}

/// A tap of `dir`'s skin drawn at 1:1 with its origin at (48, 48) of a
/// target [`W`] wide (a row's bytes a multiple of 256, as a copy needs) and
/// 96 high, black around: RGBA rows.
fn draw(adapter: &wgpu::Adapter, dir: &std::path::Path) -> Vec<u8> {
    let custom = ddi_desktop::library::noteskins::DiskSkin {
        game: "dance".into(),
        name: "Material Art".into(),
        dir: dir.to_path_buf(),
        bundled: false,
        variant_of: None,
    };
    let key = custom.key();
    let mut skins = vec![custom];
    skins.extend(ddi_desktop::library::noteskins::bundled());
    let layout = Layout::dance_single();
    let mut ready =
        ddi_desktop::play::skins::prepare_authoring(&key, &layout, Env::default(), &skins)
            .expect("the skin loads");
    assert_eq!(ready.load_error, None);
    assert!(
        !ready.play.notes.iter().any(|n| n.contains("wgsl")),
        "{:?}",
        ready.play.notes
    );
    let (device, queue) = ddi_desktop::gpu::device(adapter).unwrap();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut pipeline = TexturedPipeline::new(&device, format, None, false);
    for (i, image) in ready.images.iter().enumerate() {
        if let Some(image) = image {
            let [w, h] = image.size;
            let texture = pipeline.create_texture(&device, w, h, false, false);
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
    let (w, h) = (W as u32, 96u32);
    let geo = FieldGeometry {
        arrow: 64.0,
        left: 16.0,
        receptor_y: 48.0,
        direction: 1.0,
        width: w as f32,
        height: h as f32,
    };
    let mut demo = ddi_engine::demo::Demo::new("dance-single", 120.0, 1.0).unwrap();
    let mut frame = demo.step(0.0).frame;
    frame.notes = vec![NoteSprite {
        note_index: 0,
        lane: 0,
        y: 0.0,
        kind: SpriteKind::Tap,
        quantization: Quantization::N4,
        beat_frac: 0.0,
        color: None,
        alpha: 1.0,
        glow: 0.0,
        beat: 4.0,
        hold_life: 1.0,
    }];
    ready.play.update(&frame, std::iter::empty(), false);
    let mut quads = Vec::new();
    ready.play.notes(&frame, &layout, &geo, &mut quads);
    pipeline.name_skin_materials(&device, ready.play.skin_materials(), &mut quads);
    assert!(quads.iter().any(|q| q.material.is_some()), "painted");
    pipeline.set_materials(ddi_render::material::MaterialFrame {
        params: ready.play.materials(),
        beat: frame.beat as f32,
        seconds: frame.song_time as f32,
        linear: false,
    });
    pipeline.prepare(&device, &queue, w as f32, h as f32, &quads);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
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
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pipeline.draw(&mut pass, 0..quads.len() as u32, false, false);
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(w * h * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
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
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    slice.get_mapped_range().unwrap().to_vec()
}

#[test]
fn a_material_lines_up_with_its_parts_art() {
    common::indexes();
    if ddi_desktop::library::noteskins::bundled().is_empty() {
        eprintln!("no bundled noteskins' index: skipped");
        return;
    }
    let instance = ddi_desktop::gpu::instance();
    let Some(adapter) = ddi_desktop::gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let dir = skin();
    let drawn = draw(&adapter, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    let accepted = ddi_noteskin::raster::rasterize(&reference(), 96, 96).unwrap();
    if let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") {
        let dir = std::path::PathBuf::from(dir);
        let rows: Vec<u8> = (0..96)
            .flat_map(|y| drawn[y * W * 4..y * W * 4 + 96 * 4].to_vec())
            .collect();
        image::save_buffer(
            dir.join("material-art.png"),
            &rows,
            96,
            96,
            image::ColorType::Rgba8,
        )
        .unwrap();
        image::save_buffer(
            dir.join("material-art-reference.png"),
            &accepted,
            96,
            96,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    // Inside the part (opaque in the reference, its four neighbors too, so
    // the edges' antialiasing is left out): channel differences.
    let opaque = |x: i32, y: i32| {
        (0..96).contains(&x)
            && (0..96).contains(&y)
            && accepted[((y * 96 + x) * 4 + 3) as usize] >= 250
    };
    let mut diffs = Vec::new();
    for y in 0..96i32 {
        for x in 0..96i32 {
            if ![(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .all(|(dx, dy)| opaque(x + dx, y + dy))
            {
                continue;
            }
            let (i, j) = (
                ((y * 96 + x) * 4) as usize,
                (y as usize * W + x as usize) * 4,
            );
            for c in 0..3 {
                diffs.push(drawn[j + c].abs_diff(accepted[i + c]));
            }
        }
    }
    assert!(
        diffs.len() > 3 * 1000,
        "the part's inside: {} channels",
        diffs.len()
    );
    diffs.sort_unstable();
    let mean = diffs.iter().map(|&d| f64::from(d)).sum::<f64>() / diffs.len() as f64;
    let p99 = diffs[diffs.len() * 99 / 100];
    eprintln!(
        "material against its art: {} channels, mean {mean:.2}, 99th percentile {p99}",
        diffs.len()
    );
    assert!(
        mean < 0.5 && p99 < 2,
        "mean {mean:.2}, 99th percentile {p99}"
    );
    let at = |x: usize, y: usize| &drawn[(y * W + x) * 4..(y * W + x) * 4 + 3];
    assert_eq!(at(2, 2), [0, 0, 0], "nothing outside the part");
}
