//! The online authority against the fake server: the adapter's contract
//! (one script through the local authority and through the online one,
//! the same rooms), and what the network does to it: duplicates, drops,
//! reordering, old revisions, delays, reconnects with a missed revision,
//! commands sent again across a reconnect, and the socket's ends (4401,
//! 4410, 4000).

use ddi_room::fake::{FakeServer, FakeSocket, Fault, SharedFake};
use ddi_room::{
    ChartKey, ClientEnvelope, ClientId, Clock, Command, Connection, ContentId, ContentSource,
    ContentStatus, Identity, Intent, LinkState, Loading, LocalAuthority, ManualClock, MatchRules,
    OnlineAuthority, ParticipantId, PhaseKind, Rejection, Revision, RoomAuthority, RoomId,
    RoomPolicy, RoomState, ServerMessage, SongKey, StageId, TicketProblem,
};

const ROOM: &str = "Room000001";

fn policy() -> RoomPolicy {
    RoomPolicy::default()
}

fn hosted(clock: &ManualClock) -> FakeServer {
    FakeServer::hosted(
        ROOM,
        Identity::new("dev:alice"),
        "Alice",
        policy(),
        clock.clone(),
    )
}

/// One person's client: the online authority over a fake socket, its
/// tickets fetched from the fake server as a shell would over HTTP.
struct Client {
    who: Identity,
    client: ClientId,
    seq: u64,
    link: OnlineAuthority<FakeSocket, ManualClock>,
    server: SharedFake,
    /// What the authority handed on: answers by seq, and how many
    /// snapshots.
    answers: Vec<(u64, Option<Rejection>)>,
    snapshots: usize,
}

impl Client {
    fn new(server: &SharedFake, clock: &ManualClock, who: &str) -> Client {
        Client {
            who: Identity::new(who),
            client: ClientId::new(format!("{who}-client")),
            seq: 0,
            link: OnlineAuthority::new(
                RoomId::new(ROOM),
                FakeSocket::new(server.clone()),
                clock.clone(),
            ),
            server: server.clone(),
            answers: Vec::new(),
            snapshots: 0,
        }
    }

    fn pump(&mut self) {
        if self.link.wants_ticket() {
            self.link.ticket_asked();
            let t = self.server.borrow_mut().ticket(&self.who);
            self.link.ticket(t.map_err(|r| match r {
                Rejection::RoomClosed => TicketProblem::RoomClosed,
                _ => TicketProblem::NotAMember,
            }));
        }
        for e in self.link.poll() {
            match e.msg {
                ServerMessage::Ack { seq, .. } => self.answers.push((seq, None)),
                ServerMessage::Reject { seq, rejection, .. } => {
                    self.answers.push((seq, Some(rejection)))
                }
                ServerMessage::Snapshot { .. } => self.snapshots += 1,
                ServerMessage::Events { .. } => {}
            }
        }
    }

    fn send(&mut self, cmd: Command) -> u64 {
        self.seq += 1;
        let base = self.link.room().map(|r| r.revision);
        self.link.submit(ClientEnvelope::new(
            self.client.clone(),
            self.seq,
            base,
            cmd,
        ));
        self.seq
    }

    fn room(&self) -> &RoomState {
        self.link.room().expect("a snapshot")
    }

    fn answer(&self, seq: u64) -> Option<&Option<Rejection>> {
        self.answers.iter().find(|(s, _)| *s == seq).map(|(_, a)| a)
    }
}

fn pump_all(clients: &mut [&mut Client]) {
    for _ in 0..6 {
        for c in clients.iter_mut() {
            c.pump();
        }
    }
}

fn content(n: u8) -> ContentId {
    ContentId {
        song: SongKey::new(format!("song-{n}")),
        chart: ChartKey::new(format!("digest-{n}")),
    }
}

/// The time each step of the script takes, in both runs.
const STEP_MS: u64 = 400;

/// A step of the contract's script.
#[derive(Clone)]
enum Step {
    Join(&'static str, &'static str),
    Connect(&'static str),
    Cmd(&'static str, Command),
    /// A command made against the room as it was at revision 1.
    Stale(&'static str, Command),
    Authority(Command),
    Advance(u64),
}

fn script() -> Vec<Step> {
    let stage = StageId(1);
    let mut s = vec![
        Step::Connect("dev:alice"),
        Step::Join("dev:bob", "Bob"),
        Step::Connect("dev:bob"),
        Step::Join("dev:carol", "Carol"),
        Step::Cmd(
            "dev:alice",
            Command::SetCoHost {
                participant: ParticipantId(2),
                on: true,
            },
        ),
        // A member may not change access; a client may not send the
        // authority's commands.
        Step::Cmd(
            "dev:bob",
            Command::SetAccess {
                admission: ddi_room::Admission::Open,
                rotate_invite: true,
            },
        ),
        Step::Cmd(
            "dev:bob",
            Command::SetConnection {
                participant: ParticipantId(1),
                connection: Connection::Disconnected,
            },
        ),
        Step::Cmd(
            "dev:alice",
            Command::ChooseContent {
                stage,
                content: Some(content(1)),
            },
        ),
        Step::Stale(
            "dev:alice",
            Command::ChooseContent {
                stage,
                content: Some(content(2)),
            },
        ),
        Step::Authority(Command::SetContentStatus {
            stage,
            status: ContentStatus::Available,
            share: Some("abc12345".into()),
        }),
        Step::Cmd("dev:alice", Command::StartMatch),
    ];
    for who in ["dev:alice", "dev:bob"] {
        s.push(Step::Cmd(
            who,
            Command::ReportLoading {
                stage,
                readiness_rev: 1,
                loading: Loading::Loaded,
            },
        ));
    }
    for who in ["dev:alice", "dev:bob"] {
        s.push(Step::Cmd(
            who,
            Command::SetReady {
                stage,
                readiness_rev: 1,
                ready: true,
            },
        ));
    }
    // Carol, admitted but never connected, unready: the readiness window
    // opens, then closes on time without her.
    s.push(Step::Cmd(
        "dev:alice",
        Command::StartStage {
            stage,
            readiness_rev: 1,
        },
    ));
    s.push(Step::Advance(5_000));
    s.push(Step::Advance(15_000));
    s.push(Step::Advance(4_000));
    s
}

/// The script through the local authority: what the fake server does
/// around a socket done as the same authority commands.
fn run_local(steps: &[Step]) -> (RoomState, Vec<Option<Rejection>>) {
    let clock = ManualClock::new(1_000);
    let state = RoomState::new(
        RoomId::new(ROOM),
        Identity::new("dev:alice"),
        "Alice".into(),
        policy(),
        MatchRules::hosted(),
        ContentSource::Shared,
    );
    let mut local = LocalAuthority::new(state, clock.clone());
    let _ = local.poll();
    let _ = local.authority(Command::SetConnection {
        participant: ParticipantId(1),
        connection: Connection::Joining,
    });
    let mut seqs = std::collections::BTreeMap::<&str, u64>::new();
    let mut answers = Vec::new();
    for step in steps {
        match step {
            Step::Join(who, name) => {
                local
                    .authority(Command::Join {
                        identity: Identity::new(*who),
                        name: (*name).into(),
                        admin: false,
                        intent: Intent::Play,
                    })
                    .unwrap();
                let id = local
                    .state()
                    .member_by_identity(&Identity::new(*who))
                    .unwrap()
                    .id;
                local
                    .authority(Command::SetConnection {
                        participant: id,
                        connection: Connection::Joining,
                    })
                    .unwrap();
            }
            Step::Connect(who) => {
                let id = local
                    .state()
                    .member_by_identity(&Identity::new(*who))
                    .unwrap()
                    .id;
                local
                    .authority(Command::SetConnection {
                        participant: id,
                        connection: Connection::Connected,
                    })
                    .unwrap();
                local
                    .bind(ClientId::new(format!("{who}-client")), id)
                    .unwrap();
            }
            Step::Cmd(who, cmd) | Step::Stale(who, cmd) => {
                let seq = seqs.entry(who).or_default();
                *seq += 1;
                let base = match step {
                    Step::Stale(..) => Revision(1),
                    _ => local.state().revision,
                };
                let client = ClientId::new(format!("{who}-client"));
                local.submit(ClientEnvelope::new(
                    client.clone(),
                    *seq,
                    Some(base),
                    cmd.clone(),
                ));
                let answer = local.poll().into_iter().find_map(|e| match e.msg {
                    ServerMessage::Ack { client: c, seq: s } if c == client && s == *seq => {
                        Some(None)
                    }
                    ServerMessage::Reject {
                        client: c,
                        seq: s,
                        rejection,
                    } if c == client && s == *seq => Some(Some(rejection)),
                    _ => None,
                });
                answers.push(answer.expect("answered"));
            }
            Step::Authority(cmd) => {
                local.authority(cmd.clone()).unwrap();
            }
            Step::Advance(ms) => {
                clock.advance(*ms);
                let _ = local.poll();
            }
        }
        for _ in 0..4 {
            clock.advance(STEP_MS / 4);
            let _ = local.poll();
        }
    }
    (local.state().clone(), answers)
}

/// The script through the online authority and the fake server, `faults`
/// in the way of Bob's frames from his socket's opening.
fn run_online(
    steps: &[Step],
    faults: &[Fault],
) -> (RoomState, Vec<Option<Rejection>>, Vec<RoomState>) {
    let clock = ManualClock::new(1_000);
    let server = hosted(&clock).shared();
    let mut alice = Client::new(&server, &clock, "dev:alice");
    let mut bob = Client::new(&server, &clock, "dev:bob");
    for f in faults {
        server.borrow_mut().fault(ParticipantId(2), *f);
    }
    let mut sent = Vec::new();
    let mut bob_in = false;
    for step in steps {
        match step {
            Step::Join(who, name) => {
                server
                    .borrow_mut()
                    .join(Identity::new(*who), name, Intent::Play)
                    .unwrap();
            }
            Step::Connect(who) => {
                if *who == "dev:alice" {
                    alice.pump();
                    assert_eq!(alice.link.link(), LinkState::Connected);
                } else {
                    bob_in = true;
                    bob.pump();
                }
            }
            Step::Cmd(who, cmd) | Step::Stale(who, cmd) => {
                let c = if *who == "dev:alice" {
                    &mut alice
                } else {
                    &mut bob
                };
                let seq = if matches!(step, Step::Stale(..)) {
                    c.seq += 1;
                    c.link.submit(ClientEnvelope::new(
                        c.client.clone(),
                        c.seq,
                        Some(Revision(1)),
                        cmd.clone(),
                    ));
                    c.seq
                } else {
                    c.send(cmd.clone())
                };
                sent.push((*who, seq));
            }
            Step::Authority(cmd) => {
                server.borrow_mut().authority(cmd.clone()).unwrap();
            }
            Step::Advance(ms) => {
                clock.advance(*ms);
            }
        }
        // The step's time passes, as in the local run: delayed frames
        // arrive within it.
        if bob_in {
            pump_all(&mut [&mut alice, &mut bob]);
        } else {
            pump_all(&mut [&mut alice]);
        }
        for _ in 0..4 {
            clock.advance(STEP_MS / 4);
            if bob_in {
                pump_all(&mut [&mut alice, &mut bob]);
            } else {
                pump_all(&mut [&mut alice]);
            }
        }
    }
    // Delayed frames arrive (the room's own clock has nothing due).
    clock.advance(1_000);
    pump_all(&mut [&mut alice, &mut bob]);
    let answers = sent
        .iter()
        .map(|(who, seq)| {
            let c = if *who == "dev:alice" { &alice } else { &bob };
            c.answer(*seq)
                .cloned()
                .unwrap_or_else(|| panic!("{who} {seq} unanswered: {:?}", c.answers))
        })
        .collect();
    let state = server.borrow().state().clone();
    let mirrors = vec![alice.room().clone(), bob.room().clone()];
    (state, answers, mirrors)
}

/// The adapter contract: the same script through the local authority and
/// through the online one gives the same room, the same answers, and
/// each client's mirror equal to the server's room.
#[test]
fn the_online_adapter_keeps_the_local_authoritys_contract() {
    let steps = script();
    let (local, local_answers) = run_local(&steps);
    let (server, online_answers, mirrors) = run_online(&steps, &[]);
    assert_eq!(local_answers, online_answers);
    assert_eq!(local, server, "the server's room is the local one");
    for m in &mirrors {
        assert_eq!(m, &server, "a client's mirror is the room");
    }
    // The script did what it says: refusals, a stale edit, a window that
    // closed with Bob sitting out, the stage begun.
    assert!(matches!(
        local_answers[1],
        Some(Rejection::Unauthorized { .. })
    ));
    assert_eq!(local_answers[2], Some(Rejection::AuthorityOnly));
    assert!(matches!(local_answers[4], Some(Rejection::Stale { .. })));
    assert_eq!(local.phase.kind(), PhaseKind::Stage);
    let play = local.current.as_ref().unwrap().stages[0]
        .play
        .as_ref()
        .unwrap();
    assert_eq!(
        play.lineup.iter().copied().collect::<Vec<_>>(),
        vec![ParticipantId(1), ParticipantId(2)]
    );
}

/// Feedback round B: a report may say how its starter performed
/// (`RunDetail`, optional and additive to protocol 2). The room keeps it
/// within its bounds, the local authority and the online one agree, every
/// mirror has it, and the ranking is the runs' alone.
#[test]
fn a_reports_detail_is_kept_alike_by_both_authorities_and_every_mirror() {
    use ddi_room::{ReportedRun, RunDetail};
    let stage = StageId(1);
    let detail = RunDetail {
        score: None,
        percent: Some(9_875),
        grade: "A".repeat(40),
        life_ppm: 2_000_000,
        out_at_ms: None,
        max_combo: 120,
        tiers: (0..20).collect(),
        attacks: None,
        received: None,
        shocks: None,
    };
    let mut steps = script();
    steps.push(Step::Cmd(
        "dev:bob",
        Command::ReportOutcome {
            stage,
            run: ReportedRun::Out { at_ms: 12_500 },
            detail: None,
        },
    ));
    steps.push(Step::Cmd(
        "dev:alice",
        Command::ReportOutcome {
            stage,
            run: ReportedRun::Live {
                life_ppm: 400_000,
                accuracy_ppm: 800_000,
                absolute: None,
                score: None,
            },
            detail: Some(detail.clone()),
        },
    ));
    let (local, local_answers) = run_local(&steps);
    let (server, online_answers, mirrors) = run_online(&steps, &[]);
    assert_eq!(local_answers, online_answers);
    assert_eq!(local, server, "the server's room is the local one");
    for m in &mirrors {
        assert_eq!(m, &server, "a client's mirror is the room");
    }
    let play = local.current.as_ref().unwrap().stages[0]
        .play
        .as_ref()
        .unwrap();
    assert_eq!(play.details.len(), 1, "Bob said nothing of his");
    let kept = &play.details[&ParticipantId(1)];
    assert_eq!(kept, &detail.clone().bounded());
    assert_eq!(
        (kept.grade.len(), kept.tiers.len(), kept.life_ppm),
        (RunDetail::MAX_GRADE, RunDetail::MAX_TIERS, 1_000_000)
    );
    // Ranked by the runs: Alice, still in, first.
    let result = play.result.as_ref().expect("settled on the two reports");
    assert_eq!(result.placement(ParticipantId(1)).unwrap().place, 1);
    assert_eq!(result.placement(ParticipantId(2)).unwrap().place, 2);
}

/// M8 round 3 (D8): a member's own attack policy (`SetTargeting`,
/// additive to protocol 2) is kept alike by the local authority, the
/// online one and every mirror: set before the match and while a stage
/// gathers readiness (nobody's readiness cleared: the stage starts with
/// both), refused during the stage's play; the default is left out of the
/// room's JSON, which a peer that predates the field reads and writes.
#[test]
fn a_members_attack_policy_is_kept_alike_by_both_authorities_and_every_mirror() {
    use ddi_room::attacks::Targeting;
    let mut steps = script();
    // After the joins: Bob picks Leader, Alice Retaliate then Spread again.
    let early = [
        Step::Cmd(
            "dev:bob",
            Command::SetTargeting {
                targeting: Targeting::Leader,
            },
        ),
        Step::Cmd(
            "dev:alice",
            Command::SetTargeting {
                targeting: Targeting::Retaliate,
            },
        ),
        Step::Cmd(
            "dev:alice",
            Command::SetTargeting {
                targeting: Targeting::Spread,
            },
        ),
    ];
    steps.splice(4..4, early);
    // Both ready, the window not opened yet: Alice changes hers again.
    let start = steps
        .iter()
        .position(|s| matches!(s, Step::Cmd(_, Command::StartStage { .. })))
        .unwrap();
    steps.insert(
        start,
        Step::Cmd(
            "dev:alice",
            Command::SetTargeting {
                targeting: Targeting::Retaliate,
            },
        ),
    );
    // The stage in play: refused (the play's own reports carry a change).
    steps.push(Step::Cmd(
        "dev:bob",
        Command::SetTargeting {
            targeting: Targeting::Spread,
        },
    ));
    let (local, local_answers) = run_local(&steps);
    let (server, online_answers, mirrors) = run_online(&steps, &[]);
    assert_eq!(local_answers, online_answers);
    assert_eq!(local, server, "the server's room is the local one");
    for m in &mirrors {
        assert_eq!(m, &server, "a client's mirror is the room");
    }
    assert_eq!(&local_answers[..3], &[None, None, None]);
    assert_eq!(
        local_answers.last().unwrap(),
        &Some(Rejection::WrongPhase {
            phase: PhaseKind::Stage
        })
    );
    let policy = |id: u64| local.member(ParticipantId(id)).unwrap().targeting;
    assert_eq!(
        (policy(1), policy(2), policy(3)),
        (Targeting::Retaliate, Targeting::Leader, Targeting::Spread)
    );
    // Nobody's readiness was cleared: the stage began with both.
    assert_eq!(local.phase.kind(), PhaseKind::Stage);
    let play = local.current.as_ref().unwrap().stages[0]
        .play
        .as_ref()
        .unwrap();
    assert_eq!(play.lineup.len(), 2);
    // Additive: Carol's default is not in the JSON, and a room written
    // without the field reads as Spread.
    let carol = serde_json::to_value(local.member(ParticipantId(3)).unwrap()).unwrap();
    assert!(carol.get("targeting").is_none(), "{carol}");
    let bob = serde_json::to_value(local.member(ParticipantId(2)).unwrap()).unwrap();
    assert_eq!(bob["targeting"], "leader");
    let read: ddi_room::Member = serde_json::from_value(carol).unwrap();
    assert_eq!(read.targeting, Targeting::Spread);
}

/// Watching (2026-11-12, `watch-v0`): a member's Play or Watch is kept
/// alike by the local authority, the online one and every mirror. The
/// host watches and still starts the match; their Ready is refused (a
/// watcher has none); with Carol away, Bob alone is too few to start, and
/// both authorities say so alike.
#[test]
fn a_members_watching_is_kept_alike_by_both_authorities_and_every_mirror() {
    let mut steps = script();
    // After the joins: Alice chooses to watch.
    steps.insert(
        4,
        Step::Cmd(
            "dev:alice",
            Command::SetIntent {
                intent: Intent::Watch,
            },
        ),
    );
    let (local, local_answers) = run_local(&steps);
    let (server, online_answers, mirrors) = run_online(&steps, &[]);
    assert_eq!(local_answers, online_answers);
    assert_eq!(local, server, "the server's room is the local one");
    for m in &mirrors {
        assert_eq!(m, &server, "a client's mirror is the room");
    }
    assert_eq!(local_answers[0], None);
    // Alice's Ready was refused: a watcher has none.
    assert!(local_answers.contains(&Some(Rejection::NotPlaying)));
    // One player ready, one away: too few to start, said alike.
    assert_eq!(
        local_answers.last().unwrap(),
        &Some(Rejection::NotEnoughReady {
            ready_humans: 1,
            competitors: 1
        })
    );
    let alice = ParticipantId(1);
    assert_eq!(local.member(alice).unwrap().intent, Intent::Watch);
    assert_eq!(
        local.participation(alice),
        Some(ddi_room::Participation::Watching)
    );
    // The match began without her: its seats are the two who play.
    let m = local.current.as_ref().unwrap();
    assert!(!m.roster.contains_key(&alice));
    assert_eq!(m.roster.len(), 2);
}

/// The DDI ruleset's round 3: Difficulty and Boos are match rules, saved
/// like the others, locked with the match, and kept alike by the local
/// authority, the online one and every mirror; rules that name a ruleset
/// a room could be played under before are taken and played as DDI; a
/// report's absolute score breaks a tie between those who played the same
/// notes. (Attacks choose the battle, which a hosted room's rules carry
/// as any other field.)
#[test]
fn the_match_rules_of_ddi_are_kept_alike_by_both_authorities_and_every_mirror() {
    use ddi_room::{Absolute, MatchDifficulty, ReportedRun};
    let stage = StageId(1);
    let mut rules = MatchRules::hosted();
    rules.difficulty = MatchDifficulty::Hard;
    rules.boos = true;
    // As a client one build behind would send them.
    rules.ruleset = "itg-fa+".into();
    let mut odd = MatchRules::hosted();
    odd.ruleset = "danoni".into();
    // Before the script's first command: the lobby's draft.
    let mut steps = script();
    let at = steps
        .iter()
        .position(|s| matches!(s, Step::Cmd(..)))
        .unwrap();
    steps.insert(
        at,
        Step::Cmd("dev:alice", Command::SaveNextRules { rules: odd.clone() }),
    );
    steps.insert(
        at + 1,
        Step::Cmd(
            "dev:alice",
            Command::SaveNextRules {
                rules: rules.clone(),
            },
        ),
    );
    // Saving the rules cleared the lobby's readiness: the script's
    // loading, Ready and Start are for the readiness that follows.
    for step in &mut steps {
        if let Step::Cmd(
            _,
            Command::ReportLoading { readiness_rev, .. }
            | Command::SetReady { readiness_rev, .. }
            | Command::StartStage { readiness_rev, .. },
        ) = step
        {
            *readiness_rev = 2;
        }
    }
    // Both still in with the same life: the score (the sum, round 3b)
    // decides.
    for (who, score) in [("dev:bob", 480_007), ("dev:alice", 480_003)] {
        steps.push(Step::Cmd(
            who,
            Command::ReportOutcome {
                stage,
                run: ReportedRun::Live {
                    life_ppm: 1_000_000,
                    accuracy_ppm: 960_000,
                    absolute: Some(Absolute {
                        score,
                        most: 5_000,
                    }),
                    score: Some(960_000 + score),
                },
                detail: None,
            },
        ));
    }
    let (local, local_answers) = run_local(&steps);
    let (server, online_answers, mirrors) = run_online(&steps, &[]);
    assert_eq!(local_answers, online_answers);
    assert_eq!(local, server, "the server's room is the local one");
    for m in &mirrors {
        assert_eq!(m, &server, "a client's mirror is the room");
    }
    assert_eq!(
        local_answers[0],
        Some(Rejection::InvalidRules(
            ddi_room::RulesProblem::UnknownRuleset
        ))
    );
    assert_eq!(local_answers[1], None, "saved");
    // Locked with the match.
    let m = local.current.as_ref().unwrap();
    assert_eq!(m.rules.difficulty, MatchDifficulty::Hard);
    assert!(m.rules.boos);
    assert_eq!(m.rules.ruleset, "itg-fa+", "kept as it came");
    assert_eq!(m.rules.ruleset_id(), "ddi", "played as DDI");
    // Bob's seven W0s more put him first.
    let play = m.stages[0].play.as_ref().unwrap();
    let result = play.result.as_ref().expect("settled on the two reports");
    assert_eq!(result.placement(ParticipantId(2)).unwrap().place, 1);
    assert_eq!(result.placement(ParticipantId(1)).unwrap().place, 2);
}

/// The network in the way of Bob's frames changes nothing anyone ends
/// up with.
#[test]
fn duplicates_drops_reordering_old_revisions_and_delays_change_nothing() {
    let steps = script();
    let (local, local_answers) = run_local(&steps);
    for faults in [
        vec![
            Fault::Duplicate,
            Fault::Duplicate,
            Fault::Duplicate,
            Fault::Duplicate,
        ],
        vec![Fault::Delay(0), Fault::Stale, Fault::Stale, Fault::Stale],
        vec![Fault::Reorder, Fault::Duplicate, Fault::Reorder],
        vec![Fault::Delay(0), Fault::Drop, Fault::Delay(0), Fault::Drop],
        vec![Fault::Delay(300), Fault::Delay(50), Fault::Duplicate],
    ] {
        let (server, answers, mirrors) = run_online(&steps, &faults);
        assert_eq!(answers, local_answers, "{faults:?}");
        assert_eq!(server, local, "{faults:?}");
        for m in &mirrors {
            assert_eq!(m, &server, "{faults:?}");
        }
    }
}

fn two(clock: &ManualClock) -> (SharedFake, Client, Client) {
    let server = hosted(clock).shared();
    server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "Bob", Intent::Play)
        .unwrap();
    let mut alice = Client::new(&server, clock, "dev:alice");
    let mut bob = Client::new(&server, clock, "dev:bob");
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(alice.link.link(), LinkState::Connected);
    assert_eq!(bob.link.link(), LinkState::Connected);
    (server, alice, bob)
}

#[test]
fn a_reconnect_after_missed_revisions_gets_the_room_again_and_sends_what_waited() {
    let clock = ManualClock::new(1_000);
    let (server, mut alice, mut bob) = two(&clock);
    let bob_id = ParticipantId(2);
    // Bob's socket drops; he is Reconnecting to everyone.
    server.borrow_mut().drop_socket(bob_id);
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(bob.link.link(), LinkState::Reconnecting { attempt: 1 });
    assert_eq!(bob.link.connection(), Connection::Reconnecting);
    assert_eq!(
        alice.room().member(bob_id).unwrap().connection,
        Connection::Reconnecting
    );
    // While he is away: the room moves on, and he asks for something.
    alice.send(Command::ChooseContent {
        stage: StageId(1),
        content: Some(content(1)),
    });
    pump_all(&mut [&mut alice]);
    let ready = bob.send(Command::ReportLoading {
        stage: StageId(1),
        readiness_rev: 1,
        loading: Loading::Loading { permille: 0 },
    });
    let missed = alice.room().revision;
    assert!(bob.room().revision < missed);
    // No ticket before the backoff's wait.
    assert!(!bob.link.wants_ticket());
    clock.advance(499);
    assert!(!bob.link.wants_ticket());
    clock.advance(1);
    assert!(bob.link.wants_ticket());
    let snapshots = bob.snapshots;
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(bob.link.link(), LinkState::Connected);
    assert_eq!(
        bob.snapshots,
        snapshots + 1,
        "the room again on the new socket"
    );
    assert_eq!(bob.answer(ready), Some(&None), "sent once connected, taken");
    assert_eq!(bob.room(), server.borrow().state());
    assert_eq!(alice.room(), server.borrow().state());
}

#[test]
fn a_command_resent_after_its_answer_was_lost_is_not_applied_twice() {
    let clock = ManualClock::new(1_000);
    let (server, mut alice, mut bob) = two(&clock);
    let bob_id = ParticipantId(2);
    // The events go through; the ack is lost with the socket.
    server.borrow_mut().fault(bob_id, Fault::Delay(0));
    server.borrow_mut().fault(bob_id, Fault::Drop);
    let seq = bob.send(Command::SetIntent {
        intent: Intent::Watch,
    });
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(bob.answer(seq), None);
    assert_eq!(bob.link.unanswered(), 1);
    let before = server.borrow().state().revision;
    server.borrow_mut().drop_socket(bob_id);
    pump_all(&mut [&mut alice, &mut bob]);
    clock.advance(500);
    pump_all(&mut [&mut alice, &mut bob]);
    // Reconnecting and connected again: two revisions; the intent once.
    assert_eq!(bob.answer(seq), Some(&None), "the same answer");
    assert_eq!(bob.link.unanswered(), 0);
    assert_eq!(server.borrow().state().revision.0, before.0 + 2);
    assert_eq!(
        server.borrow().state().member(bob_id).unwrap().intent,
        Intent::Watch
    );
    assert_eq!(bob.room(), server.borrow().state());
}

#[test]
fn a_stale_edit_is_refused_and_the_room_sent_again() {
    let clock = ManualClock::new(1_000);
    let (server, mut alice, mut bob) = two(&clock);
    alice.send(Command::ChooseContent {
        stage: StageId(1),
        content: Some(content(1)),
    });
    pump_all(&mut [&mut alice, &mut bob]);
    let snaps = alice.snapshots;
    alice.seq += 1;
    alice.link.submit(ClientEnvelope::new(
        alice.client.clone(),
        alice.seq,
        Some(Revision(1)),
        Command::ChooseContent {
            stage: StageId(1),
            content: Some(content(2)),
        },
    ));
    pump_all(&mut [&mut alice, &mut bob]);
    assert!(matches!(
        alice.answer(alice.seq),
        Some(Some(Rejection::Stale { .. }))
    ));
    assert_eq!(alice.snapshots, snaps + 1);
    assert_eq!(
        server
            .borrow()
            .state()
            .stage(StageId(1))
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .id,
        content(1),
        "never a silent overwrite"
    );
}

#[test]
fn a_gap_asks_for_the_room_once_and_mends() {
    let clock = ManualClock::new(1_000);
    let (server, mut alice, mut bob) = two(&clock);
    let bob_id = ParticipantId(2);
    server.borrow_mut().fault(bob_id, Fault::Drop);
    let snaps = bob.snapshots;
    for n in 1..=3 {
        alice.send(Command::ChooseContent {
            stage: StageId(u64::from(n)),
            content: Some(content(n)),
        });
        pump_all(&mut [&mut alice]);
    }
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(bob.snapshots, snaps + 1, "one resync for the gap");
    assert_eq!(bob.room(), server.borrow().state());
}

#[test]
fn sign_in_again_room_closed_and_left_end_the_link() {
    let clock = ManualClock::new(1_000);
    let (server, mut alice, mut bob) = two(&clock);
    // 4401: no retry, no ticket, until the person signs in again.
    server.borrow_mut().fault(
        ParticipantId(2),
        Fault::Close(ddi_room::online::close::SIGN_IN_AGAIN),
    );
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(bob.link.link(), LinkState::SignInAgain);
    assert_eq!(bob.link.connection(), Connection::Disconnected);
    clock.advance(60_000);
    assert!(!bob.link.wants_ticket());
    let queued = bob.send(Command::SetIntent {
        intent: Intent::Watch,
    });
    // Signed in again: joining, the same row, the command then sent.
    bob.link.restart();
    assert!(bob.link.wants_ticket());
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(bob.link.link(), LinkState::Connected);
    assert_eq!(bob.answer(queued), Some(&None));
    assert_eq!(server.borrow().state().members.len(), 2, "no second row");
    // A ticket refused as signed out is the same end.
    server.borrow_mut().drop_socket(ParticipantId(2));
    pump_all(&mut [&mut bob]);
    clock.advance(500);
    assert!(bob.link.wants_ticket());
    bob.link.ticket_asked();
    bob.link.ticket(Err(TicketProblem::SignedOut));
    assert_eq!(bob.link.link(), LinkState::SignInAgain);

    // 4410: the room closed.
    server.borrow_mut().fault(
        ParticipantId(1),
        Fault::Close(ddi_room::online::close::ROOM_CLOSED),
    );
    pump_all(&mut [&mut alice]);
    assert_eq!(alice.link.link(), LinkState::Closed);
    clock.advance(60_000);
    assert!(!alice.link.wants_ticket());
}

#[test]
fn leaving_closes_the_socket_as_left() {
    let clock = ManualClock::new(1_000);
    let (server, mut alice, mut bob) = two(&clock);
    let seq = bob.send(Command::Leave);
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(bob.answer(seq), Some(&None));
    assert_eq!(bob.link.link(), LinkState::Left);
    assert!(
        server
            .borrow()
            .state()
            .member(ParticipantId(2))
            .unwrap()
            .left
    );
    assert_eq!(server.borrow().sockets(), 1);
    let _ = clock.now();
}

#[test]
fn the_backoff_doubles_to_its_cap() {
    let p = ddi_room::ReconnectPolicy::RECONNECT_BACKOFF_V0;
    let waits: Vec<u64> = (1..=7).map(|a| p.wait_ms(a)).collect();
    assert_eq!(waits, vec![500, 1_000, 2_000, 4_000, 8_000, 8_000, 8_000]);
}

#[test]
fn the_resync_frame_is_the_servers_first_one() {
    let text = serde_json::to_string(&ddi_room::Resync::snapshot()).unwrap();
    assert_eq!(text, r#"{"v":2,"want":"snapshot"}"#);
    let frame: ddi_room::ClientFrame = serde_json::from_str(&text).unwrap();
    assert!(matches!(frame, ddi_room::ClientFrame::Resync(_)));
    let cmd: ddi_room::ClientFrame =
        serde_json::from_str(r#"{"v":1,"client":"c","seq":3,"cmd":"leave"}"#).unwrap();
    assert!(matches!(cmd, ddi_room::ClientFrame::Command(e) if e.seq == 3));
}

/// One picker at a time (2026-11-12, additive to protocol 2): the pick
/// taken over by a co-host and back by the owner, the refusals a
/// non-picker gets, the spotlight going with its picker (its share kept
/// known) and the fallback when the picker loses the role are kept alike
/// by the local authority, the online one and every mirror, whatever the
/// network does to Bob's frames.
#[test]
fn taking_the_pick_is_kept_alike_by_both_authorities_and_every_mirror() {
    let (alice, bob) = (ParticipantId(1), ParticipantId(2));
    let rest = |n| Command::Spotlight {
        content: Some(content(n)),
        card: None,
    };
    let co_host = |on| {
        Step::Cmd(
            "dev:alice",
            Command::SetCoHost {
                participant: bob,
                on,
            },
        )
    };
    let steps = vec![
        Step::Connect("dev:alice"),
        Step::Join("dev:bob", "Bob"),
        Step::Connect("dev:bob"),
        co_host(true),
        Step::Cmd("dev:alice", rest(1)),
        Step::Authority(Command::SetSpotlightShare {
            chart: content(1).chart,
            share: "abc12345".into(),
        }),
        // 2: the co-host does not choose until they take the pick (3).
        Step::Cmd("dev:bob", rest(2)),
        Step::Cmd("dev:bob", Command::TakePick),
        // 4, 5: the owner follows now.
        Step::Cmd(
            "dev:alice",
            Command::Pick {
                content: content(1),
                card: None,
            },
        ),
        Step::Cmd("dev:alice", rest(1)),
        Step::Cmd("dev:bob", rest(2)),
        // 7, 8: taken back; the song rested on before is shared already.
        Step::Cmd("dev:alice", Command::TakePick),
        Step::Cmd("dev:alice", rest(1)),
        // 9, 10: taken again, then the role gone: the owner's again, and
        // a member cannot take it (11).
        Step::Cmd("dev:bob", Command::TakePick),
        co_host(false),
        Step::Cmd("dev:bob", Command::TakePick),
    ];
    let (local, local_answers) = run_local(&steps);
    assert_eq!(
        local_answers,
        vec![
            None,
            None,
            Some(Rejection::NotPicker {
                picker: Some(alice)
            }),
            None,
            Some(Rejection::NotPicker { picker: Some(bob) }),
            Some(Rejection::NotPicker { picker: Some(bob) }),
            None,
            None,
            None,
            None,
            None,
            Some(Rejection::Unauthorized {
                capability: ddi_room::Capability::TakePick
            }),
        ]
    );
    assert_eq!(local.picker(), Some(alice));
    assert_eq!(local.spotlight, None, "it went with Bob's pick");
    assert_eq!(
        local.known_share(&content(1)).as_deref(),
        Some("abc12345"),
        "kept from the owner's resting song"
    );
    for faults in [
        vec![],
        vec![Fault::Duplicate, Fault::Duplicate, Fault::Duplicate],
        vec![Fault::Delay(0), Fault::Stale, Fault::Stale, Fault::Stale],
        vec![Fault::Reorder, Fault::Duplicate, Fault::Reorder],
        vec![Fault::Delay(0), Fault::Drop, Fault::Delay(0), Fault::Drop],
        vec![Fault::Delay(300), Fault::Delay(50), Fault::Duplicate],
    ] {
        let (server, answers, mirrors) = run_online(&steps, &faults);
        assert_eq!(answers, local_answers, "{faults:?}");
        assert_eq!(server, local, "{faults:?}");
        for m in &mirrors {
            assert_eq!(m, &server, "{faults:?}");
        }
    }
}

/// A socket scripted by hand: what it is given to say, and what was sent
/// on it.
#[derive(Default)]
struct Scripted {
    incoming: Vec<ddi_room::TransportEvent>,
    sent: Vec<String>,
}

impl ddi_room::Transport for Scripted {
    fn open(&mut self, _room: &RoomId, _ticket: &str) {}
    fn send(&mut self, text: String) {
        self.sent.push(text);
    }
    fn close(&mut self) {}
    fn poll(&mut self) -> Vec<ddi_room::TransportEvent> {
        std::mem::take(&mut self.incoming)
    }
}

/// How a client that predates something meets an authority that has it
/// (as a build from before the picker meets `picker_changed` and
/// `not_picker`): a frame it cannot read is passed over, never an error;
/// the revision it missed shows as a gap at the next one it can read, and
/// the room asked for again brings it back in step, unknown fields and
/// all.
#[test]
fn frames_a_client_cannot_read_are_passed_over_and_mended_by_the_room_again() {
    let clock = ManualClock::new(0);
    let mut link = OnlineAuthority::new(RoomId::new(ROOM), Scripted::default(), clock.clone());
    link.ticket_asked();
    link.ticket(Ok("ticket".into()));
    let room = RoomState::new(
        RoomId::new(ROOM),
        Identity::new("dev:alice"),
        "Alice".into(),
        policy(),
        MatchRules::hosted(),
        ContentSource::Shared,
    );
    let snapshot = |room: &RoomState, extra: bool| {
        let mut v = serde_json::json!({ "v": 2, "rev": room.revision, "msg": {
            "kind": "snapshot", "room": room } });
        if extra {
            v["msg"]["room"]["a_later_builds_field"] = serde_json::json!({ "who": 2 });
        }
        ddi_room::TransportEvent::Text(v.to_string())
    };
    let text = |v: serde_json::Value| ddi_room::TransportEvent::Text(v.to_string());
    link.transport_mut().incoming = vec![ddi_room::TransportEvent::Opened, snapshot(&room, false)];
    assert_eq!(link.poll().len(), 1);
    assert_eq!(link.link(), LinkState::Connected);
    // A command of ours, then the frames of a later build: an event and a
    // refusal this build has no name for.
    link.submit(ClientEnvelope::new(
        ClientId::new("c"),
        1,
        Some(Revision(0)),
        Command::Spotlight {
            content: Some(content(1)),
            card: None,
        },
    ));
    link.transport_mut().incoming = vec![
        text(serde_json::json!({ "v": 2, "rev": 1, "msg": { "kind": "events",
            "events": [{ "a_later_builds_event": { "who": 2 } }] } })),
        text(serde_json::json!({ "v": 2, "rev": 1, "msg": { "kind": "reject",
            "client": "c", "seq": 1,
            "rejection": { "a_later_builds_refusal": { "who": 2 } } } })),
    ];
    assert!(link.poll().is_empty(), "passed over, nothing handed on");
    assert_eq!(link.link(), LinkState::Connected);
    assert_eq!(link.room().unwrap().revision, Revision(0));
    assert_eq!(link.unanswered(), 1, "its answer was not read");
    let asked = link.transport().sent.len();
    // The next revision it can read is a gap: the room asked for again.
    link.transport_mut().incoming = vec![text(serde_json::json!({ "v": 2, "rev": 2,
        "msg": { "kind": "events", "events": [{ "spotlight_changed": { "spotlight": null } }] } }))];
    assert!(link.poll().is_empty());
    let sent = &link.transport().sent;
    assert_eq!(sent.len(), asked + 1);
    assert_eq!(sent[asked], r#"{"v":2,"want":"snapshot"}"#);
    // The room as it is now (a field of the later build in it): in step.
    let mut now = room.clone();
    now.revision = Revision(2);
    link.transport_mut().incoming = vec![snapshot(&now, true)];
    assert_eq!(link.poll().len(), 1);
    assert_eq!(link.room().unwrap().revision, Revision(2));
}

/// The host's succession (2026-11-12, additive to protocol 2): the
/// setting, the handover and back, the refusals of those who may not,
/// and the room passing to the co-host when its owner leaves are kept
/// alike by the local authority, the online one and every mirror,
/// whatever the network does to Bob's frames.
#[test]
fn the_hosts_succession_is_kept_alike_by_both_authorities_and_every_mirror() {
    use ddi_room::{Capability, HostLeaves, Role};
    let (alice, bob, carol) = (ParticipantId(1), ParticipantId(2), ParticipantId(3));
    let policy = |policy| Command::SetHostLeaves { policy };
    let make = |participant| Command::MakeHost { participant };
    let steps = vec![
        Step::Connect("dev:alice"),
        Step::Join("dev:bob", "Bob"),
        Step::Connect("dev:bob"),
        Step::Join("dev:carol", "Carol"),
        Step::Cmd(
            "dev:alice",
            Command::SetCoHost {
                participant: bob,
                on: true,
            },
        ),
        // 1: the setting is the owner's (2, 3).
        Step::Cmd("dev:bob", policy(HostLeaves::Close)),
        Step::Cmd("dev:alice", policy(HostLeaves::Close)),
        Step::Cmd("dev:alice", policy(HostLeaves::PassOn)),
        // 4: so is handing the room over (5), never to one who is not
        // connected (6).
        Step::Cmd("dev:bob", make(bob)),
        Step::Cmd("dev:alice", make(bob)),
        Step::Cmd("dev:bob", make(carol)),
        // 7: the one before is a co-host; 8: handed back.
        Step::Cmd(
            "dev:alice",
            Command::SetCoHost {
                participant: carol,
                on: true,
            },
        ),
        Step::Cmd("dev:bob", make(alice)),
        // 9: the owner leaves; the room is the co-host's.
        Step::Cmd("dev:alice", Command::Leave),
    ];
    let (local, local_answers) = run_local(&steps);
    assert_eq!(
        local_answers,
        vec![
            None,
            Some(Rejection::Unauthorized {
                capability: Capability::ChangeAccess
            }),
            None,
            None,
            Some(Rejection::Unauthorized {
                capability: Capability::MakeHost
            }),
            None,
            Some(Rejection::NotPresent),
            Some(Rejection::Unauthorized {
                capability: Capability::ManageCoHosts
            }),
            None,
            None,
        ]
    );
    assert_eq!(local.owner().map(|m| m.id), Some(bob));
    assert_eq!(local.picker(), Some(bob));
    let a = local.member(alice).unwrap();
    assert!(a.left && a.role == Role::CoHost);
    assert_eq!(local.if_host_leaves(), HostLeaves::PassOn);
    assert_eq!(local.closed, None);
    for faults in [
        vec![],
        vec![Fault::Duplicate, Fault::Duplicate, Fault::Duplicate],
        vec![Fault::Delay(0), Fault::Stale, Fault::Stale, Fault::Stale],
        vec![Fault::Reorder, Fault::Duplicate, Fault::Reorder],
        vec![Fault::Delay(0), Fault::Drop, Fault::Delay(0), Fault::Drop],
        vec![Fault::Delay(300), Fault::Delay(50), Fault::Duplicate],
    ] {
        let (server, answers, mirrors) = run_online(&steps, &faults);
        assert_eq!(answers, local_answers, "{faults:?}");
        assert_eq!(server, local, "{faults:?}");
        for m in &mirrors {
            assert_eq!(m, &server, "{faults:?}");
        }
    }
}

/// A host whose socket is gone keeps the room inside the server's
/// reconnect grace (a reload); past it the room passes to whoever is
/// connected, and the host comes back a co-host.
#[test]
fn a_dropped_host_is_waited_for_then_the_room_passes_on() {
    use ddi_room::Role;
    let (a, b) = (ParticipantId(1), ParticipantId(2));
    let owner = |c: &Client| c.room().owner().map(|m| m.id);
    // Inside the grace: back with everything.
    let clock = ManualClock::new(1_000);
    let (server, mut alice, mut bob) = two(&clock);
    server.borrow_mut().set_grace(Some(30_000));
    server.borrow_mut().drop_socket(a);
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(
        bob.room().member(a).unwrap().connection,
        Connection::Reconnecting
    );
    clock.advance(29_000);
    pump_all(&mut [&mut bob]);
    assert_eq!(owner(&bob), Some(a));
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(alice.link.link(), LinkState::Connected);
    clock.advance(120_000);
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!((owner(&bob), bob.room().picker()), (Some(a), Some(a)));

    // Past it: Bob's, with the pick; Alice, back, is a co-host and in
    // step.
    server.borrow_mut().drop_socket(a);
    pump_all(&mut [&mut alice, &mut bob]);
    clock.advance(30_000);
    pump_all(&mut [&mut bob]);
    assert_eq!((owner(&bob), bob.room().picker()), (Some(b), Some(b)));
    assert_eq!(
        bob.room().member(a).unwrap().connection,
        Connection::Disconnected
    );
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(alice.link.link(), LinkState::Connected);
    assert_eq!(alice.room(), server.borrow().state());
    assert_eq!(alice.room().member(a).unwrap().role, Role::CoHost);
    // One command hands it back.
    let seq = bob.send(Command::MakeHost { participant: a });
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(bob.answer(seq), Some(&None));
    assert_eq!(owner(&alice), Some(a));
    assert_eq!(bob.room(), server.borrow().state());
}

/// A room that ends closes every socket with 4410 (the one who left is
/// told they left), opens none again and gives no ticket: under "Close
/// the room" when its host leaves or is away past the grace, and when a
/// host leaves with nobody to pass it to.
#[test]
fn a_room_that_ends_closes_every_socket() {
    use ddi_room::{Closure, HostLeaves};
    let a = ParticipantId(1);
    let close = Command::SetHostLeaves {
        policy: HostLeaves::Close,
    };
    // Leaving under Close.
    let clock = ManualClock::new(1_000);
    let (server, mut alice, mut bob) = two(&clock);
    alice.send(close.clone());
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(bob.room().if_host_leaves(), HostLeaves::Close);
    let seq = alice.send(Command::Leave);
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(alice.answer(seq), Some(&None));
    assert_eq!(alice.link.link(), LinkState::Left);
    assert_eq!(bob.link.link(), LinkState::Closed);
    assert_eq!(bob.room().closed, Some(Closure::HostLeft));
    assert_eq!(server.borrow().sockets(), 0);
    clock.advance(60_000);
    pump_all(&mut [&mut alice, &mut bob]);
    assert!(!bob.link.wants_ticket());
    assert_eq!(
        server.borrow_mut().ticket(&Identity::new("dev:bob")),
        Err(Rejection::RoomClosed)
    );

    // Away past the grace under Close: for everyone, the host included
    // when they come back.
    let clock = ManualClock::new(1_000);
    let (server, mut alice, mut bob) = two(&clock);
    server.borrow_mut().set_grace(Some(30_000));
    alice.send(close);
    pump_all(&mut [&mut alice, &mut bob]);
    server.borrow_mut().drop_socket(a);
    pump_all(&mut [&mut alice, &mut bob]);
    clock.advance(30_000);
    pump_all(&mut [&mut bob]);
    assert_eq!(bob.link.link(), LinkState::Closed);
    pump_all(&mut [&mut alice]);
    assert_eq!(alice.link.link(), LinkState::Closed);

    // Alone, passing it on: nobody to pass it to.
    let clock = ManualClock::new(1_000);
    let server = hosted(&clock).shared();
    let mut alice = Client::new(&server, &clock, "dev:alice");
    pump_all(&mut [&mut alice]);
    alice.send(Command::Leave);
    pump_all(&mut [&mut alice]);
    assert_eq!(alice.link.link(), LinkState::Left);
    assert_eq!(server.borrow().state().closed, Some(Closure::HostLeft));
}

/// Every player's place taken (a cap of two here): a join asking to play
/// is in to watch, the same in the fake server's room, a local
/// authority's and the members' mirrors; the watcher's Play is refused
/// until a place frees up, and then taken when they choose it.
#[test]
fn a_join_to_a_room_full_of_players_watches_alike_everywhere() {
    let cap2 = RoomPolicy {
        max_competitors: 2,
        ..RoomPolicy::default()
    };
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        ROOM,
        Identity::new("dev:alice"),
        "Alice",
        cap2.clone(),
        clock.clone(),
    )
    .shared();
    let mut alice = Client::new(&server, &clock, "dev:alice");
    let mut bob = Client::new(&server, &clock, "dev:bob");
    let bob_id = server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "Bob", Intent::Play)
        .unwrap();
    assert!(server.borrow().state().players_full());
    let carol = server
        .borrow_mut()
        .join(Identity::new("dev:carol"), "Carol", Intent::Play)
        .expect("in to watch, not refused");
    pump_all(&mut [&mut alice, &mut bob]);
    for room in [server.borrow().state(), alice.room(), bob.room()] {
        assert_eq!(room.member(bob_id).unwrap().intent, Intent::Play);
        assert_eq!(room.member(carol).unwrap().intent, Intent::Watch);
    }
    // The same room through a local authority.
    let mut local = LocalAuthority::new(
        RoomState::new(
            RoomId::new(ROOM),
            Identity::new("dev:alice"),
            "Alice".into(),
            cap2,
            MatchRules::hosted(),
            ContentSource::Shared,
        ),
        clock.clone(),
    );
    for (who, name) in [("dev:bob", "Bob"), ("dev:carol", "Carol")] {
        local
            .authority(Command::Join {
                identity: Identity::new(who),
                name: name.into(),
                admin: false,
                intent: Intent::Play,
            })
            .unwrap();
    }
    assert_eq!(local.state().member(carol).unwrap().intent, Intent::Watch);
    // Bob watches: Carol's Play is taken; before that it was refused.
    let carol_play = Command::SetIntent {
        intent: Intent::Play,
    };
    assert!(matches!(
        local.state().clone().apply(
            ddi_room::Actor::Member(carol),
            None,
            &carol_play,
            ddi_room::Millis(2_000)
        ),
        Err(Rejection::Cap(_))
    ));
    let seq = bob.send(Command::SetIntent {
        intent: Intent::Watch,
    });
    pump_all(&mut [&mut alice, &mut bob]);
    assert_eq!(bob.answer(seq), Some(&None));
    assert!(!alice.room().players_full());
    assert_eq!(
        alice.room().member(carol).unwrap().intent,
        Intent::Watch,
        "nobody is moved to Play by a place freeing up"
    );
}

/// The watch-only link: the fake server's join holds the newcomer to
/// watching, every mirror agrees, Play is refused until the owner lets
/// them play.
#[test]
fn a_watch_only_join_is_held_alike_everywhere_until_the_owner_lets_them_play() {
    let clock = ManualClock::new(1_000);
    let server = hosted(&clock).shared();
    let mut alice = Client::new(&server, &clock, "dev:alice");
    let mut fan = Client::new(&server, &clock, "dev:fan");
    let id = server
        .borrow_mut()
        .join_watch_only(Identity::new("dev:fan"), "Fan")
        .unwrap();
    // Again: the same row, nothing more.
    assert_eq!(
        server
            .borrow_mut()
            .join_watch_only(Identity::new("dev:fan"), "Fan")
            .unwrap(),
        id
    );
    pump_all(&mut [&mut alice, &mut fan]);
    for room in [server.borrow().state(), alice.room(), fan.room()] {
        let m = room.member(id).unwrap();
        assert_eq!((m.intent, m.watch_only), (Intent::Watch, true));
    }
    let seq = fan.send(Command::SetIntent {
        intent: Intent::Play,
    });
    pump_all(&mut [&mut alice, &mut fan]);
    assert_eq!(fan.answer(seq), Some(&Some(Rejection::WatchOnly)));
    let seq = alice.send(Command::SetWatchOnly {
        participant: id,
        on: false,
    });
    pump_all(&mut [&mut alice, &mut fan]);
    assert_eq!(alice.answer(seq), Some(&None));
    assert!(!fan.room().member(id).unwrap().watch_only);
    let seq = fan.send(Command::SetIntent {
        intent: Intent::Play,
    });
    pump_all(&mut [&mut alice, &mut fan]);
    assert_eq!(fan.answer(seq), Some(&None));
    assert_eq!(server.borrow().state().member(id).unwrap().intent, Intent::Play);
}

/// A hosted room's default Attacks rule follows what it really plays: On
/// where the room plays live (as practice's), Off where it does not; the
/// fake server makes its rooms as the server does.
#[test]
fn a_hosted_rooms_attacks_are_on_by_default_only_where_it_plays_live() {
    let live = RoomPolicy::default().with_live();
    assert!(MatchRules::hosted_for(&live).attacks());
    assert!(!MatchRules::hosted_for(&RoomPolicy::default()).attacks());
    assert_eq!(
        MatchRules::hosted_for(&RoomPolicy::default()),
        MatchRules::hosted()
    );
    let mut on = MatchRules::hosted();
    on.set_attacks(true);
    assert_eq!(MatchRules::hosted_for(&live), on, "nothing else differs");
    let clock = ManualClock::new(1_000);
    for (policy, attacks) in [(live, true), (RoomPolicy::default(), false)] {
        let server = FakeServer::hosted(
            ROOM,
            Identity::new("dev:alice"),
            "Alice",
            policy,
            clock.clone(),
        );
        assert_eq!(server.state().next.rules.attacks(), attacks);
    }
}

/// "Apply and restart" (2026-11-13): the local authority and the online
/// one (the fake server, its members' mirrors) decide `RestartMatch` the
/// same: saved and stopped as one change by a co-host during the stage,
/// then refused with no match left to stop.
#[test]
fn a_restart_is_decided_alike_by_both_authorities_and_every_mirror() {
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    rules.boos = true;
    let mut steps = script();
    steps.push(Step::Cmd(
        "dev:bob",
        Command::RestartMatch {
            rules: rules.clone(),
        },
    ));
    steps.push(Step::Cmd(
        "dev:alice",
        Command::RestartMatch {
            rules: rules.clone(),
        },
    ));
    let (local, local_answers) = run_local(&steps);
    let (server, online_answers, mirrors) = run_online(&steps, &[]);
    assert_eq!(local_answers, online_answers);
    assert_eq!(local, server, "the server's room is the local one");
    for m in &mirrors {
        assert_eq!(m, &server, "a client's mirror is the room");
    }
    let n = local_answers.len();
    assert_eq!(local_answers[n - 2], None, "restarted");
    assert_eq!(
        local_answers[n - 1],
        Some(Rejection::Unauthorized {
            capability: ddi_room::Capability::StopMatch
        })
    );
    assert_eq!(local.phase.kind(), PhaseKind::Interrupted);
    assert_eq!(local.next.rules, rules);
    assert!(local.policy.restart, "the room says it reads the command");
}
