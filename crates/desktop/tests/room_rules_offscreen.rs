//! A running practice match's rules changed from its next stage on
//! (2026-11-13), drawn offscreen on this machine's GPU as the desktop
//! window draws it: the Rules pop-up mid-match with its destination row
//! ("Changes apply: From stage 2"), a row changed and marked; then, the
//! stages played (their results reported as a play reports them), the
//! match's standings marking the stage played under other rules.
//! Skipped where there is no GPU adapter (CI). `DDI_ROOM_SHOTS=<dir>`
//! keeps the pictures (PPM).

use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_room::{Finish, Finisher, MatchDifficulty};
use ddi_ui::UiEvent;
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::screens::room_dialogs::{RulesDialog, StandingsDialog};

const SIZE: [u32; 2] = [1280, 720];

fn ppm(name: &str, pixels: &[u8]) {
    let Some(dir) = std::env::var_os("DDI_ROOM_SHOTS") else {
        return;
    };
    let mut out = format!("P6\n{} {}\n255\n", SIZE[0], SIZE[1]).into_bytes();
    out.extend(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]]),
    );
    std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), out).unwrap();
}

fn frames(ui: &mut ddi_ui::Ui, n: usize) {
    for _ in 0..n {
        ui.update(1.0 / 60.0);
    }
}

#[test]
fn the_rules_pop_up_mid_match_and_the_marked_standings_are_drawn() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("room", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    frames(&mut ui, 30);
    let room = ddi_desktop::room::current();
    room.borrow_mut().set_rules_seen(true);
    {
        let mut r = room.borrow_mut();
        let rules = ddi_room::MatchRules {
            stages: 3,
            ..r.rules().clone()
        };
        r.save_rules(rules).unwrap();
    }
    let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
    let chart = 4;
    let entry = SongEntry {
        id: "beatucada".into(),
        title: "Beatucada".into(),
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Medium,
            index: chart,
            digest: Some(song.chart_digest(chart)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    };
    // Stage 1 picked: its window open, a change lands on stage 2.
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    frames(&mut ui, 10);
    let stage_of = |n: usize| room.borrow().state().current.as_ref().unwrap().stages[n].id;
    let stage1 = stage_of(0);
    assert_eq!(room.borrow().rules_land_on().map(|(_, n)| n), Some(2));
    ui.push_over(RulesDialog::new(env.clone(), room.clone()));
    frames(&mut ui, 30);
    let v = env.state.borrow().room.clone().unwrap();
    assert_eq!(v.rules_destination.as_deref(), Some("stage"));
    assert_eq!(v.rules_from, Some(2));
    let picture = gpu.draw_offscreen(&mut ui, SIZE, format);
    ppm("rules-from-stage", &picture);
    // Hard timing from stage 2, applied as the pop-up applies it.
    let mut hard = room.borrow().rules_ahead().unwrap();
    hard.difficulty = MatchDifficulty::Hard;
    let (from, _) = room.borrow().rules_land_on().unwrap();
    room.borrow_mut().change_stage_rules(from, &hard).unwrap();
    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::down("Escape", "Escape")));
    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::up("Escape", "Escape")));
    frames(&mut ui, 30);
    assert_eq!(ui.layers().last().unwrap().name, "select");
    let status = env.state.borrow().room.clone().unwrap().status;
    assert!(status.starts_with("From stage 2: Hard timing. "), "{status}");
    ppm("rules-status", &gpu.draw_offscreen(&mut ui, SIZE, format));
    // The stages played: each finished as a play reports it.
    let finish = |stage: ddi_room::StageId| {
        let jobs = room.borrow_mut().take_load_jobs();
        for job in jobs {
            room.borrow_mut().finish_load(&job, Ok(()));
        }
        let _ = room.borrow_mut().set_ready(true);
        room.borrow_mut().clear_refusal();
        // Past the readiness window and the countdown.
        for _ in 0..3 {
            let later = room.borrow().now_ms() + 40_000;
            room.borrow_mut().tick(later);
        }
        let lineup: Vec<_> = room
            .borrow()
            .state()
            .stage(stage)
            .and_then(|s| s.play.as_ref())
            .expect("started")
            .lineup
            .iter()
            .copied()
            .collect();
        let ranking = lineup
            .into_iter()
            .map(|participant| {
                vec![Finisher {
                    participant,
                    finish: Finish::Survived,
                }]
            })
            .collect();
        room.borrow_mut().report_stage(stage, ranking, false).unwrap();
    };
    finish(stage1);
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    let stage2 = stage_of(1);
    assert_eq!(
        room.borrow().rules_for_stage(stage2).unwrap().difficulty,
        MatchDifficulty::Hard
    );
    finish(stage2);
    room.borrow_mut().results_shown();
    ui.push_over(StandingsDialog::new(env.clone(), room.clone(), Some(2)));
    frames(&mut ui, 30);
    assert_eq!(ui.layers().last().unwrap().name, "room-standings");
    assert!(ui.actor("S1.Changed").is_some() && ui.actor("S0.Changed").is_none());
    let said = |name: &str| match &ui.actor(name).expect(name).kind {
        ddi_actor::Kind::Text(t) => t.text().to_string(),
        _ => panic!("{name} is not text"),
    };
    assert!(
        (0..4).any(|k| ui.actor(&format!("Line{k}")).is_some()
            && said(&format!("Line{k}")) == "● From stage 2: DDI · Hard timing"),
        "the stage's line under the title"
    );
    ppm("standings-marked", &gpu.draw_offscreen(&mut ui, SIZE, format));
}
