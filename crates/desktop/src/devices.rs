//! The audio path and the display the game plays on, as desktop sees
//! them (`docs/plans/desktop.md`, 7.6; the web's play route probes its
//! own): the output's fingerprint from cpal's device and the stream's
//! nominal latency (`Audio::fingerprint`), the display's from the
//! window's monitor. A device seen for the first time plays on the
//! default offsets, kept as its profile from the play on; nothing is
//! asked or announced (`docs/plans/quiet-calibration.md`).

use std::time::Instant;

use ddi_platform::{DeviceFingerprint, DeviceProfile};
use ddi_ui::state::settings::Settings;

/// How often the devices are looked at again.
pub const EVERY: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Default)]
pub struct Watcher {
    /// The devices last seen (both known).
    pub last: Option<DeviceProfile>,
    /// When they were looked at.
    pub checked: Option<Instant>,
}

impl Watcher {
    /// The devices now (`None` for either: not known yet, as the web's
    /// before a gesture, or no sound device); whether they changed.
    pub fn probe(
        &mut self,
        audio: Option<DeviceFingerprint>,
        display: Option<DeviceFingerprint>,
    ) -> bool {
        self.checked = Some(Instant::now());
        let (Some(audio), Some(display)) = (audio, display) else {
            return false;
        };
        let profile = DeviceProfile { audio, display };
        if self.last.as_ref() == Some(&profile) {
            return false;
        }
        // The same output with its latency a bucket away (another
        // stream's buffer, jitter): the same devices.
        if let Some(last) = &self.last
            && last.display.id == profile.display.id
            && ddi_ui::state::settings::same_audio_device(&last.audio.id, &profile.audio.id)
        {
            return false;
        }
        self.last = Some(profile);
        true
    }

    /// The devices a play starts on, seen now (the web's play does the
    /// same): kept with the default offsets when new.
    pub fn for_play(&self) -> Option<DeviceProfile> {
        let p = self.last.clone()?;
        let mut settings = Settings::load();
        settings.register_devices(&p);
        settings.save();
        Some(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(id: &str) -> Option<DeviceFingerprint> {
        Some(DeviceFingerprint::new(id, id))
    }

    #[test]
    fn the_devices_are_followed_and_kept_at_the_defaults() {
        ddi_platform::storage::install(std::rc::Rc::new(
            ddi_platform::storage::MemoryStore::default(),
        ));
        let mut w = Watcher::default();
        assert!(w.probe(fp("audio:a"), fp("display:d")));
        assert!(!w.probe(fp("audio:a"), fp("display:d")));
        assert!(w.probe(fp("audio:b"), fp("display:d")));
        // The same output a latency bucket away: the same devices.
        w.probe(fp("audio:CoreAudio:Out:48000:10"), fp("display:d"));
        assert!(!w.probe(fp("audio:CoreAudio:Out:48000:15"), fp("display:d")));
        assert!(w.probe(fp("audio:CoreAudio:Other:48000:15"), fp("display:d")));
        // A play on new devices keeps them, at the defaults, calibrated
        // by nothing yet.
        assert!(w.probe(fp("audio:c"), fp("display:e")));
        let p = w.for_play().unwrap();
        let s = Settings::load();
        let a = s.audio_profile(&p.audio.id).unwrap();
        assert!((a.offset - s.audio_offset).abs() < 1e-9 && !a.calibrated);
        assert!(s.display_profile(&p.display.id).is_some());
    }
}
