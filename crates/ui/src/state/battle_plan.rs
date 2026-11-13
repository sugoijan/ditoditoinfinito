//! What a stage's attack ledger needs of its chart, worked out on the
//! device that has the chart (`docs/plans/multiplayer-online-play.md`, "The
//! battle plan"): the checkpoints every four beats, the chart's sections
//! with room for shocks, and how far ahead the player reads. Practice's
//! ledger (`ddi_session::battle::attacks`) and an online room's plan
//! (`ddi_room::live::plan::BattlePlan`, offered by the picker's device and
//! compared by every other) are both made here, so that one chart gives one
//! plan everywhere.
//!
//! Here rather than in the session: the room's client offers and compares
//! the plan when it prepares a stage's song, before any play.

use ddi_chart::{Song, TICKS_PER_BEAT, Tick};
use ddi_engine::player::PlayOptions;
use ddi_engine::rules::JudgeTable;
use ddi_engine::scroll::SpeedMod;
use ddi_room::attacks::{ATTACKS, AttackRules, Cell, LEAD_V0, POWER_CLASSES_V0, Schedule, Section, Slot};
use ddi_room::live::plan::BattlePlan;

use super::room::StageRules;

/// The most checkpoints a stage has (a song of an hour at 240 BPM is
/// 3 600): a bound, never reached by a real chart.
pub const MAX_CHECKPOINTS: usize = 8_192;

/// Song seconds as microseconds.
pub fn micros(seconds: f64) -> i64 {
    (seconds * 1e6).round() as i64
}

/// Chart `chart` of `song`'s checkpoints under `rules` (song µs): every
/// `checkpoint_beats` from the first such beat to one step past the
/// chart's last note (a hold's end included).
pub fn checkpoints(song: &Song, chart: usize, rules: &AttackRules) -> Vec<i64> {
    let mut checkpoints = Vec::new();
    let Some(c) = song.charts.get(chart) else {
        return checkpoints;
    };
    let timing = c.timing(song);
    let last = c
        .notes
        .iter()
        .map(|n| n.kind.hold_end().unwrap_or(n.tick).0.max(n.tick.0))
        .max()
        .unwrap_or(0);
    let step = i64::from(rules.checkpoint_beats) * TICKS_PER_BEAT;
    let mut tick = step;
    while tick <= last + step && checkpoints.len() < MAX_CHECKPOINTS {
        checkpoints.push(micros(timing.seconds_at(Tick(tick))));
        tick += step;
    }
    checkpoints
}

/// The sections chart `chart` of `song` has room for shocks in under
/// `judge` (`ddi_engine::battle::schedule_with` with `placement`, drawn
/// from `seed`), as the ledger has them under `rules`: a slot every
/// placement's beats from the chart's start, each section's shocks timed,
/// when it clears (its last shock and its window) and when its recipient
/// may first recover.
pub fn schedule_with(
    song: &Song,
    chart: usize,
    judge: &JudgeTable,
    seed: u64,
    placement: &ddi_engine::battle::Placement,
    rules: &AttackRules,
) -> Schedule {
    let Some(c) = song.charts.get(chart) else {
        return Schedule::default();
    };
    let timing = c.timing(song);
    let window = ddi_engine::battle::INJECTED_MINE_V0.window(judge);
    let every = i64::from(placement.every_beats.max(1));
    let recover = f64::from(rules.recover_beats);
    let slots = ddi_engine::battle::schedule_with(song, chart, judge, seed, placement)
        .into_iter()
        .enumerate()
        .map(|(j, section)| Slot {
            start_us: micros(timing.seconds_at(Tick(j as i64 * every * TICKS_PER_BEAT))),
            section: section.map(|s| {
                let cells: Vec<Cell> = s
                    .cells
                    .iter()
                    .map(|h| Cell {
                        tick: h.tick.0,
                        lane: h.lane,
                        at_us: micros(timing.seconds_at(h.tick)),
                    })
                    .collect();
                let last = cells.iter().map(|c| c.at_us).max().unwrap_or(0);
                let clear = last as f64 / 1e6 + window;
                // No recovery period (`attacks-v1`): over as it clears.
                let after = if recover > 0.0 {
                    let after = timing.beat_at(clear) + recover;
                    timing.seconds_at(Tick((after * TICKS_PER_BEAT as f64).ceil() as i64))
                } else {
                    clear
                };
                Section {
                    cells,
                    clear_us: micros(clear),
                    recover_after_us: micros(after.max(clear)),
                }
            }),
        })
        .collect();
    Schedule { slots }
}

/// Stage battle plan of chart `chart` of `song` under the match's `rules`
/// (DDI, its difficulty and Boos), from the stage's seed (`seed_for`):
/// what the picker's device offers and every other compares
/// (`attacks-v1`, the engine's placement, `power-classes-v0`).
///
/// Where the match's zero life steps down (`StageRules::steps_down`) the
/// sections are placed over the stage's chart and its ladder at once
/// (`ddi_engine::ladder::merged`), so one plan holds whatever chart a
/// competitor is on, and the plan names each rung's chart digest
/// (`BattlePlan::ladder`), so that every device proves it has the same.
pub fn plan(song: &Song, chart: usize, rules: &StageRules, seed: u64) -> BattlePlan {
    let judge = rules.build().judge;
    let placement = ddi_engine::battle::PLACEMENT;
    let ladder: Vec<usize> = if rules.steps_down() {
        ddi_engine::cpu::planned_ladder(song, chart)
            .into_iter()
            .map(|r| r.chart)
            .collect()
    } else {
        Vec::new()
    };
    let placed;
    let over = if ladder.is_empty() {
        song
    } else {
        placed = ddi_engine::ladder::merged(song, chart, &ladder);
        &placed
    };
    BattlePlan::new(
        ATTACKS.id,
        placement.id,
        POWER_CLASSES_V0.id,
        &rules.ruleset,
        seed,
        checkpoints(song, chart, &ATTACKS),
        schedule_with(over, chart, &judge, seed, &placement, &ATTACKS),
    )
    .with_ladder(
        ladder
            .iter()
            .map(|&r| song.chart_digest(r).to_string())
            .collect(),
    )
}

/// The first and last note of chart `chart` of `song`, song seconds.
fn note_span(song: &Song, chart: usize) -> Option<(f64, f64)> {
    let c = song.charts.get(chart)?;
    let timing = c.timing(song);
    let span = c
        .notes
        .iter()
        .map(|n| timing.seconds_at(n.tick))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), t| {
            (a.min(t), b.max(t))
        });
    span.0.is_finite().then_some(span)
}

/// A stage's scroll speed raised so that its player's lead fits under
/// [`LEAD_V0`]'s cap (D5: an extreme slow setting would read further ahead
/// than an attack may wait): the speed `options` play chart `chart` of
/// `song` at, multiplied until the longest a note is shown before it is
/// judged leaves room for the warning and the delay; `None` where it fits
/// already.
pub fn raised_speed(song: &Song, chart: usize, options: &PlayOptions) -> Option<SpeedMod> {
    let c = song.charts.get(chart)?;
    let timing = c.timing(song);
    let (first, last) = note_span(song, chart)?;
    let cap = LEAD_V0.horizon_cap_us() as f64 / 1e6;
    let mut o = *options;
    let mut raised = false;
    for _ in 0..8 {
        let h = ddi_engine::battle::reading_horizon(&o, timing, first - 2.0, last);
        if h <= cap {
            break;
        }
        let k = h / cap * 1.05;
        o.scroll.speed = match o.scroll.speed {
            SpeedMod::XMod(m) => SpeedMod::XMod(m * k),
            SpeedMod::CMod(b) => SpeedMod::CMod(b * k),
        };
        raised = true;
    }
    raised.then_some(o.scroll.speed)
}

/// How far ahead (µs) a player reads chart `chart` of `song` under
/// `options` (their speed for the chart, raised where a stage with
/// attacks raises it: [`raised_speed`]): what a room makes their lead
/// from. Zero where the chart has no notes.
pub fn reading_horizon_us(song: &Song, chart: usize, options: &PlayOptions, attacks: bool) -> i64 {
    let Some(c) = song.charts.get(chart) else {
        return 0;
    };
    let Some((first, last)) = note_span(song, chart) else {
        return 0;
    };
    let mut o = *options;
    if attacks && let Some(speed) = raised_speed(song, chart, &o) {
        o.scroll.speed = speed;
    }
    micros(ddi_engine::battle::reading_horizon(
        &o,
        c.timing(song),
        first - 2.0,
        last,
    ))
}
