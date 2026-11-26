//! A room host's chat behind the play (`docs/plans/multiplayer-twitch.md`,
//! part C; round TW2) as the desktop window draws it, on this machine's
//! GPU offscreen and a fake clock with no sound device: the scripted chat
//! (`ddi_chat::script`, TESTING ONLY) beside Beatucada's Challenge
//! autoplayed, alone and in a practice stage with eight CPUs in
//! miniature. With Options → Multiplayer's "Twitch chat in rooms" Off (the
//! default) nothing of the chat is built or drawn; on, it is drawn under
//! the field in both styles, and the play judges as without it. Skipped
//! where there is no GPU adapter (CI).
//!
//! `cargo test -p ddi-desktop --test chat_offscreen -- --ignored
//! --nocapture` (under the watchdog, a scratch `DDI_HOME`) plays the
//! study's scenarios, keeping their pictures as PNG in
//! `DDI_OFFSCREEN_PPM=<dir>`, and measures a frame's cost with the chat
//! off, on and flooded.

mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use ddi_chat::{Level, Style};
use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_room::CpuMode;
use ddi_ui::screens::data::Difficulty;
use ddi_ui::state::settings::{ReducedMotion, Settings};

/// One play.
#[derive(Clone)]
struct Scenario {
    name: String,
    size: [u32; 2],
    /// CPUs in a practice stage (none: a solo play).
    cpus: Option<u16>,
    level: Level,
    style: Style,
    names: bool,
    opacity: u8,
    reduced: bool,
    /// The scripted chat (`demo`, `emotes`, `flood`, a rate).
    script: &'static str,
    /// Song seconds pictures are kept at.
    shots: Vec<f64>,
    /// Measures frames from this song second.
    measure: Option<f64>,
    /// The look drawn in (round TW5's candidates; none: the rules' own).
    look: Option<ddi_chat::overlay::Look>,
    /// The rules laid by (round DE's comparison; none: v2's).
    rules: Option<ddi_chat::overlay::Rules>,
}

impl Scenario {
    fn new(name: &str) -> Scenario {
        Scenario {
            name: name.into(),
            size: [1280, 720],
            cpus: None,
            level: Level::Messages,
            style: Style::Across,
            names: false,
            opacity: 60,
            reduced: false,
            script: "demo",
            shots: Vec::new(),
            measure: None,
            look: None,
            rules: None,
        }
    }
}

#[derive(Debug, Default)]
struct Played {
    /// The most items, runs and emotes one frame drew.
    items: usize,
    emotes: usize,
    /// The last frame's images (ready, in flight, failed, proxied) and
    /// animated emotes ready with their atlas units (round DE).
    images: (usize, usize, usize, usize),
    animated: (usize, usize),
    /// Every frame's chat as drawn (none where nothing was built).
    built: bool,
    layer: bool,
    texts: Vec<String>,
    stats: ddi_chat::overlay::Stats,
    /// Frames' costs, milliseconds: recording them; the chat's part.
    record: Vec<f64>,
    chat_ms: Vec<f64>,
    /// The scene's batches and instances in measured frames.
    batches: Vec<f64>,
    /// The judgments' tally at the end.
    tally: String,
    /// Song seconds and items of each frame drawn.
    timeline: Vec<(f64, usize)>,
    /// At each shot: its song second, its picture and the chat drawn.
    seen: Vec<(f64, Vec<u8>, Option<ddi_shell::chat::ChatDrawn>)>,
    /// The levels and styles the frames showed.
    shown: Vec<(Level, Style)>,
    /// The animation clock at its most, milliseconds.
    anim_ms: u64,
    /// The last frame's images by provider and the hosts through the
    /// proxy (the global-emotes demo).
    providers: Vec<(&'static str, [usize; 4])>,
    proxied: Vec<String>,
    /// Emotes drawn behind the player's own field and behind an
    /// opponent's miniature, in any frame (round TW5).
    behind_field: bool,
    behind_miniature: bool,
}

fn stat(v: &[f64]) -> String {
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
    let median = v.get(v.len() / 2).copied().unwrap_or(0.0);
    let p95 = v.get(v.len() * 95 / 100).copied().unwrap_or(0.0);
    format!("mean {mean:.2} median {median:.2} p95 {p95:.2}")
}

fn play(s: &Scenario) -> Option<Played> {
    common::indexes();
    let adapter = common::gpu::adapter_or_skip()?;
    let size = s.size;
    let (device, queue) = gpu::device(&adapter).unwrap();
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let mut settings = Settings {
        twitch_chat: s.level,
        chat_style: s.style,
        chat_names: s.names,
        chat_opacity: s.opacity,
        reduced_motion: if s.reduced {
            ReducedMotion::On
        } else {
            ReducedMotion::Off
        },
        ..Settings::default()
    };
    // The hand rests out of these pictures.
    settings.dancing_hand = false;
    settings.save();
    // `global`: the global-emotes demo over its fixtures as files (round
    // TW5), never a provider.
    let script = if s.script == "global" {
        let dir = std::env::temp_dir().join(format!("ddi-chat-global-play-{}", std::process::id()));
        ddi_desktop::chat::made::global(&dir).unwrap();
        Some(ddi_desktop::chat::global_script(
            ddi_chat::EmotesFrom::TwitchAndOthers,
            "https://unused.test".into(),
            Some(format!("{}/", dir.display())),
        ))
    } else {
        ddi_chat::script::Script::builtin(s.script)
    };
    ddi_session::chat::set_scripted(script);
    ddi_shell::chat::set_look(s.look);
    ddi_shell::chat::set_rules(s.rules);
    let screen = if s.cpus.is_some() { "start" } else { "select" };
    let (mut ui, env) = common::ui::theme(screen, common::ui::lowercase(), size, s.reduced);
    ui.update(0.5);
    let mut host = Host::new(Rc::default(), None);
    let clock = common::rig::fake_clock(&mut host);
    let formats = common::gpu::formats();
    let dev = || Device {
        device: &device,
        queue: &queue,
        formats,
    };
    let target = common::gpu::Target::new(&device, size);
    if let Some(cpus) = s.cpus {
        ddi_desktop::room::develop(0);
        let room = common::practice::open(&mut ui, &env, &host);
        let entry =
            common::practice::song_entry("beatucada", "Beatucada", 0, Difficulty::Challenge);
        {
            let mut r = room.borrow_mut();
            let mut rules = ddi_room::MatchRules {
                stages: 1,
                ..r.rules().clone()
            };
            rules.seats = rules.seats.max(cpus + 1);
            rules.cpu.mode = CpuMode::Fixed { count: cpus };
            rules.cpu.opponent.level = 7;
            rules.cpu.opponent.seed = 7;
            rules.set_attacks(false);
            r.save_rules(rules).unwrap();
            r.pick(&entry, &entry.charts[0]).unwrap();
        }
        ui.update(0.1);
        let mut request = host.requested.borrow_mut().take().expect("the stage");
        request.auto = true;
        *host.requested.borrow_mut() = Some(request);
        wait_prompt(&mut host, &mut ui, &env, &dev);
        common::rig::ready_and_start(&mut host, &mut ui, &env, &room, dev);
    } else {
        *host.requested.borrow_mut() = Some(Request::song("beatucada", 0, true));
        wait_prompt(&mut host, &mut ui, &env, &dev);
        ui.update(0.5);
        common::ui::key(&mut ui, "Enter", "Enter");
        ui.update(0.05);
        host.poll(&mut ui, &env, Some(dev()), 48_000);
    }
    assert!(host.playing(), "{}: playing", s.name);
    let mut played = Played::default();
    let mut shots = s.shots.iter().copied().peekable();
    let last = s
        .shots
        .iter()
        .copied()
        .chain(s.measure.map(|m| m + 8.0))
        .fold(0.0, f64::max);
    let mut measured = 0;
    let mut frames = 0;
    // The text's fonts are read on a thread: real time for them first.
    std::thread::sleep(std::time::Duration::from_millis(400));
    loop {
        if !host.playing() {
            break;
        }
        let song_t = host.running().map_or(0.0, |p| {
            p.session
                .player
                .judge_time(ddi_platform::HostTime(clock.load(Ordering::SeqCst) as f64 / 1e9))
        });
        let in_measure = s.measure.is_some_and(|m| song_t >= m) && measured < 240;
        let step = (1e9 / 60.0) as u64;
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
        let start = Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        host.draw_play(
            &dev(),
            &mut encoder,
            &target.srgb,
            Some(&target.plain),
            size,
            ddi_platform::HostTime(now as f64 / 1e9),
            &mut ui,
            &env,
        );
        let recorded = start.elapsed().as_secs_f64() * 1000.0;
        queue.submit(Some(encoder.finish()));
        if frames % 30 == 0 {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
        let chat = host.chat_drawn();
        if std::env::var_os("DDI_CHAT_DEBUG").is_some() && frames < 400 && frames % 20 == 0 {
            eprintln!("{frames} {song_t:.2} {:?}", chat.as_ref().map(|c| (&c.texts, c.stats)));
        }
        played.built |= chat.is_some();
        played.layer |= host.chat_layer_drawn();
        if let Some(c) = &chat {
            played.items = played.items.max(c.items);
            played.emotes = played.emotes.max(c.emotes);
            played.images = c.images;
            played.animated = c.animated;
            played.stats = c.stats;
            played.providers = c.providers.clone();
            played.proxied = c.proxied.clone();
            played.anim_ms = played.anim_ms.max(c.anim_ms);
            if let Some(shown) = c.shown
                && !played.shown.contains(&shown)
            {
                played.shown.push(shown);
            }
            for t in &c.texts {
                if !played.texts.contains(t) {
                    played.texts.push(t.clone());
                }
            }
            if in_measure {
                played.chat_ms.push(c.ms);
            }
        }
        played.timeline.push((song_t, chat.as_ref().map_or(0, |c| c.items)));
        if let (Some(c), Some(fields)) = (&chat, host.opponent_fields()) {
            // The player's own field between the two sides' miniatures.
            let left = fields.slots.iter().filter(|s| s.left).map(|s| s.rect[0] + s.rect[2]).fold(0.0, f32::max);
            let right = fields
                .slots
                .iter()
                .filter(|s| !s.left)
                .map(|s| s.rect[0])
                .fold(size[0] as f32, f32::min);
            for e in &c.emotes_drawn {
                let [x0, y0, x1, y1] = e.rect;
                played.behind_field |= x0 > left + 20.0 && x1 < right - 20.0;
                played.behind_miniature |= fields.slots.iter().any(|s| {
                    let [sx, sy, sw, sh] = s.rect;
                    x0 >= sx && x1 <= sx + sw && y0 >= sy && y1 <= sy + sh
                });
            }
        }
        if in_measure {
            if measured >= 20 {
                played.record.push(recorded);
                if let Some(f) = host.frame_stats() {
                    played.batches.push(f64::from(f.quad_batches));
                }
            }
            measured += 1;
        }
        if shots.peek().is_some_and(|&t| song_t >= t) {
            let t = shots.next().unwrap();
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let pixels = gpu::read_rgba(&device, &queue, &target.texture);
            common::shot(&format!("{}-{t:05.2}s.png", s.name), &pixels, size);
            eprintln!(
                "{} at {t:.1} s: {} items, {} emotes",
                s.name,
                chat.as_ref().map_or(0, |c| c.items),
                chat.as_ref().map_or(0, |c| c.emotes)
            );
            played.seen.push((song_t, pixels, chat.clone()));
        }
        frames += 1;
        if song_t > last + 0.2 && shots.peek().is_none() && s.measure.is_none_or(|_| measured >= 240)
        {
            break;
        }
        assert!(frames < 60 * 300, "the play never ended");
    }
    if let Some(p) = host.running() {
        played.tally = format!("{:?}", p.session.player.tally());
    }
    ddi_session::chat::set_scripted(None);
    ddi_shell::chat::set_look(None);
    ddi_shell::chat::set_rules(None);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    Some(played)
}

fn wait_prompt<'a>(
    host: &mut Host,
    ui: &mut ddi_ui::Ui,
    env: &ddi_ui::screens::Env,
    dev: &impl Fn() -> Device<'a>,
) {
    let mut waited = 0;
    loop {
        host.poll(ui, env, Some(dev()), 48_000);
        ui.update(0.05);
        if host.prompt_ready() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 1500, "the song was never read");
    }
}

/// With "Twitch chat in rooms" Off (the default), a scripted chat set
/// changes nothing: no chat is built, no layer drawn, and the play
/// judges as one with it on.
#[test]
fn with_the_option_off_no_chat_is_built_or_drawn() {
    let off = Scenario {
        level: Level::Off,
        shots: vec![9.0],
        ..Scenario::new("off")
    };
    let Some(off_played) = play(&off) else {
        return;
    };
    assert!(!off_played.built, "no chat built");
    assert!(!off_played.layer, "no layer drawn");
    assert_eq!(off_played.items, 0);
    let on = Scenario {
        shots: vec![9.0],
        ..Scenario::new("on")
    };
    let on_played = play(&on).unwrap();
    assert!(on_played.built && on_played.layer);
    assert_eq!(off_played.tally, on_played.tally, "judged alike");
}

/// Round TW3: in a room's stage the room's own ceiling decides (CPU
/// practice has no host's Twitch; where a scripted chat plays, TESTING
/// ONLY, its line stands for it): a practice stage of eight CPUs shows the
/// chat in both styles under the miniatures' fields, and a device at Off
/// shows none.
#[test]
fn a_practice_stage_shows_the_rooms_chat_under_a_scripted_chat() {
    for (style, level) in [
        (Style::Falling, Level::Messages),
        (Style::Across, Level::Messages),
        (Style::Falling, Level::Off),
    ] {
        let s = Scenario {
            cpus: Some(8),
            style,
            level,
            shots: vec![9.0],
            ..Scenario::new(&format!("stage-{style:?}-{level:?}"))
        };
        let Some(p) = play(&s) else {
            return;
        };
        if level == Level::Off {
            assert!(!p.built && !p.layer, "{}: none", s.name);
        } else {
            assert!(p.built && p.layer && p.items > 0, "{}: {p:?}", s.name);
        }
    }
}

/// The scripted chat drawn under the field in both styles: its
/// messages in several scripts with their emotes inline, every emote by
/// itself,
/// what C11 leaves out left out, the delete and the clears taking items
/// off, and reduced motion standing still.
#[test]
fn the_scripted_chat_is_drawn_in_both_styles() {
    for (style, reduced) in [
        (Style::Across, false),
        (Style::Falling, false),
        (Style::Across, true),
    ] {
        // In the look that keeps a message's emotes in one item, so that
        // its four are seen together.
        let s = Scenario {
            style,
            reduced,
            shots: vec![4.5, 13.0],
            look: Some(ddi_chat::overlay::LOOK_LARGER),
            ..Scenario::new(&format!("styles-{style:?}-{reduced}"))
        };
        let Some(p) = play(&s) else {
            return;
        };
        assert!(p.layer, "{}: a layer drawn", s.name);
        assert!(p.items >= 4 && p.emotes >= 2, "{}: {p:?}", s.name);
        let has = |needle: &str| p.texts.iter().any(|t| t.contains(needle));
        assert!(has("こんにちは"), "{}: {:?}", s.name, p.texts);
        assert!(has("مرحبا"), "{}", s.name);
        // Round TW5: "Kappa Kappa Kappa Kappa" is four emotes, never one
        // with a count.
        assert!(has("◆◆◆◆"), "{}: four emotes {:?}", s.name, p.texts);
        assert!(!has("×"), "{}: no count {:?}", s.name, p.texts);
        assert!(!has("!song") && !has("example.com"), "{}: left out", s.name);
        assert!(p.stats.left_out >= 2, "{}: {:?}", s.name, p.stats);
        assert!(p.stats.removed >= 1, "{}: {:?}", s.name, p.stats);
        // The chat's clear at the script's 18th second: nothing right
        // after it until new messages come.
        assert!(
            p.timeline.iter().any(|(_, n)| *n == 0) || p.stats.removed >= 2,
            "{}",
            s.name
        );
    }
}

/// The study (`docs/plans/multiplayer-twitch.md`, C7; round TW2): both
/// styles, emotes and messages, opacity at 40, 60 and 80%, reduced
/// motion, alone and in a stage of eight CPUs, at 1280×720 and on a phone
/// held upright; and a frame's cost with the chat off, on and flooded.
#[test]
#[ignore]
fn study() {
    let shots = vec![4.6, 9.5, 14.5];
    let mut scenarios = Vec::new();
    for cpus in [None, Some(8)] {
        let who = if cpus.is_some() { "eight" } else { "alone" };
        for style in [Style::Across, Style::Falling] {
            for level in [Level::Emotes, Level::Messages] {
                for opacity in [40, 60, 80] {
                    scenarios.push(Scenario {
                        cpus,
                        style,
                        level,
                        opacity,
                        shots: shots.clone(),
                        ..Scenario::new(&format!(
                            "{who}-{}-{}-{opacity}",
                            format!("{style:?}").to_lowercase(),
                            format!("{level:?}").to_lowercase()
                        ))
                    });
                }
            }
            scenarios.push(Scenario {
                cpus,
                style,
                reduced: true,
                names: true,
                shots: shots.clone(),
                ..Scenario::new(&format!("{who}-{}-reduced-names", format!("{style:?}").to_lowercase()))
            });
            scenarios.push(Scenario {
                cpus,
                style,
                size: [390, 844],
                shots: shots.clone(),
                ..Scenario::new(&format!("{who}-{}-phone", format!("{style:?}").to_lowercase()))
            });
        }
    }
    let only = std::env::var("DDI_CHAT_STUDY").unwrap_or_default();
    for s in scenarios.iter().filter(|s| only.is_empty() || s.name.contains(&only)) {
        if play(s).is_none() {
            return;
        }
    }
    // A frame's cost: off, on, flooded (50 messages a second) and a
    // screen full of animated emotes (round TW4), alone and with eight
    // CPUs, at 1280×720 and a phone's size held upright
    // (`DDI_CHAT_COST=<text>` picks those whose names have it).
    let pick = std::env::var("DDI_CHAT_COST").unwrap_or_default();
    let sizes = [([1280, 720], "1280"), ([390, 844], "phone")];
    for (size, at, cpus) in sizes
        .into_iter()
        .flat_map(|(size, at)| [None, Some(8)].map(|cpus| (size, at, cpus)))
    {
        // Round DE: each under v1 and v2 (`chat-overlay-v2`'s limits
        // tripled), and the walls at v2's limits (60 messages a second of
        // 30 emotes each; all animated).
        let cases = [
            ("off", Level::Off, "demo"),
            ("on", Level::Messages, "demo"),
            ("flood", Level::Messages, "50"),
            ("animated", Level::Emotes, "animated"),
            ("wall", Level::Emotes, "wall"),
            ("animated-wall", Level::Emotes, "animated-wall"),
        ];
        let rules = [("v1", ddi_chat::overlay::CHAT_OVERLAY_V1), ("v2", ddi_chat::overlay::CHAT_OVERLAY_V2)];
        for ((what, level, script), (rname, r)) in cases
            .into_iter()
            .flat_map(|c| rules.map(|r| (c, r)))
            .filter(|((what, ..), (rname, _))| *what != "off" || *rname == "v2")
        {
            let s = Scenario {
                cpus,
                level,
                script,
                size,
                measure: Some(6.0),
                rules: Some(r),
                ..Scenario::new(&format!("cost-{at}-{}-{what}-{rname}", cpus.unwrap_or(0)))
            };
            if !s.name.contains(&pick) {
                continue;
            }
            let p = play(&s).unwrap();
            eprintln!(
                "{}: {} frames: record {}; chat {}; batches {}; most items {} emotes {}; images {:?} animated {:?}; stats {:?}",
                s.name,
                p.record.len(),
                stat(&p.record),
                stat(&p.chat_ms),
                stat(&p.batches),
                p.items,
                p.emotes,
                p.images,
                p.animated,
                p.stats
            );
        }
    }
}

/// Round TW5's study of the emotes' look (`docs/plans/multiplayer-twitch.md`,
/// "Round TW5"): each candidate (`ddi_chat::overlay::LOOKS`) in both
/// styles, alone and in a stage of eight CPUs, on a phone held upright
/// and under reduced motion, over the `burst` script (a hype of one
/// emote, a mixed burst, an animated one); a picture of each moment and a
/// sequence of frames a tenth of a second apart through the hype, kept
/// in `DDI_OFFSCREEN_PPM=<dir>` (`DDI_CHAT_STUDY=<text>` picks those
/// whose names have it).
#[test]
#[ignore]
fn look_study() {
    let only = std::env::var("DDI_CHAT_STUDY").unwrap_or_default();
    // The script starts about 0.8 s before the song: its hype at 1.7 to
    // 3.7 s of the song, the mixed burst 6.2 to 9.2, the animated one
    // 10.2 to 13.2, the last messages from 14.2.
    let hype: Vec<f64> = (0..12).map(|k| 2.4 + 0.1 * f64::from(k)).collect();
    let moments = [4.5, 8.5, 12.5, 15.8];
    for look in ddi_chat::overlay::LOOKS {
        for (who, cpus, size, style, reduced) in [
            ("alone", None, [1280, 720], Style::Across, false),
            ("alone", None, [1280, 720], Style::Falling, false),
            ("eight", Some(8), [1280, 720], Style::Across, false),
            ("eight", Some(8), [1280, 720], Style::Falling, false),
            ("phone", None, [390, 844], Style::Across, false),
            ("phone", None, [390, 844], Style::Falling, false),
            ("reduced", Some(8), [1280, 720], Style::Across, true),
        ] {
            let mut shots: Vec<f64> = hype.clone();
            shots.extend(moments);
            let s = Scenario {
                cpus,
                size,
                style,
                reduced,
                level: Level::Messages,
                script: "burst",
                look: Some(look),
                shots,
                ..Scenario::new(&format!(
                    "look-{}-{who}-{}",
                    look.name,
                    format!("{style:?}").to_lowercase()
                ))
            };
            if !s.name.contains(&only) {
                continue;
            }
            let Some(p) = play(&s) else {
                return;
            };
            eprintln!(
                "{}: most items {}, most emotes {}, stats {:?}",
                s.name, p.items, p.emotes, p.stats
            );
        }
    }
}

/// Round TW5: the global-emotes demo played over its fixtures (the four
/// providers' sets written from their documentation, pictures made by
/// code, read as files: never a provider): every provider's emotes ready
/// and drawn, animated ones where the provider has them, BTTV's through
/// the proxy's paths.
#[test]
fn the_global_emotes_demo_plays_from_its_fixtures() {
    let s = Scenario {
        script: "global",
        shots: vec![8.0, 15.0],
        ..Scenario::new("tw5-global")
    };
    let Some(p) = play(&s) else {
        return;
    };
    assert!(p.layer && p.emotes > 10, "{p:?}");
    for (provider, [ready, _, failed, animated]) in &p.providers {
        assert!(*ready >= 3, "{provider}: {:?}", p.providers);
        // FrankerFaceZ's globals had no animated emote on 2026-11-20.
        assert_eq!(*animated >= 1, *provider != "ffz", "{provider}: animated {:?}", p.providers);
        assert_eq!(*failed, 0, "{provider}");
    }
    assert!(p.proxied.iter().any(|h| h == "cdn.betterttv.net"), "{:?}", p.proxied);
}

/// Each item's emotes at one frame, in order, by item.
fn by_item(c: &ddi_shell::chat::ChatDrawn) -> std::collections::BTreeMap<u64, Vec<ddi_shell::chat::EmoteDrawn>> {
    let mut out: std::collections::BTreeMap<u64, Vec<_>> = Default::default();
    for e in &c.emotes_drawn {
        out.entry(e.item).or_default().push(e.clone());
    }
    out
}

/// The mean difference of two pictures' pixels inside `a` and `b` (the
/// same emote's rects at two moments), each read from its own place.
fn crop_difference(size: [u32; 2], (pa, a): (&[u8], [f32; 4]), (pb, b): (&[u8], [f32; 4])) -> f64 {
    let w = (a[2] - a[0]).min(b[2] - b[0]).floor() as i64 - 4;
    let h = (a[3] - a[1]).min(b[3] - b[1]).floor() as i64 - 4;
    let (ax, ay, bx, by) = (
        a[0].round() as i64 + 2,
        a[1].round() as i64 + 2,
        b[0].round() as i64 + 2,
        b[1].round() as i64 + 2,
    );
    let mut sum = 0u64;
    let mut n = 0u64;
    for y in 0..h.max(0) {
        for x in 0..w.max(0) {
            let at = |px: i64, py: i64| ((py * i64::from(size[0]) + px) * 4) as usize;
            let (i, j) = (at(ax + x, ay + y), at(bx + x, by + y));
            if i + 3 > pa.len() || j + 3 > pb.len() {
                continue;
            }
            for k in 0..3 {
                sum += u64::from(pa[i + k].abs_diff(pb[j + k]));
            }
            n += 3;
        }
    }
    sum as f64 / n.max(1) as f64
}

/// Round TW4: animated emotes move, every copy of one on the same frame,
/// held still under reduced motion. Round TW5: the device's style is the
/// style everywhere (a stage with opponents' fields keeps Across), the
/// emotes go behind the player's field and behind the miniatures, and a
/// phone held upright shows messages too. `DDI_OFFSCREEN_PPM=<dir>` keeps
/// the pictures.
#[test]
fn animated_emotes_move_and_the_chosen_style_holds_everywhere() {
    // A practice stage of eight CPUs, Across chosen: Across, and two
    // pictures a quarter of a second apart.
    let eight = Scenario {
        cpus: Some(8),
        style: Style::Across,
        level: Level::Emotes,
        script: "emotes",
        shots: vec![6.0, 6.25],
        ..Scenario::new("tw5-eight-animated")
    };
    let Some(p) = play(&eight) else {
        return;
    };
    assert_eq!(p.shown, [(Level::Emotes, Style::Across)], "{}", eight.name);
    assert!(p.behind_field && p.behind_miniature, "{}: behind the fields", eight.name);
    assert!(p.anim_ms > 5000, "{}", p.anim_ms);
    let [(_, pa, Some(ca)), (_, pb, Some(cb))] = &p.seen[..] else {
        panic!("{}: two shots with the chat", eight.name)
    };
    assert!(ca.animated.0 >= 2, "animated emotes ready: {:?}", ca.animated);
    // Every copy of an emote on screen shows the same frame (one phase).
    for c in [ca, cb] {
        let mut frame_of = std::collections::HashMap::new();
        for e in &c.emotes_drawn {
            let at = *frame_of.entry(e.first).or_insert(e.cell);
            assert_eq!(at, e.cell, "copies of one emote on different frames");
        }
        let copies = c.emotes_drawn.len() - frame_of.len();
        eprintln!("{} emotes drawn, {copies} of them further copies", c.emotes_drawn.len());
    }
    let (ia, ib) = (by_item(ca), by_item(cb));
    let mut moved = Vec::new();
    let mut kept = Vec::new();
    for (item, ea) in &ia {
        let Some(eb) = ib.get(item) else { continue };
        for (a, b) in ea.iter().zip(eb) {
            if a.animated {
                if a.cell != b.cell {
                    moved.push((a.rect, b.rect));
                }
            } else {
                assert_eq!(a.cell, b.cell, "a still emote's cell stays");
                kept.push((a.rect, b.rect));
            }
        }
    }
    assert!(!moved.is_empty(), "an animated emote shows another frame 0.25 s on");
    let size = eight.size;
    let mean = |pairs: &[([f32; 4], [f32; 4])]| {
        pairs
            .iter()
            .map(|(a, b)| crop_difference(size, (pa, *a), (pb, *b)))
            .sum::<f64>()
            / pairs.len().max(1) as f64
    };
    let (moving, still) = (mean(&moved), mean(&kept));
    eprintln!("animated emotes' pixels changed by {moving:.1} on average, still ones' by {still:.1}");
    assert!(moving > still + 4.0, "moving {moving:.1}, still {still:.1}");

    // Falling chosen in the same stage: Falling, behind the fields too.
    let falling = Scenario {
        cpus: Some(8),
        style: Style::Falling,
        level: Level::Emotes,
        script: "emotes",
        shots: vec![6.0],
        ..Scenario::new("tw5-eight-falling")
    };
    let p = play(&falling).unwrap();
    assert_eq!(p.shown, [(Level::Emotes, Style::Falling)]);
    assert!(p.behind_field && p.behind_miniature, "{}: behind the fields", falling.name);

    // Reduced motion: every emote its first frame, the clock at zero.
    let reduced = Scenario {
        cpus: Some(8),
        level: Level::Emotes,
        script: "emotes",
        reduced: true,
        shots: vec![6.0, 6.25],
        ..Scenario::new("tw5-eight-reduced")
    };
    let p = play(&reduced).unwrap();
    assert_eq!(p.anim_ms, 0);
    let [(_, _, Some(ca)), (_, _, Some(cb))] = &p.seen[..] else {
        panic!("two shots")
    };
    for (item, ea) in by_item(ca) {
        if let Some(eb) = by_item(cb).get(&item) {
            for (a, b) in ea.iter().zip(eb) {
                assert_eq!(a.cell, b.cell, "still under reduced motion");
            }
        }
    }

    // A phone held upright: messages too (round TW5).
    let phone = Scenario {
        size: [390, 844],
        level: Level::Messages,
        script: "demo",
        shots: vec![4.6, 9.5],
        ..Scenario::new("tw5-phone-upright")
    };
    let p = play(&phone).unwrap();
    assert_eq!(p.shown, [(Level::Messages, Style::Across)]);
    assert!(p.texts.iter().any(|t| t.chars().any(char::is_alphabetic)), "{:?}", p.texts);
}

