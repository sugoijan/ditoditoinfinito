//! Every row of Options, as it was before the sections were regrouped
//! (`docs/plans/ui-levels.md`, stage 3), held to what the model makes now:
//! the same rows at Basic and at Advanced, each with the same label,
//! choices, value, level and default, and the same effect on the settings
//! when it is changed. Only a row's section, its place in it and the
//! heading over it may differ.
//!
//! The record (`options_rows_before.tsv`) was written by the code before
//! the regrouping; write it again only on purpose, for a change that adds,
//! removes or renames a row: `DDI_WRITE_OPTIONS_ROWS=<file> cargo test -p
//! ddi-ui --lib options_rows_tests`.

use std::collections::BTreeSet;
use std::rc::Rc;

use ddi_platform::storage::{self, MemoryStore};

use super::data::{OptionKind, Platform};
use super::options_levels::CHANGED;
use super::options_model::{OptionsModel, World};
use crate::state::settings::{Level, Settings};

const BEFORE: &str = include_str!("options_rows_before.tsv");

/// The worlds recorded: Options as full as the model makes them (both
/// shells, under DDI and under StepMania's rules), and a new player's on
/// each shell.
fn worlds() -> Vec<(String, OptionsModel)> {
    let mut out: Vec<(String, OptionsModel)> = super::options_levels::tests::fullest()
        .into_iter()
        .map(|m| {
            let name = format!("full-{:?}-{}", m.world.platform, m.settings.ruleset);
            (name, m)
        })
        .collect();
    for platform in [Platform::Web, Platform::Desktop] {
        let world = World {
            platform,
            pads_supported: true,
            sample: super::options_model::Sample::Ready,
            folders: (platform == Platform::Desktop).then(|| vec!["/Songs".into()]),
            ..World::default()
        };
        out.push((
            format!("new-{platform:?}"),
            OptionsModel::new(Settings::default(), world),
        ));
    }
    out
}

/// A short, stable digest of a text (FNV-1a).
fn digest(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The settings after row `id` of `m` moves to value `to`, as stored.
fn effect(m: &OptionsModel, id: &str, to: usize) -> String {
    let mut copy = OptionsModel::new(m.settings.clone(), m.world.clone());
    copy.change(id, to);
    let value = serde_json::to_value(&copy.settings).expect("settings as JSON");
    digest(&value.to_string())
}

/// Every row as lines: at Advanced the row's section (for the map, never
/// compared), id, level, label, choices, value, default and the effect of
/// its next value; at Basic its id, and each world's changed count.
fn lines() -> Vec<String> {
    let mut out = Vec::new();
    for (name, m) in worlds() {
        for level in [Level::Advanced, Level::Basic] {
            let d = m.data_at(level);
            for s in &d.sections {
                for o in &s.options {
                    if o.id.starts_with(CHANGED) {
                        continue;
                    }
                    if level == Level::Basic {
                        out.push(format!("{name}\tBasic\t{}", o.id));
                        continue;
                    }
                    // Long lists (the speeds, the percents) by a digest.
                    let values = match o.values().join("|") {
                        v if v.chars().count() > 80 => {
                            format!("{} values #{}", o.values().len(), digest(&v))
                        }
                        v => v,
                    };
                    let change = match o.kind {
                        OptionKind::Actions(_) => "-".to_string(),
                        _ if o.values().len() < 2 => "-".to_string(),
                        _ => effect(&m, &o.id, (o.index + 1) % o.values().len()),
                    };
                    out.push(format!(
                        "{name}\tAdvanced\t{}\t{:?}\t{}\t{values}\t{}\t{:?}\t{change}\tin {}",
                        o.id, o.level, o.label, o.index, o.default, s.id
                    ));
                }
            }
            if level == Level::Basic {
                // The changed settings Basic counts: the same whatever
                // sections count them, so their sum is compared.
                let counted: usize = d
                    .sections
                    .iter()
                    .flat_map(|s| &s.options)
                    .filter(|o| o.id.starts_with(CHANGED))
                    .map(|o| {
                        let digits: String = o.label.chars().filter(char::is_ascii_digit).collect();
                        digits.parse::<usize>().expect("a count")
                    })
                    .sum();
                out.push(format!("{name}\tBasic\tchanged {counted}"));
            }
        }
    }
    out
}

/// A line without its section (`in …`), which the regrouping may change.
fn comparable(line: &str) -> &str {
    line.rsplit_once("\tin ").map_or(line, |(row, _)| row)
}

#[test]
fn every_row_is_kept_with_its_key_value_and_effect() {
    storage::install(Rc::new(MemoryStore::default()));
    let now = lines();
    if let Ok(path) = std::env::var("DDI_WRITE_OPTIONS_ROWS") {
        std::fs::write(&path, now.join("\n") + "\n").expect("write the record");
        return;
    }
    let before: BTreeSet<&str> = BEFORE.lines().map(comparable).collect();
    let after: BTreeSet<&str> = now.iter().map(|l| comparable(l)).collect();
    let lost: Vec<&&str> = before.difference(&after).collect();
    let new: Vec<&&str> = after.difference(&before).collect();
    assert!(
        lost.is_empty() && new.is_empty(),
        "rows lost or changed:\n{}\nrows new or changed:\n{}",
        lost.iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        new.iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    assert_eq!(
        before.len(),
        BEFORE.lines().count(),
        "no row twice in a world"
    );
    assert_eq!(after.len(), now.len(), "no row twice in a world");
}
