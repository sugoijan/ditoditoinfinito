//! The room's Rules pop-up (`docs/plans/multiplayer-ui.md`, "Room
//! creation and rules study"): the running match's rules, locked, beside
//! the next match's, which the player edits and applies. In the lobby the
//! next match's rules are the ones the match will start with, and
//! applying them clears readiness (the room's rule, not the dialog's);
//! during a match they are an explicit draft that leaves the match alone.
//! The room validates what is applied; the dialog only words its answer.

use ddi_actor::Actor;
use ddi_l10n::msg::{common, room as m};
use ddi_room::MatchRules;

use super::dialog::{button, fitted_dialog, shell};
use super::{Env, left, right};
use crate::look;
use crate::nav::Focusable;
use crate::screen::{Cx, Handled, Input, Screen};
use crate::state::room::{SharedRoom, cpu_levels};

/// Broadcast when the dialog applied new rules: the room redraws.
pub const RULES_SAVED: &str = "Room.RulesSaved";

/// A rule the dialog edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    Stages,
    Cpus,
    Level,
}

impl Rule {
    const ALL: [Rule; 3] = [Rule::Stages, Rule::Cpus, Rule::Level];

    fn id(self) -> &'static str {
        match self {
            Rule::Stages => "stages",
            Rule::Cpus => "cpus",
            Rule::Level => "level",
        }
    }
}

const ROW: f32 = 40.0;

/// A refusal said in a pop-up (song select choosing a stage's song:
/// a chart the CPUs cannot play): its title, its words, Close.
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

    /// A chart refused for a stage, as `why` says.
    pub fn refusal(env: Env, why: &crate::state::room::Refusal) -> NoticeDialog {
        let body: String = super::room::refusal_text(why).into();
        NoticeDialog::new(env, &m::chart_unsupported_title(), &body)
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

pub struct RulesDialog {
    env: Env,
    room: SharedRoom,
    /// The next match's rules as edited, not yet applied.
    draft: MatchRules,
    /// What the room said of the last Apply, or of the draft now.
    problem: Option<String>,
}

impl RulesDialog {
    pub fn new(env: Env, room: SharedRoom) -> RulesDialog {
        let draft = room.borrow().state().next.rules.clone();
        RulesDialog {
            env,
            room,
            draft,
            problem: None,
        }
    }

    fn value(&self, rule: Rule) -> u16 {
        let room = self.room.borrow();
        match rule {
            Rule::Stages => self.draft.stages,
            Rule::Cpus => room.cpus_of(&self.draft),
            Rule::Level => u16::from(self.draft.cpu.opponent.level) + 1,
        }
    }

    /// The range a rule may take: the room's policy and cap, the engine's
    /// levels.
    fn range(&self, rule: Rule) -> (u16, u16) {
        let room = self.room.borrow();
        match rule {
            Rule::Stages => (1, room.state().policy.max_stages),
            Rule::Cpus => (room.min_cpus(), room.max_cpus()),
            Rule::Level => (1, u16::from(cpu_levels())),
        }
    }

    fn step(&mut self, rule: Rule, up: bool) {
        let (lo, hi) = self.range(rule);
        let v = self.value(rule);
        let v = if up {
            v.saturating_add(1)
        } else {
            v.saturating_sub(1)
        }
        .clamp(lo, hi);
        match rule {
            Rule::Stages => self.draft.stages = v,
            Rule::Cpus => self.draft = self.room.borrow().with_cpus(self.draft.clone(), v),
            Rule::Level => self.draft.cpu.opponent.level = (v - 1) as u8,
        }
        self.problem = None;
    }

    fn rule_text(rule: Rule) -> String {
        match rule {
            Rule::Stages => m::rules_stages(),
            Rule::Cpus => m::rules_cpus(),
            Rule::Level => m::rules_level(),
        }
        .into()
    }

    fn value_text(&self, rule: Rule, rules: &MatchRules) -> String {
        let room = self.room.borrow();
        match rule {
            Rule::Stages => ddi_l10n::UiText::data(rules.stages.to_string()).into(),
            Rule::Cpus => ddi_l10n::UiText::data(room.cpus_of(rules).to_string()).into(),
            Rule::Level => m::rules_level_value(
                u16::from(rules.cpu.opponent.level) + 1,
                u16::from(cpu_levels()),
            )
            .into(),
        }
    }

    /// The rows every column shows below the edited ones: what is not
    /// chosen here yet, each marked as such.
    fn fixed_rows() -> [(String, String); 2] {
        [
            (m::rules_battle().into(), m::rules_battle_value().into()),
            (m::rules_points().into(), m::rules_points_value().into()),
        ]
    }

    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.page_items = self.focusables().into_iter().map(|f| f.name).collect();
        if let Some(view) = &mut st.room {
            view.rules_open = true;
            view.draft = Rule::ALL.map(|r| self.value(r)).to_vec();
            view.rules_problem = self.problem.clone();
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
        let locked = self.room.borrow().locked_rules().cloned();
        let col: f32 = 300.0;
        let columns: f32 = if locked.is_some() { 2.0 } else { 1.0 };
        let w = (columns * col + (columns - 1.0) * 32.0 + 48.0_f32).min(screen[0] - 16.0);
        let col = if locked.is_some() {
            (w - 48.0 - 32.0) / 2.0
        } else {
            w - 48.0
        };
        let rows = Rule::ALL.len() + Self::fixed_rows().len();
        let mut notes: Vec<(String, &str)> = vec![
            (m::rules_level_note().into(), "Muted"),
            (m::rules_points_note().into(), "Muted"),
        ];
        notes.push((
            if locked.is_some() {
                m::rules_locked_note()
            } else {
                m::rules_clears_note()
            }
            .into(),
            "Muted",
        ));
        let problem = self.problem.clone().or_else(|| {
            self.room
                .borrow()
                .check_rules(&self.draft)
                .err()
                .map(|p| super::room::rules_problem(&p).into())
        });
        if let Some(p) = &problem {
            notes.push((p.clone(), "Gold"));
        }
        let lines: usize = notes
            .iter()
            .map(|(t, _)| super::dialog::wrap_lines(t, w - 48.0, 12.0).len())
            .sum();
        let h = 64.0 + 32.0 + rows as f32 * ROW + 16.0 + lines as f32 * 17.0 + 72.0;
        let (mut c, [x, y]) = shell(&env, screen, [w, h], &m::rules_title());
        let top = y + 64.0;
        // The running match's, locked.
        let mut cx0 = x + 24.0;
        if let Some(rules) = &locked {
            c.push(left(
                env.text("Locked.Heading", &m::rules_this_match(), 13.0, 700, "Muted"),
                cx0,
                top + 8.0,
            ));
            for (i, rule) in Rule::ALL.iter().enumerate() {
                let ry = top + 32.0 + i as f32 * ROW + ROW / 2.0;
                c.push(left(
                    env.text(
                        &format!("Locked.{}", rule.id()),
                        &Self::rule_text(*rule),
                        14.0,
                        400,
                        "Secondary",
                    ),
                    cx0,
                    ry,
                ));
                c.push(right(
                    env.text(
                        &format!("Locked.{}.Value", rule.id()),
                        &self.value_text(*rule, rules),
                        14.0,
                        600,
                        "Text",
                    ),
                    cx0 + col,
                    ry,
                ));
            }
            cx0 += col + 32.0;
        }
        // The next match's, edited.
        c.push(left(
            env.text("Next.Heading", &m::rules_next_match(), 13.0, 700, "Muted"),
            cx0,
            top + 8.0,
        ));
        for (i, rule) in Rule::ALL.iter().enumerate() {
            let ry = top + 32.0 + i as f32 * ROW + ROW / 2.0;
            let mut label = env.text(
                &format!("Rule.{}", rule.id()),
                &Self::rule_text(*rule),
                14.0,
                400,
                "Secondary",
            );
            look::ellipsized(&mut label, col - 170.0);
            c.push(left(label, cx0, ry));
            let (lo, hi) = self.range(*rule);
            let v = self.value(*rule);
            let less = if v > lo { "Button" } else { "Button.Disabled" };
            let more = if v < hi { "Button" } else { "Button.Disabled" };
            c.push(button(
                &env,
                &format!("Dec.{}", rule.id()),
                &m::rules_less(),
                [cx0 + col - 150.0, ry],
                36.0,
                less,
            ));
            c.push(env.text(
                &format!("Rule.{}.Value", rule.id()),
                &self.value_text(*rule, &self.draft),
                14.0,
                600,
                "Text",
            ));
            if let Some(t) = c.last_mut() {
                look::call(t, "xy", &[cx0 + col - 76.0, ry]);
            }
            c.push(button(
                &env,
                &format!("Inc.{}", rule.id()),
                &m::rules_more(),
                [cx0 + col - 18.0, ry],
                36.0,
                more,
            ));
        }
        // What is not chosen here yet, in both columns.
        for (i, (label, value)) in Self::fixed_rows().iter().enumerate() {
            let ry = top + 32.0 + (Rule::ALL.len() + i) as f32 * ROW + ROW / 2.0;
            let mut columns = vec![cx0];
            if locked.is_some() {
                columns.insert(0, x + 24.0);
            }
            for (k, cxk) in columns.into_iter().enumerate() {
                c.push(left(
                    env.text(&format!("Fixed{k}.{i}"), label, 14.0, 400, "Secondary"),
                    cxk,
                    ry,
                ));
                let mut v = env.text(&format!("Fixed{k}.{i}.Value"), value, 13.0, 400, "Muted");
                look::ellipsized(&mut v, col - 110.0);
                c.push(right(v, cxk + col, ry));
            }
        }
        // The notes, and the room's answer, wrapped.
        let mut ny = top + 32.0 + rows as f32 * ROW + 16.0;
        let mut k = 0;
        for (text, color) in &notes {
            for line in super::dialog::wrap_lines(text, w - 48.0, 12.0) {
                let mut t = env.text(&format!("Note{k}"), &line, 12.0, 400, color);
                look::ellipsized(&mut t, w - 48.0);
                c.push(left(t, x + 24.0, ny));
                ny += 17.0;
                k += 1;
            }
        }
        c.push(button(
            &env,
            "Cancel",
            &common::cancel(),
            [x + w - 24.0 - 140.0 - 10.0 - 70.0, y + h - 36.0],
            140.0,
            "Button",
        ));
        c.push(button(
            &env,
            "Apply",
            &m::rules_apply(),
            [x + w - 24.0 - 70.0, y + h - 36.0],
            140.0,
            "Button",
        ));
        self.publish();
        fitted_dialog(c, screen, h, scale)
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut v = Vec::new();
        for rule in Rule::ALL {
            v.push(Focusable::new(format!("Dec.{}", rule.id())));
            v.push(Focusable::new(format!("Inc.{}", rule.id())));
        }
        v.push(Focusable::new("Cancel"));
        v.push(Focusable::new("Apply"));
        v
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Activate { name } => {
                let stepped = Rule::ALL.iter().find_map(|r| {
                    if *name == format!("Dec.{}", r.id()) {
                        Some((*r, false))
                    } else if *name == format!("Inc.{}", r.id()) {
                        Some((*r, true))
                    } else {
                        None
                    }
                });
                if let Some((rule, up)) = stepped {
                    self.step(rule, up);
                    *root = self.build(cx);
                    super::restore_focus(root, Some(name));
                    return Handled::Yes;
                }
                match name.as_str() {
                    "Cancel" => self.close(cx, true),
                    "Apply" => {
                        let saved = self.room.borrow_mut().save_rules(self.draft.clone());
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
                    _ => {}
                }
                Handled::Yes
            }
            Input::Outside => {
                self.close(cx, true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

impl RulesDialog {
    fn close(&mut self, cx: &mut Cx, cancel: bool) {
        cx.close(cancel);
    }
}

/// However it closed (Apply, Cancel, Back, a press outside), the debug
/// state stops saying it is open.
impl Drop for RulesDialog {
    fn drop(&mut self) {
        if let Ok(mut st) = self.env.state.try_borrow_mut()
            && let Some(view) = &mut st.room
        {
            view.rules_open = false;
        }
    }
}
