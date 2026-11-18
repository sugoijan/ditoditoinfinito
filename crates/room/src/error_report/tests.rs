use super::*;

/// Messages a panic or a rejection could carry, each with what must not
/// survive the scrub (a user's name in a path, a token in an address, an
/// email address, a room's or an invite's id, a long number, a song's
/// title, a person's name in quotes, an address), and what the scrub keeps.
const TABLE: &[(&str, &[&str], &str)] = &[
    (
        "panicked at /Users/kirarin/Songs/Lumen/chart.sm:12",
        &["kirarin", "Songs", "Lumen", "chart.sm"],
        "panicked at <path>",
    ),
    (
        "panicked at crates/ui/src/state/room.rs:120:9:\nno chart for stage 2",
        &[],
        "panicked at ui/src/state/room.rs:120:9: no chart for stage 2",
    ),
    (
        "panicked at /home/runner/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/wgpu-26.0.1/src/backend/webgpu.rs:44:5:\nDevice lost",
        &["runner", "1949cf8c", "index.crates"],
        "panicked at wgpu-26.0.1/src/backend/webgpu.rs:44:5: Device lost",
    ),
    (
        "panicked at /Users/jan/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/wgpu-26.0.1/src/lib.rs:7:1: x",
        &["jan", "Users"],
        "panicked at wgpu-26.0.1/src/lib.rs:7:1: x",
    ),
    (
        "panicked at /rustc/17067e9ac6d7e98f3e0e2b8dd5b1e7a0b8e1a3b1/library/core/src/panicking.rs:75:14: oops",
        &["17067e9a"],
        "panicked at core/src/panicking.rs:75:14: oops",
    ),
    (
        "could not read /Users/kirarin/src/game.rs:3",
        &["kirarin"],
        "could not read <path>",
    ),
    (
        "open C:\\Users\\Kirarin\\Music\\song.ogg failed",
        &["Kirarin", "Music", "song"],
        "open <path> failed",
    ),
    (
        "fetch https://sugoijan.dev/ditoditoinfinito/api/rooms/Room0Lumen/ws?ticket=abc.def failed",
        &["sugoijan", "Room0Lumen", "ticket", "abc.def"],
        "fetch <url> failed",
    ),
    (
        "socket api/sockets/Room0Lumen/ws?ticket=eyJ0eXAi closed",
        &["Room0Lumen", "eyJ0"],
        "socket <url> closed",
    ),
    (
        "send to me@sugoijan.dev or kirarin@example.com",
        &["me@", "sugoijan", "kirarin", "example"],
        "send to <email> or <email>",
    ),
    (
        "room zsuvRbEvax not found; Room0Lumen; invite Inv1teSecretXyz9Inv1teSe, share Shr12345",
        &["zsuvRbEvax", "Room0Lumen", "Inv1te", "Shr12345"],
        "room <id> not found; <id>; invite <id>, share <id>",
    ),
    (
        "player twitch:90123456 and 77001122",
        &["90123456", "77001122"],
        "player twitch:<n> and <n>",
    ),
    (
        "digest deadbeefcafe0123 and deadbeef",
        &["deadbeef", "cafe"],
        "digest <hex> and <hex>",
    ),
    (
        "no song \"Starlight Parade\" for 'Lumen Night' or `Nova`",
        &["Starlight", "Parade", "Lumen", "Night", "Nova"],
        "no song <text> for <text> or <text>",
    ),
    (
        "Some(\"Kir\\\"arin\") then Err(\"Hoshimi",
        &["Kir", "arin", "Hoshimi"],
        "Some(<text>) then Err(<text>",
    ),
    (
        "曲「星の歌」 not found, ‘Hoshimi’ and “Kirarin”",
        &["星", "歌", "Hoshimi", "Kirarin", "曲"],
        "<text><text> not found, <text> and <text>",
    ),
    (
        "from 203.0.113.7 and 2001:db8::1",
        &["203.0", "113", "2001", "db8"],
        "from <ip> and <ip>",
    ),
    (
        "JsValue(TypeError: Failed to fetch) can't load",
        &[],
        "JsValue(TypeError: Failed to fetch) can't load",
    ),
    (
        "RuntimeError: unreachable executed",
        &[],
        "RuntimeError: unreachable executed",
    ),
];

#[test]
fn the_scrub_leaves_nothing_of_a_name_a_path_an_address_or_an_id() {
    for (raw, gone, want) in TABLE {
        let got = scrub(raw);
        for g in *gone {
            assert!(!got.contains(g), "{g:?} survived in {got:?} (from {raw:?})");
        }
        assert_eq!(&got, want, "from {raw:?}");
        assert_eq!(scrub(&got), got, "not idempotent on {raw:?}");
    }
}

#[test]
fn a_long_message_is_cut_to_300_characters() {
    let long = format!("panicked at crates/ui/src/x.rs:1:1: {}", "word ".repeat(400));
    let got = scrub(&long);
    assert!(got.chars().count() <= MAX_MESSAGE, "{}", got.len());
    assert_eq!(scrub(&got), got);
    // A placeholder at the cut is dropped, never left half.
    let ids = "Room0Lumen ".repeat(100);
    let got = scrub(&ids);
    assert!(got.chars().count() <= MAX_MESSAGE);
    assert!(got.ends_with("<id>"), "{got}");
    assert_eq!(scrub(&got), got);
    // Whitespace and control characters out.
    assert_eq!(scrub("a\u{0}\n\n\tb\r\n"), "a b");
}

/// Whatever goes in, the scrub's output is its own fixed point and no
/// longer than the bound (a deterministic run of odd strings).
#[test]
fn the_scrub_is_idempotent_on_odd_text() {
    let alphabet: Vec<char> =
        "aZ09/\\:.@?#=\"'`“”「」_- é星<>()[]{},;!\n\t0123456789abcdefABCDEF"
            .chars()
            .collect();
    let mut x: u64 = 0x2545_f491_4f6c_dd1d;
    for _ in 0..4_000 {
        let mut s = String::new();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let n = (x % 60) as usize;
        for _ in 0..n {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            s.push(alphabet[(x % alphabet.len() as u64) as usize]);
        }
        let once = scrub(&s);
        assert_eq!(scrub(&once), once, "from {s:?}");
        assert!(once.chars().count() <= MAX_MESSAGE);
        assert!(once.chars().all(|c| c.is_ascii() && !c.is_control()), "{once:?}");
    }
}

fn made(kind: &str, msg: &str) -> Report {
    Report {
        s: SCHEMA,
        kind: kind.into(),
        route: "play".into(),
        build: "0.1.0".into(),
        system: "ios".into(),
        form: "phone".into(),
        gpu: "webgpu".into(),
        msg: msg.into(),
    }
}

#[test]
fn a_report_is_read_only_in_its_own_form() {
    let good = made("panic", &scrub("panicked at crates/ui/src/x.rs:1:1: boom"));
    assert_eq!(read(&good.text()), Some(good.clone()));
    // The loader's own: no build, no client's kind, no message.
    let boot = r#"{"s":1,"kind":"boot_no_wasm_simd","route":"start"}"#;
    assert_eq!(read(boot).map(|r| r.build), Some("unknown".to_string()));
    let trap = r#"{"s":1,"kind":"wasm_trap","route":"play","msg":"unreachable"}"#;
    assert!(read(trap).is_some());
    let text = good.text();
    let mut bad: Vec<String> = [
        // A message the client did not scrub.
        ("boom", "boom at /Users/kirarin/x"),
        ("boom", "boom me@example.com"),
        ("boom", "boom \\\"Lumen\\\""),
        ("\"kind\":\"panic\"", "\"kind\":\"Kirarin\""),
        ("\"route\":\"play\"", "\"route\":\"#/play?song=Lumen\""),
        ("\"build\":\"0.1.0\"", "\"build\":\"0.1.0+kirarin\""),
        ("\"system\":\"ios\"", "\"system\":\"iPhone OS 17_4\""),
        ("\"s\":1", "\"s\":2"),
        ("\"form\":\"phone\"", "\"form\":\"phone\",\"user\":\"kirarin\""),
    ]
    .iter()
    .map(|(a, b)| {
        let t = text.replacen(a, b, 1);
        assert_ne!(t, text, "{a}");
        t
    })
    .collect();
    bad.push(made("wasm_trap", "kirarin").text());
    bad.push(made("boot_no_wasm", "boom").text());
    bad.push(made("panic", &"a".repeat(301)).text());
    bad.push(format!("{}{}", text, " ".repeat(MAX_BYTES)));
    for b in &bad {
        assert_eq!(read(b), None, "taken: {b}");
    }
}

#[test]
fn fingerprints_and_routes() {
    let a = fingerprint("panic", "x");
    assert_eq!(a.len(), 16);
    assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(a, fingerprint("panic", "x"));
    assert_ne!(a, fingerprint("panic", "y"));
    assert_ne!(a, fingerprint("unhandled_rejection", "x"));
    for (hash, want) in [
        ("", "start"),
        ("#/", "start"),
        ("#/select", "select"),
        ("#/play?song=Lumen&chart=3", "play"),
        ("#/mp", "room"),
        ("#/mp/room/Room0Lumen", "room"),
        ("#/mp/join/Inv1te", "room"),
        ("#/mp/watch/abc/stage", "watch"),
        ("#/mp/audience/abc", "watch"),
        ("#/room", "room"),
        ("#/options", "options"),
        ("#/edit?song=x", "edit"),
        ("#/s/Shr12345", "other"),
        ("#/credits", "other"),
    ] {
        assert_eq!(route_kind(hash), want, "{hash}");
        assert!(ROUTES.contains(&route_kind(hash)));
    }
}

#[test]
fn a_page_sends_one_a_fingerprint_and_five_a_session_and_none_when_off() {
    assert!(may_send(true, false));
    assert!(!may_send(false, false), "the row Off");
    assert!(!may_send(true, true), "Global Privacy Control on");
    assert!(!may_send(false, true));
    let mut sent = Sent::read(None);
    assert!(sent.allow("a"));
    assert!(!sent.allow("a"), "the same error twice");
    // Across a crash reload, read back from its text.
    let mut sent = Sent::read(Some(&sent.text()));
    assert!(!sent.allow("a"));
    for k in ["b", "c", "d", "e"] {
        assert!(sent.allow(k));
    }
    assert!(!sent.allow("f"), "five a session");
    assert_eq!(Sent::read(Some("not json")), Sent::default());
}

#[test]
fn a_report_is_made_scrubbed() {
    let kind = cs::ClientKind {
        app: "web",
        system: "ios",
        form: "phone",
        gpu: "webgpu",
        touch: "yes",
        build: "0.1.0",
    };
    let r = report(
        "panic",
        "play",
        Some(kind),
        "panicked at crates/ui/src/x.rs:1:1:\nno song \"Starlight\" in /Users/kirarin/Songs",
    )
    .unwrap();
    assert_eq!(r.msg, "panicked at ui/src/x.rs:1:1: no song <text> in <path>");
    assert!(r.check());
    assert_eq!(read(&r.text()), Some(r.clone()));
    let t = report("wasm_trap", "select", None, "RuntimeError: unreachable executed").unwrap();
    assert_eq!((t.msg.as_str(), t.build.as_str()), ("unreachable", "unknown"));
    assert!(t.check());
    let b = report("boot_no_wasm_simd", "start", None, "anything at all").unwrap();
    assert_eq!(b.msg, "");
    assert!(b.check());
    assert_eq!(report("Kirarin", "play", None, ""), None);
    assert_eq!(report("panic", "#/play?song=x", None, "x").unwrap().route, "other");
}
