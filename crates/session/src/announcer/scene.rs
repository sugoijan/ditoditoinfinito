//! The announcer studio's scenes on song select and the results
//! (`docs/plans/announcer-studio.md`, step 3): what the game's own screens
//! are shown with so that a line is heard where the game speaks it, the
//! real announcer choosing what speaks.
//!
//! - **Song select** ([`menu`]): the bundled songs, a few of their charts
//!   played before ([`Best`]), so that each comment has a chart that gets
//!   it ([`ddi_ui::screens::select::comment`]); the scene opens on that
//!   chart and chooses it. The welcome's comes from the start menu, as
//!   song select does.
//! - **The results** ([`results`]): a play of a chart made for it, with
//!   the note count of a bundled song's chart, judged by the engine under
//!   a ruleset that gives the line (the player's where it can), each step
//!   pressed as far off as the line wants (a full combo of a tier, a
//!   grade, a fail, a quit), and what the history says of the chart
//!   before it ([`Prior`]). What speaks is [`Announcer::results`]' choice
//!   over them, as after a play. Where the line is about that history (a
//!   record, a first clear), song select shows the chart's best first.
//!
//! Each scene says what it points at as its line is said (a spot of the
//! screen's, [`ddi_ui::Screen::spot`]: the chart's level, the grade),
//! which the studio marks.
//!
//! Staged for the studio alone: nothing is kept.
//!
//! [`Announcer::results`]: super::Announcer::results

use ddi_chart::{
    Chart, Difficulty, DisplayBpm, Layout, Note, NoteKind, Song, SourceFormat, SourceInfo, Tick,
    TimingMap,
};
use ddi_engine::rules::{
    FailPolicy, FullCombo, GradeTable, Judgement, Ruleset, ScoreView, presets,
};
use ddi_engine::{InputEvent, PlayOptions, Player, Results, autoplay};
use ddi_library::announcer::{Comment, Grade, Line};
use ddi_platform::{ClockSample, HostTime};
use ddi_ui::Mark;
use ddi_ui::screens::data::{Best, ChartEntry, ResultsData, SelectData, SongEntry};
use ddi_ui::screens::play::{PlayInfo, results_data};
use ddi_ui::screens::sample;
use ddi_ui::screens::stage::{Caption, CheerAlso};
use ddi_ui::state::history::{score_text, score_value};

use super::{Ending, Prior, grade_order, old_grade};

/// The style the scenes show.
const STYLE: &str = "dance-single";

/// Milliseconds in a day: when the charts played before were played.
const DAY_MS: f64 = 86_400_000.0;

/// Song select's scene: what it shows and where it opens.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuScene {
    pub data: SelectData,
    /// The song shown at first and its chart (an index of its charts).
    pub song: String,
    pub chart: usize,
    /// The chart is chosen (a comment's scene); the intro's only shows.
    pub choose: bool,
    /// What happens (the frame's caption).
    pub caption: Caption,
    /// The start menu shows first, Singleplayer chosen on it (the
    /// welcome's scene): what happens there.
    pub start: Option<Caption>,
    /// What is pointed at as the line is said: song select's spot (the
    /// screen, the chart, its level, its best).
    pub mark: Mark,
}

/// Each comment's chart in Single: a song and a difficulty.
type Pick = (&'static str, Difficulty);
/// Never played: new (level 10, so new comes before hard).
const NEW: Pick = ("sweeteners", Difficulty::Medium);
/// Level 12, played a day before.
const HARD: Pick = ("into-my-dream", Difficulty::Hard);
/// Level 10, played this session.
const REPEAT: Pick = ("some-things-must", Difficulty::Medium);
/// Level 7, played a day before.
const GENERAL: Pick = ("beatucada", Difficulty::Medium);

fn chart_of(songs: &mut [SongEntry], (song, difficulty): Pick) -> Option<&mut ChartEntry> {
    songs
        .iter_mut()
        .find(|s| s.id == song)?
        .charts
        .iter_mut()
        .find(|c| c.style == STYLE && c.difficulty == difficulty)
}

/// Song select's scene for `line` (`None`: not one of its lines), in a
/// session that began at `started` and at `now` (milliseconds since the
/// Unix epoch, as [`ddi_platform::storage::started_ms`]).
pub fn menu(line: Line, started: f64, now: f64) -> Option<MenuScene> {
    let (pick, choose, caption, mark) = match line {
        Line::SelectMusicIntro => (GENERAL, false, Caption::SongSelect, Mark::Screen),
        Line::SelectMusic(c @ Comment::New) => (NEW, true, Caption::Chosen(c), Mark::ChartBest),
        Line::SelectMusic(c @ Comment::Hard) => (HARD, true, Caption::Chosen(c), Mark::ChartLevel),
        Line::SelectMusic(c @ Comment::Repeat) => {
            (REPEAT, true, Caption::Chosen(c), Mark::ChartBest)
        }
        Line::SelectMusic(c @ Comment::General) => (GENERAL, true, Caption::Chosen(c), Mark::Chart),
        _ => return None,
    };
    let mut songs = sample::bundled();
    let before = (started - DAY_MS).max(0.0);
    for (at, last, grade, score, lamp) in [
        (HARD, before, "A", "84.12%", ("Clear", "CLEAR")),
        (GENERAL, before, "S", "92.40%", ("FullCombo", "FC")),
        (REPEAT, now, "B+", "77.10%", ("Clear", "CLEAR")),
    ] {
        if let Some(c) = chart_of(&mut songs, at) {
            c.best = Some(Best {
                grade: grade.into(),
                score: score.into(),
                lamp: lamp.0.into(),
                lamp_text: lamp.1.into(),
                pending: false,
                plays: 2,
                play: String::new(),
                last_played: last,
            });
        }
    }
    let mut scene = select_on(songs, pick, caption, mark)?;
    scene.choose = choose;
    if line == Line::SelectMusicIntro {
        scene.start = Some(Caption::Start);
    }
    Some(scene)
}

/// Song select over `songs`, opening on `pick`'s chart, which is chosen.
fn select_on(
    mut songs: Vec<SongEntry>,
    pick: Pick,
    caption: Caption,
    mark: Mark,
) -> Option<MenuScene> {
    let chart = chart_of(&mut songs, pick)?.index;
    let styles = sample::styles()
        .into_iter()
        .filter(|s| {
            songs
                .iter()
                .any(|song| song.charts.iter().any(|c| c.style == s.id))
        })
        .collect();
    Some(MenuScene {
        data: SelectData {
            styles,
            songs,
            preferred: Some(pick.1),
            resume: Some((pick.0.into(), Some(chart))),
            style: Some(STYLE.into()),
            ..SelectData::default()
        },
        song: pick.0.into(),
        chart,
        choose: true,
        caption,
        start: None,
        mark,
    })
}

/// The results' scene: the play staged, and what it is shown and told
/// with.
#[derive(Debug)]
pub struct ResultsScene {
    /// The play as the engine judged it.
    pub results: Results,
    /// What the history says of the chart before it.
    pub prior: Prior,
    /// The ruleset's grades, and whether it keeps the combo on its
    /// fourth judgment ([`Announcer::results`]' `combo_w4`).
    ///
    /// [`Announcer::results`]: super::Announcer::results
    pub grades: GradeTable,
    pub combo_w4: bool,
    pub data: ResultsData,
    /// What happens (the frame's caption).
    pub caption: Caption,
    /// Song select first, showing the chart's best before the play (a
    /// record's scene, a first clear's), the chart chosen on it.
    pub before: Option<MenuScene>,
    /// What is pointed at as the line is said: the results' spot (the
    /// grade, the score, the lamp, FAILED).
    pub mark: Mark,
}

impl ResultsScene {
    /// What the results' line is told from.
    pub fn ending(&self) -> Ending<'_> {
        Ending::of(&self.results)
    }
}

/// The song the results are of: the bundled Some Things Must, its Hard
/// chart in Single.
const RESULTS_SONG: &str = "some-things-must";
const RESULTS_CHART: Difficulty = Difficulty::Hard;
/// Beats before the staged chart's first step.
const LEAD_IN: i64 = 8;
/// The staged presses' mean error, seconds (a little early, as most
/// players are).
const MEAN: f64 = -0.002;

/// The chart staged plays are of: `steps` steps on eighths at `bpm`
/// across the four lanes (none twice running), every sixteenth a short
/// hold.
fn staged_song(bpm: f64, steps: u32, meter: u32) -> Song {
    const LANES: [u8; 8] = [0, 1, 3, 2, 1, 0, 2, 3];
    let notes = (0..i64::from(steps))
        .map(|i| {
            let tick = Tick(LEAD_IN * 48 + i * 24);
            let kind = if i % 16 == 7 {
                NoteKind::HoldHead {
                    end: Tick(tick.0 + 24),
                }
            } else {
                NoteKind::Tap
            };
            Note::new(tick, LANES[(i % 8) as usize], kind)
        })
        .collect();
    Song {
        title: "Staged".into(),
        subtitle: String::new(),
        artist: String::new(),
        title_translit: String::new(),
        subtitle_translit: String::new(),
        artist_translit: String::new(),
        genre: String::new(),
        credit: String::new(),
        music: None,
        preview_start: 0.0,
        preview_length: 0.0,
        banner: None,
        background: None,
        jacket: None,
        cd_title: None,
        timing: TimingMap::constant(bpm, 0.0),
        display_bpm: DisplayBpm::Actual,
        charts: vec![Chart {
            derivation: None,
            layout: STYLE.into(),
            difficulty: RESULTS_CHART,
            meter,
            name: String::new(),
            description: String::new(),
            credit: String::new(),
            notes,
            timing: None,
            display_bpm: None,
            danoni: None,
            ffr: false,
        }],
        effects: Vec::new(),
        keysounds: Vec::new(),
        layouts: Vec::new(),
        source: SourceInfo {
            format: SourceFormat::Other("staged".into()),
            unknown_tags: Vec::new(),
        },
    }
}

/// The steps' timing errors, in standard deviations: the same each time,
/// so a scene is.
fn errors(n: usize) -> Vec<f64> {
    let mut s: u64 = 0x9e37_79b9_7f4a_7c15;
    (0..n)
        .map(|_| {
            // Irwin–Hall: twelve uniforms less six, about a normal.
            let mut sum = 0.0;
            for _ in 0..12 {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                sum += (s >> 11) as f64 / (1u64 << 53) as f64;
            }
            sum - 6.0
        })
        .collect()
}

/// A staged play: the chart, its steps' errors, and the ruleset each
/// play of it is judged by.
struct Stage {
    song: Song,
    errors: Vec<f64>,
    id: String,
}

/// How far off each step is pressed.
#[derive(Clone, Copy)]
enum Presses {
    /// At skill `x` (0: as close as can be; 1: far off, many missed):
    /// the mean error plus `x`'s spread times the step's error; a step
    /// past the widest window is missed, and so is step `miss` (one at
    /// least, so no full combo).
    Level { x: f64, miss: Option<usize> },
    /// Every step inside judgment `tier`'s window, a good share of them
    /// past the one before (a full combo of that tier).
    Within(Judgement),
    /// At skill `x` up to step `until`, every step after missed.
    Until { x: f64, until: usize },
}

impl Stage {
    fn new(id: &str) -> Stage {
        let entry = sample::bundled()
            .into_iter()
            .find(|s| s.id == RESULTS_SONG)
            .unwrap_or_default();
        let chart = entry
            .charts
            .iter()
            .find(|c| c.style == STYLE && c.difficulty == RESULTS_CHART);
        let steps = chart.map_or(400, |c| c.notes);
        let meter = chart.map_or(10, |c| c.meter);
        Stage {
            song: staged_song(f64::from(entry.bpm.max(60.0)), steps, meter),
            errors: errors(steps as usize),
            id: id.into(),
        }
    }

    /// The ruleset, failing as `fail` says (its own where `None`).
    fn rules(&self, fail: Option<FailPolicy>) -> Option<Ruleset> {
        let mut r = presets::by_id(&self.id)?;
        if let Some(f) = fail {
            r.fail = f;
        }
        Some(r)
    }

    /// The play judged by `rules`, pressed as `presses` says; quit at
    /// step `quit`'s time.
    fn play(&self, rules: Ruleset, presses: Presses, quit: Option<usize>) -> Results {
        let windows: Vec<Option<f64>> = Judgement::ALL[..5]
            .iter()
            .map(|&j| rules.judge.window(j).map(|w| w.early.min(w.late)))
            .collect();
        let window = |j: Judgement| windows.get(j.index()).copied().flatten();
        let widest = windows.iter().flatten().copied().fold(0.0, f64::max);
        let offset = |i: usize| -> Option<f64> {
            let z = self.errors.get(i).copied().unwrap_or(0.0);
            let level = |x: f64| {
                let d = MEAN + (0.004 + x * 0.12) * z;
                (d.abs() <= widest * 0.98).then_some(d)
            };
            match presses {
                Presses::Level { miss, .. } if miss == Some(i) => None,
                Presses::Level { x, .. } => level(x),
                Presses::Within(tier) => {
                    let limit = window(tier)? * 0.9;
                    Some((MEAN + limit * 0.5 * z).clamp(-limit, limit))
                }
                Presses::Until { x, until } => (i < until).then(|| level(x)).flatten(),
            }
        };
        let mut player = Player::new(&self.song, 0, rules, PlayOptions::default());
        let mut steps: Vec<(f64, u8)> = player
            .judge()
            .notes()
            .iter()
            .filter(|n| matches!(n.note.kind, NoteKind::Tap | NoteKind::HoldHead { .. }))
            .map(|n| (n.seconds, n.note.lane))
            .collect();
        steps.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut script: Vec<Option<autoplay::Press>> =
            autoplay::script(player.judge().notes(), 0.0)
                .into_iter()
                .map(Some)
                .collect();
        for (i, &(at, lane)) in steps.iter().enumerate() {
            let Some(p) = script
                .iter()
                .position(|p| p.is_some_and(|p| p.lane == lane && p.press == at))
            else {
                continue;
            };
            match offset(i) {
                Some(d) => {
                    if let Some(p) = script[p].as_mut() {
                        p.press += d;
                        p.release += d;
                    }
                }
                None => script[p] = None,
            }
        }
        let mut edges = autoplay::edges(script.into_iter().flatten());
        autoplay::sort_edges(&mut edges);
        player.clock_sample(ClockSample {
            context_time: 0.0,
            host_time: HostTime(0.0),
            output_latency: 0.0,
        });
        player.start(0.0, 0.0);
        let stop = quit.and_then(|q| steps.get(q)).map(|s| s.0);
        for (t, lane, pressed) in edges {
            if stop.is_some_and(|s| t >= s) || player.finished() {
                break;
            }
            player.update(HostTime(t));
            player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t),
            });
        }
        match stop {
            Some(s) => {
                player.update(HostTime(s));
                player.quit_at(s);
            }
            None => {
                let end = steps.last().map_or(0.0, |s| s.0) + 4.0;
                player.update(HostTime(end));
            }
        }
        player.results()
    }

    /// A play graded `grade`, with one step missed at least (no full
    /// combo), its skill in the middle of the grade's: `None` where no
    /// skill gives it.
    fn graded(&self, grade: &str, fail: Option<FailPolicy>) -> Option<(Results, Ruleset)> {
        let rules = self.rules(None)?;
        let order = grade_order(&rules.grades);
        let want = rank(&order, grade);
        let miss = Some(self.errors.len() * 3 / 5);
        let at = |x: f64| -> Option<(usize, Results)> {
            let r = self.play(self.rules(fail)?, Presses::Level { x, miss }, None);
            let g = if r.failed {
                order.len()
            } else {
                rank(&order, &r.grade)
            };
            Some((g, r))
        };
        // The grades fall as the skill does: the first skill as bad as
        // the grade wanted, the first worse.
        let first = |worse_than: usize| -> Option<f64> {
            if at(0.0)?.0 > worse_than {
                return Some(0.0);
            }
            if at(1.0)?.0 <= worse_than {
                return Some(1.0);
            }
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..12 {
                let mid = (lo + hi) / 2.0;
                if at(mid)?.0 > worse_than {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            Some(hi)
        };
        let from = if want == 0 { 0.0 } else { first(want - 1)? };
        let to = first(want)?;
        for x in [(from + to) / 2.0, from] {
            let (g, r) = at(x)?;
            if g == want && !r.failed {
                return Some((r, self.rules(fail)?));
            }
        }
        None
    }
}

fn rank(order: &[&str], name: &str) -> usize {
    order.iter().position(|n| *n == name).unwrap_or(order.len())
}

/// The full combo a tier's scene gets.
fn full_combo_of(tier: u8) -> FullCombo {
    match tier {
        1 => FullCombo::MarvelousFC,
        2 => FullCombo::PerfectFC,
        3 => FullCombo::GreatFC,
        _ => FullCombo::FC,
    }
}

fn tier(n: u8) -> Judgement {
    Judgement::ALL[usize::from(n.clamp(1, 5)) - 1]
}

/// What the history says before a play the line is told for: played and
/// cleared, the best out of reach (no record, no first clear).
fn known() -> Prior {
    Prior {
        played: true,
        cleared: true,
        best: f64::MAX,
    }
}

/// The results' scene for `line` (`None`: not a results line, or one no
/// play gets: `evaluation aaaaa`): judged by ruleset `ruleset` (the
/// player's, an id of `presets::by_id`) where it can give the line, else
/// ITG, else DDR A. `has`: whether the draft has a recording on a line
/// (the cheer's scene ends on a grade it has).
pub fn results(line: Line, ruleset: &str, has: &dyn Fn(Line) -> bool) -> Option<ResultsScene> {
    let mut ids: Vec<&str> = Vec::new();
    for id in [ruleset, "itg", "ddr-a"] {
        if id != "ffr" && presets::by_id(id).is_some() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids.into_iter().find_map(|id| staged(line, id, has))
}

/// [`results`] under ruleset `id`.
fn staged(line: Line, id: &str, has: &dyn Fn(Line) -> bool) -> Option<ResultsScene> {
    let stage = Stage::new(id);
    let rules = stage.rules(None)?;
    let names = rules.names.clone();
    let steps = stage.errors.len();
    let level = |x: f64| Presses::Level {
        x,
        miss: Some(steps * 3 / 5),
    };
    // Each scene's play and prior, the ruleset it was judged by, what
    // happens, what is pointed at, and the chart's best shown before
    // (as cleared, or failed).
    let mut before = None;
    let mut mark = Mark::Grade;
    let (results, prior, rules, caption) = match line {
        Line::Quit => {
            // A note missed on the way, as before most quits.
            let presses = Presses::Level {
                x: 0.2,
                miss: Some(steps / 4),
            };
            let r = stage.play(rules, presses, Some(steps * 2 / 5));
            (r, known(), stage.rules(None)?, Caption::Quit)
        }
        Line::FirstClear => {
            let r = stage.play(rules, level(0.2), None);
            if r.failed {
                return None;
            }
            // Failed before, with less.
            let prior = Prior {
                played: true,
                cleared: false,
                best: score_value(&scaled(&r.score, FAILED_BEFORE)),
            };
            before = Some((FAILED_BEFORE, true));
            (r, prior, stage.rules(None)?, Caption::FirstClear)
        }
        Line::NewRecord => {
            let r = stage.play(rules, level(0.12), None);
            if r.failed {
                return None;
            }
            let prior = Prior {
                played: true,
                cleared: true,
                best: score_value(&scaled(&r.score, BEST_BEFORE)),
            };
            before = Some((BEST_BEFORE, false));
            mark = Mark::Score;
            let caption = Caption::NewRecord {
                score: score_text(&r.score),
            };
            (r, prior, stage.rules(None)?, caption)
        }
        Line::EvaluationFullCombo(n) => {
            let j = tier(n);
            rules.judge.window(j)?;
            if n == 4 && rules.combo.continue_min != Judgement::W4 {
                return None;
            }
            let r = stage.play(rules, Presses::Within(j), None);
            if r.full_combo != full_combo_of(n) || r.failed {
                return None;
            }
            mark = Mark::Lamp;
            let caption = Caption::FullCombo {
                judgment: names.name(j).trim().to_string(),
                best: n == 1,
            };
            (r, known(), stage.rules(None)?, caption)
        }
        Line::Grade(Grade::E | Grade::F) => {
            let fail = match rules.fail {
                FailPolicy::Immediate | FailPolicy::ImmediateContinue => None,
                _ => Some(FailPolicy::Immediate),
            };
            let r = stage.play(
                stage.rules(fail)?,
                Presses::Until {
                    x: 0.15,
                    until: steps * 7 / 20,
                },
                None,
            );
            if !r.failed {
                return None;
            }
            mark = Mark::Failed;
            (r, known(), stage.rules(fail)?, Caption::Failed)
        }
        Line::Grade(Grade::Aaaaa) => return None,
        Line::Grade(g) => {
            let (r, rules) = graded(&stage, &rules.grades, g)?;
            let caption = Caption::Graded {
                grade: r.grade.clone(),
            };
            (r, known(), rules, caption)
        }
        Line::Cheer => {
            // It follows the results' line of a play graded AA or better:
            // the first such grade the draft says; else a new record or
            // a first clear it says, of an AA play.
            let said = [Grade::Aa, Grade::Aaa, Grade::Aaaa]
                .into_iter()
                .find(|g| Line::Grade(*g).chain().into_iter().any(has));
            let (r, rules) = graded(&stage, &rules.grades, said.unwrap_or(Grade::Aa))?;
            let best = score_value(&scaled(&r.score, BEST_BEFORE));
            let (prior, also) = match said {
                Some(_) => (known(), CheerAlso::Nothing),
                None if has(Line::NewRecord) => (Prior { best, ..known() }, CheerAlso::NewRecord),
                None if has(Line::FirstClear) => (
                    Prior {
                        played: true,
                        cleared: false,
                        best,
                    },
                    CheerAlso::FirstClear,
                ),
                None => (known(), CheerAlso::Nothing),
            };
            let caption = Caption::Cheer {
                grade: r.grade.clone(),
                also,
            };
            (r, prior, rules, caption)
        }
        _ => return None,
    };
    let entry = sample::bundled()
        .into_iter()
        .find(|s| s.id == RESULTS_SONG)
        .unwrap_or_default();
    let meter = stage.song.charts[0].meter;
    let layout = Layout::builtin(STYLE)?;
    let data = results_data(&PlayInfo {
        results: &results,
        ruleset: &rules,
        layout: &layout,
        song: entry,
        chart: ddi_ui::screens::prompt::chart_line(
            RESULTS_CHART,
            meter,
            &ddi_ui::screens::data::style_name(&layout.id, &layout.name),
        ),
        mods: Vec::new(),
        device_changed: false,
        video_notes: Vec::new(),
        offset: None,
        panels: String::new(),
        autoplay: false,
    });
    let before = match before {
        None => None,
        Some((k, failed)) => {
            // The chart as song select shows it before the play: its best.
            let view = scaled(&results.score, k);
            let mut songs = sample::bundled();
            let c = chart_of(&mut songs, (RESULTS_SONG, RESULTS_CHART))?;
            let (lamp, lamp_text) = if failed {
                ("Failed", "FAILED")
            } else {
                ("Clear", "CLEAR")
            };
            c.best = Some(Best {
                grade: rules.grades.grade(&view, &results.tally, failed),
                score: score_text(&view),
                lamp: lamp.into(),
                lamp_text: lamp_text.into(),
                pending: false,
                plays: 3,
                play: String::new(),
                last_played: 0.0,
            });
            let caption = if failed {
                Caption::BeforeFailed
            } else {
                Caption::BeforeBest {
                    score: score_text(&view),
                }
            };
            select_on(
                songs,
                (RESULTS_SONG, RESULTS_CHART),
                caption,
                Mark::ChartBest,
            )
        }
    };
    Some(ResultsScene {
        results,
        prior,
        grades: rules.grades.clone(),
        combo_w4: rules.combo.continue_min == Judgement::W4,
        data,
        caption,
        before,
        mark,
    })
}

/// A record's scene's best before: this much of the play's score.
const BEST_BEFORE: f64 = 0.98;
/// A first clear's: the fails before scored this much of it.
const FAILED_BEFORE: f64 = 0.9;

/// `v` times `k` (a best below it), as the history keeps a score.
fn scaled(v: &ScoreView, k: f64) -> ScoreView {
    ScoreView {
        money: v.money.map(|m| (m as f64 * k).round() as u64),
        percent: v.percent.map(|p| p * k),
        grade_percent: v.grade_percent.map(|p| p * k),
        ex: v.ex.map(|e| (f64::from(e) * k).round() as u32),
        max_ex: v.max_ex,
        ex_percent: v.ex_percent.map(|p| p * k),
    }
}

/// A play whose grade is `g` in `table`'s old seven: the first of its
/// names a play can get, failing as the ruleset does, else with failing
/// off.
fn graded(stage: &Stage, table: &GradeTable, g: Grade) -> Option<(Results, Ruleset)> {
    let names: Vec<&str> = grade_order(table)
        .into_iter()
        .filter(|n| old_grade(table, n, false) == Some(g))
        .collect();
    for fail in [None, Some(FailPolicy::Off)] {
        for n in &names {
            if let Some(found) = stage.graded(n, fail) {
                return Some(found);
            }
        }
    }
    None
}

pub mod in_play;

#[cfg(test)]
mod tests;
