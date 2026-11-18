//! The port against the oracle's numbers committed in `tests/fixture.json`
//! (written by `tests/oracle.rs` with `DDI_MSD_WRITE_FIXTURE=1`): the
//! synthetic charts and the bundled songs' four-lane charts.

mod common;

#[cfg(not(target_os = "wasi"))]
use common::bundled;
use common::{Case, SKILLSETS, compare, read_fixture, synthetic};

/// The port matches upstream to 4e-6 on these (last bits of `powf` and
/// `logf`); anything past this is a change in the port.
const TOLERANCE: f32 = 1e-4;

#[test]
fn the_port_matches_the_oracles_fixture() {
    let fixture = read_fixture();
    assert_eq!(fixture.calc_version, ddi_msd::CALC_VERSION);
    assert_eq!(fixture.commit, ddi_msd::UPSTREAM_COMMIT);
    #[allow(unused_mut)]
    let mut cases = synthetic();
    // The bundled songs are files, which a WASI test cannot read: there the
    // synthetic charts show the wasm build gives the same numbers.
    #[cfg(not(target_os = "wasi"))]
    {
        cases.extend(bundled());
        let mut names: Vec<_> = cases.iter().map(|c| c.name.clone()).collect();
        names.sort();
        let fixed: Vec<_> = fixture.expected.keys().cloned().collect();
        assert_eq!(names, fixed, "the fixture names every case");
    }

    let pairs: Vec<_> = cases
        .iter()
        .map(|c: &Case| (fixture.expected[&c.name], c.rate()))
        .collect();
    let (worst, exact) = compare(&pairs);
    println!(
        "{} cases, {exact} identical to the bit, largest differences {worst:?}",
        cases.len()
    );
    for (k, w) in worst.iter().enumerate() {
        assert!(*w <= TOLERANCE, "{}: {w}", SKILLSETS[k]);
    }
    // Nearly all to the bit.
    assert!(exact * 10 >= cases.len() * 9, "{exact} of {}", cases.len());
}

#[cfg(not(target_os = "wasi"))]
#[test]
fn bundled_charts_are_rated() {
    let cases = bundled();
    assert!(!cases.is_empty());
    for c in &cases {
        let v = c.rate();
        assert!(v[0] > 0.0, "{} has an overall rating", c.name);
    }
}
