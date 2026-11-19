//! What a stage's battle shows and decides, frame by frame, pinned by a
//! digest of everything it is read for (the player's links, everyone's
//! links as a watcher sees them, the warnings, the bars, the views, the
//! settlement, the outcome): written before the battle's parallel copies
//! (its three settle paths, its three link builders, its seat lookups)
//! were merged, so that the merge is shown to change none of it.
//!
//! **Reading a pin** (round M; the review's gaps 9 and 10). A digest
//! says only that something moved. Beside each one, in Git, is the part
//! of its trace a person can read: `pinned.txt` (practice) and
//! `live_pinned.txt` (an online room's stage) hold, a stage, its digest
//! and its [`readable`] lines (who ranked where and how they finished,
//! each competitor's score, grade, life, combo, attacks and chart, what
//! the battle told and when, how many frames the trace has), and the
//! tests compare the whole file, so a re-recording's diff shows what
//! changed in words, not forty opaque numbers.
//!
//! - A pin moved: the test fails saying which lines differ. Where the
//!   change is meant, record again with
//!   `DDI_PIN_WRITE=1 cargo test -p ddi-session --lib as_pinned`
//!   (it rewrites both files in place) and read `git diff` of them.
//! - The whole trace of every stage, frame by frame (1 to 2 MB a
//!   stage, never kept in Git):
//!   `DDI_PIN_DUMP=<an existing folder> cargo test -p ddi-session --lib as_pinned`
//!   writes `<name>.txt` a stage there; dump before and after a change
//!   and `diff` the two folders to see the first frame that moved.

use super::*;

/// FNV-1a, 64 bits: a digest that does not move between toolchains.
pub(super) fn fnv(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// One frame of `s`'s battle as everything reading it reads it, written
/// to `out`.
pub(super) fn trace(out: &mut String, host: f64, s: &PlaySession<FakeAudio, FakeInputs>) {
    use std::fmt::Write;
    let Some(b) = s.battle() else {
        return;
    };
    let p = &s.player;
    let _ = writeln!(
        out,
        "{host:.4} links {:?} seen {:?} kept {:?} incoming {:?} for {:?} power {:?} waiting {:?} policy {:?}",
        b.links(p),
        b.seen_links(),
        b.kept_seats(p, 2.0),
        b.incoming(p),
        (0..b.views().len()).map(|i| b.incoming_for(i)).collect::<Vec<_>>(),
        b.my_power(p),
        b.waiting(p),
        b.policy(p),
    );
    let _ = writeln!(
        out,
        "  decided {} at {:?} drawn {} settling {:?} standing {:?} span {:?} me {} goes {} playing {} own {:?} summary {:?} watch {:?}",
        b.decided(),
        b.decided_at(),
        b.drawn_by_final_attack(),
        b.settling(),
        b.last_standing(),
        b.last_standing_span(),
        b.last_standing_me(p),
        b.goes_on(),
        b.still_playing(),
        b.own_seat(),
        b.attack_summary(p),
        s.watch(host),
    );
    let _ = writeln!(out, "  views {:?}", b.views());
    // Each field as drawn (a digest a field: its summary and its frame).
    let t = s.frame(HostTime(host)).song_time;
    let scroll = p.options().scroll;
    let field = |f: &crate::battle::feed::FieldFeed, track| {
        fnv(&format!("{:?} {:?}", f.shown(t), f.frame(track, scroll, t, 8.0)))
    };
    let fields: Vec<u64> = b
        .feeds()
        .enumerate()
        .map(|(i, f)| field(f, b.track_of(i)))
        .chain([field(b.own_feed(), b.track())])
        .collect();
    let _ = writeln!(out, "  fields {fields:x?}");
}

/// The part of a stage's trace a person reads (the module's header):
/// its outcome (the ranking best first, ties joined by `=`, each
/// competitor `P<id>` with `x` where they went out and `f` where they
/// forfeited; whether it was decided early or drawn by a final attack;
/// the player's own run; each competitor's line) and what the battle
/// told, each moment at its second.
pub(super) fn readable(
    outcome: Option<&crate::battle::StageOutcome>,
    told: &[crate::announcer::battle::Told],
) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    match outcome {
        None => out.push_str("  outcome: none\n"),
        Some(o) => {
            let _ = writeln!(out, "  outcome: {}", outcome_line(o));
            for (p, d) in &o.details {
                let _ = writeln!(
                    out,
                    "    P{}: score {:?} percent {:?} grade {:?} life {} out {:?} combo {} tiers {:?} sent {:?} taken {:?} shocks {:?} chart {:?}{}",
                    p.0,
                    d.score,
                    d.percent,
                    d.grade,
                    d.life_ppm,
                    d.out_at_ms,
                    d.max_combo,
                    d.tiers,
                    d.attacks,
                    d.received,
                    d.shocks,
                    d.chart,
                    if d.harder { " (harder)" } else { "" },
                );
            }
        }
    }
    let told: Vec<String> = told
        .iter()
        .map(|t| {
            let said = match &t.said {
                Some(_) if t.spoken() => " said",
                Some(_) => " said by another line",
                None => "",
            };
            format!("{:?}@{:.2}{said}", t.event, t.at)
        })
        .collect();
    let _ = writeln!(out, "  told: {}", told.join(", "));
    out
}

/// A stage's outcome in a line ([`readable`]).
fn outcome_line(o: &crate::battle::StageOutcome) -> String {
    let ranking: Vec<String> = o
        .ranking
        .iter()
        .map(|group| {
            group
                .iter()
                .map(|f| {
                    let how = match f.finish {
                        ddi_room::Finish::Survived => "",
                        ddi_room::Finish::Eliminated => "x",
                        ddi_room::Finish::Forfeit => "f",
                        ddi_room::Finish::Failed => "F",
                    };
                    format!("P{}{how}", f.participant.0)
                })
                .collect::<Vec<_>>()
                .join(" = ")
        })
        .collect();
    format!(
        "{}{}{}{} · mine {:?} · policy {:?}",
        ranking.join(" > "),
        if o.confirmed_early { " · decided early" } else { "" },
        if o.drawn_by_final_attack { " · drawn by a final attack" } else { "" },
        if o.watched { " · watched" } else { "" },
        o.mine,
        o.targeting,
    )
}

/// A stage's pin as its file keeps it: its name, its trace's digest and
/// frames, and `read` (its [`readable`] lines).
pub(super) fn pin(name: &str, trace: &str, read: &str) -> String {
    format!(
        "== {name}\n  digest: {:#018x} ({} lines)\n{read}",
        fnv(trace),
        trace.lines().count()
    )
}

/// The pins `made` against the file `kept` (its text, and its name under
/// this folder): equal, or the lines that differ said; with
/// `DDI_PIN_WRITE=1` the file is written again instead (the module's
/// header).
pub(super) fn check_pins(made: &str, kept: &str, file: &str) {
    if std::env::var_os("DDI_PIN_WRITE").is_some() {
        let path = format!(
            "{}/src/play/tests/stage/{file}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::write(&path, made).unwrap_or_else(|e| panic!("{path}: {e}"));
        eprintln!("{path} written: read its diff");
        return;
    }
    // (A checkout may turn the file's line ends.)
    let kept = &kept.replace("\r\n", "\n");
    if made == kept {
        return;
    }
    let (m, k): (Vec<&str>, Vec<&str>) = (made.lines().collect(), kept.lines().collect());
    let mut said = 0;
    let mut stage = "";
    for i in 0..m.len().max(k.len()) {
        let (a, b) = (m.get(i).copied(), k.get(i).copied());
        if let Some(name) = a.and_then(|l| l.strip_prefix("== ")) {
            stage = name;
        }
        if a != b && said < 24 {
            said += 1;
            eprintln!(
                "{file}:{} ({stage})\n  pinned: {}\n  now:    {}",
                i + 1,
                b.unwrap_or("(nothing)"),
                a.unwrap_or("(nothing)")
            );
        }
    }
    panic!(
        "{file}: a stage's pin moved (above, the first lines that differ). Meant? Record again with DDI_PIN_WRITE=1 and read the diff; the whole traces: DDI_PIN_DUMP=<folder> (this module's header)"
    );
}

/// `s` run at 60 frames a second to its end (or host second `limit`),
/// traced every `every` frames; then its outcome and what it told. With
/// it, the part a person reads ([`readable`]).
fn traced(s: &mut PlaySession<FakeAudio, FakeInputs>, clock: &Clock, limit: f64) -> (String, String) {
    let mut out = String::new();
    let mut frame = 0u64;
    loop {
        *clock.borrow_mut() += 1.0 / 60.0;
        let host = *clock.borrow();
        let (_, event) = s.tick(HostTime(host));
        frame += 1;
        if frame.is_multiple_of(6) || event.is_some() {
            trace(&mut out, host, s);
        }
        if event.is_some() || host > limit {
            break;
        }
    }
    let outcome = s.take_stage_outcome();
    out.push_str(&format!("outcome {outcome:?}\n"));
    out.push_str(&format!("told {:?}\n", s.battle_told()));
    let read = readable(outcome.as_ref(), s.battle_told());
    (out, read)
}

/// A practice stage with attacks on the long chart (`cpus` at `level`,
/// seeded from `seed`), played with `settings` (a battery of lives
/// there: CPUs go out and stages are decided early), watched or played
/// by the player (`auto`: autoplayed; `keys` pressed): its trace and
/// what a person reads of it.
fn practice(
    song: &Song,
    settings: &Settings,
    seat: (u16, u8, u64),
    watching: bool,
    auto: bool,
    keys: &[(f64, &str, bool)],
) -> (String, String) {
    practice_on(song, settings, seat, watching, auto, keys, (ddi_room::Charts::AsPicked, LONG))
}

/// [`practice`] on chart `chart` of the song under the match's Charts
/// rule `charts` (Adaptive: a new room's default; the competitors step
/// down and up the song's ladder).
fn practice_on(
    song: &Song,
    settings: &Settings,
    (cpus, level, seed): (u16, u8, u64),
    watching: bool,
    auto: bool,
    keys: &[(f64, &str, bool)],
    (charts, chart): (ddi_room::Charts, usize),
) -> (String, String) {
    let mut room = room_seeded(1, cpus, level, seed);
    let rules = MatchRules {
        battle: ddi_room::BattlePreset::GAUGE_ATTACKS_V0.into(),
        charts,
        // Out at zero life on the easiest chart, as these traces were
        // pinned (Play on, a new room's since revision 15, has its own
        // tests).
        easiest: ddi_room::AtEasiest::Out,
        ..room.rules().clone()
    };
    room.save_rules(rules).unwrap();
    let mut now = 0;
    let order = if watching {
        begin_watching(song, &mut room, &mut now, chart)
    } else {
        begin_at(song, &mut room, &mut now, chart)
    };
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut inputs = FakeInputs::new(&clock);
    for &(at, code, pressed) in keys {
        inputs.key(at, code, pressed);
    }
    let mut config = SessionConfig::from_settings(settings, auto, None);
    config.opponents = Some(order.setup.clone());
    if !settings.stepmania.is_default() {
        config.stage_rules = Some(rules_of(settings, &order, song));
    }
    let mut s = PlaySession::start(
        song,
        order.chart.index,
        settings,
        config,
        FakeAudio::new(&clock, 0.0),
        110.0,
        inputs,
    )
    .unwrap();
    let (mut out, mut read) = traced(&mut s, &clock, 300.0);
    // (The steps the player made, where the charts adapt.)
    if !s.player.steps().is_empty() {
        let steps: Vec<String> = s
            .player
            .steps()
            .iter()
            .map(|st| format!("rung {}@{:.2}", st.rung, st.at))
            .collect();
        read.push_str(&format!("  steps: {}\n", steps.join(", ")));
    }
    // The same stage left at its prompt.
    let left = crate::battle::forfeit(
        order.setup.clone(),
        song,
        order.chart.index,
        &rules_of(settings, &order, song),
    );
    out.push_str(&format!("left {left:?}\n"));
    match &left {
        Ok(o) => read.push_str(&format!("  left at its prompt: {}\n", outcome_line(o))),
        Err(e) => read.push_str(&format!("  left at its prompt: {e}\n")),
    }
    (out, read)
}

/// Every way a practice stage is played and decided, pinned: the player
/// playing it through (their links, the warnings, an early decision, a
/// wait for the last attack, a draw); out of life and watching on (their
/// own field among the others', settled as it comes); out of life and
/// skipping (played out at once, settled quietly); quitting in it (a
/// forfeit); a spectator; and the stage left at its prompt; under DDI
/// and under a battery of lives (CPUs going out); and under Charts:
/// Adaptive, a new room's default (round M; the review's gap 10): a
/// player who presses nothing stepping down the song's ladder beside
/// CPUs who step too, an autoplayed player stepping up it, and such a
/// stage watched. Compared with `pinned.txt` (the module's header says
/// how to read a pin that moved and how to record it again).
#[test]
fn a_practice_stage_shows_and_decides_as_pinned() {
    let song = bundled("beatucada");
    let mut skip = Vec::new();
    for i in 0..400 {
        let at = f64::from(i) * 0.5;
        skip.push((at, "Enter", true));
        skip.push((at + 0.05, "Enter", false));
    }
    let quit = [
        (30.0, "Escape", true),
        (30.05, "Escape", false),
        (30.2, "Escape", true),
        (30.25, "Escape", false),
    ];
    let ddi = Settings::default();
    let battery = |lives: u8| {
        let mut s = Settings::default();
        s.stepmania.life_type = ddi_engine::rules::LifeType::Battery;
        s.stepmania.lives = lives;
        s
    };
    // Beatucada's singles: Hard (its ladder below it Medium, Easy,
    // Beginner) and Easy (Medium and Hard above it).
    let (hard, easy) = (super::step_down::HARD, EASY);
    assert_eq!(song.charts[easy].difficulty, ddi_chart::Difficulty::Easy);
    let adaptive = |chart| (ddi_room::Charts::Adaptive, chart);
    let mut traces: Vec<(String, (String, String))> = vec![
        ("played".into(), practice(&song, &ddi, (4, 3, 7), false, true, &[])),
        ("played hard".into(), practice(&song, &ddi, (3, 9, 7), false, true, &[])),
        ("out, watched on".into(), practice(&song, &ddi, (4, 3, 7), false, false, &[])),
        ("out, skipped".into(), practice(&song, &ddi, (4, 3, 7), false, false, &skip)),
        ("quit".into(), practice(&song, &ddi, (3, 3, 7), false, true, &quit)),
        ("watched".into(), practice(&song, &ddi, (4, 6, 7), true, false, &[])),
    ];
    for seed in 1..=12u64 {
        let settings = battery(1 + (seed % 5) as u8);
        let seat = (3 + (seed % 5) as u16, 2 + (seed % 3) as u8, seed);
        traces.push((format!("battery {seed}"), practice(&song, &settings, seat, false, true, &[])));
        let (keys, how): (&[(f64, &str, bool)], &str) = match seed % 3 {
            0 => (&skip, "skipped"),
            1 => (&[], "watched on"),
            _ => (&quit, "quit"),
        };
        traces.push((
            format!("battery {seed} out, {how}"),
            practice(&song, &settings, seat, false, seed % 3 == 2, keys),
        ));
        traces.push((format!("battery {seed} watched"), practice(&song, &settings, seat, true, false, &[])));
    }
    // Charts: Adaptive.
    traces.push((
        "adaptive, stepping down".into(),
        practice_on(&song, &ddi, (3, 1, 7), false, false, &[], adaptive(hard)),
    ));
    traces.push((
        "adaptive, stepping up".into(),
        practice_on(&song, &ddi, (3, 6, 7), false, true, &[], adaptive(easy)),
    ));
    traces.push((
        "adaptive, watched".into(),
        practice_on(&song, &ddi, (4, 2, 7), true, false, &[], adaptive(hard)),
    ));
    if let Ok(dir) = std::env::var("DDI_PIN_DUMP") {
        for (name, (t, _)) in &traces {
            std::fs::write(format!("{dir}/{name}.txt"), t).expect("DDI_PIN_DUMP: an existing folder");
        }
    }
    let made: String = traces
        .iter()
        .map(|(name, (t, read))| pin(name, t, read))
        .collect();
    check_pins(&made, include_str!("pinned.txt"), "pinned.txt");
    // The steps are in the pins: down under the player who presses
    // nothing, up under the autoplayed one.
    for name in ["adaptive, stepping down", "adaptive, stepping up"] {
        let (_, (_, read)) = traces.iter().find(|(n, _)| n == name).unwrap();
        assert!(read.contains("  steps: rung "), "{name}: {read}");
    }
}

/// Beatucada's Easy single.
const EASY: usize = 3;

// (The digests were a constant here until round M: recorded before the
// merge; again at room model revision 9, where a run still in lost its
// proportional part and absolute score; and in round D1, where each view
// says whether its competitor left the stage; the traces otherwise the
// same, line for line. `pinned.txt` holds them now, unchanged, with what
// a person reads of each.)

/// The player's own links ([`crate::battle::Battle::links`]) and
/// everyone's as a watcher sees them ([`crate::battle::Battle::seen_links`])
/// agree on who is out (the reviewers' question, 2026-11-14): the
/// player's attack against a competitor gone out is drawn by neither,
/// each one still on its way by both (from the player's field); a
/// watcher's never point at an out competitor nor at the player; out,
/// the player has no links of their own; a CPU's field takes no moment
/// after it went out, practice's as an online room's.
#[test]
fn the_player_s_links_and_the_watchers_agree_on_who_is_out() {
    let song = bundled("beatucada");
    let mut settings = Settings::default();
    settings.stepmania.life_type = ddi_engine::rules::LifeType::Battery;
    let (mut outgoing, mut watched, mut outs) = (0, 0, 0);
    for (seed, auto) in [(3u64, true), (7, true), (9, true), (4, false), (11, false)] {
        settings.stepmania.lives = 1 + (seed % 5) as u8;
        let mut room = room_seeded(1, 3 + (seed % 5) as u16, 2 + (seed % 3) as u8, seed);
        let rules = MatchRules {
            battle: ddi_room::BattlePreset::GAUGE_ATTACKS_V0.into(),
            ..room.rules().clone()
        };
        room.save_rules(rules).unwrap();
        let mut now = 0;
        let order = begin_at(&song, &mut room, &mut now, LONG);
        let clock: Clock = Rc::new(RefCell::new(0.0));
        let mut config = SessionConfig::from_settings(&settings, auto, None);
        config.opponents = Some(order.setup.clone());
        config.stage_rules = Some(rules_of(&settings, &order, &song));
        let mut s = PlaySession::start(
            &song,
            order.chart.index,
            &settings,
            config,
            FakeAudio::new(&clock, 0.0),
            110.0,
            FakeInputs::new(&clock),
        )
        .unwrap();
        // Each CPU's field as shown when it went out.
        let mut out_shown: Vec<Option<(f32, u32)>> = vec![None; order.cpus.len()];
        loop {
            *clock.borrow_mut() += 1.0 / 60.0;
            let host = *clock.borrow();
            let (_, event) = s.tick(HostTime(host));
            if event.is_some() || host > 300.0 {
                break;
            }
            let b = s.battle().unwrap();
            let views = b.views();
            let own = b.own_seat().expect("the player competes");
            let mine = b.links(&s.player);
            let seen = b.seen_links();
            for l in &mine {
                if l.outgoing {
                    assert!(!views[l.seat].out, "{l:?}");
                    outgoing += 1;
                    if !b.decided() {
                        assert!(
                            seen.iter().any(|x| x.attack == l.attack && x.from == own && x.to == l.seat),
                            "{l:?} {seen:?}"
                        );
                    }
                }
            }
            for x in &seen {
                assert_ne!(x.to, own, "{x:?}");
                assert!(!views[x.to].out, "{x:?}");
                watched += 1;
            }
            if s.player.gauge_failed_at().is_some() {
                assert!(mine.is_empty(), "out: no links of their own");
            }
            let t = s.frame(HostTime(host)).song_time;
            for (i, f) in b.feeds().enumerate().take(order.cpus.len()) {
                let shown = f.shown(t);
                match out_shown[i] {
                    None if shown.out => {
                        out_shown[i] = Some((shown.life, shown.combo));
                        outs += 1;
                    }
                    Some(was) => assert_eq!((shown.life, shown.combo), was, "seat {i} after its out"),
                    None => {}
                }
            }
        }
    }
    assert!(outgoing > 0 && watched > 0 && outs > 0, "{outgoing} {watched} {outs}");
}
