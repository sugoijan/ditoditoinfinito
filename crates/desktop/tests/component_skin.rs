//! A skin whose taps and receptors are components of its SVG
//! (`docs/plans/noteskin-scene.md`): read as play reads it, each part an
//! image of its own, and drawn by the play's renderer offscreen: the tap
//! turned for its lane, its charge's frame from the note's clock, the
//! receptor's flash from the lane's press. The rest falls back to the
//! bundled skins. The drawing is skipped without a GPU adapter (CI).

mod common;

use ddi_chart::{Layout, Quantization};
use ddi_engine::frame::{NoteSprite, SpriteKind};
use ddi_noteskin::lua::Env;
use ddi_render::FieldGeometry;
use ddi_render::noteskin::NoteLayer;
use ddi_render::textured::TexturedPipeline;

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
  <defs>
    <linearGradient id="ramp"><stop offset="0" stop-color="#00ff00"/><stop offset="1" stop-color="#ff00ff"/></linearGradient>
    <symbol id="arrow" viewBox="-32 -32 64 64">
      <path id="body" d="M-24 -8 L24 -8 L0 24 Z" fill="#ff0000"/>
      <g id="charge" data-frames="">
        <rect x="-4" y="-24" width="8" height="8" fill="#00ff00"/>
        <rect x="-4" y="-24" width="8" height="8" fill="#0000ff"/>
      </g>
    </symbol>
    <symbol id="explosion" viewBox="-32 -32 64 64">
      <rect id="spare" x="-30" y="-10" width="20" height="20" fill="#ffff00"/>
      <rect id="great" x="-30" y="-10" width="20" height="20" fill="#ffff00"/>
      <rect id="good" x="10" y="-10" width="20" height="20" fill="#0000ff"/>
    </symbol>
    <symbol id="receptor" viewBox="-32 -32 64 64">
      <rect x="-28" y="-28" width="56" height="56" fill="none" stroke="#808080" stroke-width="4"/>
      <rect id="flash" x="-20" y="-20" width="40" height="40" fill="#ffffff"/>
      <rect id="judged" x="-4" y="21" width="8" height="4" fill="#00ff00"/>
    </symbol>
  </defs>
</svg>"##;

/// Paints its part's left half (in the component's coordinates) in its
/// first parameter, its right half in the ramp a quarter across.
const WGSL: &str = "fn paint(m: Material) -> vec4<f32> {
    if (m.pos.x < 0.0) {
        return vec4<f32>(m.params[0].rgb, 1.0);
    }
    return vec4<f32>(ramp(0u, 0.25 + m.phase), 1.0);
}
";

const LUA: &str = r##"local ret = ... or {}
local oldRedir, oldLoad = ret.Redir, ret.Load
local turn = {Left = 90, Down = 0, Up = 180, Right = 270}
ret.Redir = function(button, element)
    local e = string.lower(element)
    if e == "tap note" or e == "receptor" or e == "explosion" then return button, element end
    return oldRedir(button, element)
end
ret.Load = function()
    local e = string.lower(Var "Element")
    if e == "tap note" then
        return Def.Component {
            File = "arrow.svg", Id = "arrow", Rotation = turn[Var "Button"] or 0,
            Parts = {
                charge = {Frame = Bind.Steps {clock = "note", length = 4, count = 2}},
                body = {Material = Def.Material {
                    File = "paint.wgsl", Params = {"#00ffff"}, Images = {"arrow.svg#ramp"},
                }},
            },
        }
    end
    if e == "explosion" then
        local fade = Bind.Curve {clock = "hit", keys = {0, 1, 0.2, 0}}
        return Def.Component {
            File = "arrow.svg", Id = "explosion", Duration = 0.5,
            Parts = {
                spare = {When = {"Held"}},
                great = {When = {"W1"}, Opacity = fade}, good = {When = {"W2"}, Opacity = fade},
            },
        }
    end
    if e == "receptor" then
        return Def.Component {
            File = "arrow.svg", Id = "receptor",
            Zoom = Bind.Curve {clock = "press", keys = {0, 0.5, 0.1, 1}}, Pivot = {0, 8},
            Parts = {
                flash = {Opacity = Bind.Curve {clock = "press", keys = {0, 1, 0.2, 0}}, Tint = "#ffff00"},
                judged = {When = {"W1"}, Opacity = Bind.Curve {clock = "hit", keys = {0, 1, 0.2, 0}}},
            },
        }
    end
    return oldLoad()
end
return ret
"##;

fn skin_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ddi-component-skin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("arrow.svg"), SVG).unwrap();
    std::fs::write(dir.join("NoteSkin.lua"), LUA).unwrap();
    std::fs::write(dir.join("paint.wgsl"), WGSL).unwrap();
    std::fs::write(
        dir.join("metrics.ini"),
        "[Global]\nFallbackNoteSkin=Peters-DDR\n",
    )
    .unwrap();
    dir
}

#[test]
fn a_component_skin_reads_and_draws() {
    common::indexes();
    if ddi_desktop::library::noteskins::bundled().is_empty() {
        eprintln!("no bundled noteskins' index: skipped");
        return;
    }
    let dir = skin_dir();
    let custom = ddi_desktop::library::noteskins::DiskSkin {
        game: "dance".into(),
        name: "Components".into(),
        dir: dir.clone(),
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
    // Each part an image: the arrow's body, its charge's two frames, the
    // receptor's outline and flash.
    let parts: Vec<_> = ready
        .report
        .images
        .iter()
        .filter(|i| i.path.contains('#'))
        .map(|i| (i.path.as_str(), i.outcome.clone()))
        .collect();
    let kept = |p: &str| {
        parts
            .iter()
            .any(|(q, o)| *q == p && *o == ddi_noteskin::report::Outcome::Kept)
    };
    for p in [
        "arrow.svg#arrow/0",
        "arrow.svg#arrow/1/0",
        "arrow.svg#arrow/1/1",
        "arrow.svg#receptor/0",
        "arrow.svg#receptor/1",
        "arrow.svg#ramp",
    ] {
        assert!(kept(p), "{p} drawn: {parts:?}");
    }
    // `great` is `spare`'s pixels: kept once, drawn with its texture.
    let shared = ddi_noteskin::report::Outcome::Shared("arrow.svg#explosion/0".into());
    assert!(
        parts
            .iter()
            .any(|(p, o)| *p == "arrow.svg#explosion/1" && *o == shared),
        "{parts:?}"
    );
    assert!(
        ready
            .report
            .summary()
            .contains("the same pixels as another")
    );
    let _ = std::fs::remove_dir_all(&dir);

    let instance = ddi_desktop::gpu::instance();
    let Some(adapter) = ddi_desktop::gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: drawing skipped");
        return;
    };
    let (device, queue) = ddi_desktop::gpu::device(&adapter).unwrap();
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
    let (w, h) = (256u32, 384u32);
    let geo = FieldGeometry {
        arrow: 64.0,
        left: 0.0,
        receptor_y: 64.0,
        direction: 1.0,
        width: w as f32,
        height: h as f32,
    };
    let mut demo = ddi_engine::demo::Demo::new("dance-single", 120.0, 1.0).unwrap();
    let mut frame = demo.step(0.0).frame;
    // A Left tap and a Down tap, both two arrows below the receptors, on
    // beats whose charge shows its first frame (Left, on the beat) and
    // its second (Down, an 8th: Vivid moves it two beats on).
    let tap = |lane: u8, beat: f32| NoteSprite {
        note_index: lane as usize,
        lane,
        y: 2.0,
        kind: SpriteKind::Tap,
        quantization: Quantization::N4,
        beat_frac: beat.fract(),
        color: None,
        alpha: 1.0,
        glow: 0.0,
        beat,
        hold_life: 1.0,
    };
    frame.notes = vec![tap(0, 4.0), tap(1, 4.5)];
    let draw_with = |ready: &mut ddi_desktop::play::skins::Prepared,
                     pipeline: &mut TexturedPipeline,
                     frame: &ddi_engine::frame::Frame,
                     events: &[ddi_engine::frame::SkinEvent]| {
        ready.play.update(frame, events.iter(), false);
        let mut quads = Vec::new();
        ready.play.receptors(frame, &layout, &geo, &mut quads);
        ready.play.notes(frame, &layout, &geo, &mut quads);
        ready.play.explosions(frame, &layout, &geo, &mut quads);
        pipeline.name_skin_materials(&device, ready.play.skin_materials(), &mut quads);
        // Its parts packed into atlas pages, as the renderer does.
        let mut packing = device.create_command_encoder(&Default::default());
        pipeline.pack(&device, &mut packing, ready.play.atlas_textures());
        queue.submit(Some(packing.finish()));
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
    };
    let draw =
        |ready: &mut ddi_desktop::play::skins::Prepared,
         pipeline: &mut TexturedPipeline,
         frame: &ddi_engine::frame::Frame| draw_with(ready, pipeline, frame, &[]);
    frame.song_time = 0.0;
    frame.beat = 0.0;
    for r in &mut frame.receptors {
        r.pressed = false;
    }
    let first = draw(&mut ready, &mut pipeline, &frame);
    // The Down lane pressed, drawn 0.05 s later: its flash at 3/4.
    frame.receptors[1].pressed = true;
    let _ = draw(&mut ready, &mut pipeline, &frame);
    frame.song_time = 0.05;
    let pressed = draw(&mut ready, &mut pipeline, &frame);
    let pixel = |data: &[u8], x: f32, y: f32| {
        let i = ((y as u32 * w + x as u32) * 4) as usize;
        [data[i], data[i + 1], data[i + 2]]
    };
    let (left, down) = (geo.lane_x(0.0), geo.lane_x(1.0));
    let tap_y = geo.lane_y(1, 2.0);
    // Down's arrow points down (its body's point below its middle); Left's
    // is turned a quarter, its point to the left. The body is painted by
    // its material: its own left half cyan, its right the ramp's color a
    // quarter across, turned with it (Left's own left is up the screen).
    let quarter = [64, 191, 64];
    let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 3);
    let down_right = pixel(&first, down + 6.0, tap_y);
    assert_eq!(
        pixel(&first, down - 6.0, tap_y),
        [0, 255, 255],
        "Down's body, its left"
    );
    assert!(
        near(down_right, quarter),
        "Down's body, its right: {down_right:?}"
    );
    let point = pixel(&first, down, tap_y + 18.0);
    assert!(near(point, quarter), "Down's point: {point:?}");
    assert_eq!(
        pixel(&first, left, tap_y - 6.0),
        [0, 255, 255],
        "Left's own left"
    );
    let right = pixel(&first, left, tap_y + 6.0);
    assert!(near(right, quarter), "Left's own right: {right:?}");
    assert_eq!(
        pixel(&first, left + 10.0, tap_y + 12.0),
        [0, 0, 0],
        "nothing below Left's middle"
    );
    // The charge: Left's first frame (green), Down's second (blue), each
    // turned with its arrow (above Down's middle, right of Left's).
    assert_eq!(
        pixel(&first, down, tap_y - 20.0),
        [0, 0, 255],
        "Down's charge"
    );
    assert_eq!(
        pixel(&first, left + 20.0, tap_y),
        [0, 255, 0],
        "Left's charge"
    );
    // The receptors' flash: none before a press, 3/4 of white 0.05 s
    // after one, only in the pressed lane.
    let receptor = geo.lane_receptor_y(1);
    // The whole receptor zoomed to 3/4 about its pivot (0, 8) 0.05 s after
    // the press: its outline's right side in at 21, its top at 19 (about
    // the origin it would be at 21).
    let gray = |p: [u8; 3]| p.iter().all(|c| c.abs_diff(128) <= 3);
    assert_eq!(
        pixel(&first, down + 21.0, receptor),
        [0, 0, 0],
        "at rest, no outline at 21"
    );
    assert!(
        gray(pixel(&pressed, down + 21.0, receptor)),
        "pressed, the outline at 21"
    );
    assert!(
        gray(pixel(&pressed, down, receptor - 19.0)),
        "the top about the pivot"
    );
    assert_eq!(
        pixel(&pressed, down, receptor - 22.0),
        [0, 0, 0],
        "not about the origin"
    );
    assert_eq!(pixel(&first, down, receptor), [0, 0, 0]);
    let flash = pixel(&pressed, down, receptor);
    assert!(
        flash[2] <= 3 && flash[1].abs_diff(flash[0]) <= 3,
        "the flash tinted yellow: {flash:?}"
    );
    assert!(
        (185..=197).contains(&flash[0]),
        "Down's flash at 3/4: {flash:?}"
    );
    assert_eq!(pixel(&pressed, left, receptor), [0, 0, 0]);

    // The explosion: an instance a judgment, each fading on its own
    // clock. A W1 at 0.2 s and a W2 at 0.3 s in the Down lane, drawn at
    // 0.3 s: the first at half, the second whole; at 0.6 s, none.
    frame.notes.clear();
    frame.receptors[1].pressed = false;
    let hit = |seq: u64, at: f64, judgement| ddi_engine::frame::SkinEvent {
        seq,
        lane: 1,
        kind: ddi_engine::frame::SkinEventKind::Tap {
            judgement,
            bright: false,
        },
        song_time: at,
        autoplay: true,
        danoni_judgments: false,
        note: None,
    };
    use ddi_engine::rules::Judgement;
    frame.song_time = 0.3;
    // And the press at 0.25 s a W1 for the receptor (`Step`): its judged
    // part at 3/4.
    let step = ddi_engine::frame::SkinEvent {
        kind: ddi_engine::frame::SkinEventKind::Step(Some(Judgement::W1)),
        ..hit(102, 0.25, Judgement::W1)
    };
    let events = [
        hit(100, 0.2, Judgement::W1),
        hit(101, 0.3, Judgement::W2),
        step,
    ];
    let both = draw_with(&mut ready, &mut pipeline, &frame, &events);
    let judged = pixel(&both, down, receptor + 23.0);
    assert!(
        judged[0] == 0 && judged[1].abs_diff(191) <= 3,
        "the receptor's W1 part at 3/4: {judged:?}"
    );
    let great = pixel(&both, down - 20.0, receptor);
    assert!(
        great[0].abs_diff(128) <= 3 && great[1].abs_diff(128) <= 3 && great[2] == 0,
        "the W1 at half: {great:?}"
    );
    assert_eq!(
        pixel(&both, down + 20.0, receptor),
        [0, 0, 255],
        "the W2 whole"
    );
    assert_eq!(
        pixel(&both, left - 20.0, receptor),
        [0, 0, 0],
        "nothing in other lanes"
    );
    frame.song_time = 0.6;
    let later = draw_with(&mut ready, &mut pipeline, &frame, &[]);
    assert_eq!(
        pixel(&later, down - 20.0, receptor),
        [0, 0, 0],
        "the W1 gone"
    );
    assert_eq!(
        pixel(&later, down + 20.0, receptor),
        [0, 0, 0],
        "the W2 faded"
    );
    assert_eq!(
        pixel(&later, down, receptor + 23.0),
        [0, 0, 0],
        "the receptor's W1 part faded"
    );
}
