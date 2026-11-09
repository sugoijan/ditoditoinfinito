// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Bounded actual-judge checkpoints for rolling contact publication.
use crate::replay::{buttons_at, edges_since, observe, validate_events};
use crate::*;
use ddi_engine::{
    judge::{Judge, NoteState},
    rules::{JudgeTable, Judgement},
};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(crate) struct Checkpoint {
    pub time: f64,
    judges: Vec<Judge>,
    ids: Vec<String>,
    totals: Vec<JudgeReport>,
    known_tick: Option<i64>,
    heads: Vec<usize>,
    holds: usize,
    mines: usize,
    shocks: usize,
    avoided: Vec<usize>,
}
fn empty(window: f64) -> JudgeReport {
    JudgeReport {
        passed: false,
        expected_shocks: 0,
        avoided_shocks: 0,
        expected_mines: 0,
        avoided_mines: 0,
        hits: 0,
        holds: 0,
        expected_hits: 0,
        expected_holds: 0,
        maximum_hit_delta_seconds: 0.,
        bad: Vec::new(),
        unresolved: 0,
        edges: 0,
        hold_window_seconds: window,
    }
}
impl Checkpoint {
    #[cfg(test)]
    pub fn initial(timing: &TimingMap) -> Self {
        Self::configured(timing, 4, &standard_rules())
    }
    pub fn configured(timing: &TimingMap, lanes: usize, rules: &[(String, JudgeTable)]) -> Self {
        let totals = rules
            .iter()
            .map(|(_, t)| empty(t.hold_window_seconds()))
            .collect();
        let judges = rules
            .iter()
            .map(|(_, t)| Judge::new(&[], timing, t.clone()).with_lanes(lanes))
            .collect();
        Self {
            time: -1e-9,
            judges,
            ids: rules.iter().map(|(id, _)| id.clone()).collect(),
            totals,
            known_tick: None,
            heads: vec![0; rules.len()],
            holds: 0,
            mines: 0,
            shocks: 0,
            avoided: vec![0; rules.len()],
        }
    }
    pub fn retained(&self) -> usize {
        self.judges.iter().map(|j| j.notes().len()).sum()
    }
}

pub(crate) struct WindowReplay {
    next: Checkpoint,
    active: Option<Judge>,
    totals: Option<JudgeReport>,
    edges: Vec<(f64, u8, bool)>,
    rule: usize,
    at: usize,
    until: f64,
    certified: f64,
    captured: bool,
    report: ReplayReport,
}
impl WindowReplay {
    pub fn new(
        input: &Input,
        plan: &ContactPlan,
        checkpoint: &Checkpoint,
        until: f64,
        certified: f64,
    ) -> Result<Self, String> {
        if certified < checkpoint.time || certified > until || !until.is_finite() {
            return Err("invalid rolling judge horizon".into());
        }
        validate_events(input, plan, checkpoint.time, until)?;
        let buttons = buttons_at(plan, checkpoint.time);
        if checkpoint
            .judges
            .iter()
            .any(|j| j.buttons_down() != buttons)
        {
            return Err("candidate changed physical buttons at the committed checkpoint".into());
        }
        let incoming = input
            .effective_notes
            .iter()
            .filter(|n| {
                checkpoint.known_tick.is_none_or(|t| n.tick.0 > t)
                    && input.timing.judgeable(n.tick)
                    && n.kind.hand_supported()
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut next = checkpoint.clone();
        for (i, judge) in next.judges.iter_mut().enumerate() {
            next.avoided[i] += judge
                .notes()
                .iter()
                .filter(|n| n.note.kind == NoteKind::Mine && n.state == NoteState::MineAvoided)
                .count();
            next.heads[i] += Judge::new(&incoming, &input.timing, judge.table().clone())
                .judged_note_count() as usize;
            judge.append_future_hand_contacts(&incoming, &input.timing)?;
        }
        next.shocks += incoming
            .iter()
            .filter(|n| n.kind == NoteKind::Shock)
            .map(|n| n.tick)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        for n in input
            .notes
            .iter()
            .filter(|n| n.judgeable && checkpoint.known_tick.is_none_or(|t| n.tick > t))
        {
            next.mines += usize::from(n.mine && !n.shock);
            next.holds += usize::from(n.end.is_some());
        }
        next.known_tick = input
            .effective_notes
            .last()
            .map(|n| n.tick.0)
            .or(next.known_tick);
        let edges = edges_since(plan, checkpoint.time)?
            .into_iter()
            .filter(|(t, _, _)| *t <= until)
            .collect();
        Ok(Self {
            next,
            active: None,
            totals: None,
            edges,
            rule: 0,
            at: 0,
            until,
            certified,
            captured: false,
            report: ReplayReport {
                passed: true,
                rules: BTreeMap::new(),
            },
        })
    }
    pub fn step(&mut self) -> bool {
        if self.rule == self.next.judges.len() {
            return true;
        }
        if self.active.is_none() {
            self.active = Some(self.next.judges[self.rule].clone());
            self.totals = Some(self.next.totals[self.rule].clone());
            self.at = 0;
            self.captured = false;
            return false;
        }
        let judge = self.active.as_mut().unwrap();
        let totals = self.totals.as_mut().unwrap();
        let next_edge = self.edges.get(self.at).copied();
        if !self.captured && next_edge.is_none_or(|(t, _, _)| t > self.certified) {
            observe(totals, judge.update(self.certified));
            self.next.judges[self.rule] = judge.clone();
            self.next.totals[self.rule] = totals.clone();
            self.captured = true;
            return false;
        }
        if let Some((time, lane, down)) = next_edge {
            observe(
                totals,
                if down {
                    judge.press(lane, time)
                } else {
                    judge.release(lane, time)
                },
            );
            self.at += 1;
            return false;
        }
        // Hold tails outside the known watermark remain active obligations.
        let due = self.next.totals[self.rule].holds
            + self.next.judges[self.rule]
                .notes()
                .iter()
                .filter(|n| {
                    n.judgeable
                        && !n.state.is_terminal()
                        && n.note.kind.hold_end().is_some()
                        && n.end_seconds < self.until
                })
                .count();
        observe(totals, judge.update(self.until));
        totals.expected_hits = self.next.heads[self.rule];
        totals.expected_mines = self.next.mines;
        // A received hazard at the watermark is not due until its late window
        // has elapsed. Preserve it pending across the committed checkpoint.
        let pending_shocks = judge
            .notes()
            .iter()
            .filter(|n| {
                n.judgeable
                    && n.note.kind == NoteKind::Shock
                    && n.seconds + judge.table().mine_window_seconds() >= self.until
            })
            .map(|n| n.note.tick)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        totals.expected_shocks = self.next.shocks - pending_shocks;
        totals.avoided_mines = self.next.avoided[self.rule]
            + judge
                .notes()
                .iter()
                .filter(|n| n.note.kind == NoteKind::Mine && n.state == NoteState::MineAvoided)
                .count();
        totals.expected_holds = due;
        totals.edges = self.edges.len();
        totals.unresolved = judge
            .notes()
            .iter()
            .filter(|n| {
                n.judgeable
                    // Judge::update expires hazards strictly after their late
                    // boundary. At equality they remain valid pending notes.
                    && if matches!(n.note.kind, NoteKind::Mine | NoteKind::Shock) {
                        n.seconds + judge.table().mine_window_seconds() < self.until
                    } else {
                        n.seconds <= self.until
                    }
                    && (!matches!(
                        n.state,
                        NoteState::MineAvoided
                            | NoteState::Hit {
                                judgement: Judgement::W1,
                                ..
                            }
                            | NoteState::HoldActive { .. }
                            | NoteState::Held
                    ) || (n.note.kind.hold_end().is_some()
                        && n.end_seconds < self.until
                        && n.state != NoteState::Held))
            })
            .count();
        totals.passed = totals.bad.is_empty()
            && totals.hits == totals.expected_hits
            && totals.holds == due
            && totals.unresolved == 0
            && totals.avoided_shocks == totals.expected_shocks;
        self.report.passed &= totals.passed;
        self.report
            .rules
            .insert(self.next.ids[self.rule].clone(), totals.clone());
        self.active = None;
        self.totals = None;
        self.rule += 1;
        self.rule == self.next.judges.len()
    }
    pub fn finish(mut self) -> (ReplayReport, Checkpoint) {
        self.next.time = self.certified;
        (self.report, self.next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::{Note, Tick};

    fn window(
        input: &Input,
        plan: &ContactPlan,
        checkpoint: &Checkpoint,
        until: f64,
        certified: f64,
    ) -> (ReplayReport, Checkpoint) {
        let mut replay = WindowReplay::new(input, plan, checkpoint, until, certified).unwrap();
        for _ in 0..10_000 {
            if replay.step() {
                return replay.finish();
            }
        }
        panic!("window replay did not finish");
    }

    #[test]
    fn mine_at_its_exact_late_boundary_stays_pending_until_the_next_window() {
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Hard:1:0,0,0,0,0:\n1000;",
        )
        .unwrap();
        song.charts[0].notes = vec![Note::new(Tick(96), 0, NoteKind::Mine)];
        let input = Input::from_song(&song, 0).unwrap();
        let mut search = Search::new(&input, Options::default()).unwrap();
        while !search.step() {}
        let (plan, _) = search.finish().unwrap().unwrap();
        let initial = Checkpoint::initial(&input.timing);
        let deadline = input.notes[0].time
            + ddi_engine::rules::presets::ddr_a()
                .judge
                .mine_window_seconds();
        let (report, checkpoint) = window(&input, &plan, &initial, deadline, 0.5);
        assert!(report.passed, "{report:?}");
        assert_eq!(report.rules["ddr-a"].avoided_mines, 0);
        assert_eq!(report.rules["ddr-a"].unresolved, 0);
        let (settled, _) = window(&input, &plan, &checkpoint, deadline + 0.2, deadline + 0.2);
        assert!(settled.passed, "{settled:?}");
        assert!(settled.rules.values().all(|r| r.avoided_mines == 1));

        // The pending-boundary allowance must still reject an actual mine hit.
        let mut unsafe_plan = plan.clone();
        unsafe_plan.source_music_origin_seconds = 0.;
        unsafe_plan.initial.left = 4;
        let (unsafe_report, _) = window(&input, &unsafe_plan, &initial, deadline, 0.5);
        assert!(!unsafe_report.passed);
        assert!(unsafe_report.rules.values().all(|r| !r.bad.is_empty()));
    }
}
