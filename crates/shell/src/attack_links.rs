//! Who attacks whom, as the play shows it for the local player's own
//! attacks (`docs/plans/multiplayer-prototype-progress.md`, M8 round 3,
//! S5, and feedback round L; `docs/plans/multiplayer-ui.md`, "Target
//! connection and electrical attack feedback"). CPU against CPU stays
//! what it is: shocks in their fields and the small lock look.
//!
//! - **Outgoing** (the player's full bar committed an attack): a line in
//!   the side space from the field's edge at its power bar to the
//!   recipient's miniature, at its own power bar; a pulse runs along it;
//!   the recipient's miniature is outlined while the attack is on its way
//!   and passing; the line fades.
//! - **Incoming**: the same the other way, from the sender's miniature to
//!   the warning by the receptors (`crate::battle_cue`), the sender's
//!   miniature outlined. The warning stands at the field's right edge: a
//!   sender on the left ends at the field's left edge on the receptors'
//!   row instead, never across the field.
//! - **Chained** (one waiting behind the one passing): its line and its
//!   outline quieter.
//!
//! `attack-links-v1` (feedback round L): a line is a curve, one cubic
//! from where the attack comes to where it goes, leaving and arriving
//! the natural way (out of the field's edge, into the miniature's edge
//! facing the field, into the warning's outer side). It may pass under
//! the miniatures and is drawn behind them ([`draw_behind`]: the
//! renderer puts it under their bands, `ddi_render::Renderer::
//! under_miniatures`), so a field reads cleanly and the line shows in
//! the gaps and, dimmed, through a band; it keeps out of the play's own
//! band, the HUD, the quit button's place, the status line, the ladder
//! and the warning's place (unless it ends there). Its ends, the outline
//! and where it lands are drawn over the miniatures ([`draw`]).
//!
//! No two lines share a stretch ([`Routes`], [`SPACING_V1`]): a line is
//! laid once, when its link first shows, among the lines laid before it,
//! which never move; it takes the first of its ways (its ends a step
//! further along their edges, its curve wider, tighter or bowed) on
//! which it is never as close as two strokes to another line for longer
//! than a crossing; where none of its ways is free, or more lines show
//! than the bound, it has no line: the outline alone says the attack.
//!
//! Under Reverse the bars stay where they are and the line with them;
//! the warning's end follows the receptors.
//!
//! Reduced motion: no pulse and no fade; the line shown still for its
//! time, then gone; the outline as ever.

use std::collections::HashMap;

use ddi_render::FieldGeometry;
use ddi_render::draw2d::{Blend, Command, DrawList, Quad, Shape};
use ddi_render::{Instance, Shape as Sprite};
use ddi_session::battle::Link;

use crate::opponents::{Fields, Slot};

/// How a link looks over time. PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub id: &'static str,
    /// Seconds the pulse takes along the line.
    pub pulse: f32,
    /// Seconds the line shows from the commitment: whole while the pulse
    /// runs, fading after (whole throughout under reduced motion).
    pub line: f32,
    /// Seconds the outline takes to come in.
    pub outline_in: f32,
    /// How much quieter a chained attack's line and outline are.
    pub quiet: f32,
}

/// `attack-links-v1` (PROVISIONAL): as `attack-links-v0` in time (a pulse
/// of 0.6 s, the line gone after 1.5 s, a chained attack's parts at
/// 0.45); its lines curves under the miniatures, no two sharing a
/// stretch ([`SPACING_V1`]).
pub const ATTACK_LINKS_V1: Look = Look {
    id: "attack-links-v1",
    pulse: 0.6,
    line: 1.5,
    outline_in: 0.2,
    quiet: 0.45,
};

/// How the lines are kept apart, in strokes (a stroke [`thick`] across).
/// PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    pub id: &'static str,
    /// Two lines' middles nearer than this run on top of each other.
    pub near: f32,
    /// The longest stretch two lines may be that near: a crossing (at
    /// about fifteen degrees or more).
    pub crossing: f32,
    /// Between two lines' ends on one edge.
    pub gap: f32,
    /// The ends an edge offers, at most.
    pub ports: usize,
    /// The ways tried for one line, at most.
    pub tries: usize,
    /// The lines shown at once, at most: a link beyond them has its
    /// outline alone.
    pub lines: usize,
}

/// `attack-lines-apart-v1` (PROVISIONAL).
pub const SPACING_V1: Spacing = Spacing {
    id: "attack-lines-apart-v1",
    near: 2.0,
    crossing: 16.0,
    gap: 4.0,
    ports: 6,
    tries: 96,
    lines: 64,
};

/// A line's stroke's width at a logical pixel `ratio`.
pub fn thick(ratio: f32) -> f32 {
    (1.5 * ratio).max(1.0)
}

type P = [f32; 2];

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn mul(a: P, k: f32) -> P {
    [a[0] * k, a[1] * k]
}

fn dot(a: P, b: P) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn length(a: P) -> f32 {
    dot(a, a).sqrt()
}

/// The distance from `p` to the stretch from `a` to `b`, and how far
/// along it (`0..=1`) its nearest place is.
fn off_stretch(p: P, a: P, b: P) -> (f32, f32) {
    let (ab, ap) = (sub(b, a), sub(p, a));
    let l = dot(ab, ab);
    let k = if l > 0.0 { (dot(ap, ab) / l).clamp(0.0, 1.0) } else { 0.0 };
    (length(sub(ap, mul(ab, k))), k)
}


/// A line's curve: a cubic's four control points.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Cubic([P; 4]);

impl Cubic {
    /// From `p0`, leaving the way `t0`, to `p3`, arriving the way `t3`
    /// (both unit), within the room between its ends where it can be:
    /// an S where it goes on the way it left (its handles half that
    /// way), a quarter turn where it arrives across the way it left (its
    /// handles 0.55 of each side, an ellipse's), else a way round (two
    /// fifths of the distance); the handles `scale` times as long, and
    /// both bowed to one side by `bow` handles; `r` a logical pixel.
    fn between(p0: P, t0: P, p3: P, t3: P, scale: f32, bow: f32, r: f32) -> Cubic {
        let d = sub(p3, p0);
        let dist = length(d);
        let (a0, a3) = (dot(d, t0), dot(d, t3));
        let turn = dot(t0, t3);
        let (h0, h3) = if turn > 0.5 && a0 > 0.0 {
            let h = (0.5 * a0 * scale).min(0.95 * a0);
            (h, h)
        } else if turn.abs() <= 0.5 && a0 > 0.0 && a3 > 0.0 {
            ((0.55 * a0 * scale).min(a0), (0.55 * a3 * scale).min(a3))
        } else {
            let h = (0.4 * dist * scale).clamp(12.0 * r, 96.0 * r);
            (h, h)
        };
        // The bow: each handle turned to the same side of the line
        // between the ends.
        let across = [-d[1], d[0]];
        let side = |t: P| {
            let n = [-t[1], t[0]];
            if dot(n, across) < 0.0 { mul(n, -1.0) } else { n }
        };
        Cubic([
            p0,
            add(add(p0, mul(t0, h0)), mul(side(t0), bow * h0)),
            add(sub(p3, mul(t3, h3)), mul(side(t3), bow * h3)),
            p3,
        ])
    }

    fn at(&self, t: f32) -> P {
        let [a, b, c, d] = self.0;
        let u = 1.0 - t;
        let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        [
            a[0] * w0 + b[0] * w1 + c[0] * w2 + d[0] * w3,
            a[1] * w0 + b[1] * w1 + c[1] * w2 + d[1] * w3,
        ]
    }

    /// Its control polygon's length: no shorter than the curve.
    fn reach(&self) -> f32 {
        self.0.windows(2).map(|w| length(sub(w[1], w[0]))).sum()
    }

    /// The curve in straight stretches never further than `tolerance`
    /// from it (eight at least, 48 at most), both ends included.
    fn flatten(&self, tolerance: f32) -> Vec<P> {
        let [a, b, c, d] = self.0;
        let bend = |p: P, q: P, s: P| length(add(sub(p, mul(q, 2.0)), s));
        let most = 6.0 * bend(a, b, c).max(bend(b, c, d));
        let n = ((most / (8.0 * tolerance.max(0.01))).sqrt().ceil() as usize).clamp(8, 48);
        (0..=n).map(|i| self.at(i as f32 / n as f32)).collect()
    }
}

/// One end of a line: the places on its edge a line may take, the
/// nearest to the middle first, and the way a line travels there.
#[derive(Clone, Debug)]
struct End {
    ports: Vec<P>,
    way: P,
    /// How much dearer a line ending here counts ([`cost`]): 1 for an
    /// end's first edge, more for its other edges.
    weight: f32,
}

impl End {
    /// The places from `at`, a `step` apart: `at`, then one forward, one
    /// back, two forward… as far as `back` and `forward` steps, `most`
    /// places at most.
    fn along(at: P, step: P, back: usize, forward: usize, most: usize, way: P) -> End {
        let mut ports = vec![at];
        for k in 1..=back.max(forward) {
            if k <= forward {
                ports.push(add(at, mul(step, k as f32)));
            }
            if k <= back {
                ports.push(sub(at, mul(step, k as f32)));
            }
        }
        ports.truncate(most.max(1));
        End {
            ports,
            way,
            weight: 1.0,
        }
    }
}

/// The curves' shapes tried in turn: the handles' length and the bow.
const SHAPES: [(f32, f32); 9] = [
    (1.0, 0.0),
    (1.0, 0.6),
    (1.0, -0.6),
    (0.6, 0.0),
    (1.6, 0.0),
    (1.0, 1.2),
    (1.0, -1.2),
    (0.6, 0.9),
    (0.6, -0.9),
];

/// How much a line from `a` to `b` is to be passed over for another
/// pair of ends: its plain curve's reach, and more for an S squeezed
/// into a room much narrower than it is long (a tight bend at each end).
fn cost(a: &End, b: &End, r: f32) -> f32 {
    let (p0, p3) = (a.ports[0], b.ports[0]);
    let d = sub(p3, p0);
    let along = dot(d, a.way);
    let tight = if dot(a.way, b.way) > 0.5 && along > 0.0 {
        (length(d) - 2.0 * along).max(0.0)
    } else {
        0.0
    };
    (Cubic::between(p0, a.way, p3, b.way, 1.0, 0.0, r).reach() + tight) * a.weight * b.weight
}

/// The dearest pair of ends tried for a line: so many times the
/// cheapest's cost and so many logical pixels more.
const FAR: [f32; 2] = [1.6, 60.0];

/// A line's curve: one cubic, or two joined at a place it goes round.
type Curve = (Cubic, Option<Cubic>);

/// `curve` in straight stretches ([`Cubic::flatten`]).
fn flatten(curve: &Curve, tolerance: f32) -> Vec<P> {
    let mut path = curve.0.flatten(tolerance);
    if let Some(second) = &curve.1 {
        path.extend(second.flatten(tolerance).into_iter().skip(1));
    }
    path
}

fn within(p: P, b: &[f32; 4]) -> bool {
    p[0] > b[0] && p[0] < b[2] && p[1] > b[1] && p[1] < b[3]
}

/// A line's ways, the best first: for each shape, each pair of ends (the
/// cheapest first, [`cost`]), each pair of their places (the nearest to
/// their middles first), `tries` at most. Ends without a place are left
/// out. A pair whose plain curve goes through one of `keep` goes round
/// it instead: two curves joined just off one of its corners, along its
/// edge there (the two corners that make the shortest ways, and each a
/// step or two further off, so that lines round one corner go beside
/// each other).
fn ways(pairs: &[(End, End)], keep: &[[f32; 4]], tries: usize, r: f32) -> Vec<Curve> {
    let mut pairs: Vec<&(End, End)> = pairs
        .iter()
        .filter(|(a, b)| !a.ports.is_empty() && !b.ports.is_empty())
        .collect();
    pairs.sort_by(|x, y| cost(&x.0, &x.1, r).total_cmp(&cost(&y.0, &y.1, r)));
    // A pair much dearer than the cheapest is a long way round: left
    // out (the outline alone says the attack rather than a detour).
    if let Some(least) = pairs.first().map(|(a, b)| cost(a, b, r)) {
        pairs.retain(|(a, b)| cost(a, b, r) <= least * FAR[0] + FAR[1] * r);
    }
    // What each pair's plain curve goes through first, if anything.
    let off = 6.0 * thick(r);
    let rounds: Vec<Vec<(P, P)>> = pairs
        .iter()
        .map(|(a, b)| {
            let (p0, p3) = (a.ports[0], b.ports[0]);
            let plain = Cubic::between(p0, a.way, p3, b.way, 1.0, 0.0, r).flatten(r);
            let Some(hit) = plain.iter().find_map(|p| keep.iter().find(|k| within(*p, k))) else {
                return Vec::new();
            };
            // Off each corner, the way along the edge that goes on
            // toward the end.
            let d = sub(p3, p0);
            let mut round: Vec<(P, P)> = [(0, 1), (2, 1), (0, 3), (2, 3)]
                .into_iter()
                .filter_map(|(x, y)| {
                    let (sx, sy) = (if x == 0 { -1.0 } else { 1.0 }, if y == 1 { -1.0 } else { 1.0 });
                    let at = [hit[x] + sx * off, hit[y] + sy * off];
                    (!at[0].is_nan() && at[0].is_finite() && at[1].is_finite()).then_some(())?;
                    let way = if d[0].abs() > d[1].abs() {
                        [d[0].signum(), 0.0]
                    } else {
                        [0.0, d[1].signum()]
                    };
                    Some((at, way))
                })
                .collect();
            let reach = |(at, way): &(P, P)| {
                Cubic::between(p0, a.way, *at, *way, 1.0, 0.0, r).reach()
                    + Cubic::between(*at, *way, p3, b.way, 1.0, 0.0, r).reach()
            };
            round.sort_by(|x, y| reach(x).total_cmp(&reach(y)));
            // The two nearest corners, each a little further off it as
            // well: lines round one corner go beside each other.
            let gap = SPACING_V1.gap * thick(r);
            round
                .into_iter()
                .take(2)
                .flat_map(|(at, way)| {
                    let mid = [(hit[0] + hit[2]) / 2.0, (hit[1] + hit[3]) / 2.0];
                    let out = [(at[0] - mid[0]).signum(), (at[1] - mid[1]).signum()];
                    (0..3).map(move |k| (add(at, mul(out, gap * k as f32)), way))
                })
                .collect()
        })
        .collect();
    let mut out = Vec::with_capacity(tries);
    for (i, (scale, bow)) in SHAPES.into_iter().enumerate() {
        // The plain shape over more places; the others over the first.
        let places = if i == 0 { 6 } else { 2 };
        for ((a, b), round) in pairs.iter().zip(&rounds) {
            let mut order: Vec<(usize, usize)> = (0..a.ports.len())
                .flat_map(|ka| (0..b.ports.len()).map(move |kb| (ka, kb)))
                .collect();
            order.sort_by_key(|(ka, kb)| (ka + kb, *ka));
            order.truncate(if round.is_empty() { places } else { 2 });
            for (ka, kb) in order {
                let (p0, p3) = (a.ports[ka], b.ports[kb]);
                if round.is_empty() {
                    if out.len() == tries {
                        return out;
                    }
                    out.push((Cubic::between(p0, a.way, p3, b.way, scale, bow, r), None));
                }
                // Round a corner: the plain and the tighter shapes only.
                for (at, way) in round.iter().filter(|_| bow == 0.0 && scale <= 1.0) {
                    if out.len() == tries {
                        return out;
                    }
                    out.push((
                        Cubic::between(p0, a.way, *at, *way, scale, 0.0, r),
                        Some(Cubic::between(*at, *way, p3, b.way, scale, 0.0, r)),
                    ));
                }
            }
        }
    }
    out
}

/// What a line is laid by: the pairs of ends it may join, what it keeps
/// out of (`[left, top, right, bottom]`), what it keeps out of where it
/// can (the first so many of `keep` are the rest: with no way clear of
/// all of them, it is laid clear of the rest alone) and the screen it
/// stays on.
struct Plan {
    pairs: Vec<(End, End)>,
    keep: Vec<[f32; 4]>,
    /// How many of `keep`, from its start, are kept out of always.
    always: usize,
    bounds: [f32; 4],
}

/// Where the lines laid lie: each one's stretches by the cells of a grid
/// over the screen, so a new line asks only about the lines near it.
#[derive(Clone, Debug, Default)]
struct Cells {
    cell: f32,
    across: usize,
    down: usize,
    /// Each cell's first entry in `entries`, plus one (0: none).
    heads: Vec<u32>,
    /// A line, one of its stretches, and the cell's next entry plus one.
    entries: Vec<(u16, u16, u32)>,
}

impl Cells {
    fn reset(&mut self, size: P, cell: f32) {
        self.cell = cell;
        self.across = (size[0] / cell).ceil() as usize + 1;
        self.down = (size[1] / cell).ceil() as usize + 1;
        self.heads.clear();
        self.heads.resize(self.across * self.down, 0);
        self.entries.clear();
    }

    fn cell_of(&self, p: P) -> Option<(usize, usize)> {
        let (x, y) = (p[0] / self.cell, p[1] / self.cell);
        (x >= 0.0 && y >= 0.0 && (x as usize) < self.across && (y as usize) < self.down)
            .then_some((x as usize, y as usize))
    }

    /// Notes `line`'s `path`, walked in steps of `step`.
    fn add(&mut self, line: u16, path: &[P], step: f32) {
        let mut last = None;
        walk(path, step, |p, stretch| {
            if let Some((x, y)) = self.cell_of(p) {
                let at = y * self.across + x;
                if last != Some((at, stretch)) {
                    last = Some((at, stretch));
                    self.entries.push((line, stretch as u16, self.heads[at]));
                    self.heads[at] = self.entries.len() as u32;
                }
            }
            true
        });
    }

    /// Every line's stretch noted in the cell of `p` and the eight
    /// around it.
    fn around(&self, p: P, mut each: impl FnMut(u16, u16)) {
        let Some((x, y)) = self.cell_of(p) else {
            return;
        };
        for cy in y.saturating_sub(1)..(y + 2).min(self.down) {
            for cx in x.saturating_sub(1)..(x + 2).min(self.across) {
                let mut at = self.heads[cy * self.across + cx];
                while at != 0 {
                    let (line, stretch, next) = self.entries[at as usize - 1];
                    each(line, stretch);
                    at = next;
                }
            }
        }
    }
}

/// Walks `path` from its start in steps of `step` along it (its end
/// too), telling each place and the stretch it is on, until `each` says
/// to stop: whether it walked to the end.
fn walk(path: &[P], step: f32, mut each: impl FnMut(P, usize) -> bool) -> bool {
    let mut due = 0.0;
    for (i, w) in path.windows(2).enumerate() {
        let len = length(sub(w[1], w[0]));
        while due <= len {
            let k = if len > 0.0 { due / len } else { 0.0 };
            if !each(add(w[0], mul(sub(w[1], w[0]), k)), i) {
                return false;
            }
            due += step;
        }
        due -= len;
    }
    match path.last() {
        Some(end) if path.len() >= 2 => each(*end, path.len() - 2),
        _ => true,
    }
}

/// A line's ends: between the player and the miniature in a slot
/// ([`Layout::path`]), or between two fields of a grid
/// ([`GridLayout::path`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Way {
    Own { slot: usize, outgoing: bool },
    Between { from: usize, to: usize },
}

/// A line laid: whose it is, its curve in straight stretches (empty: no
/// way was free) and the frame that asked for it last.
#[derive(Clone, Debug)]
struct Laid {
    key: (u32, Way),
    path: Vec<P>,
    /// How far along the line each of `path`'s places is.
    lengths: Vec<f32>,
    seen: u64,
}

/// What laying the lines came to, since the slots were placed: the lines
/// laid, those that found no free way (their outline alone shows), and
/// the ways tried in all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub laid: u32,
    pub without: u32,
    pub tried: u32,
}

/// The lines laid for the slots as they are placed now, each kept while
/// its link shows: a line's way depends on its ends' places and on the
/// lines that were there when it came, never on the time, so it is laid
/// once and never moves while it shows (a line does not jump when another
/// comes or goes; a new one takes a way free of those showing). The
/// caller keeps one and clears it whenever the slots are placed again.
#[derive(Clone, Debug)]
pub struct Routes {
    spacing: Spacing,
    lines: Vec<Option<Laid>>,
    by_key: HashMap<(u32, Way), usize>,
    cells: Cells,
    /// A line went since the cells were filled.
    stale: bool,
    frame: u64,
    /// The screen and the logical pixel the cells are for.
    size: P,
    ratio: f32,
    /// A line being laid: each line it is near now, how long it has
    /// been, the place of its walk that said so last, and the stretch of
    /// that other line (from where to where along it) it has been near.
    runs: Vec<(u16, f32, usize, [f32; 2])>,
    tally: Tally,
}

impl Default for Routes {
    fn default() -> Routes {
        Routes::spaced(SPACING_V1)
    }
}

impl Routes {
    pub fn spaced(spacing: Spacing) -> Routes {
        Routes {
            spacing,
            lines: Vec::new(),
            by_key: HashMap::new(),
            cells: Cells::default(),
            stale: true,
            frame: 0,
            size: [0.0; 2],
            ratio: 0.0,
            runs: Vec::new(),
            tally: Tally::default(),
        }
    }

    /// Forgets every line: the slots were placed again.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.by_key.clear();
        self.stale = true;
        self.tally = Tally::default();
    }

    /// What laying the lines came to since [`Routes::clear`].
    pub fn tally(&self) -> Tally {
        self.tally
    }

    /// The lines showing a curve now.
    pub fn showing(&self) -> usize {
        self.lines.iter().flatten().filter(|l| !l.path.is_empty()).count()
    }

    /// A frame's links are about to ask for their lines, on a screen of
    /// `size` at a logical pixel `ratio`.
    fn begin(&mut self, size: P, ratio: f32) {
        if size != self.size || ratio != self.ratio {
            self.clear();
            self.size = size;
            self.ratio = ratio;
        }
        self.frame += 1;
    }

    /// The frame's links asked: the lines none asked for are gone.
    fn end(&mut self) {
        let frame = self.frame;
        for slot in &mut self.lines {
            if slot.as_ref().is_some_and(|l| l.seen != frame) {
                let gone = slot.take().expect("a line");
                self.by_key.remove(&gone.key);
                self.stale |= !gone.path.is_empty();
            }
        }
    }

    /// How near is too near when a line is laid: a fifth more than
    /// [`Spacing::near`], for the steps it is measured in.
    fn near(&self) -> f32 {
        self.spacing.near * thick(self.ratio) * 1.2
    }

    /// The line of `key`: as laid, or laid now by `plan` among the lines
    /// showing.
    fn line(&mut self, key: (u32, Way), plan: impl FnOnce() -> Plan) -> Vec<P> {
        if let Some(&i) = self.by_key.get(&key)
            && let Some(l) = self.lines[i].as_mut()
        {
            l.seen = self.frame;
            return l.path.clone();
        }
        let near = self.near();
        if self.stale {
            self.cells.reset(self.size, near * 1.5);
            for (i, l) in self.lines.iter().enumerate() {
                if let Some(l) = l {
                    self.cells.add(i as u16, &l.path, near * 0.5);
                }
            }
            self.stale = false;
        }
        let mut path = Vec::new();
        if self.showing() < self.spacing.lines {
            let mut plan = plan();
            // An end's places taken by the lines showing are not its
            // to take: every line ends on a place of its own.
            let gap = self.spacing.gap * thick(self.ratio) - 0.01;
            let taken = |p: &P| {
                self.lines.iter().flatten().any(|l| {
                    [l.path.first(), l.path.last()]
                        .into_iter()
                        .flatten()
                        .any(|e| length(sub(*e, *p)) < gap)
                })
            };
            for (a, b) in &mut plan.pairs {
                a.ports.retain(|p| !taken(p));
                b.ports.retain(|p| !taken(p));
            }
            // Clear of everything first; then of what it must be alone.
            let passes = if plan.always < plan.keep.len() { 2 } else { 1 };
            for pass in 0..passes {
                if pass == 1 {
                    plan.keep.truncate(plan.always);
                }
                for way in &ways(&plan.pairs, &plan.keep, self.spacing.tries, self.ratio) {
                    self.tally.tried += 1;
                    let flat = flatten(way, 0.3 * self.ratio.max(0.5));
                    if self.free(&flat, &plan) {
                        path = flat;
                        break;
                    }
                }
                if !path.is_empty() {
                    break;
                }
            }
        }
        if path.is_empty() {
            self.tally.without += 1;
        } else {
            self.tally.laid += 1;
        }
        let at = match self.lines.iter().position(Option::is_none) {
            Some(at) => at,
            None => {
                self.lines.push(None);
                self.lines.len() - 1
            }
        };
        self.cells.add(at as u16, &path, near * 0.5);
        let mut along = 0.0;
        let lengths = (0..path.len())
            .map(|i| {
                along += if i > 0 { length(sub(path[i], path[i - 1])) } else { 0.0 };
                along
            })
            .collect();
        self.lines[at] = Some(Laid {
            key,
            path: path.clone(),
            lengths,
            seen: self.frame,
        });
        self.by_key.insert(key, at);
        path
    }

    /// Whether `path` is on the screen, out of what `plan` keeps clear,
    /// and never near another line for longer than a crossing.
    fn free(&mut self, path: &[P], plan: &Plan) -> bool {
        let near = self.near();
        let step = near * 0.5;
        let crossing = self.spacing.crossing * thick(self.ratio);
        let [l, t, r, b] = plan.bounds;
        let mut runs = std::mem::take(&mut self.runs);
        runs.clear();
        let (cells, lines) = (&self.cells, &self.lines);
        let mut i = 0usize;
        // The places looked at are a step apart: what is kept clear
        // counts a step larger, but at a line's ends, which may lie on
        // an edge of it.
        let last = (path_length(path) / step) as usize;
        let free = walk(path, step, |p, _| {
            i += 1;
            if p[0] < l || p[0] > r || p[1] < t || p[1] > b {
                return false;
            }
            let grown = if i <= 3 || i + 3 > last { 0.0 } else { step };
            if plan.keep.iter().any(|k| {
                p[0] > k[0] - grown && p[0] < k[2] + grown && p[1] > k[1] - grown && p[1] < k[3] + grown
            }) {
                return false;
            }
            let mut clear = true;
            cells.around(p, |line, stretch| {
                let seen = runs.iter().position(|(other, ..)| *other == line);
                let Some(other) = lines.get(usize::from(line)).and_then(Option::as_ref) else {
                    return;
                };
                let q = &other.path;
                let s = usize::from(stretch);
                // Where along the other line it is nearest, if near.
                let Some(along) = (s.saturating_sub(1)..(s + 2).min(q.len().saturating_sub(1)))
                    .filter_map(|s| {
                        let (off, k) = off_stretch(p, q[s], q[s + 1]);
                        let len = length(sub(q[s + 1], q[s]));
                        (off < near).then(|| (off, other.lengths[s] + k * len))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, along)| along)
                else {
                    return;
                };
                match seen {
                    Some(at) => {
                        let run = &mut runs[at];
                        if run.2 == i {
                            // Another part of it near the same place.
                            run.3 = [run.3[0].min(along), run.3[1].max(along)];
                        } else if run.2 + 1 == i {
                            run.1 += step;
                            run.3 = [run.3[0].min(along), run.3[1].max(along)];
                        } else {
                            run.1 = 0.0;
                            run.3 = [along; 2];
                        }
                        run.2 = i;
                        // Neither line beside the other for longer than
                        // a crossing.
                        clear &= run.1 <= crossing && run.3[1] - run.3[0] <= crossing;
                    }
                    None => runs.push((line, 0.0, i, [along; 2])),
                }
            });
            clear
        });
        self.runs = runs;
        free
    }
}

/// Where a link is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The pulse runs along the line.
    Pulse,
    /// The line fades.
    Fade,
    /// Reduced motion: the line stands still for its time.
    Still,
    /// The line is gone; the miniature stays outlined until the attack
    /// has passed.
    Outline,
}

impl Phase {
    pub fn id(self) -> &'static str {
        match self {
            Phase::Pulse => "pulse",
            Phase::Fade => "fade",
            Phase::Still => "still",
            Phase::Outline => "outline",
        }
    }
}

/// A link as a frame draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub attack: u32,
    /// The opponent's seat and the slot its miniature shows in.
    pub seat: usize,
    pub slot: usize,
    /// From the player to the opponent (else from the opponent to the
    /// player).
    pub outgoing: bool,
    pub chained: bool,
    pub phase: Phase,
    /// The line's opacity, `0..=1` (0 once gone).
    pub line: f32,
    /// The pulse's place along the line, `0..=1`, while it runs.
    pub pulse: Option<f32>,
    /// The outline's opacity, `0..=1`.
    pub outline: f32,
    /// The line's curve in straight stretches, target pixels, from where
    /// the attack comes to where it goes (empty where no way was free:
    /// the outline alone says it).
    pub path: Vec<[f32; 2]>,
    /// A watched stage's grid ([`shown_between`]): the sender's seat (the
    /// link is then between two competitors, `seat` its recipient).
    pub from: Option<usize>,
}

/// What a link's line needs of the play's layout: the field `geo`
/// (`span` lanes wide), the miniatures' `fields`, a logical pixel
/// `ratio` and what the line keeps clear of besides the band (`[left,
/// top, right, bottom]`: the HUD's areas and the quit button's place;
/// the warning's own place is where an incoming line ends).
#[derive(Clone, Copy)]
pub struct Layout<'a> {
    pub geo: &'a FieldGeometry,
    pub span: f32,
    pub fields: &'a Fields,
    pub ratio: f32,
    pub keep_out: &'a [[f32; 4]],
}

/// `fields`' status line and ladder, `[left, top, right, bottom]` each.
fn words(fields: &Fields) -> impl Iterator<Item = [f32; 4]> {
    [Some(fields.status), fields.ladder]
        .into_iter()
        .flatten()
        .filter(|[_, _, w, h]| *w > 0.0 && *h > 0.0)
        .map(|[x, y, w, h]| [x, y, x + w, y + h])
}

/// How much dearer an end's other edges count than its first.
const OTHER_EDGE: f32 = 1.3;

/// A miniature's ends for a line traveling `toward` it (1) or away
/// from it (-1), `out` the way from the field to it (1 to the right):
/// its edge at `x` from its power bar's height down, the line going the
/// way `out` (toward) or against it; and its top edge from its middle
/// outward, where its bar is, the line coming down onto it or leaving
/// upward (a sweep under the miniatures beside it rather than an S
/// squeezed into the gap before it). A `gap` apart, as far as its band
/// goes.
fn mini_ends(slot: &Slot, x: f32, out: f32, toward: f32, span: f32, gap: f32, most: usize) -> [End; 2] {
    let (mini, _) = ddi_render::scene::miniature_power_and_life(&slot.field, slot.panel, span);
    let y = mini[1] + mini[3] / 2.0;
    let down = ((slot.panel[3] - y) / gap - 1.0).max(0.0) as usize;
    let across = ((slot.panel[2] - slot.panel[0]) / 2.0 / gap - 1.0).max(0.0) as usize;
    let middle = (slot.panel[0] + slot.panel[2]) / 2.0;
    [
        End::along([x, y], [0.0, gap], 0, down, most, [out * toward, 0.0]),
        End {
            weight: OTHER_EDGE,
            ..End::along(
                [middle, slot.panel[1]],
                [gap, 0.0],
                across,
                across,
                most,
                [0.0, toward],
            )
        },
    ]
}

/// The warning in `cue` (`[left, top, right, bottom]`) as a line's end:
/// its outer side, the line arriving toward the field.
fn warning_side(cue: [f32; 4], gap: f32, most: usize) -> End {
    let cy = (cue[1] + cue[3]) / 2.0;
    let room = ((cue[3] - cue[1]) / 2.0 / gap - 0.5).max(0.0) as usize;
    End::along([cue[2], cy], [0.0, gap], room, room, most, [-1.0, 0.0])
}

/// The warning's ends: its outer side, then its top and its bottom, a
/// line coming down or up onto them.
fn warning_ends(cue: [f32; 4], gap: f32, most: usize) -> [End; 3] {
    let cx = (cue[0] + cue[2]) / 2.0;
    let room = ((cue[2] - cue[0]) / 2.0 / gap - 0.5).max(0.0) as usize;
    let other = |y: f32, way: P| End {
        weight: OTHER_EDGE,
        ..End::along([cx, y], [gap, 0.0], room, room, most, way)
    };
    [
        warning_side(cue, gap, 2 * most + 1),
        other(cue[1], [0.0, 1.0]),
        other(cue[3], [0.0, -1.0]),
    ]
}

impl Layout<'_> {
    /// The band's left and right edges.
    fn band(&self) -> [f32; 2] {
        let [l, _, r, _] = ddi_render::scene::band_box(self.geo, self.span);
        [l, r]
    }

    /// What a line stays out of: the field's band (the HUD's bars and
    /// the judgment are in it; a hair less, a line's end being on its
    /// edge), the keep-outs, the status line and the ladder; and how
    /// many they are, before what it stays out of where it can: for a
    /// line that does not end there, the warning's place (the warning is
    /// drawn over a line that has no other way, as a miniature is: a
    /// phone held upright leaves no room beside it). Never a miniature:
    /// a line goes under them.
    fn keep(&self, warning: bool) -> (Vec<[f32; 4]>, usize) {
        let [band_l, band_r] = self.band();
        let mut out = vec![[band_l + 0.75, f32::NEG_INFINITY, band_r - 0.75, f32::INFINITY]];
        out.extend(self.keep_out.iter().copied());
        out.extend(words(self.fields));
        let always = out.len();
        if warning {
            out.push(crate::battle_cue::place(self.geo, self.span));
        }
        (out, always)
    }

    /// How the line of an attack between the player and the miniature in
    /// `slot` is laid: from the field's edge at its power bar to the
    /// miniature's edge facing the field at its own bar (`outgoing`), or
    /// from the miniature to the warning's outer side (a miniature on
    /// the right; one on the left ends at the field's left edge on the
    /// receptors' row); at the miniature, its edge facing the field or
    /// its top edge ([`mini_ends`]).
    fn plan(&self, slot: &Slot, outgoing: bool, spacing: Spacing) -> Plan {
        let r = self.ratio;
        let gap = spacing.gap * thick(r);
        let most = spacing.ports;
        let [band_l, band_r] = self.band();
        let (side, out) = if slot.left { (band_l, -1.0) } else { (band_r, 1.0) };
        let edge = if slot.left { slot.panel[2] } else { slot.panel[0] };
        let pairs = if outgoing {
            // From the band's edge at the power bar.
            let (power, _) = ddi_render::scene::power_and_life(self.geo, self.span);
            let y = power[1] + power[3] / 2.0;
            let back = (((y - 2.0 * r) / gap).max(0.0) as usize).min(2);
            let from = End::along([side, y], [0.0, gap], back, 3, most, [out, 0.0]);
            mini_ends(slot, edge, out, 1.0, self.span, gap, most)
                .map(|to| (from.clone(), to))
                .to_vec()
        } else {
            let to = if slot.left {
                // To the field's left edge on the receptors' row.
                let at = [side, self.geo.receptor_y];
                vec![End::along(at, [0.0, gap], most, most, 2 * most + 1, [1.0, 0.0])]
            } else {
                warning_ends(crate::battle_cue::place(self.geo, self.span), gap, most).to_vec()
            };
            mini_ends(slot, edge, out, -1.0, self.span, gap, most)
                .iter()
                .flat_map(|from| to.iter().map(move |to| (from.clone(), to.clone())))
                .collect()
        };
        let (keep, always) = self.keep(outgoing);
        Plan {
            pairs,
            keep,
            always,
            bounds: [r, r, self.geo.width - r, self.geo.height - r],
        }
    }

    /// The line of an attack between the player and the miniature in
    /// `slot`, laid alone ([`Routes`] lays a frame's lines among each
    /// other): its curve in straight stretches. Empty where no way is
    /// clear.
    pub fn path(&self, slot: &Slot, outgoing: bool) -> Vec<[f32; 2]> {
        let mut routes = Routes::default();
        routes.begin([self.geo.width, self.geo.height], self.ratio);
        let way = Way::Own { slot: 0, outgoing };
        routes.line((0, way), || self.plan(slot, outgoing, SPACING_V1))
    }
}

/// The local player's `links` as the frame at judged song second `t`
/// shows them over `layout`, under `look`; those whose opponent's
/// miniature is not in view (taking turns beyond the cap, before
/// the stage's kept seats brought it in) are left out. The lines are
/// laid in `routes`, in the links' order, and kept there while they
/// show.
pub fn shown(
    links: &[Link],
    layout: &Layout,
    routes: &mut Routes,
    t: f64,
    reduced: bool,
    look: Look,
) -> Vec<Shown> {
    routes.begin([layout.geo.width, layout.geo.height], layout.ratio);
    let spacing = routes.spacing;
    let out = links
        .iter()
        .filter_map(|l| {
            let slot = layout.fields.slots.iter().position(|s| s.seat == l.seat)?;
            let (phase, line, pulse, outline) = timing(l, t, reduced, look);
            let path = if line > 0.0 {
                let way = Way::Own {
                    slot,
                    outgoing: l.outgoing,
                };
                routes.line((l.attack, way), || {
                    layout.plan(&layout.fields.slots[slot], l.outgoing, spacing)
                })
            } else {
                Vec::new()
            };
            Some(Shown {
                attack: l.attack,
                seat: l.seat,
                slot,
                outgoing: l.outgoing,
                chained: l.chained,
                phase,
                line,
                pulse,
                outline,
                path,
                from: None,
            })
        })
        .collect();
    routes.end();
    out
}

/// What a line between two fields of a watched stage's grid needs
/// (`crate::opponents::arrange_grid`): the fields, the screen's `size`
/// (target pixels), the lanes' `span`, a logical pixel `ratio` and what
/// the line keeps clear of (the quit button's place; a player's own
/// field's band).
#[derive(Clone, Copy)]
pub struct GridLayout<'a> {
    pub fields: &'a Fields,
    pub size: [f32; 2],
    pub span: f32,
    pub ratio: f32,
    pub keep_out: &'a [[f32; 4]],
}

impl GridLayout<'_> {
    /// How the line of an attack from the field in `from` to the field
    /// in `to` is laid: from the sender's field at its power bar, out of
    /// either side, to the warning beside the recipient's field, onto
    /// its outer side, its top or its bottom, the shortest first; under
    /// the fields, out of the keep-outs and the status line.
    fn plan(&self, from: &Slot, to: &Slot, spacing: Spacing) -> Plan {
        let r = self.ratio;
        let gap = spacing.gap * thick(r);
        let most = spacing.ports;
        // (Each side's top edge is the one top edge: once is enough.)
        let [left, up] = mini_ends(from, from.panel[0], 1.0, -1.0, self.span, gap, most);
        let [right, _] = mini_ends(from, from.panel[2], -1.0, -1.0, self.span, gap, most);
        let starts = [left, right, up];
        let ends = warning_ends(crate::battle_cue::place(&to.field, self.span), gap, most);
        let pairs = starts
            .iter()
            .flat_map(|a| ends.iter().map(move |b| (a.clone(), b.clone())))
            .collect();
        let mut keep: Vec<[f32; 4]> = self.keep_out.to_vec();
        keep.extend(words(self.fields));
        Plan {
            pairs,
            always: keep.len(),
            keep,
            bounds: [r, r, self.size[0] - r, self.size[1] - r],
        }
    }

    /// The line of an attack from the field in `from` to the field in
    /// `to`, laid alone: its curve in straight stretches. Empty where no
    /// way is clear.
    pub fn path(&self, from: &Slot, to: &Slot) -> Vec<[f32; 2]> {
        let mut routes = Routes::default();
        routes.begin(self.size, self.ratio);
        let way = Way::Between { from: 0, to: 1 };
        routes.line((0, way), || self.plan(from, to, SPACING_V1))
    }
}

/// Who attacks whom among the fields of a watched stage's grid
/// (`watch-v0`): each of `links` (between two competitors) as the frame
/// at judged song second `t` shows it over `layout`, under `look`: its
/// line from the sender's field to the warning beside the recipient's,
/// the recipient's field outlined while the attack is on its way and
/// passing. Those with an end not in view are left out. The lines are
/// laid in `routes`, in the links' order, and kept there while they
/// show.
pub fn shown_between(
    links: &[ddi_session::battle::SeenLink],
    layout: &GridLayout,
    routes: &mut Routes,
    t: f64,
    reduced: bool,
    look: Look,
) -> Vec<Shown> {
    routes.begin(layout.size, layout.ratio);
    let spacing = routes.spacing;
    let slot_of = |seat: usize| layout.fields.slots.iter().position(|s| s.seat == seat);
    let out = links
        .iter()
        .filter_map(|l| {
            let (from, to) = (slot_of(l.from)?, slot_of(l.to)?);
            let own = Link {
                attack: l.attack,
                seat: l.to,
                outgoing: true,
                shown_from: l.shown_from,
                first: l.first,
                clear: l.clear,
                chained: l.chained,
            };
            let (phase, line, pulse, outline) = timing(&own, t, reduced, look);
            let path = if line > 0.0 && from != to {
                routes.line((l.attack, Way::Between { from, to }), || {
                    layout.plan(&layout.fields.slots[from], &layout.fields.slots[to], spacing)
                })
            } else {
                Vec::new()
            };
            Some(Shown {
                attack: l.attack,
                seat: l.to,
                slot: to,
                outgoing: true,
                chained: l.chained,
                phase,
                line,
                pulse,
                outline,
                path,
                from: Some(l.from),
            })
        })
        .collect();
    routes.end();
    out
}

/// A link's phase, its line's opacity, its pulse's place and its
/// outline's opacity at judged song second `t` (a chained one quieter).
fn timing(l: &Link, t: f64, reduced: bool, look: Look) -> (Phase, f32, Option<f32>, f32) {
    let age = (t - l.shown_from).max(0.0) as f32;
    let quiet = if l.chained { look.quiet } else { 1.0 };
    let (phase, line, pulse) = if age >= look.line {
        (Phase::Outline, 0.0, None)
    } else if reduced {
        (Phase::Still, 1.0, None)
    } else if age < look.pulse {
        (Phase::Pulse, 1.0, Some(age / look.pulse.max(1e-3)))
    } else {
        let left = 1.0 - (age - look.pulse) / (look.line - look.pulse).max(1e-3);
        (Phase::Fade, left.clamp(0.0, 1.0), None)
    };
    let outline = if reduced {
        1.0
    } else {
        (age / look.outline_in.max(1e-3)).clamp(0.0, 1.0)
    };
    (phase, line * quiet, pulse, outline * quiet)
}

fn with_alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], c[3] * a]
}

/// A line's length, stretch by stretch.
fn path_length(path: &[[f32; 2]]) -> f32 {
    path.windows(2).map(|w| length(sub(w[1], w[0]))).sum()
}

/// The point `along` (`0..=1`) the line `path`, by its length: equal
/// steps of `along` are equal distances along the curve.
fn point_along(path: &[[f32; 2]], along: f32) -> [f32; 2] {
    let mut left = path_length(path) * along.clamp(0.0, 1.0);
    for w in path.windows(2) {
        let len = length(sub(w[1], w[0]));
        if left <= len || len <= 0.0 {
            let k = if len > 0.0 { left / len } else { 0.0 };
            return add(w[0], mul(sub(w[1], w[0]), k));
        }
        left -= len;
    }
    path.last().copied().unwrap_or([0.0; 2])
}

/// A dot of radius `r` at `at`.
fn dot_at(at: [f32; 2], r: f32, color: [f32; 4]) -> Command {
    let mut q = Quad::plain([at[0] - r, at[1] - r, at[0] + r, at[1] + r], color);
    q.shape = Shape {
        radius: [r; 2],
        ..Shape::NONE
    };
    q.blend = Blend::Normal;
    Command::Quad(q)
}

/// Draws over the miniatures in `fields` what of `shown` joins them: the
/// outline around each link's miniature's field and the dot where its
/// line lands (the theme's attack colors); `ratio` a logical pixel. The
/// lines themselves go under the miniatures ([`draw_behind`]).
pub fn draw(list: &mut DrawList, shown: &[Shown], fields: &Fields, ratio: f32) {
    let [line, _] = ddi_theme::attack_colors();
    for s in shown {
        // The outline around the opponent's field.
        if s.outline > 0.0
            && let Some(slot) = fields.slots.get(s.slot)
        {
            let grow = 1.5 * ratio;
            let [l, t, r, b] = slot.panel;
            let mut q = Quad::plain(
                [l - grow, t - grow, r + grow, b + grow],
                with_alpha(line, 0.9 * s.outline),
            );
            q.shape = Shape {
                radius: [3.0 * ratio; 2],
                outline: [1.5 * ratio; 2],
                ..Shape::NONE
            };
            list.push(Command::Quad(q));
        }
        if s.line <= 0.0 || s.path.len() < 2 {
            continue;
        }
        // Where it lands.
        if let Some(end) = s.path.last() {
            list.push(dot_at(*end, thick(ratio) * 1.6, with_alpha(line, 0.9 * s.line)));
        }
    }
}

/// The lines of `shown` as the renderer draws them under the miniatures
/// (`ddi_render::Renderer::under_miniatures`), added to `out`: each
/// line's curve, a stretch an instance (its stroke over its glow, the
/// theme's attack colors), and its pulse's glow and dot; `ratio` a
/// logical pixel.
pub fn draw_behind(out: &mut Vec<Instance>, shown: &[Shown], ratio: f32) {
    let [line, glow] = ddi_theme::attack_colors();
    let thick = thick(ratio);
    for s in shown {
        if s.line <= 0.0 || s.path.len() < 2 {
            continue;
        }
        let stroke = with_alpha(line, 0.85 * s.line);
        let halo = [glow[0], glow[1], glow[2], glow[3] * 0.55 * s.line];
        for w in s.path.windows(2) {
            let d = sub(w[1], w[0]);
            let len = length(d);
            if len <= 0.0 {
                continue;
            }
            let mid = mul(add(w[0], w[1]), 0.5);
            let size = [len, thick / ddi_render::sprite::LINK_STROKE];
            out.push(
                Instance::new(mid, size, Sprite::Link, stroke)
                    .params(halo)
                    .rotated(d[1].atan2(d[0])),
            );
        }
        if let Some(p) = s.pulse {
            let at = point_along(&s.path, p);
            let r = 7.0 * ratio;
            out.push(
                Instance::new(
                    at,
                    [2.0 * r; 2],
                    Sprite::Glow,
                    with_alpha(glow, (1.6 * s.line).min(1.0)),
                )
                .params([0.6, 0.0, 0.0, 0.0]),
            );
            out.push(Instance::new(
                at,
                [5.0 * ratio; 2],
                Sprite::Circle,
                with_alpha([1.0; 4], s.line),
            ));
        }
    }
}

/// What the debug state says of the links drawn (`__DDI_DEBUG.battle
/// .attacks.links`): each one's attack, from whom to whom (`me`, or an
/// opponent's seat), its phase, and its line (its curve in straight
/// stretches) and outline as drawn.
pub fn debug(shown: &[Shown]) -> serde_json::Value {
    let end = |me: bool, seat: usize| {
        if me {
            serde_json::json!("me")
        } else {
            serde_json::json!(seat)
        }
    };
    // Between two competitors (a watched stage's grid): the sender's
    // seat in the player's place.
    let from = |s: &Shown| match s.from {
        Some(seat) => serde_json::json!(seat),
        None => end(s.outgoing, s.seat),
    };
    serde_json::Value::Array(
        shown
            .iter()
            .map(|s| {
                serde_json::json!({
                    "attack": s.attack,
                    "from": from(s),
                    "to": end(!s.outgoing, s.seat),
                    "seat": s.seat,
                    "slot": s.slot,
                    "phase": s.phase.id(),
                    "chained": s.chained,
                    "line": s.line,
                    "pulse": s.pulse,
                    "outline": s.outline,
                    "path": s.path,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opponents::tests::{SIZES, keep, keep_grid};
    use ddi_session::battle::SeenLink;

    fn link(seat: usize, outgoing: bool, chained: bool) -> Link {
        Link {
            attack: 1 + seat as u32 + if outgoing { 1000 } else { 0 },
            seat,
            outgoing,
            shown_from: 10.0,
            first: 14.0,
            clear: 18.0,
            chained,
        }
    }

    fn seen(attack: u32, from: usize, to: usize) -> SeenLink {
        SeenLink {
            attack,
            from,
            to,
            shown_from: 10.0,
            first: 14.0,
            clear: 18.0,
            chained: false,
        }
    }

    /// The distance from `p` to the stretch from `a` to `b`.
    fn to_stretch(p: P, a: P, b: P) -> f32 {
        off_stretch(p, a, b).0
    }

    /// `path`'s places a pixel apart.
    fn places(path: &[P]) -> Vec<P> {
        let mut out = Vec::new();
        walk(path, 1.0, |p, _| {
            out.push(p);
            true
        });
        out
    }

    fn inside(p: P, b: [f32; 4]) -> bool {
        p[0] > b[0] && p[0] < b[2] && p[1] > b[1] && p[1] < b[3]
    }

    fn unit(v: P) -> P {
        mul(v, 1.0 / length(v).max(1e-6))
    }

    /// The way a line leaves and the way it arrives.
    fn tangents(path: &[P]) -> (P, P) {
        let n = path.len();
        (unit(sub(path[1], path[0])), unit(sub(path[n - 1], path[n - 2])))
    }

    /// The longest stretch of `a` (pixels along it) nearer than `near`
    /// to `b`, measured afresh: every place of `a` half a pixel apart
    /// against every stretch of `b`.
    fn longest_together(a: &[P], b: &[P], near: f32) -> f32 {
        let (mut run, mut longest) = (0.0f32, 0.0f32);
        walk(a, 0.5, |p, _| {
            if b.windows(2).any(|w| to_stretch(p, w[0], w[1]) < near) {
                run += 0.5;
                longest = longest.max(run);
            } else {
                run = 0.0;
            }
            true
        });
        longest
    }

    /// No two of `shown`'s lines run together: never nearer than two
    /// strokes for longer than a crossing (a quarter more allowed for the
    /// measuring's own steps). The lines counted.
    fn apart(shown: &[Shown], ratio: f32, what: &str) -> usize {
        let t = thick(ratio);
        let (near, crossing) = (SPACING_V1.near * t, SPACING_V1.crossing * t);
        let lines: Vec<&Shown> = shown.iter().filter(|s| s.path.len() >= 2).collect();
        for (i, a) in lines.iter().enumerate() {
            for b in &lines[i + 1..] {
                let together = longest_together(&a.path, &b.path, near)
                    .max(longest_together(&b.path, &a.path, near));
                assert!(
                    together <= crossing * 1.25 + 2.0,
                    "{what}: attacks {} and {} run together for {together} px (a crossing is {crossing})",
                    a.attack,
                    b.attack
                );
            }
        }
        lines.len()
    }

    /// A small generator for the arrangements' links.
    struct Draws(u64);

    impl Draws {
        fn next(&mut self, n: usize) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % n.max(1)
        }
    }

    /// A watched stage's grid: at every size of the miniatures' table,
    /// with and without Reverse, for two to eight fields, the line of
    /// every attack from one field to another starts at the sender's
    /// field's side from its power bar down, leaving outward, ends on the
    /// warning's place beside the recipient's field, arriving into it,
    /// stays on the screen and is never over the status line or the quit
    /// button's place (it goes under the fields); and a link shows its
    /// line for its time and outlines the recipient.
    #[test]
    fn a_grid_s_lines_go_between_its_fields() {
        let (mut lines, mut none) = (0, 0);
        for (w, h, ratio) in SIZES {
            for reverse in [false, true] {
                for n in 2..=8 {
                    let g = FieldGeometry::placed(
                        w * ratio,
                        h * ratio,
                        4.0,
                        reverse,
                        Default::default(),
                    );
                    let k = keep_grid(&g, ratio);
                    let f = crate::opponents::arrange_grid(n, usize::MAX, &g, 4.0, reverse, ratio, &k).unwrap();
                    let what = format!("{w}x{h}@{ratio} reverse {reverse} n {n}");
                    let layout = GridLayout {
                        fields: &f,
                        size: [g.width, g.height],
                        span: 4.0,
                        ratio,
                        keep_out: &k,
                    };
                    let gap = SPACING_V1.gap * thick(ratio);
                    for (i, from) in f.slots.iter().enumerate() {
                        for (j, to) in f.slots.iter().enumerate() {
                            if i == j {
                                continue;
                            }
                            let p = layout.path(from, to);
                            if p.is_empty() {
                                none += 1;
                                continue;
                            }
                            lines += 1;
                            let (mini, _) = ddi_render::scene::miniature_power_and_life(
                                &from.field,
                                from.panel,
                                4.0,
                            );
                            let (start, end) = (p[0], p[p.len() - 1]);
                            let (leaves, arrives) = tangents(&p);
                            if (start[1] - from.panel[1]).abs() < 0.01 {
                                // Out of its top edge, upward.
                                assert!(
                                    start[0] > from.panel[0] && start[0] < from.panel[2],
                                    "{what} {i}→{j}: {start:?}"
                                );
                                assert!(leaves[1] < 0.0, "{what} {i}→{j}: leaves {leaves:?}");
                            } else {
                                let left = (start[0] - from.panel[0]).abs() < 0.01;
                                assert!(
                                    left || (start[0] - from.panel[2]).abs() < 0.01,
                                    "{what} {i}→{j}: {start:?}"
                                );
                                assert!(
                                    start[1] >= mini[1]
                                        && start[1] <= mini[1] + mini[3] + gap * SPACING_V1.ports as f32
                                        && start[1] <= from.panel[3],
                                    "{what} {i}→{j}"
                                );
                                // Out of the side it starts on.
                                let out = if left { -1.0 } else { 1.0 };
                                assert!(leaves[0] * out > -0.02, "{what} {i}→{j}: leaves {leaves:?}");
                            }
                            let cue = crate::battle_cue::place(&to.field, 4.0);
                            assert!(
                                end[0] >= cue[0] - 0.5
                                    && end[0] <= cue[2] + 0.5
                                    && end[1] >= cue[1] - 0.5
                                    && end[1] <= cue[3] + 0.5,
                                "{what} {i}→{j}: {end:?} {cue:?}"
                            );
                            // Into the warning: toward its middle.
                            let middle = [(cue[0] + cue[2]) / 2.0, (cue[1] + cue[3]) / 2.0];
                            let before = sub(end, mul(arrives, 4.0));
                            assert!(
                                length(sub(before, middle)) > length(sub(end, middle)),
                                "{what} {i}→{j}: arrives {arrives:?}"
                            );
                            let [x, y, sw, sh] = f.status;
                            for at in places(&p) {
                                assert!(
                                    at[0] >= 0.0 && at[0] <= g.width && at[1] >= 0.0 && at[1] <= g.height,
                                    "{what} {i}→{j} off the screen"
                                );
                                for keep in &k {
                                    assert!(!inside(at, *keep), "{what} {i}→{j} over {keep:?}");
                                }
                                assert!(
                                    !inside(at, [x, y, x + sw, y + sh]),
                                    "{what} {i}→{j} over the status"
                                );
                            }
                        }
                    }
                }
            }
        }
        eprintln!("{lines} lines, {none} without a way");
        assert!(lines > 3000 && none * 50 < lines, "{lines} lines, {none} without");
        // A link over its time.
        let g = FieldGeometry::placed(1280.0, 720.0, 4.0, false, Default::default());
        let k = keep_grid(&g, 1.0);
        let f = crate::opponents::arrange_grid(4, usize::MAX, &g, 4.0, false, 1.0, &k).unwrap();
        let layout = GridLayout {
            fields: &f,
            size: [1280.0, 720.0],
            span: 4.0,
            ratio: 1.0,
            keep_out: &k,
        };
        let links = [seen(5, 3, 1)];
        let at = |t: f64, reduced: bool| shown_between(&links, &layout, &mut Routes::default(), t, reduced, ATTACK_LINKS_V1);
        let pulse = &at(10.3, false)[0];
        assert_eq!((pulse.from, pulse.seat, pulse.slot), (Some(3), 1, 1));
        assert_eq!(pulse.phase, Phase::Pulse);
        assert!(pulse.path.len() >= 2 && pulse.line == 1.0);
        let later = &at(13.0, false)[0];
        assert_eq!(later.phase, Phase::Outline);
        assert!(later.path.is_empty() && later.outline == 1.0);
        assert_eq!(at(10.3, true)[0].phase, Phase::Still);
        // The debug state says who attacks whom by seat.
        let d = debug(&at(10.3, false));
        assert_eq!((d[0]["from"].clone(), d[0]["to"].clone()), (3.into(), 1.into()));
        // The same link as the followed competitor's own.
        assert_eq!(links[0].of(1).map(|l| (l.seat, l.outgoing)), Some((3, false)));
        assert_eq!(links[0].of(3).map(|l| (l.seat, l.outgoing)), Some((1, true)));
        assert_eq!(links[0].of(0), None);
        let mut list = DrawList::new();
        draw(&mut list, &at(10.3, false), &f, 1.0);
        assert_eq!(list.commands.len(), 2, "the outline and where it lands");
        // A grid's page shows only the lines between its own fields: an
        // attack from or to a field on another page is left out (no stub
        // at the edge; that field's own page shows it).
        let off = [
            SeenLink { from: 14, ..links[0] },
            SeenLink { to: 20, ..links[0] },
        ];
        assert!(shown_between(&off, &layout, &mut Routes::default(), 10.3, false, ATTACK_LINKS_V1).is_empty());
    }

    /// At every size the miniatures' table uses, with and without
    /// Reverse, for one to 72 opponents: every outgoing and incoming
    /// line, laid alone, is a curve in the side space. It starts at the
    /// band's edge by the power bar, leaving outward (outgoing), or ends
    /// at the warning's outer side or at the band's edge on the
    /// receptors' row, arriving toward the field (incoming); it touches
    /// its own miniature's edge facing the field from its bar down, the
    /// way it travels; and no place of it is in the field's band, the
    /// HUD's areas, the judgment, the quit button's place, the status
    /// line or the ladder (it goes under the miniatures), nor, outgoing,
    /// in the warning's place.
    #[test]
    fn every_line_is_a_curve_in_the_side_space() {
        let (mut lines, mut none, mut under) = (0, 0, 0);
        for (w, h, ratio) in SIZES {
            for reverse in [false, true] {
                for n in [1, 2, 3, 4, 6, 8, 24, 72] {
                    let g = FieldGeometry::placed(
                        w * ratio,
                        h * ratio,
                        4.0,
                        reverse,
                        Default::default(),
                    );
                    let k = keep(&g, ratio, true);
                    let Some(f) = crate::opponents::tests::arrange(n, &g, 4.0, reverse, ratio, &k) else {
                        continue;
                    };
                    let what = format!("{w}x{h}@{ratio} reverse {reverse} n {n}");
                    // The line's own keep-outs: the HUD and the quit
                    // button (the warning's place is where it ends).
                    let mut own = ddi_render::scene::hud_areas(&g, 4.0);
                    own.push(k[own.len()]);
                    let layout = Layout {
                        geo: &g,
                        span: 4.0,
                        fields: &f,
                        ratio,
                        keep_out: &own,
                    };
                    let [mid, band] = ddi_render::scene::filter_band(&g, 4.0);
                    let (bl, br) = (mid - band / 2.0, mid + band / 2.0);
                    let band_box = [bl + 1.0, 0.0, br - 1.0, g.height];
                    let cue = crate::battle_cue::place(&g, 4.0);
                    let (power, _) = ddi_render::scene::power_and_life(&g, 4.0);
                    let judgment = ddi_render::scene::judgement_area(&g, 4.0);
                    let gap = SPACING_V1.gap * thick(ratio);
                    for (i, slot) in f.slots.iter().enumerate() {
                        for outgoing in [true, false] {
                            let p = layout.path(slot, outgoing);
                            if p.is_empty() {
                                eprintln!("{what} slot {i} outgoing {outgoing}: no way");
                                none += 1;
                                continue;
                            }
                            lines += 1;
                            let (mini, _) = ddi_render::scene::miniature_power_and_life(
                                &slot.field,
                                slot.panel,
                                4.0,
                            );
                            let at_mini = if outgoing { p[p.len() - 1] } else { p[0] };
                            let other = if outgoing { p[0] } else { p[p.len() - 1] };
                            let (leaves, arrives) = tangents(&p);
                            let out = if slot.left { -1.0 } else { 1.0 };
                            // At the miniature: its field-facing edge,
                            // from its power bar down, or its top edge,
                            // where its bar is; the way it travels there.
                            let edge = if slot.left { slot.panel[2] } else { slot.panel[0] };
                            let top = (at_mini[1] - slot.panel[1]).abs() < 0.01;
                            let there = if outgoing { arrives } else { mul(leaves, -1.0) };
                            if top {
                                assert!(
                                    at_mini[0] > slot.panel[0] && at_mini[0] < slot.panel[2],
                                    "{what} slot {i}"
                                );
                                assert!(there[1] > 0.0, "{what} slot {i}: {there:?}");
                            } else {
                                assert!((at_mini[0] - edge).abs() < 0.01, "{what} slot {i}");
                                assert!(
                                    at_mini[1] >= mini[1]
                                        && at_mini[1]
                                            <= mini[1] + mini[3] + gap * SPACING_V1.ports as f32
                                        && at_mini[1] <= slot.panel[3],
                                    "{what} slot {i}"
                                );
                                // (An S squeezed into a narrow room
                                // turns at once: never backward.)
                                assert!(there[0] * out > -0.02, "{what} slot {i}: {there:?}");
                            }
                            if outgoing {
                                // From the band's edge on that side, near
                                // the power bar, outward; into the
                                // miniature the same way.
                                let x = if slot.left { bl } else { br };
                                assert!((other[0] - x).abs() < 0.01, "{what} slot {i}");
                                assert!(
                                    (other[1] - (power[1] + power[3] / 2.0)).abs()
                                        <= g.arrow * 0.6,
                                    "{what} slot {i}: {other:?} {power:?}"
                                );
                                assert!(leaves[0] * out > -0.02, "{what} slot {i}: {leaves:?}");
                            } else {
                                // Out of the miniature toward the field,
                                // and into its end the same way.
                                assert!(
                                    arrives[0] * out < 0.02 || !slot.left && other[0] < cue[2] - 0.5,
                                    "{what} slot {i}: {arrives:?}"
                                );
                                if slot.left {
                                    assert!(
                                        (other[0] - bl).abs() < 0.01
                                            && (other[1] - g.receptor_y).abs()
                                                <= gap * SPACING_V1.ports as f32,
                                        "{what} slot {i}: {other:?}"
                                    );
                                } else {
                                    // On the warning's outer side, its
                                    // top or its bottom.
                                    assert!(
                                        other[0] >= cue[0]
                                            && other[0] <= cue[2] + 0.01
                                            && other[1] >= cue[1] - 0.01
                                            && other[1] <= cue[3] + 0.01,
                                        "{what} slot {i}: {other:?} {cue:?}"
                                    );
                                }
                            }
                            let mut under_warning = false;
                            for at in places(&p) {
                                let said = format!("{what} slot {i} outgoing {outgoing} at {at:?}");
                                assert!(
                                    at[0] >= 0.0 && at[0] <= g.width && at[1] >= 0.0 && at[1] <= g.height,
                                    "{said}: off the screen"
                                );
                                assert!(!inside(at, band_box), "{said}: over the band");
                                assert!(!inside(at, judgment), "{said}: over the judgment");
                                for (m, ko) in own.iter().enumerate() {
                                    assert!(!inside(at, *ko), "{said}: over keep-out {m}");
                                }
                                for word in words(&f) {
                                    assert!(!inside(at, word), "{said}: over the status or the ladder");
                                }
                                // Incoming lines end at the warning;
                                // outgoing ones cross it only with no
                                // other way.
                                under_warning |= outgoing && inside(at, cue);
                            }
                            under += usize::from(under_warning);
                        }
                    }
                }
            }
        }
        eprintln!("{lines} lines, {none} without a way, {under} under the warning's place");
        assert!(lines > 2000 && none * 20 < lines, "{lines} lines, {none} without a way");
        assert!(under * 20 < lines, "{under} of {lines} lines under the warning's place");
    }

    /// No two lines share a stretch: for 1, 8, 24 and 72 opponents at
    /// several sizes, with and without Reverse, many links at once (the
    /// player's own, to and from drawn seats, several to and from the
    /// same one) laid in one frame are never nearer than two strokes to
    /// each other for longer than a crossing; nearly every one has a
    /// line; and the same holds as links come and go over frames.
    #[test]
    fn no_two_lines_share_a_stretch() {
        let mut draws = Draws(7);
        let (mut asked, mut drawn) = (0, 0);
        for (w, h, ratio) in [SIZES[0], SIZES[1], SIZES[3], SIZES[5], SIZES[6], SIZES[8]] {
            for reverse in [false, true] {
                for n in [1, 8, 24, 72] {
                    let g = FieldGeometry::placed(w * ratio, h * ratio, 4.0, reverse, Default::default());
                    let k = keep(&g, ratio, true);
                    let Some(f) = crate::opponents::tests::arrange(n, &g, 4.0, reverse, ratio, &k) else {
                        continue;
                    };
                    let layout = Layout {
                        geo: &g,
                        span: 4.0,
                        fields: &f,
                        ratio,
                        keep_out: &k[..k.len() - 1],
                    };
                    let slots = f.slots.len();
                    for many in [2, 4, 8, 16, 32] {
                        let what = format!("{w}x{h}@{ratio} reverse {reverse} n {n}, {many} links");
                        let mut links: Vec<Link> = Vec::new();
                        for _ in 0..many {
                            let l = link(draws.next(slots), draws.next(3) == 0, false);
                            if !links.iter().any(|x| x.attack == l.attack) {
                                links.push(l);
                            }
                        }
                        let mut routes = Routes::default();
                        let s = shown(&links, &layout, &mut routes, 10.2, false, ATTACK_LINKS_V1);
                        asked += s.len();
                        let lines = apart(&s, ratio, &what);
                        drawn += lines;
                        if lines < s.len() {
                            eprintln!("{what}: {lines} lines of {}", s.len());
                        }
                        assert!(routes.tally().tried as usize <= s.len() * SPACING_V1.tries * 2);
                        // Half of them go and others come: still apart,
                        // and those that stayed where they were.
                        let mut next: Vec<Link> = links.iter().copied().step_by(2).collect();
                        for _ in 0..many / 2 {
                            let l = link(draws.next(slots), draws.next(3) == 0, false);
                            if !next.iter().any(|x| x.attack == l.attack) {
                                next.push(l);
                            }
                        }
                        let s2 = shown(&next, &layout, &mut routes, 10.3, false, ATTACK_LINKS_V1);
                        apart(&s2, ratio, &what);
                        for a in &s {
                            if let Some(b) = s2.iter().find(|b| b.attack == a.attack) {
                                assert_eq!(a.path, b.path, "{what}: a line moved");
                            }
                        }
                    }
                }
            }
        }
        eprintln!("the player's: {drawn} lines of {asked} links");
        assert!(asked > 1500 && drawn * 10 >= asked * 9, "{drawn} lines of {asked} links");
    }

    /// The same between the fields of a watcher's grid, for 2 to 72
    /// fields: everyone's attacks at once.
    #[test]
    fn no_two_lines_of_a_grid_share_a_stretch() {
        let mut draws = Draws(11);
        let (mut asked, mut drawn) = (0, 0);
        for (w, h, ratio) in [SIZES[0], SIZES[1], SIZES[5], SIZES[6], SIZES[8]] {
            for n in [2, 3, 5, 8, 12, 24, 72] {
                let g = FieldGeometry::placed(w * ratio, h * ratio, 4.0, false, Default::default());
                let k = keep_grid(&g, ratio);
                let Some(f) = crate::opponents::arrange_grid(n, usize::MAX, &g, 4.0, false, ratio, &k) else {
                    continue;
                };
                let layout = GridLayout {
                    fields: &f,
                    size: [g.width, g.height],
                    span: 4.0,
                    ratio,
                    keep_out: &k,
                };
                let slots = f.slots.len();
                for many in [2, 6, 12, 24, 48] {
                    let what = format!("{w}x{h}@{ratio} grid of {n}, {many} links");
                    let links: Vec<SeenLink> = (0..many)
                        .map(|i| seen(i as u32 + 1, f.slots[draws.next(slots)].seat, f.slots[draws.next(slots)].seat))
                        .filter(|l| l.from != l.to)
                        .collect();
                    let mut routes = Routes::default();
                    let s = shown_between(&links, &layout, &mut routes, 10.2, false, ATTACK_LINKS_V1);
                    asked += s.len();
                    drawn += apart(&s, ratio, &what);
                    assert!(routes.showing() <= SPACING_V1.lines);
                }
            }
        }
        eprintln!("a grid's: {drawn} lines of {asked} links");
        assert!(asked > 1000 && drawn * 10 >= asked * 8, "{drawn} lines of {asked} links");
    }

    /// A line stays where it was laid while it shows: frames later, with
    /// other links come and gone, its curve is the same; a link that
    /// comes takes a way free of those showing; one that comes back
    /// after its line went is laid afresh; and the lines kept are the
    /// lines a frame draws.
    #[test]
    fn a_line_stays_where_it_was_laid() {
        let g = FieldGeometry::placed(1280.0, 720.0, 4.0, false, Default::default());
        let k = keep(&g, 1.0, true);
        let f = crate::opponents::tests::arrange(8, &g, 4.0, false, 1.0, &k).unwrap();
        let layout = Layout {
            geo: &g,
            span: 4.0,
            fields: &f,
            ratio: 1.0,
            keep_out: &k[..k.len() - 1],
        };
        let all: Vec<Link> = (0..8).map(|s| link(s, false, false)).collect();
        let mut routes = Routes::default();
        let look = ATTACK_LINKS_V1;
        // Four against the player from the right side, one after another.
        let first = shown(&all[4..6], &layout, &mut routes, 10.0, false, look);
        let more = shown(&all[4..8], &layout, &mut routes, 10.2, false, look);
        assert_eq!(first[0].path, more[0].path);
        assert_eq!(first[1].path, more[1].path);
        assert_eq!(apart(&more, 1.0, "four"), 4);
        // Each on a place of its own on the warning, a gap apart at
        // least.
        let ends: Vec<P> = more.iter().map(|s| s.path[s.path.len() - 1]).collect();
        let gap = SPACING_V1.gap * thick(1.0);
        for (i, a) in ends.iter().enumerate() {
            for b in &ends[i + 1..] {
                assert!(length(sub(*a, *b)) >= gap - 0.02, "{ends:?}");
            }
        }
        // The first goes: the others stay; a new one takes a free way.
        let later = shown(&all[5..8], &layout, &mut routes, 10.4, false, look);
        for (a, b) in more[1..].iter().zip(&later) {
            assert_eq!(a.path, b.path);
        }
        assert_eq!(routes.showing(), 3);
        let again = shown(&all[4..8], &layout, &mut routes, 10.6, false, look);
        assert_eq!(apart(&again, 1.0, "again"), 4);
        for (a, b) in later.iter().zip(&again[1..]) {
            assert_eq!(a.path, b.path);
        }
        // Frame after frame the same, and what a fresh laying in the
        // same order gives.
        let kept = shown(&all[4..8], &layout, &mut routes, 10.9, false, look);
        assert_eq!(kept.iter().map(|s| &s.path).collect::<Vec<_>>(), again.iter().map(|s| &s.path).collect::<Vec<_>>());
        let fresh = shown(&all[4..6], &layout, &mut Routes::default(), 10.0, false, look);
        assert_eq!(fresh, first);
        // The slots placed again: every line forgotten.
        routes.clear();
        assert_eq!((routes.showing(), routes.tally()), (0, Tally::default()));
        // Beyond the bound of lines at once, a link has its outline
        // alone.
        let mut few = Routes::spaced(Spacing { lines: 2, ..SPACING_V1 });
        let s = shown(&all[4..8], &layout, &mut few, 10.2, false, look);
        assert_eq!(s.iter().filter(|s| !s.path.is_empty()).count(), 2);
        assert!(s.iter().all(|s| s.outline > 0.0));
        assert_eq!(few.tally().without, 2);
    }

    /// A link over time: the pulse runs for 0.6 s along a whole line by
    /// its length, the line fades to nothing by 1.5 s, the outline stays
    /// until the attack has passed (the session drops the link then); a
    /// chained one is quieter; reduced motion has no pulse and no fade,
    /// the line whole for its time.
    #[test]
    fn a_link_pulses_fades_and_keeps_its_outline() {
        let g = FieldGeometry::placed(1280.0, 720.0, 4.0, false, Default::default());
        let k = keep(&g, 1.0, true);
        let f = crate::opponents::tests::arrange(8, &g, 4.0, false, 1.0, &k).unwrap();
        let layout = Layout {
            geo: &g,
            span: 4.0,
            fields: &f,
            ratio: 1.0,
            keep_out: &k[..k.len() - 1],
        };
        let look = ATTACK_LINKS_V1;
        assert_eq!((look.id, look.pulse, look.line), ("attack-links-v1", 0.6, 1.5));
        let at = |t: f64, reduced: bool, l: Link| {
            shown(&[l], &layout, &mut Routes::default(), t, reduced, look).remove(0)
        };
        let s = at(10.0, false, link(5, true, false));
        assert_eq!((s.phase, s.line, s.pulse), (Phase::Pulse, 1.0, Some(0.0)));
        assert_eq!((s.seat, s.slot, s.outgoing), (5, 5, true));
        assert_eq!(s.outline, 0.0, "it comes in");
        let s = at(10.3, false, link(5, true, false));
        assert_eq!(s.phase, Phase::Pulse);
        assert!((s.pulse.unwrap() - 0.5).abs() < 1e-3 && s.outline == 1.0);
        let s = at(11.05, false, link(5, true, false));
        assert_eq!((s.phase, s.pulse), (Phase::Fade, None));
        assert!((s.line - 0.5).abs() < 1e-3, "{}", s.line);
        let s = at(11.6, false, link(5, true, false));
        assert_eq!((s.phase, s.line, s.outline), (Phase::Outline, 0.0, 1.0));
        assert!(s.path.is_empty());
        // Chained: quieter throughout.
        let s = at(10.3, false, link(5, false, true));
        assert!((s.line - look.quiet).abs() < 1e-6 && (s.outline - look.quiet).abs() < 1e-6);
        assert!(!s.outgoing && s.chained);
        // Reduced motion: still, then gone.
        let s = at(10.0, true, link(2, true, false));
        assert_eq!((s.phase, s.line, s.pulse, s.outline), (Phase::Still, 1.0, None, 1.0));
        let still = s.clone();
        let s = at(11.4, true, link(2, true, false));
        assert_eq!((s.phase, s.line), (Phase::Still, 1.0));
        let s = at(11.5, true, link(2, true, false));
        assert_eq!((s.phase, s.line, s.outline), (Phase::Outline, 0.0, 1.0));
        // A seat not in view has no link.
        assert!(shown(&[link(9, true, false)], &layout, &mut Routes::default(), 10.0, false, look).is_empty());
        // Drawn over the miniatures: the outline and where it lands;
        // under them: the curve, a stretch an instance, and the pulse's
        // glow and dot.
        let s = at(10.3, false, link(5, true, false));
        let stretches = s.path.len() - 1;
        assert!((8..=48).contains(&stretches), "{stretches}");
        let mut list = DrawList::new();
        draw(&mut list, std::slice::from_ref(&s), &f, 1.0);
        assert_eq!(list.commands.len(), 2);
        let mut under = Vec::new();
        draw_behind(&mut under, std::slice::from_ref(&s), 1.0);
        assert_eq!(under.len(), stretches + 2);
        assert!(under[..stretches].iter().all(|i| i.shape == Sprite::Link as u32));
        // The stretches join end to end, each as wide as the glow.
        let wide = thick(1.0) * 3.5;
        assert!(under[..stretches].iter().all(|i| (i.size[1] - wide).abs() < 1e-3));
        let drawn: f32 = under[..stretches].iter().map(|i| i.size[0]).sum();
        assert!((drawn - path_length(&s.path)).abs() < 0.01);
        // The pulse half-way along the curve's length.
        let half = point_along(&s.path, 0.5);
        assert_eq!(under[stretches].center, half);
        assert_eq!(under[stretches].shape, Sprite::Glow as u32);
        assert_eq!(under[stretches + 1].shape, Sprite::Circle as u32);
        // Reduced motion: the whole line, no pulse.
        let mut under = Vec::new();
        draw_behind(&mut under, std::slice::from_ref(&still), 1.0);
        assert_eq!(under.len(), still.path.len() - 1);
        assert!(under.iter().all(|i| i.shape == Sprite::Link as u32 && i.color[3] > 0.8));
        // The outline alone once the line is gone.
        let gone = at(12.0, false, link(5, true, false));
        let mut list = DrawList::new();
        draw(&mut list, std::slice::from_ref(&gone), &f, 1.0);
        assert_eq!(list.commands.len(), 1);
        let mut under = Vec::new();
        draw_behind(&mut under, &[gone], 1.0);
        assert!(under.is_empty());
        // The debug state says from whom to whom and the phase.
        let d = debug(std::slice::from_ref(&s));
        assert_eq!(d[0]["from"], "me");
        assert_eq!(d[0]["to"], 5);
        assert_eq!(d[0]["phase"], "pulse");
        let d = debug(&[at(10.3, false, link(1, false, false))]);
        assert_eq!((d[0]["from"].clone(), d[0]["to"].clone()), (1.into(), "me".into()));
        // The pulse travels the whole line at one speed: equal steps of
        // its place are equal lengths of the curve.
        let total = path_length(&s.path);
        let mut before = s.path[0];
        let mut gone_by = 0.0;
        for step in 1..=60 {
            let now = point_along(&s.path, step as f32 / 60.0);
            let moved = length(sub(now, before));
            // (A chord of the curve: never longer than its arc, and
            // nearly it.)
            assert!(moved <= total / 60.0 + 0.01 && moved >= total / 60.0 * 0.97, "{step}: {moved}");
            gone_by += moved;
            before = now;
        }
        assert!(gone_by > total * 0.99);
        assert_eq!(point_along(&s.path, 0.0), s.path[0]);
        assert_eq!(point_along(&s.path, 1.0), s.path[s.path.len() - 1]);
        let p = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 30.0]];
        assert_eq!(point_along(&p, 0.25), [10.0, 0.0]);
    }

    /// A curve's own shape: it starts and ends where asked, the way
    /// asked; its straight stretches keep within the tolerance of it;
    /// an S stays in the room between its ends.
    #[test]
    fn a_curve_leaves_and_arrives_the_way_asked() {
        for (p0, t0, p3, t3) in [
            ([100.0, 20.0], [1.0, 0.0], [400.0, 300.0], [1.0, 0.0]),
            ([100.0, 20.0], [1.0, 0.0], [110.0, 500.0], [1.0, 0.0]),
            ([500.0, 90.0], [1.0, 0.0], [300.0, 400.0], [-1.0, 0.0]),
            ([500.0, 90.0], [-1.0, 0.0], [300.0, 400.0], [0.0, 1.0]),
            ([40.0, 600.0], [1.0, 0.0], [45.0, 80.0], [0.0, -1.0]),
        ] {
            for (scale, bow) in SHAPES {
                let c = Cubic::between(p0, t0, p3, t3, scale, bow, 1.0);
                let flat = c.flatten(0.3);
                assert_eq!((flat[0], flat[flat.len() - 1]), (p0, p3));
                // Its own tangents at its ends (its handles'): the way
                // asked, within what the largest bow turns it by.
                let (leaves, arrives) = (unit(sub(c.0[1], c.0[0])), unit(sub(c.0[3], c.0[2])));
                assert!(dot(leaves, t0) > 0.6, "{p0:?}→{p3:?} {scale} {bow}: {leaves:?}");
                assert!(dot(arrives, t3) > 0.6, "{p0:?}→{p3:?} {scale} {bow}: {arrives:?}");
                // Unbowed, its stretches never start or end against it.
                let (first, last) = tangents(&flat);
                assert!(bow != 0.0 || (dot(first, t0) > 0.0 && dot(last, t3) > 0.0));
                // The stretches against the curve itself.
                for i in 0..=400 {
                    let at = c.at(i as f32 / 400.0);
                    let off = flat
                        .windows(2)
                        .map(|w| to_stretch(at, w[0], w[1]))
                        .fold(f32::INFINITY, f32::min);
                    assert!(off <= 0.3 + 1e-3, "{off}");
                    if dot(t0, t3) > 0.5 && p3[0] > p0[0] {
                        assert!(at[0] >= p0[0] - 1e-3 && at[0] <= p3[0] + 1e-3, "{at:?}");
                    }
                }
            }
        }
        // The ends an edge offers: the middle first, then a step either
        // way, as far as there is room.
        let end = End::along([10.0, 50.0], [0.0, 6.0], 1, 3, 6, [1.0, 0.0]);
        let ys: Vec<f32> = end.ports.iter().map(|p| p[1]).collect();
        assert_eq!(ys, [50.0, 56.0, 44.0, 62.0, 68.0]);
    }

    /// What the lines cost a frame (`cargo test --release -p ddi-shell
    /// --lib attack_links::tests::cost -- --ignored --nocapture`): 72
    /// opponents at 1280×720, the player's 24 links at once and a
    /// watcher's grid with 48; the frame that lays them, then the frames
    /// that draw them.
    #[test]
    #[ignore = "a measurement"]
    fn cost() {
        let g = FieldGeometry::placed(1280.0, 720.0, 4.0, false, Default::default());
        let k = keep(&g, 1.0, true);
        let f = crate::opponents::tests::arrange(72, &g, 4.0, false, 1.0, &k).unwrap();
        let layout = Layout {
            geo: &g,
            span: 4.0,
            fields: &f,
            ratio: 1.0,
            keep_out: &k[..k.len() - 1],
        };
        let links: Vec<Link> = (0..24).map(|i| link((i * 3) % 72, i % 3 == 0, false)).collect();
        let kg = keep_grid(&g, 1.0);
        let fg = crate::opponents::arrange_grid(72, usize::MAX, &g, 4.0, false, 1.0, &kg).unwrap();
        let grid = GridLayout {
            fields: &fg,
            size: [1280.0, 720.0],
            span: 4.0,
            ratio: 1.0,
            keep_out: &kg,
        };
        let between: Vec<SeenLink> = (0..48)
            .map(|i| seen(i as u32 + 1, fg.slots[(i * 5) % 72].seat, fg.slots[(i * 7 + 3) % 72].seat))
            .collect();
        let micros = |from: std::time::Instant, n: u32| from.elapsed().as_secs_f64() * 1e6 / f64::from(n);
        for (what, own) in [("the player's 24", true), ("a grid's 48", false)] {
            let frame = |routes: &mut Routes, t: f64| {
                let s = if own {
                    shown(&links, &layout, routes, t, false, ATTACK_LINKS_V1)
                } else {
                    shown_between(&between, &grid, routes, t, false, ATTACK_LINKS_V1)
                };
                let mut list = DrawList::new();
                draw(&mut list, &s, if own { &f } else { &fg }, 1.0);
                let mut under = Vec::new();
                draw_behind(&mut under, &s, 1.0);
                (s.iter().filter(|s| s.path.len() >= 2).count(), list.commands.len(), under.len())
            };
            let at = std::time::Instant::now();
            let mut tally = Tally::default();
            for _ in 0..50 {
                let mut routes = Routes::default();
                frame(&mut routes, 10.1);
                tally = routes.tally();
            }
            let laying = micros(at, 50);
            let mut routes = Routes::default();
            frame(&mut routes, 10.1);
            let at = std::time::Instant::now();
            let mut drawn = (0, 0, 0);
            for i in 0..2000 {
                drawn = frame(&mut routes, 10.1 + f64::from(i) * 1e-4);
            }
            eprintln!(
                "{what}: laying {laying:.0} µs ({tally:?}); a frame after {:.1} µs; {} lines, {} overlay commands, {} instances under the miniatures",
                micros(at, 2000),
                drawn.0,
                drawn.1,
                drawn.2
            );
        }
    }
}
