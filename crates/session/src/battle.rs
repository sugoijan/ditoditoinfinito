//! A stage of a multiplayer match played on this device with CPU
//! opponents (`docs/plans/multiplayer-prototype-progress.md`, M3): the
//! opponents judged beside the play each frame, what the play shows of
//! them, and the stage's outcome as the room's ranking, so that neither
//! shell keeps a copy.
//!
//! The battle is [`BattlePreset::GaugeSurvivalV0`] (PROVISIONAL, named in
//! `ddi_room::battle`): each competitor's life is their own ruleset gauge;
//! an empty gauge is out of the stage. The stage is decided once at most
//! one competitor is still in; a local winner then plays on to the song's
//! end with its fail turned off (nothing after changes the result). The
//! CPUs ([`ddi_engine::cpu::Opponent`]) play a tap-only copy of the chart
//! at a level relative to it and respond to nothing: what shows them says
//! so, and their life is their engine-judged gauge, nothing else.
//!
//! An online room's stage (M5d) is played on each member's own device:
//! the others are not judged here, nor shown during the play in this
//! prototype ([`StageSetup::others`]); nothing is decided on this device,
//! and the stage's outcome carries the local player's own run
//! ([`StageOutcome::mine`]) for the room, which ranks everyone's.
//!
//! The battle's moments as the local player lives them (M4): the stage's
//! opening, life critical, the players left, out, the outcome and the
//! winner's finished song ([`Battle::take_events`]), which the
//! announcer's battle lines speak or the play shows
//! (`crate::announcer::battle`).
//!
//! Each CPU's judged moments feed its miniature field ([`feed`], M6): the
//! tap-only copy they play, shown on this play's song clock.

pub mod attacks;
pub mod feed;

use ddi_chart::Song;
use ddi_engine::JudgeEventKind;
use ddi_engine::cpu::Opponent;
use ddi_engine::judge::JudgeEvent;
use ddi_engine::player::Player;
use ddi_engine::rules::{Ruleset, ScoreView};
use ddi_l10n::msg::room as m;
use ddi_library::announcer::{Battle as Event, SURVIVOR_MILESTONES};
use ddi_room::battle::{decided, rank};
use ddi_room::{BattlePreset, Entrant, Finisher, ParticipantId, Run, StageId};
use ddi_ui::state::room::{CpuSeat, StageOrder};

/// What a session needs to play a stage: which, under what battle, who
/// the local player is and the CPUs committed to it.
#[derive(Clone, Debug, PartialEq)]
pub struct StageSetup {
    pub stage: StageId,
    /// The stage's number, from 1, and the match's stages.
    pub number: u16,
    pub of: u16,
    pub battle: BattlePreset,
    /// The match's ruleset (D0), the engine's preset id: the play's, the
    /// CPUs' and the bots' ([`stage_ruleset`]).
    pub ruleset: String,
    pub me: ParticipantId,
    pub cpus: Vec<CpuSeat>,
    /// Other people starting the stage on their own devices (an online
    /// room's): the room decides it from everyone's reports.
    pub others: u16,
}

impl StageSetup {
    /// The stage's opening event: the match's beginning on its first stage
    /// (a one-stage match's too: one opening, never stacked), its last
    /// stage's, else a stage's.
    pub fn opening(&self) -> Event {
        if self.number <= 1 {
            Event::MatchBegin
        } else if self.number >= self.of {
            Event::FinalStage
        } else {
            Event::StageBegin
        }
    }

    pub fn of(order: &StageOrder) -> StageSetup {
        StageSetup {
            stage: order.stage,
            number: order.number,
            of: order.of,
            battle: order.battle,
            ruleset: order.ruleset.clone(),
            me: order.me,
            cpus: order.cpus.clone(),
            others: order.others,
        }
    }

    /// Decided by the room from everyone's reports, never on this device.
    pub fn remote(&self) -> bool {
        self.others > 0
    }
}

/// The ruleset a stage of a match under ruleset `id` (`MatchRules::ruleset`,
/// D0) is played with on `chart`, by everyone in it: the engine's preset
/// as it is (no player's own judge or life level, fail type or holds:
/// StepMania's own for the chart's game), its fail immediate (an empty
/// gauge is out of the stage). ITG where the id is not a preset.
pub fn stage_ruleset(id: &str, chart: &ddi_chart::Chart) -> Ruleset {
    let mut rules = ddi_engine::rules::presets::stepmania(
        id,
        ddi_engine::rules::presets::HoldStyle::ByGame,
        chart,
    );
    rules.fail = ddi_engine::FailPolicy::Immediate;
    rules
}

/// An opponent as the play shows it, in its stable slot.
#[derive(Clone, Debug, PartialEq)]
pub struct OpponentView {
    pub participant: ParticipantId,
    /// "CPU 2", and its level ("Lv 3"), in the UI's language.
    pub name: String,
    pub level: String,
    /// Its gauge's life, `0..=1`.
    pub life: f32,
    /// Its gauge emptied: out of the stage.
    pub out: bool,
    /// Its score as a fraction of the chart's best, and its combo.
    pub score: f64,
    pub combo: u32,
    /// Its attack power as a share of the bar and whether the bar is
    /// full, where the battle has attacks ([`attacks`]).
    pub power: Option<(f32, bool)>,
}

/// A stage's judged outcome, for the room: its confirmed ranking (best
/// first, groups tied), and whether it was decided before the song's end.
#[derive(Clone, Debug, PartialEq)]
pub struct StageOutcome {
    pub stage: StageId,
    pub ranking: Vec<Vec<Finisher>>,
    pub confirmed_early: bool,
    /// The local player's own run: what an online room is told
    /// (`RoomClient::report_run`); the ranking is this device's alone.
    pub mine: Run,
}

struct Seat {
    seat: CpuSeat,
    opponent: Opponent,
    name: String,
    level: String,
    /// Its moments as its miniature field is fed them.
    feed: feed::FieldFeed,
}

/// The opponents of a stage being played.
pub struct Battle {
    setup: StageSetup,
    seats: Vec<Seat>,
    /// The chart's tap-only copy the CPUs play, as their fields draw it.
    track: feed::TapTrack,
    /// The attack ledger, where the battle has attacks (a practice's
    /// only: an online stage's would be the room's).
    attacks: Option<attacks::AttackRunner>,
    /// The ranking, once decided before the song's end.
    decided: Option<Vec<Vec<Finisher>>>,
    /// The battle's events since they were last taken, and what tells
    /// them: the competitors still in at the last frame, the local
    /// player's life in danger then, and whether its critical moment and
    /// its going out were told (each once a stage).
    events: Vec<Event>,
    live: usize,
    danger: Option<bool>,
    critical: bool,
    out: bool,
}

/// A score as a fraction of the chart's best: the ruleset's percent, else
/// its money or EX over their most.
pub fn accuracy(view: &ScoreView) -> f64 {
    view.percent
        .or(view.money.map(|m| m as f64 / 1_000_000.0))
        .or(match (view.ex, view.max_ex) {
            (Some(ex), Some(max)) if max > 0 => Some(f64::from(ex) / f64::from(max)),
            _ => None,
        })
        .or(view.ex_percent)
        .unwrap_or(0.0)
}

impl Battle {
    /// The CPUs of `setup` on chart `chart` of `song` (as played), under
    /// `ruleset` (the player's, its fail immediate: out of the stage),
    /// started at song second `song_offset`. Refused, with the engine's
    /// reason, where the CPUs cannot plan the chart.
    pub fn new(
        setup: StageSetup,
        song: &Song,
        chart: usize,
        ruleset: &Ruleset,
        song_offset: f64,
    ) -> Result<Battle, String> {
        let levels = ddi_engine::cpu::STRENGTHS.len();
        let track = feed::TapTrack::new(song, chart)?;
        let seats = setup
            .cpus
            .iter()
            .map(|seat| {
                let mut rules = ruleset.fresh();
                rules.fail = ddi_engine::FailPolicy::Immediate;
                let level = usize::from(seat.level).min(levels - 1);
                // A roster CPU's personality (M7b); an unreadable one plays
                // balanced.
                let skills = seat
                    .skills
                    .as_deref()
                    .and_then(|v| ddi_engine::cpu::Skills::parse(v).ok())
                    .unwrap_or_default();
                let mut opponent =
                    Opponent::with_skills(song, chart, rules, level, seat.seed, skills)?;
                opponent.start(song_offset);
                let feed = feed::FieldFeed::new(&track, opponent.life(), feed::LOCAL_DELAY);
                Ok(Seat {
                    seat: seat.clone(),
                    opponent,
                    // A roster's name is data, the same in every language.
                    name: seat
                        .name
                        .clone()
                        .unwrap_or_else(|| m::cpu_name(seat.ordinal).into()),
                    level: m::play_level(level + 1).into(),
                    feed,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let live = seats.len() + 1 + usize::from(setup.others);
        let attacks = (setup.battle.attacks() && !setup.remote()).then(|| {
            attacks::AttackRunner::new(
                song,
                chart,
                &setup.ruleset,
                std::iter::once(setup.me).chain(seats.iter().map(|s| s.seat.participant)),
            )
        });
        Ok(Battle {
            setup,
            seats,
            track,
            attacks,
            decided: None,
            events: Vec::new(),
            live,
            danger: None,
            critical: false,
            out: false,
        })
    }

    pub fn setup(&self) -> &StageSetup {
        &self.setup
    }

    /// The opponents judged to judged song second `t`, beside `human`
    /// judged to it too (`judged`: what it was judged since the last
    /// call). `Some(true)` the moment the stage is decided with the local
    /// player the winner (the session lets them finish the song whatever
    /// their gauge does), `Some(false)` the moment it is decided
    /// otherwise.
    pub fn advance(&mut self, t: f64, human: &Player, judged: &[JudgeEvent]) -> Option<bool> {
        if let Some(a) = self.attacks.as_mut() {
            a.judged(self.setup.me, judged);
            if let Some(at) = human.gauge_failed_at() {
                a.out_at(self.setup.me, at);
            }
        }
        for s in &mut self.seats {
            s.opponent.advance(t);
            // Its judgments as moments of its stream: what its field shows.
            let (life, combo, out_at) =
                (s.opponent.life(), s.opponent.combo(), s.opponent.out_at());
            let events = s.opponent.take_judged();
            if let Some(a) = self.attacks.as_mut() {
                a.judged(s.seat.participant, &events);
                if let Some(at) = out_at {
                    a.out_at(s.seat.participant, at);
                }
            }
            for e in events {
                let JudgeEventKind::Tap(judgment, _) = e.kind else {
                    continue;
                };
                let moment = feed::Moment {
                    at: e.song_time,
                    tick: e.tick,
                    lane: e.lane,
                    judgment,
                    life,
                    combo,
                    out: out_at.is_some_and(|o| o <= e.song_time),
                };
                s.feed.push(&self.track, moment);
            }
        }
        // The attack checkpoints every competitor still in has been
        // judged past (the slowest judge decides, whatever the frames).
        if let Some(a) = self.attacks.as_mut() {
            let until = std::iter::once((human.gauge_failed_at(), human.judge().last_time()))
                .chain(
                    self.seats
                        .iter()
                        .map(|s| (s.opponent.out_at(), s.opponent.player().judge().last_time())),
                )
                .filter(|(out, _)| out.is_none())
                .map(|(_, last)| last.unwrap_or(f64::NEG_INFINITY))
                .reduce(f64::min);
            if let Some(until) = until {
                a.run_due(until);
            }
        }
        if self.decided.is_some() {
            return None;
        }
        let entrants = self.entrants(human, false);
        // Others play on their own devices: nothing is decided here.
        let decided_now = !self.setup.remote() && decided(&entrants);
        self.tell(human, decided_now);
        if !decided_now {
            return None;
        }
        let ranking = rank(&entrants);
        let won = ranking.first().is_some_and(|g| {
            g.len() == 1
                && g[0].participant == self.setup.me
                && g[0].finish == ddi_room::Finish::Survived
        });
        // The outcome as it is confirmed: won alone, or first shared (the
        // last ones out together); out, the elimination told it.
        let shared = ranking
            .first()
            .is_some_and(|g| g.len() > 1 && g.iter().any(|f| f.participant == self.setup.me));
        if won {
            self.events.push(Event::StageWon);
        } else if shared {
            self.events.retain(|e| *e != Event::Eliminated);
            self.events.push(Event::StageDrawn);
        }
        self.decided = Some(ranking);
        Some(won)
    }

    /// The moments of this frame (before the stage is decided): the local
    /// player's life entering danger while still in (once), going out
    /// (once; it silences the same frame's critical life and players
    /// left), and the players still in becoming one of
    /// [`SURVIVOR_MILESTONES`] (only the count reached, never one passed
    /// over; not at one: that decides the stage).
    fn tell(&mut self, human: &Player, decided: bool) {
        let rules = human.ruleset();
        let out = human.gauge_failed_at().is_some();
        let danger =
            !out && rules.gauge.danger() && !matches!(rules.fail, ddi_engine::FailPolicy::Off);
        let live = usize::from(!out)
            + usize::from(self.setup.others)
            + self
                .seats
                .iter()
                .filter(|s| s.opponent.out_at().is_none())
                .count();
        if out && !self.out {
            self.out = true;
            self.events.push(Event::Eliminated);
        } else if !out {
            // Entering danger, not in it from the start (a gauge of one
            // life is in danger throughout: nothing crosses).
            if danger && self.danger == Some(false) && !self.critical && !decided {
                self.critical = true;
                self.events.push(Event::LifeCritical);
            }
            if live < self.live
                && !decided
                && let Ok(n) = u8::try_from(live)
                && SURVIVOR_MILESTONES.contains(&n)
            {
                self.events.push(Event::Survivors(n));
            }
        }
        self.danger = Some(danger);
        self.live = live;
    }

    /// The battle's events since the last call, in the order they came.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Decided before the song's end.
    pub fn decided(&self) -> bool {
        self.decided.is_some()
    }

    /// The local player won the stage before the song's end and finishes
    /// it.
    pub fn won(&self) -> bool {
        self.decided.as_ref().is_some_and(|r| {
            r.first()
                .is_some_and(|g| g.len() == 1 && g[0].participant == self.setup.me)
        })
    }

    /// The attack ledger, where the battle has attacks.
    pub fn attacks(&self) -> Option<&attacks::AttackRunner> {
        self.attacks.as_ref()
    }

    /// The local player's power bar (its share and whether full), where
    /// the battle has attacks: none once the stage is won or they are
    /// out (`docs/plans/multiplayer-ui.md`: a confirmed winner's power
    /// stops).
    pub fn my_power(&self, human: &Player) -> Option<(f32, bool)> {
        let a = self.attacks.as_ref()?;
        (!self.won() && human.gauge_failed_at().is_none()).then(|| a.bar(self.setup.me))
    }

    /// The chart's tap-only copy the CPUs play, as their fields draw it.
    pub fn track(&self) -> &feed::TapTrack {
        &self.track
    }

    /// Each opponent's field's stream, in their stable slots' order.
    pub fn feeds(&self) -> impl Iterator<Item = &feed::FieldFeed> {
        self.seats.iter().map(|s| &s.feed)
    }

    /// The opponents in their stable slots (the order the room seated
    /// them in).
    pub fn views(&self) -> Vec<OpponentView> {
        self.seats
            .iter()
            .map(|s| OpponentView {
                participant: s.seat.participant,
                name: s.name.clone(),
                level: s.level.clone(),
                life: s.opponent.life(),
                out: s.opponent.out_at().is_some(),
                score: accuracy(&s.opponent.score()),
                combo: s.opponent.combo(),
                power: self
                    .attacks
                    .as_ref()
                    .filter(|_| s.opponent.out_at().is_none())
                    .map(|a| a.bar(s.seat.participant)),
            })
            .collect()
    }

    /// The stage's outcome now the play is over: the ranking decided
    /// during the song, else the opponents played to their end (nothing
    /// they do depends on the player) and everyone ranked; `quit`, the
    /// local player left while still in (a forfeit; never after the stage
    /// was decided).
    pub fn outcome(&mut self, human: &Player, quit: bool) -> StageOutcome {
        let mine = self.entrants(human, quit)[0].run;
        if let Some(ranking) = &self.decided {
            return StageOutcome {
                stage: self.setup.stage,
                ranking: ranking.clone(),
                confirmed_early: true,
                mine,
            };
        }
        for s in &mut self.seats {
            s.opponent.finish();
        }
        StageOutcome {
            stage: self.setup.stage,
            ranking: rank(&self.entrants(human, quit)),
            confirmed_early: false,
            mine,
        }
    }

    /// The stage's end as the local player's results tell it, from its
    /// outcome `o`: the winner's finished song after an early win (not
    /// after a quit); at the song's end, won alone, first shared or not
    /// won while still in; nothing for one who went out (that was told)
    /// or forfeited.
    pub fn end_event(&self, o: &StageOutcome, quit: bool) -> Option<Event> {
        // An online stage's outcome is the room's, not known here yet.
        if self.setup.remote() {
            return None;
        }
        let me = self.setup.me;
        if o.confirmed_early {
            return (self.won() && !quit).then_some(Event::SongFinished);
        }
        let (place, group) = o
            .ranking
            .iter()
            .enumerate()
            .find(|(_, g)| g.iter().any(|f| f.participant == me))?;
        let finish = group.iter().find(|f| f.participant == me)?.finish;
        if finish != ddi_room::Finish::Survived {
            return None;
        }
        Some(match (place, group.len()) {
            (0, 1) => Event::StageWon,
            (0, _) => Event::StageDrawn,
            _ => Event::StageNotWon,
        })
    }

    fn entrants(&self, human: &Player, quit: bool) -> Vec<Entrant> {
        let mut out = vec![Entrant {
            participant: self.setup.me,
            run: if quit && human.gauge_failed_at().is_none() {
                Run::Forfeit
            } else {
                run_of(
                    human.gauge_failed_at(),
                    human.ruleset().gauge.life(),
                    &human.ruleset().score.view(),
                )
            },
        }];
        out.extend(self.seats.iter().map(|s| Entrant {
            participant: s.seat.participant,
            run: run_of(s.opponent.out_at(), s.opponent.life(), &s.opponent.score()),
        }));
        out
    }
}

/// A stage left before its play began (its prompt's Back): the local
/// player forfeits; the CPUs are played to their end as they would have
/// been (on chart `chart` of `song`, under `ruleset`), and ranked.
pub fn forfeit(
    setup: StageSetup,
    song: &Song,
    chart: usize,
    ruleset: &Ruleset,
) -> Result<StageOutcome, String> {
    let mut b = Battle::new(setup, song, chart, ruleset, 0.0)?;
    for s in &mut b.seats {
        s.opponent.finish();
    }
    let mut entrants = vec![Entrant {
        participant: b.setup.me,
        run: Run::Forfeit,
    }];
    entrants.extend(b.seats.iter().map(|s| Entrant {
        participant: s.seat.participant,
        run: run_of(s.opponent.out_at(), s.opponent.life(), &s.opponent.score()),
    }));
    Ok(StageOutcome {
        stage: b.setup.stage,
        ranking: rank(&entrants),
        confirmed_early: false,
        mine: Run::Forfeit,
    })
}

fn run_of(out_at: Option<f64>, life: f32, score: &ScoreView) -> Run {
    match out_at {
        Some(at) => Run::Out { at },
        None => Run::Live {
            life: f64::from(life),
            accuracy: accuracy(score),
        },
    }
}
