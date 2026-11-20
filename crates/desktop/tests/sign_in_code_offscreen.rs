//! Desktop's sign-in code (round SF; the maintainer, 2026-11-20: "On the
//! desktop app, with the twitch device code flow the code should be
//! rendered with emphasis and with copy on click behavior"): the sign-in
//! dialog and the Twitch access trip's show the attempt's code large on a
//! plate of its own in the game's look, and a click on it asks the shell
//! to copy it (a request here: the real clipboard is never written by a
//! test), "Copied" in its place for a moment. Drawn offscreen where there
//! is a GPU, at 1280×720 and in a small window (`DDI_OFFSCREEN_PPM=<dir>`
//! keeps `sign-in-code-<what>-<w>x<h>-<lang>.ppm`).

mod common;
use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;

use ddi_ui::screens::online_home::{CODE, SignInDialog};
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
