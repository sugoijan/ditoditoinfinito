mod navigator;

use ruffle_core::{
    FloatDuration, PlayerBuilder, PlayerEvent, backend::navigator::NullExecutor,
    limits::ExecutionLimit, tag_utils::SwfMovie,
};
use ruffle_render_wgpu::{backend::WgpuRenderBackend, target::TextureTarget, wgpu};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{any::Any, error::Error, fs, io::Write, path::Path, time::Duration};

const MAX_INPUT: u64 = 16 * 1024 * 1024;

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn input_events(config: &Value) -> Result<Vec<(u64, PlayerEvent)>, Box<dyn Error>> {
    use ruffle_core::events::{
        KeyDescriptor, KeyLocation, LogicalKey, MouseButton, NamedKey, PhysicalKey,
    };
    let mut result = Vec::new();
    if let Some(events) = config["events"].as_array() {
        if events.len() > 64 {
            return Err("input event limit".into());
        }
        for event in events {
            let frame = event["frame"]
                .as_u64()
                .filter(|f| (1..=600).contains(f))
                .ok_or("invalid event frame")?;
            match event["type"].as_str() {
                Some("click") => {
                    let x = event["x"]
                        .as_f64()
                        .filter(|n| (0.0..=320.0).contains(n))
                        .ok_or("invalid click x")?;
                    let y = event["y"]
                        .as_f64()
                        .filter(|n| (0.0..=240.0).contains(n))
                        .ok_or("invalid click y")?;
                    result.push((frame, PlayerEvent::MouseMove { x, y }));
                    result.push((
                        frame,
                        PlayerEvent::MouseDown {
                            x,
                            y,
                            button: MouseButton::Left,
                            index: None,
                        },
                    ));
                    result.push((
                        frame + 1,
                        PlayerEvent::MouseUp {
                            x,
                            y,
                            button: MouseButton::Left,
                        },
                    ));
                }
                Some("enter") => {
                    let key = KeyDescriptor {
                        physical_key: PhysicalKey::Enter,
                        logical_key: LogicalKey::Named(NamedKey::Enter),
                        key_location: KeyLocation::Standard,
                    };
                    result.push((frame, PlayerEvent::KeyDown { key }));
                    result.push((frame + 1, PlayerEvent::KeyUp { key }));
                }
                _ => return Err("pilot supports only click and enter input events".into()),
            }
        }
    }
    Ok(result)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, Box<dyn Error>> {
    use std::io::Read;
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > limit {
        return Err("input size limit exceeded".into());
    }
    Ok(data)
}

fn probe(args: &[String]) -> Result<(), Box<dyn Error>> {
    // Exercises OS restrictions directly, independently of the navigator backend.
    let secret = args.get(2).ok_or("missing probe path")?;
    let address = args.get(3).ok_or("missing probe address")?.parse()?;
    let read = fs::read(secret).is_ok();
    let write = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(format!("{secret}.write"))
        .is_ok();
    let network = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(1)).is_ok();
    println!(
        "{}",
        json!({"readAllowed":read,"writeAllowed":write,"networkAllowed":network})
    );
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--probe") {
        return probe(&args);
    }
    let config_path = Path::new(args.get(1).ok_or("usage: ddi-ruffle-probe CONFIG.json")?);
    let config: Value = serde_json::from_slice(&read_bounded(config_path, 65536)?)?;
    let input = config_path
        .parent()
        .unwrap()
        .join(config["movie"].as_str().ok_or("missing movie")?);
    let bytes = read_bounded(&input, MAX_INPUT)?;
    let movie_sha256 = hash(&bytes);
    if bytes.len() < 8 || !matches!(&bytes[..3], b"FWS" | b"CWS") {
        return Err("only FWS/CWS movies are supported by this pilot".into());
    }
    if u32::from_le_bytes(bytes[4..8].try_into()?) > 64 * 1024 * 1024 {
        return Err("declared uncompressed SWF size exceeds limit".into());
    }
    let source = url::Url::parse(config["sourceUrl"].as_str().ok_or("missing sourceUrl")?)?;
    if !matches!(source.scheme(), "http" | "https")
        || !source.username().is_empty()
        || source.password().is_some()
    {
        return Err("sourceUrl must be anonymous HTTP(S)".into());
    }
    let page_url = config["pageUrl"].as_str().map(str::to_owned);
    let mut movie = SwfMovie::from_data(&bytes, source.to_string(), None, None)?;
    if movie.is_action_script_3() {
        return Err("AVM2 is outside this pilot".into());
    }
    if let Some(vars) = config["flashVars"].as_object() {
        let pairs: Result<Vec<_>, _> = vars
            .iter()
            .map(|(k, v)| {
                v.as_str()
                    .map(|s| (k.clone(), s.to_owned()))
                    .ok_or("FlashVars must be strings")
            })
            .collect();
        movie.append_parameters(pairs?);
    }
    let fps = movie.frame_rate().to_f64();
    if !(1.0..=240.0).contains(&fps) {
        return Err("SWF frame rate outside pilot limits".into());
    }
    let frames: Vec<u64> = config["captureFrames"]
        .as_array()
        .ok_or("missing captureFrames")?
        .iter()
        .map(|x| x.as_u64().ok_or("invalid capture frame"))
        .collect::<Result<_, _>>()?;
    if frames.is_empty() || frames.len() > 16 || frames.iter().any(|&f| f == 0 || f > 600) {
        return Err("capture 1..16 frames, each in 1..600".into());
    }
    let render = config["render"].as_bool().unwrap_or(false);
    let events = input_events(&config)?;
    let mut executor = NullExecutor::new();
    let audit = navigator::Audit::default();
    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry().with(audit.clone()).init();
    let resources = navigator::read_resources(&config, config_path.parent().unwrap())?;
    let mut builder = PlayerBuilder::new()
        .with_movie(movie)
        .with_navigator(navigator::CachedNavigator::new(
            source,
            &executor,
            audit.clone(),
            resources,
        ))
        .with_log(audit.clone())
        .with_page_url(page_url)
        .with_max_execution_duration(Duration::from_millis(500))
        .with_viewport_dimensions(320, 240, 1.0)
        .with_autoplay(true);
    if render {
        builder = builder.with_renderer(WgpuRenderBackend::for_offscreen(
            (320, 240),
            wgpu::Backends::METAL,
            wgpu::PowerPreference::LowPower,
        )?);
    }
    let player = builder.build();
    let mut loaded = false;
    for _ in 0..600 {
        if player.lock().map_err(|_| "player lock poisoned")?.preload(
            &mut ExecutionLimit::with_max_ops_and_time(10000, Duration::from_millis(100)),
        ) {
            loaded = true;
            break;
        }
        executor.run();
    }
    if !loaded {
        return Err("preload limit exceeded".into());
    }
    let mut captures = Vec::new();
    for frame in 1..=*frames.iter().max().unwrap() {
        {
            let mut guard = player.lock().map_err(|_| "player lock poisoned")?;
            for (_, event) in events.iter().filter(|(at, _)| *at == frame) {
                guard.handle_event(event.clone());
            }
            guard.run_frame();
            guard.update_timers(FloatDuration::from_millis(1000.0 / fps));
            guard.audio_mut().tick();
        }
        // Resource completion callbacks acquire the player lock themselves.
        executor.run();
        let mut player = player.lock().map_err(|_| "player lock poisoned")?;
        if render {
            player.render();
        }
        if frames.contains(&frame) {
            captures.push(
                json!({"frame":frame, "state":ruffle_core::ddi_snapshot::capture(&mut player)}),
            );
            if render {
                let renderer = (player.renderer() as &dyn Any)
                    .downcast_ref::<WgpuRenderBackend<TextureTarget>>()
                    .ok_or("renderer type")?;
                let image = renderer.capture_frame().ok_or("no rendered frame")?;
                // PPM avoids a new image encoder dependency; pixel data is exact RGB.
                let mut out = fs::File::create(format!("frame-{frame}.ppm"))?;
                write!(out, "P6\n{} {}\n255\n", image.width(), image.height())?;
                for p in image.pixels() {
                    out.write_all(&p.0[..3])?;
                }
            }
        }
    }
    let halted =
        ruffle_core::ddi_snapshot::halted(&mut *player.lock().map_err(|_| "player lock poisoned")?);
    let output = json!({"schema":"ddi-ruffle-probe/experimental-1", "ruffleRelease":"v0.7.1",
        "ruffleRevision":"89a7049965f3090e706265670df309a5128233ac",
        "build":serde_json::from_str::<Value>(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/build-identity.json")))?,
        "source":{"movieSha256":movie_sha256,"config":config},
        "swfHeaderFps":fps, "clock":{"frames":"one run_frame per iteration","timers":"fixed header-FPS step",
            "getTimer":"wall clock","audio":"NullAudioBackend: sound position fixed at zero","timebaseVerified":false},
        "avm1Halted":halted,"rendered":render, "captures":captures, "audit":audit.report()});
    let output = serde_json::to_vec(&output)?;
    if output.len() > 32 * 1024 * 1024 {
        return Err("snapshot output size exceeded".into());
    }
    std::io::stdout().write_all(&output)?;
    if halted {
        return Err(
            "AVM1 halted; captured state is diagnostic, not a valid chart reference".into(),
        );
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
