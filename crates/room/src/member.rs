//! Room membership and room control. Who owns or operates a room, whether
//! someone wants to play or watch, and whether their connection is up are
//! separate facts; none of them is a competitive result, and none follows
//! from another. Capabilities are functions of role, admin status and phase,
//! never of participation: a watching co-host operates the room, and a
//! playing member does not.

use crate::attacks::Targeting;
use crate::ids::{Identity, ParticipantId};
use crate::phase::PhaseKind;
use serde::{Deserialize, Serialize};

/// A room member. The record outlives connections and departures, so a
/// returning person gets the same row and results.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub id: ParticipantId,
    pub kind: MemberKind,
    pub role: Role,
    /// Server administrator: assigned by the backend to a stable identity,
    /// never by a display name or a client flag. Implies co-host powers in
    /// every room, not the owner's.
    pub admin: bool,
    pub intent: Intent,
    pub connection: Connection,
    /// Left the room. The record and its results stay.
    pub left: bool,
    /// How they pick whom to attack where a match has attacks (M8 round
    /// 3, D8): their own choice ([`crate::Command::SetTargeting`]), Spread
    /// until they make one; what a stage's battle starts them with. A
    /// CPU's is its opponent's ([`OpponentSpec::targeting`]). Additive:
    /// left out of the JSON at its default, so a peer that predates it
    /// reads and writes the same room.
    #[serde(default, skip_serializing_if = "is_spread")]
    pub targeting: Targeting,
    /// Came by the room's watch-only link: watches, and may not choose
    /// Play (`Rejection::WatchOnly`) until the owner lets them
    /// ([`crate::Command::SetWatchOnly`]); an owner is never held to it.
    /// Additive: left out of the JSON when false, so a peer that
    /// predates it reads and writes the same room.
    #[serde(default, skip_serializing_if = "is_false")]
    pub watch_only: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_spread(t: &Targeting) -> bool {
    *t == Targeting::Spread
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberKind {
    /// A person. `name` is what to show, last as seen; `identity` is who.
    Human { identity: Identity, name: String },
    /// A CPU seat of one match, numbered within it.
    Cpu {
        ordinal: u16,
        opponent: OpponentSpec,
    },
}

impl Member {
    pub fn is_cpu(&self) -> bool {
        matches!(self.kind, MemberKind::Cpu { .. })
    }

    /// A bot (M7d): a person's seat played by a simulated player, its
    /// identity `bot:<name>` (the server's to give). It plays, readies and
    /// reports as a person; the screens label it Bot.
    pub fn is_bot(&self) -> bool {
        self.identity().is_some_and(Identity::is_bot)
    }

    pub fn identity(&self) -> Option<&Identity> {
        match &self.kind {
            MemberKind::Human { identity, .. } => Some(identity),
            MemberKind::Cpu { .. } => None,
        }
    }

    /// How this member picks whom to attack: a person's own choice, a
    /// CPU's as its opponent names it (Spread where it names none).
    pub fn targeting(&self) -> Targeting {
        match &self.kind {
            MemberKind::Human { .. } => self.targeting,
            MemberKind::Cpu { opponent, .. } => opponent.targeting.unwrap_or_default(),
        }
    }

    /// Present and connected: a CPU always, a person while connected.
    pub fn reachable(&self) -> bool {
        !self.left && (self.is_cpu() || self.connection == Connection::Connected)
    }
}

/// What a CPU seat plays like, opaque to the room: the stage runner turns it
/// into a real opponent. `level` and `seed` are today's CPU controls;
/// `personality` its skills (`ddi_engine::cpu::Skills`: a preset's name or
/// seven multipliers) and `name` what the room calls it (a roster's CPU,
/// M7b; none: "CPU n"); `targeting` how it picks whom to attack (M8 round
/// 3: a roster's CPU may name one; none: Spread), never shown as a skill.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpponentSpec {
    pub level: u8,
    pub seed: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targeting: Option<Targeting>,
}

impl OpponentSpec {
    /// The longest name and personality a CPU may carry (characters).
    pub const MAX_TEXT: usize = 192;

    /// Whether its texts are within [`OpponentSpec::MAX_TEXT`].
    pub fn fits(&self) -> bool {
        [&self.personality, &self.name].iter().all(|t| {
            t.as_ref()
                .is_none_or(|t| t.chars().count() <= Self::MAX_TEXT)
        })
    }
}

impl OpponentSpec {
    /// The `ordinal`th CPU of a match: the same profile, its own fixed seed.
    pub fn for_seat(&self, ordinal: u16) -> Self {
        Self {
            seed: self
                .seed
                .wrapping_add(u64::from(ordinal).wrapping_mul(0x9E37_79B9_7F4A_7C15)),
            ..self.clone()
        }
    }
}

/// Room control. One owner (the host); named co-hosts operate without
/// owning. Server administrators are a separate flag on the member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    CoHost,
    Member,
}

/// Whether a member wants a competitive seat. Watching uses no seat, makes
/// no score row and never counts as ready.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    Play,
    Watch,
}

/// A member's connection, as the authority sees it. Reconnecting keeps
/// everything; Disconnected clears an unstarted stage's readiness, nothing
/// else. Neither is a forfeit by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Connection {
    Joining,
    Connected,
    Reconnecting,
    Disconnected,
}

/// What an actor may do in the room.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// See the roster, readiness and content blockers.
    Inspect,
    /// Take part: set one's own intent, readiness and loading.
    Participate,
    /// Choose an unstarted stage's song and chart: an operator's. In a
    /// room with one picker at a time (`crate::state::Picking`) the
    /// choosing itself is the picker's alone (`RoomState::chooses`),
    /// whatever their role.
    ChooseContent,
    /// Take the pick: become the one who chooses the room's songs
    /// (`crate::Command::TakePick`). An operator's.
    TakePick,
    /// Save the next match's rules.
    EditNextRules,
    /// Start a match or a stage, cancel a start, move to the next stage or
    /// the next match.
    Advance,
    /// Stop a running match (it becomes Interrupted).
    StopMatch,
    /// Grant or revoke co-hosts.
    ManageCoHosts,
    /// Change admission and replace the invite, and choose what the room
    /// does when its host goes (`crate::Command::SetHostLeaves`): the
    /// room's own settings.
    ChangeAccess,
    /// Hand the room over (`crate::Command::MakeHost`): the owner's, who
    /// becomes a co-host.
    MakeHost,
}

/// Whether a member with `role` (and `admin`) may use `capability` in a
/// phase of `phase`. Participation never enters this: a watching operator
/// operates, a playing member does not.
pub fn can(role: Role, admin: bool, capability: Capability, phase: PhaseKind) -> bool {
    let operator = admin || matches!(role, Role::Owner | Role::CoHost);
    match capability {
        Capability::Inspect | Capability::Participate => true,
        Capability::ChooseContent | Capability::EditNextRules | Capability::TakePick => operator,
        Capability::Advance => {
            operator && !matches!(phase, PhaseKind::Stage | PhaseKind::SongCompletion)
        }
        Capability::StopMatch => operator && phase.match_running(),
        // Owner-only: co-host status and implicit admin access do not grant
        // these.
        Capability::ManageCoHosts | Capability::ChangeAccess | Capability::MakeHost => {
            role == Role::Owner
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_cohost_admin_and_member_differ() {
        let p = PhaseKind::Lobby;
        for cap in [
            Capability::ChooseContent,
            Capability::EditNextRules,
            Capability::Advance,
            Capability::TakePick,
        ] {
            assert!(can(Role::Owner, false, cap, p));
            assert!(can(Role::CoHost, false, cap, p));
            assert!(can(Role::Member, true, cap, p), "admin operates {cap:?}");
            assert!(!can(Role::Member, false, cap, p), "member cannot {cap:?}");
        }
        for cap in [
            Capability::ManageCoHosts,
            Capability::ChangeAccess,
            Capability::MakeHost,
        ] {
            assert!(can(Role::Owner, false, cap, p));
            assert!(!can(Role::CoHost, false, cap, p));
            assert!(
                !can(Role::Member, true, cap, p),
                "admin is not owner for {cap:?}"
            );
            assert!(!can(Role::CoHost, true, cap, p));
        }
        assert!(can(Role::Member, false, Capability::Participate, p));
    }

    #[test]
    fn capabilities_follow_the_phase() {
        assert!(!can(
            Role::Owner,
            false,
            Capability::Advance,
            PhaseKind::Stage
        ));
        assert!(can(
            Role::Owner,
            false,
            Capability::Advance,
            PhaseKind::Countdown
        ));
        assert!(!can(
            Role::Owner,
            false,
            Capability::StopMatch,
            PhaseKind::Lobby
        ));
        assert!(can(
            Role::CoHost,
            false,
            Capability::StopMatch,
            PhaseKind::Stage
        ));
        assert!(can(
            Role::Owner,
            false,
            Capability::ChooseContent,
            PhaseKind::Stage
        ));
    }

    #[test]
    fn cpu_seats_get_distinct_fixed_seeds_from_one_profile() {
        let spec = OpponentSpec {
            level: 1,
            seed: 7,
            personality: None,
            name: None,
            targeting: None,
        };
        let (a, b) = (spec.for_seat(1), spec.for_seat(2));
        assert_ne!(a.seed, b.seed);
        assert_eq!(a, spec.for_seat(1));
        assert_eq!((a.level, b.level), (1, 1));
    }
}
