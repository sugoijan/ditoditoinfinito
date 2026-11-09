// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Physical millimeters, before any hand-root or render transform. Layout
//! mapping alone is not a certificate of hand reach or gameplay support.
use serde::{Deserialize, Serialize};

/// Stable physical identity: 1..9 on the left/single mat, 10..18 on the right.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Tile(u8);
impl Tile {
    pub const fn new(pad: u8, cell: u8) -> Option<Self> {
        if pad < 2 && cell >= 1 && cell <= 9 {
            Some(Self(pad * 9 + cell))
        } else {
            None
        }
    }
    pub const fn id(self) -> u8 {
        self.0
    }
    pub const fn pad(self) -> u8 {
        (self.0 - 1) / 9
    }
    pub const fn cell(self) -> u8 {
        (self.0 - 1) % 9 + 1
    }
}
impl TryFrom<u8> for Tile {
    type Error = &'static str;
    fn try_from(id: u8) -> Result<Self, Self::Error> {
        if (1..=18).contains(&id) {
            Ok(Self(id))
        } else {
            Err("invalid physical tile")
        }
    }
}
impl From<Tile> for u8 {
    fn from(t: Tile) -> Self {
        t.id()
    }
}
impl std::fmt::Display for Tile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            if self.pad() == 0 { 'L' } else { 'R' },
            self.cell()
        )
    }
}
/// Keypad coordinates. Keep the original single-mat arithmetic, including zeros.
pub fn cell_position(cell: u8, pitch: f64) -> Option<[f64; 2]> {
    Some(match cell {
        1 => [-pitch, -pitch],
        2 => [0., -pitch],
        3 => [pitch, -pitch],
        4 => [-pitch, 0.],
        5 => [0., 0.],
        6 => [pitch, 0.],
        7 => [-pitch, pitch],
        8 => [0., pitch],
        9 => [pitch, pitch],
        _ => return None,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "WireGeometry", into = "WireGeometry")]
pub struct Geometry {
    pads: u8,
    pitch_mm: f64,
    seam_gap_mm: f64,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireGeometry {
    version: u8,
    pads: u8,
    pitch_mm: f64,
    seam_gap_mm: f64,
}
impl TryFrom<WireGeometry> for Geometry {
    type Error = &'static str;
    fn try_from(w: WireGeometry) -> Result<Self, Self::Error> {
        if w.version != 1 {
            return Err("unknown mat geometry version");
        }
        Self::new(w.pads, w.pitch_mm, w.seam_gap_mm)
    }
}
impl From<Geometry> for WireGeometry {
    fn from(g: Geometry) -> Self {
        Self {
            version: 1,
            pads: g.pads,
            pitch_mm: g.pitch_mm,
            seam_gap_mm: g.seam_gap_mm,
        }
    }
}
impl Geometry {
    pub fn new(pads: u8, pitch_mm: f64, seam_gap_mm: f64) -> Result<Self, &'static str> {
        if !(1..=2).contains(&pads)
            || !pitch_mm.is_finite()
            || pitch_mm <= 0.
            || !seam_gap_mm.is_finite()
            || seam_gap_mm < 0.
            || (pads == 1 && seam_gap_mm != 0.)
        {
            return Err("invalid mat geometry");
        }
        if !(3. * pitch_mm + seam_gap_mm).is_finite() {
            return Err("mat extent overflow");
        }
        Ok(Self {
            pads,
            pitch_mm,
            seam_gap_mm,
        })
    }
    pub fn single() -> Self {
        Self {
            pads: 1,
            pitch_mm: 24.,
            seam_gap_mm: 0.,
        }
    }
    pub fn pads(self) -> u8 {
        self.pads
    }
    pub fn pitch_mm(self) -> f64 {
        self.pitch_mm
    }
    pub fn seam_gap_mm(self) -> f64 {
        self.seam_gap_mm
    }
    pub fn tiles(self) -> impl Iterator<Item = Tile> {
        (1..=self.pads * 9).map(Tile)
    }
    pub fn pad_offset_mm(self, pad: u8) -> Option<f64> {
        if pad >= self.pads {
            return None;
        }
        Some(if self.pads == 1 {
            0.
        } else {
            (3. * self.pitch_mm + self.seam_gap_mm) * if pad == 0 { -0.5 } else { 0.5 }
        })
    }
    pub fn position(self, tile: Tile) -> Option<[f64; 2]> {
        let p = cell_position(tile.cell(), self.pitch_mm)?;
        let offset = self.pad_offset_mm(tile.pad())?;
        // Preserve the single-pad coordinates bit for bit.
        Some(if self.pads == 1 {
            p
        } else {
            [p[0] + offset, p[1]]
        })
    }
    pub fn tile_half_mm(self) -> f64 {
        self.pitch_mm * (23. / 48.)
    }
    /// Full nominal pad footprint, including gaps around the tile surfaces.
    pub fn bounds_mm(self) -> [[f64; 2]; 2] {
        let x = self.pads as f64 * 1.5 * self.pitch_mm + self.seam_gap_mm * 0.5;
        let y = 1.5 * self.pitch_mm;
        [[-x, -y], [x, y]]
    }
}

/// Mapping in chart-lane order, after gameplay transforms. Oni in 9A is a
/// target set; callers must choose a physical center before publishing contact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MatLayout {
    index: usize,
}
type Entry = (&'static str, u8, &'static [u8], Option<&'static [u16]>);
/// Each layout: its id, its pads, each lane's tile in chart order (9A's Oni
/// at its left center; [`MatLayout::targets`] has both) and its logical No
/// Stretch lane-bit masks, unchanged from the accepted policy. Source:
/// StepMania NoteDataUtil::g_ValidRows at 825467b (dance/Pump); half-double
/// is its middle-six region; 9A adds Oni to each region.
const LAYOUTS: [Entry; 11] = [
    ("dance-single", 1, &[4, 2, 8, 6], None),
    ("dance-solo", 1, &[4, 7, 2, 8, 9, 6], None),
    ("danoni-5", 1, &[4, 2, 8, 6, 5], None),
    ("danoni-7", 1, &[4, 1, 2, 5, 8, 9, 6], None),
    ("danoni-9B", 1, &[7, 4, 1, 2, 5, 8, 9, 6, 3], None),
    ("pump-single", 1, &[1, 7, 5, 9, 3], None),
    ("techno-single8", 1, &[1, 4, 7, 2, 8, 9, 6, 3], None),
    (
        "dance-double",
        2,
        &[4, 2, 8, 6, 13, 11, 17, 15],
        Some(&[0x00f, 0x01e, 0x078, 0x0f0]),
    ),
    ("pump-halfdouble", 2, &[5, 9, 3, 10, 16, 14], Some(&[0x03f])),
    (
        "pump-double",
        2,
        &[1, 7, 5, 9, 3, 10, 16, 14, 18, 12],
        Some(&[0x01f, 0x0fc, 0x3e0]),
    ),
    (
        "danoni-9A",
        2,
        &[4, 2, 8, 6, 5, 13, 11, 17, 15],
        Some(&[0x01f, 0x03e, 0x0f8, 0x1f0]),
    ),
];
/// The layout whose Oni (lane 4) has a center on each pad.
const NINE_A: usize = 10;
const TARGETS: [[Tile; 1]; 18] = {
    let mut a = [[Tile(1)]; 18];
    let mut i = 0;
    while i < 18 {
        a[i] = [Tile(i as u8 + 1)];
        i += 1;
    }
    a
};
impl MatLayout {
    pub fn from_id(id: &str) -> Option<Self> {
        LAYOUTS
            .iter()
            .position(|l| l.0 == id)
            .map(|index| Self { index })
    }
    pub fn id(self) -> &'static str {
        LAYOUTS[self.index].0
    }
    pub fn pads(self) -> u8 {
        LAYOUTS[self.index].1
    }
    pub fn lane_count(self) -> usize {
        self.panels().len()
    }
    /// Each lane's canonical tile in chart order. 9A's Oni is its left
    /// center here; [`Self::targets`] gives both.
    pub fn panels(self) -> &'static [u8] {
        LAYOUTS[self.index].2
    }
    /// Lane IDs when every lane has exactly one physical target. 9A's Oni
    /// deliberately has no such flat mapping; callers must select its center.
    pub fn fixed_panels(self) -> Option<&'static [u8]> {
        (self.index != NINE_A).then_some(self.panels())
    }
    pub fn single_panels(self) -> Option<&'static [u8]> {
        (self.pads() == 1).then_some(self.panels())
    }
    pub fn targets(self, lane: usize) -> &'static [Tile] {
        if self.index == NINE_A && lane == 4 {
            return &[Tile(5), Tile(14)];
        }
        self.panels()
            .get(lane)
            .map_or(&[], |id| &TARGETS[*id as usize - 1])
    }
    pub fn lane_for_tile(self, tile: Tile) -> Option<usize> {
        (0..self.lane_count()).find(|&lane| self.targets(lane).contains(&tile))
    }
    pub fn unscored_tiles(self) -> impl Iterator<Item = Tile> {
        (1..=self.pads() * 9)
            .map(Tile)
            .filter(move |t| self.lane_for_tile(*t).is_none())
    }
    /// Logical lane-bit masks, unchanged from the accepted No Stretch policy.
    pub fn no_stretch_regions(self) -> Option<&'static [u16]> {
        LAYOUTS[self.index].3
    }
    /// Physical refinement of the logical masks. In 9A the left two regions
    /// use L5 and the right two use R5. Never approve an arbitrary center
    /// choice solely because the logical Oni bit fits.
    pub fn pair_allowed(self, a: Tile, b: Tile) -> bool {
        let Some(la) = self.lane_for_tile(a) else {
            return false;
        };
        let Some(lb) = self.lane_for_tile(b) else {
            return false;
        };
        let Some(regions) = self.no_stretch_regions() else {
            return true;
        };
        regions.iter().enumerate().any(|(i, mask)| {
            let fits = (1u16 << la | 1u16 << lb) & !mask == 0;
            fits && (self.index != NINE_A
                || [a, b]
                    .into_iter()
                    .all(|t| t.cell() != 5 || t.pad() == u8::from(i >= 2)))
        })
    }
}
