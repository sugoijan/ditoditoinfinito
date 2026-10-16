//! What the play history keeps of a play (`docs/plans/play-history.md`),
//! built the same way by both shells from the session's [`PlayFacts`]:
//! what it was of, every parameter it ran with, its timing and its devices.

use ddi_chart::Song;
use ddi_engine::player::Results;
use ddi_platform::DeviceProfile;
use ddi_ui::state::history::{
    Autoplay, ChartKey, ClockSetup, Device, DrawSetup, Ending, OffsetSource, PlayRecord, PlaySetup,
    RECORD_VERSION,
};
use ddi_ui::state::settings::{OffsetProfile, Settings};

use crate::play::PlayFacts;

/// What a shell knows of a play beyond the session's facts.
pub struct Played<'a> {
    /// The song's id, and the song as stored (unshifted).
    pub song_id: &'a str,
    pub song: &'a Song,
    pub chart: usize,
    pub settings: &'a Settings,
    /// The settings as stored, so fields this version does not know are
    /// kept (`None`: `settings` written out).
    pub stored_settings: Option<serde_json::Value>,
    pub devices: Option<&'a DeviceProfile>,
    /// The display's refresh rate as the play started.
    pub refresh_hz: f64,
    /// The graphics backend's name.
    pub graphics: String,
    /// The song offset the play was shifted by, seconds.
    pub song_offset: f64,
    pub autoplay: Option<Autoplay>,
    /// Milliseconds since the Unix epoch.
    pub time: f64,
    pub app_version: &'a str,
}

/// A setting's name as stored (its serde name).
fn setting_name(v: &impl serde::Serialize) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => String::new(),
    }
}

/// The record of a play; `None` when the chart is not the song's.
pub fn play_record(p: &Played, results: &Results, facts: &PlayFacts) -> Option<PlayRecord> {
    // The chart as stored (the song offset is kept apart).
    if p.chart >= p.song.charts.len() {
        return None;
    }
    let digest = p.song.chart_digest(p.chart);
    let settings = p.settings;
    let source = |o: Option<&OffsetProfile>| match o {
        Some(o) if o.calibrated => OffsetSource::Calibrated,
        Some(_) => OffsetSource::Profile,
        None => OffsetSource::Default,
    };
    let devices = p.devices;
    let clock = ClockSetup {
        audio_offset: facts.options.clock.audio_offset,
        audio_source: source(devices.and_then(|d| settings.audio_profile(&d.audio.id))),
        visual_offset: facts.options.clock.visual_offset,
        visual_source: source(devices.and_then(|d| settings.display_profile(&d.display.id))),
        audio: devices.map(|d| Device {
            id: d.audio.id.clone(),
            label: d.audio.label.clone(),
        }),
        display: devices.map(|d| Device {
            id: d.display.id.clone(),
            label: d.display.label.clone(),
        }),
        output_latency: facts.output_latency,
        sample_rate: facts.sample_rate,
        refresh_hz: p.refresh_hz,
        latency_min: facts.latency_min,
        latency_max: facts.latency_max,
    };
    let drawn = DrawSetup {
        // The skin the chart's layout is drawn with (pump's own choice
        // for a pump layout).
        noteskin: settings
            .noteskin_for(&p.song.charts[p.chart].layout)
            .map(str::to_string),
        note_colors: setting_name(&settings.note_colors),
        chart_colors: settings.chart_colors,
        field_filter: settings.field_filter,
        bg_brightness: settings.bg_brightness,
        background_frame: setting_name(&settings.background_frame),
        video: setting_name(&settings.video),
        show_deltas: settings.show_deltas,
        graphics: p.graphics.clone(),
    };
    Some(PlayRecord {
        version: RECORD_VERSION,
        chart: ChartKey {
            song: p.song_id.to_string(),
            digest,
            index: p.chart as u32,
        },
        ruleset: facts.ruleset.clone(),
        options: facts.options,
        song_offset: p.song_offset,
        setup: PlaySetup {
            clock,
            drawn,
            inputs: facts.inputs.clone(),
        },
        settings: p
            .stored_settings
            .clone()
            .unwrap_or_else(|| serde_json::to_value(settings).unwrap_or_default()),
        ended: if results.failed {
            Ending::Failed
        } else {
            Ending::Finished
        },
        autoplay: p.autoplay,
        time: p.time,
        app_version: p.app_version.to_string(),
        judge_version: ddi_engine::JUDGE_VERSION,
    })
}
