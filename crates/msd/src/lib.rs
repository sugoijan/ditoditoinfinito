//! MSD: Etterna's chart difficulty calculator (MinaCalc), translated to
//! Rust for four keys (`README.md`).
//!
//! A chart's rows go in (each row's columns as bits, lane 0 the lowest,
//! and its time in seconds from the first row), eight numbers come out:
//! the overall rating and the seven skillsets, at a music rate and a score
//! goal. [`msd`] is what Etterna lists for a chart (rate 1.0, goal 93%,
//! no score adjustments); [`ssr`] what it gives a score.
//!
//! The translation follows upstream file for file (each module names the
//! file it comes from) so that a later calculator version can be followed;
//! [`CALC_VERSION`] is upstream's own number and [`UPSTREAM_COMMIT`] the
//! commit translated. Values that upstream computes only for its debug
//! graphs are left out. The arithmetic is upstream's, in `f32` where it
//! is `float` and `f64` where it is `double`, in the same order; `fastsqrt`
//! is computed as upstream's build for ARM computes it, bit for bit, on
//! every target ([`pattern_mod_helpers::fastsqrt`]).

// Index loops and negated comparisons follow upstream's code: the loops
// to be read beside it, the comparisons for their answers with NaN.
#![allow(clippy::needless_range_loop, clippy::neg_cmp_op_on_partial_ord)]

pub mod agnostic;
pub mod calc_window;
pub mod dependent;
pub mod minacalc;
pub mod minacalc_helpers;
pub mod note_data;
pub mod pattern_mod_helpers;
pub mod sequenced_base_diff_calc;
pub mod sequencing_helpers;
pub mod ulbu;
pub mod ulbu_acolytes;

#[cfg(feature = "chart")]
pub mod chart;

pub use minacalc::{Calc, msd, ssr};
pub use note_data::{NUM_SKILLSET, NoteInfo, Skillset};

/// Etterna's calculator version this translation is of (upstream's
/// `mina_calc_version`). Cached ratings name it, so a new version rates
/// again.
pub const CALC_VERSION: u32 = 527;

/// The upstream commit translated (etternagame/etterna).
pub const UPSTREAM_COMMIT: &str = "2d706147cc4bcf95a8ceb24dc4f4129be207f803";

/// The rate Etterna lists a chart's MSD at.
pub const LISTED_RATE: f32 = 1.0;

/// The score goal Etterna lists a chart's MSD at (`default_score_goal`).
pub const LISTED_GOAL: f32 = minacalc_helpers::DEFAULT_SCORE_GOAL;
