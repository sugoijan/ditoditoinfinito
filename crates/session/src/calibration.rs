//! Calibration through the real gameplay path (moved from
//! `crates/web/src/calibration.rs` in `docs/plans/desktop.md` step 7.1).
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
use ddi_platform::AudioBackend;
use ddi_ui::state::settings::Settings;

/// Beat grid. Notes sit on beats 0, 1 and 2 of every four-beat group with a
/// rest on beat 3: gaps of 0.6 s, 0.6 s and 1.2 s. Every gap exceeds twice
/// `MAX_OFFSET` (0.5 s) so windows never overlap, and the pattern is not
/// periodic at the note spacing, so a press stream one note late lines up
/// after the short gaps but misses after every rest and is flagged.
pub const BPM: f64 = 100.0;
/// Beats of lead-in (clicks only, no notes): one full group.
pub const LEAD_IN_BEATS: i64 = 4;
/// Groups of three notes; the test ends early on convergence.
pub const GROUPS: i64 = 24;
pub const NOTES: i64 = GROUPS * 3;
/// Lane used by the sound-only test (any key maps to it).
pub const AUDIO_LANE: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalMode {
    Combined,
    Audio,
    Visual,
}

impl CalMode {
    pub fn parse(s: &str) -> CalMode {
        match s {
            "audio" => CalMode::Audio,
            "visual" => CalMode::Visual,
            _ => CalMode::Combined,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            CalMode::Combined => "combined",
            CalMode::Audio => "audio",
            CalMode::Visual => "visual",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            CalMode::Combined => "Calibration check: sound and arrows",
            CalMode::Audio => "Calibration 2/3: sound only",
            CalMode::Visual => "Calibration 1/3: arrows only (muted)",
        }
    }

    pub fn hint(self) -> &'static str {
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
    pub fn next(self) -> Option<CalMode> {
        match self {
            CalMode::Visual => Some(CalMode::Audio),
            CalMode::Audio => Some(CalMode::Combined),
            CalMode::Combined => None,
        }
    }

    pub fn hide_notes(self) -> bool {
        self == CalMode::Audio
    }

    pub fn muted(self) -> bool {
        self == CalMode::Visual
    }

    /// Whether any bound key should count for the single calibration lane.
    pub fn any_key(self) -> bool {
        self == CalMode::Audio
    }

    /// Which setting the correction is added to.
    pub fn target(self) -> &'static str {
        match self {
            CalMode::Combined | CalMode::Audio => "audio offset",
            CalMode::Visual => "visual offset",
        }
    }

    /// Adds `correction` to the offset in effect: the active device's
    /// profile when one is known, else the default for unknown devices.
    pub fn apply(
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

pub use ddi_engine::calibration::{Calibrator, MAX_OFFSET, MIN_RESIDUAL, MIN_STEP, Outcome};

/// What a test's result says (the web's result card and desktop's page
/// say it the same way).
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// "+12.3 ms ≈ 0.74 frames at 60 Hz", or "unreliable".
    pub headline: String,
    /// The hits, those left out, the spread.
    pub detail: String,
    /// Why it stopped, how much to trust it.
    pub status: String,
    /// What saving does ("Nothing to change." when nothing would).
    pub verdict: String,
    /// Saving it would change something worth changing.
    pub meaningful: bool,
}

/// The report of test `mode`'s outcome `o` (`refresh_hz`: the display's,
/// for the arrows-only test's frames).
pub fn report(o: &Outcome, mode: CalMode, refresh_hz: f64) -> Report {
    let total_ms = o.total() * 1000.0;
    let frames = if mode == CalMode::Visual {
        format!(
            " ≈ {:.2} frames at {refresh_hz:.0} Hz",
            o.total() * refresh_hz
        )
    } else {
        String::new()
    };
    let ci_ms = o.confidence() * 1000.0;
    let enough = o.hits >= MIN_RESIDUAL as u32;
    let unusable = o.out_of_range || o.unreliable();
    let meaningful = enough
        && !unusable
        && o.total().abs() >= MIN_STEP
        && (o.rounds > 0 || o.residual_significant());
    let status = if o.out_of_range {
        format!(
            "The measured offset is beyond ±{:.0} ms, which this test cannot tell apart from \
             hitting the neighbouring note. If you really hear the clicks that late (some \
             Bluetooth headphones do), set the audio offset by hand in the options and run the \
             test again from there.",
            MAX_OFFSET * 1000.0
        )
    } else if o.high_miss_rate() {
        format!(
            "{} of {} notes were missed, so the result is unreliable. Presses more than {:.0} ms \
             off do not count, and a press stream one note late misses after every rest. If you \
             hear the clicks later than that, set the offset by hand first; otherwise try again \
             and hit every note.",
            o.misses,
            o.hits + o.misses,
            MAX_OFFSET * 1000.0
        )
    } else if !enough {
        "Too few hits to measure; try again and hit every note.".to_string()
    } else if o.unreliable() {
        format!(
            "Spread is too high (σ {:.0} ms) to trust the mean. Try again, keeping a steady rhythm.",
            o.residual_sd * 1000.0
        )
    } else if o.converged {
        format!(
            "Converged after {} adjustment{}: the remaining error ({:+.1} ms ± {:.1}) is within noise.",
            o.rounds,
            if o.rounds == 1 { "" } else { "s" },
            o.residual_mean * 1000.0,
            ci_ms
        )
    } else if o.residual_significant() {
        format!(
            "Ran out of notes before converging; the remaining error ({:+.1} ms ± {:.1}) is \
             still significant. Try again, keeping a steady rhythm.",
            o.residual_mean * 1000.0,
            ci_ms
        )
    } else {
        "Finished; the remaining error is within noise.".to_string()
    };
    let verdict = if meaningful {
        format!(
            "Saving adds {:+.1} ms to the {}, so these hits count as on time.",
            total_ms,
            mode.target()
        )
    } else {
        "Nothing to change.".to_string()
    };
    Report {
        headline: if unusable {
            "unreliable".to_string()
        } else {
            format!("{total_ms:+.1} ms{frames}")
        },
        detail: format!(
            "{} hits ({} warm-up/adaptation ignored, {} extremes trimmed) · {} missed · \
             residual σ {:.1} ms · 95% CI ±{:.1} ms",
            o.hits,
            o.ignored,
            o.trimmed,
            o.misses,
            o.residual_sd * 1000.0,
            ci_ms
        ),
        status,
        verdict,
        meaningful,
    }
}

/// A test's outcome as the start prompt's card shows it (the web's
/// `calibration_card`): the offset measured, how it was measured, what
/// saving does, the offsets in effect on `devices`, and Save, the guided
/// flow's next, Again and Done (`save`, `next`, `again`, `done`).
pub fn card(
    o: &Outcome,
    mode: CalMode,
    settings: &Settings,
    devices: Option<&ddi_platform::DeviceProfile>,
    refresh_hz: f64,
    saved: bool,
    device_changed: bool,
) -> ddi_ui::screens::prompt::PromptStep {
    let r = report(o, mode, refresh_hz);
    let (audio_now, visual_now) = match devices {
        Some(d) => (
            settings
                .audio_profile(&d.audio.id)
                .map_or(settings.audio_offset, |p| p.offset),
            settings
                .display_profile(&d.display.id)
                .map_or(settings.visual_offset, |p| p.offset),
        ),
        None => (settings.audio_offset, settings.visual_offset),
    };
    let line = |s: String, color: &str| (s, color.to_string());
    let mut lines = vec![
        line(r.detail, "Muted"),
        line(format!("{} {}", r.status, r.verdict), "Text"),
        line(
            format!(
                "offsets in effect: audio {:+.0} ms · visual {:+.0} ms",
                audio_now * 1000.0,
                visual_now * 1000.0
            ),
            "Muted",
        ),
    ];
    if let Some(d) = devices {
        lines.push(line(
            format!("audio: {} · display: {}", d.audio.label, d.display.label),
            "Muted",
        ));
    }
    if device_changed {
        lines.push(line(
            "a device changed during the test; discard this result".into(),
            "Danger",
        ));
    }
    if saved {
        lines.push(line(format!("Saved to the {}.", mode.target()), "Accent"));
    }
    let next = mode.next().map(|m| {
        let label = match m {
            CalMode::Audio => "Next: sound only",
            CalMode::Combined => "Next: check with both",
            CalMode::Visual => "Next",
        };
        ("next".to_string(), label.to_string())
    });
    let mut actions = Vec::new();
    if r.meaningful && !saved {
        actions.push(("save".to_string(), format!("Save to {}", mode.target())));
    }
    if saved || !r.meaningful {
        actions.extend(next);
    }
    actions.push(("again".to_string(), "Again".to_string()));
    actions.push(("done".to_string(), "Done".to_string()));
    let unusable = o.out_of_range || o.unreliable();
    ddi_ui::screens::prompt::PromptStep::Card {
        heading: if unusable {
            "Unreliable".to_string()
        } else {
            r.headline
        },
        lines,
        actions,
    }
}

/// The generated song: beat 0 at 0 s, notes on beats `LEAD_IN_BEATS..`.
pub fn song(mode: CalMode) -> Song {
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
            derivation: None,
            layout: "dance-single".into(),
            difficulty: Difficulty::Beginner,
            meter: 1,
            name: "calibration".into(),
            description: String::new(),
            credit: String::new(),
            notes,
            timing: None,
            display_bpm: None,
            danoni: None,
            ffr: false,
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

/// The click on a note: its loudness and pitch (the editor's metronome
/// clicks it on each measure's first beat, `crate::listen`).
pub const NOTE_CLICK: (f32, f32) = (0.9, 1000.0);
/// The softer click on a lead-in beat (the metronome's on other beats).
pub const BEAT_CLICK: (f32, f32) = (0.35, 660.0);

/// One click at `rate`, `(amp, hz)`: 30 ms of a sine dying away fast,
/// loud from its first sample (the beat).
pub fn click(rate: f32, (amp, hz): (f32, f32)) -> Vec<f32> {
    let n = (rate * 0.03) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate;
            let env = (-t * 300.0).exp();
            (t * hz * std::f32::consts::TAU).sin() * env * amp
        })
        .collect()
}

/// The click track's samples at `rate`: a sharp click at every note (and
/// a softer one on each lead-in beat), 1 s of silence after the last note.
pub fn click_samples(rate: f32, song: &Song) -> Vec<f32> {
    let beat = 60.0 / BPM;
    let total = (LEAD_IN_BEATS + GROUPS * 4) as f64 * beat + 1.0;
    let len = (total * rate as f64) as usize;
    let mut data = vec![0.0f32; len];
    let (soft, sharp) = (click(rate, BEAT_CLICK), click(rate, NOTE_CLICK));
    let mut add = |at: f64, click: &[f32]| {
        let start = (at * rate as f64) as usize;
        for (v, c) in data.iter_mut().skip(start).zip(click) {
            *v += c;
        }
    };
    for b in 0..LEAD_IN_BEATS {
        add(b as f64 * beat, &soft);
    }
    for n in &song.charts[0].notes {
        add(song.timing.seconds_at(n.tick), &sharp);
    }
    data
}

/// The click track as a mono buffer of `audio`, at its rate.
pub fn click_track<A: AudioBackend>(audio: &A, song: &Song) -> Option<A::Buffer> {
    let rate = audio.sample_rate() as f32;
    audio
        .buffer_from_pcm(vec![click_samples(rate, song)], f64::from(rate))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gaps must all exceed twice the judge window (no overlap) and must not
    /// be uniform (a press stream one note off has to produce misses).
    #[test]
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

    #[test]
    fn the_sound_only_test_uses_one_lane_and_the_track_clicks_on_notes() {
        let song = song(CalMode::Audio);
        assert!(song.charts[0].notes.iter().all(|n| n.lane == AUDIO_LANE));
        assert_eq!(song.charts[0].notes.len() as i64, NOTES);
        let data = click_samples(48_000.0, &song);
        let beat = 60.0 / BPM;
        let total = (LEAD_IN_BEATS + GROUPS * 4) as f64 * beat + 1.0;
        assert_eq!(data.len(), (total * 48_000.0) as usize);
        // Loud at the first note, silent just before it.
        let at = (song.timing.seconds_at(song.charts[0].notes[0].tick) * 48_000.0) as usize;
        assert!(data[at..at + 200].iter().any(|s| s.abs() > 0.5));
        assert!(data[at - 2000..at].iter().all(|s| s.abs() < 1e-3));
    }
}
