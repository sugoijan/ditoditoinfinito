//! A play on desktop (`docs/plans/desktop.md`, step 7.5): the shared
//! session (`ddi_session::play`) on the desktop's audio and inputs, the
//! song read from the library ([`load`]), drawn by the play field's
//! renderer ([`view`]), its noteskin readied natively ([`skins`]), its
//! results on the theme's screen and kept in the play history with the
//! web's facts (`ddi_session::record`).
//!
//! [`Play`] holds no window: the shell feeds it the keyboard's and the
//! pads' edges ([`Feed`]) and the host time each frame, and draws it;
//! tests run it the same way on a fake clock, with no device
//! ([`crate::audio::Audio::headless`]).

pub mod host;
pub mod load;
#[cfg(feature = "video")]
pub mod movies;
pub mod replays;
pub mod skins;
pub mod stage;
pub mod view;

use std::cell::RefCell;
use std::rc::Rc;

use ddi_chart::Layout;
use ddi_engine::ConnectedPad;
use ddi_engine::player::Results;
use ddi_platform::{DeviceId, DeviceProfile, HostTime, RawInput};
use ddi_session::calibration::CalMode;
use ddi_session::play::{PlayFacts, SessionConfig, SessionEvent};
use ddi_ui::state::history::{Autoplay, Plays};
use ddi_ui::state::settings::Settings;

use crate::audio::Audio;
use load::Loaded;

/// What to play.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub song: String,
    pub chart: usize,
    /// The cached song-list image to keep during loading.
    pub backdrop: Option<String>,
    /// Autoplay (`--auto`, song select's switch): never counted.
    pub auto: bool,
    /// Autoplay through a test pad (`--auto=pad`): the presses go
    /// through the bindings as a controller's.
    pub auto_pad: bool,
    /// A calibration test instead of a song (`song` is then not read).
    pub calibration: Option<CalMode>,
    /// Autoplay presses this many seconds late (negative: early); a
    /// testing aid (`--bias MS`).
    pub bias: f64,
    /// A kept play replayed (`replays`), or its results shown.
    pub replay: Option<Rc<ddi_ui::state::history::Replay>>,
    pub view_results: bool,
    /// Why the play asked for cannot be: the prompt says so.
    pub refused: Option<String>,
    /// The chart's best play's ghost beside the play (`Settings::ghost`;
    /// lined up by the song offsets when the play starts).
    pub ghost: Option<Rc<ddi_ui::state::history::Ghost>>,
}

impl Request {
    /// Chart `chart` of song `song`.
    pub fn song(song: &str, chart: usize, auto: bool) -> Request {
        Request {
            song: song.to_string(),
            chart,
            auto,
            backdrop: None,
            auto_pad: false,
            calibration: None,
            bias: 0.0,
            replay: None,
            view_results: false,
            refused: None,
            ghost: None,
        }
    }

    /// Calibration test `mode`.
    pub fn calibration(mode: CalMode, auto: bool) -> Request {
        Request {
            song: format!("calibration:{}", mode.id()),
            chart: 0,
            auto,
            backdrop: None,
            auto_pad: false,
            calibration: Some(mode),
            bias: 0.0,
            replay: None,
            view_results: false,
            refused: None,
            ghost: None,
        }
    }

    /// The settings the play runs with: the stored ones, or a replay's
    /// over them (`Replay::settings`).
    pub fn settings(&self) -> Settings {
        let here = crate::store::settings_cached();
        match &self.replay {
            Some(r) => r.settings(&here),
            None => here,
        }
    }

    /// The song offset the notes are shifted by in a play run with
    /// `settings` ([`ddi_session::play::song_shift`]).
    pub fn song_shift(&self, settings: &Settings) -> f64 {
        ddi_session::play::song_shift(
            settings,
            &self.song,
            self.replay.as_deref(),
            self.calibration.is_some(),
        )
    }
}

/// The test pad's name (`--auto=pad`).
pub const TEST_PAD: &str = "Test Pad";

/// What the shell hands a play between frames.
#[derive(Default)]
pub struct Feed {
    /// Edges since the last frame (keyboard, pads), in any order.
    pub edges: Vec<RawInput>,
    /// The controllers connected now.
    pub pads: Vec<ConnectedPad>,
    /// The window was hidden or the sound stopped.
    pub interrupted: bool,
    /// The display or the audio device changed.
    pub device_changed: bool,
    /// `--auto=pad`: edges scheduled on the test pad, `(host time,
    /// control, pressed)`, released into `edges` when due.
    test_pad: Vec<(HostTime, String, bool)>,
    /// The keyboard and the pads are read (between `begin` and
    /// `release`).
    pub reading: bool,
    /// The host time of the frame being judged.
    pub now: HostTime,
}

/// A play's inputs: what the shell put in the [`Feed`].
pub struct DesktopInputs {
    pub feed: Rc<RefCell<Feed>>,
    auto_pad: bool,
}

impl DesktopInputs {
    pub fn new(feed: Rc<RefCell<Feed>>, auto_pad: bool) -> DesktopInputs {
        DesktopInputs { feed, auto_pad }
    }

    fn test_pad() -> ConnectedPad {
        ConnectedPad {
            id: crate::pads::pad_id(TEST_PAD, None, None),
            index: 99,
            standard: true,
        }
    }
}

impl ddi_session::play::Inputs for DesktopInputs {
    fn begin(&mut self, _capture: Vec<String>, _layout: &Layout, _reverse: bool) {
        let mut f = self.feed.borrow_mut();
        f.reading = true;
        // Edges from before the start are not the play's.
        f.edges.clear();
    }

    fn poll(&mut self, out: &mut Vec<RawInput>) {
        let mut f = self.feed.borrow_mut();
        let now = f.now;
        // The test pad's edges that are due, at their own times.
        let due: Vec<(HostTime, String, bool)> = {
            let (due, later): (Vec<_>, Vec<_>) =
                f.test_pad.drain(..).partition(|(t, _, _)| t.0 <= now.0);
            f.test_pad = later;
            due
        };
        let pad = DesktopInputs::test_pad();
        for (t, control, pressed) in due {
            f.edges.push(RawInput {
                device: DeviceId::Gamepad(pad.id.clone()),
                control,
                pressed,
                host_time: t,
                slot: pad.index,
            });
        }
        if f.reading {
            out.append(&mut f.edges);
        } else {
            f.edges.clear();
        }
    }

    fn connected_pads(&self) -> Vec<ConnectedPad> {
        let mut pads = self.feed.borrow().pads.clone();
        if self.auto_pad {
            pads.push(DesktopInputs::test_pad());
        }
        pads
    }

    /// Nothing to do: the pads' thread reads their events as they come
    /// (`crate::pads`, no polling to speed up), and their edges from
    /// before the play are dropped in [`Self::begin`].
    fn pads_running(&mut self, _fast_poll: Option<bool>) {}

    fn release(&mut self) {
        let mut f = self.feed.borrow_mut();
        f.reading = false;
        f.edges.clear();
    }

    fn interrupted(&self) -> bool {
        self.feed.borrow().interrupted
    }

    fn device_changed(&self) -> bool {
        self.feed.borrow().device_changed
    }

    fn press_test_pad(&mut self, host: HostTime, control: &str, pressed: bool) {
        self.feed
            .borrow_mut()
            .test_pad
            .push((host, control.to_string(), pressed));
    }
}

pub type Session = ddi_session::play::PlaySession<Audio, DesktopInputs>;

/// How a play ended.
pub enum Ended {
    /// Played to its end, or failed and held: its results.
    Finished {
        results: Box<Results>,
        device_changed: bool,
        /// A calibration test's outcome.
        calibration: Option<ddi_session::calibration::Outcome>,
    },
    /// The player quit.
    Aborted,
    /// The window was hidden or the sound stopped.
    Interrupted,
}

/// A play of one chart.
pub struct Play {
    pub request: Request,
    pub loaded: Rc<Loaded>,
    pub session: Session,
    pub feed: Rc<RefCell<Feed>>,
    /// The song offset the notes were shifted by.
    pub played_offset: f64,
    /// The song shifted by it (`None`: by nothing, `loaded`'s).
    shifted: Option<ddi_chart::Song>,
    /// The display's refresh rate as the play started.
    pub refresh_hz: f64,
    /// Sound dropouts before the play, to tell its own.
    xruns_at_start: u64,
    /// The play was kept in the history (at its fail, or its end).
    recorded: bool,
    /// What keeping it said (`None`: not answered yet).
    pub saved: Rc<RefCell<Option<Result<(), String>>>>,
    /// The graphics backend, for the record.
    pub graphics: String,
    /// The play's id once kept (its replay).
    pub play_id: Option<String>,
}

/// A seed for the Shuffle turn.
fn random_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    h.finish()
}

/// Milliseconds since the Unix epoch.
pub fn epoch_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

impl Play {
    /// Starts `loaded`'s chart on `audio` (the music scheduled on its
    /// clock); `devices`: the audio path and display it plays on, whose
    /// offsets it takes (`None`: the defaults).
    pub fn start(
        request: Request,
        loaded: Rc<Loaded>,
        settings: &Settings,
        audio: Audio,
        feed: Rc<RefCell<Feed>>,
        devices: Option<DeviceProfile>,
        refresh_hz: f64,
    ) -> Result<Play, String> {
        let mut config = SessionConfig::from_settings(settings, request.auto, devices);
        config.auto_pad = request.auto_pad;
        config.auto_bias = request.bias;
        config.seed = random_seed();
        if let Some(mode) = request.calibration {
            config.calibrate(mode);
        }
        // A replay: its log on the lanes, its options and rules, the
        // offsets this device needs.
        if let Some(r) = &request.replay {
            config.set_replay(r);
        }
        let shift = request.song_shift(settings);
        config.ghost = request
            .ghost
            .as_deref()
            .filter(|_| request.replay.is_none() && request.calibration.is_none())
            .map(|g| g.clone().for_play(shift));
        let song = loaded.song.shifted(shift);
        let xruns_at_start = audio.xruns();
        let inputs = DesktopInputs::new(feed.clone(), request.auto_pad);
        let session = Session::start(
            &song,
            request.chart,
            settings,
            config,
            audio,
            loaded.music.clone(),
            inputs,
        )?;
        let shifted = match song {
            std::borrow::Cow::Owned(s) => Some(s),
            std::borrow::Cow::Borrowed(_) => None,
        };
        Ok(Play {
            request,
            loaded,
            session,
            feed,
            played_offset: shift,
            shifted,
            refresh_hz,
            xruns_at_start,
            recorded: false,
            saved: Rc::default(),
            graphics: String::new(),
            play_id: None,
        })
    }

    /// The song as played: shifted by the song offset.
    pub fn song(&self) -> &ddi_chart::Song {
        self.shifted.as_ref().unwrap_or(&self.loaded.song)
    }

    /// Sound dropouts during the play.
    pub fn xruns(&self) -> u64 {
        self.session
            .audio()
            .xruns()
            .saturating_sub(self.xruns_at_start)
    }

    /// Advances to host time `now` (the edges in the feed judged); how
    /// it ended, once it did. A fail or the end is kept in `plays`.
    pub fn tick(&mut self, now: HostTime, plays: Option<&Rc<Plays>>) -> Option<Ended> {
        self.feed.borrow_mut().now = now;
        // The output kept (voices done given back, its latencies read
        // again); opened again (it failed, or the default output
        // changed), its music is gone: the play stops as interrupted.
        if self.session.audio_mut().tick() {
            self.feed.borrow_mut().interrupted = true;
        }
        let (_events, event) = self.session.tick(now);
        if let Some(SessionEvent::Failed { results, facts }) = self.session.take_fail_report() {
            self.record(&results, &facts, plays);
        }
        match event? {
            SessionEvent::Finished {
                results,
                facts,
                device_changed,
                calibration,
            } => {
                self.record(&results, &facts, plays);
                Some(Ended::Finished {
                    results,
                    device_changed,
                    calibration,
                })
            }
            SessionEvent::Failed { results, facts } => {
                self.record(&results, &facts, plays);
                None
            }
            SessionEvent::Aborted => Some(Ended::Aborted),
            SessionEvent::Interrupted => Some(Ended::Interrupted),
        }
    }

    /// Keeps the play in the history, once.
    fn record(&mut self, results: &Results, facts: &PlayFacts, plays: Option<&Rc<Plays>>) {
        // A calibration test is not a play, nor a replay.
        if std::mem::replace(&mut self.recorded, true)
            || self.request.calibration.is_some()
            || self.request.replay.is_some()
        {
            return;
        }
        let Some(plays) = plays else {
            return;
        };
        let settings = Settings::load();
        // Whole milliseconds, as the web's `Date.now()`.
        let time = epoch_ms().round();
        let played = ddi_session::record::Played {
            song_id: &self.request.song,
            song: &self.loaded.song,
            chart: self.request.chart,
            settings: &settings,
            stored_settings: ddi_platform::storage::get(
                ddi_platform::storage::Area::Local,
                ddi_ui::state::settings::KEY,
            )
            .and_then(|s| serde_json::from_str(&s).ok()),
            devices: self.session.devices.as_ref(),
            refresh_hz: self.refresh_hz,
            graphics: self.graphics.clone(),
            song_offset: self.played_offset,
            autoplay: self.request.auto.then_some(Autoplay {
                pad: self.request.auto_pad,
                bias: self.request.bias,
            }),
            time,
            app_version: env!("CARGO_PKG_VERSION"),
        };
        let Some(record) = ddi_session::record::play_record(&played, results, facts) else {
            return;
        };
        let id = ddi_platform::history::play_id(time, random_seed() as u32);
        self.play_id = Some(id.clone());
        let saved = self.saved.clone();
        let r = plays.record(id, &record, &facts.log, results, move |r| {
            if let Err(e) = &r {
                eprintln!("play history: keeping the play: {e}");
            }
            *saved.borrow_mut() = Some(r);
        });
        if let Err(e) = r {
            eprintln!("play history: {e}");
            *self.saved.borrow_mut() = Some(Err(e));
        }
    }
}
