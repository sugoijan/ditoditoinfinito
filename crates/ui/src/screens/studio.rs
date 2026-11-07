//! The announcer studio (`docs/plans/announcer-studio.md`, step 2): a
//! page ([`super::page`]) made from the draft
//! (`ddi_library::announcer::draft`) for people who only play the game.
//! Every line the game speaks in a zone of its own, by a plain name,
//! when it speaks and what speaks without it, with examples to say, the
//! recordings on it and a button to add more; recordings not sorted yet
//! on top. The shells keep the draft and the recordings and carry out
//! what a press asks ([`act`]); the page shows [`doc`] again after.

use std::collections::BTreeMap;
use std::rc::Rc;

use ddi_l10n::UiText;
use ddi_l10n::msg::studio as m;
use ddi_library::announcer::draft::{Draft, Note, Sound, Take, Write};
use ddi_library::announcer::examples::{self, Set};
use ddi_library::announcer::{Bundled, Comment, Grade, Line, Spoken};

use super::Env;
use super::editor_dialogs::ActionsDialog;
use super::page::{Action, Block, PageDoc};
use crate::screen::Cx;

/// The page's name, for tests and the debug state.
pub const NAME: &str = "announcer-studio";

/// What the studio shows beside the draft.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub examples: Set,
    /// The details shown (Advanced): each line's folder, what tidying
    /// did to each take.
    pub details: bool,
    /// What plays: a take's id (`take:<id>`) or an example
    /// (`example:<folder>:<i>`).
    pub playing: Option<String>,
    /// What the shell is doing, or did ("Reading 3 recordings…").
    pub status: String,
    /// An export made: the zip's name, saved with the next press.
    pub ready: Option<String>,
    /// An export or Use in the game under way.
    pub busy: bool,
}

/// An example of a line.
#[derive(Clone, Debug, PartialEq)]
pub struct Example {
    pub text: String,
    /// Our pack's sound of it: its path in the pack.
    pub sound: Option<String>,
}

/// The examples of every line in `set`: our pack's sounds (`ours`, the
/// set's pack from the bundled index, each with what it says), then the
/// studio's own.
pub fn examples(set: Set, ours: Option<&Bundled>) -> BTreeMap<Line, Vec<Example>> {
    let mut by: BTreeMap<Line, Vec<Example>> = BTreeMap::new();
    if let Some(b) = ours {
        for (folder, files) in &b.kept.lines {
            let Some(line) = Line::parse(folder) else {
                continue;
            };
            for f in files {
                let path = f.to_string();
                if let Some(text) = b.said.get(&path) {
                    by.entry(line).or_default().push(Example {
                        text: text.clone(),
                        sound: Some(path),
                    });
                }
            }
        }
    }
    for line in Line::all() {
        let more = examples::more(set, line);
        if !more.is_empty() {
            by.entry(line)
                .or_default()
                .extend(more.into_iter().map(|text| Example { text, sound: None }));
        }
    }
    by
}

fn grade_letters(g: Grade) -> &'static str {
    match g {
        Grade::Aaaaa => "AAAAA",
        Grade::Aaaa => "AAAA",
        Grade::Aaa => "AAA",
        Grade::Aa => "AA",
        Grade::A => "A",
        Grade::B => "B",
        Grade::C => "C",
        Grade::D => "D",
        Grade::E => "E",
        Grade::F => "F",
    }
}

/// A line's plain name.
pub fn name(line: Line) -> UiText {
    match line {
        Line::Ready => m::line_ready(),
        Line::HereWeGo => m::line_here_we_go(),
        Line::Restart => m::line_restart(),
        Line::CommentHot => m::line_comment_hot(),
        Line::CommentGood => m::line_comment_good(),
        Line::CommentDanger => m::line_comment_danger(),
        Line::CommentOni => m::line_comment_oni(),
        Line::Combo(n) => m::line_combo(n.to_string()),
        Line::ComboOverflow => m::line_combo_overflow(),
        Line::ComboStopped => m::line_combo_stopped(),
        Line::FullCombo => m::line_full_combo(),
        Line::FullComboW(1) | Line::EvaluationFullCombo(1) => m::line_full_combo_w1(),
        Line::FullComboW(2) | Line::EvaluationFullCombo(2) => m::line_full_combo_w2(),
        Line::FullComboW(_) | Line::EvaluationFullCombo(3) => m::line_full_combo_w3(),
        Line::LastLife => m::line_last_life(),
        Line::LifeLost => m::line_life_lost(),
        Line::Cleared => m::line_cleared(),
        Line::Failed => m::line_failed(),
        Line::Quit => m::line_quit(),
        Line::FirstClear => m::line_first_clear(),
        Line::NewRecord => m::line_new_record(),
        Line::EvaluationFullCombo(_) => m::line_full_combo(),
        Line::Grade(g @ (Grade::E | Grade::F)) => m::line_grade_failed(grade_letters(g)),
        Line::Grade(g) => m::line_grade(grade_letters(g)),
        Line::Cheer => m::line_cheer(),
        Line::SelectMusicIntro => m::line_select_intro(),
        Line::SelectMusic(Comment::New) => m::line_select_new(),
        Line::SelectMusic(Comment::Repeat) => m::line_select_repeat(),
        Line::SelectMusic(Comment::Hard) => m::line_select_hard(),
        Line::SelectMusic(Comment::General) => m::line_select_general(),
    }
}

/// When a line speaks, in a sentence.
pub fn when(line: Line) -> UiText {
    match line {
        Line::Ready => m::when_ready(),
        Line::HereWeGo => m::when_here_we_go(),
        Line::Restart => m::when_restart(),
        Line::CommentHot => m::when_comment_hot(),
        Line::CommentGood => m::when_comment_good(),
        Line::CommentDanger => m::when_comment_danger(),
        Line::CommentOni => m::when_comment_oni(),
        Line::Combo(n) if n > 1000 => m::when_combo_past_1000(n),
        Line::Combo(n) => m::when_combo(n),
        Line::ComboOverflow => m::when_combo_overflow(),
        Line::ComboStopped => m::when_combo_stopped(),
        Line::FullCombo => m::when_full_combo(),
        Line::FullComboW(1) => m::when_full_combo_w1(),
        Line::FullComboW(2) => m::when_full_combo_w2(),
        Line::FullComboW(_) => m::when_full_combo_w3(),
        Line::LastLife => m::when_last_life(),
        Line::LifeLost => m::when_life_lost(),
        Line::Cleared => m::when_cleared(),
        Line::Failed => m::when_failed(),
        Line::Quit => m::when_quit(),
        Line::FirstClear => m::when_first_clear(),
        Line::NewRecord => m::when_new_record(),
        Line::EvaluationFullCombo(1) => m::when_evaluation_full_combo_w1(),
        Line::EvaluationFullCombo(2) => m::when_evaluation_full_combo_w2(),
        Line::EvaluationFullCombo(3) => m::when_evaluation_full_combo_w3(),
        Line::EvaluationFullCombo(_) => m::when_evaluation_full_combo(),
        Line::Grade(Grade::Aaaaa) => m::when_grade_aaaaa(),
        Line::Grade(Grade::Aaaa) => m::when_grade_aaaa(),
        Line::Grade(Grade::Aaa) => m::when_grade_aaa(),
        Line::Grade(Grade::Aa) => m::when_grade_aa(),
        Line::Grade(Grade::A) => m::when_grade_a(),
        Line::Grade(Grade::B) => m::when_grade_b(),
        Line::Grade(Grade::C) => m::when_grade_c(),
        Line::Grade(Grade::D) => m::when_grade_d(),
        Line::Grade(_) => m::when_grade_failed(),
        Line::Cheer => m::when_cheer(),
        Line::SelectMusicIntro => m::when_select_intro(),
        Line::SelectMusic(Comment::New) => m::when_select_new(),
        Line::SelectMusic(Comment::Repeat) => m::when_select_repeat(),
        Line::SelectMusic(Comment::Hard) => m::when_select_hard(),
        Line::SelectMusic(Comment::General) => m::when_select_general(),
    }
}

/// What speaks where the draft has nothing on `line`.
pub fn without(line: Line, draft: &Draft) -> UiText {
    if let Some(other) = line
        .chain()
        .into_iter()
        .skip(1)
        .find(|l| draft.takes_of(*l).next().is_some())
    {
        return m::missing_instead(name(other));
    }
    match line {
        Line::FirstClear | Line::NewRecord | Line::EvaluationFullCombo(_) => m::missing_next_line(),
        _ => m::missing_silent(),
    }
}

/// Seconds as the studio writes them, one decimal, for a message.
pub fn seconds(s: f64) -> String {
    format!("{s:.1}")
}

/// A take's line on the page: its name, its length and what tidying does.
fn take_label(draft: &Draft, t: &Take, playing: bool) -> String {
    let mut name: String = t.file_name().chars().take(32).collect();
    if t.file_name().chars().count() > 32 {
        name.push('…');
    }
    let icon = if playing { "■" } else { "▶" };
    match (&t.sound, draft.seconds(t)) {
        (Sound::Read(_), Some(s)) => format!("{icon} {}", m::take(&name, seconds(s))),
        (Sound::Unreadable(_), _) => m::take_unplayable(&name).into(),
        _ => format!("{icon} {}", m::take_reading(&name)),
    }
}

/// What a note says.
fn note_text(note: &Note) -> UiText {
    match note {
        Note::Unreadable(why) => m::note_unreadable(super::errors::error_text(why)),
        Note::Clipped => m::note_clipped(),
        Note::TooLong { seconds: s, limit } => m::note_too_long(seconds(*s), seconds(*limit)),
    }
}

/// What tidying did to a take, where it did something.
fn tidied(draft: &Draft, t: &Take) -> Option<UiText> {
    let measure = t.measure()?;
    if !draft.tidies(t) {
        return Some(m::tidied_as_recorded());
    }
    let cut = measure.seconds - measure.tidied_seconds();
    let db = measure.gain_db();
    let cut = (cut >= 0.05).then_some(cut);
    let louder = (db.abs() >= 1.0).then_some(db > 0.0);
    let change = match (cut, louder) {
        (None, None) => return Some(m::tidied_nothing()),
        (Some(_), None) => "cut",
        (None, Some(true)) => "louder",
        (None, Some(false)) => "quieter",
        (Some(_), Some(true)) => "cut-louder",
        (Some(_), Some(false)) => "cut-quieter",
    };
    Some(m::tidied(
        change,
        seconds(cut.unwrap_or(0.0)),
        db.abs().round(),
    ))
}

/// A take's blocks: the button to hear it, what can be done with it,
/// what tidying did (with the details) and its notes.
fn take_blocks(draft: &Draft, view: &View, t: &Take, sorted: bool, out: &mut Vec<Block>) {
    let playing = view.playing.as_deref() == Some(&format!("take:{}", t.id));
    let mut actions = vec![Action::new(
        format!("play:{}", t.id),
        take_label(draft, t, playing),
    )];
    actions.push(Action::new(
        format!("move:{}", t.id),
        if sorted {
            m::take_move()
        } else {
            m::take_which_line()
        },
    ));
    actions.push(Action::new(format!("remove:{}", t.id), m::take_remove()));
    out.push(Block::Actions(actions));
    if view.details
        && let Some(s) = tidied(draft, t)
    {
        out.push(Block::Muted(s.into()));
    }
    for n in draft.notes(t) {
        out.push(Block::Note(note_text(&n).into()));
    }
}

/// A line's zone.
fn line_zone(
    draft: &Draft,
    view: &View,
    line: Line,
    examples: &BTreeMap<Line, Vec<Example>>,
) -> Block {
    let folder = line.folder();
    let mut body = vec![
        Block::Subheading(name(line).into()),
        Block::Muted(when(line).into()),
    ];
    if view.details {
        body.push(Block::Muted(m::line_folder(&folder).into()));
    }
    if let Some(ex) = examples.get(&line).filter(|e| !e.is_empty()) {
        let items = ex
            .iter()
            .enumerate()
            .map(|(i, e)| match &e.sound {
                Some(_) => {
                    let id = format!("example:{folder}:{i}");
                    let icon = if view.playing.as_deref() == Some(&id) {
                        "■"
                    } else {
                        "▶"
                    };
                    Block::Action {
                        id,
                        label: format!("{icon} {}", m::example(&e.text)),
                    }
                }
                None => Block::Text(m::example(&e.text).into()),
            })
            .collect();
        body.push(Block::Fold {
            id: format!("examples:{folder}"),
            label: m::examples_fold(ex.len()).into(),
            body: items,
        });
    }
    let takes: Vec<&Take> = draft.takes_of(line).collect();
    if takes.is_empty() {
        body.push(Block::Muted(without(line, draft).into()));
    }
    for t in takes {
        take_blocks(draft, view, t, true, &mut body);
    }
    if line == Line::HereWeGo
        && let Some(s) = draft.opening()
        && s > ddi_library::announcer::draft::OPENING_ROOM
    {
        body.push(Block::Note(m::opening_long(seconds(s)).into()));
    }
    let mut actions = vec![Action::new(format!("add:{folder}"), m::add_recording())];
    if hearable(line) {
        let label = if recorded(line, draft) {
            m::hear_recorded()
        } else {
            m::hear_example()
        };
        actions.push(Action::new(format!("hear:{folder}"), label));
    }
    body.push(Block::Actions(actions));
    Block::Zone { id: folder, body }
}

/// Whether `line` has a scene to hear it in (step 3: on song select and
/// the results; `evaluation aaaaa`, which no play gets, has none).
pub fn hearable(line: Line) -> bool {
    matches!(line.spoken(), Spoken::Menu | Spoken::Results) && line != Line::Grade(Grade::Aaaaa)
}

/// Whether the draft has something to say for `line`: a recording on it
/// or on what speaks in its place. Its scene then speaks with the draft,
/// else with our pack of the examples chosen.
pub fn recorded(line: Line, draft: &Draft) -> bool {
    line.chain()
        .into_iter()
        .any(|l| draft.takes_of(l).next().is_some())
}

/// What the studio says after a scene of `line`: the lines heard in it,
/// in order (a scene says two at most: song select's welcome and a
/// comment, a results line and the cheer), said by the draft or by our
/// pack `example` (its name).
pub fn heard(line: Line, heard: &[Line], example: Option<&str>) -> UiText {
    match (heard, example) {
        ([], _) if line == Line::Cheer => m::status_heard_nothing_cheer(),
        ([], _) => m::status_heard_nothing(),
        ([one], None) => m::status_heard(name(*one)),
        ([one], Some(pack)) => m::status_heard_example(pack, name(*one)),
        ([first, .., then], None) => m::status_heard_two(name(*first), name(*then)),
        ([first, .., then], Some(pack)) => {
            m::status_heard_example_two(pack, name(*first), name(*then))
        }
    }
}

/// A switch's state, as its message selects it.
fn state(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// A switch of Advanced: its button (its name and state), then what it
/// does.
fn switch(id: &str, label: UiText, does: UiText) -> [Block; 2] {
    [
        Block::Actions(vec![Action::new(id, label)]),
        Block::Muted(does.into()),
    ]
}

/// The studio's page.
pub fn doc(draft: &Draft, view: &View, examples: &BTreeMap<Line, Vec<Example>>) -> PageDoc {
    let mut b = vec![
        Block::Text(m::intro().into()),
        Block::Field {
            id: "name".into(),
            label: m::name().into(),
            value: draft.name.clone(),
            hint: m::name_hint().into(),
        },
    ];
    let mut actions = vec![Action::new("examples", m::examples(view.examples.name()))];
    let has = draft.takes.iter().any(|t| t.line.is_some());
    if has && !view.busy {
        actions.push(Action::new("use", m::use_in_game()));
        match &view.ready {
            Some(zip) => actions.push(Action::new("save", m::save(zip)).primary()),
            None => actions.push(Action::new("export", m::export())),
        }
    }
    b.push(Block::Actions(actions));
    if !view.status.is_empty() {
        b.push(Block::Muted(view.status.clone()));
    }
    if draft.too_big() {
        b.push(Block::Note(
            m::too_big(
                ddi_library::announcer::MAX_FILES,
                ddi_library::announcer::MAX_BYTES >> 20,
            )
            .into(),
        ));
    }
    let mut advanced = vec![
        Block::Field {
            id: "author".into(),
            label: m::author().into(),
            value: draft.author.clone(),
            hint: m::author_hint().into(),
        },
        Block::Field {
            id: "license".into(),
            label: m::license().into(),
            value: draft.license.clone(),
            hint: m::license_hint().into(),
        },
        Block::Actions(vec![
            Action::new("import-files", m::import_zip()),
            Action::new("import-folder", m::import_folder()),
        ]),
        Block::Muted(m::import_description().into()),
    ];
    advanced.extend(switch(
        "tidy",
        m::tidy(state(draft.tidy)),
        m::tidy_description(),
    ));
    advanced.extend(switch(
        "stepmania",
        m::stepmania(state(draft.stepmania)),
        m::stepmania_description(),
    ));
    advanced.extend(switch(
        "details",
        m::details(state(view.details)),
        m::details_description(),
    ));
    b.push(Block::Fold {
        id: "advanced".into(),
        label: m::advanced().into(),
        body: advanced,
    });
    let unsorted: Vec<&Take> = draft.unsorted().collect();
    if !unsorted.is_empty() {
        let mut body = vec![
            Block::Heading(m::unsorted(unsorted.len()).into()),
            Block::Muted(m::unsorted_help().into()),
        ];
        for t in unsorted {
            take_blocks(draft, view, t, false, &mut body);
        }
        b.push(Block::Zone {
            id: "unsorted".into(),
            body,
        });
    }
    let lines = draft.lines();
    for (spoken, heading) in [
        (Spoken::Menu, m::section_select()),
        (Spoken::Play, m::section_play()),
        (Spoken::Results, m::section_results()),
    ] {
        b.push(Block::Heading(heading.into()));
        for &line in lines.iter().filter(|l| l.spoken() == spoken) {
            b.push(line_zone(draft, view, line, examples));
        }
    }
    PageDoc {
        title: m::title().into(),
        blocks: b,
    }
}

/// What a press asks of the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// Pick recordings for a line.
    Add(Line),
    /// A pack made before, from a zip (loose recordings too) or a
    /// folder: its recordings on their lines, by name.
    ImportFiles,
    ImportFolder,
    /// Hear a take (again: stop it).
    Play(u64),
    /// Hear our example `i` of a line.
    Example(Line, usize),
    /// The takes' tidying turned over (the draft's choice).
    Tidy,
    /// Choose a take's line (hearing it).
    Move(u64),
    Remove(u64),
    /// The next set of examples.
    Examples,
    StepMania,
    Details,
    Export,
    /// Save the export made, in the press.
    Save,
    UseInGame,
    /// Hear a line in the game: its scene on the screen it is spoken on.
    Hear(Line),
}

/// The press `id` of [`doc`]'s.
pub fn act(id: &str) -> Option<Act> {
    let take = |rest: &str| rest.parse::<u64>().ok();
    Some(match id {
        "import-files" => Act::ImportFiles,
        "import-folder" => Act::ImportFolder,
        "tidy" => Act::Tidy,
        "examples" => Act::Examples,
        "stepmania" => Act::StepMania,
        "details" => Act::Details,
        "export" => Act::Export,
        "save" => Act::Save,
        "use" => Act::UseInGame,
        _ => {
            let (kind, rest) = id.split_once(':')?;
            match kind {
                "add" => Act::Add(Line::parse(rest)?),
                "hear" => Act::Hear(Line::parse(rest)?),
                "play" => Act::Play(take(rest)?),
                "move" => Act::Move(take(rest)?),
                "remove" => Act::Remove(take(rest)?),
                "example" => {
                    let (folder, i) = rest.rsplit_once(':')?;
                    Act::Example(Line::parse(folder)?, i.parse().ok()?)
                }
                _ => return None,
            }
        }
    })
}

/// The examples the studio shows where none were chosen: the UI's
/// language's (`docs/plans/localization.md`), else English.
pub fn first_set() -> Set {
    let tag = ddi_l10n::current().tag;
    Set::from_key(tag.split('-').next().unwrap_or(tag)).unwrap_or_default()
}

/// The next example set after `set`.
pub fn next_set(set: Set) -> Set {
    let i = Set::ALL.iter().position(|s| *s == set).unwrap_or(0);
    Set::ALL[(i + 1) % Set::ALL.len()]
}

/// What a field's press says: the draft's field it is.
pub fn set_field(draft: &mut Draft, id: &str, text: &str) -> bool {
    let to = match id {
        "name" => &mut draft.name,
        "author" => &mut draft.author,
        "license" => &mut draft.license,
        _ => return false,
    };
    *to = text.to_string();
    true
}

/// What putting a take on a line does: `Some(line)`, or `None` to leave
/// it unsorted.
pub type LineFn = Rc<dyn Fn(u64, Option<Line>, &mut Cx)>;

/// What the line picker of take `take` shows: its title, its choices
/// (each line's folder and label: every line in the game's order, by
/// name, its place said, and "Not sorted yet" first for one that is on a
/// line) and its note.
pub struct Picker {
    pub title: UiText,
    pub items: Vec<(String, String)>,
    pub note: UiText,
}

pub fn picker(draft: &Draft, take: u64) -> Option<Picker> {
    let t = draft.take(take)?;
    // In the page's order: song select's, play's, the results'.
    let mut lines = draft.lines();
    lines.sort_by_key(|l| match l.spoken() {
        Spoken::Menu => 0,
        Spoken::Play => 1,
        Spoken::Results => 2,
    });
    let mut items: Vec<(String, String)> = lines
        .into_iter()
        .map(|l| {
            let label = match l.spoken() {
                Spoken::Menu => m::picker_line_select(name(l)),
                Spoken::Play => m::picker_line_play(name(l)),
                Spoken::Results => m::picker_line_results(name(l)),
            };
            (l.folder(), label.into())
        })
        .collect();
    if t.line.is_some() {
        items.insert(0, ("unsorted".into(), m::picker_unsorted().into()));
    }
    Some(Picker {
        title: m::picker_title(),
        items,
        note: m::picker_note(t.file_name()),
    })
}

/// The dialog choosing take `take`'s line ([`picker`]).
pub fn line_picker(env: Env, draft: &Draft, take: u64, on_pick: LineFn) -> Option<ActionsDialog> {
    let p = picker(draft, take)?;
    let pick: super::editor_dialogs::PickFn = Rc::new(move |id: &str, cx: &mut Cx| {
        on_pick(take, Line::parse(id), cx);
    });
    Some(ActionsDialog::new(env, &p.title, p.items, pick).with_note(&p.note))
}

/// Whether a press of `id` needs the press itself: a picker, a sound,
/// a download.
pub fn in_gesture(a: Act) -> bool {
    !matches!(
        a,
        Act::Tidy
            | Act::Remove(_)
            | Act::Examples
            | Act::StepMania
            | Act::Details
            | Act::Export
            | Act::UseInGame
    )
}

/// How each take goes into a pack, for the shells' debug state.
pub fn writes(draft: &Draft) -> Vec<(u64, Write)> {
    draft
        .takes
        .iter()
        .map(|t| (t.id, draft.write_of(t)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_library::announcer::draft::listed;

    const MP3: &[u8] = b"ID3\x04\x00";

    #[test]
    fn every_line_has_a_name_and_a_moment() {
        let mut names = std::collections::BTreeSet::new();
        for line in Line::all().into_iter().chain([Line::Combo(1500)]) {
            assert!(
                !name(line).is_empty() && when(line).ends_with('.'),
                "{line:?}"
            );
            names.insert((name(line), line.spoken() == Spoken::Results));
        }
        // Names repeat only across play and the results (the full combos).
        assert!(names.len() >= listed().len() - 4);
    }

    #[test]
    fn presses_are_read_back() {
        for a in [
            Act::ImportFiles,
            Act::Add(Line::Combo(100)),
            Act::Play(3),
            Act::Example(Line::SelectMusic(Comment::Hard), 2),
            Act::Remove(9),
            Act::Save,
        ] {
            let id = match a {
                Act::ImportFiles => "import-files".to_string(),
                Act::Add(l) => format!("add:{}", l.folder()),
                Act::Play(t) => format!("play:{t}"),
                Act::Example(l, i) => format!("example:{}:{i}", l.folder()),
                Act::Remove(t) => format!("remove:{t}"),
                _ => "save".into(),
            };
            assert_eq!(act(&id), Some(a), "{id}");
        }
        assert_eq!(act("play:x"), None);
        assert_eq!(act("add:nothing"), None);
    }

    #[test]
    fn song_select_and_results_lines_can_be_heard_recorded_or_not() {
        let mut d = Draft::default();
        d.add("evaluation aaaa.wav", 1, MP3);
        d.add("select music intro.wav", 2, MP3);
        d.add("gameplay ready.wav", 3, MP3);
        for line in [
            Line::Grade(Grade::Aaaa),
            Line::Grade(Grade::Aaa),
            Line::SelectMusicIntro,
            Line::SelectMusic(Comment::New),
        ] {
            assert!(hearable(line), "{line:?}");
        }
        // No play gets it; play's scenes come with step 4.
        assert!(!hearable(Line::Grade(Grade::Aaaaa)));
        assert!(!hearable(Line::Ready));
        assert!(recorded(Line::Grade(Grade::Aaaa), &d));
        assert!(!recorded(Line::Grade(Grade::Aaa), &d));
        // What speaks in its place counts.
        assert!(!recorded(Line::Grade(Grade::F), &d));
        d.add("evaluation e.wav", 4, MP3);
        assert!(recorded(Line::Grade(Grade::F), &d));
        let text = format!("{:?}", doc(&d, &View::default(), &BTreeMap::new()).blocks);
        assert!(text.contains("\"hear:evaluation aaaa\", label: \"Hear it in the game\""));
        assert!(text.contains("\"hear:evaluation aaa\", label: \"Hear an example in the game\""));
        assert!(!text.contains("\"hear:gameplay ready\""));
        assert!(!text.contains("\"hear:evaluation aaaaa\""));
        assert_eq!(act("hear:evaluation cheer"), Some(Act::Hear(Line::Cheer)));
        assert!(in_gesture(Act::Hear(Line::Cheer)));
    }

    fn zones(doc: &PageDoc) -> Vec<String> {
        doc.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Zone { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_page_has_a_zone_for_every_line_and_the_unsorted() {
        let mut d = Draft::default();
        let empty = doc(&d, &View::default(), &BTreeMap::new());
        let z = zones(&empty);
        assert_eq!(z.len(), listed().len());
        assert_eq!(z[0], "select music intro");
        // Nothing to export yet.
        let text = format!("{:?}", empty.blocks);
        assert!(!text.contains("\"export\"") && !text.contains("\"use\""));
        d.add("New Recording.m4a", 1, b"");
        d.add("gameplay 1100 combo.wav", 1, MP3);
        let full = doc(&d, &View::default(), &BTreeMap::new());
        let z = zones(&full);
        assert_eq!(z[0], "unsorted");
        assert!(z.contains(&"gameplay 1100 combo".to_string()));
        let text = format!("{:?}", full.blocks);
        assert!(text.contains("\"export\"") && text.contains("\"use\""));
        assert!(text.contains("Which line?"));
    }

    /// The blocks' text, folds unfolded.
    fn flat(blocks: &[Block]) -> Vec<&Block> {
        let mut out = Vec::new();
        for b in blocks {
            out.push(b);
            if let Block::Fold { body, .. } | Block::Zone { body, .. } = b {
                out.extend(flat(body));
            }
        }
        out
    }

    #[test]
    fn credits_imports_and_switches_are_under_advanced() {
        let d = Draft::default();
        let page = doc(&d, &View::default(), &BTreeMap::new());
        let top: Vec<String> = page
            .blocks
            .iter()
            .take_while(|b| !matches!(b, Block::Fold { .. }))
            .map(|b| format!("{b:?}"))
            .collect::<Vec<_>>();
        let top = top.join(" ");
        assert!(top.contains("\"name\"") && !top.contains("\"author\""));
        assert!(!top.contains("import-") && !top.contains("\"tidy\""));
        let Some(Block::Fold { id, body, .. }) =
            page.blocks.iter().find(|b| matches!(b, Block::Fold { .. }))
        else {
            panic!("no Advanced");
        };
        assert_eq!(id, "advanced");
        let advanced = format!("{body:?}");
        for id in [
            "author",
            "license",
            "import-files",
            "import-folder",
            "tidy",
            "stepmania",
            "details",
        ] {
            assert!(advanced.contains(&format!("\"{id}\"")), "{id}");
        }
        // No take has a tidy switch of its own; the tidying is told only
        // with the details.
        let mut d = Draft::default();
        d.add("gameplay ready.mp3", 1, MP3);
        let id = d.takes[0].id;
        d.take_mut(id).unwrap().sound = Sound::Read(ddi_library::announcer::tidy::measure(
            &[vec![0.0; 4800]],
            48000,
        ));
        let says = |details: bool| {
            let view = View {
                details,
                ..View::default()
            };
            flat(&doc(&d, &view, &BTreeMap::new()).blocks)
                .iter()
                .any(|b| matches!(b, Block::Muted(s) if s.starts_with("Tid")))
        };
        assert!(!says(false) && says(true));
        assert!(!format!("{:?}", doc(&d, &View::default(), &BTreeMap::new())).contains("tidy:"));
    }

    #[test]
    fn a_missing_line_says_what_speaks_instead() {
        let mut d = Draft::default();
        assert!(without(Line::Restart, &d).contains("says nothing"));
        d.add("gameplay ready.mp3", 1, MP3);
        assert!(without(Line::Restart, &d).contains("“Ready”"));
        assert!(without(Line::FirstClear, &d).contains("next line"));
    }

    #[test]
    fn the_page_says_each_take_and_its_tidying() {
        ddi_l10n::set_language("en");
        let (d, view, ex) = crate::screens::sample::studio();
        let text = format!("{:?}", doc(&d, &view, &ex).blocks);
        for s in [
            "■ ready.mp3 · 2.5 s",
            "Tidied: 0.5 s of silence cut, louder by 6 dB",
            "Tidied: quieter by 6 dB",
            "Tidied: 0.3 s of silence cut",
            "Tidy: nothing to change",
            "As recorded",
            "double a.m4a · cannot be played",
            "▶ intro.mp3 · reading…",
            "This file cannot be played here (the data could not be read (EncodingError)).",
            "It was recorded too loud: it crackles where it peaks.",
            "At 3.0 s it runs past what comes next in the game (2.0 s).",
            "Ready and here we go take 4.5 s together",
            "Tidy recordings: on",
            "Also for StepMania: off",
            "Details: on",
            "Save Announcer.zip",
            "Not sorted yet (1)",
            "Folder: gameplay ready",
            "▶ “One hundred combo!”",
            "Nothing yet: “Ready” is said instead.",
            "Added 9 recordings. 1 file was passed over: not sounds.",
            "(2,000 sounds, 128 MB)",
        ] {
            assert!(text.contains(s), "{s} in {text}");
        }
        let p = picker(&d, d.takes[0].id).unwrap();
        assert_eq!(p.items[0].1, "Not sorted yet");
        assert!(
            p.items
                .contains(&("gameplay ready".to_string(), "Ready · Gameplay".to_string()))
        );
        assert_eq!(&*p.note, "ready.mp3 plays as you choose.");
    }

    #[test]
    fn after_a_scene_the_studio_says_what_was_heard() {
        ddi_l10n::set_language("en");
        let aa = Line::Grade(Grade::Aa);
        assert_eq!(
            &*heard(aa, &[aa, Line::Cheer], None),
            "Heard in the game: “Grade AA”, then “Cheer”."
        );
        assert_eq!(
            &*heard(
                Line::SelectMusicIntro,
                &[Line::SelectMusicIntro],
                Some("Mr. Clank")
            ),
            "Heard in the game, an example by Mr. Clank: “Song select”."
        );
        assert_eq!(&*heard(aa, &[], None), "Nothing was said in the game.");
        assert!(heard(Line::Cheer, &[], None).contains("none of those lines is recorded"));
        ddi_l10n::set_language("ja");
        assert_eq!(
            &*heard(aa, &[aa, Line::Cheer], None),
            "ゲーム内で「評価AA」、続いて「声援」を再生しました。"
        );
        ddi_l10n::set_language("en");
    }

    #[test]
    fn examples_join_our_sounds_and_the_studios() {
        let mut kept = ddi_library::announcer::Kept {
            name: "Mr. Clank".into(),
            ..Default::default()
        };
        kept.lines.insert(
            "gameplay 100 combo".into(),
            vec!["gameplay 100 combo/one.mp3".into()],
        );
        let ours = Bundled {
            path: "Mr. Clank".into(),
            kept,
            hashes: BTreeMap::new(),
            said: BTreeMap::from([(
                "gameplay 100 combo/one.mp3".to_string(),
                "One hundred combo!".to_string(),
            )]),
        };
        let ex = examples(Set::English, Some(&ours));
        let combo = &ex[&Line::Combo(100)];
        assert_eq!(combo[0].text, "One hundred combo!");
        assert_eq!(
            combo[0].sound.as_deref(),
            Some("gameplay 100 combo/one.mp3")
        );
        assert!(combo.len() > 1 && combo[1].sound.is_none());
        assert_eq!(next_set(Set::English), Set::Japanese);
        assert_eq!(next_set(Set::Japanese), Set::English);
    }
}
