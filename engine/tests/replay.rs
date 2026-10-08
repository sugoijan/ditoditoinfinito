//! Replay fixtures: a scripted play of one chart under each preset.
//!
//! Chart (120 BPM, offset 0, so beat `b` is at `b / 2` seconds), dance-single:
//!
//! | idx | beat | lane | kind            | seconds |
//! |-----|------|------|-----------------|---------|
//! | 0   | 0    | 0    | tap             | 0.0     |
//! | 1   | 1    | 1    | tap             | 0.5     |
//! | 2   | 2    | 2    | tap             | 1.0     |
//! | 3   | 3    | 3    | tap             | 1.5     |
//! | 4   | 4    | 0    | tap (jump)      | 2.0     |
//! | 5   | 4    | 3    | tap (jump)      | 2.0     |
//! | 6   | 5    | 1    | tap             | 2.5     |
//! | 7   | 6    | 2    | tap             | 3.0     |
//! | 8   | 7    | 0    | tap             | 3.5     |
//! | 9   | 8    | 1    | hold → beat 10  | 4.0–5.0 |
//! | 10  | 9    | 3    | tap             | 4.5     |
//! | 11  | 11   | 2    | hold → beat 13  | 5.5–6.5 |
//! | 12  | 12   | 0    | tap             | 6.0     |
//! | 13  | 14   | 1    | roll → beat 16  | 7.0–8.0 |
//! | 14  | 15   | 3    | mine            | 7.5     |
//! | 15  | 17   | 0    | mine            | 8.5     |
//! | 16  | 18   | 2    | tap             | 9.0     |
//! | 17  | 19   | 3    | tap             | 9.5     |
//! | 18  | 20   | 1    | tap             | 10.0    |
//! | 19  | 21   | 0    | tap             | 10.5    |
//! | 20  | 22   | 2    | fake            | 11.0    |
//!
//! 18 judged tap-like notes (15 taps + 3 heads), 17 rows (the jump is one),
//! 3 holds/rolls, 2 mines.

use ddi_chart::{
    BpmSegment, Chart, Difficulty, DisplayBpm, Layout, Note, NoteKind, Song, SourceFormat,
    SourceInfo, StopSegment, Tick, TimingMap,
};
use ddi_engine::frame::SpriteKind;
use ddi_engine::judge::{JudgeEvent, JudgeEventKind};
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::{FailPolicy, FullCombo, Judgement, Ruleset, presets};
use ddi_engine::scroll::{ScrollOptions, note_y};
use ddi_engine::{ClockOptions, InputEvent};
use ddi_platform::{ClockSample, HostTime};

fn song(timing: TimingMap, notes: Vec<Note>) -> Song {
    Song {
        title: "Replay".into(),
        subtitle: String::new(),
        artist: "test".into(),
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
        timing,
        display_bpm: DisplayBpm::Actual,
        charts: vec![Chart {
            layout: "dance-single".into(),
            difficulty: Difficulty::Hard,
            meter: 5,
            name: "replay".into(),
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
            format: SourceFormat::Other("test".into()),
            unknown_tags: Vec::new(),
        },
    }
}

fn b(beat: i64) -> Tick {
    Tick::from_beats(beat)
}

fn fixture_song() -> Song {
    let notes = vec![
        Note::new(b(0), 0, NoteKind::Tap),
        Note::new(b(1), 1, NoteKind::Tap),
        Note::new(b(2), 2, NoteKind::Tap),
        Note::new(b(3), 3, NoteKind::Tap),
        Note::new(b(4), 0, NoteKind::Tap),
        Note::new(b(4), 3, NoteKind::Tap),
        Note::new(b(5), 1, NoteKind::Tap),
        Note::new(b(6), 2, NoteKind::Tap),
        Note::new(b(7), 0, NoteKind::Tap),
        Note::new(b(8), 1, NoteKind::HoldHead { end: b(10) }),
        Note::new(b(9), 3, NoteKind::Tap),
        Note::new(b(11), 2, NoteKind::HoldHead { end: b(13) }),
        Note::new(b(12), 0, NoteKind::Tap),
        Note::new(b(14), 1, NoteKind::RollHead { end: b(16) }),
        Note::new(b(15), 3, NoteKind::Mine),
        Note::new(b(17), 0, NoteKind::Mine),
        Note::new(b(18), 2, NoteKind::Tap),
        Note::new(b(19), 3, NoteKind::Tap),
        Note::new(b(20), 1, NoteKind::Tap),
        Note::new(b(21), 0, NoteKind::Tap),
        Note::new(b(22), 2, NoteKind::Fake),
    ];
    song(TimingMap::constant(120.0, 0.0), notes)
}

/// `(host milliseconds, lane, pressed)`; every tap is released 50 ms later.
fn script() -> Vec<(u32, u8, bool)> {
    let taps: &[(u32, u8)] = &[
        (0, 0),     // idx 0: exact
        (520, 1),   // idx 1: +20 ms
        (1040, 2),  // idx 2: +40 ms
        (1580, 3),  // idx 3: +80 ms
        (2000, 0),  // idx 4: exact (jump)
        (2120, 3),  // idx 5: +120 ms (jump)
        (2660, 1),  // idx 6: +160 ms (outside DDR's windows → missed first)
        (2970, 2),  // idx 7: −30 ms
        (3500, 0),  // idx 8: exact
        (4500, 3),  // idx 10: exact (while hold 9 is held)
        (6000, 0),  // idx 12: exact
        (7520, 3),  // idx 14: mine, +20 ms → hit
        (9000, 2),  // idx 16
        (9500, 3),  // idx 17
        (10500, 0), // idx 19 (idx 18 is never pressed → miss)
        (11500, 2), // stray press, nothing nearby
    ];
    let mut out = Vec::new();
    for &(t, lane) in taps {
        out.push((t, lane, true));
        out.push((t + 50, lane, false));
    }
    // idx 9: hold pressed at its head, released after the tail.
    out.push((4000, 1, true));
    out.push((5100, 1, false));
    // idx 11: hold pressed, released after 100 ms, never re-pressed → let go.
    out.push((5500, 2, true));
    out.push((5600, 2, false));
    // idx 13: roll head plus re-taps every 200 ms until the tail.
    for t in [7000, 7200, 7400, 7600, 7800] {
        out.push((t, 1, true));
        out.push((t + 50, 1, false));
    }
    out.sort_by_key(|&(t, lane, pressed)| (t, lane, pressed));
    out
}

struct Run {
    events: Vec<JudgeEvent>,
    player: Player,
}

/// Drives the fixture with zero drift (`context_time == host_time`).
/// `latency` and `audio_offset` shift every host timestamp so the heard song
/// time stays the same.
fn run(ruleset: Ruleset, song: &Song, latency: f64, audio_offset: f64) -> Run {
    let options = PlayOptions {
        clock: ClockOptions {
            audio_offset,
            ..ClockOptions::default()
        },
        ..PlayOptions::default()
    };
    let script: Vec<(f64, u8, bool)> = script()
        .into_iter()
        .map(|(t, lane, pressed)| (f64::from(t) / 1000.0, lane, pressed))
        .collect();
    run_script(ruleset, song, options, latency, &script)
}

/// Drives `song` with `script` (`(song seconds, lane, pressed)`, sorted),
/// updating every 10 ms and drawing a frame each step.
fn run_script(
    ruleset: Ruleset,
    song: &Song,
    options: PlayOptions,
    latency: f64,
    script: &[(f64, u8, bool)],
) -> Run {
    let shift = latency + options.clock.audio_offset;
    let mut player = Player::new(song, 0, ruleset, options);
    player.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: latency,
    });
    player.start(0.0, 0.0);
    let mut next = 0;
    let mut events = Vec::new();
    for step in 0..=1250u32 {
        let now = f64::from(step) / 100.0;
        events.extend(player.update(HostTime(now + shift)));
        let _ = player.frame(HostTime(now + shift));
        while next < script.len() && script[next].0 < now + 0.01 - 1e-9 {
            let (t, lane, pressed) = script[next];
            events.extend(player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t + shift),
            }));
            next += 1;
        }
    }
    Run { events, player }
}

fn describe(ev: &JudgeEvent) -> String {
    let idx = ev.note_index.map_or("-".to_string(), |i| i.to_string());
    match ev.kind {
        JudgeEventKind::Tap(j, _) => format!("{j:?}@{idx}"),
        JudgeEventKind::Held => format!("Held@{idx}"),
        JudgeEventKind::LetGo => format!("LetGo@{idx}"),
        JudgeEventKind::HitMine => format!("Mine@{idx}"),
        JudgeEventKind::Boo => "Boo".to_string(),
    }
}

fn sequence(run: &Run) -> Vec<String> {
    run.events.iter().map(describe).collect()
}

fn event_time(run: &Run, pred: impl Fn(&JudgeEvent) -> bool) -> f64 {
    run.events
        .iter()
        .find(|e| pred(e))
        .expect("event present")
        .song_time
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() < eps
}

const SM_SEQUENCE: &[&str] = &[
    "W1@0", "W1@1", "W2@2", "W3@3", "W1@4", "W4@5", "W5@6", "W2@7", "W1@8", "W1@9", "W1@10",
    "Held@9", "W1@11", "LetGo@11", "W1@12", "W1@13", "Mine@14", "Held@13", "W1@16", "W1@17",
    "Miss@18", "W1@19",
];

#[test]
fn itg_replay() {
    let song = fixture_song();
    let run = run(presets::itg(), &song, 0.0, 0.0);
    assert_eq!(sequence(&run), SM_SEQUENCE);
    assert!(run.player.finished());
    assert!(!run.player.failed());

    // Hold 11 was released at 5.6 s; ITG's 0.32 s hold window plus the 0.0015 s
    // TimingWindowAdd drops it at 5.9215.
    assert!(close(
        event_time(&run, |e| e.kind == JudgeEventKind::LetGo),
        5.9215,
        1e-9
    ));
    // Note 18 (10.0 s) is missed once the widest window (0.180 + 0.0015) passes.
    assert!(close(
        event_time(&run, |e| e.note_index == Some(18)),
        10.1815,
        1e-9
    ));

    let r: Results = run.player.results();
    // Tally: W1 ×12 (0,1,4,8,9,10,11,12,13,16,17,19), W2 ×2 (2,7), W3 ×1 (3),
    // W4 ×1 (5), W5 ×1 (6), Miss ×1 (18); 2 held, 1 let go, 1 mine.
    assert_eq!(r.tally.taps, [12, 2, 1, 1, 1, 1]);
    assert_eq!((r.tally.held, r.tally.let_go, r.tally.mine_hit), (2, 1, 1));
    // Combo: W4 breaks (SM continues at W3), W5 breaks, Miss breaks; LetGo and
    // mines do not. Longest run: W2@7 … W1@17 = 9.
    assert_eq!(r.max_combo, 9);
    assert_eq!(run.player.combo(), 1);
    assert_eq!(r.full_combo, FullCombo::None);
    // DP = 12×5 + 2×4 + 1×2 + 1×0 + 1×(−6) + 1×(−12) + 2×5 (held) + 1×(−6) (mine) = 56.
    // Possible = 18 taps × 5 + 3 holds × 5 = 105.
    assert!(close(r.score.percent.unwrap(), 56.0 / 105.0, 1e-12));
    assert_eq!(r.score.money, None);
    // EX = 12×3 + 2×2 + 1×1 + 2×1 (held) − 1 (mine) = 42 of 18×3 + 3×1 = 57.
    assert_eq!((r.score.ex, r.score.max_ex), (Some(42), Some(57)));
    // 56/105 = 53.3% < 55% → D.
    assert_eq!(r.grade, "D");
    // Deltas: +0.02, +0.04, +0.08, +0.12, +0.16 late; −0.03 early; 11 exact.
    assert_eq!((r.fast, r.slow), (1, 5));
    assert!(close(r.mean_delta, 0.39 / 17.0, 1e-9));

    // Life (SM LifeMeterBar semantics, see rules::gauge::LifeBar):
    //  0.5 +.008 +.008 +.008 +.004 +.008 +0 = 0.536 (W1 W1 W2 W3 W1 W4)
    //  W5 −0.050 → 0.486, lockout 5
    //  W2 W1 W1 W1 swallowed (lockout 4,3,2,1); Held@9 brings it to 0 → +0.008 = 0.494
    //  W1 → 0.502; LetGo −0.080 → 0.422, lockout 5
    //  W1 W1 swallowed (4,3); Mine −0.050 → 0.372, lockout max(3, min(10, 3+5)) = 8
    //  Held@13 (7) W1 (6) W1 (5) swallowed; Miss −0.100 → 0.272, lockout 10
    //  W1 (9) swallowed → 0.272
    let frame = run.player.frame(HostTime(13.0));
    assert!(
        close(f64::from(frame.life), 0.272, 1e-5),
        "life {}",
        frame.life
    );
    assert!(!frame.danger);
    assert!(frame.finished);
}

#[test]
fn sm5_replay() {
    let song = fixture_song();
    let run = run(presets::sm5(), &song, 0.0, 0.0);
    assert_eq!(sequence(&run), SM_SEQUENCE);
    // 0.25 s hold window: dropped at 5.85.
    assert!(close(
        event_time(&run, |e| e.kind == JudgeEventKind::LetGo),
        5.85,
        1e-9
    ));
    let r = run.player.results();
    assert_eq!(r.tally.taps, [12, 2, 1, 1, 1, 1]);
    assert_eq!(r.max_combo, 9);
    // Percent weights 3/2/1/0/0/0, Held 3, mine −2:
    //   12×3 + 2×2 + 1 + 2×3 − 2 = 45 of 18×3 + 3×3 = 63.
    assert!(close(r.score.percent.unwrap(), 45.0 / 63.0, 1e-12));
    // Grade weights 2/2/1/0/−4/−8, Held 6, mine −8:
    //   12×2 + 2×2 + 1 − 4 − 8 + 2×6 − 8 = 21 of 18×2 + 3×6 = 54 → 38.9% → D.
    assert!(close(r.score.grade_percent.unwrap(), 21.0 / 54.0, 1e-12));
    assert_eq!(r.grade, "D");
    assert_eq!(r.score.ex, None);
    // Life: W5 −0.040, Miss −0.080, mine −0.160, regen 5 capped at 5:
    //  0.536 → W5 0.496 (L5) → 4 swallowed → Held +0.008 = 0.504 → W1 0.512
    //  → LetGo 0.432 (L5) → W1 W1 swallowed → Mine 0.272 (L5) → Held W1 W1 swallowed
    //  → Miss 0.192 (L5) → W1 swallowed.
    let frame = run.player.frame(HostTime(13.0));
    assert!(
        close(f64::from(frame.life), 0.192, 1e-5),
        "life {}",
        frame.life
    );
    assert!(frame.danger); // below 0.2
}

#[test]
fn ddr_a_replay() {
    let song = fixture_song();
    let run = run(presets::ddr_a(), &song, 0.0, 0.0);
    assert!(run.player.ruleset().approximate);
    // Frame-table windows: +20 ms is Perfect, +40/+80 Great, +120 Good, +160 is
    // outside Good (W5 disabled) so note 6 is missed before the press lands.
    assert_eq!(
        sequence(&run),
        [
            "W1@0", "W2@1", "W3@2", "W3@3", "W1@4", "W4@5", "Miss@6", "W2@7", "W1@8", "W1@9",
            "W1@10", "Held@9", "W1@11", "LetGo@11", "W1@12", "W1@13", "Mine@14", "Held@13",
            "W1@16", "W1@17", "Miss@18", "W1@19",
        ]
    );
    assert!(close(
        event_time(&run, |e| e.note_index == Some(6)),
        2.5 + 0.141667,
        1e-9
    ));
    let r = run.player.results();
    // Per-row tally: the jump (W1 + W4) is one Good. 17 rows:
    //   Marvelous ×10, Perfect ×2 (1, 7), Great ×2 (2, 3), Good ×1, Miss ×2 (6, 18).
    assert_eq!(r.tally.taps, [10, 2, 2, 1, 0, 2]);
    assert_eq!((r.tally.held, r.tally.let_go, r.tally.mine_hit), (2, 1, 1));
    // Combo: Good keeps it (5 after the jump), Miss@6 breaks, LetGo breaks,
    // mine breaks, O.K. does not add. Longest run = 5.
    assert_eq!(r.max_combo, 5);
    // Money: SC = 1,000,000 / (17 steps + 3 freezes) = 50,000.
    //   10 Marvelous × 50,000 + 2 O.K. × 50,000 = 600,000
    //   2 Perfect × (50,000 − 10)        =  99,980
    //   2 Great × (50,000 × 3/5 − 10)    =  59,980
    //   1 Good × (50,000 / 5 − 10)       =   9,990
    //   total                            = 769,950 → B+ (750,000–789,990)
    assert_eq!(r.score.money, Some(769_950));
    assert_eq!(r.grade, "B+");
    // EX: 10×3 + 2×2 + 2×1 + 2×3 (O.K.) = 42 of 3 × 20 = 60.
    assert_eq!((r.score.ex, r.score.max_ex), (Some(42), Some(60)));
    // Fixed-percent gauge: +0.4% M/P, +0.2% G, 0 Good, −4.8% miss (never two in a row):
    //  0.5 +.004 +.004 +.002 +.002 +0 = 0.512; Miss → 0.464; W2 W1 W1 W1 → 0.480;
    //  OK → 0.484; W1 → 0.488; NG → 0.440; W1 W1 → 0.448; mine → 0.400; OK → 0.404;
    //  W1 W1 → 0.412; Miss → 0.364; W1 → 0.368.
    let frame = run.player.frame(HostTime(13.0));
    assert!(
        close(f64::from(frame.life), 0.368, 1e-5),
        "life {}",
        frame.life
    );
    assert_eq!(r.full_combo, FullCombo::None);
}

#[test]
fn latency_and_audio_offset_shift_inputs_identically() {
    let song = fixture_song();
    let base = run(presets::itg(), &song, 0.0, 0.0);
    let shifted = run(presets::itg(), &song, 0.1, 0.02);
    assert_eq!(sequence(&base), sequence(&shifted));
    let (a, b) = (base.player.results(), shifted.player.results());
    assert_eq!(a.tally, b.tally);
    assert_eq!(a.score, b.score);
    for (x, y) in base.events.iter().zip(&shifted.events) {
        assert!(close(x.song_time, y.song_time, 1e-9));
    }
}

#[test]
fn replays_are_deterministic() {
    let song = fixture_song();
    let a = run(presets::ddr_a(), &song, 0.0, 0.0);
    let b = run(presets::ddr_a(), &song, 0.0, 0.0);
    assert_eq!(a.events, b.events);
    assert!(a.player.results() == b.player.results());
    assert_eq!(a.player.frame(HostTime(5.0)), b.player.frame(HostTime(5.0)));
}

/// Delta of the first tap judgement produced by one press on note 0 at host
/// time `press_host`, under the given offsets.
fn press_delta(judge_offset: f64, audio_offset: f64, press_host: f64) -> f64 {
    let song = fixture_song();
    let mut ruleset = presets::itg();
    ruleset.judge.judge_offset = judge_offset;
    let options = PlayOptions {
        clock: ClockOptions {
            audio_offset,
            ..ClockOptions::default()
        },
        ..PlayOptions::default()
    };
    let mut player = Player::new(&song, 0, ruleset, options);
    player.clock_sample(ClockSample::default());
    player.start(0.0, 0.0);
    player.update(HostTime(0.0));
    let ev = player.input(InputEvent {
        lane: 0,
        pressed: true,
        host_time: HostTime(press_host),
    });
    match ev[0].kind {
        JudgeEventKind::Tap(_, delta) => delta,
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn judge_offset_and_audio_offset_share_a_sign_convention() {
    // Note 0 is at 0.0 s; the press lands 20 ms after it on the host clock.
    assert!(close(press_delta(0.0, 0.0, 0.02), 0.02, 1e-9));
    // Positive JUDGMENT TIMING: the game expects you later, so the same press
    // is judged 20 ms earlier → exact.
    assert!(close(press_delta(0.02, 0.0, 0.02), 0.0, 1e-9));
    assert!(close(press_delta(-0.02, 0.0, 0.02), 0.04, 1e-9));
    // Positive audio offset: you hear (and hit) 20 ms later → same effect.
    assert!(close(press_delta(0.0, 0.02, 0.02), 0.0, 1e-9));
    assert!(close(press_delta(0.0, -0.02, 0.02), 0.04, 1e-9));
}

#[test]
fn frames_keep_notes_slightly_past_the_receptor() {
    let song = fixture_song();
    let mut ruleset = presets::itg();
    ruleset.fail = FailPolicy::Off;
    let mut player = Player::new(&song, 0, ruleset, PlayOptions::default());
    player.clock_sample(ClockSample::default());
    player.start(0.0, 0.0);
    // Nothing pressed: at beat 1.5 note 0 (beat 0) is 1.5 arrows below the
    // receptor and still drawn; at beat 2.5 it is 2.5 below and culled.
    let frame = player.frame(HostTime(0.75));
    let n0 = frame.notes.iter().find(|n| n.lane == 0).unwrap();
    assert!(close(f64::from(n0.y), -1.5, 1e-6));
    let frame = player.frame(HostTime(1.25));
    assert!(frame.notes.iter().all(|n| n.y >= -2.0));
    assert!(
        frame
            .notes
            .iter()
            .any(|n| n.lane == 1 && close(f64::from(n.y), -1.5, 1e-6))
    );
}

#[test]
fn fail_policies() {
    let notes = (0..20)
        .map(|i| Note::new(b(i), (i % 4) as u8, NoteKind::Tap))
        .collect();
    let song = song(TimingMap::constant(120.0, 0.0), notes);
    let drive = |ruleset: Ruleset| {
        let mut player = Player::new(&song, 0, ruleset, PlayOptions::default());
        player.clock_sample(ClockSample::default());
        player.start(0.0, 0.0);
        let mut misses = 0;
        for step in 0..=1200 {
            misses += player.update(HostTime(f64::from(step) * 0.01)).len();
            if player.finished() {
                break;
            }
        }
        (player, misses)
    };
    // ImmediateContinue: marked failed at the fifth miss, keeps scoring to the end.
    let mut r = presets::itg();
    r.fail = FailPolicy::ImmediateContinue;
    let (p, misses) = drive(r);
    assert!(p.failed() && p.finished());
    assert_eq!(misses, 20);
    assert_eq!(p.tally().count(Judgement::Miss), 20);
    assert_eq!(p.results().grade, "F");
    // EndOfSong: evaluated once, at the end.
    let mut r = presets::itg();
    r.fail = FailPolicy::EndOfSong { min_life: 0.5 };
    let (p, misses) = drive(r);
    assert!(p.failed() && p.finished());
    assert_eq!(misses, 20);
    let mut r = presets::itg();
    r.fail = FailPolicy::EndOfSong { min_life: 0.0 };
    let (p, _) = drive(r);
    assert!(!p.failed());
    // Off: never fails.
    let mut r = presets::itg();
    r.fail = FailPolicy::Off;
    let (p, misses) = drive(r);
    assert!(!p.failed() && p.finished());
    assert_eq!(misses, 20);
    assert_eq!(p.results().grade, "D");
}

#[test]
fn immediate_fail_ends_the_play() {
    let notes = (0..20)
        .map(|i| Note::new(b(i), (i % 4) as u8, NoteKind::Tap))
        .collect();
    let song = song(TimingMap::constant(120.0, 0.0), notes);
    let mut player = Player::new(&song, 0, presets::itg(), PlayOptions::default());
    player.clock_sample(ClockSample::default());
    player.start(0.0, 0.0);
    let mut misses = 0;
    for step in 0..=1200 {
        let ev = player.update(HostTime(f64::from(step) * 0.01));
        misses += ev.len();
        if player.finished() {
            break;
        }
    }
    // Five misses at −0.100 each drain 0.5 to zero.
    assert_eq!(misses, 5);
    assert!(player.failed());
    assert!(player.finished());
    let r = player.results();
    assert_eq!(r.grade, "F");
    assert_eq!(r.tally.count(Judgement::Miss), 5);
    assert!(
        player
            .input(InputEvent {
                lane: 0,
                pressed: true,
                host_time: HostTime(3.0),
            })
            .is_empty()
    );
}

#[test]
fn frame_reflects_note_states() {
    let song = fixture_song();
    // Notes 1–8 are deliberately missed; keep the play alive.
    let mut ruleset = presets::itg();
    ruleset.fail = FailPolicy::Off;
    let mut player = Player::new(&song, 0, ruleset, PlayOptions::default());
    player.clock_sample(ClockSample::default());
    player.start(0.0, 0.0);
    player.update(HostTime(0.0));
    player.input(InputEvent {
        lane: 0,
        pressed: true,
        host_time: HostTime(0.0),
    });
    let frame = player.frame(HostTime(0.0));
    assert_eq!(frame.lanes, 4);
    assert!(frame.receptors[0].pressed);
    assert_eq!(frame.receptors[0].flash.map(|f| f.0), Some(Judgement::W1));
    assert_eq!(frame.judgement.map(|j| j.judgement), Some(Judgement::W1));
    assert_eq!(frame.combo, 1);
    // The hit tap is gone; note 1 (beat 1) sits one arrow height up at x1.
    assert!(!frame.notes.iter().any(|n| n.lane == 0 && n.y.abs() < 1e-6));
    let n1 = frame.notes.iter().find(|n| n.lane == 1).unwrap();
    assert!(close(f64::from(n1.y), 1.0, 1e-6));
    assert_eq!(n1.kind, SpriteKind::Tap);
    // Only notes within 12 arrow heights are included (beat 12 and below).
    assert!(frame.notes.iter().all(|n| n.y <= 12.0));

    // Hold 9 (beats 8–10) active at 4.4 s: head pinned to the receptor.
    player.input(InputEvent {
        lane: 0,
        pressed: false,
        host_time: HostTime(0.05),
    });
    player.update(HostTime(3.99));
    player.input(InputEvent {
        lane: 1,
        pressed: true,
        host_time: HostTime(4.0),
    });
    player.update(HostTime(4.4));
    let frame = player.frame(HostTime(4.4));
    let hold = frame
        .notes
        .iter()
        .find(|n| matches!(n.kind, SpriteKind::HoldHead { .. }) && n.lane == 1)
        .unwrap();
    assert_eq!(hold.y, 0.0);
    match hold.kind {
        SpriteKind::HoldHead {
            tail_y,
            active,
            dropped,
        } => {
            assert!(active && !dropped);
            assert!(close(f64::from(tail_y), 1.2, 1e-6)); // beat 10 − beat 8.8
        }
        _ => unreachable!(),
    }
    // Released: it drops 0.32 s later and greys out.
    player.input(InputEvent {
        lane: 1,
        pressed: false,
        host_time: HostTime(4.5),
    });
    player.update(HostTime(4.9));
    let frame = player.frame(HostTime(4.9));
    let hold = frame
        .notes
        .iter()
        .find(|n| matches!(n.kind, SpriteKind::HoldHead { .. }) && n.lane == 1)
        .unwrap();
    assert!(matches!(
        hold.kind,
        SpriteKind::HoldHead {
            active: false,
            dropped: true,
            ..
        }
    ));
    assert!(hold.y < 0.0);
}

#[test]
fn note_y_is_zero_at_judged_time_with_bpm_change_and_stop() {
    let mut timing = TimingMap::constant(100.0, -0.25);
    timing.bpms.push(BpmSegment {
        tick: b(3),
        bpm: 180.0,
    });
    timing.stops.push(StopSegment {
        tick: b(5),
        seconds: 0.7,
    });
    timing.tidy();
    let notes: Vec<Note> = (0..9)
        .map(|i| Note::new(b(i), (i % 4) as u8, NoteKind::Tap))
        .collect();
    let song = song(timing.clone(), notes.clone());
    for n in &notes {
        let at = timing.seconds_at(n.tick);
        assert!(
            note_y(ScrollOptions::default(), &timing, n.tick, at).abs() < 1e-5,
            "tick {:?}",
            n.tick
        );
    }
    // Through the player: the sprite of each note is on the receptor when it
    // is due (zero latency, no visual offset), including during the stop.
    let mut player = Player::new(&song, 0, presets::sm5(), PlayOptions::default());
    player.clock_sample(ClockSample::default());
    player.start(0.0, 0.0);
    for n in &notes {
        let at = timing.seconds_at(n.tick);
        let frame = player.frame(HostTime(at));
        let sprite = frame
            .notes
            .iter()
            .find(|s| s.lane == n.lane && s.y.abs() < 1e-5)
            .unwrap_or_else(|| panic!("no sprite on the receptor at tick {:?}", n.tick));
        assert_eq!(sprite.kind, SpriteKind::Tap);
    }
    let in_stop = player.frame(HostTime(timing.seconds_at(b(5)) + 0.5));
    assert!(
        in_stop
            .notes
            .iter()
            .any(|s| s.lane == 1 && s.y.abs() < 1e-5)
    );
}

/// A hold whose head is inside the visible range but whose tail is beyond it
/// must still be drawn (it used to pop in only once it fit entirely).
#[test]
fn long_hold_is_drawn_while_its_tail_is_off_screen() {
    use ddi_chart::{
        Chart, Difficulty, DisplayBpm, Note, NoteKind, Song, SourceFormat, SourceInfo, Tick,
        TimingMap,
    };
    use ddi_engine::frame::SpriteKind;
    use ddi_engine::player::{PlayOptions, Player};
    use ddi_engine::scroll::{ScrollAction, ScrollOptions, SpeedMod};
    use ddi_platform::{ClockSample, HostTime};

    let timing = TimingMap::constant(60.0, 0.0); // 1 beat per second
    let song = Song {
        title: String::new(),
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
        timing,
        display_bpm: DisplayBpm::Actual,
        charts: vec![Chart {
            layout: "dance-single".into(),
            difficulty: Difficulty::Beginner,
            meter: 1,
            name: String::new(),
            description: String::new(),
            credit: String::new(),
            // Head at beat 8, tail at beat 30: 22 beats long.
            notes: vec![Note::new(
                Tick::from_beats(8),
                0,
                NoteKind::HoldHead {
                    end: Tick::from_beats(30),
                },
            )],
            timing: None,
            display_bpm: None,
            danoni: None,
        }],
        effects: Vec::new(),
        keysounds: Vec::new(),
        layouts: Vec::new(),
        source: SourceInfo {
            format: SourceFormat::Other("test".into()),
            unknown_tags: Vec::new(),
        },
    };
    let options = PlayOptions {
        scroll: ScrollOptions {
            speed: SpeedMod::XMod(1.0),
            reverse: false,
            scroll_action: ScrollAction::Normal,
        },
        visible_range: 12.0,
        ..PlayOptions::default()
    };
    let mut player = Player::new(&song, 0, ddi_engine::rules::presets::itg(), options);
    player.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: 0.0,
    });
    player.start(0.0, 0.0);
    // At song time 2 s the head is 6 beats away (visible) and the tail 28
    // beats away (far beyond the 12-beat range).
    let frame = player.frame(HostTime(2.0));
    let hold = frame
        .notes
        .iter()
        .find(|n| matches!(n.kind, SpriteKind::HoldHead { .. }))
        .expect("hold head must be drawn while its tail is off screen");
    assert!((hold.y - 6.0).abs() < 1e-6, "head y {}", hold.y);
    if let SpriteKind::HoldHead { tail_y, .. } = hold.kind {
        assert!((tail_y - 28.0).abs() < 1e-6, "tail y {tail_y}");
    }
    // Long before the head is in range nothing is drawn.
    let early = player.frame(HostTime(-10.0));
    assert!(early.notes.is_empty());
}

/// Receptor snap: within half a frame of its arrival a pending note is drawn
/// exactly on the receptor; outside that it is where the scroll puts it.
#[test]
fn receptor_snap_lands_the_note_in_its_closest_frame() {
    use ddi_chart::{
        Chart, Difficulty, DisplayBpm, Note, NoteKind, Song, SourceFormat, SourceInfo, Tick,
        TimingMap,
    };
    use ddi_engine::player::{PlayOptions, Player};
    use ddi_engine::scroll::{ScrollAction, ScrollOptions, SpeedMod};
    use ddi_platform::{ClockSample, HostTime};

    let song = Song {
        title: String::new(),
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
        timing: TimingMap::constant(60.0, 0.0),
        display_bpm: DisplayBpm::Actual,
        charts: vec![Chart {
            layout: "dance-single".into(),
            difficulty: Difficulty::Beginner,
            meter: 1,
            name: String::new(),
            description: String::new(),
            credit: String::new(),
            notes: vec![Note::new(Tick::from_beats(4), 0, NoteKind::Tap)], // at 4.0 s
            timing: None,
            display_bpm: None,
            danoni: None,
        }],
        effects: Vec::new(),
        keysounds: Vec::new(),
        layouts: Vec::new(),
        source: SourceInfo {
            format: SourceFormat::Other("test".into()),
            unknown_tags: Vec::new(),
        },
    };
    let make = |snap: bool| {
        let options = PlayOptions {
            scroll: ScrollOptions {
                speed: SpeedMod::XMod(1.0),
                reverse: false,
                scroll_action: ScrollAction::Normal,
            },
            receptor_snap: snap,
            ..PlayOptions::default()
        };
        let mut p = Player::new(&song, 0, ddi_engine::rules::presets::itg(), options);
        p.clock_sample(ClockSample {
            context_time: 0.0,
            host_time: HostTime(0.0),
            output_latency: 0.0,
        });
        p.start(0.0, 0.0);
        p.set_frame_interval(1.0 / 60.0);
        p
    };
    // 5 ms before arrival (inside half a 16.7 ms frame): snapped to 0.
    let y = make(true).frame(HostTime(3.995)).notes[0].y;
    assert_eq!(y, 0.0);
    // Same instant without snap: 5 ms of travel at 1 beat/s = 0.005 arrows.
    let y = make(false).frame(HostTime(3.995)).notes[0].y;
    assert!((y - 0.005).abs() < 1e-6, "{y}");
    // 12 ms before arrival (outside half a frame): not snapped.
    let y = make(true).frame(HostTime(3.988)).notes[0].y;
    assert!((y - 0.012).abs() < 1e-6, "{y}");
}

// --- Play options: transforms, scroll actions, appearance -----------------

use ddi_engine::autoplay;
use ddi_engine::{Appearance, ScrollAction, SpeedMod, TimingCut, TransformOptions, Turn};

const ACTIONS: [ScrollAction; 4] = [
    ScrollAction::Normal,
    ScrollAction::Boost,
    ScrollAction::Brake,
    ScrollAction::Wave,
];

const APPEARANCES: [Appearance; 5] = [
    Appearance::Visible,
    Appearance::Hidden,
    Appearance::Sudden,
    Appearance::HiddenSudden,
    Appearance::Stealth,
];

fn ms_script() -> Vec<(f64, u8, bool)> {
    script()
        .into_iter()
        .map(|(t, lane, pressed)| (f64::from(t) / 1000.0, lane, pressed))
        .collect()
}

#[test]
fn scroll_actions_and_appearance_never_change_judgements() {
    let song = fixture_song();
    let baseline = run(presets::itg(), &song, 0.0, 0.0);
    for speed in [
        SpeedMod::XMod(1.0),
        SpeedMod::XMod(3.5),
        SpeedMod::CMod(450.0),
    ] {
        for scroll_action in ACTIONS {
            for appearance in APPEARANCES {
                for reverse in [false, true] {
                    let options = PlayOptions {
                        scroll: ScrollOptions {
                            speed,
                            reverse,
                            scroll_action,
                        },
                        appearance,
                        ..PlayOptions::default()
                    };
                    let r = run_script(presets::itg(), &song, options, 0.0, &ms_script());
                    assert_eq!(
                        r.events, baseline.events,
                        "{speed:?} {scroll_action:?} {appearance:?} reverse {reverse}"
                    );
                    assert_eq!(r.player.results().tally, baseline.player.results().tally);
                }
            }
        }
    }
}

/// `(kind, tick, lane, song time)` of every event, sorted, for comparing
/// plays whose note indices differ.
fn event_set(events: &[JudgeEvent], lane_of: impl Fn(u8) -> u8) -> Vec<String> {
    let mut v: Vec<String> = events
        .iter()
        .map(|e| {
            let kind = match e.kind {
                JudgeEventKind::Tap(j, d) => format!("{j:?}{d:.9}"),
                k => format!("{k:?}"),
            };
            format!("{:.9} {} {} {kind}", e.song_time, e.tick.0, lane_of(e.lane))
        })
        .collect();
    v.sort();
    v
}

#[test]
fn turns_only_move_lanes() {
    let song = fixture_song();
    let baseline = run(presets::itg(), &song, 0.0, 0.0);
    let turns = [
        (Turn::Mirror, 0),
        (Turn::Left, 0),
        (Turn::Right, 0),
        (Turn::Shuffle, 1),
        (Turn::Shuffle, 2),
        (Turn::Shuffle, 0xDD1),
    ];
    for (turn, seed) in turns {
        let options = PlayOptions {
            transform: TransformOptions {
                turn,
                ..Default::default()
            },
            seed,
            ..PlayOptions::default()
        };
        let probe = Player::new(&song, 0, presets::itg(), options);
        let take_from = probe.lane_map().to_vec();
        let mut dest = [0u8; 4];
        for (new, &old) in take_from.iter().enumerate() {
            dest[usize::from(old)] = new as u8;
        }
        // The player presses the lanes where the notes went.
        let script: Vec<(f64, u8, bool)> = ms_script()
            .into_iter()
            .map(|(t, lane, p)| (t, dest[usize::from(lane)], p))
            .collect();
        let r = run_script(presets::itg(), &song, options, 0.0, &script);
        assert_eq!(
            event_set(&r.events, |l| l),
            event_set(&baseline.events, |l| dest[usize::from(l)]),
            "{turn:?} seed {seed}"
        );
        let res = r.player.results();
        assert_eq!(res.tally, baseline.player.results().tally);
        assert_eq!(res.lane_map, take_from);
        assert!(!res.assist);
    }
    // The same seed gives the same shuffle; the seed matters.
    let map = |seed| {
        Player::new(
            &song,
            0,
            presets::itg(),
            PlayOptions {
                transform: TransformOptions {
                    turn: Turn::Shuffle,
                    ..Default::default()
                },
                seed,
                ..PlayOptions::default()
            },
        )
        .lane_map()
        .to_vec()
    };
    assert_eq!(map(42), map(42));
    assert!(
        (0..20)
            .map(map)
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1
    );
}

/// 120 BPM; ticks in 48ths of a beat.
///
/// | tick | lane | kind |
/// |------|------|------|
/// | 0    | 0    | tap |
/// | 24   | 1    | tap (8th) |
/// | 36   | 2    | tap (16th) |
/// | 48   | 0, 3 | jump |
/// | 64   | 1    | tap (12th) |
/// | 96   | 2    | hold → 144 |
/// | 120  | 0    | tap (8th, during the hold) |
/// | 168  | 3    | mine (8th) |
/// | 192  | 1    | roll → 264 |
/// | 216  | 3    | tap (8th, during the roll) |
/// | 288  | 0    | lift |
/// | 300  | 2    | tap (16th) |
/// | 336  | 0, 1, 2 | hand |
/// | 360  | 3    | fake |
/// | 384  | 3    | tap |
fn dense_song() -> Song {
    let t = |tick: i64, lane: u8, kind| Note::new(Tick(tick), lane, kind);
    let notes = vec![
        t(0, 0, NoteKind::Tap),
        t(24, 1, NoteKind::Tap),
        t(36, 2, NoteKind::Tap),
        t(48, 0, NoteKind::Tap),
        t(48, 3, NoteKind::Tap),
        t(64, 1, NoteKind::Tap),
        t(96, 2, NoteKind::HoldHead { end: Tick(144) }),
        t(120, 0, NoteKind::Tap),
        t(168, 3, NoteKind::Mine),
        t(192, 1, NoteKind::RollHead { end: Tick(264) }),
        t(216, 3, NoteKind::Tap),
        t(288, 0, NoteKind::Lift),
        t(300, 2, NoteKind::Tap),
        t(336, 0, NoteKind::Tap),
        t(336, 1, NoteKind::Tap),
        t(336, 2, NoteKind::Tap),
        t(360, 3, NoteKind::Fake),
        t(384, 3, NoteKind::Tap),
    ];
    song(TimingMap::constant(120.0, 0.0), notes)
}

fn cut(cut: TimingCut, no_jumps: bool, no_holds: bool) -> TransformOptions {
    TransformOptions {
        turn: Turn::Off,
        cut,
        no_jumps,
        no_holds,
    }
}

#[test]
fn cuts_change_the_counted_chart_and_mark_an_assist() {
    let song = dense_song();
    let ctx = |transform| {
        let p = Player::new(
            &song,
            0,
            presets::itg(),
            PlayOptions {
                transform,
                ..PlayOptions::default()
            },
        );
        let c = p.score_ctx();
        (c.steps, c.holds, c.mines)
    };
    // Untransformed: 16 tap-like notes, 2 holds/rolls, 1 mine.
    assert_eq!(ctx(TransformOptions::default()), (16, 2, 1));
    // Quarters: ticks 0, 48 ×2, 96, 192, 288, 336 ×3, 384 = 10; the mine is off-beat.
    assert_eq!(ctx(cut(TimingCut::Quarters, false, false)), (10, 2, 0));
    // Eighths add 24, 120, 168 (mine), 216.
    assert_eq!(ctx(cut(TimingCut::Eighths, false, false)), (13, 2, 1));
    // No holds: same steps, no holds.
    assert_eq!(ctx(cut(TimingCut::Off, false, true)), (16, 0, 1));
    // No jumps: the jump and the hand lose 1 and 2; the taps at 120 (during
    // the hold) and 216 (during the roll) go.
    assert_eq!(ctx(cut(TimingCut::Off, true, false)), (11, 2, 1));

    let opts = PlayOptions {
        transform: cut(TimingCut::Quarters, false, false),
        ..PlayOptions::default()
    };
    let mut ruleset = presets::itg();
    ruleset.fail = FailPolicy::Off;
    let r = run_script(ruleset, &song, opts, 0.0, &[]).player.results();
    assert!(r.assist);
    assert_eq!(r.transform.cut, TimingCut::Quarters);
    // Every remaining step was missed: 10 misses, nothing for removed notes.
    assert_eq!(r.tally.taps[5], 10);
}

/// Plays `song` with the engine's autoplay script under `options`.
fn autoplay_run(ruleset: fn() -> Ruleset, song: &Song, options: PlayOptions) -> Run {
    let probe = Player::new(song, 0, ruleset(), options);
    let mut script: Vec<(f64, u8, bool)> = autoplay::script(probe.judge().notes(), 0.0)
        .into_iter()
        .flat_map(|p| [(p.press, p.lane, true), (p.release, p.lane, false)])
        .collect();
    script.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
    run_script(ruleset(), song, options, 0.0, &script)
}

#[test]
fn autoplay_is_perfect_with_every_option() {
    let cuts = [
        TransformOptions::default(),
        cut(TimingCut::Quarters, false, false),
        cut(TimingCut::Eighths, true, true),
        cut(TimingCut::Off, true, false),
    ];
    let turns = [
        (Turn::Off, 0),
        (Turn::Mirror, 0),
        (Turn::Left, 0),
        (Turn::Right, 0),
        (Turn::Shuffle, 7),
    ];
    for song in [fixture_song(), dense_song()] {
        for ruleset in [
            presets::itg as fn() -> Ruleset,
            presets::sm5,
            presets::ddr_a,
        ] {
            for transform in cuts {
                for (turn, seed) in turns {
                    for (i, scroll_action) in ACTIONS.into_iter().enumerate() {
                        // Pair each scroll action with every appearance once
                        // across the turns instead of the full product.
                        let appearance = APPEARANCES[(i + seed as usize + turn as usize) % 5];
                        let options = PlayOptions {
                            scroll: ScrollOptions {
                                scroll_action,
                                ..ScrollOptions::default()
                            },
                            transform: TransformOptions { turn, ..transform },
                            appearance,
                            seed,
                            ..PlayOptions::default()
                        };
                        let run = autoplay_run(ruleset, &song, options);
                        let ctx = run.player.score_ctx();
                        let r = run.player.results();
                        let label = format!(
                            "{} {transform:?} {turn:?} {scroll_action:?} {appearance:?}",
                            run.player.ruleset().names.tiers[0]
                        );
                        let rows_or_notes: u32 = r.tally.taps.iter().sum();
                        assert_eq!(r.tally.taps[0], rows_or_notes, "{label}: {:?}", r.tally);
                        assert_eq!(rows_or_notes, ctx.steps, "{label}");
                        assert_eq!(r.tally.held, ctx.holds, "{label} {:?}", sequence(&run));
                        assert_eq!(
                            (r.tally.let_go, r.tally.mine_hit, r.tally.boo),
                            (0, 0, 0),
                            "{label}"
                        );
                        assert_eq!(r.full_combo, FullCombo::MarvelousFC, "{label}");
                        assert_eq!(r.assist, transform.is_assist(), "{label}");
                        assert!(run.player.finished() && !run.player.failed(), "{label}");
                    }
                }
            }
        }
    }
}

#[test]
fn hidden_and_sudden_set_note_alpha_in_frames() {
    let song = fixture_song();
    let frame_at = |appearance, t: f64| {
        let mut p = Player::new(
            &song,
            0,
            presets::itg(),
            PlayOptions {
                appearance,
                ..PlayOptions::default()
            },
        );
        p.clock_sample(ClockSample {
            context_time: 0.0,
            host_time: HostTime(0.0),
            output_latency: 0.0,
        });
        p.start(0.0, 0.0);
        p.frame(HostTime(t))
    };
    // At x1 and 120 BPM, a note one beat (0.5 s) away is one arrow up.
    let lane_alpha = |f: &ddi_engine::Frame, y: f32| {
        f.notes
            .iter()
            .find(|n| (n.y - y).abs() < 1e-3)
            .map(|n| n.alpha)
            .expect("note at y")
    };
    let f = frame_at(Appearance::Hidden, 0.0);
    assert_eq!(f.appearance, Appearance::Hidden);
    assert_eq!(lane_alpha(&f, 1.0), 0.0);
    assert_eq!(lane_alpha(&f, 5.0), 1.0);
    let f = frame_at(Appearance::Sudden, 0.0);
    assert_eq!(lane_alpha(&f, 1.0), 1.0);
    assert_eq!(lane_alpha(&f, 5.0), 0.0);
    let f = frame_at(Appearance::Visible, 0.0);
    assert!(f.notes.iter().all(|n| n.alpha == 1.0 && n.glow == 0.0));
}

/// Solo, double and the Dancing☆Onigiri key modes the game plays.
fn test_layouts() -> Vec<Layout> {
    let mut v = vec![Layout::dance_solo(), Layout::dance_double()];
    for mode in ddi_chart::PLAYED_DANONI_MODES {
        v.push(Layout::builtin(&format!("danoni-{mode}")).unwrap());
    }
    v
}

/// A chart over every lane of a built-in layout at 150 BPM: a run through
/// all lanes, jumps (on a diagonal pair for solo, across the two pads for
/// double), a hold under taps on another lane, a roll, a mine, a lift and
/// a hand.
fn layout_song(layout: &Layout) -> Song {
    let n = layout.lane_count() as u8;
    let t = |tick: i64, lane: u8, kind| Note::new(Tick(tick), lane, kind);
    let mut notes: Vec<Note> = (0..n)
        .map(|l| t(i64::from(l) * 12, l, NoteKind::Tap))
        .collect();
    let base = i64::from(n) * 12 + 48;
    notes.extend([
        t(base, 1, NoteKind::Tap),
        t(base, n - 2, NoteKind::Tap),
        t(base + 24, n / 2 - 1, NoteKind::Tap),
        t(base + 24, n / 2, NoteKind::Tap),
        t(
            base + 48,
            0,
            NoteKind::HoldHead {
                end: Tick(base + 144),
            },
        ),
        t(base + 72, n - 1, NoteKind::Tap),
        t(base + 96, n / 2, NoteKind::Tap),
        t(base + 120, n - 1, NoteKind::Mine),
        t(
            base + 168,
            1,
            NoteKind::RollHead {
                end: Tick(base + 240),
            },
        ),
        t(base + 192, n - 2, NoteKind::Tap),
        t(base + 264, n - 1, NoteKind::Lift),
        t(base + 288, 0, NoteKind::Tap),
        t(base + 288, 2, NoteKind::Tap),
        t(base + 288, n - 1, NoteKind::Tap),
    ]);
    notes.sort_by_key(|n| (n.tick, n.lane));
    let mut s = song(TimingMap::constant(150.0, 0.0), notes);
    s.charts[0].layout = layout.id.clone();
    s
}

#[test]
fn every_layout_autoplays_perfectly_under_every_turn() {
    for layout in test_layouts() {
        let song = layout_song(&layout);
        for ruleset in [
            presets::itg as fn() -> Ruleset,
            presets::sm5,
            presets::ddr_a,
        ] {
            for (turn, seed) in [
                (Turn::Off, 0),
                (Turn::Mirror, 0),
                (Turn::Left, 0),
                (Turn::Right, 0),
                (Turn::Shuffle, 3),
                (Turn::Shuffle, 0xD0B1E),
            ] {
                for transform in [
                    TransformOptions::default(),
                    cut(TimingCut::Eighths, true, true),
                ] {
                    let options = PlayOptions {
                        transform: TransformOptions { turn, ..transform },
                        seed,
                        ..PlayOptions::default()
                    };
                    let run = autoplay_run(ruleset, &song, options);
                    let label = format!("{} {turn:?} {transform:?}", layout.id);
                    let ctx = run.player.score_ctx();
                    let r = run.player.results();
                    assert_eq!(usize::from(run.player.lanes()), layout.lane_count());
                    assert_eq!(r.tally.taps[0], ctx.steps, "{label}: {:?}", r.tally);
                    assert_eq!(r.tally.taps.iter().sum::<u32>(), ctx.steps, "{label}");
                    assert_eq!(r.tally.held, ctx.holds, "{label}");
                    assert_eq!(
                        (r.tally.let_go, r.tally.mine_hit, r.tally.boo),
                        (0, 0, 0),
                        "{label}"
                    );
                    assert_eq!(r.full_combo, FullCombo::MarvelousFC, "{label}");
                    assert!(run.player.finished() && !run.player.failed(), "{label}");
                    if transform == TransformOptions::default() {
                        assert!(ctx.steps > u32::from(layout.lane_count() as u8), "{label}");
                        assert_eq!(ctx.holds, 2, "{label}");
                    }
                }
            }
        }
    }
}

#[test]
fn turns_move_lanes_by_layout_tables_and_groups() {
    for layout in test_layouts() {
        let song = layout_song(&layout);
        let baseline = autoplay_run(presets::itg, &song, PlayOptions::default());
        for (turn, seed) in [
            (Turn::Mirror, 0),
            (Turn::Left, 0),
            (Turn::Right, 0),
            (Turn::Shuffle, 11),
        ] {
            let options = PlayOptions {
                transform: TransformOptions {
                    turn,
                    ..Default::default()
                },
                seed,
                ..PlayOptions::default()
            };
            let run = autoplay_run(presets::itg, &song, options);
            let take_from = run.player.lane_map().to_vec();
            if let (Turn::Left, Some(table)) = (turn, &layout.turn_left) {
                assert_eq!(&take_from, table, "{}", layout.id);
            }
            // Mirror and Shuffle keep every lane within its shuffle group.
            for (new, &old) in take_from.iter().enumerate() {
                assert_eq!(
                    layout.lanes[new].shuffle_group,
                    layout.lanes[usize::from(old)].shuffle_group,
                    "{} {turn:?}",
                    layout.id
                );
            }
            let mut dest = vec![0u8; take_from.len()];
            for (new, &old) in take_from.iter().enumerate() {
                dest[usize::from(old)] = new as u8;
            }
            assert_eq!(
                event_set(&run.events, |l| l),
                event_set(&baseline.events, |l| dest[usize::from(l)]),
                "{} {turn:?}",
                layout.id
            );
        }
    }
}
