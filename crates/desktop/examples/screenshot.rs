//! Generate the README's animation (and its still): `just screenshot`.
//!
//! Runs the desktop shell offscreen on a fixed clock, without a window,
//! a sound device, or the player's settings and history, pressing keys
//! as a player would, through the shots the scenario names: each a
//! stretch of the screens or of a play, recorded from where it starts to
//! where it ends, the frames between shots drawn and not recorded. Each
//! shot crossfades into the next (two stretches of the same play on a
//! downbeat of both, so that the music stays in step across the cut),
//! and the end fades to black, into an animated AVIF (no sound). The
//! player's presses are autoplayed, or a seat's log's (a person's kept
//! play, judged by the play as its own); a multiplayer stage is CPU
//! practice's, its CPUs seeded. Where the scenario asks for a still, it
//! is the frame just after the combo reaches the scenario's, at the size
//! drawn. `docs/screenshots/README.md` says what each scenario records.

use std::collections::VecDeque;
use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_chart::{ChartDigest, Difficulty};
use ddi_desktop::audio;
use ddi_desktop::play::load;
use ddi_desktop::{gpu, shell, songs};
use ddi_engine::history::{LoggedInput, PlayLog};
use ddi_platform::HostTime;
use ddi_platform::storage::{self, MemoryStore};
use ddi_ui::state::place::MenuPlace;
use ddi_ui::state::settings::Settings;
use serde::Deserialize;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    /// The chart song select opens on (the list's place), which the
    /// shots play.
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
    /// The player's presses: a seat's log (a file beside the scenario,
    /// [`SeatFile`]) on its chart, every play of it taking them as its
    /// own; without one, the plays are autoplayed.
    #[serde(default)]
    seat: Option<PathBuf>,
    /// CPU practice's room, where a shot goes there: its CPUs drawn from
    /// this seed, so the same stage in every run.
    #[serde(default)]
    practice: Option<Practice>,
    /// Beats a crossfade from a shot ending on a beat lasts, and seconds
    /// one from a shot ending otherwise.
    transition: f64,
    #[serde(default = "default_fade")]
    fade: f64,
    shots: Vec<Shot>,
    /// The still, where the scenario has one.
    #[serde(default)]
    still: Option<Still>,
    /// Seconds of the fade out at the end.
    fade_out: f64,
    /// The settings at the start (shots may change some).
    settings: Settings,
}

fn default_fade() -> f64 {
    0.5
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
struct Practice {
    seed: u64,
    /// The match's rules changed from the defaults a new practice room
    /// has (`ddi_room::MatchRules`' fields, as the Rules pop-up would
    /// save them; its roster is the settings' `practice_roster`, the
    /// one a player's last practice room was left with).
    #[serde(default)]
    rules: serde_json::Map<String, serde_json::Value>,
}

/// The combo the still shows, taken this many seconds after the note
/// that reaches it (in a shot of a play).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Still {
    combo: u32,
    after: f64,
}

/// One stretch recorded: from `from` (at once where it has none) to
/// `to`, its keys pressed in order as they come due, before and during
/// it; `settings` changed first (stored, as Options would).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Shot {
    #[serde(default)]
    settings: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    keys: Vec<Key>,
    #[serde(default)]
    from: Option<Mark>,
    to: Mark,
}

/// A key pressed and let go once screen `on` (any where none) has shown
/// `after` seconds; one key a frame. `play` is a play's screen.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Key {
    #[serde(default)]
    on: Option<String>,
    #[serde(default)]
    after: f64,
    key: String,
    #[serde(default)]
    code: Option<String>,
}

/// Where a shot starts or ends: a beat of the play (a shot from a beat
/// after one to a beat cuts in step), a screen's first frame, or (an
/// end) seconds recorded, or seconds after the shot's last key.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct Mark {
    #[serde(default)]
    beat: Option<f64>,
    #[serde(default)]
    screen: Option<String>,
    #[serde(default)]
    seconds: Option<f64>,
    #[serde(default)]
    after_keys: Option<f64>,
}

impl Mark {
    fn kinds(&self) -> usize {
        usize::from(self.beat.is_some())
            + usize::from(self.screen.is_some())
            + usize::from(self.seconds.is_some())
            + usize::from(self.after_keys.is_some())
    }

    /// Only a shot's end.
    fn ends_only(&self) -> bool {
        self.seconds.is_some() || self.after_keys.is_some()
    }
}

/// A seat's log, as committed: the presses and releases of a play, by
/// lane, at the judged song second, and the chart they were played on
/// (its content's digest and layout). Nothing else.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeatFile {
    format: String,
    song: String,
    chart: ChartDigest,
    layout: String,
    /// `[second, lane, pressed]`, pressed 1 and released 0, in order.
    inputs: Vec<(f64, u8, u8)>,
}

const SEAT_FORMAT: &str = "ddi-seat-v1";
const START: u64 = 1_000_000_000;

fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() > 3 {
        return Err("usage: screenshot [scenario.json] [animation.avif] [still.png]".into());
    }
    let path =
        |i: usize, default: &str| root.join(args.get(i).map_or(default.into(), PathBuf::from));
    let scenario_path = path(0, "docs/screenshots/readme.json");
    let scenario: Scenario = serde_json::from_str(&std::fs::read_to_string(&scenario_path)?)?;
    let output = path(1, "docs/screenshots/readme.avif");
    let still = path(2, "docs/screenshots/readme.png");
    if output.extension().is_none_or(|e| e != "avif")
        || still.extension().is_none_or(|e| e != "png")
    {
        return Err("the outputs are an .avif animation and a .png still".into());
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
    if scenario.fade_out > 4.0
        || !(0.25..=8.0).contains(&scenario.transition)
        || !(0.1..=4.0).contains(&scenario.fade)
    {
        return Err("the fade out lasts 4 s at most, a crossfade 0.25 to 8 beats or 0.1 to 4 s".into());
    }
    check_shots(&scenario)?;
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
    // The seat: its chart must be the scenario's, as played.
    let seat = match &scenario.seat {
        Some(file) => {
            let at = scenario_path.parent().unwrap_or(&root).join(file);
            let seat: SeatFile = serde_json::from_str(&std::fs::read_to_string(&at)?)?;
            if seat.format != SEAT_FORMAT
                || seat.song != scenario.song
                || seat.layout != scenario.style
                || seat.chart != loaded.song.chart_digest(chart)
            {
                return Err(format!(
                    "{}: a {SEAT_FORMAT} log of the scenario's chart is needed",
                    at.display()
                )
                .into());
            }
            let lanes = loaded
                .song
                .layout_of(&loaded.song.charts[chart])
                .map_or(0, |l| l.lanes.len());
            let ordered = seat.inputs.windows(2).all(|w| w[0].0 <= w[1].0);
            if !ordered
                || seat
                    .inputs
                    .iter()
                    .any(|&(t, lane, p)| !t.is_finite() || usize::from(lane) >= lanes || p > 1)
            {
                return Err(format!("{}: its inputs are out of order or range", at.display()).into());
            }
            let log = PlayLog {
                start: 0.0,
                inputs: seat
                    .inputs
                    .iter()
                    .map(|&(time, lane, pressed)| LoggedInput {
                        time,
                        lane,
                        pressed: pressed == 1,
                    })
                    .collect(),
                ..PlayLog::default()
            };
            Some((seat.chart, Rc::new(log)))
        }
        None => None,
    };
    let mut settings = Settings {
        style: scenario.style.clone(),
        ..scenario.settings.clone()
    };

    // Nothing of the player's: settings, the list's place, a play's
    // presses autoplayed or a seat's.
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
    // CPU practice's room seeded, made on the run's clock.
    if let Some(p) = &scenario.practice {
        ddi_desktop::room::seed_for_tests(p.seed);
        let (c, epoch) = (clock.clone(), scenario.clock);
        ddi_desktop::room::clock_for_tests(Rc::new(move || {
            let t = (c.load(Ordering::SeqCst) - START) as f64 / 1e9;
            ((epoch + t) * 1000.0) as u64
        }));
        if !p.rules.is_empty() {
            let room = ddi_desktop::room::current();
            let mut room = room.borrow_mut();
            let mut value = serde_json::to_value(room.rules())?;
            if let Some(map) = value.as_object_mut() {
                map.extend(p.rules.clone());
            }
            room.save_rules(serde_json::from_value(value)?)
                .map_err(|e| format!("the practice room refused its rules: {e:?}"))?;
        }
    }
    let library = songs::Songs::new(Vec::new());
    library.skip_offer();
    library.autoplay.set(seat.is_none());
    library.start(Arc::new(|| {}));
    // Previews at the settings' volume, as the game plays them; the start
    // menu without its Quit, as the web's.
    let mut shell = shell::Shell::named_with("start", None, Some(library), false);
    shell.play.test_clock = Some(host);
    shell.play.seat = seat;
    let instance = gpu::instance();
    let adapter = gpu::adapter(&instance, None).ok_or("a GPU adapter is needed")?;
    let (device, queue) = gpu::device(&adapter)?;
    shell.offscreen(device, queue, scenario.size);

    let mut run = Run {
        fps: f64::from(scenario.fps),
        clock,
        skew: 0,
        n: 0,
        reel: Reel::new(&output, &scenario)?,
        timing,
        still: scenario.still.as_ref().map(|s| (s, still.clone())),
        combo_at: None,
        kept: false,
        size: scenario.size,
        epoch: scenario.clock,
        shown: None,
        pressed: 0.0,
        battle: Watched::default(),
    };
    // Where the last shot ended in a play: its beat, the beat its last
    // frame was at and that frame's number (a shot from a later beat of
    // the same play cuts in step).
    let mut ended: Option<(f64, f64, u64)> = None;
    for (k, shot) in scenario.shots.iter().enumerate() {
        if !shot.settings.is_empty() {
            let mut value = serde_json::to_value(&settings)?;
            if let Some(map) = value.as_object_mut() {
                map.extend(shot.settings.clone());
            }
            settings = serde_json::from_value(value)?;
            settings.save();
        }
        let mut keys: VecDeque<&Key> = shot.keys.iter().collect();
        // To the shot's start, unrecorded.
        match (&shot.from, ended) {
            (Some(Mark { beat: Some(from), .. }), Some((to, _, fade_from))) => {
                // In step: as many frames on as its beat is after the last
                // shot's end, the frame before stretched by the difference
                // (within half a frame).
                let first = fade_from + run.span(to, *from) as u64;
                let off = (run.timing.seconds_at_beat(*from) - run.timing.seconds_at_beat(to))
                    - run.span(to, *from) as f64 / run.fps;
                if run.n + 1 >= first {
                    return Err(format!("beat {from} comes before the cut to it ends").into());
                }
                run.skew += (off * 1e9).round() as i64;
                while run.n + 1 < first {
                    run.tick(&mut shell, false, &mut keys)?;
                    if !shell.play.playing() {
                        return Err(format!("the play ended before beat {from}").into());
                    }
                }
            }
            (Some(from), _) => {
                let begun = run.n;
                while !run.reached(&shell, from, begun, keys.len()) {
                    run.tick(&mut shell, false, &mut keys)?;
                    if (run.n - begun) as f64 / run.fps > 600.0 {
                        return Err(format!("shot {k} never reached its start {from:?}").into());
                    }
                }
            }
            (None, _) => {}
        }
        // Recorded to its end.
        let begun = run.n;
        let first = run.n + 1;
        loop {
            run.tick(&mut shell, true, &mut keys)?;
            if run.n == first
                && let (Some(Mark { beat: Some(from), .. }), Some((to, beat, _))) =
                    (&shot.from, ended)
            {
                // Where each side of the crossfade is in its bar: the same.
                println!(
                    "cut from beat {to} to beat {from}: {:.4} and {:.4} beat past them",
                    beat - to,
                    run.beat(&shell).unwrap_or(f64::NAN) - from
                );
            }
            if run.reached(&shell, &shot.to, begun, keys.len()) {
                break;
            }
            if (run.n - begun) as f64 / run.fps > 600.0 {
                return Err(format!("shot {k} never reached its end {:?}", shot.to).into());
            }
        }
        if !keys.is_empty() {
            return Err(format!("shot {k} ended before its key {}", keys[0].key).into());
        }
        ended = shot
            .to
            .beat
            .map(|to| (to, run.beat(&shell).unwrap_or(f64::NAN), run.n));
        println!(
            "shot {k}: {:.2} s recorded{}",
            (run.n - begun) as f64 / run.fps,
            ended.map_or(String::new(), |(to, ..)| format!(", to beat {to}"))
        );
        if k + 1 == scenario.shots.len() {
            break;
        }
        // The crossfade's frames of the shot ending: a downbeat's
        // `transition` beats in a play, else `fade` seconds.
        let fading = match ended {
            Some((to, ..)) => run.span(to, to + scenario.transition),
            None => (scenario.fade * run.fps).round() as usize,
        }
        .max(1);
        let mut none = VecDeque::new();
        for _ in 1..fading {
            run.tick(&mut shell, true, &mut none)?;
        }
        run.reel.cut(fading)?;
    }
    if scenario.still.is_some() && !run.kept {
        return Err("no shot shows the still's combo".into());
    }
    let fade = (scenario.fade_out * run.fps).round() as usize;
    let length = run.reel.finish(fade)?;
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

/// The shots are well formed: each mark one thing, beats going forward
/// where a shot cuts in step, seconds only at an end.
fn check_shots(scenario: &Scenario) -> Result<()> {
    if scenario.shots.is_empty() {
        return Err("a scenario has shots".into());
    }
    let mut last_to: Option<f64> = None;
    for (k, shot) in scenario.shots.iter().enumerate() {
        let from_ok = shot
            .from
            .as_ref()
            .is_none_or(|m| m.kinds() == 1 && !m.ends_only());
        if !from_ok || shot.to.kinds() != 1 {
            return Err(format!(
                "shot {k}: a mark is one of a beat, a screen or (an end) seconds or seconds after its keys"
            )
            .into());
        }
        if let (Some(from), Some(to)) = (shot.from.as_ref().and_then(|m| m.beat), last_to)
            && from <= to + scenario.transition
        {
            return Err(format!(
                "shot {k} starts at beat {from}, within the crossfade from beat {to}"
            )
            .into());
        }
        if let (Some(from), Some(to)) = (shot.from.as_ref().and_then(|m| m.beat), shot.to.beat)
            && to <= from
        {
            return Err(format!("shot {k} ends before it starts").into());
        }
        last_to = shot.to.beat;
    }
    Ok(())
}

/// The run: the shell's frames on the fixed clock, what is recorded, and
/// what is pressed.
struct Run<'a> {
    fps: f64,
    clock: Arc<AtomicU64>,
    /// Nanoseconds the clock is ahead of the frames' count: the frames
    /// stretched between shots so that the next starts on its beat
    /// exactly (none of them recorded).
    skew: i64,
    /// Frames drawn.
    n: u64,
    reel: Reel,
    timing: ddi_chart::TimingMap,
    still: Option<(&'a Still, PathBuf)>,
    combo_at: Option<f64>,
    kept: bool,
    size: [u32; 2],
    /// The screens' live art's clock at the start (seconds since the
    /// epoch).
    epoch: f64,
    /// The screen shown and since when (seconds).
    shown: Option<(String, f64)>,
    /// When the last key was pressed (seconds).
    pressed: f64,
    battle: Watched,
}

impl Run<'_> {
    fn now(&self) -> HostTime {
        HostTime(self.clock.load(Ordering::SeqCst) as f64 / 1e9)
    }

    /// The play's second at the clock's time.
    fn heard(&self, shell: &shell::Shell) -> Option<f64> {
        shell
            .play
            .running()
            .map(|p| p.session.player.clock().heard_now(self.now()))
    }

    fn beat(&self, shell: &shell::Shell) -> Option<f64> {
        self.heard(shell).map(|s| self.timing.beat_at(s))
    }

    fn combo(shell: &shell::Shell) -> u32 {
        shell.play.running().map_or(0, |p| p.session.player.combo())
    }

    /// The screen shown: `play` while a play runs, else the theme's top
    /// screen's name.
    fn screen(shell: &shell::Shell) -> String {
        if shell.play.playing() {
            return "play".into();
        }
        shell
            .theme
            .as_ref()
            .map(|(env, _)| env.state.borrow().screen.clone())
            .unwrap_or_default()
    }

    /// Frames from one beat to another.
    fn span(&self, from: f64, to: f64) -> usize {
        ((self.timing.seconds_at_beat(to) - self.timing.seconds_at_beat(from)) * self.fps).round()
            as usize
    }

    /// Whether `mark` is reached, `begun` the frame its shot's stretch
    /// began after.
    fn reached(&self, shell: &shell::Shell, mark: &Mark, begun: u64, keys: usize) -> bool {
        if let Some(after) = mark.after_keys {
            return keys == 0 && (self.n as f64 / self.fps) - self.pressed >= after;
        }
        if let Some(b) = mark.beat {
            return self.beat(shell).is_some_and(|at| at >= b);
        }
        if let Some(s) = &mark.screen {
            return Self::screen(shell) == *s;
        }
        mark.seconds
            .is_some_and(|s| (self.n - begun) as usize >= (s * self.fps).round() as usize)
    }

    /// One frame on the clock (the shell polled until nothing it shows
    /// loads, the clock held meanwhile, then drawn), recorded or not;
    /// then the next of `keys` pressed if it is due.
    fn tick(
        &mut self,
        shell: &mut shell::Shell,
        record: bool,
        keys: &mut VecDeque<&Key>,
    ) -> Result<()> {
        let mut waited = 0;
        loop {
            shell.offscreen_poll();
            if !shell.offscreen_busy() {
                break;
            }
            // The last frame again, as a window draws while things load
            // (what is read on threads is taken in by the frames).
            std::thread::sleep(std::time::Duration::from_millis(5));
            let nanos = self.clock.load(Ordering::SeqCst);
            let t = (nanos - START) as f64 / 1e9;
            shell.offscreen_frame(self.epoch + t, HostTime(nanos as f64 / 1e9), 0.0, false);
            waited += 1;
            if waited > 6_000 {
                return Err("something never finished loading".into());
            }
        }
        self.n += 1;
        let nanos = START + self.n * 1_000_000_000 / self.fps as u64;
        let nanos = nanos.saturating_add_signed(self.skew);
        self.clock.store(nanos, Ordering::SeqCst);
        let t = (nanos - START) as f64 / 1e9;
        let pixels = shell.offscreen_frame(
            self.epoch + t,
            HostTime(nanos as f64 / 1e9),
            1.0 / self.fps as f32,
            record,
        );
        if record {
            self.keep(shell, pixels.unwrap_or_default())?;
        }
        let mut battle = std::mem::take(&mut self.battle);
        battle.watch(shell, self);
        self.battle = battle;
        // The screen shown, and since when (the first one since the
        // start).
        let t = self.n as f64 / self.fps;
        let on = Self::screen(shell);
        if self.shown.as_ref().is_none_or(|(s, _)| *s != on) {
            let since = if self.shown.is_none() { 0.0 } else { t };
            self.shown = Some((on.clone(), since));
        }
        let since = self.shown.as_ref().map_or(0.0, |(_, at)| t - at);
        if let Some(key) = keys.front()
            && key.on.as_ref().is_none_or(|s| *s == on)
            && since >= key.after
        {
            let code = key.code.as_deref().unwrap_or(&key.key);
            println!(
                "{t:.2} s: {} on {on} (focused {:?})",
                key.key,
                shell.ui.focused().unwrap_or("-")
            );
            shell.offscreen_key(code, &key.key);
            keys.pop_front();
            self.pressed = t;
        }
        Ok(())
    }

    /// A frame recorded, the still taken from it when it is the one.
    fn keep(&mut self, shell: &shell::Shell, pixels: Vec<u8>) -> Result<()> {
        if let Some((still, path)) = &self.still
            && let Some(at) = self.heard(shell)
        {
            let combo = Self::combo(shell);
            if self.combo_at.is_none() && combo >= still.combo {
                self.combo_at = Some(at);
            }
            if !self.kept && self.combo_at.is_some_and(|c| at >= c + still.after) {
                self.kept = true;
                if combo != still.combo {
                    return Err(format!(
                        "the still shows a combo of {combo}, not {}: take it sooner",
                        still.combo
                    )
                    .into());
                }
                image::save_buffer_with_format(
                    path,
                    &pixels,
                    self.size[0],
                    self.size[1],
                    image::ColorType::Rgba8,
                    image::ImageFormat::Png,
                )?;
                println!("{}: combo {combo}", path.display());
            }
        }
        self.reel.push(pixels)
    }
}

/// What a stage's battle did, printed as it happens (to choose its
/// shots): attacks sent and received, shocks stepped on, opponents out.
#[derive(Default)]
struct Watched {
    links: Vec<u32>,
    out: Vec<bool>,
    failed: Vec<bool>,
    hazards: usize,
    mines: u32,
    hits: u32,
    told: f64,
}

impl Watched {
    fn watch(&mut self, shell: &shell::Shell, run: &Run) {
        let Some(p) = shell.play.running() else {
            if !self.out.is_empty() || self.hazards > 0 {
                *self = Watched::default();
            }
            return;
        };
        let Some(b) = p.session.battle() else {
            return;
        };
        let player = &p.session.player;
        let beat = run.beat(shell).unwrap_or(f64::NAN);
        for l in b.links(player) {
            if !self.links.contains(&l.attack) {
                self.links.push(l.attack);
                let views = b.views();
                let who = views.get(l.seat).map_or("?".into(), |v| v.name.to_string());
                println!(
                    "beat {beat:.1}: attack {} {} {who}, shocks from beat {:.1} to {:.1}",
                    l.attack,
                    if l.outgoing { "sent to" } else { "from" },
                    run.timing.beat_at(l.first),
                    run.timing.beat_at(l.clear)
                );
            }
        }
        let views = b.views();
        self.out.resize(views.len(), false);
        self.failed.resize(views.len(), false);
        for (i, v) in views.iter().enumerate() {
            if v.out && !self.out[i] {
                self.out[i] = true;
                println!("beat {beat:.1}: {} is out", v.name);
            }
            if v.failed && !self.failed[i] {
                self.failed[i] = true;
                println!("beat {beat:.1}: {} failed and plays on", v.name);
            }
        }
        // Everyone's life every eight bars.
        let bars = (beat / 32.0).floor();
        if bars.is_finite() && bars > self.told {
            self.told = bars;
            let lives: Vec<String> = views
                .iter()
                .map(|v| format!("{} {:.0}%", v.name, v.life * 100.0))
                .collect();
            println!("beat {beat:.1}: life: {}", lives.join(", "));
        }
        let hazards = player.log().hazards.len();
        if hazards != self.hazards {
            println!("beat {beat:.1}: {} shocks put into the play", hazards - self.hazards);
            self.hazards = hazards;
        }
        let tally = player.results().tally;
        if tally.mine_hit != self.mines {
            println!("beat {beat:.1}: a mine or shock stepped on ({} in all)", tally.mine_hit);
            self.mines = tally.mine_hit;
        }
        let misses = tally.taps.last().copied().unwrap_or(0);
        if misses != self.hits {
            println!("beat {beat:.1}: {misses} misses so far");
            self.hits = misses;
        }
    }
}

/// The animation as it is made: frames go to the encoder a crossfade's
/// length late, so that a shot's last frames can be blended into the next
/// one's first.
struct Reel {
    encoder: Child,
    size: [u32; 2],
    fps: u32,
    /// The crossfade into this shot, frames.
    transition: usize,
    /// Frames not yet encoded, the latest last.
    tail: VecDeque<Vec<u8>>,
    /// The last shot's last frames, blended into this one's first.
    leaving: VecDeque<Vec<u8>>,
    blended: usize,
    encoded: usize,
    /// Every frame encoded, hashed: the same run to run.
    hash: std::hash::DefaultHasher,
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
            hash: std::hash::DefaultHasher::new(),
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

    /// The shot ends: its last `frames` are blended into the next one's
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
        std::hash::Hasher::write(&mut self.hash, pixels);
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
        println!(
            "{} frames encoded, their hash {:016x}",
            self.encoded,
            std::hash::Hasher::finish(&self.hash)
        );
        Ok(self.encoded as f64 / f64::from(self.fps))
    }
}
