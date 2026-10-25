//! A skin whose holds are long notes of its SVG (`Def.LongNote`,
//! with lifts as `Def.Lift`,
//! `docs/plans/noteskin-scene.md`): the head, a body tile repeated from the
//! head's join and a cap, their parts' roles (band, envelope, shadow,
//! protect, outside) drawn with the stencil, the band painted by the
//! skin's material along the note, in both scroll directions. Drawn by the
//! play's renderer offscreen; skipped without a GPU adapter (CI).

mod common;

use ddi_chart::{Layout, Quantization};
use ddi_engine::frame::{NoteSprite, SpriteKind};
use ddi_noteskin::lua::Env;
use ddi_render::FieldGeometry;
use ddi_render::noteskin::NoteLayer;
use ddi_render::textured::TexturedPipeline;

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
  <defs>
    <symbol id="head" viewBox="-32 -32 64 64">
      <rect id="outside" data-role="outside" x="-32" y="-32" width="64" height="64" fill="#fff"/>
      <rect id="hole" data-role="protect" x="-10" y="-10" width="20" height="20" fill="#fff"/>
      <path id="ring" data-role="band" fill-rule="evenodd" fill="#fff"
            d="M-20 -20H20V20H-20Z M-10 -10H10V10H-10Z"/>
      <circle id="mark" r="4" fill="#ffffff"/>
    </symbol>
    <symbol id="body" viewBox="-16 0 32 16">
      <rect id="shadow" data-role="shadow" x="-16" y="0" width="32" height="16" fill="#404040"/>
      <rect id="envelope" data-role="envelope" x="-14" y="0" width="28" height="16" fill="#fff"/>
      <rect id="band" data-role="band" x="-12" y="0" width="24" height="16" fill="#fff"/>
    </symbol>
    <symbol id="rollbody" viewBox="-16 0 32 16">
      <rect id="rollenvelope" data-role="envelope" x="-14" y="0" width="28" height="16" fill="#fff"/>
      <rect id="rollband" data-role="band" x="-12" y="0" width="24" height="16" fill="#fff"/>
      <rect id="pulse" data-role="pulse" x="-14" y="0" width="28" height="16" fill="#ff0000"/>
    </symbol>
    <symbol id="cap" viewBox="-16 -16 32 32">
      <rect id="capband" data-role="band" x="-12" y="-16" width="24" height="24" fill="#fff"/>
    </symbol>
    <symbol id="lifthead" viewBox="-16 -16 32 32">
      <rect id="liftbody" x="-6" y="-6" width="12" height="12" fill="#ffffff"/>
      <rect id="liftprotect" data-role="protect" x="2" y="-16" width="6" height="10" fill="#fff"/>
      <rect id="liftoutside" data-role="outside" x="-16" y="-16" width="6" height="10" fill="#fff"/>
    </symbol>
    <symbol id="trail" viewBox="-16 -300 32 316">
      <rect id="trailshadow" data-role="shadow" x="-16" y="-300" width="32" height="316" fill="#404040"/>
      <rect id="trailmask" data-role="trail" x="-8" y="-300" width="16" height="316" fill="#fff"/>
    </symbol>
  </defs>
</svg>"##;

/// Paints the band cyan near the head (its first 100 units), magenta on.
const WGSL: &str = "fn paint(m: Material) -> vec4<f32> {
    if (m.pos.y < 100.0) {
        return vec4<f32>(m.params[0].rgb, 1.0);
    }
    return vec4<f32>(m.params[1].rgb, 1.0);
}
";

const LUA: &str = r##"local ret = ... or {}
local oldRedir, oldLoad = ret.Redir, ret.Load
ret.Redir = function(button, element)
    local e = string.lower(element)
    if string.find(e, "^hold") or string.find(e, "^roll") or e == "tap lift" then
        return button, element
    end
    return oldRedir(button, element)
end
ret.Load = function()
    local kind, part = string.match(string.lower(Var "Element"), "^(%a+) (%a+)")
    if (kind == "hold" or kind == "roll")
        and (part == "head" or part == "tail" or part == "topcap" or part == "bottomcap") then
        -- The long note owns these: never asked for.
        error("not asked: " .. Var "Element")
    end
    if string.lower(Var "Element") == "tap lift" then
        return Def.Lift {
            File = "hold.svg", Id = "lifthead", Trail = "trail", Seconds = 0.5, Fade = 1,
            Material = Def.Material {File = "fill.wgsl", Params = {"#00ffff", "#ff00ff"}},
        }
    end
    if string.find(string.lower(Var "Element"), "^roll body") then
        return Def.LongNote {
            File = "hold.svg", Head = "head", Body = "rollbody", Cap = "cap", Join = 8,
            Material = Def.Material {File = "fill.wgsl", Params = {"#00ffff", "#ff00ff"}},
            Parts = {pulse = {Opacity = 0.5}},
        }
    end
    if string.find(string.lower(Var "Element"), "^hold body") then
        return Def.LongNote {
            File = "hold.svg", Head = "head", Body = "body", Cap = "cap", Join = 8,
            Material = Def.Material {File = "fill.wgsl", Params = {"#00ffff", "#ff00ff"}},
        }
    end
    return oldLoad()
end
return ret
"##;

const W: u32 = 256;
const H: u32 = 512;

fn draw(
    adapter: &wgpu::Adapter,
    ready: &mut ddi_desktop::play::skins::Prepared,
    layout: &Layout,
    geo: &FieldGeometry,
    note: SpriteKind,
) -> (Vec<u8>, ddi_engine::frame::Frame) {
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
    let mut demo = ddi_engine::demo::Demo::new("dance-single", 120.0, 1.0).unwrap();
    let mut frame = demo.step(0.0).frame;
    // A Down note an arrow from the receptor.
    frame.notes = vec![NoteSprite {
        note_index: 0,
        lane: 1,
        y: 1.0,
        kind: note,
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
    ready.play.holds(&frame, layout, geo, &mut quads);
    ready.play.notes(&frame, layout, geo, &mut quads);
    pipeline.name_skin_materials(&device, ready.play.skin_materials(), &mut quads);
    pipeline.set_materials(ddi_render::material::MaterialFrame {
        params: ready.play.materials(),
        beat: frame.beat as f32,
        seconds: frame.song_time as f32,
        linear: false,
    });
    pipeline.prepare(&device, &queue, W as f32, H as f32, &quads);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: W,
            height: H,
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
    let stencil_view = stencil.view(&device, [W, H]).clone();
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
            depth_stencil_attachment: Some(ddi_render::stencil::Attachment::pass(
                &stencil_view,
                true,
            )),
            ..Default::default()
        });
        pipeline.draw(&mut pass, 0..quads.len() as u32, false, true);
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(W * H * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(W * 4),
                rows_per_image: Some(H),
            },
        },
        target.size(),
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    (slice.get_mapped_range().unwrap().to_vec(), frame)
}

#[test]
fn a_long_note_skin_draws_its_holds() {
    common::indexes();
    if ddi_desktop::library::noteskins::bundled().is_empty() {
        eprintln!("no bundled noteskins' index: skipped");
        return;
    }
    let dir = std::env::temp_dir().join(format!("ddi-long-note-skin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hold.svg"), SVG).unwrap();
    std::fs::write(dir.join("fill.wgsl"), WGSL).unwrap();
    std::fs::write(dir.join("NoteSkin.lua"), LUA).unwrap();
    std::fs::write(
        dir.join("metrics.ini"),
        "[Global]\nFallbackNoteSkin=Peters-DDR\n",
    )
    .unwrap();
    let custom = ddi_desktop::library::noteskins::DiskSkin {
        game: "dance".into(),
        name: "Long Notes".into(),
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
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ready.load_error, None);
    assert!(
        !ready
            .play
            .notes
            .iter()
            .any(|n| n.contains("long note") || n.contains("wgsl") || n.contains("not asked")),
        "{:?}",
        ready.play.notes
    );
    let instance = ddi_desktop::gpu::instance();
    let Some(adapter) = ddi_desktop::gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: drawing skipped");
        return;
    };
    for reverse in [false, true] {
        let geo = FieldGeometry {
            arrow: 64.0,
            left: 0.0,
            receptor_y: if reverse { H as f32 - 64.0 } else { 64.0 },
            direction: if reverse { -1.0 } else { 1.0 },
            width: W as f32,
            height: H as f32,
        };
        // A hold four arrows long.
        let hold = SpriteKind::HoldHead {
            tail_y: 5.0,
            active: false,
            dropped: false,
        };
        let (drawn, _) = draw(&adapter, &mut ready, &layout, &geo, hold);
        let along = if reverse { -1.0 } else { 1.0 };
        let (x, head) = (geo.lane_x(1.0), geo.lane_y(1, 1.0));
        let pixel = |dx: f32, dy: f32| {
            let (px, py) = ((x + dx) as u32, (head + dy * along) as u32);
            let i = ((py * W + px) * 4) as usize;
            [drawn[i], drawn[i + 1], drawn[i + 2]]
        };
        let ctx = if reverse { "reverse" } else { "forward" };
        // The band painted by the material along the note: cyan near the
        // head, magenta past 100 units, to the cap.
        assert_eq!(pixel(0.0, 50.0), [0, 255, 255], "{ctx}: near the head");
        assert_eq!(pixel(0.0, 150.0), [255, 0, 255], "{ctx}: further on");
        assert_eq!(pixel(0.0, 256.0), [255, 0, 255], "{ctx}: the cap's band");
        assert_eq!(pixel(0.0, 256.0 + 12.0), [0, 0, 0], "{ctx}: past the cap");
        // The shadow outside the envelope (14 to 16 units across).
        assert_eq!(pixel(15.0, 60.0), [64, 64, 64], "{ctx}: the shadow");
        assert_eq!(
            pixel(13.0, 60.0),
            [0, 0, 0],
            "{ctx}: in the envelope, off the band"
        );
        // The head: its ring painted, its hole protected (the body's band
        // taken out under it), its mark on top.
        assert_eq!(pixel(15.0, 0.0), [0, 255, 255], "{ctx}: the head's ring");
        assert_eq!(pixel(6.0, 0.0), [0, 0, 0], "{ctx}: the head's hole");
        assert_eq!(pixel(0.0, 0.0), [255, 255, 255], "{ctx}: the head's mark");

        // A roll: its pulse at half (no shadow here).
        let roll = SpriteKind::RollHead {
            tail_y: 5.0,
            active: false,
            dropped: false,
        };
        let (drawn, _) = draw(&adapter, &mut ready, &layout, &geo, roll);
        let i = (((head + 60.0 * along) as u32 * W + (x + 13.0) as u32) * 4) as usize;
        let pulse = [drawn[i], drawn[i + 1], drawn[i + 2]];
        assert!(
            pulse[0].abs_diff(128) <= 2 && pulse[1] == 0,
            "{ctx}: the roll's pulse: {pulse:?}"
        );
        // Over the band too (the head's hole alone protected): red at half
        // over the cyan fill.
        let i = (((head + 60.0 * along) as u32 * W + x as u32) * 4) as usize;
        let over = [drawn[i], drawn[i + 1], drawn[i + 2]];
        assert!(
            over.iter().all(|c| c.abs_diff(128) <= 2),
            "{ctx}: the pulse over the band: {over:?}"
        );

        // A lift: its head, and its trail leading it up the lane, painted
        // by the material and faded toward the lead's far end, its shadow
        // beside.
        let (drawn, frame) = draw(&adapter, &mut ready, &layout, &geo, SpriteKind::Lift);
        let pixel = |dx: f32, dy: f32| {
            let (px, py) = ((x + dx) as u32, (head + dy * along) as u32);
            let i = ((py * W + px) * 4) as usize;
            [drawn[i], drawn[i + 1], drawn[i + 2]]
        };
        let m = frame.note_motion;
        let lead = (m.arrows_per_second.abs() * 0.5_f32.max(m.seconds_per_beat * 0.25) * 64.0)
            .clamp(1.0, 256.0);
        assert!(lead > 40.0, "{ctx}: a lead to see: {lead}");
        assert_eq!(pixel(0.0, 0.0), [255, 255, 255], "{ctx}: the lift's head");
        let ahead = lead * 0.25;
        // Cyan at 3/4 over the shadow (gray 64 at 3/4) beneath it.
        let under = 64.0 * 0.75;
        let expect = |c: f32| (c * 0.75 + under * 0.25).round() as u8;
        let trail = pixel(0.0, -ahead);
        assert!(
            trail[0].abs_diff(expect(0.0)) <= 4 && trail[1].abs_diff(expect(255.0)) <= 4,
            "{ctx}: the trail, cyan faded to 3/4: {trail:?}"
        );
        let shadow = pixel(12.0, -ahead);
        assert!(
            shadow[0].abs_diff((64.0 * 0.75) as u8) <= 3,
            "{ctx}: its shadow: {shadow:?}"
        );
        assert_eq!(
            pixel(0.0, -lead - 8.0),
            [0, 0, 0],
            "{ctx}: nothing past the lead"
        );
        // The head's protect and outside parts keep the trail's fill and
        // its shadow off the top of the head as it is drawn, upright in
        // either direction (the trail reaches there both ways: its lead
        // forward, its near end in reverse); under it, they go on.
        let screen = |dx: f32, dy: f32| {
            let i = (((head + dy) as u32 * W + (x + dx) as u32) * 4) as usize;
            [drawn[i], drawn[i + 1], drawn[i + 2]]
        };
        let gray = |c: [u8; 3]| c[0] > 0 && c[0] == c[1] && c[1] == c[2];
        let cyan = |c: [u8; 3]| c[1] > c[0] + 64 && c[1] == c[2];
        let protected = screen(5.0, -10.0);
        assert!(
            gray(protected),
            "{ctx}: protected, the shadow alone: {protected:?}"
        );
        assert_eq!(screen(-13.0, -10.0), [0, 0, 0], "{ctx}: no shadow outside");
        let under = screen(5.0, 10.0);
        assert!(cyan(under), "{ctx}: the trail under the head: {under:?}");
        let beside = screen(-13.0, 10.0);
        assert!(gray(beside), "{ctx}: its shadow: {beside:?}");
    }
}
