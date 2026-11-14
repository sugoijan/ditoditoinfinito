//! A practice stage under adaptive charts (a match's "Charts: Adaptive",
//! `ddi_engine::ladder`): the player and the CPUs step down the song's
//! ladder, the same at any frame rate; their fields draw the chart they
//! are on; those still in rank by the score; the status line says the
//! player's own step; a song without a ladder plays as picked.

use super::*;

/// Beatucada's Hard single: its ladder is Medium, Easy, Beginner.
const HARD: usize = 5;

/// A practice room of one stage, `cpus` CPUs at `level`, attacks on,
/// zero life stepping down.
fn step_room(song: &Song, cpus: u16, level: u8, charts: ddi_room::Charts) -> RoomClient {
    let mut room = attacks_room(song, cpus, level);
    let rules = MatchRules {
        charts,
        ..room.rules().clone()
    };
    room.save_rules(rules).unwrap();
    room
}

/// The stage of `order`, nothing pressed (the player misses every note),
/// at `fps` to its end: the session and its outcome.
fn idle(
    song: &Song,
    order: &StageOrder,
    fps: f64,
) -> (PlaySession<FakeAudio, FakeInputs>, crate::battle::StageOutcome, Vec<f64>) {
    let (mut s, clock) = stage_session(song, order, false, &[]);
    let mut cues = Vec::new();
    loop {
        *clock.borrow_mut() += 1.0 / fps;
        let now = *clock.borrow();
        let (_, event) = s.tick(HostTime(now));
        if let Some((crate::announcer::battle::StageCue::Step { .. }, age)) = s.stage_cue(now)
            && age == 0.0
        {
            cues.push(now);
        }
        if event.is_some() || now > 400.0 {
            break;
        }
    }
    let outcome = s.take_stage_outcome().expect("its outcome");
    (s, outcome, cues)
}

#[test]
fn the_player_and_the_cpus_step_down_alike_at_any_frame_rate() {
    let song = bundled("beatucada");
    assert_eq!(song.charts[HARD].difficulty, ddi_chart::Difficulty::Hard);
    let mut practice = step_room(&song, 3, 1, ddi_room::Charts::Adaptive);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, HARD);
    assert!(order.rules.adaptive());
    let (a, oa, cues) = idle(&song, &order, 60.0);
    let (b, ob, _) = idle(&song, &order, 144.0);
    // The player stepped down every rung, then went out on the last.
    let ladder: Vec<usize> = a.battle().unwrap().rungs().iter().map(|r| r.chart).collect();
    assert_eq!(ladder, ddi_engine::ladder::Ladder::of(&song, HARD).rungs());
    let below = ddi_engine::ladder::below(&song, HARD).len();
    let steps = a.player.steps().to_vec();
    assert_eq!(steps.len(), below, "{steps:?}");
    assert_eq!(steps, b.player.steps(), "60 and 144 frames a second");
    assert_eq!(a.player.results().tally, b.player.results().tally);
    assert!(a.player.gauge_failed_at().is_some(), "out on the lowest rung");
    // Its own cue, once a step, in the status line.
    assert_eq!(cues.len(), steps.len(), "{cues:?}");
    assert_eq!(a.steps_told().len(), steps.len());
    // The same ranking either way; the CPUs' steps alike.
    assert_eq!(ids(&oa), ids(&ob));
    let cpu_steps = |s: &PlaySession<FakeAudio, FakeInputs>| -> Vec<Vec<(f64, f64, u8)>> {
        s.battle().unwrap().feeds().map(|f| f.steps().collect()).collect()
    };
    assert_eq!(cpu_steps(&a), cpu_steps(&b));
    // The player's own field steps as their play did.
    let own: Vec<(f64, f64, u8)> = a.battle().unwrap().own_feed().steps().collect();
    assert_eq!(own.len(), steps.len());
    for (f, st) in own.iter().zip(&steps) {
        assert_eq!((f.0, f.1, f.2), (st.at, st.boundary, st.rung));
    }
    // The details name the chart each finished on where they stepped.
    let mine = oa
        .details
        .iter()
        .find(|(p, _)| *p == order.me)
        .map(|(_, d)| d.clone())
        .unwrap();
    assert_eq!(mine.chart.as_deref(), Some("Beginner"));
    for (p, d) in &oa.details {
        let seat = a
            .battle()
            .unwrap()
            .views()
            .iter()
            .position(|v| v.participant == *p);
        if let Some(seat) = seat {
            let field = a.battle().unwrap().feeds().nth(seat).unwrap();
            let n = field.steps().count();
            assert_eq!(d.chart.is_some(), n > 0, "{p:?}");
        }
    }
}

#[test]
fn a_field_draws_the_chart_its_player_is_on() {
    let song = bundled("beatucada");
    let mut practice = step_room(&song, 1, 1, ddi_room::Charts::Adaptive);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, HARD);
    let (a, _, _) = idle(&song, &order, 60.0);
    let own = a.battle().unwrap().own_feed();
    let (at, boundary, rung) = own.steps().next().unwrap();
    let track = crate::battle::feed::TapTrack::new(&song, HARD).unwrap();
    let scroll = ddi_engine::ScrollOptions::default();
    // Before the step: the stage chart's notes; in the breather none of
    // a chart's; from the boundary the rung's.
    assert_eq!(own.rung_at(at - 0.5), 0);
    assert_eq!(own.rung_at(at + 0.5), rung);
    let breather = own.frame(&track, scroll, (at + boundary) / 2.0 - 0.5, 1.0);
    assert!(
        breather
            .notes
            .iter()
            .all(|n| n.note_index >= ddi_engine::battle::HAZARD_INDEX_BASE),
        "only shocks: {:?}",
        breather.notes.len()
    );
    let after = own.frame(&track, scroll, boundary + 0.25, 12.0);
    assert!(
        after.notes.iter().all(|n| n.note_index >= 1 << 24),
        "the rung's notes"
    );
}

#[test]
fn those_still_in_rank_by_the_score_and_a_song_without_a_ladder_plays_as_picked() {
    let song = bundled("beatucada");
    // The song with no other single chart to step to: Adaptive is As
    // picked there, exactly.
    let mut alone = song.clone();
    for (i, c) in alone.charts.iter_mut().enumerate() {
        if i != CHART && c.layout == "dance-single" {
            c.notes.clear();
        }
    }
    assert!(ddi_engine::ladder::Ladder::of(&alone, CHART).is_empty());
    let mut now = 0;
    let mut out = step_room(&alone, 3, 1, ddi_room::Charts::AsPicked);
    let o_out = begin_at(&alone, &mut out, &mut now, CHART);
    let mut step = step_room(&alone, 3, 1, ddi_room::Charts::Adaptive);
    let o_step = begin_at(&alone, &mut step, &mut now, CHART);
    let (a, oa, _) = idle(&alone, &o_out, 60.0);
    let (b, ob, cues) = idle(&alone, &o_step, 60.0);
    assert!(cues.is_empty() && b.player.steps().is_empty());
    // The same plays (the order of those still in is the rule's own:
    // life first as picked, the score first under Adaptive).
    assert_eq!(a.player.results(), b.player.results());
    assert_eq!(oa.details, ob.details);
    // On Hard: those still in by the score as earned, whatever their
    // steps; those out after.
    let mut practice = step_room(&song, 7, 4, ddi_room::Charts::Adaptive);
    let order = begin_at(&song, &mut practice, &mut now, HARD);
    let (_, o, _) = idle(&song, &order, 60.0);
    let score = |p: ddi_room::ParticipantId| {
        o.details.iter().find(|(q, _)| *q == p).and_then(|(_, d)| d.score)
    };
    let mut last: Option<(bool, Option<i64>)> = None;
    for group in &o.ranking {
        for f in group {
            let survived = f.finish == Finish::Survived;
            let s = score(f.participant);
            if let Some((was_in, was)) = last {
                assert!(was_in || !survived, "those in first");
                if was_in && survived {
                    assert!(was >= s, "the score first: {:?}", o.details);
                }
            }
            last = Some((survived, s));
        }
    }
}

#[test]
fn the_claps_follow_the_new_chart_after_a_step() {
    let song = bundled("beatucada");
    let mut practice = step_room(&song, 1, 1, ddi_room::Charts::Adaptive);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, HARD);
    let settings = Settings {
        assist_clap: true,
        mine_sound: true,
        ..Settings::default()
    };
    let (mut s, clock) = stage_session_with(&song, &order, false, &[], &settings);
    run_until(&mut s, &clock, 60.0, 400.0);
    let steps = s.player.steps().to_vec();
    assert!(steps.len() >= 2, "{steps:?}");
    let out = s.player.gauge_failed_at().expect("out on the lowest rung");
    // What should be heard: the rows of what is judged now (the old
    // chart's notes taken away at each step are not judgeable any more).
    let rows = crate::sounds::clap_rows(
        s.player
            .judge()
            .notes()
            .iter()
            .map(|n| (n.note.kind, n.judgeable, n.seconds)),
    );
    let clap_len = 0.12;
    let music = s.audio().played[0].0;
    let claps: Vec<f64> = s
        .audio()
        .played
        .iter()
        .zip(&s.audio().sounds)
        .filter(|(_, b)| (b.0 - clap_len).abs() < 1e-9)
        .map(|(p, _)| p.0 - music)
        .collect();
    assert!(!claps.is_empty());
    for &c in &claps {
        let ours = rows.iter().any(|&r| (r - c).abs() < 1e-9);
        // Asked for before the step, at most the lookahead past it.
        let asked = steps
            .iter()
            .any(|st| c > st.at && c <= st.at + crate::sounds::CLAP_LOOKAHEAD + 1e-6);
        assert!(ours || asked, "a clap at {c} of a chart taken away ({steps:?})");
    }
    // Nothing in a breather but those asked for already; every row of
    // each new chart heard until the player went out.
    for st in &steps {
        assert!(
            !claps
                .iter()
                .any(|&c| c > st.at + crate::sounds::CLAP_LOOKAHEAD + 1e-6 && c < st.boundary - 1e-9),
            "a clap in the breather {st:?}"
        );
    }
    let heard = rows
        .iter()
        .filter(|&&r| r >= steps[0].boundary && r < out - 0.5)
        .filter(|&&r| !claps.iter().any(|&c| (c - r).abs() < 1e-9))
        .count();
    assert_eq!(heard, 0, "rows of the new charts not clapped");
    // The mine sound follows what is judged: nothing stepped on here.
    assert_eq!(s.event_sounds().1, 0);
}

/// Beatucada's Medium single: Hard and Challenge above it, Easy and
/// Beginner below.
fn medium(song: &Song) -> usize {
    song.charts
        .iter()
        .position(|c| {
            c.layout == "dance-single"
                && c.difficulty == ddi_chart::Difficulty::Medium
                && c.derivation.is_none()
        })
        .unwrap()
}

/// The stage of `order` autoplayed at `fps` to its end, the step cues
/// seen (their host seconds and directions).
fn autoplayed(
    song: &Song,
    order: &StageOrder,
    fps: f64,
) -> (PlaySession<FakeAudio, FakeInputs>, crate::battle::StageOutcome, Vec<(f64, bool)>) {
    let (mut s, clock) = stage_session(song, order, true, &[]);
    let mut cues = Vec::new();
    loop {
        *clock.borrow_mut() += 1.0 / fps;
        let now = *clock.borrow();
        let (_, event) = s.tick(HostTime(now));
        if let Some((crate::announcer::battle::StageCue::Step { up, .. }, age)) = s.stage_cue(now)
            && age == 0.0
        {
            cues.push((now, up));
        }
        if event.is_some() || now > 400.0 {
            break;
        }
    }
    let outcome = s.take_stage_outcome().expect("its outcome");
    (s, outcome, cues)
}

/// The player, autoplayed from Medium, and the strongest CPUs climb the
/// ladder above the stage's chart, the same at any frame rate; the cue
/// says each step up, the fields follow both ways, the Standings mark
/// the chart finished on as harder.
#[test]
fn the_player_and_strong_cpus_step_up_alike_at_any_frame_rate() {
    let song = bundled("beatucada");
    let stage = medium(&song);
    let mut practice = step_room(&song, 3, 10, ddi_room::Charts::Adaptive);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, stage);
    let (a, oa, cues) = autoplayed(&song, &order, 60.0);
    let (b, ob, _) = autoplayed(&song, &order, 144.0);
    let steps = a.player.steps().to_vec();
    assert!(steps.iter().any(|s| s.up), "{steps:?}");
    assert!(steps.iter().all(|s| s.up), "a perfect play only climbs: {steps:?}");
    assert_eq!(steps, b.player.steps(), "60 and 144 frames a second");
    assert_eq!(a.player.results().tally, b.player.results().tally);
    assert_eq!(ids(&oa), ids(&ob));
    // Its own cue, once a step, saying up.
    assert_eq!(cues.len(), steps.len(), "{cues:?}");
    assert!(cues.iter().all(|c| c.1));
    // Its own field steps as its play did, drawing the new chart from
    // the boundary and the old one up to it.
    let own = a.battle().unwrap().own_feed();
    let seen: Vec<(f64, f64, u8)> = own.steps().collect();
    assert_eq!(seen.len(), steps.len());
    let first = steps[0];
    assert_eq!(own.rung_at(first.boundary - 0.5), 0, "the old chart to the boundary");
    assert_eq!(own.rung_at(first.boundary + 0.5), first.rung);
    // The CPUs alike either way; the strongest climb too.
    let cpu_steps = |s: &PlaySession<FakeAudio, FakeInputs>| -> Vec<Vec<(f64, f64, u8)>> {
        s.battle().unwrap().feeds().map(|f| f.steps().collect()).collect()
    };
    assert_eq!(cpu_steps(&a), cpu_steps(&b));
    assert!(cpu_steps(&a).iter().any(|f| !f.is_empty()), "a strong CPU climbs");
    // The details: the player finished on a harder chart, marked so.
    let mine = oa.details.iter().find(|(p, _)| *p == order.me).unwrap().1.clone();
    let last = song.charts[steps.last().unwrap().chart].difficulty.name();
    assert_eq!(mine.chart.as_deref(), Some(last));
    assert!(mine.harder);
    // Its score is as earned: its play's own sum, never another scale.
    assert_eq!(
        mine.score,
        a.player.ruleset().score.view().ddi.map(|d| d.score)
    );
}

/// The CPUs' level (from 0) whose plays climb from Medium to Hard and
/// fall back on this song: three stars (`adaptive-v2`'s accuracy keeps
/// two-star CPUs from climbing at all).
const LEVEL_UP_DOWN: u8 = 2;

/// Three-star CPUs on Medium (they climb to Hard now and then, and fall
/// back; from Hard the song is too short for any to come back up): their
/// fields and their plays agree on every step, both ways, at any frame
/// rate.

#[test]
fn fields_follow_steps_down_and_back_up() {
    let song = bundled("beatucada");
    let mut practice = step_room(&song, 7, LEVEL_UP_DOWN, ddi_room::Charts::Adaptive);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, medium(&song));
    let (a, _, _) = idle(&song, &order, 60.0);
    let (b, _, _) = idle(&song, &order, 144.0);
    let battle = a.battle().unwrap();
    let mut both = false;
    for (seat, field) in battle.feeds().enumerate() {
        let player = battle.opponent_player(seat).unwrap();
        let steps: Vec<(f64, f64, u8)> = player.steps().iter().map(|s| (s.at, s.boundary, s.rung)).collect();
        let seen: Vec<(f64, f64, u8)> = field.steps().collect();
        assert_eq!(seen, steps, "seat {seat}");
        both |= player.steps().iter().any(|s| s.up) && player.steps().iter().any(|s| !s.up);
    }
    let fields = |s: &PlaySession<FakeAudio, FakeInputs>| -> Vec<Vec<(f64, f64, u8)>> {
        s.battle().unwrap().feeds().map(|f| f.steps().collect()).collect()
    };
    assert_eq!(fields(&a), fields(&b));
    assert!(both, "a three-star CPU up to Hard and back down");
}

/// A step down that closes the chart it left (`adaptive-v1`: life ran
/// out there twice; here once, `closed_after: 1`, so that a stage of
/// nothing pressed shows it) is told as staying on the chart below
/// ("Staying on Medium"), not as a step down.
#[test]
fn a_chart_closed_is_told_as_staying_on_the_one_below() {
    let song = bundled("beatucada");
    let mut practice = step_room(&song, 1, 1, ddi_room::Charts::Adaptive);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, HARD);
    let (mut s, clock) = stage_session(&song, &order, false, &[]);
    s.player.set_ladder_rule(ddi_engine::ladder::AdaptiveRule {
        closed_after: 1,
        ..ddi_engine::ladder::ADAPTIVE_V1
    });
    let mut told = None;
    loop {
        *clock.borrow_mut() += 1.0 / 60.0;
        let now = *clock.borrow();
        let (_, event) = s.tick(HostTime(now));
        if let Some((cue, 0.0)) = s.stage_cue(now)
            && matches!(
                cue,
                crate::announcer::battle::StageCue::Stay { .. }
                    | crate::announcer::battle::StageCue::Step { .. }
            )
        {
            told = Some(cue);
            break;
        }
        if event.is_some() || now > 400.0 {
            break;
        }
    }
    assert_eq!(
        told,
        Some(crate::announcer::battle::StageCue::Stay { rung: 1 }),
        "{:?}",
        s.player.steps()
    );
    assert!(s.player.steps()[0].closes);
}

