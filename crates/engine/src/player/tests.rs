//! A frame's walk is bounded ([`Player::notes_to_walk`]) and draws what
//! the walk over every note draws, on any chart.

use ddi_chart::formats::sm::parse_sm;
use ddi_chart::{
    BpmSegment, Chart, Difficulty, Note, NoteKind, ScrollSegment, Song, SpeedSegment, SpeedUnit,
    StopSegment, Tick, TimingMap, WarpSegment,
};
use ddi_platform::{ClockSample, HostTime};

use super::*;
use crate::autoplay;
use crate::rules::presets;
use crate::scroll::{ScrollAction, ScrollOptions, SpeedMod};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// What a chart's timing and notes may do.
#[derive(Clone, Copy, Default)]
struct Shape {
    tempo: bool,
    stops: bool,
    delays: bool,
    warps: bool,
    speeds: bool,
    speeds_stopping: bool,
    scrolls: bool,
    scrolls_running_back: bool,
    note_speeds: bool,
}

const BEATS: i64 = 48;

fn beat(b: i64) -> Tick {
    Tick::from_beats(b)
}

fn timing(shape: Shape) -> TimingMap {
    let mut t = TimingMap::constant(120.0, 0.0);
    if shape.tempo {
        let bpm = |b, bpm| BpmSegment { tick: beat(b), bpm };
        t.bpms.extend([bpm(12, 240.0), bpm(30, 90.0)]);
    }
    if shape.stops {
        let stop = |b, seconds| StopSegment {
            tick: beat(b),
            seconds,
        };
        t.stops
            .extend([stop(6, 0.5), stop(20, 1.5), stop(37, 0.25)]);
    }
    if shape.delays {
        t.delays.push(StopSegment {
            tick: beat(16),
            seconds: 0.75,
        });
    }
    if shape.warps {
        let warp = |b, length| WarpSegment {
            tick: beat(b),
            length: beat(length),
        };
        t.warps.extend([warp(14, 3), warp(32, 2)]);
    }
    let speed = |b, ratio, delay, unit| SpeedSegment {
        tick: beat(b),
        ratio,
        delay,
        unit,
    };
    if shape.speeds {
        t.speeds.extend([
            speed(8, 2.0, 0.0, SpeedUnit::Beats),
            speed(24, 0.5, 2.0, SpeedUnit::Seconds),
            speed(40, 1.5, 1.0, SpeedUnit::Beats),
        ]);
    }
    if shape.speeds_stopping {
        t.speeds.extend([
            speed(10, 0.0, 0.0, SpeedUnit::Beats),
            speed(18, -1.0, 0.0, SpeedUnit::Beats),
            speed(26, 1.0, 1.0, SpeedUnit::Beats),
        ]);
    }
    let scroll = |b, ratio| ScrollSegment {
        tick: beat(b),
        ratio,
    };
    if shape.scrolls {
        t.scrolls
            .extend([scroll(4, 0.5), scroll(22, 0.0), scroll(28, 3.0)]);
    }
    if shape.scrolls_running_back {
        t.scrolls.extend([scroll(16, -1.0), scroll(20, 1.0)]);
    }
    t.tidy();
    t
}

fn notes(rng: &mut Rng, shape: Shape) -> Vec<Note> {
    let mut notes = Vec::new();
    let mut free = [0i64; 4];
    for tick in (0..BEATS * 48).step_by(12) {
        for lane in 0..4u8 {
            if free[usize::from(lane)] > tick || !rng.chance(14) {
                continue;
            }
            // Some holds outlast many notes after them.
            let length = if rng.chance(25) { 64 } else { 8 };
            let end = Tick(tick + 12 * (1 + rng.below(length)) as i64);
            let kind = match rng.below(100) {
                0..=49 => NoteKind::Tap,
                50..=71 => NoteKind::HoldHead { end },
                72..=81 => NoteKind::RollHead { end },
                82..=89 => NoteKind::Mine,
                90..=94 => NoteKind::Lift,
                _ => NoteKind::Fake,
            };
            if let Some(end) = kind.hold_end() {
                free[usize::from(lane)] = end.0 + 6;
            }
            let mut note = Note::new(Tick(tick), lane, kind);
            if shape.note_speeds && rng.chance(20) {
                note.speed_mul = Some(0.5 + rng.unit() as f32 * 2.0);
            }
            notes.push(note);
        }
    }
    notes.sort_by_key(|n| (n.tick, n.lane));
    notes
}

fn song(rng: &mut Rng, shape: Shape) -> Song {
    Song::generated(
        "Walk",
        timing(shape),
        Chart::plain("dance-single", Difficulty::Hard, 9, notes(rng, shape)),
        "test",
    )
}

/// Plays `song`, pressing most notes a little off and letting go of some
/// holds, and compares the two walks every frame: drawn the same, each
/// with its held receptors. How many frames the walk was bounded in.
fn compare(song: &Song, options: PlayOptions, rng: &mut Rng) -> usize {
    let mut p = Player::new(song, 0, presets::itg(), options);
    p.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: 0.0,
    });
    p.start(0.0, 0.0);
    let mut script = autoplay::script(p.judge().notes(), 0.0);
    script.retain(|_| rng.chance(80));
    for press in &mut script {
        let off = rng.unit() * 0.2 - 0.1;
        press.press += off;
        press.release = press.release.min(press.press + rng.unit() * 6.0) + off;
    }
    let mut edges = autoplay::edges(script);
    autoplay::sort_edges(&mut edges);
    let mut next = 0;
    let mut narrowed = 0;
    let mut t = -1.0;
    while t < p.song_end_time() + 2.0 {
        while let Some(&(at, lane, down)) = edges.get(next).filter(|e| e.0 <= t) {
            p.input_at(lane, down, at);
            next += 1;
        }
        p.update_at(t);
        let scroll = ScrollState::at(p.options.scroll, &p.timing, t);
        if p.notes_to_walk(&scroll, t).len() < p.judge.notes().len() {
            narrowed += 1;
        }
        let bounded = p.frame(HostTime(t));
        let reach = p.reach.take();
        let whole = p.frame(HostTime(t));
        p.reach = reach;
        assert_eq!(bounded.notes, whole.notes, "drawn at {t}");
        assert_eq!(bounded.receptors, whole.receptors, "receptors at {t}");
        t += 1.0 / 29.0;
    }
    narrowed
}

fn options(speed: SpeedMod, scroll_action: ScrollAction, visible_range: f32) -> PlayOptions {
    PlayOptions {
        scroll: ScrollOptions {
            speed,
            reverse: false,
            scroll_action,
        },
        visible_range,
        ..PlayOptions::default()
    }
}

/// Each way of scrolling on `shape`'s charts.
fn compare_shape(shape: Shape, name: &str, bounded: bool) {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut narrowed = 0;
    for _ in 0..4 {
        let song = song(&mut rng, shape);
        let drawn_in_order = {
            let p = Player::new(&song, 0, presets::itg(), PlayOptions::default());
            p.reach.is_some()
        };
        assert_eq!(drawn_in_order, bounded, "{name}");
        for speed in [
            SpeedMod::XMod(1.0),
            SpeedMod::XMod(3.5),
            SpeedMod::XMod(0.25),
            SpeedMod::CMod(350.0),
        ] {
            for action in [
                ScrollAction::Normal,
                ScrollAction::Boost,
                ScrollAction::Brake,
                ScrollAction::Wave,
            ] {
                for visible_range in [crate::player::DEFAULT_VISIBLE_RANGE, 1.0] {
                    narrowed += compare(&song, options(speed, action, visible_range), &mut rng);
                }
            }
        }
    }
    assert_eq!(narrowed > 0, bounded, "{name}: narrowed {narrowed}");
}

#[test]
fn a_constant_tempo_chart_walks_only_what_it_draws() {
    compare_shape(Shape::default(), "plain", true);
}

#[test]
fn tempo_changes_stops_delays_and_warps_stay_in_order() {
    let shape = Shape {
        tempo: true,
        stops: true,
        delays: true,
        warps: true,
        ..Shape::default()
    };
    compare_shape(shape, "tempo, stops, delays, warps", true);
}

#[test]
fn speed_changes_and_a_scroll_at_rest_stay_in_order() {
    let shape = Shape {
        speeds: true,
        scrolls: true,
        ..Shape::default()
    };
    compare_shape(shape, "speeds and scrolls", true);
}

#[test]
fn a_speed_of_zero_or_below_walks_every_note_while_it_lasts() {
    let shape = Shape {
        stops: true,
        speeds_stopping: true,
        ..Shape::default()
    };
    compare_shape(shape, "a stopping speed", true);
}

#[test]
fn a_scroll_running_back_walks_every_note() {
    let shape = Shape {
        scrolls_running_back: true,
        ..Shape::default()
    };
    compare_shape(shape, "a scroll running back", false);
}

#[test]
fn notes_with_speeds_of_their_own_walk_every_note() {
    let shape = Shape {
        note_speeds: true,
        ..Shape::default()
    };
    compare_shape(shape, "notes with their own speeds", false);
}

/// A negative BPM as a file writes it (StepMania reads it as a warp):
/// the notes and holds of a chart run through it in order.
#[test]
fn a_negative_bpm_section_stays_in_order() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let measures = (0..14)
        .map(|m| {
            if m % 2 == 0 {
                "2000\n0100\n0010\n3001"
            } else {
                "0001\n0010\n0100\n1000"
            }
        })
        .collect::<Vec<_>>()
        .join("\n,\n");
    let text = format!(
        "#TITLE:t;\n#OFFSET:0;\n#BPMS:0=150,8=-150,10=150,20=-300,21=90;\n\
         #STOPS:4=0.5,28=0.25;\n#NOTES:\ndance-single:\n:\nHard:\n5:\n0,0,0,0,0:\n{measures}\n;\n"
    );
    let song = parse_sm(&text).expect("parses");
    let p = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    assert!(p.reach.is_some());
    let mut narrowed = 0;
    for speed in [SpeedMod::XMod(2.0), SpeedMod::CMod(180.0)] {
        narrowed += compare(
            &song,
            options(speed, ScrollAction::Normal, DEFAULT_VISIBLE_RANGE),
            &mut rng,
        );
    }
    assert!(narrowed > 0);
}
