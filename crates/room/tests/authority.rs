//! The envelope, the local authority and a client's mirror: what the
//! shells and, later, the online adapter rely on.

mod common;

use common::song;
use ddi_room::*;

fn opponent() -> OpponentSpec {
    OpponentSpec {
        level: 1,
        seed: 7,
        personality: None,
        name: None,
        targeting: None,
    }
}

fn round_trip<T>(value: &T) -> String
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(value).unwrap();
    let back: T = serde_json::from_str(&json).unwrap();
    assert_eq!(&back, value, "{json}");
    json
}

/// A client of a local authority: sends with its own sequence numbers and
/// its mirror's revision as the base, and follows every message.
struct Client {
    id: ClientId,
    seq: u64,
    mirror: Mirror,
    replies: Vec<ServerMessage>,
}

impl Client {
    fn new(id: ClientId) -> Self {
        Self {
            id,
            seq: 0,
            mirror: Mirror::new(),
            replies: Vec::new(),
        }
    }

    fn state(&self) -> &RoomState {
        self.mirror.state().unwrap()
    }

    fn send(&mut self, authority: &mut impl RoomAuthority, cmd: Command) -> ServerMessage {
        self.seq += 1;
        let base = self.mirror.state().map(|s| s.revision);
        authority.submit(ClientEnvelope::new(self.id.clone(), self.seq, base, cmd));
        self.sync(authority);
        self.replies
            .iter()
            .rev()
            .find(|m| matches!(m, ServerMessage::Ack { seq, .. } | ServerMessage::Reject { seq, .. } if *seq == self.seq))
            .cloned()
            .expect("an ack or a reject")
    }

    fn sync(&mut self, authority: &mut impl RoomAuthority) {
        for envelope in authority.poll() {
            // Every message survives the wire.
            round_trip(&envelope);
            self.mirror.receive(&envelope).unwrap();
            self.replies.push(envelope.msg);
        }
    }
}

/// The picker's pick of song `n`.
fn pick(n: u32) -> Command {
    Command::Pick {
        content: song(n),
        card: None,
    }
}

fn ack(msg: &ServerMessage) -> bool {
    matches!(msg, ServerMessage::Ack { .. })
}

#[test]
fn a_practice_match_runs_through_envelopes_and_the_mirror_keeps_up() {
    let clock = ManualClock::new(1_000);
    let mut rules = MatchRules::practice(opponent());
    rules.stages = 1;
    let (mut authority, id) = LocalAuthority::practice("Me".into(), rules, clock.clone());
    let mut me = Client::new(id.clone());
    me.sync(&mut authority);
    let player = authority.participant(&id).unwrap();
    let stage = me.state().upcoming_stage().unwrap();
    assert!(ack(&me.send(&mut authority, pick(1))));
    let rev = me.state().stage(stage).unwrap().readiness_rev;
    for cmd in [
        Command::ReportLoading {
            stage,
            readiness_rev: rev,
            loading: Loading::Loaded,
        },
        Command::SetReady {
            stage,
            readiness_rev: rev,
            ready: true,
        },
    ] {
        let reply = me.send(&mut authority, cmd.clone());
        assert!(ack(&reply), "{cmd:?}: {reply:?}");
    }
    assert!(matches!(me.state().phase, Phase::Countdown { .. }));
    // The countdown runs on the injected clock, through poll.
    clock.advance(2_999);
    me.sync(&mut authority);
    assert!(matches!(me.state().phase, Phase::Countdown { .. }));
    clock.advance(1);
    me.sync(&mut authority);
    assert_eq!(me.state().phase, Phase::Stage { stage });
    // The stage runner reports the confirmed outcome as the authority.
    let cpus: Vec<ParticipantId> = me
        .state()
        .members
        .values()
        .filter(|m| m.is_cpu())
        .map(|m| m.id)
        .collect();
    let mut ranking = vec![vec![Finisher {
        participant: player,
        finish: Finish::Survived,
    }]];
    ranking.extend(cpus.iter().map(|&participant| {
        vec![Finisher {
            participant,
            finish: Finish::Eliminated,
        }]
    }));
    authority
        .authority(Command::EndSong {
            stage,
            ranking: Some(ranking),
        })
        .unwrap();
    me.sync(&mut authority);
    assert_eq!(me.state().phase, Phase::MatchResults);
    assert_eq!(me.state().match_winners(), vec![player]);
    assert_eq!(Some(me.state()), authority.room());
    assert!(ack(&me.send(&mut authority, Command::OpenNextMatch)));
    assert_eq!(me.state().history.len(), 1);
    assert_eq!(Some(me.state()), authority.room());
    assert_eq!(authority.connection(), Connection::Connected);
}

#[test]
fn retries_are_idempotent_and_old_or_foreign_envelopes_are_refused() {
    let clock = ManualClock::new(0);
    let (mut authority, id) =
        LocalAuthority::practice("Me".into(), MatchRules::practice(opponent()), clock);
    let mut me = Client::new(id.clone());
    me.sync(&mut authority);
    let base = Some(me.state().revision);
    let choose = pick(1);
    authority.submit(ClientEnvelope::new(id.clone(), 5, base, choose.clone()));
    let first = authority.poll();
    let rev = authority.state().revision;
    // The same seq again: the same ack, nothing applied twice.
    authority.submit(ClientEnvelope::new(id.clone(), 5, base, choose.clone()));
    let again = authority.poll();
    assert_eq!(authority.state().revision, rev);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].msg, first.last().unwrap().msg);
    // A lower seq is out of order.
    authority.submit(ClientEnvelope::new(id.clone(), 4, base, Command::Leave));
    assert_eq!(
        authority.poll()[0].msg,
        ServerMessage::Reject {
            client: id.clone(),
            seq: 4,
            rejection: Rejection::OutOfOrder { last: 5 }
        }
    );
    assert!(!authority.state().members[&ParticipantId(1)].left);
    // An unknown client, and a version the authority does not speak.
    let stranger = ClientId::new("stranger");
    authority.submit(ClientEnvelope::new(
        stranger.clone(),
        1,
        None,
        Command::Leave,
    ));
    assert!(matches!(
        authority.poll()[0].msg,
        ServerMessage::Reject {
            rejection: Rejection::UnknownClient,
            ..
        }
    ));
    let mut future = ClientEnvelope::new(id.clone(), 9, base, Command::Leave);
    future.v = PROTOCOL_VERSION + 1;
    authority.submit(future);
    assert!(matches!(
        authority.poll()[0].msg,
        ServerMessage::Reject {
            rejection: Rejection::UnsupportedVersion { .. },
            ..
        }
    ));
    assert!(!authority.state().members[&ParticipantId(1)].left);
}

#[test]
fn a_stale_edit_is_refused_with_the_room_as_it_is() {
    let clock = ManualClock::new(0);
    let state = RoomState::new(
        RoomId::new("room"),
        Identity::new("twitch:host"),
        "Host".into(),
        RoomPolicy::default(),
        MatchRules::hosted(),
        ContentSource::Shared,
    );
    let mut authority = LocalAuthority::new(state, clock);
    let host_client = ClientId::new("host");
    // The owner's record exists; bind a client to it through a join.
    authority
        .connect(
            host_client.clone(),
            Identity::new("twitch:host"),
            "Host".into(),
            false,
            Intent::Play,
        )
        .unwrap();
    let co_client = ClientId::new("co");
    let co = authority
        .connect(
            co_client.clone(),
            Identity::new("twitch:co"),
            "Co".into(),
            false,
            Intent::Play,
        )
        .unwrap();
    let mut host = Client::new(host_client);
    let mut cohost = Client::new(co_client);
    host.sync(&mut authority);
    assert!(ack(&host.send(
        &mut authority,
        Command::SetCoHost {
            participant: co,
            on: true,
        },
    )));
    // The co-host's mirror starts from a snapshot of its own.
    cohost.mirror = host.mirror.clone();
    let stage = host.state().upcoming_stage().unwrap();
    // Both edit the next match's rules from the same view: the host first,
    // then the co-host, who has not seen the host's.
    let view = host.state().revision;
    let rules = |stages| {
        let mut r = MatchRules::hosted();
        r.stages = stages;
        r
    };
    authority.submit(ClientEnvelope::new(
        ClientId::new("host"),
        10,
        Some(view),
        Command::SaveNextRules { rules: rules(1) },
    ));
    authority.submit(ClientEnvelope::new(
        ClientId::new("co"),
        1,
        Some(view),
        Command::SaveNextRules { rules: rules(2) },
    ));
    let messages = authority.poll();
    let reject = messages
        .iter()
        .position(|m| matches!(m.msg, ServerMessage::Reject { ref client, rejection: Rejection::Stale { .. }, .. } if client.as_str() == "co"))
        .expect("the second edit is stale");
    assert!(matches!(
        messages[reject + 1].msg,
        ServerMessage::Snapshot { .. }
    ));
    for m in &messages {
        cohost.mirror.receive(m).unwrap();
    }
    assert_eq!(
        cohost.state().next.rules,
        rules(1),
        "the first edit stands, unseen edits are never overwritten"
    );
    // Shared content is pending until the authority confirms the share.
    host.mirror = cohost.mirror.clone();
    host.seq = 10;
    let reply = host.send(&mut authority, pick(1));
    assert!(ack(&reply), "{reply:?}");
    assert_eq!(
        host.state()
            .stage(stage)
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .status,
        ContentStatus::Pending
    );
}

#[test]
fn a_mirror_detects_a_missed_revision_and_a_snapshot_repairs_it() {
    let clock = ManualClock::new(0);
    let (mut authority, id) =
        LocalAuthority::practice("Me".into(), MatchRules::practice(opponent()), clock);
    let mut mirror = Mirror::new();
    let snapshot = authority.poll();
    mirror.receive(&snapshot[0]).unwrap();
    let base = Some(authority.state().revision);
    let rest = |n| Command::Spotlight {
        content: Some(song(n)),
        card: None,
    };
    authority.submit(ClientEnvelope::new(id.clone(), 1, base, rest(1)));
    authority.submit(ClientEnvelope::new(
        id.clone(),
        2,
        Some(authority.state().revision),
        rest(2),
    ));
    let events: Vec<ServerEnvelope> = authority
        .poll()
        .into_iter()
        .filter(|m| matches!(m.msg, ServerMessage::Events { .. }))
        .collect();
    assert_eq!(events.len(), 2);
    // The first is lost on the way.
    assert_eq!(
        mirror.receive(&events[1]),
        Err(MirrorError::Gap {
            have: Revision(0),
            got: Revision(2)
        })
    );
    mirror
        .receive(&ServerEnvelope::new(
            authority.state().revision,
            ServerMessage::Snapshot {
                room: Box::new(authority.state().clone()),
            },
        ))
        .unwrap();
    // The late one is old news now.
    mirror.receive(&events[0]).unwrap();
    assert_eq!(mirror.state(), Some(authority.state()));
    let mut fresh = Mirror::new();
    assert_eq!(fresh.receive(&events[0]), Err(MirrorError::NoSnapshot));
}

#[test]
fn a_disconnected_client_reconnects_to_the_same_participant() {
    let clock = ManualClock::new(0);
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    let state = RoomState::new(
        RoomId::new("room"),
        Identity::new("twitch:host"),
        "Host".into(),
        RoomPolicy::default(),
        rules,
        ContentSource::LocalLibrary,
    );
    let mut authority = LocalAuthority::new(state, clock);
    let first = ClientId::new("ana-1");
    let ana = authority
        .connect(
            first.clone(),
            Identity::new("twitch:ana"),
            "Ana".into(),
            false,
            Intent::Play,
        )
        .unwrap();
    authority.disconnect(&first);
    assert_eq!(
        authority.state().members[&ana].connection,
        Connection::Disconnected
    );
    // The old connection is gone; a new one is the same person.
    authority.submit(ClientEnvelope::new(first.clone(), 1, None, Command::Leave));
    assert!(matches!(
        authority.poll().last().unwrap().msg,
        ServerMessage::Reject {
            rejection: Rejection::UnknownClient,
            ..
        }
    ));
    let again = authority
        .connect(
            ClientId::new("ana-2"),
            Identity::new("twitch:ana"),
            "Ana".into(),
            false,
            Intent::Play,
        )
        .unwrap();
    assert_eq!(again, ana);
    assert_eq!(authority.state().members.len(), 2);
}

#[test]
fn every_message_kind_survives_the_wire() {
    let json = round_trip(&ClientEnvelope::new(
        ClientId::new("c"),
        17,
        Some(Revision(42)),
        Command::SaveNextRules {
            rules: MatchRules::practice(opponent()),
        },
    ));
    assert!(json.starts_with(r#"{"v":2,"client":"c","seq":17,"base":42,"cmd":{"save_next_rules""#));
    for cmd in [
        Command::SetIntent {
            intent: Intent::Watch,
        },
        Command::SetReady {
            stage: StageId(3),
            readiness_rev: 2,
            ready: true,
        },
        Command::ReportLoading {
            stage: StageId(3),
            readiness_rev: 2,
            loading: Loading::Failed { code: "x".into() },
        },
        Command::Leave,
        pick(1),
        Command::SetAccess {
            admission: Admission::Closed,
            rotate_invite: true,
        },
        Command::Join {
            identity: Identity::new("twitch:1"),
            name: "Name".into(),
            admin: false,
            intent: Intent::Play,
        },
        Command::EndSong {
            stage: StageId(3),
            ranking: Some(vec![vec![Finisher {
                participant: ParticipantId(1),
                finish: Finish::Forfeit,
            }]]),
        },
        Command::Interrupt {
            reason: Interruption::Stopped,
        },
    ] {
        round_trip(&ClientEnvelope::new(ClientId::new("c"), 1, None, cmd));
    }
    let ack = round_trip(&ServerEnvelope::new(
        Revision(43),
        ServerMessage::Ack {
            client: ClientId::new("c"),
            seq: 17,
        },
    ));
    assert_eq!(
        ack,
        r#"{"v":2,"rev":43,"msg":{"kind":"ack","client":"c","seq":17}}"#
    );
    for rejection in [
        Rejection::Stale {
            base: Revision(1),
            changed_at: Revision(2),
        },
        Rejection::InvalidRules(RulesProblem::HumansExceedSeats {
            humans: 5,
            seats: 4,
        }),
        Rejection::Cap(CapExceeded {
            requested: 9,
            cap: 8,
        }),
        Rejection::Unauthorized {
            capability: Capability::ChangeAccess,
        },
        Rejection::InvalidRanking(RankingProblem::Missing(ParticipantId(2))),
        Rejection::NotEnoughReady {
            ready_humans: 0,
            competitors: 2,
        },
    ] {
        round_trip(&ServerEnvelope::new(
            Revision(1),
            ServerMessage::Reject {
                client: ClientId::new("c"),
                seq: 1,
                rejection,
            },
        ));
    }
    // A room with a finished match, standings with fractions, CPUs and
    // history, as a snapshot.
    let clock = ManualClock::new(0);
    let (mut authority, id) =
        LocalAuthority::practice("Me".into(), MatchRules::practice(opponent()), clock.clone());
    let stage = authority.state().upcoming_stage().unwrap();
    let mut seq = 0;
    let mut send = |a: &mut LocalAuthority<ManualClock>, cmd| {
        seq += 1;
        let base = Some(a.state().revision);
        a.submit(ClientEnvelope::new(id.clone(), seq, base, cmd));
    };
    send(&mut authority, pick(1));
    send(
        &mut authority,
        Command::ReportLoading {
            stage,
            readiness_rev: 1,
            loading: Loading::Loaded,
        },
    );
    send(
        &mut authority,
        Command::SetReady {
            stage,
            readiness_rev: 1,
            ready: true,
        },
    );
    clock.advance(3_000);
    authority.poll();
    let ids: Vec<ParticipantId> = authority.state().members.keys().copied().collect();
    authority
        .authority(Command::ConfirmStage {
            stage,
            ranking: vec![
                ids.iter()
                    .take(2)
                    .map(|&participant| Finisher {
                        participant,
                        finish: Finish::Survived,
                    })
                    .collect(),
                ids.iter()
                    .skip(2)
                    .map(|&participant| Finisher {
                        participant,
                        finish: Finish::Eliminated,
                    })
                    .collect(),
            ],
        })
        .unwrap();
    let state = authority.state().clone();
    let stage_result = state
        .stage(stage)
        .unwrap()
        .play
        .as_ref()
        .unwrap()
        .result
        .clone()
        .unwrap();
    // Four: 12, 6, 3, 1; the two tied first share 12 and 6.
    assert_eq!(stage_result.placements[0].points, Points::whole(9));
    for envelope in authority.poll() {
        round_trip(&envelope);
    }
    round_trip(&ServerEnvelope::new(
        state.revision,
        ServerMessage::Snapshot {
            room: Box::new(state),
        },
    ));
}

/// The room model's revision (round B1): a client says its own; an
/// authority refuses one older than itself (or one that says none) at a
/// join, and at a ticket except while a stage is under way; a newer
/// client is never refused by the authority (it refuses an older
/// authority itself, round R2).
#[test]
fn an_older_room_model_is_refused_except_during_a_stage() {
    use ddi_room::{ROOM_MODEL_REVISION, UPDATE_REQUIRED, UpdateRequired, check_model_against};
    let rev = ROOM_MODEL_REVISION;
    assert_eq!(rev, 11, "11: adaptive-v4, a snapshot's step, the close command (10: adaptive-v3)");
    assert_eq!(UPDATE_REQUIRED, "update_required");
    // The other way (round R2): a client refuses an authority older than
    // itself, or one that says no revision.
    assert!(ddi_room::check_authority(Some(rev)));
    assert!(ddi_room::check_authority(Some(rev + 1)), "a newer authority");
    assert!(!ddi_room::check_authority(Some(rev - 1)));
    assert!(!ddi_room::check_authority(None));
    assert!(ddi_room::check_model(Some(rev)).is_ok());
    assert!(ddi_room::check_model(Some(rev + 1)).is_ok(), "a newer client");
    assert_eq!(
        ddi_room::check_model(None),
        Err(UpdateRequired { model: rev })
    );
    assert_eq!(
        check_model_against(Some(rev), rev + 1),
        Err(UpdateRequired { model: rev + 1 })
    );
    let mut state = RoomState::new(
        RoomId::new("room"),
        Identity::new("twitch:host"),
        "Host".into(),
        RoomPolicy::default(),
        MatchRules::hosted(),
        ContentSource::Shared,
    );
    assert!(state.admits_model_against(Some(rev), rev + 1).is_err());
    for phase in [
        Phase::Stage { stage: StageId(1) },
        Phase::SongCompletion { stage: StageId(1) },
    ] {
        state.phase = phase;
        assert!(state.admits_model_against(Some(rev), rev + 1).is_ok());
        assert!(state.admits_model_against(None, rev + 1).is_ok());
    }
    state.phase = Phase::Lobby;
    assert!(state.admits_model(Some(rev)).is_ok());
    assert!(state.admits_model(None).is_err());
}

/// A room stored before revision 9 still loads (round R2): the JSON a
/// revision-8 build wrote (`room-revision-8.json`, made by that build's
/// own walkthrough helpers: a hosted room of three stages, its policy as
/// the server's, a co-host, stage 1 self-reported and settled, a per-stage
/// change of rules and Points from stage 2, the host's setting Close)
/// reads with what this revision retired passed over (the policy's
/// version flags), plays on under this reducer, and writes back what it
/// reads.
#[test]
fn a_room_stored_by_revision_8_loads_and_plays_on() {
    let text = include_str!("room-revision-8.json");
    let old: serde_json::Value = serde_json::from_str(text).unwrap();
    for flag in ["restart", "points_bias", "step_down", "adaptive", "stage_rules", "stage_points", "cpus", "at_once"] {
        assert_eq!(old["policy"][flag], true, "{flag}");
    }
    let mut state: RoomState = serde_json::from_str(text).unwrap();
    let back = serde_json::to_value(&state).unwrap();
    for flag in ["restart", "adaptive", "cpus"] {
        assert!(back["policy"].get(flag).is_none(), "{flag} retired");
    }
    assert_eq!(serde_json::from_value::<RoomState>(back).unwrap(), state);
    assert!(state.policy.live.is_some() && state.policy.audience.is_some());
    assert_eq!(state.picker(), Some(ParticipantId(1)));
    assert_eq!(state.if_host_leaves(), ddi_room::HostLeaves::Close);
    let m = state.current.as_ref().unwrap();
    assert!(m.stage_rules_changed(StageId(2)));
    assert_eq!(m.rules_for(StageId(2)).points, ddi_room::PointsBias::Mild);
    assert_eq!(state.phase, Phase::Intermission { next: StageId(2) });
    // Its picker picks stage 2, as any room's.
    let before = state.standings().unwrap();
    let revision = state.revision;
    state
        .apply(
            ddi_room::Actor::Member(ParticipantId(1)),
            Some(revision),
            &Command::Pick {
                content: ContentId {
                    song: SongKey::new("song-2"),
                    chart: ChartKey::new("digest-2"),
                },
                card: None,
            },
            Millis(1_000_000),
        )
        .unwrap();
    assert!(matches!(state.phase, Phase::Preparing { stage: StageId(2), .. }));
    assert_eq!(state.standings().unwrap(), before, "stage 1's points kept");
}

/// A room stored by revision 10 loads under every later one (round D1):
/// the JSON revision 10's own reducer wrote (`room-revision-10.json`: a
/// hosted room that plays live with an audience, three stages under
/// Adaptive charts, a roster of two CPUs, a co-host who took the pick, a
/// member held to watching by the watch-only link, one who left, the
/// host's setting Close, stage 1 reported with details and settled, a
/// per-stage change of rules from stage 2, a spotlight and a kept share)
/// reads, writes back what it reads, and plays on.
#[test]
fn a_room_stored_by_revision_10_loads_and_plays_on() {
    let text = include_str!("room-revision-10.json");
    let mut state: RoomState = serde_json::from_str(text).unwrap();
    let back = serde_json::to_value(&state).unwrap();
    assert_eq!(
        back,
        serde_json::from_str::<serde_json::Value>(text).unwrap(),
        "written as it was read"
    );
    assert!(state.policy.live.is_some() && state.policy.audience.is_some());
    assert_eq!(state.closed, None);
    assert_eq!(state.picker(), Some(ParticipantId(2)));
    assert_eq!(state.if_host_leaves(), ddi_room::HostLeaves::Close);
    assert!(state.member(ParticipantId(4)).unwrap().watch_only);
    assert!(state.member(ParticipantId(5)).unwrap().left);
    let m = state.current.as_ref().unwrap();
    assert_eq!(m.rules.charts, ddi_room::Charts::Adaptive);
    assert!(m.stage_rules_changed(StageId(2)));
    assert_eq!(m.stages[0].play.as_ref().unwrap().details.len(), 4);
    assert_eq!(state.phase, Phase::Intermission { next: StageId(2) });
    let before = state.standings().unwrap();
    let revision = state.revision;
    state
        .apply(
            ddi_room::Actor::Member(ParticipantId(2)),
            Some(revision),
            &Command::Pick {
                content: ContentId {
                    song: SongKey::new("song-2"),
                    chart: ChartKey::new("digest-2"),
                },
                card: None,
            },
            Millis(1_000_000),
        )
        .unwrap();
    assert!(matches!(state.phase, Phase::Preparing { stage: StageId(2), .. }));
    assert_eq!(state.standings().unwrap(), before, "stage 1's points kept");
}
