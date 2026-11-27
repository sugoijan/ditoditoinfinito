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
//! Opened from an online room (stage 3: its More, "Find songs to add…"),
//! each result has Add to room in Play's place where its source and the
//! room allow; what is added goes to the room's list as a song of the
//! library does, uploaded from here and never imported.
//!
//! Every play of a song found here is kept (the maintainer, 2026-11-27):
//! the page lists the songs played before from the source chosen, from
//! their plays' origins alone ([`PlayedSection`]), each with Play, which
//! fetches it again; those played from a source not on stay listed under
//! it, with a line saying why they cannot be fetched now. From a room,
//! Add to room in Play's place, under a result's rules.
//!
//! How far a result may go is its source's level, lowered by the item
//! (`docs/plans/source-plugins.md`, "How far a song may go"): played on
//! this computer only, added to rooms too, or imported as well. A button
//! its level leaves out is absent, and the result's line says why
//! ([`Reach`]).
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
    /// How far its songs may go (its source's level, lowered by the
    /// item's own).
    pub reach: Reach,
    /// Opened from an online room: what it offers there.
    pub room: RoomOffer,
}

/// How far a result's songs may go, each step allowing everything before
/// it: the buttons it has, and why a button is absent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reach {
    /// Played on this computer only: no Import, no Add to room.
    Play,
    /// Played and added to rooms: no Import.
    Rooms,
    /// Everything (the default).
    #[default]
    Keep,
}

/// What a result offers an online room (stage 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RoomOffer {
    /// Not opened from a room.
    #[default]
    None,
    /// Add to room.
    Add,
    /// The room lists the same song already.
    InRoom,
    /// It may not go to rooms ([`Reach::Play`]): no button, its line
    /// saying why.
    NotForRooms,
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
    /// Its source lets it be played on this computer alone (a press its
    /// level leaves out: said in the line's words).
    PlayOnly,
    /// Its source lets it be played and added to rooms, not imported.
    NotForKeeping,
}

/// A song played before from a source, as a row of
/// [`PlayedSection`]: what its latest play's origin says (data), its
/// chart's best as the lists write it, when it was last played.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayedView {
    pub title: String,
    pub artist: String,
    /// The chart last played, as a chart's line (`prompt::chart_line`).
    pub chart: String,
    /// Its best: the score and the grade as the lists write them.
    pub best: Option<String>,
    /// When it was last played, in the player's time.
    pub when: String,
    pub state: PlayedState,
    /// How far it may go by its source's level (its item may lower it
    /// further, checked once fetched): from a room, Add to room only at
    /// `Rooms` or more.
    pub reach: Reach,
    /// Opened from an online room: what it offers there.
    pub room: RoomOffer,
}

/// Whether a song played before can be fetched again now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayedState {
    /// Play fetches it again from its source.
    #[default]
    Ready,
    /// Its source is installed but not switched on (or This computer is
    /// off): no button.
    SourceOff,
    /// Its plugin is not installed any more: no button.
    NotInstalled,
    /// Its source said it does not have it any more (a folder moved, an
    /// item taken down): no button.
    Gone,
}

/// The songs played before from one source ([`FindView::played`]).
#[derive(Clone, Debug, PartialEq)]
pub struct PlayedSection {
    /// The source's id.
    pub id: String,
    pub source: SourceName,
    /// The source chosen now: its rows are pressed for Play.
    pub chosen: bool,
    /// The most recently played first.
    pub rows: Vec<PlayedView>,
    /// Songs played from it beyond the rows (their plays kept all the
    /// same).
    pub unlisted: usize,
}

/// Rows of a [`PlayedSection`] shown before the rest are folded.
pub const PLAYED_SHOWN: usize = 5;

/// What the page says once, quietly.
#[derive(Clone, Debug, PartialEq)]
pub enum Note {
    /// A result was imported: its songs are in the library now.
    Imported { songs: usize },
    /// This computer's song imported in place: this folder (its name)
    /// added to the library's song folders.
    AddedFolder { folder: String },
    /// This computer's song is in a song folder of the library's already
    /// (that folder's name): nothing added.
    InFolders { folder: String },
    /// A song played before fetched again changed at its source since
    /// (its current revision fetched).
    Changed,
    /// Some folders were too large to read whole (their bounds).
    Partial,
    /// A result's songs were added to the room's list (their upload
    /// follows, as any song added).
    AddedToRoom { songs: usize },
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
    /// Opened from an online room: results are added to it, not played.
    pub room: bool,
    /// The songs played before: the chosen source's first, then those of
    /// sources not on now (none from a room).
    pub played: Vec<PlayedSection>,
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
    /// Added to the online room the page was opened from.
    AddToRoom(usize),
    /// A song played before (a row of the chosen source's section) fetched
    /// again and played.
    Played(usize),
    /// A song played before added to the online room the page was opened
    /// from (fetched again first where the library does not have it).
    PlayedToRoom(usize),
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
        Problem::PlayOnly => m::result_play_only(),
        Problem::NotForKeeping => m::result_not_for_keeping(),
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
    let intro = if view.room {
        m::intro_room()
    } else {
        m::intro()
    };
    let mut blocks = vec![Block::Muted(intro.into())];
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
        Some(Note::AddedToRoom { songs }) => {
            blocks.push(Block::Note(m::added_to_room(*songs as f64).into()));
        }
        Some(Note::AddedFolder { folder }) => {
            blocks.push(Block::Note(m::added_folder(UiText::data(folder)).into()));
        }
        Some(Note::InFolders { folder }) => {
            blocks.push(Block::Note(m::in_folders(UiText::data(folder)).into()));
        }
        Some(Note::Changed) => blocks.push(Block::Note(m::changed().into())),
        None => {}
    }
    for section in &view.played {
        played_blocks(section, view.busy.is_some(), view.room, &mut blocks);
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
            if r.room == RoomOffer::InRoom {
                tags.push(m::result_in_room().into());
            }
            // Why a button is absent: how far its source lets it go.
            match r.reach {
                Reach::Play => tags.push(m::result_play_only().into()),
                Reach::Rooms => tags.push(m::result_not_for_keeping().into()),
                Reach::Keep => {}
            }
            let busy = view.busy.is_some();
            // From a room: Add to room in Play's place (nothing is played
            // over a room), where it may be added.
            let first = if !view.room {
                Some(Action::new(format!("play:{i}"), m::play()).off(busy))
            } else if r.room == RoomOffer::Add && r.reach >= Reach::Rooms {
                Some(Action::new(format!("add:{i}"), m::add_to_room()).off(busy))
            } else {
                None
            };
            // Import only where it may be kept.
            let import = (r.reach == Reach::Keep)
                .then(|| Action::new(format!("import:{i}"), m::import()).off(busy || r.in_library));
            blocks.push(Block::Entry {
                text: UiText::data(&r.title).into(),
                detail: (!tags.is_empty()).then(|| tags.join(" · ")),
                actions: first.into_iter().chain(import).collect(),
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

/// A section of songs played before: its heading, its first rows, the
/// rest folded, what is not listed.
fn played_blocks(section: &PlayedSection, busy: bool, room: bool, blocks: &mut Vec<Block>) {
    if section.rows.is_empty() {
        return;
    }
    blocks.push(Block::Subheading(
        if section.chosen {
            m::played_heading()
        } else {
            m::played_heading_from(match &section.source {
                SourceName::ThisComputer => m::this_computer(),
                SourceName::Plugin(name) => UiText::data(name),
            })
        }
        .into(),
    ));
    let entry = |i: usize, r: &PlayedView| {
        let mut tags: Vec<String> = Vec::new();
        if !r.artist.is_empty() {
            tags.push(UiText::data(&r.artist).into());
        }
        if !r.chart.is_empty() {
            tags.push(r.chart.clone());
        }
        if let Some(best) = &r.best {
            tags.push(m::played_best(UiText::data(best)).into());
        }
        tags.push(m::played_last(UiText::data(&r.when)).into());
        match r.state {
            PlayedState::Ready => {}
            PlayedState::SourceOff => tags.push(m::played_source_off().into()),
            PlayedState::NotInstalled => tags.push(m::played_not_installed().into()),
            PlayedState::Gone => tags.push(m::played_gone().into()),
        }
        // From a room, as a result's: in the room already, or why it may
        // not be added.
        if room && r.state == PlayedState::Ready {
            if r.room == RoomOffer::InRoom {
                tags.push(m::result_in_room().into());
            }
            if r.reach == Reach::Play {
                tags.push(m::result_play_only().into());
            }
        }
        let ready = section.chosen && r.state == PlayedState::Ready;
        let play = if !room {
            ready.then(|| Action::new(format!("played:{i}"), m::play()).off(busy))
        } else {
            (ready && r.room == RoomOffer::Add && r.reach >= Reach::Rooms)
                .then(|| Action::new(format!("played-add:{i}"), m::add_to_room()).off(busy))
        };
        Block::Entry {
            text: UiText::data(&r.title).into(),
            detail: Some(tags.join(" · ")),
            actions: play.into_iter().collect(),
        }
    };
    let (first, rest) = section.rows.split_at(section.rows.len().min(PLAYED_SHOWN));
    blocks.extend(first.iter().enumerate().map(|(i, r)| entry(i, r)));
    if !rest.is_empty() {
        blocks.push(Block::Fold {
            id: format!("played-more:{}", section.id),
            label: m::played_more(rest.len() as f64).into(),
            body: rest
                .iter()
                .enumerate()
                .map(|(i, r)| entry(i + PLAYED_SHOWN, r))
                .collect(),
        });
    }
    if section.unlisted > 0 {
        blocks.push(Block::Muted(
            m::played_unlisted(section.unlisted as f64).into(),
        ));
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
        ("add", Some(i)) => FindPress::AddToRoom(i),
        ("played", Some(i)) => FindPress::Played(i),
        ("played-add", Some(i)) => FindPress::PlayedToRoom(i),
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
                reach: Reach::Keep,
                room: RoomOffer::None,
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
    fn from_a_room_a_result_is_added_not_played_and_its_rule_said() {
        let mut v = view();
        v.room = true;
        v.folders = None;
        v.results = vec![
            ResultView {
                id: "a".into(),
                title: "Song A".into(),
                room: RoomOffer::Add,
                ..ResultView::default()
            },
            ResultView {
                id: "b".into(),
                title: "Song B".into(),
                reach: Reach::Play,
                room: RoomOffer::NotForRooms,
                ..ResultView::default()
            },
            ResultView {
                id: "c".into(),
                title: "Song C".into(),
                room: RoomOffer::InRoom,
                ..ResultView::default()
            },
        ];
        let d = doc(&v);
        let ids = ids(&d);
        assert!(ids.contains(&"add:0".to_string()), "{ids:?}");
        assert!(!ids.iter().any(|i| i.starts_with("play:")), "{ids:?}");
        assert!(!ids.contains(&"add:1".to_string()) && !ids.contains(&"add:2".to_string()));
        assert_eq!(press_of("add:0"), Some(FindPress::AddToRoom(0)));
        let details: Vec<String> = d
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Entry { detail, .. } => Some(detail.clone().unwrap_or_default()),
                _ => None,
            })
            .collect();
        assert_eq!(details[1], m::result_play_only().as_str());
        assert_eq!(details[2], m::result_in_room().as_str());
    }

    #[test]
    fn a_results_level_leaves_its_buttons_out_and_its_line_says_why() {
        let result = |id: &str, reach: Reach, room: RoomOffer| ResultView {
            id: id.into(),
            title: id.into(),
            reach,
            room,
            ..ResultView::default()
        };
        let entries = |d: &PageDoc| -> Vec<(Vec<String>, String)> {
            d.blocks
                .iter()
                .filter_map(|b| match b {
                    Block::Entry {
                        actions, detail, ..
                    } => Some((
                        actions.iter().map(|a| a.id.clone()).collect(),
                        detail.clone().unwrap_or_default(),
                    )),
                    _ => None,
                })
                .collect()
        };
        // Alone: Play always, Import only where it may be kept.
        let mut v = view();
        v.folders = None;
        v.results = vec![
            result("keep", Reach::Keep, RoomOffer::None),
            result("rooms", Reach::Rooms, RoomOffer::None),
            result("play", Reach::Play, RoomOffer::None),
        ];
        let e = entries(&doc(&v));
        assert_eq!(
            e[0],
            (vec!["play:0".into(), "import:0".into()], String::new())
        );
        assert_eq!(
            e[1],
            (
                vec!["play:1".into()],
                m::result_not_for_keeping().as_str().into()
            )
        );
        assert_eq!(
            e[2],
            (vec!["play:2".into()], m::result_play_only().as_str().into())
        );
        // From a room: Add to room from `rooms` up, the same words.
        v.room = true;
        v.results = vec![
            result("keep", Reach::Keep, RoomOffer::Add),
            result("rooms", Reach::Rooms, RoomOffer::Add),
            result("play", Reach::Play, RoomOffer::NotForRooms),
        ];
        let e = entries(&doc(&v));
        assert_eq!(
            e[0],
            (vec!["add:0".into(), "import:0".into()], String::new())
        );
        assert_eq!(
            e[1],
            (
                vec!["add:1".into()],
                m::result_not_for_keeping().as_str().into()
            )
        );
        assert_eq!(e[2], (vec![], m::result_play_only().as_str().into()));
        // A model's mistake (Add offered below `rooms`) shows no button.
        v.results = vec![result("play", Reach::Play, RoomOffer::Add)];
        assert_eq!(entries(&doc(&v))[0].0, Vec::<String>::new());
    }

    #[test]
    fn songs_played_before_have_play_where_their_source_can_fetch_them() {
        let row = |n: usize, state: PlayedState| PlayedView {
            title: format!("Song {n}"),
            when: "2026-11-27 14:03".into(),
            state,
            ..PlayedView::default()
        };
        let mut v = view();
        v.folders = None;
        v.results.clear();
        v.played = vec![
            PlayedSection {
                id: "this-computer".into(),
                source: SourceName::ThisComputer,
                chosen: true,
                rows: (0..PLAYED_SHOWN + 2)
                    .map(|n| row(n, if n == 1 { PlayedState::Gone } else { PlayedState::Ready }))
                    .collect(),
                unlisted: 4,
            },
            PlayedSection {
                id: "example".into(),
                source: SourceName::Plugin("Example".into()),
                chosen: false,
                rows: vec![row(9, PlayedState::SourceOff)],
                unlisted: 0,
            },
        ];
        let d = doc(&v);
        let ids = ids(&d);
        // The first rows shown, a gone one without Play; the rest folded;
        // a source not on: no button at all.
        assert!(ids.contains(&"played:0".to_string()), "{ids:?}");
        assert!(!ids.contains(&"played:1".to_string()), "{ids:?}");
        assert_eq!(
            ids.iter().filter(|i| i.starts_with("played:")).count(),
            PLAYED_SHOWN - 1,
            "{ids:?}"
        );
        assert_eq!(press_of("played:6"), Some(FindPress::Played(6)));
        let fold = d.blocks.iter().find_map(|b| match b {
            Block::Fold { body, .. } => Some(body.len()),
            _ => None,
        });
        assert_eq!(fold, Some(2));
        let details: Vec<String> = d
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Entry { detail, .. } => detail.clone(),
                _ => None,
            })
            .collect();
        assert!(details.iter().any(|t| t.contains(m::played_gone().as_str())));
        assert!(details.iter().any(|t| t.contains(m::played_source_off().as_str())));
        assert!(d.blocks.iter().any(|b| matches!(b, Block::Muted(t) if t == m::played_unlisted(4.0).as_str())));
        // From a room: Add to room in Play's place, as a result's: only
        // where it may go to rooms and the room does not list it; the line
        // says which; a source not on still has none.
        v.room = true;
        v.played[0].rows = vec![
            PlayedView {
                room: RoomOffer::Add,
                ..row(0, PlayedState::Ready)
            },
            PlayedView {
                room: RoomOffer::InRoom,
                ..row(1, PlayedState::Ready)
            },
            PlayedView {
                room: RoomOffer::NotForRooms,
                reach: Reach::Play,
                ..row(2, PlayedState::Ready)
            },
            PlayedView {
                room: RoomOffer::Add,
                ..row(3, PlayedState::Gone)
            },
        ];
        let d = doc(&v);
        let ids = ids_of(&d);
        assert!(!ids.iter().any(|i| i.starts_with("played:")), "{ids:?}");
        let adds: Vec<&String> = ids.iter().filter(|i| i.starts_with("played-add:")).collect();
        assert_eq!(adds, ["played-add:0"], "{ids:?}");
        assert_eq!(press_of("played-add:0"), Some(FindPress::PlayedToRoom(0)));
        let details: Vec<String> = d
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Entry { detail, .. } => detail.clone(),
                _ => None,
            })
            .collect();
        assert!(details[1].contains(m::result_in_room().as_str()), "{details:?}");
        assert!(details[2].contains(m::result_play_only().as_str()), "{details:?}");
        assert!(details[3].contains(m::played_gone().as_str()), "{details:?}");
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
