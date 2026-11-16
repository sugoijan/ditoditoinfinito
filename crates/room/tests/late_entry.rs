//! A late entry (round P1b, the maintainer's of 2026-11-16: "Someone who
//! missed the Ready window … should they still get in?" "Yes, within a
//! few seconds"): a member counted out of a stage's lineup when its start
//! committed (not ready in time, still loading, reconnecting) who readies
//! within the live rules' late join after the song's time zero (5 s; the
//! countdown before it counts as before the start) enters the stage.
//! Every revision's events are applied to a mirror, which must stay
//! equal.

mod common;

use common::song;
use ddi_room::*;

struct Room {
    s: RoomState,
    mirror: RoomState,
    now: u64,
    host: ParticipantId,
}

const WINDOW: u64 = 15_000;

impl Room {
    fn new(live: bool) -> Self {
        let mut policy = RoomPolicy::default();
        if live {
            policy = policy.with_live();
        }
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
        assert_eq!(self.mirror, self.s);
    }

    fn by(&mut self, who: ParticipantId, cmd: Command) -> Result<Vec<Event>, Rejection> {
        let r = self
            .s
            .apply(Actor::Member(who), Some(self.s.revision), &cmd, Millis(self.now));
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

    fn rev(&self, stage: StageId) -> u32 {
        self.s.stage(stage).unwrap().readiness_rev
    }

    fn load(&mut self, who: ParticipantId, stage: StageId, loading: Loading) -> Result<Vec<Event>, Rejection> {
        let readiness_rev = self.rev(stage);
        self.by(
            who,
            Command::ReportLoading {
                stage,
                readiness_rev,
                loading,
            },
        )
    }

    fn ready(&mut self, who: ParticipantId, stage: StageId) -> Result<Vec<Event>, Rejection> {
        let readiness_rev = self.rev(stage);
        self.by(
            who,
            Command::SetReady {
                stage,
                readiness_rev,
                ready: true,
            },
        )
    }

    /// The host picks, everyone's song shared at once; host and bo load
    /// and ready, ana as `ana_loads` says; the window runs out.
    fn started_without_ana(&mut self, ana_loads: bool) -> (StageId, ParticipantId, ParticipantId) {
        let (host, ana, bo) = (self.host, self.join("ana"), self.join("bo"));
        self.by(
            host,
            Command::Pick {
                content: song(1),
                card: None,
            },
        )
        .unwrap();
        let s1 = self.s.upcoming_stage().unwrap();
        self.sys(Command::SetContentStatus {
            stage: s1,
            status: ContentStatus::Available,
            share: Some("share-1".into()),
        });
        for p in [host, bo] {
            self.load(p, s1, Loading::Loaded).unwrap();
            self.ready(p, s1).unwrap();
        }
        let ana_says = if ana_loads {
            Loading::Loaded
        } else {
            Loading::Loading { permille: 500 }
        };
        self.load(ana, s1, ana_says).unwrap();
        for _ in 0..1_200 {
            if matches!(self.s.phase, Phase::Countdown { .. }) {
                break;
            }
            let next = self.now + 100;
            self.wait_to(next);
        }
        assert!(matches!(self.s.phase, Phase::Countdown { .. }), "{:?}", self.s.phase);
        (s1, ana, bo)
    }

    fn play(&self, stage: StageId) -> StagePlay {
        self.s.stage(stage).unwrap().play.clone().unwrap()
    }
}

/// During the countdown: ana, loaded but not ready at the commit, readies
/// and is a starter (committed, in the lineup, out of the sit-outs); the
/// stage starts with her.
#[test]
fn a_ready_during_the_countdown_enters_the_stage() {
    let mut r = Room::new(true);
    let (s1, ana, _) = r.started_without_ana(true);
    assert_eq!(r.play(s1).sat_out.get(&ana), Some(&SitOutReason::NotReady));
    let events = r.ready(ana, s1).unwrap();
    assert_eq!(
        events,
        vec![Event::EnteredLate {
            stage: s1,
            participant: ana
        }]
    );
    let play = r.play(s1);
    assert!(play.lineup.contains(&ana) && !play.sat_out.contains_key(&ana));
    assert_eq!(r.s.participation(ana), Some(Participation::Committed { stage: s1 }));
    // Again: nothing more.
    assert_eq!(r.ready(ana, s1), Ok(Vec::new()));
    let countdown = r.s.policy.readiness.countdown_ms;
    r.wait_to(r.now + countdown);
    assert_eq!(r.s.phase, Phase::Stage { stage: s1 });
    assert_eq!(r.s.participation(ana), Some(Participation::Competing { stage: s1 }));
}

/// After the song's time zero: in at 1 s and 4 s, refused at 6 s
/// (`Rejection::LateEntryClosed`, the late join is 5 s), sitting out.
#[test]
fn a_ready_within_the_late_join_enters_and_one_after_it_is_refused() {
    for (after, enters) in [(1_000, true), (4_000, true), (5_000, true), (6_000, false)] {
        let mut r = Room::new(true);
        let (s1, ana, _) = r.started_without_ana(true);
        let zero = r.play(s1).song_zero_at.unwrap();
        r.wait_to(zero.0 + after);
        assert_eq!(r.s.phase, Phase::Stage { stage: s1 });
        let got = r.ready(ana, s1);
        if enters {
            assert!(got.is_ok(), "{after}: {got:?}");
            assert!(r.play(s1).lineup.contains(&ana));
        } else {
            assert_eq!(got, Err(Rejection::LateEntryClosed), "{after}");
            assert_eq!(r.play(s1).sat_out.get(&ana), Some(&SitOutReason::NotReady));
        }
    }
}

/// Still loading at the commit: her song's load is heard after it
/// (refused before round P1b, `StageStarted`), a Ready before it is
/// loaded refused (`NotLoaded`), then she enters.
#[test]
fn one_still_loading_at_the_commit_loads_then_enters() {
    let mut r = Room::new(true);
    let (s1, ana, _) = r.started_without_ana(false);
    assert_eq!(r.play(s1).sat_out.get(&ana), Some(&SitOutReason::StillLoading));
    assert_eq!(r.ready(ana, s1), Err(Rejection::NotLoaded));
    r.load(ana, s1, Loading::Loading { permille: 900 }).unwrap();
    r.load(ana, s1, Loading::Loaded).unwrap();
    r.ready(ana, s1).unwrap();
    assert!(r.play(s1).lineup.contains(&ana));
}

/// Who may not: one who never wanted to play (a watcher), one not
/// connected, a starter already in it (nothing), and a room that does not
/// play live (practice, a room before live play): as before, refused once
/// the start is committed. A load after the late join is refused as ever.
#[test]
fn a_late_entry_is_only_for_those_counted_out_and_only_live() {
    let mut r = Room::new(true);
    let (s1, ana, bo) = r.started_without_ana(true);
    assert_eq!(r.ready(bo, s1), Ok(Vec::new()), "a starter: nothing");
    r.sys(Command::SetConnection {
        participant: ana,
        connection: Connection::Reconnecting,
    });
    assert_eq!(r.ready(ana, s1), Err(Rejection::LateEntryClosed));
    r.sys(Command::SetConnection {
        participant: ana,
        connection: Connection::Connected,
    });
    let zero = r.play(s1).song_zero_at.unwrap();
    r.wait_to(zero.0 + 6_000);
    assert_eq!(
        r.load(ana, s1, Loading::Loaded),
        Err(Rejection::StageStarted)
    );
    // Not live: refused as ever.
    let mut r = Room::new(false);
    let (s1, ana, _) = r.started_without_ana(true);
    assert_eq!(r.ready(ana, s1), Err(Rejection::StartCommitted));
}
