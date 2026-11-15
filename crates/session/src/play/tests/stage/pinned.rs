//! What a stage's battle shows and decides, frame by frame, pinned by a
//! digest of everything it is read for (the player's links, everyone's
//! links as a watcher sees them, the warnings, the bars, the views, the
//! settlement, the outcome): written before the battle's parallel copies
//! (its three settle paths, its three link builders, its seat lookups)
//! were merged, so that the merge is shown to change none of it.

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

/// `s` run at 60 frames a second to its end (or host second `limit`),
/// traced every `every` frames; then its outcome and what it told.
fn traced(s: &mut PlaySession<FakeAudio, FakeInputs>, clock: &Clock, limit: f64) -> String {
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
    out.push_str(&format!("outcome {:?}\n", s.take_stage_outcome()));
    out.push_str(&format!("told {:?}\n", s.battle_told()));
    out
}

/// A practice stage with attacks on the long chart (`cpus` at `level`,
/// seeded from `seed`), played with `settings` (a battery of lives
/// there: CPUs go out and stages are decided early), watched or played
/// by the player (`auto`: autoplayed; `keys` pressed): its trace.
fn practice(
    song: &Song,
    settings: &Settings,
    (cpus, level, seed): (u16, u8, u64),
    watching: bool,
    auto: bool,
    keys: &[(f64, &str, bool)],
) -> String {
    let mut room = room_seeded(1, cpus, level, seed);
    let rules = MatchRules {
        battle: ddi_room::BattlePreset::GAUGE_ATTACKS_V0.into(),
        ..room.rules().clone()
    };
    room.save_rules(rules).unwrap();
    let mut now = 0;
    let order = if watching {
        begin_watching(song, &mut room, &mut now, LONG)
    } else {
        begin_at(song, &mut room, &mut now, LONG)
    };
    let clock: Clock = Rc::new(RefCell::new(0.0));
    let mut inputs = FakeInputs::new(&clock);
    for &(at, code, pressed) in keys {
        inputs.key(at, code, pressed);
    }
    let mut config = SessionConfig::from_settings(settings, auto, None);
    config.opponents = Some(StageSetup::of(&order));
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
    let mut out = traced(&mut s, &clock, 300.0);
    // The same stage left at its prompt.
    let left = crate::battle::forfeit(
        StageSetup::of(&order),
        song,
        order.chart.index,
        &rules_of(settings, &order, song),
    );
    out.push_str(&format!("left {left:?}\n"));
    out
}

/// Every way a practice stage is played and decided, pinned: the player
/// playing it through (their links, the warnings, an early decision, a
/// wait for the last attack, a draw); out of life and watching on (their
/// own field among the others', settled as it comes); out of life and
/// skipping (played out at once, settled quietly); quitting in it (a
/// forfeit); a spectator; and the stage left at its prompt; under DDI
/// and under a battery of lives (CPUs going out).
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
    let mut traces: Vec<(String, String)> = vec![
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
    if let Ok(dir) = std::env::var("DDI_PIN_DUMP") {
        for (name, t) in &traces {
            let _ = std::fs::write(format!("{dir}/{name}.txt"), t);
        }
    }
    let digests: Vec<(&str, u64)> = traces.iter().map(|(name, t)| (name.as_str(), fnv(t))).collect();
    eprintln!("{digests:#x?}");
    assert_eq!(digests, PINNED);
}

/// [`a_practice_stage_shows_and_decides_as_pinned`]'s digests, recorded
/// before the merge; recorded again at room model revision 9, where a run
/// still in lost its proportional part and absolute score (the traces
/// otherwise the same, line for line); and again in round D1, where each
/// view says whether its competitor left the stage (`left: false` in
/// every one here; the traces otherwise the same, line for line).
const PINNED: &[(&str, u64)] = &[
    ("played", 0x509ea838916cec89),
    ("played hard", 0x9628bd031b7187dc),
    ("out, watched on", 0xa1d3ffd43a5db1a1),
    ("out, skipped", 0x79453508e32b036b),
    ("quit", 0xc5392aaee603d416),
    ("watched", 0xbcb98777abbdab0b),
    ("battery 1", 0xf107916a48183caf),
    ("battery 1 out, watched on", 0xe3323f070532c3b5),
    ("battery 1 watched", 0x9a43744c7c4a0eab),
    ("battery 2", 0x44a2c1b86919efde),
    ("battery 2 out, quit", 0x20056639c994975c),
    ("battery 2 watched", 0x8ccf8e1c5f98d216),
    ("battery 3", 0xe3423ee7cf56c5b4),
    ("battery 3 out, skipped", 0x6f8e931ef74c32a6),
    ("battery 3 watched", 0x2a4207059a100144),
    ("battery 4", 0xa427cd789cc68b37),
    ("battery 4 out, watched on", 0x3a3467dffd6623ae),
    ("battery 4 watched", 0x6c9f54473bcdcf5f),
    ("battery 5", 0x52176ea6daa62608),
    ("battery 5 out, quit", 0xe793013e1fb2696e),
    ("battery 5 watched", 0xd761706e516d5ed2),
    ("battery 6", 0xfbd80ff2b260ea43),
    ("battery 6 out, skipped", 0xb29ff16be6857b78),
    ("battery 6 watched", 0xee37609ff5065f1f),
    ("battery 7", 0x7c45efe5d310be0c),
    ("battery 7 out, watched on", 0x864a6cc4a94cabab),
    ("battery 7 watched", 0x9ebbfc69a045143f),
    ("battery 8", 0x5b7d62b4414acef6),
    ("battery 8 out, quit", 0xbeeac68f5a6c8827),
    ("battery 8 watched", 0xd74bde09d14bdae0),
    ("battery 9", 0xd74a3b7a5064ce94),
    ("battery 9 out, skipped", 0x6f4cc75eebd9c739),
    ("battery 9 watched", 0x7d723a65439a60d8),
    ("battery 10", 0x798a289c397e33d3),
    ("battery 10 out, watched on", 0x90649028c4a2f314),
    ("battery 10 watched", 0x28a6ef396c4d88d8),
    ("battery 11", 0x1aa5f3c5db6c765),
    ("battery 11 out, quit", 0xdd9d40825da71a59),
    ("battery 11 watched", 0x9a732e323436c7fa),
    ("battery 12", 0x808d64258f30733e),
    ("battery 12 out, skipped", 0x8b766ae4a431a51b),
    ("battery 12 watched", 0xe7a6862520ad3860),
];

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
        config.opponents = Some(StageSetup::of(&order));
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
