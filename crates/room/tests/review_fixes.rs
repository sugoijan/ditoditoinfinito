//! The room model's fixes from the code review of 2026-11-15 (fix round
//! G1): each case as the review described it, every revision's events
//! applied to a mirror, which must stay equal.

#![allow(dead_code)]

mod common;

use common::{Room, song};
use ddi_room::*;

impl Room {
    /// A hosted online room (its songs shared) of two stages under
    /// `policy`.
    fn with(policy: RoomPolicy) -> Self {
        let mut rules = MatchRules::hosted();
        rules.stages = 2;
        Self::open("host", "Host", policy, rules, ContentSource::Shared)
    }

    fn new() -> Self {
        Self::with(RoomPolicy::default())
    }

    fn join(&mut self, name: &str) -> ParticipantId {
        self.join_as(name, Intent::Play, false).unwrap()
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

    fn load_and_ready(&mut self, who: ParticipantId, stage: StageId) {
        self.load_for(who, stage, Loading::Loaded).unwrap();
        self.ready_for(who, stage, true).unwrap();
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
    })
    .unwrap();
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
    let mut r = Room::with(RoomPolicy {
        max_members: 3,
        ..RoomPolicy::default()
    });
    let ana = r.join("ana");
    r.join("bo");
    r.by(ana, Command::Leave).unwrap();
    r.join("cy");
    assert_eq!(r.s.present_people(), 3);
    assert_eq!(r.join_as("ana", Intent::Play, false), Err(Rejection::RoomFull));
    assert!(r.s.members[&ana].left, "still gone");
    assert_eq!(r.s.present_people(), 3);
}

/// First pass 2: a player who left, back when every player's place is
/// taken, comes back to watch (`join-watch-v0`, as a newcomer would)
/// rather than past the competitor cap.
#[test]
fn a_returning_player_comes_back_to_watch_at_the_competitor_cap() {
    let mut r = Room::with(RoomPolicy {
        max_competitors: 2,
        ..RoomPolicy::default()
    });
    let ana = r.join("ana");
    r.by(ana, Command::Leave).unwrap();
    let bo = r.join("bo");
    assert_eq!(r.s.members[&bo].intent, Intent::Play);
    assert!(r.s.players_full());
    assert_eq!(r.join_as("ana", Intent::Play, false), Ok(ana), "the same row");
    assert!(!r.s.members[&ana].left);
    assert_eq!(r.s.members[&ana].intent, Intent::Watch);
    assert_eq!(r.s.reserved_humans(), 2, "never past the cap");
    // A returning member with room to play keeps their Play.
    let mut r = Room::with(RoomPolicy::default());
    let ana = r.join("ana");
    r.by(ana, Command::Leave).unwrap();
    r.join_as("ana", Intent::Play, false).unwrap();
    assert_eq!(r.s.members[&ana].intent, Intent::Play);
}

/// First pass 4: a committed starter who chooses Watch during the
/// countdown cancels the start, as one leaving or disconnecting does: no
/// watcher is left in the lineup.
#[test]
fn choosing_watch_after_the_start_committed_cancels_the_start() {
    let mut r = Room::new();
    let ana = r.join("ana");
    r.pick(1).unwrap();
    let s1 = r.s.upcoming_stage().unwrap();
    r.sys(Command::SetContentStatus {
        stage: s1,
        status: ContentStatus::Available,
        share: Some("share-1".into()),
    })
    .unwrap();
    let host = r.host;
    r.load_and_ready(host, s1);
    r.load_and_ready(ana, s1);
    assert!(matches!(r.s.phase, Phase::Countdown { .. }), "{:?}", r.s.phase);
    assert!(r.s.stage(s1).unwrap().play.as_ref().unwrap().lineup.contains(&ana));
    let events = r
        .by(
            ana,
            Command::SetIntent {
                intent: Intent::Watch,
            },
        )
        .unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::StartCanceled {
            reason: CancelReason::RosterChanged,
            ..
        }
    )));
    assert!(matches!(r.s.phase, Phase::Preparing { window: None, .. }));
    assert!(r.s.stage(s1).unwrap().play.is_none());
}
