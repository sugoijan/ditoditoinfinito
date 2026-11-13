//! A multiplayer stage's CPU opponents beside the field
//! (`docs/plans/multiplayer-prototype-progress.md`, M3, M6, M6b and the
//! feedback round of 2026-11-12; `docs/plans/multiplayer-ui.md`, "Play and
//! spectator layout" and "Faithful miniature fields"), in the side space,
//! never over the field.
//!
//! Each opponent has a miniature field in a stable slot ([`arrange`]): the
//! tap-only copy of the chart it plays scrolling to its receptors on this
//! play's song clock, drawn by the play's own noteskin on a band as the
//! play's field's (`ddi_render::scene::Miniature`, fed by its stream of
//! judged moments, `ddi_session::battle::feed`), its life gauge and combo
//! by the play's HUD placed on it as the play's are on the play's field
//! (with the power bar above the gauge), its judgments' hit effects (the
//! skin's where its components draw them, else a glow of the judgment's
//! color, [`draw_fields`]), and its name and its level's stars above it
//! (`ddi_ui::stars`; never a number). A miniature's field has the play's
//! own field's proportion ([`rows_for`]: as many lanes tall, ten on a
//! screen wider than tall, its receptors as far from their end), so it
//! reads as a small copy of the player's field and shows as far ahead.
//! The slots are
//! split between the two sides of the field, the first half on the left,
//! in the order the room seated them, in grids of the same size, as large
//! as each side's room clear of what is on it allows (the HUD's score, its
//! watermark, the quit button's place, the attack warning's) up to half
//! the play's own lane ([`MINI_LANE_SHARE`]: one to four opponents are
//! no larger than that), down to
//! lanes of [`MINI_LANE_MIN`] in the margins a phone held upright leaves,
//! and they never move: one that is out stays in place, dimmed, saying so.
//! A status line under the right side's slots (beside the watermark where
//! the sides are too narrow for words) says "Stage won", and a battle's
//! moment that no announcer line said for a few seconds (fading, at once
//! under reduced motion). With lyrics over the play too.
//!
//! Every opponent of a room's stage (eight at most) has its own miniature
//! at every size a play is drawn at. Only where there are more opponents
//! than slots at the smallest miniature (a development-only practice far
//! beyond the cap, or a window too small for a room's eight) do the
//! opponents take turns in the slots ([`rotate`], [`ROTATION_V0`]), but
//! for those a stage keeps in view ([`priority_seats`]: none until attacks
//! exist).

use ddi_actor::text::{TextMeasure, TextStyle};
use ddi_render::FieldGeometry;
use ddi_render::draw2d::{Affine, Blend, Command, DrawList, Mask, Quad, Shape, TextRun};
use ddi_session::battle::OpponentView;

/// The theme's text face (`crates/theme/src/infinito.toml`), with the
/// language's faces after it ([`ddi_l10n::font_stack`]).
const FACE: &str = "Iosevka Custom, Iosevka Term, monospace";

const TEXT: [f32; 4] = [0.92, 0.92, 0.95, 1.0];
const MUTED: [f32; 4] = [0.80, 0.80, 0.86, 1.0];
const WON: [f32; 4] = [0.98, 0.84, 0.45, 1.0];

/// What a stage shows of its opponents around their miniatures.
#[derive(Clone, Debug, Default)]
pub struct Shown {
    pub opponents: Vec<OpponentView>,
    /// "Out" and "Stage won", worded.
    pub out: String,
    pub won: Option<String>,
    /// A battle's moment no announcer line said, worded, and how opaque,
    /// `0..=1`.
    pub cue: Option<(String, f32)>,
    /// How each opponent's score shows on its miniature.
    pub score: ScoreShown,
    /// A quiet word at the status line's start (a watched stage's: who
    /// is followed, how to follow someone).
    pub label: Option<String>,
    /// The slot of `fields` that is the field shown large (a watched
    /// stage's focus: the play's own field's place): no name line, no
    /// stars and no small score there (the HUD's places say them).
    pub large: Option<usize>,
    /// The seats whose stream is behind (a live stage's: their numbers
    /// held, their field marked quietly, dimmed, until it catches up).
    pub stale: Vec<usize>,
    /// A large field's ladder ([`crate::rivals::Ladder`]) and its words
    /// ("17 of 73 · 41 still in"), drawn in [`Fields::ladder`]'s place.
    pub ladder: Option<(String, crate::rivals::Ladder)>,
    /// The local player's own field among the miniatures (they are out of
    /// the stage and follow a competitor large): its seat and the word it
    /// is marked with in place of [`Shown::out`] ("Left" where they left
    /// the stage).
    pub own: Option<(usize, String)>,
}

/// What a miniature says of its opponent's score, as the local player's
/// Score display has their own: under DDI the score (the sum of the
/// proportional part and the absolute score, `OpponentView::sum`: a CPU's
/// on the chart's own scale, so it compares with the player's HUD beside
/// it) or the percent; under another ruleset the percent. Never the
/// proportional part alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScoreShown {
    /// The score, the sum ("5987654").
    #[default]
    Sum,
    /// The percent to two decimals, floored ("98.76%").
    Percent,
}

impl ScoreShown {
    /// `o`'s score as shown (the percent where it has no sum).
    pub fn text(self, o: &OpponentView) -> String {
        match (self, o.sum) {
            (ScoreShown::Sum, Some(sum)) => sum.to_string(),
            // A live stage's person: the stream tells the sum alone.
            (ScoreShown::Percent, Some(sum)) if o.score.is_nan() => sum.to_string(),
            _ => {
                let hundredths = ((o.score * 1_000_000.0).round() as i64).div_euclid(100);
                let sign = if hundredths < 0 { "-" } else { "" };
                let p = hundredths.unsigned_abs();
                format!("{sign}{}.{:02}%", p / 100, p % 100)
            }
        }
    }
}

/// A miniature's score: its size by the name line's (logical pixels),
/// and how far under the life gauge it is set (in the gauge's units, a
/// 72nd of a lane: the gauge is at 22 and 8 thick).
const SCORE_SIZE: (f32, f32, f32) = (0.8, 8.0, 11.0);
/// The least a score wider than its field is shrunk to.
const SCORE_FLOOR: f32 = 6.0;
const SCORE_TOP: f32 = 33.0;

/// A lane locked by a shock on a miniature (DDI's lane lock): the lane
/// darkened and its receptor's place tinted in the shock's violet, for
/// the lock's time. No skin's Lua runs for a miniature, so this is ours,
/// whatever the skin.
const LOCK_LANE: [f32; 4] = [0.0, 0.0, 0.0, 0.42];
const LOCK_TINT: [f32; 4] = [0.62, 0.42, 1.0, 0.50];

/// PROVISIONAL sizes (`docs/plans/multiplayer-prototype-progress.md`, the
/// feedback round of 2026-11-12), all in logical pixels or lanes.
///
/// A miniature's lane: at least this for a room's opponents (eight at
/// most: every one keeps its own miniature down to it; below it fewer
/// slots, the opponents taking turns), at least this beyond them (a
/// development-only practice: as many slots as fit at it, the rest taking
/// turns), and at most this share of the play's own lane (a value to
/// tune): a miniature is a copy of the play's field at half its size at
/// the most, so one to four opponents are no larger than that however
/// much room the sides have (the feedback round of 2026-11-12, part C:
/// "when there are too few players the miniatures get bigger than they
/// should"). No other cap: the play's own lane is bounded.
pub const MINI_LANE_MIN: f32 = 8.0;
pub const MINI_LANE_CROWD: f32 = 16.0;
/// The opponents a room of nine has, each with a slot of its own; past
/// them a field is large (`crate::rivals`).
pub const ROOM_OPPONENTS: usize = 8;
pub const MINI_LANE_SHARE: f32 = 0.5;
/// A miniature's field, in its lanes' heights: the play's own field's
/// proportion (the screen's height over the play's lane: ten on a screen
/// wider than it is tall, [`rows_for`]), so that an opponent's field
/// reads as a small copy of the player's and shows as far ahead. Within
/// these where a window's shape gives the play's field another (a phone
/// held upright has thirteen lanes of height: twelve; a window too short
/// for the play's least lane: eight).
const MINI_ROWS_MIN: f32 = 8.0;
const MINI_ROWS_MAX: f32 = 12.0;
/// Its receptors' place from the end they are at: as the play's own
/// field's (`FieldGeometry`: a lane and a half, the same share of its
/// height wherever its rows are the play's), so that its life gauge, its
/// combo and its power bar sit on it as the play's do on the play's
/// field (`ddi_render::hud`).
const MINI_RECEPTOR: f32 = 1.5;

/// How tall a miniature's field is for the play's field `geo`, in lanes:
/// as the play's own, within [`MINI_ROWS_MIN`] and [`MINI_ROWS_MAX`].
pub fn rows_for(geo: &FieldGeometry) -> f32 {
    (geo.height / geo.arrow.max(1.0)).clamp(MINI_ROWS_MIN, MINI_ROWS_MAX)
}
/// Above each field: its name line, as large as a tenth of the slot's
/// width between these sizes, and the room under it (and under its
/// stars' line where they have their own).
const NAME_MIN: f32 = 9.0;
const NAME_MAX: f32 = 14.0;
const NAME_GAP: f32 = 8.0;
/// The status line under one side's slots (the right's, else the left's):
/// a battle's moment no line said, and "Stage won"; and the least width
/// a side must have for it (else it goes beside the watermark).
const STATUS: f32 = 22.0;
const STATUS_MIN_WIDTH: f32 = 110.0;
/// Between slots, from the play's field's band and from the screen's
/// edges: these shares of a side's width, within these bounds.
const SLOT_GAP: (f32, f32, f32) = (0.04, 4.0, 10.0);
const FIELD_GAP: (f32, f32, f32) = (0.08, 4.0, 20.0);
const EDGE: (f32, f32, f32) = (0.04, 3.0, 10.0);
/// From what the slots keep clear of.
const CLEAR: f32 = 6.0;
/// A field's band is wider than its lanes by this many lanes, as the
/// play's (`ddi_render::scene::filter_band`).
const BAND_MORE: f32 = 0.3;
/// A slot's width in its lanes.
const SLOT_LANES: f32 = 4.0 + BAND_MORE;

/// A slot's level's stars (`ddi_ui::stars`): beside its name from this
/// width on, under it (a short line of their own) down to this one, and
/// below it the name alone; as tall as this share of the name's size; the
/// room over a line of their own.
const STARS_BESIDE: f32 = 170.0;
const STARS_LEAST: f32 = 44.0;
const STARS_SHARE: f32 = 0.8;
const STARS_UNDER_GAP: f32 = 3.0;

/// A slot's name line's size at `lane`.
fn name_size(lane: f32) -> f32 {
    (SLOT_LANES * lane / 10.0).clamp(NAME_MIN, NAME_MAX)
}

/// Where a slot's level's stars go, and how tall they are (logical
/// pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stars {
    /// At the name line's right end.
    Beside(f32),
    /// On a line of their own under the name.
    Under(f32),
    /// The slot is too narrow for them: the name alone.
    None,
}

fn stars_at(lane: f32) -> Stars {
    let w = SLOT_LANES * lane;
    let tall = name_size(lane) * STARS_SHARE;
    if w >= STARS_BESIDE {
        Stars::Beside(tall)
    } else if w >= STARS_LEAST {
        Stars::Under(tall.min((w - 2.0) / ddi_ui::stars::width(1.0)))
    } else {
        Stars::None
    }
}

/// What a slot has above its field at `lane`: its name line, and its
/// stars' where they have their own.
fn label(lane: f32) -> f32 {
    name_size(lane)
        + NAME_GAP
        + match stars_at(lane) {
            Stars::Under(tall) => tall + STARS_UNDER_GAP,
            _ => 0.0,
        }
}

/// Whether a slot at `lane`, its field `rows` lanes tall, fits a cell.
fn fits(lane: f32, rows: f32, [w, h]: [f32; 2]) -> bool {
    SLOT_LANES * lane <= w + 0.001 && label(lane) + rows * lane <= h + 0.001
}

/// Where an opponent's miniature goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    /// The seat it shows (an index into the room's order): its place's
    /// own until [`rotate`] says otherwise.
    pub seat: usize,
    /// The whole slot, its name line to its field's end, `[x, y, width,
    /// height]` in the target's pixels.
    pub rect: [f32; 4],
    /// Its field's geometry in the target's pixels.
    pub field: FieldGeometry,
    /// Its field's band, `[left, top, right, bottom]`.
    pub panel: [f32; 4],
    /// On the field's left.
    pub left: bool,
    /// How dark it is drawn over, `0..=1`: a slot whose seat is changing
    /// ([`rotate`]).
    pub dim: f32,
}

/// The opponents' miniatures' places ([`arrange`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Fields {
    /// The slots, the left side's first, each side's row by row from its
    /// left.
    pub slots: Vec<Slot>,
    /// The status line under the right side's slots (the left's where
    /// there is no right side; beside the watermark where that side is
    /// too narrow for words), `[x, y, width, height]`: a battle's moment
    /// no announcer line said, and "Stage won".
    pub status: [f32; 4],
    /// Fewer slots than opponents: whose miniature shows turns
    /// ([`rotate`]).
    pub rotating: bool,
    /// The miniatures' lane, logical pixels.
    pub lane: f32,
    /// Their fields' height, in lanes.
    pub rows: f32,
    /// Arrow heights shown past the receptors.
    pub range: f32,
    /// Their name lines' size, logical pixels, and where their levels'
    /// stars go.
    pub name: f32,
    pub stars: Stars,
    /// A large field's ladder ([`crate::rivals::Ladder`]): `[x, y, width,
    /// height]` in the target's pixels, under the left side's slots; none
    /// where every opponent has a slot.
    pub ladder: Option<[f32; 4]>,
}

/// A side's room for slots: `[left, top, right, bottom]`, logical pixels.
type Region = [f32; 4];

/// The largest lane, `limit` at most, whose slot is no taller than
/// `height` with a field `rows` lanes tall. Its label grows with it, but
/// for the step down where the stars move beside the name: looked at on
/// either side of it.
fn lane_for_height(limit: f32, height: f32, rows: f32) -> f32 {
    let tall = |lane: f32| label(lane) + rows * lane;
    let beside = STARS_BESIDE / SLOT_LANES;
    if limit >= beside {
        let lane = limit.min((height - NAME_MAX - NAME_GAP) / rows);
        if lane >= beside {
            return lane;
        }
    }
    let (mut lo, mut hi) = (0.0_f32, limit.min(beside - 0.001).max(0.0));
    if tall(hi) <= height {
        return hi;
    }
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if tall(mid) <= height {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// One way to put a side's slots in a region: so many columns and rows,
/// each cell this wide and tall.
#[derive(Clone, Copy, Debug)]
struct Grid {
    region: Region,
    cols: usize,
    rows: usize,
    cell: [f32; 2],
}

impl Grid {
    /// The largest lane its slots can have, their fields `rows` lanes
    /// tall.
    fn lane(&self, rows: f32) -> f32 {
        lane_for_height(self.cell[0] / SLOT_LANES, self.cell[1], rows)
    }
}

/// One side of the play's field.
struct Side {
    /// Its rooms: what its keep-outs leave, each one avoided either by
    /// height (the room ends above or below it) or by width (beside it).
    regions: Vec<Region>,
    gap: f32,
    /// The field is to its right (the left side).
    left: bool,
}

impl Side {
    /// The grids `k` slots can take in its rooms.
    fn grids(&self, k: usize) -> impl Iterator<Item = Grid> + '_ {
        let gap = self.gap;
        self.regions.iter().flat_map(move |&region| {
            let (w, h) = (region[2] - region[0], region[3] - region[1]);
            (1..=k).map(move |cols| {
                let rows = k.div_ceil(cols);
                Grid {
                    region,
                    cols,
                    rows,
                    cell: [
                        (w - (cols as f32 - 1.0) * gap) / cols as f32,
                        (h - (rows as f32 - 1.0) * gap) / rows as f32,
                    ],
                }
            })
        })
    }

    /// The largest lane `k` slots can have on it, their fields `rows`
    /// lanes tall (any, with none): the best of its grids, two rows of
    /// two or four in a row, whichever gives more.
    fn lane(&self, k: usize, rows: f32) -> f32 {
        if k == 0 {
            return f32::MAX;
        }
        self.grids(k).map(|g| g.lane(rows)).fold(0.0, f32::max)
    }

    /// How `k` slots go on it at `lane`, their fields `rows` lanes tall:
    /// of the grids they fit (one, at a side's largest lane; several
    /// under the cap), the fewest columns, then the room nearest the
    /// field.
    fn grid(&self, k: usize, lane: f32, rows: f32) -> Option<Grid> {
        let near = |g: &Grid| {
            if self.left { -g.region[2] } else { g.region[0] }
        };
        let mut best: Option<Grid> = None;
        for g in self.grids(k).filter(|g| fits(lane, rows, g.cell)) {
            let better =
                best.is_none_or(|b| g.cols < b.cols || (g.cols == b.cols && near(&g) < near(&b)));
            if better {
                best = Some(g);
            }
        }
        best
    }

    /// How many slots it holds at lane `least`, their fields `rows`
    /// lanes tall.
    fn holds(&self, least: f32, rows: f32) -> usize {
        let (sw, sh) = (SLOT_LANES * least, label(least) + rows * least);
        self.regions
            .iter()
            .map(|r| {
                let cols = ((r[2] - r[0] + self.gap) / (sw + self.gap))
                    .floor()
                    .max(0.0);
                let lines = ((r[3] - r[1] + self.gap) / (sh + self.gap))
                    .floor()
                    .max(0.0);
                (cols * lines) as usize
            })
            .max()
            .unwrap_or(0)
    }
}

/// `share` of `of`, within the bounds.
fn part((share, least, most): (f32, f32, f32), of: f32) -> f32 {
    (share * of).clamp(least, most)
}

/// Places `n` miniatures in the side space of the play's field `geo`
/// (`span` lanes wide, its notes scrolling the other way under
/// `reverse`) at `ratio` target pixels a logical one, clear of
/// `keep_out` (`[left, top, right, bottom]`, the target's pixels: the
/// HUD's score and watermark, the quit button's place and the attack
/// warning's, which the slots keep clear of whether they show or not so
/// that they never move). The first half (rounded up) on the left, the
/// rest on the right (another split only where that one has no room: a
/// field placed at a side), each side a grid of slots of one size
/// against the field, in the room its keep-outs leave: under or
/// over one, or beside it, whichever gives the larger miniatures. Their
/// fields have the play's own field's proportion ([`rows_for`]: as many
/// lanes tall, their receptors as far from their end), their lane is the
/// largest both sides allow up to [`MINI_LANE_SHARE`] of the play's own,
/// and under one side goes the status line.
///
/// Where all of them do not fit at the least lane ([`MINI_LANE_MIN`] for
/// a room's eight, a window too narrow for them; [`MINI_LANE_CROWD`]
/// beyond the cap, a development-only practice), as many slots as fit at
/// it, and the opponents take turns in them ([`rotate`]); `None` where
/// not even one fits.
pub fn arrange(
    n: usize,
    geo: &FieldGeometry,
    span: f32,
    reverse: bool,
    ratio: f32,
    keep_out: &[[f32; 4]],
) -> Option<Fields> {
    if n == 0 || ratio <= 0.0 {
        return None;
    }
    // A large field (more opponents than a room of nine has): the slots
    // of a room of nine, those that matter to the player in them
    // (`crate::rivals`), and the ladder under the left side's.
    let total = n;
    let n = n.min(ROOM_OPPONENTS);
    let large = total > n;
    let (w, h) = (geo.width / ratio, geo.height / ratio);
    // The field's band (its filter's, a little wider than the lanes).
    let [mid, band] = ddi_render::scene::filter_band(geo, span);
    let field = [
        geo.left.min(mid - band / 2.0) / ratio,
        (geo.left + geo.arrow * span).max(mid + band / 2.0) / ratio,
    ];
    let keep: Vec<[f32; 4]> = keep_out.iter().map(|k| k.map(|v| v / ratio)).collect();
    // Which side the status line goes under, given the sides' counts, and
    // whether that side has the width for its words.
    let raw = [field[0], w - field[1]];
    let wide = |side: usize| {
        raw[side] - part(EDGE, raw[side]) - part(FIELD_GAP, raw[side]) >= STATUS_MIN_WIDTH
    };
    let side = |side: usize, status: bool| -> Side {
        let (edge, gap) = (part(EDGE, raw[side]), part(FIELD_GAP, raw[side]));
        let wing = if side == 0 {
            [edge, field[0] - gap]
        } else {
            [field[1] + gap, w - edge]
        };
        let bottom = h - edge - if status { STATUS } else { 0.0 };
        let whole = [wing[0], edge, wing[1], bottom];
        let mut regions = rooms(whole, &keep, h);
        if large && side == 0 {
            // The ladder's line under the slots, inside each room (clear
            // of what the room is clear of).
            let below = STATUS + part(SLOT_GAP, raw[side]);
            regions.retain_mut(|r| {
                r[3] -= below;
                r[3] > r[1]
            });
        }
        Side {
            regions,
            gap: part(SLOT_GAP, raw[side]),
            left: side == 0,
        }
    };
    // The sides for a split: the status line under the right side where
    // it has slots, else the left.
    let sides = |counts: [usize; 2]| -> ([Side; 2], usize, bool) {
        let under = usize::from(counts[1] > 0);
        let reserved = wide(under);
        (
            [
                side(0, under == 0 && reserved),
                side(1, under == 1 && reserved),
            ],
            under,
            reserved,
        )
    };
    // The play's field's proportion, at no more than this share of it.
    let rows = rows_for(geo);
    let cap = geo.arrow / ratio * MINI_LANE_SHARE;
    // Whole target pixels.
    let whole = |lane: f32| (lane * ratio).floor() / ratio;
    let lane_for = |counts: [usize; 2]| {
        let (s, ..) = sides(counts);
        let mut lane = whole(
            s[0].lane(counts[0], rows)
                .min(s[1].lane(counts[1], rows))
                .min(cap),
        );
        // A whole pixel under a side's largest may be a taller label's
        // (the stars under the name): down to one both sides fit.
        let fit = |lane: f32| {
            (0..2).all(|i| counts[i] == 0 || s[i].grid(counts[i], lane, rows).is_some())
        };
        while lane > 0.0 && !fit(lane) {
            lane -= 1.0 / ratio;
        }
        lane.max(0.0)
    };
    let least = if n <= ROOM_OPPONENTS {
        MINI_LANE_MIN
    } else {
        MINI_LANE_CROWD
    };
    let even = [n.div_ceil(2), n / 2];
    let mut counts = even;
    let mut lane = lane_for(counts);
    if lane < least && n <= ROOM_OPPONENTS {
        // Another split, where one side has the room the other lacks: the
        // nearest to even that fits best.
        let mut lefts: Vec<usize> = (0..=n).collect();
        lefts.sort_by_key(|l| l.abs_diff(even[0]));
        for left in lefts {
            let c = [left, n - left];
            let l = lane_for(c);
            if l > lane {
                (counts, lane) = (c, l);
            }
        }
    }
    if lane < least {
        // As many as fit at the least lane; the rest take turns.
        let holds = [
            side(0, wide(0)).holds(least, rows),
            side(1, wide(1)).holds(least, rows),
        ];
        let shown = n.min(holds[0] + holds[1]);
        if shown == 0 {
            return None;
        }
        let left = shown
            .div_ceil(2)
            .max(shown - holds[1].min(shown))
            .min(holds[0]);
        counts = [left, shown - left];
        lane = lane_for(counts).max(least);
    }
    let shown = counts[0] + counts[1];
    let (sides, under, reserved) = sides(counts);
    let grids: Vec<Option<Grid>> = (0..2)
        .map(|s| {
            (counts[s] > 0)
                .then(|| sides[s].grid(counts[s], lane, rows))
                .flatten()
        })
        .collect();
    if grids
        .iter()
        .zip(&counts)
        .any(|(g, &k)| k > 0 && g.is_none())
    {
        return None;
    }
    // One size for all.
    let (sw, sh) = (SLOT_LANES * lane, label(lane) + rows * lane);
    let px = |v: f32| v * ratio;
    // Both sides' blocks on one middle where their rooms allow (the
    // middle of the room both sides share), else moved into their own.
    let shared = grids.iter().flatten().fold([0.0_f32, f32::MAX], |m, g| {
        [m[0].max(g.region[1]), m[1].min(g.region[3])]
    });
    let middle = (shared[0] + shared[1]) / 2.0;
    let mut slots = Vec::with_capacity(shown);
    let mut status = None;
    let mut ladder = None;
    let mut seat = 0;
    for (s, grid) in grids.iter().enumerate() {
        let Some(g) = *grid else {
            continue;
        };
        let k = counts[s];
        let gap = sides[s].gap;
        let used = g.cols as f32 * sw + (g.cols as f32 - 1.0) * gap;
        let block = g.rows as f32 * sh + (g.rows as f32 - 1.0) * gap;
        let [left, top, right, bottom] = g.region;
        let y0 = (middle - block / 2.0).min(bottom - block).max(top);
        // Against the field's side of its room.
        let x0 = if s == 0 { right - used } else { left };
        if s == 0 && large {
            // Under the left side's slots, from its far edge to them.
            ladder = Some([
                px(left),
                px(y0 + block + gap / 2.0),
                px(x0 + used - left),
                px(STATUS),
            ]);
        }
        if s == under && reserved {
            // From the slots' left to the side's far edge.
            let to = if s == 0 { x0 + used } else { right };
            status = Some([px(x0), px(y0 + block + gap / 2.0), px(to - x0), px(STATUS)]);
        }
        for i in 0..k {
            let (row, col) = ((i / g.cols) as f32, (i % g.cols) as f32);
            let x = x0 + col * (sw + gap);
            let y = y0 + row * (sh + gap);
            let (ft, fb) = (y + label(lane), y + sh);
            // Its lanes inside its band.
            let lanes = x + BAND_MORE / 2.0 * lane;
            let receptor = if reverse {
                fb - MINI_RECEPTOR * lane
            } else {
                ft + MINI_RECEPTOR * lane
            };
            slots.push(Slot {
                seat,
                rect: [px(x), px(y), px(sw), px(sh)],
                // Its camera on its own middle.
                field: FieldGeometry {
                    arrow: px(lane),
                    left: px(lanes),
                    receptor_y: px(receptor),
                    direction: if reverse { -1.0 } else { 1.0 },
                    width: px(2.0 * (lanes + 2.0 * lane)),
                    height: px(ft + fb),
                },
                panel: [px(x), px(ft), px(x + sw), px(fb)],
                left: s == 0,
                dim: 0.0,
            });
            seat += 1;
        }
    }
    // No side wide enough for words: beside the watermark, on its row
    // (over the field's far end, where it already is), else above it.
    let status = status.unwrap_or_else(|| {
        let [_, my0, mx1, my1] = ddi_render::scene::watermark_area(geo).map(|v| v / ratio);
        let (from, to) = (mx1 + 8.0, field[1] - 4.0);
        if to - from >= 80.0 {
            [
                px(from),
                px((my0 + my1 - STATUS) / 2.0),
                px(to - from),
                px(STATUS),
            ]
        } else {
            let from = field[0] + 4.0;
            [
                px(from),
                px(my0 - STATUS - 2.0),
                px((to - from).max(0.0)),
                px(STATUS),
            ]
        }
    });
    Some(Fields {
        slots,
        status,
        rotating: shown < total,
        lane,
        rows,
        range: rows - MINI_RECEPTOR,
        name: name_size(lane),
        stars: stars_at(lane),
        ladder,
    })
}

/// The rooms `whole` (`[left, top, right, bottom]`, logical pixels, on a
/// screen `h` tall) has for slots clear of `keep`: what is on it, each
/// avoided either by height (the room ends above or below it) or by
/// width (beside it).
fn rooms(whole: Region, keep: &[[f32; 4]], h: f32) -> Vec<Region> {
    let on: Vec<[f32; 4]> = keep
        .iter()
        .copied()
        .filter(|k| k[0] < whole[2] && k[2] > whole[0] && k[1] < h && k[3] > 0.0)
        .take(4)
        .collect();
    let mut regions: Vec<Region> = Vec::new();
    for mut choice in 0..3_u32.pow(on.len() as u32) {
        let mut r = whole;
        for k in &on {
            match choice % 3 {
                // By height: above what is at the bottom, below what
                // is at the top.
                0 if (k[1] + k[3]) / 2.0 < h / 2.0 => r[1] = r[1].max(k[3] + CLEAR),
                0 => r[3] = r[3].min(k[1] - CLEAR),
                // By width: to its left, or to its right.
                1 => r[2] = r[2].min(k[0] - CLEAR),
                _ => r[0] = r[0].max(k[2] + CLEAR),
            }
            choice /= 3;
        }
        if r[2] > r[0] && r[3] > r[1] && !regions.contains(&r) {
            regions.push(r);
        }
    }
    regions
}

/// PROVISIONAL (`watch-v0`, `docs/plans/multiplayer-prototype-progress.md`,
/// "Watching a stage"): a watched stage's grid, where nobody's field is
/// the large one ([`arrange_grid`]).
///
/// A field of the grid is at most this share of the lane the play's own
/// field would have on the screen: all of it, a field as a player's own.
/// The cap that keeps a player's miniatures at half their own lane
/// ([`MINI_LANE_SHARE`]) is for miniatures beside a field of one's own;
/// where there is none it does not apply.
pub const GRID_LANE_SHARE: f32 = 1.0;
/// Of the ways to lay the fields out, the one with the largest lane; one
/// with more rows whose lane is at least this share of it is preferred
/// (it fills the screen: eight are four by two, not eight in a row).
pub const GRID_FILL: f32 = 0.9;
/// Between the grid's fields, a share of the screen's width within these
/// bounds; and beside each field, on its right, the room its attack
/// warning takes (`crate::battle_cue::place`: a lane, less a little), in
/// lanes, kept whether a warning shows or not (the fields never move).
const GRID_GAP: (f32, f32, f32) = (0.012, 6.0, 20.0);
const GRID_CUE: f32 = 1.0;
/// The least lane a grid's field has: under it the grid shows nothing
/// (a window far too small).
const GRID_LANE_MIN: f32 = 4.0;

/// Places `n` competitors' fields over the whole screen (a watched
/// stage's grid: no field of the watcher's own, so no side space), on
/// the screen of the play's field `geo` (its size, its proportion and
/// its lane are what the fields follow) at `ratio` target pixels a
/// logical one, clear of `keep_out` (the quit button's place). Rows of
/// fields of one size, each a field in the play's proportion
/// ([`rows_for`]) with its name line above it and the room of its attack
/// warning on its right, as large as the screen allows up to
/// [`GRID_LANE_SHARE`] of the play's own lane, in the layout that gives
/// the largest lane ([`GRID_FILL`]: two side by side nearly the screen's
/// height, eight four by two), the block in the screen's middle, a short
/// last row in its own middle, and the status line under the block.
///
/// Beyond a room's eight (a development-only practice), where they do
/// not fit at [`MINI_LANE_CROWD`]: as many as do, the rest taking turns
/// ([`rotate`]). `None` where not even one fits.
pub fn arrange_grid(
    n: usize,
    geo: &FieldGeometry,
    reverse: bool,
    ratio: f32,
    keep_out: &[[f32; 4]],
) -> Option<Fields> {
    if n == 0 || ratio <= 0.0 {
        return None;
    }
    // More than a page: a page's fields (`ddi_session::play::GRID_PAGE`),
    // the shell choosing whose, and the ladder in the status line's right
    // half.
    let total = n;
    let n = n.min(ddi_session::play::GRID_PAGE);
    let (w, h) = (geo.width / ratio, geo.height / ratio);
    let keep: Vec<[f32; 4]> = keep_out.iter().map(|k| k.map(|v| v / ratio)).collect();
    let edge = part(EDGE, w);
    let gap = part(GRID_GAP, w);
    let whole_room = [edge, edge, w - edge, h - edge - STATUS - 4.0];
    let rows = rows_for(geo);
    let cap = geo.arrow / ratio * GRID_LANE_SHARE;
    let whole = |lane: f32| (lane * ratio).floor() / ratio;
    // A slot with its warning's room, in lanes.
    let wide = SLOT_LANES + GRID_CUE;
    // The best layout of `k` fields: its lane, its room and its columns.
    let best = |k: usize| -> Option<(f32, Region, usize)> {
        let mut ways: Vec<(f32, Region, usize)> = Vec::new();
        for room in rooms(whole_room, &keep, h) {
            let (rw, rh) = (room[2] - room[0], room[3] - room[1]);
            for cols in 1..=k {
                let lines = k.div_ceil(cols);
                let by_width = (rw - (cols as f32 - 1.0) * gap) / (cols as f32 * wide);
                let cell = (rh - (lines as f32 - 1.0) * gap) / lines as f32;
                let lane = whole(lane_for_height(by_width.min(cap), cell, rows));
                if lane > 0.0 {
                    ways.push((lane, room, cols));
                }
            }
        }
        let most = ways.iter().map(|w| w.0).fold(0.0, f32::max);
        // The fewest columns (the most rows) among those nearly as large.
        ways.into_iter()
            .filter(|w| w.0 >= most * GRID_FILL)
            .min_by(|a, b| a.2.cmp(&b.2).then(b.0.total_cmp(&a.0)))
    };
    let least = if n <= ROOM_OPPONENTS {
        GRID_LANE_MIN
    } else {
        MINI_LANE_CROWD
    };
    // As many as fit at the least lane; the rest take turns.
    let mut shown = n;
    let mut way = best(shown);
    while shown > 1 && way.is_none_or(|w| w.0 < least) {
        shown -= 1;
        way = best(shown);
    }
    let (lane, room, cols) = way.filter(|w| w.0 >= GRID_LANE_MIN)?;
    let lines = shown.div_ceil(cols);
    let (sw, sh) = (SLOT_LANES * lane, label(lane) + rows * lane);
    let cue = GRID_CUE * lane;
    let px = |v: f32| v * ratio;
    let block = lines as f32 * sh + (lines as f32 - 1.0) * gap;
    let line_width = |m: usize| m as f32 * (sw + cue) + (m as f32 - 1.0) * gap;
    let y0 = room[1] + ((room[3] - room[1] - block) / 2.0).max(0.0);
    // Each row on the screen's own middle where its room has it there (a
    // room narrowed by the quit button's place only moves a row that
    // would reach it).
    let row_left = |m: usize| {
        let lw = line_width(m);
        (w / 2.0 - lw / 2.0).clamp(room[0], (room[2] - lw).max(room[0]))
    };
    let mut slots = Vec::with_capacity(shown);
    for i in 0..shown {
        let (line, col) = (i / cols, i % cols);
        // A short last row in its own middle.
        let in_line = if line + 1 == lines {
            shown - line * cols
        } else {
            cols
        };
        let x = row_left(in_line) + col as f32 * (sw + cue + gap);
        let y = y0 + line as f32 * (sh + gap);
        let (ft, fb) = (y + label(lane), y + sh);
        let lanes = x + BAND_MORE / 2.0 * lane;
        let receptor = if reverse {
            fb - MINI_RECEPTOR * lane
        } else {
            ft + MINI_RECEPTOR * lane
        };
        slots.push(Slot {
            seat: i,
            rect: [px(x), px(y), px(sw), px(sh)],
            field: FieldGeometry {
                arrow: px(lane),
                left: px(lanes),
                receptor_y: px(receptor),
                direction: if reverse { -1.0 } else { 1.0 },
                width: px(2.0 * (lanes + 2.0 * lane)),
                height: px(ft + fb),
            },
            panel: [px(x), px(ft), px(x + sw), px(fb)],
            left: false,
            dim: 0.0,
        });
    }
    let used = line_width(cols.min(shown)).max(320.0_f32.min(room[2] - room[0]));
    let sx = (w / 2.0 - used / 2.0).clamp(room[0], (room[2] - used).max(room[0]));
    let sy = px(y0 + block + 4.0);
    let (status, ladder) = if total > n {
        let half = used / 2.0;
        (
            [px(sx), sy, px(half - 8.0), px(STATUS)],
            Some([px(sx + half), sy, px(half), px(STATUS)]),
        )
    } else {
        ([px(sx), sy, px(used), px(STATUS)], None)
    };
    Some(Fields {
        slots,
        status,
        rotating: shown < n,
        lane,
        rows,
        range: rows - MINI_RECEPTOR,
        name: name_size(lane),
        stars: stars_at(lane),
        ladder,
    })
}

/// The place of the field a watcher follows, shown large as a player's
/// own is: the play's field `geo` (`span` lanes wide) as a slot for
/// `seat`, its band the field's from the screen's top to its bottom, so
/// that what a miniature has over its field (its lanes a shock locked,
/// its judgments' glow, Out) is drawn on it the same way.
pub fn large_slot(seat: usize, geo: &FieldGeometry, span: f32) -> Slot {
    let [mid, band] = ddi_render::scene::filter_band(geo, span);
    let panel = [mid - band / 2.0, 0.0, mid + band / 2.0, geo.height];
    Slot {
        seat,
        rect: [panel[0], 0.0, band, geo.height],
        field: *geo,
        panel,
        left: false,
        dim: 0.0,
    }
}

/// How the opponents take turns in too few slots ([`rotate`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    /// Seconds each turn lasts.
    pub every: f32,
    /// Seconds a change takes, the slots darkening to it and back (none
    /// under reduced motion).
    pub fade: f32,
}

/// PROVISIONAL (`docs/plans/multiplayer-prototype-progress.md`, M7c):
/// a calm turn of eight seconds, the change darkening for 0.4 s.
pub const ROTATION_V0: Rotation = Rotation {
    every: 8.0,
    fade: 0.4,
};

/// Which seats the slots of `fields` show at song second `time`, of `n`
/// opponents, by `policy`: every seat in its own slot where all fit (they
/// never move); else the seats in `pinned` (the opponents the play must
/// keep in view: later, who attacks the player and whom the player
/// attacks) in the first slots for as long as they are pinned, and the
/// others taking turns in the rest, a turn every `policy.every` seconds:
/// each of those slots goes round its own seats (the `k`-th free slot
/// showing the `k`-th, then the `k + free`-th, and so on), so an opponent
/// always shows in the same place and a slot with one seat never changes.
/// A slot whose seat changes darkens through the change (`Slot::dim`),
/// at once under `reduced` motion.
pub fn rotate(
    fields: &mut Fields,
    n: usize,
    pinned: &[usize],
    time: f64,
    policy: Rotation,
    reduced: bool,
) {
    let slots = fields.slots.len();
    if !fields.rotating || slots == 0 {
        for (i, s) in fields.slots.iter_mut().enumerate() {
            s.seat = i;
            s.dim = 0.0;
        }
        return;
    }
    let mut kept: Vec<usize> = Vec::new();
    for &p in pinned {
        if p < n && !kept.contains(&p) && kept.len() < slots {
            kept.push(p);
        }
    }
    let others: Vec<usize> = (0..n).filter(|s| !kept.contains(s)).collect();
    let free = slots - kept.len();
    let every = f64::from(policy.every.max(0.1));
    let time = time.max(0.0);
    let turn = (time / every).floor() as usize;
    // Darkening around each change (none before the first).
    let since = time - turn as f64 * every;
    let near = if turn == 0 {
        every - since
    } else {
        since.min(every - since)
    };
    let half = f64::from(policy.fade) / 2.0;
    let dim = if reduced || half <= 0.0 {
        0.0
    } else {
        (1.0 - near / half).clamp(0.0, 1.0) as f32
    };
    for (i, s) in fields.slots.iter_mut().enumerate() {
        if i < kept.len() {
            s.seat = kept[i];
            s.dim = 0.0;
            continue;
        }
        // Its own seats: every `free`-th from its place.
        let k = i - kept.len();
        let own = others.len().saturating_sub(k).div_ceil(free.max(1)).max(1);
        s.seat = others
            .get(k + free * (turn % own))
            .or(others.get(k))
            .copied()
            .unwrap_or(0);
        s.dim = if own > 1 { dim } else { 0.0 };
    }
}

/// The seats a stage keeps in view whatever the turn ([`rotate`]: only
/// beyond the cap, where the opponents take turns): whoever attacks the
/// player, then whom the player attacked last, while those attacks last
/// and one turn of `policy` after (`ddi_session::battle::Battle::kept_seats`),
/// so that who attacks whom can be shown.
pub fn priority_seats(
    battle: &ddi_session::battle::Battle,
    player: &ddi_engine::player::Player,
    policy: Rotation,
) -> Vec<usize> {
    battle.kept_seats(player, f64::from(policy.every))
}

/// The ladder's colors: everyone's life as a mark, the player's own in
/// the color "Stage won" has, those out a faint stub.
const LADDER_MARK: [f32; 4] = [0.62, 0.86, 0.78, 0.85];
const LADDER_OUT: [f32; 4] = [0.80, 0.80, 0.86, 0.30];
const LADDER_ME: [f32; 4] = WON;
/// Its strip's height and its marks' widths, logical pixels.
const LADDER_STRIP: f32 = 7.0;
const LADDER_MARK_MOST: f32 = 4.0;

/// A large field's ladder in `place` (`[x, y, width, height]`, the
/// target's pixels): its words ("17 of 73 · 41 still in") at the left,
/// then a thin strip of everyone's life as marks, best first, the
/// player's own highlighted, so the whole field is felt without a field
/// for everyone.
pub fn draw_ladder(
    list: &mut DrawList,
    words: &str,
    ladder: &crate::rivals::Ladder,
    place: [f32; 4],
    ratio: f32,
    measure: &mut dyn TextMeasure,
) {
    let [x, y, w, h] = place;
    let face = ddi_l10n::font_stack(FACE);
    let mut st = TextStyle::new(face.as_str(), 12.0 * ratio);
    st.weight = 600;
    let t = fitted(measure, words, &st, w * 0.6);
    let advance = measure.measure(&t, &st).advance;
    list.push(run(t, &st, [x, y + (h - 12.0 * 1.2 * ratio) / 2.0], MUTED));
    let from = x + advance + 8.0 * ratio;
    let room = (x + w - from).max(0.0);
    let n = ladder.marks.len().max(1) as f32;
    let step = (room / n).min(LADDER_MARK_MOST * ratio);
    if step < 0.5 * ratio {
        return;
    }
    let strip = LADDER_STRIP * ratio;
    let bottom = y + (h + strip) / 2.0;
    for (k, &(life, out, me)) in ladder.marks.iter().enumerate() {
        let tall = if out {
            1.5 * ratio
        } else {
            (life.clamp(0.0, 1.0) * strip).max(1.5 * ratio)
        };
        let color = if me {
            LADDER_ME
        } else if out {
            LADDER_OUT
        } else {
            LADDER_MARK
        };
        let mx = from + k as f32 * step;
        let mw = (step - 1.0 * ratio).max(0.75 * ratio);
        // The player's own reaches over the strip, so it is found at once.
        let (top, tall) = if me {
            (bottom - strip - 2.0 * ratio, strip + 4.0 * ratio)
        } else {
            (bottom - tall, tall)
        };
        list.rect([mx, top, mx + mw, top + tall], color);
    }
}

/// Cuts `s` to `room` pixels, an ellipsis at its end.
fn fitted(measure: &mut dyn TextMeasure, s: &str, style: &TextStyle, room: f32) -> String {
    let mut s = s.to_string();
    while !s.is_empty() && measure.measure(&s, style).advance > room {
        s.pop();
        while s.ends_with(' ') {
            s.pop();
        }
        if s.is_empty() {
            break;
        }
        s.pop();
        // No space before the ellipsis.
        while s.ends_with(' ') {
            s.pop();
        }
        s.push('…');
    }
    s
}

fn run(text: String, style: &TextStyle, at: [f32; 2], color: [f32; 4]) -> Command {
    Command::Text(TextRun {
        curve: None,
        text: text.into(),
        style: style.clone(),
        transform: Affine::translate(at[0], at[1]),
        color,
        blend: Blend::Normal,
        mask: Mask::None,
        perspective: None,
        blur: [0.0; 2],
    })
}

/// What [`draw_fields`] adds to [`Shown`]: each miniature's judgments
/// to flash on its receptors, where the play's noteskin does not draw
/// their hit effects (empty where it does), a slot's lanes'
/// `(judgment, seconds since)`.
pub type Flashes = Vec<Vec<Option<(ddi_engine::rules::Judgement, f32)>>>;

/// Draws what the miniatures in `fields` show around and over their
/// fields (drawn by the renderer before this, their life gauges and
/// combos by the play's HUD): each one's name and level above its field,
/// the flashes of its judgments where the skin does not draw them
/// (`flashes`, in `0..`[`FLASH`] seconds), its Out, a slot darkening as
/// its seat changes; in the status line, "Stage won" and a battle's moment
/// no line said. Whether a moment's cue was drawn.
pub fn draw_fields(
    list: &mut DrawList,
    shown: &Shown,
    fields: &Fields,
    flashes: &[Vec<Option<(ddi_engine::rules::Judgement, f32)>>],
    locks: &[Vec<Option<f32>>],
    ratio: f32,
    measure: &mut dyn TextMeasure,
) -> bool {
    let face = ddi_l10n::font_stack(FACE);
    let style = |size: f32, weight: u16| {
        let mut s = TextStyle::new(face.as_str(), size * ratio);
        s.weight = weight;
        s
    };
    // The status line: "Stage won", then the moment, at its left.
    let [sx, sy, sw, _] = fields.status;
    let mut x = sx;
    if let Some(label) = &shown.label {
        let st = style(14.0, 600);
        let t = fitted(measure, label, &st, sw);
        x += measure.measure(&t, &st).advance + 12.0 * ratio;
        list.push(run(t, &st, [sx, sy], MUTED));
    }
    if let Some(won) = &shown.won {
        let st = style(14.0, 700);
        let t = fitted(measure, won, &st, (sx + sw - x).max(0.0));
        let at = x;
        x += measure.measure(&t, &st).advance + 12.0 * ratio;
        list.push(run(t, &st, [at, sy], WON));
    }
    let cue = shown.cue.as_ref().filter(|(_, a)| *a > 0.0);
    if let Some((cue, alpha)) = cue {
        let st = style(14.0, 600);
        let t = fitted(measure, cue, &st, (sx + sw - x).max(0.0));
        let mut c = TEXT;
        c[3] *= alpha.clamp(0.0, 1.0);
        list.push(run(t, &st, [x, sy], c));
    }
    if let (Some((words, ladder)), Some(place)) = (&shown.ladder, fields.ladder) {
        draw_ladder(list, words, ladder, place, ratio, measure);
    }
    for (i, slot) in fields.slots.iter().enumerate() {
        let Some(o) = shown.opponents.get(slot.seat) else {
            continue;
        };
        let [x, y, w, h] = slot.rect;
        let dim = if o.out {
            0.55
        } else if shown.stale.contains(&slot.seat) {
            0.7
        } else {
            1.0
        };
        let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * dim];
        // Its name, cut to its room, and its level's stars: at the name
        // line's end where the slot is wide, on a short line of their own
        // under it where it is not, left out at the very smallest (a CPU's
        // level; a person has none). Out shows on the field.
        let size = fields.name;
        let name_style = style(size, 600);
        let large = shown.large == Some(i);
        let stars = o
            .level
            .filter(|_| !large)
            .map(|level| (level, fields.stars));
        let room = match stars {
            Some((level, Stars::Beside(tall))) => {
                let sw = ddi_ui::stars::width(tall * ratio);
                // On the name line's middle (a line is 1.2 of its size).
                draw_stars(
                    list,
                    level,
                    [x + w - sw, y + size * 0.6 * ratio],
                    tall * ratio,
                    dim,
                );
                w - sw - 6.0 * ratio
            }
            Some((level, Stars::Under(tall))) => {
                let top = y + (size * 1.2 + STARS_UNDER_GAP) * ratio;
                draw_stars(
                    list,
                    level,
                    [x, top + tall * ratio / 2.0],
                    tall * ratio,
                    dim,
                );
                w
            }
            _ => w,
        };
        if !large {
            let name = fitted(measure, &o.name, &name_style, room.max(0.0));
            list.push(run(name, &name_style, [x, y], fade(TEXT)));
        }
        let geo = &slot.field;
        // Its lanes a shock locked (DDI): the lane darkened to its end,
        // its receptor's place tinted, for the lock's time.
        for (lane, lock) in locks.get(i).into_iter().flatten().enumerate() {
            if lock.is_none() || o.out {
                continue;
            }
            let (cx, half) = (geo.lane_x(lane as f32), geo.arrow / 2.0);
            let [_, t, _, b] = slot.panel;
            list.rect([cx - half, t, cx + half, b], LOCK_LANE);
            let cy = geo.receptor_y;
            list.rect([cx - half, cy - half, cx + half, cy + half], LOCK_TINT);
        }
        // Its score, small, under its life gauge's right end.
        if !large {
            let s = geo.arrow / 72.0;
            let size = (size * SCORE_SIZE.0).clamp(SCORE_SIZE.1, SCORE_SIZE.2);
            let mut st = style(size, 600);
            let text = shown.score.text(o);
            let mut tw = measure.measure(&text, &st).advance;
            // A long score on a narrow field (a phone's): smaller, so that
            // it stays within its field's four lanes.
            let room = 4.0 * geo.arrow;
            if tw > room && tw > 0.0 {
                st = style((size * room / tw).max(SCORE_FLOOR), 600);
                tw = measure.measure(&text, &st).advance;
            }
            let [l, t, r, _] = slot.panel;
            let right = (l + r) / 2.0 + 2.0 * geo.arrow;
            list.push(run(
                text,
                &st,
                [right - tw, t + SCORE_TOP * s],
                fade(MUTED),
            ));
        }
        // The judgments' flashes where the skin draws no hit effect.
        for (lane, flash) in flashes.get(i).into_iter().flatten().enumerate() {
            let Some((j, age)) = *flash else {
                continue;
            };
            // A stage plays DDI alone (`ddi_room::MatchRules::RULESETS`):
            // its tiers' colors and strengths, as the play's own field.
            let palette = ddi_engine::rules::colors::Palette::Ddi;
            let k = ddi_render::scene::flash_strength(palette, j)
                * (1.0 - age / FLASH).clamp(0.0, 1.0);
            if k <= 0.0 {
                continue;
            }
            // A soft glow round the receptor, in the judgment's color.
            let mut c = ddi_render::scene::tier_color(palette, j);
            c[3] = 0.55 * k;
            let (cx, cy) = (geo.lane_x(lane as f32), geo.receptor_y);
            let r = geo.arrow * 0.55;
            let mut q = Quad::plain([cx - r, cy - r, cx + r, cy + r], c);
            // In the rectangle's units: the target's pixels.
            q.shape = Shape {
                radius: [r; 2],
                soft: [r * 0.5; 2],
                ..Shape::NONE
            };
            q.blend = Blend::Add;
            list.push(Command::Quad(q));
        }
        // Out: the field dimmed, in place, saying so.
        if o.out {
            let [l, t, r, b] = slot.panel;
            list.rect([l, t, r, b], [0.0, 0.0, 0.0, 0.45]);
            let st = style(size.min(13.0), 600);
            let said = shown
                .own
                .as_ref()
                .filter(|(seat, _)| *seat == slot.seat)
                .map_or(&shown.out, |(_, w)| w);
            let word = fitted(measure, said, &st, r - l);
            let ww = measure.measure(&word, &st).advance;
            list.push(run(
                word,
                &st,
                [
                    (l + r - ww) / 2.0,
                    (t + b) / 2.0 - size.min(13.0) * 0.6 * ratio,
                ],
                MUTED,
            ));
        }
        // Its seat changing: the whole slot darkens through the change.
        if slot.dim > 0.0 {
            list.rect([x, y, x + w, y + h], [0.0, 0.0, 0.0, 0.9 * slot.dim]);
        }
    }
    cue.is_some()
}

/// A player's own field after they are out of a stage they go on
/// watching (`watch-v0`): its band, the mark over it (Out, or that they
/// left the stage) and the skip's words, while it can be skipped.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnOut {
    /// The field's band, `[left, top, right, bottom]`, target pixels.
    pub panel: [f32; 4],
    pub mark: String,
    pub skip: Option<String>,
    /// Their own field is in the band, dimmed with the mark; not while
    /// they follow a competitor large there (the skip's plate alone, at
    /// the band's foot: their own field is then a miniature).
    pub dimmed: bool,
    /// How dark the panel is drawn under the mark where `dimmed` (a
    /// player's own field out: [`OUT_VEIL`]; a watched stage whose seat
    /// ended, over everything: [`ENDED_VEIL`]).
    pub veil: f32,
}

/// The darkness over a player's own field out of the stage (a
/// miniature's).
pub const OUT_VEIL: f32 = 0.45;
/// Over a watched stage whose seat ended: its fields stale behind it.
pub const ENDED_VEIL: f32 = 0.82;

impl OwnOut {
    /// Where a tap or a click skips to the results (target pixels): a
    /// plate under the mark, in the band's middle.
    pub fn skip_area(&self, ratio: f32) -> [f32; 4] {
        let [l, t, r, b] = self.panel;
        let cx = (l + r) / 2.0;
        let cy = if self.dimmed {
            (t + b) / 2.0
        } else {
            b - 96.0 * ratio
        };
        let half = ((r - l) / 2.0 - 8.0 * ratio).clamp(40.0 * ratio, 130.0 * ratio);
        [cx - half, cy + 22.0 * ratio, cx + half, cy + 52.0 * ratio]
    }
}

/// Draws a player's own field as out of the stage: dimmed in place, as
/// a miniature's (the same darkness), its mark in its middle and, under
/// it, the skip's words on a quiet plate.
pub fn draw_own_out(list: &mut DrawList, own: &OwnOut, ratio: f32, measure: &mut dyn TextMeasure) {
    let face = ddi_l10n::font_stack(FACE);
    let style = |size: f32, weight: u16| {
        let mut s = TextStyle::new(face.as_str(), size * ratio);
        s.weight = weight;
        s
    };
    let [l, t, r, b] = own.panel;
    let cx = (l + r) / 2.0;
    if own.dimmed {
        list.rect([l, t, r, b], [0.0, 0.0, 0.0, own.veil]);
        let cy = (t + b) / 2.0;
        let st = style(24.0, 700);
        let mark = fitted(measure, &own.mark, &st, r - l - 8.0 * ratio);
        let w = measure.measure(&mark, &st).advance;
        list.push(run(mark, &st, [cx - w / 2.0, cy - 24.0 * ratio], TEXT));
    }
    if let Some(skip) = &own.skip {
        let [pl, pt, pr, pb] = own.skip_area(ratio);
        let mut plate = Quad::plain([pl, pt, pr, pb], [1.0, 1.0, 1.0, 0.08]);
        plate.shape = Shape {
            radius: [6.0 * ratio; 2],
            ..Shape::NONE
        };
        list.push(Command::Quad(plate));
        let st = style(13.0, 600);
        let text = fitted(measure, skip, &st, pr - pl - 12.0 * ratio);
        let w = measure.measure(&text, &st).advance;
        list.push(run(
            text,
            &st,
            [cx - w / 2.0, (pt + pb) / 2.0 - 13.0 * 0.6 * ratio],
            MUTED,
        ));
    }
}

/// Draws `level`'s five stars (`ddi_ui::stars`: gold, silver, empty
/// outlines) `tall` target pixels high, their left edge at `at[0]`, their
/// middle on `at[1]`, in the theme's colors (`ddi_theme::star_colors`) at
/// `alpha`.
pub fn draw_stars(list: &mut DrawList, level: u8, at: [f32; 2], tall: f32, alpha: f32) {
    use ddi_ui::stars::Star;
    let colors = ddi_theme::star_colors();
    let centers = ddi_ui::stars::centers(at[0], at[1], tall);
    for (star, center) in ddi_ui::stars::fills(level).into_iter().zip(centers) {
        let mut color = match star {
            Star::Gold => colors[0],
            Star::Silver => colors[1],
            Star::Empty => colors[2],
        };
        color[3] *= alpha;
        if star == Star::Empty {
            // Its outline's ten strokes, their ends round, inside its box.
            let thick = ddi_ui::stars::stroke(tall);
            let shape = ddi_ui::stars::shape(center, tall - thick);
            for i in 0..10 {
                let (p, q) = (shape.outline[i], shape.outline[(i + 1) % 10]);
                let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                let len = dx.hypot(dy) + thick;
                let mut quad =
                    Quad::plain([-len / 2.0, -thick / 2.0, len / 2.0, thick / 2.0], color);
                quad.transform = Affine::rotate(dy.atan2(dx))
                    .then(Affine::translate((p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0));
                quad.shape = Shape {
                    radius: [thick / 2.0; 2],
                    ..Shape::NONE
                };
                list.push(Command::Quad(quad));
            }
            continue;
        }
        // Filled: five rhombi, each a unit square sent to its two sides
        // (cut by a hair, so its edges are drawn smooth), and the disc at
        // the middle.
        let shape = ddi_ui::stars::shape(center, tall);
        for r in shape.rhombi {
            let mut quad = Quad::plain([-0.5, -0.5, 0.5, 0.5], color);
            let [a, b] = r.sides;
            quad.transform = Affine([a[0], a[1], b[0], b[1], r.center[0], r.center[1]]);
            quad.shape = Shape {
                radius: [0.02; 2],
                ..Shape::NONE
            };
            list.push(Command::Quad(quad));
        }
        let ([cx, cy], radius) = shape.disc;
        let mut disc = Quad::plain([cx - radius, cy - radius, cx + radius, cy + radius], color);
        disc.shape = Shape {
            radius: [radius; 2],
            ..Shape::NONE
        };
        list.push(Command::Quad(disc));
    }
}

/// How long [`draw_fields`] flashes a judgment, seconds.
pub const FLASH: f32 = ddi_session::battle::feed::FLASH_SECONDS;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ddi_actor::text::FixedMeasure;
    use ddi_room::ParticipantId;

    fn shown(n: usize) -> Shown {
        Shown {
            opponents: (0..n)
                .map(|i| OpponentView {
                    participant: ParticipantId(i as u64 + 2),
                    name: format!("CPU {}", i + 1),
                    level: Some(3),
                    life: 0.5,
                    out: i == 1,
                    score: 0.9,
                    sum: Some(5_900_000),
                    combo: 10,
                    power: None,
                })
                .collect(),
            out: "Out".into(),
            won: None,
            cue: None,
            score: ScoreShown::Sum,
            label: None,
            large: None,
            stale: Vec::new(),
            ladder: None,
            own: None,
        }
    }

    fn texts(list: &DrawList) -> Vec<(String, f32)> {
        list.commands
            .iter()
            .filter_map(|c| match c {
                Command::Text(t) => Some((t.text.to_string(), t.color[3])),
                _ => None,
            })
            .collect()
    }

    fn geo(w: f32, h: f32) -> FieldGeometry {
        FieldGeometry::placed(w, h, 4.0, false, Default::default())
    }

    fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
    }

    fn ltrb([x, y, w, h]: [f32; 4]) -> [f32; 4] {
        [x, y, x + w, y + h]
    }

    /// The play's own keep-outs: its HUD, the quit button's place and,
    /// with `attacks`, the attack warning's.
    pub(crate) fn keep(g: &FieldGeometry, ratio: f32, attacks: bool) -> Vec<[f32; 4]> {
        let mut k = ddi_render::scene::hud_areas(g, 4.0);
        let [x, y, r] =
            ddi_render::scene::quit_button(g, &ddi_chart::Layout::dance_single(), ratio, [0.0; 4]);
        k.push([x - r, y - r, x + r, y + r]);
        if attacks {
            k.push(crate::battle_cue::place(g, 4.0));
        }
        k
    }

    /// The sizes a stage is looked at (the feedback round's table),
    /// logical pixels and target pixels to one.
    pub(crate) const SIZES: [(f32, f32, f32); 10] = [
        (1280.0, 720.0, 1.0),
        (1920.0, 1080.0, 1.0),
        (1024.0, 768.0, 1.0),
        (800.0, 600.0, 1.0),
        (720.0, 900.0, 1.0),
        (844.0, 390.0, 3.0),
        (390.0, 844.0, 3.0),
        (2560.0, 1080.0, 1.0),
        (1280.0, 720.0, 2.0),
        (360.0, 640.0, 2.0),
    ];

    /// Every slot on the screen, clear of the field's band, of what the
    /// play keeps clear and of each other, all of one size, each field
    /// inside its band, as many lanes tall as the play's own
    /// ([`rows_for`]) with its receptors a lane and a half from its end,
    /// its lane no more than [`MINI_LANE_SHARE`] of the play's; the
    /// status line on the screen and clear of the slots.
    fn sound(f: &Fields, g: &FieldGeometry, k: &[[f32; 4]], reverse: bool, what: &str) {
        let (w, h) = (g.width, g.height);
        let [mid, band] = ddi_render::scene::filter_band(g, 4.0);
        let field = [mid - band / 2.0, 0.0, mid + band / 2.0, h];
        for (i, s) in f.slots.iter().enumerate() {
            let r = ltrb(s.rect);
            assert!(
                r[0] >= 0.0 && r[2] <= w + 0.01 && r[1] >= 0.0 && r[3] <= h + 0.01,
                "{what} slot {i} {r:?} off the screen"
            );
            assert!(!overlaps(r, field), "{what} slot {i} over the field");
            for k in k {
                assert!(!overlaps(r, *k), "{what} slot {i} {r:?} over {k:?}");
            }
            for t in &f.slots[i + 1..] {
                assert!(!overlaps(r, ltrb(t.rect)), "{what} slot {i} over another");
                assert_eq!([s.rect[2], s.rect[3]], [t.rect[2], t.rect[3]], "{what}");
            }
            assert_eq!(s.left, r[2] <= field[0], "{what} slot {i}");
            let a = s.field.arrow;
            assert!(s.field.lane_x(0.0) - a / 2.0 > s.panel[0]);
            assert!(s.field.lane_x(3.0) + a / 2.0 < s.panel[2]);
            let from = if reverse {
                s.panel[3] - s.field.receptor_y
            } else {
                s.field.receptor_y - s.panel[1]
            };
            assert!((from - 1.5 * a).abs() < 0.01, "{what} slot {i}");
            assert!(s.panel[1] >= r[1] && s.panel[3] <= r[3] + 0.01);
            // The play's field's proportion, at half its size at most.
            let tall = (s.panel[3] - s.panel[1]) / a;
            assert!((tall - rows_for(g)).abs() < 0.01, "{what} slot {i} {tall}");
            assert!(a <= g.arrow * MINI_LANE_SHARE + 0.01, "{what} slot {i}");
            assert!(
                !overlaps(r, ltrb(f.status)),
                "{what} slot {i} over the status"
            );
        }
        let st = ltrb(f.status);
        assert!(
            st[0] >= 0.0 && st[2] <= w + 0.01 && st[1] >= 0.0 && st[3] <= h + 0.01,
            "{what} status {st:?}"
        );
        assert!(f.status[2] > 0.0, "{what} status {st:?}");
        assert!((MINI_ROWS_MIN..=MINI_ROWS_MAX).contains(&f.rows), "{what}");
        assert_eq!(f.rows, rows_for(g), "{what}");
        assert!((f.range - (f.rows - 1.5)).abs() < 1e-4);
    }

    /// The quit button's place alone: what a watched stage's grid keeps
    /// clear of (it has no HUD of its own).
    pub(crate) fn keep_grid(g: &FieldGeometry, ratio: f32) -> Vec<[f32; 4]> {
        let [x, y, r] =
            ddi_render::scene::quit_button(g, &ddi_chart::Layout::dance_single(), ratio, [0.0; 4]);
        vec![[x - r, y - r, x + r, y + r]]
    }

    /// A watched stage's grid (`watch-v0`): one to eight fields over the
    /// whole screen at every size of the miniatures' table, each on the
    /// screen with its attack warning's room, clear of the quit button's
    /// place, of the status line and of each other, all of one size, in
    /// the play's field's proportion, never larger than a play's own
    /// field; the block in the screen's middle.
    #[test]
    fn a_watched_grid_fills_the_screen_for_one_to_eight() {
        for (w, h, ratio) in SIZES {
            for reverse in [false, true] {
                let g =
                    FieldGeometry::placed(w * ratio, h * ratio, 4.0, reverse, Default::default());
                let k = keep_grid(&g, ratio);
                for n in 1..=8 {
                    let what = format!("{n} at {w}x{h}@{ratio} reverse {reverse}");
                    let f = arrange_grid(n, &g, reverse, ratio, &k)
                        .unwrap_or_else(|| panic!("{what}"));
                    assert_eq!(f.slots.len(), n, "{what}");
                    assert!(!f.rotating, "{what}");
                    assert!(f.slots.iter().enumerate().all(|(i, s)| s.seat == i));
                    let (sw, sh) = (g.width, g.height);
                    for (i, s) in f.slots.iter().enumerate() {
                        let r = ltrb(s.rect);
                        // With its warning's room on its right.
                        let cue = crate::battle_cue::place(&s.field, 4.0);
                        let all = [r[0], r[1], r[2].max(cue[2]), r[3]];
                        assert!(
                            all[0] >= 0.0
                                && all[2] <= sw + 0.01
                                && all[1] >= 0.0
                                && all[3] <= sh + 0.01,
                            "{what} slot {i} {all:?} off the screen"
                        );
                        assert!(cue[1] >= s.panel[1] && cue[3] <= s.panel[3], "{what}");
                        for keep in &k {
                            assert!(!overlaps(all, *keep), "{what} slot {i} over {keep:?}");
                        }
                        for t in &f.slots[i + 1..] {
                            let o = ltrb(t.rect);
                            let oc = crate::battle_cue::place(&t.field, 4.0);
                            let other = [o[0], o[1], o[2].max(oc[2]), o[3]];
                            assert!(!overlaps(all, other), "{what} slot {i} over another");
                            assert_eq!([s.rect[2], s.rect[3]], [t.rect[2], t.rect[3]], "{what}");
                        }
                        assert!(!overlaps(all, ltrb(f.status)), "{what} over the status");
                        let a = s.field.arrow;
                        assert!(s.field.lane_x(0.0) - a / 2.0 > s.panel[0]);
                        assert!(s.field.lane_x(3.0) + a / 2.0 < s.panel[2]);
                        let from = if reverse {
                            s.panel[3] - s.field.receptor_y
                        } else {
                            s.field.receptor_y - s.panel[1]
                        };
                        assert!((from - 1.5 * a).abs() < 0.01, "{what} slot {i}");
                        let tall = (s.panel[3] - s.panel[1]) / a;
                        assert!((tall - rows_for(&g)).abs() < 0.01, "{what} {tall}");
                        // A field as a player's own, at the most.
                        assert!(a <= g.arrow * GRID_LANE_SHARE + 0.01, "{what}");
                        assert!(!s.left);
                    }
                    let st = ltrb(f.status);
                    assert!(
                        st[0] >= 0.0 && st[2] <= sw + 0.01 && st[3] <= sh + 0.01,
                        "{what} status {st:?}"
                    );
                    // In the screen's middle: as much room left as right.
                    let left = f.slots.iter().map(|s| s.rect[0]).fold(f32::MAX, f32::min);
                    let right = f
                        .slots
                        .iter()
                        .map(|s| crate::battle_cue::place(&s.field, 4.0)[2])
                        .fold(0.0, f32::max);
                    if k.iter().all(|keep| keep[3] < f.slots[0].rect[1]) {
                        assert!(
                            (left - (sw - right)).abs() <= 1.5 * ratio + (sw * 0.2),
                            "{what}: {left} and {}",
                            sw - right
                        );
                    }
                }
            }
        }
    }

    /// The grid's sizes at the two sizes the miniatures' table gives:
    /// one and two nearly the screen's height, eight four by two; larger
    /// than the miniatures beside a player's own field at every count.
    #[test]
    fn the_grid_s_fields_are_as_large_as_the_screen_allows() {
        let lanes = |w: f32, h: f32| -> Vec<(f32, usize)> {
            let g = geo(w, h);
            let k = keep_grid(&g, 1.0);
            (1..=8)
                .map(|n| {
                    let f = arrange_grid(n, &g, false, 1.0, &k).unwrap();
                    let top = f.slots[0].rect[1];
                    let cols = f.slots.iter().filter(|s| s.rect[1] == top).count();
                    (f.lane, cols)
                })
                .collect()
        };
        let small = lanes(1280.0, 720.0);
        let large = lanes(1920.0, 1080.0);
        // (lane, fields in the first row) for one to eight.
        assert_eq!(
            small,
            [
                (65.0, 1),
                (65.0, 2),
                (63.0, 3),
                (57.0, 4),
                (45.0, 5),
                (37.0, 6),
                (29.0, 4),
                (29.0, 4)
            ]
        );
        assert_eq!(
            large,
            [
                (101.0, 1),
                (101.0, 2),
                (99.0, 3),
                (86.0, 4),
                (68.0, 5),
                (56.0, 6),
                (48.0, 4),
                (48.0, 4)
            ]
        );
        for (w, h) in [(1280.0, 720.0), (1920.0, 1080.0)] {
            let g = geo(w, h);
            // Two: nearly the screen's height.
            let two = arrange_grid(2, &g, false, 1.0, &keep_grid(&g, 1.0)).unwrap();
            let tall = two.slots[0].rect[3];
            assert!(tall > 0.9 * h, "{tall} of {h}");
            // Eight: two rows of four.
            let eight = arrange_grid(8, &g, false, 1.0, &keep_grid(&g, 1.0)).unwrap();
            let rows: std::collections::BTreeSet<i32> =
                eight.slots.iter().map(|s| s.rect[1] as i32).collect();
            assert_eq!(rows.len(), 2);
            // Larger than a player's miniatures at every count.
            for n in 1..=8 {
                let grid = arrange_grid(n, &g, false, 1.0, &keep_grid(&g, 1.0)).unwrap();
                let side = arrange(n, &g, 4.0, false, 1.0, &keep(&g, 1.0, true)).unwrap();
                assert!(grid.lane > side.lane, "{n}: {} and {}", grid.lane, side.lane);
            }
        }
    }

    /// The field a watcher follows is the play's field's place: its band
    /// from the screen's top to its bottom, its geometry the play's; the
    /// others go beside it as a player's opponents do, never over it;
    /// what it has over it is drawn as a miniature's, without the name
    /// line and the small score (the HUD's places say them).
    #[test]
    fn the_followed_field_is_the_play_s_place_and_the_others_go_beside_it() {
        let g = geo(1280.0, 720.0);
        let big = large_slot(3, &g, 4.0);
        assert_eq!(big.field, g);
        let [mid, band] = ddi_render::scene::filter_band(&g, 4.0);
        assert_eq!(big.panel, [mid - band / 2.0, 0.0, mid + band / 2.0, 720.0]);
        assert_eq!(big.seat, 3);
        let k = keep(&g, 1.0, true);
        let mut f = arrange(3, &g, 4.0, false, 1.0, &k).unwrap();
        sound(&f, &g, &k, false, "beside the followed");
        for s in &f.slots {
            assert!(!overlaps(ltrb(s.rect), big.panel));
        }
        // Drawn with the others: Out over it, no name and no score.
        f.slots.push(big);
        let mut shown = shown(4);
        shown.large = Some(3);
        shown.opponents[3].out = true;
        shown.opponents[3].name = "Followed".into();
        shown.label = Some("Watching Followed".into());
        let mut list = DrawList::new();
        let mut measure = FixedMeasure;
        draw_fields(&mut list, &shown, &f, &[], &[], 1.0, &mut measure);
        let texts = texts(&list);
        let words: Vec<&str> = texts.iter().map(|(t, _)| t.as_str()).collect();
        assert!(words.contains(&"Watching Followed"), "{words:?}");
        assert!(!words.contains(&"Followed"), "{words:?}");
        assert_eq!(words.iter().filter(|t| **t == "Out").count(), 2, "{words:?}");
        // Three small scores (the others'), none for the large field.
        assert_eq!(words.iter().filter(|t| **t == "5900000").count(), 3);
    }

    /// A player's own field after they are out of the stage: dimmed as a
    /// miniature's, its mark in its middle, the skip's plate under it
    /// inside the band.
    #[test]
    fn a_player_s_own_field_out_of_the_stage_says_so_and_how_to_skip() {
        for (w, h, ratio) in SIZES {
            let g = FieldGeometry::placed(w * ratio, h * ratio, 4.0, false, Default::default());
            let [mid, band] = ddi_render::scene::filter_band(&g, 4.0);
            let own = OwnOut {
                panel: [mid - band / 2.0, 0.0, mid + band / 2.0, g.height],
                mark: "Left the stage".into(),
                skip: Some("Enter: skip to results".into()),
                dimmed: true,
                veil: OUT_VEIL,
            };
            // Following a competitor large: the plate alone, at the
            // band's foot, below its middle.
            let low = OwnOut {
                dimmed: false,
                ..own.clone()
            };
            let [_, lt, _, lb] = low.skip_area(ratio);
            assert!(lt > g.height * 0.75 && lb < g.height, "{w}x{h}");
            let mut list = DrawList::new();
            draw_own_out(&mut list, &low, ratio, &mut FixedMeasure);
            let said = texts_of(&list);
            assert!(!said.iter().any(|t| t.starts_with("Left")) && said.iter().any(|t| t.contains("skip")));
            let [l, t, r, b] = own.skip_area(ratio);
            assert!(l >= own.panel[0] && r <= own.panel[2], "{w}x{h}");
            assert!(t > g.height / 2.0 && b < g.height, "{w}x{h}");
            let mut list = DrawList::new();
            let mut measure = FixedMeasure;
            draw_own_out(&mut list, &own, ratio, &mut measure);
            let texts = texts(&list);
            assert!(texts.iter().any(|(t, _)| t.starts_with("Left")), "{texts:?}");
            assert!(texts.iter().any(|(t, _)| t.contains("skip")), "{texts:?}");
            // Once it cannot be skipped (the results are coming): no plate.
            let over = OwnOut { skip: None, ..own };
            let mut list = DrawList::new();
            draw_own_out(&mut list, &over, ratio, &mut measure);
            assert!(!texts_of(&list).iter().any(|t| t.contains("skip")));
        }
    }

    fn texts_of(list: &DrawList) -> Vec<String> {
        texts(list).into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn eight_miniatures_of_their_own_at_every_size() {
        for (w, h, ratio) in SIZES {
            for (reverse, attacks) in [(false, false), (true, false), (false, true), (true, true)] {
                let g =
                    FieldGeometry::placed(w * ratio, h * ratio, 4.0, reverse, Default::default());
                let k = keep(&g, ratio, attacks);
                let what = format!("{w}x{h}@{ratio} reverse {reverse} attacks {attacks}");
                let f = arrange(8, &g, 4.0, reverse, ratio, &k).unwrap_or_else(|| panic!("{what}"));
                // Every opponent its own slot, never taking turns.
                assert_eq!(f.slots.len(), 8, "{what}");
                assert!(!f.rotating, "{what}");
                assert!(f.slots.iter().enumerate().all(|(i, s)| s.seat == i));
                // The first four on the left, the rest on the right; in
                // the narrowest margins (360 logical pixels wide, where
                // the lanes are at their least) a side short of height
                // for four gives one to the other.
                let left = f.slots.iter().filter(|s| s.left).count();
                if f.lane > MINI_LANE_MIN {
                    assert_eq!(left, 4, "{what}");
                } else {
                    assert!((3..=5).contains(&left), "{what} {left}");
                }
                assert!(f.slots[..left].iter().all(|s| s.left), "{what}");
                assert!(f.slots[left..].iter().all(|s| !s.left), "{what}");
                sound(&f, &g, &k, reverse, &what);
                // Smaller than the play's own lanes, never under the least.
                assert!(f.slots[0].field.arrow < g.arrow, "{what}");
                assert!(f.lane >= MINI_LANE_MIN, "{what} lane {}", f.lane);
                // Whole target pixels.
                assert_eq!((f.lane * ratio).fract(), 0.0, "{what}");
            }
        }
    }

    #[test]
    fn one_to_eight_each_have_their_own_at_1280x720() {
        let g = geo(1280.0, 720.0);
        let k = keep(&g, 1.0, true);
        let mut last = f32::MAX;
        for n in 1..=8 {
            let f = arrange(n, &g, 4.0, false, 1.0, &k).unwrap();
            assert_eq!(f.slots.len(), n);
            assert!(!f.rotating);
            sound(&f, &g, &k, false, &format!("{n}"));
            assert_eq!(f.slots.iter().filter(|s| s.left).count(), n.div_ceil(2));
            // Never larger with more of them.
            assert!(f.lane <= last, "{n}: {} after {last}", f.lane);
            last = f.lane;
        }
    }

    /// The feedback round of 2026-11-12, part C: a miniature's field has
    /// the play's own field's proportion, at half its size at the most,
    /// so one to eight opponents come out at similar sizes.
    #[test]
    fn the_miniatures_have_the_plays_proportion_under_a_size_cap() {
        // The lanes (logical pixels) for 1, 2, 4, 6 and 8 opponents: at
        // the cap, half the play's lane, up to four; six three in a row
        // a side, eight two rows of two (four in a row would be 24 and
        // 37).
        for (w, h, lanes) in [
            (1280.0, 720.0, [36.0, 36.0, 36.0, 33.0, 28.0]),
            (1920.0, 1080.0, [54.0, 54.0, 54.0, 52.0, 46.0]),
        ] {
            let g = geo(w, h);
            let k = keep(&g, 1.0, false);
            assert_eq!(rows_for(&g), 10.0);
            for (n, lane) in [1, 2, 4, 6, 8].into_iter().zip(lanes) {
                let f = arrange(n, &g, 4.0, false, 1.0, &k).unwrap();
                assert_eq!(f.lane, lane, "{w}x{h} {n}");
                assert!(f.lane <= g.arrow * MINI_LANE_SHARE);
                // Ten lanes tall as the play's field, the receptors at
                // the same share of its height.
                let s = &f.slots[0];
                assert_eq!(f.rows, 10.0);
                assert!((s.panel[3] - s.panel[1] - 10.0 * lane).abs() < 0.01);
                let share = (s.field.receptor_y - s.panel[1]) / (s.panel[3] - s.panel[1]);
                assert!((share - g.receptor_y / g.height).abs() < 1e-4, "{share}");
                sound(&f, &g, &k, false, &format!("{w}x{h} {n}"));
            }
            // Similar sizes: eight are no less than three quarters of one.
            assert!(lanes[4] >= 0.75 * lanes[0]);
            // Eight: two rows of two a side.
            let eight = arrange(8, &g, 4.0, false, 1.0, &k).unwrap();
            let tops: std::collections::BTreeSet<i32> = eight
                .slots
                .iter()
                .filter(|s| s.left)
                .map(|s| s.rect[1] as i32)
                .collect();
            assert_eq!(tops.len(), 2, "{w}x{h}");
        }
        // The attack warning's place takes a little from the right side.
        let g = geo(1280.0, 720.0);
        let with = arrange(8, &g, 4.0, false, 1.0, &keep(&g, 1.0, true)).unwrap();
        assert_eq!(with.lane, 25.0);
        // Where four in a row give more, four in a row: a very wide
        // screen, all eight at the cap.
        let g = geo(2560.0, 1080.0);
        let k = keep(&g, 1.0, false);
        let wide = arrange(8, &g, 4.0, false, 1.0, &k).unwrap();
        assert_eq!(wide.lane, 54.0);
        assert!(
            wide.slots[..4]
                .iter()
                .all(|s| s.left && s.rect[1] == wide.slots[0].rect[1])
        );
        // One alone: on the left, its status line under it; two or more,
        // under the right side.
        let g = geo(1280.0, 720.0);
        let k = keep(&g, 1.0, false);
        let one = arrange(1, &g, 4.0, false, 1.0, &k).unwrap();
        assert!(one.slots[0].left);
        assert!(one.status[0] < g.left);
        let four = arrange(4, &g, 4.0, false, 1.0, &k).unwrap();
        assert!(four.status[0] > g.left);
        // The proportion where a window's shape gives the play's field
        // another: a phone held upright (its lanes as wide as its width
        // allows, sixteen lanes of height) twelve; a window twice as
        // tall as the play's least lanes fill, eight.
        let upright =
            FieldGeometry::placed(390.0 * 3.0, 844.0 * 3.0, 4.0, false, Default::default());
        assert_eq!(rows_for(&upright), MINI_ROWS_MAX);
        let short = geo(640.0, 180.0);
        assert_eq!(short.arrow, 24.0);
        assert_eq!(rows_for(&short), MINI_ROWS_MIN);
    }

    #[test]
    fn reverse_puts_the_miniatures_receptors_at_the_bottom() {
        let g = FieldGeometry::placed(1280.0, 720.0, 4.0, true, Default::default());
        let k = keep(&g, 1.0, false);
        let f = arrange(3, &g, 4.0, true, 1.0, &k).unwrap();
        for s in &f.slots {
            assert!(s.field.direction < 0.0);
            assert!(s.field.receptor_y > (s.panel[1] + s.panel[3]) / 2.0);
        }
    }

    #[test]
    fn a_phone_held_upright_keeps_everyone_in_its_margins() {
        // In CSS pixels at 3 target pixels each: the list this used to
        // fall back to is gone.
        let ratio = 3.0;
        let g = geo(390.0 * ratio, 844.0 * ratio);
        let k = keep(&g, ratio, false);
        for n in 1..=8 {
            let f = arrange(n, &g, 4.0, false, ratio, &k).unwrap();
            assert_eq!(f.slots.len(), n);
            assert!(!f.rotating);
            sound(&f, &g, &k, false, &format!("upright {n}"));
        }
        let f = arrange(8, &g, 4.0, false, ratio, &k).unwrap();
        assert!(f.lane >= 10.0, "{}", f.lane);
        // No side has the width for words: the status line beside the
        // watermark, on the field's far end.
        let [_, top, right, _] = ddi_render::scene::watermark_area(&g);
        assert!(
            f.status[0] >= right && f.status[1] >= top - 1.0,
            "{:?}",
            f.status
        );
        assert!(arrange(0, &g, 4.0, false, ratio, &[]).is_none());
        // Nothing at all only where a side cannot hold one (a window a
        // play is hardly drawn in).
        let g = geo(200.0, 600.0);
        assert!(arrange(8, &g, 4.0, false, 1.0, &keep(&g, 1.0, false)).is_none());
    }

    #[test]
    fn a_window_too_narrow_for_eight_shows_what_fits_in_turns() {
        // Under about 290 logical pixels wide, the margins are narrower
        // than the least miniature: none, or what fits in turns.
        let g = geo(290.0, 700.0);
        let k = keep(&g, 1.0, false);
        if let Some(f) = arrange(8, &g, 4.0, false, 1.0, &k) {
            assert!(f.slots.len() <= 8);
            assert_eq!(f.rotating, f.slots.len() < 8);
            sound(&f, &g, &k, false, "290x700");
        }
        // A field at a side: everyone on the other side.
        let g = FieldGeometry::placed(500.0, 800.0, 4.0, false, ddi_engine::FieldPlace::Left);
        let k = keep(&g, 1.0, false);
        if let Some(f) = arrange(4, &g, 4.0, false, 1.0, &k) {
            sound(&f, &g, &k, false, "at the left");
        }
    }

    #[test]
    fn out_stays_in_place_saying_so() {
        let g = geo(1280.0, 720.0);
        let f = arrange(3, &g, 4.0, false, 1.0, &keep(&g, 1.0, false)).unwrap();
        let c = shown(3);
        let mut list = DrawList::new();
        let flashes = vec![
            vec![None; 4],
            vec![
                Some((ddi_engine::rules::Judgement::W1, 0.1)),
                None,
                None,
                None,
            ],
            vec![None; 4],
        ];
        assert!(!draw_fields(
            &mut list,
            &c,
            &f,
            &flashes,
            &[],
            1.0,
            &mut FixedMeasure
        ));
        let t = texts(&list);
        // The second is out: its field says so; no level is ever text.
        assert_eq!(t.iter().filter(|(s, _)| s == "Out").count(), 1);
        assert!(t.iter().all(|(s, _)| !s.contains("Lv")));
        // Each one's level as stars: three silver (five rhombi and a
        // disc each) and two empty outlines (ten strokes each), dimmer
        // for the one that is out.
        let colors = ddi_theme::star_colors();
        let quads = |c: [f32; 4]| {
            list.commands
                .iter()
                .filter(|q| matches!(q, Command::Quad(q) if q.colors[0][..3] == c[..3]))
                .count()
        };
        assert_eq!(quads(colors[1]), 3 * 3 * 6);
        assert_eq!(quads(colors[2]), 3 * 2 * 10);
        assert_eq!(quads(colors[0]), 0);
        let dimmed = list
            .commands
            .iter()
            .filter(|q| matches!(q, Command::Quad(q) if q.colors[0][..3] == colors[1][..3] && q.colors[0][3] < 0.9))
            .count();
        assert_eq!(dimmed, 3 * 6);
        // A moment and the stage won in the status line.
        let mut c = shown(3);
        c.cue = Some(("Final stage".into(), 1.0));
        c.won = Some("Stage won".into());
        let mut list = DrawList::new();
        assert!(draw_fields(&mut list, &c, &f, &[], &[], 1.0, &mut FixedMeasure));
        let t = texts(&list);
        assert!(t.iter().any(|(s, _)| s == "Final stage"));
        assert!(t.iter().any(|(s, _)| s == "Stage won"));
    }

    /// The DDI ruleset's round 3: each miniature says its opponent's
    /// score, small under its life gauge, as the player's Score display
    /// has it (the score, the sum, or the percent),
    /// and shows a lane a shock locked: darkened to its end, its
    /// receptor's place tinted, never for one who is out.
    #[test]
    fn each_miniature_shows_its_score_and_its_locked_lanes() {
        let of = |score: f64, sum: Option<i64>| OpponentView {
            score,
            sum,
            ..shown(1).opponents[0].clone()
        };
        // The sum, never the proportional part; the percent floored.
        assert_eq!(ScoreShown::Sum.text(&of(0.987_659, Some(5_912_345))), "5912345");
        assert_eq!(ScoreShown::Percent.text(&of(0.987_659, Some(5_912_345))), "98.76%");
        assert_eq!(ScoreShown::Percent.text(&of(1.0, None)), "100.00%");
        assert_eq!(ScoreShown::Sum.text(&of(0.0, Some(0))), "0");
        // DDI's score may be negative.
        assert_eq!(ScoreShown::Sum.text(&of(-0.0123, Some(-13_530))), "-13530");
        assert_eq!(ScoreShown::Percent.text(&of(-0.0123, Some(-13_530))), "-1.23%");
        // A ruleset without a sum shows its percent.
        assert_eq!(ScoreShown::Sum.text(&of(0.5, None)), "50.00%");
        for (w, h, n) in [(1280.0, 720.0, 3), (1280.0, 720.0, 8), (390.0, 844.0, 8)] {
            let g = geo(w, h);
            let f = arrange(n, &g, 4.0, false, 1.0, &keep(&g, 1.0, false)).unwrap();
            let mut c = shown(n);
            let mut list = DrawList::new();
            draw_fields(&mut list, &c, &f, &[], &[], 1.0, &mut FixedMeasure);
            let scores: Vec<_> = texts(&list)
                .into_iter()
                .filter(|(s, _)| s == "5900000")
                .collect();
            assert_eq!(scores.len(), f.slots.len(), "{w}x{h}: one a miniature");
            // Small, and inside its field's band under the gauge.
            for cmd in &list.commands {
                let Command::Text(t) = cmd else { continue };
                if &*t.text != "5900000" {
                    continue;
                }
                assert!((6.0..=11.0).contains(&t.style.size()), "{}", t.style.size());
                let (x, y) = (t.transform.0[4], t.transform.0[5]);
                let slot = f
                    .slots
                    .iter()
                    .find(|s| {
                        x >= s.panel[0] - 1.0 && x <= s.panel[2] && y >= s.panel[1] && y <= s.panel[3]
                    })
                    .unwrap_or_else(|| panic!("{w}x{h}: a score outside its field at {x},{y}"));
                let gauge_end = slot.panel[1] + 30.0 * slot.field.arrow / 72.0;
                assert!(y >= gauge_end && y <= slot.field.receptor_y, "{w}x{h}: {y}");
            }
            c.score = ScoreShown::Percent;
            let mut list = DrawList::new();
            draw_fields(&mut list, &c, &f, &[], &[], 1.0, &mut FixedMeasure);
            assert_eq!(
                texts(&list).iter().filter(|(s, _)| s == "90.00%").count(),
                f.slots.len()
            );
            // The first's third lane locked, and the second's first (who
            // is out: nothing shown of it).
            let mut locks = vec![vec![None; 4]; f.slots.len()];
            locks[0][2] = Some(0.3);
            locks[1][0] = Some(0.5);
            let mut list = DrawList::new();
            draw_fields(&mut list, &c, &f, &[], &locks, 1.0, &mut FixedMeasure);
            let of = |color: [f32; 4]| -> Vec<[f32; 4]> {
                list.commands
                    .iter()
                    .filter_map(|q| match q {
                        Command::Quad(q) if q.colors[0] == color => Some(q.rect),
                        _ => None,
                    })
                    .collect()
            };
            let (lane, tint) = (of(LOCK_LANE), of(LOCK_TINT));
            assert_eq!((lane.len(), tint.len()), (1, 1), "{w}x{h}");
            let slot = &f.slots[0];
            let cx = slot.field.lane_x(2.0);
            let half = slot.field.arrow / 2.0;
            assert_eq!(lane[0], [cx - half, slot.panel[1], cx + half, slot.panel[3]]);
            let cy = slot.field.receptor_y;
            assert_eq!(tint[0], [cx - half, cy - half, cx + half, cy + half]);
        }
    }

    #[test]
    fn far_more_opponents_than_room_take_turns_in_the_slots() {
        for (w, h) in [(1280.0, 720.0), (1920.0, 1080.0)] {
            let g = geo(w, h);
            let k = keep(&g, 1.0, false);
            let mut last = 0;
            for n in [16, 32, 64, 128] {
                let mut f = arrange(n, &g, 4.0, false, 1.0, &k).unwrap();
                assert!(f.slots.len() <= n, "{w}x{h} {n}");
                assert_eq!(f.rotating, f.slots.len() < n);
                // Beyond the cap, never under the comfortable least.
                assert!(f.lane >= MINI_LANE_CROWD);
                sound(&f, &g, &k, false, &format!("{w}x{h} {n}"));
                assert!(f.status[1] + f.status[3] <= h);
                if f.rotating {
                    assert!(f.slots.len() >= last, "{n}: as many as fit");
                    // Everyone has a turn, every slot always filled, no
                    // seat twice at once.
                    let mut seen = std::collections::BTreeSet::new();
                    let turns = n.div_ceil(f.slots.len());
                    let mut place = std::collections::BTreeMap::new();
                    for turn in 0..turns {
                        let t = f64::from(ROTATION_V0.every) * (turn as f64 + 0.5);
                        rotate(&mut f, n, &[], t, ROTATION_V0, false);
                        let seats: std::collections::BTreeSet<_> =
                            f.slots.iter().map(|s| s.seat).collect();
                        assert_eq!(seats.len(), f.slots.len());
                        seen.extend(seats);
                        // An opponent always in the same slot.
                        for (i, s) in f.slots.iter().enumerate() {
                            assert_eq!(*place.entry(s.seat).or_insert(i), i, "{n}");
                        }
                    }
                    assert_eq!(seen.len(), n, "{w}x{h} {n}");
                }
                last = f.slots.len();
            }
            // The development-only practice's 128 take turns at both.
            assert!(arrange(128, &g, 4.0, false, 1.0, &k).unwrap().rotating);
        }
    }

    #[test]
    fn a_large_field_has_a_room_of_nines_slots_and_the_ladder_under_them() {
        // Past eight opponents: the eight slots a room of nine has, no
        // smaller than its (the left side a status line's height shorter
        // for the ladder), and the ladder under the left side's.
        for (w, h) in [(1280.0, 720.0), (1920.0, 1080.0)] {
            let g = geo(w, h);
            let k = keep(&g, 1.0, false);
            let nine = arrange(8, &g, 4.0, false, 1.0, &k).unwrap();
            assert!(nine.ladder.is_none(), "a room of nine has none");
            for n in [9, 32, 72, 128] {
                let f = arrange(n, &g, 4.0, false, 1.0, &k).unwrap();
                assert_eq!(f.slots.len(), 8, "{w}x{h} {n}");
                assert!(f.rotating);
                assert!(f.lane >= nine.lane - 4.0, "{} {}", f.lane, nine.lane);
                let [lx, ly, lw, lh] = f.ladder.expect("the ladder");
                let left: Vec<_> = f.slots.iter().filter(|s| s.left).collect();
                let bottom = left.iter().map(|s| s.rect[1] + s.rect[3]).fold(0.0, f32::max);
                assert!(ly >= bottom && ly + lh <= h, "{ly} {bottom}");
                let right_edge = left.iter().map(|s| s.rect[0] + s.rect[2]).fold(0.0, f32::max);
                assert!(lx >= 0.0 && lx + lw <= right_edge + 0.5 && lw > 100.0);
                // Clear of the field and of what the slots keep clear of.
                for k in &k {
                    let apart = lx + lw <= k[0] || lx >= k[2] || ly + lh <= k[1] || ly >= k[3];
                    assert!(apart, "{w}x{h}: the ladder {:?} over {k:?}", [lx, ly, lw, lh]);
                }
            }
        }
    }

    #[test]
    fn pinned_seats_stay_while_the_rest_turn() {
        let g = geo(1280.0, 720.0);
        let k = keep(&g, 1.0, false);
        let mut f = arrange(128, &g, 4.0, false, 1.0, &k).unwrap();
        assert!(f.rotating);
        let every = f64::from(ROTATION_V0.every);
        // A stub's priority: seats 40 and 7 (later: the attacker and the
        // attacked), always in the first slots.
        let pinned = [40, 7];
        let mut windows = Vec::new();
        for turn in 0..4 {
            rotate(
                &mut f,
                128,
                &pinned,
                every * (turn as f64 + 0.5),
                ROTATION_V0,
                false,
            );
            assert_eq!(f.slots[0].seat, 40);
            assert_eq!(f.slots[1].seat, 7);
            assert!(f.slots[2..].iter().all(|s| s.seat != 40 && s.seat != 7));
            windows.push(f.slots[2].seat);
        }
        assert!(windows.windows(2).all(|w| w[0] != w[1]), "{windows:?}");
        // A change darkens the turning slots, never the pinned ones, and
        // not at all under reduced motion; between changes, nothing.
        rotate(&mut f, 128, &pinned, every, ROTATION_V0, false);
        assert_eq!(f.slots[0].dim, 0.0);
        assert!(f.slots[2].dim > 0.9);
        rotate(&mut f, 128, &pinned, every, ROTATION_V0, true);
        assert!(f.slots.iter().all(|s| s.dim == 0.0));
        rotate(&mut f, 128, &pinned, every * 1.5, ROTATION_V0, false);
        assert!(f.slots.iter().all(|s| s.dim == 0.0));
        // The first turn starts without a change.
        rotate(&mut f, 128, &pinned, 0.0, ROTATION_V0, false);
        assert!(f.slots.iter().all(|s| s.dim == 0.0));
        // All fitting: seat order, pins ignored, never moving.
        let mut eight = arrange(8, &g, 4.0, false, 1.0, &k).unwrap();
        rotate(&mut eight, 8, &pinned, every * 3.0, ROTATION_V0, false);
        assert!(eight.slots.iter().enumerate().all(|(i, s)| s.seat == i));
    }

    /// The stars' quads of a draw list: their bounds, `[left, top, right,
    /// bottom]`, over every quad in a star color.
    fn stars_bounds(list: &DrawList) -> Option<[f32; 4]> {
        let colors = ddi_theme::star_colors();
        let mut b: Option<[f32; 4]> = None;
        for c in &list.commands {
            let Command::Quad(q) = c else { continue };
            if !colors.iter().any(|c| c[..3] == q.colors[0][..3]) {
                continue;
            }
            for corner in [
                [q.rect[0], q.rect[1]],
                [q.rect[2], q.rect[1]],
                [q.rect[2], q.rect[3]],
                [q.rect[0], q.rect[3]],
            ] {
                let [x, y] = q.transform.apply(corner);
                let e = b.get_or_insert([x, y, x, y]);
                *e = [e[0].min(x), e[1].min(y), e[2].max(x), e[3].max(y)];
            }
        }
        b
    }

    #[test]
    fn the_level_is_stars_beside_the_name_under_it_or_left_out() {
        // Wide slots: at the name line's end. Narrower: a line of their
        // own. The very smallest: the name alone. Always inside the slot,
        // above its field.
        for (w, h, ratio, n, want) in [
            (1920.0, 1080.0, 1.0, 8, "beside"),
            (2560.0, 1080.0, 1.0, 8, "beside"),
            (1280.0, 720.0, 1.0, 8, "under"),
            (844.0, 390.0, 3.0, 8, "under"),
            (390.0, 844.0, 3.0, 8, "under"),
            (320.0, 568.0, 2.0, 8, "none"),
        ] {
            let g = geo(w * ratio, h * ratio);
            let f = arrange(n, &g, 4.0, false, ratio, &keep(&g, ratio, false)).unwrap();
            let got = match f.stars {
                Stars::Beside(_) => "beside",
                Stars::Under(_) => "under",
                Stars::None => "none",
            };
            assert_eq!(got, want, "{w}x{h} lane {}", f.lane);
            let mut one = shown(n);
            one.opponents.truncate(1);
            let mut list = DrawList::new();
            draw_fields(&mut list, &one, &f, &[], &[], ratio, &mut FixedMeasure);
            let slot = f.slots[0];
            match stars_bounds(&list) {
                Some(b) => {
                    assert_ne!(want, "none");
                    assert!(b[0] >= slot.rect[0] - 0.5, "{w}x{h} {b:?} {slot:?}");
                    assert!(
                        b[2] <= slot.rect[0] + slot.rect[2] + 0.5,
                        "{w}x{h} {b:?} {slot:?}"
                    );
                    assert!(b[1] >= slot.rect[1] - 0.5, "{w}x{h} {b:?}");
                    assert!(b[3] <= slot.panel[1] + 0.5, "{w}x{h} {b:?} over its field");
                }
                None => assert_eq!(want, "none"),
            }
        }
        // A person has no level: no stars.
        let g = geo(1280.0, 720.0);
        let f = arrange(2, &g, 4.0, false, 1.0, &keep(&g, 1.0, false)).unwrap();
        let mut people = shown(2);
        for o in &mut people.opponents {
            o.level = None;
        }
        let mut list = DrawList::new();
        draw_fields(&mut list, &people, &f, &[], &[], 1.0, &mut FixedMeasure);
        assert!(stars_bounds(&list).is_none());
    }

    #[test]
    fn names_are_cut_to_their_slots_at_the_smallest() {
        let ratio = 3.0;
        let g = geo(390.0 * ratio, 844.0 * ratio);
        let f = arrange(8, &g, 4.0, false, ratio, &keep(&g, ratio, false)).unwrap();
        let mut c = shown(8);
        c.opponents[0].name = "Velvet Metronome".into();
        let mut list = DrawList::new();
        draw_fields(&mut list, &c, &f, &[], &[], ratio, &mut FixedMeasure);
        // Every text inside its slot's width.
        for cmd in &list.commands {
            if let Command::Text(t) = cmd {
                let w = FixedMeasure.measure(&t.text, &t.style).advance;
                let x = t.transform.apply([0.0, 0.0])[0];
                assert!(
                    f.slots
                        .iter()
                        .any(|s| x >= s.rect[0] - 0.5 && x + w <= s.rect[0] + s.rect[2] + 0.5),
                    "{:?} at {x} wide {w}",
                    t.text
                );
            }
        }
        assert!(texts(&list).iter().any(|(s, _)| s.ends_with('…')));
    }
}
