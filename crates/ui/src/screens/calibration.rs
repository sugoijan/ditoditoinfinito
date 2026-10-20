//! Calibration's menu (`docs/plans/ui-core.md`, step 6.4) as a
//! [`PageDoc`]: what the three tests do and a button to start each (the
//! run itself is the play route's, `#/calibrate/run?mode=…`), the default
//! offsets and every device's, and resetting them.

use std::rc::Rc;

use super::Env;
use super::options_dialogs::ConfirmDialog;
use super::page::{Block, PAGE_CHANGED, PageDoc, PageFeed};
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

fn ms(s: f64) -> String {
    format!("{:+.0} ms", s * 1000.0)
}

/// The menu for `settings`; `max_ms`: the largest offset a run measures.
pub fn doc(settings: &Settings, max_ms: f64) -> PageDoc {
    let s = settings;
    let mut b = vec![
        Block::Text(
            "Each test plays a generated chart (100 BPM, three clicks and a rest, a click on \
             every note) exactly like a song. While you play, the test measures how early or \
             late your hits are, applies the correction on the fly and keeps measuring until \
             the remaining error is within noise (95% confidence), then stops. Saving adds the \
             correction to the setting."
                .into(),
        ),
        Block::Text(
            "The two offsets are independent, so the guided order is: arrows only (display), \
             then sound only (audio path), then a check with both that should read close to \
             zero."
                .into(),
        ),
        Block::Muted(format!(
            "Offsets beyond ±{max_ms:.0} ms cannot be measured this way (a press that late \
             cannot be told apart from one on the next note); set those by hand in the options, \
             then calibrate from there."
        )),
        Block::Heading("1. Arrows only".into()),
        Block::Muted(
            "Muted. Hit each arrow at the receptor. Measures display lag; saves to the visual \
             offset (only moves what is drawn)."
                .into(),
        ),
        Block::Button {
            id: "run:visual".into(),
            label: "Start guided calibration".into(),
            primary: true,
        },
        Block::Heading("2. Sound only".into()),
        Block::Muted(
            "Arrows hidden; press any arrow key on each click. Measures the audio path; saves \
             to the audio offset (the judged timeline)."
                .into(),
        ),
        Block::Button {
            id: "run:audio".into(),
            label: "Start".into(),
            primary: false,
        },
        Block::Heading("3. Check: both".into()),
        Block::Muted(
            "Play normally. Should read near zero after 1 and 2; a significant remainder can be \
             saved as a fine-tune to the audio offset."
                .into(),
        ),
        Block::Button {
            id: "run:combined".into(),
            label: "Start".into(),
            primary: false,
        },
        Block::Heading("Offsets".into()),
        Block::Text(format!(
            "Defaults, for devices without a profile: audio {}, visual {}.",
            ms(s.audio_offset),
            ms(s.visual_offset)
        )),
    ];
    if s.audio_profiles.is_empty() && s.display_profiles.is_empty() {
        b.push(Block::Muted("No device calibrated yet.".into()));
    } else {
        b.push(Block::Muted(
            "Offsets are kept per audio path and per display; the game picks the matching one \
             when a song starts."
                .into(),
        ));
        for (kind, list) in [
            ("Audio", &s.audio_profiles),
            ("Display", &s.display_profiles),
        ] {
            for p in list.iter() {
                b.push(Block::Text(format!(
                    "{kind}: {} — {}{}",
                    p.label,
                    ms(p.offset),
                    if p.calibrated { " (calibrated)" } else { "" }
                )));
            }
        }
    }
    b.push(Block::Button {
        id: "reset".into(),
        label: "Reset offsets to 0".into(),
        primary: false,
    });
    b.push(Block::Muted(
        "Tip: turn on “Timing error of each hit” in the options to see each hit's offset \
         during normal play; the debug overlay shows the running mean."
            .into(),
    ));
    b.push(Block::Button {
        id: "options".into(),
        label: "Options".into(),
        primary: false,
    });
    PageDoc {
        title: "Calibration".into(),
        blocks: b,
    }
}

/// The menu on the stored settings.
pub fn stored_doc() -> PageDoc {
    doc(
        &Settings::load(),
        ddi_engine::calibration::MAX_OFFSET * 1000.0,
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
            Some(m) if MODES.contains(&m) => CalAct::Run(m.into()),
            _ => CalAct::Nothing,
        },
    }
}

/// The confirmation's title, text and button.
pub fn reset_texts() -> (String, String, String) {
    (
        "Reset the offsets?".into(),
        "The default audio and visual offsets and every device's go back to 0 ms; the devices \
         stay known, as not calibrated."
            .into(),
        "Reset".into(),
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
        let d = doc(&s, 250.0);
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
        assert_eq!(button("run:audio"), CalAct::Run("audio".into()));
        assert_eq!(button("run:nope"), CalAct::Nothing);
        assert_eq!(button("reset"), CalAct::ConfirmReset);
        reset(&mut s);
        assert_eq!(s.audio_offset, 0.0);
        assert_eq!(s.audio_profiles[0].offset, 0.0);
        assert!(!s.audio_profiles[0].calibrated);
    }
}
