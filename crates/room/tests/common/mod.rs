//! What the room's integration tests share: a song's content, a battle
//! plan of fixed sections, a competitor's scripted play. Each test file
//! takes what it uses (`mod common;`).

#![allow(dead_code)]

use ddi_room::attacks::{Judged, POWER_CLASSES_V0};
use ddi_room::live::plan::BattlePlan;
use ddi_room::live::stream::{Moment, MomentKind};
use ddi_room::{ChartKey, ContentId, SongKey};

/// Song `n`'s content.
pub fn song(n: u32) -> ContentId {
    ContentId {
        song: SongKey::new(format!("song-{n}")),
        chart: ChartKey::new(format!("digest-{n}")),
    }
}

/// A plan of `n` slots of 2 s under the attack rules named `rules`, each
/// with four shocks (a new room's Shocks per attack: the first, third,
/// sixth and eighth of eight a quarter second apart, so a section clears
/// when a section of eight did) where `room(j)`; checkpoints every 2 s
/// from 2 s.
pub fn plan_under(rules: &str, n: usize, seed: u64, room: impl Fn(usize) -> bool) -> BattlePlan {
    plan_of(rules, n, seed, 4, room)
}

/// [`plan_under`] for attacks of `shocks` shocks (2, 4, 6 or 8 of the
/// eight a quarter second apart, the first and the last always).
pub fn plan_of(
    rules: &str,
    n: usize,
    seed: u64,
    shocks: u32,
    room: impl Fn(usize) -> bool,
) -> BattlePlan {
    let kept: &[i64] = match shocks {
        2 => &[0, 7],
        4 => &[0, 2, 5, 7],
        6 => &[0, 1, 3, 4, 6, 7],
        _ => &[0, 1, 2, 3, 4, 5, 6, 7],
    };
    use ddi_room::attacks::{Cell, Schedule, Section, Slot};
    const S: i64 = 1_000_000;
    let slots = (0..n)
        .map(|j| {
            let start_us = j as i64 * 2 * S;
            Slot {
                start_us,
                section: room(j).then(|| {
                    let cells: Vec<Cell> = kept
                        .iter()
                        .map(|&i| Cell {
                            tick: (j as i64 * 4 * 48) + i * 24,
                            lane: (i % 4) as u8,
                            at_us: start_us + i * S / 4,
                        })
                        .collect();
                    let last = cells.last().unwrap().at_us;
                    Section {
                        clear_us: last + 100_000,
                        cells,
                    }
                }),
            }
        })
        .collect();
    BattlePlan::new(
        rules,
        "placement-v2",
        POWER_CLASSES_V0.id,
        "ddi",
        seed,
        (1..=n as i64).map(|k| k * 2 * S).collect(),
        Schedule { slots },
    )
    .with_shocks(shocks)
}

/// A competitor's play, scripted: its moments in time order.
#[derive(Clone, Debug)]
pub struct Script {
    pub moments: Vec<Moment>,
    /// Out at this song ms (its life emptied), if it goes out.
    pub out_ms: Option<i32>,
    /// Plays to this song ms.
    pub end_ms: i32,
}

/// A seeded play: a moment every `gap_ms` from about 1 s to `end_ms`,
/// out at `out_ms` if given; some of its moments holds' ends where
/// `holds` (each draws once more, so the two kinds of script differ).
pub fn script(seed: u64, gap_ms: i32, end_ms: i32, out_ms: Option<i32>, holds: bool) -> Script {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let (mut life, mut combo, mut score) = (1_000_000i64, 0u32, 0i64);
    let mut moments = Vec::new();
    let mut at = 1_000 + (seed % 7) as i32 * 13;
    while at < out_ms.unwrap_or(end_ms).min(end_ms) {
        let r = next() % 100;
        let tier = match r {
            0..=79 => 0u8,
            80..=93 => 1,
            94..=97 => 2,
            _ => 5,
        };
        if tier == 5 {
            life -= 40_000;
            combo = 0;
        } else {
            life += 2_000;
            combo += 1;
            score += 6_000 - i64::from(tier) * 1_000;
        }
        life = life.clamp(1, 1_000_000);
        let kind = if holds && next() % 29 == 0 {
            MomentKind::HoldEnd { ok: tier < 5 }
        } else {
            MomentKind::Tap {
                tier,
                offset_ms: if tier == 5 {
                    0
                } else {
                    (next() % 41) as i16 - 20
                },
                w0: tier == 0 && next() % 2 == 0,
            }
        };
        moments.push(Moment {
            at_ms: at,
            tick: i64::from(at) * 48 * 2 / 1_000,
            lane: (next() % 4) as u8,
            kind,
            life_ppm: life as u32,
            combo,
            score,
        });
        at += gap_ms;
    }
    if let Some(out) = out_ms {
        moments.push(Moment {
            at_ms: out,
            tick: i64::from(out) * 48 * 2 / 1_000,
            lane: 0,
            kind: MomentKind::Tap {
                tier: 5,
                offset_ms: 0,
                w0: false,
            },
            life_ppm: 0,
            combo: 0,
            score,
        });
    }
    Script {
        moments,
        out_ms,
        end_ms,
    }
}

impl Script {
    /// The judgments of the chart's notes, as the ledger counts them.
    pub fn judged(&self) -> Vec<Judged> {
        self.moments
            .iter()
            .filter_map(|m| match m.kind {
                MomentKind::Tap { tier, .. } => Some(Judged {
                    at_us: i64::from(m.at_ms) * 1_000,
                    tier,
                }),
                _ => None,
            })
            .collect()
    }

    /// Life just before song µs `at`.
    pub fn life(&self, at: i64) -> u32 {
        self.moments
            .iter()
            .take_while(|m| i64::from(m.at_ms) * 1_000 < at)
            .last()
            .map_or(1_000_000, |m| m.life_ppm)
    }
}
