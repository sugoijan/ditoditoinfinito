//! Each player's own starting chart under Charts: Adaptive on this device
//! (room model revision 23, round OC, `ddi_room::RoomState::start_of`):
//! the pick or the draw names the song, and this player starts on their
//! own chart of it. Chosen nothing, it is the chart song select would give
//! them (their usual difficulty, `screens::data::closest`), never a harder
//! one because the picker's was; a chart chosen on the room's song list
//! for a song is remembered for it for the room's session, and one chosen
//! on the stage's Ready card too. This device tells the room as soon as
//! the stage's song is known here, again whenever what it would start on
//! changes, until the stage's start locks it.
//!
//! The charts offered are the song's ladder as its listing says it (its
//! original charts in the stage chart's style, the stage's and those
//! easier or harder by slot: `ddi_engine::ladder::is_easier`,
//! `is_harder`). The play's own ladder is worked out from the chart
//! itself once read (a chart of a timing of its own, or one the CPUs
//! cannot plan, is no rung); a start that is not on it plays the rung
//! nearest it (`ddi_session::battle::start_rung`), the same on every
//! device.

use super::{Refusal, RoomClient, song_key};
use crate::screens::data::{ChartEntry, SongEntry};
use ddi_chart::Difficulty;
use ddi_room::{ChartKey, Command, MemberKind, ParticipantId, Phase, StageId};
use std::collections::BTreeMap;

/// How this device's player starts where they chose nothing for the
/// song: a person as song select would give them; a room bot
/// (DEVELOPMENT TOOL, `ddi --mp-bots --start`) as a CPU of its level, on
/// the easiest chart or on the hardest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartRule {
    /// Their usual difficulty ([`RoomClient::set_preferred`]).
    #[default]
    Preferred,
    /// As a CPU of this level (an index of the ten) starts.
    Level(u8),
    Easiest,
    Hardest,
}

/// What this device keeps of its own starting charts.
#[derive(Clone, Debug)]
pub struct Starts {
    /// How it starts where nothing was chosen.
    pub(super) rule: StartRule,
    /// The chart chosen for each song (by its content, [`song_key`]) for
    /// the room's session: on the song list's song side or on a stage's
    /// Ready card.
    pub(super) by_song: BTreeMap<String, ChartKey>,
    /// The difficulty this player usually plays (song select's, kept as
    /// StepMania keeps it: the last one picked; Hard where none).
    pub(super) preferred: Difficulty,
    /// The last start told the room: the stage, its readiness and the
    /// chart (none: the stage's own), so a refusal is not asked again.
    pub(super) sent: Option<(StageId, u32, Option<ChartKey>)>,
}

impl Default for Starts {
    fn default() -> Self {
        Starts {
            rule: StartRule::Preferred,
            by_song: BTreeMap::new(),
            preferred: Difficulty::Hard,
            sent: None,
        }
    }
}

/// The charts a stage on `stage` of `song` may start on, as the song's
/// listing has them, the easiest first: its original charts in the stage
/// chart's style that are the stage's or easier or harder by slot (its
/// ladder as the song list can tell it).
pub fn start_line<'a>(song: &'a SongEntry, stage: &ChartEntry) -> Vec<&'a ChartEntry> {
    let key = (stage.difficulty, stage.meter);
    let mut line: Vec<&ChartEntry> = song
        .charts
        .iter()
        .filter(|c| {
            c.style == stage.style
                && c.version.is_none()
                && (c.index == stage.index
                    || ddi_engine::ladder::is_easier(key, (c.difficulty, c.meter))
                    || ddi_engine::ladder::is_harder(key, (c.difficulty, c.meter)))
        })
        .collect();
    line.sort_by_key(|c| (c.difficulty as u8, c.meter, c.index));
    line
}

/// A chart's key as the room names it (its digest).
fn key_of(chart: &ChartEntry) -> Option<ChartKey> {
    chart.digest.map(|d| ChartKey::new(d.to_string()))
}

impl RoomClient {
    /// The difficulty this player usually plays (song select's preferred
    /// one): what their start is where they chose none.
    pub fn set_preferred(&mut self, difficulty: Difficulty) {
        self.starts.preferred = difficulty;
    }

    /// How this player starts where they chose nothing ([`StartRule`]: a
    /// room bot's flag).
    pub fn set_start_rule(&mut self, rule: StartRule) {
        self.starts.rule = rule;
    }

    /// This player's chart for `song` (the room's song list's song side
    /// under Adaptive): remembered for the room's session, and their start
    /// when the song is played.
    pub fn remember_start(&mut self, song: &SongEntry, chart: &ChartEntry) {
        let (Some(song), Some(chart)) = (song_key(song), key_of(chart)) else {
            return;
        };
        // This device's own memory: nothing of the room changes (no
        // version: the screens are not built again for it).
        self.starts.by_song.insert(song, chart);
    }

    /// The chart remembered for `song`, if one was chosen this session.
    pub fn remembered_start(&self, song: &SongEntry) -> Option<&ChartKey> {
        self.starts.by_song.get(&song_key(song)?)
    }

    /// The stage's charts are each player's own (its Charts rule is
    /// Adaptive, where this room plays it).
    pub fn own_starts(&self, stage: StageId) -> bool {
        self.adaptive_offered() && self.state().charts_for(stage) == ddi_room::Charts::Adaptive
    }

    /// The next stage's song is chosen without its chart (Adaptive): the
    /// song list, the group's choices and the reveal name the song alone.
    pub fn songs_without_charts(&self) -> bool {
        let s = self.state();
        let charts = match s.choosing_for().or_else(|| s.upcoming_stage()) {
            Some(stage) => s.charts_for(stage),
            None => s.next.rules.charts,
        };
        self.adaptive_offered() && charts == ddi_room::Charts::Adaptive
    }

    /// The charts `stage` may be started on (its song as this device has
    /// it: [`start_line`]), the easiest first; empty where its song is not
    /// known here or the stage is played as picked.
    pub fn start_charts(&self, stage: StageId) -> Vec<ChartEntry> {
        if !self.own_starts(stage) {
            return Vec::new();
        }
        let Some(content) = self.state().stage(stage).and_then(|s| s.content.as_ref()) else {
            return Vec::new();
        };
        let Some(picked) = self.picked(&content.id) else {
            return Vec::new();
        };
        start_line(&picked.song, &picked.chart).into_iter().cloned().collect()
    }

    /// The chart this player would start `stage` on as this device works
    /// it out (what it tells the room): the one chosen on its Ready card or
    /// for its song on the list, where it is one of its charts; else the
    /// one song select would give them (closest to their usual
    /// difficulty, the easier of two as close).
    fn wanted_start(&self, stage: StageId) -> Option<ChartEntry> {
        let charts = self.start_charts(stage);
        if charts.is_empty() {
            return None;
        }
        let content = self.state().stage(stage)?.content.as_ref()?;
        let picked = self.picked(&content.id)?;
        let remembered = self.remembered_start(&picked.song);
        if let Some(c) = remembered.and_then(|k| charts.iter().find(|c| key_of(c).as_ref() == Some(k))) {
            return Some(c.clone());
        }
        let i = match self.starts.rule {
            StartRule::Preferred => {
                let refs: Vec<&ChartEntry> = charts.iter().collect();
                crate::screens::data::closest(&refs, self.starts.preferred)
            }
            StartRule::Level(level) => {
                let line: Vec<(Difficulty, u32)> = charts.iter().map(|c| (c.difficulty, c.meter)).collect();
                ddi_engine::ladder::cpu_start_in(&line, usize::from(level))
            }
            StartRule::Easiest => Some(0),
            StartRule::Hardest => Some(charts.len() - 1),
        };
        i.map(|i| charts[i].clone())
    }

    /// The chart `p` starts `stage` on as this device can show it: theirs
    /// the room has, else the stage's own; a CPU's by its level
    /// (`ddi_engine::ladder::cpu_start_in`, `cpu-start-v0`); this
    /// player's as this device would start it until the room has it. None
    /// where the stage is played as picked or its song is not known here.
    pub fn start_shown(&self, stage: StageId, p: ParticipantId) -> Option<ChartEntry> {
        let charts = self.start_charts(stage);
        if charts.is_empty() {
            return None;
        }
        let s = self.state();
        let content = s.stage(stage)?.content.as_ref()?;
        let own_chart = |k: &ChartKey| charts.iter().find(|c| key_of(c).as_ref() == Some(k)).cloned();
        if let Some(MemberKind::Cpu { opponent, .. }) = s.member(p).map(|m| &m.kind) {
            let line: Vec<(Difficulty, u32)> = charts.iter().map(|c| (c.difficulty, c.meter)).collect();
            let level = usize::from(opponent.level);
            return ddi_engine::ladder::cpu_start_in(&line, level).map(|i| charts[i].clone());
        }
        if let Some(k) = s.start_of(stage, p) {
            return own_chart(k);
        }
        if p == self.me && s.stage(stage).and_then(|st| st.play.as_ref()).is_none() {
            return self.wanted_start(stage);
        }
        own_chart(&content.id.chart)
    }

    /// This player's own chart for `stage`, chosen on its Ready card (the
    /// chart's index in its song): remembered for its song, told the room
    /// at once. Refused where the stage is played as picked or its start
    /// is locked.
    pub fn choose_start(&mut self, stage: StageId, chart: &ChartEntry) -> Result<(), Refusal> {
        if !self.own_starts(stage) {
            return self.refuse(Refusal::Room(ddi_room::Rejection::ChartsAsPicked));
        }
        let Some(content) = self.state().stage(stage).and_then(|s| s.content.as_ref()) else {
            return Ok(());
        };
        let Some(picked) = self.picked(&content.id).cloned() else {
            return Ok(());
        };
        self.remember_start(&picked.song, chart);
        self.sync_start();
        Ok(())
    }

    /// Stage `order`'s own start as its play starts: the room's (locked
    /// at its start; a late entrant's as told), where the stage is this
    /// room's and has started, else the order's (an order is taken as its
    /// prompt opens, before the start is told or chosen on its Ready
    /// card).
    pub fn start_for(&self, order: &super::StageOrder) -> Option<ChartKey> {
        if self.me == order.me && self.stage_started(order.stage) {
            self.state().start_of(order.stage, self.me).cloned()
        } else {
            order.start.clone()
        }
    }

    /// The stage this player's start is told for now: the one gathering
    /// readiness with them in it, or one they may still enter late.
    fn start_stage(&self) -> Option<StageId> {
        let s = self.state();
        let stage = match s.phase {
            Phase::Preparing { stage, .. } | Phase::Intermission { next: stage } => stage,
            Phase::Countdown { stage, .. } | Phase::Stage { stage }
                if self.may_enter_late(stage) =>
            {
                stage
            }
            Phase::Lobby => s.playlist.first()?.id,
            _ => return None,
        };
        (s.stage(stage)?.content.is_some() && self.in_stage(stage)).then_some(stage)
    }

    /// Tells the room this player's start where it is not the one this
    /// device would start them on (once a stage's song is known here, and
    /// whenever their choice changes), until the start locks it. Said
    /// nowhere: a refusal (a start locked meanwhile) is not asked again.
    pub(super) fn sync_start(&mut self) {
        let Some(stage) = self.start_stage() else {
            return;
        };
        let Some(wanted) = self.wanted_start(stage) else {
            return;
        };

        let s = self.state();
        let Some(st) = s.stage(stage) else {
            return;
        };
        let content = st.content.as_ref().map(|c| c.id.chart.clone());
        let chart = key_of(&wanted).filter(|k| Some(k) != content.as_ref());
        let rev = st.readiness_rev;
        if st.prep_ref(self.me).start == chart {
            return;
        }
        let ask = (stage, rev, chart.clone());
        if self.starts.sent.as_ref() == Some(&ask) {
            return;
        }
        self.starts.sent = Some(ask);
        let kept = self.refusal.clone();
        let _ = self.send(Command::SetStart {
            stage,
            readiness_rev: rev,
            chart,
        });
        self.refusal = kept;
    }
}
