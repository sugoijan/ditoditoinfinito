//! Options on desktop (`docs/plans/desktop.md`, 7.4): the theme's options
//! screen on the player's settings (`storage.json`), through the shared
//! shell (`ddi_ui::screens::options_shell`), as the web's options page
//! runs it. What only the platform does is [`Desktop`]'s: the volume
//! sample on the preview player, the controllers connected and what they
//! hold (for the bind flow), the settings as edited (the field preview,
//! the menus' bindings), leaving for the songs; with the library on disk,
//! its song folders (added through the system's picker, taken off the
//! list, read again) and the history's export to a file.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use ddi_engine::ConnectedPad;
use ddi_ui::message::Message;
use ddi_ui::screens::options::{OPTIONS_CHANGED, OptionsFeed, OptionsScreen};
use ddi_ui::screens::options_model::{OptionsModel, Reset, World};
use ddi_ui::screens::options_shell::{self, Leave, OptionsPlatform, SharesFeed};
use ddi_ui::screens::{Env, sample};
use ddi_ui::state::settings::Settings;
use ddi_ui::{Cx, Ui};

use crate::preview::SharedPlayer;

/// What the options screen and the shell share.
pub struct Shared {
    pub model: Rc<RefCell<OptionsModel>>,
    pub feed: OptionsFeed,
    /// The controls each controller (by id) holds now, kept by the shell.
    pub held: RefCell<std::collections::BTreeMap<String, HashSet<String>>>,
    /// Settings changed since the shell last took them.
    pub edited: RefCell<Option<Settings>>,
    /// Rows changed outside a call (the scan): the shell tells the screen.
    pending: std::cell::Cell<bool>,
    env: Env,
    sound: Option<SharedPlayer>,
    songs: Option<Rc<crate::songs::Songs>>,
    /// The noteskins of the song folders, listed on a thread of their own
    /// (`crate::library::noteskins`): put in the rows when they come.
    skins: RefCell<Option<std::sync::mpsc::Receiver<Skins>>>,
    /// The shares read (or one deleted and the rest read) on a thread
    /// (`crate::share`): into the dialog's feed when they come.
    shares: RefCell<
        Option<(
            SharesFeed,
            std::sync::mpsc::Receiver<ddi_ui::screens::data::SharesView>,
        )>,
    >,
}

impl Shared {
    /// The controllers connected changed: the rows say so.
    pub fn set_pads(&self, pads: Vec<ConnectedPad>, ui: &mut Ui) {
        if pads == self.model.borrow().world.connected {
            return;
        }
        self.model.borrow_mut().world.connected = pads;
        *self.feed.borrow_mut() = Some(self.model.borrow().data());
        ui.broadcast(Message::new(OPTIONS_CHANGED));
    }

    /// The song folders changed (or what the scan found): the rows say
    /// so, the next frame.
    pub fn set_folders(&self, folders: Vec<String>, note: String) {
        {
            let mut m = self.model.borrow_mut();
            if m.world.folders.as_ref() == Some(&folders) && m.world.library_note == note {
                return;
            }
            m.world.folders = Some(folders);
            m.world.library_note = note;
        }
        self.refresh();
    }

    /// The play history's import: the plays kept so far while it runs
    /// (`None`: none runs), its report when it ends, the plays kept now.
    /// The rows say so, the next frame.
    pub fn set_import(
        &self,
        importing: Option<usize>,
        report: Option<ddi_ui::state::history::ImportReport>,
        plays: usize,
    ) {
        {
            let mut m = self.model.borrow_mut();
            // `None`: the notice as it is.
            let notice = match (&report, importing) {
                (Some(r), _) => Some(r.notice()),
                (None, Some(0)) => Some("Importing plays…".to_string()),
                (None, Some(n)) => Some(format!("Importing plays… {n} kept so far.")),
                (None, None) => None,
            };
            if m.world.importing_plays == importing.is_some()
                && m.world.plays == plays
                && notice.as_ref().is_none_or(|n| *n == m.notice)
            {
                return;
            }
            m.world.importing_plays = importing.is_some();
            m.world.plays = plays;
            if let Some(n) = notice {
                m.notice = n;
            }
        }
        self.refresh();
    }

    /// The rows made again from the model: the screen is told the next
    /// frame.
    fn refresh(&self) {
        *self.feed.borrow_mut() = Some(self.model.borrow().data());
        self.pending.set(true);
    }

    /// The shares dialog's listing, when its thread is done: the feed
    /// filled, [`options_shell::SHARES_CHANGED`] to broadcast.
    pub fn take_shares(&self) -> bool {
        let got = self
            .shares
            .borrow()
            .as_ref()
            .and_then(|(feed, rx)| rx.try_recv().ok().map(|v| (feed.clone(), v)));
        let Some((feed, view)) = got else {
            return false;
        };
        *self.shares.borrow_mut() = None;
        *feed.borrow_mut() = view;
        true
    }

    /// Whether the rows changed since the screen was last told.
    pub fn take_pending(&self) -> bool {
        let skins = self
            .skins
            .borrow()
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        if let Some((skins, parameters)) = skins {
            *self.skins.borrow_mut() = None;
            // Not `complete`: a skin chosen on another machine stays chosen.
            self.model.borrow_mut().set_noteskins(skins, false, false);
            self.model.borrow_mut().set_skin_parameters(parameters);
            self.refresh();
        }
        self.pending.replace(false)
    }

    /// The settings as edited, once after each change.
    pub fn take_edited(&self) -> Option<Settings> {
        self.edited.borrow_mut().take()
    }

    /// The settings as the field preview shows them: as edited, on the
    /// layout picked for the preview.
    pub fn preview_settings(&self) -> Settings {
        self.model.borrow().preview_settings()
    }
}

/// The noteskins one can choose, `(key, name)`, and the parameters they
/// declare.
type Skins = (Vec<(String, String)>, crate::library::noteskins::Parameters);

/// The options screen on the stored settings, and what the shell keeps of
/// it.
pub fn screen(
    env: &Env,
    sound: Option<SharedPlayer>,
    songs: Option<Rc<crate::songs::Songs>>,
) -> (OptionsScreen, Rc<Shared>) {
    // The bundled skins at once; the folders' when their walk is done.
    let bundled = crate::library::noteskins::bundled();
    let noteskins = crate::library::noteskins::choices(&bundled);
    let skin_parameters = crate::library::noteskins::parameters(&bundled, &noteskins);
    let world = World {
        platform: ddi_ui::screens::data::Platform::Desktop,
        pads_supported: true,
        library_styles: songs.as_ref().map_or_else(Vec::new, |s| s.library_styles()),
        noteskins,
        skin_parameters,
        plays: songs
            .as_ref()
            .and_then(|s| s.plays.as_ref())
            .map_or(0, |p| p.history().len()),
        folders: songs.as_ref().map(|s| {
            s.folders()
                .iter()
                .map(|p| p.display().to_string())
                .collect()
        }),
        library_note: songs.as_ref().map(|s| s.folders_note()).unwrap_or_default(),
        sharing: Settings::load().sharing,
        share_url: crate::share::base(),
        ..World::default()
    };
    let model = Rc::new(RefCell::new(OptionsModel::new(Settings::load(), world)));
    let feed = OptionsFeed::default();
    let shared = Rc::new(Shared {
        model: model.clone(),
        feed: feed.clone(),
        held: RefCell::default(),
        edited: RefCell::default(),
        pending: std::cell::Cell::new(false),
        env: env.clone(),
        shares: RefCell::default(),
        skins: RefCell::new(songs.as_ref().map(|s| {
            let (tx, rx) = std::sync::mpsc::channel();
            let folders = s.folders();
            let wake = s.wake.borrow().clone();
            let _ = std::thread::Builder::new()
                .name("ddi-noteskins".into())
                .spawn(move || {
                    let skins = crate::library::noteskins::all(&folders);
                    let choices = crate::library::noteskins::choices(&skins);
                    let parameters = crate::library::noteskins::parameters(&skins, &choices);
                    let _ = tx.send((choices, parameters));
                    if let Some(w) = wake {
                        w();
                    }
                });
            rx
        })),
        sound,
        songs,
    });
    let shell = options_shell::shell(
        env.clone(),
        model.clone(),
        feed.clone(),
        Rc::new(Desktop(shared.clone())),
    );
    let data = model.borrow().data();
    let screen = OptionsScreen::new(env.clone(), data, None).with_shell(shell, feed);
    (screen, shared)
}

/// The options' platform on desktop.
struct Desktop(Rc<Shared>);

/// Today as the export's file name has it (`YYYY-MM-DD`, UTC).
fn today() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis()) as f64;
    let mut time = ddi_ui::state::history::local_time(ms, 0);
    // "YYYY-MM-DD HH:MM": the date.
    time.truncate(time.find(' ').unwrap_or(time.len()));
    time
}

impl Desktop {
    /// The shares read on a thread (after deleting `delete`, if any),
    /// polled by the shell ([`Shared::take_shares`]).
    fn shares_on_thread(&self, into: SharesFeed, delete: Option<String>) {
        let token = Settings::load().share_token;
        let (tx, rx) = std::sync::mpsc::channel();
        let wake = self.0.songs.as_ref().and_then(|s| s.wake.borrow().clone());
        *self.0.shares.borrow_mut() = Some((into, rx));
        let _ = std::thread::Builder::new()
            .name("ddi-shares".into())
            .spawn(move || {
                let result = match delete {
                    Some(id) => {
                        crate::share::delete(&token, &id).and_then(|()| crate::share::list(&token))
                    }
                    None => crate::share::list(&token),
                };
                let view = match result {
                    Ok(v) => v,
                    Err(e) => ddi_ui::screens::data::SharesView {
                        state: ddi_ui::screens::data::SharesState::Failed,
                        error: e,
                        ..Default::default()
                    },
                };
                let _ = tx.send(view);
                if let Some(w) = wake {
                    w();
                }
            });
    }
}

/// The song the volume sample plays: the first bundled one with music.
fn sample_song() -> Option<String> {
    sample::bundled().into_iter().map(|s| s.id).next()
}

impl OptionsPlatform for Desktop {
    fn sample(&self, play: bool, volume: f32, _cx: &mut Cx) -> bool {
        let (Some(sound), Some(song)) = (&self.0.sound, sample_song()) else {
            return false;
        };
        sound.borrow_mut().sample(&song, play, volume)
    }

    fn volume(&self, volume: f32) {
        if let Some(sound) = &self.0.sound {
            sound.borrow_mut().set_volume(volume);
        }
    }

    /// The share's song select over the options: Back closes it there.
    fn open_share(&self, id: &str, cx: &mut Cx) -> bool {
        let Some(songs) = &self.0.songs else {
            return false;
        };
        if let Some(sound) = &self.0.sound {
            sound.borrow_mut().pause();
        }
        cx.request(ddi_ui::UiRequest::FieldPreview(None));
        songs.open_share(id.to_string());
        cx.push(crate::songs::shared_screen(
            &self.0.env,
            songs,
            self.0.sound.clone(),
        ));
        true
    }

    fn list_shares(&self, into: SharesFeed) {
        self.shares_on_thread(into, None);
    }

    fn delete_share(&self, id: &str, into: SharesFeed) {
        self.shares_on_thread(into, Some(id.to_string()));
    }

    /// The history as JSON lines, one a play (the web's export), to a
    /// file the player names.
    fn export(&self) {
        let Some(plays) = self.0.songs.as_ref().and_then(|s| s.plays.clone()) else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("ditoditoinfinito-plays-{}.jsonl", today()))
            .save_file()
        else {
            return;
        };
        let out = Rc::new(RefCell::new(String::new()));
        let (shared, o) = (self.0.clone(), out.clone());
        plays.export(
            50,
            move |lines| o.borrow_mut().push_str(&lines),
            move |r| {
                let notice = match r.and_then(|n| {
                    std::fs::write(&path, out.borrow().as_bytes())
                        .map(|_| n)
                        .map_err(|e| e.to_string())
                }) {
                    Ok(n) => format!("{n} plays exported to {}.", path.display()),
                    Err(e) => format!("The export failed: {e}"),
                };
                shared.model.borrow_mut().notice = notice;
                shared.refresh();
            },
        );
    }

    /// Plays from an export the player picks, imported on a thread
    /// (`crate::history::Rework::import`); the rows follow it
    /// ([`Shared::set_import`]).
    fn import_plays(&self, _cx: &mut Cx) {
        let Some(songs) = &self.0.songs else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Play history", &["jsonl", "ndjson", "json", "txt"])
            .pick_file()
        else {
            return;
        };
        if !songs.import_plays(path) {
            self.0.model.borrow_mut().notice =
                "The plays cannot be imported now (an import runs, or the history is not read)."
                    .into();
            self.0.refresh();
        }
    }

    /// The song folders are forgotten (the settings' part, the folders'
    /// songs' offsets, is the model's); nothing in them is deleted.
    /// Noteskins are read in place too: none is stored to remove.
    fn remove(&self, reset: &Reset) {
        if matches!(reset, Reset::Songs | Reset::Everything)
            && let Some(s) = &self.0.songs
        {
            s.forget_all();
        }
        // The plays folder emptied (every play's two files): the history's
        // reset, and everything's.
        if matches!(reset, Reset::History | Reset::Everything)
            && let Some(plays) = self.0.songs.as_ref().and_then(|s| s.plays.clone())
        {
            let (shared, p) = (self.0.clone(), plays.clone());
            let alone = *reset == Reset::History;
            plays.remove_all(move |r| {
                {
                    let mut m = shared.model.borrow_mut();
                    match r {
                        Ok(()) if alone => m.notice = "Play history deleted.".into(),
                        Ok(()) => m.notice = "Everything reset.".into(),
                        Err(e) => m.notice = format!("Could not delete the play history: {e}"),
                    }
                    m.world.plays = p.history().len();
                }
                shared.refresh();
            });
        }
    }

    fn held(&self, id: &str) -> HashSet<String> {
        self.0.held.borrow().get(id).cloned().unwrap_or_default()
    }

    fn changed(&self, id: &str, model: &OptionsModel) {
        self.0.env.follow_cursor(&model.settings);
        // As the preview shows them: on the layout picked for it.
        *self.0.edited.borrow_mut() = Some(model.preview_settings());
        if id == "reduced_motion" {
            let env = &self.0.env;
            env.follow_motion(model.settings.reduced_motion, false);
        }
    }

    /// Back to the caller under it, or calibration's menu above it;
    /// the demo play is the web's (a WebGL2 check).
    fn leave(&self, to: Leave, cx: &mut Cx) {
        if let Some(sound) = &self.0.sound {
            sound.borrow_mut().pause();
        }
        cx.request(ddi_ui::UiRequest::FieldPreview(None));
        match to {
            Leave::Back => cx.close(false),
            Leave::Calibrate => {
                if let Some(songs) = &self.0.songs {
                    cx.push(crate::calibrate::menu(
                        &self.0.env,
                        songs,
                        self.0.sound.clone(),
                    ));
                }
            }
            Leave::Demo { .. } => {}
        }
    }

    fn add_folder(&self, _cx: &mut Cx) {
        if let Some(s) = &self.0.songs {
            s.add_folders(crate::os::pick(true));
        }
    }

    fn remove_folder(&self, index: usize) {
        if let Some(s) = &self.0.songs {
            s.remove_folder(index);
        }
    }

    fn rescan(&self) {
        if let Some(s) = &self.0.songs {
            s.rescan(s.folders());
        }
    }
}
