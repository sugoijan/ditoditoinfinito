//! One competitor followed in a watched stage (the feedback round K), as
//! the desktop window runs it, on this machine's GPU offscreen and a fake
//! clock with no sound device: a practice room watched, one CPU's field
//! in the play's field's place.
//!
//! - The followed field is drawn once, at its own time: the stage's song
//!   second less its view delay (a network's stream is drawn behind the
//!   song; a practice's fields are given a delay here as a test's hook).
//!   Before the round it was drawn twice, the second copy the delay
//!   behind the first: the "ghost notes".
//! - Under Charts: Adaptive a followed competitor's field shows one
//!   chart, the one they play: the old chart's notes end where the new
//!   chart's begin.
//! - The name plate on the followed field: who they are, their place.
//!
//! Skipped where there is no GPU adapter (CI). `DDI_OFFSCREEN_PPM=<dir>`
//! keeps pictures.

mod common;
use std::rc::Rc;

use common::ui::top;
use ddi_desktop::play::host::Host;
use ddi_platform::{DeviceId, HostTime};
use ddi_ui::screens::data::Difficulty;
use ddi_ui::state::room::SharedRoom;

const STEP: u64 = 1_000_000_000 / 30;

/// The CPUs' names, the same on every run (a practice's roster is drawn).
const NAMES: [&str; 3] = ["Quill Sprocket", "Mossy Accordion", "Hollow Cadence"];

/// A practice room's stage on Beatucada's chart `chart` against three
/// CPUs at `levels` (from 0), the same on every run, started: watched
/// (the player chose Watch) where `watch`, else played by a player who
/// presses nothing. `None` without a GPU adapter.
fn stage(
    size: [u32; 2],
    chart: usize,
    difficulty: Difficulty,
    levels: [u8; 3],
    watch: bool,
) -> Option<(common::rig::Rig, SharedRoom)> {
    common::indexes();
    let adapter = common::gpu::adapter_or_skip()?;
    let (ui, env) = common::ui::theme("start", common::ui::lowercase(), size, true);
    let host = Host::new(Rc::default(), None);
    let mut rig = common::rig::Rig::new(&adapter, size, ui, env, host).with_ui_gpu(&adapter);
    ddi_desktop::room::discard();
    let room = common::practice::open(&mut rig.ui, &rig.env, &rig.host);
    let entry = common::practice::song_entry("beatucada", "Beatucada", chart, difficulty);
    {
        let mut r = room.borrow_mut();
        let mut rules = ddi_room::MatchRules {
            stages: 1,
            charts: ddi_room::Charts::Adaptive,
            ..r.rules().clone()
        };
        rules.cpu.mode = ddi_room::CpuMode::Roster;
        rules.cpu.roster = levels
            .iter()
            .zip(NAMES)
            .enumerate()
            .map(|(i, (&level, name))| ddi_room::OpponentSpec {
                level,
                seed: 7 + i as u64,
                personality: None,
                name: Some(name.into()),
                targeting: None,
            })
            .collect();
        rules.cpu.opponent.seed = 7;
        r.save_rules(rules).unwrap();
    }
    if watch {
        ddi_ui::screens::room::toggle_intent(&room);
        rig.ui.update(0.1);
        assert!(room.borrow().watching());
    }
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    rig.ui.update(0.1);
    rig.wait_prompt();
    rig.ui.update(0.5);
    assert_eq!(top(&rig.ui), "play-prompt");
    if watch {
        rig.countdown(&room);
    } else {
        rig.ready_and_start(&room);
    }
    assert!(rig.host.playing());
    // (The fields' fonts are read on a thread: real time for them.)
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        rig.frame(STEP);
    }
    Some((rig, room))
}

/// The frame drawn last kept as `name` (a PNG).
fn shot(rig: &common::rig::Rig, name: &str, size: [u32; 2]) {
    rig.wait_gpu();
    common::shot(&format!("{name}.png"), &rig.read(), size);
}

/// Frames until the stage's song second is `t`.
fn until(rig: &mut common::rig::Rig, t: f64) {
    let mut frames = 0;
    while rig.host.watch().is_none_or(|w| w.song < t) {
        rig.frame(STEP);
        if frames % 16 == 0 {
            rig.wait_gpu();
        }
        frames += 1;
        assert!(frames < 30 * 200 && rig.host.playing(), "never at {t}");
    }
}

/// The seat followed by a press on its field.
fn follow(rig: &mut common::rig::Rig, seat: usize) {
    rig.edge(DeviceId::Touch, &format!("watch:{seat}"), true);
    rig.frame(STEP);
    rig.frame(STEP);
    assert_eq!(rig.host.watch().unwrap().focus, Some(seat));
}

/// The followed field at 1280×720 and on a phone, its stream a third of
/// a second behind the song: drawn at that time, once.
fn followed_behind(size: [u32; 2], lang: &str) {
    assert!(ddi_l10n::force(lang));
    let Some((mut rig, _room)) = stage(size, 4, Difficulty::Medium, [2, 4, 7], true) else {
        ddi_l10n::unforce();
        return;
    };
    let name = format!("follow-{}x{}-{lang}", size[0], size[1]);
    until(&mut rig, 20.0);
    follow(&mut rig, 0);
    for _ in 0..6 {
        rig.frame(STEP);
    }
    let w = rig.host.watch().unwrap();
    assert_eq!(w.view, "focus");
    // How to follow another, after the label: nothing after a tap (the
    // seat was followed by one: a field is tapped), Tab's cap once a key
    // was pressed (round M).
    assert_eq!(w.follow_key, None, "followed by a tap");
    assert_eq!(w.followed_at, Some(w.song), "a practice's fields are at the song's time");
    // The name plate on the followed field: who they are, their stars
    // (a CPU's), their place of the three; under their life gauge,
    // inside their field.
    let plate = w.plate.clone().expect("the followed competitor's plate");
    assert_eq!(&*plate.name, NAMES[0]);
    assert_eq!(plate.level, Some(3));
    let place = plate.place.clone().expect("their place");
    assert!(place.contains('3'), "of three: {place}");
    assert_eq!(plate.out, None);
    let drawn = rig.host.plate_drawn().expect("drawn");
    let field = w.fields.last().unwrap().1;
    assert!(
        drawn.plate[0] >= field[0] && drawn.end <= field[2] && drawn.plate[1] > 0.0,
        "{drawn:?} in {field:?}"
    );
    if size[0] >= 1280 {
        assert_eq!(drawn.name, NAMES[0], "whole");
        // (The Japanese place is wider: it gives way to the chart line.)
        assert!(drawn.stars && (drawn.words || lang != "en"), "{drawn:?}");
    }
    eprintln!("{name}: {plate:?} drawn {drawn:?}");
    // Another followed: the plate is theirs.
    follow(&mut rig, 2);
    let w = rig.host.watch().unwrap();
    assert_eq!(&*w.plate.as_ref().unwrap().name, NAMES[2]);
    follow(&mut rig, 0);
    shot(&rig, &name, size);
    // As a network's stream: a third of a second behind.
    const DELAY: f64 = 1.0 / 3.0;
    rig.host.delay_fields_for_tests(DELAY);
    for _ in 0..6 {
        rig.frame(STEP);
    }
    shot(&rig, &format!("{name}-behind"), size);
    let w = rig.host.watch().unwrap();
    let at = w.followed_at.expect("a field followed");
    assert!(
        (w.song - at - DELAY).abs() < 1e-6,
        "the followed field {} s behind the song, its delay {DELAY}",
        w.song - at
    );
    assert!((w.delay - DELAY).abs() < 1e-6, "{}", w.delay);
    // What is behind the fields stays on the stage's song time (round M).
    assert_eq!(w.scene_at, w.song, "the background on the stage's time");
    ddi_l10n::unforce();
}

#[test]
fn a_followed_field_behind_the_song_is_drawn_once_at_its_own_time() {
    followed_behind([1280, 720], "en");
}

#[test]
fn a_followed_field_on_a_phone() {
    followed_behind([390, 844], "en");
}

#[test]
fn a_followed_field_in_japanese() {
    followed_behind([1280, 720], "ja");
    followed_behind([390, 844], "ja");
}

/// Charts: Adaptive on Beatucada's Hard chart, the weak CPUs stepping
/// down: the first to step followed. Its field shows one chart at any
/// time: of two charts' notes in a frame, every one of the earlier chart
/// is nearer the receptors than every one of the later.
#[test]
fn a_followed_competitor_who_steps_shows_one_chart() {
    let size = [1280, 720];
    let Some((mut rig, _room)) = stage(size, 5, Difficulty::Hard, [0, 0, 1], true) else {
        return;
    };
    follow(&mut rig, 0);
    let mut step: Option<(f64, f64, u8)> = None;
    let mut shots = [false; 4];
    let mut frames = 0;
    let mut two = 0;
    while rig.host.playing() {
        let now = rig.frame(STEP);
        if frames % 16 == 0 {
            rig.wait_gpu();
        }
        frames += 1;
        assert!(frames < 30 * 200, "the play never ended");
        let Some(w) = rig.host.watch() else { break };
        let Some(p) = rig.host.running() else { break };
        let s = &p.session;
        let b = s.battle().expect("a stage");
        if step.is_none() {
            // The first competitor to step: followed from then on.
            let first = b
                .feeds()
                .enumerate()
                .find_map(|(seat, f)| f.steps().next().map(|st| (seat, st)));
            if let Some((seat, st)) = first {
                step = Some(st);
                if w.focus != Some(seat) {
                    follow(&mut rig, seat);
                }
            }
            continue;
        }
        let (at, boundary, rung) = step.unwrap();
        // One chart: the charts' notes never interleave.
        let frame = s.frame(HostTime(now));
        let chart = |i: usize| i >> 24;
        let notes: Vec<(usize, f32)> = frame
            .notes
            .iter()
            .filter(|n| n.note_index < ddi_engine::battle::HAZARD_INDEX_BASE)
            .map(|n| (chart(n.note_index), n.y))
            .collect();
        let charts: std::collections::BTreeSet<usize> = notes.iter().map(|n| n.0).collect();
        if charts.len() > 1 {
            two += 1;
            let old = notes.iter().filter(|n| n.0 == 0).map(|n| n.1).fold(f32::MIN, f32::max);
            let new = notes.iter().filter(|n| n.0 != 0).map(|n| n.1).fold(f32::MAX, f32::min);
            assert!(old < new, "the charts overlap at {}: {old} and {new}", w.song);
        }
        let t = w.song;
        let due = [
            t >= at + 0.1,
            t >= boundary - 0.5,
            t >= boundary + 0.3,
            t >= boundary + 2.5,
        ];
        let names = ["decided", "before-boundary", "after-boundary", "later"];
        for i in 0..4 {
            if due[i] && !shots[i] {
                shots[i] = true;
                eprintln!(
                    "step to rung {rung} at {at:.2}, boundary {boundary:.2}: {} at {t:.2}, {} charts in the frame",
                    names[i],
                    charts.len()
                );
                shot(&rig, &format!("follow-step-{}", names[i]), size);
            }
        }
        if shots[3] {
            break;
        }
    }
    assert!(shots[3], "nobody stepped: {step:?}");
    eprintln!("{two} frames with two charts' notes, never overlapping");
}

/// A player out of the stage who follows a competitor: the plate on the
/// large field says whom, and nobody followed (their own dimmed field
/// again), none.
#[test]
fn a_player_out_of_the_stage_sees_whom_they_follow() {
    let size = [1280, 720];
    let Some((mut rig, _room)) = stage(size, 5, Difficulty::Hard, [2, 4, 7], false) else {
        return;
    };
    // Nothing pressed: out of the stage soon.
    let mut frames = 0;
    while rig.host.watch().is_none() {
        rig.frame(STEP);
        if frames % 16 == 0 {
            rig.wait_gpu();
        }
        frames += 1;
        assert!(frames < 30 * 120 && rig.host.playing(), "never out");
    }
    let w = rig.host.watch().unwrap();
    assert_eq!((w.view, w.focus), ("out", None));
    assert_eq!(w.plate, None, "their own field: no plate");
    assert_eq!(rig.host.plate_drawn(), None);
    rig.press("Tab");
    for _ in 0..4 {
        rig.frame(STEP);
    }
    let w = rig.host.watch().unwrap();
    assert_eq!(w.focus, Some(0));
    let plate = w.plate.clone().expect("the followed competitor's plate");
    assert_eq!(&*plate.name, NAMES[0]);
    // Of four: the three CPUs and the player.
    assert!(plate.place.as_deref().is_some_and(|p| p.contains('4')), "{plate:?}");
    assert!(rig.host.plate_drawn().is_some());
    shot(&rig, "follow-out-1280x720-en", size);
    // Back to their own field (Shift+Tab from the first).
    rig.edge(DeviceId::Keyboard, "ShiftLeft", true);
    rig.press("Tab");
    rig.edge(DeviceId::Keyboard, "ShiftLeft", false);
    for _ in 0..4 {
        rig.frame(STEP);
    }
    let w = rig.host.watch().unwrap();
    assert_eq!((w.focus, w.plate.clone()), (None, None));
    assert_eq!(rig.host.plate_drawn(), None);
}
