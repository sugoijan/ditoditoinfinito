//! The announcer studio's scenes in play (`docs/plans/announcer-studio.md`,
//! step 4): for each line said during a play, a bundled chart played as
//! a staged play ([`Staging`]: from a few seconds before the moment, what
//! came before given, chosen notes missed or pressed off), so that the
//! line is said by the real announcer over the real play, the rest of
//! what it says with it.
//!
//! - [`pick`]: the chart and what of the settings the scene changes (a
//!   battery gauge, failing at once, DDR A's rules), before it loads.
//! - [`stage`]: the play once the chart is loaded, under the rules and
//!   options it is played with.

use std::collections::HashMap;

use ddi_chart::{Difficulty, NoteKind, Song};
use ddi_engine::autoplay;
use ddi_engine::player::{PlayOptions, Player};
use ddi_engine::rules::{FailType, Judgement, LifeType, Ruleset, presets};
use ddi_library::announcer::Line;
use ddi_ui::screens::stage::Caption;
use ddi_ui::state::settings::Settings;

use crate::announcer::{COMBO_GAP, COMBO_STOPPED_AT, COMMENT_GAP, GO_BEFORE};
use crate::play::START_LOOKAHEAD;
use crate::play::staging::{self, Staging, Touch};

/// Seconds a scene goes on after its line is due.
const TAIL: f64 = 1.5;
/// Seconds of play before a scene's moment.
const LEAD: f64 = 2.5;
/// The length of a line's sound where it is not known, seconds.
const UNKNOWN: f64 = 1.5;
/// Seconds here we go's scene leaves before it: room for a first frame
/// drawn late, none for a ready.
const GO_SPARE: f64 = 0.4;

/// The chart a play line's scene is on, and what it plays it with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick {
    /// A bundled song, and its chart of this difficulty in Single.
    pub song: &'static str,
    pub difficulty: Difficulty,
    /// StepMania's battery with this many lives (the oni comment, a life
    /// lost, the last life).
    pub battery: Option<u8>,
    /// The life running out fails the play at once (danger's scene, the
    /// fail's).
    pub fail_at_once: bool,
    /// The rules played by (`ddr-a`: a plain full combo is DDR A's);
    /// `None`: the player's.
    pub ruleset: Option<&'static str>,
}

impl Pick {
    /// The settings its play is played with, from the player's: their
    /// rules where they are StepMania's (ITG where they are another
    /// game's; the pick's own where it has one), the gauge it wants.
    pub fn settings(&self, settings: &Settings) -> Settings {
        let mut s = settings.clone();
        if presets::by_id(&s.ruleset).is_none_or(|r| !r.is_stepmania() || r.is_ffr()) {
            s.ruleset = "itg".into();
        }
        if let Some(id) = self.ruleset {
            s.ruleset = id.into();
        }
        s.stepmania.life_type = match self.battery {
            Some(lives) => {
                s.stepmania.lives = lives;
                LifeType::Battery
            }
            None => LifeType::Bar,
        };
        if self.fail_at_once {
            s.stepmania.fail = FailType::Immediate;
        }
        s
    }
}

/// The style the scenes are played in.
pub const STYLE: &str = "dance-single";

/// Each scene's song: Beatucada's Medium, 217 steps from 7.9 s.
const SONG: (&str, Difficulty) = ("beatucada", Difficulty::Medium);

/// The scene of play line `line` (`None`: not one said in play).
pub fn pick(line: Line) -> Option<Pick> {
    let mut p = Pick {
        song: SONG.0,
        difficulty: SONG.1,
        battery: None,
        fail_at_once: false,
        ruleset: None,
    };
    match line {
        Line::Ready
        | Line::HereWeGo
        | Line::Restart
        | Line::CommentHot
        | Line::CommentGood
        | Line::Combo(_)
        | Line::ComboOverflow
        | Line::ComboStopped
        | Line::FullComboW(_)
        | Line::Cleared => {}
        Line::CommentOni | Line::LifeLost | Line::LastLife => p.battery = Some(4),
        Line::FullCombo => p.ruleset = Some("ddr-a"),
        // A play that can fail: in danger, and failed at once.
        Line::CommentDanger | Line::Failed => p.fail_at_once = true,
        _ => return None,
    }
    Some(p)
}

/// A play line's scene, staged.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayScene {
    pub staging: Staging,
    /// The play again after the restart (the restart's scene).
    pub restarted: Option<Staging>,
    pub caption: Caption,
    /// It plays on to the end and into the results.
    pub results: bool,
}

/// The judged steps' seconds (taps, holds' and rolls' heads, lifts), in
/// order, and the song second at which each makes the combo go up (one
/// a step, or a row under per-row combo).
struct Steps {
    at: Vec<f64>,
    combos: Vec<f64>,
}

impl Steps {
    fn of(player: &Player) -> Steps {
        let mut at: Vec<f64> = player
            .judge()
            .notes()
            .iter()
            .filter(|n| {
                n.judgeable
                    && matches!(
                        n.note.kind,
                        NoteKind::Tap
                            | NoteKind::HoldHead { .. }
                            | NoteKind::RollHead { .. }
                            | NoteKind::Lift
                    )
            })
            .map(|n| n.seconds)
            .collect();
        at.sort_by(f64::total_cmp);
        let mut combos = at.clone();
        if player.ruleset().combo.per_row {
            combos.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        }
        Steps { at, combos }
    }

    /// The first step at or after `t`.
    fn after(&self, t: f64) -> Option<f64> {
        self.at.iter().copied().find(|&s| s >= t)
    }

    /// The first step at or after `t` alone at its second (no jump: one
    /// miss there costs one life).
    fn lone_after(&self, t: f64) -> Option<f64> {
        self.at
            .iter()
            .copied()
            .find(|&s| s >= t && self.at.iter().filter(|&&o| (o - s).abs() < 1e-6).count() == 1)
    }

    /// The combo's rises in `from..to`.
    fn rises(&self, from: f64, to: f64) -> u32 {
        self.combos.iter().filter(|&&s| s >= from && s < to).count() as u32
    }
}

/// Song second `at` minus `lead`, a little before a step, never before
/// the song.
fn before(at: f64, lead: f64) -> f64 {
    (at - lead).max(0.0)
}

/// What a simulated play is at a moment.
struct At {
    life: f32,
    danger: bool,
    failed: bool,
}

/// A play of `song`'s chart `chart` under `rules` and `options`, staged
/// as `s` and played on to `until`: its life then, in danger or not,
/// failed or not.
fn simulate(
    song: &Song,
    chart: usize,
    rules: &Ruleset,
    options: PlayOptions,
    s: &Staging,
    until: f64,
) -> At {
    let mut p = Player::new(song, chart, rules.fresh(), options);
    let presses = staging::touched(autoplay::script(p.judge().notes(), 0.0), &s.touches);
    let mut edges = autoplay::edges(presses);
    autoplay::sort_edges(&mut edges);
    p.start_staged(0.0, s.from, &edges, s.combo);
    for &(t, lane, pressed) in edges.iter().filter(|e| e.0 >= s.from && e.0 < until) {
        p.update_at(t);
        p.input_at(lane, pressed, t);
    }
    p.update_at(until);
    At {
        life: p.ruleset().gauge.life(),
        danger: p.ruleset().gauge.danger(),
        failed: p.failed(),
    }
}

/// The results of a scene at a play's end show this long before the
/// studio again, seconds (their line and its cheer said).
pub const RESULTS_SECONDS: f64 = 6.5;

/// The scene of play line `line` on `song`'s chart `chart`, played under
/// `rules` and `options` ([`stage`], `lengths` each line's sound's
/// length), and how long it lasts from its play's start, seconds: to its
/// end, and its results' (the frame's bar).
pub fn staged(
    line: Line,
    song: &Song,
    chart: usize,
    rules: &Ruleset,
    options: PlayOptions,
    lengths: &HashMap<Line, f64>,
) -> (Option<PlayScene>, f64) {
    let length = |l: Line| l.chain().iter().find_map(|c| lengths.get(c).copied());
    let Some(scene) = stage(line, song, chart, rules, options, &length) else {
        return (None, 0.0);
    };
    let end = scene
        .staging
        .until
        .unwrap_or_else(|| Player::new(song, chart, rules.fresh(), options).song_end_time());
    let results = if scene.results { RESULTS_SECONDS } else { 0.0 };
    let seconds = (end - scene.staging.from).max(0.0) / options.clock.rate.max(0.1) + results;
    (Some(scene), seconds)
}

/// The scene of play line `line` on `song`'s chart `chart` (its
/// [`pick`]'s), played under `rules` and `options` (as the play will
/// be), `length` saying how long each line's sound is (its next one's,
/// where the voice has it). `None`: no play gives the line on it.
pub fn stage(
    line: Line,
    song: &Song,
    chart: usize,
    rules: &Ruleset,
    options: PlayOptions,
    length: &dyn Fn(Line) -> Option<f64>,
) -> Option<PlayScene> {
    let probe = Player::new(song, chart, rules.fresh(), options);
    let steps = Steps::of(&probe);
    let first = *steps.at.first()?;
    let last = *steps.at.last()?;
    let len = |l: Line| length(l).unwrap_or(UNKNOWN);
    let mid = steps.at[steps.at.len() * 2 / 5];
    let scene = |staging: Staging, caption: Caption| PlayScene {
        staging,
        restarted: None,
        caption,
        results: false,
    };
    // A play from the song's start, as the announcer opens it: ready
    // and here we go fitting before the first note, with a moment
    // before them.
    let opening = before(
        first,
        GO_BEFORE + len(Line::HereWeGo) + len(Line::Ready) + 1.0,
    );
    Some(match line {
        Line::Ready => scene(
            Staging {
                from: opening,
                until: Some(first + TAIL),
                ..Staging::default()
            },
            Caption::PlayBegins,
        ),
        // No room for ready before here we go: the play opens its
        // lookahead before its start, here we go then fitting with a
        // little to spare (a first frame drawn late takes some), not
        // enough for a ready.
        Line::HereWeGo => scene(
            Staging {
                from: before(
                    first,
                    (GO_BEFORE + len(Line::HereWeGo) + GO_SPARE - START_LOOKAHEAD).max(0.05),
                ),
                until: Some(first + TAIL),
                ..Staging::default()
            },
            Caption::FirstNote,
        ),
        // Played a moment past the first note, then R twice: the play
        // again from the same place, opened by the restart's line.
        Line::Restart => PlayScene {
            staging: Staging {
                from: opening,
                restart: Some(first + 1.0),
                ..Staging::default()
            },
            restarted: Some(Staging {
                from: opening,
                until: Some(first + TAIL),
                ..Staging::default()
            }),
            caption: Caption::Restart,
            results: false,
        },
        // The periodic comment, due a moment after the start.
        Line::CommentHot | Line::CommentGood | Line::CommentDanger | Line::CommentOni => {
            let from = before(mid, 0.4);
            let due = from + LEAD;
            let mut s = Staging {
                from,
                since: COMMENT_GAP - LEAD,
                until: Some(due + len(line) + TAIL),
                ..Staging::default()
            };
            let caption = match line {
                Line::CommentHot => {
                    // Where a play kept full is at full life (a gauge
                    // filling slowly is not, early on).
                    s = steps
                        .at
                        .iter()
                        .filter(|&&t| t >= mid)
                        .step_by(8)
                        .find_map(|&t| {
                            let from = before(t, 0.4);
                            let due = from + LEAD;
                            let s = Staging {
                                from,
                                until: Some(due + len(line) + TAIL),
                                ..s.clone()
                            };
                            let at = simulate(song, chart, rules, options, &s, due);
                            (at.life >= 1.0).then_some(s)
                        })?;
                    Caption::FullLife
                }
                Line::CommentGood => {
                    // Steps missed just before: the life not full again
                    // by the comment.
                    s.touches = vec![Touch::miss(from - 1.0, from)];
                    Caption::BelowFull
                }
                Line::CommentDanger => {
                    // Missed for long enough before the start that the
                    // life is in danger at the comment, not run out.
                    let found = (2..=24).map(|k| f64::from(k) * 0.5).find_map(|x| {
                        let t = Staging {
                            touches: vec![Touch::miss(from - x, from)],
                            ..s.clone()
                        };
                        let at = simulate(song, chart, rules, options, &t, due + 0.2);
                        (at.danger && !at.failed).then_some(t)
                    })?;
                    s = found;
                    Caption::Danger
                }
                _ => Caption::Battery,
            };
            scene(s, caption)
        }
        // The combo crossing a hundred a few steps after the start.
        Line::Combo(_) | Line::ComboOverflow => {
            let n = match line {
                Line::Combo(n) => n,
                _ => 1100,
            };
            let from = before(steps.after(first + 4.0)?, 0.4);
            let rises = steps.rises(from, from + LEAD).max(1);
            let at = steps
                .combos
                .iter()
                .copied()
                .filter(|&s| s >= from)
                .nth(rises as usize - 1)?;
            scene(
                Staging {
                    from,
                    combo: n.checked_sub(rises)?,
                    since: COMBO_GAP,
                    until: Some(at + len(line) + TAIL),
                    ..Staging::default()
                },
                Caption::Combo {
                    count: n,
                    past: n > 1000,
                },
            )
        }
        // A combo of 60 broken by a step missed a moment after the start.
        Line::ComboStopped => {
            let from = before(mid, 0.4);
            let missed = steps.lone_after(from + 1.2)?;
            scene(
                Staging {
                    from,
                    combo: COMBO_STOPPED_AT + 10,
                    since: COMBO_GAP,
                    touches: vec![Touch::miss(missed - 0.01, missed + 0.01)],
                    until: Some(missed + len(line) + TAIL),
                    ..Staging::default()
                },
                Caption::ComboBroken,
            )
        }
        // A battery's life lost by a step missed a moment after the
        // start; the last one, two lost before it.
        Line::LifeLost | Line::LastLife => {
            let from = before(mid, 0.4);
            let missed = steps.lone_after(from + 1.2)?;
            let mut touches = vec![Touch::miss(missed - 0.01, missed + 0.01)];
            if line == Line::LastLife {
                for back in [6.0, 4.0] {
                    let s = steps.lone_after(from - back)?;
                    touches.push(Touch::miss(s - 0.01, s + 0.01));
                }
            }
            scene(
                Staging {
                    from,
                    since: COMBO_GAP,
                    touches,
                    until: Some(missed + len(line) + TAIL),
                    ..Staging::default()
                },
                if line == Line::LastLife {
                    Caption::LastLife
                } else {
                    Caption::LifeLost
                },
            )
        }
        // The last steps, every step pressed into the full combo's
        // window (or a step missed before, for a clear alone); on into the
        // results.
        Line::FullComboW(_) | Line::FullCombo | Line::Cleared => {
            let from = before(last, 3.5);
            let tier = match line {
                Line::FullComboW(n) => Some(n.clamp(1, 3)),
                Line::FullCombo => Some(4),
                _ => None,
            };
            let mut touches = Vec::new();
            let caption = match tier {
                Some(n) => {
                    let j = Judgement::ALL[usize::from(n) - 1];
                    if j > rules.combo.continue_min {
                        return None;
                    }
                    if n > 1 {
                        // Inside the tier's window, past the one before.
                        let window =
                            |j: Judgement| rules.judge.window(j).map(|w| w.early.min(w.late));
                        let inner = window(Judgement::ALL[usize::from(n) - 2])?;
                        let outer = window(j)?;
                        touches.push(Touch::off(f64::MIN, f64::MAX, (inner + outer) / 2.0));
                    }
                    let judgment = rules.names.name(j).trim().to_string();
                    Caption::EndsFullCombo {
                        judgment,
                        best: n == 1,
                    }
                }
                None => {
                    let s = steps.lone_after(from - 8.0)?;
                    touches.push(Touch::miss(s - 0.01, s + 0.01));
                    Caption::Cleared
                }
            };
            PlayScene {
                staging: Staging {
                    from,
                    touches,
                    ..Staging::default()
                },
                restarted: None,
                caption,
                results: true,
            }
        }
        // Every step missed from a while before the start: the life runs
        // out a moment after it; on into the results.
        Line::Failed => {
            let from = before(mid, 0.4);
            let found = (1..=24).rev().map(|k| f64::from(k) * 0.5).find_map(|x| {
                let s = Staging {
                    from,
                    touches: vec![Touch::miss(from - x, f64::MAX)],
                    ..Staging::default()
                };
                let then = simulate(song, chart, rules, options, &s, from);
                let later = simulate(song, chart, rules, options, &s, from + LEAD + 1.0);
                (!then.failed && later.failed).then_some(s)
            })?;
            PlayScene {
                staging: found,
                restarted: None,
                caption: Caption::Failed,
                results: true,
            }
        }
        _ => return None,
    })
}

/// The full combo a play line is for (tests).
#[cfg(test)]
pub(crate) fn full_combo_of(line: Line) -> Option<ddi_engine::rules::FullCombo> {
    use ddi_engine::rules::FullCombo;
    match line {
        Line::FullComboW(1) => Some(FullCombo::MarvelousFC),
        Line::FullComboW(2) => Some(FullCombo::PerfectFC),
        Line::FullComboW(3) => Some(FullCombo::GreatFC),
        Line::FullCombo => Some(FullCombo::FC),
        _ => None,
    }
}
