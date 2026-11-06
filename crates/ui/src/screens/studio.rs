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
pub fn name(line: Line) -> String {
    match line {
        Line::Ready => "Ready".into(),
        Line::HereWeGo => "Here we go".into(),
        Line::Restart => "Restart".into(),
        Line::CommentHot => "Doing great".into(),
        Line::CommentGood => "Doing fine".into(),
        Line::CommentDanger => "In danger".into(),
        Line::CommentOni => "On lives".into(),
        Line::Combo(n) => format!("{n} combo"),
        Line::ComboOverflow => "More combo".into(),
        Line::ComboStopped => "Combo broken".into(),
        Line::FullCombo => "Full combo".into(),
        Line::FullComboW(1) => "Full combo, all perfect".into(),
        Line::FullComboW(2) => "Full combo, nearly perfect".into(),
        Line::FullComboW(_) => "Full combo, all good".into(),
        Line::LastLife => "Last life".into(),
        Line::LifeLost => "Life lost".into(),
        Line::Cleared => "Cleared".into(),
        Line::Failed => "Failed".into(),
        Line::Quit => "Quit".into(),
        Line::FirstClear => "First clear".into(),
        Line::NewRecord => "New record".into(),
        Line::EvaluationFullCombo(1) => "Full combo, all perfect".into(),
        Line::EvaluationFullCombo(2) => "Full combo, nearly perfect".into(),
        Line::EvaluationFullCombo(3) => "Full combo, all good".into(),
        Line::EvaluationFullCombo(_) => "Full combo".into(),
        Line::Grade(g @ (Grade::E | Grade::F)) => format!("Failed play ({})", grade_letters(g)),
        Line::Grade(g) => format!("Grade {}", grade_letters(g)),
        Line::Cheer => "Cheer".into(),
        Line::SelectMusicIntro => "Song select".into(),
        Line::SelectMusic(Comment::New) => "A new chart".into(),
        Line::SelectMusic(Comment::Repeat) => "Played again".into(),
        Line::SelectMusic(Comment::Hard) => "A hard chart".into(),
        Line::SelectMusic(Comment::General) => "Any other chart".into(),
    }
}

/// When a line speaks, in a sentence.
pub fn when(line: Line) -> String {
    let grade = |place: &str| format!("On the results, for the {place} grade.");
    match line {
        Line::Ready => "As a song starts.".into(),
        Line::HereWeGo => "Just before the first note: it ends half a second before it.".into(),
        Line::Restart => "As a song starts again at once (R held, or pressed twice).".into(),
        Line::CommentHot => "Every 10 seconds while the life is full.".into(),
        Line::CommentGood => "Every 10 seconds otherwise.".into(),
        Line::CommentDanger => "Every 10 seconds while the life is about to run out.".into(),
        Line::CommentOni => "Every 10 seconds on a life gauge of lives (a battery).".into(),
        Line::Combo(n) if n > 1000 => format!("{n} notes in a row (in place of More combo)."),
        Line::Combo(n) => format!("{n} notes in a row."),
        Line::ComboOverflow => "Each hundred notes in a row past 1000.".into(),
        Line::ComboStopped => "When more than 50 notes in a row end with a miss.".into(),
        Line::FullCombo => "As the last note is hit with no note missed.".into(),
        Line::FullComboW(1) => "As the last note is hit, every note at the best judgment.".into(),
        Line::FullComboW(2) => {
            "As the last note is hit, every note at the second judgment or better.".into()
        }
        Line::FullComboW(_) => {
            "As the last note is hit, every note at the third judgment or better.".into()
        }
        Line::LastLife => "When one life is left on a gauge of lives.".into(),
        Line::LifeLost => "When a life is lost on a gauge of lives.".into(),
        Line::Cleared => "As the last note is judged.".into(),
        Line::Failed => "When the life runs out.".into(),
        Line::Quit => "On the results of a song the player quit.".into(),
        Line::FirstClear => "On the results, the first time a chart is cleared.".into(),
        Line::NewRecord => "On the results, for a new best on a chart played before.".into(),
        Line::EvaluationFullCombo(1) => {
            "On the results of a full combo with every note at the best judgment.".into()
        }
        Line::EvaluationFullCombo(2) => {
            "On the results of a full combo with every note at the second judgment or better."
                .into()
        }
        Line::EvaluationFullCombo(3) => {
            "On the results of a full combo with every note at the third judgment or better.".into()
        }
        Line::EvaluationFullCombo(_) => "On the results of any other full combo.".into(),
        Line::Grade(Grade::Aaaaa) => "Never: no rules give this grade.".into(),
        Line::Grade(Grade::Aaaa) => grade("best"),
        Line::Grade(Grade::Aaa) => grade("second best"),
        Line::Grade(Grade::Aa) => grade("third best"),
        Line::Grade(Grade::A) => grade("fourth"),
        Line::Grade(Grade::B) => grade("fifth"),
        Line::Grade(Grade::C) => grade("sixth"),
        Line::Grade(Grade::D) => grade("lowest passing"),
        Line::Grade(_) => "On the results of a failed song.".into(),
        Line::Cheer => {
            "2.5 seconds after the results' line, for the third best grade or better.".into()
        }
        Line::SelectMusicIntro => "Each time the song list shows, after a song too.".into(),
        Line::SelectMusic(Comment::New) => {
            "When a chart is chosen that was never played under these rules.".into()
        }
        Line::SelectMusic(Comment::Repeat) => {
            "When a chart is chosen that was played earlier this session.".into()
        }
        Line::SelectMusic(Comment::Hard) => "When a chart of level 10 or more is chosen.".into(),
        Line::SelectMusic(Comment::General) => "When any other chart is chosen.".into(),
    }
}

/// What speaks where the draft has nothing on `line`.
pub fn without(line: Line, draft: &Draft) -> String {
    if let Some(other) = line
        .chain()
        .into_iter()
        .skip(1)
        .find(|l| draft.takes_of(*l).next().is_some())
    {
        return format!("Nothing yet: “{}” is said instead.", name(other));
    }
    match line {
        Line::FirstClear | Line::NewRecord | Line::EvaluationFullCombo(_) => {
            "Nothing yet: the results' next line is said instead.".into()
        }
        _ => "Nothing yet: the game says nothing then.".into(),
    }
}

/// A take's line on the page: its name, its length and what tidying does.
fn take_label(draft: &Draft, t: &Take, playing: bool) -> String {
    let mut name: String = t.file_name().chars().take(32).collect();
    if t.file_name().chars().count() > 32 {
        name.push('…');
    }
    let icon = if playing { "■" } else { "▶" };
    match (&t.sound, draft.seconds(t)) {
        (Sound::Read(_), Some(s)) => format!("{icon} {name} · {s:.1} s"),
        (Sound::Unreadable(_), _) => format!("{name} · cannot be played"),
        _ => format!("{icon} {name} · reading…"),
    }
}

/// What a note says.
fn note_text(note: &Note) -> String {
    match note {
        Note::Unreadable(why) => format!("This file cannot be played here ({why})."),
        Note::Clipped => "It was recorded too loud: it crackles where it peaks.".into(),
        Note::TooLong { seconds, limit } => {
            format!("At {seconds:.1} s it runs past what comes next in the game ({limit:.1} s).")
        }
    }
}

/// What tidying did to a take, where it did something.
fn tidied(draft: &Draft, t: &Take) -> Option<String> {
    let m = t.measure()?;
    if !draft.tidies(t) {
        return Some("As recorded".into());
    }
    let cut = m.seconds - m.tidied_seconds();
    let db = m.gain_db();
    let mut said = Vec::new();
    if cut >= 0.05 {
        said.push(format!("{cut:.1} s of silence cut"));
    }
    if db.abs() >= 1.0 {
        said.push(format!(
            "{} by {:.0} dB",
            if db > 0.0 { "louder" } else { "quieter" },
            db.abs()
        ));
    }
    Some(if said.is_empty() {
        "Tidy: nothing to change".into()
    } else {
        format!("Tidied: {}", said.join(", "))
    })
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
        if sorted { "Move…" } else { "Which line?" },
    ));
    actions.push(Action::new(format!("remove:{}", t.id), "Remove"));
    out.push(Block::Actions(actions));
    if view.details
        && let Some(s) = tidied(draft, t)
    {
        out.push(Block::Muted(s));
    }
    for n in draft.notes(t) {
        out.push(Block::Note(note_text(&n)));
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
    let mut body = vec![Block::Subheading(name(line)), Block::Muted(when(line))];
    if view.details {
        body.push(Block::Muted(format!("Folder: {folder}")));
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
                        label: format!("{icon} “{}”", e.text),
                    }
                }
                None => Block::Text(format!("“{}”", e.text)),
            })
            .collect();
        body.push(Block::Fold {
            id: format!("examples:{folder}"),
            label: format!("Examples ({})", ex.len()),
            body: items,
        });
    }
    let takes: Vec<&Take> = draft.takes_of(line).collect();
    if takes.is_empty() {
        body.push(Block::Muted(without(line, draft)));
    }
    for t in takes {
        take_blocks(draft, view, t, true, &mut body);
    }
    if line == Line::HereWeGo
        && let Some(s) = draft.opening()
        && s > ddi_library::announcer::draft::OPENING_ROOM
    {
        body.push(Block::Note(format!(
            "Ready and here we go take {s:.1} s together: a song whose first note comes sooner \
             says only here we go."
        )));
    }
    let mut actions = vec![Action::new(format!("add:{folder}"), "Add recording…")];
    if hearable(line) {
        let label = if recorded(line, draft) {
            "Hear it in the game"
        } else {
            "Hear an example in the game"
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

/// A switch of Advanced: its button, then what it does.
fn switch(id: &str, label: &str, on: bool, does: &str) -> [Block; 2] {
    [
        Block::Actions(vec![Action::new(
            id,
            format!("{label}: {}", if on { "on" } else { "off" }),
        )]),
        Block::Muted(does.into()),
    ]
}

/// The studio's page.
pub fn doc(draft: &Draft, view: &View, examples: &BTreeMap<Line, Vec<Example>>) -> PageDoc {
    let mut b = vec![
        Block::Text(
            "Drop each recording of your voice on its line below, or add it with the line's \
             Add recording button. Then use your announcer in the game, or export it to share."
                .into(),
        ),
        Block::Field {
            id: "name".into(),
            label: "Name".into(),
            value: draft.name.clone(),
            hint: "Your announcer's name".into(),
        },
    ];
    let mut actions = vec![Action::new(
        "examples",
        format!("Examples: {}", view.examples.name()),
    )];
    let has = draft.takes.iter().any(|t| t.line.is_some());
    if has && !view.busy {
        actions.push(Action::new("use", "Use in the game"));
        match &view.ready {
            Some(zip) => actions.push(Action::new("save", format!("Save {zip}")).primary()),
            None => actions.push(Action::new("export", "Export…")),
        }
    }
    b.push(Block::Actions(actions));
    if !view.status.is_empty() {
        b.push(Block::Muted(view.status.clone()));
    }
    if draft.too_big() {
        b.push(Block::Note(format!(
            "The pack is past what the game keeps of an announcer ({} sounds, {} MB): the last \
             ones would be left out.",
            ddi_library::announcer::MAX_FILES,
            ddi_library::announcer::MAX_BYTES >> 20
        )));
    }
    let mut advanced = vec![
        Block::Field {
            id: "author".into(),
            label: "By".into(),
            value: draft.author.clone(),
            hint: "Who made it".into(),
        },
        Block::Field {
            id: "license".into(),
            label: "License".into(),
            value: draft.license.clone(),
            hint: "How others may use it".into(),
        },
        Block::Actions(vec![
            Action::new("import-files", "Import a pack from a zip…"),
            Action::new("import-folder", "Import a pack folder…"),
        ]),
        Block::Muted(
            "An announcer pack made before, here or for StepMania: its recordings go on their \
             lines."
                .into(),
        ),
    ];
    advanced.extend(switch(
        "tidy",
        "Tidy recordings",
        draft.tidy,
        "Cuts the silence before and after each recording and evens its loudness with our \
         announcers'.",
    ));
    advanced.extend(switch(
        "stepmania",
        "Also for StepMania",
        draft.stepmania,
        "Writes only the sound formats StepMania reads (MP3, WAV and Ogg Vorbis), others as WAV.",
    ));
    advanced.extend(switch(
        "details",
        "Details",
        view.details,
        "Shows each line's folder and what tidying did to each recording.",
    ));
    b.push(Block::Fold {
        id: "advanced".into(),
        label: "Advanced".into(),
        body: advanced,
    });
    let unsorted: Vec<&Take> = draft.unsorted().collect();
    if !unsorted.is_empty() {
        let mut body = vec![
            Block::Heading(format!("Not sorted yet ({})", unsorted.len())),
            Block::Muted(
                "Press Which line? to hear a recording and choose its line, or drop it on its \
                 line below."
                    .into(),
            ),
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
        (Spoken::Menu, "On the song select screen"),
        (Spoken::Play, "During gameplay"),
        (Spoken::Results, "On the results screen"),
    ] {
        b.push(Block::Heading(heading.into()));
        for &line in lines.iter().filter(|l| l.spoken() == spoken) {
            b.push(line_zone(draft, view, line, examples));
        }
    }
    PageDoc {
        title: "Announcer studio".into(),
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

/// The dialog choosing take `take`'s line: every line in the game's
/// order (by name, its place said), and "Not sorted yet" for one that
/// is on a line.
pub fn line_picker(env: Env, draft: &Draft, take: u64, on_pick: LineFn) -> Option<ActionsDialog> {
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
            let place = match l.spoken() {
                Spoken::Menu => "Song select screen",
                Spoken::Play => "Gameplay",
                Spoken::Results => "Results screen",
            };
            (l.folder(), format!("{} · {place}", name(l)))
        })
        .collect();
    if t.line.is_some() {
        items.insert(0, ("unsorted".into(), "Not sorted yet".into()));
    }
    let pick: super::editor_dialogs::PickFn = Rc::new(move |id: &str, cx: &mut Cx| {
        on_pick(take, Line::parse(id), cx);
    });
    Some(
        ActionsDialog::new(env, "Which line is it?", items, pick)
            .with_note(&format!("{} plays as you choose.", t.file_name())),
    )
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
