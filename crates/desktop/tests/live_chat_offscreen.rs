//! The live chat (round TW6, `docs/plans/multiplayer-twitch.md`) through
//! desktop's play host offscreen, on a fake clock with no sound device,
//! against local fakes of Twitch's EventSub socket and of our server's
//! subscribe route (never Twitch, never a provider): the development
//! script's own channel in CPU practice's stage, the socket welcomed,
//! subscribed, its chat drawn with a Twitch emote from a fragment, the
//! play judging as without it; and a solo play under the same script and
//! options showing none and asking nothing (round TW7). Skipped where there is no
//! GPU adapter (CI) for the pictures; the connection is checked anyway.

mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ddi_desktop::gpu;
use ddi_desktop::play::Request;
use ddi_desktop::play::host::{Device, Host};
use ddi_room::CpuMode;
use ddi_ui::screens::data::Difficulty;
use ddi_ui::state::settings::Settings;

/// A local EventSub fake: each connection welcomes and chats a message
/// with a Twitch emote every 100 ms, until the client closes it.
fn fake_eventsub() -> String {
    use tungstenite::Message;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for (n, stream) in listener.incoming().enumerate() {
            let Ok(stream) = stream else { return };
            std::thread::spawn(move || {
                let Ok(mut ws) = tungstenite::accept(stream) else { return };
                let _ = ws.get_mut().set_read_timeout(Some(Duration::from_millis(100)));
                let welcome = format!(
                    r#"{{"metadata":{{"message_id":"w{n}","message_type":"session_welcome"}},"payload":{{"session":{{"id":"s{n}","status":"connected","keepalive_timeout_seconds":10}}}}}}"#
                );
                if ws.send(Message::text(welcome)).is_err() {
                    return;
                }
                for k in 0.. {
                    let chat = format!(
                        r#"{{"metadata":{{"message_id":"n{n}-{k}","message_type":"notification","subscription_type":"channel.chat.message"}},"payload":{{"event":{{"broadcaster_user_id":"1001","chatter_user_id":"c{k}","chatter_user_login":"v","chatter_user_name":"V","message_id":"m{n}-{k}","message_type":"text","message":{{"text":"live {k} Kappa","fragments":[{{"type":"text","text":"live {k} "}},{{"type":"emote","text":"Kappa","emote":{{"id":"25","format":["static"]}}}}]}}}}}}}}"#
                    );
                    if ws.send(Message::text(chat)).is_err() {
                        return;
                    }
                    match ws.read() {
                        Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => return,
                        Err(tungstenite::Error::Io(_)) | Ok(_) => {}
                        Err(_) => return,
                    }
                }
            });
        }
    });
    format!("ws://127.0.0.1:{port}/ws")
}

/// A local fake of our server's subscribe route (every subscribe enabled,
/// what was asked kept) and of the providers' hosts under `/g/` (the
/// channel's sets unknown, Twitch's emote `25` a red PNG).
fn fake_http() -> (String, Arc<Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let log = asked.clone();
    let png = ddi_desktop::chat::made::png([28, 28], [220u8, 40, 40, 255].repeat(28 * 28));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            let _ = reader.read_line(&mut first);
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let path = first.split(' ').nth(1).unwrap_or("").to_string();
            log.lock().unwrap().push(path.clone());
            let (status, kind, answer): (u16, &str, Vec<u8>) = if path.ends_with("/chat/subscribe") {
                (200, "application/json", br#"{"channel":"me","subscriptions":[{"type":"channel.chat.message","outcome":"enabled","http":202,"status":"enabled","cost":0,"total":1,"total_cost":0,"max_total_cost":10,"ms":2}]}"#.to_vec())
            } else if path.contains("/emoticons/v2/25/") {
                (200, "image/png", png.clone())
            } else {
                (404, "application/json", b"{}".to_vec())
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                answer.len()
            );
            let _ = stream.write_all(&answer);
        }
    });
    (base, asked)
}

struct Played {
    items: usize,
    emotes: usize,
    texts: Vec<String>,
    live: Option<serde_json::Value>,
    tally: String,
    shot: Option<Vec<u8>>,
}

/// Beatucada's Challenge autoplayed with the live chat on (or off), alone
/// or in CPU practice's stage: what the chat drew and the tally.
fn play(chat: bool, stage: bool) -> Option<Played> {
    common::indexes();
    let adapter = common::gpu::adapter_or_skip()?;
    let size = [1280, 720];
    let (device, queue) = gpu::device(&adapter).unwrap();
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let mut settings = Settings {
        twitch_chat: if chat { ddi_chat::Level::Messages } else { ddi_chat::Level::Off },
        ..Settings::default()
    };
    settings.dancing_hand = false;
    settings.save();
    let socket = fake_eventsub();
    let (http, asked) = fake_http();
    ddi_session::chat::set_scripted(None);
    ddi_session::chat::set_live(Some(ddi_desktop::chat::live_setup_with(
        socket,
        http.clone(),
        Some(format!("{http}/g/")),
        || None,
    )));
    ddi_ui::state::twitch::set_dev_live(Some(ddi_ui::state::twitch::DevLive::Own {
        channel_id: "1001".into(),
        login: "me".into(),
    }));
    let (mut ui, env) = common::ui::theme(if stage { "start" } else { "select" }, common::ui::lowercase(), size, false);
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
    let wait = |host: &mut Host, ui: &mut ddi_ui::Ui| {
        for _ in 0..1500 {
            host.poll(ui, &env, Some(dev()), 48_000);
            ui.update(0.05);
            if host.prompt_ready() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the song was never read");
    };
    if stage {
        ddi_desktop::room::develop(0);
        let room = common::practice::open(&mut ui, &env, &host);
        let entry = common::practice::song_entry("beatucada", "Beatucada", 0, Difficulty::Challenge);
        {
            let mut r = room.borrow_mut();
            let mut rules = ddi_room::MatchRules {
                stages: 1,
                ..r.rules().clone()
            };
            rules.seats = rules.seats.max(3);
            rules.cpu.mode = CpuMode::Fixed { count: 2 };
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
        wait(&mut host, &mut ui);
        common::rig::ready_and_start(&mut host, &mut ui, &env, &room, dev);
    } else {
        *host.requested.borrow_mut() = Some(Request::song("beatucada", 0, true));
        wait(&mut host, &mut ui);
        ui.update(0.5);
        common::ui::key(&mut ui, "Enter", "Enter");
        ui.update(0.05);
        host.poll(&mut ui, &env, Some(dev()), 48_000);
    }
    assert!(host.playing());
    let mut played = Played {
        items: 0,
        emotes: 0,
        texts: Vec::new(),
        live: None,
        tally: String::new(),
        shot: None,
    };
    std::thread::sleep(Duration::from_millis(400));
    let mut frames = 0;
    while host.playing() {
        let step = (1e9 / 60.0) as u64;
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
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
        queue.submit(Some(encoder.finish()));
        if frames % 30 == 0 {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
        // The sockets and the route are real time: give them some.
        if frames % 4 == 0 {
            std::thread::sleep(Duration::from_millis(4));
        }
        if let Some(c) = host.chat_drawn() {
            played.items = played.items.max(c.items);
            played.emotes = played.emotes.max(c.emotes);
            for t in &c.texts {
                if !played.texts.contains(t) && played.texts.len() < 40 {
                    played.texts.push(t.clone());
                }
            }
            if played.shot.is_none() && c.emotes > 3 && frames > 600 {
                let _ = device.poll(wgpu::PollType::wait_indefinitely());
                let pixels = gpu::read_rgba(&device, &queue, &target.texture);
                common::shot(&format!("live-chat-{}.png", if stage { "stage" } else { "solo" }), &pixels, size);
                played.shot = Some(pixels);
            }
        }
        if played.live.is_none()
            && let Some(p) = host.running()
            && let Some(r) = p.session.chat().and_then(|c| c.live_report())
            && r["connection"]["state"] == "live"
        {
            played.live = Some(r);
        }
        frames += 1;
        if frames > 60 * 25 {
            break;
        }
    }
    if let Some(p) = host.running() {
        played.tally = format!("{:?}", p.session.player.tally());
    }
    let asked = asked.lock().unwrap().clone();
    if chat && stage {
        assert!(asked.iter().any(|a| a == "/me/chat/subscribe"), "{asked:?}");
    } else {
        assert!(asked.is_empty(), "nothing asked with the option off or alone: {asked:?}");
    }
    ddi_ui::state::twitch::set_dev_live(None);
    ddi_session::chat::set_live(None);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    Some(played)
}

/// The development script's own channel in CPU practice's stage:
/// subscribed through the own route, its messages and Twitch's emote
/// drawn; with the option off nothing is asked or drawn, and the first 25
/// seconds judge alike. A solo play under the same script with the
/// option on shows none and asks nothing (round TW7: never in
/// singleplayer).
#[test]
fn the_live_chat_plays_in_a_practice_stage_and_never_alone() {
    let Some(on) = play(true, true) else { return };
    let off = play(false, true).unwrap();
    let live = on.live.as_ref().expect("live");
    assert_eq!(live["route"], "own");
    assert_eq!(live["connection"]["subscriptions"][0]["outcome"], "enabled");
    assert!(on.items > 0 && on.emotes > 0, "{} items, {} emotes", on.items, on.emotes);
    assert!(on.texts.iter().any(|t| t.contains("live") && t.contains('◆')), "{:?}", on.texts);
    assert_eq!(off.items, 0);
    assert!(off.live.is_none());
    assert_eq!(on.tally, off.tally, "judged alike");
    let alone = play(true, false).unwrap();
    assert_eq!(alone.items, 0);
    assert!(alone.live.is_none());
}
