//! The grade a stage's competitor is shown with (round AC, 2026-11-19:
//! "we should be careful not to show an "SSS" grade on it"): a forfeit is
//! QUIT however well its first seconds went, a competitor out by life or
//! failed is F, whoever judged them (the player's own device, a CPU's).

use super::step_down::HARD;
use super::*;

/// The local player's own detail of an outcome.
fn own_detail(o: &crate::battle::StageOutcome, me: ddi_room::ParticipantId) -> ddi_room::RunDetail {
    o.details
        .iter()
        .find(|(p, _)| *p == me)
        .map(|(_, d)| d.clone())
        .expect("the player's own detail")
}

/// An autoplayed (every note W0) stage's play interrupted ten seconds in
/// (the audio stopped by the system, a lost input: `SessionEvent::
/// Interrupted`) is a forfeit; its grade is QUIT, never the top grade of
/// the notes judged before it, online (reported as the player's own run)
/// and in practice.
#[test]
fn an_interrupted_stage_play_is_a_forfeit_graded_quit() {
    let song = bundled("beatucada");
    for online in [true, false] {
        let mut practice = attacks_room(3, 3);
        let mut now = 0;
        let mut order = begin_at(&song, &mut practice, &mut now, LONG);
        if online {
            order.cpus.clear();
            order.others = 1;
        }
        let (mut s, clock) = stage_session(&song, &order, true, &[]);
        assert!(run_until(&mut s, &clock, 60.0, 10.0).is_none());
        s.inputs.interrupted = true;
        let event = run_until(&mut s, &clock, 60.0, 10.2).expect("interrupted");
        assert!(matches!(event, SessionEvent::Interrupted), "{online}");
        let outcome = s.take_stage_outcome().expect("its outcome");
        assert_eq!(outcome.mine, ddi_room::Run::Forfeit, "online {online}");
        let detail = own_detail(&outcome, order.me);
        assert_eq!(detail.grade, "QUIT", "online {online}: {detail:?}");
    }
}

/// A quit ten seconds into a perfect (autoplayed) play: QUIT, in practice
/// (watched on) and online (ended at once).
#[test]
fn a_quit_after_a_perfect_start_is_graded_quit() {
    let song = bundled("beatucada");
    let keys = [
        (10.0, "Escape", true),
        (10.05, "Escape", false),
        (10.2, "Escape", true),
        (10.25, "Escape", false),
    ];
    for online in [true, false] {
        let mut practice = attacks_room(3, 3);
        let mut now = 0;
        let mut order = begin_at(&song, &mut practice, &mut now, LONG);
        if online {
            order.cpus.clear();
            order.others = 1;
        }
        let (mut s, clock) = stage_session(&song, &order, true, &keys);
        let mut outcome = None;
        let _ = run_until(&mut s, &clock, 60.0, 11.0);
        outcome = outcome.or_else(|| s.take_stage_outcome());
        // Practice watches on: the skip.
        if outcome.is_none() {
            outcome = s.leave_now(HostTime(11.1));
        }
        let outcome = outcome.expect("its outcome");
        assert_eq!(outcome.mine, ddi_room::Run::Forfeit, "online {online}");
        assert_eq!(own_detail(&outcome, order.me).grade, "QUIT", "online {online}");
    }
}

/// A CPU out by life (the match's Out at zero life) is graded F, not by
/// the notes its play judged to the song's end.
#[test]
fn a_cpu_out_by_life_is_graded_f() {
    let song = bundled("beatucada");
    let mut practice = attacks_room(3, 1);
    let rules = MatchRules {
        difficulty: ddi_room::MatchDifficulty::Hard,
        ..practice.rules().clone()
    };
    practice.save_rules(rules).unwrap();
    let mut now = 0;
    let order = begin_at(&song, &mut practice, &mut now, HARD);
    let (_, _, outcome) = play(&song, &Settings::default(), &order, true, &[]);
    let outs: Vec<_> = ids(&outcome)
        .into_iter()
        .flatten()
        .filter(|(_, f)| *f == Finish::Eliminated)
        .map(|(p, _)| p)
        .collect();
    assert!(!outs.is_empty(), "a level-1 CPU goes out on Hard: {:?}", ids(&outcome));
    for p in outs {
        let d = &outcome.details.iter().find(|(q, _)| q.0 == p).unwrap().1;
        assert_eq!(d.grade, ddi_engine::rules::ddi::FAILED, "CPU {p}: {d:?}");
    }
}
