//! A play's backgrounds beside the song's schedule
//! (`ddi_library::backgrounds`): the shape they are fitted into, the
//! display's refresh rate measured from the frames, and the automatic video
//! setting's step (`ddi_platform::video::VideoGuard`) each frame. Moved
//! from `web/src/game_loop.rs` (step 7.1); the shell keeps the textures and
//! the movies' decoders.

use ddi_platform::video::{GuardAction, Second, VideoGuard};
use ddi_ui::state::settings::BackgroundFrame;

pub const FOUR_THREE: f32 = 4.0 / 3.0;
pub const SIXTEEN_NINE: f32 = 16.0 / 9.0;

/// The cabinet screen shape nearest to `aspect` (by ratio): a square
/// jacket fills a 4:3 frame, an ultrawide picture a 16:9 one.
pub fn cabinet_shape(aspect: f32) -> f32 {
    if aspect < (FOUR_THREE * SIXTEEN_NINE).sqrt() {
        FOUR_THREE
    } else {
        SIXTEEN_NINE
    }
}

/// Frames per second over the last second, from frame timestamps.
#[derive(Clone, Debug, Default)]
pub struct FpsMeter {
    window_start: f64,
    count: u32,
    /// The rate of the last full second.
    pub fps: f64,
    last: Option<f64>,
    /// Longest frame interval in the current window, ms.
    worst_ms: f64,
    /// The longest of the last full second, ms.
    pub worst_last: f64,
    /// Recent per-second rates; frames can be dropped but never exceed the
    /// refresh rate, so the maximum is the robust estimate. Single
    /// intervals are no clue: Firefox's frame timestamps jitter by several
    /// ms (1.2–9 ms apart at a steady 125 fps), which made the shortest
    /// one read as a faster screen and the automatic video setting turn
    /// every movie off.
    history: Vec<f64>,
}

impl FpsMeter {
    pub fn new() -> FpsMeter {
        FpsMeter::default()
    }

    /// Best estimate of the display refresh rate, or `None` before two full
    /// seconds have been measured.
    pub fn refresh_estimate(&self) -> Option<f64> {
        if self.history.len() < 2 {
            return None;
        }
        Some(self.history.iter().copied().fold(f64::MIN, f64::max))
    }

    /// Counts a frame; the rate of a second that ended with it, if one did.
    pub fn tick(&mut self, time_ms: f64) -> Option<f64> {
        if let Some(last) = self.last {
            let dt = time_ms - last;
            self.worst_ms = self.worst_ms.max(dt);
        }
        self.last = Some(time_ms);
        self.count += 1;
        if time_ms - self.window_start >= 1000.0 {
            self.fps = self.count as f64 * 1000.0 / (time_ms - self.window_start);
            self.count = 0;
            self.window_start = time_ms;
            self.worst_last = self.worst_ms;
            self.worst_ms = 0.0;
            self.history.push(self.fps);
            if self.history.len() > 4 {
                self.history.remove(0);
            }
            return Some(self.fps);
        }
        None
    }

    /// The second that ended with the frame [`FpsMeter::tick`] counted, as
    /// the video guard measures it.
    pub fn second(&self, fps: f64) -> Second {
        Second {
            fps,
            worst_ms: self.worst_last,
        }
    }
}

/// The shape backgrounds are fitted into, by the setting: the whole screen,
/// 4:3, 16:9, or automatic: the song's first movie's picture (its black
/// bars left out), else the opening movie's still, else the background
/// image, settled once a movie frame shows, or when the song starts with
/// nothing left to learn, so it never jumps while a movie plays.
#[derive(Clone, Debug, Default)]
pub struct FrameShape {
    pub setting: BackgroundFrame,
    /// The automatic frame, once settled.
    frozen: Option<Option<f32>>,
    /// Width over height of the song's background image.
    pub song_bg_aspect: Option<f32>,
    /// Width over height of the opening movie's still.
    pub still_aspect: Option<f32>,
}

impl FrameShape {
    /// A song's shape under `setting`, nothing learnt yet.
    pub fn new(setting: BackgroundFrame) -> FrameShape {
        FrameShape {
            setting,
            ..FrameShape::default()
        }
    }

    /// The frame now (`None`: the whole screen). `movie` asks (only when
    /// the automatic frame is not settled) for the first movie's
    /// picture's aspect, or whether its probe is still running;
    /// `shown_any`: a movie frame has shown; `started`: the song started.
    pub fn aspect(
        &mut self,
        movie: impl FnOnce() -> Result<f32, bool>,
        shown_any: bool,
        started: bool,
    ) -> Option<f32> {
        match self.setting {
            BackgroundFrame::Screen => return None,
            BackgroundFrame::FourThree => return Some(FOUR_THREE),
            BackgroundFrame::SixteenNine => return Some(SIXTEEN_NINE),
            BackgroundFrame::Auto => {}
        }
        if let Some(frame) = self.frozen {
            return frame;
        }
        let (movie, probing) = match movie() {
            Ok(a) => (Some(a), false),
            Err(probing) => (None, probing),
        };
        // The still has the movie's own shape (bars included) until the
        // probe answers.
        let frame = movie
            .or(self.still_aspect)
            .or(self.song_bg_aspect)
            .map(cabinet_shape);
        if shown_any || (started && !probing) {
            self.frozen = Some(frame);
        }
        frame
    }
}

/// The automatic video setting over a play: the guard, and the refresh
/// rate measured before the play started.
#[derive(Clone, Debug, Default)]
pub struct MovieGuard {
    pub guard: VideoGuard,
    refresh_at_start: f64,
}

impl MovieGuard {
    /// A play starts on a display refreshing at `refresh` Hz.
    pub fn start(&mut self, refresh: f64) {
        self.guard = VideoGuard::default();
        self.refresh_at_start = refresh;
    }

    /// This frame's step (only under the automatic setting): `drawing`
    /// what a movie draws, `second` a second that ended, `refresh` the
    /// rate measured now, `late` the movies' late frames so far.
    pub fn step(
        &mut self,
        time_ms: f64,
        drawing: Option<usize>,
        second: Option<Second>,
        refresh: f64,
        late: u64,
    ) -> Option<GuardAction> {
        let refresh = self.refresh_at_start.max(refresh);
        self.guard.update(time_ms, drawing, second, refresh, late)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_and_the_settled_frame() {
        assert_eq!(cabinet_shape(1.4), FOUR_THREE);
        assert_eq!(cabinet_shape(1.6), SIXTEEN_NINE);
        let mut f = FrameShape {
            song_bg_aspect: Some(1.33),
            ..FrameShape::default()
        };
        // The probe still running: the background image's shape, not
        // settled even once the song started.
        assert_eq!(f.aspect(|| Err(true), false, true), Some(FOUR_THREE));
        // The movie's picture answers: its shape, settled once a frame
        // shows; later answers change nothing.
        assert_eq!(f.aspect(|| Ok(1.78), true, true), Some(SIXTEEN_NINE));
        assert_eq!(f.aspect(|| Ok(1.0), true, true), Some(SIXTEEN_NINE));
        f.setting = BackgroundFrame::Screen;
        assert_eq!(f.aspect(|| Ok(1.0), true, true), None);
    }

    #[test]
    fn the_meter_measures_seconds_and_the_refresh_is_the_fastest() {
        let mut m = FpsMeter::new();
        let mut seconds = Vec::new();
        let mut t = 0.0;
        for i in 0..400 {
            // 120 Hz, with one 50 ms stall.
            t += if i == 150 { 50.0 } else { 1000.0 / 120.0 };
            if let Some(fps) = m.tick(t) {
                seconds.push((fps, m.worst_last));
            }
        }
        assert!(seconds.len() >= 2);
        let hz = m.refresh_estimate().unwrap();
        assert!((hz - 120.0).abs() < 3.0, "{hz}");
        assert!(seconds.iter().any(|(_, worst)| *worst >= 49.0));
    }
}
