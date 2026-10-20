//! A short chart that shows what a noteskin draws, played by the
//! autoplayer and looped: taps of several note types in every lane, hit
//! in every judgement's window (and one missed now and then), holds and
//! rolls kept and let go, and a mine it steps on (for the mine's
//! explosion), on a
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
use crate::rules::{RulesetKey, presets};
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
            derivation: None,
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
    rules: RulesetKey,
    player: Player,
    autoplay: bool,
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

/// Each tap's timing in turn, seconds late (`None`: missed; hold and roll
/// heads are hit on time): on time between the others, each of ITG's windows early or
/// late (W2 to W5: 43, 102, 135 and 180 ms, `rules::presets::itg`), and a
/// miss.
const TIMINGS: [Option<f64>; 12] = [
    Some(0.0),
    Some(0.0),
    Some(0.033),
    Some(0.0),
    Some(-0.075),
    Some(0.0),
    Some(0.12),
    Some(0.0),
    Some(-0.16),
    Some(0.0),
    None,
    Some(0.0),
];

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
        let options = PlayOptions {
            scroll: ScrollOptions {
                speed: SpeedMod::XMod(speed),
                ..ScrollOptions::default()
            },
            ..PlayOptions::default()
        };
        Demo::with_options(layout, bpm, options, &presets::itg().key())
    }

    /// The demo played with `options` (the scroll, Turn and Cut, the
    /// appearance, receptor snap: what a player chose) and judged by
    /// the ruleset `rules` builds (never failing), as play would play and
    /// judge it.
    pub fn with_options(
        layout: &str,
        bpm: f64,
        options: PlayOptions,
        rules: &RulesetKey,
    ) -> Option<Demo> {
        let rules = rules.clone();
        let song = song(layout, bpm)?;
        let length = (LEAD_IN + REPEATS * PHRASE + TAIL) as f64 * 60.0 / bpm;
        let (player, script) = Self::start(&song, options, &rules);
        Some(Demo {
            song,
            options,
            rules,
            player,
            autoplay: false,
            script,
            next: 0,
            length,
            loop_start: 0.0,
            seq: 0,
            seq_base: 0,
            last_now: 0.0,
        })
    }

    /// Preview only: keep its scripted timing examples, present their flashes
    /// as autoplay. The hint survives a loop restart.
    pub fn set_autoplay(&mut self, autoplay: bool) {
        self.autoplay = autoplay;
        self.player.set_autoplay(autoplay);
    }

    /// A fresh play and its presses: the autoplayer's, moved through
    /// [`TIMINGS`], every other hold let go and every other roll left a
    /// third of the way, and a step on each mine.
    fn start(
        song: &Song,
        options: PlayOptions,
        rules: &RulesetKey,
    ) -> (Player, Vec<(f64, u8, bool)>) {
        // Its misses and drops must not end it.
        let mut rules = rules.build(&song.charts[0]);
        rules.fail = crate::rules::FailPolicy::Off;
        let mut player = Player::new(song, 0, rules, options);
        let mut presses: Vec<Option<autoplay::Press>> =
            autoplay::script(player.judge().notes(), 0.0)
                .into_iter()
                .map(Some)
                .collect();
        // The autoplayer presses each note at its judged second exactly.
        let first_press = |presses: &[Option<autoplay::Press>], lane: u8, at: f64| {
            presses
                .iter()
                .position(|p| p.is_some_and(|p| p.lane == lane && p.press == at))
        };
        // Taps so far, and holds and rolls so far.
        let (mut heads, mut long) = (0, [0usize; 2]);
        let mut notes: Vec<_> = player.judge().notes().iter().collect();
        notes.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
        for n in notes {
            let lane = n.note.lane;
            match n.note.kind {
                NoteKind::Tap => {
                    let Some(i) = first_press(&presses, lane, n.seconds) else {
                        continue;
                    };
                    match TIMINGS[heads % TIMINGS.len()] {
                        Some(late) => {
                            if let Some(p) = presses[i].as_mut() {
                                p.press += late;
                                p.release += late;
                            }
                        }
                        None => presses[i] = None,
                    }
                    heads += 1;
                }
                NoteKind::HoldHead { .. } | NoteKind::RollHead { .. } => {
                    let count =
                        &mut long[usize::from(matches!(n.note.kind, NoteKind::RollHead { .. }))];
                    *count += 1;
                    if *count % 2 == 1 {
                        continue;
                    }
                    let early = n.seconds + (n.end_seconds - n.seconds) / 3.0;
                    if matches!(n.note.kind, NoteKind::HoldHead { .. }) {
                        if let Some(p) =
                            first_press(&presses, lane, n.seconds).and_then(|i| presses[i].as_mut())
                        {
                            p.release = early;
                        }
                    } else {
                        for p in presses.iter_mut() {
                            if p.is_some_and(|p| {
                                p.lane == lane && p.press > early && p.press <= n.end_seconds
                            }) {
                                *p = None;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let mut script = autoplay::edges(presses.into_iter().flatten());
        for n in player.judge().notes() {
            if matches!(n.note.kind, NoteKind::Mine) {
                script.push((n.seconds, n.note.lane, true));
                script.push((n.seconds + 0.05, n.note.lane, false));
            }
        }
        autoplay::sort_edges(&mut script);
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

    /// The ruleset it is judged by (its judgement names, as drawn).
    pub fn ruleset(&self) -> &crate::rules::Ruleset {
        self.player.ruleset()
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
            let (player, script) = Self::start(&self.song, self.options, &self.rules);
            self.player = player;
            self.player.set_autoplay(self.autoplay);
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
        let (mut held, mut mines, mut kinds) = (0, 0, Vec::new());
        let (mut judged, mut dropped, mut passed) = (Vec::new(), false, false);
        let mut last_seq = None;
        // Two loops at 60 frames a second.
        let frames = (demo.length() * 2.0 * 60.0) as u32;
        for i in 0..frames {
            let step = demo.step(f64::from(i) / 60.0);
            for e in &step.events {
                assert!(last_seq.is_none_or(|s| e.seq > s), "{e:?}");
                last_seq = Some(e.seq);
                match e.kind {
                    SkinEventKind::Tap { judgement, .. } if !judged.contains(&judgement) => {
                        judged.push(judgement)
                    }
                    SkinEventKind::Held { .. } => held += 1,
                    SkinEventKind::HitMine => mines += 1,
                    _ => {}
                }
            }
            for n in &step.frame.notes {
                // A missed tap goes on past its receptor.
                passed |= matches!(n.kind, SpriteKind::Tap) && n.y < -0.5;
                dropped |= matches!(
                    n.kind,
                    SpriteKind::HoldHead { dropped: true, .. }
                        | SpriteKind::RollHead { dropped: true, .. }
                );
                let k = std::mem::discriminant(&n.kind);
                if !kinds.contains(&k) {
                    kinds.push(k);
                }
            }
        }
        let per_loop = REPEATS as usize;
        // Every judgement's explosion shows (a miss has none: its tap goes
        // by); every other hold and roll is let go.
        for j in [
            Judgement::W1,
            Judgement::W2,
            Judgement::W3,
            Judgement::W4,
            Judgement::W5,
        ] {
            assert!(judged.contains(&j), "{j:?}: {judged:?}");
        }
        assert!(dropped);
        assert!(passed, "no tap missed");
        assert_eq!(held, 2 * per_loop);
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
    fn every_built_in_layout_has_a_demo() {
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
                        SkinEventKind::Held { .. } => held += 1,
                        SkinEventKind::HitMine => mines += 1,
                        _ => {}
                    }
                }
            }
            assert!(seen.iter().all(|s| *s), "{id}: {seen:?}");
            // A hold and a roll each phrase, every other one let go; a
            // mine where there are three lanes or more.
            assert_eq!(held, REPEATS, "{id}");
            assert_eq!(mines, if lanes > 2 { REPEATS } else { 0 }, "{id}");
        }
        assert!(Demo::new("pump-couple", 120.0, 1.0).is_none());
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

    /// The demo keeps every other hold and drops the rest: its frames
    /// show O.K. and N.G. over the lanes in the dance game, under the
    /// combo under Dancing☆Onigiri's rules, and none in the pump game.
    #[test]
    fn kept_and_dropped_holds_show_their_outcomes_where_the_rules_say() {
        use crate::frame::HoldPlace;
        let seen = |layout: &str, rules: &RulesetKey| {
            let mut demo =
                Demo::with_options(layout, 120.0, PlayOptions::default(), rules).unwrap();
            let (mut ok, mut ng, mut place) = (false, false, HoldPlace::None);
            for i in 0..(demo.length() * 30.0) as u32 {
                let step = demo.step(f64::from(i) / 30.0);
                place = step.frame.holds.place;
                for f in &step.frame.holds.flashes {
                    if f.held {
                        ok = true;
                    } else {
                        ng = true;
                    }
                    assert!(usize::from(f.lane) < usize::from(step.frame.lanes));
                }
            }
            (place, ok, ng)
        };
        let itg = presets::itg().key();
        assert_eq!(seen("dance-single", &itg), (HoldPlace::Lanes, true, true));
        assert_eq!(
            seen("dance-single", &presets::danoni().key()),
            (HoldPlace::Centre, true, true)
        );
        assert_eq!(seen("pump-single", &itg).0, HoldPlace::None);
        assert!(!seen("pump-single", &itg).1);
    }
}

#[cfg(test)]
mod autoplay_presentation_tests {
    use super::*;

    #[test]
    fn autoplay_hint_survives_preview_loops_without_changing_timing_examples() {
        let mut demo = Demo::new("danoni-7", 120.0, 1.0).unwrap();
        demo.set_autoplay(true);
        let mut seen = [0; 2];
        let mut judgements = Vec::new();
        for i in 0..(demo.length() * 120.0) as u32 {
            let now = f64::from(i) / 60.0;
            let loop_index = usize::from(now >= demo.length());
            for event in demo.step(now).events {
                assert!(event.autoplay);
                seen[loop_index] += 1;
                if let crate::SkinEventKind::Tap { judgement, .. } = event.kind
                    && !judgements.contains(&judgement)
                {
                    judgements.push(judgement);
                }
            }
        }
        assert!(seen.iter().all(|&n| n > 0));
        assert!(judgements.contains(&crate::rules::Judgement::W5));
    }
}
