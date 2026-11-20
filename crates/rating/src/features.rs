//! Row features in the order used to fit single-pass-rating-v1.
use ddi_chart::{NoteKind, Song};
use ddi_engine::{
    cpu::{FullPlan, RowDemand},
    player::{PlayOptions, Player},
    rules::ddi::{self, DdiOptions, Difficulty},
};
use std::collections::{BTreeMap, VecDeque};

#[derive(Default)]
struct Group {
    seconds: f64,
    masks: [u8; 5],
    durations: [f64; 4],
}

pub(super) struct Row {
    pub time: f64,
    pub heads: f64,
    pub near: f64,
    pub demand: RowDemand,
    pub units: f64,
    pub longest: f64,
    pub mine_near: f64,
    pub mine_inside: f64,
    pub channels: [f64; 25],
}

pub(super) struct Chart {
    pub rows: Vec<Row>,
    pub mines: usize,
    pub duration: f64,
    pub(super) source_notes: usize,
    has_lifts: bool,
    pub readouts_studied: bool,
    pub matched_profile_studied: bool,
    pub units: f64,
    pub heads: f64,
}

impl Chart {
    pub(super) fn qualifies_scalar(&self, decimal: f64) -> bool {
        if self.heads < 48.0 || self.has_lifts {
            return false;
        }
        if self.source_notes <= 1200 {
            return (15.0..=180.0).contains(&self.duration);
        }
        if !(15.0..=600.0).contains(&self.duration) || self.source_notes > 8000 {
            return false;
        }
        // Only the upper plateau of the frozen mapping qualified in this band.
        if self.source_notes > 4800 && decimal != 19.0 {
            return false;
        }

        // Frozen long-chart rule. Density counts playable heads, including
        // hold and roll heads, and excludes mines.
        let duration = (self.duration.ln() - 5.3788065525642725) / 0.4897688972455666;
        let density = ((self.heads / self.duration).ln() - 2.0491978721387354) / 0.5077202437864153;
        let risk =
            0.04756054481605719 - 0.04640558484658968 * duration - 0.07233864496876079 * density;
        risk <= 0.0774651151114101
    }
}

pub(super) fn extract(song: &Song, index: usize) -> Result<Chart, String> {
    let chart = song.charts.get(index).ok_or("missing chart")?;
    if chart.layout != "dance-single" || chart.derivation.is_some() {
        return Err("estimate requires an original dance-single chart".into());
    }
    if chart.notes.len() > 8192 {
        return Err("estimate accepts up to 8,192 source notes".into());
    }
    let plan = FullPlan::new(song, index)?;
    let player = Player::new(
        song,
        index,
        ddi::ruleset(&DdiOptions {
            difficulty: Difficulty::Normal,
            practice: true,
            boos: false,
        }),
        PlayOptions::default(),
    );
    let mut groups = BTreeMap::<_, Group>::new();
    let mut first = f64::INFINITY;
    let mut last = f64::NEG_INFINITY;
    let mut lifts = false;
    let mut last_head = None;
    for n in player.judge().notes().iter().filter(|n| n.judgeable) {
        let kind = match n.note.kind {
            NoteKind::Tap => 0,
            NoteKind::HoldHead { .. } => 1,
            NoteKind::RollHead { .. } => 2,
            NoteKind::Lift => 3,
            NoteKind::Mine | NoteKind::Shock => 4,
            _ => continue,
        };
        let lane = usize::from(n.note.lane);
        if lane >= 4
            || !n.seconds.is_finite()
            || !n.end_seconds.is_finite()
            || n.seconds < -60.
            || n.end_seconds > 1800.
        {
            return Err("note outside the teacher-v1 domain".into());
        }
        first = first.min(n.seconds);
        last = last.max(n.end_seconds);
        let g = groups.entry(n.note.tick).or_default();
        if g.masks.iter().any(|m| m & (1 << lane) != 0) {
            return Err("duplicate lane on one tick".into());
        }
        if kind < 4 {
            if last_head.is_some_and(|(tick, t)| tick != n.note.tick && n.seconds <= t) {
                return Err("non-increasing head times".into());
            }
            last_head = Some((n.note.tick, n.seconds));
        }
        g.seconds = n.seconds;
        g.masks[kind] |= 1 << lane;
        if kind == 1 || kind == 2 {
            g.durations[lane] = n.end_seconds - n.seconds;
        }
        lifts |= kind == 3;
    }
    let duration = last - first;
    if duration <= 0.0 {
        return Err("estimate requires positive chart duration".into());
    }
    let mut groups: Vec<_> = groups.into_values().collect();
    groups.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    let mut times = [const { Vec::new() }; 4];
    for g in &groups {
        let mask = g.masks[..4].iter().fold(0, |a, b| a | b);
        for (lane, list) in times.iter_mut().enumerate() {
            if mask & (1 << lane) != 0 {
                list.push(g.seconds);
            }
        }
    }
    let mut rows = Vec::with_capacity(groups.len());
    let mut demands = plan.demands().iter();
    let mut active = [f64::NEG_INFINITY; 4];
    let mut previous = None;
    let mut previous_head = None;
    let mut last_mask = 0;
    let mut recent = VecDeque::<(f64, u32)>::new();
    let mut near = 0;
    let mut total_units = 0.;
    let mut total_heads = 0.;
    for g in groups {
        let t = g.seconds;
        while recent.front().is_some_and(|(old, _)| *old < t - 1.) {
            near -= recent.pop_front().unwrap().1;
        }
        let [taps, holds, rolls, lift, mines] = g.masks.map(|m| f64::from(m.count_ones()));
        let mask = g.masks[..4].iter().fold(0, |a, b| a | b);
        let h = mask.count_ones();
        let heads = f64::from(h);
        let dt = previous.map_or(0., |p: f64| t - p);
        previous = Some(t);
        let mut rate = 0.;
        let demand = if h > 0 {
            let d = *demands
                .next()
                .filter(|d| u32::from(d.count) == h)
                .ok_or("mismatched CPU row demands")?;
            rate = previous_head.map_or(0., |p: f64| (1. / (t - p)).min(20.));
            previous_head = Some(t);
            d
        } else {
            RowDemand {
                speed: 0.,
                sustain: 0.,
                lane: 0.,
                irregular: 0.,
                count: 0,
                held: 0,
            }
        };
        let mut min_gap = 1.0_f64;
        let mut mine_near = 0.;
        let mut mine_inside = 0.;
        for (lane, ts) in times.iter().enumerate() {
            if g.masks[4] & (1 << lane) != 0 {
                let j = ts.partition_point(|x| *x < t);
                let mut gap = 1.0_f64;
                if let Some(x) = ts.get(j) {
                    gap = gap.min(x - t);
                }
                if j > 0 {
                    gap = gap.min(t - ts[j - 1]);
                }
                min_gap = min_gap.min(gap);
                mine_near += f64::from(gap <= 0.5);
                mine_inside += f64::from(active[lane] > t);
            }
            if g.durations[lane] > 0. {
                active[lane] = t + g.durations[lane];
            }
        }
        let repeats = f64::from((mask & last_mask).count_ones());
        let channels = [
            heads,
            taps,
            holds,
            rolls,
            lift,
            mines,
            g.durations.iter().sum(),
            f64::from(near),
            demand.sustain,
            rate,
            demand.lane,
            demand.irregular,
            demand.speed,
            f64::from(demand.held),
            repeats,
            mine_near,
            mine_inside,
            min_gap,
        ];
        let mut features = [0.; 25];
        for (i, x) in channels.into_iter().enumerate() {
            features[i] = x.max(0.).ln_1p();
        }
        features[18..].copy_from_slice(&[
            f64::from(h == 1),
            f64::from(h == 2),
            f64::from(h >= 3),
            f64::from(rate > 8.),
            f64::from(demand.lane > 6.),
            f64::from(h > 1 && demand.lane > 5.),
            dt.min(2.),
        ]);
        let units = g
            .durations
            .iter()
            .map(|d| ((d * 1e6).round_ties_even() / 1e6).ceil())
            .sum();
        for (lane, d) in g.durations.iter().enumerate() {
            if (g.masks[1] | g.masks[2]) & (1 << lane) != 0 {
                total_units += ((d * 1e6).round() / 1e6).ceil().max(1.);
            }
        }
        rows.push(Row {
            time: t,
            heads,
            near: f64::from(near),
            demand,
            units,
            longest: g.durations.into_iter().fold(0., f64::max),
            mine_near,
            mine_inside,
            channels: features,
        });
        if h > 0 {
            recent.push_back((t, h));
            near += h;
            last_mask = mask;
        }
        total_heads += heads;
    }
    if demands.next().is_some() || total_heads == 0. {
        return Err("chart has inconsistent or empty head rows".into());
    }
    let short_domain = total_heads >= 48. && !lifts && (15.0..=180.).contains(&duration);
    let notes = chart.notes.len();
    let mines = plan.counts().2;
    let readouts_studied = short_domain && notes <= 900;
    Ok(Chart {
        rows,
        mines,
        duration,
        heads: total_heads,
        units: total_units,
        source_notes: notes,
        has_lifts: lifts,
        readouts_studied,
        matched_profile_studied: readouts_studied || (short_domain && mines == 0 && notes <= 1200),
    })
}
