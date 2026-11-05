// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Conservative mine windows include every supported judge plus 5 ms clearance.
//! Keep center escape steps on four-panel charts and air-time flights on either layout.
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
        mut height: f64,
    ) -> Option<f64> {
        let xy = |panel| match panel {
            4 => [-24., 0.],
            2 => [0., -24.],
            8 => [0., 24.],
            6 => [24., 0.],
            _ => [0., 0.],
        };
        let a = xy(from);
        let b = xy(to);
        for mine in &self.mines {
            let time = mine.time - self.origin;
            let center = xy(super::panel(mine));
            for boundary in [time - 0.09, time, time + 0.09] {
                if boundary <= release || boundary >= land {
                    continue;
                }
                let u = (boundary - release) / (land - release);
                let w = u.powi(3) * (10. + u * (-15. + 6. * u));
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
    /// At most three hazard alternatives per parent, within the existing beam.
    /// Prefer a safe center escape when available: returning to a hazardous
    /// arrow after a fixed hop needlessly traps the next repeated mine window.
    /// Five-key charts retain air-time avoidance without assuming a safe tile.
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
        let clear = |mut state: State, mode: u8| {
            for mine in pending
                .iter()
                .take_while(|m| m.time - self.origin - WINDOW <= until)
            {
                let time = mine.time - self.origin;
                state = match mode {
                    0 => self.escape_mines(state, std::slice::from_ref(mine), time)?,
                    1 => self.avoid_mines(state, std::slice::from_ref(mine), time)?,
                    _ => self.head_clearance(state, std::slice::from_ref(mine), time)?,
                };
            }
            Some(state)
        };
        let has_hazard = hazards.iter().any(|n| n.mine)
            || pending
                .first()
                .is_some_and(|m| m.time - self.origin - WINDOW <= until);
        let escape = (has_hazard && self.layout == "dance-single")
            .then(|| clear(s.clone(), 0))
            .flatten();
        let head = has_hazard.then(|| clear(s.clone(), 2)).flatten();
        let mut flight = if has_hazard { clear(s, 1) } else { Some(s) };
        if escape.is_some()
            && let Some(flight) = &mut flight
        {
            flight.cost += 4.;
        }
        [escape, flight, head].into_iter().flatten()
    }
    fn head_clearance(&self, mut s: State, hazards: &[InputNote], time: f64) -> Option<State> {
        for d in 0..2 {
            let source = s.panels[d];
            let Some(mine) = hazards.iter().find(|n| n.mine && panel(n) == source) else {
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
                .filter(|n| !n.mine && !n.lift && !n.roll_refresh)
                .take_while(|n| n.time - self.origin <= time + 0.170)
                .find_map(|n| {
                    let land = n.time - self.origin;
                    let target = panel(n);
                    if land < time
                        || target == source
                        || target == s.panels[1 - d]
                        || !self.allowed(target, land)
                    {
                        return None;
                    }
                    let mut panels = s.panels;
                    panels[d] = target;
                    let (heading, span) = shape(panels);
                    if !legal(heading, span) {
                        return None;
                    }
                    let ls = self.departure_limits(&s, d, land);
                    let dist = distance(target, source, d);
                    let release = (land - 0.45_f64.min(0.20 + dist / 140.))
                        .max(s.last[d] + ls.0)
                        .max(
                            s.holds[d] + if s.holds[d] > 0. { 0.01 } else { 0. }
                                - self.options.hold_tail_grace,
                        )
                        .max(self.release_floor());
                    if release > time - WINDOW || land - release < ls.1 - 1e-9 {
                        return None;
                    }
                    let height = self.clearance_lift(
                        source,
                        target,
                        release,
                        land,
                        lift(dist, false, land - release, self.options.adaptive_timing),
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
                s.record(d, s.last[d], release, land, ls, self.options);
                s.last[d] = land;
                s.panels[d] = target;
                // Preserve the established escape preference when both work.
                s.cost += 8. + square(dist / 18.) * 0.65;
            }
        }
        if self.layout == "dance-single" {
            self.escape_mines(s, hazards, time)
        } else {
            self.avoid_mines(s, hazards, time)
        }
    }
    fn escape_mines(&self, mut s: State, hazards: &[InputNote], land: f64) -> Option<State> {
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
            if release > land - WINDOW || land - release < ls.1 - 1e-9 {
                return None;
            }
            let dist = distance(5, source, d);
            s.tracks[d].push(Swing {
                release,
                land,
                from: source,
                to: 5,
                lift_m: self.clearance_lift(
                    source,
                    5,
                    release,
                    land,
                    lift(dist, false, land - release, self.options.adaptive_timing),
                )?,
            });
            s.record(d, s.last[d], release, land, ls, self.options);
            s.aux.push(Auxiliary {
                time: land,
                foot: Foot::at(d),
                panel: 5,
                kind: RestoreKind::MineAvoid,
                end_tick: mine.tick,
            });
            s.last[d] = land;
            s.panels[d] = 5;
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
                    from: 5,
                    to: source,
                    lift_m: self.clearance_lift(
                        5,
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
        let (heading, span) = shape(s.panels);
        if !legal(heading, span) {
            return None;
        }
        s.cost += square((heading - s.heading) / 45.) * 0.25;
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
                    .find(|n| !n.mine && !n.lift && !n.roll_refresh && panel(n) == source)
            {
                let t = n.time - self.origin;
                if t >= last_hazard + 0.1099 && t - last_hazard <= 0.170 {
                    restored = t;
                }
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
            s.last[d] = restored;
            s.cost += 0.6;
        }
        let (heading, span) = shape(s.panels);
        if !legal(heading, span) {
            return None;
        }
        s.cost += square((heading - s.heading) / 45.) * 0.25;
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
                for swing in s.tracks[d].backwards() {
                    initial = swing.from;
                    if swing.release > until {
                        continue;
                    }
                    if swing.to == panel(m) && swing.land <= hi && end >= lo {
                        return false;
                    }
                    end = end.min(swing.release);
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
pub(crate) fn validate_auxiliary(
    input: &Input,
    p: &ContactPlan,
    a: &Auxiliary,
) -> Result<(), String> {
    let track = if a.foot == Foot::L {
        &p.tracks.left
    } else {
        &p.tracks.right
    };
    let swing = track
        .iter()
        .find(|s| s.land == a.time && s.to == a.panel)
        .ok_or("mine clearance has no physical landing")?;
    if !input.notes.iter().any(|n| {
        let t = n.time - p.source_music_origin_seconds;
        n.judgeable
            && n.mine
            && n.tick == a.end_tick
            && panel(n) == swing.from
            && ((swing.to == swing.from && swing.release <= t - WINDOW && swing.land >= t + WINDOW)
                || (input.layout == "dance-single" && a.panel == 5 && (a.time - t).abs() < 1e-9)
                || (swing.release <= t - WINDOW
                    && a.time >= t
                    && a.time <= t + 0.170
                    && input.notes.iter().any(|head| {
                        head.judgeable
                            && !head.mine
                            && !head.lift
                            && !head.roll_refresh
                            && panel(head) == a.panel
                            && (head.time - p.source_music_origin_seconds - a.time).abs() < 1e-9
                    })))
    }) {
        return Err("air-time clearance does not span an original hazard".into());
    }
    Ok(())
}
