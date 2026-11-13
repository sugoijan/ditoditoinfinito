//! Desktop's online rooms against a local server (M5c): `DDI_ONLINE_API`
//! names one run by `just dev` in `../ddi-server` with its development
//! identity provider (`http://127.0.0.1:8787/ditoditoinfinito/api`); the
//! test is skipped without it. Two people sign in through desktop's
//! attempt (the system browser played by plain HTTP here, the provider
//! told who it is), one makes a room from the home's Private room, the
//! other reads its invite and joins; both connect through desktop's socket
//! and see each other. M5d: the host chooses a bundled song, desktop's
//! upload sends it under the room's grant, the guest reads it from the
//! room's share into the cache and reports it loaded, both ready, the
//! room counts down, each reports its own run and the room ranks them;
//! the guest leaves. `DDI_HOME` should name a scratch folder (the share
//! is read into its cache).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ddi_desktop::online::{SocketTransport, ThreadHttp};
use ddi_room::{ClientId, LinkState};
use ddi_ui::state::online::{Dest, Online, OnlineConfig, Platform, Redirect, SharedOnline};
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

/// What the system browser does with the attempt's page: on to the
/// development provider as `user`, back to the server's callback, with
/// the sign-in's binding cookie.
fn browser(url: &str, user: &str) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut cookie = String::new();
    let mut next = url.to_string();
    for _ in 0..4 {
        let mut req = agent.get(&next);
        if !cookie.is_empty() {
            req = req.header("cookie", &cookie);
        }
        let r = req.call().expect("the server answers");
        for c in r.headers().get_all("set-cookie") {
            let pair = c.to_str().unwrap().split(';').next().unwrap().to_string();
            if !pair.ends_with('=') {
                cookie = pair;
            }
        }
        let Some(location) = r.headers().get("location") else {
            assert_eq!(r.status().as_u16(), 200, "the callback's page");
            return;
        };
        next = location.to_str().unwrap().to_string();
        if next.contains("/auth/fake/authorize") {
            next = format!("{next}&user={user}&name={user}");
        }
    }
    panic!("too many redirects");
}

/// Signs `online` in as `user` (for `dest`), the browser played.
fn sign_in(online: &SharedOnline, user: &str, dest: Dest) {
    online.borrow_mut().sign_in(dest);
    let mut url = None;
    until(online, |o| {
        if let Some(Redirect::Browser(u)) = o.take_redirect() {
            url = Some(u);
        }
        url.is_some()
    });
    browser(&url.unwrap(), user);
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
    let mut url = None;
    until(&host, |o| {
        if let Some(Redirect::Browser(u)) = o.take_redirect() {
            url = Some(u);
        }
        url.is_some()
    });
    browser(&url.unwrap(), &host_name);
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
    // M7b: the host's song select rests on it; after the rest the room
    // shows it to the guest and it is uploaded ahead, under a grant for no
    // stage (desktop's sharing code).
    a.borrow_mut().hover(Some((&song, &chart)));
    let mut jobs = Vec::new();
    let waited = Instant::now();
    while jobs.is_empty() {
        a.borrow_mut().tick(now());
        jobs = a.borrow_mut().take_upload_jobs();
        assert!(waited.elapsed() < Duration::from_secs(30), "timed out");
        std::thread::sleep(Duration::from_millis(30));
    }
    for j in jobs {
        assert_eq!(j.stage, ddi_ui::state::room::SPOTLIGHT);
        assert!(!j.grant.is_empty());
        ddi_desktop::room::upload(j);
    }
    tick_until(&mut [&mut a.borrow_mut(), &mut b.borrow_mut()], |r| {
        r.iter().all(|r| {
            r.state()
                .spotlight
                .as_ref()
                .is_some_and(|s| s.share.is_some())
        })
    });
    // The guest reads it from the room's share (its banner and preview).
    b.borrow_mut().set_inbox(ddi_desktop::room::inbox());
    let jobs = b.borrow_mut().take_load_jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].stage, ddi_ui::state::room::SPOTLIGHT);
    ddi_desktop::room::prepare_online(&b, jobs.into_iter().next().unwrap());
    tick_until(&mut [&mut b.borrow_mut()], |r| {
        r[0].spotlight().is_some_and(|(_, p)| p.is_some())
    });
    // The pick reuses that share: available at once, nothing uploaded.
    a.borrow_mut().pick(&song, &chart).unwrap();
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
    assert_eq!(
        Some(share.as_str()),
        a.borrow()
            .state()
            .spotlight
            .as_ref()
            .and_then(|s| s.share.as_deref())
    );
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
            r.stages()[0].prep(r.me())
                == ddi_room::Prep {
                    ready: false,
                    loading: ddi_room::Loading::Loaded,
                }
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
                accuracy: 0.9,
                absolute: None,
                score: None,
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
