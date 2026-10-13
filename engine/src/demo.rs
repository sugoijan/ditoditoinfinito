//! A short chart that shows what a noteskin draws, played by the
//! autoplayer and looped: taps of several note types in every lane, a
//! hold, a roll, and a mine it steps on (for the mine's explosion), on a
//! built-in layout (four-lane dance by default, Dancing☆Onigiri's key
//! modes too). The skin previews draw its frames (`#/skin-preview?demo=1`,
//! Options).

use ddi_chart::{
    Chart, Difficulty, DisplayBpm, Layout, Note, NoteKind, Song, SourceFormat, SourceInfo, Tick,
    TimingMap,
};
use ddi_platform::{ClockSample, HostTime};

use crate::InputEvent;
use crate::autoplay;
use crate::frame::{Frame, SkinEvent};
use crate::player::{PlayOptions, Player};
use crate::rules::presets;
use crate::scroll::{ScrollOptions, SpeedMod};

/// Beats before the first note.
const LEAD_IN: i64 = 4;
/// Beats of the phrase.
const PHRASE: i64 = 8;
/// Times the phrase plays before the demo starts again.
const REPEATS: i64 = 4;
/// Beats after the last phrase.
const TAIL: i64 = 4;

/// The phrase on four lanes: `(48ths of a beat, lane, note)`, lanes Left,
/// Down, Up, Right. Quarter, eighth, twelfth and sixteenth notes, a hold, a
/// roll and a mine.
const PHRASE_NOTES: &[(i64, u8, Kind)] = &[
    (0, 0, Kind::Tap),
    (48, 1, Kind::Tap),
    (72, 2, Kind::Tap),
    (96, 3, Kind::Tap),
    (132, 0, Kind::Tap),
    (144, 1, Kind::Hold(72)),
    (160, 3, Kind::Tap),
    (192, 2, Kind::Roll(72)),
    (216, 0, Kind::Tap),
    (240, 3, Kind::Mine),
    (264, 0, Kind::Tap),
    (288, 3, Kind::Tap),
    (312, 0, Kind::Tap),
    (336, 1, Kind::Tap),
    (348, 2, Kind::Tap),
    (360, 3, Kind::Tap),
];

#[derive(Clone, Copy, Debug)]
enum Kind {
    Tap,
    /// Its length in 48ths.
    Hold(i64),
    Roll(i64),
    Mine,
}

/// The phrase on `lanes` lanes other than four: eighth notes across the
/// lanes, a hold on the first and a roll on the last, a mine in the middle
/// one, sixteenths back across.
fn phrase(lanes: u8) -> Vec<(i64, u8, Kind)> {
    if lanes == 4 {
        return PHRASE_NOTES.to_vec();
    }
    let mut out: Vec<(i64, u8, Kind)> = (0..lanes)
        .map(|l| (i64::from(l) * 24, l, Kind::Tap))
        .filter(|n| n.0 < 192)
        .collect();
    out.push((192, 0, Kind::Hold(72)));
    if lanes > 1 {
        out.push((192, lanes - 1, Kind::Roll(72)));
    }
    if lanes > 2 {
        out.push((240, lanes / 2, Kind::Mine));
    }
    out.extend(
        (0..lanes)
            .map(|i| (288 + i64::from(i) * 12, lanes - 1 - i, Kind::Tap))
            .filter(|n| n.0 < 372),
    );
    out
}

/// The demo's song on built-in layout `layout` (an id, `dance-single`) at
/// `bpm`; `None` for a layout that is not built in.
pub fn song(layout: &str, bpm: f64) -> Option<Song> {
    let lanes = u8::try_from(Layout::builtin(layout)?.lane_count()).ok()?;
    let phrase = phrase(lanes);
    let mut notes = Vec::new();
    for r in 0..REPEATS {
        let base = (LEAD_IN + r * PHRASE) * 48;
        for &(at, lane, kind) in &phrase {
            let tick = Tick(base + at);
            let kind = match kind {
                Kind::Tap => NoteKind::Tap,
                Kind::Hold(len) => NoteKind::HoldHead {
                    end: Tick(tick.0 + len),
                },
                Kind::Roll(len) => NoteKind::RollHead {
                    end: Tick(tick.0 + len),
                },
                Kind::Mine => NoteKind::Mine,
            };
            notes.push(Note::new(tick, lane, kind));
        }
    }
    notes.sort_by_key(|n| (n.tick, n.lane));
    Some(Song {
        title: "Noteskin demo".into(),
        subtitle: String::new(),
        artist: String::new(),
        title_translit: String::new(),
        subtitle_translit: String::new(),
        artist_translit: String::new(),
        genre: String::new(),
        credit: String::new(),
        music: None,
        preview_start: 0.0,
        preview_length: 0.0,
        banner: None,
        background: None,
        jacket: None,
        cd_title: None,
        timing: TimingMap::constant(bpm, 0.0),
        display_bpm: DisplayBpm::Actual,
        charts: vec![Chart {
            layout: layout.into(),
            difficulty: Difficulty::Beginner,
            meter: 1,
            name: "demo".into(),
            description: String::new(),
            credit: String::new(),
            notes,
            timing: None,
            display_bpm: None,
            danoni: None,
        }],
        effects: Vec::new(),
        keysounds: Vec::new(),
        layouts: Vec::new(),
        source: SourceInfo {
            format: SourceFormat::Other("generated".into()),
            unknown_tags: Vec::new(),
        },
    })
}

/// The demo playing.
pub struct Demo {
    song: Song,
    options: PlayOptions,
    player: Player,
    /// `(song seconds, lane, pressed)`, in order.
    script: Vec<(f64, u8, bool)>,
    next: usize,
    /// Seconds the chart lasts; it starts again after.
    length: f64,
    /// Seconds since the demo began at which this loop began.
    loop_start: f64,
    /// The player's next event number, and what this loop's numbers are
    /// moved by so that they keep rising across loops.
    seq: u64,
    seq_base: u64,
    /// The loop's time at the last step.
    last_now: f64,
}

/// A step further on than this (a page that was hidden) catches the play
/// up without its events: a backlog of them at once would cost a skin's
/// Lua a frame's worth of fuel many times over.
const MAX_STEP: f64 = 0.25;

/// One step of the demo.
pub struct DemoFrame {
    pub frame: Frame,
    /// What happened since the last step, numbered on from the last
    /// step's events, loops included.
    pub events: Vec<SkinEvent>,
}

impl Demo {
    /// The demo on built-in layout `layout` at `bpm`, notes `speed` times
    /// StepMania's 1x apart; `None` for a layout that is not built in.
    pub fn new(layout: &str, bpm: f64, speed: f64) -> Option<Demo> {
        let song = song(layout, bpm)?;
        let options = PlayOptions {
            scroll: ScrollOptions {
                speed: SpeedMod::XMod(speed),
                ..ScrollOptions::default()
            },
            ..PlayOptions::default()
        };
        let length = (LEAD_IN + REPEATS * PHRASE + TAIL) as f64 * 60.0 / bpm;
        let (player, script) = Self::start(&song, options);
        Some(Demo {
            song,
            options,
            player,
            script,
            next: 0,
            length,
            loop_start: 0.0,
            seq: 0,
            seq_base: 0,
            last_now: 0.0,
        })
    }

    /// A fresh play and its presses: the autoplayer's, and a step on each
    /// mine.
    fn start(song: &Song, options: PlayOptions) -> (Player, Vec<(f64, u8, bool)>) {
        let mut player = Player::new(song, 0, presets::itg(), options);
        let mut script: Vec<(f64, u8, bool)> = autoplay::script(player.judge().notes(), 0.0)
            .into_iter()
            .flat_map(|p| [(p.press, p.lane, true), (p.release, p.lane, false)])
            .collect();
        for n in player.judge().notes() {
            if matches!(n.note.kind, NoteKind::Mine) {
                script.push((n.seconds, n.note.lane, true));
                script.push((n.seconds + 0.05, n.note.lane, false));
            }
        }
        script.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
        player.clock_sample(ClockSample {
            context_time: 0.0,
            host_time: HostTime(0.0),
            output_latency: 0.0,
        });
        player.start(0.0, 0.0);
        (player, script)
    }

    /// Seconds one loop lasts.
    pub fn length(&self) -> f64 {
        self.length
    }

    /// The demo at `seconds` since it began (looping), with the events
    /// since the last step. Steps go forward in time; one of more than
    /// [`MAX_STEP`] (or across the loop's end) brings no events.
    pub fn step(&mut self, seconds: f64) -> DemoFrame {
        let mut silent = seconds - self.loop_start - self.last_now > MAX_STEP;
        if seconds - self.loop_start >= self.length {
            // Whole loops skipped (a hidden page) are not played.
            let loops = ((seconds - self.loop_start) / self.length).floor();
            self.loop_start += loops * self.length;
            self.seq_base += self.seq;
            self.seq = 0;
            let (player, script) = Self::start(&self.song, self.options);
            self.player = player;
            self.script = script;
            self.next = 0;
            // Rounding may leave the time at the loop's very end.
            silent |= seconds - self.loop_start > MAX_STEP;
        }
        let now = (seconds - self.loop_start).rem_euclid(self.length);
        self.last_now = now;
        while let Some(&(t, lane, pressed)) = self.script.get(self.next) {
            if t > now {
                break;
            }
            self.player.update(HostTime(t));
            self.player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t),
            });
            self.next += 1;
        }
        self.player.update(HostTime(now));
        let base = self.seq_base;
        let events: Vec<SkinEvent> = self
            .player
            .skin_events_since(self.seq)
            .map(|e| SkinEvent {
                seq: e.seq + base,
                ..*e
            })
            .collect();
        if let Some(last) = events.last() {
            self.seq = last.seq - base + 1;
        }
        DemoFrame {
            frame: self.player.frame(HostTime(now)),
            events: if silent { Vec::new() } else { events },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{SkinEventKind, SpriteKind};
    use crate::rules::Judgement;

    #[test]
    fn the_demo_shows_every_kind_of_note_and_loops() {
        let mut demo = Demo::new("dance-single", 120.0, 1.5).unwrap();
        let (mut taps, mut held, mut mines, mut kinds) = (0, 0, 0, Vec::new());
        let mut last_seq = None;
        // Two loops at 60 frames a second.
        let frames = (demo.length() * 2.0 * 60.0) as u32;
        for i in 0..frames {
            let step = demo.step(f64::from(i) / 60.0);
            for e in &step.events {
                assert!(last_seq.is_none_or(|s| e.seq > s), "{e:?}");
                last_seq = Some(e.seq);
                match e.kind {
                    SkinEventKind::Tap { judgement, .. } => {
                        assert_eq!(judgement, Judgement::W1);
                        taps += 1;
                    }
                    SkinEventKind::Held { .. } => held += 1,
                    SkinEventKind::HitMine => mines += 1,
                    _ => {}
                }
            }
            for n in &step.frame.notes {
                let k = std::mem::discriminant(&n.kind);
                if !kinds.contains(&k) {
                    kinds.push(k);
                }
            }
        }
        let per_loop = REPEATS as usize;
        // Hold and roll heads flash as taps too.
        let taps_per_phrase = PHRASE_NOTES
            .iter()
            .filter(|n| !matches!(n.2, Kind::Mine))
            .count();
        assert_eq!(taps, 2 * per_loop * taps_per_phrase);
        assert_eq!(held, 2 * per_loop * 2);
        assert_eq!(mines, 2 * per_loop);
        for k in [
            SpriteKind::Tap,
            SpriteKind::Mine,
            SpriteKind::HoldHead {
                tail_y: 0.0,
                active: false,
                dropped: false,
            },
            SpriteKind::RollHead {
                tail_y: 0.0,
                active: false,
                dropped: false,
            },
        ] {
            assert!(kinds.contains(&std::mem::discriminant(&k)), "{k:?}");
        }
    }

    #[test]
    fn every_built_in_layout_has_a_demo_played_perfectly() {
        for id in Layout::builtin_ids() {
            let mut demo = Demo::new(&id, 150.0, 1.5).unwrap();
            let lanes = Layout::builtin(&id).unwrap().lane_count();
            let mut seen = vec![false; lanes];
            let (mut held, mut mines) = (0, 0);
            for i in 0..(demo.length() * 30.0) as u32 {
                let step = demo.step(f64::from(i) / 30.0);
                for e in &step.events {
                    seen[usize::from(e.lane)] = true;
                    match e.kind {
                        SkinEventKind::Tap { judgement, .. } => {
                            assert_eq!(judgement, Judgement::W1, "{id}")
                        }
                        SkinEventKind::Held { .. } => held += 1,
                        SkinEventKind::HitMine => mines += 1,
                        _ => {}
                    }
                }
            }
            assert!(seen.iter().all(|s| *s), "{id}: {seen:?}");
            // A hold and a roll each phrase; a mine where there are three
            // lanes or more.
            assert_eq!(held, 2 * REPEATS, "{id}");
            assert_eq!(mines, if lanes > 2 { REPEATS } else { 0 }, "{id}");
        }
        assert!(Demo::new("pump-single", 120.0, 1.0).is_none());
    }

    #[test]
    fn a_jump_brings_no_backlog_of_events() {
        let mut demo = Demo::new("dance-single", 120.0, 1.5).unwrap();
        let _ = demo.step(0.0);
        // Most of a loop at once (a hidden page): caught up silently.
        let step = demo.step(demo.length() * 0.9);
        assert!(step.events.is_empty());
        // Then on as usual, numbered after what was skipped.
        let mut events = Vec::new();
        for i in 1..=60 {
            events.extend(demo.step(demo.length() * 0.9 + f64::from(i) / 60.0).events);
        }
        // Across a skipped loop's end too.
        assert!(demo.step(demo.length() * 3.5).events.is_empty());
        let after: Vec<_> = (1..=240)
            .flat_map(|i| demo.step(demo.length() * 3.5 + f64::from(i) / 60.0).events)
            .collect();
        assert!(!after.is_empty());
        assert!(events.iter().chain(&after).is_sorted_by_key(|e| e.seq));
    }
}
