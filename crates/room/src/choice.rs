//! Choosing by the group (room model revision 22, round RL4: stage 4 of
//! `docs/plans/multiplayer-room-songs.md`). Under the match rule "Song
//! choice: Group" ([`SongChoice::Group`]) each member who will play the
//! next stage marks one song of the room's list (and one of its charts)
//! as their choice, or none ([`Choice`], [`crate::Command::Choose`]);
//! everyone sees who chose what as it is chosen. Nothing counts down
//! until a host or co-host (an administrator too: a co-host's powers) has
//! a choice of their own standing while the room could pick: that opens
//! the draw's countdown ([`Draw`], [`DRAW_V0`]). It ends early once every
//! player who may choose has chosen (never before [`DrawRules::least_ms`]);
//! it is cancelled once every choice has gone. At its end the authority
//! draws one member's choice at random ([`crate::Command::Draw`], the
//! roll a number only it makes, at that moment): each choosing member one
//! entry, so a song two members chose has two chances. The drawn song is
//! then picked as the picker's pick of a listed song is, and the choices
//! are cleared. The draw is kept ([`DrawRecord`]: the entries in order,
//! the winner and the roll), so every device shows the same reveal and a
//! stored room replays it.
//!
//! The maintainer's decisions of 2026-11-19 (`room-list-decisions.md`):
//! Host by default; only those who will play choose (no watchers, no
//! audience, no CPUs); no separate Draw action ("The draw should be
//! triggered by a host or co-host choice, so there should be at least one
//! choice otherwise the draw is cancelled"); choices cleared after each
//! draw; repeats Allowed by default, Not in a row and Not in a match as
//! a rule ([`Repeats`]).

use crate::ids::{ChartKey, Millis, ParticipantId, Revision, RoomSongId, StageId};
use serde::{Deserialize, Serialize};

/// The match rule "Song choice" (a per-stage rule, `crate::rules::PlayRules`).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum SongChoice {
    /// The picker picks from the room's list (as ever).
    #[default]
    Host,
    /// The members who will play choose; a draw takes one of their
    /// choices. Only where the room has a song list (an online room).
    Group,
}

impl SongChoice {
    pub const ALL: [SongChoice; 2] = [Self::Host, Self::Group];

    pub fn is_host(&self) -> bool {
        *self == Self::Host
    }

    /// Its place in [`SongChoice::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// The match rule "Repeats" under Song choice: Group (a per-stage rule): a
/// song the rule excludes cannot be chosen. Where it would leave no song
/// of the list to choose, it gives way: every song may be chosen
/// ([`crate::RoomState::excluded_songs`]).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Repeats {
    /// Any song, again and again (the maintainer's default).
    #[default]
    Allowed,
    /// Never the song of the stage just before (the last stage with a
    /// song: this match's, or the last match's last where this is a
    /// match's first).
    NotInARow,
    /// Never a song a stage of this match played or picked already.
    NotInAMatch,
}

impl Repeats {
    pub const ALL: [Repeats; 3] = [Self::Allowed, Self::NotInARow, Self::NotInAMatch];

    pub fn is_allowed(&self) -> bool {
        *self == Self::Allowed
    }

    /// Its place in [`Repeats::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// The draw's numbers (`draw-v0`, PROVISIONAL), the same on every device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawRules {
    pub id: &'static str,
    /// From the countdown's opening to the draw.
    pub countdown_ms: u64,
    /// The countdown ends early once every player who may choose has a
    /// choice, but never before this, so that it is seen.
    pub least_ms: u64,
    /// How long the screens show the draw's reveal before the stage's
    /// Ready card (every device alike; the room's timing does not wait
    /// for it: the stage's readiness window opens at the draw).
    pub reveal_ms: u64,
}

/// `draw-v0` (round RL4, PROVISIONAL): ten seconds, early once everyone
/// chose but not before three; a reveal of three seconds.
pub const DRAW_V0: DrawRules = DrawRules {
    id: "draw-v0",
    countdown_ms: 10_000,
    least_ms: 3_000,
    reveal_ms: 3_000,
};

/// One member's choice: a listed song and one of its charts (the chart a
/// pick of it would name), and the revision it was made at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub song: RoomSongId,
    pub chart: ChartKey,
    pub at: Revision,
}

/// The draw's countdown, open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draw {
    /// The stage it picks the song of.
    pub stage: StageId,
    /// The host or co-host whose own choice opened it.
    pub by: ParticipantId,
    pub opened_at: Millis,
    /// Its end at the latest (`DRAW_V0.countdown_ms` after its opening);
    /// earlier where everyone chose ([`crate::RoomState::draw_ends_at`]).
    pub ends_at: Millis,
}

/// One entry of a draw: a member's choice as it stood.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawEntry {
    pub participant: ParticipantId,
    pub song: RoomSongId,
    pub chart: ChartKey,
}

/// A draw made ([`crate::Event::Drawn`]): its entries by participant id,
/// the winner's index among them and the authority's roll (`winner` is
/// `roll % entries.len()`), when, and for which stage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawRecord {
    pub stage: StageId,
    pub entries: Vec<DrawEntry>,
    pub winner: u16,
    pub roll: u64,
    pub at: Millis,
}

impl DrawRecord {
    /// The entry drawn.
    pub fn won(&self) -> Option<&DrawEntry> {
        self.entries.get(usize::from(self.winner))
    }

    /// The winner's index for `roll` among `n` entries (none: no entry).
    pub fn winner_of(roll: u64, n: usize) -> Option<u16> {
        (n > 0).then(|| u16::try_from(roll % n as u64).unwrap_or(0))
    }
}

/// Why a draw's countdown ended without a draw ([`crate::Event::DrawCanceled`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawEnd {
    /// Every choice went before it ended.
    Empty,
    /// The stage's rule is Song choice: Host now, or the room can no
    /// longer pick that stage (the match stopped, a stage began).
    Rules,
    /// At its end the drawn song could not be picked (too few players,
    /// the next match's rules to review): the choices stand.
    NoStart,
}

/// Why someone may not choose ([`crate::Rejection::CannotChoose`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoChoice {
    /// They watch (by their own choice, or held to it by the watch-only
    /// link): only those who will play choose.
    Watching,
    /// They play from the next match only (the running match seats
    /// nobody new now).
    NextMatch,
    /// Out of the running match.
    OutForMatch,
}

use crate::command::Rejection;
use crate::member::{Capability, Intent, can};
use crate::phase::Phase;
use crate::rules::{ContentSource, MatchRules};
use crate::state::{Match, RoomState};
use std::collections::BTreeSet;

impl RoomState {
    /// The stage a pick made now would take (and so a draw): the next
    /// match's first in the lobby and after a match; between stages the
    /// next; a stage being prepared with no readiness window open. None
    /// while a stage's window, countdown or play runs.
    pub fn pick_stage(&self) -> Option<StageId> {
        match &self.phase {
            Phase::Lobby | Phase::MatchResults | Phase::Interrupted { .. } => {
                self.playlist.first().map(|s| s.id)
            }
            Phase::Intermission { next } => Some(*next),
            Phase::Preparing {
                stage,
                window: None,
            } => Some(*stage),
            _ => None,
        }
    }

    /// The stage being readied or played now, if any.
    fn stage_under_way(&self) -> Option<StageId> {
        match &self.phase {
            Phase::Preparing { stage, .. }
            | Phase::Countdown { stage, .. }
            | Phase::Stage { stage }
            | Phase::SongCompletion { stage } => Some(*stage),
            _ => None,
        }
    }

    /// The stage choices are for now (revision 22): the one a pick would
    /// take ([`RoomState::pick_stage`]); while a stage's window,
    /// countdown or play runs, the one after it (the next match's first
    /// after a match's last). None in an open-ended match whose next
    /// stage is not made yet: its rules are then the stage under way's.
    pub fn choosing_for(&self) -> Option<StageId> {
        if let Some(s) = self.pick_stage() {
            return Some(s);
        }
        let stage = self.stage_under_way()?;
        let m = self.current.as_ref()?;
        if m.is_last(stage, self.policy.max_stages) {
            return self.playlist.first().map(|s| s.id);
        }
        let i = m.stage_index(stage)?;
        m.stages.get(i + 1).map(|s| s.id)
    }

    /// Song choice and Repeats as the stage choices are for plays them
    /// ([`RoomState::choosing_for`]).
    pub fn choice_rules(&self) -> (SongChoice, Repeats) {
        let of = |r: &MatchRules| (r.choice, r.repeats);
        let in_match = |stage: StageId| {
            self.current
                .as_ref()
                .filter(|m| m.stage(stage).is_some())
                .map(|m| of(&m.rules_for(stage)))
        };
        match self.choosing_for() {
            Some(stage) => in_match(stage).unwrap_or_else(|| of(&self.next.rules)),
            None => self
                .stage_under_way()
                .and_then(in_match)
                .unwrap_or_else(|| of(&self.next.rules)),
        }
    }

    /// The next stage's song is chosen by the group: Song choice: Group in
    /// a room with a song list.
    pub fn group_choice(&self) -> bool {
        self.source == ContentSource::Shared && self.choice_rules().0 == SongChoice::Group
    }

    /// Why `id` may not choose now, if anything: the room has no list,
    /// the rule is Host, or they will not play the next stage
    /// ([`RoomState::chooser_refusal`]).
    pub fn choice_refusal(&self, id: ParticipantId) -> Option<Rejection> {
        if self.source != ContentSource::Shared {
            return Some(Rejection::NoSongList);
        }
        if !self.group_choice() {
            return Some(Rejection::NotGroupChoice);
        }
        self.chooser_refusal(id)
    }

    /// Why `id` will not play the stage choices are for, if not: a person
    /// present (a bot too) who plays, seated in the running match or to
    /// be seated in its next stage (joining during a match, a place free);
    /// before a match, anyone who plays. Never a CPU, a watcher, one out
    /// of the match or one who waits for the next match.
    pub fn chooser_refusal(&self, id: ParticipantId) -> Option<Rejection> {
        let Some(m) = self.members.get(&id) else {
            return Some(Rejection::NotAMember);
        };
        if m.left {
            return Some(Rejection::NotPresent);
        }
        if m.is_cpu() {
            return Some(Rejection::NotAHuman);
        }
        if m.intent == Intent::Watch {
            return Some(Rejection::CannotChoose {
                why: NoChoice::Watching,
            });
        }
        let target = self.choosing_for();
        let in_match = self
            .running_match()
            .filter(|mm| target.is_none_or(|t| mm.stage(t).is_some()));
        if let Some(mm) = in_match {
            match mm.roster.get(&id) {
                Some(seat) if seat.out_for_match => {
                    return Some(Rejection::CannotChoose {
                        why: NoChoice::OutForMatch,
                    });
                }
                Some(_) => {}
                None if self.admits_next_stage() && self.late_places() > 0 => {}
                None => {
                    return Some(Rejection::CannotChoose {
                        why: NoChoice::NextMatch,
                    });
                }
            }
        }
        None
    }

    /// The people who may choose now and are there (connected, or
    /// reconnecting for a moment): whom the countdown's early end waits
    /// for.
    pub fn choosers(&self) -> Vec<ParticipantId> {
        if !self.group_choice() {
            return Vec::new();
        }
        self.members
            .values()
            .filter(|m| {
                m.present_person()
                    && m.connection.holds_place()
                    && self.chooser_refusal(m.id).is_none()
            })
            .map(|m| m.id)
            .collect()
    }

    /// The listed songs the Repeats rule excludes from the choices now,
    /// with the rule: Not in a row, the song of the last stage before the
    /// one chosen for (this match's, else the last match's last); Not in
    /// a match, every song a stage of this match played or picked. Where
    /// it would exclude every song there to load, it gives way: none.
    pub fn excluded_songs(&self) -> (Repeats, BTreeSet<RoomSongId>) {
        let (_, repeats) = self.choice_rules();
        let mut out = BTreeSet::new();
        if repeats == Repeats::Allowed {
            return (repeats, out);
        }
        let target = self.choosing_for();
        let songs = |m: &Match| -> Vec<RoomSongId> {
            m.stages
                .iter()
                .filter(|s| Some(s.id) != target)
                .filter_map(|s| s.content.as_ref()?.song)
                .collect()
        };
        // The match the choice is for, where it is the running one.
        let this_match = self
            .running_match()
            .filter(|m| target.is_none_or(|t| m.stage(t).is_some()));
        match repeats {
            Repeats::Allowed => {}
            Repeats::NotInAMatch => {
                if let Some(m) = this_match {
                    out.extend(songs(m));
                }
            }
            Repeats::NotInARow => {
                let last = this_match
                    .and_then(|m| songs(m).last().copied())
                    .or_else(|| self.current.as_ref().and_then(|m| songs(m).last().copied()))
                    .or_else(|| {
                        self.history.last().and_then(|r| {
                            r.played
                                .stages
                                .iter()
                                .filter_map(|s| s.content.as_ref()?.song)
                                .next_back()
                        })
                    });
                out.extend(last);
            }
        }
        let mut there = self.songs.entries.iter().filter(|e| e.available()).peekable();
        if there.peek().is_some() && there.all(|e| out.contains(&e.id)) {
            out.clear();
        }
        (repeats, out)
    }

    /// Why `id` may not choose chart `chart` of listed song `song` now,
    /// if anything: who they are ([`RoomState::choice_refusal`]), the
    /// song (listed, its chart, there to load) and the Repeats rule. What
    /// the reducer decides [`crate::Command::Choose`] by, and what a
    /// client asks before it sends one.
    pub fn choose_refusal(
        &self,
        id: ParticipantId,
        song: RoomSongId,
        chart: &ChartKey,
    ) -> Option<Rejection> {
        if let Some(why) = self.choice_refusal(id) {
            return Some(why);
        }
        let Some(e) = self.songs.get(song) else {
            return Some(Rejection::NotListed);
        };
        if e.chart(chart).is_none() {
            return Some(Rejection::ChartNotInSong);
        }
        if !e.available() {
            return Some(Rejection::SongNotReady);
        }
        let (rule, excluded) = self.excluded_songs();
        if excluded.contains(&song) {
            return Some(Rejection::Repeated { rule });
        }
        None
    }

    /// The host or co-host whose own standing choice opens the draw's
    /// countdown (an administrator too: a co-host's powers): the earliest
    /// such choice, by when it was made, then id.
    pub fn draw_starter(&self) -> Option<ParticipantId> {
        self.choices
            .iter()
            .filter(|(id, _)| {
                self.members.get(id).is_some_and(|m| {
                    m.present_person()
                        && can(m.role, m.admin, Capability::StartDraw, self.phase.kind())
                })
            })
            .min_by_key(|(id, c)| (c.at, **id))
            .map(|(id, _)| *id)
    }

    /// When the draw's countdown ends: at its end, or once every player
    /// who may choose has chosen, [`DrawRules::least_ms`] after it opened
    /// at the earliest. None: no countdown.
    pub fn draw_ends_at(&self) -> Option<Millis> {
        let d = self.draw.as_ref()?;
        let choosers = self.choosers();
        let everyone = !choosers.is_empty() && choosers.iter().all(|id| self.choices.contains_key(id));
        Some(if everyone {
            d.ends_at.min(d.opened_at.after(DRAW_V0.least_ms))
        } else {
            d.ends_at
        })
    }

    /// The draw is due by `now`: the authority makes its roll and issues
    /// [`crate::Command::Draw`].
    pub fn draw_due(&self, now: Millis) -> bool {
        self.draw_ends_at().is_some_and(|at| now >= at)
    }

    /// Who chose `song`, in the order the draw would take them (by id).
    pub fn chosen_by(&self, song: RoomSongId) -> Vec<ParticipantId> {
        self.choices
            .iter()
            .filter(|(_, c)| c.song == song)
            .map(|(id, _)| *id)
            .collect()
    }
}
