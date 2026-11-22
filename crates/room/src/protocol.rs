//! The versioned envelope commands and updates travel in, and a client's
//! mirror of the room built from them. JSON on the wire (serde); the same
//! types serve the local practice authority, so practice and online rooms
//! exchange the same messages.
//!
//! Client to authority: `{"v":1,"client":"…","seq":17,"base":42,"cmd":{…}}`.
//! `seq` counts within a client id (retries are idempotent); `base` is the
//! room revision the client's view was at, which conflicting edits need.
//! Authority to client: `{"v":1,"rev":43,"msg":{"kind":"events",…}}`, or an
//! `ack`, a `reject` or a full `snapshot`. `rev` is the room's revision
//! after the message.
//!
//! One more frame from a client: [`Resync`], `{"v":1,"want":"snapshot"}`,
//! which asks for the whole room after its mirror found a gap
//! ([`MirrorError::Gap`]); the authority answers it alone with a snapshot.
//! A client's frames are either ([`ClientFrame`]).

use crate::command::{Command, Event, Rejection};
use crate::ids::{ClientId, Revision};
use crate::state::RoomState;
use serde::{Deserialize, Serialize};

/// The protocol this build speaks. 2 (M7b): the match flow's `pick`,
/// `spotlight` and `set_spotlight_share` commands, the `spotlight_changed`
/// event, content cards, CPU rosters; version 1's envelopes and frames are
/// still accepted. Additive since, with no new version (an authority or a
/// client that predates it ignores it): a report's optional `detail`
/// (`crate::RunDetail`, feedback round B) on `report_outcome` and
/// `outcome_reported`, kept in a stage's `details`; one picker at a time
/// (2026-11-12): the room's `picking`, the `take_pick` command, the
/// `picker_changed` event and the `not_picker` refusal. A client sends
/// `take_pick` only to a room whose state has `picking` (an authority
/// that predates it has none, and would close the socket on a command it
/// cannot read); a client that predates it passes over the frames it
/// cannot read and asks for the room again (`Resync`). A new version is
/// for what the other side must understand; it would turn away every
/// client or server one step behind.
pub const PROTOCOL_VERSION: u32 = 2;
/// The oldest protocol an authority still accepts. The web app deploys on
/// push, the server separately and desktop builds lag, so an authority
/// keeps accepting the previous version once there is one.
pub const OLDEST_PROTOCOL_VERSION: u32 = 1;

pub fn supported(version: u32) -> bool {
    (OLDEST_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(&version)
}

/// The room model's revision: raised whenever a round changes what peers
/// must agree on (awards, roles, capabilities, seeds, the live contract):
///
/// 1. round B1 (2026-11-13);
/// 2. `attacks-v2`, `settlement-v1` (the ledger every device and the room
///    run);
/// 3. `adaptive-v0` (the Charts rule's ladder in every battle plan, a
///    stage ranked by the score under it);
/// 4. `attacks-v3` (a launch only within its window, the match's Attacks
///    at once in every battle plan);
/// 5. `cpu-levels-v1` (a CPU's level the same ability on every chart);
/// 6. `adaptive-v1` (sixteen bars back up, a chart closed after two
///    empty lives);
/// 7. `cpu-play-v1` (a CPU plays the chart a person is given, its score
///    its own);
/// 8. `hosted-deadline-v2` (the loading grace) and `adaptive-v2` (a step
///    up needs the two best judgments, a step down's breather shorter).
///
/// A client says
/// its revision where the authority first hears from it (`model` in the
/// join's, the ticket's and the audience join's request bodies); an
/// authority refuses a client whose revision is lower than its own
/// ([`check_model`], [`UPDATE_REQUIRED`]: "reload to update", the
/// maintainer: "I'd rather not focus on keeping compatibility right
/// now"). The check goes both ways since revision 9: the authority says
/// its own revision in the same answers (`model`), and a client refuses
/// an authority whose revision is older than its own, or that says none
/// ([`check_authority`], [`SERVER_BEHIND`]: "the server is being
/// updated"), opening no socket. Server and game deploy together, the
/// server first.
///
/// 9. the check both ways; the policy's version flags, the commands
///    before `Pick`, the attack ledger's recovery, the step-down flag of
///    the binary format (`BATCH_V2`, `FRAME_V2`) and ranking without a
///    score retired;
/// 10. `adaptive-v3` (an early step up as `adaptive-v2`'s, and a normal
///     one after more bars with a lower floor: every device steps the
///     room's CPUs alike);
/// 11. the maintainer's decisions of 2026-11-15, together: `adaptive-v4`
///     (the early step up asks 90% in the three best judgments), a live
///     snapshot's `step` (the chart of the ladder each competitor is on),
///     the authority's `Command::CloseRoom` with
///     `Closure::ByAdministrator`, and the public view's additions (a
///     song's banner, the Standings);
/// 12. feedback round N2 (2026-11-16): the match rule Shocks per attack
///     (`MatchRules::shocks`, four for a new room: every device places
///     the room's CPUs' attacks alike, the battle plan says the count),
///     a returning member's administrator flag kept as their session says
///     it (`Event::AdminChanged`), a member reconnecting for a moment
///     keeping their place in a stage and its readiness window, a sit-out
///     for a song still loading said as such, and a member's socket's
///     keep-alive (`crate::online::KEEPALIVE_V0`, answered by the
///     server's runtime: a server before it closed the socket on it);
///     and, in the same revision since it was deployed nowhere yet,
///     round P1a (2026-11-16): DDI's score counts its combos
///     (`ddi_engine::rules::ddi::COMBO_SCORE`, `ddi-combo-v0`: the scores
///     every device computes for the room's CPUs and reports, and ranks
///     a stage by) and `adaptive-v5` (a normal step up after six bars:
///     every device steps the room's CPUs alike); and round P1b's
///     decisions (2026-11-16, still revision 12, deployed nowhere before
///     them): a starter's Ready taken back during the countdown cancels
///     it where the policy says so
///     (`ReadinessPolicy::unready_cancels_countdown`, CPU practice's;
///     `CancelReason::ReadyTakenBack`); a member's socket closed 4429
///     (the server's fifth socket of one person) an end for that device
///     (`LinkState::OpenedElsewhere`, "Use this device" to join again);
///     a live room's clock pinged once a minute between stages
///     (`LivePolicy::quiet_ping_ms`, additive, a burst at each pick);
///     the clock's estimate starting over after a step of either clock
///     (two agreeing exchanges, `ClockEstimator::STEP_US`), the devices'
///     rooms run on one steady clock (`ddi_platform::steady`); and a late
///     entry (`Event::EnteredLate`: a member counted out of a stage's
///     lineup at its commit who readies within the live rules' late join
///     enters it; `Rejection::LateEntryClosed`, `NotLoaded`; their load
///     and readiness to play live heard after the commit; the live side
///     streams and seats them from then, `StageBattle::seat_late`);
/// 13. round U (2026-11-17): DDI's points, percent and letters
///     (`ddi_engine::rules::ddi::PERCENT_RULE`, `ddi-percent-v1`): a miss
///     costs −120 points and a mine or a battle's shock stepped on −50,
///     so the scores every device computes for the room's CPUs, reports
///     and ranks a stage by move; the percent a starter reports
///     (`RunDetail::percent`) is the new Percent (nine tenths accuracy,
///     one tenth the combo part over a full combo's), no longer the
///     accuracy alone, and the grade beside it reads it (SSS 99, SS 95,
///     S 89, A 75, B 60, C 42); and a member's own usage report on its
///     room's socket (`crate::client_stats`, `stats-v1`, the stats
///     proposal's step 2, 2026-11-17): what kind of client it is and how
///     its previous socket fared, in fixed lists, one a socket where
///     Options' Usage statistics is on; the server takes or drops it by
///     its prefix (`{"v":2,"stats":`), never answers it and never closes a
///     socket over it (an authority before it closed the socket on a
///     frame it could not read);
/// 14. round V (2026-11-17): the CPUs' ten levels measured again and
///     made more like a person (`ddi_engine::cpu::CPU_LEVELS`,
///     `cpu-levels-v2`: a steady lean of their own and a spread that
///     loosens under load before notes are left out), so every device
///     computes the room's CPUs' presses, judgments, life, attacks and
///     scores anew; a stored roster's level numbers mean the new levels;
/// 15. feedback round AA (2026-11-19): joining during a match
///     (`MatchRules::mid_match_entry`, On for a new room: a newcomer or a
///     watcher who chooses Play seated for the next stage not begun, up to
///     the competitor cap, `Participation::WaitingNextStage` and
///     `WaitingPlace`, an invite's `next_stage`); open-ended matches
///     (`MatchRules::open_ended`, `Stage::last`, `Command::SetLastStage`,
///     `Command::EndMatch`, `Event::StageAdded`, `LastStageChanged`,
///     `StagesDropped`); and under Charts: Adaptive the rule On the
///     easiest chart (`MatchRules::easiest`, Play on for a new room: a
///     competitor whose life runs out there is failed and plays on,
///     `Run::Failed`, `ReportedRun::Failed`, `Finish::Failed`, a live
///     stream's `MomentKind::Failed`, every device computing the room's
///     CPUs failing alike); and a 4401 asks for a ticket before it ends
///     a link (`SIGN_IN_RECHECK_MS`), the server's ticket carrying the
///     re-signed session's expiry;
/// 16. round AC (2026-11-19): When life runs out under both chart rules
///     (`MatchRules::easiest` read under As picked too where
///     `MatchRules::life_out_any_chart` says so, which every rule set made
///     since does: `MatchRules::life_out`, `normalized`; rules stored
///     before read as they were), and a retrying download
///     (`Loading::Retrying`) holding the readiness window's early end
///     until its loading grace would have ended
///     (`ReadinessWindow::hold_until`);
/// 17. round RL1 (2026-11-19): the room's song list (`crate::songs`,
///     `RoomState::songs`, `Command::AddSong`, `RemoveSong`,
///     `SetSongList`, `SetSongContent`, `PickListed`, `Event::SongAdded`,
///     `SongRemoved`, `SongContentSet`, `SongListChanged`,
///     `StageContent::song`, `Capability::AddSongs`, the public view's
///     `songs`): an online room's songs added by its hosts, uploaded when
///     added and picked from the list.
/// 18. round TW1 (2026-11-20, `docs/plans/multiplayer-twitch.md` B10):
///     the host's Twitch in a room (`crate::twitch`, `RoomState::twitch`,
///     `Command::SetTwitch`, `Event::TwitchChanged`: the mirror's choice
///     and the chat's ceiling, set by the authority) and removal
///     (`Command::Remove`, a host's or a co-host's Kick,
///     `Capability::Kick`, `Event::MemberRemoved`, or the mirror's,
///     told to the others as a leave; `Rejection::NotAdmitted`; the
///     sockets' closes 4403 and 4406, `LinkState::Kicked` and
///     `NotAdmitted`).
/// 19. round DB (2026-11-20): a bundled song added to the room's list is
///     uploaded as any other (`NewSong` no longer says `bundled`; an
///     entry is `Pending` until its share is there), the room's share the
///     one source, a device that has the song by its content playing its
///     own copy; an entry a room stored before listed as bundled (no
///     share) still reads and plays (`RoomSong::bundled`,
///     `tests/room-revision-18.json`). An older client would list a
///     bundled song it never uploads, so it is refused. Hosts, co-hosts
///     and administrators exempt from the per-person half of the list
///     (`RoomState::song_quota`) rides in the same revision.
/// 20. round DC (2026-11-20): Top score attacks the top score
///     (`attacks-v4`, `crate::attacks::ATTACKS_V4`: the highest score
///     reported at the checkpoint, ties by the seeded pick), each
///     checkpoint report carrying its score (`attacks::Report::score`,
///     `live::LiveReport::score`; binary `BATCH_V3`, `CPUS_V2`, the
///     earlier forms no longer read); `attacks-v3` retired (a plan naming
///     it refused as unknown). An older client would rank Top score by
///     life and send reports without scores, so it is refused. A room
///     stored by revision 19 reads as it was
///     (`tests/room-revision-19.json`).
/// 21. round RL3 (2026-11-21): who may add songs to the room's list, the
///     owner's room setting "Who can add songs" (`crate::songs::WhoAdds`,
///     Hosts or Everyone, `RoomSongs::who_adds`, `Command::SetWhoAdds`,
///     `Event::WhoAddsChanged`), read by `RoomState::may_add`; under
///     Everyone a person who is not a host adds up to half the list's size
///     and removes what they added. An older client would neither show the
///     setting nor read the event, so it is refused. A room stored by
///     revision 20 reads as Hosts (`tests/room-revision-20.json`).
/// 22. round RL4 (2026-11-21): choosing by the group (`crate::choice`):
///     the per-stage match rules Song choice (Host or Group) and Repeats,
///     each player's choice (`Command::Choose`, `Event::ChoiceChanged`),
///     the draw's countdown opened by a host's or co-host's own choice
///     (`Event::DrawOpened`, `Event::DrawCanceled`) and the authority's
///     draw (`Command::Draw`, `Event::Drawn`, `RoomState::drawn`). An older
///     client would read none of it, so it is refused. A room stored by
///     revision 21 reads as Host (`tests/room-revision-21.json`).
/// 23. round OC (2026-11-22): under Charts: Adaptive each competitor's
///     own starting chart (`Command::SetStart`, `Event::StartChosen`,
///     `Prep::start`, `RoomState::start_of`, `Rejection::ChartsAsPicked`):
///     the pick or the draw names the song, everyone starts on their own
///     chart of it (the stage's own where they chose none), which every
///     device must know to draw their field from its first note; a run's
///     starting chart in its detail (`RunDetail::start`). An older client
///     would draw everyone on the stage's chart, so it is refused. A room
///     stored by revision 22 reads with no starts and plays as it did
///     (`tests/room-revision-22.json`).
pub const ROOM_MODEL_REVISION: u32 = 23;

/// What a client makes of an authority older than itself (never sent by
/// an authority): the code its screens word as "the server is being
/// updated, try again in a moment".
pub const SERVER_BEHIND: &str = "server_behind";

/// Whether an authority that said room model revision `authority` (none:
/// one that predates revision 9, which said none) runs this client's
/// room model or a newer one; an older one is refused by the client.
pub fn check_authority(authority: Option<u32>) -> bool {
    authority.is_some_and(|r| r >= ROOM_MODEL_REVISION)
}

/// The problem code an authority answers a client too old for it with
/// (the HTTP body's `error`): a client that predates it shows it as a
/// join it could not make, with the code.
pub const UPDATE_REQUIRED: &str = "update_required";

/// A client too old for this authority's room model: it must update
/// (reload the page, or a newer desktop build).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRequired {
    /// The authority's revision.
    pub model: u32,
}

/// Whether a client of room model revision `client` (none: a client that
/// predates the revision) may join or watch this authority's rooms.
pub fn check_model(client: Option<u32>) -> Result<(), UpdateRequired> {
    check_model_against(client, ROOM_MODEL_REVISION)
}

/// [`check_model`] for an authority of revision `authority` (a test's
/// authority ahead of this build).
pub fn check_model_against(client: Option<u32>, authority: u32) -> Result<(), UpdateRequired> {
    match client {
        Some(r) if r >= authority => Ok(()),
        _ => Err(UpdateRequired { model: authority }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientEnvelope {
    pub v: u32,
    pub client: ClientId,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<Revision>,
    pub cmd: Command,
}

impl ClientEnvelope {
    pub fn new(client: ClientId, seq: u64, base: Option<Revision>, cmd: Command) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            client,
            seq,
            base,
            cmd,
        }
    }
}

/// A client's request for the room as it is, after a missed revision:
/// `{"v":1,"want":"snapshot"}`. Protocol 1 has had it from the start (the
/// server's first frame of its own), so it needs no new version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resync {
    pub v: u32,
    pub want: Want,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Want {
    Snapshot,
}

impl Resync {
    pub fn snapshot() -> Self {
        Self {
            v: PROTOCOL_VERSION,
            want: Want::Snapshot,
        }
    }
}

/// What a client sends over its socket as text: a command in its
/// envelope, a resync request, or a live message (`crate::live`: sent only
/// to a room whose policy plays live, `RoomPolicy::live`; an authority
/// that predates it would close the socket on it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClientFrame {
    Command(Box<ClientEnvelope>),
    Resync(Resync),
    Live(Box<crate::live::ClientLiveEnvelope>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEnvelope {
    pub v: u32,
    pub rev: Revision,
    pub msg: ServerMessage,
}

impl ServerEnvelope {
    pub fn new(rev: Revision, msg: ServerMessage) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            rev,
            msg,
        }
    }

    /// The whole room at its revision.
    pub fn snapshot(room: &RoomState) -> Self {
        Self::new(
            room.revision,
            ServerMessage::Snapshot {
                room: Box::new(room.clone()),
            },
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerMessage {
    /// The command was accepted (its events came before this).
    Ack { client: ClientId, seq: u64 },
    /// The command was refused and changed nothing.
    Reject {
        client: ClientId,
        seq: u64,
        rejection: Rejection,
    },
    /// One revision's events, in order.
    Events { events: Vec<Event> },
    /// The whole room, for a new or out-of-step client.
    Snapshot { room: Box<RoomState> },
}

/// A client's copy of the room, kept by applying the authority's messages.
#[derive(Clone, Debug, Default)]
pub struct Mirror {
    state: Option<RoomState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorError {
    /// Events arrived before any snapshot.
    NoSnapshot,
    /// A revision was missed: ask for a snapshot.
    Gap {
        have: Revision,
        got: Revision,
    },
    UnsupportedVersion {
        got: u32,
    },
}

impl Mirror {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> Option<&RoomState> {
        self.state.as_ref()
    }

    /// Apply one message. Events are applied only at the next revision; an
    /// old one is ignored, a later one is a gap.
    pub fn receive(&mut self, envelope: &ServerEnvelope) -> Result<(), MirrorError> {
        if !supported(envelope.v) {
            return Err(MirrorError::UnsupportedVersion { got: envelope.v });
        }
        match &envelope.msg {
            ServerMessage::Snapshot { room } => {
                if self
                    .state
                    .as_ref()
                    .is_none_or(|s| room.revision >= s.revision)
                {
                    self.state = Some((**room).clone());
                }
                Ok(())
            }
            ServerMessage::Events { events } => {
                let state = self.state.as_mut().ok_or(MirrorError::NoSnapshot)?;
                if envelope.rev <= state.revision {
                    Ok(())
                } else if envelope.rev == state.revision.next() {
                    state.commit(events);
                    Ok(())
                } else {
                    Err(MirrorError::Gap {
                        have: state.revision,
                        got: envelope.rev,
                    })
                }
            }
            ServerMessage::Ack { .. } | ServerMessage::Reject { .. } => Ok(()),
        }
    }
}
