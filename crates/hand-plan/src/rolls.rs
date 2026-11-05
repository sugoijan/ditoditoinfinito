// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Unscored roll contacts. Source heads and tails keep their exact identity.
use super::*;
const PERIOD: f64 = 0.180;
const HEAD_GUARD: f64 = 0.190;

pub(super) fn refreshes(
    input: &Input,
    known: f64,
    after: f64,
    adaptive: bool,
) -> Result<(Vec<InputNote>, usize), String> {
    let mut out = Vec::new();
    let mut skipped = 0;
    for head in input
        .notes
        .iter()
        .filter(|n| n.judgeable && n.roll && !n.roll_refresh)
    {
        let Some((_, end)) = head.end else {
            return Err("roll has no tail".into());
        };
        if end <= head.time {
            return Err("invalid roll tail".into());
        }
        let until = end.min(known);
        let hazards = input
            .notes
            .iter()
            .filter(|n| {
                n.judgeable && n.mine && n.lane == head.lane && n.time > head.time && n.time < end
            })
            .collect::<Vec<_>>();
        let following = input
            .notes
            .iter()
            .filter(|n| {
                n.judgeable
                    && !n.mine
                    && !n.roll_refresh
                    && n.lane == head.lane
                    && n.time > head.time
                    // Keep the early-hit guard only if its replacement can
                    // occur before the tail. Otherwise it erases a needed
                    // refresh and then discards the replacement as too late.
                    && n.time - HEAD_GUARD < end - 0.020
            })
            .collect::<Vec<_>>();
        let mut times = Vec::new();
        let first =
            (((after - 1. - head.time) / PERIOD).floor().max(0.) as usize).saturating_add(1);
        let last = ((until - head.time) / PERIOD).floor().max(0.) as usize;
        let count = last.saturating_sub(first).saturating_add(1);
        if count > 10000 || out.len() + count > 10000 {
            return Err("roll refresh limit".into());
        }
        for i in first..=last {
            times.push((head.time + i as f64 * PERIOD, false));
        }
        for m in &hazards {
            let a = m.time - 0.130;
            let b = m.time + 0.110;
            times.retain(|(t, _)| *t < a - 0.080 || *t > b + 0.080);
            times.extend([(a, true), (b, true)]);
        }
        for n in &following {
            let last = n.time - HEAD_GUARD;
            times.retain(|(t, _)| *t < last - 0.080 || *t >= n.time);
            times.push((last, true));
        }
        times.retain(|(t, _)| {
            *t > head.time
                && *t > after - 1.
                && *t < end - 0.020
                && *t <= known
                && !hazards.iter().any(|m| (*t - m.time).abs() < 0.100)
                && !following
                    .iter()
                    .any(|n| *t > n.time - 0.182 && *t <= n.time)
        });
        times.sort_by(|a, b| a.0.total_cmp(&b.0));
        times.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
        let mut previous = head.time + (first - 1) as f64 * PERIOD;
        for (t, _) in times {
            // Final recovery may omit a redundant tail refresh. A short roll
            // can finish from its preceding press, freeing that finger for a
            // scored transfer. The actual judge replay validates the budget.
            let tail_budget = input
                .rules
                .iter()
                .map(|(_, rule)| rule.roll_window_seconds())
                .fold(0.250, f64::min)
                - 0.001;
            if adaptive && t > end - tail_budget && end - previous <= tail_budget {
                skipped += usize::from(t > after);
                continue;
            }
            // Fill a locally widened gap without rephasing earlier contacts.
            if t - previous > 0.245 {
                let mid = (t + previous) * 0.5;
                if !hazards.iter().any(|m| (mid - m.time).abs() < 0.100)
                    && !following
                        .iter()
                        .any(|n| mid > n.time - 0.182 && mid <= n.time)
                {
                    out.push(InputNote {
                        time: mid,
                        roll_refresh: true,
                        ..*head
                    });
                }
            }
            if t - previous < 0.080 - 1e-9 {
                continue;
            }
            out.push(InputNote {
                time: t,
                roll_refresh: true,
                ..*head
            });
            previous = t;
        }
    }
    Ok((out, skipped))
}

impl Search {
    pub(super) fn roll_contact(
        &self,
        s: &mut State,
        n: &InputNote,
        d: usize,
        land: f64,
    ) -> Option<()> {
        let h = held(s, d, land).filter(|h| {
            h.roll && h.panel == panel(n) && n.end.is_some_and(|(tick, _)| tick == h.end_tick)
        })?;
        if s.panels[d] != panel(n) || s.panels[1 - d] == panel(n) {
            return None;
        }
        if (s.last[d] - land).abs() < 1e-9
            && s.aux.backwards().any(|a| {
                a.kind == RestoreKind::RollRefresh
                    && a.foot.index() == d
                    && (a.time - land).abs() < 1e-9
            })
        {
            return Some(()); // A declared mine-spanning return already lands here.
        }
        let release = (land - 0.060)
            .max(s.last[d] + 0.020)
            .max(self.release_floor());
        if land - release < 0.060 - 1e-9 {
            return None;
        }
        s.tracks[d].push(Swing {
            release,
            land,
            from: panel(n),
            to: panel(n),
            lift_m: 0.012,
        });
        s.aux.push(Auxiliary {
            time: land,
            foot: Foot::at(d),
            panel: panel(n),
            kind: RestoreKind::RollRefresh,
            end_tick: h.end_tick,
        });
        s.last[d] = land;
        s.cost += 0.15;
        Some(())
    }
    pub(super) fn roll_mine(&self, s: &mut State, d: usize, time: f64) -> Option<()> {
        let h = held(s, d, time).filter(|h| h.roll)?;
        let release = time - 0.110;
        let land = time + 0.110;
        if release < self.release_floor() || release < s.last[d] + 0.020 - 1e-9 {
            return None;
        }
        if land >= h.end {
            if h.end - s.last[d] >= 0.250 - 1e-9 {
                return None;
            }
            let mine = self
                .mines
                .iter()
                .find(|n| panel(n) == h.panel && n.time - self.origin == time)?;
            s.tracks[d].push(Swing {
                release,
                land,
                from: h.panel,
                to: h.panel,
                lift_m: self.clearance_lift(h.panel, h.panel, release, land, 0.045)?,
            });
            s.aux.push(Auxiliary {
                time: land,
                foot: Foot::at(d),
                panel: h.panel,
                kind: RestoreKind::MineAvoid,
                end_tick: mine.tick,
            });
            s.last[d] = land;
            s.cost += 0.6;
            return Some(());
        }
        if land - s.last[d] >= 0.250 - 1e-9 {
            return None;
        }
        s.tracks[d].push(Swing {
            release,
            land,
            from: h.panel,
            to: h.panel,
            lift_m: self.clearance_lift(h.panel, h.panel, release, land, 0.045)?,
        });
        s.aux.push(Auxiliary {
            time: land,
            foot: Foot::at(d),
            panel: h.panel,
            kind: RestoreKind::RollRefresh,
            end_tick: h.end_tick,
        });
        s.last[d] = land;
        s.cost += 0.6;
        Some(())
    }
}

pub(super) fn pulses(holds: &[Hold], tracks: &[Vec<Swing>; 2]) -> Vec<ddi_hand_pose::RollPulse> {
    let mut pulses = Vec::new();
    for (d, track) in tracks.iter().enumerate() {
        for s in track {
            if holds
                .iter()
                .any(|h| h.roll && s.to == h.panel && s.land > h.start && s.land < h.end)
            {
                pulses.push(ddi_hand_pose::RollPulse {
                    release: s.release,
                    land: s.land,
                    from: s.from,
                    panel: s.to,
                    foot: if d == 0 { "L".into() } else { "R".into() },
                });
            }
        }
    }
    pulses.sort_by(|a, b| a.land.total_cmp(&b.land).then(a.foot.cmp(&b.foot)));
    pulses
}

pub(crate) fn validate_auxiliary(
    input: &Input,
    p: &ContactPlan,
    a: &Auxiliary,
) -> Result<(), String> {
    let tr = if a.foot == Foot::L {
        &p.tracks.left
    } else {
        &p.tracks.right
    };
    if !input.notes.iter().any(|n| {
        n.judgeable
            && n.roll
            && n.end.is_some_and(|(end, time)| {
                end == a.end_tick
                    && n.time < a.time + p.source_music_origin_seconds
                    && a.time + p.source_music_origin_seconds < time
            })
            && panel(n) == a.panel
    }) || !tr.iter().any(|s| s.land == a.time && s.to == a.panel)
    {
        return Err("roll refresh has no original active roll and landing".into());
    }
    Ok(())
}

pub(crate) fn validate_pulses(p: &ContactPlan) -> Result<(), String> {
    if p.roll_pulses.is_empty() && !p.holds.iter().any(|h| h.roll) {
        if p.aux_contacts
            .iter()
            .flatten()
            .any(|a| a.kind == RestoreKind::RollRefresh)
        {
            return Err("roll refresh lost its source roll".into());
        }
        return Ok(());
    }
    if p.roll_pulses != pulses(&p.holds, &[p.tracks.left.clone(), p.tracks.right.clone()]) {
        return Err("roll motion declarations differ from physical refreshes".into());
    }
    Ok(())
}
