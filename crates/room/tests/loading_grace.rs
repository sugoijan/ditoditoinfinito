//! `hosted-deadline-v2` (follow-up round F, the maintainer's of
//! 2026-11-14: "a separate 'grace' for the loading and the ready stays
//! short"): after a pick the room waits for everyone connected who could
//! play to load the song, up to the loading grace counted from the song's
//! availability, and only then counts the Ready countdown (the readiness
//! window), which ends early once everyone who loaded is ready. Every
//! revision's events are applied to a mirror, which must stay equal.

use ddi_room::*;

const GRACE: u64 = ReadinessPolicy::LOADING_GRACE_V2_MS;
/// The deployed Ready countdown.
const WINDOW: u64 = 15_000;

struct Room {
    s: RoomState,
    mirror: RoomState,
    now: u64,
    host: ParticipantId,
}

fn song(n: u32) -> ContentId {
    ContentId {
        song: SongKey::new(format!("song-{n}")),
        chart: ChartKey::new(format!("digest-{n}")),
    }
}

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
        let s = RoomState::new(
            RoomId::new("room"),
            Identity::new("twitch:host"),
            "Host".into(),
            policy,
            rules,
            ContentSource::Shared,
        );
        Self {
            mirror: s.clone(),
            s,
            now: 0,
            host: ParticipantId(1),
        }
    }

    fn follow(&mut self, events: &[Event]) {
        if !events.is_empty() {
            self.mirror.commit(events);
        }
        assert_eq!(self.mirror, self.s, "the mirror follows the events");
    }

    fn by(&mut self, who: ParticipantId, cmd: Command) -> Result<Vec<Event>, Rejection> {
        let r = self.s.apply(
            Actor::Member(who),
            Some(self.s.revision),
            &cmd,
            Millis(self.now),
        );
        if let Ok(events) = &r {
            self.follow(&events.clone());
        }
        r
    }

    fn sys(&mut self, cmd: Command) -> Vec<Event> {
        let events = self
            .s
            .apply(Actor::Authority, None, &cmd, Millis(self.now))
            .unwrap();
        self.follow(&events);
        events
    }

    fn join(&mut self, name: &str) -> ParticipantId {
        let identity = Identity::new(format!("twitch:{name}"));
        self.sys(Command::Join {
            identity: identity.clone(),
            name: name.into(),
            admin: false,
            intent: Intent::Play,
        });
        self.s.member_by_identity(&identity).unwrap().id
    }

    /// Time passes to `ms` after the start: the authority's alarm at
    /// every deadline the room names on the way, as a server's is.
    fn wait_to(&mut self, ms: u64) {
        loop {
            match self.s.next_deadline() {
                Some(at) if at.0 <= ms && at.0 > self.now => {
                    self.now = at.0;
                    let events = self.s.advance(Millis(self.now));
                    self.follow(&events);
                }
                _ => break,
            }
        }
        self.now = ms;
        let events = self.s.advance(Millis(self.now));
        self.follow(&events);
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
        });
        stage
    }

    fn rev(&self, stage: StageId) -> u32 {
        self.s.stage(stage).unwrap().readiness_rev
    }

    fn load(&mut self, who: ParticipantId, stage: StageId, loading: Loading) {
        let readiness_rev = self.rev(stage);
        self.by(
            who,
            Command::ReportLoading {
                stage,
                readiness_rev,
                loading,
            },
        )
        .unwrap();
    }

    fn ready(&mut self, who: ParticipantId, stage: StageId) {
        let readiness_rev = self.rev(stage);
        self.by(
            who,
            Command::SetReady {
                stage,
                readiness_rev,
                ready: true,
            },
        )
        .unwrap();
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
    assert_eq!(play.sat_out.get(&ana), Some(&SitOutReason::NotReady));
    assert_eq!(
        r.s.participation(ana),
        Some(Participation::SittingOut {
            stage: s1,
            reason: SitOutReason::NotReady
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
    });
    assert_eq!(r.window().loading_until, Some(Millis(GRACE + 10_000 + GRACE)));
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
