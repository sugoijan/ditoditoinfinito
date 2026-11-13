//! A stage's Standings with nine starters who all finished on another
//! chart than the picked one (a match's Adaptive charts), drawn offscreen
//! on this machine's GPU in the game's theme at 1280×720 and 844×390:
//! each name keeps its room and the mark of the chart they finished on
//! ("↑ Challenge") goes on the line under it where the line is tall
//! enough, else after it, giving way down to its arrow. Skipped where
//! there is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<dir>` keeps the
//! pictures (`standings-stepped-<w>x<h>-<lang>.ppm`).

mod room_fake;

use ddi_desktop::gpu::{self, Gpu};
use ddi_room::{Finish, Finisher, OpponentSpec, ParticipantId, RunDetail};
use ddi_ui::Ui;
use ddi_ui::event::UiEvent;
use ddi_ui::screens::data::{ChartEntry, Difficulty, ResultsKind, SongEntry};
use ddi_ui::screens::results::{ResultsActions, ResultsScreen};
use ddi_ui::screens::{Env, sample};
use ddi_ui::state::room::{RoomClient, StageOrder};
use room_fake::{drawn, keep};

fn song() -> SongEntry {
    SongEntry {
        id: "single".into(),
        title: "Single Song".into(),
        artist: "Artist".into(),
        bpm_text: "150".into(),
        length: 120.0,
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Hard,
            meter: 9,
            notes: 300,
            digest: Some(ddi_chart::ChartDigest(0xabc0)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    }
}

/// A practice room of two stages against eight named CPUs, the first
/// played and reported: each starter a little worse than the one
/// before, everyone on the harder chart at the end.
fn stage() -> (ddi_ui::state::room::SharedRoom, StageOrder) {
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let mut rules = room.borrow().rules().clone();
    rules.set_attacks(false);
    rules.stages = 2;
    rules.seats = rules.seats.max(9);
    let first = rules.cpu.roster[0].clone();
    let names = [
        "Velvet Metronome",
        "Saltglass Rook",
        "Mossy Accordion",
        "Hollow Cadence",
        "Juniper Drift",
        "Quill Sprocket",
        "Plinth Sparrow",
        "Zephyr Abacus",
    ];
    rules.cpu.roster = names
        .iter()
        .enumerate()
        .map(|(n, name)| OpponentSpec {
            name: Some((*name).into()),
            seed: 100 + n as u64,
            level: (9 - n) as u8,
            ..first.clone()
        })
        .collect();
    room.borrow_mut().save_rules(rules).unwrap();
    let order = {
        let mut r = room.borrow_mut();
        let s = song();
        r.pick(&s, &s.charts[0]).unwrap();
        for job in r.take_load_jobs() {
            r.finish_load(&job, Ok(()));
        }
        r.set_ready(true).unwrap();
        let now = r.now_ms() + 3_000;
        r.tick(now);
        r.take_stage_order().expect("the stage's order")
    };
    let id = |i: usize| match i {
        0 => order.me,
        i => order.cpus[i - 1].participant,
    };
    let detail = |i: usize| RunDetail {
        percent: Some(9_900 - 250 * i as i32),
        grade: ["AAA", "AA", "A", "B"][i.min(3)].into(),
        life_ppm: 900_000,
        max_combo: 280 - 20 * i as u32,
        tiers: vec![200 - 10 * i as u32, 60, 10 + 5 * i as u32, 5, 2, 1, 2 + i as u32],
        chart: Some("Challenge".into()),
        harder: true,
        ..RunDetail::default()
    };
    let ranking: Vec<Vec<Finisher>> = (0..9)
        .map(|i| {
            vec![Finisher {
                participant: id(i),
                finish: Finish::Survived,
            }]
        })
        .collect();
    let details: Vec<(ParticipantId, RunDetail)> = (0..9).map(|i| (id(i), detail(i))).collect();
    room.borrow_mut()
        .report_played(order.stage, ddi_room::Run::Forfeit, ranking, false, details)
        .unwrap();
    (room, order)
}

#[test]
fn the_standings_give_each_name_its_room_beside_the_mark_of_a_step() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    for (size, lang) in [([1280u32, 720u32], "en"), ([844, 390], "en"), ([1280, 720], "ja")] {
        assert!(ddi_l10n::set_language(lang));
        let css = [size[0] as f32, size[1] as f32];
        let (room, order) = stage();
        let look = std::rc::Rc::new(ddi_theme::Infinito::new());
        let env = Env::new(look.clone(), std::rc::Rc::new(|s: &str| s.to_string()));
        env.reduced_motion.set(true);
        let mut data = sample::results();
        data.kind = ResultsKind::Stage;
        let design = ddi_ui::screens::design_for(&*look, css);
        let mut ui = Ui::new(design, ddi_ui::TextMode::Own, false);
        ui.set_focus_ring(false);
        ui.push(
            ResultsScreen::new(env.clone(), data, ResultsActions::default())
                .with_room(room.clone(), order.stage),
        );
        ui.handle(UiEvent::Resize {
            size: css,
            ratio: 1.0,
        });
        for _ in 0..90 {
            ui.update(1.0 / 60.0);
        }
        let view = env.state.borrow().stage.clone().expect("the standings");
        assert_eq!(view.rows.len(), 9);
        assert!(view.rows.iter().all(|r| r.stepped.as_deref().is_some()));
        let pixels = gpu.draw_offscreen(&mut ui, size, format);
        keep("standings-stepped", size, lang, &pixels);
        // Something drawn where the table is.
        assert!(drawn(&pixels, size, [0.4, 0.25, 0.95, 0.6]), "{size:?}");
    }
    assert!(ddi_l10n::set_language("en"));
}
