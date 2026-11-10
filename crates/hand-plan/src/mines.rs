// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Conservative mine windows include every supported judge plus 5 ms clearance.
//! Layouts use unscored center/side/corner stances where available, or air time.
use super::*;
pub(crate) const WINDOW: f64 = 0.095;
impl Search {
    pub(super) fn departure_limits(&self, s: &State, d: usize, land: f64) -> (f64, f64) {
        // The source tile must be clear by the hazard window, which can be
        // earlier than the scored destination. Use that actual deadline for
        // the existing adaptive planted/swing floors.
        let deadline = if self.options.adaptive_timing {
            self.mines
                .iter()
                .filter(|m| panel(m) == s.panels[d] && m.time - self.origin > s.last[d])
                .map(|m| m.time - self.origin - WINDOW)
                .fold(land, f64::min)
        } else {
            land
        };
        limits(deadline - s.last[d], self.options.adaptive_timing)
    }
    /// A released button is not yet a raised fingertip. Reserve half a
    /// millimeter of target clearance at the physical mine-window boundaries;
    /// cap the hop at 15 mm so an impossible late takeoff is rejected instead
    /// of producing an arbitrarily tall flight.
    pub(super) fn clearance_lift(
        &self,
        from: u8,
        to: u8,
        release: f64,
        land: f64,
        height: f64,
    ) -> Option<f64> {
        self.clearance_lift_for([from, to], [release, land], height, false, None)
    }
    fn scored_landing(&self, s: &State, d: usize, target: u8, time: f64) -> bool {
        s.events
            .backwards()
            .any(|e| e.foot.index() == d && !e.lift && e.panel == target && e.time == time)
            || s.aux.backwards().any(|a| {
                a.kind == RestoreKind::MineAvoid
                    && a.foot.index() == d
                    && a.panel == target
                    && a.time == time
                    && self.rows.iter().skip(self.row).flatten().any(|n| {
                        n.is_head()
                            && self.rules.has_panel(n, target)
                            && n.time - self.origin == time
                    })
            })
    }
    fn departure_deadline(&self, s: &State, d: usize, mine: f64) -> f64 {
        if self.hazard_retry && self.scored_landing(s, d, s.panels[d], s.last[d]) {
            ddi_hand_pose::mine_head_clearance_start(mine, s.last[d], s.last[d].next_up())
                .unwrap_or(mine - WINDOW)
        } else {
            mine - WINDOW
        }
    }
    pub(super) fn clearance_lift_from_head(
        &self,
        s: &State,
        d: usize,
        to: u8,
        times: [f64; 2],
        height: f64,
        scored: bool,
    ) -> Option<f64> {
        let head = (self.hazard_retry && self.scored_landing(s, d, s.panels[d], s.last[d]))
            .then_some(s.last[d]);
        self.clearance_lift_for([s.panels[d], to], times, height, scored, head)
    }
    fn clearance_lift_for(
        &self,
        [from, to]: [u8; 2],
        [release, land]: [f64; 2],
        mut height: f64,
        scored: bool,
        head: Option<f64>,
    ) -> Option<f64> {
        // Every boundary below lies within a mine's ±90 ms window, and only
        // one inside the flight matters: the mines (sorted) whose windows meet it.
        let reach = 0.09 + 1e-6;
        let first = self
            .mines
            .partition_point(|m| m.time - self.origin + reach <= release);
        let near = &self.mines[first..];
        let near = &near[..near.partition_point(|m| m.time - self.origin - reach < land)];
        if near.is_empty() {
            return (height <= 0.075).then_some(height);
        }
        let xy = |panel| self.position(panel);
        let a = xy(from);
        let b = xy(to);
        for mine in near {
            let time = mine.time - self.origin;
            let center = xy(super::panel(mine));
            let departure = head
                .filter(|_| super::panel(mine) == from)
                .and_then(|h| ddi_hand_pose::mine_head_clearance_start(time, h, release));
            // The received destination may precede this mine. Its actual
            // outgoing flight is still required by mine_safe before acceptance.
            let arrival = (self.hazard_retry && scored && super::panel(mine) == to)
                .then(|| ddi_hand_pose::mine_head_clearance_start(time, land, land.next_up()))
                .flatten();
            let start = arrival.or(departure).unwrap_or(time - 0.09);
            let end = if scored
                && super::panel(mine) == to
                && (self.hazard_retry || self.staggered_recovery || land <= time + 0.095)
            {
                ddi_hand_pose::mine_head_clearance_end(time, release, land, from == to)
                    .unwrap_or(time + 0.09)
            } else {
                time + 0.09
            };
            for boundary in [start, time, end] {
                if boundary < start || boundary > end || boundary <= release || boundary >= land {
                    continue;
                }
                let u = (boundary - release) / (land - release);
                let w = ddi_hand_pose::smoother(u);
                // The centered runtime changes within-tile offsets only. Leave
                // an extra millimeter around the tile for that fitting step.
                if (0..2).any(|i| (a[i] + (b[i] - a[i]) * w - center[i]).abs() > 12.5) {
                    continue;
                }
                let gain = 200. * 64. * u.powi(3) * (1. - u).powi(3);
                height = height.max(0.5 / gain);
            }
        }
        (height <= 0.075).then_some(height)
    }
    /// At most five hazard alternatives per parent, within the existing beam.
    /// Prefer an unscored escape when available: returning to a hazardous
    /// arrow after a fixed hop needlessly traps the next repeated mine window.
    /// DanOni five-key charts use air time without assuming a safe tile.
    pub(super) fn hazard_candidates(
        &self,
        s: State,
        hazards: &[InputNote],
        land: f64,
    ) -> impl Iterator<Item = State> + use<> {
        // A head inside a mine's early window is checked before the mine's
        // own row. Plan clearance now, rather than rejecting a stationary
        // companion before the search has had a chance to lift it.
        let until = s.last.into_iter().fold(land, f64::max);
        let start = self
            .mines
            .partition_point(|m| m.time - self.origin < land - 0.6);
        let pending = &self.mines[start..];
        let clear = |mut state: State, mode: u8, escape: [u8; 2]| {
            let mut horizon = until;
            let mut i = 0;
            while i < pending.len() {
                let mine = &pending[i];
                if mine.time - self.origin - WINDOW > horizon {
                    break;
                }
                let time = mine.time - self.origin;
                // The new direct-flight alternative treats a simultaneous row
                // as one maneuver. Its intermediate one-foot stance need not
                // be a legal final two-foot stance.
                let end = if mode >= 3 {
                    i + pending[i..].partition_point(|m| m.time == mine.time)
                } else {
                    i + 1
                };
                let group = &pending[i..end];
                state = match mode {
                    0 => self.escape_mines(state, group, time, escape)?,
                    1 => self.avoid_mines(state, group, time)?,
                    2 => self.head_clearance(state, group, time, false, false)?,
                    3 => self.head_clearance(state, group, time, true, false)?,
                    5 => self.head_clearance(state, group, time, true, true)?,
                    _ => self.escape_mines(state, group, time, escape)?,
                };
                i = end;
                // A clearance landing can expose the companion to another
                // known mine before that landing. Plan through the enlarged
                // horizon too: mine_safe validates this same whole interval.
                // The sorted, already-received mine list bounds this scan.
                horizon = state.last.into_iter().fold(horizon, f64::max);
            }
            Some(state)
        };
        let has_hazard = hazards.iter().any(|n| n.mine)
            || pending
                .first()
                .is_some_and(|m| m.time - self.origin - WINDOW <= until);
        let mut choices: [Option<State>; 5] = std::array::from_fn(|_| None);
        // Keep mine-free rows allocation-free, with no extra history clone.
        if !has_hazard {
            choices[0] = Some(s);
            return choices.into_iter().flatten();
        }
        let mut count = 0;
        for &escape in self.escape_options(&s).iter() {
            if let Some(state) = clear(s.clone(), 0, escape) {
                choices[count] = Some(state);
                count += 1;
            }
        }
        if count == 0 {
            for (i, &escape) in self.escape_options(&s).iter().enumerate() {
                choices[3 + i] = clear(s.clone(), 4, escape);
            }
        }
        let head = clear(s.clone(), 2, [5, 5]);
        let direct = clear(s.clone(), 3, [5, 5]);
        let mut flight = clear(s.clone(), 1, [5, 5]);
        if count != 0
            && let Some(flight) = &mut flight
        {
            flight.cost += 4.;
        }
        choices[count] = flight;
        choices[count + 1] = head;
        choices[count + 2] = direct;
        // Only when legacy alternatives fail: reserve alternating scored
        // flights, allowing a local unscored support at the received horizon.
        // Reuse an empty slot; the per-parent candidate bound stays five.
        if self.hazard_retry && choices.iter().all(Option::is_none) {
            choices[0] = clear(s, 5, [5, 5]);
        }
        if self.hazard_retry {
            for state in choices.iter_mut().flatten() {
                state.hazard_recovery = true;
            }
        }
        choices.into_iter().flatten()
    }
    fn head_clearance(
        &self,
        mut s: State,
        hazards: &[InputNote],
        time: f64,
        direct: bool,
        local_support: bool,
    ) -> Option<State> {
        for d in 0..2 {
            let source = s.panels[d];
            let Some(mine) = hazards
                .iter()
                .find(|n| n.mine && self.rules.has_panel(n, source))
            else {
                continue;
            };
            if s.last[d] > time || held(&s, d, time).is_some() {
                continue;
            }
            let choice = self
                .rows
                .iter()
                .skip(self.row + 1)
                .flatten()
                .filter(|n| n.is_head())
                .take_while(|n| n.time - self.origin <= time + if direct { 0.45 } else { 0.170 })
                .flat_map(|n| self.rules.choices(*n))
                .find_map(|n| {
                    let land = n.time - self.origin;
                    let target = panel(&n);
                    // This is the received head itself, not an unscored restore
                    // that could steal a nearby press. Its landing is reused.
                    if land < time
                        || (!direct && target == source)
                        || target == s.panels[1 - d]
                        || (!self.staggered_recovery && !self.allowed(target, land))
                    {
                        return None;
                    }
                    let mut panels = s.panels;
                    panels[d] = target;
                    let (heading, span) = self.shape(panels);
                    if !self.legal(panels, heading, span) {
                        return None;
                    }
                    let ls = self.departure_limits(&s, d, land);
                    let dist = self.distance(target, source, d);
                    let release = (land - 0.45_f64.min(0.20 + dist / 140.))
                        .min(if direct { time - 0.170 } else { f64::INFINITY })
                        .max(s.last[d] + ls.0)
                        .max(
                            s.holds[d] + if s.holds[d] > 0. { 0.01 } else { 0. }
                                - self.options.hold_tail_grace,
                        )
                        .max(self.release_floor())
                        .max(if direct {
                            land - 0.45
                        } else {
                            f64::NEG_INFINITY
                        });
                    if release > self.departure_deadline(&s, d, time)
                        || land - release < ls.1 - 1e-9
                    {
                        return None;
                    }
                    let height = self.clearance_lift_from_head(
                        &s,
                        d,
                        target,
                        [release, land],
                        lift(dist, false, land - release, self.options.adaptive_timing),
                        true,
                    )?;
                    Some((target, land, release, height, ls, dist))
                });
            if let Some((target, land, release, height, ls, dist)) = choice {
                // Reserve the exact upcoming press now. Its ordinary source
                // row reuses this landing and emits its one scored event.
                s.tracks[d].push(Swing {
                    release,
                    land,
                    from: source,
                    to: target,
                    lift_m: height,
                });
                s.aux.push(Auxiliary {
                    time: land,
                    foot: Foot::at(d),
                    panel: target,
                    kind: RestoreKind::MineAvoid,
                    end_tick: mine.tick,
                });
                s.staggered_flights += usize::from(self.staggered_recovery);
                s.record(d, s.last[d], release, land, ls, self.options);
                s.last[d] = land;
                s.panels[d] = target;
                // Preserve the established escape preference when both work.
                s.cost += 8. + square(dist / 18.) * 0.65;
            }
        }
        if self.rules.escapes.is_empty() {
            self.avoid_mines(s, hazards, time)
        } else {
            self.escape_options(&s)
                .iter()
                .find_map(|&escape| self.escape_mines(s.clone(), hazards, time, escape))
                .or_else(|| {
                    local_support
                        .then(|| self.local_escape(s, hazards, time))
                        .flatten()
                })
        }
    }
    /// A single threatened finger may park on a nearby unscored tile while
    /// its companion supports the hand. Do not force both onto one center.
    /// Two threatened fingers retain the established grouped/air-time paths.
    fn local_escape(&self, s: State, hazards: &[InputNote], time: f64) -> Option<State> {
        if !self.rules.double {
            return None;
        }
        let affected = |d: usize| {
            s.last[d] <= time && hazards.iter().any(|n| n.mine && panel(n) == s.panels[d])
        };
        let d = match [affected(0), affected(1)] {
            [true, false] => 0,
            [false, true] => 1,
            _ => return None,
        };
        // This shared helper keeps only legal unscored stances, ordered by
        // travel distance, and returns at most two targets.
        self.support_targets(&s, s.panels, 1 - d)
            .iter()
            .find_map(|&target| {
                let mut escape = s.panels;
                escape[d] = target;
                self.escape_mines(s.clone(), hazards, time, escape)
                    .map(|mut next| {
                        next.hazard_recovery = true;
                        next
                    })
            })
    }
    fn escape_mines(
        &self,
        mut s: State,
        hazards: &[InputNote],
        land: f64,
        escape: [u8; 2],
    ) -> Option<State> {
        if !hazards.iter().any(|n| n.mine) {
            return Some(s);
        }
        for (d, &target) in escape.iter().enumerate() {
            let Some(mine) = hazards.iter().find(|n| n.mine && panel(n) == s.panels[d]) else {
                continue;
            };
            if s.last[d] > land {
                continue;
            } // A fixed pending return is checked below.
            if held(&s, d, land).is_some_and(|h| h.roll) {
                self.roll_mine(&mut s, d, land)?;
                continue;
            }
            let source = s.panels[d];
            let ls = limits(land - WINDOW - s.last[d], self.options.adaptive_timing);
            let active = held(&s, d, land);
            let restore =
                active.is_some() && s.holds[d] - self.options.hold_tail_grace > land - WINDOW;
            if restore && self.options.hold_restore_window == 0. {
                return None;
            }
            let ideal = if restore { 0.10 } else { 0.14 };
            let earliest = (s.last[d] + ls.0)
                .max(self.release_floor())
                .max(if restore {
                    f64::NEG_INFINITY
                } else {
                    s.holds[d] + if s.holds[d] > 0. { 0.01 } else { 0. }
                        - self.options.hold_tail_grace
                });
            let release = (land - ideal).max(earliest);
            if release > self.departure_deadline(&s, d, land) || land - release < ls.1 - 1e-9 {
                return None;
            }
            let dist = self.distance(target, source, d);
            s.tracks[d].push(Swing {
                release,
                land,
                from: source,
                to: target,
                lift_m: self.clearance_lift_from_head(
                    &s,
                    d,
                    target,
                    [release, land],
                    lift(dist, false, land - release, self.options.adaptive_timing),
                    false,
                )?,
            });
            s.record(d, s.last[d], release, land, ls, self.options);
            s.aux.push(Auxiliary {
                time: land,
                foot: Foot::at(d),
                panel: target,
                kind: RestoreKind::MineAvoid,
                end_tick: mine.tick,
            });
            s.last[d] = land;
            s.panels[d] = target;
            s.cost += 0.5 + square(dist / 18.) * 0.65;
            if restore {
                let h = active?;
                let restored = land + 0.115;
                if restored - release > self.options.hold_restore_window + 1e-9
                    || restored >= h.end - 0.005
                    || !self.allowed(source, restored)
                {
                    return None;
                }
                s.tracks[d].push(Swing {
                    release: land + 0.055,
                    land: restored,
                    from: target,
                    to: source,
                    lift_m: self.clearance_lift(
                        target,
                        source,
                        land + 0.055,
                        restored,
                        lift(dist, false, 0.060, self.options.adaptive_timing),
                    )?,
                });
                s.aux.push(Auxiliary {
                    time: restored,
                    foot: Foot::at(d),
                    panel: source,
                    kind: RestoreKind::HoldRestore,
                    end_tick: h.end_tick,
                });
                s.gaps.push(Gap {
                    foot: Foot::at(d),
                    panel: source,
                    release,
                    restore: restored,
                    end_tick: h.end_tick,
                    restore_foot: None,
                });
                s.last[d] = restored;
                s.panels[d] = source;
                s.cost += 2.;
            }
        }
        let (heading, span) = self.shape(s.panels);
        if !self.legal(s.panels, heading, span) {
            return None;
        }
        s.cost += square((heading - s.heading) / 45.) * 0.25 + self.facing_cost(heading);
        s.heading = heading;
        Some(s)
    }
    fn avoid_mines(&self, mut s: State, hazards: &[InputNote], land: f64) -> Option<State> {
        if !hazards.iter().any(|n| n.mine) {
            return Some(s);
        }
        for d in 0..2 {
            let Some(mine) = hazards.iter().find(|n| n.mine && panel(n) == s.panels[d]) else {
                continue;
            };
            if s.last[d] > land {
                continue;
            } // A fixed pending return is checked below.
            if held(&s, d, land).is_some_and(|h| h.roll) {
                self.roll_mine(&mut s, d, land)?;
                continue;
            }
            let source = s.panels[d];
            let ls = limits(land - WINDOW - s.last[d], self.options.adaptive_timing);
            let active = held(&s, d, land);
            // Consecutive mines on one arrow are one danger span. A five-key
            // chart cannot assume an unscored center escape, so stay airborne
            // across a short cluster instead of landing between its mines.
            // Bound the span to keep both planning and the flight local.
            let mut last_hazard = land;
            if active.is_none() {
                for next in self.mines.iter().filter(|n| panel(n) == source) {
                    let time = next.time - self.origin;
                    if time <= last_hazard {
                        continue;
                    }
                    if time > land + 0.4 || time - last_hazard > 0.2198 {
                        break;
                    }
                    last_hazard = time;
                }
            }
            let clustered = last_hazard > land;
            let clearance = if clustered { 0.170 } else { 0.1099 };
            let release = land - clearance;
            let mut restored = last_hazard + clearance;
            // Avoid an unscored return stealing a nearby same-panel head.
            // Reuse its exact touchdown instead, if the flight remains local.
            if active.is_none()
                && let Some(n) = self
                    .rows
                    .iter()
                    .skip(self.row + 1)
                    .flatten()
                    .find(|n| n.is_head() && self.rules.has_panel(n, source))
            {
                let t = n.time - self.origin;
                if t >= last_hazard + 0.1099 && t - last_hazard <= 0.170 {
                    restored = t;
                }
            }
            if !self.safe_return(source, restored) {
                return None;
            }
            if release < self.release_floor()
                || release < s.last[d] + ls.0 - 1e-9
                || active.is_none() && release < s.holds[d] - self.options.hold_tail_grace
                || active.is_some() && restored - release > self.options.hold_restore_window + 1e-9
            {
                return None;
            }
            s.tracks[d].push(Swing {
                release,
                land: restored,
                from: source,
                to: source,
                // For long anticipatory flights retain clearance at the window
                // boundaries, not merely at the hop's midpoint.
                lift_m: self.clearance_lift(
                    source,
                    source,
                    release,
                    restored,
                    if clustered {
                        0.045
                    } else {
                        0.045 * ((restored - release) / 0.220).powi(2)
                    },
                )?,
            });
            s.aux.push(Auxiliary {
                time: restored,
                foot: Foot::at(d),
                panel: source,
                kind: RestoreKind::MineAvoid,
                end_tick: mine.tick,
            });
            if let Some(h) = active {
                s.gaps.push(Gap {
                    foot: Foot::at(d),
                    panel: source,
                    release,
                    restore: restored,
                    end_tick: h.end_tick,
                    restore_foot: None,
                });
            }
            s.record(d, s.last[d], release, restored, ls, self.options);
            s.last[d] = restored;
            s.cost += 0.6;
        }
        let (heading, span) = self.shape(s.panels);
        if !self.legal(s.panels, heading, span) {
            return None;
        }
        s.cost += square((heading - s.heading) / 45.) * 0.25 + self.facing_cost(heading);
        s.heading = heading;
        Some(s)
    }
    /// Only the bounded span new flights can change; old certified contacts have
    /// already passed the same guard. Mine-free inputs skip hazard scans.
    pub(super) fn mine_safe(&self, s: &State, land: f64) -> bool {
        if self.mines.is_empty() {
            return true;
        }
        let until = s.last.into_iter().fold(land, f64::max);
        // 600 ms covers the maximum 450 ms outward flight plus the mine window.
        let start = self
            .mines
            .partition_point(|m| m.time - self.origin < land - 0.6);
        for m in self.mines[start..]
            .iter()
            .take_while(|m| m.time - self.origin <= until + WINDOW)
        {
            let lo = m.time - self.origin - WINDOW;
            let hi = (m.time - self.origin + WINDOW).min(until);
            if hi < lo {
                continue;
            }
            for d in 0..2 {
                let mut end = until;
                let mut initial = s.panels[d];
                let mut outgoing: Option<&Swing> = None;
                for swing in s.tracks[d].backwards() {
                    initial = swing.from;
                    if swing.release > until {
                        continue;
                    }
                    if swing.to == panel(m) && swing.land <= hi && end >= lo {
                        let scored = self.scored_landing(s, d, swing.to, swing.land);
                        let approach = ddi_hand_pose::mine_head_clearance_end(
                            m.time - self.origin,
                            swing.release,
                            swing.land,
                            swing.from == panel(m),
                        )
                        .is_some();
                        let departure = (self.hazard_retry || s.hazard_recovery)
                            && outgoing.is_some_and(|out| {
                                out.from == panel(m)
                                    && out.release == end
                                    && ddi_hand_pose::mine_head_clearance_start(
                                        m.time - self.origin,
                                        swing.land,
                                        out.release,
                                    )
                                    .is_some()
                            });
                        if !scored || !(approach || departure) {
                            return false;
                        }
                    }
                    end = end.min(swing.release);
                    outgoing = Some(swing);
                    if end < lo {
                        break;
                    }
                }
                if initial == panel(m) && end >= lo {
                    return false;
                }
            }
        }
        true
    }
}
/// A scored source hit may postpone takeoff within the existing mine guard.
/// Require its connected incoming flight and received head on this same finger;
/// a nearby note, unscored landing or another finger cannot authorize the exit.
fn guarded_departure(
    input: &Input,
    p: &ContactPlan,
    foot: Foot,
    track: &[Swing],
    index: usize,
    mine: f64,
) -> bool {
    let Some(incoming) = index.checked_sub(1).and_then(|i| track.get(i)) else {
        return false;
    };
    let outgoing = &track[index];
    incoming.release < incoming.land
        && incoming.to == outgoing.from
        && incoming.land < outgoing.release
        && ddi_hand_pose::mine_head_clearance_start(mine, incoming.land, outgoing.release).is_some()
        && p.events.iter().any(|e| {
            e.foot == foot
                && !e.lift
                && e.panel == outgoing.from
                && e.time == incoming.land
                && input.notes.iter().any(|n| {
                    n.judgeable
                        && n.is_head()
                        && n.tick == e.tick
                        && note_has_panel(&p.layout, n, e.panel)
                        && (n.time - p.source_music_origin_seconds - e.time).abs() < 1e-9
                })
        })
}
pub(crate) fn validate_auxiliary(
    input: &Input,
    p: &ContactPlan,
    a: &Auxiliary,
) -> Result<(), String> {
    let track = p.tracks.of(a.foot);
    let index = track
        .iter()
        .position(|s| s.land == a.time && s.to == a.panel)
        .ok_or("mine clearance has no physical landing")?;
    let swing = &track[index];
    if !input.notes.iter().any(|n| {
        let t = n.time - p.source_music_origin_seconds;
        n.judgeable
            && n.mine
            && n.tick == a.end_tick
            && note_has_panel(&p.layout, n, swing.from)
            && ((swing.to == swing.from && swing.release <= t - WINDOW && swing.land >= t + WINDOW)
                || ((crate::escape_stances(&input.layout)
                    .iter()
                    .any(|p| p[a.foot.index()] == a.panel)
                    || (double_mat(&input.layout)
                        && crate::support_tiles(&input.layout).contains(&a.panel)))
                    && (a.time - t).abs() < 1e-9)
                || ((swing.release <= t - WINDOW
                    || guarded_departure(input, p, a.foot, track, index, t))
                    && a.time >= t
                    && a.time <= t + 0.450
                    && input.notes.iter().any(|head| {
                        head.judgeable
                            && head.is_head()
                            && note_has_panel(&p.layout, head, a.panel)
                            && (head.time - p.source_music_origin_seconds - a.time).abs() < 1e-9
                    })))
    }) {
        return Err("air-time clearance does not span an original hazard".into());
    }
    Ok(())
}

#[cfg(test)]
mod escape_tests {
    use super::*;
    #[test]
    fn an_early_head_needs_a_real_departure_not_a_window_end() {
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#OFFSET:-4;#BPMS:0=120;#NOTES:dance-single::Easy:1:0,0,0,0,0:1000;",
        )
        .unwrap();
        song.charts[0].notes = vec![
            Note::new(ddi_chart::Tick(192), 0, NoteKind::Tap),
            Note::new(ddi_chart::Tick(200), 0, NoteKind::Mine),
        ];
        let input = Input::from_song(&song, 0).unwrap();
        let mut search = Search::new(&input, Options::default()).unwrap();
        search.hazard_retry = true;
        let head = input.notes[0].time - search.origin;
        let mine = search.mines[0].time - search.origin;
        let mut state = State::initial();
        state.panels = [4, 6];
        state.last = [head, head + 0.02];
        state.events.push(Event {
            time: head,
            music_time: input.notes[0].time,
            tick: 192,
            panel: 4,
            foot: Foot::L,
            lift: false,
        });
        state.tracks[0].push(Swing {
            release: head - 0.2,
            land: head,
            from: 5,
            to: 4,
            lift_m: 0.035,
        });
        assert!(
            !search.mine_safe(&state, head),
            "the other finger's future time is not a release"
        );
        state.tracks[0].push(Swing {
            release: head + 0.02,
            land: mine,
            from: 4,
            to: 5,
            lift_m: 0.035,
        });
        state.panels[0] = 5;
        state.last[0] = mine;
        assert!(search.mine_safe(&state, head));
        state.events = Default::default();
        assert!(
            !search.mine_safe(&state, head),
            "unscored contact is not authorized"
        );
    }
    #[test]
    fn guarded_departure_to_a_scored_head_requires_its_source_hit_and_flight() {
        for mirror in [false, true] {
            let mut song = ddi_chart::formats::sm::parse_sm(
                "#OFFSET:-4;#BPMS:0=220;#NOTES:dance-single::Easy:1:0,0,0,0,0:1000;",
            )
            .unwrap();
            let target_lane = if mirror { 0 } else { 3 };
            song.charts[0].notes = vec![
                Note::new(ddi_chart::Tick(192), 2, NoteKind::Tap),
                Note::new(ddi_chart::Tick(216), 2, NoteKind::Mine),
                Note::new(ddi_chart::Tick(216), target_lane, NoteKind::Tap),
            ];
            song.charts[0].notes.sort_by_key(|n| (n.tick, n.lane));
            let input = Input::from_song(&song, 0).unwrap();
            let mut search = Search::new(&input, Options::default()).unwrap();
            let head = input.notes[0].time - search.origin;
            let mine = search.mines[0].time - search.origin;
            let d = usize::from(!mirror);
            let target = if mirror { 4 } else { 6 };
            let mut state = State::initial();
            state.panels[d] = target;
            state.last[d] = mine;
            state.tracks[d].push(Swing {
                release: head - 0.2,
                land: head,
                from: 5,
                to: 8,
                lift_m: 0.04,
            });
            state.tracks[d].push(Swing {
                release: head + 0.055,
                land: mine,
                from: 8,
                to: target,
                lift_m: 0.04,
            });
            for (time, tick, panel) in [(head, 192, 8), (mine, 216, target)] {
                state.events.push(Event {
                    time,
                    music_time: time + search.origin,
                    tick,
                    panel,
                    foot: Foot::at(d),
                    lift: false,
                });
            }
            let auxiliary = Auxiliary {
                time: mine,
                foot: Foot::at(d),
                panel: target,
                kind: RestoreKind::MineAvoid,
                end_tick: 216,
            };
            state.aux.push(auxiliary);
            search.states = vec![state];
            let plan = search.build().0;
            fn track(p: &mut ContactPlan, mirror: bool) -> &mut Vec<Swing> {
                if mirror {
                    &mut p.tracks.left
                } else {
                    &mut p.tracks.right
                }
            }
            assert!(head + 0.055 > mine - WINDOW);
            assert!(validate_auxiliary(&input, &plan, &auxiliary).is_ok());
            assert!(crate::replay(&input, &plan).unwrap().passed);
            for forgery in 0..7 {
                let mut bad = plan.clone();
                let mut source = input.clone();
                match forgery {
                    0 => {
                        bad.events.remove(0);
                    }
                    1 => {
                        bad.events[0].foot = Foot::at(1 - d);
                    }
                    2 => {
                        source.notes[0].time += 0.001;
                    }
                    3 => {
                        track(&mut bad, mirror).remove(0);
                    }
                    4 => {
                        track(&mut bad, mirror)[0].to = target;
                    }
                    5 => {
                        track(&mut bad, mirror)[1].release = (head + mine) * 0.5 - 0.005;
                    }
                    _ => {
                        bad.events[0].lift = true;
                    }
                }
                assert!(
                    validate_auxiliary(&source, &bad, &auxiliary).is_err(),
                    "mirror={mirror}, forgery={forgery}"
                );
            }
        }
    }
    #[test]
    fn local_support_keeps_the_companion_and_requires_one_releasable_finger() {
        let song = ddi_chart::formats::sm::parse_sm(
            "#OFFSET:-4;#BPMS:0=128;#NOTES:dance-double::Easy:1:0,0,0,0,0:0000MMMM;",
        )
        .unwrap();
        let input = Input::from_song(&song, 0).unwrap();
        let mut search = Search::new(&input, Options::default()).unwrap();
        let time = search.mines[0].time - search.origin;
        let mut state = State::initial();
        state.panels = [6, 13];
        state.last = [time - 0.24; 2];
        let next = search
            .local_escape(state.clone(), &search.mines, time)
            .unwrap();
        assert_eq!(next.panels[0], state.panels[0]);
        assert_eq!(next.last[0], state.last[0]);
        assert!(next.tracks[0].backwards().next().is_none());
        assert!([1, 3, 7, 9, 10, 12, 16, 18].contains(&next.panels[1]));
        assert!(search.mine_safe(&next, time));
        let mut two = search.mines.clone();
        two.push(InputNote {
            panel: 6,
            lane: 3,
            ..two[0]
        });
        assert!(search.local_escape(state.clone(), &two, time).is_none());
        search.states = vec![next];
        let plan = search.build().0;
        let auxiliary = plan.aux_contacts.as_ref().unwrap()[0];
        assert!(validate_auxiliary(&input, &plan, &auxiliary).is_ok());
        let forged = Auxiliary {
            end_tick: -1,
            ..auxiliary
        };
        assert!(validate_auxiliary(&input, &plan, &forged).is_err());
        state.holds[1] = time + 0.5;
        assert!(search.local_escape(state, &search.mines, time).is_none());
    }
    #[test]
    fn pump_offers_both_side_escape_orientations_and_air_time() {
        let song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:pump-single::Easy:1:0,0,0,0,0:\nM00M0;",
        )
        .unwrap();
        let input = Input::from_song(&song, 0).unwrap();
        let search = Search::new(&input, Options::default()).unwrap();
        let mut state = State::initial();
        state.panels = [9, 1];
        let time = search.mines[0].time - search.origin;
        let choices = search
            .hazard_candidates(state, &search.mines, time)
            .collect::<Vec<_>>();
        for panels in [[4, 6], [6, 4], [9, 1]] {
            assert!(
                choices.iter().any(|s| s.panels == panels),
                "missing {panels:?}"
            );
        }
    }
    #[test]
    fn seven_key_hazards_offer_both_unused_corner_stances_and_air_time() {
        let song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:danoni-7::Easy:1:0,0,0,0,0:\n0M000M0;",
        )
        .unwrap();
        let input = Input::from_song(&song, 0).unwrap();
        let search = Search::new(&input, Options::default()).unwrap();
        let mut state = State::initial();
        state.panels = [1, 9];
        let time = search.mines[0].time - search.origin;
        let choices = search
            .hazard_candidates(state, &search.mines, time)
            .collect::<Vec<_>>();
        for panels in [[7, 3], [3, 7], [1, 9]] {
            assert!(
                choices.iter().any(|s| s.panels == panels),
                "missing {panels:?}"
            );
        }
    }
    #[test]
    fn techno_hazards_offer_center_escape_and_air_time() {
        let song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:techno-single8::Easy:1:0,0,0,0,0:\nM0000M00;",
        )
        .unwrap();
        let input = Input::from_song(&song, 0).unwrap();
        let search = Search::new(&input, Options::default()).unwrap();
        let mut state = State::initial();
        state.panels = [1, 9];
        let time = search.mines[0].time - search.origin;
        let choices = search
            .hazard_candidates(state, &search.mines, time)
            .collect::<Vec<_>>();
        for panels in [[5, 5], [1, 9]] {
            assert!(
                choices.iter().any(|s| s.panels == panels),
                "missing {panels:?}"
            );
        }
        assert_eq!(crate::support_tiles("techno-single8"), &[5]);
        assert_eq!(crate::support_tiles("pump-single"), &[4, 6]);
        assert_eq!(crate::support_tiles("danoni-7"), &[7, 3]);
    }
    #[test]
    fn nine_key_all_tile_hazards_have_no_escape_stance() {
        assert!(crate::escape_stances("danoni-9B").is_empty());
        assert!(crate::support_tiles("danoni-9B").is_empty());
        let song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:danoni-9B::Easy:1:0,0,0,0,0:\nMMMMMMMMM;",
        )
        .unwrap();
        let input = Input::from_song(&song, 0).unwrap();
        let search = Search::new(&input, Options::default()).unwrap();
        let mut state = State::initial();
        state.panels = [1, 9];
        let time = search.mines[0].time - search.origin;
        let choices = search
            .hazard_candidates(state, &search.mines, time)
            .collect::<Vec<_>>();
        assert!(!choices.is_empty());
        assert!(choices.iter().all(|s| s.panels == [1, 9]));
        for s in choices {
            for track in s.tracks {
                assert!(
                    track
                        .backwards()
                        .any(|f| f.release <= time - 0.095 && f.land >= time + 0.095)
                );
            }
        }
    }
}
