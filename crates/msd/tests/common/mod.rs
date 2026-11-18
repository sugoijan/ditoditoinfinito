//! What the fixture test and the oracle comparison share: synthetic charts
//! for each skillset (generated, seeded), the bundled songs' four-lane
//! charts, and the fixture's format.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ddi_msd::NoteInfo;

/// One rating to compare: its rows and how it is rated.
#[derive(Clone, Debug)]
pub struct Case {
    pub name: String,
    pub rows: Vec<NoteInfo>,
    pub rate: f32,
    pub goal: f32,
    pub ssr: bool,
}

impl Case {
    pub fn listed(name: impl Into<String>, rows: Vec<NoteInfo>) -> Case {
        Case {
            name: name.into(),
            rows,
            rate: ddi_msd::LISTED_RATE,
            goal: ddi_msd::LISTED_GOAL,
            ssr: false,
        }
    }

    /// The Rust port's eight numbers.
    pub fn rate(&self) -> [f32; 8] {
        if self.ssr {
            ddi_msd::ssr(&self.rows, self.rate, self.goal)
        } else {
            ddi_msd::msd(&self.rows, self.rate)
        }
    }
}

/// xorshift64*, seeded: the same charts on every machine.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    pub fn chance(&mut self, p: f64) -> bool {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 * 1.0 < p
    }
}

/// Rows from (beat, notes) at a constant `bpm`.
fn at_bpm(bpm: f64, rows: &[(f64, u32)]) -> Vec<NoteInfo> {
    let origin = rows.first().map_or(0.0, |r| r.0);
    rows.iter()
        .map(|&(beat, notes)| NoteInfo {
            notes,
            row_time: ((beat - origin) * 60.0 / bpm) as f32,
        })
        .collect()
}

fn single_not(rng: &mut Rng, avoid: u32) -> u32 {
    let avoid = if avoid & 0b1111 == 0b1111 { 0 } else { avoid };
    loop {
        let n = 1 << rng.below(4);
        if n & avoid == 0 {
            return n;
        }
    }
}

fn chord(rng: &mut Rng, size: u32, avoid: u32) -> u32 {
    for _ in 0..64 {
        let mut n = 0u32;
        while n.count_ones() < size {
            n |= 1 << rng.below(4);
        }
        if n & avoid == 0 || size + avoid.count_ones() > 4 {
            return n;
        }
    }
    0b1111
}

/// Streams: singles, no jacks.
fn stream(rng: &mut Rng, bpm: f64, beats: f64, snap: f64) -> Vec<NoteInfo> {
    let mut rows = Vec::new();
    let mut last = 0;
    let mut beat = 0.0;
    while beat < beats {
        last = single_not(rng, last);
        rows.push((beat, last));
        beat += 1.0 / snap;
    }
    at_bpm(bpm, &rows)
}

/// Chorded streams: a chord of `size` every `every` rows.
fn chordstream(rng: &mut Rng, bpm: f64, beats: f64, size: u32, every: u32) -> Vec<NoteInfo> {
    let mut rows = Vec::new();
    let mut last = 0;
    let mut beat = 0.0;
    let mut k = 0;
    while beat < beats {
        last = if k % every == 0 {
            chord(rng, size, last)
        } else {
            single_not(rng, last)
        };
        rows.push((beat, last));
        beat += 0.25;
        k += 1;
    }
    at_bpm(bpm, &rows)
}

/// Jacks: runs on one column, other columns between runs.
fn jacks(rng: &mut Rng, bpm: f64, beats: f64, run: u32, snap: f64) -> Vec<NoteInfo> {
    let mut rows = Vec::new();
    let mut beat = 0.0;
    while beat < beats {
        let col = 1 << rng.below(4);
        for _ in 0..run {
            rows.push((beat, col));
            beat += 1.0 / snap;
        }
        beat += 1.0 / snap;
    }
    at_bpm(bpm, &rows)
}

/// Chordjacks: chords sharing a column with the last.
fn chordjacks(rng: &mut Rng, bpm: f64, beats: f64, snap: f64) -> Vec<NoteInfo> {
    let mut rows = Vec::new();
    let mut last = 0b0011u32;
    let mut beat = 0.0;
    while beat < beats {
        let size = 2 + rng.below(3) as u32;
        let mut n = chord(rng, size, 0);
        if n & last == 0 {
            n |= 1 << last.trailing_zeros();
        }
        rows.push((beat, n));
        last = n;
        beat += 1.0 / snap;
    }
    at_bpm(bpm, &rows)
}

/// Technical: rolls, one hand trills, runningmen, jumptrills, flams and
/// mixed snaps, a section each.
fn technical(rng: &mut Rng, bpm: f64, sections: u32) -> Vec<NoteInfo> {
    let mut rows: Vec<(f64, u32)> = Vec::new();
    let mut beat = 0.0f64;
    for _ in 0..sections {
        match rng.below(8) {
            0 => {
                // rolls 1234 / 4321
                let order: [u32; 4] = if rng.chance(0.5) {
                    [1, 2, 4, 8]
                } else {
                    [8, 4, 2, 1]
                };
                for k in 0..32 {
                    rows.push((beat, order[k % 4]));
                    beat += 0.25;
                }
            }
            1 => {
                // one hand trill
                let (a, b) = if rng.chance(0.5) { (1, 2) } else { (4, 8) };
                for k in 0..32 {
                    rows.push((beat, if k % 2 == 0 { a } else { b }));
                    beat += 0.25;
                }
            }
            2 => {
                // runningman: an anchor with other notes between
                let anchor = 1 << rng.below(4);
                for k in 0..32 {
                    let n = if k % 2 == 0 {
                        anchor
                    } else {
                        single_not(rng, anchor)
                    };
                    rows.push((beat, n));
                    beat += 0.25;
                }
            }
            3 => {
                // jumptrill
                for k in 0..32 {
                    rows.push((beat, if k % 2 == 0 { 0b0011 } else { 0b1100 }));
                    beat += 0.25;
                }
            }
            4 => {
                // flams: chords split a few ms apart
                for _ in 0..16 {
                    let size = 2 + rng.below(2) as u32;
                    let c = chord(rng, size, 0);
                    let mut t = beat;
                    for bit in 0..4 {
                        if c & (1 << bit) != 0 {
                            rows.push((t, 1 << bit));
                            t += 0.01 * bpm / 60.0;
                        }
                    }
                    beat += 0.5;
                }
            }
            5 => {
                // mixed snaps
                let mut last = 0;
                for _ in 0..32 {
                    last = single_not(rng, last);
                    rows.push((beat, last));
                    beat += [0.25, 1.0 / 3.0, 1.0 / 6.0, 0.125][rng.below(4) as usize];
                }
            }
            6 => {
                // minijacks inside a stream
                let mut last = 0;
                for k in 0..32 {
                    if k % 7 == 3 {
                        rows.push((beat, last));
                    } else {
                        last = single_not(rng, last);
                        rows.push((beat, last));
                    }
                    beat += 0.25;
                }
            }
            _ => {
                // split jumps and hands
                let mut last = 0;
                for _ in 0..32 {
                    let size = 1 + rng.below(3) as u32;
                    last = chord(rng, size, last);
                    rows.push((beat, last));
                    beat += 0.25;
                }
            }
        }
        beat += rng.below(3) as f64;
    }
    at_bpm(bpm, &rows)
}

/// Anything: random rows, snaps and gaps.
fn random(rng: &mut Rng, bpm: f64, beats: f64) -> Vec<NoteInfo> {
    let mut rows = Vec::new();
    let mut beat = 0.0;
    while beat < beats {
        let n = (rng.below(15) + 1) as u32;
        rows.push((beat, n));
        beat += [0.25, 0.5, 1.0 / 3.0, 0.125, 1.0, 2.0][rng.below(6) as usize];
        if rng.chance(0.01) {
            beat += 8.0;
        }
    }
    at_bpm(bpm, &rows)
}

/// Synthetic charts exercising each skillset, at a few speeds each, and a
/// few rated at other rates and goals and as scores.
pub fn synthetic() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut rng = Rng::new(527);
    for (i, bpm) in [120.0, 150.0, 175.0, 200.0, 230.0].into_iter().enumerate() {
        cases.push(Case::listed(
            format!("stream/{bpm}"),
            stream(&mut rng, bpm, 128.0, 4.0),
        ));
        cases.push(Case::listed(
            format!("stream24/{bpm}"),
            stream(&mut rng, bpm * 0.75, 96.0, 6.0),
        ));
        cases.push(Case::listed(
            format!("jumpstream/{bpm}"),
            chordstream(&mut rng, bpm, 128.0, 2, 2 + i as u32 % 2),
        ));
        cases.push(Case::listed(
            format!("handstream/{bpm}"),
            chordstream(&mut rng, bpm, 128.0, 3, 3),
        ));
        cases.push(Case::listed(
            format!("quadstream/{bpm}"),
            chordstream(&mut rng, bpm * 0.8, 96.0, 4, 4),
        ));
        cases.push(Case::listed(
            format!("jacks/{bpm}"),
            jacks(&mut rng, bpm, 96.0, 4 + i as u32, 4.0),
        ));
        cases.push(Case::listed(
            format!("longjacks/{bpm}"),
            jacks(&mut rng, bpm * 0.6, 96.0, 16, 4.0),
        ));
        cases.push(Case::listed(
            format!("chordjacks/{bpm}"),
            chordjacks(&mut rng, bpm, 96.0, 2.0 + i as f64 % 2.0),
        ));
        cases.push(Case::listed(
            format!("technical/{bpm}"),
            technical(&mut rng, bpm, 16),
        ));
        cases.push(Case::listed(
            format!("random/{bpm}"),
            random(&mut rng, bpm, 160.0),
        ));
    }
    for bpm in [140.0, 170.0, 190.0] {
        cases.push(Case::listed(
            format!("stamina/{bpm}"),
            stream(&mut rng, bpm, bpm * 4.0, 4.0),
        ));
        cases.push(Case::listed(
            format!("stamina-js/{bpm}"),
            chordstream(&mut rng, bpm, bpm * 4.0, 2, 3),
        ));
    }
    for seed in 0..12u64 {
        let mut r = Rng::new(1000 + seed);
        let bpm = 100.0 + r.below(160) as f64;
        cases.push(Case::listed(
            format!("mix/{seed}"),
            technical(&mut r, bpm, 24),
        ));
    }
    // Small and edge cases.
    cases.push(Case::listed("edge/empty", Vec::new()));
    cases.push(Case::listed("edge/one", at_bpm(120.0, &[(0.0, 1)])));
    cases.push(Case::listed(
        "edge/two",
        at_bpm(120.0, &[(0.0, 1), (1.0, 2)]),
    ));
    cases.push(Case::listed(
        "edge/quads",
        at_bpm(150.0, &(0..64).map(|k| (k as f64, 15)).collect::<Vec<_>>()),
    ));
    cases.push(Case::listed(
        "edge/sparse",
        at_bpm(
            60.0,
            &(0..40)
                .map(|k| (k as f64 * 2.0, 1 << (k % 4)))
                .collect::<Vec<_>>(),
        ),
    ));
    cases.push(Case::listed(
        "edge/burst",
        at_bpm(
            200.0,
            &(0..200)
                .map(|k| (k as f64 / 12.0, 1 << (k % 4)))
                .collect::<Vec<_>>(),
        ),
    ));
    cases.push(Case::listed(
        "edge/too-dense",
        at_bpm(
            200.0,
            &(0..200)
                .map(|k| (k as f64 / 48.0, 1 << (k % 4)))
                .collect::<Vec<_>>(),
        ),
    ));
    cases.push(Case::listed(
        "edge/out-of-order",
        vec![
            NoteInfo {
                notes: 1,
                row_time: 0.0,
            },
            NoteInfo {
                notes: 2,
                row_time: 0.5,
            },
            NoteInfo {
                notes: 4,
                row_time: 0.5,
            },
        ],
    ));
    cases.push(Case::listed(
        "edge/late-start",
        at_bpm(
            120.0,
            &(0..64)
                .map(|k| (8.0 + k as f64 * 0.25, 1 << (k % 4)))
                .collect::<Vec<_>>(),
        )
        .into_iter()
        .map(|mut r| {
            r.row_time += 3.2;
            r
        })
        .collect(),
    ));
    // Other rates, goals and the score path.
    let mut r = Rng::new(9);
    let base = technical(&mut r, 170.0, 20);
    for (rate, goal, ssr) in [
        (1.5, 0.93, false),
        (0.8, 0.93, false),
        (1.0, 0.965, true),
        (1.0, 0.85, true),
        (1.2, 0.99, true),
    ] {
        cases.push(Case {
            name: format!("rated/{rate}/{goal}/{ssr}"),
            rows: base.clone(),
            rate,
            goal,
            ssr,
        });
    }
    cases
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A simfile parsed as the library parses it (generated versions added).
pub fn parse(path: &Path) -> Option<ddi_chart::Song> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mut song = ddi_chart::formats::sm::parse_simfile(&text, &ext).ok()?;
    song.add_chart_versions();
    Some(song)
}

/// The simfile a folder plays: its `.ssc`, else its `.sm`.
pub fn simfile_in(dir: &Path) -> Option<PathBuf> {
    let mut sm = None;
    let mut ssc = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        match p
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("ssc") => ssc = Some(p),
            Some("sm") => sm = Some(p),
            _ => {}
        }
    }
    ssc.or(sm)
}

/// The four-lane charts of `song`, generated versions included.
pub fn song_cases(prefix: &str, song: &ddi_chart::Song) -> Vec<Case> {
    song.charts
        .iter()
        .enumerate()
        .filter(|(_, c)| ddi_msd::chart::four_keys(&c.layout))
        .map(|(i, c)| {
            Case::listed(
                format!("{prefix}/{i}"),
                ddi_msd::chart::rows(c, c.timing(song)),
            )
        })
        .collect()
}

/// The bundled songs' four-lane charts.
pub fn bundled() -> Vec<Case> {
    let songs = repo_root().join("assets/songs");
    let mut dirs: Vec<_> = std::fs::read_dir(&songs)
        .expect("assets/songs")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let mut cases = Vec::new();
    for dir in dirs {
        let id = dir.file_name().unwrap().to_string_lossy().into_owned();
        let Some(file) = simfile_in(&dir) else {
            continue;
        };
        let song = parse(&file).expect("a bundled simfile parses");
        cases.extend(song_cases(&format!("bundled/{id}"), &song));
    }
    cases
}

/// The committed fixture: the oracle's eight numbers a case, as f32 bits.
pub const FIXTURE: &str = "tests/fixture.json";

#[derive(Debug)]
pub struct Fixture {
    pub calc_version: u32,
    pub commit: String,
    pub expected: BTreeMap<String, [f32; 8]>,
}

pub fn read_fixture() -> Fixture {
    // Compiled in, so the test runs under WASI too (no files there).
    let text = include_str!("../fixture.json");
    let v: serde_json::Value = serde_json::from_str(text).expect("fixture JSON");
    let mut expected = BTreeMap::new();
    for (name, bits) in v["expected"].as_object().expect("expected") {
        let mut out = [0f32; 8];
        for (k, b) in bits.as_array().expect("eight").iter().enumerate() {
            out[k] = f32::from_bits(u32::from_str_radix(b.as_str().unwrap(), 16).unwrap());
        }
        expected.insert(name.clone(), out);
    }
    Fixture {
        calc_version: v["calc_version"].as_u64().unwrap() as u32,
        commit: v["commit"].as_str().unwrap().to_string(),
        expected,
    }
}

pub fn write_fixture(expected: &BTreeMap<String, [f32; 8]>) {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!("  \"calc_version\": {},\n", ddi_msd::CALC_VERSION));
    s.push_str(&format!(
        "  \"commit\": \"{}\",\n",
        ddi_msd::UPSTREAM_COMMIT
    ));
    s.push_str("  \"expected\": {\n");
    let n = expected.len();
    for (i, (name, vals)) in expected.iter().enumerate() {
        let bits: Vec<String> = vals
            .iter()
            .map(|v| format!("\"{:08x}\"", v.to_bits()))
            .collect();
        s.push_str(&format!(
            "    \"{name}\": [{}]{}\n",
            bits.join(", "),
            if i + 1 < n { "," } else { "" }
        ));
    }
    s.push_str("  }\n}\n");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    std::fs::write(path, s).expect("write the fixture");
}

/// Largest absolute difference a skillset over `pairs`, and how many cases
/// match to the bit.
pub fn compare(pairs: &[([f32; 8], [f32; 8])]) -> ([f32; 8], usize) {
    let mut worst = [0f32; 8];
    let mut exact = 0;
    for (a, b) in pairs {
        let mut same = true;
        for k in 0..8 {
            let d = (a[k] - b[k]).abs();
            if d > worst[k] || d.is_nan() {
                worst[k] = if d.is_nan() { f32::INFINITY } else { d };
            }
            if a[k].to_bits() != b[k].to_bits() {
                same = false;
            }
        }
        if same {
            exact += 1;
        }
    }
    (worst, exact)
}

pub const SKILLSETS: [&str; 8] = [
    "Overall",
    "Stream",
    "Jumpstream",
    "Handstream",
    "Stamina",
    "JackSpeed",
    "Chordjack",
    "Technical",
];
