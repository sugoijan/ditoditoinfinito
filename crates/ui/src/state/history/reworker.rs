//! When stale plays are worked out again (`docs/plans/play-history.md`,
//! Versions and re-working), for both shells, sans IO: a song's whose
//! chart is at hand first, a few at a time; then the rest in the
//! background from their charts alone, a song at a time; never during a
//! play; until nothing is stale. The shell runs what a [`ReworkStep`] asks
//! (reads a chart, judges a batch) on its own timers and threads, and
//! answers back.
//!
//! The shell's loop: [`Reworker::step`] when the time it was last told
//! comes; [`ReworkStep::Read`] answered with [`Reworker::read`],
//! [`ReworkStep::Rework`] with [`Reworker::worked`] (the next step
//! `next_ms` later); [`ReworkStep::Wait`] and [`ReworkStep::Done`] schedule nothing:
//! [`Reworker::set_playing`] and [`Reworker::chart_loaded`] say when to
//! step again.

use std::collections::HashSet;

use super::{ChartDigest, Plays};

/// Plays worked out again in one step (one replay each: a long chart's
/// takes about a quarter of a millisecond).
pub const REWORK_BATCH: usize = 8;
/// Time between steps for a song whose chart is at hand, ms (a yield).
pub const FOCUS_STEP_MS: u32 = 16;
/// Time between the background pass's steps, ms; also the first step's
/// wait once the summaries are in.
pub const PASS_STEP_MS: u32 = 1500;

/// What the shell does next.
#[derive(Debug)]
pub enum ReworkStep<S> {
    /// Nothing now (a play is on, a step is under way, or the summaries
    /// are not in): nothing is scheduled.
    Wait,
    /// Nothing left to work out this session: nothing is scheduled.
    Done,
    /// Read song `song`'s chart alone (no music, no art); where that song
    /// is gone (a pack imported again from another folder), a song's with
    /// a chart of one of `digests`. Then [`Reworker::read`].
    Read {
        song: String,
        digests: Vec<ChartDigest>,
    },
    /// Work out up to [`REWORK_BATCH`] of song `song`'s stale plays on
    /// `chart` (`Plays::rework_song`, or `Plays::stale_batch`,
    /// `rework_entries` and `Plays::set_worked`). Then
    /// [`Reworker::worked`], and the next step `next_ms` later.
    Rework {
        song: String,
        chart: S,
        next_ms: u32,
    },
}

/// Which stale plays are worked out when. `S` is the shell's handle on a
/// song's chart as read (`Rc<Song>`, `Arc<Song>`).
pub struct Reworker<S> {
    /// A song is being played: re-working waits.
    playing: bool,
    /// A step is under way (a chart read, a batch judged).
    busy: bool,
    /// The song whose chart is at hand: its stale plays come first.
    focus: Option<(String, S)>,
    /// The background pass's song and chart, read once for its steps.
    background: Option<(String, S)>,
    /// Songs whose chart could not be read this session (a kept folder
    /// asking for permission, a removed song): the pass leaves them.
    skipped: HashSet<String>,
}

impl<S> Default for Reworker<S> {
    fn default() -> Self {
        Reworker {
            playing: false,
            busy: false,
            focus: None,
            background: None,
            skipped: HashSet::new(),
        }
    }
}

impl<S: Clone> Reworker<S> {
    /// A song's play is on (re-working waits) or over (it goes on): when
    /// to step again, in ms.
    pub fn set_playing(&mut self, on: bool) -> Option<u32> {
        self.playing = on;
        (!on).then_some(FOCUS_STEP_MS)
    }

    /// The song's chart is no longer at hand (its screen left): its plays
    /// go on in the background.
    pub fn clear_focus(&mut self) {
        self.focus = None;
    }

    /// Song `id`'s chart is at hand: its stale plays come first, from
    /// `chart` (made only when it has some). When to step again, in ms.
    pub fn chart_loaded(
        &mut self,
        plays: &Plays,
        id: &str,
        chart: impl FnOnce() -> S,
    ) -> Option<u32> {
        if !plays.history().has_stale(id) {
            return None;
        }
        self.focus = Some((id.to_string(), chart()));
        Some(0)
    }

    /// Songs whose chart could not be read are tried again (the library
    /// changed).
    pub fn retry_skipped(&mut self) {
        self.skipped.clear();
    }

    /// The next step: the focused song's plays, else a song's in the
    /// background (its chart read first).
    pub fn step(&mut self, plays: &Plays) -> ReworkStep<S> {
        if self.playing || self.busy || !plays.loaded() {
            return ReworkStep::Wait;
        }
        let stale = plays.stale_songs();
        if let Some((id, chart)) = self.focus.clone() {
            if stale.contains(&id) {
                self.busy = true;
                return ReworkStep::Rework {
                    song: id,
                    chart,
                    next_ms: FOCUS_STEP_MS,
                };
            }
            self.focus = None;
        }
        let Some(id) = stale.into_iter().find(|s| !self.skipped.contains(s)) else {
            self.background = None;
            return ReworkStep::Done;
        };
        if let Some((_, chart)) = self.background.as_ref().filter(|(i, _)| *i == id) {
            self.busy = true;
            return ReworkStep::Rework {
                song: id,
                chart: chart.clone(),
                next_ms: PASS_STEP_MS,
            };
        }
        // Where the song is gone, its charts are looked for by content.
        let digests = {
            let h = plays.history();
            h.stale_of(&id)
                .iter()
                .filter_map(|p| h.get(p).map(|s| s.chart.digest))
                .collect()
        };
        self.busy = true;
        ReworkStep::Read { song: id, digests }
    }

    /// [`ReworkStep::Read`]'s chart, or why not (the song is left for the
    /// session). When to step again, in ms (a play may have started
    /// meanwhile: the step looks again).
    pub fn read<E>(&mut self, song: String, chart: Result<S, E>) -> u32 {
        self.busy = false;
        match chart {
            Ok(chart) => {
                self.background = Some((song, chart));
                0
            }
            Err(_) => {
                self.skipped.insert(song);
                PASS_STEP_MS
            }
        }
    }

    /// [`ReworkStep::Rework`] is done (worked out, or failed: the next step
    /// tries again); the next step comes its `next_ms` later.
    pub fn worked(&mut self) {
        self.busy = false;
    }
}
