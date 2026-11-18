//! Opt-in: the port against upstream's own calculator, built by
//! `tools/msd-oracle/build.sh` and named by `DDI_MSD_ORACLE`.
//!
//! ```text
//! DDI_MSD_ORACLE=target/msd-oracle/oracle cargo test --release -p ddi-msd \
//!   --test oracle -- --ignored --nocapture
//! ```
//!
//! `DDI_MSD_WRITE_FIXTURE=1` writes the oracle's numbers for the synthetic
//! and bundled charts into `tests/fixture.json`. `DDI_MSD_CORPUS=<dir>[:<dir>…]`
//! also compares every four-lane chart of the simfiles under local folders,
//! read in place; only aggregates are printed (no names).

#![allow(clippy::neg_cmp_op_on_partial_ord)]

mod common;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use common::{Case, SKILLSETS, bundled, compare, parse, simfile_in, song_cases, synthetic};

/// How close the port must come to the oracle, by skillset.
const TOLERANCE: f32 = 0.005;

fn oracle() -> Option<PathBuf> {
    std::env::var_os("DDI_MSD_ORACLE").map(PathBuf::from)
}

/// The oracle's eight numbers for each case.
fn run_oracle(oracle: &Path, cases: &[Case]) -> Vec<[f32; 8]> {
    let mut child = Command::new(oracle)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start the oracle");
    let mut stdin = child.stdin.take().unwrap();
    let input: String = cases
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut s = format!(
                "chart c{i} {:08x} {:08x} {} {}\n",
                c.rate.to_bits(),
                c.goal.to_bits(),
                u8::from(c.ssr),
                c.rows.len()
            );
            for r in &c.rows {
                s.push_str(&format!("{} {:08x}\n", r.notes, r.row_time.to_bits()));
            }
            s
        })
        .collect();
    let writer = std::thread::spawn(move || {
        stdin.write_all(input.as_bytes()).unwrap();
    });
    let out = BufReader::new(child.stdout.take().unwrap());
    let mut results = Vec::with_capacity(cases.len());
    for line in out.lines() {
        let line = line.unwrap();
        let mut parts = line.split_whitespace();
        parts.next();
        let mut v = [0f32; 8];
        for x in &mut v {
            *x = f32::from_bits(u32::from_str_radix(parts.next().unwrap(), 16).unwrap());
        }
        results.push(v);
        if results.len() == cases.len() {
            break;
        }
    }
    writer.join().unwrap();
    let _ = child.wait();
    results
}

fn report(label: &str, cases: &[Case], theirs: &[[f32; 8]], ours: &[[f32; 8]]) -> [f32; 8] {
    let pairs: Vec<_> = theirs.iter().copied().zip(ours.iter().copied()).collect();
    let (worst, exact) = compare(&pairs);
    println!(
        "{label}: {} cases, {exact} identical to the bit",
        cases.len()
    );
    for k in 0..8 {
        println!("  {:<11} largest difference {:.6}", SKILLSETS[k], worst[k]);
    }
    let over: usize = pairs
        .iter()
        .filter(|(a, b)| (0..8).any(|k| !((a[k] - b[k]).abs() <= TOLERANCE)))
        .count();
    println!("  over {TOLERANCE}: {over}");
    worst
}

#[test]
#[ignore = "needs DDI_MSD_ORACLE (tools/msd-oracle/build.sh)"]
fn the_port_matches_the_oracle_on_synthetic_and_bundled_charts() {
    let Some(oracle) = oracle() else {
        eprintln!("DDI_MSD_ORACLE not set");
        return;
    };
    let mut cases = synthetic();
    cases.extend(bundled());
    let theirs = run_oracle(&oracle, &cases);
    let ours: Vec<_> = cases.iter().map(Case::rate).collect();
    for (c, (a, b)) in cases.iter().zip(theirs.iter().zip(&ours)) {
        let differ: Vec<_> = (0..8)
            .filter(|&k| a[k].to_bits() != b[k].to_bits())
            .map(|k| SKILLSETS[k])
            .collect();
        if !differ.is_empty() {
            println!("  {} differs in its last bits: {differ:?}", c.name);
        }
        if (0..8).any(|k| !((a[k] - b[k]).abs() <= TOLERANCE)) {
            println!(
                "  {}: oracle {a:?}\n  {}  port   {b:?}",
                c.name,
                " ".repeat(c.name.len())
            );
        }
    }
    let worst = report("synthetic and bundled", &cases, &theirs, &ours);
    if std::env::var_os("DDI_MSD_WRITE_FIXTURE").is_some() {
        let expected: BTreeMap<_, _> = cases
            .iter()
            .map(|c| c.name.clone())
            .zip(theirs.iter().copied())
            .collect();
        common::write_fixture(&expected);
        println!("wrote {}", common::FIXTURE);
    }
    assert!(worst.iter().all(|w| *w <= TOLERANCE), "{worst:?}");
}

#[test]
#[ignore = "needs DDI_MSD_ORACLE (tools/msd-oracle/build.sh)"]
fn fastsqrt_matches_the_oracles_build_bit_for_bit() {
    let Some(oracle) = oracle() else {
        eprintln!("DDI_MSD_ORACLE not set");
        return;
    };
    // Every exponent with the high mantissa bits stepped, and random
    // values: what FRSQRTE reads, and the Newton step's rounding.
    let mut inputs = Vec::new();
    let mut rng = common::Rng::new(7);
    for exp in 0u32..255 {
        for hi in 0u32..512 {
            let bits = (exp << 23) | (hi << 14) | (rng.next() as u32 & 0x3fff);
            inputs.push(f32::from_bits(bits));
        }
    }
    for _ in 0..200_000 {
        inputs.push(f32::from_bits(rng.next() as u32 & 0x7fff_ffff));
    }
    inputs.retain(|x| x.is_finite());
    let mut child = Command::new(&oracle)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start the oracle");
    let mut stdin = child.stdin.take().unwrap();
    let mut input = format!("fastsqrt {}\n", inputs.len());
    for x in &inputs {
        input.push_str(&format!("{:08x}\n", x.to_bits()));
    }
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()).unwrap());
    let out = BufReader::new(child.stdout.take().unwrap());
    let mut mismatches = 0;
    for (x, line) in inputs.iter().zip(out.lines()) {
        let want = u32::from_str_radix(&line.unwrap(), 16).unwrap();
        let got = ddi_msd::pattern_mod_helpers::fastsqrt(*x).to_bits();
        if got != want {
            if mismatches < 10 {
                println!(
                    "fastsqrt({:08x}): oracle {want:08x}, port {got:08x}",
                    x.to_bits()
                );
            }
            mismatches += 1;
        }
    }
    writer.join().unwrap();
    let _ = child.wait();
    println!("fastsqrt: {} inputs, {mismatches} differ", inputs.len());
    assert_eq!(mismatches, 0);
}

fn simfile_dirs(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut sub = Vec::new();
    let mut has_simfile = false;
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            sub.push(p);
        } else if matches!(
            p.extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("sm" | "ssc")
        ) {
            has_simfile = true;
        }
    }
    if has_simfile {
        out.push(root.to_path_buf());
    }
    sub.sort();
    for s in sub {
        simfile_dirs(&s, out);
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

#[test]
#[ignore = "needs DDI_MSD_ORACLE and DDI_MSD_CORPUS (local simfiles)"]
fn the_port_matches_the_oracle_over_a_local_corpus() {
    let (Some(oracle), Some(corpus)) = (oracle(), std::env::var_os("DDI_MSD_CORPUS")) else {
        eprintln!("DDI_MSD_ORACLE or DDI_MSD_CORPUS not set");
        return;
    };
    let mut dirs = Vec::new();
    for root in std::env::split_paths(&corpus) {
        simfile_dirs(&root, &mut dirs);
    }
    let mut cases = Vec::new();
    let mut unreadable = 0;
    for (i, dir) in dirs.iter().enumerate() {
        let Some(song) = simfile_in(dir).and_then(|f| parse(&f)) else {
            unreadable += 1;
            continue;
        };
        cases.extend(song_cases(&format!("corpus/{i}"), &song));
    }
    println!(
        "corpus: {} song folders ({unreadable} unreadable), {} four-lane charts",
        dirs.len(),
        cases.len()
    );
    let theirs = run_oracle(&oracle, &cases);
    let mut times = Vec::with_capacity(cases.len());
    let ours: Vec<_> = cases
        .iter()
        .map(|c| {
            let t = Instant::now();
            let v = c.rate();
            times.push(t.elapsed().as_secs_f64() * 1000.0);
            v
        })
        .collect();
    report("corpus", &cases, &theirs, &ours);
    let mut buckets = [0usize; 5];
    for (a, b) in theirs.iter().zip(&ours) {
        let d = (0..8).map(|k| (a[k] - b[k]).abs()).fold(0f32, f32::max);
        let i = if d == 0.0 {
            0
        } else if d < 0.001 {
            1
        } else if d < 0.005 {
            2
        } else if d < 0.05 {
            3
        } else {
            4
        };
        buckets[i] += 1;
    }
    println!(
        "  largest difference a chart: 0 {}, <0.001 {}, <0.005 {}, <0.05 {}, more {}",
        buckets[0], buckets[1], buckets[2], buckets[3], buckets[4]
    );
    times.sort_by(f64::total_cmp);
    println!(
        "  port time a chart (this build): median {:.2} ms, p90 {:.2} ms, worst {:.2} ms",
        percentile(&times, 0.5),
        percentile(&times, 0.9),
        times.last().copied().unwrap_or(0.0)
    );
}
