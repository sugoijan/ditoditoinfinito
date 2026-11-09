//! The chart editor at `#/edit?song=<id>&chart=<n>` (`docs/plans/editor.md`):
//! the UI core's editor screen (`ddi_ui::screens::editor`) on the app's
//! canvas over the whole window, as song select is, on the song read
//! from the library (bundled or imported) or, back from a playtest, on
//! the session's (`crate::editor`).

use std::cell::RefCell;
use std::rc::Rc;

use ddi_chart::Song;
use ddi_ui::screens::editor::EditorScreen;
use ddi_ui::state::editor::Editor;

use crate::component::{Component, Context};
use crate::components::ui_dev::{self, Host, UiPage};
use crate::editor::WebEditorHost;

#[derive(Clone, PartialEq)]
pub(crate) struct Props {
    pub(crate) song: String,
    pub(crate) chart: usize,
}

pub(crate) enum Msg {
    Loaded(Box<Result<Song, String>>),
    Failed(String),
}

pub(crate) struct EditPage {
    ui: UiPage,
    host: Rc<RefCell<Option<std::rc::Weak<RefCell<Host>>>>>,
    /// The song came back from its autosave.
    recovered: bool,
}

impl Component for EditPage {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let id = ctx.props().song.clone();
        let mut recovered = false;
        if crate::editor::editing(&id).is_none() {
            // Unsaved changes from before (a reload, a closed tab) come back.
            if let Some(a) = crate::editor::autosaved(&id) {
                crate::editor::begin(&id, Editor::recover(a));
                recovered = true;
            } else {
                ctx.link().send_future(async move {
                    let song = crate::songs::load_chart(&id).await.map_err(|e| e.message());
                    Msg::Loaded(Box::new(song))
                });
            }
        }
        EditPage {
            ui: UiPage::default(),
            host: Rc::new(RefCell::new(None)),
            recovered,
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if first_render && let Some(editor) = crate::editor::editing(&ctx.props().song) {
            self.start(ctx, editor);
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Loaded(r) => match *r {
                Ok(song)
                    if !matches!(
                        song.source.format,
                        ddi_chart::SourceFormat::Sm
                            | ddi_chart::SourceFormat::Ssc { .. }
                            | ddi_chart::SourceFormat::Dwi
                    ) =>
                {
                    self.ui
                        .fail("Only StepMania and DWI songs can be edited.".into());
                    true
                }
                Ok(song) => {
                    let props = ctx.props();
                    let editor = crate::editor::begin(&props.song, Editor::open(song, props.chart));
                    self.start(ctx, editor);
                    false
                }
                Err(e) => {
                    self.ui
                        .fail(format!("The song could not be read for editing: {e}"));
                    true
                }
            },
            Msg::Failed(e) => {
                self.ui.fail(e);
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) {
        self.ui.view(ctx, "select-page");
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.ui.destroy(&["ui"]);
    }
}

impl EditPage {
    fn start(&mut self, ctx: &Context<Self>, editor: Rc<RefCell<Editor>>) {
        let id = ctx.props().song.clone();
        let theme = ui_dev::theme_host(Rc::new(ui_dev::nfkd));
        // The music decoded now, for the first press to play it.
        let listen = Rc::new(RefCell::new(crate::editor::Listen::default()));
        crate::editor::Listen::read(&listen, &id, editor.borrow().chart_index(), None);
        let host = Rc::new(WebEditorHost {
            id: id.clone(),
            ui: self.host.clone(),
            listen,
        });
        let debug = crate::editor::debug(&host.listen);
        let status: String = if std::mem::take(&mut self.recovered) {
            ddi_l10n::msg::editor::status_recovered().into()
        } else if crate::editor::fresh(&id) {
            ddi_l10n::msg::editor::status_new_song().into()
        } else {
            String::new()
        };
        let screen = EditorScreen::shared(theme.env().clone(), editor, crate::editor::place(&id))
            .with_host(host)
            .with_status(&status);
        let mut ui = crate::components::ui_dev::theme_ui();
        ui.push(screen);
        let Some(started) = self
            .ui
            .start(ctx, ui, Some(theme), Some(debug), Msg::Failed)
        else {
            return;
        };
        *self.host.borrow_mut() = Some(Rc::downgrade(&started.0));
        // The field in the player's skin and the audio view's picture,
        // before the 2D pass.
        ui_dev::set_frame_hook(
            &started.0,
            Some(crate::editor_audio::hook(crate::editor_field::hook())),
        );
    }
}
