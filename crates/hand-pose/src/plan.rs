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
/// The late part of a mine's conservative clearance interval can be replaced
/// by the approach to an exact, received scored press. Keep 5 ms beyond the
/// midpoint where that press becomes closer than the old mine. A foot already
/// on this mine must have departed before its early window; a different source
/// is still checked against its own hazards. Actual judge replay remains the
/// authority for accepting the complete contact plan. The 170 ms bound is
/// where midpoint + 5 ms reaches the unchanged 90 ms physical window end.
/// A press at most 10 ms after a mine uses the judge's post-mine press
/// precedence. It requires an incoming finger from another tile that released
/// before the entire conservative window; this cannot protect an existing
/// planted finger or a same-tile hop. Exact received-head proofs, other-finger
/// clearance and actual judge replay are still required by callers.
pub fn mine_head_clearance_end(mine: f64, release: f64, land: f64, from_mine: bool) -> Option<f64> {
    if mine.is_finite()
        && release.is_finite()
        && land.is_finite()
        && land > mine
        && land <= mine + 0.010
        && !from_mine
        && release <= mine - 0.095
    {
        return Some(mine - 0.09);
    }
    (mine.is_finite()
        && release.is_finite()
        && land.is_finite()
        && land > mine + 0.010
        && land <= mine + 0.170
        && release < land
        && (!from_mine || release <= mine - 0.095))
        .then_some((mine + land) * 0.5 + 0.005)
}

/// Guarded departure from an exact scored head before a nearby mine.
/// The finger must release before the midpoint-minus-5ms boundary and actual
/// judge replay must still accept every press, release and crossed mine.
/// At 170 ms separation the boundary reaches the unchanged physical window
/// start, so more distant heads need no permission.
pub fn mine_head_clearance_start(mine: f64, head: f64, release: f64) -> Option<f64> {
    let clear_after = (mine + head) * 0.5 - 0.005;
    (mine.is_finite()
        && head.is_finite()
        && release.is_finite()
        && head < mine - 0.010
        && head >= mine - 0.170
        && release > head
        && release < clear_after)
        .then_some(clear_after)
}
#[derive(Clone)]
struct MineDeparture {
    mine: Mine,
    foot: usize,
    arrive_release: f64,
    head: f64,
    release: f64,
    land: f64,
    to: u8,
    clear_after: f64,
}

#[derive(Clone)]
struct MineLanding {
    mine: Mine,
    foot: usize,
    release: f64,
    land: f64,
    clear_until: f64,
    next_land: f64,
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
fn legacy_version() -> u8 {
    1
}
#[derive(Deserialize, Clone)]
pub struct Plan {
    #[serde(default = "legacy_version")]
    version: u8,
    /// Explicit versioned geometry is required for doubled IDs, including live
    /// dance-double plans. Legacy single plans keep their implicit geometry.
    #[serde(default)]
    mat: Option<ddi_dance_mat::Geometry>,
    #[serde(default)]
    layout: String,
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
    #[serde(skip)]
    mine_landings: Vec<MineLanding>,
    #[serde(skip)]
    mine_departures: Vec<MineDeparture>,
}
#[derive(Clone, Copy)]
pub struct Targets {
    pub tips: [Point; 2],
    pub contact: [bool; 2],
}
/// Physical keypad tile coordinates, before the hand's -16 mm root offset.
/// Solo uses the upper corners; Pump single uses all four corners and center.
pub fn panel_position(panel: u8, pitch: f64) -> Option<[f64; 2]> {
    ddi_dance_mat::cell_position(panel, pitch)
}
fn smoother(u: f64) -> f64 {
    u * u * u * (10. + u * (-15. + 6. * u))
}
impl Plan {
    // Ease very wide corner bases toward centered balance, including their
    // lift/landing approaches. Other layouts retain their accepted guide.
    fn wide_balance(&self, tips: [Point; 2]) -> f64 {
        if self.mat.is_none_or(|mat| mat.pads() == 1)
            && !matches!(
                self.layout.as_str(),
                "pump-single" | "danoni-7" | "techno-single8" | "danoni-9B"
            )
        {
            return 0.;
        }
        let span = (tips[1][0] - tips[0][0]).hypot(tips[1][1] - tips[0][1]);
        smoother(((span - 54.) / 14.).clamp(0., 1.))
    }
    pub(crate) fn centered_balance(&self, tips: [Point; 2], center: [f64; 2]) -> [f64; 2] {
        let w = self.wide_balance(tips);
        if w == 0. {
            return center;
        }
        std::array::from_fn(|j| center[j] + ((tips[0][j] + tips[1][j]) * 0.5 - center[j]) * w)
    }
    pub fn empty(duration: f64) -> Self {
        Self {
            version: 1,
            mat: None,
            layout: String::new(),
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
            mine_landings: Vec::new(),
            mine_departures: Vec::new(),
        }
    }
    pub(crate) fn double_mat(&self) -> bool {
        self.mat.is_some_and(|g| g.pads() == 2)
    }

    pub(crate) fn periodic_yaw(&self, m: &Model) -> bool {
        self.mat.is_some() && m.lo[13] <= -180. && m.hi[13] >= 180.
    }
    /// A constant opening guide in the explicitly declared coordinate system.
    /// Copy no future tracks/events: they must still enter through the rolling
    /// window. Legacy plans retain their accepted neutral startup guide.
    pub(crate) fn opening_seed_plan(&self, duration: f64) -> Self {
        if self.mat.is_none() {
            return Self::empty(duration);
        }
        let mut opening = Self::empty(duration);
        opening.version = self.version;
        opening.mat = self.mat;
        opening.layout = self.layout.clone();
        opening.initial = self.initial.clone();
        opening.placement = self.placement.clone();
        opening.centered = Some(CenterTargets {
            initial: self.targets(0.).tips,
            landings: [Vec::new(), Vec::new()],
        });
        opening
    }
    pub fn load(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("plan byte limit".into());
        }
        let mut p: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        match (p.version, p.mat) {
            (1, None)
                if ddi_dance_mat::MatLayout::from_id(&p.layout)
                    .is_none_or(|layout| layout.pads() == 1) => {}
            (2, Some(mat))
                if ddi_dance_mat::MatLayout::from_id(&p.layout)
                    .is_some_and(|layout| layout.pads() == mat.pads())
                    && mat.pitch_mm() == p.placement.tile_pitch_mm => {}
            _ => return Err("invalid plan version/geometry/layout".into()),
        }
        let mat = p.mat;
        let valid_panel = |id: u8| match mat {
            Some(g) => ddi_dance_mat::Tile::try_from(id)
                .ok()
                .and_then(|t| g.position(t))
                .is_some(),
            None => matches!(id, 1..=9),
        };
        if !p.duration.is_finite()
            || p.duration <= 0.
            || p.duration > 3600.
            || p.events.len() > 10000
            || p.mines.len() > 10000
            || p.roll_pulses.len() > 10000
            || p.mines.iter().any(|m| {
                !valid_panel(m.panel) || !m.time.is_finite() || m.time < 0. || m.time > p.duration
            })
        {
            return Err("plan limits".into());
        }
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
                || !valid_panel(r.panel)
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
        p.rebuild_mine_landings();
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
        self.rebuild_mine_landings();
    }
    fn rebuild_mine_landings(&mut self) {
        self.mine_landings.clear();
        self.mine_departures.clear();
        if self.mines.is_empty() {
            return;
        }
        let heads = self
            .events
            .iter()
            .filter(|e| !e.lift)
            .map(|e| (usize::from(e.foot == "R"), e.panel, e.time.to_bits()))
            .collect::<std::collections::BTreeSet<_>>();
        let mut hazards: [Vec<Mine>; 18] = std::array::from_fn(|_| Vec::new());
        for m in &self.mines {
            if (1..=18).contains(&m.panel) {
                hazards[m.panel as usize - 1].push(*m);
            }
        }
        for v in &mut hazards {
            v.sort_by(|a, b| a.time.total_cmp(&b.time));
        }
        for (d, tr) in [&self.tracks.left, &self.tracks.right]
            .into_iter()
            .enumerate()
        {
            for (i, swing) in tr.iter().enumerate() {
                if !(1..=18).contains(&swing.to)
                    || !heads.contains(&(d, swing.to, swing.land.to_bits()))
                {
                    continue;
                }
                let mines = &hazards[swing.to as usize - 1];
                if let Some(outgoing) = tr.get(i + 1).filter(|s| s.from == swing.to) {
                    let start = mines.partition_point(|m| m.time <= swing.land + 0.010);
                    for mine in mines[start..]
                        .iter()
                        .take_while(|m| m.time <= swing.land + 0.170)
                    {
                        if let Some(clear_after) =
                            mine_head_clearance_start(mine.time, swing.land, outgoing.release)
                        {
                            if self.mine_landings.len() + self.mine_departures.len() == 32768 {
                                return;
                            }
                            self.mine_departures.push(MineDeparture {
                                mine: *mine,
                                foot: d,
                                arrive_release: swing.release,
                                head: swing.land,
                                release: outgoing.release,
                                land: outgoing.land,
                                to: outgoing.to,
                                clear_after,
                            });
                        }
                    }
                }

                let earliest = if swing.from == swing.to {
                    (swing.land - 0.170).max(swing.release + 0.095 - 1e-9)
                } else {
                    swing.land - 0.170
                };
                let start = mines.partition_point(|m| m.time < earliest);
                for mine in mines[start..].iter().take_while(|m| m.time < swing.land) {
                    if let Some(clear_until) = mine_head_clearance_end(
                        mine.time,
                        swing.release,
                        swing.land,
                        swing.from == mine.panel,
                    ) {
                        // Bound this derived permission cache too. Excess
                        // entries retain conservative hazard checking.
                        if self.mine_landings.len() + self.mine_departures.len() == 32768 {
                            return;
                        }
                        self.mine_landings.push(MineLanding {
                            mine: *mine,
                            foot: d,
                            release: swing.release,
                            land: swing.land,
                            clear_until,
                            next_land: tr.get(i + 1).map_or(f64::INFINITY, |s| s.land),
                        });
                    }
                }
            }
        }
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
        let mine_landings = p.mine_landings.clone();
        let mine_departures = p.mine_departures.clone();
        let received_heads = self
            .events
            .iter()
            .filter(|e| !e.lift)
            .map(|e| (usize::from(e.foot == "R"), e.panel, e.time.to_bits()))
            .collect::<std::collections::BTreeSet<_>>();
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
                            && ((s.land - e.time - 0.060).abs() < 1e-9
                                || (s.land > e.time
                                    && s.land < e.time + 0.060
                                    && received_heads.contains(&(d, s.to, s.land.to_bits()))))
                    })
                {
                    releases[d].push(s.release);
                }
            }
        }
        // A received near-coincident head declares its incoming flight through
        // a known mine, even while the scored event remains beyond this view.
        for g in &mine_landings {
            if g.land > g.mine.time
                && g.land <= g.mine.time + 0.010
                && g.release <= end
                && p.mines.contains(&g.mine)
            {
                releases[g.foot].push(g.release);
            }
        }
        // A received scored head plus its known mine declares the departure,
        // even if the actual release is just beyond this watermark.
        for g in &mine_departures {
            if g.head <= end && p.mines.contains(&g.mine) {
                releases[g.foot].push(g.release);
            }
        }
        p.tracks
            .left
            .retain(|s| s.land <= end || releases[0].contains(&s.release));
        p.tracks
            .right
            .retain(|s| s.land <= end || releases[1].contains(&s.release));
        p.rebuild_edges();
        // Keep only proofs from already-received heads whose declared clearing
        // flight survives the watermark. Future scored events stay hidden.
        p.mine_landings = mine_landings
            .into_iter()
            .filter(|g| {
                p.mines.contains(&g.mine)
                    && p.track(g.foot)
                        .0
                        .iter()
                        .any(|s| s.release == g.release && s.land == g.land && s.to == g.mine.panel)
            })
            .collect();
        p.mine_departures = mine_departures
            .into_iter()
            .filter(|g| {
                p.mines.contains(&g.mine)
                    && g.head <= end
                    && p.track(g.foot).0.iter().any(|s| {
                        s.release == g.arrive_release && s.land == g.head && s.to == g.mine.panel
                    })
                    && p.track(g.foot).0.iter().any(|s| {
                        s.release == g.release
                            && s.land == g.land
                            && s.from == g.mine.panel
                            && s.to == g.to
                    })
            })
            .collect();
        p.edges.retain(|t| *t <= end);
        p
    }
    /// A floor contact inside a live mine is forbidden, including stream motion.
    pub(crate) fn mine_contact(&self, t: f64, tips: &[Point; 2]) -> bool {
        self.mines
            .iter()
            .filter(|m| (t - m.time).abs() <= 0.09)
            .any(|m| {
                let [x, y] = self.physical_position(m.panel).expect("checked panel");
                let half = self.tile_half_mm();
                tips.iter().enumerate().any(|(d, p)| {
                    let scored_approach = self.mine_landings.iter().any(|g| {
                        g.mine == *m
                            && g.foot == d
                            && t >= g.clear_until
                            && t >= g.release
                            && t < g.next_land
                    });
                    let scored_departure = self.mine_departures.iter().any(|g| {
                        g.mine == *m
                            && g.foot == d
                            && t >= g.arrive_release
                            && t < g.clear_after
                            && t < g.land
                    });
                    !scored_approach
                        && !scored_departure
                        && p[2] <= 0.25
                        && (p[0] + 16. - x).abs() <= half
                        && (p[1] - y).abs() <= half
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
        for g in &self.mine_landings {
            for t in [g.clear_until - 1e-6, g.clear_until, g.clear_until + 1e-6] {
                if t >= start && t <= end {
                    times.push(t);
                }
            }
        }
        for g in &self.mine_departures {
            for t in [g.clear_after - 1e-6, g.clear_after, g.clear_after + 1e-6] {
                if t >= start && t <= end {
                    times.push(t);
                }
            }
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
        let (cell, pad_offset) = self.mat.map_or((p, 0.), |mat| {
            let t = ddi_dance_mat::Tile::try_from(p).expect("checked panel");
            (t.cell(), mat.pad_offset_mm(t.pad()).expect("checked pad"))
        });
        let [mut x, y] = panel_position(cell, scale).expect("checked panel");
        if self.mat.is_some() {
            x += pad_offset;
        }
        let offset = if cell == 5 {
            self.placement.center_offset_mm
        } else {
            self.placement.other_offset_mm
        };
        let center = self.physical_position(p).expect("checked panel")[0];
        let half = self.placement.tile_pitch_mm * 0.47 - 1.;
        let x = (x + if d == 0 { -offset } else { offset }).clamp(center - half, center + half);
        [x - 16., y, 0.]
    }
    fn shared_offset(&self, panel: u8) -> f64 {
        if (panel - 1) % 9 + 1 == 5 {
            self.placement.center_offset_mm
        } else {
            self.placement.other_offset_mm
        }
    }
    /// The old endpoint of a published contact/flight. Shared-slot direction
    /// selection must inherit it before considering any newly received approach.
    fn frozen_contact(&self, previous: Option<(&Self, f64)>, d: usize, i: usize) -> Option<Point> {
        let (old, until) = previous?;
        let track = if d == 0 {
            &self.tracks.left
        } else {
            &self.tracks.right
        };
        if i == 0 {
            let first = track.first().map_or(until, |s| s.release);
            let mut point = old.targets(first.min(until).max(0.)).tips[d];
            point[2] = 0.;
            return Some(point);
        }
        let s = &track[i - 1];
        if s.release > until || until.is_nan() {
            return None;
        }
        let old_track = if d == 0 {
            &old.tracks.left
        } else {
            &old.tracks.right
        };
        let j = old_track.partition_point(|v| v.release < s.release);
        old_track
            .get(j)
            .filter(|v| {
                v.release == s.release && v.land == s.land && v.from == s.from && v.to == s.to
            })
            .map(|_| old.landing_anchor(d, j))
    }

    /// One separation axis per connected run of overlapping shared contacts.
    /// A re-hit must not rotate the other finger's still-planted contact spot.
    /// Both contact lists are ordered and disjoint: their overlap sweep is
    /// linear; union/find and frozen-endpoint lookup keep the work bounded.
    fn shared_axes(
        &self,
        spans: &[Vec<(f64, f64, u8)>; 2],
        previous: Option<(&Self, f64)>,
    ) -> [Vec<Option<[f64; 2]>>; 2] {
        #[derive(Clone, Copy)]
        struct Axis {
            direction: [f64; 2],
            inherited: bool,
            since: f64,
        }
        fn choose(a: Option<Axis>, b: Option<Axis>) -> Option<Axis> {
            match (a, b) {
                (Some(a), Some(b))
                    if (b.inherited && !a.inherited)
                        || (a.inherited == b.inherited && b.since < a.since) =>
                {
                    Some(b)
                }
                (Some(a), _) => Some(a),
                (_, b) => b,
            }
        }
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        fn direction(a: Point, b: Point) -> Option<[f64; 2]> {
            let v = [b[0] - a[0], b[1] - a[1]];
            let length = v[0].hypot(v[1]);
            (length > 1e-9).then(|| v.map(|v| v / length))
        }
        let tracks = [&self.tracks.left, &self.tracks.right];
        let split = spans[0].len();
        let count = split + spans[1].len();
        let mut parent = (0..count).collect::<Vec<_>>();
        let mut axes = vec![None; count];
        for d in 0..2 {
            for (i, &(a, _, panel)) in spans[d].iter().enumerate() {
                if let Some(point) = self.frozen_contact(previous, d, i) {
                    let center = self.tile_center(panel);
                    let axis = if d == 0 {
                        direction(point, center)
                    } else {
                        direction(center, point)
                    };
                    if let Some(direction) = axis {
                        axes[if d == 0 { i } else { split + i }] = Some(Axis {
                            direction,
                            inherited: true,
                            since: a,
                        });
                    }
                }
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < spans[0].len() && j < spans[1].len() {
            let (a, b, panel) = spans[0][i];
            let (x, y, other) = spans[1][j];
            if panel == other && a.max(x) < b.min(y) - 1e-9 {
                let indices = [i, j];
                let begins = [a, x];
                let before: [Point; 2] = std::array::from_fn(|d| {
                    let k = indices[d];
                    let incoming = begins[d] >= begins[1 - d] && k > 0;
                    let previous_index = if incoming { k - 1 } else { k };
                    let tile = if incoming {
                        tracks[d][k - 1].from
                    } else {
                        panel
                    };
                    if let Some(point) = self.frozen_contact(previous, d, previous_index) {
                        return point;
                    }
                    let mut point = self.tile_center(tile);
                    let root = find(
                        &mut parent,
                        if d == 0 {
                            previous_index
                        } else {
                            split + previous_index
                        },
                    );
                    if let Some(axis) = axes[root] {
                        // Both fingers may jump from the same shared tile.
                        // Retain their preceding order rather than resetting
                        // it merely because the tile centers coincide.
                        let offset = self.shared_offset(tile);
                        let step = if d == 0 { -offset } else { offset };
                        for j in 0..2 {
                            point[j] += step * axis.direction[j];
                        }
                    }
                    point
                });
                let candidate = Axis {
                    direction: direction(before[0], before[1]).unwrap_or([1., 0.]),
                    inherited: false,
                    since: a.max(x),
                };
                let l = find(&mut parent, i);
                let r = find(&mut parent, split + j);
                let (root, child) = (l.min(r), l.max(r));
                parent[child] = root;
                axes[root] = choose(choose(axes[l], axes[r]), Some(candidate));
            }
            if b <= y {
                i += 1;
            } else {
                j += 1;
            }
        }
        std::array::from_fn(|d| {
            (0..spans[d].len())
                .map(|i| {
                    let root = find(&mut parent, if d == 0 { i } else { split + i });
                    axes[root].map(|axis| axis.direction)
                })
                .collect()
        })
    }
    /// Center isolated contacts. Shared planted intervals retain separated
    /// spots (18 mm apart at center, 11 mm on arrows). Each spot belongs to a
    /// whole contact, never the current frame: another finger lifting must not
    /// slide an already planted tip.
    /// Reused published flights retain both endpoints across rolling windows.
    pub fn center_contacts(&mut self, previous: Option<(&Self, f64)>) {
        let previous = previous.filter(|(old, _)| self.same_geometry(old));
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
        let axes = self.mat.map(|_| self.shared_axes(&spans, previous));
        let points: [Vec<Point>; 2] = std::array::from_fn(|d| {
            let mut other_at = 0;
            spans[d]
                .iter()
                .enumerate()
                .map(|(i, &(a, b, panel))| {
                    let mut p = self.tile_center(panel);
                    while other_at < spans[1 - d].len() && spans[1 - d][other_at].1 <= a {
                        other_at += 1;
                    }
                    if spans[1 - d][other_at..]
                        .iter()
                        .take_while(|&&(x, _, _)| x < b)
                        .any(|&(x, y, other)| panel == other && a.max(x) < b.min(y) - 1e-9)
                    {
                        let offset = self.shared_offset(panel);
                        let step = if d == 0 { -offset } else { offset };
                        if let Some(axes) = &axes {
                            let axis = axes[d][i].expect("shared contact axis");
                            for j in 0..2 {
                                p[j] += step * axis[j];
                            }
                        } else {
                            // Preserve the accepted legacy placement arithmetic.
                            p[0] += step;
                        }
                    }
                    p
                })
                .collect()
        });
        let mut centered = CenterTargets {
            initial: [points[0][0], points[1][0]],
            landings: std::array::from_fn(|d| points[d][1..].to_vec()),
        };
        for d in 0..2 {
            if let Some(point) = self.frozen_contact(previous, d, 0) {
                centered.initial[d] = point;
            }
            for (i, point) in centered.landings[d].iter_mut().enumerate() {
                if let Some(frozen) = self.frozen_contact(previous, d, i + 1) {
                    *point = frozen;
                }
            }
        }
        self.centered = Some(centered);
    }
    /// Geometry identity is part of publication compatibility. Callers changing
    /// this must discard prepared runtime/renderer state rather than reuse it.
    pub fn same_geometry(&self, other: &Self) -> bool {
        self.version == other.version
            && self.mat == other.mat
            && self.placement.tile_pitch_mm == other.placement.tile_pitch_mm
            && self.placement.panel_anchor_mm == other.placement.panel_anchor_mm
            && self.placement.center_offset_mm == other.placement.center_offset_mm
            && self.placement.other_offset_mm == other.placement.other_offset_mm
    }
    pub(crate) fn physical_position(&self, panel: u8) -> Option<[f64; 2]> {
        match self.mat {
            Some(g) => g.position(ddi_dance_mat::Tile::try_from(panel).ok()?),
            None => panel_position(panel, self.placement.tile_pitch_mm),
        }
    }
    pub(crate) fn tile_half_mm(&self) -> f64 {
        self.mat.map_or(11.5, ddi_dance_mat::Geometry::tile_half_mm)
    }
    pub fn tile_center(&self, panel: u8) -> Point {
        let [x, y] = self.physical_position(panel).expect("checked panel");
        [x - 16., y, 0.]
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
        let support = self.support_u(t);
        let u = support + (0.5 - support) * self.wide_balance(target);
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
    fn shared_transfer(mirror: bool) -> Plan {
        let mut p = Plan::empty(6.);
        p.version = 2;
        p.layout = "dance-double".into();
        p.mat = Some(ddi_dance_mat::Geometry::new(2, 24., 0.).unwrap());
        p.initial = Initial { left: 6, right: 8 };
        p.tracks.left.push(flight(2., 2.6, 6, 13));
        p.tracks.right.push(flight(1.3, 1.9, 8, 13));
        if mirror {
            let tile = |id: u8| {
                let pad = (id - 1) / 9;
                let cell = (id - 1) % 9;
                (1 - pad) * 9 + cell / 3 * 3 + (2 - cell % 3) + 1
            };
            p.initial.left = tile(p.initial.left);
            p.initial.right = tile(p.initial.right);
            for track in [&mut p.tracks.left, &mut p.tracks.right] {
                for s in track {
                    s.from = tile(s.from);
                    s.to = tile(s.to);
                }
            }
        }
        p.rebuild_edges();
        p
    }
    #[test]
    fn shared_spots_follow_the_approach_and_mirror_inside_the_same_tile() {
        for mirror in [false, true] {
            let mut p = shared_transfer(mirror);
            p.center_contacts(None);
            let panel = if mirror { 6 } else { 13 };
            let center = p.tile_center(panel);
            let tips = p.targets(2.7).tips;
            let sign = if mirror { -1. } else { 1. };
            assert_eq!(tips[0], [center[0] - sign * 5.5, center[1], 0.]);
            assert_eq!(tips[1], [center[0] + sign * 5.5, center[1], 0.]);
            for i in 0..=600 {
                let t = 2. + i as f64 / 1000.;
                let q = p.targets(t).tips;
                assert!((q[1][0] - q[0][0]) * sign > 0., "approach crosses at {t}");
                assert_eq!(q[1], tips[1], "planted finger slides at {t}");
            }
            for tip in tips {
                assert!(11.5 - (tip[0] - center[0]).abs().max((tip[1] - center[1]).abs()) >= 1.);
            }
        }
    }
    #[test]
    fn shared_contact_chain_keeps_its_axis_through_rehits_and_pruning() {
        let mut old = shared_transfer(true);
        old.center_contacts(None);
        let mut next = old.clone();
        next.tracks.right.extend([
            flight(3., 3.4, 6, 13),
            flight(4., 4.4, 13, 6),
            flight(5., 5.5, 6, 6),
        ]);
        next.tracks.left.push(flight(5., 5.5, 6, 6));
        next.rebuild_edges();
        next.center_contacts(Some((&old, 2.8)));
        for i in 0..=2800 {
            let t = i as f64 / 1000.;
            assert_eq!(old.targets(t).tips, next.targets(t).tips);
        }
        let spots = next.targets(2.8).tips;
        assert_eq!(
            next.targets(4.5).tips,
            spots,
            "return rotated the shared slots"
        );
        assert_eq!(
            next.targets(5.6).tips,
            spots,
            "same-tile jump reversed the slots"
        );
        for i in 0..=2400 {
            let t = 2.6 + i as f64 / 1000.;
            assert_eq!(next.targets(t).tips[0], spots[0]);
        }
        // Once earlier flights leave the window, inherited planted spots still
        // set the shared axis; the removed approach is not guessed again.
        let mut pruned = next.clone();
        pruned.initial = Initial { left: 6, right: 6 };
        pruned.tracks.left.remove(0);
        pruned.tracks.right.remove(0);
        pruned.rebuild_edges();
        pruned.center_contacts(Some((&next, 2.8)));
        assert_eq!(pruned.targets(2.8).tips, spots);
        assert_eq!(pruned.targets(4.5).tips, spots);
        assert_eq!(pruned.targets(5.6).tips, spots);
    }
    #[test]
    fn simultaneous_shared_landing_uses_the_incoming_pair_order() {
        let mut p = shared_transfer(true);
        p.tracks.left = vec![flight(1., 2., 13, 6)];
        p.tracks.right = vec![flight(1., 2., 17, 6)];
        p.rebuild_edges();
        p.center_contacts(None);
        let q = p.targets(2.1).tips;
        let v = [q[1][0] - q[0][0], q[1][1] - q[0][1]];
        let from = [
            p.tile_center(17)[0] - p.tile_center(13)[0],
            p.tile_center(17)[1] - p.tile_center(13)[1],
        ];
        assert!(v[0] * from[0] + v[1] * from[1] > 0.);
        assert!((v[0].hypot(v[1]) - 11.).abs() < 1e-12);
    }
    #[test]
    fn periodic_yaw_requires_explicit_geometry_and_a_full_root_turn() {
        let mut m = Model::load(include_bytes!("../../../assets/hand/model.json")).unwrap();
        let mut p = Plan::empty(3.);
        assert!(!p.periodic_yaw(&m));
        p.mat = Some(ddi_dance_mat::Geometry::new(2, 24., 0.).unwrap());
        assert!(p.periodic_yaw(&m));
        m.lo[13] = -179.;
        assert!(!p.periodic_yaw(&m));
        m.lo[13] = -180.;
        m.hi[13] = 179.;
        assert!(!p.periodic_yaw(&m));
    }
    #[test]
    fn explicit_opening_seed_plan_keeps_contacts_without_future_context() {
        let mut p = Plan::empty(20.);
        p.version = 2;
        p.mat = Some(ddi_dance_mat::Geometry::new(2, 24., 0.).unwrap());
        p.layout = "pump-double".into();
        p.initial = Initial { left: 9, right: 3 };
        p.tracks.left.push(flight(0.7, 1.6, 9, 5));
        p.tracks.right.push(flight(0.7, 1.6, 3, 14));
        p.mines.push(Mine {
            time: 10.,
            panel: 14,
        });
        p.center_contacts(None);
        let opening = p.opening_seed_plan(3.5);
        assert!(opening.same_geometry(&p));
        assert_eq!(opening.targets(0.).tips, p.targets(0.).tips);
        assert_eq!(opening.targets(3.).tips, p.targets(0.).tips);
        assert_eq!(opening.initial.left, 9);
        assert_eq!(opening.initial.right, 3);
        assert_eq!(opening.edges, [0., 3.5]);
        assert!(opening.tracks.left.is_empty() && opening.tracks.right.is_empty());
        assert!(
            opening.events.is_empty() && opening.mines.is_empty() && opening.roll_pulses.is_empty()
        );
        assert!(opening.release_events.is_empty());
        // Sharing an opening tile must preserve its published separated spots.
        p.initial = Initial {
            left: 14,
            right: 14,
        };
        p.tracks.left.clear();
        p.tracks.right.clear();
        p.center_contacts(None);
        let opening = p.opening_seed_plan(3.5);
        assert_eq!(opening.targets(0.).tips, p.targets(0.).tips);
    }
    #[test]
    fn legacy_opening_seed_plan_keeps_the_accepted_neutral_seed() {
        let mut p = Plan::empty(20.);
        p.layout = "pump-single".into();
        p.initial = Initial { left: 9, right: 3 };
        p.center_contacts(None);
        let opening = p.opening_seed_plan(3.5);
        let old = Plan::empty(3.5);
        assert!(opening.same_geometry(&old));
        assert_eq!(opening.layout, old.layout);
        assert_eq!(opening.targets(0.).tips, old.targets(0.).tips);
        assert!(opening.centered.is_none());
    }
    #[test]
    fn double_mines_use_physical_tiles_and_scaled_surface_bounds() {
        let mut p = Plan::empty(3.);
        p.version = 2;
        p.mat = Some(ddi_dance_mat::Geometry::new(2, 24., 0.).unwrap());
        p.mines.push(Mine {
            time: 1.,
            panel: 14,
        });
        assert!(!p.mine_contact(1., &[p.tile_center(5), p.tile_center(6)]));
        assert!(p.mine_contact(1., &[p.tile_center(14), p.tile_center(6)]));
        assert!(!p.mine_contact(1.1, &[p.tile_center(14), p.tile_center(6)]));
    }
    #[test]
    fn solo_diagonal_mines_use_their_corner_instead_of_a_cardinal_tile() {
        for (panel, center) in [(7, [-40., 24., 0.]), (9, [8., 24., 0.])] {
            let mut p = Plan::empty(3.);
            p.mines.push(Mine { time: 1., panel });
            assert_eq!(p.tile_center(panel), center);
            assert!(p.mine_contact(1., &[center, [-16., 0., 0.]]));
            assert!(!p.mine_contact(1., &[[-40., 0., 0.], [8., 0., 0.]]));
            for d in 0..2 {
                let anchor = p.anchor(panel, d);
                assert!((anchor[0] - center[0]).abs() <= 10.5);
                assert!((anchor[1] - center[1]).abs() <= 10.5);
            }
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
    #[test]
    fn wide_balance_is_smooth_and_scoped_to_corner_layouts() {
        let mut p = Plan::empty(1.);
        let center = [12., 4.];
        let tips = |span: f64| [[-span / 2., 0., 0.], [span / 2., 0., 0.]];
        for layout in ["dance-single", "dance-solo", "danoni-5"] {
            p.layout = layout.into();
            assert_eq!(p.centered_balance(tips(70.), center), center);
        }
        for layout in ["pump-single", "danoni-7", "techno-single8", "danoni-9B"] {
            p.layout = layout.into();
            assert_eq!(p.centered_balance(tips(54.), center), center);
            assert_eq!(p.centered_balance(tips(68.), center), [0., 0.]);
            for edge in [54., 68.] {
                let h = 0.001;
                let a = p.centered_balance(tips(edge - h), center)[0];
                let b = p.centered_balance(tips(edge), center)[0];
                let c = p.centered_balance(tips(edge + h), center)[0];
                assert!((c - a).abs() / (2. * h) < 1e-5);
                assert!((a - 2. * b + c).abs() / h.powi(2) < 0.001);
            }
            let next = p.visible(0.5);
            assert_eq!(next.centered_balance(tips(68.), center), [0., 0.]);
        }
    }
}

#[cfg(test)]
mod mine_landing_tests {
    use super::*;
    fn study() -> Plan {
        let mut p = Plan::empty(2.);
        p.initial.left = 4;
        p.initial.right = 6;
        p.tracks.left.push(Swing {
            release: 0.8,
            land: 1.084,
            from: 4,
            to: 4,
            lift_m: 0.035,
        });
        p.events.push(Event {
            time: 1.084,
            music_time: 1.084,
            tick: 52,
            panel: 4,
            foot: "L".into(),
            lift: false,
        });
        p.mines.push(Mine { time: 1., panel: 4 });
        p.rebuild_edges();
        p
    }
    fn floor(p: &Plan) -> [Point; 2] {
        [p.tile_center(4), p.tile_center(6)]
    }
    #[test]
    fn exact_head_protects_only_its_own_post_mine_approach() {
        let p = study();
        let tips = floor(&p);
        for t in [0.95, 1., 1.04] {
            assert!(p.mine_contact(t, &tips), "still hazardous at {t}");
        }
        for t in [1.05, 1.084, 1.089] {
            assert!(!p.mine_contact(t, &tips), "known scored approach at {t}");
        }
        let probes = p.mine_probes(0.9, 1.1);
        assert!(probes.iter().any(|t| (*t - 1.047).abs() < 1e-12));
        let mut p = p;
        p.mines.push(Mine { time: 1., panel: 6 });
        p.rebuild_edges();
        assert!(
            p.mine_contact(1.084, &tips),
            "the other foot is not protected"
        );
    }
    #[test]
    fn unknown_unscored_wrong_foot_and_late_release_do_not_protect_contact() {
        for mutation in 0..4 {
            let mut p = study();
            match mutation {
                0 => p.events.clear(),
                1 => p.events[0].lift = true,
                2 => p.events[0].foot = "R".into(),
                _ => p.tracks.left[0].release = 0.99,
            }
            p.rebuild_edges();
            assert!(p.mine_contact(1.084, &floor(&p)), "mutation {mutation}");
        }
        let mut p = study();
        p.tracks.left.push(Swing {
            release: 1.085,
            land: 1.089,
            from: 4,
            to: 4,
            lift_m: 0.01,
        });
        p.rebuild_edges();
        assert!(
            p.mine_contact(1.089, &floor(&p)),
            "a second unscored press is not authorized"
        );
    }
    #[test]
    fn visible_clearing_flight_retains_known_head_proof_without_revealing_the_head() {
        let p = study().visible(1.06);
        assert!(p.events.is_empty());
        assert_eq!(p.tracks.left.len(), 1);
        assert!(!p.mine_contact(1.055, &floor(&p)));
        let p = study().visible(0.95);
        assert!(p.events.is_empty());
        assert!(p.tracks.left.is_empty());
        assert!(
            p.mine_contact(0.95, &floor(&p)),
            "no retained clearing flight"
        );
    }
    #[test]
    fn near_coincident_press_needs_a_clear_source_and_exact_received_head() {
        for gap in [0.000125, 0.00075, 0.009] {
            let mut p = study();
            p.initial.left = 5;
            p.tracks.left[0].from = 5;
            p.tracks.left[0].land = 1. + gap;
            p.events[0].time = 1. + gap;
            p.events[0].music_time = 1. + gap;
            p.rebuild_edges();
            assert!(!p.mine_contact(1., &floor(&p)));
            assert!(!p.mine_contact(0.999999, &floor(&p)));
            let view = p.visible(0.99);
            assert!(view.events.is_empty());
            assert_eq!(view.tracks.left.len(), 1);
            assert!(!view.mine_contact(0.99, &floor(&view)));
            for mutation in 0..6 {
                let mut bad = p.clone();
                match mutation {
                    0 => bad.events.clear(),
                    1 => bad.events[0].foot = "R".into(),
                    2 => bad.tracks.left[0].from = 4,
                    3 => bad.tracks.left[0].release = 0.906,
                    4 => bad.events[0].lift = true,
                    _ => {
                        bad.events[0].time += 0.001;
                    }
                }
                bad.rebuild_edges();
                assert!(bad.mine_contact(1., &floor(&bad)), "mutation {mutation}");
            }
            p.tracks.left.push(Swing {
                release: 1. + gap + 0.001,
                land: 1. + gap + 0.002,
                from: 4,
                to: 4,
                lift_m: 0.01,
            });
            p.rebuild_edges();
            assert!(p.mine_contact(1. + gap + 0.003, &floor(&p)));
        }
    }
    #[test]
    fn visible_staggered_companion_keeps_its_received_landing_without_revealing_it() {
        let mut p = Plan::empty(2.);
        p.initial.left = 8;
        p.initial.right = 5;
        p.tracks.left.push(Swing {
            release: 0.9,
            land: 1.03,
            from: 8,
            to: 5,
            lift_m: 0.035,
        });
        p.tracks.right.push(Swing {
            release: 0.8,
            land: 1.,
            from: 5,
            to: 8,
            lift_m: 0.035,
        });
        p.events = vec![
            Event {
                time: 1.,
                music_time: 1.,
                tick: 48,
                panel: 8,
                foot: "R".into(),
                lift: false,
            },
            Event {
                time: 1.03,
                music_time: 1.03,
                tick: 51,
                panel: 5,
                foot: "L".into(),
                lift: false,
            },
        ];
        p.rebuild_edges();
        let view = p.visible(1.01);
        assert_eq!(view.events.len(), 1);
        assert_eq!(view.tracks.left.len(), 1);
        p.events.pop();
        p.rebuild_edges();
        assert!(p.visible(1.01).tracks.left.is_empty());
    }
    #[test]
    fn an_exact_head_100ms_after_a_mine_has_a_guarded_approach() {
        let mut p = study();
        p.tracks.left[0].land = 1.1;
        p.events[0].time = 1.1;
        p.events[0].music_time = 1.1;
        p.rebuild_edges();
        assert!(p.mine_contact(1.054, &floor(&p)));
        assert!(!p.mine_contact(1.056, &floor(&p)));
        assert!(
            p.mine_probes(1., 1.09)
                .iter()
                .any(|t| (*t - 1.055).abs() < 1e-12)
        );
    }
    fn departure_study() -> Plan {
        let mut p = Plan::empty(2.);
        p.initial.left = 5;
        p.initial.right = 6;
        p.tracks.left = vec![
            Swing {
                release: 0.7,
                land: 1.,
                from: 5,
                to: 4,
                lift_m: 0.035,
            },
            Swing {
                release: 1.02,
                land: 1.08,
                from: 4,
                to: 5,
                lift_m: 0.035,
            },
        ];
        p.events.push(Event {
            time: 1.,
            music_time: 1.,
            tick: 48,
            panel: 4,
            foot: "L".into(),
            lift: false,
        });
        p.mines.push(Mine {
            time: 1.08,
            panel: 4,
        });
        p.rebuild_edges();
        p
    }
    #[test]
    fn scored_departure_protects_only_the_known_contact_before_its_boundary() {
        let p = departure_study();
        assert!(!p.mine_contact(1., &floor(&p)));
        assert!(!p.mine_contact(1.025, &floor(&p)));
        assert!(p.mine_contact(1.036, &floor(&p)));
        assert!(
            p.mine_probes(0.99, 1.09)
                .iter()
                .any(|t| (*t - 1.035).abs() < 1e-12)
        );
        for mutation in 0..4 {
            let mut p = departure_study();
            match mutation {
                0 => p.events.clear(),
                1 => p.events[0].foot = "R".into(),
                2 => {
                    p.tracks.left.pop();
                }
                _ => p.tracks.left[1].release = 1.04,
            }
            p.rebuild_edges();
            assert!(p.mine_contact(1.01, &floor(&p)), "mutation {mutation}");
        }
        let mut p = departure_study();
        p.tracks.left[1].land = 1.03;
        p.rebuild_edges();
        assert!(
            p.mine_contact(1.031, &floor(&p)),
            "permission expires at next landing"
        );
        assert!(mine_head_clearance_start(f64::NAN, 1., 1.02).is_none());
        assert!(mine_head_clearance_start(1.2, 1., 1.02).is_none());
    }
    #[test]
    fn visible_departure_retains_its_declared_flight_without_future_scored_events() {
        let p = departure_study().visible(1.01);
        assert_eq!(p.tracks.left.len(), 2);
        assert_eq!(p.events.len(), 1);
        assert!(!p.mine_contact(1.005, &floor(&p)));
        let hidden = departure_study().visible(0.999);
        assert!(hidden.events.is_empty());
        assert!(hidden.mine_departures.is_empty());
        assert!(hidden.mine_contact(0.999, &floor(&hidden)));
    }
}
