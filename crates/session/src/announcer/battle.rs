//! The announcer in a multiplayer stage (`docs/plans/multiplayer.md`,
//! "Announcer events implied by the mechanism" and "Conflicts and
//! freshness"; `docs/plans/multiplayer-prototype-progress.md`, M4): what
//! a stage's play decodes and speaks of the battle's lines
//! (`ddi_library::announcer::Battle`, folders `battle …`), which moment
//! wins when two meet, and which moments show on screen instead where the
//! pack has no line for them. An attack's two moments
//! (`crate::battle::AttackMoment`) are battle lines as the rest: one
//! against the player said at its commitment, which is before its shocks
//! can be seen, or not at all (its deadline, [`Announcer::battle_by`]),
//! one of their own at the lowest priority in play; each shown in the
//! status line where it was not said.
//!
//! [`Announcer::battle_by`]: super::Announcer::battle_by
//!
//! The policy is [`BATTLE_VOICE_V1`], PROVISIONAL: its numbers are
//! nobody's tuning yet, and are kept here alone. A battle line never
//! waits behind another battle line: one due while a line that matters as
//! much or more is still speaking is passed over (shown, not said late),
//! and nothing is queued (a canceled start, a state that ended, leave
//! nothing behind). The voice bus cannot cut a sound short, so a line that
//! matters more starts over the end of one that matters less. Two
//! exceptions, each scheduled on the audio clock rather than queued: a
//! step's line (`battle step up`, `battle step down`) never talks over a
//! line being said, starting at its end where that is within
//! [`VoicePolicy::step_window`] of the step's moment, else passed over;
//! and a battle line due while a combo line speaks starts at the combo
//! line's end (they are short). The combo lines ([`Line::is_combo`]) came
//! back into a stage with `battle-voice-v1` (feedback round J), below
//! every battle line: said only where nothing speaks or is due over them,
//! up to [`VoicePolicy::combo_window`] late, else dropped. Judging, the
//! clock and the music never wait for any of it.

use ddi_library::announcer::{Battle as Event, Kept, Line, Spoken};
use ddi_library::path::RelPathBuf;

use super::{Bags, speaks, take};
use crate::battle::AttackMoment;

/// How a stage's voice is played: which of the play's own lines it keeps,
/// how the battle's moments rank and how long a moment shows on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoicePolicy {
    /// Its name, for the progress note and the debug state.
    pub id: &'static str,
    /// Seconds a moment shows on screen where its line was not said.
    pub cue_seconds: f64,
    /// The cue's fade in and out, seconds (none under reduced motion).
    pub cue_fade: f64,
    /// Seconds within which a second attack against the player is not
    /// told again, by line or by cue (a chained attack right behind the
    /// first: the warning by the receptors says it).
    pub incoming_again: f64,
    /// Seconds after a step's moment (its new chart's notes beginning)
    /// within which its line may still start, after a line being said;
    /// later it is passed over (and shown).
    pub step_window: f64,
    /// Seconds after the combo reaches its count (or breaks) within which
    /// its line may still start, after a line being said; later, or where
    /// a battle line is due while it would speak, it is dropped.
    pub combo_window: f64,
}

/// What a combo line speaking counts as among the battle's priorities:
/// below every battle line ([`VoicePolicy::priority`] gives none under 5).
pub const COMBO_PRIORITY: u8 = 0;

/// The battle voice of the prototype (`battle-voice-v1`, feedback round
/// J, 2026-11-15; it replaced `battle-voice-v0`, which left the combo
/// lines out and had no step lines): one battle line at a time, outcomes
/// over an attack against the player over the player's own life over a
/// step of the player's own (up or down) over who is left over an attack
/// of the player's own over the stage's opening and the winner's finished
/// song; a step's line never over a line being said, at most a second
/// late; the combo lines below them all, said only in a quiet moment, at
/// most a second late, never over a battle line nor one due; the rest of
/// a play's routine lines left out of a stage (a chart's clear is not a
/// stage's win); moments shown 2.5 s.
pub const BATTLE_VOICE_V1: VoicePolicy = VoicePolicy {
    id: "battle-voice-v1",
    cue_seconds: 2.5,
    cue_fade: 0.25,
    incoming_again: 6.0,
    step_window: 1.0,
    combo_window: 1.0,
};

impl VoicePolicy {
    /// How much `event` matters when two meet: the higher speaks, the
    /// other is passed over. The design's order: terminal local states
    /// and confirmed outcomes, then an attack against the player, then
    /// critical life, then a step of the player's own, then milestones,
    /// then feedback (an attack of the player's own just under the
    /// milestones). The combo lines are under all of them
    /// ([`COMBO_PRIORITY`]).
    pub fn priority(&self, event: Event) -> u8 {
        match event {
            Event::Eliminated
            | Event::StageWon
            | Event::StageNotWon
            | Event::StageDrawn
            | Event::MatchWon
            | Event::MatchNotWon
            | Event::MatchDrawn => 40,
            Event::AttackIncoming => 30,
            Event::LifeCritical => 20,
            Event::StepUp | Event::StepDown => 12,
            Event::Survivors(_) => 10,
            Event::AttackLaunched => 8,
            Event::MatchBegin | Event::StageBegin | Event::FinalStage | Event::SongFinished => 5,
        }
    }

    /// How much an attack's moment matters among the battle's
    /// ([`VoicePolicy::priority`]'s scale): one against the player just
    /// under the outcomes (the 30 the design kept for it), one of their
    /// own just under the milestones.
    pub fn attack_priority(&self, moment: AttackMoment) -> u8 {
        self.priority(moment.event())
    }

    /// Whether the play shows `event` beside the field where no battle
    /// line said it: the moments of the play itself. The stage won early
    /// shows already ("Stage won", for the rest of the song); the
    /// results' moments show on the results and in the room. An attack's
    /// moments are told apart ([`AttackTold`], [`StageCue::Attack`]).
    pub fn shows(&self, event: Event) -> bool {
        matches!(
            event,
            Event::MatchBegin
                | Event::StageBegin
                | Event::FinalStage
                | Event::LifeCritical
                | Event::Survivors(_)
                | Event::Eliminated
                | Event::StageDrawn
        )
    }

    /// How opaque a cue `age` seconds old is, `0..=1` (gone past
    /// [`VoicePolicy::cue_seconds`]): faded in and out, or on and off
    /// under reduced motion.
    pub fn cue_alpha(&self, age: f64, reduced: bool) -> f32 {
        if !(0.0..self.cue_seconds).contains(&age) {
            return 0.0;
        }
        if reduced || self.cue_fade <= 0.0 {
            return 1.0;
        }
        let edge = age.min(self.cue_seconds - age) / self.cue_fade;
        edge.clamp(0.0, 1.0) as f32
    }
}

/// Whether a stage speaks `line`: the battle's lines and those spoken in
/// their place, the play's here we go, the combo lines
/// ([`Line::is_combo`], `battle-voice-v1`) and a quit's results (the rest
/// of a play's lines are left out of a stage).
pub fn stage_speaks(line: Line) -> bool {
    match line {
        Line::Battle(_) | Line::HereWeGo | Line::Quit => true,
        l if l.is_combo() => true,
        _ => Event::ALL
            .iter()
            .any(|&e| Line::Battle(e).chain().contains(&line)),
    }
}

/// Whether a stage leaves `line` out: a play's line no battle moment
/// falls back on.
pub fn routine(line: Line) -> bool {
    matches!(line.spoken(), Spoken::Play | Spoken::Results) && !stage_speaks(line)
}

/// The sounds a stage's play decodes ([`super::wanted`]'s for a stage):
/// those of the lines it speaks ([`stage_speaks`]), up to
/// [`super::variants`] of each, its opening's first.
pub fn wanted(kept: &Kept, bags: &mut Bags) -> Vec<(Line, Vec<RelPathBuf>)> {
    let mut lines = take(kept, bags, |line| {
        stage_speaks(line) && (speaks(line) || line.spoken() == Spoken::Battle)
    });
    lines.sort_by_key(|(line, _)| match line {
        Line::Battle(e) if e.opens() => 0,
        Line::Ready => 1,
        Line::HereWeGo => 2,
        _ => 3,
    });
    lines
}

/// A battle's moment as the play told it: the event, the line said for
/// it (one of its chain: `None`, nothing was), and when (host seconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Told {
    pub event: Event,
    pub said: Option<Line>,
    pub at: f64,
}

impl Told {
    /// Its own line said it (one in its place, as ready or `battle stage
    /// begin` for the final stage, does not name the moment).
    pub fn spoken(&self) -> bool {
        self.said == Some(Line::Battle(self.event))
    }

    /// Whether the play shows it beside the field under `policy`.
    pub fn shown(&self, policy: &VoicePolicy) -> bool {
        !self.spoken() && policy.shows(self.event)
    }
}

/// An attack's moment as the play told it: its line where it was said
/// (`None`: it was not, and the status line shows it), and when (host
/// seconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackTold {
    pub moment: AttackMoment,
    pub said: Option<Line>,
    pub at: f64,
}

impl AttackTold {
    /// Its line said it (so the status line does not).
    pub fn spoken(&self) -> bool {
        self.said.is_some()
    }
}

/// What a stage's status line shows: a battle's moment no line said, or
/// an attack's no line said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageCue {
    Event(Event),
    Attack(AttackMoment),
    /// The player steps to this rung of the stage's ladder (a match's
    /// "Charts: Adaptive"): down where their life ran out, up after full
    /// bars; no line says it, the status line does ("Stepping down:
    /// Medium", "Stepping up: Challenge").
    Step { rung: u8, up: bool },
    /// The player's life ran out on a chart a second time, so it is
    /// closed to them for the stage (`adaptive-v1`): they stay on this
    /// rung's chart ("Staying on Hard"), said once instead of the step
    /// down.
    Stay { rung: u8 },
}

/// A stage's voice: the policy, the line speaking and until when (audio
/// clock), how much it matters (a combo line [`COMBO_PRIORITY`]), the
/// battle lines known to be due (a step's, decided ahead of its
/// boundary: audio clock), and the results' line kept until the room has
/// confirmed the stage.
#[derive(Clone, Debug)]
pub(super) struct StageVoice {
    pub policy: VoicePolicy,
    pub busy: Option<(f64, u8)>,
    pub due: Vec<f64>,
    pub end: Option<Event>,
    /// The line the opening was said with, once it was (`Some(None)`:
    /// nothing was).
    pub opened: Option<Option<Line>>,
}

impl StageVoice {
    pub fn new(policy: VoicePolicy) -> StageVoice {
        StageVoice {
            policy,
            busy: None,
            due: Vec::new(),
            end: None,
            opened: None,
        }
    }

    /// When `event` due at `at` may start: at once where nothing speaks
    /// or it matters more than what does, at the end of a combo line
    /// speaking (never over it), else never (`None`).
    pub fn start(&self, event: Event, at: f64) -> Option<f64> {
        match self.busy {
            Some((until, COMBO_PRIORITY)) if at < until => Some(until),
            Some((until, p)) if at < until => (self.policy.priority(event) > p).then_some(at),
            _ => Some(at),
        }
    }

    /// When a line that never talks over another, due at `at`, may start:
    /// at once, or at the end of the line being said.
    pub fn after_busy(&self, at: f64) -> f64 {
        match self.busy {
            Some((until, _)) if at < until => until,
            _ => at,
        }
    }

    /// Whether a battle line is due within `from..to` (audio clock).
    pub fn due_within(&self, from: f64, to: f64) -> bool {
        self.due.iter().any(|&d| d >= from - 1e-9 && d < to)
    }
}

/// How a match ended for the local player, as the room confirmed it: its
/// line on the match's last stage's results.
pub fn match_event(end: ddi_ui::state::room::MatchEnd) -> Event {
    use ddi_ui::state::room::MatchEnd;
    match end {
        MatchEnd::Won => Event::MatchWon,
        MatchEnd::Drawn => Event::MatchDrawn,
        MatchEnd::NotWon => Event::MatchNotWon,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_priorities_follow_the_design_s_order() {
        let p = BATTLE_VOICE_V1;
        assert!(p.priority(Event::Eliminated) > p.priority(Event::LifeCritical));
        assert!(p.priority(Event::StageWon) > p.priority(Event::LifeCritical));
        assert!(p.priority(Event::LifeCritical) > p.priority(Event::Survivors(2)));
        assert!(p.priority(Event::Survivors(4)) > p.priority(Event::SongFinished));
        assert_eq!(
            p.priority(Event::MatchWon),
            p.priority(Event::StageWon),
            "both outcomes"
        );
        // An attack against the player: under the outcomes, over their
        // life; one of their own: under the milestones, over the openings.
        assert_eq!(p.priority(Event::AttackIncoming), 30);
        assert_eq!(p.priority(Event::AttackLaunched), 8);
        assert!(p.priority(Event::Eliminated) > p.priority(Event::AttackIncoming));
        assert!(p.priority(Event::AttackIncoming) > p.priority(Event::LifeCritical));
        assert!(p.priority(Event::Survivors(2)) > p.priority(Event::AttackLaunched));
        assert!(p.priority(Event::AttackLaunched) > p.priority(Event::StageBegin));
        // A step of the player's own: under an attack against them and
        // their life, over who is left; the combo lines under every one.
        for step in [Event::StepUp, Event::StepDown] {
            assert!(p.priority(Event::AttackIncoming) > p.priority(step));
            assert!(p.priority(Event::StageWon) > p.priority(step));
            assert!(p.priority(step) > p.priority(Event::Survivors(2)));
            assert!(p.priority(step) > p.priority(Event::AttackLaunched));
        }
        for e in Event::ALL {
            assert!(p.priority(e) > COMBO_PRIORITY, "{e:?}");
        }
        assert_eq!(p.id, "battle-voice-v1");
        for m in [AttackMoment::Incoming, AttackMoment::Launched] {
            assert_eq!(p.attack_priority(m), p.priority(m.event()));
        }
    }

    #[test]
    fn a_stage_keeps_the_battle_lines_and_their_fallbacks_alone() {
        for line in Line::all() {
            let kept = stage_speaks(line);
            let expected = matches!(
                line,
                Line::Battle(_)
                    | Line::Ready
                    | Line::HereWeGo
                    | Line::CommentDanger
                    | Line::Failed
                    | Line::Cleared
                    | Line::Quit
                    | Line::Combo(_)
                    | Line::ComboOverflow
                    | Line::ComboStopped
            );
            assert_eq!(kept, expected, "{line:?}");
        }
        // The combo lines came back (`battle-voice-v1`).
        for combo in [Line::Combo(100), Line::Combo(64), Line::ComboOverflow, Line::ComboStopped] {
            assert!(!routine(combo) && stage_speaks(combo), "{combo:?}");
        }
        for routine_line in [
            Line::CommentHot,
            Line::CommentGood,
            Line::FullCombo,
            Line::LifeLost,
            Line::LastLife,
            Line::Restart,
            Line::Grade(ddi_library::announcer::Grade::Aa),
            Line::Cheer,
            Line::NewRecord,
        ] {
            assert!(routine(routine_line), "{routine_line:?}");
        }
        assert!(!routine(Line::SelectMusicIntro));
    }

    #[test]
    fn a_stage_decodes_its_lines_its_opening_first() {
        let mut kept = Kept::default();
        for folder in [
            "gameplay 100 combo",
            "gameplay here we go normal",
            "gameplay ready",
            "battle final stage",
            "battle stage won",
            "evaluation aa",
        ] {
            kept.lines.insert(folder.into(), vec!["a.ogg".into()]);
        }
        let mut bags = Bags::new(1);
        let lines: Vec<Line> = wanted(&kept, &mut bags)
            .into_iter()
            .map(|(l, _)| l)
            .collect();
        assert_eq!(
            lines,
            [
                Line::Battle(Event::FinalStage),
                Line::Ready,
                Line::HereWeGo,
                Line::Battle(Event::StageWon),
                Line::Combo(100)
            ]
        );
        // A play of one's own decodes none of the battle's.
        let solo: Vec<Line> = super::super::wanted(&kept, 300, &mut bags)
            .into_iter()
            .map(|(l, _)| l)
            .collect();
        assert!(
            !solo.iter().any(|l| matches!(l, Line::Battle(_))),
            "{solo:?}"
        );
    }

    #[test]
    fn a_cue_fades_in_and_out_or_not_at_all_under_reduced_motion() {
        let p = BATTLE_VOICE_V1;
        assert_eq!(p.cue_alpha(-0.1, false), 0.0);
        assert!(p.cue_alpha(0.05, false) < 1.0);
        assert_eq!(p.cue_alpha(1.0, false), 1.0);
        assert!(p.cue_alpha(p.cue_seconds - 0.05, false) < 1.0);
        assert_eq!(p.cue_alpha(p.cue_seconds, false), 0.0);
        assert_eq!(p.cue_alpha(0.0, true), 1.0);
        assert_eq!(p.cue_alpha(p.cue_seconds - 0.01, true), 1.0);
        assert_eq!(p.cue_alpha(p.cue_seconds + 0.01, true), 0.0);
    }

    /// A stage's announcer opened by `opening`, its voice `lines` each
    /// `len` seconds long, the first note at song second 10 (on the audio
    /// clock at 110).
    fn stage(lines: &[&str], len: f64) -> super::super::Announcer<f64> {
        use super::super::{Announcer, Timing, Voice};
        let mut v = Voice::default();
        for f in lines {
            let line = Line::parse(f).unwrap();
            v.expect([line]);
            v.add(line, len);
        }
        let timing = Timing {
            start: 100.0,
            offset: 0.0,
            rate: 1.0,
        };
        let mut a = Announcer::new(true, 0.5, None, timing, Some(10.0))
            .in_stage(Event::StageBegin, BATTLE_VOICE_V1);
        a.set_voice(std::rc::Rc::new(std::cell::RefCell::new(v)));
        a
    }

    /// One battle line at a time: a moment that matters less than the
    /// line speaking is passed over, never queued; one that matters more
    /// speaks at once; once a line is over, anything speaks.
    #[test]
    fn a_battle_line_speaks_over_less_and_never_waits() {
        let mut audio = super::super::tests::Fake::default();
        let mut a = stage(
            &[
                "battle life critical",
                "battle survivors 2",
                "battle eliminated",
            ],
            2.0,
        );
        let critical = Line::Battle(Event::LifeCritical);
        assert_eq!(
            a.battle(&mut audio, Event::LifeCritical, 120.0),
            Some(critical)
        );
        // Two left, while life critical speaks: passed over.
        assert_eq!(a.battle(&mut audio, Event::Survivors(2), 121.0), None);
        assert_eq!(a.report.dropped, 1);
        // Out, while it speaks: an outcome over the player's life.
        let out = Line::Battle(Event::Eliminated);
        assert_eq!(a.battle(&mut audio, Event::Eliminated, 121.5), Some(out));
        // Its end over, a milestone speaks again.
        assert_eq!(
            a.battle(&mut audio, Event::Survivors(2), 124.0),
            Some(Line::Battle(Event::Survivors(2)))
        );
        let at: Vec<f64> = audio.voiced.iter().map(|v| v.1).collect();
        assert_eq!(at.len(), 3);
        assert!((at[0] - 120.03).abs() < 1e-9 && (at[1] - 121.53).abs() < 1e-9);
    }

    /// A stage's play `moments` frames of half a second from song second
    /// 10 (the audio clock at 110), the combo `combo(i)` at frame `i`, and
    /// `each(i, a, audio)` before its frame.
    fn run(
        a: &mut super::super::Announcer<f64>,
        audio: &mut super::super::tests::Fake,
        frames: u32,
        combo: impl Fn(u32) -> u32,
        mut each: impl FnMut(u32, &mut super::super::Announcer<f64>, &mut super::super::tests::Fake),
    ) {
        use super::super::Moment;
        let mut m = Moment {
            dt: 0.5,
            life: 1.0,
            ..Moment::default()
        };
        for i in 0..frames {
            m.now = 110.0 + f64::from(i) * 0.5;
            m.song_time = 10.0 + f64::from(i) * 0.5;
            m.combo = combo(i);
            m.ended = i + 1 == frames;
            each(i, a, audio);
            a.tick(audio, &m);
        }
    }

    /// No two lines at once: each said line's start is at or after the end
    /// of the one before (`len` seconds each).
    fn one_at_a_time(audio: &super::super::tests::Fake, len: f64) {
        let mut at: Vec<f64> = audio.voiced.iter().map(|v| v.1).collect();
        at.sort_by(f64::total_cmp);
        for w in at.windows(2) {
            assert!(w[1] >= w[0] + len - 1e-9, "two voices at once: {at:?}");
        }
    }

    /// `battle-voice-v1`: a combo line is said in a quiet moment of a
    /// stage; the other routine lines stay out (no comment, no cleared)
    /// whatever the play does.
    #[test]
    fn a_stage_says_its_combo_in_a_quiet_moment_and_no_other_routine_line() {
        let mut audio = super::super::tests::Fake::default();
        let mut a = stage(
            &[
                "gameplay 100 combo",
                "gameplay 200 combo",
                "gameplay combo stopped",
                "gameplay comment hot",
                "gameplay cleared",
            ],
            0.5,
        );
        // The combo up by five a frame to 245, then broken (5.1's gap of
        // two seconds between combo lines counts a frame a quarter second
        // at most).
        run(&mut a, &mut audio, 60, |i| if i < 50 { i * 5 } else { 0 }, |_, _, _| {});
        let said: Vec<&str> = a.report.spoken.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(said, ["gameplay 100 combo", "gameplay 200 combo", "gameplay combo stopped"]);
        // Said at once: the combo reached 100 at frame 20 (audio 120).
        assert!((a.report.spoken[0].1 - 120.03).abs() < 1e-9);
        one_at_a_time(&audio, 0.5);
    }

    /// A combo line is held back while a battle line speaks and said at
    /// its end where that is within the combo window; dropped where its
    /// moment has passed, or where a battle line is due before it would
    /// be over; a battle line due while a combo line speaks starts at its
    /// end. Never two voices at once.
    #[test]
    fn a_combo_line_waits_a_moment_for_a_battle_line_or_is_dropped() {
        let p = BATTLE_VOICE_V1;
        // Held: an attack line of 0.8 s at 119.6, the combo at 120.
        let mut audio = super::super::tests::Fake::default();
        let mut a = stage(&["gameplay 100 combo", "battle attack incoming"], 0.8);
        run(&mut a, &mut audio, 30, |i| i * 5, |i, a, audio| {
            if i == 19 {
                a.battle_by(audio, Event::AttackIncoming, 119.5, Some(130.0));
            }
        });
        let said: Vec<(&str, f64)> = a.report.spoken.iter().map(|(f, t)| (f.as_str(), *t)).collect();
        assert_eq!(said[0].0, "battle attack incoming");
        assert_eq!(said[1].0, "gameplay 100 combo");
        assert!((said[1].1 - (119.53 + 0.8)).abs() < 1e-9, "at its end: {said:?}");
        one_at_a_time(&audio, 0.8);
        // Dropped: the battle line longer than the combo's window.
        let mut audio = super::super::tests::Fake::default();
        let mut a = stage(&["gameplay 100 combo", "battle attack incoming"], p.combo_window + 1.0);
        run(&mut a, &mut audio, 30, |i| i * 5, |i, a, audio| {
            if i == 19 {
                a.battle_by(audio, Event::AttackIncoming, 119.5, Some(130.0));
            }
        });
        let said: Vec<&str> = a.report.spoken.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(said, ["battle attack incoming"], "its moment passed");
        assert!(a.report.dropped >= 1);
        // Dropped: a step's line due while it would speak.
        let mut audio = super::super::tests::Fake::default();
        let mut a = stage(&["gameplay 100 combo", "battle step up"], 0.8);
        a.battle_due(120.4);
        run(&mut a, &mut audio, 30, |i| i * 5, |_, _, _| {});
        assert!(a.report.spoken.is_empty(), "{:?}", a.report.spoken);
        // A battle line due while a combo line speaks: at its end.
        let mut audio = super::super::tests::Fake::default();
        let mut a = stage(&["gameplay 100 combo", "battle survivors 2"], 0.8);
        run(&mut a, &mut audio, 30, |i| i * 5, |i, a, audio| {
            if i == 21 {
                a.battle(audio, Event::Survivors(2), 120.5);
            }
        });
        let said: Vec<(&str, f64)> = a.report.spoken.iter().map(|(f, t)| (f.as_str(), *t)).collect();
        assert_eq!(said[0].0, "gameplay 100 combo");
        assert_eq!(said[1].0, "battle survivors 2");
        assert!((said[1].1 - (120.03 + 0.8)).abs() < 1e-9, "{said:?}");
        one_at_a_time(&audio, 0.8);
    }

    /// A step's line never talks over a line being said: said at once,
    /// or at the end of the line speaking where that is within the step
    /// window, else passed over. A pack without it says nothing for it
    /// (no fallback).
    #[test]
    fn a_step_line_waits_up_to_a_second_never_over_a_line() {
        let p = BATTLE_VOICE_V1;
        let up = Line::Battle(Event::StepUp);
        let mut audio = super::super::tests::Fake::default();
        let mut a = stage(&["battle step up", "battle step down", "battle life critical"], 0.8);
        assert_eq!(a.battle_patient(&mut audio, Event::StepUp, 120.0), Some(up));
        // Life critical (it matters more) speaks over it, as v0 had it.
        assert!(a.battle(&mut audio, Event::LifeCritical, 120.2).is_some());
        // A step down while life critical speaks (until 121.03): at its end.
        assert_eq!(
            a.battle_patient(&mut audio, Event::StepDown, 120.5),
            Some(Line::Battle(Event::StepDown))
        );
        let last = a.report.spoken.last().unwrap();
        assert!((last.1 - 121.03).abs() < 1e-9, "{last:?}");
        // A line of two seconds: a step more than a second behind it is
        // passed over.
        let mut long = stage(&["battle step up", "battle life critical"], 2.0);
        long.battle(&mut audio, Event::LifeCritical, 130.0);
        assert_eq!(long.battle_patient(&mut audio, Event::StepUp, 130.2), None);
        assert_eq!(long.report.dropped, 1);
        assert!(130.03 + 2.0 > 130.23 + p.step_window);
        // A pack without the step lines: nothing in their place.
        let mut plain = stage(&["gameplay ready", "battle attack launched"], 1.0);
        assert_eq!(plain.battle_patient(&mut audio, Event::StepUp, 150.0), None);
        assert_eq!(plain.report.absent, ["battle step up"]);
    }

    /// An attack against the player speaks over their life running low
    /// and under an outcome, and only before its deadline (its shocks
    /// coming into view): past it nothing is said late. One of their own
    /// speaks when nothing that matters as much does, else is passed
    /// over, never queued. Neither falls back on another line.
    #[test]
    fn the_attack_lines_speak_by_their_priority_and_deadline() {
        let mut audio = super::super::tests::Fake::default();
        let mut a = stage(
            &[
                "battle attack incoming",
                "battle attack launched",
                "battle life critical",
                "battle survivors 2",
                "battle eliminated",
            ],
            1.2,
        );
        let incoming = Line::Battle(Event::AttackIncoming);
        let launched = Line::Battle(Event::AttackLaunched);
        // Their life low, then an attack against them: over it, at once.
        a.battle(&mut audio, Event::LifeCritical, 120.0);
        assert_eq!(
            a.battle_by(&mut audio, Event::AttackIncoming, 120.5, Some(124.0)),
            Some(incoming)
        );
        // Their own attack while that speaks: passed over.
        assert_eq!(a.battle(&mut audio, Event::AttackLaunched, 121.0), None);
        assert_eq!(a.report.dropped, 1);
        // Once it is over, their own speaks; two left speaks over it.
        assert_eq!(
            a.battle(&mut audio, Event::AttackLaunched, 122.0),
            Some(launched)
        );
        assert_eq!(
            a.battle(&mut audio, Event::Survivors(2), 122.5),
            Some(Line::Battle(Event::Survivors(2)))
        );
        // While two left speaks, their own again: passed over.
        assert_eq!(a.battle(&mut audio, Event::AttackLaunched, 123.0), None);
        // Out, then an attack against them: an outcome matters more.
        a.battle(&mut audio, Event::Eliminated, 130.0);
        assert_eq!(
            a.battle_by(&mut audio, Event::AttackIncoming, 130.5, Some(134.0)),
            None
        );
        assert_eq!(a.report.dropped, 3);
        // Past its deadline (its shocks in view already): never late.
        assert_eq!(
            a.battle_by(&mut audio, Event::AttackIncoming, 140.0, Some(140.0)),
            None
        );
        assert_eq!(a.report.dropped, 4);
        assert_eq!(
            a.battle_by(&mut audio, Event::AttackIncoming, 140.0, Some(140.5)),
            Some(incoming)
        );
        let said: Vec<&str> = a.report.spoken.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(
            said,
            [
                "battle life critical",
                "battle attack incoming",
                "battle attack launched",
                "battle survivors 2",
                "battle eliminated",
                "battle attack incoming",
            ]
        );
        // A pack without them stays silent for them: nothing in their
        // place, though it has the play's lines.
        let mut plain = stage(&["gameplay ready", "gameplay comment danger"], 1.0);
        assert_eq!(
            plain.battle_by(&mut audio, Event::AttackIncoming, 150.0, Some(155.0)),
            None
        );
        assert_eq!(plain.battle(&mut audio, Event::AttackLaunched, 151.0), None);
        assert_eq!(
            plain.report.absent,
            ["battle attack incoming", "battle attack launched"]
        );
    }

    #[test]
    fn only_the_moment_s_own_line_counts_as_said() {
        let told = |said| Told {
            event: Event::FinalStage,
            said,
            at: 0.0,
        };
        let p = BATTLE_VOICE_V1;
        assert!(told(Some(Line::Battle(Event::FinalStage))).spoken());
        assert!(!told(Some(Line::Battle(Event::StageBegin))).spoken());
        assert!(told(Some(Line::Battle(Event::StageBegin))).shown(&p));
        assert!(told(Some(Line::Ready)).shown(&p), "ready names no stage");
        assert!(told(None).shown(&p));
        let won = Told {
            event: Event::StageWon,
            said: None,
            at: 0.0,
        };
        assert!(!won.shown(&p), "Stage won shows already");
    }
}
