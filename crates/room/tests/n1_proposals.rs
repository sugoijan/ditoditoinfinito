//! Feedback round N1 (2026-11-16): what a group of 5 to 15 people hit.
//! That round stated each as a failing, ignored test of a change proposed
//! to the room model; round N2 made the four changes in room model
//! revision 12, and the tests run.

#![allow(dead_code)]

mod common;

use common::song;
use ddi_room::*;

struct Room {
    s: RoomState,
    now: u64,
    host: ParticipantId,
}

impl Room {
    /// A hosted online room (its songs shared) of two stages whose owner
    /// is `owner`.
    fn of(owner: &str) -> Self {
        let mut rules = MatchRules::hosted();
        rules.stages = 2;
        let s = RoomState::new(
            RoomId::new("room"),
            Identity::new(format!("twitch:{owner}")),
            owner.into(),
            RoomPolicy::default(),
            rules,
            ContentSource::Shared,
        );
        Self {
            s,
            now: 0,
            host: ParticipantId(1),
        }
    }

    fn by(&mut self, who: ParticipantId, cmd: Command) -> Result<Vec<Event>, Rejection> {
        self.s
            .apply(Actor::Member(who), Some(self.s.revision), &cmd, Millis(self.now))
    }

    fn sys(&mut self, cmd: Command) -> Vec<Event> {
        self.s
            .apply(Actor::Authority, None, &cmd, Millis(self.now))
            .unwrap()
    }

    /// The authority's join of `name` (as the server sends it for a
    /// newcomer, for one who had left, and could for anyone connecting).
    fn join(&mut self, name: &str, admin: bool) -> ParticipantId {
        let identity = Identity::new(format!("twitch:{name}"));
        self.sys(Command::Join {
            identity: identity.clone(),
            name: name.into(),
            admin,
            intent: Intent::Play,
        });
        self.s.member_by_identity(&identity).unwrap().id
    }

    fn may(&self, who: ParticipantId, capability: Capability) -> bool {
        let m = self.s.member(who).unwrap();
        can(m.role, m.admin, capability, self.s.phase.kind())
    }

    /// The host's pick, its song there for everyone: the readiness window.
    fn picked(&mut self) -> StageId {
        self.by(
            self.host,
            Command::Pick {
                content: song(1),
                card: None,
            },
        )
        .unwrap();
        let stage = self.s.upcoming_stage().unwrap();
        self.sys(Command::SetContentStatus {
            stage,
            status: ContentStatus::Available,
            share: Some("share".into()),
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

    /// Time passes: what the room's deadlines decide.
    fn pass(&mut self, ms: u64) {
        self.now += ms;
        self.s.advance(Millis(self.now));
    }
}

/// Item 4: a server administrator who made a room is its owner with
/// `admin: false` (`RoomState::new` knows no administrator), and the flag
/// is only ever set by a newcomer's join: `Command::Join` for a member
/// already in the room's list (present, or back after leaving) restores
/// the row and passes its `admin` over. So an administrator whose room
/// passed to someone else comes back a plain member, with none of the
/// co-host powers an administrator has everywhere else (the local server
/// showed the same: `take_pick` refused `unauthorized`, while the HTTP
/// routes, which ask the session, still treated them as an operator).
///
/// Proposed: `Command::Join` for an existing member whose `admin` differs
/// from the join's sets it (a new `Event::AdminChanged { participant,
/// admin }`, additive; a bot never), the server sends that join (or a
/// `SetAdmin` of the authority's) whenever a member's socket opens with a
/// ticket whose `admin` differs from the row's, and `RoomState::new`
/// takes the owner's flag.
#[test]
fn an_administrator_who_comes_back_is_still_an_administrator() {
    let mut r = Room::of("boss");
    let boss = r.host;
    let carl = r.join("carl", false);
    // The administrator leaves; the room passes to Carl.
    r.by(boss, Command::Leave).unwrap();
    assert_eq!(r.s.member(carl).unwrap().role, Role::Owner);
    // Back, as the server joins an administrator.
    assert_eq!(r.join("boss", true), boss);
    let m = r.s.member(boss).unwrap();
    assert!(!m.left);
    assert_eq!(m.role, Role::Member, "the one before a member: by design");
    assert!(m.admin, "an administrator wherever they are");
    for capability in [
        Capability::TakePick,
        Capability::ChooseContent,
        Capability::EditNextRules,
        Capability::LetPlay,
        Capability::CloseAudience,
        Capability::ReplaceInvite,
    ] {
        assert!(r.may(boss, capability), "{capability:?}");
    }
    // The owner's alone stay the owner's.
    assert!(!r.may(boss, Capability::ManageCoHosts));
}

/// Item 2: someone whose song was still loading when the stage started
/// is recorded as `NotReady` wherever they had not pressed Ready, which
/// they could not sensibly do (the Ready screen says the song is loading):
/// `sit_out_reason` asks readiness before loading. The screens now read
/// the reason from the stage's preparation instead
/// (`ddi_ui::screens::room::sat_out_why`); the record itself is wrong for
/// the standings and for any other reader.
///
/// Proposed: `sit_out_reason` asks the loading first (`StillLoading` for
/// `NotStarted` and `Loading`), then readiness.
#[test]
fn one_whose_song_was_still_loading_sits_out_as_still_loading() {
    let mut r = Room::of("host");
    let ana = r.join("ana", false);
    let slow = r.join("slow", false);
    let stage = r.picked();
    for who in [r.host, ana] {
        r.load(who, stage, Loading::Loaded);
        r.ready(who, stage);
    }
    r.load(slow, stage, Loading::Loading { permille: 400 });
    // The loading grace, then the Ready window: the stage starts.
    r.pass(60_000);
    r.pass(60_000);
    let play = r.s.stage(stage).unwrap().play.as_ref().expect("started");
    assert_eq!(play.sat_out.get(&slow), Some(&SitOutReason::StillLoading));
}

/// Item 1: a member whose socket dropped a moment ago (`Reconnecting`:
/// inside the server's reconnect grace, back within a second where their
/// page runs) is counted out of a stage that starts in that moment, ready
/// and loaded as they are: `sit_out_reason` takes anything but
/// `Connected` as `NotConnected`, and a start never waits for them. A
/// restart of the server's room (every socket closes at once, everyone
/// reconnects within a second) at the wrong moment starts a stage with
/// whoever came back first.
///
/// Proposed: `Reconnecting` counts as present for a lineup (ready and
/// loaded: in; the live rules' no-show and late join deal with one who
/// does not come back), and a readiness window waits for a reconnecting
/// member as it does for a connected one, up to its deadline.
#[test]
fn a_member_reconnecting_for_a_moment_keeps_their_place_in_the_stage() {
    let mut r = Room::of("host");
    let ana = r.join("ana", false);
    let bea = r.join("bea", false);
    let stage = r.picked();
    for who in [ana, bea] {
        r.load(who, stage, Loading::Loaded);
        r.ready(who, stage);
    }
    r.load(r.host, stage, Loading::Loaded);
    // Bea's socket drops; a second later she would be back.
    r.sys(Command::SetConnection {
        participant: bea,
        connection: Connection::Reconnecting,
    });
    // The host readies in that second: the stage starts.
    r.ready(r.host, stage);
    r.pass(1_000);
    let play = r.s.stage(stage).unwrap().play.as_ref().expect("started");
    assert!(play.lineup.contains(&bea), "ready and loaded: in");
}

/// Item 1, seen on a local server: a member on the Ready screen, loaded
/// and not yet ready, whose socket dropped for 0.3 s (their page opened
/// it again by itself) lost the stage in that instant: the others were
/// ready, and a readiness window never waits for anyone not `Connected`
/// (`start_committed … sat_out [[4, "not_connected"]]` in the same
/// revision as `connection_changed … reconnecting`), though 12 s of
/// their Ready window were left.
///
/// Proposed, with the test above: a window waits for a `Reconnecting`
/// member as for a connected one (its deadline bounds the wait; the
/// authority ends `Reconnecting` with `Disconnected` after its grace).
#[test]
fn a_readiness_window_waits_for_a_member_reconnecting_for_a_moment() {
    let mut r = Room::of("host");
    let ana = r.join("ana", false);
    let bea = r.join("bea", false);
    let stage = r.picked();
    for who in [r.host, ana, bea] {
        r.load(who, stage, Loading::Loaded);
    }
    r.ready(r.host, stage);
    r.ready(ana, stage);
    // Everyone loaded: the Ready window counts; Bea has not pressed yet.
    r.pass(2_000);
    assert!(matches!(r.s.phase, Phase::Preparing { window: Some(_), .. }));
    // Her socket drops.
    r.sys(Command::SetConnection {
        participant: bea,
        connection: Connection::Reconnecting,
    });
    assert!(
        matches!(r.s.phase, Phase::Preparing { window: Some(_), .. }),
        "the window still waits for her: {:?}",
        r.s.phase
    );
    // Back a moment later, she readies: the stage starts with her.
    r.sys(Command::SetConnection {
        participant: bea,
        connection: Connection::Connected,
    });
    r.ready(bea, stage);
    r.pass(1_000);
    let play = r.s.stage(stage).unwrap().play.as_ref().expect("started");
    assert!(play.lineup.contains(&bea));
}

/// Round N2, with the first test above: the flag follows the join both
/// ways (someone who is no longer an administrator loses it, as one
/// event and nothing else for a member who is there), a bot is never
/// one, and a room's creation takes its owner's.
#[test]
fn the_administrators_flag_follows_the_join_both_ways() {
    let mut r = Room::of("boss");
    let op = r.join("op", true);
    assert!(r.s.member(op).unwrap().admin);
    // Connected, the same flag: nothing changes.
    let rev = r.s.revision;
    r.join("op", true);
    assert_eq!(r.s.revision, rev);
    // No longer an administrator: one event, the row brought to it.
    let events = r.sys(Command::Join {
        identity: Identity::new("twitch:op"),
        name: "op".into(),
        admin: false,
        intent: Intent::Play,
    });
    assert_eq!(
        events,
        vec![Event::AdminChanged {
            participant: op,
            admin: false
        }]
    );
    assert!(!r.s.member(op).unwrap().admin);
    assert!(!r.may(op, Capability::TakePick));
    // A bot never, whatever its join says.
    let identity = Identity::new("bot:rob");
    assert!(identity.is_bot());
    for _ in 0..2 {
        r.sys(Command::Join {
            identity: identity.clone(),
            name: "rob".into(),
            admin: true,
            intent: Intent::Play,
        });
    }
    assert!(!r.s.member_by_identity(&identity).unwrap().admin);
    // A mirror applies the event as the authority did.
    let mut mirror = r.s.clone();
    let events = r.sys(Command::Join {
        identity: Identity::new("twitch:op"),
        name: "op".into(),
        admin: true,
        intent: Intent::Play,
    });
    mirror.commit(&events);
    assert_eq!(mirror, r.s);
    // The owner's flag at the room's creation.
    let made = |admin| {
        RoomState::new(
            RoomId::new("room"),
            Identity::new("twitch:boss"),
            "boss".into(),
            RoomPolicy::default(),
            MatchRules::hosted(),
            ContentSource::Shared,
        )
        .with_owner_admin(admin)
    };
    assert!(made(true).member(ParticipantId(1)).unwrap().admin);
    assert!(!made(false).member(ParticipantId(1)).unwrap().admin);
}

/// Round N2, the bounds of waiting for a reconnecting member: the loading
/// grace waits for one still loading as for a connected one; past the
/// authority's reconnect grace (`Disconnected`) nothing waits, and a
/// committed starter who goes then cancels the start, as ever.
#[test]
fn a_reconnecting_member_is_waited_for_only_inside_the_reconnect_grace() {
    let mut r = Room::of("host");
    let ana = r.join("ana", false);
    let bea = r.join("bea", false);
    let stage = r.picked();
    for who in [r.host, ana] {
        r.load(who, stage, Loading::Loaded);
        r.ready(who, stage);
    }
    r.load(bea, stage, Loading::Loading { permille: 500 });
    r.sys(Command::SetConnection {
        participant: bea,
        connection: Connection::Reconnecting,
    });
    r.pass(1_000);
    let Phase::Preparing { window: Some(w), .. } = r.s.phase else {
        panic!("{:?}", r.s.phase)
    };
    assert!(w.loading(), "the loading grace waits for her");
    // Gone past the grace: the room goes on without her.
    r.sys(Command::SetConnection {
        participant: bea,
        connection: Connection::Disconnected,
    });
    r.pass(1);
    assert!(matches!(r.s.phase, Phase::Countdown { .. }), "{:?}", r.s.phase);
    let play = r.s.stage(stage).unwrap().play.as_ref().unwrap();
    assert_eq!(play.sat_out.get(&bea), Some(&SitOutReason::NotConnected));

    // A reconnecting starter committed, then gone: the start is canceled.
    let mut r = Room::of("host");
    let ana = r.join("ana", false);
    let bea = r.join("bea", false);
    let stage = r.picked();
    for who in [ana, bea] {
        r.load(who, stage, Loading::Loaded);
        r.ready(who, stage);
    }
    r.load(r.host, stage, Loading::Loaded);
    r.sys(Command::SetConnection {
        participant: bea,
        connection: Connection::Reconnecting,
    });
    r.ready(r.host, stage);
    assert!(matches!(r.s.phase, Phase::Countdown { .. }));
    let events = r.sys(Command::SetConnection {
        participant: bea,
        connection: Connection::Disconnected,
    });
    assert!(events.iter().any(|e| matches!(e, Event::StartCanceled { .. })));
}
