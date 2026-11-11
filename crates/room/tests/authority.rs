//! The envelope, the local authority and a client's mirror: what the
//! shells and, later, the online adapter rely on.

use ddi_room::*;

fn song(n: u32) -> ContentId {
    ContentId {
        song: SongKey::new(format!("song-{n}")),
        chart: ChartKey::new(format!("digest-{n}")),
    }
}

fn opponent() -> OpponentSpec {
    OpponentSpec {
        level: 1,
        seed: 7,
        personality: None,
        name: None,
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
    assert!(ack(&me.send(
        &mut authority,
        Command::ChooseContent {
            stage,
            content: Some(song(1)),
        },
    )));
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
        Command::StartMatch,
        Command::StartStage {
            stage,
            readiness_rev: rev,
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
    let stage = me.state().upcoming_stage().unwrap();
    let base = Some(me.state().revision);
    let choose = Command::ChooseContent {
        stage,
        content: Some(song(1)),
    };
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
            Intent::Watch,
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
    // Both choose a song for the same stage from the same view.
    let view = host.state().revision;
    authority.submit(ClientEnvelope::new(
        ClientId::new("host"),
        10,
        Some(view),
        Command::ChooseContent {
            stage,
            content: Some(song(1)),
        },
    ));
    authority.submit(ClientEnvelope::new(
        ClientId::new("co"),
        1,
        Some(view),
        Command::ChooseContent {
            stage,
            content: Some(song(2)),
        },
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
        cohost
            .state()
            .stage(stage)
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .id,
        song(1),
        "the first edit stands, unseen edits are never overwritten"
    );
    // Shared content is pending until the authority confirms the share.
    assert_eq!(
        cohost
            .state()
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
    let stage = authority.state().upcoming_stage().unwrap();
    let base = Some(authority.state().revision);
    authority.submit(ClientEnvelope::new(
        id.clone(),
        1,
        base,
        Command::ChooseContent {
            stage,
            content: Some(song(1)),
        },
    ));
    authority.submit(ClientEnvelope::new(
        id.clone(),
        2,
        Some(authority.state().revision),
        Command::ChooseContent {
            stage,
            content: Some(song(2)),
        },
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
        Command::ChooseContent {
            stage: StageId(3),
            content: Some(song(1)),
        },
        Command::StartMatch,
        Command::StartStage {
            stage: StageId(3),
            readiness_rev: 0,
        },
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
    send(
        &mut authority,
        Command::ChooseContent {
            stage,
            content: Some(song(1)),
        },
    );
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
    send(&mut authority, Command::StartMatch);
    send(
        &mut authority,
        Command::StartStage {
            stage,
            readiness_rev: 1,
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
    assert_eq!(stage_result.placements[0].points, Points::ratio(7, 2));
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
