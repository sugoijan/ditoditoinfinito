//! Generate the README's animation and still: `just screenshot`.
//!
//! Runs the desktop shell offscreen on a fixed clock, without a window,
//! a sound device, or the player's settings and history: the start menu,
//! song select and the start prompt as a player goes through them, into
//! the play, then the play's stretches the scenario names in beats, and
//! the results. Each crossfades into the next on a downbeat of both, so
//! that the music stays in step across the cut, and the end fades to
//! black, into an animated AVIF (no sound). The still is the frame just
//! after the
//! combo reaches the scenario's, at the size drawn.

use std::cell::Cell;
use std::collections::VecDeque;
use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_chart::Difficulty;
use ddi_desktop::audio;
use ddi_desktop::play::load;
use ddi_desktop::{gpu, shell, songs};
use ddi_platform::HostTime;
use ddi_platform::storage::{self, MemoryStore};
use ddi_ui::state::place::MenuPlace;
use ddi_ui::state::settings::Settings;
use serde::Deserialize;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    song: String,
    style: String,
    difficulty: Difficulty,
    /// Pixels drawn (the still's) and frames a second simulated; the
    /// animation's, scaled down from them, and its AV1 encoding.
    size: [u32; 2],
    fps: u32,
    animation: Animation,
    /// Where the screens' live art starts, seconds since the epoch.
    clock: f64,
    menu: Menu,
    /// The play's stretches in beats of its chart: the first from the
    /// menus on, up to its `to`; the others from their `from` to their
    /// `to`. Each crossfades into the next for `transition` beats, from
    /// its `to` and the next's `from`, so that both play in step; the
    /// last into the results.
    cuts: Vec<Cut>,
    transition: f64,
    /// The combo the still shows, taken this many seconds after the
    /// note that reaches it (in a cut).
    combo: u32,
    still_after: f64,
    /// Seconds of the results, and of the fade out at their end.
    results: f64,
    fade_out: f64,
    settings: Settings,
}

/// When the start menu's choice is focused and pressed (seconds from
/// the start), and how long song select and the prompt show before
/// their Enter.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Menu {
    focus: f64,
    open: f64,
    select: f64,
    start: f64,
}

/// The animation: its size and frames a second (a divisor of the
/// simulation's), and SVT-AV1's constant rate factor (lower: closer to
/// the frames, larger) and preset (lower: slower, smaller).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Animation {
    size: [u32; 2],
    fps: u32,
    crf: u32,
    preset: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cut {
    #[serde(default)]
    from: Option<f64>,
    to: f64,
}

const START: u64 = 1_000_000_000;

fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() > 3 {
        return Err("usage: screenshot [scenario.json] [animation.avif] [still.png]".into());
    }
    let path =
        |i: usize, default: &str| root.join(args.get(i).map_or(default.into(), PathBuf::from));
    let scenario: Scenario = serde_json::from_str(&std::fs::read_to_string(path(
        0,
        "docs/screenshots/readme.json",
    ))?)?;
    let output = path(1, "docs/screenshots/readme.avif");
    let still = path(2, "docs/screenshots/readme.png");
    if output.extension().is_none_or(|e| e != "avif")
        || still.extension().is_none_or(|e| e != "png")
    {
        return Err("the outputs are an .avif animation and a .png still".into());
    }
    if scenario.fade_out > 4.0 || scenario.results < scenario.fade_out {
        return Err("the fade out lasts 4 s at most, the results longer".into());
    }
    let sides = |s: [u32; 2]| s.iter().all(|v| (64..=4096).contains(v) && v % 2 == 0);
    if !(24..=120).contains(&scenario.fps)
        || scenario.animation.fps == 0
        || !scenario.fps.is_multiple_of(scenario.animation.fps)
        || !sides(scenario.size)
        || !sides(scenario.animation.size)
    {
        return Err(
            "fps runs 24 to 120, the animation's divides it, and the sides are even, 64 to 4096"
                .into(),
        );
    }
    let ordered = scenario.cuts.iter().enumerate().all(|(i, c)| {
        (i == 0) == c.from.is_none()
            && c.from.is_none_or(|f| f < c.to)
            && (i == 0 || c.from > Some(scenario.cuts[i - 1].to + scenario.transition))
    });
    if scenario.cuts.is_empty() || !ordered || !(0.25..=8.0).contains(&scenario.transition) {
        return Err(
            "cuts go forward, a `from` on every one but the first, each \
             starting past the last one's crossfade (of 0.25 to 8 beats)"
                .into(),
        );
    }
    if !ddi_desktop::library::bundled()
        .iter()
        .any(|s| s.id == scenario.song)
    {
        return Err(
            "scenario song must be bundled; run `just screenshot` to generate the indexes".into(),
        );
    }
    let loaded = load::load(&scenario.song, 48_000)?;
    let chart = loaded
        .song
        .charts
        .iter()
        .position(|c| c.layout == scenario.style && c.difficulty == scenario.difficulty)
        .ok_or("scenario chart is not in the song")?;
    let timing = loaded.song.charts[chart].timing(&loaded.song).clone();
    let settings = Settings {
        style: scenario.style.clone(),
        ..scenario.settings.clone()
    };

    // Nothing of the player's: settings, the list's place, an autoplay.
    storage::install(Rc::new(MemoryStore::default()));
    settings.save();
    MenuPlace {
        known: true,
        open: Some(ddi_ui::state::place::pack_key(None)),
        chart: Some((scenario.song.clone(), chart)),
        song: Some(scenario.song.clone()),
        style: Some(scenario.style.clone()),
        closed: Some(Vec::new()),
        ..Default::default()
    }
    .save();
    // Every sound on one fake clock, heard by nothing.
    let clock = Arc::new(AtomicU64::new(START));
    let c = clock.clone();
    let host: audio::HostClock = Arc::new(move || c.load(Ordering::SeqCst));
    audio::set_headless(true);
    audio::set_headless_clock(Some(host.clone()));
    shell::hide_quit();
    let library = songs::Songs::new(Vec::new());
    library.skip_offer();
    library.autoplay.set(true);
    library.start(Arc::new(|| {}));
    // Previews at the settings' volume, as the game plays them.
    let mut shell = shell::Shell::named_with("start", None, Some(library));
    shell.play.test_clock = Some(host);
    let instance = gpu::instance();
    let adapter = gpu::adapter(&instance, None).ok_or("a GPU adapter is needed")?;
    let (device, queue) = gpu::device(&adapter)?;
    shell.offscreen(device, queue, scenario.size);

    let fps = f64::from(scenario.fps);
    let mut reel = Reel::new(&output, &scenario)?;
    let mut frame: u64 = 0;
    // Nanoseconds a frame between cuts is stretched by, so that the next
    // cut starts on its beat exactly (none of it is recorded).
    let skew = Cell::new(0_i64);
    // One frame on the clock: the shell polled until nothing it shows
    // loads (the clock held meanwhile), and drawn.
    let mut step = |shell: &mut shell::Shell, read: bool| -> Result<Option<Vec<u8>>> {
        let mut waited = 0;
        loop {
            shell.offscreen_poll();
            if !shell.offscreen_busy() {
                break;
            }
            // The last frame again, as a window draws while things load
            // (what is read on threads is taken in by the frames).
            std::thread::sleep(std::time::Duration::from_millis(5));
            let nanos = clock.load(Ordering::SeqCst);
            let t = (nanos - START) as f64 / 1e9;
            shell.offscreen_frame(scenario.clock + t, HostTime(nanos as f64 / 1e9), 0.0, false);
            waited += 1;
            if waited > 6_000 {
                return Err("something never finished loading".into());
            }
        }
        frame += 1;
        let nanos = START + frame * 1_000_000_000 / u64::from(scenario.fps);
        let nanos = nanos.saturating_add_signed(skew.get());
        clock.store(nanos, Ordering::SeqCst);
        let t = (nanos - START) as f64 / 1e9;
        let pixels = shell.offscreen_frame(
            scenario.clock + t,
            HostTime(nanos as f64 / 1e9),
            1.0 / fps as f32,
            read,
        );
        Ok(pixels)
    };
    let now = || HostTime(clock.load(Ordering::SeqCst) as f64 / 1e9);
    // The play's second and beat at a frame, and its combo.
    let heard = |shell: &shell::Shell, at: HostTime| {
        shell
            .play
            .running()
            .map(|p| p.session.player.clock().heard_now(at))
    };
    let beat = |shell: &shell::Shell| heard(shell, now()).map(|s| timing.beat_at(s));
    let combo = |shell: &shell::Shell| shell.play.running().map_or(0, |p| p.session.player.combo());
    let screen = |shell: &shell::Shell| {
        shell
            .theme
            .as_ref()
            .map(|(env, _)| env.state.borrow().screen.clone())
            .unwrap_or_default()
    };
    // Frames from one beat to another.
    let span = |from: f64, to: f64| {
        ((timing.seconds_at_beat(to) - timing.seconds_at_beat(from)) * fps).round() as usize
    };
    let combo_at = Cell::new(None);
    let kept = Cell::new(false);
    // A frame recorded, the still taken from it when it is the one.
    let keep = |shell: &shell::Shell, pixels: Vec<u8>, reel: &mut Reel| -> Result<()> {
        if let Some(at) = heard(shell, now()) {
            if combo_at.get().is_none() && combo(shell) >= scenario.combo {
                combo_at.set(Some(at));
            }
            if !kept.get()
                && combo_at
                    .get()
                    .is_some_and(|c| at >= c + scenario.still_after)
            {
                kept.set(true);
                if combo(shell) != scenario.combo {
                    return Err(format!(
                        "the still shows a combo of {}, not {}: take it sooner",
                        combo(shell),
                        scenario.combo
                    )
                    .into());
                }
                image::save_buffer_with_format(
                    &still,
                    &pixels,
                    scenario.size[0],
                    scenario.size[1],
                    image::ColorType::Rgba8,
                    image::ImageFormat::Png,
                )?;
                println!("{}: combo {}", still.display(), combo(shell));
            }
        }
        reel.push(pixels)
    };

    // The menus, then the play up to the first cut's end: the start
    // menu's choice focused and pressed, song select and the prompt
    // shown a moment each.
    let mut n = 0;
    let mut pressed = [false; 4];
    let mut shown: Option<(String, f64)> = None;
    let mut fade_from;
    let mut fade_beat;
    loop {
        let pixels = step(&mut shell, true)?;
        n += 1;
        keep(&shell, pixels.unwrap_or_default(), &mut reel)?;
        let t = n as f64 / fps;
        let on = screen(&shell);
        if shown.as_ref().is_none_or(|(s, _)| *s != on) {
            shown = Some((on.clone(), t));
        }
        let since = shown.as_ref().map_or(0.0, |(_, at)| t - at);
        let due = match on.as_str() {
            "start" if !pressed[0] => (t >= scenario.menu.focus).then_some(0),
            "start" if !pressed[1] => (t >= scenario.menu.open).then_some(1),
            "select" if !pressed[2] => (since >= scenario.menu.select).then_some(2),
            "prompt" if !pressed[3] => (since >= scenario.menu.start).then_some(3),
            _ => None,
        };
        if let Some(i) = due {
            pressed[i] = true;
            shell.offscreen_key("Enter", "Enter");
        }
        if let Some(b) = beat(&shell).filter(|b| *b >= scenario.cuts[0].to) {
            fade_beat = b;
            fade_from = n;
            break;
        }
        if t > 60.0 {
            return Err(format!("the play never reached its first cut's end (on {on})").into());
        }
    }
    let mut to = scenario.cuts[0].to;
    for next in scenario.cuts.iter().skip(1).map(Some).chain([None]) {
        // The crossfade's frames of the cut ending.
        let fading = span(to, to + scenario.transition).max(1);
        for _ in 1..fading {
            let pixels = step(&mut shell, true)?;
            n += 1;
            keep(&shell, pixels.unwrap_or_default(), &mut reel)?;
        }
        reel.cut(fading)?;
        let Some(next) = next else { break };
        // The next starts as many frames on as its beat is after this
        // one's crossfade, so both are in step (within half a frame).
        let from = next.from.unwrap_or(to);
        let first = fade_from + span(to, from);
        let off = (timing.seconds_at_beat(from) - timing.seconds_at_beat(to))
            - span(to, from) as f64 / fps;
        if n + 1 >= first {
            return Err(format!("beat {from} comes before the cut to it ends").into());
        }
        skew.set(skew.get() + (off * 1e9).round() as i64);
        while n + 1 < first {
            step(&mut shell, false)?;
            n += 1;
            if !shell.play.playing() {
                return Err(format!("the play ended before beat {from}").into());
            }
        }
        // Where each side of the crossfade is in its bar: the same.
        let pixels = step(&mut shell, true)?;
        n += 1;
        println!(
            "cut from beat {to} to beat {from}: {:.4} and {:.4} beat past them",
            fade_beat - to,
            beat(&shell).unwrap_or(f64::NAN) - from
        );
        keep(&shell, pixels.unwrap_or_default(), &mut reel)?;
        loop {
            let pixels = step(&mut shell, true)?;
            n += 1;
            keep(&shell, pixels.unwrap_or_default(), &mut reel)?;
            if let Some(b) = beat(&shell).filter(|b| *b >= next.to) {
                fade_beat = b;
                fade_from = n;
                break;
            }
            if !shell.play.playing() {
                return Err(format!("the play ended before beat {}", next.to).into());
            }
        }
        to = next.to;
    }
    if !kept.get() {
        return Err(format!("no cut shows the combo reaching {}", scenario.combo).into());
    }
    // Up to the results, unrecorded; then they come in.
    while screen(&shell) != "results" {
        step(&mut shell, false)?;
        n += 1;
        if n as f64 / fps > 600.0 {
            return Err("the play never ended".into());
        }
    }
    for _ in 0..(scenario.results * fps).round() as usize {
        let pixels = step(&mut shell, true)?;
        keep(&shell, pixels.unwrap_or_default(), &mut reel)?;
    }
    let length = reel.finish((scenario.fade_out * fps).round() as usize)?;
    println!(
        "{}: {length:.2} s at {}x{} (drawn at {}x{}), {} fps",
        output.display(),
        scenario.animation.size[0],
        scenario.animation.size[1],
        scenario.size[0],
        scenario.size[1],
        scenario.animation.fps
    );
    Ok(())
}

/// The animation as it is made: frames go to the encoder a crossfade's
/// length late, so that a cut's last frames can be blended into the next
/// one's first.
struct Reel {
    encoder: Child,
    size: [u32; 2],
    fps: u32,
    /// The crossfade into this cut, frames.
    transition: usize,
    /// Frames not yet encoded, the latest last.
    tail: VecDeque<Vec<u8>>,
    /// The last cut's last frames, blended into this one's first.
    leaving: VecDeque<Vec<u8>>,
    blended: usize,
    encoded: usize,
}

impl Reel {
    fn new(output: &Path, scenario: &Scenario) -> Result<Reel> {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let [w, h] = scenario.size;
        let a = &scenario.animation;
        let [aw, ah] = a.size;
        // Every frame simulated in, the animation's taken from them,
        // scaled down (Lanczos), as AV1 in yuv420p, BT.709 as the frames
        // are sRGB, looping.
        let encoder = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "rawvideo", "-pixel_format", "rgba", "-video_size"])
            .arg(format!("{w}x{h}"))
            .arg("-framerate")
            .arg(scenario.fps.to_string())
            .args(["-i", "pipe:0", "-vf"])
            .arg(format!(
                "fps={},scale={aw}:{ah}:flags=lanczos:out_color_matrix=bt709:out_range=tv,format=yuv420p",
                a.fps
            ))
            .args(["-c:v", "libsvtav1", "-crf"])
            .arg(a.crf.to_string())
            .arg("-preset")
            .arg(a.preset.to_string())
            .args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709"])
            .args(["-color_range", "tv", "-loop", "0", "-f", "avif"])
            .arg(output)
            // SVT-AV1's banner and settings, its errors kept.
            .env("SVT_LOG", "1")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| format!("the animation needs ffmpeg on PATH: {e}"))?;
        Ok(Reel {
            encoder,
            size: scenario.size,
            fps: scenario.fps,
            transition: 0,
            tail: VecDeque::new(),
            leaving: VecDeque::new(),
            blended: 0,
            encoded: 0,
        })
    }

    fn push(&mut self, mut pixels: Vec<u8>) -> Result<()> {
        if pixels.len() != (self.size[0] * self.size[1] * 4) as usize {
            return Err("a frame came back empty".into());
        }
        if let Some(old) = self.leaving.pop_front() {
            self.blended += 1;
            let t = self.blended as f32 / (self.transition + 1) as f32;
            for (p, o) in pixels.iter_mut().zip(&old) {
                *p = (f32::from(*o) * (1.0 - t) + f32::from(*p) * t).round() as u8;
            }
        }
        self.tail.push_back(pixels);
        // Room for a crossfade's or the fade out's frames at the end.
        while self.tail.len() > self.fps as usize * 4 {
            let p = self.tail.pop_front().unwrap_or_default();
            self.encode(&p)?;
        }
        Ok(())
    }

    /// The cut ends: its last `frames` are blended into the next one's
    /// first.
    fn cut(&mut self, frames: usize) -> Result<()> {
        self.transition = frames.min(self.fps as usize * 4);
        while self.tail.len() > self.transition {
            let p = self.tail.pop_front().unwrap_or_default();
            self.encode(&p)?;
        }
        self.leaving = std::mem::take(&mut self.tail);
        self.blended = 0;
        Ok(())
    }

    fn encode(&mut self, pixels: &[u8]) -> Result<()> {
        self.encoder
            .stdin
            .as_mut()
            .ok_or("the encoder's input closed")?
            .write_all(pixels)?;
        self.encoded += 1;
        Ok(())
    }

    /// The last frames fade to black over `fade` frames. The animation's
    /// length in seconds.
    fn finish(mut self, fade: usize) -> Result<f64> {
        let n = self.tail.len();
        let tail = std::mem::take(&mut self.tail);
        for (k, mut p) in tail.into_iter().enumerate() {
            let left = n - k;
            if left <= fade {
                let g = (left - 1) as f32 / fade as f32;
                for (i, v) in p.iter_mut().enumerate() {
                    if i % 4 != 3 {
                        *v = (f32::from(*v) * g).round() as u8;
                    }
                }
            }
            self.encode(&p)?;
        }
        drop(self.encoder.stdin.take());
        if !self.encoder.wait()?.success() {
            return Err("ffmpeg could not encode the animation".into());
        }
        Ok(self.encoded as f64 / f64::from(self.fps))
    }
}
