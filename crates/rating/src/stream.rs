//! The validated pattern check for a displayed Stream spoke at 1,201–3,200 notes.
use crate::features::{Chart, Row};

pub(super) fn pattern_qualifies(chart: &Chart) -> bool {
    let mut active = 0;
    let mut chords = 0;
    let mut triples = 0;
    let mut irregular = 0.0;
    let mut held_heads = 0.0;
    let mut near_mines = 0.0;
    for row in &chart.rows {
        near_mines += row.mine_near;
        if row.heads > 0.0 {
            active += 1;
            chords += usize::from(row.heads >= 2.0);
            triples += usize::from(row.heads >= 3.0);
            irregular += row.demand.irregular;
            if row.demand.held > 0 {
                held_heads += row.heads;
            }
        }
    }
    let n = f64::from(active);
    // Each descriptor with its frozen development mean, standard deviation and weight.
    #[rustfmt::skip]
    let terms = [
        (peak_rate(&chart.rows, 1.0).ln(), 2.612837458231664, 0.47649712283668644, 0.15958865971990477),
        (peak_rate(&chart.rows, 8.0).ln(), 2.404582685005189, 0.45328459075450417, -0.048761383936358774),
        (chords as f64 / n, 0.12111653878470076, 0.16946545357195136, 0.15560440938522566),
        (triples as f64 / n, 0.016748617079547626, 0.06724591190093433, -0.11467314606538472),
        (chart.repeated_heads / n, 0.1203449282100979, 0.18156436521822922, 0.05305438647668296),
        (irregular / n, 0.1269303842791267, 0.1451763942866504, 0.005317543196600369),
        (held_heads / chart.heads, 0.01920734691196665, 0.03900184580497076, 0.17729728641798098),
        ((chart.mines as f64 / chart.duration * 60.0).ln_1p(), 1.099584910575268, 1.744809310003799, 0.04357716169738723),
        (near_mines / chart.heads, 0.017849239664068674, 0.05993587034276378, 0.11049572345851744),
    ];
    let risk = 0.5183333333333336
        + terms
            .iter()
            .map(|(value, mean, sd, weight)| (value - mean) / sd * weight)
            .sum::<f64>();
    risk <= 0.7207357344075805
}

/// Maximum heads per second in [head time, head time + seconds).
fn peak_rate(rows: &[Row], seconds: f64) -> f64 {
    let mut end = 0;
    let mut heads = 0.0;
    let mut max_heads = 0.0_f64;
    for row in rows.iter().filter(|r| r.heads > 0.0) {
        while end < rows.len() && rows[end].time < row.time + seconds {
            heads += rows[end].heads;
            end += 1;
        }
        max_heads = max_heads.max(heads);
        heads -= row.heads;
    }
    max_heads / seconds
}
