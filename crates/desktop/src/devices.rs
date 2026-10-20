//! The audio path and the display the game plays on, as desktop sees
//! them (`docs/plans/desktop.md`, 7.6), and the notices about them (the
//! web's `components/device_watcher.rs` in the UI core's chrome): the
//! output's fingerprint from cpal's device and the stream's nominal
//! latency (`Audio::fingerprint`), the display's from the window's
//! monitor; a banner when one has no calibration ("First time here?",
//! "New audio path: …", `ddi_ui::screens::notices::device_notice`), a
//! short line when the devices change to known ones. Banners wait while
//! a song plays; "later" holds for the devices it was said on.

use std::time::{Duration, Instant};

use ddi_platform::{DeviceFingerprint, DeviceProfile};
use ddi_ui::screens::notices::{AppNotices, NoticesFeed, device_notice, register_devices};
use ddi_ui::state::settings::Settings;

/// How long the devices' line shows.
const TOAST: Duration = Duration::from_secs(5);
/// How often the devices are looked at again.
pub const EVERY: Duration = Duration::from_secs(1);

#[derive(Default)]
pub struct Watcher {
    /// The devices last seen (both known).
    pub last: Option<DeviceProfile>,
    dismissed: bool,
    dismissed_for: Option<(String, String)>,
    toast: Option<(String, Instant)>,
    pub feed: NoticesFeed,
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
        // stream's buffer, jitter): the same devices, nothing said.
        if let Some(last) = &self.last
            && last.display.id == profile.display.id
            && ddi_ui::state::settings::same_audio_device(&last.audio.id, &profile.audio.id)
        {
            return false;
        }
        let first = self.last.is_none();
        self.last = Some(profile.clone());
        let settings = Settings::load();
        let unknown = device_notice(&settings, Some(&profile)).is_some();
        let ids = (profile.audio.id.clone(), profile.display.id.clone());
        if unknown {
            // "later" holds for the devices it was said on; new ones bring
            // the banner back.
            if self.dismissed {
                match &self.dismissed_for {
                    None => self.dismissed_for = Some(ids),
                    Some(d) if *d == ids => {}
                    Some(_) => {
                        self.dismissed = false;
                        self.dismissed_for = None;
                    }
                }
            }
        } else if !first {
            let a = settings.audio_profile(&profile.audio.id);
            let d = settings.display_profile(&profile.display.id);
            self.toast = Some((
                format!(
                    "devices: {} ({:+.0} ms) · {} ({:+.0} ms)",
                    profile.audio.label,
                    a.map(|p| p.offset).unwrap_or(0.0) * 1000.0,
                    profile.display.label,
                    d.map(|p| p.offset).unwrap_or(0.0) * 1000.0
                ),
                Instant::now() + TOAST,
            ));
        }
        true
    }

    /// A banner's button: `later`, `defaults`, `copy-audio:<id>`,
    /// `copy-display:<id>` (`calibrate` opens the calibration page: the
    /// shell's).
    pub fn act(&mut self, id: &str) {
        match id {
            "later" => {
                self.dismissed = true;
                self.dismissed_for = self
                    .last
                    .as_ref()
                    .map(|p| (p.audio.id.clone(), p.display.id.clone()));
            }
            "calibrate" => {}
            other => {
                if let Some(profile) = self.last.clone() {
                    let mut settings = Settings::load();
                    register_devices(&mut settings, &profile, other);
                    settings.save();
                }
            }
        }
    }

    /// What the notices show now (`suppressed` while a song plays);
    /// whether that changed.
    pub fn publish(&mut self, suppressed: bool) -> bool {
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, until)| Instant::now() >= *until)
        {
            self.toast = None;
        }
        let banner = if suppressed || self.dismissed {
            None
        } else {
            device_notice(&crate::store::settings_cached(), self.last.as_ref())
        };
        let now = AppNotices {
            banner,
            toast: if suppressed {
                None
            } else {
                self.toast.as_ref().map(|(t, _)| t.clone())
            },
        };
        if *self.feed.borrow() == now {
            return false;
        }
        *self.feed.borrow_mut() = now;
        true
    }

    /// The devices a play starts on, seen now (the web's play does the
    /// same): kept with the default offsets when new (the banner said
    /// so over the prompt; starting goes ahead without calibrating, as
    /// the web's "play anyway").
    pub fn for_play(&self) -> Option<DeviceProfile> {
        let p = self.last.clone()?;
        let mut settings = Settings::load();
        register_devices(&mut settings, &p, "defaults");
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
    fn notices_follow_the_devices() {
        ddi_platform::storage::install(std::rc::Rc::new(
            ddi_platform::storage::MemoryStore::default(),
        ));
        let mut w = Watcher::default();
        // No profiles, nothing probed: the first run's banner.
        assert!(w.publish(false));
        let first = w.feed.borrow().banner.clone().unwrap();
        assert!(first.text.starts_with("First time here?"));
        // A new audio path and display: their banner; "later" hides it
        // for them.
        assert!(w.probe(fp("audio:a"), fp("display:d")));
        w.publish(false);
        assert!(
            w.feed
                .borrow()
                .banner
                .as_ref()
                .unwrap()
                .text
                .contains("New audio path")
        );
        w.act("later");
        w.publish(false);
        assert!(w.feed.borrow().banner.is_none());
        // Another device brings it back; "use defaults" keeps both.
        assert!(w.probe(fp("audio:b"), fp("display:d")));
        w.publish(false);
        assert!(w.feed.borrow().banner.is_some());
        w.act("defaults");
        w.publish(false);
        assert!(w.feed.borrow().banner.is_none());
        // Back to a known one: the devices' line.
        assert!(w.probe(fp("audio:a"), fp("display:d")));
        w.act("defaults");
        assert!(w.probe(fp("audio:b"), fp("display:d")));
        w.publish(false);
        assert!(
            w.feed
                .borrow()
                .toast
                .as_ref()
                .unwrap()
                .starts_with("devices: audio:b")
        );
        // The same output a latency bucket away: the same devices.
        w.probe(fp("audio:CoreAudio:Out:48000:10"), fp("display:d"));
        assert!(!w.probe(fp("audio:CoreAudio:Out:48000:15"), fp("display:d")));
        assert!(w.probe(fp("audio:CoreAudio:Other:48000:15"), fp("display:d")));
        // Nothing while a song plays.
        w.publish(true);
        assert_eq!(*w.feed.borrow(), AppNotices::default());
        // A play on new devices keeps them, at the defaults.
        assert!(w.probe(fp("audio:c"), fp("display:e")));
        let p = w.for_play().unwrap();
        let s = Settings::load();
        assert!(s.audio_profile(&p.audio.id).is_some());
        assert!(s.display_profile(&p.display.id).is_some());
    }
}
