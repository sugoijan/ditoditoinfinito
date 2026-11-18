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
use ddi_library::announcer::draft::{self, Draft, Note, Script, Sound, Take};
use ddi_library::announcer::examples::{self, Set};
use ddi_library::announcer::{
    Battle, Bundled, Comment, Grade, Line, OUR_COMBOS, Spoken, past_1000,
};
use ddi_platform::storage::{self, Area};

use super::Env;
use super::data::Platform;
use super::dialog::{ActionsDialog, PickFn};
use super::editor_dialogs::{SubmitFn, ValueDialog};
use super::page::{Action, Block, Icon, PageDoc, PageTitle};
use crate::screen::Cx;

/// The page's name, for tests and the debug state.
pub const NAME: &str = "announcer-studio";
/// Where the example set chosen is remembered.
const EXAMPLES_KEY: &str = "ddi.studio.examples";

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
        Line::Grade(g @ (Grade::E | Grade::F)) => {
            m::line_grade_failed(g.name().to_ascii_uppercase())
        }
        Line::Grade(g) => m::line_grade(g.name().to_ascii_uppercase()),
        Line::Cheer => m::line_cheer(),
        Line::SelectMusicIntro => m::line_select_intro(),
        Line::SelectMusic(Comment::New) => m::line_select_new(),
        Line::SelectMusic(Comment::Repeat) => m::line_select_repeat(),
        Line::SelectMusic(Comment::Hard) => m::line_select_hard(),
        Line::SelectMusic(Comment::General) => m::line_select_general(),
        Line::Battle(b) => match b {
            Battle::MatchBegin => m::line_battle_match_begin(),
            Battle::StageBegin => m::line_battle_stage_begin(),
            Battle::FinalStage => m::line_battle_final_stage(),
            Battle::LifeCritical => m::line_battle_life_critical(),
            Battle::AttackIncoming => m::line_battle_attack_incoming(),
            Battle::AttackLaunched => m::line_battle_attack_launched(),
            Battle::StepUp => m::line_battle_step_up(),
            Battle::StepDown => m::line_battle_step_down(),
            Battle::Survivors(n) => m::line_battle_survivors(n),
            Battle::Eliminated => m::line_battle_eliminated(),
            Battle::StageWon => m::line_battle_stage_won(),
            Battle::StageNotWon => m::line_battle_stage_not_won(),
            Battle::StageDrawn => m::line_battle_stage_drawn(),
            Battle::SongFinished => m::line_battle_song_finished(),
            Battle::MatchWon => m::line_battle_match_won(),
            Battle::MatchNotWon => m::line_battle_match_not_won(),
            Battle::MatchDrawn => m::line_battle_match_drawn(),
        },
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
        Line::Combo(n) if past_1000(n) => m::when_combo_past_1000(n),
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
        Line::Battle(b) => match b {
            Battle::MatchBegin => m::when_battle_match_begin(),
            Battle::StageBegin => m::when_battle_stage_begin(),
            Battle::FinalStage => m::when_battle_final_stage(),
            Battle::LifeCritical => m::when_battle_life_critical(),
            Battle::AttackIncoming => m::when_battle_attack_incoming(),
            Battle::AttackLaunched => m::when_battle_attack_launched(),
            Battle::StepUp => m::when_battle_step_up(),
            Battle::StepDown => m::when_battle_step_down(),
            Battle::Survivors(n) => m::when_battle_survivors(n),
            Battle::Eliminated => m::when_battle_eliminated(),
            Battle::StageWon => m::when_battle_stage_won(),
            Battle::StageNotWon => m::when_battle_stage_not_won(),
            Battle::StageDrawn => m::when_battle_stage_drawn(),
            Battle::SongFinished => m::when_battle_song_finished(),
            Battle::MatchWon => m::when_battle_match_won(),
            Battle::MatchNotWon => m::when_battle_match_not_won(),
            Battle::MatchDrawn => m::when_battle_match_drawn(),
        },
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
        // A battle's moment shows on screen without a line
        // (`ddi_session::announcer::battle`).
        Line::Battle(_) => m::missing_shown(),
        _ => m::missing_silent(),
    }
}

/// Seconds as the studio writes them, one decimal, for a message.
pub fn seconds(s: f64) -> String {
    format!("{s:.1}")
}

/// A take's line on the page: its name and its length.
fn take_label(draft: &Draft, t: &Take) -> String {
    let mut name: String = t.file_name().chars().take(32).collect();
    if t.file_name().chars().count() > 32 {
        name.push('…');
    }
    match (&t.sound, draft.seconds(t)) {
        (Sound::Read(_), Some(s)) => m::take(&name, seconds(s)).into(),
        (Sound::Unreadable(_), _) => m::take_unplayable(&name).into(),
        _ => m::take_reading(&name).into(),
    }
}

/// The play button of `id` (`take:<id>`, `example:<folder>:<i>`),
/// pressed as `press`: a stop while it plays.
fn play_button(view: &View, id: &str, press: String) -> Action {
    let playing = view.playing.as_deref() == Some(id);
    Action::icon(press, if playing { Icon::Stop } else { Icon::Play })
}

/// The field of the words of text line or take `id`.
fn words_field(id: u64, words: &str) -> Block {
    Block::Field {
        id: format!("words:{id}"),
        label: String::new(),
        value: words.to_string(),
        hint: m::words_hint().into(),
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

/// A take's blocks: its words where it was made for a text line, its
/// name with the buttons to hear it and to see it in its scene (where it
/// is on a line seen in one), what can be done with it, what tidying did
/// (with the details) and its notes.
fn take_blocks(draft: &Draft, view: &View, t: &Take, sorted: bool, out: &mut Vec<Block>) {
    if let Some(words) = &t.words {
        out.push(words_field(t.id, words));
    }
    let mut icons = Vec::new();
    if !matches!(t.sound, Sound::Unreadable(_)) {
        icons.push(play_button(
            view,
            &format!("take:{}", t.id),
            format!("play:{}", t.id),
        ));
    }
    if t.line.is_some_and(hearable) && matches!(t.sound, Sound::Read(_)) {
        icons.push(Action::icon(format!("hear-take:{}", t.id), Icon::Eye));
    }
    out.push(Block::Entry {
        text: take_label(draft, t),
        detail: None,
        actions: icons,
    });
    if !t.tags.is_empty() {
        out.push(Block::Muted(m::take_tags(t.tags.join(", ")).into()));
    }
    let mut actions = vec![Action::new(
        format!("move:{}", t.id),
        if sorted {
            m::take_move()
        } else {
            m::take_which_line()
        },
    )];
    actions.push(Action::new(format!("tags:{}", t.id), m::take_tags_button()));
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

/// A text line's blocks: its words to write, and its buttons.
fn script_blocks(s: &Script, out: &mut Vec<Block>) {
    out.push(words_field(s.id, &s.words));
    out.push(Block::Actions(vec![
        Action::new(format!("add-to:{}", s.id), m::add_text_recording()),
        Action::new(format!("unwrite:{}", s.id), m::take_remove()),
    ]));
}

/// A line's examples, each its words; beside those our announcer says,
/// a button to hear it and one to see it in the line's scene.
fn example_blocks(view: &View, line: Line, ex: &[Example]) -> Vec<Block> {
    let folder = line.folder();
    let mut out = vec![Block::Muted(m::examples_heading().into())];
    for (i, e) in ex.iter().enumerate() {
        let mut actions = Vec::new();
        if e.sound.is_some() {
            let id = format!("example:{folder}:{i}");
            actions.push(play_button(view, &id, id.clone()));
            if hearable(line) {
                actions.push(Action::icon(
                    format!("hear-example:{folder}:{i}"),
                    Icon::Eye,
                ));
            }
        }
        out.push(Block::Entry {
            text: m::example(&e.text).into(),
            detail: None,
            actions,
        });
    }
    out
}

/// A line's zone: its name and when it is said, then its recordings and
/// its buttons beside its examples.
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
    // Which languages have it, where there are several.
    if draft.languages.len() > 1 {
        let langs = draft.languages_of(line);
        if !langs.is_empty() {
            let names: Vec<&str> = langs.iter().map(|l| draft::language_name(l)).collect();
            body.push(Block::Muted(m::recorded_in(names.join(", ")).into()));
        }
    }
    let ex = examples.get(&line).map(Vec::as_slice).unwrap_or_default();
    let mut left = vec![Block::Muted(m::recordings_heading().into())];
    let takes: Vec<&Take> = draft.takes_of(line).collect();
    let unrecorded = takes.is_empty();
    if unrecorded {
        // Each recording and example is seen in the game by its own
        // eye; the line by this one where neither is there but
        // something speaks in its place.
        let see = if recorded(line, draft) {
            true
        } else {
            has_example(line) && !ex.iter().any(|e| e.sound.is_some())
        };
        left.push(Block::Entry {
            text: String::new(),
            detail: Some(without(line, draft).into()),
            actions: if see && hearable(line) {
                vec![Action::icon(format!("hear:{folder}"), Icon::Eye)]
            } else {
                Vec::new()
            },
        });
    }
    for t in takes {
        take_blocks(draft, view, t, true, &mut left);
    }
    let scripts: Vec<&Script> = draft.scripts_of(line).collect();
    let written = !scripts.is_empty();
    for s in scripts {
        script_blocks(s, &mut left);
    }
    if line == Line::HereWeGo
        && let Some(s) = draft.opening()
        && s > ddi_library::announcer::draft::OPENING_ROOM
    {
        left.push(Block::Note(m::opening_long(seconds(s)).into()));
    }
    let mut actions = vec![
        Action::new(format!("add:{folder}"), m::add_recording()),
        Action::new(format!("write:{folder}"), m::add_text()),
    ];
    if let Line::Combo(n) = line
        && unrecorded
        && !written
    {
        actions.push(Action::new(format!("remove-combo:{n}"), m::remove_combo()));
    }
    left.push(Block::Actions(actions));
    let right = if ex.is_empty() {
        Vec::new()
    } else {
        example_blocks(view, line, ex)
    };
    body.push(Block::Columns { left, right });
    Block::Zone { id: folder, body }
}

/// Whether `line` has a scene to hear it in: on song select, in play
/// (step 4) and on the results (`evaluation aaaaa`, which no play gets,
/// has none, nor have a multiplayer battle's lines yet).
pub fn hearable(line: Line) -> bool {
    line != Line::Grade(Grade::Aaaaa) && line.spoken() != Spoken::Battle
}

/// Whether our packs of the examples say `line`, or what speaks in its
/// place: a count of a pack's own (64) has nothing to hear until it is
/// recorded.
fn has_example(line: Line) -> bool {
    match line {
        Line::Combo(n) => OUR_COMBOS.contains(&n) || past_1000(n),
        _ => true,
    }
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
    // A scene in play says more besides: the line asked for, and how
    // many more.
    if heard.len() > 2
        && let Some(said) = heard
            .iter()
            .find(|l| **l == line)
            .or_else(|| heard.iter().find(|l| line.chain().contains(l)))
    {
        let others = heard.len() as u32 - 1;
        return match example {
            None => m::status_heard_with(name(*said), others),
            Some(pack) => m::status_heard_example_with(pack, name(*said), others),
        };
    }
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

/// The draft's languages: what they are, a button for each to work in
/// (the one worked in marked), one to add a language (to name its first),
/// one to take off the one worked in while nothing is in it, and Mixed
/// where there are two or more.
fn language_blocks(draft: &Draft) -> Vec<Block> {
    let working = draft.working_language();
    let mut out = Vec::new();
    let mut actions = Vec::new();
    match draft.languages.as_slice() {
        [] => {
            out.push(Block::Muted(m::languages_none().into()));
            actions.push(Action::new("add-language", m::name_language()));
        }
        [one] => {
            out.push(Block::Muted(m::languages_one(draft::language_name(one)).into()));
            actions.push(Action::new("add-language", m::add_language()));
        }
        many => {
            out.push(Block::Muted(m::languages_many().into()));
            for (i, l) in many.iter().enumerate() {
                let a = Action::new(format!("language:{i}"), UiText::data(draft::language_name(l)));
                actions.push(if Some(l.as_str()) == working { a.primary() } else { a });
            }
            actions.push(Action::new("add-language", m::add_language()));
        }
    }
    out.push(Block::Actions(actions));
    if draft.languages.len() > 1 {
        if let Some(w) = working.filter(|w| !draft.speaks(w))
            && let Some(i) = draft.languages.iter().position(|l| l == w)
        {
            out.push(Block::Actions(vec![Action::new(
                format!("remove-language:{i}"),
                m::remove_language(draft::language_name(w)),
            )]));
        }
        out.extend(switch("mixed", m::mixed(state(draft.mixed)), m::mixed_description()));
    }
    out
}

/// What a line a tag switched off would leave silent comes to.
fn silenced_text(s: &draft::Silenced) -> UiText {
    let line = name(s.line);
    let instead = s.instead.map(name);
    let shown = matches!(s.line, Line::Battle(_));
    match &s.lang {
        None => match (instead, shown) {
            (Some(i), _) => m::tag_off_instead(line, i),
            (None, true) => m::tag_off_shown(line),
            (None, false) => m::tag_off_silent(line),
        },
        Some(lang) => {
            let lang = draft::language_name(lang).to_string();
            match (instead, shown) {
                (Some(i), _) => m::tag_off_instead_in(line, i, lang),
                (None, true) => m::tag_off_shown_in(line, lang),
                (None, false) => m::tag_off_silent_in(line, lang),
            }
        }
    }
}

/// The tags' fold: what tags are for, then each tag (data) with how
/// many recordings it is on and whether players may switch it off; a
/// switchable one with its name in Options, its first state, and the
/// lines it would leave with no recording when off.
fn tag_blocks(draft: &Draft, tags: &[String]) -> Vec<Block> {
    let mut out = vec![Block::Muted(m::tags_help().into())];
    for (i, tag) in tags.iter().enumerate() {
        out.push(Block::Subheading(UiText::data(tag).into()));
        out.push(Block::Muted(m::tag_used(draft.tagged(tag)).into()));
        let switch = draft.switch_of(tag);
        out.push(Block::Actions(vec![Action::new(
            format!("switchable:{i}"),
            m::tag_switchable(state(switch.is_some())),
        )]));
        let Some(switch) = switch else {
            continue;
        };
        out.push(Block::Field {
            id: format!("tag-label:{tag}"),
            label: m::tag_label().into(),
            value: switch.label.clone(),
            hint: m::tag_label_hint().into(),
        });
        out.extend(self::switch(
            &format!("tag-default:{i}"),
            m::tag_default(state(switch.default)),
            m::tag_default_description(),
        ));
        for s in draft.silenced(tag) {
            out.push(Block::Note(silenced_text(&s).into()));
        }
    }
    out
}

/// The studio's page, in `platform`'s words on dropping files and on
/// keeping them.
pub fn doc(
    draft: &Draft,
    view: &View,
    examples: &BTreeMap<Line, Vec<Example>>,
    platform: Platform,
) -> PageDoc {
    let mut b = vec![
        Block::Text(m::intro(platform.l10n()).into()),
        Block::Muted(m::icons_help().into()),
        Block::Field {
            id: "name".into(),
            label: m::name().into(),
            value: draft.name.clone(),
            hint: m::name_hint().into(),
        },
    ];
    let mut actions = vec![Action::new("examples", m::examples(view.examples.name()))];
    // Always there, grayed out while there is nothing to use or export.
    let can = !view.busy && draft.takes.iter().any(|t| t.line.is_some());
    actions.push(Action::new("use", m::use_in_game()).off(!can));
    match &view.ready {
        Some(zip) if can => actions.push(Action::new("save", m::save(zip)).primary()),
        _ => actions.push(Action::new("export", m::export()).off(!can)),
    }
    b.push(Block::Actions(actions));
    if !view.status.is_empty() {
        b.push(Block::Muted(view.status.clone()));
    }
    b.extend(language_blocks(draft));
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
        "stadium",
        m::stadium(state(draft.stadium)),
        m::stadium_description(),
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
    let tags = draft.tags();
    if !tags.is_empty() {
        b.push(Block::Fold {
            id: "tags".into(),
            label: m::tags(tags.len()).into(),
            body: tag_blocks(draft, &tags),
        });
    }
    let unsorted: Vec<&Take> = draft.unsorted().collect();
    if !unsorted.is_empty() {
        let mut body = vec![
            Block::Heading(m::unsorted(unsorted.len()).into()),
            Block::Muted(m::unsorted_help(platform.l10n()).into()),
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
        (Spoken::Battle, m::section_battle()),
    ] {
        b.push(Block::Heading(heading.into()));
        // Heard in multiplayer alone, with no scene here yet.
        if spoken == Spoken::Battle {
            b.push(Block::Muted(m::section_battle_note().into()));
        }
        for &line in lines.iter().filter(|l| l.spoken() == spoken) {
            // The combo lines' own button, after them.
            if line == Line::ComboOverflow {
                b.extend(switch("add-combo", m::add_combo(), m::add_combo_help()));
            }
            b.push(line_zone(draft, view, line, examples));
        }
    }
    PageDoc {
        title: PageTitle::new(m::title(), m::heading()),
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
    Stadium,
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
    /// The same with a recording of it, by its id, said in it.
    HearTake(u64),
    /// The same with our example `i` of a line said in it.
    HearExample(Line, usize),
    /// Pick a recording for text line `id`, which it takes the place
    /// of ([`Draft::add_to`]).
    AddTo(u64),
    /// A text line on a line, its words to write.
    Write(Line),
    /// Text line `id` off the page.
    Unwrite(u64),
    /// A combo line for a count typed in a dialog ([`combo_dialog`]).
    AddCombo,
    /// Combo line `n` off the page.
    RemoveCombo(u32),
    /// A language for the draft, chosen in a dialog ([`language_dialog`]):
    /// its first named, or another.
    AddLanguage,
    /// The page works in the draft's language `i`.
    Language(usize),
    /// The draft's language `i` off it (nothing being in it).
    RemoveLanguage(usize),
    /// Mixed offered or not.
    Mixed,
    /// Take `id`'s tags, chosen in a dialog ([`tags_dialog`]).
    Tags(u64),
    /// The draft's tag `i` ([`Draft::tags`]) made switchable or not.
    Switchable(usize),
    /// The draft's tag `i`'s switch's first state turned over.
    TagDefault(usize),
}

/// The press `id` of [`doc`]'s.
pub fn act(id: &str) -> Option<Act> {
    let take = |rest: &str| rest.parse::<u64>().ok();
    Some(match id {
        "import-files" => Act::ImportFiles,
        "import-folder" => Act::ImportFolder,
        "tidy" => Act::Tidy,
        "stadium" => Act::Stadium,
        "examples" => Act::Examples,
        "stepmania" => Act::StepMania,
        "details" => Act::Details,
        "export" => Act::Export,
        "save" => Act::Save,
        "use" => Act::UseInGame,
        "add-combo" => Act::AddCombo,
        "add-language" => Act::AddLanguage,
        "mixed" => Act::Mixed,
        _ => {
            let (kind, rest) = id.split_once(':')?;
            match kind {
                "add" => Act::Add(Line::parse(rest)?),
                "add-to" => Act::AddTo(take(rest)?),
                "write" => Act::Write(Line::parse(rest)?),
                "unwrite" => Act::Unwrite(take(rest)?),
                "hear" => Act::Hear(Line::parse(rest)?),
                "hear-take" => Act::HearTake(take(rest)?),
                "play" => Act::Play(take(rest)?),
                "move" => Act::Move(take(rest)?),
                "remove" => Act::Remove(take(rest)?),
                "remove-combo" => Act::RemoveCombo(rest.parse().ok()?),
                "language" => Act::Language(rest.parse().ok()?),
                "tags" => Act::Tags(take(rest)?),
                "switchable" => Act::Switchable(rest.parse().ok()?),
                "tag-default" => Act::TagDefault(rest.parse().ok()?),
                "remove-language" => Act::RemoveLanguage(rest.parse().ok()?),
                "example" | "hear-example" => {
                    let (folder, i) = rest.rsplit_once(':')?;
                    let (line, i) = (Line::parse(folder)?, i.parse().ok()?);
                    if kind == "example" {
                        Act::Example(line, i)
                    } else {
                        Act::HearExample(line, i)
                    }
                }
                _ => return None,
            }
        }
    })
}

/// The examples the studio shows where none were chosen: the UI's
/// language's (`docs/plans/localization.md`), else English.
fn first_set() -> Set {
    let tag = ddi_l10n::current().tag;
    Set::from_key(tag.split('-').next().unwrap_or(tag)).unwrap_or_default()
}

/// The example set remembered, or [`first_set`].
pub fn remembered_set() -> Set {
    storage::get(Area::Local, EXAMPLES_KEY)
        .and_then(|k| Set::from_key(&k))
        .unwrap_or_else(first_set)
}

/// The next example set after `set`.
fn next_set(set: Set) -> Set {
    let i = Set::ALL.iter().position(|s| *s == set).unwrap_or(0);
    Set::ALL[(i + 1) % Set::ALL.len()]
}

/// What [`apply`] changed, which the shell follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    /// The draft: the shell keeps it.
    Draft,
    /// The example set, remembered here: the shell shows its examples.
    Examples,
    /// Only how the page shows.
    View,
    /// What the draft keeps of how the page shows (the language worked
    /// in): the shell keeps it, nothing made of the draft changed.
    Kept,
}

/// The presses every shell carries out alike, on the draft and the
/// view; `None` for any other, the shell's to carry out.
pub fn apply(act: Act, draft: &mut Draft, view: &mut View) -> Option<Applied> {
    Some(match act {
        Act::Tidy => {
            draft.tidy = !draft.tidy;
            for t in &mut draft.takes {
                t.tidy = None;
            }
            Applied::Draft
        }
        Act::Stadium => {
            draft.stadium = !draft.stadium;
            Applied::Draft
        }
        Act::StepMania => {
            draft.stepmania = !draft.stepmania;
            Applied::Draft
        }
        Act::Examples => {
            view.examples = next_set(view.examples);
            let _ = storage::set(Area::Local, EXAMPLES_KEY, view.examples.key());
            Applied::Examples
        }
        Act::Details => {
            view.details = !view.details;
            Applied::View
        }
        Act::RemoveCombo(n) => {
            if !draft.remove_combo(n) {
                return None;
            }
            Applied::Draft
        }
        Act::Write(line) => {
            draft.write(line);
            Applied::Draft
        }
        Act::Unwrite(id) => {
            draft.unwrite(id)?;
            Applied::Draft
        }
        Act::Language(i) => {
            draft.working = Some(draft.languages.get(i)?.clone());
            Applied::Kept
        }
        Act::RemoveLanguage(i) => {
            let tag = draft.languages.get(i)?.clone();
            if !draft.remove_language(&tag) {
                return None;
            }
            Applied::Draft
        }
        Act::Mixed => {
            draft.mixed = !draft.mixed;
            Applied::Draft
        }
        Act::Switchable(i) => {
            let tag = draft.tags().into_iter().nth(i)?;
            draft.toggle_switch(&tag);
            Applied::Draft
        }
        Act::TagDefault(i) => {
            let tag = draft.tags().into_iter().nth(i)?;
            if !draft.toggle_default(&tag) {
                return None;
            }
            Applied::Draft
        }
        _ => return None,
    })
}

/// What a field's press says: the draft's field it is (a text line's
/// or a take's words, `words:<id>`).
pub fn set_field(draft: &mut Draft, id: &str, text: &str) -> bool {
    if let Some(tag) = id.strip_prefix("tag-label:") {
        let Some(s) = draft.switches.iter_mut().find(|s| s.tag == tag) else {
            return false;
        };
        s.label = text.to_string();
        return true;
    }
    let to = match id {
        "name" => &mut draft.name,
        "author" => &mut draft.author,
        "license" => &mut draft.license,
        _ => match id
            .strip_prefix("words:")
            .and_then(|i| i.parse().ok())
            .and_then(|i| draft.words_mut(i))
        {
            Some(words) => words,
            None => return false,
        },
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
    // In the page's order: song select's, play's, the results', the
    // battle's.
    let mut lines = draft.lines();
    lines.sort_by_key(|l| match l.spoken() {
        Spoken::Menu => 0,
        Spoken::Play => 1,
        Spoken::Results => 2,
        Spoken::Battle => 3,
    });
    let mut items: Vec<(String, String)> = lines
        .into_iter()
        .map(|l| {
            let label = match l.spoken() {
                Spoken::Menu => m::picker_line_select(name(l)),
                Spoken::Play => m::picker_line_play(name(l)),
                Spoken::Results => m::picker_line_results(name(l)),
                Spoken::Battle => m::picker_line_battle(name(l)),
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
    let pick: PickFn = Rc::new(move |id: &str, cx: &mut Cx| {
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
            | Act::Stadium
            | Act::Remove(_)
            | Act::Examples
            | Act::StepMania
            | Act::Details
            | Act::Export
            | Act::UseInGame
            | Act::RemoveCombo(_)
            | Act::Write(_)
            | Act::Unwrite(_)
            | Act::Language(_)
            | Act::RemoveLanguage(_)
            | Act::Mixed
            | Act::Switchable(_)
            | Act::TagDefault(_)
    )
}

/// What choosing a tag of a take does (`Some(tag)`; `None`: New tag…).
pub type TagFn = Rc<dyn Fn(Option<&str>, &mut Cx)>;

/// The dialog of a take's Tags…: the tags the draft uses, then those the
/// studio suggests, each marked where the take has it (a press puts it on
/// or takes it off), then New tag…
pub fn tags_dialog(env: Env, draft: &Draft, take: u64, on_pick: TagFn) -> Option<ActionsDialog> {
    let t = draft.take(take)?;
    let mut tags = draft.tags();
    for s in draft::SUGGESTED_TAGS {
        if !tags.iter().any(|t| t == s) {
            tags.push(s.to_string());
        }
    }
    let mut items: Vec<(String, String)> = tags
        .into_iter()
        .map(|tag| {
            let label = if t.tags.contains(&tag) {
                m::tag_on(&tag).into()
            } else {
                tag.clone()
            };
            (format!("tag:{tag}"), label)
        })
        .collect();
    items.push(("new".into(), m::tag_new().into()));
    let pick: PickFn = Rc::new(move |id: &str, cx: &mut Cx| {
        on_pick(id.strip_prefix("tag:"), cx);
    });
    Some(
        ActionsDialog::new(env, &m::tags_title(t.file_name()), items, pick)
            .with_note(&m::tags_note()),
    )
}

/// What a tag typed does: whether the take took it.
pub type NewTagFn = Rc<dyn Fn(&str, &mut Cx) -> bool>;

/// The dialog of New tag…: a word typed (letters a to z, digits, `-` and
/// `_`), put on the take.
pub fn new_tag_dialog(env: Env, on_add: NewTagFn) -> ValueDialog {
    let submit: SubmitFn = Rc::new(move |text: &str, cx: &mut Cx| {
        let tag = text.trim().to_ascii_lowercase();
        if !ddi_library::announcer::script::is_tag(&tag) || !on_add(&tag, cx) {
            return Err(m::tag_not_a_word().to_string());
        }
        Ok(())
    });
    ValueDialog::new(env, &m::tag_new_title(), &m::tag_new_note(), "", submit)
}

/// What choosing a language does: `true` where the draft took it.
pub type LanguageFn = Rc<dyn Fn(&str, &mut Cx) -> bool>;

/// The dialog of Add a language… (or Name its language…, where the draft
/// has none named): the languages the studio offers that the draft has
/// not, each named in itself.
pub fn language_dialog(env: Env, draft: &Draft, on_pick: LanguageFn) -> ActionsDialog {
    let naming = draft.languages.is_empty();
    let items: Vec<(String, String)> = draft::LANGUAGES
        .iter()
        .filter(|(tag, _)| !draft.languages.iter().any(|l| l == tag))
        .map(|(tag, name)| (tag.to_string(), name.to_string()))
        .collect();
    let pick: PickFn = Rc::new(move |tag: &str, cx: &mut Cx| {
        on_pick(tag, cx);
    });
    let (title, note) = if naming {
        (m::language_title_name(), m::language_note_name())
    } else {
        (m::language_title_add(), m::language_note_add())
    };
    ActionsDialog::new(env, &title, items, pick).with_note(&note)
}

/// What adding combo line `n` does: whether it was not listed already.
pub type ComboFn = Rc<dyn Fn(u32, &mut Cx) -> bool>;

/// A count typed: a whole number from 1 (spaces and digit grouping
/// left out: "1,024").
pub fn combo_count(text: &str) -> Option<u32> {
    let digits: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, ',' | '.' | '_'))
        .collect();
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok().filter(|&n| n >= 1)
}

/// The dialog of Add a combo line…: a count typed, a whole number from
/// 1, not listed already.
pub fn combo_dialog(env: Env, on_add: ComboFn) -> ValueDialog {
    let submit: SubmitFn = Rc::new(move |text: &str, cx: &mut Cx| {
        let n = combo_count(text).ok_or_else(|| m::combo_not_a_count().to_string())?;
        if on_add(n, cx) {
            Ok(())
        } else {
            Err(m::combo_listed(n).to_string())
        }
    });
    ValueDialog::new(env, &m::combo_title(), &m::combo_note(), "", submit).numeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_library::announcer::draft::listed;

    const MP3: &[u8] = b"ID3\x04\x00";

    /// Take `name` added to `d`, read as a tenth of a second.
    fn add_read(d: &mut Draft, name: &str) -> u64 {
        let id = d.add(name, 1, MP3);
        d.take_mut(id).unwrap().sound = Sound::Read(ddi_library::announcer::tidy::measure(
            &[vec![0.0; 4800]],
            48000,
        ));
        id
    }

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

    /// A text line is written on its line, recorded in its place and
    /// back when its recording goes.
    #[test]
    fn text_lines_are_written_recorded_and_removed() {
        ddi_l10n::set_language("en");
        let mut d = Draft::default();
        let mut view = View::default();
        let line = Line::Combo(64);
        assert!(d.add_combo(64));
        let press = |d: &mut Draft, view: &mut View, id: &str| apply(act(id).expect(id), d, view);
        assert_eq!(
            press(&mut d, &mut view, "write:gameplay 64 combo"),
            Some(Applied::Draft)
        );
        let id = d.scripts[0].id;
        assert!(set_field(&mut d, &format!("words:{id}"), "Sixty-four!"));
        assert!(!set_field(&mut d, "words:999", "Nothing"));
        let blocks = |d: &Draft| {
            let ex = BTreeMap::new();
            format!("{:?}", line_zone(d, &View::default(), line, &ex))
        };
        let text = blocks(&d);
        assert!(text.contains(&format!(
            "id: \"words:{id}\", label: \"\", value: \"Sixty-four!\""
        )));
        assert!(text.contains(&format!("\"add-to:{id}\"")));
        assert!(text.contains(&format!("\"unwrite:{id}\"")));
        // Not taken off while it is written for.
        assert!(!text.contains("remove-combo:64"));
        assert_eq!(act(&format!("add-to:{id}")), Some(Act::AddTo(id)));
        assert!(in_gesture(Act::AddTo(id)));
        assert!(!in_gesture(Act::Write(line)) && !in_gesture(Act::Unwrite(id)));
        // Recorded (as the shells do), its words above the recording.
        let take = d.add_to(id, "64.mp3", 1, MP3);
        let text = blocks(&d);
        assert!(text.contains(&format!(
            "id: \"words:{take}\", label: \"\", value: \"Sixty-four!\""
        )));
        assert!(!text.contains("\"add-to:"));
        assert!(set_field(
            &mut d,
            &format!("words:{take}"),
            "Sixty-four combo!"
        ));
        // Its recording removed, the line is back to write for.
        d.remove(take);
        let back = d.scripts[0].id;
        assert_eq!(d.scripts[0].words, "Sixty-four combo!");
        assert_eq!(
            press(&mut d, &mut view, &format!("unwrite:{back}")),
            Some(Applied::Draft)
        );
        assert_eq!(press(&mut d, &mut view, &format!("unwrite:{back}")), None);
        assert!(blocks(&d).contains("remove-combo:64"));
    }

    /// The combo lines are the author's: ours to begin with, any count
    /// added, any without a recording removed.
    #[test]
    fn combo_lines_are_added_and_removed() {
        let combos = |d: &Draft| -> Vec<String> {
            zones(&doc(
                d,
                &View::default(),
                &BTreeMap::new(),
                Default::default(),
            ))
            .into_iter()
            .filter(|z| z.ends_with(" combo") && z != "gameplay full combo")
            .collect()
        };
        let mut d = Draft::default();
        assert_eq!(combos(&d).len(), OUR_COMBOS.len());
        let text = format!(
            "{:?}",
            doc(&d, &View::default(), &BTreeMap::new(), Default::default()).blocks
        );
        // Add a combo line… after them, before More combo.
        let add = text.find("\"add-combo\"").unwrap();
        assert!(text.find("\"gameplay 2000 combo\"").unwrap() < add);
        assert!(add < text.find("id: \"gameplay combo overflow\"").unwrap());
        assert!(text.contains("\"remove-combo:100\""));
        assert!(text.contains("\"hear:gameplay 100 combo\""));
        assert_eq!(act("add-combo"), Some(Act::AddCombo));
        assert_eq!(act("remove-combo:64"), Some(Act::RemoveCombo(64)));
        assert_eq!(act("remove-combo:x"), None);
        assert!(in_gesture(Act::AddCombo) && !in_gesture(Act::RemoveCombo(1)));
        // Counted by powers of two.
        let mut v = View::default();
        for n in OUR_COMBOS {
            assert_eq!(
                apply(Act::RemoveCombo(n), &mut d, &mut v),
                Some(Applied::Draft)
            );
        }
        assert_eq!(apply(Act::RemoveCombo(100), &mut d, &mut v), None);
        for t in ["64", " 128 ", "1,024"] {
            assert!(d.add_combo(combo_count(t).unwrap()));
        }
        for not in ["", "0", "-5", "1.5e3", "abc", "99999999999"] {
            assert_eq!(combo_count(not), None, "{not}");
        }
        assert_eq!(
            combos(&d),
            [
                "gameplay 64 combo",
                "gameplay 128 combo",
                "gameplay 1024 combo"
            ]
        );
        // A count of its own has no example to hear until recorded, and
        // is not removed while recorded.
        let text = format!(
            "{:?}",
            doc(&d, &View::default(), &BTreeMap::new(), Default::default()).blocks
        );
        assert!(!text.contains("\"hear:gameplay 64 combo\""));
        let id = add_read(&mut d, "gameplay 64 combo.wav");
        let text = format!(
            "{:?}",
            doc(&d, &View::default(), &BTreeMap::new(), Default::default()).blocks
        );
        assert!(text.contains(&format!("\"hear-take:{id}\"")));
        assert!(!text.contains("\"remove-combo:64\""));
        assert!(text.contains("\"remove-combo:128\""));
        assert_eq!(apply(Act::RemoveCombo(64), &mut d, &mut v), None);
        assert_eq!(when(Line::Combo(1024)).to_string(), "1,024 notes in a row.");
        assert_eq!(name(Line::Combo(64)).to_string(), "64 combo");
    }

    /// Use in the game and Export are always on the page, grayed out while
    /// there is nothing to use or export.
    #[test]
    fn use_and_export_are_grayed_out_with_nothing_to_give() {
        use crate::screens::page::{Action, Block};
        let actions = |d: &Draft, v: &View| -> Vec<Action> {
            doc(d, v, &BTreeMap::new(), Default::default())
                .blocks
                .iter()
                .find_map(|b| match b {
                    Block::Actions(a) if a.iter().any(|a| a.id == "examples") => Some(a.clone()),
                    _ => None,
                })
                .unwrap()
        };
        let off = |a: &[Action], id: &str| a.iter().find(|a| a.id == id).unwrap().off;
        let mut d = Draft::default();
        let v = View::default();
        let a = actions(&d, &v);
        assert!(off(&a, "use") && off(&a, "export"));
        // A recording not on a line yet is still nothing to give.
        let id = d.add("New Recording.mp3", 1, MP3);
        let a = actions(&d, &v);
        assert!(off(&a, "use") && off(&a, "export"));
        d.take_mut(id).unwrap().line = Some(Line::Ready);
        let a = actions(&d, &v);
        assert!(!off(&a, "use") && !off(&a, "export"));
        // While the studio is at work.
        let busy = View {
            busy: true,
            ..View::default()
        };
        let a = actions(&d, &busy);
        assert!(off(&a, "use") && off(&a, "export"));
        // Made: Save takes Export's place, and is on.
        let ready = View {
            ready: Some("Voice.zip".into()),
            ..View::default()
        };
        let a = actions(&d, &ready);
        assert!(!off(&a, "use") && !off(&a, "save"));
    }

    #[test]
    fn song_select_and_results_lines_can_be_heard_recorded_or_not() {
        let mut d = Draft::default();
        let aaaa = add_read(&mut d, "evaluation aaaa.wav");
        let intro = d.add("select music intro.wav", 2, MP3);
        let ready = add_read(&mut d, "gameplay ready.wav");
        for line in [
            Line::Grade(Grade::Aaaa),
            Line::Grade(Grade::Aaa),
            Line::SelectMusicIntro,
            Line::SelectMusic(Comment::New),
        ] {
            assert!(hearable(line), "{line:?}");
        }
        // No play gets it; the play's lines have scenes of their own.
        assert!(!hearable(Line::Grade(Grade::Aaaaa)));
        assert!(hearable(Line::Ready));
        assert!(recorded(Line::Grade(Grade::Aaaa), &d));
        assert!(!recorded(Line::Grade(Grade::Aaa), &d));
        // What speaks in its place counts.
        assert!(!recorded(Line::Grade(Grade::F), &d));
        d.add("evaluation e.wav", 4, MP3);
        assert!(recorded(Line::Grade(Grade::F), &d));
        let aaa = Line::Grade(Grade::Aaa);
        let ours = BTreeMap::from([(
            aaa,
            vec![
                Example {
                    text: "Triple A!".into(),
                    sound: Some("evaluation aaa/1.ogg".into()),
                },
                Example {
                    text: "Wonderful.".into(),
                    sound: None,
                },
            ],
        )]);
        add_read(&mut d, &format!("{}.wav", Line::FullCombo.folder()));
        let page = doc(&d, &View::default(), &ours, Default::default());
        let text = format!("{:?}", page.blocks);
        // Each recording and each example our announcer says has its own
        // button; the line has one only where neither is there and
        // something speaks in its place.
        let eye = |id: &str| {
            format!("\"{id}\", label: \"◉\", primary: false, off: false, icon: Some(Eye)")
        };
        assert!(text.contains(&eye(&format!("hear-take:{aaaa}"))));
        assert!(text.contains(&format!("\"hear-take:{ready}\"")));
        assert!(!text.contains("\"hear:evaluation aaaa\""));
        assert!(!text.contains("\"hear:gameplay ready\""));
        assert!(text.contains(&eye("hear-example:evaluation aaa:0")));
        assert!(!text.contains("\"hear-example:evaluation aaa:1\""));
        assert!(!text.contains("\"hear:evaluation aaa\""));
        assert!(text.contains(&eye(&format!("hear:{}", Line::FullComboW(1).folder()))));
        assert!(text.contains(&eye("hear:evaluation aa")));
        assert!(!text.contains("\"hear:evaluation aaaaa\""));
        // Not read yet: no scene to hear it in.
        assert!(!text.contains("\"hear:select music intro\""));
        assert!(!text.contains(&format!("\"hear-take:{intro}\"")));
        // The recordings beside the examples, in the line's zone.
        let zone = page
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Zone { id, body } if id == "evaluation aaa" => Some(body),
                _ => None,
            })
            .unwrap();
        let Some(Block::Columns { left, right }) = zone.last() else {
            panic!("{zone:?}");
        };
        assert!(format!("{left:?}").contains("\"add:evaluation aaa\""));
        // Each example alike: its words, and its buttons where it has a
        // sound.
        let entries: Vec<(&str, Vec<&str>)> = right
            .iter()
            .filter_map(|b| match b {
                Block::Entry { text, actions, .. } => Some((
                    text.as_str(),
                    actions.iter().map(|a| a.id.as_str()).collect(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            entries,
            [
                (
                    "“Triple A!”",
                    vec!["example:evaluation aaa:0", "hear-example:evaluation aaa:0"]
                ),
                ("“Wonderful.”", vec![]),
            ]
        );
        assert_eq!(act("hear:evaluation cheer"), Some(Act::Hear(Line::Cheer)));
        assert_eq!(act(&format!("hear-take:{aaaa}")), Some(Act::HearTake(aaaa)));
        assert_eq!(
            act("hear-example:evaluation aaa:0"),
            Some(Act::HearExample(aaa, 0))
        );
        assert_eq!(act("example:evaluation aaa:0"), Some(Act::Example(aaa, 0)));
        // A scene in play says more besides its line: that one is named.
        let restart = [Line::Ready, Line::HereWeGo, Line::Restart, Line::HereWeGo];
        let text = heard(Line::Restart, &restart, None).to_string();
        assert!(text.contains(&name(Line::Restart).to_string()), "{text}");
        assert!(text.contains('3'), "{text}");
        assert!(in_gesture(Act::Hear(Line::Cheer)));
    }

    #[test]
    fn the_battle_lines_are_their_own_section_without_scenes() {
        let mut d = Draft::default();
        d.add("battle stage won.wav", 1, MP3);
        let page = doc(&d, &View::default(), &BTreeMap::new(), Default::default());
        let text = format!("{:?}", page.blocks);
        // The section comes last, its note under its heading.
        let heading = format!("Heading({:?})", m::section_battle().to_string());
        let note = format!("Muted({:?})", m::section_battle_note().to_string());
        let at = text.find(&heading).expect("the battle's section");
        assert!(
            text[at..].starts_with(&format!("{heading}, {note}")),
            "{text}"
        );
        assert!(
            at > text
                .find(&format!("{:?}", m::section_results().to_string()))
                .unwrap()
        );
        for b in Battle::ALL {
            let line = Line::Battle(b);
            assert!(zones(&page).contains(&line.folder()), "{b:?}");
            // Recorded or not, there is no scene to hear it in yet.
            assert!(!hearable(line), "{b:?}");
            assert!(
                !text.contains(&format!("\"hear:{}\"", line.folder())),
                "{b:?}"
            );
        }
        assert!(recorded(Line::Battle(Battle::StageWon), &d));
        // Unrecorded: shown on screen, or what speaks in its place.
        assert_eq!(
            without(Line::Battle(Battle::StageNotWon), &d).to_string(),
            m::missing_shown().to_string()
        );
        // An attack's two and a step's two fall back on nothing, whatever
        // the draft has.
        for b in [
            Battle::AttackIncoming,
            Battle::AttackLaunched,
            Battle::StepUp,
            Battle::StepDown,
        ] {
            assert_eq!(
                without(Line::Battle(b), &d).to_string(),
                m::missing_shown().to_string()
            );
            assert!(!name(Line::Battle(b)).to_string().is_empty());
            assert!(!when(Line::Battle(b)).to_string().is_empty());
        }
        let mut ready = Draft::default();
        ready.add("gameplay ready.wav", 1, MP3);
        assert_eq!(
            without(Line::Battle(Battle::StageBegin), &ready).to_string(),
            m::missing_instead(name(Line::Ready)).to_string()
        );
        // The picker lists them after the results', marked multiplayer.
        let p = picker(&d, 1).expect("a take");
        let last = p.items.last().unwrap();
        assert_eq!(last.0, "battle match drawn");
        let label = m::picker_line_battle(name(Line::Battle(Battle::MatchDrawn)));
        assert_eq!(last.1, label.to_string());
    }

    /// The draft's languages on the page: named, added, worked in one
    /// at a time, each line saying which have it, Mixed where there are
    /// two, a language with nothing in it taken off.
    #[test]
    fn languages_are_named_added_worked_in_and_removed() {
        ddi_l10n::set_language("en");
        let page = |d: &Draft| format!("{:?}", doc(d, &View::default(), &BTreeMap::new(), Default::default()).blocks);
        let mut d = Draft::default();
        let mut v = View::default();
        let ready = add_read(&mut d, "gameplay ready.wav");
        let text = page(&d);
        assert!(text.contains(&m::languages_none().to_string()));
        assert!(text.contains("\"add-language\", label: \"Name its language…\""));
        assert!(!text.contains("\"mixed\"") && !text.contains("Recorded in"));
        assert_eq!(act("add-language"), Some(Act::AddLanguage));
        assert!(in_gesture(Act::AddLanguage));
        let names = |d: &Draft| -> Vec<String> {
            language_dialog(crate::screens::test_kit::env(), d, Rc::new(|_, _| true))
                .items()
                .iter()
                .map(|(t, _)| t.clone())
                .collect()
        };
        assert_eq!(names(&d)[..2], ["en".to_string(), "ja".to_string()]);
        assert!(d.add_language("en"));
        assert!(page(&d).contains("Its recordings are in English."));
        assert!(!names(&d).contains(&"en".to_string()));
        assert!(d.add_language("ja"));
        let text = page(&d);
        assert!(text.contains("\"language:0\", label: \"English\", primary: false"));
        assert!(text.contains("\"language:1\", label: \"日本語\", primary: true"));
        // Nothing in Japanese yet: it can go.
        assert!(text.contains("\"remove-language:1\", label: \"Remove 日本語\""));
        assert!(text.contains("Mixed languages: off"));
        assert!(text.contains("Recorded in English."));
        // Its take is not shown in Japanese.
        assert!(!text.contains(&format!("\"play:{ready}\"")));
        assert_eq!(apply(Act::Language(0), &mut d, &mut v), Some(Applied::Kept));
        assert_eq!(d.working_language(), Some("en"));
        let text = page(&d);
        assert!(text.contains(&format!("\"play:{ready}\"")));
        assert!(!text.contains("remove-language:0"));
        assert_eq!(apply(Act::Mixed, &mut d, &mut v), Some(Applied::Draft));
        assert!(page(&d).contains("Mixed languages: on"));
        assert_eq!(apply(Act::RemoveLanguage(0), &mut d, &mut v), None);
        assert_eq!(apply(Act::Language(7), &mut d, &mut v), None);
        assert_eq!(apply(Act::RemoveLanguage(1), &mut d, &mut v), Some(Applied::Draft));
        assert_eq!(d.languages, ["en"]);
        assert!(!d.mixed);
        for (id, a) in [("language:1", Act::Language(1)), ("remove-language:0", Act::RemoveLanguage(0)), ("mixed", Act::Mixed)] {
            assert_eq!(act(id), Some(a));
            assert!(!in_gesture(a));
        }
    }

    /// Tags on the page: a take's own and its button, the tags' fold with
    /// each tag switchable, its name in Options and first state, and the
    /// lines it would leave silent; the dialog marking the take's tags
    /// before the suggested ones.
    #[test]
    fn tags_are_put_on_takes_and_made_switchable() {
        ddi_l10n::set_language("en");
        let page = |d: &Draft| format!("{:?}", doc(d, &View::default(), &BTreeMap::new(), Default::default()).blocks);
        let mut d = Draft::default();
        let mut v = View::default();
        let restart = add_read(&mut d, "gameplay restart.wav");
        add_read(&mut d, "gameplay ready.wav");
        let text = page(&d);
        assert!(text.contains(&format!("\"tags:{restart}\", label: \"Tags…\"")));
        assert!(!text.contains("id: \"tags\""));
        assert_eq!(act(&format!("tags:{restart}")), Some(Act::Tags(restart)));
        assert!(in_gesture(Act::Tags(restart)));
        let items = |d: &Draft| -> Vec<(String, String)> {
            tags_dialog(crate::screens::test_kit::env(), d, restart, Rc::new(|_, _| {}))
                .unwrap()
                .items()
                .to_vec()
        };
        assert_eq!(items(&d)[0], ("tag:calm".to_string(), "calm".to_string()));
        assert_eq!(items(&d).last().unwrap().0, "new");
        d.toggle_tag(restart, "rude");
        // The take's own first, marked; the suggestions after.
        assert_eq!(items(&d)[0], ("tag:rude".to_string(), "● rude".to_string()));
        assert_eq!(items(&d)[1].0, "tag:calm");
        let text = page(&d);
        assert!(text.contains("Muted(\"Tags: rude\")"));
        assert!(text.contains("id: \"tags\", label: \"Tags (1)\""));
        assert!(text.contains("On one recording."));
        assert!(text.contains("\"switchable:0\", label: \"Players can switch it off: no\""));
        assert!(!text.contains("tag-label:rude"));
        assert_eq!(apply(Act::Switchable(0), &mut d, &mut v), Some(Applied::Draft));
        let text = page(&d);
        assert!(text.contains("Players can switch it off: yes"));
        assert!(text.contains("id: \"tag-label:rude\", label: \"In Options\", value: \"rude\""));
        assert!(text.contains("At first: on"));
        assert!(text.contains("Switched off, “Restart” has no recording left: “Ready” is said in its place."));
        assert!(set_field(&mut d, "tag-label:rude", "Rude words"));
        assert!(!set_field(&mut d, "tag-label:calm", "Calm"));
        assert_eq!(d.switches[0].label, "Rude words");
        assert_eq!(apply(Act::TagDefault(0), &mut d, &mut v), Some(Applied::Draft));
        assert!(!d.switches[0].default);
        assert_eq!(apply(Act::TagDefault(3), &mut d, &mut v), None);
        for (id, a) in [("switchable:2", Act::Switchable(2)), ("tag-default:1", Act::TagDefault(1))] {
            assert_eq!(act(id), Some(a));
            assert!(!in_gesture(a));
        }
        // In one of two languages, it says which.
        d.add_language("en");
        d.add_language("ja");
        d.working = Some("en".into());
        assert!(page(&d).contains("Switched off, “Restart” has no recording left in English: “Ready” is said in its place."));
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
        let empty = doc(&d, &View::default(), &BTreeMap::new(), Default::default());
        let z = zones(&empty);
        assert_eq!(z.len(), listed().len() + OUR_COMBOS.len());
        assert_eq!(z[0], "select music intro");
        d.add("New Recording.m4a", 1, b"");
        d.add("gameplay 1100 combo.wav", 1, MP3);
        let full = doc(&d, &View::default(), &BTreeMap::new(), Default::default());
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
            for inner in b.inner() {
                out.extend(flat(std::slice::from_ref(inner)));
            }
        }
        out
    }

    #[test]
    fn stadium_toggle_is_saved_as_a_draft_change() {
        let mut draft = Draft::default();
        let mut view = View::default();
        assert_eq!(act("stadium"), Some(Act::Stadium));
        assert!(!in_gesture(Act::Stadium));
        assert_eq!(
            apply(Act::Stadium, &mut draft, &mut view),
            Some(Applied::Draft)
        );
        assert!(draft.stadium);
        let text = format!(
            "{:?}",
            doc(&draft, &view, &BTreeMap::new(), Default::default()).blocks
        );
        assert!(text.contains("Stadium effect: on"));
        apply(Act::Stadium, &mut draft, &mut view);
        assert!(!draft.stadium);
    }

    #[test]
    fn credits_imports_and_switches_are_under_advanced() {
        let d = Draft::default();
        let page = doc(&d, &View::default(), &BTreeMap::new(), Default::default());
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
            "stadium",
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
            flat(&doc(&d, &view, &BTreeMap::new(), Default::default()).blocks)
                .iter()
                .any(|b| matches!(b, Block::Muted(s) if s.starts_with("Tid")))
        };
        assert!(!says(false) && says(true));
        assert!(
            !format!(
                "{:?}",
                doc(&d, &View::default(), &BTreeMap::new(), Default::default())
            )
            .contains("tidy:")
        );
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
        let text = format!("{:?}", doc(&d, &view, &ex, Default::default()).blocks);
        for s in [
            "ready.mp3 · 2.5 s",
            "icon: Some(Stop)",
            "value: \"Get ready!\", hint: \"What it says\"",
            "value: \"So hot!\"",
            "Add a text line",
            "Tidied: 0.5 s of silence cut, louder by 6 dB",
            "Tidied: quieter by 6 dB",
            "Tidied: 0.3 s of silence cut",
            "Tidy: nothing to change",
            "As recorded",
            "double a.m4a · cannot be played",
            "intro.mp3 · reading…",
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
            "“One hundred combo!”",
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
    fn the_shells_switches_change_the_draft_or_the_view() {
        let mut draft = Draft::default();
        let id = draft.add("a.wav", 1, &[]);
        draft.take_mut(id).unwrap().tidy = Some(false);
        let was = draft.tidy;
        let mut view = View::default();
        assert_eq!(
            apply(Act::Tidy, &mut draft, &mut view),
            Some(Applied::Draft)
        );
        assert_eq!(draft.tidy, !was);
        assert_eq!(draft.take(id).unwrap().tidy, None);
        assert_eq!(
            apply(Act::Details, &mut draft, &mut view),
            Some(Applied::View)
        );
        assert!(view.details);
        assert_eq!(
            apply(Act::StepMania, &mut draft, &mut view),
            Some(Applied::Draft)
        );
        assert!(draft.stepmania);
        let set = view.examples;
        assert_eq!(
            apply(Act::Examples, &mut draft, &mut view),
            Some(Applied::Examples)
        );
        assert_eq!(view.examples, next_set(set));
        assert_eq!(remembered_set(), view.examples);
        assert_eq!(apply(Act::Export, &mut draft, &mut view), None);
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
