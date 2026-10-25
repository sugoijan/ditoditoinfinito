//! Capture Peters-DDR Vivid through the actual loader and textured renderer.
//! No window/audio. `cargo run -p ddi-desktop --no-default-features --example
//! noteskin_reference -- OUTPUT.png` (scratch DDI_HOME, GPU required).
#[path = "common/reference.rs"]
mod reference;

use ddi_chart::{Layout, Quantization};
use ddi_engine::frame::{NoteSprite, SpriteKind};
use ddi_noteskin::lua::Env;
use ddi_render::{FieldGeometry, noteskin::NoteLayer};

fn main() -> reference::Result<()> {
    let output = std::env::args().nth(1).ok_or("OUTPUT.png required")?;
    let mut gpu = reference::Gpu::new()?;
    let layout = Layout::dance_single();
    let mut ready = ddi_desktop::play::skins::prepare_authoring(
        "bundled:dance/peters-ddr-vivid",
        &layout,
        Env::default(),
        &ddi_desktop::library::noteskins::bundled(),
    )
    .ok_or("skin load failed")?;
    if let Some(e) = ready.load_error {
        return Err(e.into());
    }
    gpu.upload(&mut ready);
    let (w, h) = (2048u32, 1024u32);
    let mut quads = Vec::new();
    let mut demo = ddi_engine::demo::Demo::new("dance-single", 120.0, 1.0).ok_or("demo")?;
    let mut frame = demo.step(0.0).frame;
    frame.notes = vec![NoteSprite {
        note_index: 0,
        lane: 1,
        y: 0.,
        kind: SpriteKind::Tap,
        quantization: Quantization::N4,
        beat_frac: 0.,
        color: None,
        alpha: 1.,
        glow: 0.,
        beat: 0.,
        hold_life: 1.,
    }];
    for index in 0..128 {
        frame.beat = index as f64 / 32.;
        frame.song_time = frame.beat / 2.;
        ready.play.update(&frame, std::iter::empty(), false);
        let geo = FieldGeometry {
            arrow: 128.,
            left: (index % 16) as f32 * 128. - 128.,
            receptor_y: (index / 16) as f32 * 128. + 64.,
            direction: 1.,
            width: w as f32,
            height: h as f32,
        };
        let mut receptor = Vec::new();
        ready.play.receptors(&frame, &layout, &geo, &mut receptor);
        ready.play.notes(&frame, &layout, &geo, &mut quads);
    }
    gpu.capture(
        &quads,
        [w, h],
        wgpu::Color::TRANSPARENT,
        Some(false),
        &output,
    )?;
    eprintln!("captured 128 frames / four beats / original model + textures");
    Ok(())
}
