//! Generated charts and press scripts the engine's tests share
//! (`cadence.rs`, `history.rs`).

#![allow(dead_code)]

use ddi_chart::{
    BpmSegment, Chart, Difficulty, DisplayBpm, Note, NoteKind, Song, SourceFormat, SourceInfo,
    StopSegment, Tick, TimingMap,
};
use ddi_engine::player::Player;

/// A small deterministic generator (xorshift64*).
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Uniform in `0..1`.
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

pub fn song(layout: &str, timing: TimingMap, notes: Vec<Note>) -> Song {
    Song {
        title: "Cadence".into(),
        subtitle: String::new(),
        artist: "test".into(),
        title_translit: String::new(),
        subtitle_translit: String::new(),
        artist_translit: String::new(),
        genre: String::new(),
        credit: String::new(),
        music: None,
        preview_start: 0.0,
        preview_length: 0.0,
        banner: None,
        background: None,
        jacket: None,
        cd_title: None,
        timing,
        display_bpm: DisplayBpm::Actual,
        charts: vec![Chart {
            derivation: None,
            layout: layout.into(),
            difficulty: Difficulty::Hard,
            meter: 9,
            name: "cadence".into(),
            description: String::new(),
            credit: String::new(),
            notes,
            timing: None,
            display_bpm: None,
            danoni: None,
            ffr: false,
        }],
        effects: Vec::new(),
        keysounds: Vec::new(),
        layouts: Vec::new(),
        source: SourceInfo {
            format: SourceFormat::Other("test".into()),
            unknown_tags: Vec::new(),
        },
    }
}

/// A chart of four lanes on a 48ths grid: taps, jumps, holds and rolls
/// (some long, crossing mines in other lanes and checkpoints), lifts,
/// mines (some in a held lane), and rows of mines in every lane (shocks
/// under DDR A); 150 BPM with a change to 180 and a stop.
pub fn chart(seed: u64, layout: &str) -> Song {
    chart_of(seed, layout, 64)
}

/// [`chart`], `beats` long.
pub fn chart_of(seed: u64, layout: &str, beats: i64) -> Song {
    let mut rng = Rng(seed | 1);
    let lanes = 4u8;
    let mut notes = Vec::new();
    // Per lane, the tick from which it is free again.
    let mut free = [0i64; 4];
    let mut tick = Tick::from_beats(2).0;
    let end = Tick::from_beats(beats).0;
    while tick < end {
        let roll = rng.below(100);
        if roll < 6 {
            // A row of mines in every lane, where every lane is free.
            if free.iter().all(|&f| f <= tick) {
                for lane in 0..lanes {
                    notes.push(Note::new(Tick(tick), lane, NoteKind::Mine));
                }
            }
        } else {
            let count = if roll < 20 { 2 } else { 1 };
            for _ in 0..count {
                let lane = rng.below(u64::from(lanes)) as u8;
                if free[usize::from(lane)] > tick
                    || notes
                        .iter()
                        .any(|n: &Note| n.tick.0 == tick && n.lane == lane)
                {
                    continue;
                }
                let kind = match rng.below(100) {
                    0..=59 => NoteKind::Tap,
                    60..=71 => NoteKind::HoldHead {
                        end: Tick(tick + 12 * (2 + rng.below(16) as i64)),
                    },
                    72..=79 => NoteKind::RollHead {
                        end: Tick(tick + 12 * (2 + rng.below(12) as i64)),
                    },
                    80..=85 => NoteKind::Lift,
                    _ => NoteKind::Mine,
                };
                let until = match kind {
                    NoteKind::HoldHead { end } | NoteKind::RollHead { end } => end.0 + 12,
                    _ => tick + 6,
                };
                free[usize::from(lane)] = until;
                notes.push(Note::new(Tick(tick), lane, kind));
            }
        }
        tick += [6, 12, 12, 24, 24, 48][rng.below(6) as usize];
    }
    notes.sort_by_key(|n| (n.tick, n.lane));
    let mut timing = TimingMap::constant(150.0, 0.03);
    timing.bpms.push(BpmSegment {
        tick: Tick::from_beats(32),
        bpm: 180.0,
    });
    timing.stops.push(StopSegment {
        tick: Tick::from_beats(20),
        seconds: 0.4,
    });
    song(layout, timing, notes)
}

/// `(song seconds, lane, pressed)` in time order: the notes pressed with
/// random errors (some not at all), holds let go and taken again, rolls
/// tapped at uneven intervals, mines stepped on and lanes held across
/// them, and stray presses.
pub fn script(player: &Player, seed: u64) -> Vec<(f64, u8, bool)> {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let lanes = usize::from(player.lanes());
    let mut spans: Vec<Vec<(f64, f64)>> = vec![Vec::new(); lanes];
    for n in player.judge().notes() {
        let lane = usize::from(n.note.lane);
        let t = n.seconds;
        match n.note.kind {
            NoteKind::Tap => {
                if rng.unit() < 0.85 {
                    let p = t + rng.range(-0.2, 0.2);
                    spans[lane].push((p, p + rng.range(0.01, 0.12)));
                }
            }
            NoteKind::Lift => {
                if rng.unit() < 0.85 {
                    let r = t + rng.range(-0.15, 0.15);
                    spans[lane].push((r - rng.range(0.05, 0.3), r));
                }
            }
            NoteKind::HoldHead { .. } => {
                if rng.unit() < 0.9 {
                    let p = t + rng.range(-0.15, 0.15);
                    let end = n.end_seconds + rng.range(-0.3, 0.2);
                    // Let go a while and taken again, sometimes.
                    if rng.unit() < 0.5 && end - p > 0.4 {
                        let off = rng.range(p + 0.05, end - 0.3);
                        let back = off + rng.range(0.02, 0.5);
                        spans[lane].push((p, off));
                        if back < end {
                            spans[lane].push((back, end));
                        }
                    } else {
                        spans[lane].push((p, end));
                    }
                }
            }
            NoteKind::RollHead { .. } => {
                let mut p = t + rng.range(-0.12, 0.12);
                while p < n.end_seconds + 0.1 {
                    spans[lane].push((p, p + 0.04));
                    p += rng.range(0.05, 0.45);
                }
            }
            NoteKind::Mine | NoteKind::Shock => match rng.below(5) {
                // Stepped on, near it.
                0 => {
                    let p = t + rng.range(-0.1, 0.1);
                    spans[lane].push((p, p + 0.05));
                }
                // Held across it.
                1 => spans[lane].push((t - rng.range(0.01, 0.3), t + rng.range(0.0, 0.3))),
                _ => {}
            },
            _ => {}
        }
    }
    // Stray presses.
    let last = player.song_end_time();
    for _ in 0..40 {
        let lane = rng.below(lanes as u64) as usize;
        let p = rng.range(0.0, last);
        spans[lane].push((p, p + rng.range(0.01, 0.2)));
    }
    let mut out = Vec::new();
    for (lane, mut s) in spans.into_iter().enumerate() {
        s.sort_by(|a, b| a.0.total_cmp(&b.0));
        // A lane's presses one after another: each let go before the
        // next.
        let mut until = f64::NEG_INFINITY;
        for (p, r) in s {
            let p = p.max(until + 0.001);
            if r <= p + 0.001 {
                continue;
            }
            out.push((p, lane as u8, true));
            out.push((r, lane as u8, false));
            until = r;
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)).then(a.1.cmp(&b.1)));
    out
}
