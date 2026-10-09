//! Player settings persisted in `localStorage`.

use std::collections::BTreeMap;

pub(crate) use ddi_engine::PadBindings;
use ddi_engine::clock::ClockOptions;
use ddi_engine::player::PlayOptions;
use ddi_engine::rules::danoni::DanoniOptions;
use ddi_engine::rules::presets::RulesetMode;
use ddi_engine::scroll::{ScrollAction, ScrollOptions, SpeedMod, SpeedSource};
use ddi_engine::{Appearance, ControlBindings, TransformOptions};
use gloo::storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

const KEY: &str = "ddi.settings.v1";

/// Song offsets closer to zero than this (half a millisecond) are dropped.
const SONG_OFFSET_EPSILON: f64 = 0.0005;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    /// X-mod multiplier.
    pub(crate) speed: f64,
    /// The speed above everywhere, or a chart's own when it has one.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub(crate) speed_source: SpeedSource,
    pub(crate) reverse: bool,
    /// Boost / Brake / Wave: how notes move on their way to the receptors.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub(crate) scroll_action: ScrollAction,
    /// Turn and simplifications applied to the chart of every song (not to
    /// calibration). Simplified plays are marked as assisted.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub(crate) transform: TransformOptions,
    /// Hidden / Sudden / Stealth (drawing only).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub(crate) appearance: Appearance,
    /// The import report also lists details for pack authors:
    /// inconsistencies that make no difference in play.
    pub(crate) import_details: bool,
    /// Copy a picked folder's songs into the browser even where they can
    /// be linked (Chromium), so they survive the folder moving.
    pub(crate) import_copy: bool,
    /// How notes are coloured.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub(crate) note_colors: NoteColors,
    /// Draw notes in the colours a chart gives them (Dancing☆Onigiri
    /// works) instead of `note_colors` (decision 23: off by default).
    pub(crate) chart_colors: bool,
    /// Show the lyrics of songs that have them (Dancing☆Onigiri works).
    pub(crate) lyrics: bool,
    /// Song background brightness, 0..=1; 0 turns the background off.
    pub(crate) bg_brightness: f32,
    /// Darkening behind the lanes, one of [`FIELD_FILTERS`] (0 = off).
    pub(crate) field_filter: f32,
    /// Layout id the song list shows charts of (`dance-single`, …).
    pub(crate) style: String,
    /// Ruleset preset id (`itg`, `sm5`, `ddr-a`).
    pub(crate) ruleset: String,
    /// Which rules each chart is played by (decision 23): the preset above
    /// for every chart by default.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub(crate) ruleset_mode: RulesetMode,
    /// Gauge, judge range and Excessive under the Dancing☆Onigiri rules.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub(crate) danoni: DanoniOptions,
    /// Seconds; positive = you hear the music later than the audio clock says.
    pub(crate) audio_offset: f64,
    /// Seconds added to the rendered time only.
    pub(crate) visual_offset: f64,
    /// Music gain, 0..=1 (amplitude, not loudness; the options slider maps
    /// it through [`volume_to_slider`]).
    pub(crate) volume: f32,
    /// Keyboard and controller bindings per layout (`keys`, `pads`; older
    /// `keys_single` is migrated on load).
    #[serde(flatten)]
    pub(crate) controls: ControlBindings,
    /// Poll controllers every millisecond during play; off polls once per
    /// frame (less CPU, coarser timing).
    pub(crate) fast_pad_poll: bool,
    /// Debug overlay: backend, FPS, clock drift, timing error stats.
    pub(crate) debug: bool,
    /// Show the signed timing error of each hit under the judgement.
    pub(crate) show_deltas: bool,
    /// Draw a note exactly on the receptor in the closest frame.
    pub(crate) receptor_snap: bool,
    /// Per-audio-path calibrated offsets, keyed by fingerprint id.
    pub(crate) audio_profiles: Vec<OffsetProfile>,
    /// Per-display calibrated offsets, keyed by fingerprint id.
    pub(crate) display_profiles: Vec<OffsetProfile>,
    /// Per-song offsets in seconds, keyed by song id; positive moves the
    /// notes later against the music. Corrects songs whose audio is out of
    /// sync with the chart (MP3 decoder delay, badly synced packs), which no
    /// device calibration can fix. Zero entries are not stored.
    pub(crate) song_offsets: BTreeMap<String, f64>,
}

/// Volume slider position (0..=1) for a gain. Loudness is perceived
/// roughly logarithmically, so equal slider steps should be roughly equal
/// steps in decibels: gain = position³ covers about 60 dB that way (50% is
/// −18 dB, 10% is −60 dB) and still reaches silence at 0.
pub(crate) fn volume_to_slider(gain: f32) -> f32 {
    gain.clamp(0.0, 1.0).cbrt()
}

/// Gain for a volume slider position; inverse of [`volume_to_slider`].
pub(crate) fn slider_to_volume(position: f32) -> f32 {
    position.clamp(0.0, 1.0).powi(3)
}

/// Note colour scheme (see `ddi_render::note_colors`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum NoteColors {
    /// By subdivision.
    Quantized,
    /// Cycling rainbow, shifted by position in the beat (the default, as
    /// on DDR from 3rdMIX to 2013).
    #[default]
    Vivid,
    /// Cycling rainbow, all notes alike.
    Flat,
    /// By position in the beat, in four colour families with flowing
    /// gradients (DDR 2014–A3).
    Rainbow,
    /// One colour.
    Single,
}

impl NoteColors {
    pub(crate) fn scheme(self) -> ddi_render::ColorScheme {
        use ddi_render::ColorScheme;
        match self {
            NoteColors::Quantized => ColorScheme::Quantized,
            NoteColors::Vivid => ColorScheme::Vivid,
            NoteColors::Flat => ColorScheme::Flat,
            NoteColors::Rainbow => ColorScheme::Rainbow,
            NoteColors::Single => ColorScheme::Single([0.35, 0.85, 1.0, 1.0]),
        }
    }
}

/// Field filter strengths the options screen offers (0 = off).
pub(crate) const FIELD_FILTERS: [f32; 4] = [0.0, 0.4, 0.6, 0.8];

/// Song offsets are clamped to this many seconds either way.
pub(crate) const MAX_SONG_OFFSET: f64 = 1.0;

/// A calibrated offset for one device fingerprint. `audio_offset` /
/// `visual_offset` on `Settings` remain the defaults for devices without a
/// profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct OffsetProfile {
    pub(crate) id: String,
    pub(crate) label: String,
    /// Seconds.
    pub(crate) offset: f64,
    /// Whether a calibration run produced this value (as opposed to a copied
    /// or default one).
    pub(crate) calibrated: bool,
    /// Last time this device was seen or calibrated (ms since epoch).
    pub(crate) last_seen: f64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            speed: 2.5,
            speed_source: SpeedSource::Player,
            reverse: false,
            scroll_action: ScrollAction::Normal,
            transform: TransformOptions::default(),
            appearance: Appearance::Visible,
            note_colors: NoteColors::Vivid,
            chart_colors: false,
            lyrics: true,
            import_details: false,
            import_copy: false,
            bg_brightness: 0.4,
            field_filter: 0.4,
            style: "dance-single".into(),
            ruleset: "itg".into(),
            ruleset_mode: RulesetMode::AllStepMania,
            danoni: DanoniOptions::default(),
            audio_offset: 0.0,
            visual_offset: 0.0,
            volume: 0.8,
            debug: false,
            show_deltas: false,
            receptor_snap: true,
            audio_profiles: Vec::new(),
            display_profiles: Vec::new(),
            song_offsets: BTreeMap::new(),
            controls: ControlBindings::default(),
            fast_pad_poll: true,
        }
    }
}

impl Settings {
    pub(crate) fn load() -> Settings {
        let stored: Settings = LocalStorage::get(KEY).unwrap_or_default();
        let mut s = stored.clone();
        s.sanitize();
        if s != stored {
            // Persist migrations and clamping so storage matches what runs.
            s.save();
        }
        s
    }

    /// Clamps stored values to the ranges the options screen allows and
    /// normalises the key table (one row per lane, a key on one lane only),
    /// so a hand-edited or stale `localStorage` entry cannot misbehave.
    fn sanitize(&mut self) {
        let defaults = Settings::default();
        if !self.speed.is_finite() {
            self.speed = defaults.speed;
        }
        self.speed = self.speed.clamp(0.25, 10.0);
        for v in [&mut self.audio_offset, &mut self.visual_offset] {
            *v = if v.is_finite() {
                v.clamp(-0.5, 0.5)
            } else {
                0.0
            };
        }
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0.0, 1.0)
        } else {
            defaults.volume
        };
        self.bg_brightness = if self.bg_brightness.is_finite() {
            self.bg_brightness.clamp(0.0, 1.0)
        } else {
            defaults.bg_brightness
        };
        self.field_filter = if self.field_filter.is_finite() {
            // Snap to the nearest offered strength so the select shows it.
            let f = self.field_filter;
            FIELD_FILTERS
                .into_iter()
                .min_by(|a, b| (a - f).abs().total_cmp(&(b - f).abs()))
                .unwrap_or(defaults.field_filter)
        } else {
            defaults.field_filter
        };
        // Older display ids carried the refresh rate (`display:WxH@dpr:hz`);
        // the rate is not an identity (variable-rate panels change it), so
        // strip it and merge duplicates, keeping a calibrated entry.
        for p in &mut self.display_profiles {
            if let Some(idx) = p.id.rfind(':')
                && idx > "display:".len()
                && p.id[idx + 1..].parse::<f64>().is_ok()
            {
                p.id.truncate(idx);
            }
        }
        let mut merged: Vec<OffsetProfile> = Vec::new();
        for p in self.display_profiles.drain(..) {
            match merged.iter_mut().find(|m| m.id == p.id) {
                Some(m) => {
                    let better = p.calibrated && (!m.calibrated || p.last_seen > m.last_seen);
                    if better {
                        *m = p;
                    }
                }
                None => merged.push(p),
            }
        }
        self.display_profiles = merged;
        for p in self
            .audio_profiles
            .iter_mut()
            .chain(self.display_profiles.iter_mut())
        {
            p.offset = if p.offset.is_finite() {
                p.offset.clamp(-0.5, 0.5)
            } else {
                0.0
            };
        }
        self.song_offsets
            .retain(|_, v| v.is_finite() && v.abs() >= SONG_OFFSET_EPSILON);
        for v in self.song_offsets.values_mut() {
            *v = v.clamp(-MAX_SONG_OFFSET, MAX_SONG_OFFSET);
        }
        self.controls.sanitize();
        if self.style.is_empty() {
            self.style = defaults.style;
        }
    }

    /// Saved bindings for a controller, else the standard defaults when it
    /// has the standard mapping.
    pub(crate) fn pad_bindings(&self, id: &str, standard: bool) -> Option<PadBindings> {
        self.controls.pad(id, standard)
    }

    /// Stores a controller's bindings, replacing earlier ones.
    pub(crate) fn set_pad_bindings(&mut self, pad: PadBindings) {
        self.controls.set_pad(pad, Self::now_ms());
    }

    pub(crate) fn save(&self) {
        let _ = LocalStorage::set(KEY, self);
    }

    /// Play options for the given devices: profile offsets when known,
    /// else the defaults. The shuffle seed is left at 0 for the play session
    /// to pick.
    pub(crate) fn play_options(
        &self,
        devices: Option<&ddi_platform::DeviceProfile>,
    ) -> PlayOptions {
        let (audio_offset, visual_offset) = match devices {
            Some(d) => (
                self.audio_profile(&d.audio.id)
                    .map(|p| p.offset)
                    .unwrap_or(self.audio_offset),
                self.display_profile(&d.display.id)
                    .map(|p| p.offset)
                    .unwrap_or(self.visual_offset),
            ),
            None => (self.audio_offset, self.visual_offset),
        };
        PlayOptions {
            scroll: ScrollOptions {
                speed: SpeedMod::XMod(self.speed),
                reverse: self.reverse,
                scroll_action: self.scroll_action,
            },
            transform: self.transform,
            appearance: self.appearance,
            clock: ClockOptions {
                audio_offset,
                visual_offset,
                rate: 1.0,
            },
            receptor_snap: self.receptor_snap,
            ..PlayOptions::default()
        }
    }

    pub(crate) fn audio_profile(&self, id: &str) -> Option<&OffsetProfile> {
        find_profile(&self.audio_profiles, id).map(|i| &self.audio_profiles[i])
    }

    pub(crate) fn display_profile(&self, id: &str) -> Option<&OffsetProfile> {
        find_profile(&self.display_profiles, id).map(|i| &self.display_profiles[i])
    }

    fn now_ms() -> f64 {
        js_sys::Date::now()
    }

    /// Creates or refreshes the profile for a fingerprint. New profiles start
    /// from `initial` and are marked uncalibrated.
    pub(crate) fn touch_profile<'a>(
        profiles: &'a mut Vec<OffsetProfile>,
        fp: &ddi_platform::DeviceFingerprint,
        initial: f64,
    ) -> &'a mut OffsetProfile {
        let now = Self::now_ms();
        let idx = match find_profile(profiles, &fp.id) {
            Some(i) => i,
            None => {
                profiles.push(OffsetProfile {
                    id: fp.id.clone(),
                    label: fp.label.clone(),
                    offset: initial,
                    calibrated: false,
                    last_seen: now,
                });
                profiles.len() - 1
            }
        };
        let p = &mut profiles[idx];
        p.label = fp.label.clone();
        p.last_seen = now;
        p
    }

    /// Store a calibrated offset for the audio or display profile.
    pub(crate) fn set_profile_offset(
        profiles: &mut Vec<OffsetProfile>,
        fp: &ddi_platform::DeviceFingerprint,
        offset: f64,
    ) {
        let p = Self::touch_profile(profiles, fp, offset);
        p.offset = offset.clamp(-0.5, 0.5);
        p.calibrated = true;
    }

    pub(crate) fn song_offset(&self, song_id: &str) -> f64 {
        self.song_offsets.get(song_id).copied().unwrap_or(0.0)
    }

    pub(crate) fn set_song_offset(&mut self, song_id: &str, seconds: f64) {
        let seconds = seconds.clamp(-MAX_SONG_OFFSET, MAX_SONG_OFFSET);
        if seconds.abs() < SONG_OFFSET_EPSILON {
            self.song_offsets.remove(song_id);
        } else {
            self.song_offsets.insert(song_id.to_string(), seconds);
        }
    }

    pub(crate) fn ruleset(&self) -> ddi_engine::rules::Ruleset {
        ddi_engine::rules::presets::by_id(&self.ruleset)
            .unwrap_or_else(ddi_engine::rules::presets::itg)
    }

    /// The ruleset `chart` is played with.
    pub(crate) fn ruleset_for(&self, chart: &ddi_chart::Chart) -> ddi_engine::rules::Ruleset {
        ddi_engine::rules::presets::for_chart(self.ruleset_mode, &self.ruleset, &self.danoni, chart)
    }
}

/// Exact id match, or for audio ids (`audio:<rate>:<base>:<out>`) the entry
/// whose output-latency bucket is within 10 ms with the same rate and base
/// latency, so jitter across a 5 ms bucket boundary is the same device.
fn find_profile(profiles: &[OffsetProfile], id: &str) -> Option<usize> {
    if let Some(i) = profiles.iter().position(|p| p.id == id) {
        return Some(i);
    }
    let (rate, base, out) = audio_parts(id)?;
    profiles
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let (r, b, o) = audio_parts(&p.id)?;
            (r == rate && b == base && (o as i64 - out as i64).abs() <= 10).then_some((i, o))
        })
        .min_by_key(|(_, o)| (*o as i64 - out as i64).abs())
        .map(|(i, _)| i)
}

fn audio_parts(id: &str) -> Option<(u32, u32, u32)> {
    let rest = id.strip_prefix("audio:")?;
    let mut it = rest.split(':');
    let rate = it.next()?.parse().ok()?;
    let base = it.next()?.parse().ok()?;
    let out = it.next()?.parse().ok()?;
    Some((rate, base, out))
}
