// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Actual judge replay of aggregated contacts, including unscored restorations.
use crate::*;
use ddi_engine::judge::{Judge, JudgeEvent, JudgeEventKind, NoteState};
use ddi_engine::rules::{JudgeTable, Judgement};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize)]
pub struct JudgeReport {
    pub passed: bool,
    #[serde(skip_serializing_if = "zero")]
    pub expected_mines: usize,
    #[serde(skip_serializing_if = "zero")]
    pub avoided_mines: usize,
    #[serde(skip_serializing_if = "zero")]
    pub expected_shocks: usize,
    #[serde(skip_serializing_if = "zero")]
    pub avoided_shocks: usize,
    pub hits: usize,
    pub holds: usize,
    pub expected_hits: usize,
    pub expected_holds: usize,
    pub maximum_hit_delta_seconds: f64,
    pub bad: Vec<String>,
    pub unresolved: usize,
    pub edges: usize,
    pub hold_window_seconds: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct ReplayReport {
    pub passed: bool,
    pub rules: BTreeMap<String, JudgeReport>,
}
fn zero(n: &usize) -> bool {
    *n == 0
}
pub(crate) fn lane(layout: &str, panel: u8) -> Option<u8> {
    layout_panels(layout)
        .ok()?
        .iter()
        .position(|p| *p == panel)
        .map(|i| i as u8)
}

fn same(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}
fn landing(p: &ContactPlan, foot: Foot, panel: u8, time: f64) -> bool {
    let tr = if foot == Foot::L {
        &p.tracks.left
    } else {
        &p.tracks.right
    };
    let i = tr.partition_point(|s| s.land < time - 1e-9);
    tr.get(i)
        .is_some_and(|s| s.to == panel && same(s.land, time))
}
pub(crate) fn validate_events(
    input: &Input,
    p: &ContactPlan,
    after: f64,
    until: f64,
) -> Result<(), String> {
    if p.layout != input.layout {
        return Err("contact plan layout differs from source".into());
    }
    crate::search::rolls::validate_pulses(p)?;
    for n in input
        .notes
        .iter()
        .filter(|n| n.judgeable && n.roll && !n.roll_refresh)
    {
        if !p.holds.iter().any(|h| {
            h.roll
                && Some(n.lane) == lane(&p.layout, h.panel)
                && same(h.start + p.source_music_origin_seconds, n.time)
                && n.end.is_some_and(|(tick, t)| {
                    tick == h.end_tick && same(h.end + p.source_music_origin_seconds, t)
                })
        }) {
            return Err("original roll head/tail identity changed".into());
        }
    }
    let expected = input
        .notes
        .iter()
        .filter(|n| n.judgeable && !n.mine && n.time > after && n.time <= until)
        .collect::<Vec<_>>();
    let events = p
        .events
        .iter()
        .filter(|e| {
            let t = e.time + p.source_music_origin_seconds;
            t > after && t <= until
        })
        .collect::<Vec<_>>();
    if expected.len() != events.len() {
        return Err("chart event count differs".into());
    }
    for (n, e) in expected.iter().zip(events) {
        if n.tick != e.tick
            || Some(n.lane) != lane(&p.layout, e.panel)
            || !same(e.time + p.source_music_origin_seconds, n.time)
            || e.lift != n.lift
            || if e.lift {
                let tr = if e.foot == Foot::L {
                    &p.tracks.left
                } else {
                    &p.tracks.right
                };
                !tr.iter().any(|s| s.release == e.time && s.from == e.panel)
            } else {
                !landing(p, e.foot, e.panel, e.time)
            }
        {
            return Err("chart event identity, exact timing or physical landing changed".into());
        }
    }
    let mut hold_index: BTreeMap<(u8, i64), Vec<(f64, f64)>> = BTreeMap::new();
    for n in &input.notes {
        if n.judgeable
            && let Some((tick, end)) = n.end
        {
            hold_index
                .entry((n.lane, tick))
                .or_default()
                .push((n.time, end));
        }
    }
    for a in p.aux_contacts.iter().flatten().filter(|a| {
        let t = a.time + p.source_music_origin_seconds;
        t > after && t <= until
    }) {
        if a.kind == RestoreKind::SupportStep {
            if !crate::wide_stances(&p.layout)
                || !crate::support_tiles(&p.layout).contains(&a.panel)
                || !input.notes.iter().any(|n| {
                    n.judgeable
                        && !n.mine
                        && !n.lift
                        && !n.roll_refresh
                        && n.tick == a.end_tick
                        && same(n.time, a.time + p.source_music_origin_seconds)
                })
                || !p
                    .events
                    .iter()
                    .any(|e| e.tick == a.end_tick && e.foot != a.foot && same(e.time, a.time))
                || !landing(p, a.foot, a.panel, a.time)
            {
                return Err("support step has no matching chart head or unscored landing".into());
            }
            continue;
        }
        if a.kind == RestoreKind::CompanionHop {
            if !input.notes.iter().any(|n| {
                n.judgeable
                    && !n.mine
                    && !n.lift
                    && n.tick == a.end_tick
                    && (if a.foot == Foot::L {
                        &p.tracks.left
                    } else {
                        &p.tracks.right
                    })
                    .iter()
                    .any(|s| {
                        s.land == a.time
                            && s.to == a.panel
                            && lane(&p.layout, s.from) == Some(n.lane)
                    })
                    && (same(n.time + 0.060, a.time + p.source_music_origin_seconds)
                        || (p.layout == "danoni-9B"
                            && a.time + p.source_music_origin_seconds >= n.time + 0.060
                            && a.time + p.source_music_origin_seconds <= n.time + 0.4
                            && input.notes.iter().any(|m| {
                                m.judgeable
                                    && m.mine
                                    && m.panel == a.panel
                                    && same(m.time + 0.170, a.time + p.source_music_origin_seconds)
                            })))
            }) || !landing(p, a.foot, a.panel, a.time)
            {
                return Err("companion hop has no matching source head".into());
            }
            continue;
        }
        if a.kind == RestoreKind::MineAvoid {
            crate::search::mines::validate_auxiliary(input, p, a)?;
            continue;
        }
        if a.kind == RestoreKind::RollRefresh {
            crate::search::rolls::validate_auxiliary(input, p, a)?;
            continue;
        }
        if matches!(a.kind, RestoreKind::LiftPlant | RestoreKind::LiftClear) {
            crate::search::lifts::validate_auxiliary(input, p, a)?;
            continue;
        }
        let l = lane(&p.layout, a.panel).ok_or("restore lane")?;
        let time = a.time + p.source_music_origin_seconds;
        if !hold_index
            .get(&(l, a.end_tick))
            .is_some_and(|h| h.iter().any(|&(start, end)| start < time && time < end))
            || !landing(p, a.foot, a.panel, a.time)
            || p.events
                .iter()
                .any(|e| e.foot == a.foot && same(e.time, a.time))
        {
            return Err(
                "restore does not target an original active hold or physical landing".into(),
            );
        }
    }
    Ok(())
}
fn trace(input: &Input, p: &ContactPlan) -> Result<Vec<(f64, u8, bool)>, String> {
    validate_events(input, p, f64::NEG_INFINITY, f64::INFINITY)?;
    edges_since(p, f64::NEG_INFINITY)
}
pub(crate) fn buttons_at(p: &ContactPlan, time: f64) -> Vec<bool> {
    let mut down = vec![false; layout_lanes(&p.layout).unwrap()];
    if time < p.source_music_origin_seconds {
        return down;
    }
    for (initial, track) in [
        (p.initial.left, &p.tracks.left),
        (p.initial.right, &p.tracks.right),
    ] {
        let mut at = Some(initial);
        for s in track {
            if s.release + p.source_music_origin_seconds > time {
                break;
            }
            at = if s.land + p.source_music_origin_seconds <= time {
                Some(s.to)
            } else {
                None
            };
        }
        if let Some(l) = at
            .and_then(|tile| lane(&p.layout, tile))
            .filter(|l| (*l as usize) < down.len())
        {
            down[l as usize] = true;
        }
    }
    down
}
pub(crate) fn edges_since(p: &ContactPlan, after: f64) -> Result<Vec<(f64, u8, bool)>, String> {
    let mut contacts = [Some(p.initial.left), Some(p.initial.right)];
    let mut changes = Vec::new();
    for (d, track) in [&p.tracks.left, &p.tracks.right].iter().enumerate() {
        let mut at = contacts[d].unwrap();
        let mut last = 0.;
        // Index scored/auxiliary lands once, preserving exact physical edges.
        let mut lands = p
            .events
            .iter()
            .filter(|e| e.foot.index() == d && !e.lift)
            .map(|e| (e.time, e.panel))
            .chain(
                p.aux_contacts
                    .iter()
                    .flatten()
                    .filter(|a| a.foot.index() == d)
                    .map(|a| (a.time, a.panel)),
            )
            .collect::<Vec<_>>();
        lands.sort_by(|a, b| a.0.total_cmp(&b.0));
        for s in *track {
            if s.from != at
                || s.release < last
                || s.release >= s.land
                || s.land > p.duration
                || ![s.release, s.land].iter().all(|x| x.is_finite())
            {
                return Err("invalid swing ordering".into());
            }
            let k = lands.partition_point(|(t, _)| *t < s.land - 1e-9);
            if s.land + p.source_music_origin_seconds > after
                && !lands
                    .get(k)
                    .is_some_and(|(t, panel)| same(*t, s.land) && *panel == s.to)
            {
                return Err("landing has no matching chart event".into());
            }
            changes.push((p.source_music_origin_seconds + s.release, d, None));
            changes.push((p.source_music_origin_seconds + s.land, d, Some(s.to)));
            last = s.land;
            at = s.to;
        }
    }
    changes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let counts = |c: [Option<u8>; 2]| {
        let mut n = vec![0; layout_lanes(&p.layout).unwrap()];
        for panel in c.into_iter().flatten() {
            if let Some(l) = lane(&p.layout, panel).filter(|l| (*l as usize) < n.len()) {
                n[l as usize] += 1;
            }
        }
        n
    };
    let mut edges = Vec::new();
    for (l, count) in counts(contacts).iter().enumerate() {
        if *count > 0 {
            edges.push((p.source_music_origin_seconds, l as u8, true));
        }
    }
    let mut i = 0;
    while i < changes.len() {
        let time = changes[i].0;
        let before = counts(contacts);
        while i < changes.len() && changes[i].0 == time {
            contacts[changes[i].1] = changes[i].2;
            i += 1;
        }
        let after = counts(contacts);
        for l in 0..before.len() {
            if before[l] > 0 && after[l] == 0 {
                edges.push((time, l as u8, false));
            }
        }
        for l in 0..before.len() {
            if before[l] == 0 && after[l] > 0 {
                edges.push((time, l as u8, true));
            }
        }
    }
    edges.retain(|(t, _, _)| *t > after);
    Ok(edges)
}
struct Active {
    judge: Judge,
    report: JudgeReport,
    id: String,
    at: usize,
}
pub(crate) struct ReplayJob {
    notes: Vec<Note>,
    rules: Vec<(String, JudgeTable)>,
    lanes: usize,
    timing: TimingMap,
    edges: Vec<(f64, u8, bool)>,
    rule: usize,
    active: Option<Active>,
    report: ReplayReport,
}
impl ReplayJob {
    pub fn new(input: &Input, p: &ContactPlan) -> Result<Self, String> {
        if input.rules.is_empty() {
            return Err("missing gameplay judge rules".into());
        }
        Ok(Self {
            notes: input.effective_notes.clone(),
            rules: input.rules.clone(),
            lanes: layout_lanes(&input.layout)?,
            timing: input.timing.clone(),
            edges: trace(input, p)?,
            rule: 0,
            active: None,
            report: ReplayReport {
                passed: true,
                rules: BTreeMap::new(),
            },
        })
    }
    pub fn step(&mut self) -> bool {
        if self.rule == self.rules.len() {
            return true;
        }
        if self.active.is_none() {
            let (id, table) = &self.rules[self.rule];
            let window = table.hold_window_seconds();
            let judge = Judge::new(&self.notes, &self.timing, table.clone()).with_lanes(self.lanes);
            let report = JudgeReport {
                passed: false,
                expected_shocks: judge.shock_count() as usize,
                avoided_shocks: 0,
                expected_mines: judge.mine_count() as usize,
                avoided_mines: 0,
                hits: 0,
                holds: 0,
                expected_hits: judge.judged_note_count() as usize,
                expected_holds: judge.hold_count() as usize,
                maximum_hit_delta_seconds: 0.,
                bad: Vec::new(),
                unresolved: 0,
                edges: self.edges.len(),
                hold_window_seconds: window,
            };
            self.active = Some(Active {
                judge,
                report,
                id: id.clone(),
                at: 0,
            });
            return false;
        }
        let a = self.active.as_mut().unwrap();
        if let Some(&(time, lane, down)) = self.edges.get(a.at) {
            let events = if down {
                a.judge.press(lane, time)
            } else {
                a.judge.release(lane, time)
            };
            observe(&mut a.report, events);
            a.at += 1;
            return false;
        }
        let events = a
            .judge
            .update(a.judge.last_end_seconds().unwrap_or(0.) + 1.);
        observe(&mut a.report, events);
        a.report.avoided_mines = a
            .judge
            .notes()
            .iter()
            .filter(|n| {
                n.judgeable && n.note.kind == NoteKind::Mine && n.state == NoteState::MineAvoided
            })
            .count();
        a.report.unresolved = a
            .judge
            .notes()
            .iter()
            .filter(|n| {
                n.judgeable
                    && !matches!(
                        n.state,
                        NoteState::MineAvoided
                            | NoteState::Held
                            | NoteState::Hit {
                                judgement: Judgement::W1,
                                ..
                            }
                    )
            })
            .count();
        a.report.passed = a.report.bad.is_empty()
            && a.report.hits == a.report.expected_hits
            && a.report.holds == a.report.expected_holds
            && a.report.unresolved == 0
            && a.report.avoided_mines == a.report.expected_mines
            && a.report.avoided_shocks == a.report.expected_shocks;
        let a = self.active.take().unwrap();
        self.report.passed &= a.report.passed;
        self.report.rules.insert(a.id, a.report);
        self.rule += 1;
        self.rule == self.rules.len()
    }
    pub fn finish(self) -> ReplayReport {
        self.report
    }
}
pub(crate) fn observe(r: &mut JudgeReport, events: Vec<JudgeEvent>) {
    for e in events {
        match e.kind {
            JudgeEventKind::Tap(Judgement::W1, delta) if delta.abs() <= 1e-9 => {
                r.hits += 1;
                r.maximum_hit_delta_seconds = r.maximum_hit_delta_seconds.max(delta.abs());
            }
            JudgeEventKind::Held => r.holds += 1,
            JudgeEventKind::AvoidShock => r.avoided_shocks += 1,
            JudgeEventKind::Checkpoint { missed, .. } if missed > 0 && r.bad.len() < 4 => {
                r.bad.push(format!(
                    "missed {missed} hold checkpoints at {}",
                    e.song_time
                ));
            }
            JudgeEventKind::Tap(..)
            | JudgeEventKind::LetGo
            | JudgeEventKind::HitMine
            | JudgeEventKind::HitShock
                if r.bad.len() < 4 =>
            {
                r.bad
                    .push(format!("{:?} lane {} at {}", e.kind, e.lane, e.song_time));
            }
            _ => {}
        }
    }
}
pub fn replay(input: &Input, p: &ContactPlan) -> Result<ReplayReport, String> {
    let mut j = ReplayJob::new(input, p)?;
    while !j.step() {}
    Ok(j.finish())
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn unscored_center_never_becomes_a_solo_button() {
        assert_eq!(lane("dance-solo", 5), None);
        assert_eq!(lane("dance-solo", 7), Some(1));
        assert_eq!(lane("dance-solo", 9), Some(4));
        assert_eq!(lane("danoni-5", 5), Some(4));
        assert_eq!(lane("dance-single", 5), None);
        assert_eq!(lane("pump-single", 1), Some(0));
        assert_eq!(lane("pump-single", 5), Some(2));
        assert_eq!(lane("pump-single", 3), Some(4));
        assert_eq!(lane("danoni-7", 7), None);
        assert_eq!(lane("danoni-7", 3), None);
        assert_eq!(lane("danoni-7", 5), Some(3));
        assert_eq!(lane("danoni-7", 1), Some(1));
        assert_eq!(lane("danoni-7", 9), Some(5));
        assert_eq!(lane("techno-single8", 5), None);
        assert_eq!(lane("techno-single8", 1), Some(0));
        assert_eq!(lane("techno-single8", 3), Some(7));
        for panel in 1..=9 {
            assert!(lane("danoni-9B", panel).is_some());
        }
    }
}
