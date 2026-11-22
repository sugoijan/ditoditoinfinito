//! Choosing by the group on this device (room model revision 22, round
//! RL4, `ddi_room::choice`): this player's choice sent (said at the press
//! where the room would refuse it), the room's choices and countdown read
//! for the screens, and the draw's reveal: shown for
//! `ddi_room::DRAW_V0.reveal_ms` from when this device first saw the draw,
//! the stage's prompt held back meanwhile (its readiness window runs on
//! the room's clock all the same).

use super::{Refusal, RoomClient};
use ddi_room::{
    ChartKey, Choice, Command, Draw, DrawRecord, ParticipantId, Rejection, Repeats, RoomSongId,
    SongChoice, StageId,
};

/// A draw being revealed on this device: the record and when (this
/// device's clock) it began.
#[derive(Clone, Debug, PartialEq)]
pub struct Reveal {
    pub record: DrawRecord,
    pub from_ms: u64,
}

impl Reveal {
    /// How far into it `now_ms` is, 0 to 1.
    pub fn progress(&self, now_ms: u64) -> f64 {
        let ms = ddi_room::DRAW_V0.reveal_ms.max(1);
        (now_ms.saturating_sub(self.from_ms) as f64 / ms as f64).clamp(0.0, 1.0)
    }

    /// When it ends on this device's clock.
    pub fn ends_ms(&self) -> u64 {
        self.from_ms + ddi_room::DRAW_V0.reveal_ms
    }
}

/// What this device keeps of the group's choosing.
#[derive(Clone, Debug, Default)]
pub struct Choosing {
    /// The draw being revealed now, if any.
    pub(super) reveal: Option<Reveal>,
    /// The draws already revealed (or past): by stage and roll.
    pub(super) revealed: std::collections::BTreeSet<(StageId, u64)>,
    /// The countdown as first seen here: its opening (the room's clock)
    /// and this device's clock then, so its seconds count here whatever
    /// the two clocks' difference.
    pub(super) seen: Option<(ddi_room::Millis, u64)>,
}

impl RoomClient {
    /// The next stage's song is chosen by the group (Song choice: Group).
    pub fn group_choice(&self) -> bool {
        self.state().group_choice()
    }

    /// Song choice and Repeats as the next stage plays them.
    pub fn choice_rules(&self) -> (SongChoice, Repeats) {
        self.state().choice_rules()
    }

    /// This player's choice, if any.
    pub fn my_choice(&self) -> Option<&Choice> {
        self.state().choices.get(&self.me)
    }

    /// Why this player may not choose now, if anything (watching, playing
    /// from the next match only, out of the match; the rule is Host).
    pub fn choice_blocked(&self) -> Option<Rejection> {
        self.state().choice_refusal(self.me)
    }

    /// Whether this player's own choice opens the draw's countdown (a
    /// host or co-host who may choose).
    pub fn starts_draw(&self) -> bool {
        self.may(ddi_room::Capability::StartDraw) && self.choice_blocked().is_none()
    }

    /// This player's own choice stands but opens no draw: made while the
    /// room could not pick (another stage readied or played; round FX),
    /// by a host or co-host, the room able to pick now. Their Enter on the
    /// song starts the draw.
    pub fn choice_waits(&self) -> bool {
        self.state().choice_waits_for(self.me)
    }

    /// Someone else's choice waits so (a host or co-host chose while a
    /// stage was played and has not started the draw since).
    pub fn others_choice_waits(&self) -> bool {
        let s = self.state();
        s.choices.keys().any(|id| *id != self.me && s.choice_waits_for(*id))
    }

    /// Whether this player's choice made now opens the draw's countdown: a
    /// host or co-host who may choose, the room able to pick, no countdown
    /// running.
    pub fn starts_draw_now(&self) -> bool {
        let s = self.state();
        self.starts_draw() && s.draw.is_none() && s.pick_stage().is_some()
    }

    /// Enter on chart `chart` of listed song `song` under Group: chosen;
    /// the same again, cleared (or, where the choice waits, made again:
    /// the draw started); another, moved there. Refused at the press as
    /// the room would refuse it (who they are, the song, the Repeats
    /// rule).
    pub fn choose(&mut self, song: RoomSongId, chart: &ChartKey) -> Result<(), Refusal> {
        if self
            .my_choice()
            .is_some_and(|c| c.song == song && c.chart == *chart)
            && !self.choice_waits()
        {
            return self.clear_choice();
        }
        if let Some(why) = self.state().choose_refusal(self.me, song, chart) {
            return self.refuse(Refusal::Room(why));
        }
        self.send(Command::Choose {
            song: Some(song),
            chart: Some(chart.clone()),
        })
    }

    /// This player's choice cleared.
    pub fn clear_choice(&mut self) -> Result<(), Refusal> {
        if self.my_choice().is_none() {
            return Ok(());
        }
        self.send(Command::Choose {
            song: None,
            chart: None,
        })
    }

    /// Who chose `song`, in the draw's order.
    pub fn chosen_by(&self, song: RoomSongId) -> Vec<ParticipantId> {
        self.state().chosen_by(song)
    }

    /// Why `song` may not be chosen now by the Repeats rule, if it says so
    /// (as the rows and Enter's hint show it: nothing while this device
    /// reveals a draw, whose song it would give away; round RL4b).
    pub fn repeat_refused(&self, song: RoomSongId) -> Option<Repeats> {
        if self.reveal().is_some() {
            return None;
        }
        let (rule, excluded) = self.state().excluded_songs();
        excluded.contains(&song).then_some(rule)
    }

    /// How many have chosen, of those who may.
    pub fn chose_count(&self) -> (usize, usize) {
        let s = self.state();
        let choosers = s.choosers();
        let chose = s.choices.len();
        (chose, choosers.len().max(chose))
    }

    /// The draw's countdown while it runs: it, and the seconds left as
    /// this device counts them (the room's clock where known).
    pub fn draw_countdown(&self) -> Option<(&Draw, u64)> {
        let s = self.state();
        let d = s.draw.as_ref()?;
        let end = s.draw_ends_at()?;
        let left_ms = match (self.room_now(), self.choosing.seen) {
            (Some(now), _) => end.0.saturating_sub(now.0),
            (None, Some((opened, here))) if opened == d.opened_at => {
                (here + end.0.saturating_sub(d.opened_at.0)).saturating_sub(self.now_ms())
            }
            _ => end.0.saturating_sub(d.opened_at.0),
        };
        Some((d, left_ms.div_ceil(1000)))
    }

    /// Whether anyone in the room may start the draw by their own choice
    /// (round RL4b): a host or co-host there who may choose. None where
    /// every one of them watches or is away: then no countdown can open.
    pub fn someone_starts_draw(&self) -> bool {
        let s = self.state();
        s.members.values().any(|m| {
            m.present_person()
                && m.connection.holds_place()
                && ddi_room::can(m.role, m.admin, ddi_room::Capability::StartDraw, s.phase.kind())
                && s.chooser_refusal(m.id).is_none()
        })
    }

    /// Why a host's or co-host's standing choice opens no countdown now,
    /// if it is so (too few players for a stage, rules to review).
    pub fn draw_waits(&self) -> Option<Rejection> {
        let s = self.state();
        if s.draw.is_some() || s.pick_stage().is_none() || !s.group_choice() {
            return None;
        }
        let by = s.draw_starter()?;
        s.draw_refusal(by, ddi_room::Millis(self.now_ms()))
    }

    /// The draw being revealed on this device now, if any.
    pub fn reveal(&self) -> Option<&Reveal> {
        self.choosing
            .reveal
            .as_ref()
            .filter(|r| self.now_ms() < r.ends_ms())
    }

    /// Whether stage `stage`'s prompt waits for its draw's reveal here.
    pub(super) fn revealing(&self, stage: StageId) -> bool {
        self.reveal().is_some_and(|r| r.record.stage == stage)
    }

    /// Run with the client's tick: a countdown first seen noted; a draw
    /// first seen for a stage not begun revealed from now (a draw seen
    /// once its stage began, or before this device knew the room, is
    /// passed over).
    pub(super) fn sync_choosing(&mut self) {
        let now = self.now_ms();
        let (opened, drawn) = {
            let s = self.state();
            let drawn = s.drawn.as_ref().map(|r| {
                let fresh = s
                    .stage(r.stage)
                    .is_some_and(|st| st.play.is_none() && st.content.as_ref().is_some_and(|c| c.song == r.won().map(|w| w.song)));
                (r.clone(), fresh)
            });
            (s.draw.as_ref().map(|d| d.opened_at), drawn)
        };
        match opened {
            Some(at) if self.choosing.seen.is_none_or(|(o, _)| o != at) => {
                self.choosing.seen = Some((at, now));
                self.version += 1;
            }
            None if self.choosing.seen.is_some() => {
                self.choosing.seen = None;
                self.version += 1;
            }
            _ => {}
        }
        // A draw past its reveal on the room's clock (a page loaded again,
        // or joined, during the stage's Ready window) is passed over, as the
        // audience's seat passes it over (review 2026-11-22, room-client
        // note); a room played live is read once its clock is known.
        let room_now = self.room_now();
        let clock_known = room_now.is_some() || !self.plays_live();
        if let Some((record, fresh)) = drawn.filter(|_| clock_known) {
            let fresh = fresh
                && room_now.is_none_or(|n| n.0 < record.at.0.saturating_add(ddi_room::DRAW_V0.reveal_ms));
            let key = (record.stage, record.roll);
            if self.choosing.revealed.insert(key) && fresh {
                self.choosing.reveal = Some(Reveal { record, from_ms: now });
                self.version += 1;
            }
        }
        if self.choosing.reveal.as_ref().is_some_and(|r| now >= r.ends_ms()) {
            self.choosing.reveal = None;
            self.version += 1;
        }
    }
}
