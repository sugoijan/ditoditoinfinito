//! Player settings persisted in `localStorage`.

use ddi_engine::clock::ClockOptions;
use ddi_engine::player::PlayOptions;
use ddi_engine::scroll::{ScrollAction, ScrollOptions, SpeedMod};
use gloo::storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

const KEY: &str = "ddi.settings.v1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    /// X-mod multiplier.
    pub(crate) speed: f64,
    pub(crate) reverse: bool,
    /// Ruleset preset id (`itg`, `sm5`, `ddr-a`).
    pub(crate) ruleset: String,
    /// Seconds; positive = you hear the music later than the audio clock says.
    pub(crate) audio_offset: f64,
    /// Seconds added to the rendered time only.
    pub(crate) visual_offset: f64,
    /// 0..=1
    pub(crate) volume: f32,
    /// Key bindings for `dance-single`, per lane (`KeyboardEvent.code`).
    pub(crate) keys_single: Vec<Vec<String>>,
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
}

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
        let layout = ddi_chart::Layout::dance_single();
        Settings {
            speed: 2.5,
            reverse: false,
            ruleset: "itg".into(),
            audio_offset: 0.0,
            visual_offset: 0.0,
            volume: 0.8,
            debug: false,
            show_deltas: false,
            receptor_snap: true,
            audio_profiles: Vec::new(),
            display_profiles: Vec::new(),
            keys_single: layout
                .lanes
                .iter()
                .map(|l| l.default_keys.clone())
                .collect(),
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
        let lanes = defaults.keys_single.len();
        self.keys_single.resize_with(lanes, Vec::new);
        let mut seen = std::collections::HashSet::new();
        for keys in &mut self.keys_single {
            keys.retain(|k| !k.is_empty() && seen.insert(k.clone()));
        }
    }

    pub(crate) fn save(&self) {
        let _ = LocalStorage::set(KEY, self);
    }

    /// Play options for the given devices: profile offsets when known,
    /// else the defaults.
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
                scroll_action: ScrollAction::Normal,
            },
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

    pub(crate) fn ruleset(&self) -> ddi_engine::rules::Ruleset {
        ddi_engine::rules::presets::by_id(&self.ruleset)
            .unwrap_or_else(ddi_engine::rules::presets::itg)
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
