//! `hosted-deadline-v2` (follow-up round F, the maintainer's of
//! 2026-11-14: "a separate 'grace' for the loading and the ready stays
//! short"): after a pick the room waits for everyone connected who could
//! play to load the song, up to the loading grace counted from the song's
//! availability, and only then counts the Ready countdown (the readiness
//! window), which ends early once everyone who loaded is ready. Every
//! revision's events are applied to a mirror, which must stay equal.

mod common;

use common::{Room, song};
use ddi_room::*;

const GRACE: u64 = ReadinessPolicy::LOADING_GRACE_V2_MS;
/// The deployed Ready countdown.
const WINDOW: u64 = 15_000;

impl Room {
    /// A hosted online room (its songs shared) under `hosted-deadline-v2`
    /// with the deployed 15 s window.
    fn new() -> Self {
        let mut policy = RoomPolicy::default();
        assert_eq!(policy.readiness.id, ReadinessPolicy::HOSTED_DEADLINE_V2);
        assert_eq!(policy.readiness.loading_grace_ms, GRACE);
        policy.readiness.window_ms = WINDOW;
        let mut rules = MatchRules::hosted();
        rules.stages = 2;
        Self::open("host", "Host", policy, rules, ContentSource::Shared)
    }

    fn join(&mut self, name: &str) -> ParticipantId {
        self.join_as(name, Intent::Play, false).unwrap()
    }

    /// The host picks song `n`; its share is up `shared_after` ms later.
    fn pick(&mut self, n: u32, shared_after: u64) -> StageId {
        self.by(
            self.host,
            Command::Pick {
                content: song(n),
                card: None,
            },
        )
        .unwrap();
        let stage = self.s.upcoming_stage().unwrap();
        self.wait_to(self.now + shared_after);
        self.sys(Command::SetContentStatus {
            stage,
            status: ContentStatus::Available,
            share: Some(format!("share-{n}")),
        })
        .unwrap();
        stage
    }

    fn load(&mut self, who: ParticipantId, stage: StageId, loading: Loading) {
        self.load_for(who, stage, loading).unwrap();
    }

    fn ready(&mut self, who: ParticipantId, stage: StageId) {
        self.ready_for(who, stage, true).unwrap();
    }

    fn window(&self) -> ReadinessWindow {
        match self.s.phase {
            Phase::Preparing {
                window: Some(w), ..
            } => w,
            ref p => panic!("no readiness window: {p:?}"),
        }
    }

    fn counting_down(&self) -> bool {
        matches!(self.s.phase, Phase::Countdown { .. })
    }
}

/// Everyone loads at once: the Ready countdown starts as soon as the last
/// one has, and ends as soon as all are ready.
#[test]
fn all_load_fast_the_countdown_starts_at_once() {
    let mut r = Room::new();
    let (host, ana) = (r.host, r.join("ana"));
    let s1 = r.pick(1, 2_000);
    let w = r.window();
    assert_eq!(w.loading_until, Some(Millis(2_000 + GRACE)), "from availability");
    assert_eq!(r.s.next_deadline(), Some(Millis(2_000 + GRACE)));
    r.wait_to(3_000);
    r.load(host, s1, Loading::Loaded);
    assert!(r.window().loading(), "ana still loading");
    r.wait_to(3_500);
    r.load(ana, s1, Loading::Loaded);
    let w = r.window();
    assert!(!w.loading());
    assert_eq!(w.deadline, Millis(3_500 + WINDOW), "the countdown from the last load");
    assert_eq!(r.s.next_deadline(), Some(w.deadline));
    r.ready(host, s1);
    r.ready(ana, s1);
    assert!(r.counting_down(), "everyone loaded is ready: at once");
}

/// One slow loader: the countdown waits for them, then starts.
#[test]
fn one_slow_loader_holds_the_countdown_until_they_load() {
    let mut r = Room::new();
    let (host, ana) = (r.host, r.join("ana"));
    let s1 = r.pick(1, 0);
    r.load(host, s1, Loading::Loaded);
    r.ready(host, s1);
    r.load(ana, s1, Loading::Loading { permille: 300 });
    r.wait_to(30_000);
    assert!(r.window().loading(), "still waiting for ana at 30 s");
    r.load(ana, s1, Loading::Loaded);
    let w = r.window();
    assert!(!w.loading());
    assert_eq!(w.deadline, Millis(30_000 + WINDOW));
    // The host is ready, ana loaded but not: the countdown runs.
    r.wait_to(30_000 + WINDOW - 1);
    assert!(matches!(r.s.phase, Phase::Preparing { .. }));
    r.ready(ana, s1);
    assert!(r.counting_down());
}

/// One who never loads: the countdown starts at the grace's end, the
/// others play, and the one still loading sits the stage out (as one who
/// was not ready), back in for the next stage.
#[test]
fn one_who_never_loads_sits_out_after_the_grace_and_the_countdown() {
    let mut r = Room::new();
    let (host, ana) = (r.host, r.join("ana"));
    let bo = r.join("bo");
    let s1 = r.pick(1, 0);
    for p in [host, bo] {
        r.load(p, s1, Loading::Loaded);
        r.ready(p, s1);
    }
    r.load(ana, s1, Loading::Loading { permille: 0 });
    r.wait_to(GRACE - 1);
    assert!(r.window().loading());
    // The grace over and everyone who loaded ready: the Ready countdown
    // does not wait for the one still loading.
    r.wait_to(GRACE);
    assert!(r.counting_down(), "{:?}", r.s.phase);
    let countdown = r.s.policy.readiness.countdown_ms;
    r.wait_to(GRACE + countdown);
    assert_eq!(r.s.phase, Phase::Stage { stage: s1 });
    let play = r.s.stage(s1).unwrap().play.clone().unwrap();
    assert!(play.lineup.contains(&host) && play.lineup.contains(&bo));
    // Revision 12: the loading is asked before the readiness.
    assert_eq!(play.sat_out.get(&ana), Some(&SitOutReason::StillLoading));
    assert_eq!(
        r.s.participation(ana),
        Some(Participation::SittingOut {
            stage: s1,
            reason: SitOutReason::StillLoading
        })
    );
}

/// One who loads during the Ready countdown may still ready and play:
/// where someone loaded is not ready yet the countdown runs, and a late
/// loader readies in time.
#[test]
fn a_late_loader_may_still_ready_during_the_countdown() {
    let mut r = Room::new();
    let (host, ana) = (r.host, r.join("ana"));
    let bo = r.join("bo");
    let s1 = r.pick(1, 0);
    r.load(host, s1, Loading::Loaded);
    r.ready(host, s1);
    r.load(bo, s1, Loading::Loaded);
    // Ana loads nothing in the grace; bo, loaded, is not ready yet.
    r.wait_to(GRACE);
    assert!(!r.window().loading());
    assert!(!r.counting_down(), "bo loaded and not ready: the countdown runs");
    r.wait_to(GRACE + 5_000);
    r.load(ana, s1, Loading::Loaded);
    r.ready(ana, s1);
    assert!(!r.counting_down());
    r.ready(bo, s1);
    assert!(r.counting_down(), "everyone loaded is ready");
    let play = r.s.stage(s1).unwrap().play.clone().unwrap();
    assert!(play.lineup.contains(&ana) && play.lineup.contains(&bo));
}

/// The grace counts from the song's availability: a share that takes a
/// while to upload does not eat it.
#[test]
fn the_grace_counts_from_the_songs_availability() {
    let mut r = Room::new();
    let ana = r.join("ana");
    let _ = ana;
    r.by(
        r.host,
        Command::Pick {
            content: song(1),
            card: None,
        },
    )
    .unwrap();
    let s1 = r.s.upcoming_stage().unwrap();
    // Pending: no alarm while the song is shared.
    assert_eq!(r.s.next_deadline(), None);
    r.wait_to(GRACE + 10_000);
    assert!(r.window().loading());
    r.sys(Command::SetContentStatus {
        stage: s1,
        status: ContentStatus::Available,
        share: Some("share-1".into()),
    })
    .unwrap();
    assert_eq!(r.window().loading_until, Some(Millis(GRACE + 10_000 + GRACE)));
}

/// Round P1b (the maintainer's of 2026-11-16: "45 seconds flat, but we
/// should start counting after uploads finishes"): a pick whose upload
/// takes 5 s or 60 s, everyone else's whole grace counts from the end of
/// the upload. The picker's own device has the song from the start (its
/// Loaded counts), nobody else can load before the share is there, and
/// nothing ends while it is uploaded however long that takes.
#[test]
fn the_grace_counts_from_the_end_of_a_5_s_and_a_60_s_upload() {
    for upload in [5_000, 60_000] {
        let mut r = Room::new();
        let (host, ana) = (r.host, r.join("ana"));
        r.by(
            host,
            Command::Pick {
                content: song(1),
                card: None,
            },
        )
        .unwrap();
        let s1 = r.s.upcoming_stage().unwrap();
        assert!(r.s.content_pending(s1));
        r.load(host, s1, Loading::Loaded);
        r.ready(host, s1);
        // Throughout the upload: no alarm, nothing ends, ana is waiting
        // for the song (she reported nothing).
        r.wait_to(upload - 1);
        assert_eq!(r.s.next_deadline(), None, "{upload}");
        assert!(r.window().loading(), "{upload}");
        assert_eq!(r.s.stage(s1).unwrap().prep(ana).loading, Loading::NotStarted);
        r.wait_to(upload);
        r.sys(Command::SetContentStatus {
            stage: s1,
            status: ContentStatus::Available,
            share: Some("share-1".into()),
        })
        .unwrap();
        let w = r.window();
        assert_eq!(w.loading_until, Some(Millis(upload + GRACE)), "{upload}");
        assert_eq!(w.deadline, Millis(upload + GRACE + WINDOW), "{upload}");
        // Ana loading slowly keeps the whole grace after the upload.
        r.load(ana, s1, Loading::Loading { permille: 100 });
        r.wait_to(upload + GRACE - 1);
        assert!(r.window().loading(), "{upload}");
        r.wait_to(upload + GRACE);
        assert!(!r.window().loading(), "{upload}");
        assert_eq!(r.window().deadline, Millis(upload + GRACE + WINDOW));
    }
}

/// The pick of the song the picker rested on, whose share the spotlight
/// already made: available at the pick, the grace from it.
#[test]
fn a_pick_that_reuses_the_resting_songs_share_counts_from_the_pick() {
    let mut r = Room::new();
    let host = r.host;
    let _ana = r.join("ana");
    r.wait_to(3_000);
    r.by(
        host,
        Command::Spotlight {
            content: Some(song(1)),
            card: None,
        },
    )
    .unwrap();
    r.sys(Command::SetSpotlightShare {
        chart: song(1).chart,
        share: "share-spot".into(),
    })
    .unwrap();
    r.wait_to(7_000);
    r.by(
        host,
        Command::Pick {
            content: song(1),
            card: None,
        },
    )
    .unwrap();
    let s1 = r.s.upcoming_stage().unwrap();
    assert!(!r.s.content_pending(s1));
    let c = r.s.stage(s1).unwrap().content.clone().unwrap();
    assert_eq!(c.share.as_deref(), Some("share-spot"));
    assert_eq!(r.window().loading_until, Some(Millis(7_000 + GRACE)));
    // A spotlight share finished after the pick (the picker picked while
    // the resting song was still being uploaded): from its end.
    let mut r = Room::new();
    let host = r.host;
    let _ana = r.join("ana");
    r.by(
        host,
        Command::Spotlight {
            content: Some(song(2)),
            card: None,
        },
    )
    .unwrap();
    r.by(
        host,
        Command::Pick {
            content: song(2),
            card: None,
        },
    )
    .unwrap();
    let s1 = r.s.upcoming_stage().unwrap();
    assert!(r.s.content_pending(s1));
    r.wait_to(12_000);
    r.sys(Command::SetSpotlightShare {
        chart: song(2).chart,
        share: "share-late".into(),
    })
    .unwrap();
    assert!(!r.s.content_pending(s1));
    assert_eq!(r.window().loading_until, Some(Millis(12_000 + GRACE)));
}

/// `hosted-deadline-v1`, kept by name: no grace, the window from the
/// song's availability as before; and practice keeps it.
#[test]
fn v1_and_practice_have_no_loading_grace() {
    let v1 = ReadinessPolicy::v1();
    assert_eq!(v1.id, ReadinessPolicy::HOSTED_DEADLINE_V1);
    assert_eq!(v1.grace(), None);
    // Read from JSON written before v2: no grace.
    let mut json = serde_json::to_value(&v1).unwrap();
    assert!(json.get("loading_grace_ms").is_none());
    json.as_object_mut().unwrap().remove("loading_grace_ms");
    let read: ReadinessPolicy = serde_json::from_value(json).unwrap();
    assert_eq!(read.grace(), None);
    let (practice, _) = LocalAuthority::practice(
        "Player".into(),
        MatchRules::practice(OpponentSpec {
            level: 1,
            seed: 7,
            personality: None,
            name: None,
            targeting: None,
        }),
        ManualClock::new(0),
    );
    assert_eq!(practice.state().policy.readiness.grace(), None);
}
