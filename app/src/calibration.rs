//! Calibration through the real gameplay path.
//!
//! A generated 120 BPM chart with a click baked into the audio at every note
//! is played with a wide-window ruleset so each hit's signed error is
//! recorded. While the test runs, the session applies the mean error of each
//! batch of hits as a temporary offset and keeps measuring, until the
//! residual mean is no longer statistically distinguishable from zero
//! (|mean| ≤ 2σ/√n) or the chart ends. Saving adds the accumulated
//! correction to the setting.
//!
//! The two offsets are separable: changing the audio offset moves the judged
//! timeline and the drawn arrows together, so the **arrows-only** (muted)
//! test measures display lag independently of the audio offset, and the
//! **sound-only** test (arrows hidden, any key accepted) measures the audio
//! path independently of the visual offset. The **combined** test is a check
//! that should read near zero after both; a significant remainder can be
//! applied as a fine-tune to the audio offset.

use ddi_chart::{
    Chart, Difficulty, DisplayBpm, Note, NoteKind, Song, SourceFormat, SourceInfo, Tick, TimingMap,
};
use web_sys::AudioBuffer;

use crate::settings::Settings;
use crate::web::audio::WebAudio;

/// Beat grid. Notes sit on beats 0, 1 and 2 of every four-beat group with a
/// rest on beat 3: gaps of 0.6 s, 0.6 s and 1.2 s. Every gap exceeds twice
/// `MAX_OFFSET` (0.5 s) so windows never overlap, and the pattern is not
/// periodic at the note spacing, so a press stream one note late lines up
/// after the short gaps but misses after every rest and is flagged.
pub(crate) const BPM: f64 = 100.0;
/// Beats of lead-in (clicks only, no notes): one full group.
pub(crate) const LEAD_IN_BEATS: i64 = 4;
/// Groups of three notes; the test ends early on convergence.
pub(crate) const GROUPS: i64 = 24;
pub(crate) const NOTES: i64 = GROUPS * 3;
/// Lane used by the sound-only test (any key maps to it).
pub(crate) const AUDIO_LANE: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CalMode {
    Combined,
    Audio,
    Visual,
}

impl CalMode {
    pub(crate) fn parse(s: &str) -> CalMode {
        match s {
            "audio" => CalMode::Audio,
            "visual" => CalMode::Visual,
            _ => CalMode::Combined,
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            CalMode::Combined => "combined",
            CalMode::Audio => "audio",
            CalMode::Visual => "visual",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            CalMode::Combined => "Calibration check: sound and arrows",
            CalMode::Audio => "Calibration 2/3: sound only",
            CalMode::Visual => "Calibration 1/3: arrows only (muted)",
        }
    }

    pub(crate) fn hint(self) -> &'static str {
        match self {
            CalMode::Combined => {
                "play normally; the test adjusts itself and stops once it converges"
            }
            CalMode::Audio => {
                "press ANY arrow key on each click; the test adjusts itself and stops once it converges"
            }
            CalMode::Visual => {
                "hit each arrow as it reaches the receptor; the test adjusts itself and stops once it converges"
            }
        }
    }

    /// The step that follows in the guided flow.
    pub(crate) fn next(self) -> Option<CalMode> {
        match self {
            CalMode::Visual => Some(CalMode::Audio),
            CalMode::Audio => Some(CalMode::Combined),
            CalMode::Combined => None,
        }
    }

    pub(crate) fn hide_notes(self) -> bool {
        self == CalMode::Audio
    }

    pub(crate) fn muted(self) -> bool {
        self == CalMode::Visual
    }

    /// Whether any bound key should count for the single calibration lane.
    pub(crate) fn any_key(self) -> bool {
        self == CalMode::Audio
    }

    /// Which setting the correction is added to.
    pub(crate) fn target(self) -> &'static str {
        match self {
            CalMode::Combined | CalMode::Audio => "audio offset",
            CalMode::Visual => "visual offset",
        }
    }

    /// Adds `correction` to the offset in effect: the active device's
    /// profile when one is known, else the default for unknown devices.
    pub(crate) fn apply(
        self,
        settings: &mut Settings,
        devices: Option<&ddi_platform::DeviceProfile>,
        correction: f64,
    ) {
        match self {
            CalMode::Combined | CalMode::Audio => match devices {
                Some(d) => {
                    let current = settings
                        .audio_profile(&d.audio.id)
                        .map(|p| p.offset)
                        .unwrap_or(settings.audio_offset);
                    Settings::set_profile_offset(
                        &mut settings.audio_profiles,
                        &d.audio,
                        current + correction,
                    );
                }
                None => {
                    settings.audio_offset = (settings.audio_offset + correction).clamp(-0.5, 0.5)
                }
            },
            CalMode::Visual => match devices {
                Some(d) => {
                    let current = settings
                        .display_profile(&d.display.id)
                        .map(|p| p.offset)
                        .unwrap_or(settings.visual_offset);
                    Settings::set_profile_offset(
                        &mut settings.display_profiles,
                        &d.display,
                        current + correction,
                    );
                }
                None => {
                    settings.visual_offset = (settings.visual_offset + correction).clamp(-0.5, 0.5)
                }
            },
        }
    }
}

pub(crate) use ddi_engine::calibration::{Calibrator, MAX_OFFSET, MIN_RESIDUAL, MIN_STEP, Outcome};

/// The generated song: beat 0 at 0 s, notes on beats `LEAD_IN_BEATS..`.
pub(crate) fn song(mode: CalMode) -> Song {
    let lanes = [0u8, 1, 2, 3, 1, 0, 3, 2];
    let notes = (0..NOTES)
        .map(|i| {
            let lane = if mode.any_key() {
                AUDIO_LANE
            } else {
                lanes[(i as usize) % lanes.len()]
            };
            // Three notes then a rest: beats 0, 1, 2 of each four-beat group.
            let beat = LEAD_IN_BEATS + (i / 3) * 4 + (i % 3);
            Note::new(Tick::from_beats(beat), lane, NoteKind::Tap)
        })
        .collect();
    let timing = TimingMap::constant(BPM, 0.0);
    Song {
        title: "Calibration".into(),
        subtitle: String::new(),
        artist: String::new(),
        title_translit: String::new(),
        subtitle_translit: String::new(),
        artist_translit: String::new(),
        genre: String::new(),
        credit: String::new(),
        music: None,
        preview_start: 0.0,
        preview_length: 0.0,
        banner: None,
        background: None,
        jacket: None,
        cd_title: None,
        timing,
        display_bpm: DisplayBpm::Actual,
        charts: vec![Chart {
            layout: "dance-single".into(),
            difficulty: Difficulty::Beginner,
            meter: 1,
            name: "calibration".into(),
            description: String::new(),
            credit: String::new(),
            notes,
            timing: None,
            display_bpm: None,
        }],
        effects: Vec::new(),
        keysounds: Vec::new(),
        layouts: Vec::new(),
        source: SourceInfo {
            format: SourceFormat::Other("generated".into()),
            unknown_tags: Vec::new(),
        },
    }
}

/// Mono buffer with a sharp click at every note (and a softer one on each
/// lead-in beat), 1 s of silence after the last note.
pub(crate) fn click_track(audio: &WebAudio, song: &Song) -> Option<AudioBuffer> {
    let ctx = audio.context();
    let rate = ctx.sample_rate();
    let beat = 60.0 / BPM;
    let total = (LEAD_IN_BEATS + GROUPS * 4) as f64 * beat + 1.0;
    let len = (total * rate as f64) as usize;
    let mut data = vec![0.0f32; len];
    let mut click = |at: f64, amp: f32, hz: f32| {
        let start = (at * rate as f64) as usize;
        let n = (rate * 0.03) as usize;
        for i in 0..n {
            let Some(v) = data.get_mut(start + i) else {
                break;
            };
            let t = i as f32 / rate;
            let env = (-t * 300.0).exp();
            *v += (t * hz * std::f32::consts::TAU).sin() * env * amp;
        }
    };
    for b in 0..LEAD_IN_BEATS {
        click(b as f64 * beat, 0.35, 660.0);
    }
    for n in &song.charts[0].notes {
        click(song.timing.seconds_at(n.tick), 0.9, 1000.0);
    }
    let buffer = ctx.create_buffer(1, len as u32, rate).ok()?;
    buffer.copy_to_channel(&data, 0).ok()?;
    Some(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    /// Gaps must all exceed twice the judge window (no overlap) and must not
    /// be uniform (a press stream one note off has to produce misses).
    #[wasm_bindgen_test]
    fn chart_is_non_periodic_and_windows_never_overlap() {
        let song = song(CalMode::Combined);
        let t: Vec<f64> = song.charts[0]
            .notes
            .iter()
            .map(|n| song.timing.seconds_at(n.tick))
            .collect();
        let gaps: Vec<f64> = t.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.iter().all(|g| *g > 2.0 * MAX_OFFSET + 0.05));
        assert!((gaps[0] - 0.6).abs() < 1e-9 && (gaps[2] - 1.2).abs() < 1e-9);
    }
}
