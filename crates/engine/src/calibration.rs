//! Converging offset estimation for calibration sessions.
//!
//! Hits arrive as signed timing errors (seconds, + = late). The first
//! [`WARMUP`] hits are ignored (the player is still locking on), as are the
//! first [`ADAPT`] hits after every correction (the offset just moved under
//! them). Each batch of [`BATCH`] counted hits is summarised with a trimmed
//! mean (the most extreme [`TRIM`] fraction on each side is dropped) so one
//! fumbled note cannot steer the result. A batch whose mean is statistically
//! distinguishable from zero (|mean| > 2σ/√n and at least [`MIN_STEP`]) is
//! applied by the caller as a correction and the residual window restarts.
//! Once at least [`MIN_RESIDUAL`] counted hits after the last correction have
//! a trimmed mean within noise, the test has converged.

/// Hits per adjustment round.
pub const BATCH: usize = 8;
/// Residual hits needed before convergence can be declared.
pub const MIN_RESIDUAL: usize = 16;
/// Corrections smaller than this are never applied (seconds).
pub const MIN_STEP: f64 = 0.003;
/// Hits ignored at the start of a test.
pub const WARMUP: usize = 4;
/// Hits ignored after each applied correction.
pub const ADAPT: usize = 2;
/// Fraction trimmed from each end before computing a mean (per batch, the
/// single most extreme hit on each side).
pub const TRIM: f64 = 0.125;
/// Largest offset a test can measure (seconds): also the judge window on
/// each side. Beyond this a press cannot be told apart from a hit on the
/// neighbouring note, so the test gives up and the offset must be set by
/// hand. The calibration chart must keep every gap between notes wider than
/// twice this, and must not be periodic (see the app's chart generator), so
/// that presses one note off produce misses rather than a consistent alias.
pub const MAX_OFFSET: f64 = 0.25;
/// Residual spread above which a result is not trusted (seconds).
pub const MAX_SD: f64 = 0.08;
/// Misses needed (with a high miss rate) before a test is cut short.
pub const EARLY_STOP_MISSES: u32 = 6;

/// Mean and (population) standard deviation.
pub fn mean_sd(v: &[f64]) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    let var = v.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / v.len() as f64;
    (mean, var.sqrt())
}

/// Trimmed mean and standard deviation: sorts, drops `ceil(TRIM·n)` values
/// from each end (when n ≥ 6) and returns `(mean, sd, n_used)`.
pub fn trimmed_mean_sd(v: &[f64]) -> (f64, f64, usize) {
    if v.len() < 6 {
        let (m, s) = mean_sd(v);
        return (m, s, v.len());
    }
    let mut sorted = v.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    let k = (TRIM * sorted.len() as f64).ceil() as usize;
    let kept = &sorted[k..sorted.len() - k];
    let (m, s) = mean_sd(kept);
    (m, s, kept.len())
}

/// Half-width of the 95 % confidence interval of a mean: 2σ/√n.
pub fn confidence(sd: f64, n: usize) -> f64 {
    if n == 0 {
        return f64::INFINITY;
    }
    2.0 * sd / (n as f64).sqrt()
}

/// A mean is significant when zero lies outside its confidence interval and
/// it is at least `MIN_STEP`.
pub fn significant(mean: f64, sd: f64, n: usize) -> bool {
    mean.abs() >= MIN_STEP && mean.abs() > confidence(sd, n)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Calibrator {
    batch: Vec<f64>,
    residual: Vec<f64>,
    /// Hits seen so far, counted or not.
    seen: usize,
    /// Hits still to ignore after the last correction.
    adapt_left: usize,
    /// Hits ignored for warm-up or adaptation.
    pub ignored: usize,
    /// Accumulated correction the caller has applied, seconds.
    pub correction: f64,
    pub rounds: u32,
    pub converged: bool,
    /// The accumulated correction left the measurable range.
    pub out_of_range: bool,
}

impl Calibrator {
    /// The test has ended, one way or the other.
    pub fn done(&self) -> bool {
        self.converged || self.out_of_range
    }

    /// Records a hit. Returns `Some(step)` when the caller should add `step`
    /// to the offset under test now.
    pub fn record(&mut self, delta: f64) -> Option<f64> {
        self.seen += 1;
        if self.seen <= WARMUP || self.adapt_left > 0 {
            self.adapt_left = self.adapt_left.saturating_sub(1);
            self.ignored += 1;
            return None;
        }
        self.batch.push(delta);
        self.residual.push(delta);
        if self.batch.len() < BATCH {
            return None;
        }
        let (mean, sd, n) = trimmed_mean_sd(&self.batch);
        self.batch.clear();
        if significant(mean, sd, n) {
            if (self.correction + mean).abs() > MAX_OFFSET {
                self.out_of_range = true;
                return None;
            }
            self.correction += mean;
            self.rounds += 1;
            self.residual.clear();
            self.adapt_left = ADAPT;
            return Some(mean);
        }
        if self.residual.len() >= MIN_RESIDUAL {
            let (m, s, n) = trimmed_mean_sd(&self.residual);
            if !significant(m, s, n) {
                self.converged = true;
            }
        }
        None
    }

    pub fn outcome(&self, hits: u32, misses: u32) -> Outcome {
        let (residual_mean, residual_sd, residual_n) = trimmed_mean_sd(&self.residual);
        Outcome {
            correction: self.correction,
            residual_mean,
            residual_sd,
            residual_n,
            trimmed: self.residual.len() - residual_n,
            ignored: self.ignored,
            rounds: self.rounds,
            converged: self.converged,
            out_of_range: self.out_of_range,
            hits,
            misses,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub correction: f64,
    pub residual_mean: f64,
    pub residual_sd: f64,
    /// Residual hits used after trimming.
    pub residual_n: usize,
    /// Residual hits dropped as extremes.
    pub trimmed: usize,
    /// Hits ignored for warm-up and post-correction adaptation.
    pub ignored: usize,
    pub rounds: u32,
    pub converged: bool,
    pub out_of_range: bool,
    pub hits: u32,
    pub misses: u32,
}

impl Outcome {
    /// More than a quarter of the notes were missed (presses fell outside the
    /// window: lag beyond [`MAX_OFFSET`], a note-off alias, or a lapse), or
    /// the residual spread is implausibly wide.
    pub fn unreliable(&self) -> bool {
        self.high_miss_rate() || (self.residual_n >= 6 && self.residual_sd > MAX_SD)
    }

    pub fn high_miss_rate(&self) -> bool {
        self.misses * 4 > self.hits.max(1)
    }

    /// Enough evidence to stop the test as unreliable.
    pub fn should_stop_early(&self) -> bool {
        self.misses >= EARLY_STOP_MISSES && self.high_miss_rate()
    }

    /// What saving should add to the setting: the applied correction plus the
    /// residual mean (within noise when converged).
    pub fn total(&self) -> f64 {
        self.correction + self.residual_mean
    }

    pub fn residual_significant(&self) -> bool {
        significant(self.residual_mean, self.residual_sd, self.residual_n)
    }

    pub fn confidence(&self) -> f64 {
        confidence(self.residual_sd, self.residual_n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player who is `bias` late with deterministic ±jitter, and who reacts
    /// to corrections (their error shrinks by what has been applied).
    fn simulate(bias: f64, jitter: f64, max_hits: usize) -> (Calibrator, f64) {
        let mut cal = Calibrator::default();
        let mut applied = 0.0;
        for i in 0..max_hits {
            let j = if i % 2 == 0 { jitter } else { -jitter };
            let delta = bias - applied + j;
            if let Some(step) = cal.record(delta) {
                applied += step;
            }
            if cal.converged {
                break;
            }
        }
        (cal, applied)
    }

    #[test]
    fn converges_on_a_late_player() {
        let (cal, applied) = simulate(0.040, 0.008, 96);
        assert!(cal.converged, "{cal:?}");
        assert!((applied - 0.040).abs() < 0.002, "applied {applied}");
        assert_eq!(cal.rounds, 1);
        let o = cal.outcome(32, 0);
        assert!((o.total() - 0.040).abs() < 0.003);
        assert!(!o.residual_significant());
        assert_eq!(o.ignored, WARMUP + ADAPT);
    }

    #[test]
    fn warmup_hits_are_ignored() {
        let mut cal = Calibrator::default();
        // Four wild warm-up hits, then an in-sync player.
        for d in [0.150, -0.120, 0.090, 0.200] {
            assert_eq!(cal.record(d), None);
        }
        for _ in 0..MIN_RESIDUAL {
            assert_eq!(cal.record(0.0), None);
        }
        assert!(cal.converged);
        assert_eq!(cal.rounds, 0);
        assert_eq!(cal.ignored, WARMUP);
    }

    #[test]
    fn single_outlier_does_not_steer_a_batch() {
        let mut cal = Calibrator::default();
        for _ in 0..WARMUP {
            cal.record(0.0);
        }
        // One fumbled note 150 ms late in an otherwise in-sync batch.
        let batch = [0.002, -0.003, 0.150, 0.001, -0.002, 0.003, -0.001, 0.002];
        let mut step = None;
        for d in batch {
            step = cal.record(d).or(step);
        }
        assert_eq!(step, None, "outlier must not trigger a correction");
        assert_eq!(cal.rounds, 0);
    }

    #[test]
    fn offset_beyond_range_stops_the_test() {
        let mut cal = Calibrator::default();
        for _ in 0..WARMUP {
            cal.record(0.0);
        }
        // Every press 200 ms late: first batch applies +0.2; a second +0.2
        // would exceed MAX_OFFSET.
        for _ in 0..BATCH {
            cal.record(0.2);
        }
        assert_eq!(cal.rounds, 1);
        for _ in 0..ADAPT + BATCH {
            cal.record(0.2);
        }
        assert!(cal.out_of_range);
        assert!(cal.done());
        assert!(!cal.converged);
        assert_eq!(cal.rounds, 1, "no further correction applied");
    }

    #[test]
    fn unreliable_on_misses_or_spread() {
        let mut cal = Calibrator::default();
        for _ in 0..WARMUP + MIN_RESIDUAL {
            cal.record(0.0);
        }
        assert!(!cal.outcome(16, 2).unreliable());
        assert!(cal.outcome(16, 5).high_miss_rate());
        assert!(cal.outcome(16, 6).should_stop_early());
        let mut noisy = Calibrator::default();
        for i in 0..WARMUP + MIN_RESIDUAL {
            noisy.record(if i % 2 == 0 { 0.15 } else { -0.15 });
        }
        assert!(noisy.outcome(16, 0).unreliable());
    }

    #[test]
    fn trimmed_mean_drops_extremes() {
        let v = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let (m, _, n) = trimmed_mean_sd(&v);
        assert_eq!(n, 6);
        assert_eq!(m, 0.0);
        // Too few values: untrimmed.
        let (m, _, n) = trimmed_mean_sd(&[0.0, 1.0]);
        assert_eq!(n, 2);
        assert_eq!(m, 0.5);
    }

    #[test]
    fn converges_on_an_early_player_with_noise() {
        let (cal, applied) = simulate(-0.025, 0.020, 96);
        assert!(cal.converged, "{cal:?}");
        assert!((applied + 0.025).abs() < 0.004, "applied {applied}");
    }

    #[test]
    fn in_sync_player_converges_without_corrections() {
        let (cal, applied) = simulate(0.0, 0.010, 96);
        assert!(cal.converged);
        assert_eq!(cal.rounds, 0);
        assert_eq!(applied, 0.0);
    }

    #[test]
    fn tiny_bias_is_not_applied() {
        let (cal, _) = simulate(0.002, 0.004, 96);
        assert_eq!(cal.rounds, 0);
        assert!(cal.converged);
    }

    #[test]
    fn significance_uses_standard_error_not_sd() {
        // 10 ms mean with 30 ms sd over 32 hits: 2σ/√n = 10.6 ms → not yet.
        assert!(!significant(0.010, 0.030, 32));
        // Over 64 hits: 7.5 ms → significant.
        assert!(significant(0.010, 0.030, 64));
        assert!(!significant(0.002, 0.001, 64));
    }
}
