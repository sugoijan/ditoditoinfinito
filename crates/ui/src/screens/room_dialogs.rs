//! The room's pop-ups (`docs/plans/multiplayer-ui.md`, "Room creation and
//! rules study"; M7b's flow): Rules (the first one entering opens, Reset and
//! OK; from its button later, Cancel and Apply), with practice's CPU roster
//! in it (Add random, Add custom by level or name: [`AddCpuDialog`]); the
//! players and the match's picks ([`PlayersDialog`]), from which an online
//! room's host opens Host options ([`HostDialog`]: what the room does when
//! its host leaves, co-hosts made and removed, the room handed over); a
//! stage's standings and the match's results ([`StandingsDialog`]); and a
//! refusal said ([`NoticeDialog`]). The room validates what is applied;
//! the dialogs only word its answer.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::Actor;
use ddi_l10n::UiText;
use ddi_l10n::msg::{common, room as m};
use ddi_room::{
    Capability, HostLeaves, MatchDifficulty, MatchRules, MemberKind, OpponentSpec, ParticipantId,
    Phase, PointsBias, Role, StageStanding,
};

use super::dialog::{button, fitted_dialog, shell};
use super::room::{
    PanelRow, cpu_level, cpu_name, cpu_sub, level_line_right, mark_text, panel, person_name,
    players_first_rows, publish_dialog,
};
use super::{Env, left, right};
use crate::look;
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::state::cpu_roster::{self, RosterAim};
use crate::state::room::{SharedRoom, cpu_levels};

/// Broadcast when the dialog applied new rules: the room redraws.
pub const RULES_SAVED: &str = "Room.RulesSaved";
/// Broadcast when Add custom changed the Rules pop-up's roster.
const ROSTER_CHANGED: &str = "Room.RosterChanged";
/// Broadcast when the restart's question was answered Restart match.
const RESTART_CONFIRMED: &str = "Room.RestartConfirmed";
/// The restart's question among the layers.
pub const RESTART_QUESTION: &str = "room-restart";

/// The CPU rows the Rules pop-up shows at once (more scroll).
const CPU_ROWS: usize = 4;
const ROW: f32 = 40.0;
/// The row a scrolled roster's "more" mark takes, above and below it.
const MARK: f32 = 24.0;

/// A refusal said in a pop-up (song select picking a song: a chart the
/// CPUs cannot play): its title, its words, Close.
pub struct NoticeDialog {
    env: Env,
    title: String,
    body: String,
}

impl NoticeDialog {
    pub fn new(env: Env, title: &str, body: &str) -> NoticeDialog {
        NoticeDialog {
            env,
            title: title.into(),
            body: body.into(),
        }
    }

    /// A pick refused, as `why` says.
    pub fn refusal(env: Env, why: &crate::state::room::Refusal) -> NoticeDialog {
        let body: String = super::room::refusal_text(why).into();
        let title = match why {
            crate::state::room::Refusal::Chart(_) => m::chart_unsupported_title(),
            _ => m::refused_title(),
        };
        NoticeDialog::new(env, &title, &body)
    }
}

impl Screen for NoticeDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "room-notice"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let env = self.env.clone();
        let w = 480.0_f32.min(screen[0] - 32.0);
        let body = super::dialog::wrap_lines(&self.body, w - 48.0, 14.0);
        let h = 150.0 + body.len() as f32 * 18.0;
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &self.title);
        super::dialog::dialog_body(&env, &mut c, &body, [x, y], w, |_| "Secondary");
        c.push(button(
            &env,
            "Close",
            &common::close(),
            [x + w - 24.0 - 70.0, y + h - 36.0],
            140.0,
            "Button",
        ));
        self.env.state.borrow_mut().page_items = vec!["Close".into()];
        fitted_dialog(c, screen, h, scale)
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Close")]
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Activate { .. } | Input::Outside => {
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// Scrolls a window of `len` rows showing `fit` from `first` by `by`.
fn scrolled(first: usize, by: i32, len: usize, fit: usize) -> usize {
    let max = len.saturating_sub(fit);
    (first as i64 + i64::from(by)).clamp(0, max as i64) as usize
}

/// The defaults Reset restores: practice's rules and starting CPUs (from
/// the room's seed), or a hosted room's (`MatchRules::hosted_for`: Attacks
/// On where the room plays live).
fn defaults(room: &SharedRoom) -> MatchRules {
    let r = room.borrow();
    if r.is_online() {
        return MatchRules::hosted_for(&r.state().policy);
    }
    let seed = r.state().next.rules.cpu.opponent.seed;
    let rules = crate::state::room::practice_rules(OpponentSpec {
        level: crate::state::room::DEFAULT_CPU_LEVEL,
        seed,
        personality: None,
        name: None,
        targeting: None,
    });
    cpu_roster::with_roster(
        rules,
        cpu_roster::starting(crate::state::room::STARTING_CPUS, seed),
        r.humans(),
    )
}

pub struct RulesDialog {
    env: Env,
    room: SharedRoom,
    /// The next match's rules as edited, not yet applied (shared with Add
    /// custom).
    draft: Rc<RefCell<MatchRules>>,
    /// Opened by itself on entering (M7b): Reset and OK.
    first: bool,
    /// The first CPU row shown.
    cpu_first: usize,
    /// What the room said of the last Apply.
    problem: Option<String>,
    /// Draws for Add random, one more each.
    draws: u64,
    /// The Points row is focused (its sentence brightens).
    points_focused: bool,
}

impl RulesDialog {
    /// From its button: Cancel and Apply.
    pub fn new(env: Env, room: SharedRoom) -> RulesDialog {
        Self::opened(env, room, false)
    }

    /// Opened by itself as the player enters (M7b): Reset (the defaults,
    /// staying open) and OK (applies).
    pub fn first(env: Env, room: SharedRoom) -> RulesDialog {
        Self::opened(env, room, true)
    }

    fn opened(env: Env, room: SharedRoom, first: bool) -> RulesDialog {
        let draft = room.borrow().state().next.rules.clone();
        let draws = room.borrow().now_ms();
        RulesDialog {
            env,
            room,
            draft: Rc::new(RefCell::new(draft)),
            first,
            cpu_first: 0,
            problem: None,
            draws,
            points_focused: false,
        }
    }

    /// The player may change the rules (an operator).
    fn editable(&self) -> bool {
        self.room.borrow().may(Capability::EditNextRules)
    }

    /// The CPUs are practice's (online rooms seat none yet).
    fn cpus_shown(&self) -> bool {
        !self.room.borrow().is_online()
    }

    /// The Points row is a choice (`RoomClient::points_offered`).
    fn points_offered(&self) -> bool {
        self.room.borrow().points_offered()
    }

    /// The Attacks rule can be set here (`room::attacks_offered`).
    fn attacks_offered(&self) -> bool {
        super::room::attacks_offered(&self.room.borrow())
    }

    fn roster(&self) -> Vec<OpponentSpec> {
        cpu_roster::roster_of(&self.draft.borrow(), self.room.borrow().humans())
    }

    fn set_roster(&self, roster: Vec<OpponentSpec>) {
        let humans = self.room.borrow().humans();
        let rules = self.draft.borrow().clone();
        *self.draft.borrow_mut() = cpu_roster::with_roster(rules, roster, humans);
    }

    fn free_seats(&self) -> usize {
        usize::from(self.room.borrow().max_cpus()).saturating_sub(self.roster().len())
    }

    /// A match is running (its first stage's readiness to its last
    /// stage's end): the pop-up says so at its top and its rows are the
    /// next match's.
    fn running(&self) -> bool {
        let r = self.room.borrow();
        r.state().phase.kind().match_running() && r.locked_rules().is_some()
    }

    /// The running match's stage, from 1, and its stages: the one being
    /// readied, counted down or played, or the next after an intermission.
    fn stage_of(&self) -> (usize, u16) {
        let r = self.room.borrow();
        let stages = r.locked_rules().map_or(1, |l| l.stages);
        let n = match r.state().phase {
            Phase::Preparing { stage, .. }
            | Phase::Countdown { stage, .. }
            | Phase::Stage { stage }
            | Phase::SongCompletion { stage }
            | Phase::Intermission { next: stage } => super::room::number_of(&r, stage),
            _ => 1,
        };
        (n, stages)
    }

    /// The rows whose draft differs from the running match's rules (none
    /// between matches).
    fn changed(&self) -> Vec<&'static str> {
        if !self.running() {
            return Vec::new();
        }
        let r = self.room.borrow();
        let Some(locked) = r.locked_rules() else {
            return Vec::new();
        };
        let d = self.draft.borrow();
        let humans = r.humans();
        let mut v = Vec::new();
        if d.stages != locked.stages {
            v.push("stages");
        }
        if d.difficulty != locked.difficulty {
            v.push("difficulty");
        }
        if d.boos != locked.boos {
            v.push("boos");
        }
        if !r.is_online()
            && cpu_roster::roster_of(&d, humans) != cpu_roster::roster_of(locked, humans)
        {
            v.push("cpus");
        }
        if super::room::attacks_offered(&r) && d.attacks() != locked.attacks() {
            v.push("attacks");
        }
        if r.points_offered() && d.points != locked.points {
            v.push("points");
        }
        v
    }

    /// Apply and restart is offered: from its button, while a match runs,
    /// to who may both save the rules and stop the match, in a room whose
    /// authority takes the two as one change (`RoomClient::may_restart`).
    fn restart_offered(&self) -> bool {
        !self.first && self.running() && self.room.borrow().may_restart()
    }

    /// The buttons at its foot, the one Enter means last (Apply and
    /// restart, where offered, first: it sits apart at the foot's left).
    fn buttons(&self) -> Vec<(&'static str, String)> {
        if self.restart_offered() {
            return vec![
                ("Restart", m::rules_apply_restart().into()),
                ("Cancel", common::cancel().into()),
                ("Apply", m::rules_apply().into()),
            ];
        }
        match (self.first, self.editable()) {
            (true, true) => vec![
                ("Reset", common::reset().into()),
                ("Ok", m::rules_ok().into()),
            ],
            (true, false) => vec![("Ok", m::rules_ok().into())],
            (false, true) => vec![
                ("Cancel", common::cancel().into()),
                ("Apply", m::rules_apply().into()),
            ],
            (false, false) => vec![("Cancel", common::close().into())],
        }
    }

    fn publish(&self) {
        let items: Vec<String> = self.focusables().into_iter().map(|f| f.name).collect();
        self.env.state.borrow_mut().page_items = items;
        let draft = self.draft.borrow();
        let cpus = u16::try_from(self.roster().len()).unwrap_or(u16::MAX);
        let (stages, problem, first) = (draft.stages, self.problem.clone(), self.first);
        let rules = (
            super::room::difficulty_id(draft.difficulty).to_string(),
            draft.boos,
            draft.attacks(),
        );
        let points = self
            .points_offered()
            .then(|| draft.points.id().to_string());
        drop(draft);
        let running = self.running();
        let changed: Vec<String> = self.changed().into_iter().map(String::from).collect();
        let restart = self.restart_offered();
        publish_dialog(&self.env, |v| {
            v.rules_open = true;
            v.rules_first = first;
            v.draft = vec![stages, cpus];
            v.draft_rules = Some(rules);
            v.draft_points = points;
            v.rules_problem = problem;
            v.rules_running = running;
            v.rules_changed = changed;
            v.restart_offered = restart;
        });
    }

    /// Apply and restart: the draft checked, then asked once ("Restart
    /// the match?", Cancel first).
    fn ask_restart(&mut self, root: &mut Actor, cx: &mut Cx) {
        let draft = self.draft.borrow().clone();
        let checked = self.room.borrow().check_rules(&draft);
        if let Err(p) = checked {
            self.problem = Some(super::room::rules_problem(&p).into());
            *root = self.build(cx);
            super::restore_focus(root, Some("Restart"));
            return;
        }
        let on_yes: super::dialog::ConfirmFn =
            Rc::new(|cx| cx.broadcast(crate::Message::new(RESTART_CONFIRMED)));
        cx.overlay(
            super::dialog::ConfirmDialog::new(
                self.env.clone(),
                &m::rules_restart_title(),
                &m::rules_restart_body(),
                &m::rules_restart_confirm(),
                on_yes,
            )
            .named(RESTART_QUESTION),
            true,
        );
    }

    /// The question answered: the draft saved and the match stopped as one
    /// change; closes, or says why not. A match that ended meanwhile has
    /// nothing to stop: the draft is saved for the next one, as Apply.
    fn restart(&mut self, root: &mut Actor, cx: &mut Cx) {
        let draft = self.draft.borrow().clone();
        let may = self.room.borrow().may_restart();
        let done = if may {
            self.room.borrow_mut().restart_match(draft)
        } else if self.editable() {
            self.room.borrow_mut().save_rules(draft)
        } else {
            Ok(())
        };
        match done {
            Ok(()) => {
                cx.broadcast(crate::Message::new(RULES_SAVED));
                self.close(cx, false);
            }
            Err(why) => {
                self.room.borrow_mut().clear_refusal();
                self.problem = Some(super::room::refusal_text(&why).into());
                *root = self.build(cx);
                super::restore_focus(root, Some("Apply"));
            }
        }
    }

    fn close(&mut self, cx: &mut Cx, cancel: bool) {
        cx.close(cancel);
    }

    /// Applies the draft (when it differs); closes, or says why not.
    fn apply(&mut self, root: &mut Actor, cx: &mut Cx, name: &str) {
        let draft = self.draft.borrow().clone();
        let same = self.room.borrow().state().next.rules == draft;
        let saved = if same || !self.editable() {
            Ok(())
        } else {
            self.room.borrow_mut().save_rules(draft)
        };
        match saved {
            Ok(()) => {
                cx.broadcast(crate::Message::new(RULES_SAVED));
                self.close(cx, false);
            }
            Err(why) => {
                self.room.borrow_mut().clear_refusal();
                self.problem = Some(super::room::refusal_text(&why).into());
                *root = self.build(cx);
                super::restore_focus(root, Some(name));
            }
        }
    }
}

impl Screen for RulesDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "room-rules"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let env = self.env.clone();
        let editable = self.editable();
        let locked = self.room.borrow().locked_rules().cloned();
        let humans = self.room.borrow().humans();
        let w = 560.0_f32.min(screen[0] - 16.0);
        let inner = w - 48.0;
        let draft = self.draft.borrow().clone();
        let roster = self.roster();
        let cpus = self.cpus_shown();
        let mut notes: Vec<(String, &str)> = Vec::new();
        if cpus {
            notes.push((m::rules_level_note().into(), "Muted"));
        }
        let points = self.points_offered();
        notes.push((
            if points {
                m::rules_points_bias_note()
            } else {
                m::rules_points_note()
            }
            .into(),
            "Muted",
        ));
        notes.push((m::rules_ruleset_note().into(), "Muted"));
        notes.push((m::rules_battle_note().into(), "Muted"));
        let running = self.running();
        let changed = self.changed();
        // What applying does: nothing to say where the player cannot, nor
        // during a match, whose statement at the top says it.
        if editable && !running {
            notes.push((
                if locked.is_some() {
                    m::rules_locked_note()
                } else {
                    m::rules_clears_note()
                }
                .into(),
                "Muted",
            ));
        }
        let problem = self.problem.clone().or_else(|| {
            self.room
                .borrow()
                .check_rules(&draft)
                .err()
                .map(|p| super::room::rules_problem(&p).into())
        });
        if let Some(p) = &problem {
            notes.push((p.clone(), "Gold"));
        }
        let lines: usize = notes
            .iter()
            .map(|(t, _)| super::dialog::wrap_lines(t, inner, 12.0).len())
            .sum();
        let shown = roster.len().min(CPU_ROWS);
        self.cpu_first = scrolled(self.cpu_first, 0, roster.len(), shown);
        // During a match, a statement at the top: the match, the rules it
        // keeps, and that the rows are the next match's.
        let statement = running.then(|| {
            let (n, stages) = self.stage_of();
            let rules = locked.as_ref().map_or_else(String::new, |l| {
                m::rules_locked_line(
                    l.stages,
                    cpu_roster::roster_of(l, humans).len(),
                    super::room::rules_words(
                        &crate::state::room::StageRules::of(l),
                        self.attacks_offered().then(|| l.attacks()),
                    ),
                )
                .into()
            });
            let marked = if changed.is_empty() { "no" } else { "yes" };
            (
                super::dialog::wrap_lines(&m::rules_running(n, stages), inner, 14.0),
                super::dialog::wrap_lines(&rules, inner, 14.0),
                super::dialog::wrap_lines(&m::rules_running_next(marked), inner, 13.0),
            )
        });
        let locked_h = match &statement {
            Some((a, b, c)) => {
                16.0 + a.len() as f32 * 20.0 + b.len() as f32 * 20.0 + c.len() as f32 * 18.0
            }
            None if locked.is_some() => 52.0,
            None => 0.0,
        };
        let next_h = if running { 0.0 } else { 28.0 };
        // Apply and restart sits at the foot's left, or on a row of its
        // own above the others where the pop-up is too narrow for it.
        let restart_w = look::mono_width(&m::rules_apply_restart(), 15.0) + 28.0;
        let restart_row =
            self.restart_offered() && 24.0 + restart_w + 10.0 > w - 24.0 - 70.0 - 150.0 - 70.0;
        let restart_h = if restart_row { 48.0 } else { 0.0 };
        let cpu_h = if cpus {
            28.0 + shown.max(1) as f32 * ROW
                + if roster.len() > shown {
                    2.0 * MARK
                } else {
                    0.0
                }
                + if editable { ROW + 8.0 } else { 0.0 }
        } else {
            0.0
        };
        // The Points choice's sentence, under its row.
        let about = if points {
            super::dialog::wrap_lines(&super::room::points_about(draft.points), inner, 12.0)
        } else {
            Vec::new()
        };
        let about_h = about.len() as f32 * 17.0;
        let h = 64.0
            + locked_h
            + next_h
            + 3.0 * ROW
            + cpu_h
            + 2.0 * ROW
            + about_h
            + 12.0
            + lines as f32 * 17.0
            + restart_h
            + 72.0;
        let title = if running {
            m::rules_title_next()
        } else {
            m::rules_title()
        };
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &title);
        let x0 = x + 24.0;
        let mut ry = y + 64.0;
        // The running match's: said at the top, where it cannot be missed.
        if let Some((heading, rules, next)) = &statement {
            let mut k = 0;
            for (lines, size, weight, color, step) in [
                (heading, 14.0, 700, "Secondary", 20.0),
                (rules, 14.0, 600, "Text", 20.0),
                (next, 13.0, 600, "Gold", 18.0),
            ] {
                for line in lines {
                    let mut t =
                        env.text(&format!("Running.{k}"), line, size, weight, color);
                    look::ellipsized(&mut t, inner);
                    c.push(left(t, x0, ry + step / 2.0));
                    ry += step;
                    k += 1;
                }
            }
            ry += 16.0;
        } else if let Some(rules) = &locked {
            c.push(left(
                env.text("Locked.Heading", &m::rules_this_match(), 13.0, 700, "Muted"),
                x0,
                ry + 8.0,
            ));
            let line = m::rules_locked_line(
                rules.stages,
                cpu_roster::roster_of(rules, humans).len(),
                super::room::rules_words(
                    &crate::state::room::StageRules::of(rules),
                    self.attacks_offered().then(|| rules.attacks()),
                ),
            );
            let mut t = env.text("Locked.Line", &line, 14.0, 400, "Secondary");
            look::ellipsized(&mut t, inner);
            c.push(left(t, x0, ry + 32.0));
            ry += locked_h;
        }
        // The next match's (during a match, the title says so).
        if !running {
            c.push(left(
                env.text("Next.Heading", &m::rules_next_match(), 13.0, 700, "Muted"),
                x0,
                ry + 8.0,
            ));
            ry += 28.0;
        }
        // A row whose draft differs from the running match's: a quiet mark
        // in the margin before its label.
        let mark = |c: &mut Vec<Actor>, id: &str, ry: f32| {
            if changed.contains(&id) {
                c.push(left(
                    env.text(&format!("Changed.{id}"), "●", 9.0, 400, "Gold"),
                    x0 - 14.0,
                    ry,
                ));
            }
        };
        // A row: its label, then its value between the two buttons that
        // change it (a number's − and +, a choice's ‹ and ›; `lo` and `hi`,
        // whether there is a value that way); the value alone at the
        // row's end for someone who may not change it.
        let stepper = |c: &mut Vec<Actor>,
                       id: &str,
                       label: &str,
                       value: String,
                       (lo, hi): (bool, bool),
                       number: bool,
                       ry: f32| {
            let mut l = env.text(&format!("Rule.{id}"), label, 14.0, 400, "Secondary");
            look::ellipsized(&mut l, inner - 190.0);
            c.push(left(l, x0, ry));
            let mut v = env.text(&format!("Rule.{id}.Value"), &value, 14.0, 600, "Text");
            if editable {
                let (less, more) = if number {
                    (m::rules_less(), m::rules_more())
                } else {
                    (m::rules_previous(), m::rules_next())
                };
                c.push(button(
                    &env,
                    &format!("Dec.{id}"),
                    &less,
                    [x0 + inner - 150.0, ry],
                    36.0,
                    if lo { "Button" } else { "Button.Disabled" },
                ));
                look::ellipsized(&mut v, 92.0);
                look::call(&mut v, "xy", &[x0 + inner - 84.0, ry]);
                c.push(v);
                c.push(button(
                    &env,
                    &format!("Inc.{id}"),
                    &more,
                    [x0 + inner - 18.0, ry],
                    36.0,
                    if hi { "Button" } else { "Button.Disabled" },
                ));
            } else {
                c.push(right(v, x0 + inner, ry));
            }
        };
        let on_off = |on: bool| -> String {
            if on { common::on() } else { common::off() }.into()
        };
        let max_stages = self.room.borrow().state().policy.max_stages;
        mark(&mut c, "stages", ry + ROW / 2.0);
        stepper(
            &mut c,
            "stages",
            &m::rules_stages(),
            UiText::data(draft.stages.to_string()).into(),
            (draft.stages > 1, draft.stages < max_stages),
            true,
            ry + ROW / 2.0,
        );
        ry += ROW;
        // DDI's options that are the match's, everyone's (D0): its
        // difficulty and Boos. (The ruleset has no row: it is DDI, which
        // the note under the rows says.)
        let at = draft.difficulty.index();
        mark(&mut c, "difficulty", ry + ROW / 2.0);
        stepper(
            &mut c,
            "difficulty",
            &m::rules_difficulty(),
            super::room::difficulty_name(draft.difficulty).into(),
            (at > 0, at + 1 < MatchDifficulty::ALL.len()),
            false,
            ry + ROW / 2.0,
        );
        ry += ROW;
        mark(&mut c, "boos", ry + ROW / 2.0);
        stepper(
            &mut c,
            "boos",
            &m::rules_boos(),
            on_off(draft.boos),
            (draft.boos, !draft.boos),
            false,
            ry + ROW / 2.0,
        );
        ry += ROW;
        // Practice's CPUs: the roster, each removable, then the adds.
        if cpus {
            mark(&mut c, "cpus", ry + 14.0);
            c.push(left(
                env.text("Cpus.Heading", &m::rules_cpus(), 14.0, 400, "Secondary"),
                x0,
                ry + 14.0,
            ));
            c.push(right(
                env.text(
                    "Cpus.Count",
                    &m::players_count(roster.len(), self.room.borrow().max_cpus()),
                    13.0,
                    400,
                    "Muted",
                ),
                x0 + inner,
                ry + 14.0,
            ));
            ry += 28.0;
            if roster.len() > shown && self.cpu_first > 0 {
                c.push(env.component(
                    "Link",
                    "Button",
                    &look::Spec::new([inner, 22.0], &*m::panel_more_above(self.cpu_first)),
                    "Cpu.Up",
                    [x0 + inner / 2.0, ry + MARK / 2.0],
                ));
            }
            // A row of its own for each mark, clear of the roster's rows
            // and of the buttons under them.
            if roster.len() > shown {
                ry += MARK;
            }
            if roster.is_empty() {
                c.push(left(
                    env.text("Cpus.None", &m::rules_no_cpus(), 13.0, 400, "Faint"),
                    x0,
                    ry + ROW / 2.0,
                ));
            }
            for (k, spec) in roster.iter().enumerate().skip(self.cpu_first).take(shown) {
                let row_y = ry + ROW / 2.0;
                let ordinal = u16::try_from(k + 1).unwrap_or(u16::MAX);
                let mut n = env.text(
                    &format!("Cpu{k}.Name"),
                    &cpu_name(spec, ordinal),
                    14.0,
                    600,
                    "Text",
                );
                look::ellipsized(&mut n, inner * 0.45);
                c.push(left(n, x0, row_y));
                // Its personality, then its level's stars at the row's end.
                let end = x0 + inner - if editable { 48.0 } else { 0.0 };
                level_line_right(
                    &env,
                    &mut c,
                    &format!("Cpu{k}"),
                    (cpu_level(spec), &cpu_sub(spec)),
                    [end, row_y],
                    inner * 0.55 - if editable { 48.0 } else { 0.0 },
                );
                if editable {
                    c.push(button(
                        &env,
                        &format!("Remove.{k}"),
                        &m::rules_remove(),
                        [x0 + inner - 18.0, row_y],
                        36.0,
                        "Button",
                    ));
                }
                ry += ROW;
            }
            if roster.is_empty() {
                ry += ROW;
            }
            if roster.len() > shown {
                let below = roster.len() - self.cpu_first - shown;
                if below > 0 {
                    c.push(env.component(
                        "Link",
                        "Button",
                        &look::Spec::new([inner, 22.0], &*m::panel_more_below(below)),
                        "Cpu.Down",
                        [x0 + inner / 2.0, ry + MARK / 2.0],
                    ));
                }
                ry += MARK;
            }
            if editable {
                let free = self.free_seats() > 0;
                let kind = if free { "Button" } else { "Button.Disabled" };
                let bw = (inner - 10.0) / 2.0;
                c.push(button(
                    &env,
                    "AddRandom",
                    &m::rules_add_random(),
                    [x0 + bw / 2.0, ry + 4.0 + ROW / 2.0],
                    bw,
                    kind,
                ));
                c.push(button(
                    &env,
                    "AddCustom",
                    &m::rules_add_custom(),
                    [x0 + inner - bw / 2.0, ry + 4.0 + ROW / 2.0],
                    bw,
                    kind,
                ));
                ry += ROW + 8.0;
            }
        }
        // Attacks, where they work (practice); said unavailable where
        // they do not yet (an online room).
        if self.attacks_offered() {
            mark(&mut c, "attacks", ry + ROW / 2.0);
            stepper(
                &mut c,
                "attacks",
                &m::rules_attacks(),
                on_off(draft.attacks()),
                (draft.attacks(), !draft.attacks()),
                false,
                ry + ROW / 2.0,
            );
        } else {
            let row_y = ry + ROW / 2.0;
            c.push(left(
                env.text("Rule.attacks", &m::rules_attacks(), 14.0, 400, "Secondary"),
                x0,
                row_y,
            ));
            let mut v = env.text(
                "Rule.attacks.Value",
                &m::rules_attacks_online(),
                13.0,
                400,
                "Muted",
            );
            look::ellipsized(&mut v, inner - 110.0);
            c.push(right(v, x0 + inner, row_y));
        }
        ry += ROW;
        // Points: how strongly a large field favors its top places, where
        // the room's authority awards so; else what it is, fixed.
        if points {
            let at = draft.points.index();
            mark(&mut c, "points", ry + ROW / 2.0);
            stepper(
                &mut c,
                "points",
                &m::rules_points(),
                super::room::points_name(draft.points).into(),
                (at > 0, at + 1 < PointsBias::ALL.len()),
                false,
                ry + ROW / 2.0,
            );
            ry += ROW;
            let color = if self.points_focused { "Secondary" } else { "Muted" };
            for (k, line) in about.iter().enumerate() {
                let mut t = env.text(&format!("Rule.points.About{k}"), line, 12.0, 400, color);
                look::ellipsized(&mut t, inner);
                c.push(left(t, x0, ry + 4.0));
                ry += 17.0;
            }
        } else {
            let row_y = ry + ROW / 2.0;
            c.push(left(
                env.text("Fixed.0", &m::rules_points(), 14.0, 400, "Secondary"),
                x0,
                row_y,
            ));
            let mut v = env.text("Fixed.0.Value", &m::rules_points_value(), 13.0, 400, "Muted");
            look::ellipsized(&mut v, inner - 110.0);
            c.push(right(v, x0 + inner, row_y));
            ry += ROW;
        }
        // The notes, and the room's answer, wrapped.
        let mut ny = ry + 12.0;
        let mut k = 0;
        for (text, color) in &notes {
            for line in super::dialog::wrap_lines(text, inner, 12.0) {
                let mut t = env.text(&format!("Note{k}"), &line, 12.0, 400, color);
                look::ellipsized(&mut t, inner);
                c.push(left(t, x0, ny));
                ny += 17.0;
                k += 1;
            }
        }
        let buttons = self.buttons();
        let mut bx = x + w - 24.0 - 70.0;
        for (name, label) in buttons.iter().rev() {
            if *name == "Restart" {
                let (at, by) = if restart_row {
                    (x + w / 2.0, y + h - 36.0 - 48.0)
                } else {
                    (x0 + restart_w / 2.0, y + h - 36.0)
                };
                c.push(button(&env, name, label, [at, by], restart_w, "Button"));
                continue;
            }
            c.push(button(
                &env,
                name,
                label,
                [bx, y + h - 36.0],
                140.0,
                "Button",
            ));
            bx -= 150.0;
        }
        self.publish();
        fitted_dialog(c, screen, h, scale)
    }

    fn focusables(&self) -> Vec<Focusable> {
        // A row's two buttons: Right and Left move between them (the rows
        // touch and the wide Add buttons reach over the Attacks row's, so
        // where the buttons are alone would not always say so).
        fn pair(v: &mut Vec<Focusable>, id: &str) {
            let (dec, inc) = (format!("Dec.{id}"), format!("Inc.{id}"));
            v.push(Focusable {
                right: Some(inc.clone()),
                ..Focusable::new(dec.clone())
            });
            v.push(Focusable {
                left: Some(dec),
                ..Focusable::new(inc)
            });
        }
        let mut v = Vec::new();
        if self.editable() {
            for id in ["stages", "difficulty", "boos"] {
                pair(&mut v, id);
            }
            if self.cpus_shown() {
                let roster = self.roster();
                for k in (self.cpu_first..roster.len()).take(CPU_ROWS) {
                    v.push(Focusable::new(format!("Remove.{k}")));
                }
                v.push(Focusable::new("AddRandom"));
                v.push(Focusable::new("AddCustom"));
            }
            if self.attacks_offered() {
                pair(&mut v, "attacks");
            }
            if self.points_offered() {
                pair(&mut v, "points");
            }
        }
        for (name, _) in self.buttons() {
            v.push(Focusable::new(name));
        }
        v
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        // The Points row's sentence brightens while the row is focused.
        if self.points_offered() {
            let focused = cx.focused().map(str::to_string);
            let on = focused
                .as_deref()
                .is_some_and(|f| f == "Dec.points" || f == "Inc.points");
            if on != self.points_focused {
                self.points_focused = on;
                *root = self.build(cx);
                super::restore_focus(root, focused.as_deref());
            }
        }
    }

    fn message(&mut self, msg: &crate::Message, root: &mut Actor, cx: &mut Cx) {
        if msg.name == RESTART_CONFIRMED {
            self.restart(root, cx);
            return;
        }
        if msg.name == ROSTER_CHANGED {
            self.problem = None;
            let focused = cx.focused().map(str::to_string);
            *root = self.build(cx);
            super::restore_focus(root, focused.as_deref());
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let rebuild = |this: &mut Self, root: &mut Actor, cx: &mut Cx, name: &str| {
            this.problem = None;
            *root = this.build(cx);
            super::restore_focus(root, Some(name));
        };
        match input {
            Input::Wheel { delta, .. } if self.cpus_shown() => {
                let by = if delta[1] > 0.0 { 1 } else { -1 };
                let len = self.roster().len();
                self.cpu_first = scrolled(self.cpu_first, by, len, len.min(CPU_ROWS));
                let focused = cx.focused().map(str::to_string);
                *root = self.build(cx);
                super::restore_focus(root, focused.as_deref());
                Handled::Yes
            }
            Input::Activate { name } => {
                let max_stages = self.room.borrow().state().policy.max_stages;
                match name.as_str() {
                    "Dec.stages" | "Inc.stages" if self.editable() => {
                        let mut d = self.draft.borrow_mut();
                        d.stages = if name == "Inc.stages" {
                            d.stages.saturating_add(1)
                        } else {
                            d.stages.saturating_sub(1)
                        }
                        .clamp(1, max_stages);
                        drop(d);
                        rebuild(self, root, cx, name);
                    }
                    "Dec.difficulty" | "Inc.difficulty" if self.editable() => {
                        let mut d = self.draft.borrow_mut();
                        let at = d.difficulty.index();
                        let at = if name == "Inc.difficulty" {
                            (at + 1).min(MatchDifficulty::ALL.len() - 1)
                        } else {
                            at.saturating_sub(1)
                        };
                        d.difficulty = MatchDifficulty::ALL[at];
                        drop(d);
                        rebuild(self, root, cx, name);
                    }
                    "Dec.boos" | "Inc.boos" if self.editable() => {
                        self.draft.borrow_mut().boos = name == "Inc.boos";
                        rebuild(self, root, cx, name);
                    }
                    "Dec.points" | "Inc.points" if self.editable() && self.points_offered() => {
                        let mut d = self.draft.borrow_mut();
                        let at = d.points.index();
                        let at = if name == "Inc.points" {
                            (at + 1).min(PointsBias::ALL.len() - 1)
                        } else {
                            at.saturating_sub(1)
                        };
                        d.points = PointsBias::ALL[at];
                        drop(d);
                        self.points_focused = true;
                        rebuild(self, root, cx, name);
                    }
                    "Dec.attacks" | "Inc.attacks"
                        if self.editable() && self.attacks_offered() =>
                    {
                        self.draft.borrow_mut().set_attacks(name == "Inc.attacks");
                        rebuild(self, root, cx, name);
                    }
                    "Cpu.Up" | "Cpu.Down" => {
                        let len = self.roster().len();
                        let by = if name == "Cpu.Down" { 1 } else { -1 };
                        self.cpu_first = scrolled(self.cpu_first, by, len, len.min(CPU_ROWS));
                        rebuild(self, root, cx, name);
                    }
                    "AddRandom" if self.free_seats() > 0 => {
                        let mut roster = self.roster();
                        self.draws = self.draws.wrapping_add(1);
                        if let Some(spec) =
                            cpu_roster::random_cpu(&roster, RosterAim::Any, self.draws)
                        {
                            roster.push(spec);
                            self.cpu_first = roster.len().saturating_sub(CPU_ROWS);
                            self.set_roster(roster);
                        }
                        rebuild(self, root, cx, name);
                    }
                    "AddCustom" if self.free_seats() > 0 => {
                        cx.overlay(
                            AddCpuDialog::new(
                                self.env.clone(),
                                self.room.clone(),
                                self.draft.clone(),
                                self.draws,
                            ),
                            true,
                        );
                    }
                    "Reset" if self.editable() => {
                        *self.draft.borrow_mut() = defaults(&self.room);
                        self.cpu_first = 0;
                        rebuild(self, root, cx, name);
                    }
                    "Cancel" => self.close(cx, true),
                    "Restart" if self.restart_offered() => self.ask_restart(root, cx),
                    "Apply" | "Ok" => {
                        let name = name.clone();
                        self.apply(root, cx, &name);
                    }
                    other => {
                        if let Some(k) = other
                            .strip_prefix("Remove.")
                            .and_then(|k| k.parse::<usize>().ok())
                            && self.editable()
                        {
                            let mut roster = self.roster();
                            if k < roster.len() {
                                roster.remove(k);
                                self.set_roster(roster);
                            }
                            let next = self
                                .focusables()
                                .into_iter()
                                .find(|f| f.name.starts_with("Remove."));
                            let to = next.map_or_else(|| "AddRandom".to_string(), |f| f.name);
                            self.problem = None;
                            *root = self.build(cx);
                            cx.focus(to.clone());
                            super::restore_focus(root, Some(&to));
                        }
                    }
                }
                Handled::Yes
            }
            // The first one is not dismissed by a press beside it: OK or
            // Back (which applies nothing) close it.
            Input::Outside if !self.first => {
                self.close(cx, true);
                Handled::Yes
            }
            Input::Menu {
                action: MenuAction::Back,
                ..
            } if self.first => {
                self.close(cx, true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// However it closed (Apply, Cancel, Back, a press outside), the debug
/// state stops saying it is open.
impl Drop for RulesDialog {
    fn drop(&mut self) {
        publish_dialog(&self.env, |v| {
            v.rules_open = false;
            v.rules_first = false;
            v.rules_running = false;
            v.rules_changed.clear();
            v.restart_offered = false;
        });
    }
}

/// Add custom (M7b): several CPUs at a level at once, or one by its name,
/// into the Rules pop-up's roster (`draft`).
pub struct AddCpuDialog {
    env: Env,
    room: SharedRoom,
    draft: Rc<RefCell<MatchRules>>,
    /// The level (the engine's index) and how many to add by level.
    level: u8,
    count: usize,
    /// The first catalog row shown.
    first: usize,
    seed: u64,
}

/// Catalog rows Add custom shows at once.
const NAME_ROWS: usize = 5;
/// The level's stars between Add custom's two level buttons.
const LEVEL_STARS: f32 = 15.0;

impl AddCpuDialog {
    pub fn new(
        env: Env,
        room: SharedRoom,
        draft: Rc<RefCell<MatchRules>>,
        seed: u64,
    ) -> AddCpuDialog {
        AddCpuDialog {
            env,
            room,
            draft,
            level: crate::state::room::DEFAULT_CPU_LEVEL,
            count: 1,
            first: 0,
            seed,
        }
    }

    fn roster(&self) -> Vec<OpponentSpec> {
        cpu_roster::roster_of(&self.draft.borrow(), self.room.borrow().humans())
    }

    fn free(&self) -> usize {
        usize::from(self.room.borrow().max_cpus()).saturating_sub(self.roster().len())
    }

    fn add(&mut self, more: Vec<OpponentSpec>, cx: &mut Cx) {
        if more.is_empty() {
            return;
        }
        let humans = self.room.borrow().humans();
        let mut roster = self.roster();
        roster.extend(more);
        let rules = self.draft.borrow().clone();
        *self.draft.borrow_mut() = cpu_roster::with_roster(rules, roster, humans);
        self.seed = self.seed.wrapping_add(1);
        cx.broadcast(crate::Message::new(ROSTER_CHANGED));
    }

    fn names(&self) -> Vec<&'static cpu_roster::RosterCpu> {
        cpu_roster::unseated(&self.roster())
    }
}

impl Screen for AddCpuDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "room-add-cpus"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let env = self.env.clone();
        let w = 520.0_f32.min(screen[0] - 16.0);
        let inner = w - 48.0;
        let free = self.free();
        self.count = self.count.clamp(1, free.max(1));
        let names = self.names();
        let shown = names.len().min(NAME_ROWS);
        self.first = scrolled(self.first, 0, names.len(), shown);
        let h = 64.0 + 2.0 * ROW + ROW + 12.0 + 28.0 + shown.max(1) as f32 * ROW + 24.0 + 72.0;
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &m::add_cpus_title());
        let x0 = x + 24.0;
        let mut ry = y + 64.0;
        let levels = cpu_levels();
        for (id, label, value, lo, hi) in [
            // The level: its stars between the two buttons, no number.
            (
                "level",
                m::rules_level(),
                None,
                self.level > 0,
                self.level + 1 < levels,
            ),
            (
                "count",
                m::add_cpus_count(),
                Some(UiText::data(self.count.to_string())),
                self.count > 1,
                self.count < free,
            ),
        ] {
            let row_y = ry + ROW / 2.0;
            c.push(left(
                env.text(&format!("Add.{id}"), &label, 14.0, 400, "Secondary"),
                x0,
                row_y,
            ));
            c.push(button(
                &env,
                &format!("Dec.{id}"),
                &m::rules_less(),
                [x0 + inner - 150.0, row_y],
                36.0,
                if lo { "Button" } else { "Button.Disabled" },
            ));
            let v = match value {
                Some(value) => {
                    let mut v = env.text(&format!("Add.{id}.Value"), &value, 14.0, 600, "Text");
                    look::call(&mut v, "xy", &[x0 + inner - 84.0, row_y]);
                    v
                }
                None => {
                    let mut v = crate::stars::stars(
                        &env,
                        &format!("Add.{id}.Stars"),
                        self.level + 1,
                        LEVEL_STARS,
                    );
                    look::call(&mut v, "xy", &[x0 + inner - 84.0, row_y]);
                    v
                }
            };
            c.push(v);
            c.push(button(
                &env,
                &format!("Inc.{id}"),
                &m::rules_more(),
                [x0 + inner - 18.0, row_y],
                36.0,
                if hi { "Button" } else { "Button.Disabled" },
            ));
            ry += ROW;
        }
        let label = m::add_cpus_add(self.count);
        c.push(button(
            &env,
            "Add.ByLevel",
            &label,
            [x0 + inner / 2.0, ry + ROW / 2.0],
            inner,
            if free > 0 {
                "Button"
            } else {
                "Button.Disabled"
            },
        ));
        ry += ROW + 12.0;
        c.push(left(
            env.text("Add.ByName", &m::add_cpus_by_name(), 13.0, 700, "Muted"),
            x0,
            ry + 14.0,
        ));
        ry += 28.0;
        if self.first > 0 {
            c.push(env.component(
                "Link",
                "Button",
                &look::Spec::new([inner, 20.0], &*m::panel_more_above(self.first)),
                "Name.Up",
                [x0 + inner / 2.0, ry - 4.0],
            ));
        }
        if names.is_empty() || free == 0 {
            let words = if free == 0 {
                m::add_cpus_full()
            } else {
                m::add_cpus_none_left()
            };
            c.push(left(
                env.text("Add.None", &words, 13.0, 400, "Faint"),
                x0,
                ry + ROW / 2.0,
            ));
        } else {
            for (k, cpu) in names.iter().enumerate().skip(self.first).take(shown) {
                let row_y = ry + ROW / 2.0;
                // A list row's origin is its top left.
                let mut row = env.component(
                    "List.Row",
                    "Button",
                    &look::Spec::new([inner, ROW - 4.0], ""),
                    &format!("Name.{k}"),
                    [x0, row_y - (ROW - 4.0) / 2.0],
                );
                crate::widgets::set_text(&mut row, "Title", "");
                crate::widgets::set_text(&mut row, "Subtitle", "");
                // No picture: a CPU's row is its name and stars.
                for part in ["Banner", "BannerBack"] {
                    if let Some(banner) = crate::widgets::part(&mut row, part) {
                        banner.visible = false;
                    }
                }
                c.push(row);
                let spec = cpu.spec(0);
                let mut n = env.text(
                    &format!("Name.{k}.Name"),
                    &UiText::data(&cpu.name),
                    14.0,
                    600,
                    "Text",
                );
                look::ellipsized(&mut n, inner * 0.5);
                c.push(left(n, x0 + 12.0, row_y));
                level_line_right(
                    &env,
                    &mut c,
                    &format!("Name.{k}"),
                    (cpu_level(&spec), &cpu_sub(&spec)),
                    [x0 + inner - 12.0, row_y],
                    inner * 0.5 - 24.0,
                );
                ry += ROW;
            }
        }
        if names.len() > self.first + shown {
            c.push(env.component(
                "Link",
                "Button",
                &look::Spec::new(
                    [inner, 20.0],
                    &*m::panel_more_below(names.len() - self.first - shown),
                ),
                "Name.Down",
                [x0 + inner / 2.0, ry + 10.0],
            ));
        }
        c.push(button(
            &env,
            "Close",
            &common::close(),
            [x + w - 24.0 - 70.0, y + h - 36.0],
            140.0,
            "Button",
        ));
        let items: Vec<String> = self.focusables().into_iter().map(|f| f.name).collect();
        self.env.state.borrow_mut().page_items = items;
        publish_dialog(&env, |v| v.add_open = true);
        fitted_dialog(c, screen, h, scale)
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut v: Vec<Focusable> = [
            "Dec.level",
            "Inc.level",
            "Dec.count",
            "Inc.count",
            "Add.ByLevel",
        ]
        .into_iter()
        .map(Focusable::new)
        .collect();
        if self.free() > 0 {
            let n = self.names().len();
            for k in (self.first..n).take(NAME_ROWS) {
                v.push(Focusable::new(format!("Name.{k}")));
            }
        }
        v.push(Focusable::new("Close"));
        v
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let rebuild = |this: &mut Self, root: &mut Actor, cx: &mut Cx, name: &str| {
            *root = this.build(cx);
            let to = if this.focusables().iter().any(|f| f.name == name) {
                name.to_string()
            } else {
                "Close".into()
            };
            cx.focus(to.clone());
            super::restore_focus(root, Some(&to));
        };
        match input {
            Input::Wheel { delta, .. } => {
                let n = self.names().len();
                let by = if delta[1] > 0.0 { 1 } else { -1 };
                self.first = scrolled(self.first, by, n, n.min(NAME_ROWS));
                let focused = cx.focused().unwrap_or("Close").to_string();
                rebuild(self, root, cx, &focused);
                Handled::Yes
            }
            Input::Focus { to: Some(to), .. } => {
                // Moving onto the last row shown scrolls the next one in.
                if let Some(k) = to
                    .strip_prefix("Name.")
                    .and_then(|k| k.parse::<usize>().ok())
                {
                    let n = self.names().len();
                    let before = self.first;
                    if k + 1 >= self.first + NAME_ROWS && k + 1 < n {
                        self.first = scrolled(self.first, 1, n, NAME_ROWS);
                    } else if k == self.first && k > 0 {
                        self.first = scrolled(self.first, -1, n, NAME_ROWS);
                    }
                    if before != self.first {
                        let to = to.clone();
                        rebuild(self, root, cx, &to);
                    }
                }
                Handled::No
            }
            Input::Activate { name } => {
                let levels = cpu_levels();
                let free = self.free();
                match name.as_str() {
                    "Dec.level" => self.level = self.level.saturating_sub(1),
                    "Inc.level" => self.level = (self.level + 1).min(levels - 1),
                    "Dec.count" => self.count = self.count.saturating_sub(1).max(1),
                    "Inc.count" => self.count = (self.count + 1).min(free.max(1)),
                    "Add.ByLevel" if free > 0 => {
                        let more = cpu_roster::at_level(
                            &self.roster(),
                            self.level,
                            self.count.min(free),
                            self.seed,
                        );
                        self.add(more, cx);
                        if self.free() == 0 {
                            cx.close(false);
                            return Handled::Yes;
                        }
                    }
                    "Name.Up" | "Name.Down" => {
                        let n = self.names().len();
                        let by = if name == "Name.Down" { 1 } else { -1 };
                        self.first = scrolled(self.first, by, n, n.min(NAME_ROWS));
                    }
                    "Close" => {
                        cx.close(true);
                        return Handled::Yes;
                    }
                    other => {
                        if let Some(k) = other
                            .strip_prefix("Name.")
                            .and_then(|k| k.parse::<usize>().ok())
                            && free > 0
                        {
                            let names = self.names();
                            if let Some(cpu) = names.get(k) {
                                let spec =
                                    cpu_roster::by_name(&self.roster(), &cpu.name, self.seed);
                                self.add(spec.into_iter().collect(), cx);
                            }
                            if self.free() == 0 {
                                cx.close(false);
                                return Handled::Yes;
                            }
                        }
                    }
                }
                let name = name.clone();
                rebuild(self, root, cx, &name);
                Handled::Yes
            }
            Input::Outside => {
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

impl Drop for AddCpuDialog {
    fn drop(&mut self) {
        publish_dialog(&self.env, |v| v.add_open = false);
    }
}

/// The players, then the match's picks (M7b: the host's view of what the
/// others see beside song select; the players first since 2026-11-12:
/// their rows are what an online room's host acts on), scrolling: Up and
/// Down, the wheel, or the marks. Everyone's says what the room does when
/// its host leaves; the host's has Host options ([`HostDialog`]) and a
/// tap on another person's row opens it on them.
pub struct PlayersDialog {
    env: Env,
    room: SharedRoom,
    first: usize,
    /// The room's version built.
    seen: u64,
    /// The local player may change the room's hosts (as built): Host
    /// options is offered.
    manages: bool,
    /// The local player may change between Play and Watch now (as
    /// built): its button is offered.
    chooses: bool,
}

impl PlayersDialog {
    pub fn new(env: Env, room: SharedRoom) -> PlayersDialog {
        PlayersDialog {
            env,
            room,
            first: 0,
            seen: u64::MAX,
            manages: false,
            chooses: false,
        }
    }

    fn scroll(&mut self, by: i32, root: &mut Actor, cx: &mut Cx) {
        self.first = (self.first as i64 + i64::from(by)).max(0) as usize;
        let focused = cx.focused().map(str::to_string);
        *root = self.build(cx);
        super::restore_focus(root, focused.as_deref().or(Some("Close")));
    }
}

impl Screen for PlayersDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "room-players"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let env = self.env.clone();
        let w = 560.0_f32.min(screen[0] - 16.0);
        let h = (screen[1] - 32.0).min(620.0);
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &m::players());
        let r = self.room.borrow();
        self.seen = r.version();
        let mut rows = players_first_rows(&r);
        let status: String = super::room::status(&r).into();
        // What the room does when its host leaves, said to everyone
        // (where the room says).
        let about: Option<String> = r.has_succession().then(|| host_leaves_about(&r).into());
        // The host's: Host options, and a tap on another person's row.
        self.manages = r.manages_hosts();
        self.chooses = r.may_choose_intent();
        if self.manages {
            let others: Vec<u64> = r.other_people().iter().map(|mb| mb.id.0).collect();
            for row in &mut rows {
                if let PanelRow::Player(p) = row
                    && p.id.is_some_and(|id| others.contains(&id))
                {
                    p.manage = true;
                }
            }
        }
        drop(r);
        let mut sy = y + 64.0;
        for (i, line) in super::dialog::wrap_lines(&status, w - 48.0, 13.0)
            .iter()
            .take(2)
            .enumerate()
        {
            let mut t = env.text(&format!("Status{i}"), line, 13.0, 400, "Secondary");
            look::ellipsized(&mut t, w - 48.0);
            c.push(left(t, x + 24.0, sy + 8.0));
            sy += 18.0;
        }
        if let Some(about) = &about {
            for (i, line) in super::dialog::wrap_lines(about, w - 48.0, 12.0)
                .iter()
                .take(2)
                .enumerate()
            {
                let mut t = env.text(&format!("HostLeaves{i}"), line, 12.0, 400, "Muted");
                look::ellipsized(&mut t, w - 48.0);
                c.push(left(t, x + 24.0, sy + 10.0));
                sy += 17.0;
            }
            sy += 2.0;
        }
        let (p, first) = panel(
            &env,
            &rows,
            self.first,
            [x + 24.0, sy + 8.0, w - 48.0, y + h - 72.0 - sy - 8.0],
        );
        self.first = first;
        c.push(p);
        c.push(button(
            &env,
            "Close",
            &common::close(),
            [x + w - 24.0 - 70.0, y + h - 36.0],
            140.0,
            "Button",
        ));
        // The foot's buttons from the left: Play or Watch (the player's
        // own), then the host's Host options.
        let mut bx = x + 24.0;
        let room_left = w - 48.0 - 150.0;
        if self.chooses {
            let label = super::room::intent_label(&self.room.borrow());
            let share = if self.manages { 0.5 } else { 1.0 };
            let bw = (look::mono_width(&label, 15.0) + 32.0).clamp(96.0, room_left * share - 4.0);
            c.push(button(
                &env,
                INTENT,
                &label,
                [bx + bw / 2.0, y + h - 36.0],
                bw,
                "Button",
            ));
            bx += bw + 8.0;
        }
        if self.manages {
            let label = m::host_options();
            let bw = (look::mono_width(&label, 15.0) + 32.0)
                .clamp(96.0, (x + 24.0 + room_left - bx).max(96.0));
            c.push(button(
                &env,
                HOST_OPTIONS,
                &label,
                [bx + bw / 2.0, y + h - 36.0],
                bw,
                "Button",
            ));
        }
        self.env.state.borrow_mut().page_items =
            self.focusables().into_iter().map(|f| f.name).collect();
        publish_dialog(&env, |v| v.players_open = true);
        fitted_dialog(c, screen, h, scale)
    }

    fn focusables(&self) -> Vec<Focusable> {
        // Close first: the pop-up opens on it, as everyone's; the foot's
        // other buttons to its left, in their order.
        let mut row: Vec<&str> = Vec::new();
        if self.chooses {
            row.push(INTENT);
        }
        if self.manages {
            row.push(HOST_OPTIONS);
        }
        row.push("Close");
        let link = |i: usize| Focusable {
            left: i.checked_sub(1).map(|k| row[k].to_string()),
            right: row.get(i + 1).map(|n| n.to_string()),
            ..Focusable::new(row[i])
        };
        let last = row.len() - 1;
        std::iter::once(link(last))
            .chain((0..last).map(link))
            .collect()
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        if self.room.borrow().version() != self.seen {
            *root = self.build(cx);
            // The host's button goes with the role.
            let focused = cx.focused().map(str::to_string);
            let kept = focused
                .filter(|f| self.focusables().iter().any(|n| n.name == *f))
                .unwrap_or_else(|| {
                    cx.focus("Close");
                    "Close".into()
                });
            super::restore_focus(root, Some(&kept));
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Menu {
                action: MenuAction::Up | MenuAction::Down,
                ..
            } => {
                let down = matches!(
                    input,
                    Input::Menu {
                        action: MenuAction::Down,
                        ..
                    }
                );
                self.scroll(if down { 1 } else { -1 }, root, cx);
                Handled::Yes
            }
            Input::Wheel { delta, .. } => {
                self.scroll(if delta[1] > 0.0 { 1 } else { -1 }, root, cx);
                Handled::Yes
            }
            Input::Activate { name } if name == "Panel.Up" || name == "Panel.Down" => {
                let by = if name == "Panel.Down" { 3 } else { -3 };
                self.scroll(by, root, cx);
                Handled::Yes
            }
            Input::Activate { name } if name == INTENT => {
                super::room::toggle_intent(&self.room);
                Handled::Yes
            }
            Input::Activate { name } if name == HOST_OPTIONS => {
                if self.manages {
                    cx.overlay(
                        HostDialog::new(self.env.clone(), self.room.clone(), None),
                        true,
                    );
                }
                Handled::Yes
            }
            // The host taps another person's row: Host options, on them.
            Input::Pointer {
                target: Some(t),
                gesture: crate::Gesture::Tap,
                ..
            } if self.manages && t.name.starts_with("Person.") => {
                let id = t.name.strip_prefix("Person.").and_then(|n| n.parse().ok());
                cx.overlay(
                    HostDialog::new(self.env.clone(), self.room.clone(), id),
                    true,
                );
                Handled::Yes
            }
            Input::Activate { .. } | Input::Outside => {
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// What the room does when its host leaves, in a sentence.
fn host_leaves_about(r: &crate::state::room::RoomClient) -> UiText {
    match r.if_host_leaves() {
        HostLeaves::PassOn => m::host_leaves_pass_about(),
        HostLeaves::Close => m::host_leaves_close_about(),
    }
}

/// The Players pop-up's button that opens Host options (the host's).
pub const HOST_OPTIONS: &str = "HostOptions";

/// The Players pop-up's button that changes the local player between
/// Play and Watch (`watch-v0`): it says what a press does.
pub const INTENT: &str = "Intent";

/// A person's row in Host options: its height with the buttons beside
/// the name, and with them under it (a narrow pop-up).
const PERSON_ROW: f32 = 48.0;
const PERSON_ROW_TALL: f32 = 80.0;

/// Host options (2026-11-12): an online room's host's own. What the room
/// does when its host leaves (`ddi_room::HostLeaves`, where the room
/// passes on: its two values, each said in a sentence), and the other
/// people in the room, each with Make co-host or Remove co-host and Make
/// host (asked once more: the giver cannot undo it alone). Never a bot or
/// a CPU: they follow a room. The room decides; the rows follow it.
pub struct HostDialog {
    env: Env,
    room: SharedRoom,
    /// The people's first row shown.
    first: usize,
    /// The person to show first (their row tapped).
    at: Option<u64>,
    /// What to focus on opening: that person's first button, else Close
    /// (nothing changes by a press too many).
    focus_first: Option<String>,
    /// The room's version built.
    seen: u64,
    /// The rows that fit.
    fit: usize,
}

/// One of the other people, as Host options lists them.
struct Person {
    id: u64,
    name: String,
    sub: String,
    co_host: bool,
    /// Came by the watch-only link and held to watching: the first
    /// button lets them play (`RoomClient::let_play`) in the place of
    /// Make co-host, until it is pressed.
    watch_only: bool,
    /// The room may be handed to them now (there, connected).
    can_host: bool,
}

impl HostDialog {
    pub fn new(env: Env, room: SharedRoom, at: Option<u64>) -> HostDialog {
        HostDialog {
            env,
            room,
            first: 0,
            at,
            focus_first: Some(at.map_or_else(|| "Close".to_string(), |id| format!("CoHost.{id}"))),
            seen: u64::MAX,
            fit: 1,
        }
    }

    fn people(&self) -> Vec<Person> {
        let r = self.room.borrow();
        let mut people = r
            .other_people()
            .into_iter()
            .map(|mb| {
                let role: Option<String> = match (mb.role, mb.admin) {
                    (Role::CoHost, _) => Some(m::role_co_host().into()),
                    (_, true) => Some(m::role_admin().into()),
                    _ => None,
                };
                let held: Option<String> =
                    (mb.watch_only && mb.role != Role::Owner).then(|| m::host_watch_only().into());
                let away: Option<String> = match mb.connection {
                    ddi_room::Connection::Connected => None,
                    ddi_room::Connection::Joining => Some(m::conn_joining().into()),
                    ddi_room::Connection::Reconnecting => Some(m::conn_reconnecting().into()),
                    ddi_room::Connection::Disconnected => Some(m::conn_away().into()),
                };
                Person {
                    id: mb.id.0,
                    name: person_name(mb),
                    sub: [role, held, away]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · "),
                    co_host: mb.role == Role::CoHost,
                    watch_only: mb.watch_only && mb.role != Role::Owner,
                    can_host: r.may_make_host(mb.id),
                }
            })
            .collect::<Vec<_>>();
        // A large room: its co-hosts and administrators first.
        if people.len() > super::room::ROOM_OF_NINE {
            people.sort_by_key(|p| p.sub.is_empty() || !(p.co_host || p.sub.starts_with(&String::from(m::role_admin()))));
        }
        people
    }

    fn rebuild(&mut self, root: &mut Actor, cx: &mut Cx) {
        let focused = cx.focused().map(str::to_string);
        *root = self.build(cx);
        let kept = focused
            .filter(|f| self.focusables().iter().any(|n| n.name == *f))
            .unwrap_or_else(|| {
                cx.focus("Close");
                "Close".into()
            });
        super::restore_focus(root, Some(&kept));
    }

    fn scroll(&mut self, by: i32, root: &mut Actor, cx: &mut Cx) {
        let len = self.people().len();
        self.first = scrolled(self.first, by, len, len.min(self.fit));
        self.rebuild(root, cx);
    }
}

impl Screen for HostDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "room-host"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let env = self.env.clone();
        let w = 560.0_f32.min(screen[0] - 16.0);
        let inner = w - 48.0;
        let people = self.people();
        let (succession, policy) = {
            let r = self.room.borrow();
            self.seen = r.version();
            (r.has_succession(), r.if_host_leaves())
        };
        let about: Vec<String> = if succession {
            let text: String = host_leaves_about(&self.room.borrow()).into();
            super::dialog::wrap_lines(&text, inner, 13.0)
        } else {
            Vec::new()
        };
        let setting_h = if succession {
            ROW + about.len() as f32 * 18.0 + 14.0
        } else {
            0.0
        };
        // The buttons beside the name where there is room, else under it.
        let one_line = inner >= 500.0;
        let row_h = if one_line { PERSON_ROW } else { PERSON_ROW_TALL };
        let fixed = 64.0 + setting_h + 28.0 + 72.0;
        // As many rows as the screen has room for (six at most); where
        // not everyone fits, the marks that scroll them take their room.
        let rows_in = |h: f32| ((h / row_h).floor().max(1.0) as usize).min(6);
        let room_for = screen[1] - 32.0 - fixed;
        self.fit = rows_in(room_for);
        if people.len() > self.fit {
            self.fit = rows_in(room_for - 2.0 * MARK);
        }
        let shown = people.len().min(self.fit);
        // The person whose row was tapped is in view.
        if let Some(at) = self.at
            && let Some(k) = people.iter().position(|p| p.id == at)
            && !(self.first..self.first + shown).contains(&k)
        {
            self.first = k;
        }
        self.first = scrolled(self.first, 0, people.len(), shown);
        let marks = if people.len() > shown { 2.0 * MARK } else { 0.0 };
        let h = fixed + shown.max(1) as f32 * row_h + marks;
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &m::host_options());
        let x0 = x + 24.0;
        let mut ry = y + 64.0;
        if succession {
            // The setting: its label, its value between the two buttons
            // that change it, and what the value does.
            let mid = ry + ROW / 2.0;
            let mut label = env.text("HostLeaves", &m::host_leaves(), 14.0, 400, "Secondary");
            look::ellipsized(&mut label, inner - 250.0);
            c.push(left(label, x0, mid));
            let (value, lo, hi) = match policy {
                HostLeaves::PassOn => (m::host_leaves_pass(), false, true),
                HostLeaves::Close => (m::host_leaves_close(), true, false),
            };
            c.push(button(
                &env,
                "Dec.leaves",
                &m::rules_previous(),
                [x0 + inner - 210.0, mid],
                36.0,
                if lo { "Button" } else { "Button.Disabled" },
            ));
            let mut v = env.text("HostLeaves.Value", &value, 14.0, 600, "Text");
            look::ellipsized(&mut v, 150.0);
            look::call(&mut v, "xy", &[x0 + inner - 114.0, mid]);
            c.push(v);
            c.push(button(
                &env,
                "Inc.leaves",
                &m::rules_next(),
                [x0 + inner - 18.0, mid],
                36.0,
                if hi { "Button" } else { "Button.Disabled" },
            ));
            let mut ay = ry + ROW + 8.0;
            for (i, line) in about.iter().enumerate() {
                let t = env.text(&format!("HostLeaves.About{i}"), line, 13.0, 400, "Muted");
                c.push(left(t, x0, ay));
                ay += 18.0;
            }
            ry += setting_h;
        }
        c.push(left(
            look::tracked(
                env.text("People.Heading", &m::host_people(), 11.0, 700, "Faint"),
                120,
            ),
            x0,
            ry + 12.0,
        ));
        ry += 28.0;
        if people.is_empty() {
            c.push(left(
                env.text("People.None", &m::host_nobody(), 14.0, 400, "Muted"),
                x0,
                ry + row_h / 2.0,
            ));
        }
        if people.len() > shown {
            c.push(env.component(
                "Link",
                "Button",
                &look::Spec::new([inner, 22.0], &*m::panel_more_above(self.first)),
                "People.Up",
                [x0 + inner / 2.0, ry + MARK / 2.0],
            ));
            ry += MARK;
        }
        // The two buttons' widths, as wide as their longer labels.
        let width = |t: &str| look::mono_width(t, 15.0) + 28.0;
        let co_w = width(&m::make_co_host())
            .max(width(&m::remove_co_host()))
            .max(width(&m::let_play()))
            .min((inner - 10.0) * 0.58);
        let host_w = width(&m::make_host()).min((inner - 10.0) * 0.42);
        for p in people.iter().skip(self.first).take(shown) {
            let name_w = if one_line {
                (inner - co_w - host_w - 28.0).max(60.0)
            } else {
                inner
            };
            let (name_y, sub_y) = if one_line {
                (ry + row_h / 2.0 - 9.0, ry + row_h / 2.0 + 10.0)
            } else {
                (ry + 12.0, ry + 30.0)
            };
            let alone = p.sub.is_empty() && one_line;
            let mut name = env.text(&format!("Person{}.Name", p.id), &p.name, 15.0, 600, "Text");
            look::ellipsized(&mut name, name_w);
            c.push(left(
                name,
                x0,
                if alone { ry + row_h / 2.0 } else { name_y },
            ));
            if !p.sub.is_empty() {
                let mut sub = env.text(&format!("Person{}.Sub", p.id), &p.sub, 12.0, 400, "Muted");
                look::ellipsized(&mut sub, name_w);
                c.push(left(sub, x0, sub_y));
            }
            let by = if one_line {
                ry + row_h / 2.0
            } else {
                ry + row_h - 26.0
            };
            let co_label = if p.watch_only {
                m::let_play()
            } else if p.co_host {
                m::remove_co_host()
            } else {
                m::make_co_host()
            };
            c.push(button(
                &env,
                &format!("CoHost.{}", p.id),
                &co_label,
                [x0 + inner - host_w - 10.0 - co_w / 2.0, by],
                co_w,
                "Button",
            ));
            if succession {
                c.push(button(
                    &env,
                    &format!("MakeHost.{}", p.id),
                    &m::make_host(),
                    [x0 + inner - host_w / 2.0, by],
                    host_w,
                    if p.can_host {
                        "Button"
                    } else {
                        "Button.Disabled"
                    },
                ));
            }
            ry += row_h;
        }
        if people.len() > shown {
            let below = people.len() - self.first - shown;
            c.push(env.component(
                "Link",
                "Button",
                &look::Spec::new([inner, 22.0], &*m::panel_more_below(below)),
                "People.Down",
                [x0 + inner / 2.0, ry + MARK / 2.0],
            ));
        }
        c.push(button(
            &env,
            "Close",
            &common::close(),
            [x + w - 24.0 - 70.0, y + h - 36.0],
            140.0,
            "Button",
        ));
        self.env.state.borrow_mut().page_items =
            self.focusables().into_iter().map(|f| f.name).collect();
        publish_dialog(&env, |v| v.host_open = true);
        fitted_dialog(c, screen, h, scale)
    }

    fn focusables(&self) -> Vec<Focusable> {
        let pair = |v: &mut Vec<Focusable>, a: String, b: String| {
            v.push(Focusable {
                right: Some(b.clone()),
                ..Focusable::new(a.clone())
            });
            v.push(Focusable {
                left: Some(a),
                ..Focusable::new(b)
            });
        };
        let mut v = Vec::new();
        let succession = self.room.borrow().has_succession();
        if succession {
            pair(&mut v, "Dec.leaves".into(), "Inc.leaves".into());
        }
        let people = self.people();
        let shown = people.len().min(self.fit);
        if people.len() > shown {
            v.push(Focusable::new("People.Up"));
        }
        for p in people.iter().skip(self.first).take(shown) {
            if succession {
                pair(
                    &mut v,
                    format!("CoHost.{}", p.id),
                    format!("MakeHost.{}", p.id),
                );
            } else {
                v.push(Focusable::new(format!("CoHost.{}", p.id)));
            }
        }
        if people.len() > shown {
            v.push(Focusable::new("People.Down"));
        }
        v.push(Focusable::new("Close"));
        v
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        // No longer the host's (the room handed over, the link gone).
        if !self.room.borrow().manages_hosts() {
            cx.close(true);
            return;
        }
        if let Some(name) = self.focus_first.take() {
            self.at = None;
            if self.focusables().iter().any(|f| f.name == name) {
                cx.focus(name);
            }
            self.rebuild(root, cx);
            return;
        }
        if self.room.borrow().version() != self.seen {
            self.rebuild(root, cx);
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Wheel { delta, .. } => {
                self.scroll(if delta[1] > 0.0 { 1 } else { -1 }, root, cx);
                Handled::Yes
            }
            Input::Activate { name } => {
                match name.as_str() {
                    "Dec.leaves" | "Inc.leaves" => {
                        let policy = if name == "Inc.leaves" {
                            HostLeaves::Close
                        } else {
                            HostLeaves::PassOn
                        };
                        let _ = self.room.borrow_mut().set_host_leaves(policy);
                        self.rebuild(root, cx);
                    }
                    "People.Up" => self.scroll(-1, root, cx),
                    "People.Down" => self.scroll(1, root, cx),
                    "Close" => cx.close(true),
                    other => {
                        let person = |prefix: &str| {
                            other
                                .strip_prefix(prefix)
                                .and_then(|n| n.parse::<u64>().ok())
                                .and_then(|id| self.people().into_iter().find(|p| p.id == id))
                        };
                        if let Some(p) = person("CoHost.")
                            && p.watch_only
                        {
                            let _ = self.room.borrow_mut().let_play(ParticipantId(p.id));
                            self.rebuild(root, cx);
                        } else if let Some(p) = person("CoHost.") {
                            let _ = self
                                .room
                                .borrow_mut()
                                .set_co_host(ParticipantId(p.id), !p.co_host);
                            self.rebuild(root, cx);
                        } else if let Some(p) = person("MakeHost.")
                            && p.can_host
                        {
                            // Asked once more: the giver cannot take it
                            // back alone.
                            let room = self.room.clone();
                            let id = ParticipantId(p.id);
                            cx.overlay(
                                super::dialog::ConfirmDialog::new(
                                    self.env.clone(),
                                    &m::make_host_title(p.name.as_str()),
                                    &m::make_host_body(p.name.as_str()),
                                    &m::make_host(),
                                    Rc::new(move |_| {
                                        let _ = room.borrow_mut().make_host(id);
                                    }),
                                ),
                                true,
                            );
                        }
                    }
                }
                Handled::Yes
            }
            Input::Outside => {
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

impl Drop for HostDialog {
    fn drop(&mut self) {
        publish_dialog(&self.env, |v| v.host_open = false);
    }
}

impl Drop for PlayersDialog {
    fn drop(&mut self) {
        publish_dialog(&self.env, |v| v.players_open = false);
    }
}

/// A stage's standings after its results, or the match's results (M7b:
/// results, then standings and points, then song select): the player's
/// result, everyone's place, each stage's points and the total; Continue.
pub struct StandingsDialog {
    env: Env,
    room: SharedRoom,
    /// The stage's number, or `None`: the match's results.
    stage: Option<u16>,
    first: usize,
}

const STANDING_ROWS: usize = 9;

impl StandingsDialog {
    pub fn new(env: Env, room: SharedRoom, stage: Option<u16>) -> StandingsDialog {
        StandingsDialog {
            env,
            room,
            stage,
            first: 0,
        }
    }
}

impl Screen for StandingsDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "room-standings"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let env = self.env.clone();
        let r = self.room.borrow();
        let s = r.state();
        let standings = s.standings().unwrap_or_default();
        let shown = standings.len().min(STANDING_ROWS);
        self.first = scrolled(self.first, 0, standings.len(), shown);
        let w = 600.0_f32.min(screen[0] - 16.0);
        let inner = w - 48.0;
        let row = 30.0;
        let mark = match self.stage {
            Some(_) => r.last_mark().map(|mk| mark_text(&mk)),
            None => None,
        };
        let completed = s.phase == Phase::MatchResults;
        let title: String = match self.stage {
            Some(n) => m::standings_title(n).into(),
            None if completed => m::results_title().into(),
            None => m::results_stopped().into(),
        };
        let name = |id| match s.member(id) {
            Some(mb) if mb.id == r.me() => String::from(m::you()),
            Some(mb) => match &mb.kind {
                MemberKind::Cpu { ordinal, opponent } => cpu_name(opponent, *ordinal),
                MemberKind::Human { .. } => super::room::person_name(mb),
            },
            None => String::new(),
        };
        let mut lines: Vec<(String, &str)> = Vec::new();
        if let Some(mark) = &mark {
            lines.push((mark.clone(), "Secondary"));
        }
        if self.stage.is_none() && completed {
            let winners: Vec<String> = standings
                .iter()
                .filter(|st| st.rank == 1)
                .map(|st| name(st.participant))
                .collect();
            lines.push((
                m::results_winners(winners.len(), UiText::data(winners.join(", "))).into(),
                "Gold",
            ));
        }
        let wrapped: Vec<(String, &str)> = lines
            .iter()
            .flat_map(|(t, c)| {
                super::dialog::wrap_lines(t, inner, 14.0)
                    .into_iter()
                    .map(move |l| (l, *c))
            })
            .collect();
        let more = standings.len() > shown;
        let h = 64.0
            + wrapped.len() as f32 * 20.0
            + 30.0
            + shown as f32 * row
            + if more { 24.0 } else { 0.0 }
            + 72.0;
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &title);
        let x0 = x + 24.0;
        let mut ry = y + 64.0;
        for (k, (line, color)) in wrapped.iter().enumerate() {
            let mut t = env.text(&format!("Line{k}"), line, 14.0, 600, color);
            look::ellipsized(&mut t, inner);
            c.push(left(t, x0, ry + 10.0));
            ry += 20.0;
        }
        // The table: place, name, each stage (where they fit), the total.
        let stages = standings.first().map_or(0, |st| st.stages.len());
        let cell = 40.0;
        let total_w = 64.0;
        let show_stages = inner - 36.0 - 120.0 - total_w >= cell * stages as f32;
        let hy = ry + 15.0;
        c.push(right(
            env.text("Total", &m::results_total(), 11.0, 700, "Muted"),
            x0 + inner,
            hy,
        ));
        if show_stages {
            for i in 0..stages {
                let at = x0 + inner - total_w - cell * (stages - 1 - i) as f32;
                c.push(right(
                    env.text(
                        &format!("S{i}"),
                        &m::results_stage(i + 1),
                        11.0,
                        700,
                        "Muted",
                    ),
                    at,
                    hy,
                ));
            }
        }
        ry += 30.0;
        // The local player's line stays in view however far it is
        // scrolled (a large room's).
        let me_at = standings.iter().position(|st| st.participant == r.me());
        let window = super::room::window_keeping(standings.len(), self.first, shown, me_at);
        for (k, st) in window.into_iter().map(|k| (k, &standings[k])) {
            let cy = ry + row / 2.0;
            let me = st.participant == r.me();
            let color = if me { "Text" } else { "Secondary" };
            c.push(left(
                env.numbers(&format!("Rank{k}"), &st.rank.to_string(), 14.0, "Muted"),
                x0,
                cy,
            ));
            let mut n = env.text(&format!("Name{k}"), &name(st.participant), 14.0, 600, color);
            let room = inner
                - 36.0
                - total_w
                - if show_stages {
                    cell * stages as f32
                } else {
                    0.0
                };
            look::ellipsized(&mut n, room.max(40.0));
            c.push(left(n, x0 + 36.0, cy));
            c.push(right(
                env.numbers(
                    &format!("Total{k}"),
                    &super::room::points_text(st.total),
                    14.0,
                    color,
                ),
                x0 + inner,
                cy,
            ));
            if show_stages {
                for (i, cell_of) in st.stages.iter().enumerate() {
                    let text: String = match cell_of {
                        StageStanding::Placed(p) => super::room::points_text(p.points).into(),
                        _ => m::results_none().into(),
                    };
                    let at = x0 + inner - total_w - cell * (stages - 1 - i) as f32;
                    c.push(right(
                        env.numbers(&format!("Cell{k}.{i}"), &text, 13.0, "Muted"),
                        at,
                        cy,
                    ));
                }
            }
            ry += row;
        }
        if more {
            let words = m::panel_more_below(standings.len().saturating_sub(self.first + shown));
            c.push(left(
                env.text("More", &words, 12.0, 400, "Faint"),
                x0,
                ry + 12.0,
            ));
        }
        drop(r);
        c.push(button(
            &env,
            "Continue",
            &m::standings_continue(),
            [x + w - 24.0 - 70.0, y + h - 36.0],
            140.0,
            "Button",
        ));
        self.env.state.borrow_mut().page_items = vec!["Continue".into()];
        publish_dialog(&env, |v| v.standings_open = true);
        fitted_dialog(c, screen, h, scale)
    }

    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("Continue")]
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let len = self
            .room
            .borrow()
            .state()
            .standings()
            .map_or(0, |s| s.len());
        let fit = len.min(STANDING_ROWS);
        let by = match input {
            Input::Menu {
                action: MenuAction::Down,
                ..
            } => 1,
            Input::Menu {
                action: MenuAction::Up,
                ..
            } => -1,
            Input::Wheel { delta, .. } => {
                if delta[1] > 0.0 {
                    1
                } else {
                    -1
                }
            }
            Input::Activate { .. } | Input::Outside => {
                self.room.borrow_mut().results_shown();
                cx.close(false);
                return Handled::Yes;
            }
            _ => return Handled::No,
        };
        self.first = scrolled(self.first, by, len, fit);
        *root = self.build(cx);
        super::restore_focus(root, Some("Continue"));
        Handled::Yes
    }
}

impl Drop for StandingsDialog {
    fn drop(&mut self) {
        publish_dialog(&self.env, |v| v.standings_open = false);
    }
}
