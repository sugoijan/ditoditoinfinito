//! A practice stage whose rules step down at zero life (a match's "At
//! zero life: Step down", `ddi_engine::ladder`): the player and the CPUs
//! step down the song's ladder, the same at any frame rate; their fields
//! draw the chart they are on; the ranking puts fewer steps first; the
//! status line says the player's own step; a song without a ladder plays
//! as Out.

use super::*;

/// Beatucada's Hard single: its ladder is Medium, Easy, Beginner.
const HARD: usize = 5;

/// A practice room of one stage, `cpus` CPUs at `level`, attacks on,
/// zero life stepping down.
fn step_room(song: &Song, cpus: u16, level: u8, at_zero: ddi_room::AtZero) -> RoomClient {
    let mut room = attacks_room(song, cpus, level);
    let rules = MatchRules {
        at_zero,
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
        if let Some((crate::announcer::battle::StageCue::StepDown(_), age)) = s.stage_cue(now)
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
    let mut practice = step_room(&song, 3, 1, ddi_room::AtZero::StepDown);
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, HARD);
    assert!(order.rules.steps_down());
    let (a, oa, cues) = idle(&song, &order, 60.0);
    let (b, ob, _) = idle(&song, &order, 144.0);
    // The player stepped down every rung, then went out on the last.
    let ladder: Vec<usize> = a.battle().unwrap().rungs().iter().map(|r| r.chart).collect();
    assert_eq!(ladder, ddi_engine::ladder::below(&song, HARD));
    let steps = a.player.steps().to_vec();
    assert_eq!(steps.len(), ladder.len(), "{steps:?}");
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
    let mut practice = step_room(&song, 1, 1, ddi_room::AtZero::StepDown);
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
    let breather = own.frame(&track, scroll, (at + boundary) / 2.0 - 2.0, 1.0);
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
fn fewer_steps_rank_first_and_a_song_without_a_ladder_plays_as_out() {
    let song = bundled("beatucada");
    // The easiest single chart has no ladder: Step down is Out there,
    // exactly.
    assert!(ddi_engine::ladder::below(&song, CHART).is_empty());
    let mut now = 0;
    let mut out = step_room(&song, 3, 1, ddi_room::AtZero::Out);
    let o_out = begin_at(&song, &mut out, &mut now, CHART);
    let mut step = step_room(&song, 3, 1, ddi_room::AtZero::StepDown);
    let o_step = begin_at(&song, &mut step, &mut now, CHART);
    let (a, oa, _) = idle(&song, &o_out, 60.0);
    let (b, ob, cues) = idle(&song, &o_step, 60.0);
    assert!(cues.is_empty() && b.player.steps().is_empty());
    assert_eq!(a.player.results(), b.player.results());
    assert_eq!(ids(&oa), ids(&ob));
    assert_eq!(oa.details, ob.details);
    // On Hard: whoever stepped places below everyone still in on the
    // stage's chart, and above everyone out.
    let mut practice = step_room(&song, 7, 4, ddi_room::AtZero::StepDown);
    let order = begin_at(&song, &mut practice, &mut now, HARD);
    let (s, o, _) = idle(&song, &order, 60.0);
    let steps_of = |p: ddi_room::ParticipantId| -> Option<usize> {
        if p == order.me {
            return Some(s.player.steps().len());
        }
        let b = s.battle().unwrap();
        let seat = b.views().iter().position(|v| v.participant == p)?;
        Some(b.feeds().nth(seat)?.steps().count())
    };
    let mut last: Option<(bool, usize)> = None;
    for group in &o.ranking {
        for f in group {
            let survived = f.finish == Finish::Survived;
            let n = steps_of(f.participant).unwrap();
            if let Some((was_in, was_n)) = last
                && was_in
                && survived
            {
                assert!(was_n <= n, "fewer steps first: {:?}", o.ranking);
            }
            if let Some((was_in, _)) = last {
                assert!(was_in || !survived, "those in first");
            }
            last = Some((survived, n));
        }
    }
}

#[test]
fn the_claps_follow_the_new_chart_after_a_step() {
    let song = bundled("beatucada");
    let mut practice = step_room(&song, 1, 1, ddi_room::AtZero::StepDown);
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
