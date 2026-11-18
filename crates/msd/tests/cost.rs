//! Opt-in: how long a chart's rating takes, on this build (release or
//! debug, natively or under WASI for wasm):
//!
//! ```text
//! cargo test --release -p ddi-msd --test cost -- --ignored --nocapture
//! cargo test --release -p ddi-msd --target wasm32-wasip1 --test cost -- --ignored --nocapture
//! ```

mod common;

use std::time::Instant;

use common::{Case, synthetic};

fn time(label: &str, cases: &[Case]) {
    let mut ms: Vec<f64> = cases
        .iter()
        .filter(|c| c.rows.len() > 1)
        .map(|c| {
            let t = Instant::now();
            std::hint::black_box(c.rate());
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    ms.sort_by(f64::total_cmp);
    println!(
        "{label}: {} charts, median {:.2} ms, worst {:.2} ms",
        ms.len(),
        ms[ms.len() / 2],
        ms[ms.len() - 1]
    );
}

#[test]
#[ignore = "a measurement"]
fn a_charts_rating_time() {
    time("synthetic", &synthetic());
    #[cfg(not(target_os = "wasi"))]
    time("bundled", &common::bundled());
}
