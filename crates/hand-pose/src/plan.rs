use crate::{Model, Parameters, Point};
use serde::Deserialize;
#[derive(Deserialize, Clone)]
pub struct Swing {
    pub release: f64,
    pub land: f64,
    pub from: u8,
    pub to: u8,
    pub lift_m: f64,
}
#[derive(Deserialize, Clone)]
pub struct Tracks {
    #[serde(rename = "L")]
    pub left: Vec<Swing>,
    #[serde(rename = "R")]
    pub right: Vec<Swing>,
}
#[derive(Deserialize, Clone)]
pub struct Initial {
    #[serde(rename = "L")]
    pub left: u8,
    #[serde(rename = "R")]
    pub right: u8,
}
#[derive(Deserialize, Clone)]
pub struct Placement {
    pub tile_pitch_mm: f64,
    pub panel_anchor_mm: f64,
    pub center_offset_mm: f64,
    pub other_offset_mm: f64,
}
#[derive(Deserialize, Clone)]
pub struct Event {
    pub time: f64,
    pub music_time: f64,
    pub tick: i64,
    pub panel: u8,
    pub foot: String,
    #[serde(default)]
    pub lift: bool,
}
/// A received arrow mine, in the Plan's local song seconds.
#[derive(Deserialize, serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct Mine {
    pub time: f64,
    pub panel: u8,
}
/// An actual unscored repress during a roll, with a visible flight and contact.
#[derive(Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct RollPulse {
    pub release: f64,
    pub land: f64,
    pub from: u8,
    pub panel: u8,
    pub foot: String,
}
#[derive(Clone)]
struct CenterTargets {
    initial: [Point; 2],
    landings: [Vec<Point>; 2],
}
#[derive(Deserialize, Clone)]
pub struct Plan {
    pub duration: f64,
    pub initial: Initial,
    pub tracks: Tracks,
    pub placement: Placement,
    pub events: Vec<Event>,
    #[serde(default)]
    pub mines: Vec<Mine>,
    #[serde(default)]
    pub roll_pulses: Vec<RollPulse>,
    #[serde(skip)]
    pub edges: Vec<f64>,
    /// Every track's swings release after the previous one lands and land
    /// after they release (as `load` validates; `rebuild_edges` checks), so
    /// the swings around a time are found by binary search.
    #[serde(skip)]
    ordered: bool,
    #[serde(skip)]
    centered: Option<CenterTargets>,
    #[serde(skip)]
    release_events: Vec<usize>,
}
#[derive(Clone, Copy)]
pub struct Targets {
    pub tips: [Point; 2],
    pub contact: [bool; 2],
}
fn smoother(u: f64) -> f64 {
    u * u * u * (10. + u * (-15. + 6. * u))
}
impl Plan {
    pub fn empty(duration: f64) -> Self {
        Self {
            duration,
            initial: Initial { left: 5, right: 5 },
            tracks: Tracks {
                left: Vec::new(),
                right: Vec::new(),
            },
            placement: Placement {
                tile_pitch_mm: 24.,
                panel_anchor_mm: 15.,
                center_offset_mm: 9.,
                other_offset_mm: 5.5,
            },
            events: Vec::new(),
            mines: Vec::new(),
            roll_pulses: Vec::new(),
            edges: vec![0., duration],
            // Swings pushed later are scanned as before until edges are rebuilt.
            ordered: false,
            centered: None,
            release_events: Vec::new(),
        }
    }
    pub fn load(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("plan byte limit".into());
        }
        let mut p: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if !p.duration.is_finite()
            || p.duration <= 0.
            || p.duration > 3600.
            || p.events.len() > 10000
            || p.mines.len() > 10000
            || p.roll_pulses.len() > 10000
            || p.mines.iter().any(|m| {
                !matches!(m.panel, 2 | 4 | 5 | 6 | 8)
                    || !m.time.is_finite()
                    || m.time < 0.
                    || m.time > p.duration
            })
        {
            return Err("plan limits".into());
        }
        let valid_panel = |p: u8| matches!(p, 2 | 4 | 5 | 6 | 8);
        if !valid_panel(p.initial.left)
            || !valid_panel(p.initial.right)
            || p.events.iter().any(|e| {
                !valid_panel(e.panel)
                    || !matches!(e.foot.as_str(), "L" | "R")
                    || !e.time.is_finite()
                    || e.time < 0.
                    || e.time > p.duration
            })
        {
            return Err("invalid panel/event".into());
        }
        p.edges = vec![0., p.duration];
        for track in [&p.tracks.left, &p.tracks.right] {
            let mut last = 0.;
            for s in track {
                if !(valid_panel(s.from)
                    && valid_panel(s.to)
                    && s.lift_m >= 0.
                    && last <= s.release
                    && s.release < s.land
                    && s.land <= p.duration
                    && s.lift_m.is_finite())
                {
                    return Err("invalid swing".into());
                }
                last = s.land;
                p.edges.extend([s.release, s.land]);
            }
        }
        p.edges.sort_by(f64::total_cmp);
        p.edges.dedup();
        if p.roll_pulses.iter().any(|r| {
            !matches!(r.foot.as_str(), "L" | "R")
                || !valid_panel(r.from)
                || !matches!(r.panel, 2 | 4 | 5 | 6 | 8)
                || !r.release.is_finite()
                || !r.land.is_finite()
                || r.release < 0.
                || r.release >= r.land
                || r.land > p.duration
                || !(if r.foot == "L" {
                    &p.tracks.left
                } else {
                    &p.tracks.right
                })
                .iter()
                .any(|s| {
                    s.release == r.release
                        && s.land == r.land
                        && s.from == r.from
                        && s.to == r.panel
                })
        }) {
            return Err("invalid roll pulse".into());
        }
        if p.events.iter().any(|e| {
            e.lift
                && p.lift_interval(e)
                    .is_none_or(|[a, b]| a < 0. || b > p.duration)
        }) {
            return Err("invalid lift release".into());
        }
        p.ordered = true;
        p.release_events = p
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.lift)
            .map(|(i, _)| i)
            .collect();
        Ok(p)
    }
    pub fn rebuild_edges(&mut self) {
        self.release_events = self
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.lift)
            .map(|(i, _)| i)
            .collect();
        self.edges = vec![0., self.duration];
        for tr in [&self.tracks.left, &self.tracks.right] {
            for s in tr {
                self.edges.extend([s.release, s.land]);
            }
        }
        self.edges.sort_by(f64::total_cmp);
        self.edges.dedup();
        self.ordered = [&self.tracks.left, &self.tracks.right].iter().all(|tr| {
            tr.iter().all(|s| s.release < s.land)
                && tr.windows(2).all(|w| w[0].land <= w[1].release)
        });
    }
    fn track(&self, d: usize) -> (&[Swing], u8) {
        if d == 0 {
            (&self.tracks.left, self.initial.left)
        } else {
            (&self.tracks.right, self.initial.right)
        }
    }
    /// The panel finger `d` stands on, or last stood on, at `t`: the last
    /// swing landed by then, in track order up to the first that has not.
    pub fn panel_at(&self, d: usize, t: f64) -> u8 {
        let (track, initial) = self.track(d);
        let k = if self.ordered {
            track.partition_point(|s| s.land <= t)
        } else {
            track.iter().position(|s| t < s.land).unwrap_or(track.len())
        };
        k.checked_sub(1).map_or(initial, |k| track[k].to)
    }
    pub fn visible(&self, end: f64) -> Self {
        let mut p = self.clone();
        p.duration = end;
        p.events.retain(|e| e.time <= end);
        p.release_events = p
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.lift)
            .map(|(i, _)| i)
            .collect();
        p.mines.retain(|m| m.time - 0.09 <= end);
        p.roll_pulses.retain(|r| r.release <= end);
        // A received release already declares its outgoing flight. Its landing
        // may be just beyond the watermark; discarding it would leave a scored
        // release with no physical swing. Future scored landings stay hidden.
        let mut releases: [Vec<f64>; 2] = std::array::from_fn(|d| {
            p.release_events
                .iter()
                .map(|i| &p.events[*i])
                .filter(|e| usize::from(e.foot == "R") == d)
                .map(|e| e.time)
                .collect()
        });
        for r in &p.roll_pulses {
            releases[usize::from(r.foot == "R")].push(r.release);
        }
        // A known hazard declares a whole clearing flight, even if its return
        // crosses the input watermark. Likewise a received shared-tile head
        // declares its companion's delayed landing. Neither reveals a future hit.
        for (d, track) in [&p.tracks.left, &p.tracks.right].into_iter().enumerate() {
            for s in track.iter().filter(|s| s.release <= end) {
                if p.events
                    .iter()
                    .any(|e| e.lift && e.time == s.release && e.panel == s.from)
                    || p.mines.iter().any(|m| {
                        m.time <= end && m.panel == s.from && s.release < m.time && s.land > m.time
                    })
                    || p.events.iter().any(|e| {
                        usize::from(e.foot == "R") != d
                            && !e.lift
                            && e.panel == s.from
                            && (s.land - e.time - 0.060).abs() < 1e-9
                    })
                {
                    releases[d].push(s.release);
                }
            }
        }
        p.tracks
            .left
            .retain(|s| s.land <= end || releases[0].contains(&s.release));
        p.tracks
            .right
            .retain(|s| s.land <= end || releases[1].contains(&s.release));
        p.rebuild_edges();
        p.edges.retain(|t| *t <= end);
        p
    }
    /// A floor contact inside a live mine is forbidden, including stream motion.
    pub(crate) fn mine_contact(&self, t: f64, tips: &[Point; 2]) -> bool {
        self.mines
            .iter()
            .filter(|m| (t - m.time).abs() <= 0.09)
            .any(|m| {
                let (x, y) = match m.panel {
                    4 => (-self.placement.tile_pitch_mm, 0.),
                    2 => (0., -self.placement.tile_pitch_mm),
                    8 => (0., self.placement.tile_pitch_mm),
                    5 => (0., 0.),
                    _ => (self.placement.tile_pitch_mm, 0.),
                };
                tips.iter().any(|p| {
                    p[2] <= 0.25 && (p[0] + 16. - x).abs() <= 11.5 && (p[1] - y).abs() <= 11.5
                })
            })
    }
    /// Merge overlapping danger windows before adding a 480 Hz validation grid.
    /// Shared times are deduplicated by the preparation/publication checker.
    pub(crate) fn mine_probes(&self, start: f64, end: f64) -> Vec<f64> {
        let mut spans = Vec::<[f64; 2]>::new();
        let mut times = Vec::new();
        for m in &self.mines {
            let a = (m.time - 0.09).max(start);
            let b = (m.time + 0.09).min(end);
            if a > b {
                continue;
            }
            times.extend([a, b]);
            if m.time >= start && m.time <= end {
                times.push(m.time);
            }
            spans.push([a, b]);
        }
        spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut merged = Vec::<[f64; 2]>::new();
        for [a, b] in spans {
            if let Some(last) = merged.last_mut()
                && a <= last[1]
            {
                last[1] = last[1].max(b);
            } else {
                merged.push([a, b]);
            }
        }
        for [a, b] in merged {
            times.extend(
                ((a * 480.).ceil() as usize..=(b * 480.).floor() as usize).map(|i| i as f64 / 480.),
            );
        }
        times
    }
    /// Historical nominal anchor. Use `targets(time)` for contact-specific spots.
    pub fn anchor(&self, p: u8, d: usize) -> Point {
        let scale = self.placement.panel_anchor_mm;
        let (x, y) = match p {
            2 => (0., -scale),
            4 => (-scale, 0.),
            5 => (0., 0.),
            6 => (scale, 0.),
            8 => (0., scale),
            _ => unreachable!("checked panel"),
        };
        let offset = if p == 5 {
            self.placement.center_offset_mm
        } else {
            self.placement.other_offset_mm
        };
        let center = match p {
            4 => -self.placement.tile_pitch_mm,
            6 => self.placement.tile_pitch_mm,
            _ => 0.,
        };
        let half = self.placement.tile_pitch_mm * 0.47 - 1.;
        let x = (x + if d == 0 { -offset } else { offset }).clamp(center - half, center + half);
        [x - 16., y, 0.]
    }
    /// Center isolated contacts. Shared planted intervals retain separated
    /// spots (18 mm apart at center, 11 mm on arrows). Each spot belongs to a
    /// whole contact, never the current frame: another finger lifting must not
    /// slide an already planted tip.
    /// Reused published flights retain both endpoints across rolling windows.
    pub fn center_contacts(&mut self, previous: Option<(&Self, f64)>) {
        let tracks = [&self.tracks.left, &self.tracks.right];
        let panels = [self.initial.left, self.initial.right];
        let spans: [Vec<(f64, f64, u8)>; 2] = std::array::from_fn(|d| {
            let mut v = vec![(
                f64::NEG_INFINITY,
                tracks[d].first().map_or(f64::INFINITY, |s| s.release),
                panels[d],
            )];
            v.extend(tracks[d].iter().enumerate().map(|(i, s)| {
                (
                    s.land,
                    tracks[d].get(i + 1).map_or(f64::INFINITY, |s| s.release),
                    s.to,
                )
            }));
            v
        });
        let points: [Vec<Point>; 2] = std::array::from_fn(|d| {
            let mut other_at = 0;
            spans[d]
                .iter()
                .map(|&(a, b, panel)| {
                    let mut p = self.tile_center(panel);
                    while other_at < spans[1 - d].len() && spans[1 - d][other_at].1 <= a {
                        other_at += 1;
                    }
                    if spans[1 - d][other_at..]
                        .iter()
                        .take_while(|&&(x, _, _)| x < b)
                        .any(|&(x, y, other)| panel == other && a.max(x) < b.min(y) - 1e-9)
                    {
                        let offset = if panel == 5 {
                            self.placement.center_offset_mm
                        } else {
                            self.placement.other_offset_mm
                        };
                        p[0] += if d == 0 { -offset } else { offset };
                    }
                    p
                })
                .collect()
        });
        let mut centered = CenterTargets {
            initial: [points[0][0], points[1][0]],
            landings: std::array::from_fn(|d| points[d][1..].to_vec()),
        };
        if let Some((old, until)) = previous {
            let old_tracks = [&old.tracks.left, &old.tracks.right];
            for d in 0..2 {
                let first = tracks[d].first().map_or(until, |s| s.release);
                // Pruning may make an old landing the new initial contact.
                centered.initial[d] = old.targets(first.min(until).max(0.)).tips[d];
                centered.initial[d][2] = 0.;
                for (i, s) in tracks[d]
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.release <= until)
                {
                    let j = old_tracks[d].partition_point(|v| v.release < s.release);
                    if old_tracks[d].get(j).is_some_and(|v| {
                        v.release == s.release
                            && v.land == s.land
                            && v.from == s.from
                            && v.to == s.to
                    }) {
                        centered.landings[d][i] = old.landing_anchor(d, j);
                    }
                }
            }
        }
        self.centered = Some(centered);
    }
    pub fn tile_center(&self, panel: u8) -> Point {
        let pitch = self.placement.tile_pitch_mm;
        match panel {
            2 => [-16., -pitch, 0.],
            4 => [-pitch - 16., 0., 0.],
            5 => [-16., 0., 0.],
            6 => [pitch - 16., 0., 0.],
            8 => [-16., pitch, 0.],
            _ => unreachable!("checked panel"),
        }
    }
    fn landing_anchor(&self, d: usize, i: usize) -> Point {
        self.centered.as_ref().map_or_else(
            || {
                let track = if d == 0 {
                    &self.tracks.left
                } else {
                    &self.tracks.right
                };
                self.anchor(track[i].to, d)
            },
            |c| c.landings[d][i],
        )
    }
    /// Geometry probes bracket a scored release, without moving its exact input edge.
    pub fn lift_interval(&self, e: &Event) -> Option<[f64; 2]> {
        if !e.lift {
            return None;
        }
        let track = if e.foot == "L" {
            &self.tracks.left
        } else {
            &self.tracks.right
        };
        let s = track
            .iter()
            .find(|s| s.release == e.time && s.from == e.panel)?;
        Some([
            e.time - 0.010,
            e.time + (0.35 * (s.land - s.release)).min(0.050),
        ])
    }
    pub(crate) fn release_probes(&self, start: f64, end: f64) -> Vec<f64> {
        self.release_events
            .iter()
            .flat_map(|i| self.lift_interval(&self.events[*i]))
            .flatten()
            .filter(|t| *t >= start && *t <= end)
            .collect()
    }
    pub(crate) fn roll_probes(&self, start: f64, end: f64) -> Vec<f64> {
        self.roll_pulses
            .iter()
            .flat_map(|r| [r.release - 0.005, (r.release + r.land) * 0.5, r.land])
            .filter(|t| *t >= start && *t <= end)
            .collect()
    }
    pub(crate) fn roll_violation(&self, t: f64, tips: &[Point; 2]) -> bool {
        self.roll_pulses.iter().any(|r| {
            let d = usize::from(r.foot == "R");
            let p = tips[d];
            let anchor = if (t - (r.release - 0.005)).abs() < 1e-9 || (t - r.land).abs() < 1e-9 {
                Some(self.targets(t).tips[d])
            } else {
                None
            };
            anchor.is_some_and(|a| p[2].abs() > 0.25 || (p[0] - a[0]).hypot(p[1] - a[1]) > 2.)
                || ((t - (r.release + r.land) * 0.5).abs() < 1e-9 && p[2] <= 0.25)
        })
    }
    pub(crate) fn lift_violation(&self, t: f64, tips: &[Point; 2]) -> bool {
        self.release_events.iter().any(|i| {
            let e = &self.events[*i];
            let Some([a, b]) = self.lift_interval(e) else {
                return true;
            };
            let d = usize::from(e.foot == "R");
            let p = tips[d];
            let anchor = self.targets(a).tips[d];
            ((t - a).abs() < 1e-9
                && (p[2].abs() > 0.25 || (p[0] - anchor[0]).hypot(p[1] - anchor[1]) > 2.))
                || ((t - b).abs() < 1e-9 && p[2] <= 0.25)
        })
    }
    pub fn targets(&self, t: f64) -> Targets {
        let t = if t.is_finite() {
            t.clamp(0., self.duration)
        } else {
            0.
        };
        let mut out = Targets {
            tips: [[0.; 3]; 2],
            contact: [true; 2],
        };
        for d in 0..2 {
            let track = if d == 0 {
                &self.tracks.left
            } else {
                &self.tracks.right
            };
            let initial = if d == 0 {
                self.initial.left
            } else {
                self.initial.right
            };
            // Only the last assignment of the original scan survives, so only
            // that anchor is computed. The scan stops at the first swing not
            // landed by `t` (in flight, or not yet released); on an ordered
            // plan that is the first landing after `t`.
            let k = if self.ordered {
                track.partition_point(|s| s.land <= t)
            } else {
                track.iter().take_while(|s| s.land <= t).count()
            };
            let a = if k > 0 {
                self.landing_anchor(d, k - 1)
            } else {
                self.centered
                    .as_ref()
                    .map_or_else(|| self.anchor(initial, d), |c| c.initial[d])
            };
            if let Some(s) = track.get(k).filter(|s| t >= s.release) {
                let u = ((t - s.release) / (s.land - s.release)).clamp(0., 1.);
                let w = smoother(u);
                let b = self.landing_anchor(d, k);
                out.tips[d] = std::array::from_fn(|i| a[i] * (1. - w) + b[i] * w);
                out.tips[d][2] = s.lift_m * 200. * 64. * u.powi(3) * (1. - u).powi(3);
                out.contact[d] = false;
            } else {
                out.tips[d] = a;
            }
        }
        out
    }
    pub fn support_u(&self, t: f64) -> f64 {
        let t = if t.is_finite() {
            t.clamp(0., self.duration)
        } else {
            0.
        };
        let i = self
            .edges
            .partition_point(|v| *v <= t)
            .saturating_sub(1)
            .min(self.edges.len() - 2);
        let state = |j: usize| {
            self.targets((self.edges[j] + self.edges[j + 1]) * 0.5)
                .contact
        };
        let c = state(i);
        if c[0] ^ c[1] {
            return if c[1] { 1. } else { 0. };
        }
        let single = |j: Option<usize>| {
            j.filter(|j| *j < self.edges.len() - 1)
                .and_then(|j| {
                    let c = state(j);
                    if c[0] ^ c[1] {
                        Some(if c[1] { 1. } else { 0. })
                    } else {
                        None
                    }
                })
                .unwrap_or(0.5)
        };
        let a = single(i.checked_sub(1));
        let b = single(Some(i + 1));
        let u = ((t - self.edges[i]) / (self.edges[i + 1] - self.edges[i])).clamp(0., 1.);
        a + (b - a) * smoother(u)
    }
    /// Shape matching against016 style poses; no source time or017 poses are read.
    pub fn seed(&self, m: &Model, t: f64, previous: Option<&Parameters>) -> Parameters {
        let target = self.targets(t).tips;
        let delta = [target[1][0] - target[0][0], target[1][1] - target[0][1]];
        let span = delta[0].hypot(delta[1]);
        let angle = delta[1].atan2(delta[0]);
        let u = self.support_u(t);
        let mut best = f64::INFINITY;
        let mut result = m.style[0].q;
        for (s, shape) in m.style.iter().zip(&m.style_shape) {
            let ss = shape.span;
            // `rem_euclid(TAU)` of a value in [−π, 3π], bit for bit: a
            // negative one gains TAU as it would, one of TAU or more loses it
            // exactly (Sterbenz), as the remainder is exact.
            let wrapped = angle - shape.angle + std::f64::consts::PI;
            let wrapped = if wrapped < 0. {
                wrapped + std::f64::consts::TAU
            } else if wrapped >= std::f64::consts::TAU {
                wrapped - std::f64::consts::TAU
            } else {
                wrapped
            };
            let theta = wrapped - std::f64::consts::PI;
            let yaw = s.q[13] + theta.to_degrees();
            let old_u = shape.support;
            let mut score = (ss - span).powi(2)
                + (0..2)
                    .map(|d| (s.tips[d][2] - target[d][2]).powi(2))
                    .sum::<f64>()
                    * 6.
                + ((yaw - m.hi[13] + 1.).max(0.).powi(2) + (m.lo[13] + 1. - yaw).max(0.).powi(2))
                    * 5.
                + (old_u - u).powi(2) * 20.;
            if let Some(p) = previous {
                score += (0..8).map(|j| ((s.q[j] - p[j]) / 10.).powi(2)).sum::<f64>() * 0.03
                    + ((yaw - p[13]) / 20.).powi(2) * 0.10;
            }
            if score < best {
                best = score;
                let (sn, cs) = theta.sin_cos();
                let mid = [
                    (s.tips[0][0] + s.tips[1][0]) * 0.5,
                    (s.tips[0][1] + s.tips[1][1]) * 0.5,
                ];
                let x = s.q[8] - mid[0];
                let y = s.q[9] - mid[1];
                let mut q = s.q;
                q[8] = cs * x - sn * y + (target[0][0] + target[1][0]) * 0.5;
                q[9] = sn * x + cs * y + (target[0][1] + target[1][1]) * 0.5;
                q[10] += (target[0][2] + target[1][2] - s.tips[0][2] - s.tips[1][2]) * 0.5;
                q[13] = yaw;
                m.clamp(&mut q);
                result = q;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flight(release: f64, land: f64, from: u8, to: u8) -> Swing {
        Swing {
            release,
            land,
            from,
            to,
            lift_m: 0.01,
        }
    }
    #[test]
    fn centers_single_contacts_and_keeps_shared_contacts_fixed_until_takeoff() {
        let mut p = Plan::empty(8.);
        p.tracks.left = vec![
            flight(0.5, 1., 5, 4),
            flight(2.7, 3.06, 4, 4),
            flight(5., 5.5, 4, 8),
        ];
        p.tracks.right = vec![
            flight(0.5, 1., 5, 6),
            flight(2.7, 3., 6, 4),
            flight(4., 4.5, 4, 6),
        ];
        p.rebuild_edges();
        p.center_contacts(None);
        assert_eq!(p.targets(1.).tips, [p.tile_center(4), p.tile_center(6)]);
        let shared = p.targets(3.06).tips;
        assert_eq!(shared[1][0] - shared[0][0], 11.);
        assert_eq!(p.targets(4.8).tips[0], shared[0]);
        assert_eq!(p.targets(5.5).tips[0], p.tile_center(8));
        assert_eq!(p.targets(2.7).tips[0], p.tile_center(4));
        for d in 0..2 {
            for i in 0..500 {
                let t = i as f64 / 100.;
                if p.targets(t).contact[d] && p.targets(t + 0.001).contact[d] {
                    assert_eq!(p.targets(t).tips[d], p.targets(t + 0.001).tips[d]);
                }
            }
        }
    }
    #[test]
    fn centered_rolling_publication_preserves_existing_flight_endpoints() {
        let mut old = Plan::empty(8.);
        old.tracks.left = vec![flight(0.5, 1., 5, 4)];
        old.tracks.right = vec![flight(0.5, 1., 5, 6)];
        old.rebuild_edges();
        old.center_contacts(None);
        let mut next = old.clone();
        next.tracks.right.push(flight(2., 2.5, 6, 8));
        next.rebuild_edges();
        next.center_contacts(Some((&old, 1.5)));
        for i in 0..=1500 {
            let t = i as f64 / 1000.;
            assert_eq!(old.targets(t).tips, next.targets(t).tips);
        }
        // A pruned flight becomes the new initial planted interval.
        next.initial.left = 4;
        next.initial.right = 6;
        next.tracks.left.clear();
        next.tracks.right.remove(0);
        next.rebuild_edges();
        next.center_contacts(Some((&old, 1.5)));
        assert_eq!(old.targets(1.5).tips, next.targets(1.5).tips);
    }
}
