//! Room model revision 18 (round TW1, `docs/plans/multiplayer-twitch.md`
//! B3, B5, B9, B10): a host's Kick (who may remove whom, what the room is
//! told, a start or a stage under way, coming back by the invite), the
//! mirror's removal (told to the others as a leave), and the room's Twitch
//! setting (who sets it, who may change it after the room changed hands,
//! its end when the one who set it leaves); a room stored by revision 17.
//! Every revision's events are applied to a mirror, which must stay equal.

mod common;

use common::{Room, song};
use ddi_room::*;

impl Room {
    fn hosted() -> Self {
        let mut policy = RoomPolicy::default().with_live().with_audience();
        policy.readiness.window_ms = 15_000;
        let mut rules = MatchRules::hosted();
        rules.stages = 3;
        Self::open("host", "Host", policy, rules, ContentSource::Shared)
    }

    fn join(&mut self, name: &str) -> ParticipantId {
        self.join_as(name, Intent::Play, false).unwrap()
    }

    fn co_host(&mut self, name: &str) -> ParticipantId {
        let id = self.join(name);
        let host = self.host;
        self.by(host, Command::SetCoHost { participant: id, on: true }).unwrap();
        id
    }

    fn kick(&mut self, by: ParticipantId, who: ParticipantId) -> Result<Vec<Event>, Rejection> {
        self.by(by, Command::Remove { participant: who })
    }

    fn left(&self, who: ParticipantId) -> bool {
        self.s.member(who).unwrap().left
    }

    /// `who`'s Twitch setting: their lists for the mirror, their channel's
    /// chat up to `chat`.
    fn setting(&self, who: ParticipantId, mirror: TwitchMirror, chat: TwitchChat) -> TwitchSetting {
        let name = match &self.s.member(who).unwrap().kind {
            MemberKind::Human { name, .. } => name.clone(),
            MemberKind::Cpu { .. } => unreachable!(),
        };
        TwitchSetting {
            by: who,
            mirror,
            chat,
            user_id: format!("{}", 1000 + who.0),
            login: name,
        }
    }

    fn set_twitch(&mut self, member: ParticipantId, twitch: Option<TwitchSetting>) -> Result<Vec<Event>, Rejection> {
        self.sys(Command::SetTwitch { member, twitch })
    }

    /// The host picks song `n` for the next stage, shared at once, and
    /// everyone in `ready` loads and readies: the start commits.
    fn counted_down(&mut self, n: u32, ready: &[ParticipantId]) -> StageId {
        let host = self.host;
        self.by(host, Command::Pick { content: song(n), card: None }).unwrap();
        let s = self.s.upcoming_stage().unwrap();
        self.sys(Command::SetContentStatus {
            stage: s,
            status: ContentStatus::Available,
            share: Some(format!("share-{n}")),
        })
        .unwrap();
        for &p in ready {
            self.load_for(p, s, Loading::Loaded).unwrap();
            self.ready_for(p, s, true).unwrap();
        }
        for _ in 0..400 {
            if matches!(self.s.phase, Phase::Countdown { .. }) {
                break;
            }
            let next = self.now + 100;
            self.wait_to(next);
        }
        assert!(matches!(self.s.phase, Phase::Countdown { .. }), "{:?}", self.s.phase);
        s
    }
}

fn unauthorized_kick() -> Rejection {
    Rejection::Unauthorized {
        capability: Capability::Kick,
    }
}

fn unauthorized_twitch() -> Rejection {
    Rejection::Unauthorized {
        capability: Capability::Twitch,
    }
}

/// B9: the host may kick anyone but themselves; a co-host ordinary
/// members only (never the host, another co-host or an administrator who
/// plays); a member nobody; nobody an administrator who observes, nor a
/// CPU, nor someone gone already.
#[test]
fn who_may_kick_whom() {
    let mut r = Room::hosted();
    let host = r.host;
    let (ana, bo) = (r.join("ana"), r.join("bo"));
    let (cy, di) = (r.co_host("cy"), r.co_host("di"));
    let playing_admin = r.join_as("pat", Intent::Play, true).unwrap();
    let observer = r.join_as("obs", Intent::Watch, true).unwrap();
    assert!(r.s.observes(r.s.member(observer).unwrap()));
    // A member kicks nobody.
    assert_eq!(r.kick(ana, bo), Err(unauthorized_kick()));
    assert_eq!(r.kick(ana, host), Err(unauthorized_kick()));
    // A co-host: never the host, another co-host or an administrator.
    assert_eq!(r.kick(cy, host), Err(unauthorized_kick()));
    assert_eq!(r.kick(cy, di), Err(unauthorized_kick()));
    assert_eq!(r.kick(cy, playing_admin), Err(unauthorized_kick()));
    assert_eq!(r.kick(cy, cy), Err(unauthorized_kick()));
    // Nobody the observing administrator; the host never themselves.
    assert_eq!(r.kick(host, observer), Err(unauthorized_kick()));
    assert_eq!(r.kick(cy, observer), Err(unauthorized_kick()));
    assert_eq!(r.kick(host, host), Err(unauthorized_kick()));
    assert!(!r.s.can_kick(cy, di) && r.s.can_kick(cy, ana) && r.s.can_kick(host, di));
    // A co-host kicks an ordinary member: told to everyone.
    let events = r.kick(cy, ana).unwrap();
    assert!(events.contains(&Event::MemberRemoved { participant: ana, by: cy }));
    assert!(r.left(ana));
    assert_eq!(r.kick(cy, ana), Err(Rejection::NotPresent), "gone already");
    // The host kicks a co-host and an administrator who plays.
    r.kick(host, di).unwrap();
    r.kick(host, playing_admin).unwrap();
    assert!(r.left(di) && r.left(playing_admin));
    // An administrator who plays may kick as a co-host would.
    let pat2 = r.join_as("pat2", Intent::Play, true).unwrap();
    r.kick(pat2, bo).unwrap();
    assert_eq!(r.kick(pat2, cy), Err(unauthorized_kick()));
    // Someone unknown.
    assert_eq!(r.kick(host, ParticipantId(99)), Err(Rejection::UnknownParticipant));
}

/// A CPU is never kicked: it leaves by the roster.
#[test]
fn a_cpu_is_never_kicked() {
    let mut r = Room::open(
        "host",
        "Host",
        RoomPolicy::default(),
        MatchRules::practice(OpponentSpec {
            level: 3,
            seed: 7,
            personality: None,
            name: None,
            targeting: None,
        }),
        ContentSource::LocalLibrary,
    );
    let host = r.host;
    r.by(host, Command::Pick { content: song(1), card: None }).unwrap();
    let cpu = r
        .s
        .members
        .values()
        .find(|m| m.is_cpu())
        .map(|m| m.id)
        .expect("practice seats its CPU at the pick");
    assert_eq!(r.kick(host, cpu), Err(Rejection::NotAHuman));
}

/// A kick bans nobody: the one kicked comes back by the invite (the
/// authority's join restores their row) and plays on.
#[test]
fn the_kicked_come_back_by_the_invite() {
    let mut r = Room::hosted();
    let host = r.host;
    let ana = r.join("ana");
    r.kick(host, ana).unwrap();
    assert!(r.left(ana));
    let events = r
        .sys(Command::Join {
            identity: Identity::new("twitch:ana"),
            name: "ana".into(),
            admin: false,
            intent: Intent::Play,
        })
        .unwrap();
    assert!(events.contains(&Event::MemberRestored { participant: ana }));
    assert!(!r.left(ana));
    assert_eq!(r.s.member(ana).unwrap().intent, Intent::Play);
}

/// A kick during a committed countdown cancels the start as a leave does;
/// during the stage it leaves the lineup as it was (the live side ends the
/// stream as dropped: a forfeit, as leaving is).
#[test]
fn a_kick_during_a_start_or_a_stage_is_a_leave() {
    let mut r = Room::hosted();
    let host = r.host;
    let (ana, bo) = (r.join("ana"), r.join("bo"));
    r.counted_down(1, &[host, ana, bo]);
    let events = r.kick(host, ana).unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::StartCanceled {
                reason: CancelReason::RosterChanged,
                ..
            }
        )),
        "{events:?}"
    );
    assert!(r.left(ana));
    // Kicked while the stage plays (another room, the same story).
    let mut r = Room::hosted();
    let host = r.host;
    let (ana, bo) = (r.join("ana"), r.join("bo"));
    let s1 = r.counted_down(1, &[host, ana, bo]);
    for _ in 0..600 {
        if matches!(r.s.phase, Phase::Stage { .. }) {
            break;
        }
        let next = r.now + 100;
        r.wait_to(next);
    }
    assert!(matches!(r.s.phase, Phase::Stage { .. }), "{:?}", r.s.phase);
    let lineup = r.s.stage(s1).unwrap().play.as_ref().unwrap().lineup.clone();
    assert!(lineup.contains(&ana));
    r.kick(host, ana).unwrap();
    assert!(r.left(ana));
    assert_eq!(
        r.s.stage(s1).unwrap().play.as_ref().unwrap().lineup,
        lineup,
        "the stage's lineup as it started: the kicked one's run a forfeit"
    );
    assert!(matches!(r.s.phase, Phase::Stage { .. }), "the stage goes on");
}

/// The mirror's removal (the authority's `Remove`) reads to the others as
/// the member leaving (review note 3): no `MemberRemoved`, nobody told the
/// host's lists barred them. Never a CPU nor someone gone.
#[test]
fn the_mirrors_removal_reads_as_a_leave() {
    let mut r = Room::hosted();
    let ana = r.join("ana");
    let events = r.sys(Command::Remove { participant: ana }).unwrap();
    assert!(events.contains(&Event::MemberLeft { participant: ana }));
    assert!(!events.iter().any(|e| matches!(e, Event::MemberRemoved { .. })));
    assert!(r.left(ana));
    assert_eq!(r.sys(Command::Remove { participant: ana }), Err(Rejection::NotPresent));
}

/// Kicking the one who chooses the songs gives the pick back to the host;
/// kicking the host away is impossible, but the mirror's removal of a host
/// passes the room on as their leaving would.
#[test]
fn a_removal_hands_on_the_pick_and_the_room() {
    let mut r = Room::hosted();
    let host = r.host;
    let cy = r.co_host("cy");
    let ana = r.join("ana");
    r.by(cy, Command::TakePick).unwrap();
    assert_eq!(r.s.picker(), Some(cy));
    r.kick(host, cy).unwrap();
    assert_eq!(r.s.picker(), Some(host));
    // The authority removes the host (another's lists bar them): the room
    // passes on to the member who joined first.
    r.sys(Command::Remove { participant: host }).unwrap();
    assert_eq!(r.s.owner().map(|o| o.id), Some(ana));
}

/// B1, B3: the owner turns the setting on (their own lists); nobody else
/// does; it is the authority's command alone; a bot has no Twitch.
#[test]
fn the_owner_turns_the_twitch_setting_on() {
    let mut r = Room::hosted();
    let host = r.host;
    let cy = r.co_host("cy");
    let ana = r.join("ana");
    let mine = r.setting(host, TwitchMirror::Bans, TwitchChat::Off);
    // Only the authority gives it.
    assert_eq!(
        r.by(host, Command::SetTwitch { member: host, twitch: Some(mine.clone()) }),
        Err(Rejection::AuthorityOnly)
    );
    // A co-host's or a member's own: refused.
    let cys = r.setting(cy, TwitchMirror::Bans, TwitchChat::Off);
    assert_eq!(r.set_twitch(cy, Some(cys)), Err(unauthorized_twitch()));
    let anas = r.setting(ana, TwitchMirror::BansAndBlocks, TwitchChat::Emotes);
    assert_eq!(r.set_twitch(ana, Some(anas)), Err(unauthorized_twitch()));
    // The owner's, never in someone else's name.
    assert_eq!(r.set_twitch(host, Some(r.setting(cy, TwitchMirror::Bans, TwitchChat::Off))), Err(unauthorized_twitch()));
    let events = r.set_twitch(host, Some(mine.clone())).unwrap();
    assert_eq!(events, vec![Event::TwitchChanged { twitch: Some(mine.clone()) }]);
    assert_eq!(r.s.twitch, Some(mine.clone()));
    // The same again: nothing changes.
    let rev = r.s.revision;
    assert!(r.set_twitch(host, Some(mine.clone())).unwrap().is_empty());
    assert_eq!(r.s.revision, rev);
    // Raised: blocks too, and the chat's ceiling.
    let more = r.setting(host, TwitchMirror::BansAndBlocks, TwitchChat::EmotesAndMessages);
    r.set_twitch(host, Some(more.clone())).unwrap();
    assert_eq!(r.s.twitch, Some(more));
    // A co-host may not turn the owner's off; the owner may.
    assert_eq!(r.set_twitch(cy, None), Err(unauthorized_twitch()));
    r.set_twitch(host, None).unwrap();
    assert_eq!(r.s.twitch, None);
    // A setting with nothing on is none.
    r.set_twitch(host, Some(r.setting(host, TwitchMirror::Off, TwitchChat::Off))).unwrap();
    assert_eq!(r.s.twitch, None);
    // Past its bounds.
    let mut long = r.setting(host, TwitchMirror::Bans, TwitchChat::Off);
    long.login = "x".repeat(65);
    assert_eq!(r.set_twitch(host, Some(long)), Err(Rejection::InvalidTwitch));
}

/// A bot hosts no Twitch.
#[test]
fn a_bot_has_no_twitch() {
    let mut r = Room::open(
        "bot:host",
        "Host bot",
        RoomPolicy::default().with_live(),
        MatchRules::hosted(),
        ContentSource::Shared,
    );
    let host = r.host;
    let bots = TwitchSetting {
        by: host,
        mirror: TwitchMirror::Bans,
        chat: TwitchChat::Off,
        user_id: "1".into(),
        login: "bot".into(),
    };
    // (Room::open names its owner twitch:<owner>; this one is `bot:`.)
    let bot = Identity::new("bot:guest");
    r.sys(Command::Join { identity: bot.clone(), name: "guest".into(), admin: false, intent: Intent::Play }).unwrap();
    let guest = r.s.member_by_identity(&bot).unwrap().id;
    let mut setting = bots.clone();
    setting.by = guest;
    assert_eq!(r.set_twitch(guest, Some(setting)), Err(Rejection::NotAHuman));
}

/// B3 and answer 11: after the room changed hands, the setting stays the
/// one who set it's while they are a member; they may turn it (or a part
/// of it) off but not raise it; the new host may turn it off or replace
/// it with their own; nobody else changes it; it ends when the one who
/// set it leaves (not when they are away).
#[test]
fn after_a_change_of_hands_the_setting_stays_theirs_until_they_leave() {
    let mut r = Room::hosted();
    let host = r.host;
    let (ana, bo) = (r.join("ana"), r.join("bo"));
    let full = r.setting(host, TwitchMirror::BansAndBlocks, TwitchChat::EmotesAndMessages);
    r.set_twitch(host, Some(full.clone())).unwrap();
    // The room handed to ana: the host is a co-host, the setting stays.
    r.by(host, Command::MakeHost { participant: ana }).unwrap();
    assert_eq!(r.s.member(host).unwrap().role, Role::CoHost);
    assert_eq!(r.s.twitch, Some(full.clone()));
    // Away past the grace: still theirs.
    r.sys(Command::SetConnection { participant: host, connection: Connection::Disconnected }).unwrap();
    assert_eq!(r.s.twitch, Some(full.clone()));
    r.sys(Command::SetConnection { participant: host, connection: Connection::Connected }).unwrap();
    // The one who set it lowers a part of it, never raises it again.
    let lower = r.setting(host, TwitchMirror::Bans, TwitchChat::EmotesAndMessages);
    r.set_twitch(host, Some(lower.clone())).unwrap();
    assert_eq!(r.s.twitch, Some(lower));
    assert_eq!(r.set_twitch(host, Some(full)), Err(unauthorized_twitch()));
    // Nobody else changes it.
    assert_eq!(r.set_twitch(bo, None), Err(unauthorized_twitch()));
    // The new host replaces it with their own lists.
    let anas = r.setting(ana, TwitchMirror::Bans, TwitchChat::Emotes);
    r.set_twitch(ana, Some(anas.clone())).unwrap();
    assert_eq!(r.s.twitch, Some(anas.clone()));
    // The earlier host may no longer touch it: it is not theirs.
    assert_eq!(r.set_twitch(host, None), Err(unauthorized_twitch()));
    // The one who set it leaves: it is off, for everyone.
    let events = r.by(ana, Command::MakeHost { participant: bo }).unwrap();
    assert!(events.iter().all(|e| !matches!(e, Event::TwitchChanged { .. })));
    let events = r.by(ana, Command::Leave).unwrap();
    assert!(events.contains(&Event::TwitchChanged { twitch: None }), "{events:?}");
    assert_eq!(r.s.twitch, None);
}

/// The one who set it removed by a kick (they are a co-host by then):
/// the setting ends with them.
#[test]
fn a_kick_of_the_one_who_set_it_ends_the_setting() {
    let mut r = Room::hosted();
    let host = r.host;
    let ana = r.join("ana");
    let mine = r.setting(host, TwitchMirror::Bans, TwitchChat::Off);
    r.set_twitch(host, Some(mine)).unwrap();
    r.by(host, Command::MakeHost { participant: ana }).unwrap();
    let events = r.kick(ana, host).unwrap();
    assert!(events.contains(&Event::TwitchChanged { twitch: None }));
    assert_eq!(r.s.twitch, None);
}

/// The audience's public view carries nothing of it.
#[test]
fn the_public_view_carries_none_of_it() {
    let mut r = Room::hosted();
    let host = r.host;
    let mine = r.setting(host, TwitchMirror::BansAndBlocks, TwitchChat::EmotesAndMessages);
    r.set_twitch(host, Some(mine)).unwrap();
    let ana = r.join("ana");
    r.kick(host, ana).unwrap();
    let view = serde_json::to_string(&ddi_room::audience::public::PublicView::of(&r.s, "Room")).unwrap();
    for word in ["twitch", "1001", "mirror", "removed", "kick"] {
        assert!(!view.contains(word), "{word} in {view}");
    }
}

/// A room stored by revision 17 (`room-revision-17.json`, written by
/// revision 17's own reducer: an online room with its song list, a
/// co-host, an observing administrator and a stage picked from the list)
/// reads with no Twitch setting, writes back what it reads, and plays on:
/// a kick and the setting from then.
#[test]
fn a_room_stored_by_revision_17_loads_as_it_was() {
    let text = include_str!("room-revision-17.json");
    let mut state: RoomState = serde_json::from_str(text).unwrap();
    let back = serde_json::to_value(&state).unwrap();
    assert_eq!(
        back,
        serde_json::from_str::<serde_json::Value>(text).unwrap(),
        "written as it was read"
    );
    assert_eq!(state.twitch, None);
    let host = ParticipantId(1);
    let ana = state.member_by_identity(&Identity::new("twitch:ana")).unwrap().id;
    state
        .apply(Actor::Member(host), Some(state.revision), &Command::Remove { participant: ana }, Millis(70_000))
        .unwrap();
    assert!(state.member(ana).unwrap().left);
    let setting = TwitchSetting {
        by: host,
        mirror: TwitchMirror::Bans,
        chat: TwitchChat::Off,
        user_id: "1001".into(),
        login: "host".into(),
    };
    state
        .apply(Actor::Authority, None, &Command::SetTwitch { member: host, twitch: Some(setting.clone()) }, Millis(71_000))
        .unwrap();
    assert_eq!(state.twitch, Some(setting));
    assert!(serde_json::to_value(&state).unwrap().get("twitch").is_some());
}
