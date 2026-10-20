//! The player's settings (`docs/plans/ui-core.md`, P12), kept under
//! `ddi.settings.v1` in the installed store ([`ddi_platform::storage`]):
//! on the web `localStorage`, as JSON, as they always were, so players keep
//! theirs; older shapes migrate on load ([`Settings::load`]).

use std::collections::BTreeMap;

pub use ddi_engine::PadBindings;
use ddi_engine::clock::ClockOptions;
use ddi_engine::player::PlayOptions;
use ddi_engine::rules::danoni::DanoniOptions;
use ddi_engine::rules::presets::{HoldStyle, RulesetMode};
use ddi_engine::scroll::{ScrollAction, ScrollOptions, SpeedMod, SpeedSource};
use ddi_engine::{Appearance, ControlBindings, TransformOptions};
use ddi_platform::storage::{self, Area};
use serde::{Deserialize, Serialize};

/// Where the settings are kept.
pub const KEY: &str = "ddi.settings.v1";

/// Song offsets closer to zero than this (half a millisecond) are dropped.
const SONG_OFFSET_EPSILON: f64 = 0.0005;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// X-mod multiplier.
    pub speed: f64,
    /// The speed above everywhere, or a chart's own when it has one.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub speed_source: SpeedSource,
    pub reverse: bool,
    /// Boost / Brake / Wave: how notes move on their way to the receptors.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub scroll_action: ScrollAction,
    /// Turn and simplifications applied to the chart of every song (not to
    /// calibration). Simplified plays are marked as assisted.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub transform: TransformOptions,
    /// Offer additional DanOni charts without their onigiri lane.
    pub simplify_onigiri: bool,
    /// Hidden / Sudden / Stealth (drawing only).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub appearance: Appearance,
    /// The import report also lists details for pack authors:
    /// inconsistencies that make no difference in play.
    pub import_details: bool,
    /// Copy a picked folder's songs into the browser even where they can
    /// be linked (Chromium), so they survive the folder moving.
    pub import_copy: bool,
    /// Also copy background-change movies when songs are copied into the
    /// browser (they are most of a pack's size; on by default, since a
    /// copied song's movie left out never plays); linked songs read them
    /// from their folder either way.
    pub import_videos: bool,
    /// How notes are colored.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub note_colors: NoteColors,
    /// The noteskin layouts are drawn with: an imported one's key,
    /// `dance/<name>`, a bundled one's, `bundled:dance/<name>`, or
    /// `builtin` for the built-in arrows without a skin; `None`: Dito, the
    /// built-in arrows as a skin ([`Settings::play_skin`]).
    pub noteskin: Option<String>,
    /// The pump noteskin pump layouts are drawn with (`pump/<name>`, an
    /// imported one's key); `None`: the noteskin above draws them too.
    pub pump_noteskin: Option<String>,
    /// Draw notes in the colors a chart gives them (Dancing☆Onigiri
    /// works) instead of `note_colors` (decision 23: off by default).
    pub chart_colors: bool,
    /// Show the lyrics of songs that have them (Dancing☆Onigiri works).
    pub lyrics: bool,
    /// Song background brightness, 0..=1; 0 turns the background off.
    pub bg_brightness: f32,
    /// Whether background-change movies play.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub video: VideoMode,
    /// The shape backgrounds and movies are fitted into.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub background_frame: BackgroundFrame,
    /// Darkening behind the lanes, one of [`FIELD_FILTERS`] (0 = off).
    pub field_filter: f32,
    /// Layout id the song list shows charts of (`dance-single`, …).
    pub style: String,
    /// Ruleset preset id (`itg`, `sm5`, `ddr-a`).
    pub ruleset: String,
    /// Which rules each chart is played by (decision 23): the preset above
    /// for every chart by default.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub ruleset_mode: RulesetMode,
    /// Gauge, judge range and Excessive under the Dancing☆Onigiri rules.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub danoni: DanoniOptions,
    /// How the StepMania presets keep holds: ticked on pump charts by
    /// default (`docs/plans/pump.md`).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub holds: HoldStyle,
    /// Seconds; positive = you hear the music later than the audio clock says.
    pub audio_offset: f64,
    /// Seconds added to the rendered time only.
    pub visual_offset: f64,
    /// Music gain, 0..=1 (amplitude, not loudness; the options slider maps
    /// it through [`volume_to_slider`]).
    pub volume: f32,
    /// Keyboard and controller bindings per layout (`keys`, `pads`; older
    /// `keys_single` is migrated on load).
    #[serde(flatten)]
    pub controls: ControlBindings,
    /// Poll controllers every millisecond during play; off polls once per
    /// frame (less CPU, coarser timing).
    pub fast_pad_poll: bool,
    /// Reduced motion in the menus (no beat pulse, no slides): as the
    /// system says, or always, or never.
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub reduced_motion: ReducedMotion,
    /// The results' panels the player chose ("Overview", "Timing",
    /// "Song"; one a ruleset does not offer falls back to its default).
    pub results_panels: String,
    /// Debug overlay: backend, FPS, clock drift, timing error stats.
    pub debug: bool,
    /// Which graphics backend the app starts on (read once, at boot).
    #[serde(deserialize_with = "ddi_engine::lenient")]
    pub graphics: Graphics,
    /// Show the signed timing error of each hit under the judgement.
    pub show_deltas: bool,
    /// Show how far ahead of the chart's best play (or behind it) a play
    /// is, under the score (`docs/plans/play-history.md`, Replays).
    pub ghost: bool,
    /// Draw a note exactly on the receptor in the closest frame.
    pub receptor_snap: bool,
    /// Per-audio-path calibrated offsets, keyed by fingerprint id.
    pub audio_profiles: Vec<OffsetProfile>,
    /// Per-display calibrated offsets, keyed by fingerprint id.
    pub display_profiles: Vec<OffsetProfile>,
    /// Per-song offsets in seconds, keyed by song id; positive moves the
    /// notes later against the music. Corrects songs whose audio is out of
    /// sync with the chart (MP3 decoder delay, badly synced packs), which no
    /// device calibration can fix. Zero entries are not stored.
    pub song_offsets: BTreeMap<String, f64>,
}

/// Volume slider position (0..=1) for a gain. Loudness is perceived
/// roughly logarithmically, so equal slider steps should be roughly equal
/// steps in decibels: gain = position³ covers about 60 dB that way (50% is
/// −18 dB, 10% is −60 dB) and still reaches silence at 0.
pub fn volume_to_slider(gain: f32) -> f32 {
    gain.clamp(0.0, 1.0).cbrt()
}

/// Gain for a volume slider position; inverse of [`volume_to_slider`].
pub fn slider_to_volume(position: f32) -> f32 {
    position.clamp(0.0, 1.0).powi(3)
}

/// Which graphics backend draws (`web::screen`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Graphics {
    /// WebGPU where the browser gives an adapter, else WebGL2.
    #[default]
    Auto,
    /// WebGL2 always (a driver's trouble with WebGPU; testing the
    /// fallback).
    WebGl2,
}

/// Reduced motion in the menus.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReducedMotion {
    /// As the system's setting (the web's `prefers-reduced-motion`;
    /// desktop has none and moves).
    #[default]
    Auto,
    On,
    Off,
}

impl ReducedMotion {
    /// Whether motion is reduced, given the system's preference.
    pub fn reduced(self, system: bool) -> bool {
        match self {
            ReducedMotion::Auto => system,
            ReducedMotion::On => true,
            ReducedMotion::Off => false,
        }
    }
}

/// Whether background-change movies play.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoMode {
    /// Play them, and turn one off for the rest of the song when it costs
    /// frames (the display falls behind or the decoder cannot keep up).
    #[default]
    Auto,
    /// Always play them.
    On,
    /// Never: the decoder is never fetched.
    Off,
}

/// The shape backgrounds and movies are fitted into (StepMania's cover
/// fit inside it); outside it the screen shows a blurred extension of it.
/// Pack assets were drawn for a cabinet's screen, so a still and a movie
/// line up in the frame they were made for.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackgroundFrame {
    /// The song's movie picture (its frame less its own black bars), else
    /// the background image's shape.
    #[default]
    Auto,
    FourThree,
    SixteenNine,
    /// The whole screen (StepMania's look on the same display).
    Screen,
}

/// Note color scheme (see `ddi_render::note_colors`; the app maps it).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteColors {
    /// By subdivision.
    Quantized,
    /// Cycling rainbow, shifted by position in the beat (the default, as
    /// on DDR from 3rdMIX to 2013).
    #[default]
    Vivid,
    /// Cycling rainbow, all notes alike.
    Flat,
    /// By position in the beat, in four color families with flowing
    /// gradients (DDR 2014–A3).
    Rainbow,
    /// One color.
    Single,
}

/// Field filter strengths the options screen offers (0 = off).
pub const FIELD_FILTERS: [f32; 4] = [0.0, 0.4, 0.6, 0.8];

/// Song offsets are clamped to this many seconds either way.
pub const MAX_SONG_OFFSET: f64 = 1.0;

/// A calibrated offset for one device fingerprint. `audio_offset` /
/// `visual_offset` on `Settings` remain the defaults for devices without a
/// profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OffsetProfile {
    pub id: String,
    pub label: String,
    /// Seconds.
    pub offset: f64,
    /// Whether a calibration run produced this value (as opposed to a copied
    /// or default one).
    pub calibrated: bool,
    /// Last time this device was seen or calibrated (ms since epoch).
    pub last_seen: f64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            speed: 2.5,
            speed_source: SpeedSource::Player,
            reverse: false,
            scroll_action: ScrollAction::Normal,
            transform: TransformOptions::default(),
            simplify_onigiri: false,
            appearance: Appearance::Visible,
            note_colors: NoteColors::Vivid,
            noteskin: None,
            pump_noteskin: None,
            chart_colors: false,
            lyrics: true,
            import_details: false,
            import_copy: false,
            import_videos: true,
            bg_brightness: 0.4,
            video: VideoMode::Auto,
            background_frame: BackgroundFrame::Auto,
            field_filter: 0.4,
            style: "dance-single".into(),
            ruleset: "itg".into(),
            ruleset_mode: RulesetMode::AllStepMania,
            danoni: DanoniOptions::default(),
            holds: HoldStyle::ByGame,
            audio_offset: 0.0,
            visual_offset: 0.0,
            volume: 0.8,
            reduced_motion: ReducedMotion::Auto,
            results_panels: "Overview".into(),
            debug: false,
            graphics: Graphics::Auto,
            show_deltas: false,
            ghost: false,
            receptor_snap: true,
            audio_profiles: Vec::new(),
            display_profiles: Vec::new(),
            song_offsets: BTreeMap::new(),
            controls: ControlBindings::default(),
            fast_pad_poll: true,
        }
    }
}

/// The noteskin keys' forms, defined with the skins
/// ([`ddi_noteskin::bundled`]): the built-in arrows' value, bundled skins'
/// prefix ([`BUNDLED`]) and Dito's key.
pub use ddi_noteskin::bundled::{BUILT_IN, DITO, KEY_PREFIX as BUNDLED};

/// Whether a noteskin key is a pump skin's (`pump/…`, `bundled:pump/…`).
pub fn is_pump_skin(key: &str) -> bool {
    let key = key.strip_prefix(BUNDLED).unwrap_or(key);
    key.split('/')
        .next()
        .is_some_and(|g| g.eq_ignore_ascii_case("pump"))
}

/// Whether a [`Settings::pump_noteskin`] key draws the pump layouts: a
/// pump skin's, or the built-in arrows (`builtin`).
fn draws_pump(key: &str) -> bool {
    is_pump_skin(key) || key == BUILT_IN
}

impl Settings {
    /// The noteskin key chosen for layout `layout` (an id): the pump skin
    /// on a pump layout when one is chosen (and draws them), else
    /// [`Settings::noteskin`].
    pub fn noteskin_for(&self, layout: &str) -> Option<&str> {
        match &self.pump_noteskin {
            Some(k) if layout.starts_with("pump") && draws_pump(k) => Some(k),
            _ => self.noteskin.as_deref(),
        }
    }

    /// The skin a play draws layout `layout` (an id) with, by key: the
    /// chosen one ([`Settings::noteskin_for`]), or Dito (the built-in
    /// arrows as a skin, when none is chosen) in its variant for the note
    /// colors; none for the built-in arrows, and none instead of Dito when
    /// `chart`'s own colors are drawn (a skin colors notes by their place
    /// in the beat, not one by one). `None` for a chart without colors,
    /// as the field preview's demo.
    pub fn play_skin(&self, layout: &str, chart: Option<&ddi_chart::Chart>) -> Option<String> {
        let chart_colors =
            self.chart_colors && chart.is_some_and(|c| c.notes.iter().any(|n| n.color.is_some()));
        match self.noteskin_for(layout) {
            Some(BUILT_IN) => None,
            Some(key) => Some(key.to_string()),
            None if chart_colors => None,
            None => Some(ddi_noteskin::bundled::dito(match self.note_colors {
                NoteColors::Quantized => None,
                NoteColors::Vivid => Some("vivid"),
                NoteColors::Flat => Some("flat"),
                NoteColors::Rainbow => Some("rainbow"),
                NoteColors::Single => Some("single"),
            })),
        }
    }

    pub fn load() -> Settings {
        let stored: Settings = storage::load(Area::Local, KEY).unwrap_or_default();
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
    pub(crate) fn sanitize(&mut self) {
        let defaults = Settings::default();
        // Only a pump skin draws the pump layouts.
        if self
            .pump_noteskin
            .as_deref()
            .is_some_and(|k| !draws_pump(k))
        {
            self.pump_noteskin = None;
        }
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
    pub fn pad_bindings(&self, id: &str, standard: bool) -> Option<PadBindings> {
        self.controls.pad(id, standard)
    }

    /// Stores a controller's bindings, replacing earlier ones.
    pub fn set_pad_bindings(&mut self, pad: PadBindings) {
        self.controls.set_pad(pad, Self::now_ms());
    }

    pub fn save(&self) {
        let _ = storage::save(Area::Local, KEY, self);
    }

    /// Play options for the given devices: profile offsets when known,
    /// else the defaults. The shuffle seed is left at 0 for the play session
    /// to pick.
    pub fn play_options(&self, devices: Option<&ddi_platform::DeviceProfile>) -> PlayOptions {
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

    pub fn audio_profile(&self, id: &str) -> Option<&OffsetProfile> {
        find_profile(&self.audio_profiles, id).map(|i| &self.audio_profiles[i])
    }

    pub fn display_profile(&self, id: &str) -> Option<&OffsetProfile> {
        find_profile(&self.display_profiles, id).map(|i| &self.display_profiles[i])
    }

    fn now_ms() -> f64 {
        storage::now_ms()
    }

    /// Creates or refreshes the profile for a fingerprint. New profiles start
    /// from `initial` and are marked uncalibrated.
    pub fn touch_profile<'a>(
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
    pub fn set_profile_offset(
        profiles: &mut Vec<OffsetProfile>,
        fp: &ddi_platform::DeviceFingerprint,
        offset: f64,
    ) {
        let p = Self::touch_profile(profiles, fp, offset);
        p.offset = offset.clamp(-0.5, 0.5);
        p.calibrated = true;
    }

    pub fn song_offset(&self, song_id: &str) -> f64 {
        self.song_offsets.get(song_id).copied().unwrap_or(0.0)
    }

    pub fn set_song_offset(&mut self, song_id: &str, seconds: f64) {
        let seconds = seconds.clamp(-MAX_SONG_OFFSET, MAX_SONG_OFFSET);
        if seconds.abs() < SONG_OFFSET_EPSILON {
            self.song_offsets.remove(song_id);
        } else {
            self.song_offsets.insert(song_id.to_string(), seconds);
        }
    }

    pub fn ruleset(&self) -> ddi_engine::rules::Ruleset {
        ddi_engine::rules::presets::by_id(&self.ruleset)
            .unwrap_or_else(ddi_engine::rules::presets::itg)
    }

    /// The ruleset `chart` is played with.
    pub fn ruleset_for(&self, chart: &ddi_chart::Chart) -> ddi_engine::rules::Ruleset {
        ddi_engine::rules::presets::for_chart(
            self.ruleset_mode,
            &self.ruleset,
            &self.danoni,
            self.holds,
            chart,
        )
    }

    /// The key the play history keeps a chart's bests under, from what
    /// the song list knows of the chart: its layout, whether it is a
    /// Dancing☆Onigiri work's, and the excessive setting such a chart
    /// gives ([`chart_excessive`]). The same as `ruleset_for(chart)`'s
    /// key's `best_key` (a test holds them equal).
    pub fn best_key_for(
        &self,
        layout: &str,
        danoni: bool,
        excessive: Option<bool>,
    ) -> ddi_engine::rules::RulesetKey {
        let chart = ddi_chart::Chart {
            derivation: None,
            layout: layout.to_string(),
            difficulty: ddi_chart::Difficulty::Edit,
            meter: 0,
            name: String::new(),
            description: String::new(),
            credit: String::new(),
            notes: Vec::new(),
            timing: None,
            display_bpm: None,
            danoni: danoni.then(Default::default),
        };
        let mut key = self.ruleset_for(&chart).key();
        // Excessive as the chart sets it, where the player left it to the
        // chart.
        if let Some(d) = key.danoni.as_mut()
            && self.danoni.excessive.is_none()
        {
            d.excessive = Some(excessive.unwrap_or(false));
        }
        key.best_key()
    }
}

/// The excessive setting a Dancing☆Onigiri chart gives
/// (`excessiveJdgUse`, `excessiveUse`), for the song list
/// ([`Settings::best_key_for`]); `None` for other charts.
pub fn chart_excessive(chart: &ddi_chart::Chart) -> Option<bool> {
    ddi_engine::rules::danoni::chart_excessive(chart)
}

/// Exact id match, or for audio ids (the web's `audio:<rate>:<base>:<out>`,
/// desktop's `audio:<host>:<device>:<rate>:<bucket>`) the entry whose
/// output-latency bucket is within 10 ms on the same path (rate and base
/// latency; host, device and rate), so jitter across a 5 ms bucket
/// boundary, or a new buffer size, is the same device.
fn find_profile(profiles: &[OffsetProfile], id: &str) -> Option<usize> {
    if let Some(i) = profiles.iter().position(|p| p.id == id) {
        return Some(i);
    }
    let (path, out) = audio_parts(id)?;
    profiles
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let (pp, o) = audio_parts(&p.id)?;
            (pp == path && (o - out).abs() <= 10).then_some((i, o))
        })
        .min_by_key(|(_, o)| (*o - out).abs())
        .map(|(i, _)| i)
}

/// Whether audio ids `a` and `b` name the same output (the same id, or
/// the same path with latency buckets within 10 ms; see
/// [`Settings::audio_profile`]).
pub fn same_audio_device(a: &str, b: &str) -> bool {
    a == b
        || matches!((audio_parts(a), audio_parts(b)), (Some((pa, oa)), Some((pb, ob))) if pa == pb && (oa - ob).abs() <= 10)
}

/// An audio id's path and its latency bucket (ms): the web's
/// `audio:<rate>:<base>:<out>`, desktop's
/// `audio:<host>:<device>:<rate>:<bucket>` (`ddi-desktop`'s
/// `Audio::fingerprint`): everything before the last field, and it.
fn audio_parts(id: &str) -> Option<(&str, i64)> {
    let rest = id.strip_prefix("audio:")?;
    let (path, out) = rest.rsplit_once(':')?;
    // The web's form has its rate and base latency before it.
    if !path.contains(':') {
        return None;
    }
    Some((path, out.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn audio_profiles_match_within_ten_ms_on_both_shells_ids() {
        let p = |id: &str| OffsetProfile {
            id: id.into(),
            label: id.into(),
            offset: 0.01,
            calibrated: true,
            last_seen: 0.0,
        };
        let list = vec![
            p("audio:48000:5:170"),
            p("audio:CoreAudio:External Headphones:44100:10"),
        ];
        assert_eq!(find_profile(&list, "audio:48000:5:175"), Some(0));
        assert_eq!(find_profile(&list, "audio:48000:10:170"), None);
        assert_eq!(
            find_profile(&list, "audio:CoreAudio:External Headphones:44100:20"),
            Some(1)
        );
        assert_eq!(
            find_profile(&list, "audio:CoreAudio:External Headphones:44100:25"),
            None
        );
        assert_eq!(
            find_profile(&list, "audio:CoreAudio:Speakers:44100:10"),
            None
        );
        assert_eq!(find_profile(&list, "display:x:1x1@1"), None);
    }

    use ddi_platform::storage::MemoryStore;

    /// Settings as an older version stored them: a key table under
    /// `keys_single`, a display id with its refresh rate twice, values out
    /// of range, a field it no longer knows, one it never had.
    const OLD: &str = r#"{
        "speed": 99,
        "reverse": true,
        "keys_single": [["ArrowLeft"], ["ArrowDown"], ["ArrowUp"], ["ArrowRight"]],
        "display_profiles": [
            {"id": "display:1920x1080@2:60", "label": "a", "offset": 0.01, "calibrated": false, "last_seen": 1},
            {"id": "display:1920x1080@2:144", "label": "b", "offset": 0.02, "calibrated": true, "last_seen": 2}
        ],
        "song_offsets": {"s1": 0.0001, "s2": 5},
        "note_colors": "NoSuchScheme",
        "gone": 1
    }"#;

    #[test]
    fn older_settings_load_and_migrate() {
        let store = Rc::new(MemoryStore::default());
        storage::install(store.clone());
        storage::set(Area::Local, KEY, OLD).unwrap();
        let s = Settings::load();
        assert_eq!(s.speed, 10.0);
        assert!(s.reverse);
        assert_eq!(s.note_colors, NoteColors::Vivid);
        assert_eq!(s.display_profiles.len(), 1);
        assert_eq!(s.display_profiles[0].id, "display:1920x1080@2");
        assert!(s.display_profiles[0].calibrated);
        assert_eq!(s.song_offset("s1"), 0.0);
        assert_eq!(s.song_offset("s2"), MAX_SONG_OFFSET);
        // The migration is stored, and loads as it is.
        let again: Settings = storage::load(Area::Local, KEY).unwrap();
        assert_eq!(again, s);
        // Unreadable: the same as nothing stored (the defaults, sanitized).
        storage::remove(Area::Local, KEY);
        let fresh = Settings::load();
        assert_eq!(fresh.speed, Settings::default().speed);
        storage::set(Area::Local, KEY, "not json").unwrap();
        assert_eq!(Settings::load(), fresh);
    }

    #[test]
    fn the_song_lists_best_key_is_the_plays() {
        use ddi_chart::{Chart, DanoniChart, Difficulty};
        let work = |headers: Vec<(&str, &str)>| Chart {
            derivation: None,
            layout: "danoni-7".into(),
            difficulty: Difficulty::Edit,
            meter: 0,
            name: "Normal".into(),
            description: String::new(),
            credit: String::new(),
            notes: Vec::new(),
            timing: None,
            display_bpm: None,
            danoni: Some(DanoniChart {
                headers: headers
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                ..Default::default()
            }),
        };
        let plain = Chart {
            danoni: None,
            layout: "dance-single".into(),
            ..work(Vec::new())
        };
        for mode in [
            RulesetMode::Original,
            RulesetMode::AllDanoni,
            RulesetMode::AllStepMania,
        ] {
            for excessive in [None, Some(true), Some(false)] {
                let mut s = Settings {
                    ruleset_mode: mode,
                    ..Settings::default()
                };
                s.danoni.excessive = excessive;
                for chart in [
                    work(vec![("excessiveJdgUse", "true")]),
                    work(vec![("excessiveUse", "true,on")]),
                    work(Vec::new()),
                    plain.clone(),
                ] {
                    let play = s.ruleset_for(&chart).key().best_key();
                    let list = s.best_key_for(
                        &chart.layout,
                        chart.danoni.is_some(),
                        chart_excessive(&chart),
                    );
                    assert_eq!(list, play, "{mode:?} {excessive:?} {:?}", chart.danoni);
                }
            }
        }
        assert_eq!(
            chart_excessive(&work(vec![("excessiveJdgUse", "true")])),
            Some(true)
        );
        assert_eq!(chart_excessive(&work(Vec::new())), Some(false));
        assert_eq!(chart_excessive(&plain), None);
    }
}
