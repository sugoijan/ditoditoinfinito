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
use ddi_room::{AttacksAtOnce, MatchDifficulty};
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
    stage_with(song, chart, difficulty, levels, seed, skills, Targeting::Spread, AttacksAtOnce::Three).0
}

/// What a stage's attacks came to as a whole (`attacks-v2`'s study).
#[derive(Clone, Default)]
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
    /// Each attack's landing: how stale it is (`attacks-v3`'s study).
    landings: Vec<Landing>,
    /// The sender with the most launches: how many different competitors
    /// it hit.
    top_sender_hit: u32,
    /// Checkpoints a full bar waited for room within the window.
    waited_window: u32,
}

/// An attack's landing: seconds from its launch (its checkpoint) to its
/// first shock and to its last, and whether its reason for the choice
/// still held at its first shock (Leader: the recipient still the leader
/// among the others still in; Retaliate, where it answered its last
/// attacker: nothing from anyone else landed on the sender since; Spread:
/// none).
#[derive(Clone, Copy)]
struct Landing {
    first_s: f64,
    last_s: f64,
    held: Option<bool>,
}

/// [`stage`] with every CPU's policy `targeting` and the match's Attacks
/// at once, and the stage's facts.
#[allow(clippy::too_many_arguments)]
fn stage_with(
    song: &Song,
    chart: usize,
    difficulty: MatchDifficulty,
    levels: &[u8],
    seed: u64,
    skills: Option<&str>,
    targeting: Targeting,
    at_once: AttacksAtOnce,
) -> (Vec<Seat>, Facts) {
    let rules = StageRules {
        difficulty,
        at_once,
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
    let facts = facts(&b, ledger, targeting);
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
                waited: stats.waited(),
            }
        })
        .collect();
    (seats, facts)
}

fn facts(b: &Battle, ledger: &ddi_room::attacks::StageBattle, targeting: Targeting) -> Facts {
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
    let (mut waited, mut launched, mut waited_window) = (0, 0, 0);
    let mut top_sender = (0u32, ParticipantId(0));
    for s in &b.seats {
        let st = ledger.stats(s.seat.participant);
        waited += st.waited();
        waited_window += st.waited_no_room_in_window;
        launched += st.launched;
        if st.launched > top_sender.0 {
            top_sender = (st.launched, s.seat.participant);
        }
    }
    let top_sender_hit = attacks
        .iter()
        .filter(|a| a.sender == top_sender.1)
        .map(|a| a.recipient)
        .collect::<std::collections::BTreeSet<_>>()
        .len() as u32;
    // Each CPU's life and whether it is still in at a song second.
    let seat_of = |p: ParticipantId| b.seats.iter().find(|s| s.seat.participant == p);
    let life = |p: ParticipantId, t: f64| -> Option<f32> {
        let s = seat_of(p)?;
        if s.opponent.out_at().is_some_and(|o| o <= t) {
            return None;
        }
        Some(s.opponent.player().life_at(t))
    };
    let landings = attacks
        .iter()
        .map(|a| {
            let launch = checkpoints[a.checkpoint];
            let first = a.cells.first().map_or(launch, |c| c.at_us);
            let last = a.cells.last().map_or(launch, |c| c.at_us);
            let t = first as f64 / 1e6;
            let held = match targeting {
                Targeting::Spread => None,
                Targeting::Leader => Some(life(a.recipient, t).is_some_and(|mine| {
                    b.seats
                        .iter()
                        .map(|s| s.seat.participant)
                        .filter(|&p| p != a.recipient && p != a.sender)
                        .all(|p| life(p, t).is_none_or(|l| l <= mine))
                })),
                Targeting::Retaliate => {
                    // An answer: the last attack committed against the
                    // sender before this launch came from its recipient.
                    let answered = attacks
                        .iter()
                        .filter(|o| o.recipient == a.sender && o.checkpoint < a.checkpoint)
                        .max_by_key(|o| (o.checkpoint, o.id))
                        .is_some_and(|o| o.sender == a.recipient);
                    answered.then(|| {
                        !attacks.iter().any(|o| {
                            o.recipient == a.sender
                                && o.sender != a.recipient
                                && o.cells
                                    .first()
                                    .is_some_and(|c| c.at_us > launch && c.at_us <= first)
                        })
                    })
                }
            };
            Landing {
                first_s: (first - launch) as f64 / 1e6,
                last_s: (last - launch) as f64 / 1e6,
                held,
            }
        })
        .collect();
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
        landings,
        top_sender_hit,
        waited_window,
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
                        let (seats, x) =
                            stage_with(song, *chart, d, &levels, seed, None, t, AttacksAtOnce::Three);
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

/// One set of rules the window study compares: its name, the rules, and
/// the match's Attacks at once.
fn window_configs() -> Vec<(String, ddi_room::attacks::AttackRules, AttacksAtOnce)> {
    use ddi_room::attacks::{ATTACKS_V2, ATTACKS_V3};
    let w = |s: i64, id: &'static str| ddi_room::attacks::AttackRules {
        id,
        window_us: s * 1_000_000,
        ..ATTACKS_V3
    };
    let mut v = vec![("attacks-v2".to_string(), ATTACKS_V2, AttacksAtOnce::NoLimit)];
    for a in AttacksAtOnce::ALL {
        v.push((format!("v3 15 s, at once {a:?}"), ATTACKS_V3, a));
    }
    v.push(("v3 10 s, no limit".into(), w(10, "attacks-v3-w10"), AttacksAtOnce::NoLimit));
    v.push(("v3 20 s, no limit".into(), w(20, "attacks-v3-w20"), AttacksAtOnce::NoLimit));
    v
}

/// Median, 90th percentile and the most of `v` (sorted in place).
fn spread_of(v: &mut [f64]) -> (f64, f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    (at(0.5), at(0.9), v[v.len() - 1])
}

/// `attacks-v3`'s study (2026-11-13): `chain_pileups`' fields and charts
/// under `attacks-v2` and under `attacks-v3` with Attacks at once at 1,
/// 2, 3 and No limit (the 15 s window), and the window alone at 10 and
/// 20 s: what each does to pile-ups, and how stale an attack is when it
/// lands (its launch to its first and last shock; whether Leader's
/// recipient is still the leader then, and whether Retaliate's answer
/// still answers the last one to hit the sender).
///
/// `cargo test --release -p ddi-session --lib attack_window -- --ignored
/// --nocapture`; `DDI_ATTACK_SEEDS` seeds a chart (2), `DDI_CHAIN_SIZES`
/// the fields (`4,9,33`), `DDI_WINDOW_CONFIGS` which rules by their place
/// in the list (all), `DDI_WINDOW_DIFFICULTIES` (`normal,hard`).
#[test]
#[ignore = "a study: prints what the window and Attacks at once do"]
fn attack_window() {
    use MatchDifficulty as D;
    let seeds: u64 = std::env::var("DDI_ATTACK_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);
    let sizes: Vec<usize> = std::env::var("DDI_CHAIN_SIZES")
        .ok()
        .map(|v| v.split(',').filter_map(|n| n.parse().ok()).collect())
        .unwrap_or_else(|| vec![4, 9, 33]);
    let only: Option<Vec<usize>> = std::env::var("DDI_WINDOW_CONFIGS")
        .ok()
        .map(|v| v.split(',').filter_map(|n| n.parse().ok()).collect());
    let difficulties: Vec<D> = match std::env::var("DDI_WINDOW_DIFFICULTIES").as_deref() {
        Ok("normal") => vec![D::Normal],
        Ok("hard") => vec![D::Hard],
        _ => vec![D::Normal, D::Hard],
    };
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
    for (ci, (name, rules, at_once)) in window_configs().into_iter().enumerate() {
        if only.as_ref().is_some_and(|o| !o.contains(&ci)) {
            continue;
        }
        attacks::RULES_OVERRIDE.with(|c| c.set(Some(rules)));
        println!("== [{ci}] {name}: {} charts, {seeds} seeds; mixed levels", songs.len());
        for &n in &sizes {
            for &d in &difficulties {
                for t in Targeting::ALL {
                    let mut by_level = [Seat::default(); 10];
                    let (mut stages, mut attacks_n, mut waited, mut launched, mut waited_window) =
                        (0u32, 0u32, 0u32, 0u32, 0u32);
                    let (mut ahead, mut share, mut filled, mut hit) = (0u32, 0.0, 0.0, 0u32);
                    let (mut drawn, mut stood, mut stood_s) = (0u32, 0u32, 0.0);
                    let (mut firsts, mut lasts) = (Vec::new(), Vec::new());
                    let (mut held, mut asked) = (0u32, 0u32);
                    for (song, chart) in &songs {
                        for seed in 1..=seeds {
                            let levels: Vec<u8> =
                                (0..n).map(|i| MIX[(i + seed as usize) % MIX.len()]).collect();
                            let (seats, x) =
                                stage_with(song, *chart, d, &levels, seed, None, t, at_once);
                            for (s, &l) in seats.iter().zip(&levels) {
                                by_level[usize::from(l)].add(s);
                            }
                            stages += 1;
                            attacks_n += x.attacks;
                            waited += x.waited;
                            waited_window += x.waited_window;
                            launched += x.launched;
                            ahead = ahead.max(x.most_ahead);
                            share += x.top_share;
                            filled += x.top_filled;
                            hit += x.top_sender_hit;
                            drawn += u32::from(x.drawn);
                            if let Some(sec) = x.last_standing {
                                stood += 1;
                                stood_s += sec;
                            }
                            for l in &x.landings {
                                firsts.push(l.first_s);
                                lasts.push(l.last_s);
                                if let Some(h) = l.held {
                                    asked += 1;
                                    held += u32::from(h);
                                }
                            }
                        }
                    }
                    let k = f64::from(stages.max(1));
                    let (f50, f90, fmax) = spread_of(&mut firsts);
                    let (l50, l90, lmax) = spread_of(&mut lasts);
                    let held_words = match t {
                        Targeting::Spread => String::new(),
                        _ => format!(
                            " | reason held {:4.1}% of {asked}",
                            100.0 * f64::from(held) / f64::from(asked.max(1))
                        ),
                    };
                    println!(
                        "{n:>2} {d:?} {t:?}: attacks/stage {:5.1} | most at once {:2} | top recipient {:4.1}% \
                         of attacks, {:4.1}% of its span | top sender hit {:4.1} | blocked {:4.1}% \
                         (window {:4.1}%) | drawn {:4.1}% | last standing {:4.1}% mean {:4.1} s \
                         | to first shock {:4.1}/{:4.1}/{:4.1} s, to last {:4.1}/{:4.1}/{:4.1} s{held_words}",
                        f64::from(attacks_n) / k,
                        ahead,
                        100.0 * share / k,
                        100.0 * filled / k,
                        f64::from(hit) / k,
                        100.0 * f64::from(waited) / f64::from((waited + launched).max(1)),
                        100.0 * f64::from(waited_window) / f64::from((waited + launched).max(1)),
                        100.0 * f64::from(drawn) / k,
                        100.0 * f64::from(stood) / k,
                        stood_s / f64::from(stood.max(1)),
                        f50,
                        f90,
                        fmax,
                        l50,
                        l90,
                        lmax,
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
                                (f64::from(s.stepped) * mine + f64::from(s.locked_missed) * miss)
                                    / seats,
                                100.0 * f64::from(s.knocked_out) / seats,
                            )
                        })
                        .collect();
                    println!("     by level (a seat): {}", words.join(" | "));
                    for s in &by_level {
                        assert!(s.stepped <= 8 * s.received, "double counting");
                    }
                }
            }
        }
    }
    attacks::RULES_OVERRIDE.with(|c| c.set(None));
}

/// How many attacks fit in the window on a chart: from each checkpoint,
/// a recipient with nothing placed against them reading 2 s ahead (the
/// practice lead: `practice_lead_us`), the attacks that could be placed
/// back to back (each in the first safe section from where the one
/// before it clears) starting within 10, 15 and 20 s. Over every bundled
/// dance-single chart, and the charts under `DDI_WINDOW_CORPUS`
/// (folders, `:` between; read in place, aggregates only, at most
/// `DDI_WINDOW_MAX` charts, 600), by tempo.
///
/// `DDI_WINDOW_CORPUS=… cargo test --release -p ddi-session --lib
/// window_fit -- --ignored --nocapture`
#[test]
#[ignore = "a study: prints how many attacks fit in the window"]
fn window_fit() {
    let bundled_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        // A song folder: its .ssc, else its .sm.
        let ssc = entries.iter().find(|p| p.extension().is_some_and(|e| e == "ssc"));
        let sm = entries.iter().find(|p| p.extension().is_some_and(|e| e == "sm"));
        if let Some(p) = ssc.or(sm) {
            out.push(p.clone());
            return;
        }
        for p in entries.iter().filter(|p| p.is_dir()) {
            walk(p, out);
        }
    }
    walk(&bundled_dir, &mut files);
    let bundled_n = files.len();
    if let Ok(dirs) = std::env::var("DDI_WINDOW_CORPUS") {
        for d in dirs.split(':') {
            walk(std::path::Path::new(d), &mut files);
        }
    }
    let max: usize = std::env::var("DDI_WINDOW_MAX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);
    let rules = ddi_room::attacks::ATTACKS_V3;
    let judge = stage_ruleset(&StageRules::default()).judge;
    let lead = practice_lead_us(attacks::micros(FORFEIT_HORIZON));
    let windows = [10i64, 15, 20];
    // (bundled or local, tempo band) → for each window: the fits seen.
    let mut fits: BTreeMap<(bool, usize), Vec<Vec<f64>>> = BTreeMap::new();
    let mut charts = 0usize;
    let bands = ["under 120 BPM", "120 to 160 BPM", "over 160 BPM"];
    for (i, path) in files.iter().enumerate() {
        if charts >= max {
            break;
        }
        let local = i >= bundled_n;
        let Ok(text) = std::fs::read(path) else { continue };
        let text = String::from_utf8_lossy(&text);
        let ext = path.extension().unwrap().to_string_lossy().into_owned();
        let Ok(song) = ddi_library::scan::parse_simfile(&text, &ext) else {
            continue;
        };
        for (c, chart) in song.charts.iter().enumerate() {
            if chart.layout != "dance-single" || chart.derivation.is_some() || chart.notes.len() < 50 {
                continue;
            }
            if charts >= max {
                break;
            }
            charts += 1;
            let timing = chart.timing(&song);
            let (a, z) = (chart.notes[0].tick, chart.notes[chart.notes.len() - 1].tick);
            let secs = timing.seconds_at(z) - timing.seconds_at(a);
            let beats = (z.0 - a.0) as f64 / ddi_chart::TICKS_PER_BEAT as f64;
            let bpm = if secs > 0.0 { beats / secs * 60.0 } else { 0.0 };
            let band = if bpm < 120.0 { 0 } else if bpm <= 160.0 { 1 } else { 2 };
            let cps = ddi_ui::state::battle_plan::checkpoints(&song, c, &rules);
            let schedule = ddi_ui::state::battle_plan::schedule_with(
                &song,
                c,
                &judge,
                1,
                &ddi_engine::battle::PLACEMENT,
                &rules,
            );
            let slots = &schedule.slots;
            let entry = fits
                .entry((local, band))
                .or_insert_with(|| vec![Vec::new(); windows.len()]);
            for (k, &at) in cps.iter().enumerate() {
                let next = cps.get(k + 1).copied().unwrap_or(at + 1);
                for (wi, &w) in windows.iter().enumerate() {
                    let latest = at + w * 1_000_000;
                    let mut from = (at + lead).max(next);
                    let mut n = 0;
                    loop {
                        let j = slots.partition_point(|s| s.start_us < from);
                        let Some(j) = (j..slots.len())
                            .take_while(|&j| slots[j].start_us <= latest)
                            .find(|&j| slots[j].section.is_some())
                        else {
                            break;
                        };
                        n += 1;
                        from = slots[j].section.as_ref().unwrap().clear_us;
                    }
                    entry[wi].push(f64::from(n));
                }
            }
        }
    }
    println!(
        "== attacks that fit back to back in the window from a checkpoint (lead {:.2} s), {charts} charts",
        lead as f64 / 1e6
    );
    for ((local, band), per) in &mut fits {
        let words: Vec<String> = windows
            .iter()
            .zip(per.iter_mut())
            .map(|(w, v)| {
                let none = v.iter().filter(|&&x| x == 0.0).count() as f64 / v.len().max(1) as f64;
                let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
                let (m, p90, most) = spread_of(v);
                let mut sorted = v.clone();
                sorted.sort_by(f64::total_cmp);
                let p10 = sorted.get(sorted.len() / 10).copied().unwrap_or(0.0);
                format!(
                    "{w:>2} s: none {:4.1}%, mean {mean:3.1}, p10/median/p90/most {p10:.0}/{m:.0}/{p90:.0}/{most:.0}",
                    100.0 * none
                )
            })
            .collect();
        println!(
            "{} {:<15} ({} checkpoints): {}",
            if *local { "local  " } else { "bundled" },
            bands[*band],
            per[0].len(),
            words.join(" | ")
        );
    }
}
