//! The room's phase, as `docs/plans/multiplayer-ui.md`'s State model draws
//! it: Lobby → Preparing → Countdown → Stage → (SongCompletion) →
//! Intermission or MatchResults → Lobby, with Interrupted from any phase of
//! a running match. The phase is one fact among several: membership,
//! participation, readiness and connection are kept apart from it.

use crate::ids::{Millis, StageId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// No match is running; the playlist and rules are for the next one.
    Lobby,
    /// A match is running and its next stage gathers readiness. With a
    /// window, an operator pressed Start while someone was unready: at its
    /// deadline the ready and loaded start and the rest sit out.
    Preparing {
        stage: StageId,
        window: Option<ReadinessWindow>,
    },
    /// The stage's lineup is committed and starts together at `start_at`.
    Countdown { stage: StageId, start_at: Millis },
    /// The song is playing and its result is not confirmed yet.
    Stage { stage: StageId },
    /// The result is confirmed before the song's end; the winner may finish
    /// the song, which adds nothing to the result.
    SongCompletion { stage: StageId },
    /// Between stages: the last stage's results, and readiness for `next`.
    Intermission { next: StageId },
    /// The match's final song ended; overall results until the next match.
    MatchResults,
    /// The match cannot continue. Confirmed stages stay as partial results;
    /// an unconfirmed stage awards nothing and there is no overall winner.
    Interrupted { reason: Interruption },
}

/// A host-triggered readiness deadline (`ReadinessPolicy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessWindow {
    pub deadline: Millis,
    /// The stage's readiness revision the window was opened on.
    pub readiness_rev: u32,
    /// `hosted-deadline-v2`: while the room waits for everyone to load,
    /// when its loading grace ends (`deadline` is then the latest the
    /// window can end, not yet counting); `None` once the Ready countdown
    /// counts (and always under v1). ADDITIVE: left out of the JSON while
    /// `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loading_until: Option<Millis>,
}

impl ReadinessWindow {
    /// The room still waits for its players to load.
    pub fn loading(&self) -> bool {
        self.loading_until.is_some()
    }
}

/// Why a match stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interruption {
    /// An operator stopped it early.
    Stopped,
    /// The stage authority failed or lost the stage.
    AuthorityFailed,
    /// Required content became unusable for everyone.
    ContentLost,
}

/// The phase without its data, for permission checks and error reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseKind {
    Lobby,
    Preparing,
    Countdown,
    Stage,
    SongCompletion,
    Intermission,
    MatchResults,
    Interrupted,
}

impl Phase {
    pub fn kind(&self) -> PhaseKind {
        match self {
            Self::Lobby => PhaseKind::Lobby,
            Self::Preparing { .. } => PhaseKind::Preparing,
            Self::Countdown { .. } => PhaseKind::Countdown,
            Self::Stage { .. } => PhaseKind::Stage,
            Self::SongCompletion { .. } => PhaseKind::SongCompletion,
            Self::Intermission { .. } => PhaseKind::Intermission,
            Self::MatchResults => PhaseKind::MatchResults,
            Self::Interrupted { .. } => PhaseKind::Interrupted,
        }
    }
}

impl PhaseKind {
    /// A match exists and has not reached its results or interruption.
    pub fn match_running(self) -> bool {
        matches!(
            self,
            Self::Preparing
                | Self::Countdown
                | Self::Stage
                | Self::SongCompletion
                | Self::Intermission
        )
    }
}
