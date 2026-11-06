// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Optional hand-input preparation proposals; ordinary playback never skips notes.
use crate::{Input, PlanningReport, Status};
use ddi_chart::{NoteKind, TICKS_PER_BEAT};
use serde::{Deserialize, Serialize};

/// One tap proposal, with its three-tile burst for an auditable reduction recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapReduction {
    pub tick: i64,
    pub lane: u8,
    pub burst_start: i64,
    pub burst_end: i64,
}

/// A narrow eligibility gate after ordinary and recovery contact planning fail.
/// Hosts must validate reduced hand input before committing its contacts. This
/// is not enabled by RollingPlanner or Player and must never modify the play's
/// judged chart or the hand's published contacts.
#[derive(Clone, Copy, Debug)]
pub struct BurstFallback {
    window_ms: u8,
}
impl Default for BurstFallback {
    fn default() -> Self {
        Self { window_ms: 60 }
    }
}
impl BurstFallback {
    pub fn new(window_ms: u8) -> Result<Self, String> {
        if !(1..=90).contains(&window_ms) {
            return Err("burst eligibility must be within 1–90 ms".into());
        }
        Ok(Self { window_ms })
    }
    /// At most nine proposals, all from tight three-tap sequences containing
    /// the last failed row. Existing successful plans never become candidates.
    /// `report` must be the completed planning result for this exact `input`.
    pub fn candidates(
        &self,
        input: &Input,
        report: &PlanningReport,
        mutable_after: f64,
    ) -> Vec<TapReduction> {
        if report.status != Status::ContactFailure || mutable_after.is_nan() {
            return Vec::new();
        }
        let Some(failed) = report.failure_tick else {
            return Vec::new();
        };
        let raw_taps = input
            .effective_notes
            .iter()
            .filter(|n| n.kind == NoteKind::Tap)
            .map(|n| (n.tick.0, n.lane))
            .collect::<std::collections::BTreeSet<_>>();
        let taps = input
            .notes
            .iter()
            .filter(|n| {
                n.judgeable
                    && !n.mine
                    && !n.lift
                    && !n.roll
                    && n.end.is_none()
                    && raw_taps.contains(&(n.tick, n.lane))
            })
            .collect::<Vec<_>>();
        let mut choices = Vec::new();
        for burst in taps.windows(3) {
            if !burst.iter().any(|n| n.tick == failed)
                || burst[0].lane == burst[1].lane
                || burst[0].lane == burst[2].lane
                || burst[1].lane == burst[2].lane
                || burst[2].time - burst[0].time < 0.
                || burst[2].time - burst[0].time > f64::from(self.window_ms) / 1000. + 1e-12
            {
                continue;
            }
            for tap in burst.iter().filter(|n| n.time > mutable_after) {
                if !choices
                    .iter()
                    .any(|c: &TapReduction| c.tick == tap.tick && c.lane == tap.lane)
                {
                    choices.push(TapReduction {
                        tick: tap.tick,
                        lane: tap.lane,
                        burst_start: burst[0].tick,
                        burst_end: burst[2].tick,
                    });
                }
            }
        }
        // Prefer removing weak subdivisions to strong rhythmic anchors. This
        // ordering is independent of arrow direction; feasibility is decided
        // by replanning and actual-judge validation, not by this preference.
        choices.sort_by_key(|c| {
            let strength = if c.tick % TICKS_PER_BEAT == 0 {
                3
            } else if c.tick % (TICKS_PER_BEAT / 2) == 0 {
                2
            } else if c.tick % (TICKS_PER_BEAT / 4) == 0 {
                1
            } else {
                0
            };
            (strength, c.tick, c.lane)
        });
        choices.truncate(9);
        choices
    }
    /// Apply exactly one eligible tap removal to an uncommitted planning copy.
    /// Both note lists describe that copy; the source and gameplay are untouched.
    pub fn reduced_input(
        &self,
        input: &Input,
        report: &PlanningReport,
        mutable_after: f64,
        choice: TapReduction,
    ) -> Result<Input, String> {
        if !self
            .candidates(input, report, mutable_after)
            .contains(&choice)
        {
            return Err("tap is not an eligible uncommitted burst reduction".into());
        }
        let mut reduced = input.clone();
        reduced
            .notes
            .retain(|n| n.tick != choice.tick || n.lane != choice.lane);
        reduced
            .effective_notes
            .retain(|n| n.tick.0 != choice.tick || n.lane != choice.lane);
        if reduced.notes.len() + 1 != input.notes.len()
            || reduced.effective_notes.len() + 1 != input.effective_notes.len()
        {
            return Err("burst reduction must identify exactly one original tap".into());
        }
        Ok(reduced)
    }
}
