//! Calibration's menu on desktop (`docs/plans/desktop.md`, 7.6): the UI
//! core's page (`ddi_ui::screens::calibration`, the web's `#/calibrate`)
//! with its buttons wired: a test starts as a play does
//! (`crate::play::host`, which opens the menu again when the test is
//! left), Options opens the options, the reset is asked first.

use std::rc::Rc;

use ddi_session::calibration::{CalMode, MAX_OFFSET};
use ddi_ui::Cx;
use ddi_ui::message::Message;
use ddi_ui::screens::Env;
use ddi_ui::screens::calibration::{self, CalAct};
use ddi_ui::screens::options_dialogs::ConfirmDialog;
use ddi_ui::screens::page::{PAGE_CHANGED, PageCall, PageDoc, PageFeed, PageScreen, PageShell};
use ddi_ui::state::settings::Settings;

use crate::preview::SharedPlayer;
use crate::songs::Songs;

/// The menu on the stored settings.
pub fn doc() -> PageDoc {
    calibration::doc(&Settings::load(), MAX_OFFSET * 1000.0)
}

/// The menu: its tests played through `songs`' play call.
pub fn menu(env: &Env, songs: &Rc<Songs>, sound: Option<SharedPlayer>) -> PageScreen {
    let feed = PageFeed::default();
    let shell: PageShell = {
        let (env, songs, feed) = (env.clone(), songs.clone(), feed.clone());
        Rc::new(move |call, cx| match call {
            PageCall::Leave => {
                cx.close(true);
                None
            }
            PageCall::Button(id) => button(&env, &songs, sound.as_ref(), &feed, &id, cx),
        })
    };
    PageScreen::new(env.clone(), "calibration", doc()).with_shell(shell, feed)
}

fn button(
    env: &Env,
    songs: &Rc<Songs>,
    sound: Option<&SharedPlayer>,
    feed: &PageFeed,
    id: &str,
    cx: &mut Cx,
) -> Option<PageDoc> {
    match calibration::button(id) {
        CalAct::Run(mode) => {
            let (auto, bias) = songs.testing.get();
            let mut r = crate::play::Request::calibration(CalMode::parse(&mode), auto);
            r.bias = bias;
            *songs.play.borrow_mut() = Some(r);
            // The test's prompt takes its place; leaving the test opens
            // the menu again, on the offsets as they are then.
            cx.close(false);
            None
        }
        CalAct::Options => {
            let (screen, shared) = crate::options::screen(env, sound.cloned(), Some(songs.clone()));
            *songs.options.borrow_mut() = Some(shared);
            songs.tell_options();
            cx.replace(screen);
            None
        }
        CalAct::ConfirmReset => {
            let (title, body, yes) = calibration::reset_texts();
            let feed = feed.clone();
            cx.overlay(
                ConfirmDialog::new(
                    env.clone(),
                    &title,
                    &body,
                    &yes,
                    Rc::new(move |cx| {
                        let mut s = Settings::load();
                        calibration::reset(&mut s);
                        *feed.borrow_mut() = Some(doc());
                        cx.broadcast(Message::new(PAGE_CHANGED));
                    }),
                ),
                true,
            );
            None
        }
        CalAct::Nothing => None,
    }
}
