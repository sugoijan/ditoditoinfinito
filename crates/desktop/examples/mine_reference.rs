//! Inspect Infinito mines: full row, adjacent pair, gap and isolated shapes.
//! No window/audio. `cargo run -p ddi-desktop --no-default-features --example
//! mine_reference -- OUTPUT.png` (scratch DDI_HOME, GPU required).
//! `DDI_REFERENCE_TAPS=1` compares aligned taps with receptors instead.
//! `DDI_REFERENCE_EFFECTS=1` draws W1/W2/W3/W4/mine/held/holding rows,
//! at `DDI_EFFECT_AGE` seconds; variation is off unless DDI_REFERENCE_VARIATION.
//! `DDI_REFERENCE_EFFECT_CASE=0..6` isolates one row's effect on the Up lane.
#[path = "common/reference.rs"]
mod reference;

use ddi_chart::Quantization;
use ddi_engine::frame::{NoteSprite, SpriteKind};
use ddi_noteskin::lua::Env;
use ddi_render::noteskin::NoteLayer;
use reference::{env, flag};

fn main() -> reference::Result<()> {
    let output = std::env::args().nth(1).ok_or("OUTPUT.png required")?;
    let mut gpu = reference::Gpu::new()?;
    let layout = reference::ten_lanes();
    let effects = flag("DDI_REFERENCE_EFFECTS");
    let skin_env = if effects && !flag("DDI_REFERENCE_VARIATION") {
        Env {
            params: [(
                "HitVariation".into(),
                ddi_noteskin::params::Value::Bool(false),
            )]
            .into(),
            ..Env::default()
        }
    } else {
        Env::default()
    };
    let mut ready = reference::infinito(&layout, skin_env)?;
    gpu.upload(&mut ready);
    eprintln!(
        "mine textures: {:?}",
        ready
            .play
            .texture_keys()
            .iter()
            .filter(|k| format!("{k:?}").contains("_spiral mine"))
            .collect::<Vec<_>>()
    );
    let size = [1280, 768];
    let mut quads = Vec::new();
    let mut effect_params = Vec::new();
    let mut frame = reference::frame()?;
    let taps = flag("DDI_REFERENCE_TAPS");
    let lifts = flag("DDI_REFERENCE_LIFTS");
    frame.notes = [
        (0.0_f32, 0.0_f32, (0..10).collect::<Vec<_>>()),
        (1.5, 4.0, vec![1, 2, 5, 7]),
        (3.0, 8.0, vec![4, 9]),
    ]
    .into_iter()
    .flat_map(|(y, beat, lanes)| {
        lanes.into_iter().map(move |lane| NoteSprite {
            note_index: (beat as usize) * 10 + lane,
            lane: lane as u8,
            y: y + if taps { 1.5 } else { 0.0 },
            kind: if lifts {
                SpriteKind::Lift
            } else if taps {
                SpriteKind::Tap
            } else {
                SpriteKind::Mine
            },
            quantization: Quantization::N4,
            beat_frac: 0.0,
            color: None,
            alpha: 1.0,
            glow: 0.0,
            beat,
            hold_life: 1.0,
        })
    })
    .collect();
    let target_time = frame.song_time;
    for step in 0..(target_time * 60.0).floor() as u32 {
        frame.song_time = step as f64 / 60.0;
        frame.beat = frame.song_time * 2.0;
        ready.play.update(&frame, std::iter::empty(), false);
    }
    frame.song_time = target_time;
    frame.beat = target_time * 2.0;
    ready.play.update(&frame, std::iter::empty(), false);
    let mut geo = reference::geometry(size);
    if effects {
        use ddi_engine::frame::{HoldKind, ReceptorState, SkinEvent, SkinEventKind};
        use ddi_engine::rules::Judgement;
        let age = env("DDI_EFFECT_AGE", 0.04_f64);
        let isolated = std::env::var("DDI_REFERENCE_EFFECT_CASE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok());
        frame.notes.clear();
        frame.lanes = 10;
        let cases = [
            Some(SkinEventKind::Tap {
                judgement: Judgement::W1,
                bright: false,
            }),
            Some(SkinEventKind::Tap {
                judgement: Judgement::W2,
                bright: false,
            }),
            Some(SkinEventKind::Tap {
                judgement: Judgement::W3,
                bright: false,
            }),
            Some(SkinEventKind::Tap {
                judgement: Judgement::W4,
                bright: false,
            }),
            Some(SkinEventKind::HitMine),
            Some(SkinEventKind::Held { bright: false }),
            None,
        ];
        for (row, kind) in cases.into_iter().enumerate() {
            if isolated.is_some_and(|selected| selected != row) {
                continue;
            }
            let start = row as f64 + 1.0;
            frame.song_time = start;
            frame.beat = start * 2.0;
            frame.receptors = vec![
                ReceptorState {
                    pressed: false,
                    flash: None,
                    holding: None
                };
                10
            ];
            ready.play.update(&frame, std::iter::empty(), false);
            if kind.is_none() {
                for (lane, r) in frame.receptors.iter_mut().enumerate() {
                    if isolated.is_none() || lane == 5 {
                        r.holding = Some(HoldKind::Hold);
                    }
                }
            }
            let events: Vec<_> = kind
                .into_iter()
                .flat_map(|kind| {
                    (0..10)
                        .filter(move |&lane| isolated.is_none() || lane == 5)
                        .map(move |lane| SkinEvent {
                            seq: 100 + (row * 10 + lane) as u64,
                            lane: lane as u8,
                            kind,
                            song_time: start,
                            autoplay: false,
                            danoni_judgments: false,
                            note: None,
                        })
                })
                .collect();
            ready.play.update(&frame, events.iter(), false);
            let elapsed = if kind.is_none() { age + 0.1 } else { age };
            for i in 1..=(elapsed * 600.0).ceil() as u32 {
                frame.song_time = start + (i as f64 / 600.0).min(elapsed);
                frame.beat = frame.song_time * 2.0;
                ready.play.update(&frame, std::iter::empty(), false);
            }
            geo.receptor_y = if isolated.is_some() {
                size[1] as f32 / 2.0
            } else {
                64.0 + row as f32 * 96.0
            };
            let from = quads.len();
            ready.play.explosions(&frame, &layout, &geo, &mut quads);
            let base = effect_params.len() as u16;
            effect_params.extend_from_slice(ready.play.materials());
            for q in &mut quads[from..] {
                if let Some(m) = &mut q.material {
                    m.slot += base;
                }
            }
        }
    } else {
        if taps {
            ready.play.receptors(&frame, &layout, &geo, &mut quads);
        }
        ready.play.notes(&frame, &layout, &geo, &mut quads);
    }
    eprintln!(
        "{} {} quads",
        quads.len(),
        if effects { "effect" } else { "note" }
    );
    let params = if effects {
        effect_params
    } else {
        ready.play.materials().to_vec()
    };
    gpu.materials(&ready, &mut quads, &params, &frame);
    gpu.capture(&quads, size, reference::GROUND, None, &output)?;
    eprintln!(
        "captured Spiral {}",
        if effects {
            "judgment effects"
        } else {
            "mine bases, local arcs and adjacent-lane links"
        }
    );
    Ok(())
}
