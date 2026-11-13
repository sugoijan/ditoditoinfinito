//! Opt-in study: attack rates in a practice stage of CPUs under DDI with
//! the Attacks rule On (`docs/plans/multiplayer-prototype-progress.md`,
//! the DDI ruleset's round 3; the M8b study's charts).
//!
//! `cargo test --release -p ddi-session --lib attack_rates -- --ignored
//! --nocapture` plays each of the bundled charts below as a stage of
//! eight CPUs, the local player gone at its start (a stage left at its
//! prompt: the CPUs and the attacks between them played out a checkpoint
//! at a time, as `forfeit` does), with the stage's fail on (an empty life
//! is out), under each difficulty; once with every CPU at the same level
//! (each of the ten), once with a field of mixed levels. It prints
//! aggregates only.
//!
//! What it counts, a CPU: the power its timing earned (W0 and W1 two, W2
//! one, the cap aside) as bars a minute while it was in; the attacks it
//! launched; and, an attack received: the shocks stepped on, the notes
//! that passed missed in a lane a shock had locked, the life those cost
//! by the difficulty (a mine's and a Miss's), and whether its recipient
//! went out before the attack's last shock and its lock were over.
//!
//! Last, by the CPUs' Evasion (the eighth skill axis, round 4 of the DDI
//! ruleset): eight CPUs of one level, all at Evasion 0.5, 1 or 2, under
//! Normal. `DDI_ATTACK_ONLY=evasion` runs that table alone.
//!
//! MEASUREMENTS OF CPUS AGAINST CPUS, not of people: the CPUs' response
//! to shocks is `MINE_RESPONSE_V1`'s (the levels' `MINE_RESPONSE_V0`,
//! invented, bent by Evasion), a model of a CPU.

use super::*;
use ddi_engine::battle::HazardState;
use ddi_engine::judge::NoteState;
use ddi_room::MatchDifficulty;
use ddi_ui::state::room::StageRules;

/// The M8b study's charts: a bundled song's folder and its chart.
const CHARTS: [(&str, usize); 6] = [
    ("beatucada", 4),
    ("beatucada", 5),
    ("sweeteners", usize::MAX),
    ("into-my-dream", usize::MAX - 1),
    ("some-things-must", usize::MAX),
    ("some-things-must", usize::MAX - 2),
];

/// A bundled song by its folder (for the other studies).
pub(super) fn bundled_song(id: &str) -> Song {
    bundled(id)
}

fn bundled(id: &str) -> Song {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/songs")
        .join(id);
    let path = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "ssc" || e == "sm"))
        .unwrap();
    let ext = path.extension().unwrap().to_string_lossy().into_owned();
    ddi_library::scan::parse_simfile(&std::fs::read_to_string(path).unwrap(), &ext).unwrap()
}

/// The chart a [`CHARTS`] entry names: an index, or the song's Hard
/// (`MAX`), Medium (`MAX - 1`) or Challenge (`MAX - 2`) dance-single
/// chart as written.
fn chart_of(song: &Song, which: usize) -> usize {
    use ddi_chart::Difficulty as D;
    let want = match which {
        usize::MAX => D::Hard,
        w if w == usize::MAX - 1 => D::Medium,
        w if w == usize::MAX - 2 => D::Challenge,
        index => return index,
    };
    song.charts
        .iter()
        .position(|c| {
            c.layout == "dance-single" && c.derivation.is_none() && c.difficulty == want
        })
        .expect("the chart")
}

#[derive(Clone, Copy, Default)]
struct Seat {
    seats: u32,
    /// Minutes in the stage (to its going out, or the chart's end).
    minutes: f64,
    earned: f64,
    launched: u32,
    received: u32,
    stepped: u32,
    locked_missed: u32,
    knocked_out: u32,
    out: u32,
    percent: f64,
    /// Checkpoints its full bar waited (no one to attack, no safe section).
    waited: u32,
}

impl Seat {
    fn add(&mut self, o: &Seat) {
        self.seats += o.seats;
        self.minutes += o.minutes;
        self.earned += o.earned;
        self.launched += o.launched;
        self.received += o.received;
        self.stepped += o.stepped;
        self.locked_missed += o.locked_missed;
        self.knocked_out += o.knocked_out;
        self.out += o.out;
        self.percent += o.percent;
        self.waited += o.waited;
    }
}

/// One stage of CPUs at `levels` (engine indices) on chart `chart` of
/// `song` under `difficulty`, seeded: each seat's counts, in seat order.
fn stage(
    song: &Song,
    chart: usize,
    difficulty: MatchDifficulty,
    levels: &[u8],
    seed: u64,
    skills: Option<&str>,
) -> Vec<Seat> {
    let rules = StageRules {
        difficulty,
        ..StageRules::default()
    };
    let ruleset = stage_ruleset(&rules);
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
                skills: skills.map(str::to_string),
                targeting: Targeting::Spread,
            })
            .collect(),
        dev_mines_at_beat: None,
        others: 0,
        watching: false,
        live: None,
    };
    let ladder = super::stage_ladder(song, chart, &setup);
    let mut b = Battle::new(setup, song, chart, &ruleset, 0.0, FORFEIT_HORIZON, &ladder).unwrap();
    if let Some(a) = b.attacks.as_mut() {
        a.out_at(b.setup.me, f64::NEG_INFINITY);
    }
    b.play_out_as(Some(Run::Forfeit));
    let ledger = b.attacks.as_ref().unwrap().ledger();
    let table = ruleset.judge.clone();
    let (lock, window, reach) = (
        table.lock.unwrap_or(0.0),
        table.mine_window_seconds(),
        table.miss_after(),
    );
    b.seats
        .iter()
        .map(|s| {
            let p = s.opponent.player();
            let id = s.seat.participant;
            let out_at = s.opponent.out_at();
            let end = out_at.unwrap_or_else(|| p.end_time()).max(0.0);
            let tally = p.tally();
            let stats = ledger.stats(id);
            // Each shock it stepped on, when (its feed's moments).
            let hits: Vec<(u8, f64)> = s
                .feed
                .moments()
                .iter()
                .filter(|m| m.kind == feed::MomentKind::ShockHit)
                .map(|m| (m.lane, m.at))
                .collect();
            // Notes that passed, missed, in a lane while a shock locked it.
            let locked_missed = p
                .judge()
                .notes()
                .iter()
                .filter(|n| n.state == NoteState::Missed)
                .filter(|n| {
                    hits.iter().any(|&(lane, at)| {
                        lane == n.note.lane && n.seconds > at - reach && n.seconds < at + lock + reach
                    })
                })
                .count() as u32;
            let stepped = p.hazards().map_or(0, |h| {
                h.mines()
                    .iter()
                    .filter(|m| m.state == HazardState::Hit)
                    .count() as u32
            });
            // Out while an attack against it ran: from its first shock's
            // window to its last one's lock.
            let knocked_out = u32::from(out_at.is_some_and(|out| {
                ledger.attacks().iter().any(|a| {
                    a.recipient == id
                        && a.cells.first().is_some_and(|c| {
                            out >= c.at_us as f64 / 1e6 - window
                        })
                        && a.cells.last().is_some_and(|c| {
                            out <= c.at_us as f64 / 1e6 + window + lock + reach
                        })
                })
            }));
            Seat {
                seats: 1,
                minutes: end / 60.0,
                earned: f64::from(
                    2 * tally.taps[ddi_engine::rules::Judgement::W1.index()]
                        + tally.taps[ddi_engine::rules::Judgement::W2.index()],
                ),
                launched: stats.launched,
                received: stats.received,
                stepped,
                locked_missed,
                knocked_out,
                out: u32::from(out_at.is_some()),
                percent: accuracy(&s.opponent.score()),
                waited: stats.waited_no_eligible + stats.waited_no_safe_section,
            }
        })
        .collect()
}

#[test]
#[ignore = "a study: prints the attack rates over the bundled charts"]
fn attack_rates() {
    use MatchDifficulty as D;
    let seeds: u64 = std::env::var("DDI_ATTACK_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let songs: Vec<(Song, usize)> = CHARTS
        .iter()
        .map(|&(id, which)| {
            let song = bundled(id);
            let chart = chart_of(&song, which);
            (song, chart)
        })
        .collect();
    // What a mine and a Miss cost, life points, by the difficulty.
    let costs = |d: D| match d {
        D::Easy => (1.0, 2.0),
        D::Normal => (5.0, 10.0),
        D::Hard => (10.0, 20.0),
    };
    let line = |name: &str, d: D, s: &Seat, stages: u32| {
        let n = f64::from(s.seats.max(1));
        let received = f64::from(s.received.max(1));
        let (mine, miss) = costs(d);
        println!(
            "{name:<22} bars/min {:5.2} | launched/seat {:5.2} /min {:5.2} | received/seat {:5.2} | \
             a received attack: stepped {:4.2} of 8, missed in locks {:4.2}, life {:5.1} | \
             knocked out by one {:5.1}% of attacks | out {:5.1}% | percent {:5.1} | \
             waited/seat {:5.2} | {stages} stages",
            s.earned / 100.0 / s.minutes.max(1e-9),
            f64::from(s.launched) / n,
            f64::from(s.launched) / s.minutes.max(1e-9),
            f64::from(s.received) / n,
            f64::from(s.stepped) / received,
            f64::from(s.locked_missed) / received,
            (f64::from(s.stepped) * mine + f64::from(s.locked_missed) * miss) / received,
            100.0 * f64::from(s.knocked_out) / received,
            100.0 * f64::from(s.out) / n,
            100.0 * s.percent / n,
            f64::from(s.waited) / n,
        );
    };
    let only_evasion = std::env::var("DDI_ATTACK_ONLY").is_ok_and(|v| v == "evasion");
    for d in [D::Normal, D::Easy, D::Hard].into_iter().filter(|_| !only_evasion) {
        println!("== DDI {d:?}: eight CPUs at one level, {} charts, {seeds} seeds", songs.len());
        for level in 0..10u8 {
            let mut sum = Seat::default();
            let mut stages = 0;
            for (song, chart) in &songs {
                for seed in 1..=seeds {
                    for s in stage(song, *chart, d, &[level; 8], seed, None) {
                        sum.add(&s);
                    }
                    stages += 1;
                }
            }
            line(&format!("level {:>2}", level + 1), d, &sum, stages);
        }
        println!("== DDI {d:?}: a mixed field (levels 1, 2, 3, 4, 5, 6, 8, 10), by level");
        let field = [0u8, 1, 2, 3, 4, 5, 7, 9];
        let mut by_level = vec![Seat::default(); field.len()];
        let mut stages = 0;
        let mut total = Seat::default();
        for (song, chart) in &songs {
            for seed in 1..=seeds {
                // The seats in turn, so that no level always has a seat's
                // place in the launch order.
                let turn = (seed as usize) % field.len();
                let levels: Vec<u8> = (0..field.len())
                    .map(|i| field[(i + turn) % field.len()])
                    .collect();
                for (i, s) in stage(song, *chart, d, &levels, seed, None).iter().enumerate() {
                    by_level[(i + turn) % field.len()].add(s);
                    total.add(s);
                }
                stages += 1;
            }
        }
        for (i, s) in by_level.iter().enumerate() {
            line(&format!("mixed, level {:>2}", field[i] + 1), d, s, stages);
        }
        line("mixed, everyone", d, &total, stages);
        println!(
            "  a mixed stage: {:.1} attacks launched, {:.1} checkpoints waited",
            f64::from(total.launched) / f64::from(stages.max(1)),
            f64::from(total.waited) / f64::from(stages.max(1))
        );
    }
    // By Evasion: eight CPUs of one level, all at one Evasion.
    println!(
        "== DDI Normal: eight CPUs at one level and one Evasion, {} charts, {seeds} seeds",
        songs.len()
    );
    for level in 0..10u8 {
        for evasion in ["0.5", "1", "2"] {
            let skills = format!("1,1,1,1,1,1,1,{evasion}");
            let mut sum = Seat::default();
            let mut stages = 0;
            for (song, chart) in &songs {
                for seed in 1..=seeds {
                    for s in stage(song, *chart, D::Normal, &[level; 8], seed, Some(&skills)) {
                        sum.add(&s);
                    }
                    stages += 1;
                }
            }
            line(
                &format!("level {:>2} evasion {evasion:<3}", level + 1),
                D::Normal,
                &sum,
                stages,
            );
        }
    }
    if only_evasion {
        return;
    }
    // Per chart, Normal, a mixed field: how long, how many attacks.
    println!("== per chart, DDI Normal, the mixed field");
    for (song, chart) in &songs {
        let c = &song.charts[*chart];
        let mut total = Seat::default();
        for seed in 1..=seeds {
            for s in stage(song, *chart, D::Normal, &[0, 1, 2, 3, 4, 5, 7, 9], seed, None) {
                total.add(&s);
            }
        }
        println!(
            "{:?} {} ({} notes): {:.1} attacks a stage, {:.2} launched a minute a seat in, \
             {:.1} checkpoints waited a stage",
            c.difficulty,
            c.meter,
            c.notes.len(),
            f64::from(total.launched) / seeds as f64,
            f64::from(total.launched) / total.minutes.max(1e-9),
            f64::from(total.waited) / seeds as f64,
        );
    }
}
