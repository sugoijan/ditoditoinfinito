//! Every text the screens show passes through `ddi_l10n`
//! (`docs/plans/localization.md`). In the pseudo-language `qps-ploc` a
//! message reads «…» and data ‹…›; any other text with a letter in it is
//! written in code, and fails (make it a message, in every language), as
//! does a run joining two messages into a sentence (JOINED). Each run of
//! digits reads `#` in the failure.
//!
//! Walked: the start menu, song select, the start prompt at each step
//! but a card (whose lines are its caller's), the results,
//! Options' rows and confirmations, the credits, calibration's menu, the
//! folders offer, the import and manage dialogs, the number forms, the
//! announcer studio and its scenes' frame, the chart editor with its
//! menus and dialogs, on both shells' wording and in both layouts.

use std::collections::BTreeSet;
use std::rc::Rc;

use ddi_platform::storage::{self, MemoryStore};

use super::calibration::{RunEnd, run_line};
use super::credits;
use super::data::{PackEntry, Platform};
use super::folders_offer::FoldersOfferDialog;
use super::numbers::{Format as NumberFormat, Number, NumberDialog, Numbers};
use super::options_model::{OptionsModel, Reset, Sample, World};
use super::page::Block;
use super::prompt::PromptStep;
use super::select_dialogs::{ImportDialog, Manage, ManageDialog};
use super::{COMPACT, DESIGN, Env, sample};
use crate::dev_widgets::FoldFn;
use crate::look::Plain;
use crate::message::{Message, Param};
use crate::state::settings::Settings;
use crate::testing::Harness;
use crate::{Modifiers, PointerKind, TextMode};

fn env() -> Env {
    let fold: FoldFn =
        Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s));
    Env::new(Rc::new(Plain), fold)
}

/// Sets the hidden option that opens a room's Rules by themselves on
/// entering (`Settings::multiplayer_rules_first`), so the walk shows
/// their first form too.
fn rules_first(on: bool) {
    let mut settings = crate::state::settings::Settings::load();
    settings.multiplayer_rules_first = on;
    settings.save();
}

/// Text written in code: not a message, not data, with a letter in it
/// (numbers and signs are no one's language).
fn in_code(text: &str) -> bool {
    let t = text.trim();
    !(t.is_empty()
        || t.starts_with('«')
        || t.ends_with('»')
        || (t.starts_with('‹') && t.ends_with('›'))
        // Data cut to fit (a name in a narrow column) is data still.
        || (t.starts_with('‹') && t.ends_with('…'))
        || !t.chars().any(char::is_alphabetic))
}

/// The list's form of a text: one line, each run of digits `#` (the
/// offsets' hundred values are one text).
fn line(text: &str) -> String {
    let mut out = String::new();
    let mut digits = false;
    for c in text.trim().chars() {
        match c {
            '0'..='9' if digits => {}
            '0'..='9' => {
                digits = true;
                out.push('#');
            }
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
        if !c.is_ascii_digit() {
            digits = false;
        }
    }
    out
}

/// A run joining two messages: a sentence built in code, which another
/// language cannot reorder or agree. A list of tags (the mods line's
/// "×2 · «reverse» · «mirror»", a chart's "Hard 13 · «Single»") is fine:
/// messages with " · " alone between them. A message holding others in
/// its variables (a lane's name in "«KeyF moved from «Down» to «Left».»")
/// is one message: its translation places them.
fn joined(text: &str) -> bool {
    let mut depth = 0usize;
    // What stands since the last whole message, once there was one.
    let mut between: Option<String> = None;
    for c in text.chars() {
        match c {
            '«' => {
                if depth == 0 && between.as_deref().is_some_and(|b| !list_separated(b)) {
                    return true;
                }
                depth += 1;
            }
            '»' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    between = Some(String::new());
                }
            }
            c if depth == 0 => {
                if let Some(b) = &mut between {
                    b.push(c);
                }
            }
            _ => {}
        }
    }
    false
}

/// What may stand between two messages of a list of tags: " · ", data
/// between two such (a speed, a count), never words.
fn list_separated(between: &str) -> bool {
    between.split(" · ").count() > 1
        && between.split(" · ").all(|part| {
            part.is_empty() || !part.chars().any(char::is_alphabetic) || part.starts_with('‹')
        })
}

#[test]
fn a_list_of_tags_is_not_a_sentence() {
    assert!(!joined("«Hard» 13"));
    assert!(!joined("×2 · «reverse» · «mirror»"));
    assert!(!joined("‹Hard 13› · «Single» · «AUTOPLAY»"));
    assert!(!joined("«a» · 3 · «b»"));
    assert!(joined("«Off» «On»"));
    assert!(joined("«Remove» the pack «Songs»?"));
    assert!(joined("«a» · and · «b»"));
    // Messages in a message's variables.
    assert!(!joined("«KeyF moved from «Down~» to «Left~».~~»"));
    assert!(!joined("«‹KeyF› «Down» · «Left»» · «b»"));
    assert!(joined("«a «x» b» «c»"));
}

/// Whether a message is still open after `text`, the next of the runs
/// drawn in order: a message wrapped over several runs (a dialog's text,
/// a line each) opens with « on its first and closes with » on its last;
/// the lines between have neither.
fn still_open(open: bool, text: &str) -> bool {
    match (text.rfind('«'), text.rfind('»')) {
        (Some(a), Some(b)) => a > b,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => open,
    }
}

#[test]
fn a_wrapped_message_s_lines_are_the_message_s() {
    let lines = ["«Šóɱé ļóñĝ", "ţéхţ ŵŕáppéď", "ó'éŕ ļíñéš~~»", "Then code"];
    let mut open = false;
    let inside: Vec<bool> = lines
        .iter()
        .map(|t| {
            let was = open;
            open = still_open(open, t);
            was
        })
        .collect();
    assert_eq!(inside, [false, true, true, false]);
    assert!(!still_open(false, "«a» and «b»"));
    assert!(still_open(false, "‹data› · «a long"));
}

/// The texts of a page's blocks, folds unfolded; not a field's value
/// (what the player typed).
fn doc_texts(blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            Block::Heading(s)
            | Block::Subheading(s)
            | Block::Text(s)
            | Block::Muted(s)
            | Block::Mono(s)
            | Block::Note(s) => out.push(s.clone()),
            Block::Link { label, .. }
            | Block::Button { label, .. }
            | Block::Action { label, .. } => out.push(label.clone()),
            Block::Fold { label, body, .. } => {
                out.push(label.clone());
                doc_texts(body, out);
            }
            Block::Zone { .. } | Block::Columns { .. } => {
                for inner in b.inner() {
                    doc_texts(std::slice::from_ref(inner), out);
                }
            }
            Block::Actions(actions) => out.extend(actions.iter().map(|a| a.label.clone())),
            Block::Field { label, hint, .. } => out.extend([label.clone(), hint.clone()]),
            Block::Entry {
                text,
                detail,
                actions,
            } => {
                out.push(text.clone());
                out.extend(detail.clone());
                out.extend(actions.iter().map(|a| a.label.clone()));
            }
            Block::Gap => {}
        }
    }
}

/// Every text the announcer studio shows but its page's own chrome:
/// the page in each state of the sample (folds unfolded), the line
/// picker, each line's name and moment, what it says after a scene, the
/// scenes' captions.
fn studio_texts() -> Vec<String> {
    use super::stage::{Caption, CheerAlso};
    use super::studio::{self, View};
    use ddi_library::announcer::draft::Draft;
    use ddi_library::announcer::{Comment, Line};
    let (draft, view, examples) = sample::studio();
    let mut out = Vec::new();
    let mut other = draft.clone();
    other.tidy = false;
    other.stepmania = true;
    for (d, v, platform) in [
        (&draft, view.clone(), Platform::Web),
        (&other, View::default(), Platform::Web),
        (
            &draft,
            View {
                ready: None,
                ..view.clone()
            },
            Platform::Web,
        ),
        (&draft, view.clone(), Platform::Desktop),
    ] {
        doc_texts(&studio::doc(d, &v, &examples, platform).blocks, &mut out);
    }
    for t in &draft.takes {
        let p = studio::picker(&draft, t.id).expect("a take");
        out.push(p.title.into());
        out.extend(p.items.into_iter().map(|(_, label)| label));
        out.push(p.note.into());
    }
    let empty = Draft::default();
    // A pack's own counts too, and what the combo dialog says.
    out.push(ddi_l10n::msg::studio::combo_not_a_count().into());
    out.push(ddi_l10n::msg::studio::combo_listed(64).into());
    for line in Line::all()
        .into_iter()
        .chain([64, 1024, 1500].map(Line::Combo))
    {
        out.push(studio::name(line).into());
        out.push(studio::when(line).into());
        out.push(studio::without(line, &empty).into());
        out.push(studio::without(line, &draft).into());
        // Two lines heard are one message with two names in it, which
        // the walk cannot tell from two messages put together.
        out.push(studio::heard(line, &[line], None).into());
        out.push(studio::heard(line, &[line], Some("Mr. Clank")).into());
        out.push(studio::heard(line, &[], None).into());
    }
    for c in [
        Caption::Start,
        Caption::SongSelect,
        Caption::Chosen(Comment::New),
        Caption::Chosen(Comment::Hard),
        Caption::Chosen(Comment::Repeat),
        Caption::Chosen(Comment::General),
        Caption::Quit,
        Caption::FirstClear,
        Caption::NewRecord {
            score: "92.40%".into(),
        },
        Caption::FullCombo {
            judgment: "Fantastic".into(),
            best: true,
        },
        Caption::FullCombo {
            judgment: "Excellent".into(),
            best: false,
        },
        Caption::Failed,
        Caption::Graded { grade: "S".into() },
        Caption::Cheer {
            grade: "AA".into(),
            also: CheerAlso::Nothing,
        },
        Caption::Cheer {
            grade: "AA".into(),
            also: CheerAlso::NewRecord,
        },
        Caption::Cheer {
            grade: "AA".into(),
            also: CheerAlso::FirstClear,
        },
    ] {
        out.push(c.text().into());
    }
    out
}

/// The frame a studio's scene is shown in, over the start menu, a line's
/// words said over it.
fn stage_frame(h: &mut Harness, env: &Env, words: ddi_l10n::UiText) {
    use super::stage::{Caption, SAY, StageFrame};
    use crate::message::{Message, Param};
    use ddi_l10n::msg::studio as ms;
    use ddi_library::announcer::{Comment, Line};
    h.push(sample::start_screen(env, false));
    let line = Line::SelectMusic(Comment::Hard);
    h.ui.push_chrome(StageFrame::new(
        env.clone(),
        &super::studio::name(line),
        &ms::stage_voice_example("Mr. Clank"),
        &Caption::Chosen(Comment::Hard).text(),
        Rc::new(|_| {}),
    ));
    h.run(0.5);
    h.ui.broadcast(
        Message::new(SAY)
            .with("Text", Param::Str(words.into()))
            .with("Seconds", Param::Num(2.0)),
    );
}

fn shown(h: &mut Harness, found: &mut BTreeSet<String>) {
    h.run(0.5);
    checked(h.texts(), found);
}

/// [`shown`]'s check of texts drawn in order (or each whole, unwrapped).
fn checked(texts: Vec<String>, found: &mut BTreeSet<String>) {
    let mut open = false;
    for t in texts {
        // A line inside a wrapped message is the message's.
        let inside = open;
        open = still_open(open, &t);
        if !inside && in_code(&t) {
            found.insert(line(&t));
        }
        if joined(&t) {
            found.insert(format!("JOINED {}", line(&t)));
        }
    }
}

/// Song select's states beyond its first (typing a search, what it
/// found, a share's songs) and its dialogs in each of their steps.
fn select_walk(env: &Env, harness: &dyn Fn() -> Harness, found: &mut BTreeSet<String>) {
    use super::data::{
        DanoniImportView, DanoniOriginRequest, ImportReportView, ImportRunning, ImportView, Notice,
        PlayRow, ShareState, ShareView, SharedView,
    };
    use super::select_dialogs::{NoticesDialog, PlaysDialog, ShareDialog};
    use crate::PointerKind;
    use ddi_library::report::{ImportFailure, ImportReport};
    use std::cell::RefCell;
    // Typing a search, then what it found.
    let mut h = harness();
    h.push(sample::select_screen(env, None));
    h.run(0.3);
    h.key("Slash", "/");
    shown(&mut h, found);
    h.sync("dream", [5, 5], None);
    h.run(0.2);
    h.key("Escape", "Escape");
    h.key("Escape", "Escape");
    shown(&mut h, found);
    // A share's songs.
    let mut data = sample::select();
    data.shared = Some(SharedView {
        pack: "Pack".into(),
        expires: Some("…".into()),
    });
    let play: super::select::PlayFn = Rc::new(|_, _, _, _| {});
    let open: super::select::OpenFn = Rc::new(|_, _| {});
    let mut h = harness();
    h.push(super::select::SelectScreen::new(
        env.clone(),
        data,
        Some("some-things-must"),
        play,
        open,
    ));
    shown(&mut h, found);
    for platform in [Platform::Web, Platform::Desktop] {
        let mut env = env.clone();
        env.platform = platform;
        // Removing (hiding) a pack, a song: confirmed.
        let pack = PackEntry {
            name: Some("Pack".into()),
            removable: true,
            rereadable: false,
            banner: None,
        };
        for (what, button) in [
            (
                Manage::Pack {
                    pack: pack.clone(),
                    songs: 1,
                },
                "RemovePack",
            ),
            (
                Manage::Song {
                    id: "s".into(),
                    title: "Song".into(),
                    pack: pack.clone(),
                    songs: 1,
                },
                "RemoveSong",
            ),
        ] {
            let mut h = harness();
            h.push(ManageDialog::new(env.clone(), what.clone(), None, false));
            shown(&mut h, found);
            h.tap(PointerKind::Mouse, button);
            shown(&mut h, found);
            // While an import runs.
            let mut h = harness();
            h.push(ManageDialog::new(env.clone(), what, None, true));
            shown(&mut h, found);
        }
        // The import: a browser keeping a folder, importing, its report
        // with every kind of line (each line checked whole too: the
        // dialog's list shows the first few); desktop's URL tab, asked
        // for another address, importing, failed.
        let (report, steps) = super::import_report::every_line();
        let views = if platform == Platform::Web {
            let mut texts: Vec<String> = steps
                .iter()
                .map(|s| super::import_report::step_text(s).into())
                .collect();
            for details in [false, true] {
                let view = ImportReportView::Files {
                    report: report.clone(),
                    details,
                };
                texts.push(view.summary());
                for (title, lines) in view.sections() {
                    texts.push(title);
                    texts.extend(lines);
                }
            }
            for failure in [
                None,
                Some(ImportFailure::NothingFound),
                Some(ImportFailure::System("QuotaExceededError".into())),
            ] {
                texts.push(
                    ImportReportView::Files {
                        report: ImportReport {
                            error: failure,
                            ..ImportReport::default()
                        },
                        details: false,
                    }
                    .summary(),
                );
            }
            checked(texts, found);
            vec![
                ImportView {
                    folders: true,
                    ..Default::default()
                },
                ImportView {
                    running: Some(steps[4].clone().into()),
                    ..Default::default()
                },
                ImportView {
                    report: Some(ImportReportView::Files {
                        report: report.clone(),
                        details: true,
                    }),
                    ..Default::default()
                },
            ]
        } else {
            // On the URL tab (an address typed is the player's, drawn
            // by the field as typed).
            let danoni = |danoni: DanoniImportView| ImportView {
                danoni_urls: true,
                danoni_cancel: true,
                danoni,
                ..Default::default()
            };
            vec![
                danoni(DanoniImportView::default()),
                danoni(DanoniImportView {
                    origin: Some(DanoniOriginRequest {
                        request_id: 42,
                        origin: "https://example.org".into(),
                    }),
                    ..Default::default()
                }),
                ImportView {
                    running: Some(ImportRunning::Worded(String::new())),
                    danoni_cancel: true,
                    ..danoni(DanoniImportView {
                        stage: Some("…".into()),
                        progress: Some((1, 3)),
                        ..Default::default()
                    })
                },
                ImportView {
                    report: Some(ImportReportView::Worded {
                        summary: String::new(),
                        error: true,
                        sections: Vec::new(),
                    }),
                    ..danoni(DanoniImportView::default())
                },
            ]
        };
        for view in views {
            let mut h = harness();
            h.push(ImportDialog::new(
                env.clone(),
                Rc::new(RefCell::new(view)),
                None,
            ));
            shown(&mut h, found);
        }
    }
    // Sharing: what is shown, the list; the upload, the link, a failure.
    for title in [Some("Song".to_string()), None] {
        let view = Rc::new(RefCell::new(ShareView::default()));
        let mut h = harness();
        h.push(ShareDialog::new(
            env.clone(),
            "Pack".into(),
            vec!["a".into(), "b".into()],
            title,
            view.clone(),
            None,
        ));
        shown(&mut h, found);
        h.tap(PointerKind::Mouse, "Expiry.1w");
        shown(&mut h, found);
        for state in [ShareState::Done, ShareState::Failed] {
            *view.borrow_mut() = ShareView {
                state,
                link: "https://example.org/s/x".into(),
                error: Some(ddi_library::share::ShareError::of(
                    "bg/clip.avi",
                    ddi_library::share::refusal(403, None),
                )),
                ..Default::default()
            };
            h.ui.broadcast(crate::Message::new(super::select::SHARE_CHANGED));
            shown(&mut h, found);
        }
    }
    // A chart's plays: none; a page of them, one removal armed.
    let row = |i: usize| PlayRow {
        id: format!("p{i}"),
        when: "2026-10-16 14:03".into(),
        grade: "AA".into(),
        score: "96.12%".into(),
        lamp: "FullCombo".into(),
        lamp_text: "FC".into(),
        ruleset: "ITG".into(),
        mods: "×2 · reverse".into(),
        autoplay: i == 0,
        best: i == 1,
        pending: i == 2,
        rejudged: i == 3,
    };
    for rows in [Vec::new(), (0..12).map(row).collect::<Vec<_>>()] {
        let some = !rows.is_empty();
        let mut h = harness();
        h.push(PlaysDialog::new(
            env.clone(),
            "Hard 9",
            Some(ddi_chart::simplify::Kind::WithoutCenter),
            rows,
            None,
        ));
        shown(&mut h, found);
        if some {
            h.tap(PointerKind::Mouse, "Older");
            shown(&mut h, found);
            h.tap(PointerKind::Mouse, "Remove");
            shown(&mut h, found);
        }
    }
    // The notices in full (their texts are the shells').
    let mut h = harness();
    h.push(NoticesDialog::new(
        env.clone(),
        vec![Notice {
            text: String::new(),
            link: None,
            error: false,
        }],
    ));
    shown(&mut h, found);
}

/// A list of actions (the editor's menus), page by page.
fn pages(h: &mut Harness, found: &mut BTreeSet<String>) {
    for _ in 0..12 {
        shown(h, found);
        if h.ui.bounds("NextPage").is_none() {
            return;
        }
        h.tap(PointerKind::Touch, "NextPage");
    }
}

/// Typed into the field being typed into, then Enter.
fn type_in(h: &mut Harness, text: &str) {
    for _ in 0..8 {
        h.key("Backspace", "Backspace");
    }
    h.send(crate::UiEvent::Text(crate::event::TextEvent::Commit(
        text.into(),
    )));
    h.key("Enter", "Enter");
}

/// The chart editor on `env`'s wording and everything it opens (with
/// `all`; else what that wording changes: the menu, the questions): its
/// panel and status lines, the menu page by page, the note tools, the
/// timing at the cursor and each change's form (typed too, and refused),
/// the song's and the chart's fields, tap tempo, the questions before
/// saving and leaving.
fn editor_walk(env: &Env, harness: &dyn Fn() -> Harness, all: bool, found: &mut BTreeSet<String>) {
    use super::editor::{ACTION, CHART, SONG, TIMING};
    let open = |new_song: bool| {
        let mut h = harness();
        h.push(sample::editor_screen(env, new_song));
        h.run(0.3);
        h
    };
    let pick = |h: &mut Harness, menu: &'static str, id: &str| {
        h.ui.broadcast(Message::new(menu).with("Action", Param::Str(id.into())));
        h.run(0.3);
    };
    let taps = |h: &mut Harness| {
        for i in 0..8 {
            h.ui.broadcast(
                Message::new("Editor.Tap").with("Time", Param::Num(1.0 + f64::from(i) * 0.5)),
            );
        }
    };
    // The menu, with what taps add to it.
    let mut h = open(false);
    taps(&mut h);
    h.key("Tab", "Tab");
    pages(&mut h, found);
    // Saving over the song, leaving it unsaved, leaving a new song.
    let mut h = open(false);
    let command = Modifiers {
        meta: true,
        ..Modifiers::default()
    };
    h.key_with("KeyS", "s", command);
    shown(&mut h, found);
    for new_song in [false, true] {
        let mut h = open(new_song);
        h.key("Escape", "Escape");
        shown(&mut h, found);
    }
    if !all {
        return;
    }
    // The screen, the taps' lines, and what keys say.
    let mut h = open(false);
    shown(&mut h, found);
    taps(&mut h);
    shown(&mut h, found);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    for (code, key, modifiers) in [
        ("KeyK", "k", Modifiers::default()),
        ("KeyK", "K", shift),
        ("KeyV", "v", Modifiers::default()),
        ("KeyN", "n", Modifiers::default()),
        ("ArrowLeft", "ArrowLeft", Modifiers::default()),
        ("KeyG", "g", Modifiers::default()),
        ("KeyY", "Y", shift),
    ] {
        h.key_with(code, key, modifiers);
        shown(&mut h, found);
    }
    // The note tools, the timing, the song, the chart, tap tempo.
    for (key, id) in [
        (None, "tool"),
        (Some(("KeyT", "t")), "timing"),
        (Some(("KeyI", "i")), "song"),
        (Some(("KeyC", "c")), "chart"),
        (None, "tap-tempo"),
    ] {
        let mut h = open(false);
        match key {
            Some((code, key)) => {
                h.key(code, key);
            }
            None => pick(&mut h, ACTION, id),
        }
        pages(&mut h, found);
        if id == "tap-tempo" {
            h.tap(PointerKind::Touch, "Tap");
            shown(&mut h, found);
        }
    }
    // Each timing change's form, then typed, then a value refused. The
    // value field draws what is typed in it as it is (its widget's to
    // mark as data), so what is typed and the values it opens on have no
    // letters here: the speed's typed form ("1.5 2 beats") is left out.
    let segments = [
        "seg:bpm",
        "seg:stop",
        "seg:delay",
        "seg:warp",
        "seg:speed",
        "seg:scroll",
        "seg:fake",
        "seg:timesig",
        "seg:tickcount",
        "seg:combo",
        "seg:label",
        "offset",
    ];
    for id in segments {
        let mut h = open(false);
        pick(&mut h, TIMING, id);
        shown(&mut h, found);
        if id == "seg:speed" {
            continue;
        }
        if h.ui.bounds("Type").is_some() {
            h.tap(PointerKind::Touch, "Type");
            shown(&mut h, found);
        }
        type_in(&mut h, "-");
        shown(&mut h, found);
    }
    // The song's fields (of each kind, on no value or a number) and the
    // chart's.
    for id in [
        "subtitle",
        "background",
        "preview_start",
        "display_bpm",
        "artist_translit",
    ] {
        let mut h = open(false);
        pick(&mut h, SONG, id);
        shown(&mut h, found);
        if h.ui.bounds("Type").is_some() {
            h.tap(PointerKind::Touch, "Type");
            shown(&mut h, found);
        }
    }
    for id in ["add", "remove", "meter", "name", "description", "credit"] {
        let mut h = open(false);
        pick(&mut h, CHART, id);
        pages(&mut h, found);
        if id == "meter" {
            h.tap(PointerKind::Touch, "Type");
            type_in(&mut h, "-");
            shown(&mut h, found);
        }
    }
}

/// The multiplayer prototype's screens (`room.rs`): the gated start
/// menu, its home, the room in each phase with a song and without, its
/// Rules (in the lobby and locked) and More, song select choosing a
/// stage's song, and a chart refused.
fn room_walk(env: &Env, harness: &dyn Fn() -> Harness, found: &mut BTreeSet<String>) {
    use crate::state::room::{Refusal, RoomClient, Unsupported};
    for desktop in [false, true] {
        let mut h = harness();
        h.push(sample::start_screen(env, desktop).with_multiplayer(true));
        shown(&mut h, found);
    }
    let mut h = harness();
    h.push(sample::multiplayer_screen(env));
    shown(&mut h, found);
    let song = sample::bundled()
        .into_iter()
        .find(|s| s.charts.iter().any(|c| c.style == "dance-single"))
        .expect("a sample song with a Single chart");
    let chart = song
        .charts
        .iter()
        .find(|c| c.style == "dance-single")
        .unwrap()
        .clone();
    let room = RoomClient::shared(0, 1);
    // The room's song select: under the hidden option the Rules first
    // (Reset and OK), then Rules from its button, Add custom, Players and
    // More over it.
    rules_first(true);
    let show = |h: &mut Harness, found: &mut BTreeSet<String>| {
        shown(h, found);
        // The standings pop up by themselves after a stage: shown, closed.
        while h.ui.layers().last().is_some_and(|l| l.name != "select") {
            shown(h, found);
            h.key("Enter", "Enter");
            h.run(0.3);
        }
        for (open, inner) in [
            ("Room.Rules", Some("AddCustom")),
            ("Room.Players", None),
            ("More", None),
        ] {
            if h.ui.bounds(open).is_none() {
                continue;
            }
            h.tap(PointerKind::Mouse, open);
            shown(h, found);
            if let Some(inner) = inner
                && h.ui.bounds(inner).is_some()
            {
                h.tap(PointerKind::Mouse, inner);
                shown(h, found);
                h.key("Escape", "Escape");
                h.run(0.3);
            }
            h.key("Escape", "Escape");
            h.run(0.3);
        }
    };
    let mut h = harness();
    h.push(sample::room_screen(env, room.clone()));
    shown(&mut h, found);
    rules_first(false);
    h.tap(PointerKind::Mouse, "Ok");
    show(&mut h, found);
    // A pick refused, then made: the prompt with Ready and its players,
    // ready, counting down, on, sitting out; stopped.
    if let Some(double) = song.charts.iter().find(|c| c.style != "dance-single") {
        assert!(room.borrow_mut().pick(&song, double).is_err());
        shown(&mut h, found);
        room.borrow_mut().clear_refusal();
    }
    room.borrow_mut().pick(&song, &chart).unwrap();
    show(&mut h, found);
    let stage = room.borrow().stages()[0].id;
    let quiet: super::room::RoomShell = Rc::new(|_, _| {});
    let prompt = |h: &mut Harness, found: &mut BTreeSet<String>| {
        let state = Rc::new(std::cell::RefCell::new(
            super::prompt::PromptState::default(),
        ));
        state.borrow_mut().info.title = ddi_l10n::UiText::data(&song.title).into();
        h.push(
            super::prompt::PromptScreen::new(env.clone(), state, Rc::new(|_, _| {})).with_room(
                room.clone(),
                stage,
                quiet.clone(),
            ),
        );
        shown(h, found);
    };
    let mut hp = harness();
    prompt(&mut hp, found);
    for job in room.borrow_mut().take_load_jobs() {
        room.borrow_mut().finish_load(&job, Ok(()));
    }
    room.borrow_mut().set_ready(true).unwrap();
    show(&mut h, found);
    let mut hp = harness();
    prompt(&mut hp, found);
    room.borrow_mut().tick(10_000);
    show(&mut h, found);
    room.borrow_mut().stop_match().unwrap();
    show(&mut h, found);
    let mut h = harness();
    h.push(super::room_dialogs::NoticeDialog::refusal(
        env.clone(),
        &Refusal::Chart(Unsupported::Layout {
            style: "dance-double".into(),
        }),
    ));
    shown(&mut h, found);
    // Every refusal's words.
    use ddi_room::{CapExceeded, Rejection, RulesProblem};
    for r in [
        Rejection::NoContent,
        Rejection::NotEnoughReady {
            ready_humans: 0,
            competitors: 1,
        },
        Rejection::StageStarted,
        Rejection::WrongPhase {
            phase: ddi_room::PhaseKind::Stage,
        },
        Rejection::Unauthorized {
            capability: ddi_room::Capability::ChooseContent,
        },
        Rejection::InvalidRules(RulesProblem::StageCount { stages: 0, max: 16 }),
        Rejection::InvalidRules(RulesProblem::OverCap(CapExceeded {
            requested: 9,
            cap: 8,
        })),
        Rejection::InvalidRules(RulesProblem::TooFewSeats { seats: 1, min: 2 }),
        Rejection::InvalidRules(RulesProblem::EliminationWithLateEntry),
        Rejection::UnknownClient,
    ] {
        checked(
            vec![super::room::refusal_text(&Refusal::Room(r)).into()],
            found,
        );
        let mut h = harness();
        h.push(super::room_dialogs::NoticeDialog::refusal(
            env.clone(),
            &Refusal::Room(Rejection::UnknownClient),
        ));
        shown(&mut h, found);
    }
    use crate::state::room::LoadProblem;
    for r in [
        Refusal::Preparing,
        Refusal::Load(LoadProblem::Unreadable),
        Refusal::Load(LoadProblem::Changed),
        Refusal::Load(LoadProblem::CpuCannotPlay),
    ] {
        checked(vec![super::room::refusal_text(&r).into()], found);
    }
    for skills in [
        "balanced",
        "stream",
        "jacks",
        "chords",
        "stamina",
        "technical",
        "1,1,1,1,1,1,0.9",
    ] {
        checked(
            vec![super::room::personality_text(Some(skills)).into()],
            found,
        );
    }
    // A match played (M3): the standings with the player's result and
    // the places, the match's results, a stopped match's partial ones.
    use ddi_room::{Finish, Finisher};
    let report = |room: &crate::state::room::SharedRoom, last: bool| {
        let mut r = room.borrow_mut();
        r.pick(&song, &chart).unwrap();
        for job in r.take_load_jobs() {
            r.finish_load(&job, Ok(()));
        }
        r.set_ready(true).unwrap();
        let now = r.now_ms() + 3_000;
        r.tick(now);
        let order = r.take_stage_order().unwrap();
        let mut ranking: Vec<Vec<Finisher>> = order
            .cpus
            .iter()
            .map(|c| {
                vec![Finisher {
                    participant: c.participant,
                    finish: Finish::Eliminated,
                }]
            })
            .collect();
        let me = vec![Finisher {
            participant: order.me,
            finish: Finish::Survived,
        }];
        if last {
            ranking.push(me);
        } else {
            ranking.insert(0, me);
        }
        r.report_stage(order.stage, ranking, false).unwrap();
    };
    for stop in [false, true] {
        let room = RoomClient::shared(0, 1);
        room.borrow_mut().set_rules_seen(true);
        let rules = ddi_room::MatchRules {
            stages: 2,
            ..room.borrow().rules().clone()
        };
        room.borrow_mut().save_rules(rules).unwrap();
        let mut h = harness();
        h.push(sample::room_screen(env, room.clone()));
        shown(&mut h, found);
        report(&room, false);
        // The standings pop up; Continue; song select between stages.
        shown(&mut h, found);
        h.key("Enter", "Enter");
        show(&mut h, found);
        if stop {
            room.borrow_mut().stop_match().unwrap();
        } else {
            report(&room, true);
        }
        shown(&mut h, found);
        h.key("Enter", "Enter");
        show(&mut h, found);
    }
    // A stage's results, its prompt's note, its play's column and every
    // kind of the player's stage result.
    let mut h = harness();
    let mut data = sample::results();
    data.kind = super::data::ResultsKind::Stage;
    h.push(super::results::ResultsScreen::new(
        env.clone(),
        data,
        Default::default(),
    ));
    shown(&mut h, found);
    use ddi_l10n::msg::room as m;
    // A stage's results over its room (feedback round B): the Standings
    // tab after a middle stage and after the last (the match's result on
    // top), with how everyone performed (one out, one who left), then
    // the Graphs tab; and the lines' words while a result has not come.
    {
        use super::results::ResultsScreen;
        let room = RoomClient::shared(0, 1);
        room.borrow_mut().set_rules_seen(true);
        let rules = ddi_room::MatchRules {
            stages: 2,
            ..room.borrow().rules().clone()
        };
        room.borrow_mut().save_rules(rules).unwrap();
        for last in [false, true] {
            let order = {
                let mut r = room.borrow_mut();
                r.pick(&song, &chart).unwrap();
                for job in r.take_load_jobs() {
                    r.finish_load(&job, Ok(()));
                }
                r.set_ready(true).unwrap();
                let now = r.now_ms() + 3_000;
                r.tick(now);
                r.take_stage_order().unwrap()
            };
            let ids: Vec<_> = std::iter::once(order.me)
                .chain(order.cpus.iter().map(|c| c.participant))
                .collect();
            let finishes = [
                Finish::Survived,
                Finish::Survived,
                Finish::Eliminated,
                Finish::Forfeit,
            ];
            let ranking = ids
                .iter()
                .zip(finishes)
                .map(|(participant, finish)| {
                    vec![Finisher {
                        participant: *participant,
                        finish,
                    }]
                })
                .collect();
            let details = ids
                .iter()
                .zip(finishes)
                .map(|(participant, finish)| {
                    let out = finish == Finish::Eliminated;
                    (
                        *participant,
                        ddi_room::RunDetail {
                            percent: Some(9_512),
                            grade: "AA".into(),
                            life_ppm: if out { 0 } else { 640_000 },
                            out_at_ms: out.then_some(62_000),
                            max_combo: 120,
                            tiers: vec![100, 40, 9, 0, 0, 0, 1],
                            attacks: Some(2),
                            shocks: Some(1),
                            ..Default::default()
                        },
                    )
                })
                .collect();
            room.borrow_mut()
                .report_played(order.stage, ddi_room::Run::Forfeit, ranking, false, details)
                .unwrap();
            let mut data = sample::results();
            data.kind = super::data::ResultsKind::Stage;
            let mut h = harness();
            h.push(
                ResultsScreen::new(env.clone(), data, Default::default())
                    .with_room(room.clone(), order.stage),
            );
            shown(&mut h, found);
            if last {
                assert_eq!(
                    env.state.borrow().stage.as_ref().map(|v| v.winners.len()),
                    Some(1),
                    "the match's result is shown"
                );
            }
            h.key("ArrowRight", "ArrowRight");
            shown(&mut h, found);
            h.key("ArrowLeft", "ArrowLeft");
            shown(&mut h, found);
        }
        checked(
            vec![
                m::table_playing().into(),
                m::table_waiting().into(),
                m::table_sent().into(),
                m::table_taken().into(),
                m::table_shocks().into(),
                m::summary_sent(ddi_l10n::UiText::data("Mira ×2")).into(),
                m::summary_sent_none().into(),
                m::summary_taken(ddi_l10n::UiText::data("Tobi")).into(),
                m::summary_taken_none().into(),
                m::summary_shocks(3).into(),
                m::table_title(2, 4).into(),
            ],
            found,
        );
    }
    // M8 round 3: a match with attacks. The room's song select and the
    // Players pop-up with everyone's attack policy, and the stage's
    // prompt with the player's own, each of the three.
    {
        let room = RoomClient::shared(0, 1);
        room.borrow_mut().set_rules_seen(true);
        let mut rules = room.borrow().rules().clone();
        rules.set_attacks(true);
        room.borrow_mut().save_rules(rules).unwrap();
        let mut h = harness();
        h.push(sample::room_screen(env, room.clone()));
        shown(&mut h, found);
        let mut hd = harness();
        hd.push(super::room_dialogs::PlayersDialog::new(
            env.clone(),
            room.clone(),
        ));
        shown(&mut hd, found);
        room.borrow_mut().pick(&song, &chart).unwrap();
        let stage = room.borrow().stages()[0].id;
        let quiet: super::room::RoomShell = Rc::new(|_, _| {});
        for _ in 0..3 {
            let state = Rc::new(std::cell::RefCell::new(
                super::prompt::PromptState::default(),
            ));
            state.borrow_mut().info.title = ddi_l10n::UiText::data(&song.title).into();
            let mut hp = harness();
            hp.push(
                super::prompt::PromptScreen::new(env.clone(), state, Rc::new(|_, _| {}))
                    .with_room(room.clone(), stage, quiet.clone()),
            );
            shown(&mut hp, found);
            assert!(
                hp.ui.actor("Opt.room.targeting").is_some(),
                "the policy's row"
            );
            room.borrow_mut().cycle_targeting().unwrap();
        }
        use ddi_room::attacks::Targeting;
        checked(
            Targeting::ALL
                .iter()
                .flat_map(|t| {
                    [
                        super::room::policy_word(*t).into(),
                        super::room::policy_about(*t).into(),
                    ]
                })
                .chain([m::play_attack_waiting().into(), m::targeting().into()])
                .collect(),
            found,
        );
    }
    let mut info = super::prompt::PromptInfo::default();
    // (Practice with its attacks turned off: said. An online room's has
    // none to turn off: nothing said.)
    info.set_stage(1, 4, 3, 0, &Default::default(), Some(false));
    let mut online = super::prompt::PromptInfo::default();
    online.set_stage(
        2,
        4,
        0,
        2,
        &crate::state::room::StageRules {
            difficulty: ddi_room::MatchDifficulty::Hard,
            boos: true,
            ..Default::default()
        },
        None,
    );
    checked(
        vec![
            info.mods,
            online.mods,
            m::play_out().into(),
            m::play_won().into(),
            // A battle's moments shown beside the play.
            m::play_cue_match_begin().into(),
            m::play_cue_stage_begin(2).into(),
            m::play_cue_final_stage().into(),
            m::play_cue_life_critical().into(),
            m::play_cue_survivors(2).into(),
            m::play_cue_eliminated().into(),
            m::play_cue_stage_drawn().into(),
            m::play_cue_attack_incoming().into(),
            m::play_cue_attack_launched().into(),
        ],
        found,
    );
    for finish in [Finish::Survived, Finish::Eliminated, Finish::Forfeit] {
        let mark = crate::state::room::StageMark {
            number: 2,
            placement: ddi_room::Placement {
                participant: ddi_room::ParticipantId(1),
                place: 2,
                finish,
                points: ddi_room::Points::ratio(5, 2),
            },
            field: 4,
            experimental: true,
        };
        checked(vec![super::room::mark_text(&mark)], found);
    }
}

/// Online rooms (M5c): the home signed out, signing in on desktop
/// (waiting, canceled, failed) and signed in with the invite field; an
/// invite's page in each state; the online room as its owner and as a
/// member, its connection's states, the development stop and More.
fn online_walk(env: &Env, harness: &dyn Fn() -> Harness, found: &mut BTreeSet<String>) {
    use super::online::{OnlinePage, OnlineScreen};
    use crate::state::online::tests::{INVITE, ME, err, ok, rig};
    use crate::state::online::{Dest, Platform};
    let quiet = || -> super::online::OnlineShell { Rc::new(|_, _| {}) };
    // The home, signed out, on both shells; then signing in on desktop.
    for platform in [Platform::Web, Platform::Desktop] {
        let r = rig(platform, Box::new(|_| err(401, "signed_out")));
        let mut h = harness();
        h.push(OnlineScreen::new(
            env.clone(),
            r.online.clone(),
            OnlinePage::Home,
            quiet(),
        ));
        shown(&mut h, found);
    }
    let attempt = r#"{"attempt_id":"a1","poll_secret":"s1","open_url":"u","expires_at_ms":9}"#;
    let r = rig(
        Platform::Desktop,
        Box::new(move |req| match req.path.as_str() {
            "/auth/attempts" => ok(attempt),
            "/me" => err(401, "signed_out"),
            _ => None,
        }),
    );
    let mut h = harness();
    h.push(OnlineScreen::new(
        env.clone(),
        r.online.clone(),
        OnlinePage::Home,
        quiet(),
    ));
    r.online.borrow_mut().sign_in(Dest::Create);
    shown(&mut h, found);
    r.online.borrow_mut().cancel_sign_in();
    shown(&mut h, found);
    // Signed in: the account, Sign out, the invite field and its refusal.
    let r = rig(
        Platform::Desktop,
        Box::new(|req| match req.path.as_str() {
            "/me" => ok(ME),
            _ => err(409, "room_limit"),
        }),
    );
    let mut h = harness();
    h.push(OnlineScreen::new(
        env.clone(),
        r.online.clone(),
        OnlinePage::Home,
        quiet(),
    ));
    shown(&mut h, found);
    r.online.borrow_mut().private_room();
    shown(&mut h, found);
    // Join by link, its field and its refusal; the home's sign-in dialog
    // asked for from the footer, signed out.
    let r = rig(Platform::Web, Box::new(|_| err(401, "signed_out")));
    let mut h = harness();
    h.push(OnlineScreen::new(
        env.clone(),
        r.online.clone(),
        OnlinePage::JoinLink,
        quiet(),
    ));
    shown(&mut h, found);
    let mut h = harness();
    h.push(OnlineScreen::new(
        env.clone(),
        r.online.clone(),
        OnlinePage::Home,
        quiet(),
    ));
    h.run(0.5);
    h.tap(crate::PointerKind::Mouse, super::online_home::ACCOUNT);
    shown(&mut h, found);
    // An invite's page: reading, each admission, each refusal.
    let previews = [
        None,
        ok(
            r#"{"room_id":"Room000001","room_name":"After Hours","host_display_name":"Alice","phase":"lobby","admission":"open","members":2}"#,
        ),
        ok(
            r#"{"room_id":"Room000001","room_name":"After Hours","host_display_name":"Alice","phase":"stage","admission":"next_match","members":1}"#,
        ),
        ok(
            r#"{"room_id":"Room000001","room_name":"After Hours","host_display_name":"Alice","phase":"lobby","admission":"closed","members":3}"#,
        ),
        ok(
            r#"{"room_id":"Room000001","room_name":"After Hours","host_display_name":"Alice","phase":"lobby","admission":"full","members":32}"#,
        ),
        err(410, "invite_expired"),
        err(403, "invite_rotated"),
        err(404, "invite_unknown"),
        err(410, "room_closed"),
        err(500, "worker_error"),
        Some(crate::state::online::api::ApiResponse::unreachable()),
    ];
    for (signed_in, preview) in [false, true]
        .into_iter()
        .flat_map(|s| previews.iter().cloned().map(move |p| (s, p)))
    {
        let p = std::cell::RefCell::new(preview);
        let r = rig(
            Platform::Web,
            Box::new(move |req| match req.path.as_str() {
                "/me" if signed_in => ok(ME),
                "/me" => err(401, "signed_out"),
                _ => p.borrow_mut().take(),
            }),
        );
        let mut h = harness();
        h.push(OnlineScreen::new(
            env.clone(),
            r.online.clone(),
            OnlinePage::Invite(INVITE.into()),
            quiet(),
        ));
        shown(&mut h, found);
    }
    // The online room: an owner and a member over the fake room, through
    // its states.
    use crate::state::online::tests::room::{client, person, song, tick};
    use ddi_room::fake::{FakeServer, Fault};
    use ddi_room::{Identity, Intent, ManualClock, ParticipantId, RoomPolicy};
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "bob", Intent::Play)
        .unwrap();
    let alice = Rc::new(std::cell::RefCell::new(client(
        &server,
        person(&server, "alice"),
        "alice",
        1_000,
    )));
    let bob = Rc::new(std::cell::RefCell::new(client(
        &server,
        person(&server, "bob"),
        "bob",
        1_000,
    )));
    // Under the hidden option: each one's Rules first.
    rules_first(true);
    let mut ha = harness();
    ha.push(sample::room_screen(env, alice.clone()));
    let mut hb = harness();
    hb.push(sample::room_screen(env, bob.clone()));
    shown(&mut ha, found);
    let step = |ms: u64| {
        tick(
            &clock,
            &mut [&mut alice.borrow_mut(), &mut bob.borrow_mut()],
            ms,
        );
    };
    step(400);
    // Each one's Rules first (Bob's read only), then the room.
    for h in [&mut ha, &mut hb] {
        shown(h, found);
        h.tap(PointerKind::Mouse, "Ok");
        shown(h, found);
    }
    rules_first(false);
    // The host's resting song shown to Bob, then picked.
    let s = song();
    alice.borrow_mut().hover(Some((&s, &s.charts[0])));
    for _ in 0..8 {
        step(500);
    }
    shown(&mut hb, found);
    alice.borrow_mut().pick(&s, &s.charts[0]).unwrap();
    let first = alice.borrow().stages()[0].id;
    step(400);
    for h in [&mut ha, &mut hb] {
        shown(h, found);
        h.tap(PointerKind::Mouse, "More");
        shown(h, found);
        h.key("Escape", "Escape");
        h.run(0.3);
    }
    checked(
        [
            crate::state::online::link::LinkNotice::InviteCopied,
            crate::state::online::link::LinkNotice::InviteReplaced,
            crate::state::online::link::LinkNotice::InviteNotReplaced,
        ]
        .into_iter()
        .map(|n| {
            alice.borrow_mut().set_link_notice(Some(n));
            ha.run(0.5);
            ha.texts()
        })
        .collect::<Vec<_>>()
        .concat(),
        found,
    );
    alice.borrow_mut().set_link_notice(None);
    // Sharing the stage's song (M5d): the upload as it goes, its failure
    // and More's retry, then the room's refusal of a share without the
    // chosen chart, then the shared song read by both.
    {
        use crate::state::online::tests::room::{prepare, upload};
        // The screen handed the uploads to its (sample) shell already.
        let _ = alice.borrow_mut().take_upload_jobs();
        alice.borrow_mut().upload_progress(first, 10.0, 40.0);
        shown(&mut ha, found);
        alice
            .borrow_mut()
            .upload_finished(first, Err(ddi_library::share::ShareError::TooLarge));
        shown(&mut ha, found);
        ha.tap(PointerKind::Mouse, "More");
        shown(&mut ha, found);
        ha.key("Escape", "Escape");
        server
            .borrow_mut()
            .authority(ddi_room::Command::SetContentStatus {
                stage: first,
                status: ddi_room::ContentStatus::Unavailable {
                    code: "digest_mismatch".into(),
                },
                share: None,
            })
            .unwrap();
        step(400);
        shown(&mut ha, found);
        shown(&mut hb, found);
        alice.borrow_mut().retry_sharing(first);
        server
            .borrow_mut()
            .authority(ddi_room::Command::SetContentStatus {
                stage: first,
                status: ddi_room::ContentStatus::Pending,
                share: None,
            })
            .unwrap();
        step(400);
        upload(&server, &mut alice.borrow_mut(), "abc12345");
        step(400);
        prepare(&mut alice.borrow_mut());
        prepare(&mut bob.borrow_mut());
        step(400);
        shown(&mut ha, found);
        shown(&mut hb, found);
    }
    // The connection's states, as Bob's row and Bob's own status say them.
    server.borrow_mut().drop_socket(ParticipantId(2));
    step(100);
    shown(&mut ha, found);
    shown(&mut hb, found);
    server.borrow_mut().fault(
        ParticipantId(1),
        Fault::Close(ddi_room::online::close::SIGN_IN_AGAIN),
    );
    step(100);
    shown(&mut ha, found);
    ha.tap(PointerKind::Mouse, "More");
    shown(&mut ha, found);
    ha.key("Escape", "Escape");
    server.borrow_mut().fault(
        ParticipantId(2),
        Fault::Close(ddi_room::online::close::ROOM_CLOSED),
    );
    step(1_000);
    shown(&mut hb, found);
}

fn walk() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let env = env();
    for (size, design) in [([1280.0, 720.0], DESIGN), ([844.0, 390.0], COMPACT)] {
        let harness = || Harness::new(design, size, 1.0, TextMode::Own);
        for desktop in [false, true] {
            let mut h = harness();
            h.push(sample::start_screen(&env, desktop));
            shown(&mut h, &mut found);
        }
        let mut h = harness();
        h.push(sample::select_screen(&env, None));
        shown(&mut h, &mut found);
        select_walk(&env, &harness, &mut found);
        room_walk(&env, &harness, &mut found);
        online_walk(&env, &harness, &mut found);
        // The prompt's steps, as the shells hand them in: the loading
        // screen's own word is an identifier, never shown; an error's
        // explanation is still the loader's.
        use ddi_l10n::{UiText, msg};
        for (step, platform) in [
            (PromptStep::Ready, Platform::Web),
            (PromptStep::Loading, Platform::Web),
            (PromptStep::Interrupted, Platform::Web),
            (PromptStep::Interrupted, Platform::Desktop),
            (
                PromptStep::Access {
                    folder: "Pack".into(),
                    refused: Some(msg::prompt::access_refused().into()),
                },
                Platform::Web,
            ),
            (
                PromptStep::Error(msg::prompt::song_failed(UiText::data("no music file")).into()),
                Platform::Desktop,
            ),
            (
                PromptStep::Error(msg::prompt::no_chart(3).into()),
                Platform::Web,
            ),
        ] {
            let mut h = harness();
            let mut env = env.clone();
            env.platform = platform;
            h.push(sample::prompt_screen(&env, step));
            shown(&mut h, &mut found);
        }
        let mut h = harness();
        h.push(sample::results_screen(&env));
        shown(&mut h, &mut found);
        let mut h = harness();
        h.push(sample::options_screen(&env, None));
        shown(&mut h, &mut found);
        let mut h = harness();
        h.push(sample::calibration_screen(&env));
        shown(&mut h, &mut found);
        let mut h = harness();
        h.push(sample::studio_screen(&env));
        shown(&mut h, &mut found);
        for words in [
            ddi_l10n::UiText::data("One hundred combo!"),
            ddi_l10n::msg::studio::stage_said(super::studio::name(
                ddi_library::announcer::Line::Cheer,
            )),
        ] {
            let mut h = harness();
            stage_frame(&mut h, &env, words);
            shown(&mut h, &mut found);
        }
        for count in [1, 7] {
            let mut h = harness();
            h.push(folders_offer(&env, count));
            shown(&mut h, &mut found);
        }
        for platform in [Platform::Web, Platform::Desktop] {
            let mut env = env.clone();
            env.platform = platform;
            let mut h = harness();
            h.push(sample::credits_screen(&env, &credits::sample()));
            shown(&mut h, &mut found);
            let mut h = harness();
            let view = super::data::ImportView::default();
            h.push(ImportDialog::new(
                env.clone(),
                Rc::new(std::cell::RefCell::new(view)),
                None,
            ));
            shown(&mut h, &mut found);
            let pack = PackEntry {
                name: Some("Pack".into()),
                removable: true,
                rereadable: true,
                banner: None,
            };
            for what in [
                Manage::Pack {
                    pack: pack.clone(),
                    songs: 3,
                },
                Manage::Song {
                    id: "s".into(),
                    title: "Song".into(),
                    pack: pack.clone(),
                    songs: 3,
                },
                Manage::Unreadable { count: 2 },
            ] {
                let mut h = harness();
                h.push(ManageDialog::new(env.clone(), what, None, false));
                shown(&mut h, &mut found);
            }
            editor_walk(&env, &harness, platform == Platform::Web, &mut found);
        }
        // The number forms, each kind, their fields named as their
        // callers name them.
        for format in [
            NumberFormat::Scalar,
            NumberFormat::Pair("/"),
            NumberFormat::Speed,
            NumberFormat::DisplayBpm,
        ] {
            let mut numbers = Numbers::scalar(1.5, 0.5, 0.0, 10.0);
            if !matches!(format, NumberFormat::Scalar) {
                numbers.fields.push(Number::new(
                    |v| ddi_l10n::UiText::data(format!("Second: {v}")),
                    2.0,
                    1.0,
                    0.0,
                    10.0,
                ));
            }
            numbers.format = format;
            numbers.remove = matches!(format, NumberFormat::DisplayBpm);
            let mut h = harness();
            h.push(NumberDialog::new(
                env.clone(),
                &ddi_l10n::UiText::data("Title"),
                numbers,
                Rc::new(|_, _| Ok(())),
            ));
            shown(&mut h, &mut found);
        }
        // The announcer studio's Add a combo line….
        let mut h = harness();
        h.push(super::studio::combo_dialog(
            env.clone(),
            Rc::new(|_, _| true),
        ));
        shown(&mut h, &mut found);
    }
    // Options' rows on each shell, every value of each, and the
    // confirmations.
    for platform in [Platform::Web, Platform::Desktop] {
        let world = World {
            platform,
            pads_supported: true,
            sample: Sample::Ready,
            plays: 3,
            folders: (platform == Platform::Desktop).then(|| vec!["/Songs".into()]),
            noteskins: vec![("folder:x".into(), "X".into())],
            ..World::default()
        };
        let settings = Settings {
            noteskin: Some("folder:x".into()),
            ..Settings::default()
        };
        let model = OptionsModel::new(settings, world);
        options_texts(&model, &mut found);
        for r in [
            Reset::Settings,
            Reset::Songs,
            Reset::Everything,
            Reset::History,
        ] {
            let (title, body, yes) = r.texts(&model);
            for t in [title, body, yes] {
                if in_code(&t) {
                    found.insert(line(&t));
                }
            }
        }
    }
    // The announcer studio's texts the walk of its page leaves folded.
    for t in studio_texts() {
        if in_code(&t) {
            found.insert(line(&t));
        }
        if joined(&t) {
            found.insert(format!("JOINED {}", line(&t)));
        }
    }
    found
}

/// Options' sections, rows, values, notes, headings and descriptions, and
/// the preview's line and layout.
fn options_texts(model: &OptionsModel, found: &mut BTreeSet<String>) {
    let data = model.data();
    let preview = data.preview_layout.iter();
    for o in data.sections.iter().flat_map(|s| &s.options).chain(preview) {
        for t in [&o.label, &o.note, &o.description]
            .into_iter()
            .chain(o.values())
            .chain(o.heading.as_ref())
        {
            if in_code(t) {
                found.insert(line(t));
            }
            if joined(t) {
                found.insert(format!("JOINED {}", line(t)));
            }
        }
    }
    for t in data
        .sections
        .iter()
        .map(|s| &s.name)
        .chain([&data.preview, &data.notice])
    {
        if in_code(t) {
            found.insert(line(t));
        }
        if joined(t) {
            found.insert(format!("JOINED {}", line(t)));
        }
    }
}

/// Options in every state the walk's plain one does not show: controllers
/// connected and saved, known devices, sharing, an announcer and pump
/// skins chosen, an import running, the notices; nothing written in code.
#[test]
fn every_options_row_is_a_message_or_data() {
    use crate::state::bind::Bound;
    use crate::state::settings::OffsetProfile;
    use ddi_engine::ConnectedPad;
    storage::install(Rc::new(MemoryStore::default()));
    assert!(ddi_l10n::set_language("qps-ploc"));
    let mut found = BTreeSet::new();
    for platform in [Platform::Web, Platform::Desktop] {
        for (pads_supported, importing) in [(true, false), (false, true)] {
            let world = World {
                platform,
                pads_supported,
                importing,
                importing_plays: importing,
                sample: Sample::Playing,
                plays: 1,
                imported: 2,
                shared: 1,
                folders: (platform == Platform::Desktop).then(Vec::new),
                noteskins: vec![
                    ("folder:x".into(), "X".into()),
                    ("pump/cmd".into(), "cmd".into()),
                ],
                default_lua_off: true,
                announcers: vec![("techno".into(), "Techno".into())],
                connected: vec![
                    ConnectedPad {
                        vendor_product: None,
                        id: "Pad (STANDARD GAMEPAD)".into(),
                        index: 0,
                        standard: true,
                    },
                    ConnectedPad {
                        vendor_product: Some([0x045e, 0x0b12]),
                        id: "Xbox".into(),
                        index: 1,
                        standard: true,
                    },
                ],
                sharing: true,
                share_url: "https://example.org/api".into(),
                ..World::default()
            };
            // Devices worded from their facts, from their ids, and one
            // only described.
            let profile = |id: &str, facts: Option<ddi_platform::DeviceFacts>| OffsetProfile {
                label: id.into(),
                id: id.into(),
                facts,
                offset: 0.01,
                calibrated: true,
                last_seen: 0.0,
            };
            let audio = |name: Option<&str>, link| {
                Some(ddi_platform::DeviceFacts::Audio {
                    name: name.map(str::to_string),
                    rate: 44100,
                    latency_ms: 20,
                    link,
                })
            };
            let display = |name: Option<&str>, refresh_hz| {
                Some(ddi_platform::DeviceFacts::Display {
                    name: name.map(str::to_string),
                    width: 1280,
                    height: 720,
                    scale_percent: 150,
                    refresh_hz,
                })
            };
            use ddi_platform::AudioLink;
            let audio_profiles = vec![
                profile("48 kHz", None),
                profile("audio:48000:5:170", None),
                profile("audio:a", audio(None, Some(AudioLink::Wired))),
                profile("audio:b", audio(None, Some(AudioLink::Unknown))),
                profile("audio:c", audio(None, None)),
                profile("audio:d", audio(Some("Speakers"), None)),
            ];
            let display_profiles = vec![
                profile("display:1280x720@2", None),
                profile("display:a", display(None, Some(60))),
                profile("display:b", display(Some("Monitor"), None)),
            ];
            let settings = Settings {
                noteskin: Some("folder:x".into()),
                pump_noteskin: Some("pump/cmd".into()),
                announcer: Some("techno".into()),
                share_token: "token".into(),
                lane_cover: ddi_engine::LaneCover {
                    mode: ddi_engine::CoverMode::HiddenSudden,
                    ..Default::default()
                },
                audio_profiles,
                display_profiles,
                ..Settings::default()
            };
            let mut model = OptionsModel::new(settings, world);
            for layout in ["dance-single", "dance-double"] {
                let at = model
                    .bind_layouts()
                    .iter()
                    .position(|l| l.id == layout)
                    .unwrap();
                model.change("bind_layout", at);
                model.bound(Bound::Pad {
                    id: "Xbox".into(),
                    table: vec![vec!["button:0".into()]; model.bind_layout.lane_count()],
                    start: Some("button:9".into()),
                    back: None,
                });
                // A generic pad's: faces by their places, a list of
                // controls, raw ones in words.
                model.bound(Bound::Pad {
                    id: "Pad (STANDARD GAMEPAD)".into(),
                    table: vec![
                        vec![
                            "button:0".into(),
                            "button:4".into(),
                            "button:20".into(),
                            "axis:1-".into()
                        ];
                        model.bind_layout.lane_count()
                    ],
                    start: Some("hat:0:up".into()),
                    back: Some("button:1".into()),
                });
                options_texts(&model, &mut found);
                model.add_pad_control("Pad (STANDARD GAMEPAD)", 0, "hat:1:left");
                options_texts(&model, &mut found);
            }
            model.add_key(0, "KeyQ");
            model.add_key(1, "KeyQ");
            options_texts(&model, &mut found);
            model.set_share_token("");
            options_texts(&model, &mut found);
            for r in [
                Reset::Songs,
                Reset::Everything,
                Reset::Noteskin("folder:x".into()),
                Reset::Noteskin("pump/cmd".into()),
                Reset::Announcer("techno".into()),
            ] {
                let (title, body, yes) = r.texts(&model);
                for t in [title, body, yes] {
                    if in_code(&t) {
                        found.insert(line(&t));
                    }
                }
            }
        }
    }
    ddi_l10n::set_language("en");
    assert!(found.is_empty(), "written in code: {found:#?}");
}

/// Every text of a page's `blocks`, as written (a page draws a Mono
/// block's lines each as data).
fn block_texts(blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            Block::Heading(t)
            | Block::Subheading(t)
            | Block::Text(t)
            | Block::Muted(t)
            | Block::Note(t) => out.push(t.clone()),
            Block::Link { label, .. }
            | Block::Button { label, .. }
            | Block::Action { label, .. } => out.push(label.clone()),
            Block::Fold { label, body, .. } => {
                out.push(label.clone());
                block_texts(body, out);
            }
            Block::Zone { .. } | Block::Columns { .. } => {
                for inner in b.inner() {
                    block_texts(std::slice::from_ref(inner), out);
                }
            }
            Block::Field { label, hint, .. } => out.extend([label.clone(), hint.clone()]),
            Block::Actions(actions) => out.extend(actions.iter().map(|a| a.label.clone())),
            Block::Entry {
                text,
                detail,
                actions,
            } => {
                out.push(text.clone());
                out.extend(detail.clone());
                out.extend(actions.iter().map(|a| a.label.clone()));
            }
            Block::Mono(_) | Block::Gap => {}
        }
    }
}

/// Desktop's first-start offer of `count` song folders found.
fn folders_offer(env: &Env, count: usize) -> FoldersOfferDialog {
    let folders = (1..=count)
        .map(|i| format!("/Users/me/StepMania {i}/Songs"))
        .collect();
    FoldersOfferDialog::new(env.clone(), folders, Rc::new(|_, _| {}))
}

#[test]
fn every_text_shown_is_a_message_or_data() {
    storage::install(Rc::new(MemoryStore::default()));
    assert!(ddi_l10n::set_language("qps-ploc"));
    let found = walk();
    ddi_l10n::set_language("en");
    assert!(
        found.is_empty(),
        "shown as written in code, not through a message (add it to \
         locales/en/<area>.ftl and every other language; a JOINED line \
         is two messages put together in one run: make them one message \
         with variables; locales/README.md):\n  {}",
        found
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn moved_screens_show_no_text_written_in_code() {
    storage::install(Rc::new(MemoryStore::default()));
    assert!(ddi_l10n::set_language("qps-ploc"));
    let env = env();
    for desktop in [false, true] {
        let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
        h.push(sample::start_screen(&env, desktop));
        h.run(0.5);
        let texts = h.texts();
        assert!(
            texts.iter().any(|t| t.starts_with("«Šíñĝļépļáýéŕ")),
            "{texts:?}"
        );
        let left: Vec<&String> = texts.iter().filter(|t| in_code(t)).collect();
        assert!(left.is_empty(), "the start menu: {left:?}");
    }
    // The pages and the folders offer, wide and on a phone, on both
    // shells' wording.
    for (design, size) in [(DESIGN, [1280.0, 720.0]), (COMPACT, [390.0, 844.0])] {
        let check = |what: &str, push: &dyn Fn(&mut Harness)| {
            let mut h = Harness::new(design, size, 1.0, TextMode::Own);
            push(&mut h);
            h.run(0.5);
            let texts = h.texts();
            let left: Vec<&String> = texts.iter().filter(|t| in_code(t) || joined(t)).collect();
            assert!(left.is_empty(), "{what}: {left:?}");
        };
        check("calibration", &|h| h.push(sample::calibration_screen(&env)));
        for count in [1, 7] {
            check("the folders offer", &|h| h.push(folders_offer(&env, count)));
        }
        for platform in [Platform::Web, Platform::Desktop] {
            let mut env = env.clone();
            env.platform = platform;
            let c = credits::sample();
            check("the credits", &|h| h.push(sample::credits_screen(&env, &c)));
        }
    }
    // Every block of the pages, not only what shows at once: the credits
    // on each shell, calibration's menu with devices and the last runs'
    // lines, the credits waiting.
    let mut settings = Settings::default();
    for (list, kind) in [
        (&mut settings.audio_profiles, "audio"),
        (&mut settings.display_profiles, "display"),
    ] {
        Settings::set_profile_offset(
            list,
            &ddi_platform::DeviceFingerprint::new(format!("{kind}:1"), "Device"),
            0.02,
        );
    }
    settings.display_profiles[0].calibrated = false;
    let runs: Vec<String> = [
        RunEnd::DeviceChanged,
        RunEnd::Unreliable {
            out_of_range: false,
        },
        RunEnd::Unreliable { out_of_range: true },
        RunEnd::Right,
        RunEnd::Set { seconds: -0.012 },
    ]
    .into_iter()
    .flat_map(|end| super::calibration::MODES.map(|m| run_line(m, end)))
    .collect();
    let mut docs = vec![
        (
            "calibration",
            super::calibration::doc(&settings, 250.0, &runs),
        ),
        ("the credits", credits::waiting(None)),
        ("the credits", credits::waiting(Some("No such file"))),
    ];
    for platform in [Platform::Web, Platform::Desktop] {
        docs.push(("the credits", credits::doc(&credits::sample(), platform)));
    }
    for (what, doc) in docs {
        let mut texts = vec![doc.title.text.clone(), doc.title.heading.clone()];
        block_texts(&doc.blocks, &mut texts);
        let left: Vec<&String> = texts.iter().filter(|t| in_code(t) || joined(t)).collect();
        assert!(left.is_empty(), "{what}: {left:?}");
    }
    let (title, body, yes) = super::calibration::reset_texts();
    for t in [title, body, yes] {
        assert!(!in_code(&t), "{t}");
    }
    // The results, wide and on a phone, as a play's and a replay's.
    for (design, size) in [(DESIGN, [1280.0, 720.0]), (COMPACT, [390.0, 844.0])] {
        for (replay, ddi) in [(false, false), (true, false), (true, true)] {
            let mut data = sample::results();
            if ddi {
                // DDI's: a grade of two words, both words of a tier with
                // its early and late counts, its own stats and note.
                use ddi_engine::rules::ddi;
                let rules = ddi::ruleset(&ddi::DdiOptions {
                    boos: true,
                    practice: true,
                    ..Default::default()
                });
                let (early, late) = rules.names.sides(ddi_engine::Judgement::W2);
                data.grade = ddi::ABSOLUTE_DITO.into();
                data.judgements[1].name =
                    ddi_l10n::UiText::data(format!("{early} / {late}")).into();
                data.judgements[1].sides = Some([3, 4]);
                data.lamp = Some(ddi_l10n::UiText::data("GLORIOUS FLAG").into());
                data.stats = vec![
                    (ddi_l10n::msg::results::early_late().into(), "3 / 4".into()),
                    (
                        ddi_l10n::msg::results::ddi_percent().into(),
                        "99.50%".into(),
                    ),
                    (ddi_l10n::msg::results::ddi_score().into(), "1185020".into()),
                ];
                data.mods = super::play::ddi_summary(&rules.ddi.unwrap()).join(" · ");
                data.notes = vec![
                    ddi_l10n::msg::results::note_practice("1:35").into(),
                    ddi_l10n::msg::results::note_rejudged("ITG", "DDI").into(),
                ];
            }
            if replay {
                super::play::mark_replay(
                    &mut data,
                    super::data::ResultsKind::Replay,
                    &ddi_l10n::msg::results::played_at("2026-10-16 14:03"),
                );
            }
            let mut h = Harness::new(design, size, 1.0, TextMode::Own);
            h.push(super::results::ResultsScreen::new(
                env.clone(),
                data,
                super::results::ResultsActions::default(),
            ));
            h.run(0.5);
            let texts = h.texts();
            let left: Vec<&String> = texts.iter().filter(|t| in_code(t) || joined(t)).collect();
            assert!(left.is_empty(), "the results: {left:?}");
        }
    }
    // The announcer studio: its page's texts (the page's own chrome is
    // the pages'), and the frame of its scenes, wide and on a phone.
    let texts = studio_texts();
    assert!(texts.iter().any(|t| t.starts_with("«Ďŕóp")), "{texts:?}");
    let left: Vec<&String> = texts.iter().filter(|t| in_code(t) || joined(t)).collect();
    assert!(left.is_empty(), "the announcer studio: {left:?}");
    for (design, size) in [(DESIGN, [1280.0, 720.0]), (COMPACT, [844.0, 390.0])] {
        let mut h = Harness::new(design, size, 1.0, TextMode::Own);
        stage_frame(&mut h, &env, ddi_l10n::UiText::data("One hundred combo!"));
        h.run(0.1);
        let texts = h.texts();
        assert!(
            texts.iter().any(|t| t == "‹One hundred combo!›"),
            "{texts:?}"
        );
        let left: Vec<&String> = texts.iter().filter(|t| in_code(t) || joined(t)).collect();
        assert!(left.is_empty(), "the studio's scene frame: {left:?}");
    }
    // The chart editor and everything it opens, wide and on a phone, on
    // both shells' wording.
    for (design, size) in [(DESIGN, [1280.0, 720.0]), (COMPACT, [844.0, 390.0])] {
        let mut found = BTreeSet::new();
        let harness = || Harness::new(design, size, 1.0, TextMode::Own);
        for platform in [Platform::Web, Platform::Desktop] {
            let mut env = env.clone();
            env.platform = platform;
            editor_walk(&env, &harness, true, &mut found);
        }
        assert!(found.is_empty(), "the editor at {size:?}: {found:?}");
    }
    ddi_l10n::set_language("en");
}

#[test]
fn the_editor_speaks_japanese() {
    storage::install(Rc::new(MemoryStore::default()));
    ddi_l10n::set_language("ja");
    let mut env = env();
    env.platform = Platform::Desktop;
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(sample::editor_screen(&env, false));
    h.run(0.5);
    let texts = h.texts();
    for t in [
        "編集 · Some Things Must",
        "スナップ 16分 · タップ / ホールド",
        "停止 0.5秒",
        "現在位置：",
        "選択した拍：2–4",
    ] {
        assert!(texts.iter().any(|s| s == t), "{t}: {texts:?}");
    }
    h.ui.broadcast(
        Message::new(super::editor::TIMING).with("Action", Param::Str("seg:speed".into())),
    );
    h.run(0.5);
    let texts = h.texts();
    assert!(texts.iter().any(|s| s == "4拍目のスピード"), "{texts:?}");
    assert!(texts.iter().any(|s| s == "‹  倍率：1.5  ›"), "{texts:?}");
    ddi_l10n::set_language("en");
}

#[test]
fn a_new_language_rebuilds_the_screens_in_place() {
    storage::install(Rc::new(MemoryStore::default()));
    ddi_l10n::set_language("en");
    let env = env();
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(sample::start_screen(&env, true));
    h.run(0.5);
    assert!(h.texts().iter().any(|t| t == "Singleplayer"));
    ddi_l10n::set_language("ja");
    h.ui.rebuild_all();
    h.run(0.1);
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "シングルプレイ"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "Singleplayer"), "{texts:?}");
    ddi_l10n::set_language("en");
}

#[test]
fn the_language_row_offers_pseudo_languages_with_the_debug_overlay() {
    storage::install(Rc::new(MemoryStore::default()));
    ddi_l10n::set_language("en");
    let find = |model: &OptionsModel| {
        model
            .data()
            .sections
            .into_iter()
            .flat_map(|s| s.options)
            .find(|o| o.id == "language")
            .expect("the Language row")
    };
    let mut model = OptionsModel::new(Settings::default(), World::default());
    let row = find(&model);
    let values: Vec<String> = row.values().to_vec();
    // A preview is offered to everyone.
    assert_eq!(values, ["As the system (English)", "English", "日本語"]);
    assert_eq!(row.index, 0);
    model.settings.debug = true;
    let values: Vec<String> = find(&model).values().to_vec();
    assert_eq!(
        values,
        [
            "As the system (English)",
            "English",
            "日本語",
            "Ƥšéúðó-áççéñţš",
            "Ｐｓｅｕｄｏ－ｗｉｄｅ"
        ]
    );
    model.change("language", 3);
    assert_eq!(model.settings.language.as_deref(), Some("qps-ploc"));
    // Chosen, it stays offered with the overlay off.
    model.settings.debug = false;
    let row = find(&model);
    assert_eq!(
        row.values().last().map(String::as_str),
        Some("Ƥšéúðó-áççéñţš")
    );
    assert_eq!(row.index, 3);
    model.change("language", 2);
    assert_eq!(model.settings.language.as_deref(), Some("ja"));
    model.change("language", 0);
    assert_eq!(model.settings.language, None);
}

#[test]
fn styles_and_lanes_are_named_by_their_messages() {
    use super::data::{lane_name, style_name};
    ddi_l10n::set_language("en");
    assert_eq!(&*style_name("dance-single", "Single"), "Single");
    assert_eq!(&*style_name("danoni-9A", "9A keys"), "9A keys");
    assert_eq!(&*style_name("generic-3", "3 lanes"), "3 lanes");
    assert_eq!(&*style_name("kb7-single", "kb7"), "kb7");
    let single = ddi_chart::Layout::builtin("dance-single").unwrap();
    let names: Vec<String> = (0..4).map(|i| lane_name(&single, i).to_string()).collect();
    assert_eq!(names, ["Left", "Down", "Up", "Right"]);
    // The pads of a two-pad style, as the chart crate labels them.
    for id in ["dance-double", "pump-halfdouble", "pump-double"] {
        let layout = ddi_chart::Layout::builtin(id).unwrap();
        for (i, lane) in layout.lanes.iter().enumerate() {
            assert_eq!(&*lane_name(&layout, i), lane.label(), "{id}");
        }
    }
    let pump = ddi_chart::Layout::builtin("pump-single").unwrap();
    let names: Vec<String> = (0..5).map(|i| lane_name(&pump, i).to_string()).collect();
    assert_eq!(
        names,
        ["Down-left", "Up-left", "Center", "Up-right", "Down-right"]
    );
    // Dancing☆Onigiri's lanes as the chart crate labels them in English.
    for mode in ["5", "7", "9A", "9B"] {
        let layout = ddi_chart::Layout::builtin(&format!("danoni-{mode}")).unwrap();
        for (i, lane) in layout.lanes.iter().enumerate() {
            assert_eq!(&*lane_name(&layout, i), lane.label());
        }
    }
    ddi_l10n::set_language("ja");
    assert_eq!(&*style_name("dance-double", "Double"), "ダブル");
    assert_eq!(&*lane_name(&single, 0), "左");
    let five = ddi_chart::Layout::builtin("danoni-5").unwrap();
    assert_eq!(&*lane_name(&five, 4), "5 · おにぎり");
    ddi_l10n::set_language("en");
}

#[test]
fn chart_versions_are_named_by_their_messages() {
    use ddi_chart::simplify::Derivation;
    let versions = [
        Derivation::WithoutOnigiri { source: 0 },
        Derivation::WithoutCenter { source: 0 },
        Derivation::FromPumpSingle { source: 0 },
        Derivation::FromFiveKeys { source: 0 },
    ];
    ddi_l10n::set_language("en");
    for d in &versions {
        assert!(!super::select::version_name(d.kind()).is_empty());
    }
    assert!(ddi_l10n::set_language("qps-ploc"));
    for d in &versions {
        let name = super::select::version_name(d.kind());
        assert!(name.starts_with('«'), "{name}");
    }
    ddi_l10n::set_language("en");
}
