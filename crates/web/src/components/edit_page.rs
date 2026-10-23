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
use ddi_ui::{TextMode, Ui};
use web_sys::HtmlElement;

use crate::component::{Component, Context, NodeRef};
use crate::components::ui_dev::{self, Host, Started};
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
    root: NodeRef,
    host: Rc<RefCell<Option<std::rc::Weak<RefCell<Host>>>>>,
    started: Option<Started>,
    failure: Option<String>,
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
            root: NodeRef::default(),
            host: Rc::new(RefCell::new(None)),
            started: None,
            failure: None,
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
                    self.failure = Some("Only StepMania and DWI songs can be edited.".into());
                    true
                }
                Ok(song) => {
                    let props = ctx.props();
                    let editor = crate::editor::begin(&props.song, Editor::open(song, props.chart));
                    self.start(ctx, editor);
                    false
                }
                Err(e) => {
                    self.failure = Some(format!("The song could not be read for editing: {e}"));
                    true
                }
            },
            Msg::Failed(e) => {
                self.failure = Some(e);
                if let Some((host, _, pads)) = &self.started {
                    ui_dev::stop_ui(host, pads.as_ref());
                }
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) {
        if let Some(e) = &self.failure {
            crate::component::no_gpu(ctx.root(), e);
            return;
        }
        ctx.root().set_class_name("select-page");
        self.root.set(ctx.root());
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        if let Some((host, _, pads)) = &self.started {
            ui_dev::stop_ui(host, pads.as_ref());
        }
        crate::web::forget_debug(&["ui"]);
    }
}

impl EditPage {
    fn start(&mut self, ctx: &Context<Self>, editor: Rc<RefCell<Editor>>) {
        let Some(el) = self.root.cast::<HtmlElement>() else {
            return;
        };
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
        let status = if std::mem::take(&mut self.recovered) {
            "Unsaved changes recovered from the last time; leave without saving to drop them"
        } else if crate::editor::fresh(&id) {
            ddi_ui::state::editor::new_song::STATUS
        } else {
            ""
        };
        let screen = EditorScreen::shared(theme.env().clone(), editor, crate::editor::place(&id))
            .with_host(host)
            .with_status(status);
        let mut ui = Ui::new(ddi_ui::screens::DESIGN, TextMode::Mirror, true);
        ui.set_focus_ring(false);
        ui.push(screen);
        match ui_dev::start_ui(&el, ui, Some(theme), Some(debug)) {
            Ok(started) => {
                *self.host.borrow_mut() = Some(Rc::downgrade(&started.0));
                let link = ctx.link().clone();
                ui_dev::attach_gfx(started.0.clone(), move |e| {
                    link.send_message(Msg::Failed(e))
                });
                // The field in the player's skin, drawn before the 2D pass.
                ui_dev::set_frame_hook(&started.0, Some(crate::editor_field::hook()));
                self.started = Some(started);
            }
            Err(e) => ctx.link().send_message(Msg::Failed(e)),
        }
    }
}
