//! A chart's identity by its content (`docs/plans/play-history.md`): a
//! digest of what playing it reads, so that a play's record finds its
//! chart again after a pack is moved or renamed, and does not when its
//! notes change.
//!
//! What it covers: the notes (tick, lane, kind, a hold's or roll's end,
//! a Dancing☆Onigiri boost); the timing the chart plays by (its offset,
//! BPMs, stops, delays, warps, speeds, scrolls, fakes, tick counts and
//! combos); the layout (its id, lane count, the lanes' shuffle groups and
//! its quarter turn, what turns read); and what a
//! Dancing☆Onigiri chart says that changes its play (its speed, gauge
//! values and headers, its place in the work, where it starts and ends).
//! What it leaves out: titles, artists, credits, the chart's name,
//! difficulty and meter, art, music, file names, note colours,
//! keysounds, background and foreground changes, time signatures and
//! labels.
//!
//! The bytes hashed are written field by field in a fixed order, numbers
//! as their exact bits (ticks as integers, seconds as an `f64`'s bits),
//! so the digest is the same on every platform and parse. The hash is
//! 128-bit FNV-1a: no dependency, and plenty against chance collisions
//! between charts (it is no defence against forgery, which nothing here
//! needs).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::{NoteKind, Song, Tick};

/// The digest of a chart's content ([`Song::chart_digest`]); written as 32
/// hex digits.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChartDigest(pub u128);

impl fmt::Display for ChartDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// A string that is not 32 hex digits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DigestError;

impl fmt::Display for DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a chart digest is 32 hex digits")
    }
}

impl std::error::Error for DigestError {}

impl FromStr for ChartDigest {
    type Err = DigestError;

    fn from_str(s: &str) -> Result<ChartDigest, DigestError> {
        if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(DigestError);
        }
        u128::from_str_radix(s, 16)
            .map(ChartDigest)
            .map_err(|_| DigestError)
    }
}

impl Serialize for ChartDigest {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ChartDigest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<ChartDigest, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// 128-bit FNV-1a over what is written to it.
struct Fnv(u128);

impl Fnv {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

    fn new() -> Fnv {
        Fnv(Fnv::OFFSET)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u128::from(b);
            self.0 = self.0.wrapping_mul(Fnv::PRIME);
        }
    }

    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    fn i64(&mut self, v: i64) {
        self.bytes(&v.to_le_bytes());
    }

    fn tick(&mut self, t: Tick) {
        self.i64(t.0);
    }

    /// An `f64`'s exact bits (`-0.0` and `0.0` alike, so a value written
    /// either way is the same).
    fn f64(&mut self, v: f64) {
        let v = if v == 0.0 { 0.0 } else { v };
        self.u64(v.to_bits());
    }

    fn str(&mut self, s: &str) {
        self.u64(s.len() as u64);
        self.bytes(s.as_bytes());
    }

    /// A section's name and its number of entries, so that entries never
    /// run into the next section's.
    fn section(&mut self, name: &str, len: usize) {
        self.str(name);
        self.u64(len as u64);
    }
}

impl Song {
    /// The digest of chart `index`'s content (see the module's notes).
    ///
    /// # Panics
    /// If `index` is out of range.
    pub fn chart_digest(&self, index: usize) -> ChartDigest {
        let chart = &self.charts[index];
        let mut h = Fnv::new();
        h.str("ddi-chart-digest 1");

        // The layout: its id, its lanes and what a turn reads of them
        // (each lane's shuffle group, the quarter turn's mapping), which a
        // song may define for itself.
        let layout = self.layout_of(chart);
        h.section("layout", 1);
        h.str(&chart.layout);
        match &layout {
            Some(l) => {
                h.u64(l.lanes.len() as u64);
                for lane in &l.lanes {
                    h.u64(u64::from(lane.shuffle_group));
                }
                match &l.turn_left {
                    Some(map) => {
                        h.u64(map.len() as u64 + 1);
                        h.bytes(map);
                    }
                    None => h.u64(0),
                }
            }
            None => h.u64(0),
        }

        h.section("notes", chart.notes.len());
        for n in &chart.notes {
            h.tick(n.tick);
            h.u64(u64::from(n.lane));
            let (kind, end) = match n.kind {
                NoteKind::Tap => (0, None),
                NoteKind::HoldHead { end } => (1, Some(end)),
                NoteKind::RollHead { end } => (2, Some(end)),
                NoteKind::Mine => (3, None),
                NoteKind::Lift => (4, None),
                NoteKind::Fake => (5, None),
                NoteKind::Shock => (6, None),
                NoteKind::AutoKeysound => (7, None),
                NoteKind::Dummy => (8, None),
            };
            h.u64(kind);
            if let Some(end) = end {
                h.tick(end);
            }
            match n.speed_mul {
                Some(m) => {
                    h.u64(1);
                    h.u64(u64::from(m.to_bits()));
                }
                None => h.u64(0),
            }
        }

        let t = chart.timing(self);
        h.section("offset", 1);
        h.f64(t.offset_seconds);
        h.section("bpms", t.bpms.len());
        for s in &t.bpms {
            h.tick(s.tick);
            h.f64(s.bpm);
        }
        h.section("stops", t.stops.len());
        for s in &t.stops {
            h.tick(s.tick);
            h.f64(s.seconds);
        }
        h.section("delays", t.delays.len());
        for s in &t.delays {
            h.tick(s.tick);
            h.f64(s.seconds);
        }
        h.section("warps", t.warps.len());
        for s in &t.warps {
            h.tick(s.tick);
            h.tick(s.length);
        }
        h.section("speeds", t.speeds.len());
        for s in &t.speeds {
            h.tick(s.tick);
            h.f64(s.ratio);
            h.f64(s.delay);
            h.u64(match s.unit {
                crate::SpeedUnit::Beats => 0,
                crate::SpeedUnit::Seconds => 1,
            });
        }
        h.section("scrolls", t.scrolls.len());
        for s in &t.scrolls {
            h.tick(s.tick);
            h.f64(s.ratio);
        }
        h.section("fakes", t.fakes.len());
        for s in &t.fakes {
            h.tick(s.tick);
            h.tick(s.length);
        }
        h.section("tick counts", t.tick_counts.len());
        for s in &t.tick_counts {
            h.tick(s.tick);
            h.u64(u64::from(s.ticks_per_beat));
        }
        h.section("combos", t.combos.len());
        for s in &t.combos {
            h.tick(s.tick);
            h.u64(u64::from(s.combo));
            h.u64(u64::from(s.miss_combo));
        }

        match &chart.danoni {
            None => h.section("danoni", 0),
            Some(d) => {
                h.section("danoni", 1);
                h.f64(d.init_speed);
                for g in &d.gauge {
                    h.str(g);
                }
                h.u64(d.headers.len() as u64);
                for (k, v) in &d.headers {
                    h.str(k);
                    h.str(v);
                }
                h.u64(d.index as u64);
                for at in [d.start, d.end] {
                    match at {
                        Some(s) => {
                            h.u64(1);
                            h.f64(s);
                        }
                        None => h.u64(0),
                    }
                }
            }
        }
        ChartDigest(h.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_print_and_parse_as_hex() {
        let d = ChartDigest(0x0123_4567_89ab_cdef_0011_2233_4455_6677);
        assert_eq!(d.to_string(), "0123456789abcdef0011223344556677");
        assert_eq!("0123456789abcdef0011223344556677".parse(), Ok(d));
        assert_eq!(
            "0123456789ABCDEF0011223344556677".parse::<ChartDigest>(),
            Ok(d)
        );
        assert!("123".parse::<ChartDigest>().is_err());
        assert!(
            "+123456789abcdef0011223344556677"
                .parse::<ChartDigest>()
                .is_err()
        );
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, "\"0123456789abcdef0011223344556677\"");
        assert_eq!(serde_json::from_str::<ChartDigest>(&json).unwrap(), d);
    }

    #[test]
    fn fnv_128_has_its_reference_value() {
        // FNV-1a 128 of "" is its offset basis; of "a", the published
        // test vector.
        let mut h = Fnv::new();
        assert_eq!(h.0, Fnv::OFFSET);
        h.bytes(b"a");
        assert_eq!(h.0, 0xd228_cb69_6f1a_8caf_7891_2b70_4e4a_8964);
    }
}
