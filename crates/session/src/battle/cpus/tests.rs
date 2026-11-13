//! The proof that an online room's CPUs play alike on every device
//! (`ddi_room::live::cpus`, `online-cpus-v0`): a closed loop of CPUs,
//! their reports, the room's ledger and its commitments back, run with
//! very different frame rates and network delays, gives the same reports,
//! ends, shocks and plays every time, and the same as the recorded
//! fixture (`fixture.txt`), which the same test checks built for the
//! browser's architecture too:
//!
//! ```text
//! CARGO_TARGET_WASM32_WASIP1_RUNNER="node target/scratch/mp/ocpu/wasi-run.mjs" \
//!   cargo test -p ddi-session --target wasm32-wasip1 --lib cpus::tests -- --test-threads=1
//! ```
//!
//! (`wasi-run.mjs` runs a WASI module under Node's `node:wasi`; the web
//! build is wasm32 with the same floating point and the same code: the
//! song and the fixture are compiled in, no file is read.)

use super::*;
use ddi_engine::cpu::PlannedRung;
use ddi_room::attacks::{
    AttackEvent, POWER_CLASSES_V0, Report, RosterSeat, StageBattle, StageRoster, Targeting, ATTACKS,
};
use ddi_room::live::cpus::{ONLINE_CPUS_V0, report_digest};
use ddi_room::live::LIVE_V2;
use ddi_room::{Charts, MatchDifficulty, PointsBias};
use ddi_ui::state::room::{CpuSeat, StageRules};
use std::fmt::Write as _;

const SONG: &str = include_str!("../../../../../assets/songs/beatucada/Beatucada.ssc");
const FIXTURE: &str = include_str!("fixture.txt");

/// Two stages: Hard timing, out at zero life; Hard timing, a step down
/// at zero life (the ladder), each with CPUs of several levels and
/// personalities, attacks among them.
fn scenarios() -> Vec<(StageRules, Vec<CpuSeat>)> {
    let seat = |n: u64, level: u8, skills: Option<&str>, targeting: Targeting| CpuSeat {
        participant: ParticipantId(100 + n),
        ordinal: n as u16,
        level,
        seed: 0x5EED_0000 + n * 7_919,
        name: None,
        skills: skills.map(str::to_string),
        targeting,
    };
    let rules = |difficulty, charts| StageRules {
        ruleset: "ddi".into(),
        difficulty,
        boos: false,
        points: PointsBias::Strong,
        charts,
        changed: false,
        play_changed: false,
    };
    vec![
        (
            rules(MatchDifficulty::Hard, Charts::AsPicked),
            vec![
                seat(1, 0, Some("reckless"), Targeting::Spread),
                seat(2, 4, Some("stamina"), Targeting::Leader),
                seat(3, 7, Some("evasive"), Targeting::Retaliate),
                seat(4, 9, Some("1.4,0.7,1,1.2,0.8,1,1.3,0.6"), Targeting::Spread),
            ],
        ),
        (
            rules(MatchDifficulty::Hard, Charts::Adaptive),
            vec![
                seat(5, 2, Some("reckless"), Targeting::Spread),
                seat(6, 6, None, Targeting::Leader),
                seat(7, 10, Some("technical"), Targeting::Spread),
            ],
        ),
    ]
}

/// How a run's frames and network go: each frame's length (µs, cycled)
/// and the trips up and down (µs).
struct Net {
    frames: Vec<i64>,
    up_us: i64,
    down_us: i64,
}

/// A checkpoint's commitments: attack, recipient, slot.
type Commits = Vec<(u32, ParticipantId, usize)>;

/// One stage played by its CPUs through a room's ledger, as `net` has it:
/// what every report, end and shock was, line by line.
fn play(song: &ddi_chart::Song, chart: usize, rules: &StageRules, cpus: &[CpuSeat], net: &Net) -> String {
    let seed = 0xD1CE_u64 + cpus.len() as u64;
    let plan = ddi_ui::state::battle_plan::plan(song, chart, rules, seed);
    let ruleset = crate::battle::stage_ruleset(rules);
    let ladder: Vec<PlannedRung> = if rules.adaptive() {
        ddi_engine::cpu::planned_ladder(song, chart)
    } else {
        Vec::new()
    };
    let o = ONLINE_CPUS_V0;
    let mut opps: Vec<Opponent> = cpus
        .iter()
        .map(|c| opponent(song, chart, &ruleset, c, &ladder).expect("plans"))
        .collect();
    let mut drives: Vec<Drive> = cpus
        .iter()
        .zip(&mut opps)
        .map(|(c, opp)| Drive::new(o, c.participant, Some(plan.checkpoints_us.clone()), opp))
        .collect();
    let mut ledger = StageBattle::new(
        ATTACKS,
        POWER_CLASSES_V0,
        "ddi",
        plan.checkpoints_us.clone(),
        StageRoster {
            seats: cpus
                .iter()
                .map(|c| RosterSeat {
                    participant: c.participant,
                    targeting: c.targeting,
                    lead_us: LIVE_V2.lead_us(o.horizon_us),
                })
                .collect(),
            seed: plan.seed,
        },
        plan.schedule.clone(),
    );
    let mut lines = Vec::new();
    // The room's inbox (arrival, cpu, report or end) and the device's
    // (arrival, checkpoint, commitments).
    let mut up: Vec<(i64, usize, Option<LiveReport>, Option<StreamEnd>)> = Vec::new();
    let mut down: Vec<(i64, usize, Commits)> = Vec::new();
    let mut reported: Vec<Option<u32>> = vec![None; cpus.len()];
    let mut ended: Vec<bool> = vec![false; cpus.len()];
    let mut taken: Vec<std::collections::BTreeMap<u32, LiveReport>> = vec![Default::default(); cpus.len()];
    let mut digests: Vec<Vec<String>> = vec![Vec::new(); cpus.len()];
    let end_us = plan.checkpoints_us.last().copied().unwrap_or(0) + 8_000_000;
    let mut t = o.start_us;
    let mut f = 0;
    while (t < end_us || ledger.next_checkpoint().is_some()) && t < end_us + 60_000_000 {
        t += net.frames[f % net.frames.len()];
        f += 1;
        // The device: the room's news that came, then each CPU judged on.
        down.sort_by_key(|d| d.0);
        while down.first().is_some_and(|d| d.0 <= t) {
            let (_, k, commits) = down.remove(0);
            for (attack, recipient, slot) in commits {
                let Some(i) = cpus.iter().position(|c| c.participant == recipient) else {
                    continue;
                };
                let cells = plan.schedule.slots[slot]
                    .section
                    .as_ref()
                    .map(|s| {
                        s.cells
                            .iter()
                            .map(|c| Hazard {
                                tick: ddi_chart::Tick(c.tick),
                                lane: c.lane,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                drives[i].commit(k, attack, cells);
            }
            for d in &mut drives {
                d.known(k + 1);
            }
        }
        for (i, (d, opp)) in drives.iter_mut().zip(&mut opps).enumerate() {
            let _ = d.advance(opp, t);
            for r in d.take_reports() {
                digests[i].push(format!("{}:{:016x}", r.k, report_digest(&r)));
                up.push((t + net.up_us, i, Some(r), None));
            }
            if let Some(e) = d.take_end() {
                up.push((t + net.up_us, i, None, Some(e)));
            }
        }
        // The room: what came, then every checkpoint all reported (it
        // waits for them all here: the ledger's own timing is the room's
        // affair, not the CPUs').
        up.sort_by_key(|u| u.0);
        while up.first().is_some_and(|u| u.0 <= t) {
            let (_, i, r, e) = up.remove(0);
            if let Some(r) = r {
                reported[i] = Some(reported[i].map_or(r.k, |k| k.max(r.k)));
                taken[i].insert(r.k, r);
            }
            // Out: no more reports to wait for (finished, it reports on
            // to the last checkpoint).
            if matches!(e, Some(StreamEnd::Out { .. })) {
                ended[i] = true;
            }
        }
        while let Some((k, _)) = ledger.next_checkpoint() {
            let all = (0..cpus.len())
                .all(|i| ended[i] || reported[i].is_some_and(|r| r >= k as u32));
            if !all {
                break;
            }
            let reports: Vec<Report> = (0..cpus.len())
                .map(|i| {
                    let keys: Vec<u32> = taken[i].range(..=k as u32).map(|(k, _)| *k).collect();
                    let mut merged: Option<LiveReport> = None;
                    for key in keys {
                        let r = taken[i].remove(&key).unwrap();
                        merged = Some(match merged {
                            None => r,
                            Some(mut m) => {
                                m.judged.extend(r.judged);
                                m.life_ppm = r.life_ppm;
                                m.out_at_us = m.out_at_us.or(r.out_at_us);
                                m
                            }
                        });
                    }
                    merged.map_or(
                        Report {
                            participant: cpus[i].participant,
                            judged: Vec::new(),
                            life_ppm: 0,
                            out_at_us: Some(0),
                            targeting: None,
                        },
                        |r| r.into_report(cpus[i].participant),
                    )
                })
                .collect();
            let events = ledger.checkpoint(k, &reports);
            let mut commits = Vec::new();
            for e in events {
                if let AttackEvent::Committed { attack, recipient, .. } = e
                    && let Some(a) = ledger.attack(attack)
                {
                    commits.push((attack, recipient, a.slot));
                }
            }
            let rec = ledger.record().last().unwrap();
            lines.push(format!(
                "k {k} at {} power {:?} commits {:?}",
                rec.at_us, rec.power, commits
            ));
            down.push((t + net.down_us, k, commits));
        }
    }
    for (i, (d, opp)) in drives.iter().zip(&opps).enumerate() {
        let p = opp.player();
        let v = p.ruleset().score.view();
        lines.push(format!(
            "cpu {} end {:?} applied {:?} late {} combo {} life {} score {:?} steps {} tally {:?}",
            cpus[i].participant.0,
            d.end(),
            d.applied(),
            d.late,
            p.max_combo(),
            p.ruleset().gauge.life(),
            v.ddi.map(|x| (x.score, x.points, x.absolute)),
            p.steps().len(),
            p.tally(),
        ));
        lines.push(format!(
            "cpu {} reports {}",
            cpus[i].participant.0,
            digests[i].join(" ")
        ));
    }
    lines.join("\n")
}

/// Every report a run made, as digests: the lines the fixture keeps.
fn reports_of(song: &ddi_chart::Song, chart: usize, net: &Net) -> String {
    let mut out = String::new();
    for (n, (rules, cpus)) in scenarios().into_iter().enumerate() {
        let _ = writeln!(out, "scenario {n}");
        out.push_str(&play(song, chart, &rules, &cpus, net));
        out.push('\n');
    }
    out
}

fn song() -> ddi_chart::Song {
    ddi_library::scan::parse_simfile(SONG, "ssc").unwrap()
}

/// The same reports, ends, shocks and plays whatever the frames and the
/// network, and the same as recorded (`fixture.txt`, made natively on an
/// Apple silicon Mac and checked in wasm32: the module's header).
#[test]
fn the_cpus_play_alike() {
    let song = song();
    let chart = 4;
    let nets = [
        Net {
            frames: vec![4_167],
            up_us: 30_000,
            down_us: 30_000,
        },
        Net {
            frames: vec![7_000, 16_667, 90_000, 33_333, 11_000, 61_000],
            up_us: 310_000,
            down_us: 470_000,
        },
        Net {
            frames: vec![250_000],
            up_us: 900_000,
            down_us: 1_100_000,
        },
    ];
    let runs: Vec<String> = nets.iter().map(|n| reports_of(&song, chart, n)).collect();
    for (i, r) in runs.iter().enumerate().skip(1) {
        first_difference(&runs[0], r, &format!("frames and network {i}"));
    }
    if std::env::var_os("DDI_WRITE_CPU_FIXTURE").is_some() {
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/battle/cpus/fixture.txt"),
            &runs[0],
        )
        .unwrap();
        return;
    }
    first_difference(FIXTURE, &runs[0], "the recorded fixture");
    // The fixture is a real stage: attacks went both ways, CPUs went out.
    assert!(FIXTURE.matches("commits [(").count() >= 4, "{FIXTURE}");
    assert!(FIXTURE.contains("Out {"), "{FIXTURE}");
    assert!(!FIXTURE.contains("late 1"), "{FIXTURE}");
}

/// Equal, or a failure that names the first line that differs.
fn first_difference(a: &str, b: &str, what: &str) {
    for (n, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        assert_eq!(x, y, "{what}: line {} differs", n + 1);
    }
    assert_eq!(a.lines().count(), b.lines().count(), "{what}: lengths");
}

/// The reports of a run, digested as the room keeps them (what a device
/// computing differently would be counted a mismatch by).
#[test]
fn a_report_s_digest_is_its_contents() {
    let song = song();
    let (rules, cpus) = scenarios().remove(0);
    let ruleset = crate::battle::stage_ruleset(&rules);
    let plan = ddi_ui::state::battle_plan::plan(&song, 4, &rules, 9);
    let mut opp = opponent(&song, 4, &ruleset, &cpus[0], &[]).unwrap();
    let mut d = Drive::new(ONLINE_CPUS_V0, cpus[0].participant, Some(plan.checkpoints_us.clone()), &mut opp);
    // No checkpoint heard of: it waits at the first one's shocks' time.
    let gate = ONLINE_CPUS_V0.apply_at_us(plan.checkpoints_us[0]) - 1;
    let _ = d.advance(&mut opp, i64::MAX);
    assert!(d.at_us() <= gate);
    d.known(plan.checkpoints_us.len());
    let _ = d.advance(&mut opp, plan.checkpoints_us[3] + 1_000_000);
    let reports = d.take_reports();
    assert_eq!(reports.len(), 4);
    let digests: Vec<u64> = reports.iter().map(report_digest).collect();
    assert_eq!(digests.len(), 4);
    assert!(digests.windows(2).all(|w| w[0] != w[1]));
}

/// The same chart picked for two stages is played two ways
/// (`cpu-stage-seed-v1`): the CPU's seed for stage 2 is not stage 1's,
/// and its presses differ; each stage's the same wherever it is computed.
#[test]
fn the_same_chart_in_two_stages_is_played_two_ways() {
    let song = song();
    let (rules, cpus) = scenarios().remove(0);
    let ruleset = crate::battle::stage_ruleset(&rules);
    let presses = |number: u16| {
        let mut seat = cpus[0].clone();
        seat.seed = ddi_room::OpponentSpec::stage_seed(cpus[0].seed, number);
        opponent(&song, 4, &ruleset, &seat, &[]).unwrap().presses().to_vec()
    };
    let (one, two) = (presses(1), presses(2));
    assert!(!one.is_empty());
    assert_ne!(one, two);
    assert_eq!(two, presses(2));
}

/// What computing an online room's CPUs costs a device a frame: 8, 24 and
/// 72 CPUs of the bundled chart judged on at 60 frames a second, the
/// room's shocks put in (every checkpoint known at once). Printed
/// (`--ignored --nocapture`); run natively and built for wasm32 under
/// Node (the browser's engine) as the module's header says.
#[test]
#[ignore = "a measurement"]
fn what_computing_the_cpus_costs_a_frame() {
    let song = song();
    let (rules, _) = scenarios().remove(0);
    let ruleset = crate::battle::stage_ruleset(&rules);
    let plan = ddi_ui::state::battle_plan::plan(&song, 4, &rules, 5);
    for n in [8u64, 24, 72] {
        let seats: Vec<CpuSeat> = (0..n)
            .map(|i| CpuSeat {
                participant: ParticipantId(100 + i),
                ordinal: i as u16,
                level: (i % 10) as u8,
                seed: i,
                name: None,
                skills: None,
                targeting: Targeting::Spread,
            })
            .collect();
        let built = std::time::Instant::now();
        let mut cpus: Vec<(Opponent, Drive)> = seats
            .iter()
            .map(|s| {
                let mut o = opponent(&song, 4, &ruleset, s, &[]).unwrap();
                let mut d = Drive::new(ONLINE_CPUS_V0, s.participant, Some(plan.checkpoints_us.clone()), &mut o);
                d.known(plan.checkpoints_us.len());
                (o, d)
            })
            .collect();
        let built = built.elapsed().as_secs_f64() * 1e3;
        let mut frames = Vec::new();
        let mut t = 0i64;
        while t < 100_000_000 {
            t += 16_667;
            let f = std::time::Instant::now();
            for (o, d) in &mut cpus {
                let _ = d.advance(o, t);
                let _ = d.take_reports();
            }
            frames.push(f.elapsed().as_secs_f64() * 1e3);
        }
        frames.sort_by(f64::total_cmp);
        let mean = frames.iter().sum::<f64>() / frames.len() as f64;
        eprintln!(
            "{n} CPUs: built in {built:.0} ms; a frame {mean:.3} ms mean, {:.3} p99, {:.3} max",
            frames[frames.len() * 99 / 100],
            frames[frames.len() - 1]
        );
    }
}
