// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Scored releases have explicit planted preparation and unscored return.
use super::*;

impl Search {
    pub(super) fn lift_destination(&self, s: &State, source: u8, foot: usize) -> Option<u8> {
        if self.layout == "danoni-9A" {
            let offset = crate::pad_offset(source);
            let mut choices = [offset + 1, offset + 3, offset + 7, offset + 9];
            choices.sort_by(|a, b| {
                self.distance(*a, source, foot)
                    .total_cmp(&self.distance(*b, source, foot))
                    .then(a.cmp(b))
            });
            return choices.into_iter().find(|&p| {
                let mut stance = s.panels;
                stance[foot] = p;
                let (h, span) = self.shape(stance);
                self.legal(stance, h, span)
            });
        }
        let preferred = crate::lift_escape(&self.layout, source, foot);
        if !self.rules.double_pump {
            return Some(preferred);
        }
        let offset = crate::pad_offset(source);
        let alternate = offset + if (preferred - 1) % 9 + 1 == 4 { 6 } else { 4 };
        [preferred, alternate].into_iter().find(|&p| {
            let mut stance = s.panels;
            stance[foot] = p;
            let (h, span) = self.shape(stance);
            self.legal(stance, h, span)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lift_contact(
        &self,
        s: &mut State,
        n: &InputNote,
        d: usize,
        time: f64,
        next: f64,
        direct_landing: bool,
        force_plant: bool,
    ) -> Option<()> {
        let target = panel(n);
        let ls = limits(time - s.last[d], self.options.adaptive_timing);
        if time < self.release_floor() || s.holds[d] - self.options.hold_tail_grace > time + 1e-9 {
            return None;
        }
        if s.panels[d] != target || force_plant {
            let plant = time - 0.080;
            let release = (plant - 0.200)
                .max(s.last[d] + ls.0)
                .max(
                    s.holds[d] + if s.holds[d] > 0. { 0.010 } else { 0. }
                        - self.options.hold_tail_grace,
                )
                .max(self.release_floor());
            if plant - release < ls.1 - 1e-9 || !self.allowed(target, plant) {
                return None;
            }
            let dist = self.distance(target, s.panels[d], d);
            s.tracks[d].push(Swing {
                release,
                land: plant,
                from: s.panels[d],
                to: target,
                lift_m: lift(dist, false, plant - release, self.options.adaptive_timing),
            });
            s.aux.push(Auxiliary {
                time: plant,
                foot: Foot::at(d),
                panel: target,
                kind: RestoreKind::LiftPlant,
                end_tick: n.tick,
            });
            s.last[d] = plant;
            s.panels[d] = target;
            s.cost += square(dist / 18.) * 0.65;
        }
        if time - s.last[d] < ls.0 - 1e-9 {
            return None;
        }
        let (heading, span) = self.shape(s.panels);
        if !self.legal(s.panels, heading, span) {
            return None;
        }
        let duration = 0.140_f64.min((next - time - ls.0).max(ls.1));
        // Preserve the ordinary return. Offer one bounded alternative when
        // its intermediate landing would leave too little time to take off
        // again for the next same-panel head. The other finger can land on
        // center for support while this one continues its original flight.
        let direct = direct_landing
            .then(|| self.rows.get(self.row + 1))
            .flatten()
            .into_iter()
            .flatten()
            .find(|head| {
                let gap = head.time - self.origin - time;
                head.is_head()
                    && self.rules.has_panel(head, target)
                    && gap >= ls.1
                    && gap <= 0.45
                    && gap - duration < ls.0 + ls.1
                    && self.allowed(target, head.time - self.origin)
            });
        if self.rules.same_button(s.panels[1 - d], target) {
            let other = 1 - d;
            if !self.rules.scored_center || s.holds[other] > time || time - s.last[other] < 0.055 {
                return None;
            }
            // A shared tile releases only when both contacts leave together.
            let clear = self.lift_destination(s, s.panels[other], other)?;
            let companion_land = if self.layout == "danoni-9B" {
                direct.map_or(time + duration, |head| head.time - self.origin + 0.060)
            } else {
                time + duration
            };
            s.tracks[other].push(Swing {
                release: time,
                land: companion_land,
                from: s.panels[other],
                to: clear,
                lift_m: 0.035,
            });
            s.aux.push(Auxiliary {
                time: companion_land,
                foot: Foot::at(other),
                panel: clear,
                kind: RestoreKind::LiftClear,
                end_tick: n.tick,
            });
            s.last[other] = companion_land;
            s.panels[other] = clear;
            s.holds[other] = 0.;
            s.cost += 0.5;
        }
        let (clear, duration) = if let Some(head) = direct {
            (target, head.time - self.origin - time)
        } else {
            (self.lift_destination(s, target, d)?, duration)
        };
        let dist = self.distance(clear, target, d);
        s.tracks[d].push(Swing {
            release: time,
            land: time + duration,
            from: target,
            to: clear,
            lift_m: lift(dist, false, duration, self.options.adaptive_timing),
        });
        s.events.push(Event {
            time,
            music_time: n.time,
            tick: n.tick,
            panel: target,
            foot: Foot::at(d),
            lift: true,
        });
        s.aux.push(Auxiliary {
            time: time + duration,
            foot: Foot::at(d),
            panel: clear,
            kind: RestoreKind::LiftClear,
            end_tick: n.tick,
        });
        s.panels[d] = clear;
        s.last[d] = time + duration;
        s.holds[d] = 0.;
        s.cost += 0.5 + square(dist / 18.) * 0.65;
        Some(())
    }
}

pub(crate) fn validate_auxiliary(
    input: &Input,
    p: &ContactPlan,
    a: &Auxiliary,
) -> Result<(), String> {
    let tr = p.tracks.of(a.foot);
    let matches = input.notes.iter().any(|n| {
        if !n.judgeable || !n.lift || n.tick != a.end_tick {
            return false;
        }
        let t = n.time - p.source_music_origin_seconds;
        match a.kind {
            RestoreKind::LiftPlant => {
                note_has_panel(&p.layout, n, a.panel)
                    && a.time <= t - 0.020
                    && tr.iter().any(|s| s.land == a.time && s.to == a.panel)
            }
            RestoreKind::LiftClear => {
                ((if p.layout == "danoni-9A" {
                    crate::support_tiles(&p.layout).contains(&a.panel)
                } else if double_pump(&p.layout) {
                    let offset = crate::pad_offset(panel(n));
                    [offset + 4, offset + 6].contains(&a.panel)
                } else {
                    a.panel == crate::lift_escape(&p.layout, panel(n), a.foot.index())
                }) || (note_has_panel(&p.layout, n, a.panel)
                    && a.time - t >= 0.025
                    && a.time - t <= 0.45
                    && p.events.iter().any(|e| {
                        !e.lift && e.foot == a.foot && e.panel == a.panel && e.time == a.time
                    })))
                    && tr.iter().any(|s| {
                        s.release == t
                            && note_has_panel(&p.layout, n, s.from)
                            && s.land == a.time
                            && s.to == a.panel
                    })
            }
            _ => false,
        }
    });
    if matches {
        Ok(())
    } else {
        Err("lift auxiliary does not match an original release".into())
    }
}
