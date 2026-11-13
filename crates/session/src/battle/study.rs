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
use std::collections::BTreeMap;

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
    stage_with(song, chart, difficulty, levels, seed, skills, Targeting::Spread).0
}

/// What a stage's attacks came to as a whole (`attacks-v2`'s study).
#[derive(Clone, Copy, Default)]
struct Facts {
    attacks: u32,
    /// The longest run of consecutive safe sections filled against one
    /// recipient.
    longest_run: u32,
    /// The most attacks against one recipient still to come at once
    /// (the one committed and those before it not cleared yet).
    most_ahead: u32,
    /// The most attacked recipient's share of the stage's attacks, and
    /// how much of the safe sections over its attacked span were filled
    /// against it.
    top_share: f64,
    top_filled: f64,
    /// Checkpoints a full bar waited, and launches.
    waited: u32,
    launched: u32,
    drawn: bool,
    /// Seconds from last standing to the decision, where it waited.
    last_standing: Option<f64>,
}

/// [`stage`] with every CPU's policy `targeting`, and the stage's facts.
fn stage_with(
    song: &Song,
    chart: usize,
    difficulty: MatchDifficulty,
    levels: &[u8],
    seed: u64,
    skills: Option<&str>,
    targeting: Targeting,
) -> (Vec<Seat>, Facts) {
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
                targeting,
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
    let facts = facts(&b, ledger);
    let table = ruleset.judge.clone();
    let (lock, window, reach) = (
        table.lock.unwrap_or(0.0),
        table.mine_window_seconds(),
        table.miss_after(),
    );
    let seats = b.seats
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
        .collect();
    (seats, facts)
}

fn facts(b: &Battle, ledger: &ddi_room::attacks::StageBattle) -> Facts {
    let attacks = ledger.attacks();
    let slots = &ledger.schedule().slots;
    // The first safe slot starting at or after `us`.
    let first_from = |us: i64| {
        (slots.partition_point(|s| s.start_us < us)..slots.len()).find(|&j| slots[j].section.is_some())
    };
    let mut by: BTreeMap<ParticipantId, Vec<&ddi_room::attacks::Attack>> = BTreeMap::new();
    for a in attacks {
        by.entry(a.recipient).or_default().push(a);
    }
    // A run: attacks against one recipient each in the very first safe
    // section after the one before it cleared (nothing free between).
    let mut longest_run = 0;
    let mut top = (0usize, 0.0f64);
    for got in by.values_mut() {
        got.sort_by_key(|a| a.slot);
        let mut run = 0u32;
        let mut last: Option<i64> = None;
        for a in got.iter() {
            run = if last.is_some_and(|c| first_from(c) == Some(a.slot)) { run + 1 } else { 1 };
            longest_run = longest_run.max(run);
            last = Some(a.clear_us);
        }
        if got.len() > top.0 {
            // How many attacks back to back would fit over its attacked
            // span, and how many it got.
            let (first, end) = (got[0].slot, got.last().unwrap().slot);
            let mut fit = 0;
            let mut j = Some(first);
            while let Some(k) = j.filter(|&k| k <= end) {
                fit += 1;
                j = slots[k].section.as_ref().and_then(|x| first_from(x.clear_us));
            }
            top = (got.len(), got.len() as f64 / f64::from(fit.max(1)));
        }
    }
    let checkpoints = ledger.checkpoints_us();
    let most_ahead = attacks
        .iter()
        .map(|x| {
            let at = checkpoints[x.checkpoint];
            1 + attacks
                .iter()
                .filter(|o| o.id < x.id && o.recipient == x.recipient && o.clear_us >= at)
                .count() as u32
        })
        .max()
        .unwrap_or(0);
    let (mut waited, mut launched) = (0, 0);
    for s in &b.seats {
        let st = ledger.stats(s.seat.participant);
        waited += st.waited_no_eligible + st.waited_no_safe_section;
        launched += st.launched;
    }
    let (stood, settled) = b.last_standing_span();
    Facts {
        attacks: attacks.len() as u32,
        longest_run,
        most_ahead,
        top_share: top.0 as f64 / attacks.len().max(1) as f64,
        top_filled: top.1,
        waited,
        launched,
        drawn: b.drawn_by_final_attack(),
        last_standing: stood.zip(settled).map(|(a, z)| z - a),
    }
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

/// `attacks-v2`'s study (2026-11-13): what chaining without limit does in
/// fields of 4, 9 and 33 CPUs of mixed levels under DDI Normal and Hard,
/// every CPU on one targeting policy, over the same charts. Run it once a
/// rules version (`DDI_ATTACK_RULES=v1`, none for `attacks-v2`,
/// `v2-gap1` or `v2-cool2` for the limits proposed):
///
/// `DDI_ATTACK_RULES=v1 cargo test --release -p ddi-session --lib
/// chain_pileups -- --ignored --nocapture`
///
/// `DDI_ATTACK_SEEDS` seeds a chart (2), `DDI_CHAIN_SIZES` the fields
/// (`4,9,33`).
#[test]
#[ignore = "a study: prints what unlimited chaining does"]
fn chain_pileups() {
    use MatchDifficulty as D;
    let seeds: u64 = std::env::var("DDI_ATTACK_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);
    let sizes: Vec<usize> = std::env::var("DDI_CHAIN_SIZES")
        .ok()
        .map(|v| v.split(',').filter_map(|n| n.parse().ok()).collect())
        .unwrap_or_else(|| vec![4, 9, 33]);
    let songs: Vec<(Song, usize)> = CHARTS
        .iter()
        .map(|&(id, which)| {
            let song = bundled(id);
            let chart = chart_of(&song, which);
            (song, chart)
        })
        .collect();
    let costs = |d: D| match d {
        D::Easy => (1.0, 2.0),
        D::Normal => (5.0, 10.0),
        D::Hard => (10.0, 20.0),
    };
    const MIX: [u8; 8] = [0, 1, 2, 3, 4, 5, 7, 9];
    println!(
        "== {}: {} charts, {seeds} seeds; fields of mixed levels (1, 2, 3, 4, 5, 6, 8, 10 in turn)",
        attacks::rules().id,
        songs.len()
    );
    for &n in &sizes {
        for d in [D::Normal, D::Hard] {
            for t in Targeting::ALL {
                let mut by_level = [Seat::default(); 10];
                let mut f = Facts::default();
                let (mut stages, mut runs, mut drawn, mut stood, mut stood_s, mut stood_max) =
                    (0u32, 0u64, 0u32, 0u32, 0.0, 0.0f64);
                let (mut longest, mut ahead) = (0u32, 0u32);
                let (mut share, mut filled) = (0.0, 0.0);
                for (song, chart) in &songs {
                    for seed in 1..=seeds {
                        let levels: Vec<u8> =
                            (0..n).map(|i| MIX[(i + seed as usize) % MIX.len()]).collect();
                        let (seats, x) = stage_with(song, *chart, d, &levels, seed, None, t);
                        for (s, &l) in seats.iter().zip(&levels) {
                            by_level[usize::from(l)].add(s);
                        }
                        stages += 1;
                        f.attacks += x.attacks;
                        f.waited += x.waited;
                        f.launched += x.launched;
                        runs += u64::from(x.longest_run);
                        longest = longest.max(x.longest_run);
                        ahead = ahead.max(x.most_ahead);
                        share += x.top_share;
                        filled += x.top_filled;
                        drawn += u32::from(x.drawn);
                        if let Some(sec) = x.last_standing {
                            stood += 1;
                            stood_s += sec;
                            stood_max = stood_max.max(sec);
                        }
                    }
                }
                let k = f64::from(stages.max(1));
                println!(
                    "{n:>2} {d:?} {t:?}: attacks/stage {:5.1} | longest run of sections against one: mean {:4.1} max {:2} \
                     | most at once {:2} | top recipient {:4.1}% of attacks, {:4.1}% of its span filled \
                     | blocked {:4.1}% of checkpoints with a full bar | drawn by a final attack {:4.1}% \
                     | last standing {:4.1}% of stages, mean {:4.1} s, max {:4.1} s",
                    f64::from(f.attacks) / k,
                    runs as f64 / k,
                    longest,
                    ahead,
                    100.0 * share / k,
                    100.0 * filled / k,
                    100.0 * f64::from(f.waited) / f64::from((f.waited + f.launched).max(1)),
                    100.0 * f64::from(drawn) / k,
                    100.0 * f64::from(stood) / k,
                    stood_s / f64::from(stood.max(1)),
                    stood_max,
                );
                let (mine, miss) = costs(d);
                let words: Vec<String> = MIX
                    .iter()
                    .map(|&l| {
                        let s = &by_level[usize::from(l)];
                        let seats = f64::from(s.seats.max(1));
                        format!(
                            "L{} recv {:.1} life {:.0} KO {:.0}%",
                            l + 1,
                            f64::from(s.received) / seats,
                            (f64::from(s.stepped) * mine + f64::from(s.locked_missed) * miss) / seats,
                            100.0 * f64::from(s.knocked_out) / seats,
                        )
                    })
                    .collect();
                println!("     by level (a seat): {}", words.join(" | "));
                // A CPU never steps on more shocks than it was sent.
                for s in &by_level {
                    assert!(s.stepped <= 8 * s.received, "double counting");
                }
            }
        }
    }
}
