//! What a [`crate::DeviceFingerprint`] tells of its device: the numbers
//! and kinds the shells read, and the name the system gives it, for the
//! screens to word in the player's language (`ddi_ui`; the shells write
//! no text of their own). [`DeviceFacts::describe`] writes them in
//! English for logs and the play history's record, never for a screen.

use serde::{Deserialize, Serialize};

/// What an audio output or a display is, as its fingerprint saw it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceFacts {
    /// A sound output.
    Audio {
        /// Its name as the system gives it (desktop's); the web is told
        /// none.
        name: Option<String>,
        /// Sample rate, Hz.
        rate: u32,
        /// Output latency, whole milliseconds.
        latency_ms: u32,
        /// What the latency suggests of how it is connected, where no
        /// name says what it is (the web's).
        link: Option<AudioLink>,
    },
    /// A display.
    Display {
        /// Its name as the system gives it (desktop's); the web is told
        /// none.
        name: Option<String>,
        /// Logical size, pixels.
        width: u32,
        height: u32,
        /// Device pixels per logical pixel, in hundredths (200: 2×).
        scale_percent: u32,
        /// The refresh rate measured when it was seen, Hz (the web's; not
        /// part of its identity, variable-rate panels change it).
        refresh_hz: Option<u32>,
    },
    /// Only a description (a test's), shown as it is.
    Described(String),
}

/// How an output seems to be connected, guessed from its latency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioLink {
    /// A short latency: built-in speakers or wired headphones.
    Wired,
    /// A long one (100 ms or more): Bluetooth, most likely.
    Wireless,
    /// No latency reported at all.
    Unknown,
}

impl AudioLink {
    /// The guess from the output latency and the graph's own (whole
    /// milliseconds), as the web's audio path reports them.
    pub fn guess(output_ms: u32, base_ms: u32) -> AudioLink {
        if output_ms >= 100 {
            AudioLink::Wireless
        } else if output_ms == 0 && base_ms == 0 {
            AudioLink::Unknown
        } else {
            AudioLink::Wired
        }
    }
}

impl From<&str> for DeviceFacts {
    fn from(text: &str) -> DeviceFacts {
        DeviceFacts::Described(text.to_string())
    }
}

impl From<String> for DeviceFacts {
    fn from(text: String) -> DeviceFacts {
        DeviceFacts::Described(text)
    }
}

impl DeviceFacts {
    /// What a fingerprint's id says of its device, for a profile kept
    /// before its facts were (`None` for an id of no shell's form): the
    /// web's `audio:<rate>:<base>:<out>` and `display:<w>x<h>@<scale>`,
    /// desktop's `audio:<host>:<device>:<rate>:<bucket>` (its latency
    /// the 5 ms bucket) and `display:<name>:<w>x<h>@<scale>`. A web
    /// display's refresh rate is in no id.
    pub fn from_id(id: &str) -> Option<DeviceFacts> {
        if let Some(rest) = id.strip_prefix("audio:") {
            let parts: Vec<&str> = rest.split(':').collect();
            if let [rate, base, out] = parts[..]
                && let (Ok(rate), Ok(base), Ok(out)) =
                    (rate.parse::<u32>(), base.parse::<u32>(), out.parse::<u32>())
            {
                return Some(DeviceFacts::Audio {
                    name: None,
                    rate,
                    latency_ms: out + base,
                    link: Some(AudioLink::guess(out, base)),
                });
            }
            let (rest, bucket) = rest.rsplit_once(':')?;
            let (rest, rate) = rest.rsplit_once(':')?;
            let (_host, device) = rest.split_once(':')?;
            return Some(DeviceFacts::Audio {
                name: (!device.is_empty()).then(|| device.to_string()),
                rate: rate.parse().ok()?,
                latency_ms: bucket.parse().ok()?,
                link: None,
            });
        }
        let rest = id.strip_prefix("display:")?;
        let (name, geometry) = match rest.rsplit_once(':') {
            Some((name, geometry)) => (Some(name), geometry),
            None => (None, rest),
        };
        let (size, scale) = geometry.split_once('@')?;
        let (width, height) = size.split_once('x')?;
        let scale: f64 = scale.parse().ok()?;
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        Some(DeviceFacts::Display {
            // Desktop's word for a monitor the system names not.
            name: name
                .filter(|n| !n.is_empty() && *n != "display")
                .map(str::to_string),
            width: width.parse().ok()?,
            height: height.parse().ok()?,
            scale_percent: (scale * 100.0).round() as u32,
            refresh_hz: None,
        })
    }

    /// The device in English, for logs and the play history's record
    /// (the screens word it in the player's language).
    pub fn describe(&self) -> String {
        match self {
            DeviceFacts::Audio {
                name,
                rate,
                latency_ms,
                link,
            } => {
                let khz = f64::from(*rate) / 1000.0;
                let mut out = match name {
                    Some(name) => format!("{name} · {khz} kHz"),
                    None => format!("{khz} kHz"),
                };
                if *link == Some(AudioLink::Unknown) {
                    out.push_str(" · output latency unknown");
                } else {
                    out.push_str(&format!(" · ~{latency_ms} ms output latency"));
                }
                match link {
                    Some(AudioLink::Wired) => out.push_str(" (wired/built-in?)"),
                    Some(AudioLink::Wireless) => out.push_str(" (wireless?)"),
                    _ => {}
                }
                out
            }
            DeviceFacts::Display {
                name,
                width,
                height,
                scale_percent,
                refresh_hz,
            } => {
                let scale = f64::from(*scale_percent) / 100.0;
                match (name, refresh_hz) {
                    (Some(name), _) => format!("{name} · {width}×{height} · ×{scale}"),
                    (None, Some(hz)) => format!("{width}×{height} ({scale}×), ~{hz} Hz"),
                    (None, None) => format!("{width}×{height} ({scale}×)"),
                }
            }
            DeviceFacts::Described(text) => text.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_shells_ids_give_their_facts() {
        assert_eq!(
            DeviceFacts::from_id("audio:48000:5:170"),
            Some(DeviceFacts::Audio {
                name: None,
                rate: 48000,
                latency_ms: 175,
                link: Some(AudioLink::Wireless),
            })
        );
        assert_eq!(
            DeviceFacts::from_id("audio:44100:0:0"),
            Some(DeviceFacts::Audio {
                name: None,
                rate: 44100,
                latency_ms: 0,
                link: Some(AudioLink::Unknown),
            })
        );
        // A device's name may hold the separator.
        assert_eq!(
            DeviceFacts::from_id("audio:CoreAudio:Studio: Out 1:44100:15"),
            Some(DeviceFacts::Audio {
                name: Some("Studio: Out 1".into()),
                rate: 44100,
                latency_ms: 15,
                link: None,
            })
        );
        assert_eq!(
            DeviceFacts::from_id("display:1512x982@2"),
            Some(DeviceFacts::Display {
                name: None,
                width: 1512,
                height: 982,
                scale_percent: 200,
                refresh_hz: None,
            })
        );
        assert_eq!(
            DeviceFacts::from_id("display:Built-in Retina Display:3024x1964@1.25"),
            Some(DeviceFacts::Display {
                name: Some("Built-in Retina Display".into()),
                width: 3024,
                height: 1964,
                scale_percent: 125,
                refresh_hz: None,
            })
        );
        assert_eq!(
            DeviceFacts::from_id("display:display:1920x1080@1"),
            Some(DeviceFacts::Display {
                name: None,
                width: 1920,
                height: 1080,
                scale_percent: 100,
                refresh_hz: None,
            })
        );
        for other in [
            "audio:1",
            "audio:x:y",
            "display:1",
            "display:big@2",
            "display:10x10@0",
            "screen:10x10@1",
            "",
        ] {
            assert_eq!(DeviceFacts::from_id(other), None, "{other}");
        }
    }

    #[test]
    fn described_in_english_as_the_shells_wrote_it() {
        let web = DeviceFacts::Audio {
            name: None,
            rate: 44100,
            latency_ms: 20,
            link: Some(AudioLink::guess(15, 5)),
        };
        assert_eq!(
            web.describe(),
            "44.1 kHz · ~20 ms output latency (wired/built-in?)"
        );
        let desktop = DeviceFacts::Audio {
            name: Some("Speakers".into()),
            rate: 48000,
            latency_ms: 12,
            link: None,
        };
        assert_eq!(
            desktop.describe(),
            "Speakers · 48 kHz · ~12 ms output latency"
        );
        let screen = DeviceFacts::Display {
            name: None,
            width: 1512,
            height: 982,
            scale_percent: 200,
            refresh_hz: Some(120),
        };
        assert_eq!(screen.describe(), "1512×982 (2×), ~120 Hz");
        let monitor = DeviceFacts::Display {
            name: Some("DELL U2720Q".into()),
            width: 3840,
            height: 2160,
            scale_percent: 150,
            refresh_hz: None,
        };
        assert_eq!(monitor.describe(), "DELL U2720Q · 3840×2160 · ×1.5");
    }

    #[test]
    fn facts_are_kept_as_json() {
        let facts = DeviceFacts::Audio {
            name: None,
            rate: 48000,
            latency_ms: 20,
            link: Some(AudioLink::Wired),
        };
        let json = serde_json::to_string(&facts).unwrap();
        assert_eq!(serde_json::from_str::<DeviceFacts>(&json).unwrap(), facts);
        let described = DeviceFacts::from("Headphones");
        let json = serde_json::to_string(&described).unwrap();
        assert_eq!(
            serde_json::from_str::<DeviceFacts>(&json).unwrap(),
            described
        );
    }
}
