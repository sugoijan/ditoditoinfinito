//! Desktop's online rooms against a local server (M5c): `DDI_ONLINE_API`
//! names one run by `just dev` in `../ddi-server` with its development
//! identity provider (`http://127.0.0.1:8787/ditoditoinfinito/api`); the
//! test is skipped without it. Two people sign in through desktop's
//! attempt (the system browser played by plain HTTP here, the provider
//! told who it is), one makes a room from the home's Private room, the
//! other reads its invite and joins; both connect through desktop's socket
//! and see each other. M5d and revision 17: the host adds a bundled song
//! (as an imported one) to the room's list, desktop's upload sends it
//! under that entry's grant, the host picks it from the list, the guest
//! reads it from the
//! room's share into the cache and reports it loaded, both ready, the
//! room counts down, each reports its own run and the room ranks them;
//! the guest leaves. `DDI_HOME` should name a scratch folder (the share
//! is read into its cache).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ddi_desktop::online::{SocketTransport, ThreadHttp};
use ddi_room::{ClientId, LinkState};
use ddi_ui::state::online::{
    Dest, Online, OnlineConfig, Platform, Redirect, SharedOnline, SignIn,
};
use ddi_ui::state::room::RoomClient;

fn api() -> Option<String> {
    std::env::var("DDI_ONLINE_API")
        .ok()
        .filter(|a| !a.is_empty())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn person(api: &str) -> SharedOnline {
    Online::new(
        Box::new(ThreadHttp::with_base(api.into())),
        OnlineConfig {
            platform: Platform::Desktop,
            site: "http://localhost:8080/".into(),
        },
    )
    .with_token(None, std::rc::Rc::new(|_| {}))
    .shared()
}

/// Polls `online` until `done`, at most 30 s.
fn until(online: &SharedOnline, mut done: impl FnMut(&mut Online) -> bool) {
    let start = Instant::now();
    loop {
        {
            let mut o = online.borrow_mut();
            o.poll(now());
            if done(&mut o) {
                return;
            }
        }
        assert!(start.elapsed() < Duration::from_secs(30), "timed out");
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// What the system browser and the person do with the attempt's page
/// (since 2026-11-15): the page asks for the code the app shows (and
/// leads nowhere by itself); a post from another origin is refused; a
/// wrong code is counted; the code typed as shown, posted from the page's
/// own origin, goes on to the development provider as `user` and back to
/// the server's callback, with the sign-in's binding cookie.
fn browser(url: &str, code: &str, user: &str) {
    browser_with(url, code, user, "");
}

/// [`browser`], `extra` added to the provider's address (a link trip's
/// fake Twitch lists, `&bans=eve`); the scopes the trip asked for.
fn browser_with(url: &str, code: &str, user: &str, extra: &str) -> Option<String> {
    let mut asked = None;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut page = agent.get(url).call().expect("the server answers");
    assert_eq!(page.status().as_u16(), 200, "the attempt's page");
    assert!(page.headers().get("location").is_none(), "never on by itself");
    let text = page.body_mut().read_to_string().unwrap();
    assert!(text.contains("action=confirm"), "{text}");
    let confirm = format!("{}/confirm", url.trim_end_matches("/open"));
    let origin = {
        let (scheme, rest) = url.split_once("://").unwrap();
        format!("{scheme}://{}", rest.split('/').next().unwrap())
    };
    let post = |origin: &str, form: &str| {
        agent
            .post(&confirm)
            .header("origin", origin)
            .content_type("application/x-www-form-urlencoded")
            .send(form.to_string())
            .expect("the server answers")
    };
    // Another site's form: refused before the attempt is read.
    let r = post("http://192.0.2.7", &format!("code={code}&action=continue"));
    assert_eq!(r.status().as_u16(), 403, "a cross-origin confirm");
    // A wrong code: the page again, counted.
    let mut r = post(&origin, "code=ZZZ-ZZZ&action=continue");
    assert_eq!(r.status().as_u16(), 400, "a wrong code");
    assert!(r.body_mut().read_to_string().unwrap().contains("4 more tries"));
    // The code as the app shows it, typed in lower case.
    let shown = ddi_ui::state::online::api::grouped_code(code).to_lowercase();
    let r = post(&origin, &format!("code={shown}&action=continue"));
    let mut cookie = String::new();
    let mut next = None;
    let mut r = r;
    for _ in 0..4 {
        for c in r.headers().get_all("set-cookie") {
            let pair = c.to_str().unwrap().split(';').next().unwrap().to_string();
            if !pair.ends_with('=') {
                cookie = pair;
            }
        }
        let Some(location) = r.headers().get("location") else {
            assert_eq!(r.status().as_u16(), 200, "the callback's page");
            assert!(next.is_some(), "the confirmation led on");
            return asked;
        };
        let mut to = location.to_str().unwrap().to_string();
        if to.contains("/auth/fake/authorize") {
            assert!(to.contains("force_verify=true"), "{to}");
            asked = to
                .split(['?', '&'])
                .find_map(|p| p.strip_prefix("scope="))
                .map(|s| s.replace("%3A", ":").replace(['+'], " ").replace("%20", " "));
            to = format!("{to}&user={user}&name={user}{extra}");
        }
        next = Some(to.clone());
        let mut req = agent.get(&to);
        if !cookie.is_empty() {
            req = req.header("cookie", &cookie);
        }
        r = req.call().expect("the server answers");
    }
    panic!("too many redirects");
}

/// The browser's URL and the code the app shows, once it asked for an
/// attempt.
fn attempt_of(online: &SharedOnline) -> (String, String) {
    let mut url = None;
    until(online, |o| {
        if let Some(Redirect::Browser(u)) = o.take_redirect() {
            url = Some(u);
        }
        url.is_some()
    });
    let code = match online.borrow().sign_in_state() {
        SignIn::Waiting { code: Some(c), .. } => c.clone(),
        other => panic!("no code shown: {other:?}"),
    };
    (url.unwrap(), code)
}

/// Signs `online` in as `user` (for `dest`), the browser played.
fn sign_in(online: &SharedOnline, user: &str, dest: Dest) {
    online.borrow_mut().sign_in(dest);
    let (url, code) = attempt_of(online);
    browser(&url, &code, user);
}

fn connect(online: &SharedOnline, api: &str, room: &str, who: &str) -> RoomClient {
    let mut client = RoomClient::online(
        online.clone(),
        room,
        Box::new(SocketTransport::with_base(api.into())),
        ClientId::new(format!("desktop-test-{who}-{}", now())),
        now(),
    );
    let start = Instant::now();
    while client.link_state() != LinkState::Connected {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "{who} never connected"
        );
        std::thread::sleep(Duration::from_millis(30));
        client.tick(now());
    }
    client
}

fn tick_until(rooms: &mut [&mut RoomClient], mut done: impl FnMut(&[&mut RoomClient]) -> bool) {
    let start = Instant::now();
    loop {
        for r in rooms.iter_mut() {
            r.tick(now());
        }
        if done(rooms) {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(30), "timed out");
        std::thread::sleep(Duration::from_millis(30));
    }
}

#[test]
fn desktop_signs_in_makes_a_room_and_a_guest_joins_it() {
    let Some(api) = api() else {
        eprintln!("DDI_ONLINE_API not set: skipped");
        return;
    };
    let suffix = now() % 100_000;
    let (host_name, guest_name) = (format!("deskhost{suffix}"), format!("deskguest{suffix}"));
    // The host: Private room signs in first, then the room is made.
    let host = person(&api);
    host.borrow_mut().check();
    until(&host, |o| {
        !matches!(o.account(), ddi_ui::state::online::Account::Checking)
    });
    host.borrow_mut().private_room();
    let (url, code) = attempt_of(&host);
    browser(&url, &code, &host_name);
    let mut room_id = None;
    until(&host, |o| {
        room_id = o.take_entered();
        room_id.is_some()
    });
    let room_id = room_id.unwrap();
    assert_eq!(
        host.borrow().me().unwrap().player_id,
        format!("dev:{host_name}")
    );
    let mut a = connect(&host, &api, &room_id, "host");
    assert_eq!(a.my_role().map(|(r, _)| r), Some(ddi_room::Role::Owner));
    a.fetch_invite();
    tick_until(&mut [&mut a], |r| {
        r[0].invite_link().is_some() && r[0].name().is_some()
    });
    let link = a.invite_link().unwrap();
    let invite = ddi_ui::state::online::api::invite_in(&link).unwrap();

    // The guest: the invite's preview signed out, then sign in and join.
    let guest = person(&api);
    guest.borrow_mut().open_invite(&invite);
    until(&guest, |o| o.invite().is_some_and(|v| v.preview.is_some()));
    {
        let g = guest.borrow();
        let p = g
            .invite()
            .unwrap()
            .preview
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap();
        assert_eq!(p.host_display_name, host_name);
        assert_eq!(p.admission, ddi_ui::state::online::api::Admission::Open);
    }
    sign_in(&guest, &guest_name, Dest::Join(invite.clone()));
    let mut joined = None;
    until(&guest, |o| {
        joined = o.take_entered();
        joined.is_some()
    });
    assert_eq!(joined.as_deref(), Some(room_id.as_str()));
    let mut b = connect(&guest, &api, &room_id, "guest");
    tick_until(&mut [&mut a, &mut b], |r| {
        r[0].state().members.values().filter(|m| !m.left).count() == 2
            && r[0].state() == r[1].state()
    });
    assert_eq!(b.my_role().map(|(r, _)| r), Some(ddi_room::Role::Member));
    assert!(!b.may(ddi_room::Capability::ChooseContent));

    // M5d: the host's pick, a bundled song's single chart, uploaded with
    // desktop's sharing code under the room's grant.
    // The share client reads the server from the environment.
    // SAFETY: this file's one test sets it before any thread reads it.
    unsafe { std::env::set_var("DDI_API_URL", &api) };
    let entry = ddi_desktop::library::bundled()
        .into_iter()
        .find(|e| e.id == "beatucada")
        .expect("the bundled song");
    let song = ddi_ui::state::select_view::plain_song_entry(&entry);
    let chart = song
        .charts
        .iter()
        .find(|c| c.style == "dance-single")
        .unwrap()
        .clone();
    let (a, b) = (Rc::new(RefCell::new(a)), Rc::new(RefCell::new(b)));
    a.borrow_mut().set_inbox(ddi_desktop::room::inbox());
    // Revision 17: the host adds it to the room's list (as an imported
    // song; a bundled one is uploaded the same since revision 19), and it
    // is uploaded at once under that entry's grant (desktop's sharing
    // code); both see it there.
    let mut song = song;
    song.pack = Some("RL1".into());
    a.borrow_mut().set_library(std::slice::from_ref(&song));
    a.borrow_mut().add_song(&song).unwrap();
    let mut jobs = Vec::new();
    let waited = Instant::now();
    while jobs.is_empty() {
        a.borrow_mut().tick(now());
        jobs = a.borrow_mut().take_upload_jobs();
        assert!(waited.elapsed() < Duration::from_secs(30), "timed out");
        std::thread::sleep(Duration::from_millis(30));
    }
    for j in jobs {
        assert!(j.entry.is_some());
        assert!(!j.grant.is_empty());
        ddi_desktop::room::upload(j);
    }
    tick_until(&mut [&mut a.borrow_mut(), &mut b.borrow_mut()], |r| {
        r.iter()
            .all(|r| r.room_songs().first().is_some_and(|e| e.available() && e.share.is_some()))
    });
    let listed = b.borrow().room_songs()[0].clone();
    assert_eq!(listed.card.title, song.title);
    // Nothing is read ahead of a pick in this stage.
    b.borrow_mut().set_inbox(ddi_desktop::room::inbox());
    assert!(b.borrow_mut().take_load_jobs().is_empty());
    // The pick from the list: available at once, nothing uploaded.
    let key = ddi_room::ChartKey::new(chart.digest.unwrap().to_string());
    a.borrow_mut().pick_listed(listed.id, &key).unwrap();
    tick_until(&mut [&mut a.borrow_mut(), &mut b.borrow_mut()], |r| {
        r.iter().all(|r| {
            r.stages()[0]
                .content
                .as_ref()
                .is_some_and(|c| c.status == ddi_room::ContentStatus::Available)
        })
    });
    assert!(a.borrow_mut().take_upload_jobs().is_empty());
    let stage = a.borrow().stages()[0].id;
    let share = b.borrow().stages()[0]
        .content
        .as_ref()
        .and_then(|c| c.share.clone())
        .expect("the room's share");
    assert_eq!(Some(share.as_str()), listed.share.as_deref());
    // Each prepares it: the host from its library, the guest from the
    // room's share (desktop's reader, into the cache).
    let jobs = a.borrow_mut().take_load_jobs();
    for j in jobs {
        assert!(j.share.is_none());
        ddi_desktop::room::prepare_online(&a, j);
    }
    let jobs = b.borrow_mut().take_load_jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].share.as_deref(), Some(share.as_str()));
    ddi_desktop::room::prepare_online(&b, jobs.into_iter().next().unwrap());
    tick_until(&mut [&mut a.borrow_mut(), &mut b.borrow_mut()], |r| {
        r.iter().all(|r| {
            let prep = r.stages()[0].prep(r.me());
            !prep.ready && prep.loading == ddi_room::Loading::Loaded
        })
    });
    let content = b.borrow().stages()[0].content.clone().unwrap();
    assert_eq!(
        b.borrow().picked(&content.id).unwrap().song.id,
        format!("shared:{share}:0")
    );
    // Each one's prompt; both Ready: the room starts the stage at once
    // (its countdown); each plays it on its own device and reports its
    // own run; the room ranks them.
    let mut orders = Vec::new();
    for r in [&a, &b] {
        orders.push(r.borrow_mut().take_stage_order().expect("its prompt"));
    }
    a.borrow_mut().set_ready(true).unwrap();
    b.borrow_mut().set_ready(true).unwrap();
    tick_until(&mut [&mut a.borrow_mut(), &mut b.borrow_mut()], |r| {
        r.iter().all(|r| r.stage_started(stage))
    });
    assert_eq!((orders[0].others, orders[1].others), (1, 1));
    // The host's report says how they performed (feedback round B); the
    // guest's only its run.
    let detail = ddi_room::RunDetail {
        percent: Some(9_000),
        grade: "AA".into(),
        life_ppm: 1_000_000,
        max_combo: 77,
        tiers: vec![70, 5, 2, 0, 0, 0, 1],
        ..Default::default()
    };
    a.borrow_mut()
        .report_run(
            stage,
            ddi_room::Run::Live {
                life: 1.0,
                score: 0,
                steps: 0,
            },
            Some(detail.clone()),
        )
        .unwrap();
    b.borrow_mut()
        .report_run(stage, ddi_room::Run::Out { at: 30.0 }, None)
        .unwrap();
    tick_until(&mut [&mut a.borrow_mut(), &mut b.borrow_mut()], |r| {
        r.iter()
            .all(|r| r.state().phase.kind() == ddi_room::PhaseKind::Intermission)
    });
    // The guest's standings of the stage: the host first with what they
    // said, as the server passed it on (a server built before the detail
    // drops it: build it again against this checkout).
    let table = b
        .borrow()
        .stage_table(stage)
        .expect("the stage's standings");
    assert!(table.confirmed);
    assert_eq!(table.rows[0].placement.unwrap().place, 1);
    assert_eq!(
        table.rows[0].detail.as_ref(),
        Some(&detail),
        "the server passed the report's detail on"
    );
    assert_eq!(table.rows[1].detail, None);
    assert_eq!(a.borrow().mark_of(stage).unwrap().placement.place, 1);
    assert_eq!(b.borrow().mark_of(stage).unwrap().placement.place, 2);
    let mut a = Rc::try_unwrap(a).ok().unwrap().into_inner();
    let mut b = Rc::try_unwrap(b).ok().unwrap().into_inner();

    // The guest leaves; the host sees it.
    b.leave();
    tick_until(&mut [&mut a, &mut b], |r| {
        r[0].state().members.values().filter(|m| !m.left).count() == 1
    });
    tick_until(&mut [&mut b], |r| r[0].link_state() == LinkState::Left);
}

/// review3 desktop-3: an idle room's socket stays connected past the
/// keepalive's bounds (the server answers its pings), never dropped and
/// reconnected for being quiet.
#[test]
fn an_idle_rooms_socket_stays_connected() {
    let Some(api) = api() else {
        eprintln!("DDI_ONLINE_API not set: skipped");
        return;
    };
    let name = format!("deskidle{}", now() % 100_000);
    let host = person(&api);
    host.borrow_mut().check();
    until(&host, |o| {
        !matches!(o.account(), ddi_ui::state::online::Account::Checking)
    });
    host.borrow_mut().private_room();
    let (url, code) = attempt_of(&host);
    browser(&url, &code, &name);
    let mut room_id = None;
    until(&host, |o| {
        room_id = o.take_entered();
        room_id.is_some()
    });
    let mut a = connect(&host, &api, &room_id.unwrap(), "idle");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(26) {
        a.tick(now());
        assert_eq!(
            a.link_state(),
            LinkState::Connected,
            "dropped after {:?}",
            start.elapsed()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    a.leave();
}

/// Feedback round N2 (room model revision 12): the room's own text
/// keep-alive (`ddi_room::online::KEEPALIVE_V0`, the web's; desktop's
/// socket keeps its own at the WebSocket layer, turned off here) against a
/// local server reached through a proxy that can cut a socket silently:
/// `DDI_ONLINE_PROXY` is the API's address through the proxy,
/// `DDI_ONLINE_PROXY_CUT` the file whose appearance cuts its WebSockets
/// (every frame swallowed, nothing closed). Skipped without them. An idle
/// room's socket stays two minutes (a live room's clock pings every 10 s,
/// so its keep-alive is sent only where those stop); cut silently, it is
/// noticed and replaced within about 40 s.
#[test]
fn the_rooms_keep_alive_keeps_an_idle_socket_and_replaces_one_cut_silently() {
    let (Ok(api), Ok(cut)) = (
        std::env::var("DDI_ONLINE_PROXY"),
        std::env::var("DDI_ONLINE_PROXY_CUT"),
    ) else {
        eprintln!("DDI_ONLINE_PROXY or DDI_ONLINE_PROXY_CUT not set: skipped");
        return;
    };
    let name = format!("deskkeep{}", now() % 100_000);
    let host = person(&api);
    host.borrow_mut().check();
    until(&host, |o| {
        !matches!(o.account(), ddi_ui::state::online::Account::Checking)
    });
    host.borrow_mut().private_room();
    let (url, code) = attempt_of(&host);
    browser(&url, &code, &name);
    let mut room_id = None;
    until(&host, |o| {
        room_id = o.take_entered();
        room_id.is_some()
    });
    let mut a = RoomClient::online(
        host.clone(),
        &room_id.unwrap(),
        Box::new(SocketTransport::with_base(api.clone()).with_room_keepalive()),
        ClientId::new(format!("desktop-test-keep-{}", now())),
        now(),
    );
    tick_until(&mut [&mut a], |r| r[0].link_state() == LinkState::Connected);
    let keep = |a: &RoomClient| a.online_link().unwrap().keepalives();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(125) {
        a.tick(now());
        assert_eq!(
            a.link_state(),
            LinkState::Connected,
            "dropped after {:?}",
            start.elapsed()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!("idle 125 s: keep-alives sent, dead {:?}", keep(&a));
    assert_eq!(keep(&a).1, 0);
    std::fs::write(&cut, b"cut").unwrap();
    let cut_at = Instant::now();
    let mut noticed = None;
    loop {
        a.tick(now());
        if noticed.is_none() && a.link_state() != LinkState::Connected {
            noticed = Some(cut_at.elapsed());
        }
        if noticed.is_some() && a.link_state() == LinkState::Connected {
            break;
        }
        assert!(cut_at.elapsed() < Duration::from_secs(60), "never replaced");
        std::thread::sleep(Duration::from_millis(100));
    }
    let back = cut_at.elapsed();
    eprintln!("cut silently: noticed after {noticed:?}, connected again after {back:?}; {:?}", keep(&a));
    let noticed = noticed.unwrap();
    assert!(
        noticed >= Duration::from_secs(25) && noticed <= Duration::from_secs(43),
        "{noticed:?}"
    );
    assert!(back <= Duration::from_secs(46), "{back:?}");
    assert_eq!(keep(&a).1, 1);
    a.leave();
}

/// One step a room's host takes (round TB): a change in Host options (the
/// room's Twitch setting asked as the dialog asks it, its trip through
/// Twitch where the access lacks a scope), or this device's own "Twitch
/// chat in rooms" turned on (its own trip, for `chat`).
#[derive(Clone, Copy, Debug)]
enum TwitchStep {
    Host(ddi_room::TwitchMirror, ddi_room::TwitchChat),
    Device,
}

/// Round TB (the maintainer, 2026-11-22: "Does the 'Twitch bans' work
/// together with 'Show my Twitch chat'?"): on desktop, against a local
/// server's fake Twitch, a host turns on the room's Twitch bans (or bans
/// and blocks) and Show my Twitch chat in every order, with the access
/// starting empty or with one feature connected first by this device's
/// own chat option, each trip through Twitch played as desktop's (the
/// attempt's code typed in the browser's page). Every case ends with both
/// features on in the room, the access granting all they need, a person
/// the host banned refused at the door with the setting still on, and the
/// host's device subscribed to the room's chat.
#[test]
fn a_hosts_twitch_bans_and_chat_are_both_kept_in_any_order() {
    use ddi_room::{TwitchChat as C, TwitchMirror as M};
    use ddi_ui::state::online::api::{ApiRequest, Method};
    use TwitchStep::{Device, Host};
    let Some(api) = api() else {
        eprintln!("DDI_ONLINE_API not set: skipped");
        return;
    };
    let suffix = now() % 100_000;
    // Eve, whom every host here banned (the fake Twitch's `bans=eve`).
    let eve = person(&api);
    eve.borrow_mut().check();
    until(&eve, |o| !matches!(o.account(), ddi_ui::state::online::Account::Checking));
    sign_in(&eve, "eve", Dest::Home);
    until(&eve, |o| o.me().is_some() && !o.sign_in_state().busy());
    let cases: Vec<(&str, Vec<TwitchStep>)> = vec![
        ("banschat", vec![Host(M::Bans, C::Off), Host(M::Bans, C::Emotes)]),
        ("chatbans", vec![Host(M::Off, C::Emotes), Host(M::Bans, C::Emotes)]),
        ("blockschat", vec![Host(M::BansAndBlocks, C::Off), Host(M::BansAndBlocks, C::EmotesAndMessages)]),
        ("deviceboth", vec![Device, Host(M::Bans, C::Emotes)]),
        ("devicebans", vec![Device, Host(M::Bans, C::Off), Host(M::Bans, C::Emotes)]),
        ("bansdevice", vec![Host(M::Bans, C::Off), Device, Host(M::Bans, C::Emotes)]),
        ("bothdevice", vec![Host(M::Bans, C::Off), Host(M::Bans, C::Emotes), Device]),
    ];
    for (case, steps) in cases {
        let user = format!("tb{case}{suffix}");
        let host = person(&api);
        host.borrow_mut().check();
        until(&host, |o| !matches!(o.account(), ddi_ui::state::online::Account::Checking));
        host.borrow_mut().private_room();
        let (url, code) = attempt_of(&host);
        browser(&url, &code, &user);
        let mut room_id = None;
        until(&host, |o| {
            room_id = o.take_entered();
            room_id.is_some()
        });
        let room_id = room_id.unwrap();
        let mut a = connect(&host, &api, &room_id, &user);
        let mut trips = Vec::new();
        let mut last = (M::Off, C::Off);
        for step in &steps {
            match *step {
                Host(mirror, chat) => {
                    // What Host options sends: the room's setting with the
                    // row changed; a trip where the access lacks a scope,
                    // the setting made by itself on the way back.
                    a.set_twitch(mirror, chat);
                    let start = Instant::now();
                    loop {
                        a.tick(now());
                        if let Some(need) = a.take_twitch_trip() {
                            let (url, code) = attempt_of(&host);
                            let asked = browser_with(&url, &code, &user, "&bans=eve&blocks=eve");
                            trips.push((need, asked));
                        }
                        let now_set = a.twitch().map(|t| (t.mirror, t.chat));
                        if now_set == Some((mirror, chat)) && !host.borrow().sign_in_state().busy() {
                            break;
                        }
                        assert!(
                            start.elapsed() < Duration::from_secs(20),
                            "{case}: {step:?} never made: the room's setting {now_set:?}; trips (need, scopes asked) {trips:?}; access {:?}",
                            host.borrow().twitch()
                        );
                        std::thread::sleep(Duration::from_millis(30));
                    }
                    last = (mirror, chat);
                }
                Device => {
                    host.borrow_mut()
                        .connect_twitch_chat(Dest::Room(room_id.clone()), ddi_chat::Level::Emotes);
                    let (url, code) = attempt_of(&host);
                    let asked = browser_with(&url, &code, &user, "&bans=eve&blocks=eve");
                    trips.push(("chat (device)".into(), asked));
                    tick_until(&mut [&mut a], |_| {
                        let o = host.borrow();
                        !o.sign_in_state().busy() && o.twitch().is_some_and(|t| t.features.iter().any(|f| f == "chat"))
                    });
                }
            }
        }
        // The access grants everything both features need.
        let needs = ddi_ui::state::online::api::twitch_needs(last.0, last.1);
        let features = host.borrow().twitch().map(|t| t.features.clone()).unwrap_or_default();
        for n in &needs {
            assert!(features.iter().any(|f| f == n), "{case}: the access lacks {n}: {features:?}; trips {trips:?}");
        }
        // Eve, banned by the host, is refused at the door, and the setting
        // stays as it is (the host's lists read with the access kept).
        a.fetch_invite();
        tick_until(&mut [&mut a], |r| r[0].invite_link().is_some());
        let invite = ddi_ui::state::online::api::invite_in(&a.invite_link().unwrap()).unwrap();
        eve.borrow_mut().open_invite(&invite);
        until(&eve, |o| o.invite().is_some_and(|v| v.preview.is_some()));
        eve.borrow_mut().join_invite();
        until(&eve, |o| o.invite().is_some_and(|v| !v.joining && v.refused.is_some()));
        assert_eq!(
            eve.borrow().invite().unwrap().refused.as_ref().unwrap().code,
            "not_admitted",
            "{case}"
        );
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(600) {
            a.tick(now());
            std::thread::sleep(Duration::from_millis(30));
        }
        assert_eq!(a.twitch().map(|t| (t.mirror, t.chat)), Some(last), "{case}: the setting after the door's check");
        // The host's device subscribes to the room's chat (the fake
        // Twitch answers).
        let id = host.borrow_mut().request(ApiRequest {
            method: Method::Post,
            path: format!("/rooms/{room_id}/chat/subscribe"),
            body: Some(r#"{"session_id":"tb-session","messages":false}"#.into()),
            bearer: None,
            poll: None,
        });
        let mut answer = None;
        until(&host, |o| {
            answer = o.take_answer(id);
            answer.is_some()
        });
        let answer = answer.unwrap();
        assert_eq!(answer.status, 200, "{case}: the chat's subscription: {}", answer.body);
        eprintln!("{case}: {last:?}, access {features:?}, trips (need, scopes asked) {trips:?}");
        a.leave();
    }
}

thread_local! {
    /// The test's epoch clock (milliseconds) for the device's settings.
    static EPOCH: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn epoch() -> f64 {
    EPOCH.with(std::cell::Cell::get)
}

/// Round TB (the maintainer, 2026-11-22: "we should avoid downgrading the
/// permissions, at least within a time-frame"): on desktop against a local
/// server, a host who used Twitch bans and chat in their room, signed out
/// and in again, has both back from the sign-in's own step through Twitch
/// (the attempt asks for them), with no second trip; a permission unused
/// past `TWITCH_KEEP_DAYS` (a test clock) is not asked for; after Options
/// → Data's Disconnect nothing is asked.
#[test]
fn a_sign_in_brings_back_the_twitch_permissions_used_lately() {
    use ddi_room::{TwitchChat as C, TwitchMirror as M};
    use ddi_ui::state::twitch::TWITCH_KEEP_DAYS;
    let Some(api) = api() else {
        eprintln!("DDI_ONLINE_API not set: skipped");
        return;
    };
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    ddi_platform::storage::install_clock(epoch);
    let t0 = 1_800_000_000_000.0;
    EPOCH.with(|e| e.set(t0));
    let user = format!("tbkeep{}", now() % 100_000);
    let features = |o: &SharedOnline| o.borrow().twitch().map(|t| t.features.clone()).unwrap_or_default();
    // Signs in with desktop's attempt as the app does it; the scopes the
    // step through Twitch asked.
    let sign_in_fresh = || {
        let o = person(&api);
        o.borrow_mut().check();
        until(&o, |o| !matches!(o.account(), ddi_ui::state::online::Account::Checking));
        o.borrow_mut().sign_in(Dest::Home);
        let (url, code) = attempt_of(&o);
        let asked = browser_with(&url, &code, &user, "&bans=eve").unwrap_or_default();
        until(&o, |o| o.me().is_some() && !o.sign_in_state().busy());
        (o, asked)
    };
    let (host, asked) = sign_in_fresh();
    assert_eq!(asked, "", "nothing used yet: a plain sign-in");
    host.borrow_mut().private_room();
    let mut room_id = None;
    until(&host, |o| {
        room_id = o.take_entered();
        room_id.is_some()
    });
    let room_id = room_id.unwrap();
    let mut a = connect(&host, &api, &room_id, &user);
    for (mirror, chat) in [(M::Bans, C::Off), (M::Bans, C::Emotes)] {
        a.set_twitch(mirror, chat);
        let start = Instant::now();
        loop {
            a.tick(now());
            if a.take_twitch_trip().is_some() {
                let (url, code) = attempt_of(&host);
                browser_with(&url, &code, &user, "&bans=eve");
            }
            if a.twitch().map(|t| (t.mirror, t.chat)) == Some((mirror, chat)) && !host.borrow().sign_in_state().busy() {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(20), "{mirror:?}/{chat:?} never made");
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    // Used in the room: written down on the device.
    tick_until(&mut [&mut a], |_| ddi_ui::state::twitch::kept_features() == ["bans", "chat"]);
    a.leave();
    host.borrow_mut().sign_out();
    until(&host, |_| true);
    // Signed in again: both back from the sign-in's own step.
    let (again, asked) = sign_in_fresh();
    assert_eq!(asked, "moderation:read user:read:chat");
    assert_eq!(features(&again), ["bans", "chat"], "no second trip");
    // Past the window without use: not asked for.
    again.borrow_mut().sign_out();
    until(&again, |_| true);
    EPOCH.with(|e| e.set(t0 + TWITCH_KEEP_DAYS as f64 * 86_400_000.0 + 1.0));
    let (late, asked) = sign_in_fresh();
    assert_eq!(asked, "", "unused for {TWITCH_KEEP_DAYS} days");
    assert!(features(&late).is_empty());
    // Used again, then Disconnect: nothing asked at the next sign-in.
    ddi_ui::state::twitch::mark_used(&["chat"]);
    late.borrow_mut().disconnect_twitch();
    until(&late, |_| true);
    late.borrow_mut().sign_out();
    until(&late, |_| true);
    let (after, asked) = sign_in_fresh();
    assert_eq!(asked, "", "nothing after Disconnect");
    assert!(features(&after).is_empty());
}
