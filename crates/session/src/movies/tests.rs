use super::*;
use ddi_library::backgrounds::MovieEffect;
use ddi_platform::video::{VideoEvent, VideoInfo, YuvMatrix, planes_len};
use std::cell::RefCell;
use std::rc::Rc;

/// What the fake decoders were asked.
#[derive(Debug, Default)]
struct Log {
    opened: Vec<String>,
    seeks: Vec<f64>,
    discarded: usize,
    stopped: usize,
    warnings: Vec<String>,
}

/// A movie of `duration` seconds at 10 frames a second, 4×4, decoded at
/// once.
struct FakeDecoder {
    duration: f64,
    log: Rc<RefCell<Log>>,
    opened: bool,
    next: f64,
    asked: u32,
    ended: bool,
}

fn frame(pts: f64) -> VideoFrame {
    VideoFrame {
        pts,
        width: 4,
        height: 4,
        matrix: YuvMatrix::Bt601,
        full_range: false,
        planes: vec![128; planes_len(4, 4)],
        decode_ms: 0.0,
        external: None,
    }
}

impl VideoDecoder for FakeDecoder {
    fn seek(&mut self, t: f64) {
        self.log.borrow_mut().seeks.push(t);
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
        if !self.opened {
            self.opened = true;
            out.push(VideoEvent::Opened(VideoInfo {
                width: 4,
                height: 4,
                duration: Some(self.duration),
                codec: "fake".into(),
            }));
        }
        while self.asked > 0 && !self.ended {
            if self.next > self.duration + 1e-9 {
                self.ended = true;
                out.push(VideoEvent::End {
                    last_pts: Some(self.next - 0.1),
                });
                break;
            }
            out.push(VideoEvent::Frame(frame(self.next)));
            self.next += 0.1;
            self.asked -= 1;
        }
        if self.ended {
            self.asked = 0;
        }
    }
}

#[derive(Default)]
struct FakeShell {
    log: Rc<RefCell<Log>>,
    /// Movies that cannot open, and why.
    broken: Vec<(String, String)>,
    /// Answers to [`MovieShell::fall_back`], taken one per call.
    fall_backs: Vec<Result<bool, String>>,
}

impl MovieShell for FakeShell {
    type Decoder = FakeDecoder;

    fn open(&mut self, name: &str) -> Option<Result<FakeDecoder, String>> {
        if name == "missing" {
            return None;
        }
        if let Some((_, e)) = self.broken.iter().find(|(n, _)| n == name) {
            return Some(Err(e.clone()));
        }
        self.log.borrow_mut().opened.push(name.to_string());
        Some(Ok(FakeDecoder {
            duration: 0.5,
            log: Rc::clone(&self.log),
            opened: false,
            next: 0.0,
            asked: 0,
            ended: false,
        }))
    }

    fn fall_back(&mut self, _name: &str, _decoder: &mut FakeDecoder) -> Result<bool, String> {
        if self.fall_backs.is_empty() {
            Ok(false)
        } else {
            self.fall_backs.remove(0)
        }
    }

    fn discard(&mut self, _frame: VideoFrame) {
        self.log.borrow_mut().discarded += 1;
    }

    fn stopped(&mut self) {
        self.log.borrow_mut().stopped += 1;
    }

    fn warn(&self, line: &str) {
        self.log.borrow_mut().warnings.push(line.to_string());
    }
}

fn seg(seconds: f64, image: BgImage) -> BgSegment {
    BgSegment::new(seconds, image)
}

fn movie(name: &str) -> BgImage {
    BgImage::Movie(name.to_string())
}

/// The song background, then movie `name` from second 1.
fn layers_with(name: &str) -> Vec<Vec<BgSegment>> {
    vec![
        vec![seg(f64::NEG_INFINITY, BgImage::Song), seg(1.0, movie(name))],
        Vec::new(),
    ]
}

/// Shows what `update` hands out as a shell would: a texture per movie.
fn run(slots: &mut MovieSlots<FakeShell>, layers: &[Vec<BgSegment>], t: f64) -> Vec<f64> {
    let due = slots.update(layers, t);
    for (name, w, h, _) in due.early {
        if slots.fits(&name, w, h, 8192) && slots.texture(&name).is_none() {
            slots.set_texture(&name, 7);
        }
    }
    let mut shown = Vec::new();
    for (name, frame) in due.show {
        if slots.texture(&name).is_none() {
            slots.set_texture(&name, 7);
        }
        slots.shown(&name);
        shown.push(frame.pts);
    }
    shown
}

#[test]
fn opens_ahead_and_shows_the_held_frame_at_the_change() {
    let layers = layers_with("a");
    let mut slots = MovieSlots::new(FakeShell::default());
    // Before the change: opened, its first frame held, its texture made.
    let due = slots.update(&layers, 0.0);
    assert_eq!(due.early, vec![("a".to_string(), 4, 4, false)]);
    assert!(due.show.is_empty());
    assert_eq!(slots.opened().len(), 1);
    let v = slots.version();
    slots.set_texture("a", 7);
    assert!(slots.version() > v, "a new texture changes the version");
    assert_eq!(slots.shown_ids().count(), 0, "no frame of this run yet");
    assert_eq!(slots.drawing(&layers), None);
    // The change: the held frame goes up at once.
    assert_eq!(run(&mut slots, &layers, 1.0), vec![0.0]);
    assert!(slots.shown_any());
    assert_eq!(slots.shown_ids().collect::<Vec<_>>(), vec![(movie("a"), 7)]);
    assert_eq!(slots.drawing(&layers), Some(LAYERS));
    // Frames follow the clock; one decoder all along.
    assert!(run(&mut slots, &layers, 1.05).is_empty());
    assert_eq!(run(&mut slots, &layers, 1.15), vec![0.1]);
    assert_eq!(slots.shell().log.borrow().opened, vec!["a"]);
    assert_eq!(slots.stats().shown, 2);
}

#[test]
fn a_movie_that_cannot_open_fails_once() {
    let layers = layers_with("b");
    let mut slots = MovieSlots::new(FakeShell {
        broken: vec![("b".into(), "no such file".into())],
        ..Default::default()
    });
    let v = slots.version();
    assert!(slots.opening_plays(&layers[0]));
    run(&mut slots, &layers, 0.0);
    run(&mut slots, &layers, 1.0);
    assert!(slots.version() > v);
    assert!(slots.opened().is_empty());
    assert_eq!(slots.failed().len(), 1);
    assert_eq!(slots.shell().log.borrow().warnings.len(), 1);
    assert_eq!(
        slots.trouble().as_deref(),
        Some("the video b could not play: no such file")
    );
    assert!(!slots.opening_plays(&layers[0]));
    // A retry tries it again; the results still say why.
    slots.stop();
    assert!(slots.failed().is_empty());
    assert!(slots.trouble().is_some());
    slots.reset();
    assert!(slots.trouble().is_none());
}

#[test]
fn a_movie_not_there_yet_is_tried_again() {
    let layers = layers_with("missing");
    let mut slots = MovieSlots::new(FakeShell::default());
    run(&mut slots, &layers, 1.0);
    assert!(slots.opened().is_empty());
    assert!(slots.failed().is_empty());
}

#[test]
fn a_texture_too_large_fails_the_movie() {
    let mut slots = MovieSlots::new(FakeShell::default());
    assert!(!slots.fits("a", 9000, 10, 8192));
    assert!(!slots.fits("b", 0, 10, 8192));
    assert!(slots.fits("c", 8192, 10, 8192));
    assert_eq!(slots.failed().len(), 2);
}

#[test]
fn every_close_learns_the_end() {
    let layers = layers_with("a");
    let mut slots = MovieSlots::new(FakeShell::default());
    let mut t = 0.0;
    while t < 2.0 {
        run(&mut slots, &layers, t);
        t += 0.05;
    }
    assert!(slots.ends.is_empty(), "learned when the track closes");
    let v = slots.version();
    slots.stop();
    assert!(slots.version() > v);
    assert_eq!(slots.ends, vec![("a".to_string(), 0.5)]);
    assert_eq!(slots.shell().log.borrow().stopped, 1);
    assert_eq!(slots.shown_ids().count(), 0, "a retry shows no old frame");
    // Stopped again (desktop, every frame after the song): nothing changes.
    let v = slots.version();
    slots.stop();
    assert_eq!(slots.version(), v);
    // The retry's track knows it: past the end it wraps at once (the
    // seek lands within the movie, not at 2.5).
    run(&mut slots, &layers, 3.5);
    let log = slots.shell().log.borrow();
    let seek = *log.seeks.last().unwrap();
    assert!(seek <= 0.5, "{seek}");
}

#[test]
fn a_decoder_moves_to_its_movie_s_next_segment() {
    // The same movie at 1 and 5: a jump past the first moves its decoder.
    let layers = vec![
        vec![
            seg(f64::NEG_INFINITY, BgImage::Song),
            seg(1.0, movie("a")),
            seg(5.0, movie("a")),
        ],
        Vec::new(),
    ];
    let mut slots = MovieSlots::new(FakeShell::default());
    run(&mut slots, &layers, 1.5);
    let open = &slots.opened()[0];
    assert_eq!((open.layer, open.segment), (0, 1));
    run(&mut slots, &layers, 5.5);
    let open = &slots.opened()[0];
    assert_eq!((open.layer, open.segment), (0, 2));
    let log = slots.shell().log.borrow();
    assert_eq!(log.opened, vec!["a"], "not opened again");
    assert!(log.seeks.len() >= 2, "it seeks to where the movie stands");
}

#[test]
fn a_shell_s_fall_back_starts_the_track_again_or_fails_it() {
    let layers = layers_with("a");
    let mut slots = MovieSlots::new(FakeShell {
        fall_backs: vec![Ok(true)],
        ..Default::default()
    });
    run(&mut slots, &layers, 0.0);
    // Opened, then sought again where it stands after the fall back.
    assert_eq!(slots.shell().log.borrow().seeks, vec![0.0, 0.0]);
    assert_eq!(slots.opened().len(), 1);
    slots.shell_mut().fall_backs = vec![Err("no module".into())];
    run(&mut slots, &layers, 1.0);
    assert!(slots.opened().is_empty(), "a failed movie closes");
    assert_eq!(slots.failed()[0].1, "no module");
}

#[test]
fn version_follows_what_shows() {
    let layers = layers_with("a");
    let mut slots = MovieSlots::new(FakeShell::default());
    run(&mut slots, &layers, 0.0);
    run(&mut slots, &layers, 1.0);
    let v = slots.version();
    // The same texture again, or a frame into a fresh one: no change.
    slots.set_texture("a", 7);
    slots.shown("a");
    assert_eq!(slots.version(), v);
    // Paused, the frame shown stays.
    slots.pause();
    assert!(slots.paused());
    assert_eq!(slots.version(), v);
    assert_eq!(slots.shown_ids().count(), 1);
    assert!(slots.update(&layers, 1.2).show.is_empty());
    slots.resume();
    // Another texture (the web's direct path) shows instead.
    slots.set_texture("a", 9);
    assert!(slots.version() > v);
    assert_eq!(slots.shown_ids().next(), Some((movie("a"), 9)));
    let v = slots.version();
    slots.turn_off(StopReason::DecoderBehind);
    assert!(slots.version() > v);
    assert_eq!(slots.shown_ids().count(), 0);
    assert_eq!(slots.off(), Some(StopReason::DecoderBehind));
    assert!(!slots.opening_plays(&layers[0]));
}

#[test]
fn a_restarting_change_shows_nothing_old() {
    let mut layers = vec![
        vec![
            seg(f64::NEG_INFINITY, BgImage::Song),
            seg(1.0, movie("a")),
            seg(2.0, BgImage::File("x.png".into())),
            seg(3.0, movie("a")),
        ],
        Vec::new(),
    ];
    layers[0][3].effect = MovieEffect::Rewind;
    let mut slots = MovieSlots::new(FakeShell::default());
    run(&mut slots, &layers, 1.0);
    assert_eq!(slots.shown_ids().count(), 1);
    let mut t = 1.0;
    while t < 2.5 {
        run(&mut slots, &layers, t);
        t += 0.05;
    }
    // Its last frame stays in its texture while the image shows.
    assert_eq!(slots.shown_ids().count(), 1);
    let v = slots.version();
    let due = slots.update(&layers, 3.0);
    assert!(slots.version() > v);
    assert_eq!(slots.shown_ids().count(), 0, "until its first frame");
    assert_eq!(due.show.len(), 1);
}

#[test]
fn backdrop_textures_are_made_again_only_on_a_change() {
    let mut cache = BackdropTextures::default();
    let images = vec![(BgImage::Song, 1)];
    let movies = || vec![(movie("a"), 2)];
    let list = cache.get(&images, 0, movies, 99).to_vec();
    assert_eq!(
        list,
        vec![(BgImage::Song, 1), (movie("a"), 2), (BgImage::Plain, 99)]
    );
    // Same key: the movies are not asked.
    let list = cache.get(
        &images,
        0,
        || -> Vec<(BgImage, usize)> { unreachable!() },
        99,
    );
    assert_eq!(list.len(), 3);
    // A new version or image: made again.
    assert_eq!(cache.get(&images, 1, Vec::new, 99).len(), 2);
    let images = vec![(BgImage::Song, 1), (BgImage::File("x".into()), 3)];
    assert_eq!(cache.get(&images, 1, Vec::new, 99).len(), 3);
    // The first call makes it even with nothing in it.
    let mut empty = BackdropTextures::default();
    assert_eq!(empty.get(&[], 0, Vec::new, 99), &[(BgImage::Plain, 99)]);
}
