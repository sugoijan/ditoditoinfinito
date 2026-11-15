//! A watched stage of 73 (`grid-all-v1`, the feedback round I), as the
//! desktop window runs it, on this machine's GPU offscreen and a fake
//! clock with no sound device: a practice room of 73 CPUs (the cap) with
//! the player watching. The grid shows every field at once, each in its
//! place from frame to frame however the standings move (the flicker's
//! fix, `grid-pages-v1`); one competitor followed shows the 72 others
//! beside its field as a player's view shows its opponents.
//!
//! Skipped where there is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<dir>`
//! keeps pictures (PPM).

mod common;
use std::rc::Rc;

use ddi_desktop::gpu;
use ddi_desktop::play::host::Host;
use ddi_room::CpuMode;
use ddi_ui::screens::data::Difficulty;
use common::ui::top;

const CPUS: u16 = 73;

fn watched(size: [u32; 2], name: &str) {
    common::indexes();
    let Some(adapter) = common::gpu::adapter_or_skip() else {
        return;
    };
    let (ui, env) = common::ui::theme("start", common::ui::lowercase(), size, true);
    let host = Host::new(Rc::default(), None);
    let mut rig = common::rig::Rig::new(&adapter, size, ui, env, host).with_ui_gpu(&adapter);
    // 73 CPUs and the watcher's own seat: one past a practice's cap of
    // 73 seats, so the development-only practice (`room::develop`); the
    // 73 competitors are an audience's room of 73.
    ddi_desktop::room::develop(CPUS + 1);
    let room = common::practice::open(&mut rig.ui, &rig.env, &rig.host);
    let entry =
        common::practice::song_entry("beatucada", "Beatucada", 0, Difficulty::Challenge);
    {
        let mut r = room.borrow_mut();
        let mut rules = ddi_room::MatchRules {
            stages: 1,
            ..r.rules().clone()
        };
        rules.seats = rules.seats.max(CPUS + 1);
        rules.cpu.mode = CpuMode::Fixed { count: CPUS };
        rules.cpu.opponent.level = 7;
        rules.cpu.opponent.seed = 7;
        r.save_rules(rules).unwrap();
    }
    // Watch only: the player is no competitor, the 73 CPUs are.
    ddi_ui::screens::room::toggle_intent(&room);
    rig.ui.update(0.1);
    assert!(room.borrow().watching());
    room.borrow_mut()
        .pick(&entry, &entry.charts[0])
        .unwrap();
    rig.ui.update(0.1);
    rig.wait_prompt();
    rig.ui.update(0.5);
    assert_eq!(top(&rig.ui), "play-prompt");
    rig.countdown(&room);
    assert!(rig.host.playing());
    let step = 1_000_000_000 / 30;
    // Into the song, the grid's places watched frame by frame.
    let mut last: Option<Vec<usize>> = None;
    for i in 0..(30 * 12) {
        rig.frame(step);
        if i % 16 == 0 {
            rig.wait_gpu();
        }
        if let Some(w) = rig.host.watch() {
            let seats: Vec<usize> = w.fields.iter().map(|f| f.0).collect();
            if let Some(l) = &last
                && l.len() == seats.len()
            {
                assert_eq!(l, &seats, "the grid's places held");
            }
            last = Some(seats);
        }
    }
    let w = rig.host.watch().expect("a watched stage");
    assert_eq!((w.view, w.focus), ("grid", None));
    eprintln!("{name}: the grid of {} at lanes of {}", w.fields.len(), w.lane);
    if size == [1280, 720] {
        assert_eq!(w.fields.len(), 73, "everyone at once");
        assert_eq!(w.page, None);
        assert!(w.lane >= ddi_shell::opponents::MINI_LANE_ALL);
    }
    assert_eq!(w.trace.seat_changes, 0, "{:?}", w.trace);
    let drawn = rig.host.miniatures_drawn();
    assert_eq!(drawn, w.fields.len(), "a frame for every field");
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        rig.frame(step);
    }
    rig.wait_gpu();
    common::shot(&format!("{name}-grid.ppm"), &gpu::read_rgba(&rig.device, &rig.queue, &rig.target.texture), size);

    // One followed: the 72 others beside its field, as a player's view.
    rig.press("Tab");
    for _ in 0..4 {
        rig.frame(step);
    }
    let w = rig.host.watch().unwrap();
    assert_eq!((w.view, w.focus), ("focus", Some(0)));
    eprintln!("{name}: followed, {} fields at lanes of {}", w.fields.len(), w.lane);
    if size == [1280, 720] {
        assert_eq!(w.fields.len(), 73, "72 beside, the large one last");
        assert_eq!(w.fields[72].0, 0);
    }
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        rig.frame(step);
    }
    rig.wait_gpu();
    common::shot(&format!("{name}-focus.ppm"), &gpu::read_rgba(&rig.device, &rig.queue, &rig.target.texture), size);
}

#[test]
fn seventy_three_watched_at_1280x720_show_everyone() {
    watched([1280, 720], "watch-73-1280x720");
}

#[test]
fn seventy_three_watched_on_a_phone() {
    watched([390, 844], "watch-73-390x844");
}
