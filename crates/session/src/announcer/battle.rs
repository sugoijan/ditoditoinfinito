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
//! The policy is [`BATTLE_VOICE_V0`], PROVISIONAL: its numbers are
//! nobody's tuning yet, and are kept here alone. A battle line never
//! waits: one due while a line that matters as much or more is still
//! speaking is passed over (shown, not said late), and nothing is queued
//! (a canceled start, a state that ended, leave nothing behind). The
//! voice bus cannot cut a sound short, so a line that matters more starts
//! over the end of one that matters less. Judging, the clock and the
//! music never wait for any of it.

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
}

/// The battle voice of the prototype (`battle-voice-v0`): one battle line
/// at a time, outcomes over an attack against the player over the
/// player's own life over who is left over an attack of the player's own
/// over the stage's opening and the winner's finished song; a play's routine
/// lines left out of a stage (a chart's clear is not a stage's win);
/// moments shown 2.5 s.
pub const BATTLE_VOICE_V0: VoicePolicy = VoicePolicy {
    id: "battle-voice-v0",
    cue_seconds: 2.5,
    cue_fade: 0.25,
    incoming_again: 6.0,
};

impl VoicePolicy {
    /// How much `event` matters when two meet: the higher speaks, the
    /// other is passed over. The design's order: terminal local states
    /// and confirmed outcomes, then an attack against the player, then
    /// critical life, then milestones, then feedback (an attack of the
    /// player's own just under the milestones).
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
/// their place, the play's here we go and a quit's results (the rest of
/// a play's lines are left out of a stage).
pub fn stage_speaks(line: Line) -> bool {
    match line {
        Line::Battle(_) | Line::HereWeGo | Line::Quit => true,
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
}

/// A stage's voice: the policy, the battle line speaking and until when
/// (audio clock), how much it matters, and the results' line kept until
/// the room has confirmed the stage.
#[derive(Clone, Debug)]
pub(super) struct StageVoice {
    pub policy: VoicePolicy,
    pub busy: Option<(f64, u8)>,
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
            end: None,
            opened: None,
        }
    }

    /// Whether `event` may speak at `at`: nothing speaks, or it matters
    /// more than what does.
    pub fn may_speak(&self, event: Event, at: f64) -> bool {
        match self.busy {
            Some((until, p)) if at < until => self.policy.priority(event) > p,
            _ => true,
        }
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
        let p = BATTLE_VOICE_V0;
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
            );
            assert_eq!(kept, expected, "{line:?}");
        }
        for routine_line in [
            Line::Combo(100),
            Line::ComboOverflow,
            Line::ComboStopped,
            Line::CommentHot,
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
                Line::Battle(Event::StageWon)
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
        let p = BATTLE_VOICE_V0;
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
            .in_stage(Event::StageBegin, BATTLE_VOICE_V0);
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

    /// A stage's routine lines are left out whatever the play does: no
    /// combo line as the combo passes a count the pack has, no comment,
    /// no cleared; a battle moment due then speaks.
    #[test]
    fn a_stage_says_no_combo_and_its_moment_speaks_instead() {
        use super::super::Moment;
        let mut audio = super::super::tests::Fake::default();
        let mut a = stage(
            &[
                "gameplay 100 combo",
                "gameplay comment hot",
                "gameplay cleared",
                "battle survivors 2",
            ],
            0.5,
        );
        let mut m = Moment {
            now: 110.0,
            dt: 0.5,
            song_time: 10.0,
            life: 1.0,
            ..Moment::default()
        };
        for i in 0..60 {
            m.now = 110.0 + f64::from(i) * 0.5;
            m.song_time = 10.0 + f64::from(i) * 0.5;
            m.combo = i * 5;
            m.ended = i == 59;
            a.tick(&mut audio, &m);
            if i == 20 {
                // The combo reaches 100 as two are left.
                a.battle(&mut audio, Event::Survivors(2), m.now);
            }
        }
        let said: Vec<&str> = a.report.spoken.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(said, ["battle survivors 2"]);
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
        let p = BATTLE_VOICE_V0;
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
