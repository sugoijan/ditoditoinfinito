//! Desktop's sign-in code (round SF; the maintainer, 2026-11-20: "On the
//! desktop app, with the twitch device code flow the code should be
//! rendered with emphasis and with copy on click behavior"): the sign-in
//! dialog and the Twitch access trip's show the attempt's code large on a
//! plate of its own in the game's look, and a click on it asks the shell
//! to copy it (a request here: the real clipboard is never written by a
//! test), "Copied" in its place for a moment. Since round DC an invite's
//! page shows the same plate while its Join signs in (the maintainer,
//! 2026-11-20: "yes, the same plate"). Drawn offscreen where there is a
//! GPU, at 1280×720 and in a small window (`DDI_OFFSCREEN_PPM=<dir>` keeps
//! `sign-in-code-<what>-<w>x<h>-<lang>.ppm`). Since round TU2 a sign-in
//! (never the access trip) says under its text and the code that signing
//! in means agreeing to the terms of use, with the terms' and the privacy
//! note's links (`sign-in-terms-<when>-<w>x<h>-<lang>.ppm`).

mod common;
use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;

use ddi_ui::screens::online::{OnlinePage, OnlineScreen};
use ddi_ui::screens::online_home::{CODE, SIGN_IN_PRIVACY, SIGN_IN_TERMS, SignInDialog};
use ddi_ui::state::online::api::{ApiRequest, ApiResponse, Method};
use ddi_ui::state::online::{Dest, Http, Online, OnlineConfig, Platform};
use ddi_ui::testing::Harness;
use ddi_ui::{PointerKind, TextMode, UiRequest};

/// The server's answers to a desktop's sign-in attempt that stays
/// pending (no network).
struct Attempt {
    signed_in: bool,
    out: VecDeque<(u64, ApiResponse)>,
}

impl Http for Attempt {
    fn start(&mut self, id: u64, request: ApiRequest) {
        let (status, body) = match (request.method, request.path.as_str()) {
            (Method::Get, "/me") if self.signed_in => (
                200,
                r#"{"player_id":"dev:carol","login":"carol","display_name":"Carol","admin":false,"expires_at_ms":1}"#,
            ),
            (Method::Get, "/me") => (401, r#"{"error":"signed_out"}"#),
            (Method::Post, "/auth/attempts") => (
                201,
                r#"{"attempt_id":"a1","poll_secret":"s1","open_url":"http://127.0.0.1/auth/attempts/a1/open","user_code":"K7MX3Q","expires_at_ms":9}"#,
            ),
            (Method::Get, "/auth/attempts/a1") => {
                (200, r#"{"status":"pending","return":"/mp","expires_at_ms":9}"#)
            }
            (Method::Get, p) if p.starts_with("/invites/") => (
                200,
                r#"{"room_id":"Room000001","room_name":"After Hours","host_display_name":"Alice","phase":"lobby","admission":"open","members":2}"#,
            ),
            _ => (404, r#"{"error":"not_found"}"#),
        };
        self.out.push_back((
            id,
            ApiResponse {
                status,
                body: body.into(),
                session: None,
            },
        ));
    }

    fn poll(&mut self) -> Vec<(u64, ApiResponse)> {
        self.out.drain(..).collect()
    }
}

/// The game's look, its text folded plainly.
fn env() -> ddi_ui::screens::Env {
    ddi_ui::screens::Env::new(
        Rc::new(ddi_theme::Infinito::new()),
        Rc::new(|s: &str| s.to_string()),
    )
}

#[test]
fn the_sign_in_code_is_large_and_copied_on_a_click() {
    common::indexes();
    let mut pictures = common::gpu::Pictures::new();
    for (size, lang) in [([1280u32, 720u32], "en"), ([640, 400], "en"), ([1280, 720], "ja")] {
        assert!(ddi_l10n::set_language(lang));
        for connect in [false, true] {
            let online = Online::new(
                Box::new(Attempt {
                    signed_in: connect,
                    out: VecDeque::new(),
                }),
                OnlineConfig {
                    platform: Platform::Desktop,
                    site: "http://localhost:8080/".into(),
                },
            )
            .shared();
            online.borrow_mut().check();
            online.borrow_mut().poll(0);
            let dialog = if connect {
                online
                    .borrow_mut()
                    .connect_twitch(Dest::Home, "chat");
                online.borrow_mut().poll(0);
                SignInDialog::connecting(
                    env(),
                    online.clone(),
                    Dest::Home,
                    "chat",
                    None,
                )
            } else {
                online.borrow_mut().sign_in(Dest::Home);
                online.borrow_mut().poll(0);
                SignInDialog::new(env(), online.clone(), Dest::Home, Rc::new(Cell::new(true)))
            };
            let css = [size[0] as f32, size[1] as f32];
            let look = ddi_theme::Infinito::new();
            let design = ddi_ui::screens::design_for(&look, css);
            let mut h = Harness::with_gestures(design, css, 1.0, TextMode::Own, false);
            h.ui.set_focus_ring(false);
            h.push(dialog);
            h.run(0.5);
            let what = if connect { "twitch" } else { "sign-in" };
            assert!(h.ui.bounds(CODE).is_some(), "{what} {size:?}: the code's plate");
            // Round TU2: the quiet line about the terms under the code, on
            // a sign-in alone.
            assert_eq!(
                h.ui.bounds(SIGN_IN_TERMS).is_some(),
                !connect,
                "{what} {size:?}: the terms' link"
            );
            // Measured as the window measures (the harness's frames measure
            // by its own estimate).
            h.ui.forget_metrics();
            if let Some(p) = pictures.draw(&mut h.ui, size) {
                common::online::keep(&format!("sign-in-code-{what}"), size, lang, &p);
            }
            // The plate's middle, where the code is written (the plate's
            // box needs no measured text: without a GPU the frame is laid
            // out undrawn and the code's own box is not there to aim at).
            h.tap(PointerKind::Mouse, CODE);
            h.run(0.3);
            let copied: Vec<&UiRequest> = h
                .requests
                .iter()
                .filter(|r| matches!(r, UiRequest::CopyText(t) if t == "K7M-X3Q"))
                .collect();
            assert_eq!(copied.len(), 1, "{what} {size:?}");
            // Measured as the window measures (the harness's frames measure
            // by its own estimate).
            h.ui.forget_metrics();
            if let Some(p) = pictures.draw(&mut h.ui, size) {
                common::online::keep(&format!("sign-in-code-{what}-copied"), size, lang, &p);
            }
        }
    }
}

/// An invite's page (round DC): signed out, its Join signs in, and the
/// attempt's code is on the same plate, copied by a click on the plate
/// (aimed at the plate's middle: without a GPU the frame is laid out
/// undrawn and the code's own box is not there to aim at).
#[test]
fn an_invites_page_shows_the_sign_in_code_on_its_plate() {
    common::indexes();
    let mut pictures = common::gpu::Pictures::new();
    for (size, lang) in [([1280u32, 720u32], "en"), ([640, 400], "en"), ([1280, 720], "ja")] {
        assert!(ddi_l10n::set_language(lang));
        let online = Online::new(
            Box::new(Attempt {
                signed_in: false,
                out: VecDeque::new(),
            }),
            OnlineConfig {
                platform: Platform::Desktop,
                site: "http://localhost:8080/".into(),
            },
        )
        .shared();
        online.borrow_mut().check();
        online.borrow_mut().poll(0);
        let env = env();
        let state = env.state.clone();
        let css = [size[0] as f32, size[1] as f32];
        let look = ddi_theme::Infinito::new();
        let design = ddi_ui::screens::design_for(&look, css);
        let mut h = Harness::with_gestures(design, css, 1.0, TextMode::Own, false);
        h.ui.set_focus_ring(false);
        h.push(OnlineScreen::new(
            env,
            online.clone(),
            OnlinePage::Invite("Room000001.abcdefghijABCDEFGHIJ01".into()),
            Rc::new(|_, _| {}),
        ));
        h.run(0.5);
        online.borrow_mut().poll(0);
        h.run(0.3);
        online.borrow_mut().join_invite();
        online.borrow_mut().poll(0);
        h.run(0.5);
        // The plate is the page's item `button:code`.
        let n = state
            .borrow()
            .page_items
            .iter()
            .position(|i| i == "button:code")
            .unwrap_or_else(|| panic!("{size:?} {lang}: {:?}", state.borrow().page_items));
        let plate = format!("Item{n}");
        let b = h.ui.bounds(&plate).expect("the code's plate");
        assert!(b[3] - b[1] >= 60.0, "{b:?}");
        h.ui.forget_metrics();
        if let Some(p) = pictures.draw(&mut h.ui, size) {
            common::online::keep("sign-in-code-invite", size, lang, &p);
        }
        h.tap(PointerKind::Mouse, &plate);
        h.run(0.3);
        let copied = h
            .requests
            .iter()
            .filter(|r| matches!(r, UiRequest::CopyText(t) if t == "K7M-X3Q"))
            .count();
        assert_eq!(copied, 1, "{size:?} {lang}");
        h.ui.forget_metrics();
        if let Some(p) = pictures.draw(&mut h.ui, size) {
            common::online::keep("sign-in-code-invite-copied", size, lang, &p);
        }
    }
}

/// Round TU2 (the maintainer, 2026-11-27: "Let's go with the quiet
/// line"): the sign-in dialog before its press says that signing in means
/// agreeing to the terms of use, the terms' and the privacy note's links
/// after it, inside the panel and above its buttons; each link opens its
/// page (a request here), the dialog staying.
#[test]
fn signing_in_says_it_agrees_to_the_terms() {
    common::indexes();
    let mut pictures = common::gpu::Pictures::new();
    for (size, lang) in [([1280u32, 720u32], "en"), ([640, 400], "en"), ([1280, 720], "ja")] {
        assert!(ddi_l10n::set_language(lang));
        let online = Online::new(
            Box::new(Attempt {
                signed_in: false,
                out: VecDeque::new(),
            }),
            OnlineConfig {
                platform: Platform::Desktop,
                site: "http://localhost:8080/".into(),
            },
        )
        .shared();
        online.borrow_mut().check();
        online.borrow_mut().poll(0);
        let css = [size[0] as f32, size[1] as f32];
        let look = ddi_theme::Infinito::new();
        let design = ddi_ui::screens::design_for(&look, css);
        let mut h = Harness::with_gestures(design, css, 1.0, TextMode::Own, false);
        h.ui.set_focus_ring(false);
        h.push(SignInDialog::new(env(), online.clone(), Dest::Create, Rc::new(Cell::new(true))));
        h.run(0.5);
        let panel = h.ui.bounds("Panel").expect("the dialog");
        let confirm = h.ui.bounds("Confirm").expect("its press");
        for name in ["SignIn.Agree0", SIGN_IN_TERMS, SIGN_IN_PRIVACY] {
            let b = h.ui.bounds(name).unwrap_or_else(|| panic!("{size:?} {lang}: {name}"));
            assert!(b[0] >= panel[0] && b[2] <= panel[2], "{name} {b:?} in {panel:?}");
            assert!(b[3] <= confirm[1], "{name} {b:?} above {confirm:?}");
        }
        h.ui.forget_metrics();
        if let Some(p) = pictures.draw(&mut h.ui, size) {
            common::online::keep("sign-in-terms-before", size, lang, &p);
        }
        h.tap(PointerKind::Mouse, SIGN_IN_TERMS);
        h.run(0.3);
        let want = if lang == "ja" { "terms.html#ja" } else { "terms.html" };
        assert!(
            h.requests
                .iter()
                .any(|r| matches!(r, UiRequest::OpenUrl(u) if u == want)),
            "{:?}",
            h.requests
        );
        assert!(h.ui.bounds("Panel").is_some(), "the dialog stays");
    }
    assert!(ddi_l10n::set_language("en"));
}
