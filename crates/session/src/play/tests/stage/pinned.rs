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
    let mut room = room_seeded(song, 1, cpus, level, seed);
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
/// before the merge.
const PINNED: &[(&str, u64)] = &[
    ("played", 0x5ca2d062c8cf09ba),
    ("played hard", 0x68a96dbe9e4ce38f),
    ("out, watched on", 0x0522b1d5d564bdc5),
    ("out, skipped", 0x7a300ac97dd857d3),
    ("quit", 0x01be014131336056),
    ("watched", 0x0683bbea5500fcb5),
    ("battery 1", 0xf6b93659f4a510ce),
    ("battery 1 out, watched on", 0xb152d7b317c05ea7),
    ("battery 1 watched", 0x2149b277ca837c11),
    ("battery 2", 0xf47c7ef3cdc65707),
    ("battery 2 out, quit", 0xc16eca03be2fbb48),
    ("battery 2 watched", 0xad1bc9d2e680e166),
    ("battery 3", 0x0a88bfd7282ffbb9),
    ("battery 3 out, skipped", 0x78e39ed0ad40da3a),
    ("battery 3 watched", 0x7b2a9ac03f1a499a),
    ("battery 4", 0xa5c93fbfd5acea28),
    ("battery 4 out, watched on", 0x2ac08bde530e9ae8),
    ("battery 4 watched", 0x5c931e357dc90267),
    ("battery 5", 0xef993d3133aeaaa3),
    ("battery 5 out, quit", 0x89635528496d0260),
    ("battery 5 watched", 0xf919a32ac48ee804),
    ("battery 6", 0x2050a5f4d9f3a04a),
    ("battery 6 out, skipped", 0x834acde9a5a15d54),
    ("battery 6 watched", 0xafdf2af437d54eaf),
    ("battery 7", 0xe96c5f944eb29c59),
    ("battery 7 out, watched on", 0x0449a9398de29e8b),
    ("battery 7 watched", 0x4936fd4c8514a189),
    ("battery 8", 0x0f13a680976cce27),
    ("battery 8 out, quit", 0xc5cb8078e1290d49),
    ("battery 8 watched", 0xd07db6fdbc5ccd36),
    ("battery 9", 0x74a4c427adf1e755),
    ("battery 9 out, skipped", 0xdf6845565f94ce99),
    ("battery 9 watched", 0x832f4902d492fbe6),
    ("battery 10", 0x4bde1442ccfc2d08),
    ("battery 10 out, watched on", 0x830776a3542df3c2),
    ("battery 10 watched", 0x5bc745522ede814e),
    ("battery 11", 0x58cbf4452e5caee0),
    ("battery 11 out, quit", 0x4b573c805732d7f7),
    ("battery 11 watched", 0xab03707df495b8ac),
    ("battery 12", 0x9df42fc88aa371f9),
    ("battery 12 out, skipped", 0x4bb4bfd677084e57),
    ("battery 12 watched", 0xefa5ee3646bf93c6),
];
