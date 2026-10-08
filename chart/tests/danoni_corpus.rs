//! Opt-in comparison of the Dancing☆Onigiri importer with danoniplus's own
//! parse, over a local corpus of dumped works (`docs/research/danoni-corpus.md`,
//! dump format `ddi-danoni-dump/1`). Third-party works stay local: nothing
//! here is committed or run in CI.
//!
//! ```sh
//! DDI_DANONI_CORPUS=~/Music/DanOni/corpus cargo test -p ddi-chart \
//!     --test danoni_corpus -- --nocapture
//! ```
//!
//! For every captured chart with a reference, the chart's effective dos
//! fields (the object danoniplus converted) are imported and compared with
//! the reference: lane names, tap and hold frames, speed and boost events
//! (reference frames include danoniplus's integer adjustment, which the
//! importer keeps in the timing instead), and that the chart imports exactly
//! when its key mode is one the game plays.

use std::path::{Path, PathBuf};

use ddi_chart::formats::danoni::{self, Dos, keys};
use serde_json::Value;

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn ints(v: &Value) -> Vec<i64> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_f64)
                .map(|f| f as i64)
                .collect()
        })
        .unwrap_or_default()
}

fn events(v: &Value) -> Vec<(i64, f64)> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| Some((e["frame"].as_f64()? as i64, e["multiplier"].as_f64()?)))
                .collect()
        })
        .unwrap_or_default()
}

/// Compares one chart; returns the differences found.
fn compare_chart(work: &Path, chart: &Value) -> Result<Vec<String>, String> {
    let paths = &chart["paths"];
    let (Some(eff), Some(reference)) =
        (paths["effectiveDos"].as_str(), paths["reference"].as_str())
    else {
        return Err("no effective dos or reference".into());
    };
    let eff = read_json(&work.join(eff)).ok_or("unreadable effective dos")?;
    let r = read_json(&work.join(reference)).ok_or("unreadable reference")?;
    let mut dos = Dos::default();
    for (k, v) in eff["fields"].as_object().ok_or("no fields")? {
        if let Some(v) = v.as_str() {
            dos.set(k, v.to_string());
        }
    }
    let chart_id = chart["chartId"].as_u64().ok_or("no chartId")? as usize;
    let suffix = chart["sourceSuffix"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let mode = keys::canonical_label(chart["keyMode"].as_str().unwrap_or("7")).to_string();
    let mut diffs = Vec::new();

    let (def, from_header) =
        keys::key_def(&dos, &mode)?.ok_or(format!("unknown key mode {mode}"))?;
    let ref_lanes = r["lanes"].as_array().ok_or("no lanes")?;
    let ref_chara: Vec<&str> = ref_lanes
        .iter()
        .filter_map(|l| l["chara"].as_str())
        .collect();
    if def.chara != ref_chara {
        diffs.push(format!("lanes {:?} vs {:?}", def.chara, ref_chara));
    }
    let adj = r["timing"]["intAdjustment"].as_i64().unwrap_or(0);
    let pre = r["timing"]["preblankFrame"].as_i64().unwrap_or(0);
    let shift = |v: Vec<i64>| v.into_iter().map(|f| f + adj).collect::<Vec<_>>();
    let ours = danoni::lane_frames(&dos, &def.chara, &suffix);
    for (j, (o, l)) in ours.iter().zip(ref_lanes).enumerate() {
        let (taps, holds) = (ints(&l["taps"]), ints(&l["holdEndpoints"]));
        if shift(o.taps.clone()) != taps {
            diffs.push(format!(
                "lane {j} taps: {} ours vs {} reference",
                o.taps.len(),
                taps.len()
            ));
        }
        if shift(o.holds.clone()) != holds {
            diffs.push(format!(
                "lane {j} holds: {} ours vs {} reference",
                o.holds.len(),
                holds.len()
            ));
        }
    }
    for (name, ours, theirs) in [
        (
            "speed",
            danoni::speed_pairs(&dos, &suffix),
            events(&r["speed"]),
        ),
        (
            "boost",
            danoni::boost_pairs(&dos, &suffix),
            events(&r["boost"]),
        ),
    ] {
        let ours: Vec<(i64, f64)> = ours.into_iter().map(|(f, m)| (f + adj, m)).collect();
        if ours != theirs {
            diffs.push(format!("{name}: {ours:?} vs {theirs:?}"));
        }
    }
    // Timing: the importer folds floor(adjustment) into the offset; the
    // reference's integer adjustment also carries the preblank lead-in.
    let header_adj = r["timing"]["headerAdjustment"].as_f64().unwrap_or(0.0);
    if header_adj.floor() as i64 != adj - pre {
        diffs.push(format!(
            "adjustment {header_adj} vs integer adjustment {adj} − preblank {pre}"
        ));
    }

    // The whole work imports (or reports why not); a chart in a key mode
    // the game plays must be among the imported ones.
    let played = ddi_chart::PLAYED_DANONI_MODES.contains(&mode.as_str()) && !from_header;
    match danoni::import(&dos) {
        Ok(import) => {
            let imported = import.charts.get(chart_id).copied().flatten().is_some();
            if imported != played {
                diffs.push(format!(
                    "chart {chart_id} imported: {imported}, expected {played}: {:?}",
                    import.warnings
                ));
            }
        }
        Err(e) if played => diffs.push(format!("import: {e}")),
        Err(_) => {}
    }
    Ok(diffs)
}

#[test]
fn local_corpus() {
    let Ok(root) = std::env::var("DDI_DANONI_CORPUS") else {
        eprintln!("DDI_DANONI_CORPUS not set; skipping");
        return;
    };
    let mut works: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("corpus directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("manifest.json").is_file())
        .collect();
    works.sort();
    let (mut compared, mut failed) = (0, 0);
    for work in works {
        let Some(manifest) = read_json(&work.join("manifest.json")) else {
            continue;
        };
        let name = work.file_name().unwrap_or_default().to_string_lossy();
        for chart in manifest["charts"].as_array().into_iter().flatten() {
            let id = chart["chartId"].as_u64().unwrap_or(0);
            match compare_chart(&work, chart) {
                Err(skip) => eprintln!("{name} chart {id}: skipped ({skip})"),
                Ok(diffs) if diffs.is_empty() => {
                    compared += 1;
                    eprintln!("{name} chart {id}: matches");
                }
                Ok(diffs) => {
                    compared += 1;
                    failed += 1;
                    eprintln!("{name} chart {id}: {} difference(s)", diffs.len());
                    for d in diffs {
                        eprintln!("    {d}");
                    }
                }
            }
        }
    }
    eprintln!("{compared} chart(s) compared, {failed} with differences");
    assert_eq!(failed, 0);
}
