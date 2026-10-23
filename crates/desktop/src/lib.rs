//! The desktop shell (`docs/plans/ui-core.md`, step 3): a winit window
//! running the UI core, drawn by `ddi_render`'s painter with cosmic-text
//! and the system's fonts. It runs the developer screen
//! (`ddi_ui::dev`), the same code as the web's `#/ui-dev`, and the
//! theme's screens on sample data (song select by default, `--screen
//! select|results|options|credits|calibration` as `#/ui-dev?screen=…`; `--dev` for the
//! developer screen); audio, play and the library on disk come
//! with the rest of phase 9.
//!
//! - [`keys`]: winit's keys as the DOM names them.
//! - [`store`]: settings in a file, through `ddi_platform::storage`.
//! - [`gpu`]: the painter over a surface or an offscreen texture, with
//!   the bundled faces (the HUD's, the theme's) and the system's.
//! - [`images`]: banners and backgrounds for the texture cache.
//! - [`os`]: links, file dialogs, the clipboard.
//! - [`plays`]: the play history, a file a play.
//! - [`shell`]: the window and its events; [`pacing`]: when a play's
//!   frames start.
//! - [`audio`]: sound (cpal, our mixer, the clock, the decoder thread);
//!   [`clock`]: the host clock everything stamps against; [`preview`]:
//!   song select's previews.

pub mod audio;
pub mod calibrate;
pub mod clock;
pub mod devices;
pub mod display_link;
pub mod editor;
pub mod field_preview;
pub mod gpu;
pub mod history;
pub mod images;
pub mod input;
pub mod keyboard;
pub mod keys;
pub mod library;
pub mod options;
pub mod os;
pub mod pacing;
pub mod pads;
pub mod paths;
pub mod play;
pub mod plays;
pub mod preview;
pub mod preview_movie;
pub mod shell;
pub mod songs;
pub mod store;
#[cfg(feature = "video")]
pub mod video;

/// `--video-check FILE [--reference REF.yuv] [--frames]
/// [--decoder auto|software|videotoolbox|mediafoundation|mediafoundation-auto|both]`: the exit code.
fn video_check(args: &[String], at: usize) -> i32 {
    let Some(file) = args.get(at + 1) else {
        eprintln!("--video-check needs a file");
        return 2;
    };
    #[cfg(feature = "video")]
    {
        let reference = args
            .iter()
            .position(|a| a == "--reference")
            .and_then(|i| args.get(i + 1))
            .map(std::path::PathBuf::from);
        let path = std::path::Path::new(file);
        let decoder = args
            .iter()
            .position(|a| a == "--decoder")
            .map(|i| args.get(i + 1).map(String::as_str).unwrap_or(""))
            .unwrap_or("auto");
        let both = decoder == "both";
        let backend = if both {
            video::Backend::diagnostic_native()
        } else if let Some(b) = video::Backend::parse(decoder) {
            b
        } else {
            eprintln!(
                "--decoder must be auto, software, videotoolbox, mediafoundation, mediafoundation-auto or both"
            );
            return 2;
        };
        let mut host = video::VideoHost::with_backend(video::helper_path(), backend);
        let mut report = video::check(&mut host, path, reference.as_deref());
        if both {
            let mut module = video::VideoHost::with_helper(video::helper_path());
            let expected = video::check(&mut module, path, reference.as_deref());
            report
                .failures
                .extend(expected.failures.iter().map(|e| format!("module: {e}")));
            if report.info != expected.info {
                report
                    .failures
                    .push("native decoder metadata differs from the module".into());
            }
            if report.frames != expected.frames {
                report
                    .failures
                    .push("native decoder timestamps or pixels differ from the module".into());
            }
            for line in expected.lines("module reference") {
                println!("{line}");
            }
        }
        let name = path
            .file_name()
            .map_or(file.clone(), |n| n.to_string_lossy().into_owned());
        for line in report.lines(&name) {
            println!("{line}");
        }
        // Each frame's time and planes' CRC-32, to compare with the web
        // module's under Node (`video/check.mjs --frames`).
        if args.iter().any(|a| a == "--frames") {
            for (i, (pts, crc)) in report.frames.iter().enumerate() {
                println!("frame {i} {pts:.6} {crc:08x}");
            }
        }
        i32::from(!report.failures.is_empty())
    }
    #[cfg(not(feature = "video"))]
    {
        eprintln!("{file}: this build has no video decoder (feature `video`)");
        2
    }
}

/// Windows: the console the game was started from, for what its flags
/// print. Outside debug builds the game is a GUI program (`crates/app`),
/// started with no console and, unless redirected, no standard handles,
/// so its output went nowhere. When neither stdout nor stderr was given
/// a file or a pipe, this attaches to the parent's console (cmd's,
/// PowerShell's; from Explorer there is none, and nothing changes):
/// attaching fills empty standard handles with the console's (one it
/// leaves unusable is opened here, `CONOUT$`), and std looks them up on
/// every write (`GetStdHandle`), so what follows reaches it. A run given
/// no flags (a player's) never attaches: an attached process ends with
/// its console's Ctrl+C or close, and a shell does not wait for a GUI
/// program, so its prompt is back while the game runs. A debug build is
/// a console program and has its handles already.
#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_UNKNOWN, GetFileType,
        OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE,
        STD_OUTPUT_HANDLE, SetStdHandle,
    };
    // SAFETY (the calls below): this process's standard handles, read and
    // set; the console's output opened once and kept as one of them.
    let unset = |id: STD_HANDLE| unsafe {
        let handle = GetStdHandle(id);
        handle.is_null()
            || handle == INVALID_HANDLE_VALUE
            || GetFileType(handle) == FILE_TYPE_UNKNOWN
    };
    let outputs = [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];
    if !outputs.into_iter().all(unset) || unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
        return;
    }
    for id in outputs.into_iter().filter(|&id| unset(id)) {
        let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        let console = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if console != INVALID_HANDLE_VALUE {
            unsafe { SetStdHandle(id, console) };
        }
    }
}

/// How desktop folds text for the search: Unicode's tables (the web asks
/// the browser).
pub use ddi_library::search::Nfkd;

/// Polls a future to its end on this thread (wgpu's native requests are
/// ready at once or soon; no executor needed).
pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

/// Runs the desktop app: `ditoditoinfinito [--screen select|results|options|credits|calibration |
/// --dev | --widgets] [--seconds N] [--volume V] [--fake-pad SCRIPT]`,
/// the theme's screens on sample data in a window (options on the stored
/// settings), song select unless `--screen` names another (`--dev`: the
/// UI core's developer screen; `--widgets`: its widget screen); with
/// `--seconds`, it quits after that long and prints what it drew, how
/// the audio clock went and the input it had; `--volume` sets the master
/// volume (0–1, a gain) instead of the settings'; `--fake-pad` plays a
/// scripted standard controller instead of the real ones
/// (`input::script`: `1.0:13+,1.1:13-`). `--video-check FILE
/// [--reference REF.yuv]` decodes a movie through the software decoder
/// (feature `video`), seeks through it, prints what it found and quits.
/// `--play SONG [--chart N] [--auto | --auto=pad] [--start]` opens
/// that chart's start prompt over song select (charts are played from
/// song select too), `--start` starting it once the song is read;
/// `--edit SONG [--chart N]` opens the chart editor on that chart over
/// song select once the library has the song;
/// `--calibrate visual|audio|combined` opens that calibration test's
/// prompt (`--auto` autoplays it following the cue, `--bias MS` that
/// late; leaving it opens calibration's menu); `--silent` opens no sound device at all
/// (plays run on the host clock, silent). `--pacing block|late` paces a
/// play's frames one after another or each as late before its refresh as
/// it can start (`pacing`), `--in-flight 1|2` with that many frames in
/// flight (by default late with two where a display link runs, macOS,
/// else block with one); `--pacing-report` alternates
/// block and late with one and two in flight, four seconds each, and
/// prints what each did when the play ends.
pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if !args.is_empty() {
        attach_console();
    }
    if args.iter().any(|a| a == "--danoni-bundle-info") {
        #[cfg(danoni_importer)]
        println!("{}", danoni::bundle_info());
        #[cfg(not(danoni_importer))]
        println!("{{\"enabled\":false}}");
        return;
    }
    if args.iter().any(|a| a == "--danoni-importer-check") {
        #[cfg(danoni_importer)]
        match danoni::check() {
            Ok(songs) => println!(
                "{}",
                serde_json::json!({"ready":true,"songs":songs,"protocolVersion":1, "bundledSongs":library::bundled().len()})
            ),
            Err(e) => {
                eprintln!("DanOni importer check failed: {e}");
                std::process::exit(1);
            }
        }
        #[cfg(not(danoni_importer))]
        {
            eprintln!("This build has no supported DanOni importer");
            std::process::exit(2);
        }
        #[cfg(danoni_importer)]
        return;
    }
    clock::epoch();
    store::install();
    // Flags that would be misread: said before anything opens.
    if let Some(i) = args.iter().position(|a| a == "--calibrate")
        && !args
            .get(i + 1)
            .is_some_and(|m| matches!(m.as_str(), "visual" | "audio" | "combined"))
    {
        eprintln!("--calibrate takes a test: visual, audio or combined");
        std::process::exit(2);
    }
    if let Some(i) = args.iter().position(|a| a == "--bias")
        && !args
            .get(i + 1)
            .and_then(|v| v.parse::<f64>().ok())
            .is_some_and(f64::is_finite)
    {
        eprintln!("--bias takes milliseconds (a number)");
        std::process::exit(2);
    }
    let pacing = match args.iter().position(|a| a == "--pacing") {
        Some(i) => match args.get(i + 1).and_then(|m| pacing::Mode::parse(m)) {
            Some(m) => Some(m),
            None => {
                eprintln!("--pacing takes block or late");
                std::process::exit(2);
            }
        },
        None => None,
    };
    let in_flight = match args.iter().position(|a| a == "--in-flight") {
        Some(i) => match args.get(i + 1).map(String::as_str) {
            Some("1") => Some(1),
            Some("2") => Some(2),
            _ => {
                eprintln!("--in-flight takes 1 or 2");
                std::process::exit(2);
            }
        },
        None => None,
    };
    if let Some(i) = args.iter().position(|a| a == "--video-check") {
        std::process::exit(video_check(&args, i));
    }
    // No sound device at all: plays run on the host clock.
    if args.iter().any(|a| a == "--silent") {
        audio::set_headless(true);
    }
    let screen = match args.iter().position(|a| a == "--screen") {
        Some(i) => args.get(i + 1).cloned().unwrap_or_default(),
        None if args.iter().any(|a| a == "--widgets") => "widgets".into(),
        None if args.iter().any(|a| a == "--dev") => "dev".into(),
        None if args
            .iter()
            .any(|a| matches!(a.as_str(), "--play" | "--replay" | "--calibrate" | "--edit")) =>
        {
            "select".into()
        }
        None => "start".into(),
    };
    let volume = args
        .iter()
        .position(|a| a == "--volume")
        .and_then(|i| args.get(i + 1)?.parse::<f32>().ok())
        .map(|v| v.clamp(0.0, 1.0));
    // Song select and options on the library on disk (the bundled songs
    // and the song folders), unless `--sample` asks for the sample data;
    // `--songs <dir>` adds a folder for this run.
    let songs = (!args.iter().any(|a| a == "--sample")
        && matches!(screen.as_str(), "start" | "select" | "options"))
    .then(|| {
        let extra: Vec<std::path::PathBuf> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| *a == "--songs")
            .filter_map(|(i, _)| args.get(i + 1).map(std::path::PathBuf::from))
            .collect();
        songs::Songs::new(extra)
    });
    // A play or a test opened from here: no first start's offer over it.
    if let Some(s) = &songs
        && args
            .iter()
            .any(|a| matches!(a.as_str(), "--play" | "--calibrate" | "--replay" | "--edit"))
    {
        s.skip_offer();
    }
    // What an earlier run left half extracted.
    play::load::clean_stale_parts();
    let mut shell = shell::Shell::named_with(&screen, volume, songs);
    shell.play.master = volume;
    shell.play.start_when_ready = args.iter().any(|a| a == "--start");
    // `--bias MS`: autoplay presses that late (negative: early), as the
    // web's `&bias=`.
    // (Checked above.)
    let bias = args
        .iter()
        .position(|a| a == "--bias")
        .and_then(|i| args.get(i + 1)?.parse::<f64>().ok())
        .map_or(0.0, |ms| ms / 1000.0);
    if let Some(songs) = &shell.songs {
        songs
            .testing
            .set((args.iter().any(|a| a == "--auto"), bias));
    }
    // `--play <song> [--chart N] [--auto | --auto=pad]`: that chart's
    // prompt at once (song select under it).
    if let Some(song) = args
        .iter()
        .position(|a| a == "--play")
        .and_then(|i| args.get(i + 1))
    {
        let chart = args
            .iter()
            .position(|a| a == "--chart")
            .and_then(|i| args.get(i + 1)?.parse().ok())
            .unwrap_or(0);
        let auto_pad = args.iter().any(|a| a == "--auto=pad");
        let mut r =
            play::Request::song(song, chart, auto_pad || args.iter().any(|a| a == "--auto"));
        r.auto_pad = auto_pad;
        r.bias = bias;
        *shell.play.requested.borrow_mut() = Some(r);
    }
    // `--edit <song> [--chart N]`: the chart editor on it (song select
    // under it), once the library has the song.
    if let Some(song) = args
        .iter()
        .position(|a| a == "--edit")
        .and_then(|i| args.get(i + 1))
        && let Some(songs) = &shell.songs
    {
        let chart = args
            .iter()
            .position(|a| a == "--chart")
            .and_then(|i| args.get(i + 1)?.parse().ok())
            .unwrap_or(0);
        *songs.edit.borrow_mut() = Some((song.clone(), chart));
    }
    // `--replay <play id> [--results]`: a kept play's replay (its
    // prompt at once), or its results (`docs/plans/play-history.md`).
    if let Some(id) = args
        .iter()
        .position(|a| a == "--replay")
        .and_then(|i| args.get(i + 1))
        && let Some(plays) = shell.play.plays.clone()
    {
        let results = args.iter().any(|a| a == "--results");
        *shell.play.requested.borrow_mut() = Some(play::replays::request(&plays, id, results));
    }
    // `--calibrate visual|audio|combined [--auto [--bias MS]]`: that
    // test's prompt at once (leaving it opens calibration's menu).
    if let Some(mode) = args
        .iter()
        .position(|a| a == "--calibrate")
        .and_then(|i| args.get(i + 1))
    {
        // (Checked above.)
        let mode = ddi_session::calibration::CalMode::parse(mode);
        let mut r = play::Request::calibration(mode, args.iter().any(|a| a == "--auto"));
        r.bias = bias;
        *shell.play.requested.borrow_mut() = Some(r);
    }
    // `--lose-device-after S`: the GPU device lost on purpose, to see the
    // window recover (as the web's `__DDI_LOSE_DEVICE`).
    shell.lose_device_after = args
        .iter()
        .position(|a| a == "--lose-device-after")
        .and_then(|i| args.get(i + 1)?.parse().ok());
    // `--pacing block|late`, `--in-flight 1|2`: how a play's frames
    // are paced (`pacing`; the defaults follow the display link, set
    // once the window is open); `--pacing-report`: block and late with one
    // and two in flight, four seconds each in turn, what each did
    // printed when the play ends.
    shell.pacing_flag = pacing;
    shell.in_flight_flag = in_flight;
    if args.iter().any(|a| a == "--pacing-report") {
        shell.pacing_report = Some(pacing::Report::alternating(4.0, &pacing::COLUMNS));
    }
    if let Some(i) = args.iter().position(|a| a == "--seconds") {
        shell.quit_after = args.get(i + 1).and_then(|s| s.parse().ok());
    }
    // A timed run samples the audio clock every millisecond.
    let probe = shell.quit_after.and_then(|_| {
        let probe = shell.sound.borrow().audio.probe()?;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut samples = Vec::new();
            while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                if let Some(s) = probe.now() {
                    samples.push(s);
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            samples
        });
        Some((stop, thread))
    });
    let event_loop = match winit::event_loop::EventLoop::new() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("no event loop: {e}");
            std::process::exit(1);
        }
    };
    // Controllers on their thread, waking the loop with their news; with
    // `--fake-pad <script>`, a scripted standard pad instead
    // (`input::script`).
    let proxy = event_loop.create_proxy();
    {
        // A song read for a play, its pictures: the loop wakes.
        let proxy = std::sync::Mutex::new(proxy.clone());
        shell.play.wake = Some(std::sync::Arc::new(move || {
            if let Ok(p) = proxy.lock() {
                let _ = p.send_event(());
            }
        }));
    }
    if let Some(songs) = &shell.songs {
        let proxy = std::sync::Mutex::new(proxy.clone());
        songs.start(std::sync::Arc::new(move || {
            if let Ok(p) = proxy.lock() {
                let _ = p.send_event(());
            }
        }));
    }
    let fake = args
        .iter()
        .position(|a| a == "--fake-pad")
        .and_then(|i| args.get(i + 1).cloned());
    shell.input.start_pads(
        Box::new(move || {
            let _ = proxy.send_event(());
        }),
        fake.as_deref(),
    );
    if let Err(e) = event_loop.run_app(&mut shell) {
        eprintln!("event loop: {e}");
    }
    if let Some(e) = &shell.error {
        eprintln!("{e}");
        std::process::exit(1);
    }
    // A play still running when the window closed.
    if let Some(text) = shell.pacing_text() {
        println!("{text}");
    }
    println!(
        "frames {} · redraws asked {} · skipped while occluded {} · devices lost {} · status {:?}",
        shell.frames,
        shell.asked,
        shell.occluded_skips,
        shell.devices_lost,
        shell.dev.borrow().status,
    );
    if let Some((env, _)) = &shell.theme {
        let s = env.state.borrow();
        println!(
            "theme: screen {} · song {:?} · style {} · chart {:?} · compact {} · pulse {}",
            s.screen, s.song, s.style, s.chart, s.compact, s.pulsing
        );
    }
    let layers: Vec<String> = shell.ui.layers().into_iter().map(|l| l.name).collect();
    println!("layers: {}", layers.join(" › "));
    let player = shell.sound.borrow();
    println!("{}", shell.input.summary());
    if let Some(songs) = &shell.songs
        && let Some(l) = songs.library.borrow().as_ref()
    {
        let r = &l.report;
        println!(
            "library: {} songs from {} folders{} and {} bundled · listed from the cache in {} · scan {:.0} ms, {} parsed, {} from the cache · {} not read, {} duplicate packs, {} missing",
            l.songs.len(),
            songs.folders().len(),
            if l.scanning { " (still reading)" } else { "" },
            songs.bundled.len(),
            r.cached_ms
                .map_or("—".to_string(), |ms| format!("{ms:.0} ms")),
            r.ms,
            r.parsed,
            r.reused,
            r.failed.len(),
            r.duplicates.len(),
            r.missing.len(),
        );
    }
    println!("{}", player.audio.status_line());
    if let Some(info) = player.audio.info() {
        println!("audio: {info:?}");
    }
    for (id, took) in &player.decoded {
        println!(
            "preview decoded: {id} in {:.0} ms",
            took.as_secs_f64() * 1000.0
        );
    }
    for e in &player.errors {
        println!("preview error: {e}");
    }
    for line in &shell.play.log {
        println!("{line}");
    }
    println!(
        "play: {}",
        if shell.play.playing() {
            "playing".to_string()
        } else if shell.play.prompting() {
            format!("prompt {:?}", shell.play.prompt_step())
        } else if shell.play.idle() {
            "none".to_string()
        } else {
            "results".to_string()
        }
    );
    // The devices as last seen, and what the notices said.
    if let Some(d) = &shell.play.watcher.last {
        println!("devices: {} · {}", d.audio.id, d.display.id);
    }
    let notices = shell.play.watcher.feed.borrow().clone();
    if let Some(b) = &notices.banner {
        println!("notice: {}", b.text);
    }
    if let Some((stop, thread)) = probe {
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Ok(samples) = thread.join() {
            // The first half second settles (the anchor, the lead).
            let skip = samples.len().min(500);
            let s = audio::ClockStats::of(&samples[skip..]);
            println!(
                "audio clock: {} samples · jitter sd {:.1} µs, max {:.1} µs · drift {:+.1} ppm · latency {:.2} ms ({:.2}–{:.2})",
                s.samples,
                s.jitter_sd * 1e6,
                s.jitter_max * 1e6,
                s.drift_ppm,
                s.latency_mean * 1e3,
                s.latency_min * 1e3,
                s.latency_max * 1e3,
            );
        }
    }
}

#[cfg(danoni_importer)]
pub mod danoni;
