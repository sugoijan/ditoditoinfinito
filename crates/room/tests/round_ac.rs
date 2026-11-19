//! Round AC (room model revision 16), through the reducer: When life runs
//! out under both chart rules (a failed run under As picked placed after
//! every survivor by score); a forfeit's detail kept graded QUIT, one out
//! or failed F; and a retrying download holding the readiness window's
//! early end until its loading grace would have ended. Every revision's
//! events are applied to a mirror, which must stay equal.

mod common;

use common::{Room, song};
use ddi_room::*;

impl Room {
    fn join(&mut self, name: &str) -> ParticipantId {
        self.join_as(name, Intent::Play, false).unwrap()
    }

    fn pick(&mut self, n: u32) -> StageId {
        let host = self.host;
        self.by(
            host,
            Command::Pick {
                content: song(n),
                card: None,
            },
        )
        .unwrap();
        self.s.upcoming_stage().expect("a stage gathers readiness")
    }

    fn window(&self) -> ReadinessWindow {
        match self.s.phase {
            Phase::Preparing {
                window: Some(w), ..
            } => w,
            ref p => panic!("no window: {p:?}"),
        }
    }

    fn report(&mut self, who: ParticipantId, stage: StageId, run: Run, detail: Option<RunDetail>) {
        self.by(
            who,
            Command::ReportOutcome {
                stage,
                run: ReportedRun::of(run),
                detail,
            },
        )
        .unwrap();
    }
}

/// A room whose stages' songs are shared (each member reports its
/// download), under `hosted-deadline-v2` (a loading grace first).
fn shared(rules: MatchRules) -> Room {
    Room::open("host", "Host", RoomPolicy::default(), rules, ContentSource::Shared)
}

/// The stage's song made available (its share), as the room hears it.
fn available(r: &mut Room, stage: StageId) {
    r.sys(Command::SetContentStatus {
        stage,
        status: ContentStatus::Available,
        share: Some("abc12345".into()),
    })
    .unwrap();
}

/// Everyone named loaded and ready.
fn load_and_ready(r: &mut Room, stage: StageId, who: &[ParticipantId]) {
    for &p in who {
        r.load_for(p, stage, Loading::Loaded).unwrap();
        r.ready_for(p, stage, true).unwrap();
    }
}

/// Part 1: When life runs out, Play on (a new room's) under Charts: As
/// picked too: a competitor whose life ran out is failed, plays on and is
/// placed after every survivor, the failed by score, before those out and
/// forfeits; the awards the same table over the whole order.
#[test]
fn under_as_picked_the_failed_play_on_and_rank_after_the_survivors() {
    let mut rules = MatchRules::hosted();
    rules.stages = 1;
    assert_eq!(rules.charts, Charts::AsPicked);
    assert!(rules.plays_on_failed(), "As picked, Play on");
    let mut r = shared(rules);
    let host = r.host;
    let ana = r.join("ana");
    let bo = r.join("bo");
    let cy = r.join("cy");
    let stage = r.pick(1);
    available(&mut r, stage);
    load_and_ready(&mut r, stage, &[host, ana, bo, cy]);
    r.wait(r.s.policy.readiness.countdown_ms);
    assert_eq!(r.s.phase, Phase::Stage { stage });
    let detail = |grade: &str| RunDetail {
        grade: grade.into(),
        ..RunDetail::default()
    };
    r.report(cy, stage, Run::Out { at: 40.0 }, Some(detail("A")));
    r.report(ana, stage, Run::Failed { at: 30.0, score: 3_000_000 }, Some(detail("SSS")));
    r.report(
        host,
        stage,
        Run::Live {
            life: 0.3,
            score: 1_000_000,
            steps: 0,
        },
        Some(detail("B")),
    );
    r.report(bo, stage, Run::Failed { at: 50.0, score: 2_000_000 }, Some(detail("C")));
    let play = r.s.stage(stage).unwrap().play.clone().unwrap();
    let result = play.result.expect("settled on the four reports");
    let place = |p| result.placement(p).unwrap();
    assert_eq!((place(host).place, place(host).finish), (1, Finish::Survived));
    assert_eq!((place(ana).place, place(ana).finish), (2, Finish::Failed));
    assert_eq!((place(bo).place, place(bo).finish), (3, Finish::Failed));
    assert_eq!((place(cy).place, place(cy).finish), (4, Finish::Eliminated));
    // Graded as each run ended: the failed and the one out F, whatever
    // their reports said; one still in keeps their own.
    let grade = |p| play.details.get(&p).unwrap().grade.clone();
    assert_eq!(
        [grade(host), grade(ana), grade(bo), grade(cy)],
        ["B", "F", "F", "F"].map(String::from)
    );
    // Out, the rule chosen: out at zero life under As picked as before.
    let mut out = MatchRules::hosted();
    out.set_life_out(AtEasiest::Out);
    assert!(!out.plays_on_failed());
    out.charts = Charts::Adaptive;
    assert!(!out.plays_on_failed());
}

/// Part 3: a forfeit's detail is kept graded QUIT, never the letter of
/// the few notes judged before it (a perfect first ten seconds read SSS
/// in the Standings), by the room and by every mirror.
#[test]
fn a_forfeit_is_kept_graded_quit_whatever_it_reported() {
    let mut rules = MatchRules::hosted();
    rules.stages = 1;
    let mut r = shared(rules);
    let host = r.host;
    let ana = r.join("ana");
    let stage = r.pick(1);
    available(&mut r, stage);
    load_and_ready(&mut r, stage, &[host, ana]);
    r.wait(r.s.policy.readiness.countdown_ms);
    let sss = RunDetail {
        grade: "SSS".into(),
        percent: Some(9_990),
        ..RunDetail::default()
    };
    r.report(ana, stage, Run::Forfeit, Some(sss.clone()));
    let play = r.s.stage(stage).unwrap().play.clone().unwrap();
    let kept = play.details.get(&ana).unwrap();
    assert_eq!(kept.grade, RunDetail::QUIT_GRADE);
    assert_eq!(kept.percent, sss.percent, "the rest as reported");
}

/// Part 5: a download being tried again (`Loading::Retrying`) holds the
/// loading grace as any loading does; and where the grace ended early
/// (everyone loaded), a newcomer's retrying download holds the window's
/// early end on everyone else being ready until the grace would have
/// ended, then the stage starts without them. A download that comes
/// through in time lets them ready and start with the others.
#[test]
fn a_retrying_download_holds_the_early_end_until_the_grace_would_have_ended() {
    let grace = RoomPolicy::default().readiness.grace().expect("v2's grace");
    // During the grace: Retrying waits as loading does.
    let mut r = shared(MatchRules::hosted());
    let host = r.host;
    let ana = r.join("ana");
    let bo = r.join("bo");
    let stage = r.pick(1);
    available(&mut r, stage);
    let opened = r.now;
    load_and_ready(&mut r, stage, &[host, ana]);
    r.load_for(bo, stage, Loading::Retrying).unwrap();
    assert!(r.window().loading(), "the grace waits for bo");
    assert!(r.s.retrying(stage));
    r.wait_to(opened + grace - 1);
    assert!(matches!(r.s.phase, Phase::Preparing { .. }));
    // Its end: the window counts, and starts at once without bo.
    r.wait_to(opened + grace);
    assert!(matches!(r.s.phase, Phase::Countdown { .. }), "{:?}", r.s.phase);

    // The grace ended early: everyone loaded 20 s in (its Ready window
    // then to 50 s, past the grace's own end at 45 s).
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    let mut r = shared(rules);
    let host = r.host;
    let ana = r.join("ana");
    let stage = r.pick(1);
    available(&mut r, stage);
    let opened = r.now;
    r.wait(20_000);
    r.load_for(host, stage, Loading::Loaded).unwrap();
    r.load_for(ana, stage, Loading::Loaded).unwrap();
    assert!(!r.window().loading(), "the grace over early");
    assert_eq!(r.window().hold_until.map(|m| m.0), Some(opened + grace));
    // Cy joins during the window (seated at once) and his download fails
    // and is tried again.
    r.wait(4_000);
    let cy = r.join("cy");
    assert_eq!(r.s.participation(cy), Some(Participation::NotReady { stage }));
    r.load_for(cy, stage, Loading::Loading { permille: 300 }).unwrap();
    r.load_for(cy, stage, Loading::Retrying).unwrap();
    assert_eq!(r.s.next_deadline().map(|m| m.0), Some(opened + grace));
    // Everyone else ready: no early start while he retries.
    r.ready_for(host, stage, true).unwrap();
    r.ready_for(ana, stage, true).unwrap();
    assert!(matches!(r.s.phase, Phase::Preparing { .. }), "held for cy");
    r.wait_to(opened + grace - 1);
    assert!(matches!(r.s.phase, Phase::Preparing { .. }), "held for cy");
    // The grace's own end: it starts without him.
    r.wait_to(opened + grace);
    let Phase::Countdown { stage: started, .. } = r.s.phase else {
        panic!("{:?}", r.s.phase)
    };
    assert_eq!(started, stage);
    let play = r.s.stage(stage).unwrap().play.as_ref().unwrap();
    assert!(!play.lineup.contains(&cy));
    assert_eq!(play.sat_out.get(&cy), Some(&SitOutReason::StillLoading));

    // The download comes through in time: he readies and they start.
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    let mut r = shared(rules);
    let host = r.host;
    let ana = r.join("ana");
    let stage = r.pick(1);
    available(&mut r, stage);
    r.load_for(host, stage, Loading::Loaded).unwrap();
    r.load_for(ana, stage, Loading::Loaded).unwrap();
    let cy = r.join("cy");
    r.load_for(cy, stage, Loading::Retrying).unwrap();
    r.ready_for(host, stage, true).unwrap();
    r.ready_for(ana, stage, true).unwrap();
    assert!(matches!(r.s.phase, Phase::Preparing { .. }));
    r.wait(6_000);
    load_and_ready(&mut r, stage, &[cy]);
    let Phase::Countdown { .. } = r.s.phase else {
        panic!("{:?}", r.s.phase)
    };
    r.wait(r.s.policy.readiness.countdown_ms);
    assert!(r.s.stage(stage).unwrap().play.as_ref().unwrap().lineup.contains(&cy));

    // The window's own deadline before the grace's end (everyone loaded
    // a second in, the window to 31 s): the hold ends with it.
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    let mut r = shared(rules);
    let host = r.host;
    let ana = r.join("ana");
    let stage = r.pick(1);
    available(&mut r, stage);
    let opened = r.now;
    r.wait(1_000);
    r.load_for(host, stage, Loading::Loaded).unwrap();
    r.load_for(ana, stage, Loading::Loaded).unwrap();
    let cy = r.join("cy");
    r.load_for(cy, stage, Loading::Retrying).unwrap();
    r.ready_for(host, stage, true).unwrap();
    r.ready_for(ana, stage, true).unwrap();
    let deadline = r.window().deadline.0;
    assert!(deadline < opened + grace);
    assert_eq!(r.s.next_deadline().map(|m| m.0), Some(deadline));
    r.wait_to(deadline);
    assert!(matches!(r.s.phase, Phase::Countdown { .. }), "{:?}", r.s.phase);

    // Nobody retrying: the early end as before (no hold, no event at
    // the grace's end).
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    let mut r = shared(rules);
    let host = r.host;
    let ana = r.join("ana");
    let stage = r.pick(1);
    available(&mut r, stage);
    load_and_ready(&mut r, stage, &[host, ana]);
    assert!(matches!(r.s.phase, Phase::Countdown { .. }), "{:?}", r.s.phase);
}
