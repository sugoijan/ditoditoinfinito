//! The options screen's shell on the player's settings, for any platform:
//! a call changes the [`OptionsModel`] (saved as it goes) or opens a
//! dialog (bind, a key, a confirmation); what only the platform can do
//! (the volume sample, the export, removing songs, going elsewhere) goes
//! to its [`OptionsPlatform`].

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::Env;
use super::options::{OPTIONS_CHANGED, OptionsCall, OptionsFeed, OptionsShell};
use super::options_dialogs::{BindDialog, ConfirmDialog, KeyDialog};
use super::options_model::{Act, OptionsModel, Reset, Sample};
use crate::message::Message;
use crate::screen::Cx;
use crate::state::bind::BindFlow;
use ddi_platform::DeviceId;

/// Where the options lead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leave {
    /// Return to the screen that opened Options.
    Back,
    Calibrate,
    /// Autoplay a bundled song (on WebGL2).
    Demo {
        gl: bool,
    },
}

/// What the platform does for the options.
pub trait OptionsPlatform {
    /// Starts (inside the press: an audio context is made) or stops the
    /// volume sample; whether it plays now.
    fn sample(&self, play: bool, volume: f32, cx: &mut Cx) -> bool;
    /// The volume changed (a sample playing follows it).
    fn volume(&self, _volume: f32) {}
    /// Downloads the play history (the platform says how it went, in
    /// the model's notice, later).
    fn export(&self);
    /// Imports plays from an export the player picks (the picker, inside
    /// the press); the platform says how it went in the model's notice
    /// and `World::importing_plays`.
    fn import_plays(&self, _cx: &mut Cx) {}
    /// The storage's part of a confirmed reset or removal (imported songs,
    /// a noteskin), after the settings' part is done.
    fn remove(&self, reset: &Reset);
    /// The controls controller `id` holds now.
    fn held(&self, _id: &str) -> HashSet<String> {
        HashSet::new()
    }
    /// A setting changed (`reduced_motion` is followed at once).
    fn changed(&self, _id: &str, _model: &OptionsModel) {}
    /// Goes to another screen.
    fn leave(&self, to: Leave, cx: &mut Cx);
    /// Song folders (desktop, `World::folders`): add one (the picker,
    /// inside the press), forget the one at `index`, read them all again.
    /// The platform puts the new list in the model and sends it.
    fn add_folder(&self, _cx: &mut Cx) {}
    fn remove_folder(&self, _index: usize) {}
    fn rescan(&self) {}
    /// Reads the shares the token may see into `into` (state `Loading`
    /// meanwhile) and broadcasts [`SHARES_CHANGED`] when they are there;
    /// `delete_share` deletes one and reads them again the same way.
    fn list_shares(&self, _into: SharesFeed) {}
    fn delete_share(&self, _id: &str, _into: SharesFeed) {}
    /// Opens share `id` in the game (its song select, over the options);
    /// `false` where the platform cannot, and the link opens instead.
    fn open_share(&self, _id: &str, _cx: &mut Cx) -> bool {
        false
    }
}

/// The shares dialog's listing, filled by the shell
/// ([`OptionsPlatform::list_shares`]).
pub type SharesFeed = Rc<RefCell<super::data::SharesView>>;

/// The listing in a [`SharesFeed`] changed: its dialog redraws.
pub const SHARES_CHANGED: &str = "SharesChanged";

/// The shell for an options screen on `model`; dialogs' changes come
/// back through `feed`.
pub fn shell(
    env: Env,
    model: Rc<RefCell<OptionsModel>>,
    feed: OptionsFeed,
    platform: Rc<dyn OptionsPlatform>,
) -> OptionsShell {
    Rc::new(move |call, cx| call_with(&env, &model, &feed, &platform, call, cx))
}

/// The rows, now, to the screen from a dialog.
fn send(model: &Rc<RefCell<OptionsModel>>, feed: &OptionsFeed, cx: &mut Cx) {
    *feed.borrow_mut() = Some(model.borrow().data());
    cx.broadcast(Message::new(OPTIONS_CHANGED));
}

/// The connected controllers with a standard mapping, by id.
fn standard_pads(m: &OptionsModel) -> Vec<String> {
    m.world
        .connected
        .iter()
        .filter(|p| p.standard)
        .map(|p| p.id.clone())
        .collect()
}

fn call_with(
    env: &Env,
    model: &Rc<RefCell<OptionsModel>>,
    feed: &OptionsFeed,
    platform: &Rc<dyn OptionsPlatform>,
    call: OptionsCall,
    cx: &mut Cx,
) -> Option<super::data::OptionsData> {
    let act = match call {
        OptionsCall::Change { id, index } => {
            if !model.borrow_mut().change(&id, index) {
                return None;
            }
            if id == "volume" {
                let (volume, ready) = {
                    let m = model.borrow();
                    (m.settings.volume, m.world.sample == Sample::Ready)
                };
                platform.volume(volume);
                // The sample plays as the volume changes (from a key, a
                // pad or a tap: inside the gesture; a drag's moves are
                // not one, and it starts at the release).
                if ready && cx.in_gesture() && platform.sample(true, volume, cx) {
                    model.borrow_mut().world.sample = Sample::Playing;
                }
            }
            platform.changed(&id, &model.borrow());
            return Some(model.borrow().data());
        }
        OptionsCall::Leave => {
            platform.sample(false, model.borrow().settings.volume, cx);
            platform.leave(Leave::Back, cx);
            return None;
        }
        OptionsCall::Action { id, button } => model.borrow_mut().action(&id, button),
    };
    match act {
        Act::Done => return Some(model.borrow().data()),
        Act::Nothing => {}
        Act::Calibrate | Act::Demo { .. } => {
            let volume = model.borrow().settings.volume;
            platform.sample(false, volume, cx);
            if model.borrow().world.sample != Sample::None {
                model.borrow_mut().world.sample = Sample::Ready;
            }
            platform.leave(
                match act {
                    Act::Demo { gl } => Leave::Demo { gl },
                    _ => Leave::Calibrate,
                },
                cx,
            );
        }
        Act::Export => platform.export(),
        Act::ImportPlays => platform.import_plays(cx),
        Act::AddFolder => platform.add_folder(cx),
        Act::RemoveFolder(i) => platform.remove_folder(i),
        Act::Rescan => platform.rescan(),
        Act::Sample => {
            let (playing, volume) = {
                let m = model.borrow();
                (m.world.sample == Sample::Playing, m.settings.volume)
            };
            let now = platform.sample(!playing, volume, cx);
            let mut m = model.borrow_mut();
            if m.world.sample != Sample::None {
                m.world.sample = if now { Sample::Playing } else { Sample::Ready };
            }
            return Some(m.data());
        }
        Act::Bind(device) => {
            let (layout, controls, standard) = {
                let m = model.borrow();
                let standard = standard_pads(&m);
                (m.bind_layout.clone(), m.settings.controls.clone(), standard)
            };
            // A controller bound from its row may hold something already.
            let held = match &device {
                Some(DeviceId::Gamepad(id)) => platform.held(id),
                _ => HashSet::new(),
            };
            let flow = BindFlow::new(device, &layout, controls, standard, held);
            let (m, f) = (model.clone(), feed.clone());
            cx.overlay(
                BindDialog::new(
                    env.clone(),
                    flow,
                    &layout.name,
                    Rc::new(move |bound, cx| {
                        m.borrow_mut().bound(bound);
                        send(&m, &f, cx);
                    }),
                ),
                true,
            );
        }
        Act::AddKey(lane) => {
            let name = model
                .borrow()
                .bind_layout
                .lanes
                .get(lane)
                .map(|l| l.label().to_string())
                .unwrap_or_default();
            let (controls, standard) = {
                let m = model.borrow();
                (m.settings.controls.clone(), standard_pads(&m))
            };
            let (m, f) = (model.clone(), feed.clone());
            cx.overlay(
                KeyDialog::new(
                    env.clone(),
                    &name,
                    Rc::new(move |code, cx| {
                        m.borrow_mut().add_key(lane, &code);
                        send(&m, &f, cx);
                    }),
                )
                .with_pads(controls, standard),
                true,
            );
        }
        Act::AddButton { id, lane } => {
            let (name, controls, standard) = {
                let m = model.borrow();
                let name = m
                    .bind_layout
                    .lanes
                    .get(lane)
                    .map(|l| l.label().to_string())
                    .unwrap_or_default();
                (name, m.settings.controls.clone(), standard_pads(&m))
            };
            let (m, f, pad) = (model.clone(), feed.clone(), id.clone());
            cx.overlay(
                KeyDialog::for_pad(
                    env.clone(),
                    &name,
                    &id,
                    controls,
                    standard,
                    Rc::new(move |control, cx| {
                        m.borrow_mut().add_pad_control(&pad, lane, &control);
                        send(&m, &f, cx);
                    }),
                ),
                true,
            );
        }
        Act::ShareToken => {
            let token = model.borrow().settings.share_token.clone();
            let (m, f) = (model.clone(), feed.clone());
            cx.overlay(
                super::options_dialogs::TokenDialog::new(
                    env.clone(),
                    &token,
                    Rc::new(move |token, cx| {
                        m.borrow_mut().set_share_token(token);
                        send(&m, &f, cx);
                    }),
                ),
                true,
            );
        }
        Act::ListShares => {
            let into: SharesFeed = Rc::default();
            platform.list_shares(into.clone());
            let (p, o) = (platform.clone(), platform.clone());
            cx.overlay(
                super::options_dialogs::SharesDialog::new(
                    env.clone(),
                    into.clone(),
                    Rc::new(move |id| p.delete_share(id, into.clone())),
                    Rc::new(move |id, cx| o.open_share(id, cx)),
                ),
                true,
            );
        }
        Act::Confirm(reset) => {
            let (title, body, yes) = reset.texts(&model.borrow());
            let (m, f, p) = (model.clone(), feed.clone(), platform.clone());
            let history = reset == Reset::History;
            let dialog = ConfirmDialog::new(
                env.clone(),
                &title,
                &body,
                &yes,
                Rc::new(move |cx| {
                    m.borrow_mut().reset(&reset);
                    {
                        let model = m.borrow();
                        p.volume(model.settings.volume);
                        p.changed("reduced_motion", &model);
                    }
                    p.remove(&reset);
                    send(&m, &f, cx);
                }),
            );
            // The history: its export offered first.
            let dialog = if history {
                let p = platform.clone();
                dialog.with_extra("Export first", Rc::new(move |_cx| p.export()))
            } else {
                dialog
            };
            cx.overlay(dialog, true);
        }
    }
    None
}
