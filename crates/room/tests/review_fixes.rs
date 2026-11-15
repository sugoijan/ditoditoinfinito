//! The room model's fixes from the code review of 2026-11-15 (fix round
//! G1): each case as the review described it, every revision's events
//! applied to a mirror, which must stay equal.

#![allow(dead_code)]

mod common;

use common::song;
use ddi_room::*;

struct Room {
    s: RoomState,
    mirror: RoomState,
    now: u64,
    host: ParticipantId,
}

impl Room {
    /// A hosted online room (its songs shared) of two stages under
    /// `policy`.
    fn with(policy: RoomPolicy) -> Self {
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

    fn new() -> Self {
        Self::with(RoomPolicy::default())
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

    fn sys_try(&mut self, cmd: Command) -> Result<Vec<Event>, Rejection> {
        let r = self.s.apply(Actor::Authority, None, &cmd, Millis(self.now));
        if let Ok(events) = &r {
            self.follow(&events.clone());
        }
        r
    }

    fn sys(&mut self, cmd: Command) -> Vec<Event> {
        self.sys_try(cmd).unwrap()
    }

    fn join_as(&mut self, name: &str, intent: Intent) -> Result<ParticipantId, Rejection> {
        let identity = Identity::new(format!("twitch:{name}"));
        self.sys_try(Command::Join {
            identity: identity.clone(),
            name: name.into(),
            admin: false,
            intent,
        })?;
        Ok(self.s.member_by_identity(&identity).unwrap().id)
    }

    fn join(&mut self, name: &str) -> ParticipantId {
        self.join_as(name, Intent::Play).unwrap()
    }

    fn pick(&mut self, n: u32) -> Result<Vec<Event>, Rejection> {
        self.by(
            self.host,
            Command::Pick {
                content: song(n),
                card: None,
            },
        )
    }

    fn rev(&self, stage: StageId) -> u32 {
        self.s.stage(stage).unwrap().readiness_rev
    }

    fn load_and_ready(&mut self, who: ParticipantId, stage: StageId) {
        let readiness_rev = self.rev(stage);
        self.by(
            who,
            Command::ReportLoading {
                stage,
                readiness_rev,
                loading: Loading::Loaded,
            },
        )
        .unwrap();
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

    fn status(&self, stage: StageId) -> ContentStatus {
        self.s.stage(stage).unwrap().content.as_ref().unwrap().status.clone()
    }
}

/// First pass 1: a song whose share failed (the room set it Unavailable,
/// which canceled the window) picked again is chosen again, Pending, so
/// it is shared anew; the window never opens on content nobody can load.
#[test]
fn a_song_picked_again_after_its_share_failed_is_shared_again() {
    let mut r = Room::new();
    r.join("ana");
    r.pick(1).unwrap();
    let s1 = r.s.upcoming_stage().unwrap();
    assert_eq!(r.status(s1), ContentStatus::Pending);
    let before = r.s.stage(s1).unwrap().content_changed_at;
    r.sys(Command::SetContentStatus {
        stage: s1,
        status: ContentStatus::Unavailable {
            code: "digest_mismatch".into(),
        },
        share: None,
    });
    assert!(matches!(
        r.s.phase,
        Phase::Preparing { window: None, .. }
    ));
    let rev = r.rev(s1);
    r.pick(1).unwrap();
    assert_eq!(
        r.status(s1),
        ContentStatus::Pending,
        "the same song chosen again, to be shared again"
    );
    assert!(r.s.stage(s1).unwrap().content_changed_at > before);
    assert!(r.rev(s1) > rev, "a fresh readiness revision");
}

/// First pass 2: a member who left, back when every place in the room is
/// taken, is refused as any arrival at the member bound is.
#[test]
fn a_returning_member_is_refused_at_the_member_bound() {
    let mut policy = RoomPolicy::default();
    policy.max_members = 3;
    let mut r = Room::with(policy);
    let ana = r.join("ana");
    r.join("bo");
    r.by(ana, Command::Leave).unwrap();
    r.join("cy");
    assert_eq!(r.s.present_people(), 3);
    assert_eq!(r.join_as("ana", Intent::Play), Err(Rejection::RoomFull));
    assert!(r.s.members[&ana].left, "still gone");
    assert_eq!(r.s.present_people(), 3);
}

/// First pass 2: a player who left, back when every player's place is
/// taken, comes back to watch (`join-watch-v0`, as a newcomer would)
/// rather than past the competitor cap.
#[test]
fn a_returning_player_comes_back_to_watch_at_the_competitor_cap() {
    let mut policy = RoomPolicy::default();
    policy.max_competitors = 2;
    let mut r = Room::with(policy);
    let ana = r.join("ana");
    r.by(ana, Command::Leave).unwrap();
    let bo = r.join("bo");
    assert_eq!(r.s.members[&bo].intent, Intent::Play);
    assert!(r.s.players_full());
    assert_eq!(r.join_as("ana", Intent::Play), Ok(ana), "the same row");
    assert!(!r.s.members[&ana].left);
    assert_eq!(r.s.members[&ana].intent, Intent::Watch);
    assert_eq!(r.s.reserved_humans(), 2, "never past the cap");
    // A returning member with room to play keeps their Play.
    let mut r = Room::with(RoomPolicy::default());
    let ana = r.join("ana");
    r.by(ana, Command::Leave).unwrap();
    r.join_as("ana", Intent::Play).unwrap();
    assert_eq!(r.s.members[&ana].intent, Intent::Play);
}
