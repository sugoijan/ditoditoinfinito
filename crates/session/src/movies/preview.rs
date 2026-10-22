//! Song select's preview movie (`.ssc` `#PREVIEWVID`,
//! `docs/plans/lane-covers.md`, "Preview video"), both shells': the song
//! song select shows is followed each frame ([`PreviewMovie::follow`]);
//! once the selection has settled ([`SETTLE_SECONDS`], the sample's own
//! wait) its movie plays, muted, looping, from its start, on a schedule
//! of one segment ([`Step::Play`], for the shell's [`super::MovieSlots`]
//! as a play's background movies are driven), and its frames are drawn
//! into the picture song select shows in the banner's place
//! (`ddi_render::movie_picture`; `ddi_ui::screens::select::PREVIEW_MOVIE`).
//! The banner shows until the first frame is in, and again as soon as the
//! selection moves, song select is left or covered by another screen, or
//! the movie cannot play. Under reduced motion the first frame shows,
//! still: the decoder closes once it is in. The Background videos setting
//! off never opens one ([`movie_of`]); automatic plays it as on (a
//! picture this small costs no play frames).
//!
//! The shell opens the movie's file and decoder, uploads its frames, draws
//! the picture, says when a frame is in ([`PreviewMovie::shown`]), and
//! tells song select which song's movie shows ([`PreviewMovie::news`],
//! `ddi_ui::screens::select::Feed::preview_movie`).

use ddi_library::backgrounds::{BgImage, BgSegment};
use ddi_library::manifest::ManifestEntry;
use ddi_ui::LayerInfo;
use ddi_ui::state::settings::VideoMode;

/// Wait after a selection before its movie opens, seconds (the sample's,
/// so the wheel moving through songs opens none).
pub const SETTLE_SECONDS: f64 = 0.3;

/// A movie whose first frame has not come this long after it opened fails
/// (a decoder that stalled): the banner stays and frames stop.
pub const FIRST_FRAME_SECONDS: f64 = 10.0;

/// The preview movie of `entry` that may play under the Background videos
/// setting `mode`: its name in the song's folder, when it has one the
/// import kept and both shells can play (`VideoRef::plays`), unless the
/// setting is off.
pub fn movie_of(entry: &ManifestEntry, mode: VideoMode) -> Option<String> {
    if mode == VideoMode::Off {
        return None;
    }
    entry
        .preview_video
        .as_ref()
        .filter(|v| v.plays())
        .map(|v| v.name.to_string())
}

/// Whether song select is what shows among `layers` (bottom first): the
/// topmost screen, under whatever dialogs, is song select's, not leaving.
pub fn select_shows(layers: &[LayerInfo]) -> bool {
    layers
        .iter()
        .rev()
        .find(|l| !l.overlay)
        .is_some_and(|l| l.name == "select" && l.phase != "out")
}

/// What the shell does this frame ([`PreviewMovie::step`]).
#[derive(Debug, PartialEq)]
pub enum Step<'a> {
    /// No decoder: no movie, the selection settling, the movie failed, or
    /// its still frame in (reduced motion). One left open closes.
    Idle,
    /// Decode movie `name` for second `t` of the schedule `layers` (one
    /// layer, the movie looping from 0: `MovieSlots::update`'s).
    Play {
        name: &'a str,
        layers: &'a [Vec<BgSegment>],
        t: f64,
    },
}

/// Song select's preview movie (module doc).
#[derive(Debug, Default)]
pub struct PreviewMovie {
    /// The song song select shows, as last followed, and its movie.
    song: Option<String>,
    movie: Option<String>,
    /// When the song was selected (the shell's seconds).
    since: f64,
    /// Changes with each selection: what the shell opened for an older
    /// one goes.
    generation: u64,
    /// When the movie's first frame was in: its clock runs from there.
    first: Option<f64>,
    /// Frames drawn into the picture this selection.
    frames: u64,
    failed: Option<String>,
    /// The movie's schedule: one segment, from 0.
    layers: Vec<Vec<BgSegment>>,
    still: bool,
    /// The song song select was last told shows its movie.
    told: Option<String>,
}

impl PreviewMovie {
    /// Follows song select at `now` (seconds): `song` the song it shows
    /// (`None`: none, or song select is not what shows,
    /// [`select_shows`]); `still` under reduced motion; `movie` names the
    /// song's preview movie that may play ([`movie_of`]), asked only when
    /// the song changes.
    pub fn follow(
        &mut self,
        song: Option<&str>,
        still: bool,
        now: f64,
        movie: impl FnOnce(&str) -> Option<String>,
    ) {
        self.still = still;
        let song = song.filter(|s| !s.is_empty());
        if self.song.as_deref() == song {
            return;
        }
        self.song = song.map(str::to_string);
        self.movie = song.and_then(movie);
        self.generation += 1;
        self.since = now;
        self.first = None;
        self.frames = 0;
        self.failed = None;
        self.layers = self
            .movie
            .iter()
            .map(|name| vec![BgSegment::new(0.0, BgImage::Movie(name.clone()))])
            .collect();
    }

    /// Changes with each selection ([`PreviewMovie::follow`]).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The song selected and its movie, when it has one that may play.
    pub fn wanted(&self) -> Option<(&str, &str)> {
        Some((self.song.as_deref()?, self.movie.as_deref()?))
    }

    /// What the shell does at `now`.
    pub fn step(&mut self, now: f64) -> Step<'_> {
        if self.movie.is_some()
            && self.first.is_none()
            && now - self.since > SETTLE_SECONDS + FIRST_FRAME_SECONDS
        {
            self.fail("no frame came in time".into());
        }
        let Some(name) = self.movie.as_deref() else {
            return Step::Idle;
        };
        if self.failed.is_some()
            || now - self.since < SETTLE_SECONDS
            || (self.still && self.first.is_some())
        {
            return Step::Idle;
        }
        let t = match self.first {
            // The first frame, before the clock runs.
            None => 0.0,
            Some(first) => (now - first).max(0.0),
        };
        Step::Play {
            name,
            layers: &self.layers,
            t,
        }
    }

    /// A frame of the movie went into the picture at `now`.
    pub fn shown(&mut self, now: f64) {
        if self.movie.is_none() || self.failed.is_some() {
            return;
        }
        self.first.get_or_insert(now);
        self.frames += 1;
    }

    /// Frames drawn into the picture this selection.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The movie cannot play (`why`): the banner stays until the selection
    /// changes.
    pub fn fail(&mut self, why: String) {
        if self.movie.is_some() && self.failed.is_none() {
            self.failed = Some(why);
        }
    }

    pub fn failed(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// The song whose movie the picture shows now: a frame of it is in and
    /// it has not failed.
    pub fn showing(&self) -> Option<&str> {
        self.first
            .filter(|_| self.failed.is_none())
            .and(self.song.as_deref())
    }

    /// What song select is to be told shows, once it changed: the song
    /// whose movie shows (`None`: the banner again).
    pub fn news(&mut self) -> Option<Option<String>> {
        if self.told.as_deref() == self.showing() {
            return None;
        }
        self.told = self.showing().map(str::to_string);
        Some(self.told.clone())
    }

    /// Whether frames are to go on: a movie wanted is settling, opening
    /// or playing (not once it failed, nor once its still frame is in).
    pub fn busy(&self) -> bool {
        self.movie.is_some() && self.failed.is_none() && !(self.still && self.first.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movies::{MovieShell, MovieSlots};
    use ddi_library::manifest::VideoRef;
    use ddi_platform::video::{
        VideoCodec, VideoContainer, VideoDecoder, VideoEvent, VideoFormat, VideoFrame, VideoInfo,
        YuvMatrix, planes_len,
    };

    /// A song selected, as the shell names its movie.
    fn movie(_: &str) -> Option<String> {
        Some("Preview.mp4".into())
    }

    fn play_t(step: Step<'_>) -> Option<f64> {
        match step {
            Step::Play { t, .. } => Some(t),
            Step::Idle => None,
        }
    }

    #[test]
    fn the_movie_waits_for_the_selection_to_settle_and_starts_from_its_first_frame() {
        let mut m = PreviewMovie::default();
        m.follow(Some("a"), false, 10.0, movie);
        assert_eq!(m.wanted(), Some(("a", "Preview.mp4")));
        assert!(m.busy());
        assert_eq!(m.step(10.1), Step::Idle);
        // Settled: opened at its start, held there until a frame is in.
        assert_eq!(play_t(m.step(10.3)), Some(0.0));
        assert_eq!(play_t(m.step(11.0)), Some(0.0));
        assert_eq!(m.showing(), None);
        m.shown(11.0);
        assert_eq!(m.showing(), Some("a"));
        assert_eq!(play_t(m.step(12.5)), Some(1.5));
        assert_eq!(m.frames(), 1);
    }

    #[test]
    fn a_new_selection_starts_over_and_leaving_song_select_stops_it() {
        let mut m = PreviewMovie::default();
        m.follow(Some("a"), false, 0.0, movie);
        m.shown(1.0);
        let g = m.generation();
        // The same song again changes nothing (the movie is not asked).
        m.follow(Some("a"), false, 2.0, |_| unreachable!());
        assert_eq!(m.generation(), g);
        assert_eq!(play_t(m.step(2.0)), Some(1.0));
        // Another song: the banner until its own first frame.
        m.follow(Some("b"), false, 3.0, movie);
        assert_ne!(m.generation(), g);
        assert_eq!(m.showing(), None);
        assert_eq!(m.step(3.1), Step::Idle);
        // A song without one, then song select covered: nothing.
        m.follow(Some("c"), false, 4.0, |_| None);
        assert_eq!(m.wanted(), None);
        assert!(!m.busy());
        assert_eq!(m.step(9.0), Step::Idle);
        m.follow(None, false, 5.0, movie);
        assert_eq!(m.step(9.0), Step::Idle);
        // Back on the song: it plays again, from its start.
        m.follow(Some("c"), false, 6.0, movie);
        assert_eq!(play_t(m.step(6.5)), Some(0.0));
    }

    #[test]
    fn reduced_motion_keeps_the_first_frame_still() {
        let mut m = PreviewMovie::default();
        m.follow(Some("a"), true, 0.0, movie);
        assert_eq!(play_t(m.step(0.5)), Some(0.0));
        m.shown(0.6);
        // The decoder closes; the picture keeps the frame; no more frames.
        assert_eq!(m.step(1.0), Step::Idle);
        assert_eq!(m.showing(), Some("a"));
        assert!(!m.busy());
        // Motion allowed again: it plays on.
        m.follow(Some("a"), false, 2.0, movie);
        assert!(m.busy());
        assert_eq!(play_t(m.step(2.6)), Some(2.0));
    }

    #[test]
    fn a_movie_that_cannot_play_leaves_the_banner() {
        let mut m = PreviewMovie::default();
        m.follow(Some("a"), false, 0.0, movie);
        m.shown(0.5);
        assert_eq!(m.news(), Some(Some("a".into())));
        assert_eq!(m.news(), None);
        m.fail("no decoder".into());
        assert_eq!(m.failed(), Some("no decoder"));
        assert_eq!(m.showing(), None);
        assert_eq!(m.step(1.0), Step::Idle);
        assert!(!m.busy());
        assert_eq!(m.news(), Some(None));
        // Frames after the failure do not bring it back.
        m.shown(1.0);
        assert_eq!(m.showing(), None);
        // A decoder that never answers fails too.
        m.follow(Some("b"), false, 2.0, movie);
        assert!(matches!(m.step(12.0), Step::Play { .. }));
        assert_eq!(
            m.step(2.0 + SETTLE_SECONDS + FIRST_FRAME_SECONDS + 0.1),
            Step::Idle
        );
        assert_eq!(m.failed(), Some("no frame came in time"));
        assert!(!m.busy());
    }

    fn entry(video: Option<VideoRef>) -> ManifestEntry {
        let mut e = ddi_library::manifest::summarize(
            Default::default(),
            &ddi_chart::formats::sm::parse_simfile("#TITLE:x;", "ssc").unwrap(),
        );
        e.preview_video = video;
        e
    }

    fn video(codec: VideoCodec, stored: bool) -> Option<VideoRef> {
        Some(VideoRef {
            name: "Preview.mp4".into(),
            format: VideoFormat {
                container: VideoContainer::Mp4,
                codec,
            },
            stored,
        })
    }

    #[test]
    fn the_videos_setting_off_and_movies_not_kept_never_play() {
        let kept = entry(video(VideoCodec::H264, true));
        assert_eq!(
            movie_of(&kept, VideoMode::Auto).as_deref(),
            Some("Preview.mp4")
        );
        assert_eq!(
            movie_of(&kept, VideoMode::On).as_deref(),
            Some("Preview.mp4")
        );
        assert_eq!(movie_of(&kept, VideoMode::Off), None);
        assert_eq!(
            movie_of(&entry(video(VideoCodec::H264, false)), VideoMode::On),
            None
        );
        assert_eq!(
            movie_of(&entry(video(VideoCodec::Av1, true)), VideoMode::On),
            None
        );
        assert_eq!(movie_of(&entry(None), VideoMode::On), None);
    }

    fn layer(name: &str, overlay: bool, phase: &'static str) -> LayerInfo {
        LayerInfo {
            name: name.into(),
            overlay,
            modal: overlay,
            phase,
            focused: None,
        }
    }

    #[test]
    fn song_select_shows_under_its_dialogs_only() {
        let select = layer("select", false, "active");
        assert!(select_shows(std::slice::from_ref(&select)));
        assert!(select_shows(&[select.clone(), layer("import", true, "in")]));
        assert!(!select_shows(&[
            select.clone(),
            layer("options", false, "in")
        ]));
        assert!(!select_shows(&[layer("select", false, "out")]));
        assert!(!select_shows(&[]));
    }

    /// A 2 s movie at 10 frames a second, decoded at once.
    struct Decoder {
        next: f64,
        asked: u32,
        opened: bool,
        ended: bool,
    }

    impl VideoDecoder for Decoder {
        fn seek(&mut self, t: f64) {
            self.next = (t * 10.0).floor() / 10.0;
            self.asked = 0;
            self.ended = false;
        }

        fn want(&mut self, n: u32) {
            self.asked += n;
        }

        fn pending(&self) -> u32 {
            self.asked
        }

        fn poll(&mut self, out: &mut Vec<VideoEvent>) {
            if !std::mem::replace(&mut self.opened, true) {
                out.push(VideoEvent::Opened(VideoInfo {
                    width: 4,
                    height: 4,
                    duration: Some(2.0),
                    codec: "fake".into(),
                }));
            }
            while self.asked > 0 && !self.ended {
                if self.next > 2.0 - 0.05 {
                    self.ended = true;
                    out.push(VideoEvent::End {
                        last_pts: Some(self.next - 0.1),
                    });
                    break;
                }
                out.push(VideoEvent::Frame(VideoFrame {
                    pts: self.next,
                    width: 4,
                    height: 4,
                    matrix: YuvMatrix::Bt601,
                    full_range: false,
                    planes: vec![128; planes_len(4, 4)],
                    decode_ms: 0.0,
                    external: None,
                }));
                self.next += 0.1;
                self.asked -= 1;
            }
        }
    }

    #[derive(Default)]
    struct Shell {
        opened: Vec<String>,
    }

    impl MovieShell for Shell {
        type Decoder = Decoder;

        fn open(&mut self, name: &str) -> Option<Result<Decoder, String>> {
            self.opened.push(name.to_string());
            Some(Ok(Decoder {
                next: 0.0,
                asked: 0,
                opened: false,
                ended: false,
            }))
        }

        fn warn(&self, _line: &str) {}
    }

    #[test]
    fn the_shells_movie_slots_play_it_looping_from_its_start() {
        let mut m = PreviewMovie::default();
        m.follow(Some("a"), false, 0.0, movie);
        let mut slots = MovieSlots::new(Shell::default());
        let mut shown = Vec::new();
        let mut now = 0.0;
        while now < 6.0 {
            if let Step::Play { layers, t, .. } = m.step(now) {
                let due = slots.update(layers, t);
                for (name, frame) in due.show {
                    assert_eq!(name, "Preview.mp4");
                    slots.set_texture(&name, 0);
                    slots.shown(&name);
                    m.shown(now);
                    shown.push((t, frame.pts));
                }
            }
            now += 1.0 / 60.0;
        }
        assert_eq!(slots.shell().opened, ["Preview.mp4"]);
        // Its first frame first, then frames as its clock runs.
        assert_eq!(shown[0].1, 0.0);
        assert!(shown.len() > 40, "{} frames", shown.len());
        // It loops: the movie's start again after its 2 s.
        let wrapped = shown.windows(2).filter(|w| w[1].1 < w[0].1).count();
        assert!(wrapped >= 1, "{shown:?}");
        assert_eq!(slots.shown_ids().count(), 1);
    }
}
