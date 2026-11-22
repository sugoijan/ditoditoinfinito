//! The live connection on a fake clock against scripted frames (round
//! TW6): every state, Twitch's ten seconds, the watchdog, the reconnect's
//! hand-over, revocations, each close code, the backoff and giving up.

use super::*;

const P: Policy = LIVE_CHAT_V0;

fn welcome(session: &str, keepalive: Option<u32>) -> String {
    json!({
        "metadata": {"message_id": format!("w-{session}"), "message_type": "session_welcome",
                     "message_timestamp": "2026-11-21T10:00:00.0Z"},
        "payload": {"session": {"id": session, "status": "connected",
                     "connected_at": "2026-11-21T10:00:00.0Z",
                     "keepalive_timeout_seconds": keepalive, "reconnect_url": null}}
    })
    .to_string()
}

fn keepalive(n: u32) -> String {
    json!({"metadata": {"message_id": format!("k{n}"), "message_type": "session_keepalive",
        "message_timestamp": "2026-11-21T10:00:00.0Z"}, "payload": {}})
    .to_string()
}

fn chat(id: &str, text: &str) -> String {
    json!({
        "metadata": {"message_id": format!("n-{id}"), "message_type": "notification",
                     "message_timestamp": "2026-11-21T10:00:01.0Z",
                     "subscription_type": "channel.chat.message", "subscription_version": "1"},
        "payload": {"subscription": {"type": "channel.chat.message"},
                    "event": {"broadcaster_user_id": "1", "chatter_user_id": "9",
                              "chatter_user_login": "viewer", "chatter_user_name": "Viewer",
                              "message_id": id, "message_type": "text",
                              "message": {"text": text, "fragments": [{"type": "text", "text": text}]}}}
    })
    .to_string()
}

fn reconnect(url: &str) -> String {
    json!({"metadata": {"message_id": "r1", "message_type": "session_reconnect",
        "message_timestamp": "2026-11-21T10:00:00.0Z"},
        "payload": {"session": {"id": "s1", "status": "reconnecting",
            "keepalive_timeout_seconds": null, "reconnect_url": url,
            "connected_at": "2026-11-21T10:00:00.0Z"}}})
    .to_string()
}

fn revocation(kind: &str, status: &str) -> String {
    json!({"metadata": {"message_id": "v1", "message_type": "revocation",
        "message_timestamp": "2026-11-21T10:00:00.0Z",
        "subscription_type": kind, "subscription_version": "1"},
        "payload": {"subscription": {"type": kind, "status": status}}})
    .to_string()
}

fn sub(kind: &str, outcome: &str) -> Subscription {
    Subscription {
        kind: kind.into(),
        outcome: outcome.into(),
        http: Some(if outcome == "enabled" { 202 } else { 403 }),
        status: (outcome == "enabled").then(|| "enabled".into()),
        cost: Some(0),
        total: Some(1),
        total_cost: Some(0),
        max_total_cost: Some(10),
        ..Subscription::default()
    }
}

fn enabled() -> SubscribeAnswer {
    SubscribeAnswer::Subscribed(
        vec![
            sub("channel.chat.message", "enabled"),
            sub("channel.chat.message_delete", "enabled"),
        ],
        None,
    )
}

fn opens(d: &[Do]) -> Vec<(u32, String)> {
    d.iter()
        .filter_map(|d| match d {
            Do::Open { socket, url } => Some((*socket, url.clone())),
            _ => None,
        })
        .collect()
}

fn closes(d: &[Do]) -> Vec<u32> {
    d.iter()
        .filter_map(|d| match d {
            Do::Close { socket } => Some(*socket),
            _ => None,
        })
        .collect()
}

fn subscribes(d: &[Do]) -> Vec<(u32, String)> {
    d.iter()
        .filter_map(|d| match d {
            Do::Subscribe { socket, session_id } => Some((*socket, session_id.clone())),
            _ => None,
        })
        .collect()
}

/// A connection live on socket 1 at time 1000 + 300.
fn live() -> Live {
    let mut l = Live::new(P, "wss://eventsub.test/ws");
    l.start(1_000);
    assert_eq!(opens(&l.take_do()), [(1, "wss://eventsub.test/ws".to_string())]);
    l.opened(1, 1_100);
    l.text(1, &welcome("s1", Some(10)), 1_200);
    assert_eq!(subscribes(&l.take_do()), [(1, "s1".to_string())]);
    l.subscribed(1, enabled(), 1_300);
    assert_eq!(l.state(), "live");
    l
}

#[test]
fn a_welcome_is_subscribed_and_chat_comes() {
    let mut l = Live::new(P, "wss://eventsub.test/ws");
    assert_eq!(l.state(), "idle");
    l.start(1_000);
    assert_eq!(l.state(), "connecting");
    l.take_do();
    l.opened(1, 1_100);
    l.text(1, &welcome("s1", Some(10)), 1_200);
    assert_eq!(l.state(), "subscribing");
    l.take_do();
    l.subscribed(1, enabled(), 1_350);
    assert_eq!(l.state(), "live");
    l.text(1, &chat("m1", "hello"), 2_000);
    l.text(1, &keepalive(1), 5_000);
    let events = l.take_events();
    assert!(matches!(&events[..], [Event::Message(m)] if m.text == "hello"));
    let r = l.report();
    assert_eq!(r["welcome_ms"], 100);
    assert_eq!(r["subscribed_after_welcome_ms"], 150);
    assert_eq!(r["keepalive_s"], 10);
    assert_eq!(r["subscriptions"][0]["cost"], 0);
    assert_eq!(r["subscriptions"][0]["max_total_cost"], 10);
    assert_eq!(r["received"]["keepalive"], 1);
    assert_eq!(r["events"]["message"], 1);
    let lines = l.take_lines();
    assert!(lines.iter().any(|x| x.contains("welcomed in 200 ms (keepalive 10 s")), "{lines:?}");
    assert!(lines.iter().any(|x| x.contains("subscribed channel.chat.message: enabled (HTTP 202")));
    // Twitch's totals are each call's own (round LO), never one in all.
    let subscribed: Vec<&String> = lines.iter().filter(|x| x.contains("subscribed channel.chat.")).collect();
    assert_eq!(subscribed.len(), 2, "{lines:?}");
    assert!(
        subscribed
            .iter()
            .all(|x| x.contains("total 1 at this call, total cost 0 of 10")),
        "{subscribed:?}"
    );
    // Nothing of the chat in the lines.
    assert!(!lines.iter().any(|x| x.contains("hello")));
}

/// Twitch's ten seconds: a subscribe that failed in passing is asked
/// again while four of them are left; past them, or a second failure,
/// the socket is lost and opened again.
#[test]
fn the_ten_seconds_after_the_welcome() {
    let mut l = Live::new(P, "wss://e.test/ws");
    l.start(0);
    l.take_do();
    l.opened(1, 50);
    l.text(1, &welcome("s1", Some(10)), 100);
    l.take_do();
    l.subscribed(1, SubscribeAnswer::Failed("twitch_unreachable".into()), 2_100);
    l.tick(2_500);
    assert!(subscribes(&l.take_do()).is_empty(), "a second later");
    l.tick(3_100);
    assert_eq!(subscribes(&l.take_do()), [(1, "s1".to_string())]);
    l.subscribed(1, SubscribeAnswer::Failed("twitch_unreachable".into()), 4_000);
    let d = l.take_do();
    assert_eq!(closes(&d), [1], "lost after the second failure");
    assert_eq!(l.state(), "waiting");
    // The first loss opens again at once.
    l.tick(4_000);
    assert_eq!(opens(&l.take_do()), [(2, "wss://e.test/ws".to_string())]);

    // A first failure too late in the window: lost at once.
    let mut l = Live::new(P, "wss://e.test/ws");
    l.start(0);
    l.take_do();
    l.text(1, &welcome("s1", Some(10)), 100);
    l.take_do();
    l.subscribed(1, SubscribeAnswer::Failed("no answer".into()), 6_500);
    assert_eq!(closes(&l.take_do()), [1]);

    // No answer at all: lost past the window (and Twitch's 4003 would
    // have closed it anyway).
    let mut l = Live::new(P, "wss://e.test/ws");
    l.start(0);
    l.take_do();
    l.text(1, &welcome("s1", Some(10)), 100);
    l.take_do();
    l.tick(9_000);
    assert!(l.take_do().is_empty());
    l.text(1, &keepalive(1), 9_000);
    l.tick(15_200);
    assert_eq!(closes(&l.take_do()), [1]);
    assert!(l.take_lines().iter().any(|x| x.contains("not subscribed within")));
}

/// A socket that never welcomes is lost after the wait.
#[test]
fn no_welcome_is_a_loss() {
    let mut l = Live::new(P, "wss://e.test/ws");
    l.start(0);
    l.take_do();
    l.opened(1, 100);
    l.tick(9_999);
    assert!(l.take_do().is_empty());
    l.tick(10_000);
    assert_eq!(closes(&l.take_do()), [1]);
    assert_eq!(l.phase(), &Phase::Waiting { until: 10_000 });
}

/// The watchdog: silence past the keep-alive Twitch stated and the margin
/// is a loss; keep-alives and notifications reset it; a longer keep-alive
/// asked by the welcome is honored.
#[test]
fn the_watchdog_on_silence() {
    let mut l = live();
    l.text(1, &keepalive(1), 10_000);
    l.tick(22_999);
    assert!(l.take_do().is_empty(), "10 s and the 3 s margin");
    l.text(1, &chat("a", "x"), 22_999);
    l.tick(35_000);
    assert!(l.take_do().is_empty());
    l.tick(36_000);
    assert_eq!(closes(&l.take_do()), [1]);
    assert!(l.take_lines().iter().any(|x| x.contains("past the keepalive of 10 s")));
    // A stated keepalive of 30 s.
    let mut l = Live::new(P, "wss://e.test/ws");
    l.start(0);
    l.take_do();
    l.text(1, &welcome("s", Some(30)), 0);
    l.take_do();
    l.subscribed(1, enabled(), 100);
    l.tick(32_000);
    assert!(l.take_do().is_empty());
    l.tick(33_001);
    assert_eq!(closes(&l.take_do()), [1]);
    // A welcome without one: Twitch's default (10 s), and its shape kept.
    let mut l = Live::new(P, "wss://e.test/ws");
    l.start(0);
    l.take_do();
    l.text(1, &welcome("s", None), 0);
    assert_eq!(l.report()["not_understood"]["count"], 1);
    assert_eq!(l.report()["keepalive_s"], 10);
}

/// Twitch's reconnect: the new address opened, the old socket kept (its
/// events taken, once each) until the new one welcomes, then closed;
/// nothing subscribed again.
#[test]
fn a_reconnect_hands_over() {
    let mut l = live();
    l.text(1, &reconnect("wss://eventsub.test/ws?id=xyz"), 5_000);
    let d = l.take_do();
    assert_eq!(opens(&d), [(2, "wss://eventsub.test/ws?id=xyz".to_string())]);
    assert!(closes(&d).is_empty(), "the old socket is kept");
    assert_eq!(l.state(), "reconnecting");
    assert!(l.is_live());
    // Events on both meanwhile, the same one once.
    l.text(1, &chat("a", "one"), 5_100);
    l.opened(2, 5_200);
    l.text(2, &chat("a", "one"), 5_250);
    l.text(2, &welcome("s2", Some(10)), 5_300);
    let d = l.take_do();
    assert_eq!(closes(&d), [1]);
    assert!(subscribes(&d).is_empty(), "carried over");
    assert_eq!(l.state(), "live");
    assert_eq!(l.take_events().len(), 1);
    let r = l.report();
    assert_eq!(r["received"]["duplicate"], 1);
    assert_eq!(r["reconnects"]["done"], 1);
    // The reconnect's query is never reported.
    assert!(!r.to_string().contains("xyz"));
    // Its own keep-alive watched from now on.
    l.tick(18_300);
    assert!(l.take_do().is_empty());
    l.tick(18_301);
    assert_eq!(closes(&l.take_do()), [2]);
}

/// The old socket closed (4004 or earlier) during the hand-over: the new
/// one goes on; a new socket that never welcomes is a loss, opened again
/// at the first address and subscribed again.
#[test]
fn a_reconnect_that_fails() {
    let mut l = live();
    l.text(1, &reconnect("wss://eventsub.test/ws?id=2"), 5_000);
    l.take_do();
    l.closed(1, 4004, 6_000);
    assert_eq!(l.state(), "connecting");
    l.text(2, &welcome("s2", Some(10)), 7_000);
    assert_eq!(l.state(), "live");
    assert!(subscribes(&l.take_do()).is_empty());

    let mut l = live();
    l.text(1, &reconnect("wss://eventsub.test/ws?id=2"), 5_000);
    l.take_do();
    l.text(1, &keepalive(1), 20_000);
    l.tick(35_000);
    let d = l.take_do();
    assert_eq!(closes(&d), [2], "the new socket given up, the old kept");
    assert_eq!(l.state(), "live");
    l.closed(1, 4004, 35_100);
    assert_eq!(l.state(), "waiting");
    l.tick(35_100);
    assert_eq!(opens(&l.take_do()), [(3, "wss://eventsub.test/ws".to_string())]);
    l.text(3, &welcome("s3", Some(10)), 35_500);
    assert_eq!(subscribes(&l.take_do()), [(3, "s3".to_string())]);
    // A reconnect message without a usable address: noted, waited out.
    let mut l = live();
    l.text(1, &reconnect("http://nope"), 5_000);
    assert!(l.take_do().is_empty());
    assert_eq!(l.report()["not_understood"]["count"], 1);
}

/// Review 2026-11-22 (chat-touch, a suspicion): a reconnect is followed
/// only to Twitch's own sockets (`wss://…twitch.tv`) or to the host the
/// connection was opened at (a test's local fake); any other address is
/// noted and waited out, never opened.
#[test]
fn a_reconnect_goes_only_to_twitch_or_the_first_host() {
    let mut l = Live::new(P, EVENTSUB_URL);
    l.start(1_000);
    l.take_do();
    l.opened(1, 1_100);
    l.text(1, &welcome("s1", Some(10)), 1_200);
    l.subscribed(1, enabled(), 1_300);
    l.take_do();
    for bad in [
        "wss://evil.example/ws",
        "ws://eventsub.wss.twitch.tv/ws",
        "wss://twitch.tv.evil.example/ws",
        "wss://eventsub.wss.twitch.tv@evil.example/ws",
        "wss://evil.example#.twitch.tv/ws",
        "wss://evil.example\\.twitch.tv/ws",
        "wss://eviltwitch.tv/ws",
    ] {
        l.text(1, &reconnect(bad), 5_000);
        assert!(opens(&l.take_do()).is_empty(), "{bad}");
        assert_eq!(l.state(), "live", "{bad}");
    }
    l.text(1, &reconnect("wss://eventsub.wss.twitch.tv/ws?id=abc"), 6_000);
    assert_eq!(opens(&l.take_do()).len(), 1, "Twitch's own");
    // A local fake's own host (its port too).
    let mut l = Live::new(P, "ws://127.0.0.1:8314/normal");
    l.start(1_000);
    l.take_do();
    l.opened(1, 1_100);
    l.text(1, &welcome("s1", Some(10)), 1_200);
    l.subscribed(1, enabled(), 1_300);
    l.take_do();
    l.text(1, &reconnect("ws://127.0.0.1:9999/ws"), 5_000);
    assert!(opens(&l.take_do()).is_empty(), "another port");
    l.text(1, &reconnect("ws://127.0.0.1:8314/ws?scenario=normal&reconnected=1"), 5_100);
    assert_eq!(opens(&l.take_do()).len(), 1);
}

/// Each close code: 4001 (we never send) gives up; the others are losses
/// opened again with the backoff; a stop is quiet.
#[test]
fn every_close_code() {
    for code in [4000, 4002, 4003, 4004, 4005, 4006, 4007, 1000, 1001, 1005, 1006] {
        let mut l = live();
        l.closed(1, code, 2_000);
        assert_eq!(l.phase(), &Phase::Waiting { until: 2_000 }, "{code}");
        let r = l.report();
        assert_eq!(r["closes"][0]["code"], code);
        assert_eq!(r["closes"][0]["word"], close_word(code));
    }
    let mut l = live();
    l.closed(1, 4001, 2_000);
    assert!(matches!(l.phase(), Phase::GaveUp(w) if w.contains("4001")));
    assert_ne!(close_word(4003), close_word(4007));
    // Stopped: a later close is nothing.
    let mut l = live();
    l.stop(3_000);
    assert_eq!(closes(&l.take_do()), [1]);
    l.closed(1, 1000, 3_100);
    assert_eq!(l.phase(), &Phase::Stopped);
    l.tick(100_000);
    assert!(l.take_do().is_empty());
}

/// Losses in a row wait longer each time, and past eight it gives up
/// quietly; a socket that went live clears the count.
#[test]
fn backoff_and_giving_up() {
    let mut l = Live::new(P, "wss://e.test/ws");
    let mut now = 0;
    l.start(now);
    let mut waits = Vec::new();
    for n in 1..=8u32 {
        let socket = opens(&l.take_do())[0].0;
        assert_eq!(socket, n);
        l.closed(socket, 1006, now);
        match l.phase().clone() {
            Phase::Waiting { until } => {
                waits.push(until - now);
                now = until;
                l.tick(now);
            }
            Phase::GaveUp(why) => {
                assert!(why.starts_with("8 losses in a row"), "{why}");
                break;
            }
            p => panic!("{p:?}"),
        }
    }
    assert_eq!(waits, [0, 1_000, 2_000, 5_000, 10_000, 30_000, 60_000]);
    assert!(l.take_do().is_empty(), "nothing after giving up");
    // Live in between: the count starts again.
    let mut l = live();
    for _ in 0..3 {
        l.closed(l.conns[0].id, 1006, 2_000);
        l.tick(100_000);
        let s = opens(&l.take_do())[0].0;
        l.text(s, &welcome("s", Some(10)), 100_000);
        l.subscribed(s, enabled(), 100_000);
    }
    assert_eq!(l.losses, 0);
}

/// Refusals that would repeat give up at once; Twitch not knowing the
/// session is a loss; a revocation of the chat's subscription ends it,
/// of another type is noted.
#[test]
fn refusals_and_revocations() {
    for answer in [
        SubscribeAnswer::Refused("scope_missing".into()),
        SubscribeAnswer::Refused("chat_off".into()),
        SubscribeAnswer::Subscribed(vec![sub("channel.chat.message", "forbidden")], None),
        SubscribeAnswer::Subscribed(vec![sub("channel.chat.message", "limits")], None),
        SubscribeAnswer::Subscribed(vec![sub("channel.chat.clear", "enabled")], None),
    ] {
        let mut l = Live::new(P, "wss://e.test/ws");
        l.start(0);
        l.take_do();
        l.text(1, &welcome("s", Some(10)), 0);
        l.take_do();
        l.subscribed(1, answer.clone(), 500);
        assert!(matches!(l.phase(), Phase::GaveUp(_)), "{answer:?}");
        assert_eq!(closes(&l.take_do()), [1]);
    }
    let mut l = Live::new(P, "wss://e.test/ws");
    l.start(0);
    l.take_do();
    l.text(1, &welcome("s", Some(10)), 0);
    l.take_do();
    l.subscribed(
        1,
        SubscribeAnswer::Subscribed(vec![sub("channel.chat.message", "bad_session")], None),
        500,
    );
    assert_eq!(l.state(), "waiting");
    // Only the messages' subscription decides: a moderation type refused
    // is reported, the chat comes.
    let mut l = Live::new(P, "wss://e.test/ws");
    l.start(0);
    l.take_do();
    l.text(1, &welcome("s", Some(10)), 0);
    l.take_do();
    l.subscribed(
        1,
        SubscribeAnswer::Subscribed(
            vec![
                sub("channel.chat.message", "enabled"),
                sub("channel.chat.clear", "forbidden"),
            ],
            None,
        ),
        500,
    );
    assert_eq!(l.state(), "live");
    assert_eq!(l.report()["subscriptions"][1]["outcome"], "forbidden");

    let mut l = live();
    l.text(1, &revocation("channel.chat.clear", "authorization_revoked"), 3_000);
    assert_eq!(l.state(), "live");
    l.text(1, &revocation("channel.chat.message", "user_removed"), 3_100);
    assert!(matches!(l.phase(), Phase::GaveUp(w) if w.contains("user_removed")));
    assert_eq!(l.report()["revocations"][1]["status"], "user_removed");
}

/// Round TW7: why the chat could not show, for the player's one line
/// after the stage: each refusal, Twitch's answers and revocations read
/// as their trouble; the room's own changes, a chat that showed and one
/// stopped while connecting say nothing; a connection that never went
/// live and kept failing is unreachable.
#[test]
fn each_failure_is_one_trouble_for_the_player() {
    let after = |answer: SubscribeAnswer| {
        let mut l = Live::new(P, "wss://e.test/ws");
        l.start(0);
        l.take_do();
        l.text(1, &welcome("s", Some(10)), 0);
        l.take_do();
        l.subscribed(1, answer, 500);
        l.trouble()
    };
    let refused = |w: &str| after(SubscribeAnswer::Refused(w.into()));
    let twitch = |o: &str| after(SubscribeAnswer::Subscribed(vec![sub("channel.chat.message", o)], None));
    assert_eq!(refused("scope_missing"), Some(Trouble::Access));
    assert_eq!(refused("twitch_refused"), Some(Trouble::Access));
    assert_eq!(refused("no_such_channel"), Some(Trouble::ChannelGone));
    assert_eq!(refused("channel_not_allowed"), Some(Trouble::NotAllowed));
    assert_eq!(refused("chat_off"), None, "the room's own change");
    assert_eq!(refused("not_a_member"), None);
    assert_eq!(twitch("refused"), Some(Trouble::Access));
    assert_eq!(twitch("forbidden"), Some(Trouble::Refused));
    assert_eq!(twitch("limits"), Some(Trouble::Refused));
    assert_eq!(twitch("enabled"), None, "it showed");
    for (status, trouble) in [
        ("authorization_revoked", Trouble::Access),
        ("user_removed", Trouble::ChannelGone),
        ("moderator_removed", Trouble::Refused),
    ] {
        let mut l = live();
        l.text(1, &revocation("channel.chat.message", status), 3_000);
        assert_eq!(l.trouble(), Some(trouble), "{status}");
        assert_eq!(l.report()["trouble"], trouble.word());
    }
    // Never live, lost again and again: unreachable, before and after it
    // gives up.
    let mut l = Live::new(P, "wss://e.test/ws");
    l.start(0);
    l.take_do();
    assert_eq!(l.trouble(), None, "connecting");
    l.closed(1, 1006, 100);
    assert_eq!(l.trouble(), Some(Trouble::Unreachable));
    let mut now = 100;
    while !matches!(l.phase(), Phase::GaveUp(_)) {
        now += 61_000;
        l.tick(now);
        let socket = opens(&l.take_do()).last().map(|o| o.0).unwrap();
        l.closed(socket, 1006, now + 10);
    }
    assert_eq!(l.trouble(), Some(Trouble::Unreachable));
    // Stopped while connecting: nothing to say.
    let mut l = Live::new(P, "wss://e.test/ws");
    l.start(0);
    l.stop(50);
    assert_eq!(l.trouble(), None);
}

/// Round TW7: our server's answer names the channel read (a named one as
/// Twitch found it): kept, said once in the report.
#[test]
fn the_channel_read_is_named_by_the_answer() {
    let body = r#"{"channel":"busy","channel_id":"141981764","subscriptions":[{"type":"channel.chat.message","outcome":"enabled","http":202}]}"#;
    let answer = read_subscribe_answer(200, body);
    let SubscribeAnswer::Subscribed(_, Some(c)) = &answer else {
        panic!("{answer:?}")
    };
    assert_eq!((&*c.id, &*c.login), ("141981764", "busy"));
    let older = r#"{"channel":"busy","subscriptions":[]}"#;
    assert!(matches!(read_subscribe_answer(200, older), SubscribeAnswer::Subscribed(_, None)));
    let mut l = Live::new(P, "wss://e.test/ws");
    l.start(0);
    l.take_do();
    l.text(1, &welcome("s", Some(10)), 0);
    l.take_do();
    l.take_lines();
    l.subscribed(1, answer, 300);
    assert_eq!(l.channel().map(|c| c.id.as_str()), Some("141981764"));
    assert_eq!(l.report()["channel"]["login"], "busy");
    assert!(l.take_lines().iter().any(|t| t.ends_with("channel busy is Twitch user 141981764")));
    assert_eq!(l.state(), "live");
}

/// What the readers did not understand is kept as its shape, a few, never
/// a person's words.
#[test]
fn what_is_not_understood_is_kept_as_its_shape() {
    let mut l = live();
    l.text(1, r#"{"metadata":{"message_type":"session_disconnect"},"payload":{"x":"secret words"}}"#, 2_000);
    l.text(1, "not json at all", 2_100);
    let mut odd: Value = serde_json::from_str(&chat("m2", "visible words")).unwrap();
    odd["payload"]["event"]["message"]["fragments"] =
        json!([{"type": "gif", "text": "visible words", "gif": {"id": "g1"}}]);
    let odd = odd.to_string();
    l.text(1, &odd, 2_200);
    for k in 0..10 {
        l.text(1, &format!(r#"{{"metadata":{{"message_type":"new_{k}"}}}}"#), 3_000);
    }
    let r = l.report();
    assert_eq!(r["not_understood"]["count"], 13);
    let samples = r["not_understood"]["samples"].as_array().unwrap();
    assert_eq!(samples.len(), SAMPLES);
    let text = r.to_string();
    assert!(!text.contains("secret words") && !text.contains("visible words"), "{text}");
    assert_eq!(samples[0]["shape"]["metadata"]["message_type"], "session_disconnect");
    assert_eq!(samples[0]["shape"]["payload"]["x"], "<text 12>");
    assert_eq!(samples[2]["why"], "a fragment of another kind");
    assert_eq!(
        samples[2]["shape"]["payload"]["event"]["message"]["fragments"][0]["type"],
        "gif"
    );
    assert_eq!(r["received"]["unreadable"], 1);
    // The message itself still comes.
    assert_eq!(l.take_events().len(), 1);
}

#[test]
fn our_servers_answers_are_read() {
    let body = r#"{"channel":"host","subscriptions":[
        {"type":"channel.chat.message","outcome":"enabled","http":202,"status":"enabled",
         "cost":0,"total":1,"total_cost":0,"max_total_cost":10,"ms":120},
        {"type":"channel.chat.clear","outcome":"forbidden","http":403,
         "error":"subscription missing proper authorization"}]}"#;
    let SubscribeAnswer::Subscribed(list, _) = read_subscribe_answer(200, body) else {
        panic!()
    };
    assert_eq!(list[0].cost, Some(0));
    assert_eq!(list[0].max_total_cost, Some(10));
    assert_eq!(list[0].ms, Some(120));
    assert_eq!(list[1].error.as_deref(), Some("subscription missing proper authorization"));
    assert_eq!(
        read_subscribe_answer(409, r#"{"error":"scope_missing","need":["chat"]}"#),
        SubscribeAnswer::Refused("scope_missing".into())
    );
    assert_eq!(
        read_subscribe_answer(403, r#"{"error":"not_a_member"}"#),
        SubscribeAnswer::Refused("not_a_member".into())
    );
    assert_eq!(
        read_subscribe_answer(502, r#"{"error":"twitch_unreachable"}"#),
        SubscribeAnswer::Failed("twitch_unreachable".into())
    );
    assert_eq!(read_subscribe_answer(429, "{}"), SubscribeAnswer::Failed("HTTP 429".into()));
    assert_eq!(read_subscribe_answer(0, ""), SubscribeAnswer::Failed("no answer".into()));
    assert!(matches!(read_subscribe_answer(200, "{}"), SubscribeAnswer::Failed(_)));
}

#[test]
fn hosts_alone_are_reported() {
    assert_eq!(host_of("wss://eventsub.wss.twitch.tv/ws?keepalive_timeout_seconds=30"), "eventsub.wss.twitch.tv");
    assert_eq!(host_of("ws://127.0.0.1:9/x"), "127.0.0.1:9");
}
