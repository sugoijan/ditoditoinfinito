//! Calibration's menu (`docs/plans/ui-core.md`, step 6.4) as a
//! [`PageDoc`]: what the three tests do and a button to start each (the
//! run itself is the play route's, `#/calibrate/run?mode=…`), the default
//! offsets and every device's, and resetting them.
//!
//! At the Basic level (`docs/plans/ui-levels.md`, stage 2: Show advanced
//! options Off, `ddi_l10n::advanced`) its words are plain ("delays", no
//! confidence or audio path), this device's delays are one sentence in
//! place of the defaults and every device's list, and where it sends the
//! player to rows Basic hides it says how to show them.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_l10n::msg::{common, pages as pm};

use super::Env;
use super::dialog::ConfirmDialog;
use super::options_model::device_name;
use super::page::{Block, PAGE_CHANGED, PageDoc, PageFeed, PageTitle};
use crate::message::Message;
use crate::screen::Cx;
use crate::state::settings::Settings;

/// The tests, as the run's route names them.
pub const MODES: [&str; 3] = ["visual", "audio", "combined"];

/// What a button asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum CalAct {
    /// A test (`visual`, `audio`, `combined`).
    Run(String),
    /// Reset the offsets, once confirmed.
    ConfirmReset,
    Options,
    Nothing,
}

/// An offset in seconds as its messages take it: signed milliseconds
/// ("+12", "-20").
fn ms(s: f64) -> String {
    format!("{:+.0}", s * 1000.0)
}

/// What a run came to ([`run_line`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RunEnd {
    /// A device changed during it: what it measured is of nothing.
    DeviceChanged,
    /// It could not measure reliably; `out_of_range`: its clicks seemed
    /// later than it can measure.
    Unreliable { out_of_range: bool },
    /// The offset was right as it is.
    Right,
    /// It set the offset to `seconds`.
    Set { seconds: f64 },
}

/// The menu's line for a run of test `mode` (`visual`, `audio`,
/// `combined`: [`MODES`]) that came to `end`
/// (`ddi_session::calibration::settle`).
pub fn run_line(mode: &str, end: RunEnd) -> String {
    match end {
        RunEnd::DeviceChanged => pm::calibration_run_device_changed(mode),
        RunEnd::Unreliable {
            out_of_range: false,
        } => pm::calibration_run_unreliable(mode),
        RunEnd::Unreliable { out_of_range: true } => pm::calibration_run_out_of_range(mode),
        RunEnd::Right => pm::calibration_run_right(mode),
        RunEnd::Set { seconds } => pm::calibration_run_set(mode, ms(seconds)),
    }
    .into()
}

thread_local! {
    /// What the runs since the menu's last Start came to, a line each
    /// (`ddi_session::calibration::settle`): the menu shows them at its
    /// top, where the outcome card used to hold the prompt.
    static LAST_RUNS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A run ended: `line` says what it came to, on the menu.
pub fn note_last_run(line: String) {
    LAST_RUNS.with_borrow_mut(|l| l.push(line));
}

/// The lines of the runs since the menu's last Start.
pub fn last_runs() -> Vec<String> {
    LAST_RUNS.with_borrow(Clone::clone)
}

/// The menu for `settings`; `max_ms`: the largest offset a run measures;
/// `last`: what the last runs came to ([`last_runs`]).
pub fn doc(settings: &Settings, max_ms: f64, last: &[String]) -> PageDoc {
    let s = settings;
    let mut b = Vec::new();
    if !last.is_empty() {
        b.push(Block::Heading(pm::calibration_last_test().into()));
        for line in last {
            b.push(Block::Text(line.clone()));
        }
    }
    b.extend([
        Block::Text(pm::calibration_about().into()),
        Block::Text(pm::calibration_order().into()),
        Block::Muted(pm::calibration_range(max_ms.round()).into()),
        Block::Heading(pm::calibration_visual().into()),
        Block::Muted(pm::calibration_visual_about().into()),
        Block::Button {
            id: "run:visual".into(),
            label: pm::calibration_start_guided().into(),
            primary: true,
        },
        Block::Heading(pm::calibration_audio().into()),
        Block::Muted(pm::calibration_audio_about().into()),
        Block::Button {
            id: "run:audio".into(),
            label: pm::calibration_start().into(),
            primary: false,
        },
        Block::Heading(pm::calibration_combined().into()),
        Block::Muted(pm::calibration_combined_about().into()),
        Block::Button {
            id: "run:combined".into(),
            label: pm::calibration_start().into(),
            primary: false,
        },
        Block::Heading(pm::calibration_offsets().into()),
    ]);
    if ddi_l10n::advanced() {
        b.push(Block::Text(
            pm::calibration_defaults(ms(s.audio_offset), ms(s.visual_offset)).into(),
        ));
        if s.audio_profiles.is_empty() && s.display_profiles.is_empty() {
            b.push(Block::Muted(pm::calibration_no_device().into()));
        } else {
            b.push(Block::Muted(pm::calibration_per_device().into()));
            let yes_no = |calibrated: bool| if calibrated { "yes" } else { "no" };
            for p in &s.audio_profiles {
                b.push(Block::Text(
                    pm::calibration_audio_device(device_name(p), ms(p.offset), yes_no(p.calibrated))
                        .into(),
                ));
            }
            for p in &s.display_profiles {
                b.push(Block::Text(
                    pm::calibration_display_device(
                        device_name(p),
                        ms(p.offset),
                        yes_no(p.calibrated),
                    )
                    .into(),
                ));
            }
        }
    } else {
        // Basic (`docs/plans/ui-levels.md`, stage 2): this device's
        // delays in a sentence (the sound output and the display seen
        // last, else the defaults); the defaults and every device's list
        // are the Advanced level's, as their rows in Options are.
        let latest = |list: &[crate::state::settings::OffsetProfile], default: f64| {
            list.iter()
                .max_by(|a, b| a.last_seen.total_cmp(&b.last_seen))
                .map_or(default, |p| p.offset)
        };
        b.push(Block::Text(
            pm::calibration_this_device(
                ms(latest(&s.audio_profiles, s.audio_offset)),
                ms(latest(&s.display_profiles, s.visual_offset)),
            )
            .into(),
        ));
    }
    b.push(Block::Button {
        id: "reset".into(),
        label: pm::calibration_reset().into(),
        primary: false,
    });
    b.push(Block::Muted(pm::calibration_tip().into()));
    b.push(Block::Button {
        id: "options".into(),
        label: pm::calibration_options().into(),
        primary: false,
    });
    PageDoc {
        title: PageTitle::new(pm::calibration_title(), pm::calibration_heading()),
        blocks: b,
    }
}

/// The menu on the stored settings.
pub fn stored_doc() -> PageDoc {
    doc(
        &Settings::load(),
        ddi_engine::calibration::MAX_OFFSET * 1000.0,
        &last_runs(),
    )
}

/// Asks before resetting the offsets ([`reset`]); once reset, the menu
/// in `feed` is the stored settings' again.
pub fn confirm_reset(env: &Env, feed: &PageFeed, cx: &mut Cx) {
    let (title, body, yes) = reset_texts();
    let feed = feed.clone();
    cx.overlay(
        ConfirmDialog::new(
            env.clone(),
            &title,
            &body,
            &yes,
            Rc::new(move |cx| {
                let mut s = Settings::load();
                reset(&mut s);
                *feed.borrow_mut() = Some(stored_doc());
                cx.broadcast(Message::new(PAGE_CHANGED));
            }),
        ),
        true,
    );
}

/// What button `id` does.
pub fn button(id: &str) -> CalAct {
    match id {
        "reset" => CalAct::ConfirmReset,
        "options" => CalAct::Options,
        other => match other.strip_prefix("run:") {
            Some(m) if MODES.contains(&m) => {
                // A fresh flow: the last runs' lines go.
                LAST_RUNS.with_borrow_mut(Vec::clear);
                CalAct::Run(m.into())
            }
            _ => CalAct::Nothing,
        },
    }
}

/// The confirmation's title, text and button.
pub fn reset_texts() -> (String, String, String) {
    (
        pm::calibration_reset_title().into(),
        pm::calibration_reset_body().into(),
        common::reset().into(),
    )
}

/// Every offset back to 0: the defaults and each device's (kept, as not
/// calibrated). Saved.
pub fn reset(settings: &mut Settings) {
    settings.audio_offset = 0.0;
    settings.visual_offset = 0.0;
    for p in settings
        .audio_profiles
        .iter_mut()
        .chain(settings.display_profiles.iter_mut())
    {
        p.offset = 0.0;
        p.calibrated = false;
    }
    settings.save();
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_platform::DeviceFingerprint;

    #[test]
    fn the_menu_lists_the_offsets_and_resets_them() {
        let mut s = Settings {
            audio_offset: 0.012,
            ..Settings::default()
        };
        Settings::set_profile_offset(
            &mut s.audio_profiles,
            &DeviceFingerprint::new("audio:1", "Headphones"),
            -0.02,
        );
        let d = doc(&s, 250.0, &[]);
        let texts: Vec<String> = d
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|t| t.contains("audio +12 ms")),
            "{texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t == "Audio: Headphones — -20 ms (calibrated)"),
            "{texts:?}"
        );
        // Basic: this device's delays in a sentence, no list; the words
        // plain (stage 2).
        let basic = ddi_l10n::at_level(false, || doc(&s, 250.0, &[]));
        let texts: Vec<String> = basic
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Text(t) | Block::Muted(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        assert!(
            texts.contains(&"This device: sound -20 ms, picture +0 ms.".to_string()),
            "{texts:?}"
        );
        assert!(texts.iter().all(|t| !t.contains("Headphones")), "{texts:?}");
        assert!(texts.iter().all(|t| !t.contains("offset")), "{texts:?}");
        assert!(
            texts.iter().any(|t| t.contains("Show advanced options")),
            "the hidden rows are said where they are: {texts:?}"
        );
        assert_eq!(button("run:audio"), CalAct::Run("audio".into()));
        assert_eq!(button("run:nope"), CalAct::Nothing);
        assert_eq!(button("reset"), CalAct::ConfirmReset);
        reset(&mut s);
        assert_eq!(s.audio_offset, 0.0);
        assert_eq!(s.audio_profiles[0].offset, 0.0);
        assert!(!s.audio_profiles[0].calibrated);
    }
}
