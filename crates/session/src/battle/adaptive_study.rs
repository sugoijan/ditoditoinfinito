//! Opt-in study: the match rule "Charts: Adaptive" (`adaptive-v0`,
//! `ddi_engine::ladder`; `docs/plans/multiplayer-prototype-progress.md`,
//! "Adaptive charts").
//!
//! `cargo test --release -p ddi-session --lib adaptive_study -- --ignored
//! --nocapture` reads the bundled songs and, with `DDI_STEP_CORPUS=<dir>[:<dir>…]`,
//! every song folder under the folders (in place; one simfile a folder,
//! `.ssc` before `.sm` before `.dwi`), and prints aggregates only, never a
//! song's name:
//!
//! 1. the ladders: by the stage chart's slot, how many dance-single
//!    charts have rungs below and above, and how many;
//! 2. what placing a stage's battle plan over every rung at once costs:
//!    the sections a stage chart alone has against those its ladder
//!    merged leaves, the rungs below alone (`step-down-v0`'s) and every
//!    rung (`placement-v2`);
//! 3. stages of ten CPUs, one at each level, attacks on, from every Medium
//!    and Hard pick with a ladder (every `DDI_STEP_EVERY`th, 1), under DDI
//!    Normal and Hard, each As picked and Adaptive, seeded
//!    (`DDI_STEP_SEEDS`, 2): by level, where it finished (its rung from
//!    the pick), its steps up and down, how often it goes out and
//!    bounces, its place and its score, against As picked; and how often
//!    a stronger level places above a weaker one;
//! 4. whether playing an easier chart on purpose pays: for each pair of
//!    neighbouring rungs, a perfect play of the easier against plays of
//!    85% and 95% of the harder (the score as earned, DDI's sum).
//!
//! MEASUREMENTS OF CPUS AGAINST CPUS, not of people.

use super::*;
use ddi_room::{Charts, MatchDifficulty};
use ddi_ui::state::room::StageRules;
use std::path::{Path, PathBuf};

fn song_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut here: Vec<PathBuf> = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            song_files(&p, out);
        } else if let Some(ext) = p.extension() {
            let ext = ext.to_string_lossy().to_ascii_lowercase();
            if ["ssc", "sm", "dwi"].contains(&ext.as_str()) {
                here.push(p);
            }
        }
    }
    let rank = |p: &PathBuf| match p
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("ssc") => 0,
        Some("sm") => 1,
        _ => 2,
    };
    if let Some(best) = here.iter().min_by_key(|p| rank(p)) {
        out.push(best.clone());
    }
}

fn read(path: &Path) -> Option<Song> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let bytes = std::fs::read(path).ok()?;
    let text = ddi_library::pack::decode_text(&bytes);
    ddi_library::scan::parse_simfile(&text, &ext).ok()
}

const SLOTS: [ddi_chart::Difficulty; 6] = ddi_chart::Difficulty::ALL;

/// A rung's place from the pick, -4..=4, as an index 0..=8.
fn place_index(level: i8) -> usize {
    (i32::from(level).clamp(-4, 4) + 4) as usize
}

#[derive(Clone, Default)]
struct Tally {
    /// By the stage chart's slot: charts, by rungs below and above (0 to
    /// 4+ each).
    below: [[u32; 5]; 6],
    above: [[u32; 5]; 6],
    /// By slot: the plan's sections alone, merged over the rungs below,
    /// merged over every rung (charts with a ladder only); and the charts
    /// that lost more than a third of their sections merged over every
    /// rung, of those counted.
    sections: [(u64, u64, u64); 6],
    lost_third: [(u32, u32); 6],
    songs: u32,
    songs_with_ladder: u32,
    /// By pick (Medium, Hard), difficulty (Normal, Hard), rule (As
    /// picked, Adaptive) and level.
    levels: [[[[LevelSum; 10]; 2]; 2]; 2],
    stages: [[u32; 2]; 2],
    /// By pick, difficulty and rule: pairs of levels, and those the
    /// stronger placed above the weaker in.
    pairs: [[[(u64, u64); 2]; 2]; 2],
    /// Neighbouring rungs: pairs, and those where a perfect play of the
    /// easier beats an 85% and a 95% play of the harder; the ratios'
    /// sums (perfect easier over 85% and over 95% harder).
    under: (u32, u32, u32, f64, f64),
}

#[derive(Clone, Copy, Default)]
struct LevelSum {
    stages: u32,
    out: u32,
    place: u64,
    score: i64,
    /// Where it finished, -4..=4 from the pick.
    finished: [u32; 9],
    ups: u32,
    downs: u32,
    bounced: u32,
}

impl LevelSum {
    fn add(&mut self, b: &LevelSum) {
        self.stages += b.stages;
        self.out += b.out;
        self.place += b.place;
        self.score += b.score;
        for k in 0..9 {
            self.finished[k] += b.finished[k];
        }
        self.ups += b.ups;
        self.downs += b.downs;
        self.bounced += b.bounced;
    }
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        for s in 0..6 {
            for d in 0..5 {
                self.below[s][d] += o.below[s][d];
                self.above[s][d] += o.above[s][d];
            }
            self.sections[s].0 += o.sections[s].0;
            self.sections[s].1 += o.sections[s].1;
            self.sections[s].2 += o.sections[s].2;
            self.lost_third[s].0 += o.lost_third[s].0;
            self.lost_third[s].1 += o.lost_third[s].1;
        }
        self.songs += o.songs;
        self.songs_with_ladder += o.songs_with_ladder;
        for p in 0..2 {
            for d in 0..2 {
                self.stages[p][d] += o.stages[p][d];
                for r in 0..2 {
                    self.pairs[p][d][r].0 += o.pairs[p][d][r].0;
                    self.pairs[p][d][r].1 += o.pairs[p][d][r].1;
                    for l in 0..10 {
                        let b = o.levels[p][d][r][l];
                        self.levels[p][d][r][l].add(&b);
                    }
                }
            }
        }
        self.under.0 += o.under.0;
        self.under.1 += o.under.1;
        self.under.2 += o.under.2;
        self.under.3 += o.under.3;
        self.under.4 += o.under.4;
    }
}

/// One CPU of a studied stage: its level (from 0), its steps, out, its
/// place from 1 and its score as ranked.
struct Seated {
    level: usize,
    steps: Vec<ddi_engine::ladder::Step>,
    below: usize,
    out: bool,
    place: usize,
    score: i64,
}

/// A stage of ten CPUs, one at each level (seated in turn by the seed),
/// on chart `chart` of `song` under `difficulty` and `charts`, attacks
/// on.
fn stage(
    song: &Song,
    chart: usize,
    difficulty: MatchDifficulty,
    charts: Charts,
    seed: u64,
) -> Option<Vec<Seated>> {
    let rules = StageRules {
        difficulty,
        charts,
        ..StageRules::default()
    };
    let ruleset = stage_ruleset(&rules);
    let turn = seed as usize % 10;
    let levels: Vec<u8> = (0..10).map(|i| ((i + turn) % 10) as u8).collect();
    let setup = StageSetup {
        stage: StageId(seed),
        number: 1,
        of: 1,
        battle: BattlePreset::GaugeAttacksV0,
        rules,
        me: ParticipantId(1),
        targeting: Targeting::Spread,
        cpus: levels
            .iter()
            .enumerate()
            .map(|(i, &level)| CpuSeat {
                participant: ParticipantId(i as u64 + 2),
                ordinal: i as u16 + 1,
                level,
                seed: seed
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add(i as u64 * 7919 + 1),
                name: None,
                skills: None,
                targeting: Targeting::Spread,
            })
            .collect(),
        dev_mines_at_beat: None,
        others: 0,
        watching: false,
        live: None,
    };
    let ladder = stage_ladder(song, chart, &setup);
    let below = ladder.iter().filter(|r| !r.up).count();
    let mut b = Battle::new(setup, song, chart, &ruleset, 0.0, FORFEIT_HORIZON, &ladder).ok()?;
    if let Some(a) = b.attacks.as_mut() {
        a.out_at(b.setup.me, f64::NEG_INFINITY);
    }
    b.play_out_as(Some(Run::Forfeit));
    let entrants = b.entrants_as(None);
    let ranking = match &b.decided {
        Some(r) => r.clone(),
        None => {
            let checkpoints = b
                .attacks
                .as_ref()
                .map_or(&[][..], |a| a.ledger().checkpoints_us());
            SETTLEMENT
                .rank(&entrants, checkpoints, b.settling, b.setup.rules.rank_by())
                .0
        }
    };
    let place_of = |p: ParticipantId| {
        let mut place = 1;
        for g in &ranking {
            if g.iter().any(|f| f.participant == p) {
                return place;
            }
            place += g.len();
        }
        place
    };
    Some(
        b.seats
            .iter()
            .enumerate()
            .map(|(i, s)| Seated {
                level: usize::from(s.seat.level),
                steps: s.opponent.player().steps().to_vec(),
                below,
                out: s.out_at().is_some(),
                place: place_of(s.seat.participant),
                score: b.cpu_detail(i).score.unwrap_or(0),
            })
            .collect(),
    )
}

/// Whether a play bounced: stepped down from a rung, back up to it, and
/// down from it again.
fn bounced(steps: &[ddi_engine::ladder::Step]) -> bool {
    steps.iter().enumerate().any(|(i, a)| {
        !a.up
            && steps[i + 1..].iter().enumerate().any(|(j, b)| {
                b.up
                    && b.rung == a.from
                    && steps[i + 1 + j + 1..].iter().any(|c| !c.up && c.from == a.from)
            })
    })
}

/// DDI's sum for a play of `share` of chart `chart`'s most (its W0s
/// left out: at most one a note, nothing beside a hundred a point).
fn sum_at(song: &Song, chart: usize, share: f64) -> Option<i64> {
    let most = chart_most(song, chart, &stage_ruleset(&StageRules::default()))?;
    let points = (most as f64 * share).round() as i64;
    Some(ddi_engine::rules::ddi::DdiScoreView::of(points, most, 0).score)
}

/// Everything of one song.
fn study_song(song: &Song, bundled: bool, every: usize, seeds: u64, counter: &mut usize) -> Tally {
    let mut t = Tally {
        songs: 1,
        ..Tally::default()
    };
    let judge = stage_ruleset(&StageRules::default()).judge;
    let mut any = false;
    for (index, chart) in song.charts.iter().enumerate() {
        if chart.layout != "dance-single" || chart.derivation.is_some() || chart.notes.is_empty() {
            continue;
        }
        let slot = SLOTS.iter().position(|d| *d == chart.difficulty).unwrap_or(5);
        let ladder = ddi_engine::ladder::Ladder::of(song, index);
        t.below[slot][ladder.below.len().min(4)] += 1;
        t.above[slot][ladder.above.len().min(4)] += 1;
        if ladder.is_empty() {
            continue;
        }
        any = true;
        // Playing down on purpose: the next easier rung played perfectly
        // against the stage's chart played at 85% and 95%.
        if let Some(&easier) = ladder.below.first()
            && let (Some(p), Some(a), Some(b)) = (
                sum_at(song, easier, 1.0),
                sum_at(song, index, 0.85),
                sum_at(song, index, 0.95),
            )
        {
            t.under.0 += 1;
            t.under.1 += u32::from(p > a);
            t.under.2 += u32::from(p > b);
            t.under.3 += p as f64 / a.max(1) as f64;
            t.under.4 += p as f64 / b.max(1) as f64;
        }
        let count = |s: &Song| {
            ddi_engine::battle::schedule(s, index, &judge, 11)
                .iter()
                .filter(|s| s.is_some())
                .count() as u64
        };
        let alone = count(song);
        let below = if ladder.below.is_empty() {
            alone
        } else {
            count(&ddi_engine::ladder::merged(song, index, &ladder.below))
        };
        let all = count(&ddi_engine::ladder::merged(song, index, &ladder.rungs()));
        t.sections[slot].0 += alone;
        t.sections[slot].1 += below;
        t.sections[slot].2 += all;
        if alone > 0 {
            t.lost_third[slot].0 += 1;
            t.lost_third[slot].1 += u32::from(3 * all < 2 * alone);
        }
        let pick = match chart.difficulty {
            ddi_chart::Difficulty::Medium => 0,
            ddi_chart::Difficulty::Hard => 1,
            _ => continue,
        };
        *counter += 1;
        if !bundled && !(*counter - 1).is_multiple_of(every) {
            continue;
        }
        for (d, difficulty) in [MatchDifficulty::Normal, MatchDifficulty::Hard]
            .into_iter()
            .enumerate()
        {
            for seed in 1..=seeds {
                for (r, charts) in [Charts::AsPicked, Charts::Adaptive].into_iter().enumerate() {
                    let Some(seats) = stage(song, index, difficulty, charts, seed) else {
                        continue;
                    };
                    if r == 0 {
                        t.stages[pick][d] += 1;
                    }
                    for a in &seats {
                        for b in &seats {
                            if a.level > b.level {
                                t.pairs[pick][d][r].0 += 1;
                                t.pairs[pick][d][r].1 += u64::from(a.place < b.place);
                            }
                        }
                    }
                    for s in seats {
                        let l = &mut t.levels[pick][d][r][s.level.min(9)];
                        l.stages += 1;
                        l.out += u32::from(s.out);
                        l.place += s.place as u64;
                        l.score += s.score;
                        let rung = s.steps.last().map_or(0, |st| st.rung);
                        l.finished[place_index(ddi_engine::ladder::level_of(s.below, rung))] += 1;
                        l.ups += s.steps.iter().filter(|st| st.up).count() as u32;
                        l.downs += s.steps.iter().filter(|st| !st.up).count() as u32;
                        l.bounced += u32::from(bounced(&s.steps));
                    }
                }
            }
        }
    }
    t.songs_with_ladder = u32::from(any);
    t
}

fn report(name: &str, t: &Tally, started: std::time::Instant) {
    println!("== {name}: {} songs, {} with a ladder", t.songs, t.songs_with_ladder);
    println!(
        "ladders by the stage chart's slot (charts | below 0/1/2/3/4+ | above 0/1/2/3/4+ | plan's sections: alone, below merged, all merged, kept %, charts losing over a third)"
    );
    for (s, d) in SLOTS.iter().enumerate() {
        let (b, a) = (t.below[s], t.above[s]);
        let n: u32 = b.iter().sum();
        if n == 0 {
            continue;
        }
        let (alone, below, all) = t.sections[s];
        let pct = |x: u64| if alone > 0 { 100.0 * x as f64 / alone as f64 } else { 0.0 };
        println!(
            "  {:<10} {n:>5} | {:>4} {:>4} {:>4} {:>4} {:>4} | {:>4} {:>4} {:>4} {:>4} {:>4} | {alone:>6} {below:>6} {all:>6} {:5.1}% {:5.1}% | {}/{}",
            d.name(),
            b[0],
            b[1],
            b[2],
            b[3],
            b[4],
            a[0],
            a[1],
            a[2],
            a[3],
            a[4],
            pct(below),
            pct(all),
            t.lost_third[s].1,
            t.lost_third[s].0,
        );
    }
    let (n, p85, p95, r85, r95) = t.under;
    if n > 0 {
        println!(
            "playing down on purpose: {n} charts; a perfect play of the next easier beats 85% of the chart in {p85} ({:.1}%), 95% in {p95} ({:.1}%); mean ratio {:.3} and {:.3}",
            100.0 * f64::from(p85) / f64::from(n),
            100.0 * f64::from(p95) / f64::from(n),
            r85 / f64::from(n),
            r95 / f64::from(n),
        );
    }
    for (p, pick) in ["Medium", "Hard"].iter().enumerate() {
        for (d, name) in ["Normal", "Hard"].iter().enumerate() {
            println!(
                "picked {pick}, DDI {name}, ten CPUs (one a level), attacks on: {} stages a rule; stronger above weaker: as picked {:.1}%, adaptive {:.1}%",
                t.stages[p][d],
                100.0 * t.pairs[p][d][0].1 as f64 / t.pairs[p][d][0].0.max(1) as f64,
                100.0 * t.pairs[p][d][1].1 as f64 / t.pairs[p][d][1].0.max(1) as f64,
            );
            println!(
                "  level | as picked: out%  place  score     | adaptive: out%  place  score     ups  downs  bounce% | finished -4..+4 %"
            );
            for l in 0..10 {
                let (o, s) = (t.levels[p][d][0][l], t.levels[p][d][1][l]);
                let (on, sn) = (f64::from(o.stages.max(1)), f64::from(s.stages.max(1)));
                let finished: Vec<String> = s
                    .finished
                    .iter()
                    .map(|c| format!("{:4.1}", 100.0 * f64::from(*c) / sn))
                    .collect();
                println!(
                    "  {:>5} | {:5.1} {:5.2} {:9.0} | {:5.1} {:5.2} {:9.0} {:5.2} {:5.2} {:6.1} | {}",
                    l + 1,
                    100.0 * f64::from(o.out) / on,
                    o.place as f64 / on,
                    o.score as f64 / on,
                    100.0 * f64::from(s.out) / sn,
                    s.place as f64 / sn,
                    s.score as f64 / sn,
                    f64::from(s.ups) / sn,
                    f64::from(s.downs) / sn,
                    100.0 * f64::from(s.bounced) / sn,
                    finished.join(" "),
                );
            }
        }
    }
    println!("  ({:.0} s so far)", started.elapsed().as_secs_f64());
}

#[test]
#[ignore = "a study: prints what adaptive charts do over the bundled songs and a local corpus"]
fn adaptive_study() {
    let every: usize = std::env::var("DDI_STEP_EVERY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let seeds: u64 = std::env::var("DDI_STEP_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);
    let started = std::time::Instant::now();
    // The bundled songs, every chart.
    let mut bundled_tally = Tally::default();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs");
    let mut files = Vec::new();
    song_files(&dir, &mut files);
    let mut counter = 0;
    for f in &files {
        if let Some(song) = read(f) {
            let t = study_song(&song, true, every, seeds, &mut counter);
            let (n, p85, p95, r85, r95) = t.under;
            println!(
                "{}: playing down on purpose, {n} charts: a perfect easier beats 85% in {p85}, 95% in {p95}; mean ratios {:.3}, {:.3}",
                song.title,
                r85 / f64::from(n.max(1)),
                r95 / f64::from(n.max(1)),
            );
            bundled_tally.add(&t);
        }
    }
    report("the bundled songs", &bundled_tally, started);
    // A local corpus, in place: aggregates only.
    let Some(dirs) = std::env::var_os("DDI_STEP_CORPUS") else {
        return;
    };
    let mut files = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        song_files(&dir, &mut files);
    }
    files.sort();
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let chunk = files.len().div_ceil(workers).max(1);
    let parts: Vec<(Tally, u32)> = std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|files| {
                scope.spawn(move || {
                    let mut t = Tally::default();
                    let mut unread = 0;
                    let mut counter = 0;
                    for f in files {
                        match read(f) {
                            Some(song) => t.add(&study_song(&song, false, every, seeds, &mut counter)),
                            None => unread += 1,
                        }
                    }
                    (t, unread)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut total = Tally::default();
    let mut unread = 0;
    for (t, u) in &parts {
        total.add(t);
        unread += u;
    }
    println!("({} song files, {unread} unread)", files.len());
    report("the local corpus", &total, started);
}

/// What a stage of 72 CPUs pays for a full ladder when it loads (on
/// Beatucada's Medium: two rungs below, two above): the ladder planned
/// once (`planned_ladder`), each CPU's script for every rung, the plan
/// placed over every rung; against the same stage as picked. Then a stage
/// played out (judging every rung a CPU reaches). Prints times.
#[test]
#[ignore = "a study: prints what a full ladder costs a stage of 72 CPUs"]
fn adaptive_cost_of_72_cpus() {
    let song = super::study::bundled_song("beatucada");
    let chart = song
        .charts
        .iter()
        .position(|c| c.layout == "dance-single" && c.difficulty == ddi_chart::Difficulty::Medium)
        .unwrap();
    for charts in [Charts::AsPicked, Charts::Adaptive] {
        let rules = StageRules {
            charts,
            ..StageRules::default()
        };
        let ruleset = stage_ruleset(&rules);
        let setup = StageSetup {
            stage: StageId(3),
            number: 1,
            of: 1,
            battle: BattlePreset::GaugeAttacksV0,
            rules,
            me: ParticipantId(1),
            targeting: Targeting::Spread,
            cpus: (0..72u64)
                .map(|i| CpuSeat {
                    participant: ParticipantId(i + 2),
                    ordinal: i as u16 + 1,
                    level: (i % 10) as u8,
                    seed: i * 7919 + 1,
                    name: None,
                    skills: None,
                    targeting: Targeting::Spread,
                })
                .collect(),
            dev_mines_at_beat: None,
            others: 0,
            watching: false,
            live: None,
        };
        let t = std::time::Instant::now();
        let ladder = stage_ladder(&song, chart, &setup);
        let planned = t.elapsed().as_secs_f64();
        let mut b = Battle::new(setup, &song, chart, &ruleset, 0.0, FORFEIT_HORIZON, &ladder).unwrap();
        let loaded = t.elapsed().as_secs_f64();
        if let Some(a) = b.attacks.as_mut() {
            a.out_at(b.setup.me, f64::NEG_INFINITY);
        }
        b.play_out_as(Some(Run::Forfeit));
        let played = t.elapsed().as_secs_f64();
        let down = b
            .seats
            .iter()
            .filter(|s| s.opponent.player().steps().iter().any(|st| !st.up))
            .count();
        let up = b
            .seats
            .iter()
            .filter(|s| s.opponent.player().steps().iter().any(|st| st.up))
            .count();
        println!(
            "{charts:?}: {} rungs; ladder planned {:.0} ms, stage loaded {:.0} ms, played out {:.0} ms; of 72, {down} stepped down, {up} up",
            ladder.len(),
            planned * 1e3,
            loaded * 1e3,
            played * 1e3,
        );
    }
}

