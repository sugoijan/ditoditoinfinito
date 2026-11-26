//! Matched-level Jumpstream readout for scalar-qualified charts with two-head rows.
use super::{features::Chart, radar::matched_levels};
use ddi_engine::cpu::{CPU_LEVELS_V2, Skills, row_response};

pub(super) fn display_cost(chart: &Chart, decimal: f64, means: &[[f64; 9]; 10]) -> Option<f64> {
    let (i, j, f) = matched_levels(decimal);
    let cost = (1.0 - f) * means[i][2] + f * means[j][2];
    if cost > 1.0 {
        return (decimal >= 16.0).then_some(cost);
    }

    let balanced = Skills::default();
    let weakened = Skills::new([1.0, 0.75, 1.0, 1.0, 1.0, 1.0, 1.0]).unwrap();
    let mut head_rows = 0;
    let mut chords = 0;
    let mut spread = 0.0;
    let mut omission = 0.0;
    for row in &chart.rows {
        let demand = &row.demand;
        head_rows += u32::from(demand.count > 0);
        if demand.count != 2 {
            continue;
        }
        chords += 1;
        for (level, weight) in [(i, 1.0 - f), (j, f)] {
            if weight == 0.0 {
                continue;
            }
            let ability = &CPU_LEVELS_V2.levels[level];
            let (base_spread, base_omission, _) =
                row_response(ability, &balanced, demand, ability.model);
            let (weak_spread, weak_omission, _) =
                row_response(ability, &weakened, demand, ability.model);
            spread += weight * (weak_spread - base_spread);
            omission += weight * (weak_omission - base_omission);
        }
    }

    // Weight each two-note row by its two heads' share of all playable heads.
    let head_weight = 2.0 / chart.heads;
    // Input, training mean, standard deviation and coefficient of the frozen linear readout.
    #[rustfmt::skip]
    let terms = [
        (cost, 0.12423688551856323, 0.1995346643389374, -0.165719096250649),
        (f64::from(chords) / f64::from(head_rows), 0.03326201634393299, 0.029650048082878508, -0.008168252773809765),
        ((chart.heads / chart.duration).ln(), 1.9788418943119412, 0.2597275952688918, 0.0052285473537225),
        (spread * head_weight, 0.07296396383557882, 0.0911704947141271, 0.07571793491478707),
        (omission * head_weight, 0.0001610566999761104, 0.00032092762202789397, 0.10477045441598785),
    ];
    let correction = 0.008183212359719035
        + terms
            .iter()
            .map(|(x, m, s, w)| (x - m) / s * w)
            .sum::<f64>();
    Some(cost + correction.clamp(-0.10, 0.10))
}
