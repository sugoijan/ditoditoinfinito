//! Touch controls for a play (`docs/plans/touch-controls.md`): where a
//! touch screen's buttons or pad are, from the screen's size, its safe
//! area, the layout's lanes and the player's choices, and which lane a
//! finger at a point presses. Pure: both shells place the controls with
//! it, the play's view draws them over the field (translucent, the field
//! unchanged) and the prompt's diagram shows them.
//!
//! Three styles ([`Style`]):
//! - **Buttons**: the lanes in the receptors' order, cut in two groups at
//!   the bottom left and right, each under a thumb; an odd layout's extra
//!   button on the right (or the left, [`Hand::Left`]). Offered up to
//!   [`BUTTONS_MOST`] lanes: past it the layout plays the pad.
//! - **Pad**: a 3×3 pad whose tiles are the dance mat's
//!   (`ddi_dance_mat::MatLayout`): each lane on the tile the mat gives it
//!   (Single: left 4, down 2, up 8, right 6, keypad numbering), the other
//!   tiles plain; two pads side by side for a layout on two mats.
//! - **Columns**: the screen cut in full-height columns at the receptors'
//!   midpoints (the first touch lanes, kept as the legacy style): no plan
//!   here, the shells keep their own.
//!
//! Every length is in CSS (logical) pixels, the origin the screen's top
//! left. A slide keeps its lane until the finger is well inside another
//! lane's target ([`DEAD_BAND`]): the gaps, the pad's plain tiles and the
//! screen beyond the controls press nothing new.

use ddi_chart::layout::{Glyph, Layout};
use ddi_dance_mat::MatLayout;
use serde::{Deserialize, Serialize};

pub mod glyphs;

/// The touch controls' style (Options → Controls, "Touch controls").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Style {
    /// Buttons under the thumbs, in two groups.
    #[default]
    Buttons,
    /// A 3×3 pad, the dance mat's tiles.
    Pad,
    /// The play screen's full-height columns (legacy).
    Columns,
}

/// Which side an odd layout's extra button goes to (and a lone pad's
/// side beside the field on a wide screen).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hand {
    #[default]
    Right,
    Left,
}

/// The most lanes Buttons plays (the maintainer, 2026-11-20: for eight
/// to ten lanes "we can defer it and only support a pad").
pub const BUTTONS_MOST: usize = 7;

/// The narrowest button drawn (CSS pixels: about 7 mm on a phone, the
/// platforms' smallest touch target, 44 points): narrower, a layout the
/// mat has plays the pad on that screen instead (a phone upright for
/// seven lanes).
pub const BUTTON_LEAST: f32 = 44.0;

/// How far into another target a sliding finger goes before it presses
/// that lane instead (CSS pixels).
pub const DEAD_BAND: f32 = 8.0;

/// How far beyond its group a finger going down still presses the
/// nearest target (a thumb lands a little low or wide).
pub const REACH: f32 = 14.0;

/// From the screen's safe area to the controls.
pub const MARGIN: f32 = 10.0;
/// Between two buttons, or two tiles.
pub const GAP: f32 = 8.0;
/// Between the two groups, at least.
pub const MIDDLE: f32 = 16.0;

/// Millimeters a CSS pixel measures on a device class, for the sizes'
/// arithmetic (`docs/plans/touch-controls.md`, "Sizes"): a phone's
/// (about 153 CSS pixels an inch: a 460 ppi panel at 3×), a tablet's
/// (132: 264 ppi at 2×), a desktop touch screen's (96 a nominal inch).
pub const MM_PER_PX_PHONE: f32 = 25.4 / 153.0;
pub const MM_PER_PX_TABLET: f32 = 25.4 / 132.0;
pub const MM_PER_PX_DESKTOP: f32 = 25.4 / 96.0;

/// The player's choices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Choice {
    pub style: Style,
    pub hand: Hand,
}

/// The screen the controls are placed on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    /// Width and height.
    pub size: [f32; 2],
    /// The safe area's insets: top, right, bottom, left.
    pub insets: [f32; 4],
    /// The field's left and right edges (where a lone pad may sit beside
    /// it); `None`: centered, four lanes of a tenth of the height.
    pub field: Option<[f32; 2]>,
}

impl Screen {
    pub fn new(size: [f32; 2], insets: [f32; 4]) -> Screen {
        Screen {
            size,
            insets,
            field: None,
        }
    }

    pub fn with_field(self, field: [f32; 2]) -> Screen {
        Screen {
            field: Some(field),
            ..self
        }
    }

    fn safe(&self) -> Rect {
        let [w, h] = self.size;
        let [t, r, b, l] = self.insets.map(|v| v.max(0.0));
        [l + MARGIN, t + MARGIN, w - r - MARGIN, h - b - MARGIN]
    }
}

/// A rectangle: left, top, right, bottom.
pub type Rect = [f32; 4];

/// What a target is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A button of a group (0 the left, 1 the right).
    Button { group: u8 },
    /// A pad's tile: the pad (0 the left or only, 1 the right) and its
    /// cell, keypad numbering (7 8 9 the top row).
    Tile { pad: u8, cell: u8 },
}

/// One button or tile.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// The lane it presses; `None`: a pad's plain tile.
    pub lane: Option<usize>,
    pub rect: Rect,
    pub kind: Kind,
}

impl Target {
    pub fn center(&self) -> [f32; 2] {
        let [l, t, r, b] = self.rect;
        [(l + r) / 2.0, (t + b) / 2.0]
    }

    pub fn size(&self) -> [f32; 2] {
        let [l, t, r, b] = self.rect;
        [r - l, b - t]
    }
}

/// The controls of a play on a screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Controls {
    /// The style drawn (Buttons gives way to Pad past [`BUTTONS_MOST`]).
    pub style: Style,
    pub targets: Vec<Target>,
    /// Each group's or pad's bounds.
    pub groups: Vec<Rect>,
}

/// The style a layout plays under `style`: Buttons up to [`BUTTONS_MOST`]
/// lanes on one row (and on a screen where they are at least
/// [`BUTTON_LEAST`] wide, [`plan`]), else the pad; the pad where the mat has the layout,
/// else Buttons where it may, else the columns.
pub fn effective(style: Style, layout: &Layout) -> Style {
    let mat = MatLayout::from_id(&layout.id).filter(|m| m.lane_count() == layout.lanes.len());
    let one_row = layout.lanes.iter().all(|l| l.scroll_sign >= 0)
        || layout.lanes.iter().all(|l| l.scroll_sign < 0);
    let buttons = one_row && !layout.lanes.is_empty() && layout.lanes.len() <= BUTTONS_MOST;
    match style {
        Style::Columns => Style::Columns,
        Style::Buttons if buttons => Style::Buttons,
        Style::Buttons | Style::Pad if mat.is_some() => Style::Pad,
        Style::Pad if buttons => Style::Buttons,
        _ => Style::Columns,
    }
}

/// The controls for `layout` on `screen` under `choice`; `None` for the
/// columns (the shells' own).
pub fn plan(layout: &Layout, screen: Screen, choice: Choice) -> Option<Controls> {
    match effective(choice.style, layout) {
        Style::Buttons => {
            let b = buttons(layout, screen, choice.hand);
            if b.smallest() < BUTTON_LEAST
                && let Some(p) = pad(layout, screen, choice.hand)
            {
                return Some(p);
            }
            Some(b)
        }
        Style::Pad => pad(layout, screen, choice.hand),
        Style::Columns => None,
    }
}

/// The lanes in the receptors' order across the field.
pub fn receptor_order(layout: &Layout) -> Vec<usize> {
    let mut order: Vec<usize> = (0..layout.lanes.len()).collect();
    order.sort_by(|a, b| layout.lanes[*a].column.total_cmp(&layout.lanes[*b].column));
    order
}

/// How many of `n` lanes go to the left group.
pub fn left_count(n: usize, hand: Hand) -> usize {
    match hand {
        Hand::Right => n / 2,
        Hand::Left => n.div_ceil(2),
    }
}

/// A button's width and height for `k` buttons a side.
fn button_size(screen: &Screen, k: usize) -> [f32; 2] {
    let [l, _, r, _] = screen.safe();
    let side = ((r - l) - MIDDLE) / 2.0;
    let k = k.max(1) as f32;
    let short = screen.size[0].min(screen.size[1]);
    // Wide enough for a thumb, no wider than a thumb wants.
    let most = (short * 0.29).clamp(72.0, 120.0);
    let w = ((side - (k - 1.0) * GAP) / k).min(most).max(1.0);
    let h = (short * 0.26).clamp(64.0, 120.0).max(w.min(120.0));
    [w, h]
}

fn buttons(layout: &Layout, screen: Screen, hand: Hand) -> Controls {
    let order = receptor_order(layout);
    let n = order.len();
    let left = left_count(n, hand);
    let k = left.max(n - left);
    let [w, h] = button_size(&screen, k);
    let [sl, _, sr, sb] = screen.safe();
    let top = sb - h;
    let mut targets = Vec::with_capacity(n);
    let mut groups = Vec::new();
    // The left group from the left edge, the right one to the right edge.
    if left > 0 {
        for (i, &lane) in order[..left].iter().enumerate() {
            let x = sl + i as f32 * (w + GAP);
            targets.push(Target {
                lane: Some(lane),
                rect: [x, top, x + w, sb],
                kind: Kind::Button { group: 0 },
            });
        }
        groups.push([sl, top, sl + left as f32 * (w + GAP) - GAP, sb]);
    }
    let right = n - left;
    if right > 0 {
        let start = sr - right as f32 * (w + GAP) + GAP;
        for (i, &lane) in order[left..].iter().enumerate() {
            let x = start + i as f32 * (w + GAP);
            targets.push(Target {
                lane: Some(lane),
                rect: [x, top, x + w, sb],
                kind: Kind::Button { group: 1 },
            });
        }
        groups.push([start, top, sr, sb]);
    }
    Controls {
        style: Style::Buttons,
        targets,
        groups,
    }
}

/// The smallest pad drawn beside the field rather than under it.
const PAD_BESIDE_LEAST: f32 = 3.0 * BUTTON_LEAST + 2.0 * GAP;
/// What a pad beside the field at the screen's left leaves above it: the
/// quit button and the volume over it (each 44 px, gaps of 8).
pub const QUIT_ROOM: f32 = 2.0 * 44.0 + 3.0 * GAP;
/// The most of the screen's height a pad under the field takes.
const PAD_TALL: f32 = 0.42;
/// A pad's largest side (tiles of about 20 mm on a phone).
const PAD_MOST: f32 = 3.0 * 120.0 + 2.0 * GAP;

fn pad(layout: &Layout, screen: Screen, hand: Hand) -> Option<Controls> {
    let mat = MatLayout::from_id(&layout.id)?;
    let pads = mat.pads();
    let [sl, st, sr, sb] = screen.safe();
    let [w, h] = screen.size;
    let field = screen.field.unwrap_or_else(|| {
        let arrow = (h / 10.0).min(w / (field_span(layout) + 2.0));
        let fw = arrow * field_span(layout);
        [(w - fw) / 2.0, (w + fw) / 2.0]
    });
    let tall = (sb - st).max(0.0);
    // Each pad's square and its place.
    let mut squares: Vec<Rect> = Vec::new();
    let beside_left = field[0] - MARGIN - sl;
    let beside_right = sr - (field[1] + MARGIN);
    if pads == 1 {
        let room = match hand {
            Hand::Right => beside_right,
            Hand::Left => beside_left,
        };
        // A pad at the left leaves the quit button and the volume their
        // room above it.
        let high = match hand {
            Hand::Right => tall,
            Hand::Left => tall - QUIT_ROOM,
        };
        let side = room.min(high).min(PAD_MOST);
        if w > h && side >= PAD_BESIDE_LEAST {
            let x = match hand {
                Hand::Right => sr - side,
                Hand::Left => sl,
            };
            squares.push([x, sb - side, x + side, sb]);
        } else {
            // Under the field, centered: as wide as the screen lets it,
            // its top under the screen's middle (the judgment and the
            // combo stay clear).
            let side = (sr - sl).min(tall * PAD_TALL).clamp(1.0, PAD_MOST);
            let x = (sl + sr - side) / 2.0;
            squares.push([x, sb - side, x + side, sb]);
        }
    } else {
        let side = beside_left
            .min(beside_right)
            .min(tall - QUIT_ROOM)
            .min(PAD_MOST);
        if w > h && side >= PAD_BESIDE_LEAST {
            squares.push([sl, sb - side, sl + side, sb]);
            squares.push([sr - side, sb - side, sr, sb]);
        } else {
            // Side by side at the bottom, each in its half.
            let side = (((sr - sl) - MIDDLE) / 2.0)
                .min(tall * PAD_TALL)
                .clamp(1.0, PAD_MOST);
            squares.push([sl, sb - side, sl + side, sb]);
            squares.push([sr - side, sb - side, sr, sb]);
        }
    }
    let mut targets = Vec::new();
    for (p, sq) in squares.iter().enumerate() {
        let tile = ((sq[2] - sq[0]) - 2.0 * GAP) / 3.0;
        for cell in 1..=9u8 {
            let col = f32::from((cell - 1) % 3);
            let row = f32::from(2 - (cell - 1) / 3);
            let x = sq[0] + col * (tile + GAP);
            let y = sq[1] + row * (tile + GAP);
            let id = ddi_dance_mat::Tile::new(p as u8, cell)?;
            targets.push(Target {
                lane: mat.lane_for_tile(id),
                rect: [x, y, x + tile, y + tile],
                kind: Kind::Tile { pad: p as u8, cell },
            });
        }
    }
    Some(Controls {
        style: Style::Pad,
        targets,
        groups: squares,
    })
}

/// A layout's field width in lanes (`ddi_render::scene::field_span`).
fn field_span(layout: &Layout) -> f32 {
    layout
        .lanes
        .iter()
        .map(|l| l.column)
        .fold(0.0f32, f32::max)
        + 1.0
}

fn inside([l, t, r, b]: Rect, [x, y]: [f32; 2], by: f32) -> bool {
    x >= l - by && x <= r + by && y >= t - by && y <= b + by
}

fn distance([l, t, r, b]: Rect, [x, y]: [f32; 2]) -> f32 {
    let dx = (l - x).max(x - r).max(0.0);
    let dy = (t - y).max(y - b).max(0.0);
    dx.hypot(dy)
}

impl Controls {
    /// The lane a finger at `at` presses. Going down (`held` none): the
    /// lane whose target is nearest, within half a gap of it, or within
    /// [`REACH`] beyond the group's edge; a plain tile presses nothing.
    /// Sliding with `held` down: another lane only once the finger is
    /// [`DEAD_BAND`] inside its target; anywhere else it keeps `held`.
    pub fn lane_at(&self, at: [f32; 2], held: Option<usize>) -> Option<usize> {
        match held {
            Some(held) => Some(
                self.targets
                    .iter()
                    .filter(|t| t.lane.is_some_and(|l| l != held))
                    .find(|t| inside(t.rect, at, -DEAD_BAND))
                    .and_then(|t| t.lane)
                    .unwrap_or(held),
            ),
            None => {
                let reach = |t: &Target| {
                    let grown = GAP / 2.0;
                    if inside(t.rect, at, grown) {
                        return true;
                    }
                    // Past the group's outer edges, a little more (never
                    // between two groups).
                    let between = self.groups.iter().any(|a| {
                        self.groups
                            .iter()
                            .any(|b| a[2] < at[0] && at[0] < b[0])
                    });
                    !between
                        && inside(t.rect, at, REACH)
                        && self.groups.iter().all(|g| !inside(*g, at, grown))
                };
                let near = self
                    .targets
                    .iter()
                    .filter(|t| reach(t))
                    .min_by(|a, b| distance(a.rect, at).total_cmp(&distance(b.rect, at)))?;
                near.lane
            }
        }
    }

    /// Whether `at` is on the controls (a group, or a little beyond).
    pub fn holds(&self, at: [f32; 2]) -> bool {
        self.groups.iter().any(|g| inside(*g, at, REACH))
    }

    /// Each lane's targets' smallest side, for the sizes' report.
    pub fn smallest(&self) -> f32 {
        self.targets
            .iter()
            .filter(|t| t.lane.is_some())
            .map(|t| {
                let [w, h] = t.size();
                w.min(h)
            })
            .fold(f32::INFINITY, f32::min)
    }

    /// The controls' top: what of the screen's height they cover from it
    /// down.
    pub fn top(&self) -> f32 {
        self.groups.iter().map(|g| g[1]).fold(f32::INFINITY, f32::min)
    }

    /// The first group on the screen's left half (`width` wide), if any.
    pub fn left_group(&self, width: f32) -> Option<Rect> {
        self.groups.iter().copied().find(|g| g[0] < width / 2.0)
    }
}

/// The glyph a lane's target shows (the receptor's).
pub fn glyph(layout: &Layout, lane: usize) -> Option<&Glyph> {
    layout.lanes.get(lane).map(|l| &l.glyph)
}

#[cfg(test)]
mod tests;
