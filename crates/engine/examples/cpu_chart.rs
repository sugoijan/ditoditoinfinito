//! Extract native chart times or benchmark a prepared CPU through the real judge.
use ddi_engine::{
    FailPolicy, Judge, PlayOptions, cpu,
    history::{LoggedInput, PlayLog},
    presets,
};
use serde::Deserialize;
use serde_json::json;
use std::{path::Path, time::Instant};

fn song(path: &Path) -> Result<ddi_chart::Song, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    Ok(ddi_library::scan::parse_simfile(
        &ddi_library::pack::decode_text(&bytes),
        path.extension()
            .and_then(|x| x.to_str())
            .ok_or("extension")?,
    )?)
}
#[derive(Deserialize)]
struct Prepared {
    source_file: String,
    chart_index: usize,
    profile: cpu::Profile,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("automatic-benchmark") => {
            let original = song(Path::new(args.get(2).ok_or("source")?))?;
            let index = args.get(3).ok_or("index")?.parse()?;
            let start = Instant::now();
            let plan = cpu::Plan::automatic(&original, index)?;
            let prepare_ms = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            plan.check_song(&original, index)?;
            let validate_ms = start.elapsed().as_secs_f64() * 1000.;
            let mut records = vec![];
            for level in 0..5 {
                let start = Instant::now();
                let skills = cpu::Skills::parse(args.get(4).map_or("balanced", String::as_str))?;
                let script = plan.script_with_skills(level, 7, skills)?;
                records.push(json!({"level":level,"presses":script.len(),"sample_ms":start.elapsed().as_secs_f64()*1000.}));
            }
            println!(
                "{}",
                json!({"model":plan.model(),"prepare_ms":prepare_ms,"validate_ms":validate_ms,"records":records})
            );
        }
        Some("extract") => {
            let path = Path::new(args.get(2).ok_or("source")?).canonicalize()?;
            let index: usize = args.get(3).ok_or("chart index")?.parse()?;
            let original = song(&path)?;
            let projected = cpu::project(&original, index)?;
            let chart = &projected.charts[index];
            let judge = Judge::new(&chart.notes, chart.timing(&projected), presets::itg().judge);
            println!(
                "{}",
                json!({"source_file":path,"chart_index":index,"title":original.title,"chart_digest":original.chart_digest(index),"rows":cpu::rows(judge.notes())?})
            );
        }
        Some("sample" | "automatic") => {
            let automatic = args[1] == "automatic";
            let (profile, original, index, shift) = if automatic {
                let original = song(Path::new(args.get(2).ok_or("source")?))?;
                let index = args.get(3).ok_or("index")?.parse()?;
                (cpu::Plan::automatic(&original, index)?, original, index, 1)
            } else {
                let p: Prepared =
                    serde_json::from_slice(&std::fs::read(args.get(2).ok_or("artifact")?)?)?;
                (
                    cpu::Plan::Prepared(p.profile),
                    song(Path::new(&p.source_file))?,
                    p.chart_index,
                    0,
                )
            };
            let level = cpu::strength(args.get(3 + shift).ok_or("strength")?)?;
            let seed: u64 = args.get(4 + shift).ok_or("seed")?.parse()?;
            let skills =
                cpu::Skills::parse(args.get(5 + shift).map_or("balanced", String::as_str))?;
            let projected = profile.check_song(&original, index)?;
            let mut inputs: Vec<_> = profile
                .script_with_skills(level, seed, skills)?
                .iter()
                .flat_map(|p| {
                    [
                        LoggedInput {
                            time: (p.press * 1e6).round() / 1e6,
                            lane: p.lane,
                            pressed: true,
                        },
                        LoggedInput {
                            time: (p.release * 1e6).round() / 1e6,
                            lane: p.lane,
                            pressed: false,
                        },
                    ]
                })
                .collect();
            inputs.sort_by(|a, b| a.time.total_cmp(&b.time).then(a.lane.cmp(&b.lane)));
            let log = PlayLog {
                start: 0.,
                inputs,
                ..Default::default()
            };
            let mut rules = presets::itg();
            rules.fail = FailPolicy::Off;
            let options = PlayOptions {
                seed,
                ..Default::default()
            };
            let results =
                ddi_engine::history::replay_with(&projected, index, rules, &options, &log);
            println!(
                "{}",
                json!({"log":log,"results":results,"seed":seed.to_string(),"strength":cpu::LABELS[level]})
            );
        }
        Some("benchmark") => {
            let start = Instant::now();
            let p: Prepared =
                serde_json::from_slice(&std::fs::read(args.get(2).ok_or("artifact")?)?)?;
            let projected = p
                .profile
                .check_song(&song(Path::new(&p.source_file))?, p.chart_index)?;
            let load_ms = start.elapsed().as_secs_f64() * 1000.;
            let mut records = vec![];
            for level in 0..5 {
                for seed in 1..=16 {
                    let start = Instant::now();
                    let presses = p.profile.script(level, seed)?;
                    let sample_ms = start.elapsed().as_secs_f64() * 1000.;
                    let mut inputs: Vec<_> = presses
                        .iter()
                        .flat_map(|p| {
                            [
                                LoggedInput {
                                    time: p.press,
                                    lane: p.lane,
                                    pressed: true,
                                },
                                LoggedInput {
                                    time: p.release,
                                    lane: p.lane,
                                    pressed: false,
                                },
                            ]
                        })
                        .collect();
                    inputs.sort_by(|a, b| a.time.total_cmp(&b.time).then(a.lane.cmp(&b.lane)));
                    let log = PlayLog {
                        start: 0.,
                        inputs,
                        ..Default::default()
                    };
                    let mut rules = presets::itg();
                    rules.fail = FailPolicy::Off;
                    let start = Instant::now();
                    let result = ddi_engine::history::replay_with(
                        &projected,
                        p.chart_index,
                        rules,
                        &PlayOptions::default(),
                        &log,
                    );
                    records.push(json!({"level":level,"seed":seed,"presses":presses.len(),"sample_ms":sample_ms,"judge_ms":start.elapsed().as_secs_f64()*1000.,"score":result.score,"tally":result.tally,"late_edges":result.late_edges,"timeline":result.timeline.len()}));
                }
            }
            let presses = p
                .profile
                .script(2, 1)?
                .iter()
                .map(|p| json!([p.press, p.release, p.lane]))
                .collect::<Vec<_>>();
            println!(
                "{}",
                json!({"load_ms":load_ms,"rows":p.profile.rows.len(),"duration":p.profile.rows.last().unwrap().seconds-p.profile.rows[0].seconds,"records":records,"parity_presses":presses})
            );
        }
        _ => {
            return Err(
                "cpu_chart extract FILE INDEX | benchmark PROFILE | sample PROFILE STRENGTH SEED | automatic FILE INDEX STRENGTH SEED | automatic-benchmark FILE INDEX"
                    .into(),
            );
        }
    }
    Ok(())
}
