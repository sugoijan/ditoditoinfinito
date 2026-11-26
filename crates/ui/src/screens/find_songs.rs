//! Find songs in a first form (`docs/plans/source-plugins.md`, stage 2;
//! desktop only, the module absent from the web's build): a page to try
//! song sources from, "This computer" (folders the player names, searched
//! without adding them to the library) and the plugins switched on. A
//! source chooser where more than one is on, This computer's folders, a
//! search field (searched as it is typed), the results as a list, each
//! with Play (its songs listed alone, played without importing) and
//! Import (written into the library). The real Find songs screen is the
//! plan's stage 6.
//!
//! The page is made from [`FindView`], what the shell's model shows now;
//! a press goes to the model ([`FindPress`]), which asks the sources.
//! Every text is a message or data: a plugin's name, a song's title, a
//! folder's path and a source's own words (in Details) are data.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_l10n::UiText;
use ddi_l10n::msg::sources as m;

use super::Env;
use super::page::{
    Action, Block, FedPage, PageCall, PageDoc, PageFeed, PageModel, PageScreen, PageShell,
    PageTitle,
};
use crate::screen::Cx;

/// A source as the page names it.
#[derive(Clone, Debug, PartialEq)]
pub enum SourceName {
    /// The folder search built into the app.
    ThisComputer,
    /// A plugin, by the name it gives (data).
    Plugin(String),
}

/// A source the page offers.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceView {
    pub id: String,
    pub name: SourceName,
    /// Why it cannot be asked now (off for the run, failed to start).
    pub problem: Option<Problem>,
}

/// One result: an item of the source's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResultView {
    pub id: String,
    /// Its title (data).
    pub title: String,
    /// Its line under the title, as tags shown joined by " · ": its
    /// artist and pack as the source states them (data), its charts (each
    /// [`chart_tag`]).
    pub tags: Vec<String>,
    /// The library has a song of the same charts already.
    pub in_library: bool,
}

/// A chart of a result, as a tag of its line: its style's name and its
/// level.
pub fn chart_tag(style: &str, meter: Option<u32>) -> String {
    let name = super::data::style_name(style, style);
    match meter {
        Some(m) => m::chart(name, m as f64).into(),
        None => name.into(),
    }
}

/// What a source's step is, as the page words it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Resolving,
    Searching,
    Downloading,
    Reading,
    Converting,
    Checking,
}

/// What runs now.
#[derive(Clone, Debug, PartialEq)]
pub enum Busy {
    Searching,
    /// A result fetched (for Play or Import), how far.
    Fetching {
        title: String,
        step: Option<Step>,
        done: Option<u64>,
        total: Option<u64>,
    },
    /// A fetched result written into the library.
    Importing { title: String },
}

/// Why something failed, worded by the page (a source's own words go in
/// Details, as data).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Busy,
    InvalidInput,
    Unsupported,
    NotFound,
    Refused,
    RateLimited,
    Unreachable,
    TooLarge,
    Unreadable,
    Changed,
    Failed,
    /// Its process stopped working, or exited while working.
    Crashed,
    TimedOut,
    /// It broke the protocol and was stopped.
    Misbehaved,
    /// It could not be started.
    NotStarted,
    /// It needs a newer game.
    NeedsNewerGame,
    /// It is too old for this game.
    TooOld,
    /// Off until the game starts again (it failed too often).
    OffForTheRun,
    /// Not switched on, or no longer installed.
    SwitchedOff,
    /// The game stopped it.
    Stopped,
    /// The import could not write the song into the library.
    ImportFailed,
}

/// What the page says once, quietly.
#[derive(Clone, Debug, PartialEq)]
pub enum Note {
    /// A result was imported: its songs are in the library now.
    Imported { songs: usize },
    /// Some folders were too large to read whole (their bounds).
    Partial,
}

/// What the page shows now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FindView {
    pub sources: Vec<SourceView>,
    /// The source chosen, an index of `sources`.
    pub chosen: usize,
    /// This computer's folders, while it is the source chosen (paths,
    /// data).
    pub folders: Option<Vec<String>>,
    pub query: String,
    /// A search was made (an empty list then means none was found).
    pub searched: bool,
    pub results: Vec<ResultView>,
    /// How many the source has in all, where it said.
    pub total: Option<u64>,
    /// Another page of results is there.
    pub more: bool,
    pub busy: Option<Busy>,
    pub problem: Option<Problem>,
    /// The source's own words on the problem, or its diagnostics (data).
    pub details: Option<String>,
    pub note: Option<Note>,
}

/// What a press asks of the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FindPress {
    Source(usize),
    AddFolder,
    RemoveFolder(usize),
    Search,
    More,
    Play(usize),
    Import(usize),
    Cancel,
}

/// The shell's side of the page: what it shows, and what a press does.
pub trait FindModel {
    /// Moves whenever [`FindModel::view`] may have changed.
    fn version(&self) -> u64;
    fn view(&self) -> FindView;
    /// A press, in the input's handler (the folder picker opens there).
    fn press(&mut self, press: FindPress, cx: &mut Cx);
    /// The search field's text, as typed (the model searches once it
    /// rests).
    fn typed(&mut self, query: String);
    /// Each frame: the sources' answers taken.
    fn tick(&mut self, cx: &mut Cx);
    /// The page is left (Back).
    fn leave(&mut self, cx: &mut Cx);
}

pub type SharedModel = Rc<RefCell<dyn FindModel>>;

/// The search field's id.
pub const QUERY: &str = "query";

fn source_name(s: &SourceView) -> String {
    match &s.name {
        SourceName::ThisComputer => m::this_computer().into(),
        SourceName::Plugin(name) => UiText::data(name).into(),
    }
}

pub fn problem_text(p: Problem) -> UiText {
    match p {
        Problem::Busy => m::problem_busy(),
        Problem::InvalidInput => m::problem_invalid_input(),
        Problem::Unsupported => m::problem_unsupported(),
        Problem::NotFound => m::problem_not_found(),
        Problem::Refused => m::problem_refused(),
        Problem::RateLimited => m::problem_rate_limited(),
        Problem::Unreachable => m::problem_unreachable(),
        Problem::TooLarge => m::problem_too_large(),
        Problem::Unreadable => m::problem_unreadable(),
        Problem::Changed => m::problem_changed(),
        Problem::Failed => m::problem_failed(),
        Problem::Crashed => m::problem_crashed(),
        Problem::TimedOut => m::problem_timed_out(),
        Problem::Misbehaved => m::problem_misbehaved(),
        Problem::NotStarted => m::problem_not_started(),
        Problem::NeedsNewerGame => m::problem_needs_newer_game(),
        Problem::TooOld => m::problem_too_old(),
        Problem::OffForTheRun => m::problem_off_for_the_run(),
        Problem::SwitchedOff => m::problem_switched_off(),
        Problem::Stopped => m::problem_stopped(),
        Problem::ImportFailed => m::problem_import_failed(),
    }
}

fn step_text(step: Option<Step>) -> UiText {
    match step {
        Some(Step::Resolving) => m::step_resolving(),
        Some(Step::Searching) => m::step_searching(),
        Some(Step::Downloading) | None => m::step_downloading(),
        Some(Step::Reading) => m::step_reading(),
        Some(Step::Converting) => m::step_converting(),
        Some(Step::Checking) => m::step_checking(),
    }
}

/// The page as `view` stands.
pub fn doc(view: &FindView) -> PageDoc {
    let mut blocks = vec![Block::Muted(m::intro().into())];
    if view.sources.is_empty() {
        blocks.push(Block::Note(m::no_sources().into()));
        return PageDoc {
            title: title(),
            blocks,
        };
    }
    // The sources, where there is a choice.
    if view.sources.len() > 1 {
        blocks.push(Block::Subheading(m::sources_heading().into()));
        blocks.push(Block::Actions(
            view.sources
                .iter()
                .enumerate()
                .map(|(i, s)| Action::new(format!("source:{i}"), source_name(s)))
                .collect(),
        ));
    }
    let chosen = view.sources.get(view.chosen);
    if let Some(s) = chosen {
        blocks.push(Block::Muted(m::source_line(source_name(s)).into()));
    }
    if let Some(p) = chosen.and_then(|s| s.problem) {
        blocks.push(Block::Note(problem_text(p).into()));
    }
    // This computer's folders.
    if let Some(folders) = &view.folders {
        blocks.push(Block::Subheading(m::folders_heading().into()));
        if folders.is_empty() {
            blocks.push(Block::Muted(m::folders_none().into()));
        }
        for (i, f) in folders.iter().enumerate() {
            blocks.push(Block::Entry {
                text: UiText::data(f).into(),
                detail: None,
                actions: vec![Action::new(
                    format!("remove-folder:{i}"),
                    m::folder_remove(),
                )],
            });
        }
        blocks.push(Block::Actions(vec![Action::new(
            "add-folder",
            m::folder_add(),
        )]));
    }
    blocks.push(Block::Field {
        id: QUERY.into(),
        label: m::search_label().into(),
        value: view.query.clone(),
        hint: m::search_hint().into(),
    });
    blocks.push(Block::Actions(vec![
        Action::new("search", m::search()),
    ]));
    // What runs, and Cancel.
    if let Some(busy) = &view.busy {
        let line: UiText = match busy {
            Busy::Searching => m::searching(),
            Busy::Fetching { title, step, .. } => {
                m::fetching(UiText::data(title), step_text(*step))
            }
            Busy::Importing { title } => m::importing(UiText::data(title)),
        };
        blocks.push(Block::Note(line.into()));
        if let Busy::Fetching {
            done: Some(done),
            total: Some(total),
            ..
        } = busy
            && *total > 0
        {
            blocks.push(Block::Muted(
                m::progress((done * 100 / total).min(100) as f64).into(),
            ));
        }
        blocks.push(Block::Actions(vec![Action::new(
            "cancel",
            ddi_l10n::msg::common::cancel(),
        )]));
    }
    if let Some(p) = view.problem {
        blocks.push(Block::Note(problem_text(p).into()));
        if let Some(d) = &view.details {
            blocks.push(Block::Fold {
                id: "details".into(),
                label: m::details().into(),
                body: vec![Block::Mono(UiText::data(d).into())],
            });
        }
    }
    match &view.note {
        Some(Note::Imported { songs }) => {
            blocks.push(Block::Note(m::imported(*songs as f64).into()));
        }
        Some(Note::Partial) => blocks.push(Block::Muted(m::partial().into())),
        None => {}
    }
    // The results.
    if view.searched {
        blocks.push(Block::Subheading(m::results_heading().into()));
        if view.results.is_empty() && view.busy.is_none() {
            blocks.push(Block::Muted(m::results_none().into()));
        } else if !view.results.is_empty() {
            let shown = view.results.len() as f64;
            blocks.push(Block::Muted(
                match view.total {
                    Some(total) if total as f64 > shown => m::results_of(shown, total as f64),
                    _ => m::results_count(shown),
                }
                .into(),
            ));
        }
        for (i, r) in view.results.iter().enumerate() {
            let mut tags = r.tags.clone();
            if r.in_library {
                tags.push(m::result_in_library().into());
            }
            blocks.push(Block::Entry {
                text: UiText::data(&r.title).into(),
                detail: (!tags.is_empty()).then(|| tags.join(" · ")),
                actions: vec![
                    Action::new(format!("play:{i}"), m::play()).off(view.busy.is_some()),
                    Action::new(format!("import:{i}"), m::import())
                        .off(view.busy.is_some() || r.in_library),
                ],
            });
        }
        if view.more {
            blocks.push(Block::Actions(vec![Action::new("more", m::more())]));
        }
    }
    PageDoc {
        title: title(),
        blocks,
    }
}

fn title() -> PageTitle {
    PageTitle::new(m::title(), m::heading())
}

/// The press a button id names.
fn press_of(id: &str) -> Option<FindPress> {
    let (what, n) = match id.split_once(':') {
        Some((w, n)) => (w, n.parse::<usize>().ok()),
        None => (id, None),
    };
    Some(match (what, n) {
        ("source", Some(i)) => FindPress::Source(i),
        ("remove-folder", Some(i)) => FindPress::RemoveFolder(i),
        ("play", Some(i)) => FindPress::Play(i),
        ("import", Some(i)) => FindPress::Import(i),
        ("add-folder", None) => FindPress::AddFolder,
        ("search", None) => FindPress::Search,
        ("more", None) => FindPress::More,
        ("cancel", None) => FindPress::Cancel,
        _ => return None,
    })
}

/// The page's model as [`FedPage`] reads it.
pub struct Fed(SharedModel);

impl PageModel for Fed {
    fn version(&self) -> u64 {
        self.0.borrow().version()
    }

    fn doc(&self) -> PageDoc {
        doc(&self.0.borrow().view())
    }

    fn tick(&mut self, cx: &mut Cx) {
        self.0.borrow_mut().tick(cx);
    }
}

pub type FindScreen = FedPage<Fed>;

/// The page over `model`.
pub fn screen(env: Env, model: SharedModel) -> FindScreen {
    let feed: PageFeed = Rc::new(RefCell::new(None));
    let first = doc(&model.borrow().view());
    let shell: PageShell = {
        let model = model.clone();
        Rc::new(move |call, cx| {
            match call {
                PageCall::Leave => {
                    model.borrow_mut().leave(cx);
                    cx.close(true);
                    return None;
                }
                PageCall::Field { id, text } if id == QUERY => model.borrow_mut().typed(text),
                PageCall::Field { .. } => return None,
                PageCall::Button(id) => {
                    let press = press_of(&id)?;
                    model.borrow_mut().press(press, cx);
                }
            }
            Some(doc(&model.borrow().view()))
        })
    };
    let page = PageScreen::new(env, "find-songs", first).with_shell(shell, feed.clone());
    FedPage::fed(page, feed, Fed(model))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> FindView {
        FindView {
            sources: vec![
                SourceView {
                    id: "this-computer".into(),
                    name: SourceName::ThisComputer,
                    problem: None,
                },
                SourceView {
                    id: "example".into(),
                    name: SourceName::Plugin("Example Songs".into()),
                    problem: Some(Problem::Crashed),
                },
            ],
            chosen: 0,
            folders: Some(vec!["/songs".into()]),
            query: "gen".into(),
            searched: true,
            results: vec![ResultView {
                id: "a".into(),
                title: "Song A".into(),
                tags: vec!["Artist".into(), "Pack".into()],
                in_library: true,
            }],
            total: Some(3),
            more: true,
            ..FindView::default()
        }
    }

    fn ids(doc: &PageDoc) -> Vec<String> {
        let mut out = Vec::new();
        for b in &doc.blocks {
            match b {
                Block::Actions(a) => out.extend(a.iter().map(|a| a.id.clone())),
                Block::Entry { actions, .. } => out.extend(actions.iter().map(|a| a.id.clone())),
                Block::Field { id, .. } => out.push(id.clone()),
                _ => {}
            }
        }
        out
    }

    #[test]
    fn every_button_names_a_press_and_the_page_follows_the_view() {
        let d = doc(&view());
        let ids = ids(&d);
        assert_eq!(
            ids,
            [
                "source:0",
                "source:1",
                "remove-folder:0",
                "add-folder",
                QUERY,
                "search",
                "play:0",
                "import:0",
                "more"
            ]
        );
        for id in ids.iter().filter(|i| *i != QUERY) {
            assert!(press_of(id).is_some(), "{id}");
        }
        // A song the library has is not imported again.
        let import = d.blocks.iter().find_map(|b| match b {
            Block::Entry { actions, .. } => actions.iter().find(|a| a.id == "import:0").cloned(),
            _ => None,
        });
        assert!(import.is_some_and(|a| a.off));
        // A plugin chosen: no folders; its problem said.
        let mut v = view();
        v.chosen = 1;
        v.folders = None;
        let d = doc(&v);
        assert!(!ids_of(&d).contains(&"add-folder".to_string()));
        assert!(d.blocks.iter().any(|b| matches!(b, Block::Note(_))));
    }

    fn ids_of(d: &PageDoc) -> Vec<String> {
        ids(d)
    }

    #[test]
    fn while_busy_play_and_import_wait_and_cancel_is_offered() {
        let mut v = view();
        v.busy = Some(Busy::Fetching {
            title: "Song A".into(),
            step: Some(Step::Downloading),
            done: Some(1),
            total: Some(4),
        });
        let d = doc(&v);
        assert!(ids(&d).contains(&"cancel".to_string()));
        let offs: Vec<bool> = d
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Entry { actions, .. } if actions.len() == 2 => {
                    Some(actions.iter().all(|a| a.off))
                }
                _ => None,
            })
            .collect();
        assert_eq!(offs, [true]);
    }
}
