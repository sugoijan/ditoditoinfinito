//! A stage of a multiplayer match played on this device with CPU
//! opponents (`docs/plans/multiplayer-prototype-progress.md`, M3): the
//! opponents judged beside the play each frame, what the play shows of
//! them, and the stage's outcome as the room's ranking, so that neither
//! shell keeps a copy.
//!
//! What a room shows of a competitor's score and ranks by after life is
//! DDI's score, the sum (round 3b); a CPU's is put on the chart's own
//! scale first ([`cpu_score`], `CPU_SCORE_V0`).
//!
//! The battle is [`BattlePreset::GaugeSurvivalV0`] or, where the match's
//! Attacks rule is On, [`BattlePreset::GaugeAttacksV0`] (PROVISIONAL, named
//! in `ddi_room::battle`): each competitor's life is their own gauge under
//! the match's rules (DDI with the match's difficulty and Boos,
//! [`stage_ruleset`]); an empty gauge is out of the stage. The stage is decided once at most
//! one competitor is still in; a local winner then plays on to the song's
//! end with its fail turned off (nothing after changes the result). The
//! CPUs ([`ddi_engine::cpu::Opponent`]) play a tap-only copy of the chart
//! at a level relative to it; battle shocks put into their plays (M8b) are
//! judged by their own players and responded to by level alone
//! (`ddi_engine::cpu::MINE_RESPONSE_V0`); their life is their
//! engine-judged gauge, nothing else.
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
pub mod live;

use ddi_chart::Song;
use ddi_engine::JudgeEventKind;
use ddi_engine::cpu::Opponent;
use ddi_engine::judge::JudgeEvent;
use ddi_engine::player::Player;
use ddi_engine::rules::{Ruleset, ScoreView};
use ddi_l10n::msg::room as m;
use ddi_library::announcer::{Battle as Event, SURVIVOR_MILESTONES};
use ddi_room::attacks::{AttackEvent, LEAD_V0, RosterSeat, SETTLEMENT_V0, Settling, StageRoster};
pub use ddi_room::attacks::{BlockReason, Targeting};
use ddi_room::battle::decided;
use ddi_room::{Absolute, BattlePreset, Entrant, Finisher, ParticipantId, Run, RunDetail, StageId};
pub use ddi_ui::state::room::StageRules;
pub use ddi_ui::state::room::AttackSummary;
use ddi_ui::state::room::{CpuSeat, LiveOrder, StageOrder, run_detail};

/// What a session needs to play a stage: which, under what battle, who
/// the local player is and the CPUs committed to it.
#[derive(Clone, Debug, PartialEq)]
pub struct StageSetup {
    pub stage: StageId,
    /// The stage's number, from 1, and the match's stages.
    pub number: u16,
    pub of: u16,
    pub battle: BattlePreset,
    /// The match's rules a stage is judged by (D0: DDI, its difficulty
    /// and Boos): the play's, the CPUs' and the bots' ([`stage_ruleset`]).
    pub rules: StageRules,
    pub me: ParticipantId,
    /// The local player's attack policy as the stage begins (their
    /// member's, chosen before it: the room's players panel, the stage's
    /// prompt); changed in play by [`Battle::set_targeting`].
    pub targeting: Targeting,
    pub cpus: Vec<CpuSeat>,
    /// DEVELOPMENT ONLY: a section of battle shocks put into the local
    /// player's play from this beat on (`--mp-dev-mines`, `mpdevmines=`).
    pub dev_mines_at_beat: Option<u32>,
    /// Other people starting the stage on their own devices (an online
    /// room's): the room decides it from everyone's reports.
    pub others: u16,
    /// The local player is not in the stage: they watch it (`watch-v0`,
    /// `StageOrder::watching`). `me` is then nobody's seat: the CPUs
    /// alone are its competitors, judged on the song clock as ever, and
    /// nothing of the session's own player is judged.
    pub watching: bool,
    /// An online room plays the stage live
    /// (`docs/plans/multiplayer-online-play.md`, [`live`]): the other
    /// competitors' plays are streamed to this one and its attacks are
    /// the room's (a play given its room's client, `Battle::with_live`).
    pub live: Option<LiveOrder>,
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
            rules: order.rules.clone(),
            dev_mines_at_beat: order.dev_mines_at_beat,
            me: order.me,
            targeting: order.targeting,
            cpus: order.cpus.clone(),
            others: order.others,
            watching: order.watching,
            live: order.live.clone(),
        }
    }

    /// Decided by the room from everyone's reports, never on this device.
    pub fn remote(&self) -> bool {
        self.others > 0
    }
}

/// The policies in the order a press cycles them (D8), and each one's
/// number in a play's log (`ddi_engine::history::PolicyChange`).
pub const POLICIES: [Targeting; 3] = Targeting::ALL;

/// The policy after `t` in the cycle.
pub fn next_policy(t: Targeting) -> Targeting {
    let i = POLICIES.iter().position(|p| *p == t).unwrap_or(0);
    POLICIES[(i + 1) % POLICIES.len()]
}

/// A policy's number in a play's log.
pub fn policy_code(t: Targeting) -> u8 {
    POLICIES.iter().position(|p| *p == t).unwrap_or(0) as u8
}

/// The policy a log's number names (none: one this build does not know).
pub fn policy_of(code: u8) -> Option<Targeting> {
    POLICIES.get(usize::from(code)).copied()
}

/// The ruleset a stage of a match under `rules` (`MatchRules`: D0, DDI
/// with the match's difficulty and Boos) is played with by everyone in
/// it, the CPUs included: `presets::ddi()` with those options, nothing of
/// the player's own (never Practice, whatever their settings say), its
/// fail immediate (an empty life is out of the stage). DDI plays every
/// chart alike, so the chart is not asked.
pub fn stage_ruleset(rules: &StageRules) -> Ruleset {
    rules.build()
}

/// A stage's scroll speed raised so that its player's lead fits under
/// [`LEAD_V0`]'s cap (D5: an extreme slow setting would read further ahead
/// than an attack may wait): the speed `options` play chart `chart` of
/// `song` at, multiplied until the longest a note is shown before it is
/// judged leaves room for the warning and the delay; `None` where it fits
/// already.
pub fn raised_speed(
    song: &Song,
    chart: usize,
    options: &ddi_engine::player::PlayOptions,
) -> Option<ddi_engine::scroll::SpeedMod> {
    ddi_ui::state::battle_plan::raised_speed(song, chart, options)
}

/// Whether a stage with attacks raises the speed `settings` would play
/// chart `chart` of `song` at ([`raised_speed`]): what its prompt says.
pub fn stage_speed_raised(
    song: &Song,
    chart: usize,
    settings: &ddi_ui::state::settings::Settings,
) -> bool {
    let mut options = settings.play_options(None);
    options.scroll.speed = ddi_engine::scroll::speed_for_chart(
        song,
        chart,
        options.scroll.speed,
        settings.speed_source,
    );
    raised_speed(song, chart, &options).is_some()
}

/// DEVELOPMENT ONLY: the first section of battle shocks chart `chart` of
/// `song` has from beat `beat` on under `judge` (`ddi_engine::battle::schedule`,
/// drawn from `seed`), put into `player`'s play: what it put in, if any.
pub fn dev_shocks(
    player: &mut Player,
    song: &Song,
    chart: usize,
    beat: u32,
    seed: u64,
) -> Option<ddi_engine::battle::Section> {
    let schedule = ddi_engine::battle::schedule(song, chart, &player.ruleset().judge, seed);
    let every = ddi_engine::battle::PLACEMENT.every_beats.max(1);
    let first = beat.div_ceil(every) as usize;
    let section = schedule.into_iter().skip(first).flatten().next()?;
    player.add_hazards(&section.cells).ok()?;
    Some(section)
}

/// The seed a stage's battle draws its sections and ties from: the
/// stage's and its CPUs' (the match's profile), so that two stages on the
/// same chart differ.
pub fn battle_seed(setup: &StageSetup) -> u64 {
    setup
        .cpus
        .iter()
        .fold(setup.stage.0.wrapping_mul(0x9E37_79B9_7F4A_7C15), |s, c| {
            s.rotate_left(7) ^ c.seed
        })
}

/// An opponent as the play shows it, in its stable slot.
#[derive(Clone, Debug, PartialEq)]
pub struct OpponentView {
    pub participant: ParticipantId,
    /// "CPU 2", in the UI's language (a roster's name is data).
    pub name: String,
    /// A CPU's level, from 1 to ten (drawn as stars); none for a person.
    pub level: Option<u8>,
    /// Its gauge's life, `0..=1`.
    pub life: f32,
    /// Its gauge emptied: out of the stage.
    pub out: bool,
    /// Its score as a fraction of the best its notes give (the percent),
    /// and as the room shows it and ranks by it under DDI: the sum, a
    /// CPU's on the chart's own scale ([`cpu_score`]); none under another
    /// ruleset. Then its combo.
    pub score: f64,
    pub sum: Option<i64>,
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
    /// First shared because the last one in was knocked out while the
    /// stage waited on an attack against them (`SETTLEMENT_V0`; the
    /// results say "Drawn: final attack").
    pub drawn_by_final_attack: bool,
    /// How each competitor judged on this device performed (the local
    /// player, a practice's CPUs), for the stage's standings
    /// (`ddi_ui::state::room::run_detail`): the local player's goes with
    /// its report to an online room, a practice's stay with its client.
    pub details: Vec<(ParticipantId, RunDetail)>,
    /// The local player's attack policy as the play left it (changed in
    /// play or not): what the room keeps as theirs for the next stage.
    pub targeting: Targeting,
    /// The local player's attacks of the stage, where it had them: whom
    /// they sent how many, who sent them how many, the shocks they
    /// stepped on, a final attack's draw (the results' battle summary).
    pub summary: Option<AttackSummary>,
    /// The local player was not in the stage (they watched it): `mine`
    /// is then no run of theirs (a forfeit's placeholder, never
    /// reported: a watched stage is a practice's, ranked here), and
    /// `details` are the CPUs' alone.
    pub watched: bool,
}

/// An attack against the local player, as its warning shows it
/// (`ddi_shell`'s cue by the field's receptors).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Incoming {
    /// Its shocks (all eight, D4).
    pub shocks: u8,
    /// When it was committed, its first shock and when it clears
    /// (judged song seconds).
    pub committed: f64,
    pub first: f64,
    pub clear: f64,
    /// One more attack is reserved behind it (`attacks-v1`'s chain).
    pub more: bool,
}

/// One of the local player's attack relationships, while its attack is
/// on its way or passing (M8 round 3, S5): an attack they sent to an
/// opponent, or one an opponent sent them. What the play draws large
/// (`ddi_shell::attack_links`: a curve at the commitment, the opponent's
/// miniature outlined); CPU against CPU has none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    /// The attack's number in the stage.
    pub attack: u32,
    /// The opponent at its other end: its seat (its place in
    /// [`Battle::views`] and [`Battle::feeds`]).
    pub seat: usize,
    /// The local player sent it (else it is against them).
    pub outgoing: bool,
    /// When this play learned of its commitment (the judged song second
    /// of the frame that ran its checkpoint: what its curve is timed
    /// from), its first shock and when it clears (judged song seconds).
    pub shown_from: f64,
    pub first: f64,
    pub clear: f64,
    /// It waits behind another attack against the same recipient that
    /// has not cleared yet (a chain): shown quieter.
    pub chained: bool,
}

/// An attack's moment for the local player, as the play tells it (M8
/// round 3, S7): one against them committed, or one of their own. Each
/// has a battle line (`battle attack incoming`, `battle attack
/// launched`: [`AttackMoment::event`]), said under the stage's voice
/// (`crate::announcer::battle`); where it was not said, the status
/// line's words tell it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackMoment {
    Incoming,
    Launched,
}

impl AttackMoment {
    /// The battle's event its line names.
    pub fn event(self) -> Event {
        match self {
            AttackMoment::Incoming => Event::AttackIncoming,
            AttackMoment::Launched => Event::AttackLaunched,
        }
    }
}

/// A full bar of the local player's waiting, as the line under it says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Waiting {
    pub targeting: Targeting,
    pub reason: BlockReason,
}

/// An attack between two competitors as someone watching sees it
/// (`watch-v0`: [`Battle::seen_links`]): the seats at its ends (their
/// places in [`Battle::views`] and [`Battle::feeds`]), and its times as a
/// [`Link`]'s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeenLink {
    pub attack: u32,
    pub from: usize,
    pub to: usize,
    pub shown_from: f64,
    pub first: f64,
    pub clear: f64,
    /// It waits behind another attack against the same recipient.
    pub chained: bool,
}

impl SeenLink {
    /// The link as `seat`'s own relationship ([`Link`]: what a play
    /// draws for its player), where it is at one of its ends: what a
    /// watcher following `seat` is shown.
    pub fn of(&self, seat: usize) -> Option<Link> {
        let (other, outgoing) = if self.from == seat {
            (self.to, true)
        } else if self.to == seat {
            (self.from, false)
        } else {
            return None;
        };
        Some(Link {
            attack: self.attack,
            seat: other,
            outgoing,
            shown_from: self.shown_from,
            first: self.first,
            clear: self.clear,
            chained: self.chained,
        })
    }
}

/// A stage's moment as someone watching it is told (`watch-v0`), in the
/// status line: nobody's "you". The seats are places in
/// [`Battle::views`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchMoment {
    /// A competitor's life ran out.
    Out { seat: usize },
    /// The stage is decided: this competitor won it alone.
    Won { seat: usize },
    /// The stage is decided with first place shared.
    Drawn,
    /// An attack committed.
    Attack { from: usize, to: usize },
}

/// The reading horizon a CPU's lead is reckoned from (D5): the local
/// player's own, so that the shocks on its miniature field, drawn under
/// the local player's scroll, come in from the far end as the player's
/// own would. PROVISIONAL (`docs/plans/multiplayer-prototype-progress.md`,
/// M8b).
pub const CPU_HORIZON: &str = "local-player-horizon-v0";

/// The horizon a stage left at its prompt reckons CPUs' leads from (no
/// play to read it from), seconds.
pub const FORFEIT_HORIZON: f64 = 2.0;

struct Seat {
    seat: CpuSeat,
    opponent: Opponent,
    name: String,
    /// From 1.
    level: u8,
    /// Its moments as its miniature field is fed them.
    feed: feed::FieldFeed,
    /// The stage was decided with it the winner (alone or first shared):
    /// it plays the song out and is never put out after it, whatever its
    /// gauge does (the decision stands; [`Seat::out_at`]).
    plays_on: bool,
}

impl Seat {
    /// When it went out of the stage: its gauge's first emptying, never
    /// once it plays the song out as the decided winner.
    fn out_at(&self) -> Option<f64> {
        if self.plays_on {
            None
        } else {
            self.opponent.out_at()
        }
    }
}

/// The opponents of a stage being played.
pub struct Battle {
    setup: StageSetup,
    seats: Vec<Seat>,
    /// The chart's tap-only copy the CPUs play, as their fields draw it.
    track: feed::TapTrack,
    /// The most the whole chart gives (DDI): the scale the CPUs' scores
    /// are put on ([`cpu_score`]).
    most: Option<i64>,
    /// The attack ledger, where the battle has attacks (a practice's
    /// only: an online stage's would be the room's).
    attacks: Option<attacks::AttackRunner>,
    /// The local player's policy as they have it now (the ledger takes a
    /// change at its next checkpoint).
    policy: Targeting,
    /// The ranking, once decided before the song's end, and whether a
    /// final attack drew it.
    decided: Option<Vec<Vec<Finisher>>>,
    drawn: bool,
    /// When it was decided (the local player's judged song second).
    decided_at: Option<f64>,
    /// At most one competitor left, waiting on an attack against them
    /// (`SETTLEMENT_V0`).
    settling: Option<Settling>,
    /// The commitments since they were last taken, worded for a log.
    log: Vec<String>,
    /// The local player's attack moments since they were last taken
    /// ([`Battle::take_attack_moments`]), each with its attack's first
    /// shock (judged song seconds).
    attack_moments: Vec<(AttackMoment, f64)>,
    /// When this play learned of each attack's commitment (its number,
    /// the judged song second): what the local player's links are timed
    /// from ([`Battle::links`]).
    seen: Vec<(u32, f64)>,
    /// Shocks a play refused (arrived too late: a frame stalled past the
    /// lead).
    refused: u32,
    /// The battle's events since they were last taken, and what tells
    /// them: the competitors still in at the last frame, the local
    /// player's life in danger then, and whether its critical moment and
    /// its going out were told (each once a stage).
    events: Vec<Event>,
    live: usize,
    danger: Option<bool>,
    critical: bool,
    out: bool,
    /// The local player's judged song second, as of the last frame.
    now: f64,
    /// The local player left the stage's play while still in (a forfeit),
    /// at this judged song second ([`Battle::forfeit_now`]): out of the
    /// stage from then, the rest of it watched.
    forfeit: Option<f64>,
    /// The stage's moments for someone watching, since they were last
    /// taken ([`Battle::take_watch_moments`]), and the seats whose going
    /// out was told.
    watch_moments: Vec<WatchMoment>,
    outs_told: Vec<bool>,
    /// A stage the room plays live: its stream, the others' fields and the
    /// room's ledger as told ([`live::Live`]).
    remote_live: Option<live::Live>,
    /// The local player's own play as a field of the stage, fed from its
    /// judged events as the others' are from their streams: what shows
    /// as their miniature once they are out of the stage and follow a
    /// competitor large ([`Battle::own_feed`]); whether its going out was
    /// put in.
    own: feed::FieldFeed,
    own_out: bool,
    /// The stage's ladder where zero life steps down (a match's "At zero
    /// life: Step down", [`stage_ladder`]): each rung's chart, its copy as
    /// the fields draw it and its name; empty where zero life is out.
    rungs: Vec<Rung>,
}

/// A rung of the stage's ladder as the battle shows it.
#[derive(Clone, Debug)]
pub struct Rung {
    pub chart: usize,
    pub track: feed::TapTrack,
    /// Its name as the song list names it (data).
    pub name: String,
}

/// The ladder a stage on chart `chart` of `song` steps down where its
/// rules say so (`StageRules::steps_down`; `ddi_engine::ladder`): the
/// song's easier charts in its style, the next first, cut at the first
/// one the CPUs cannot plan (so everyone has the same ladder), each
/// planned for them once (`ddi_engine::cpu::planned_ladder`). Empty
/// where zero life is out, and in a stage played on several devices that
/// is not played live (its contract carries the ladder only live: the
/// battle plan's rung digests, a stream's step moments).
pub fn stage_ladder(song: &Song, chart: usize, setup: &StageSetup) -> Vec<ddi_engine::cpu::PlannedRung> {
    let online = setup.remote() || setup.live.is_some();
    if !setup.rules.steps_down()
        || (online && (setup.live.is_none() || !ddi_ui::state::room::ONLINE_STEP_DOWN))
    {
        return Vec::new();
    }
    ddi_engine::cpu::planned_ladder(song, chart)
}

/// The most points chart `chart` of `song`, whole, gives under `ruleset`
/// (DDI's: 50 a tap-like note and a hold unit; none under another
/// ruleset): the scale a CPU's score is put on ([`cpu_score`]).
pub fn chart_most(song: &Song, chart: usize, ruleset: &Ruleset) -> Option<i64> {
    song.charts.get(chart)?;
    let whole = ddi_engine::Player::new(song, chart, ruleset.fresh(), Default::default());
    whole.ruleset().score.view().ddi.map(|d| d.most)
}

/// A CPU's score as a room shows it and ranks by it
/// (`ddi_engine::rules::ddi::CPU_SCORE_V0`, PROVISIONAL): its play of the
/// tap-only copy put on the whole chart's scale (`chart_most`,
/// [`chart_most`]), as if it had done on the holds as it did on the taps.
/// Its percent is unchanged. Under another ruleset, or without the
/// chart's most, the score as it is.
pub fn cpu_score(view: &ScoreView, chart_most: Option<i64>) -> ScoreView {
    let mut view = *view;
    if let (Some(d), Some(most)) = (view.ddi.as_mut(), chart_most) {
        *d = d.on_scale(most);
    }
    view
}

/// A score as a fraction of the chart's best: DDI's proportional part
/// (points over the most the notes played give, the percent), else the
/// ruleset's percent, else its money or EX over their most.
pub fn accuracy(view: &ScoreView) -> f64 {
    if let Some(d) = &view.ddi {
        return d.ratio as f64 / 1_000_000.0;
    }
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
    /// started at song second `song_offset`; `horizon`, how far ahead the
    /// local player reads (`Player::reading_horizon`, seconds: every
    /// competitor's lead is reckoned from it, [`CPU_HORIZON`]). Refused,
    /// with the engine's reason, where the CPUs cannot plan the chart.
    pub fn new(
        setup: StageSetup,
        song: &Song,
        chart: usize,
        ruleset: &Ruleset,
        song_offset: f64,
        horizon: f64,
        ladder: &[ddi_engine::cpu::PlannedRung],
    ) -> Result<Battle, String> {
        let levels = ddi_engine::cpu::LEVELS;
        let rungs = ladder
            .iter()
            .map(|r| {
                Ok(Rung {
                    chart: r.chart,
                    track: if setup.remote() {
                        feed::TapTrack::full(song, r.chart)?
                    } else {
                        feed::TapTrack::new(song, r.chart)?
                    },
                    name: ddi_engine::ladder::rung_name(&song.charts[r.chart]),
                })
            })
            .collect::<Result<Vec<Rung>, String>>()?;
        // People play the chart itself; the CPUs a tap-only copy of it.
        let track = if setup.remote() {
            feed::TapTrack::full(song, chart)?
        } else {
            feed::TapTrack::new(song, chart)?
        };
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
                opponent.add_rungs(ladder)?;
                opponent.start(song_offset);
                let mut feed = feed::FieldFeed::new(&track, opponent.life(), feed::LOCAL_DELAY);
                // A shock stepped on locks its lane where the rules do
                // (DDI): its miniature shows the lock.
                feed.set_lock(opponent.player().ruleset().judge.lock);
                Ok(Seat {
                    seat: seat.clone(),
                    opponent,
                    // A roster's name is data, the same in every language.
                    name: seat
                        .name
                        .clone()
                        .unwrap_or_else(|| m::cpu_name(seat.ordinal).into()),
                    level: u8::try_from(level + 1).unwrap_or(u8::MAX),
                    feed,
                    plays_on: false,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let live = seats.len() + usize::from(!setup.watching) + usize::from(setup.others);
        let attacks = (setup.battle.attacks() && !setup.remote()).then(|| {
            let lead = LEAD_V0.of(attacks::micros(horizon));
            let roster = StageRoster {
                // A watcher has no seat: neither a sender nor a target.
                seats: (!setup.watching)
                    .then_some(RosterSeat {
                        participant: setup.me,
                        targeting: setup.targeting,
                        lead_us: lead,
                    })
                    .into_iter()
                .chain(seats.iter().map(|s| RosterSeat {
                    participant: s.seat.participant,
                    targeting: s.seat.targeting,
                    lead_us: lead,
                }))
                .collect(),
                seed: battle_seed(&setup),
            };
            // Its sections placed over every rung at once: safe whatever
            // chart its recipient is on.
            let charts: Vec<usize> = ladder.iter().map(|r| r.chart).collect();
            let placed;
            let over = if charts.is_empty() {
                song
            } else {
                placed = ddi_engine::ladder::merged(song, chart, &charts);
                &placed
            };
            attacks::AttackRunner::new(over, chart, &setup.rules.ruleset, &ruleset.judge, roster)
        });
        let own = feed::FieldFeed::new(&track, 1.0, feed::LOCAL_DELAY);
        Ok(Battle {
            policy: setup.targeting,
            setup,
            seats,
            own,
            own_out: false,
            track,
            most: chart_most(song, chart, ruleset),
            attacks,
            decided: None,
            drawn: false,
            decided_at: None,
            settling: None,
            log: Vec::new(),
            attack_moments: Vec::new(),
            seen: Vec::new(),
            refused: 0,
            events: Vec::new(),
            live,
            danger: None,
            critical: false,
            out: false,
            now: f64::NEG_INFINITY,
            forfeit: None,
            watch_moments: Vec::new(),
            outs_told: Vec::new(),
            remote_live: None,
            rungs,
        })
    }

    /// The stage's ladder ([`stage_ladder`]): empty where zero life is
    /// out.
    pub fn rungs(&self) -> &[Rung] {
        &self.rungs
    }

    /// The name of the chart a competitor who made `steps` steps down
    /// plays (none on the stage's chart).
    pub fn rung_name(&self, steps: usize) -> Option<&str> {
        steps
            .checked_sub(1)
            .and_then(|i| self.rungs.get(i))
            .map(|r| r.name.as_str())
    }

    /// A field's steps down not put in yet, from `player`'s (its
    /// `field`'s): each its rung's chart from the boundary and a moment
    /// with the life given back.
    fn feed_steps(rungs: &[Rung], field: &mut feed::FieldFeed, player: &Player) {
        let known = field.steps().count();
        for st in player.steps().iter().skip(known) {
            let Some(rung) = rungs.get(usize::from(st.rung) - 1) else {
                continue;
            };
            field.step_down(rung.track.clone(), st.at, st.boundary, st.rung);
            let life = player.life_at(st.at + 1e-6).clamp(0.0, 1.0);
            field.push(
                &rung.track,
                feed::Moment {
                    at: st.at,
                    tick: st.tick,
                    lane: 0,
                    kind: feed::MomentKind::StepDown(st.rung),
                    life,
                    combo: player.combo(),
                    out: false,
                    offset: 0.0,
                    w0: None,
                    score: player.ruleset().score.view(),
                },
            );
        }
    }

    /// The stage played live (its setup's [`StageSetup::live`]) through
    /// `handle`, the room's client: the play's stream sent, the others'
    /// fields and the room's attacks read from its frames (`lock`: the
    /// seconds a shock stepped on locks a lane under the stage's rules).
    pub fn with_live(mut self, handle: live::LiveHandle, lock: Option<f64>) -> Battle {
        let Some(order) = self.setup.live.clone() else {
            return self;
        };
        let prep = live::prep_now(&handle, self.setup.stage, &order);
        let most = self.most;
        self.remote_live = Some(live::Live::new(
            handle,
            self.setup.stage,
            self.setup.me,
            self.competes() && prep.is_some(),
            prep.as_ref()
                .and_then(|p| p.plan.clone())
                .filter(|_| self.setup.battle.attacks()),
            prep.map_or(0, |p| p.horizon_us),
            &order.people,
            &self.track,
            lock,
        ));
        if let Some(l) = self.remote_live.as_mut() {
            l.set_most(most);
            l.set_rungs(self.rungs.iter().map(|r| r.track.clone()).collect());
            self.log.push(l.describe());
        }
        self
    }

    /// Each remote competitor's song clock against the room's as its taps
    /// say it (µs; `live::RemoteSeat::shift_us`), in seat order.
    #[cfg(test)]
    pub(crate) fn remote_shift_us(&self) -> Vec<i64> {
        self.remote_live
            .as_ref()
            .map_or_else(Vec::new, |l| l.seats.iter().map(|s| s.shift_us).collect())
    }

    /// The live stage's own part as this play has it (the debug state's):
    /// whether it streams, whether it has the battle plan, its horizon and
    /// what became of each attack against it.
    pub fn live_state(&self) -> Option<live::LiveState<'_>> {
        self.remote_live.as_ref().map(live::Live::state)
    }

    /// The stage is played live here (`with_live`).
    pub fn is_live(&self) -> bool {
        self.remote_live.is_some()
    }

    /// The room lost the stage's live part (it restarted): the play goes
    /// on unstreamed and its run is reported at its end.
    pub fn live_lost(&self) -> bool {
        self.remote_live.as_ref().is_some_and(live::Live::lost)
    }

    /// Why the seat this stage is watched through (with a room's
    /// audience) ended during it, once it did: its view stops and says
    /// why, with the way back.
    pub fn seat_end(&self) -> Option<&ddi_ui::state::room::SeatEnd> {
        self.remote_live.as_ref().and_then(live::Live::seat_end)
    }

    /// The live stage's seats whose stream is behind (marked stale on
    /// their fields), by their places in [`Battle::views`].
    pub fn stale_seats(&self) -> Vec<usize> {
        let Some(l) = &self.remote_live else {
            return Vec::new();
        };
        let base = self.seats.len();
        l.seats
            .iter()
            .enumerate()
            .filter(|(_, s)| l.stale(s, self.now))
            .map(|(i, _)| base + i)
            .collect()
    }

    /// Its live part this frame: the room's client ticked, the local
    /// play's moments streamed (`human`, while it plays), the room's news
    /// read and what it committed delivered, the stage's decision as the
    /// room confirmed it.
    fn advance_live(&mut self, t: f64, human: Option<&mut Player>, judged: &[JudgeEvent]) -> Option<bool> {
        let me = self.setup.me;
        let mut human = human;
        let in_now = human.as_deref().is_some_and(|h| self.me_in(h));
        let mut l = self.remote_live.take()?;
        self.log.extend(l.sync(t));
        if let Some(h) = human.as_deref() {
            l.send(t, h, judged);
        }
        l.tick_room();
        let got = match human.as_deref_mut().filter(|_| in_now) {
            Some(h) => {
                let mut deliver = |cells: &[ddi_engine::battle::Hazard]| h.add_hazards(cells).is_ok();
                l.receive(&self.track, t, Some(&mut deliver))
            }
            None => l.receive(&self.track, t, None),
        };
        self.log.extend(got.log);
        let base = self.seats.len();
        for (attack, sender, recipient, first_us, delivered) in got.committed {
            let first = first_us as f64 / 1e6;
            if self.competes() && in_now {
                if recipient == me && delivered {
                    self.attack_moments.push((AttackMoment::Incoming, first));
                } else if sender == me {
                    self.attack_moments.push((AttackMoment::Launched, first));
                }
            } else if delivered {
                let seat = |p: ParticipantId| l.seats.iter().position(|s| s.participant == p);
                if let (Some(from), Some(to)) = (seat(sender), seat(recipient)) {
                    self.watch_moments.push(WatchMoment::Attack {
                        from: base + from,
                        to: base + to,
                    });
                }
            }
            self.seen.push((attack, t));
        }
        if !self.competes() || !in_now {
            for p in got.outs {
                if let Some(seat) = l.seats.iter().position(|s| s.participant == p && s.out()) {
                    self.watch_moments.push(WatchMoment::Out { seat: base + seat });
                }
            }
        }
        let settling = l.settling;
        self.remote_live = Some(l);
        self.settling = settling.or(self.settling);
        self.settle_live(human.as_deref())
    }

    /// The room's decision, once it confirmed the stage: what the local
    /// player is told, as practice's own decision tells it.
    fn settle_live(&mut self, human: Option<&Player>) -> Option<bool> {
        if self.decided.is_some() {
            return None;
        }
        let human = human.filter(|_| self.competes());
        let decision = self.remote_live.as_ref().and_then(live::Live::decision);
        let Some((ranking, early)) = decision.filter(|(_, early)| *early) else {
            self.tell(human, false);
            return None;
        };
        let _ = early;
        self.tell(human, true);
        let me = self.setup.me;
        let won = human.is_some()
            && ranking.first().is_some_and(|g| {
                g.len() == 1 && g[0].participant == me && g[0].finish == ddi_room::Finish::Survived
            });
        let shared = human.is_some()
            && ranking
                .first()
                .is_some_and(|g| g.len() > 1 && g.iter().any(|f| f.participant == me));
        let drawn = self.settling.is_some() && ranking.first().is_some_and(|g| g.len() > 1);
        if won {
            self.events.push(Event::StageWon);
        } else if shared {
            self.events.retain(|e| *e != Event::Eliminated);
            self.events.push(Event::StageDrawn);
        }
        let base = self.seats.len();
        if let Some(l) = &self.remote_live {
            match ranking.first().map(Vec::as_slice) {
                Some([one]) => {
                    if let Some(seat) = l.seats.iter().position(|s| s.participant == one.participant) {
                        self.watch_moments.push(WatchMoment::Won { seat: base + seat });
                    }
                }
                Some([_, _, ..]) => self.watch_moments.push(WatchMoment::Drawn),
                _ => {}
            }
        }
        self.drawn = drawn;
        self.winners_play_on(&ranking);
        self.decided = Some(ranking);
        self.decided_at = match human {
            Some(h) => h.judge().last_time(),
            None => Some(self.now),
        };
        Some(won)
    }

    /// The local player competes in the stage (they do not watch it).
    pub fn competes(&self) -> bool {
        !self.setup.watching
    }

    /// The local player is still in the stage: competing, their gauge
    /// not emptied and not left ([`Battle::forfeit_now`]).
    pub fn me_in(&self, human: &Player) -> bool {
        self.competes() && self.forfeit.is_none() && human.gauge_failed_at().is_none()
    }

    /// The local player's battle goes on: still in, the stage not won.
    fn me_battling(&self, human: &Player) -> bool {
        !self.won() && self.me_in(human)
    }

    /// The local player leaves the stage's play while still in, at
    /// judged song second `at` (their quit: a forfeit, ranked as one):
    /// out of the stage from then, a target and a sender of attacks no
    /// more; the stage goes on between the others, watched. Whether it
    /// was one (not for a watcher, once out, or after the stage was
    /// decided).
    pub fn forfeit_now(&mut self, at: f64, human: &Player) -> bool {
        if self.decided.is_some() || !self.me_in(human) {
            return false;
        }
        self.forfeit = Some(at);
        if let Some(a) = self.attacks.as_mut() {
            a.out_at(self.setup.me, at);
        }
        if let Some(l) = self.remote_live.as_mut() {
            l.end(ddi_room::live::StreamEnd::Forfeit, at);
        }
        true
    }

    /// The local player left the stage's play while still in.
    pub fn forfeited(&self) -> bool {
        self.forfeit.is_some()
    }

    /// When the local player left the stage's play (judged song second).
    pub fn forfeited_at(&self) -> Option<f64> {
        self.forfeit
    }

    /// Whether anything is left to watch of a stage the local player is
    /// not in (or no longer in): it is not decided and a competitor is
    /// still playing it.
    pub fn goes_on(&self) -> bool {
        if let Some(l) = &self.remote_live {
            return self.decided.is_none() && l.seats.iter().any(|s| s.end.is_none());
        }
        self.decided.is_none()
            && !self.setup.remote()
            && self
                .seats
                .iter()
                .any(|s| s.out_at().is_none() && !s.opponent.finished())
    }

    /// Whether someone is still playing the stage's song as it is shown
    /// here, decided or not (`watch-v1`, the maintainer, 2026-11-13: a
    /// watched stage ends when the winner finishes): a CPU neither out nor
    /// done with the chart; another device's competitor whose stream has
    /// not ended, or whose end its field (drawn the view's delay behind)
    /// has not reached yet. The local player's own play is not counted:
    /// whoever asks is watching.
    pub fn still_playing(&self) -> bool {
        if let Some(l) = &self.remote_live {
            return l.seats.iter().any(|s| match (s.end, s.through_us) {
                (None, _) => true,
                (Some(_), Some(end_us)) => !l.shown_to(end_us, self.now),
                (Some(_), None) => false,
            });
        }
        !self.setup.remote()
            && self
                .seats
                .iter()
                .any(|s| s.out_at().is_none() && !s.opponent.finished())
    }

    pub fn setup(&self) -> &StageSetup {
        &self.setup
    }

    /// DEVELOPMENT ONLY (`--mp-dev-mines`): `section`'s shocks put into
    /// every CPU's play too, at song second `at` (the play's start), and
    /// shown on their fields.
    pub fn dev_shocks(&mut self, section: &ddi_engine::battle::Section, at: f64) {
        for s in &mut self.seats {
            if s.opponent.receive_hazards(&section.cells).is_ok() {
                s.feed.add_hazards(&self.track, &section.cells, at);
            }
        }
    }

    /// The opponents judged to judged song second `t`, beside `human`
    /// judged to it too (`judged`: what it was judged since the last
    /// call); the attack checkpoints everyone has been judged past run,
    /// their commitments delivered (into `human`'s play, a CPU's and its
    /// field). `Some(true)` the moment the stage is decided with the local
    /// player the winner (the session lets them finish the song whatever
    /// their gauge does), `Some(false)` the moment it is decided
    /// otherwise.
    pub fn advance(&mut self, t: f64, human: &mut Player, judged: &[JudgeEvent]) -> Option<bool> {
        if !self.competes() {
            return self.advance_watched(t);
        }
        self.now = t;
        self.feed_own(human, judged);
        if self.remote_live.is_some() {
            return self.advance_live(t, Some(human), judged);
        }
        if let Some(a) = self.attacks.as_mut() {
            a.judged(self.setup.me, judged);
            if let Some(at) = human.gauge_failed_at() {
                a.out_at(self.setup.me, at);
            }
        }
        self.step_cpus(t);
        // Out of the stage (by life, or left): nothing more is put into
        // their play.
        if self.me_in(human) {
            self.run_attacks(Some(human));
        } else {
            self.run_attacks(None);
        }
        self.settle(Some(human))
    }

    /// The local player's judged events as moments of their own field
    /// ([`Battle::own_feed`]): taps with their early or late and inner
    /// window, holds' ends, mines and shocks stepped on, Boos; their
    /// going out (their gauge, or the forfeit) once.
    fn feed_own(&mut self, human: &Player, judged: &[JudgeEvent]) {
        Self::feed_steps(&self.rungs, &mut self.own, human);
        let judge = &human.ruleset().judge;
        let (life, combo) = (human.ruleset().gauge.life().clamp(0.0, 1.0), human.combo());
        let score = human.ruleset().score.view();
        let gone = human.gauge_failed_at().or(self.forfeit);
        for e in judged {
            let kind = match e.kind {
                JudgeEventKind::Tap(j, _) => feed::MomentKind::Tap(j),
                JudgeEventKind::Held => feed::MomentKind::HoldEnd(true),
                JudgeEventKind::LetGo => feed::MomentKind::HoldEnd(false),
                JudgeEventKind::HitMine
                    if e.note_index
                        .is_some_and(|n| n >= ddi_engine::battle::HAZARD_INDEX_BASE) =>
                {
                    feed::MomentKind::ShockHit
                }
                JudgeEventKind::HitMine => feed::MomentKind::MineHit,
                JudgeEventKind::Boo => feed::MomentKind::Boo,
                _ => continue,
            };
            let (offset, w0) = match e.kind {
                JudgeEventKind::Tap(j, delta) if !j.is_miss() => {
                    (delta as f32, judge.is_w0(j, delta))
                }
                _ => (0.0, None),
            };
            self.own.push(
                &self.track,
                feed::Moment {
                    at: e.song_time,
                    tick: e.tick,
                    lane: e.lane,
                    kind,
                    life,
                    combo,
                    out: false,
                    offset,
                    w0,
                    score,
                },
            );
        }
        if let Some(at) = gone
            && !self.own_out
        {
            self.own_out = true;
            self.own.push(
                &self.track,
                feed::Moment {
                    at,
                    tick: ddi_chart::Tick(0),
                    lane: 0,
                    kind: feed::MomentKind::Mark,
                    life: 0.0,
                    combo,
                    out: true,
                    offset: 0.0,
                    w0: None,
                    score,
                },
            );
        }
    }

    /// The local player's own field as a stream draws it (their judged
    /// play, their going out): their miniature among the others' once
    /// they are out of the stage and follow a competitor large.
    pub fn own_feed(&self) -> &feed::FieldFeed {
        &self.own
    }

    /// The local player as a miniature's opponent (`name`: "You"), as
    /// their own field shows them now.
    pub fn own_view(&self, human: &Player, name: String) -> OpponentView {
        let shown = self.own.shown(self.now);
        let view = human.ruleset().score.view();
        OpponentView {
            participant: self.setup.me,
            name,
            level: None,
            life: shown.life,
            out: shown.out,
            score: accuracy(&view),
            sum: view.ddi.map(|d| d.score),
            combo: shown.combo,
            power: None,
        }
    }

    /// [`Battle::advance`] for a stage the local player is not in (a
    /// watcher's): its competitors judged to song second `t`, the attacks
    /// between them run and delivered, the stage settled as it comes.
    /// `Some(false)` the moment it is decided.
    pub fn advance_watched(&mut self, t: f64) -> Option<bool> {
        self.now = t;
        if self.remote_live.is_some() {
            return self.advance_live(t, None, &[]);
        }
        self.step_cpus(t);
        self.run_attacks(None);
        self.settle(None)
    }

    /// Each CPU judged to `t`, its judgments to the ledger and to its
    /// field.
    fn step_cpus(&mut self, t: f64) {
        for s in &mut self.seats {
            s.opponent.advance(t);
            // Its steps down: its field draws the rung's chart.
            Self::feed_steps(&self.rungs, &mut s.feed, s.opponent.player());
            // Its judgments as moments of its stream: what its field shows.
            let (life, combo, out_at) =
                (s.opponent.life(), s.opponent.combo(), s.out_at());
            // Its score as the room shows it, for its field shown large.
            let score = cpu_score(&s.opponent.score(), self.most);
            let events = s.opponent.take_judged();
            let judge = &s.opponent.player().ruleset().judge;
            // What a field shown large says of each: early or late, and
            // the ruleset's inner window.
            let said: Vec<(f32, Option<bool>)> = events
                .iter()
                .map(|e| match e.kind {
                    JudgeEventKind::Tap(j, delta) if !j.is_miss() => {
                        (delta as f32, judge.is_w0(j, delta))
                    }
                    _ => (0.0, None),
                })
                .collect();
            if let Some(a) = self.attacks.as_mut() {
                a.judged(s.seat.participant, &events);
                if let Some(at) = out_at {
                    a.out_at(s.seat.participant, at);
                }
            }
            for (e, (offset, w0)) in events.into_iter().zip(said) {
                let kind = match e.kind {
                    JudgeEventKind::Tap(judgment, _) => feed::MomentKind::Tap(judgment),
                    // A battle shock stepped on (its copy has no mines of
                    // its own).
                    JudgeEventKind::HitMine
                        if e.note_index
                            .is_some_and(|n| n >= ddi_engine::battle::HAZARD_INDEX_BASE) =>
                    {
                        feed::MomentKind::ShockHit
                    }
                    _ => continue,
                };
                let moment = feed::Moment {
                    at: e.song_time,
                    tick: e.tick,
                    lane: e.lane,
                    kind,
                    life,
                    combo,
                    out: out_at.is_some_and(|o| o <= e.song_time),
                    offset,
                    w0,
                    score,
                };
                s.feed.push(&self.track, moment);
            }
        }
    }

    /// The attack checkpoints every competitor still in has been judged
    /// past (the slowest judge decides, whatever the frames; one whose
    /// play is over has been judged to its end), and their commitments
    /// delivered: into `human`'s play where it is given (not after the
    /// play: it is out then), into a CPU's and onto its field.
    fn run_attacks(&mut self, mut human: Option<&mut Player>) {
        let Some(a) = self.attacks.as_mut() else {
            return;
        };
        let me = self.setup.me;
        let judged_to = |p: &Player| {
            if p.finished() {
                f64::INFINITY
            } else {
                p.judge().last_time().unwrap_or(f64::NEG_INFINITY)
            }
        };
        let until = human
            .as_deref()
            .filter(|h| h.gauge_failed_at().is_none())
            .map(judged_to)
            .into_iter()
            .chain(
                self.seats
                    .iter()
                    .filter(|s| s.out_at().is_none())
                    .map(|s| judged_to(s.opponent.player())),
            )
            .reduce(f64::min);
        let Some(until) = until else {
            return;
        };
        let seats = &self.seats;
        let human_ref = human.as_deref();
        let life = |p: ParticipantId, at: f64| -> u32 {
            let l = if p == me {
                human_ref.map_or(0.0, |h| h.life_at(at))
            } else {
                seats
                    .iter()
                    .find(|s| s.seat.participant == p)
                    .map_or(0.0, |s| s.opponent.player().life_at(at))
            };
            (f64::from(l.clamp(0.0, 1.0)) * 1e6).round() as u32
        };
        let events = a.run_due(until, &life);
        for (at, e) in events {
            let AttackEvent::Committed {
                attack,
                sender,
                recipient,
            } = e
            else {
                continue;
            };
            let Some(attack) = a.ledger().attack(attack) else {
                continue;
            };
            let cells = attacks::hazards(attack);
            let committed = at as f64 / 1e6;
            self.seen.push((attack.id, self.now.max(committed)));
            let delivered = if recipient == me {
                human
                    .as_deref_mut()
                    .is_some_and(|h| h.add_hazards(&cells).is_ok())
            } else if let Some(s) = self
                .seats
                .iter_mut()
                .find(|s| s.seat.participant == recipient)
            {
                let ok = s.opponent.receive_hazards(&cells).is_ok();
                if ok {
                    s.feed.add_hazards(&self.track, &cells, committed);
                }
                ok
            } else {
                false
            };
            if !delivered {
                self.refused += 1;
            }
            // The local player's own moments (while they play).
            if human.is_some() {
                let first = attack.cells.first().map_or(at, |c| c.at_us) as f64 / 1e6;
                if recipient == me && delivered {
                    self.attack_moments.push((AttackMoment::Incoming, first));
                } else if sender == me {
                    self.attack_moments.push((AttackMoment::Launched, first));
                }
            } else if delivered {
                // Watched: who attacks whom, nobody's "you".
                let seat = |p: ParticipantId| self.seats.iter().position(|s| s.seat.participant == p);
                if let (Some(from), Some(to)) = (seat(sender), seat(recipient)) {
                    self.watch_moments.push(WatchMoment::Attack { from, to });
                }
            }
            let name = |p: ParticipantId| {
                if p == me {
                    "you".to_string()
                } else {
                    self.seats
                        .iter()
                        .find(|s| s.seat.participant == p)
                        .map_or_else(|| format!("{p:?}"), |s| s.name.clone())
                }
            };
            self.log.push(format!(
                "attack: committed {}→{} at {committed:.2} s, {} shocks from {:.2} s{}",
                name(sender),
                name(recipient),
                cells.len(),
                attack.cells.first().map_or(0.0, |c| c.at_us as f64 / 1e6),
                if delivered {
                    ""
                } else {
                    " (refused: too late)"
                },
            ));
        }
    }

    /// Whether the stage is decided now (`SETTLEMENT_V0`): at most one
    /// competitor in and no attack against anyone still in outstanding;
    /// the ranking then, and what it tells. `human`: the local player's
    /// play (none: a watcher's stage, the CPUs alone).
    fn settle(&mut self, human: Option<&Player>) -> Option<bool> {
        if self.decided.is_some() {
            return None;
        }
        let human = human.filter(|_| self.competes());
        let entrants = self.entrants_as(human.map(|h| self.my_run(h, false)));
        // Others play on their own devices: nothing is decided here.
        if self.setup.remote() {
            self.tell(human, false);
            return None;
        }
        // Attacks still to clear against those in: an attack clears once
        // its recipient's play is judged past it.
        let me = self.setup.me;
        let outstanding: Vec<ParticipantId> = self.attacks.as_ref().map_or_else(Vec::new, |a| {
            a.ledger()
                .outstanding()
                .filter(|(p, clear)| {
                    let judged = if *p == me {
                        human.and_then(|h| h.judge().last_time())
                    } else {
                        self.seats
                            .iter()
                            .find(|s| s.seat.participant == *p)
                            .and_then(|s| s.opponent.player().judge().last_time())
                    };
                    judged.is_none_or(|t| attacks::micros(t) <= *clear)
                })
                .map(|(p, _)| p)
                .collect()
        });
        let decided_now = SETTLEMENT_V0.decided(&entrants, &outstanding);
        // At most one left, an attack against them still to come: the
        // stage waits for it.
        if !decided_now && decided(&entrants) && self.settling.is_none() {
            let survivor = entrants
                .iter()
                .find(|e| matches!(e.run, Run::Live { .. }))
                .map(|e| e.participant);
            let last_out = entrants
                .iter()
                .filter_map(|e| match e.run {
                    Run::Out { at } => Some(attacks::micros(at)),
                    _ => None,
                })
                .max();
            if let (Some(survivor), Some(last_out_us)) = (survivor, last_out) {
                self.settling = Some(Settling {
                    survivor,
                    last_out_us,
                });
            }
        }
        self.tell(human, decided_now);
        if !decided_now {
            return None;
        }
        let checkpoints = self
            .attacks
            .as_ref()
            .map_or(&[][..], |a| a.ledger().checkpoints_us());
        let (ranking, drawn) = SETTLEMENT_V0.rank(&entrants, checkpoints, self.settling);
        let won = human.is_some()
            && ranking.first().is_some_and(|g| {
                g.len() == 1
                    && g[0].participant == self.setup.me
                    && g[0].finish == ddi_room::Finish::Survived
            });
        // The outcome as it is confirmed: won alone, or first shared (the
        // last ones out together, or a final attack's knockout); out, the
        // elimination told it.
        let shared = human.is_some()
            && ranking
                .first()
                .is_some_and(|g| g.len() > 1 && g.iter().any(|f| f.participant == self.setup.me));
        if won {
            self.events.push(Event::StageWon);
        } else if shared {
            self.events.retain(|e| *e != Event::Eliminated);
            self.events.push(Event::StageDrawn);
        }
        // For someone watching: who won it, or that first is shared.
        match ranking.first().map(Vec::as_slice) {
            Some([one]) => {
                if let Some(seat) = self
                    .seats
                    .iter()
                    .position(|s| s.seat.participant == one.participant)
                {
                    self.watch_moments.push(WatchMoment::Won { seat });
                }
            }
            Some([_, _, ..]) => self.watch_moments.push(WatchMoment::Drawn),
            _ => {}
        }
        self.drawn = drawn;
        self.winners_play_on(&ranking);
        self.decided = Some(ranking);
        self.decided_at = match human {
            Some(h) => h.judge().last_time(),
            None => Some(self.now),
        };
        Some(won)
    }

    /// The local player out of the play (out, or `quit` while in: a
    /// forfeit at its judged time): the CPUs and the attacks between them
    /// played out a checkpoint at a time to the song's end, as they would
    /// have gone on (nothing they do depends on the player any more),
    /// the stage settled as it comes.
    pub fn play_out(&mut self, human: &Player, quit: bool) {
        if !self.competes() {
            return self.play_out_as(None);
        }
        if let Some(a) = self.attacks.as_mut() {
            let at = human
                .gauge_failed_at()
                .or(self.forfeit)
                .or_else(|| quit.then(|| human.judge().last_time().unwrap_or(f64::NEG_INFINITY)));
            if let Some(at) = at {
                a.out_at(self.setup.me, at);
            }
        }
        let me = self.my_run(human, quit);
        self.play_out_as(Some(me));
    }

    /// [`Battle::play_out`] with the local player's run `me` (out, or a
    /// forfeit) already given to the ledger; none: a watcher's stage.
    fn play_out_as(&mut self, me: Option<Run>) {
        // A bound: one step a checkpoint, and a last one.
        let mut steps = 0;
        while self.decided.is_none() && steps < 10_000 {
            steps += 1;
            let Some(next) = self.attacks.as_ref().and_then(|a| a.next_checkpoint()) else {
                break;
            };
            if self.seats.iter().all(|s| s.out_at().is_some()) {
                break;
            }
            let before = self.attacks.as_ref().map(|a| a.checkpoints().0);
            self.step_cpus(next + ddi_engine::player::JUDGE_LAG + 0.001);
            self.run_attacks(None);
            self.settle_quietly(me);
            if self.attacks.as_ref().map(|a| a.checkpoints().0) == before
                && self
                    .seats
                    .iter()
                    .all(|s| s.opponent.finished() || s.out_at().is_some())
            {
                break;
            }
        }
        for s in &mut self.seats {
            s.opponent.finish();
        }
    }

    /// [`Battle::settle`] for a play over: decided as it comes, nothing
    /// told (the player's results are on their way).
    fn settle_quietly(&mut self, me: Option<Run>) {
        if self.decided.is_some() || self.setup.remote() {
            return;
        }
        let entrants = self.entrants_as(me);
        let outstanding: Vec<ParticipantId> = self.attacks.as_ref().map_or_else(Vec::new, |a| {
            a.ledger()
                .outstanding()
                .filter(|(p, clear)| {
                    self.seats
                        .iter()
                        .find(|s| s.seat.participant == *p)
                        .and_then(|s| s.opponent.player().judge().last_time())
                        .is_none_or(|t| attacks::micros(t) <= *clear)
                })
                .map(|(p, _)| p)
                .collect()
        });
        if !SETTLEMENT_V0.decided(&entrants, &outstanding) {
            if decided(&entrants) && self.settling.is_none() {
                let survivor = entrants
                    .iter()
                    .find(|e| matches!(e.run, Run::Live { .. }))
                    .map(|e| e.participant);
                let last_out = entrants
                    .iter()
                    .filter_map(|e| match e.run {
                        Run::Out { at } => Some(attacks::micros(at)),
                        _ => None,
                    })
                    .max();
                if let (Some(survivor), Some(last_out_us)) = (survivor, last_out) {
                    self.settling = Some(Settling {
                        survivor,
                        last_out_us,
                    });
                }
            }
            return;
        }
        // Only once the attacks matter: without them a stage played out
        // is ranked at the song's end, as before (a watcher's is decided
        // as it comes either way, as it is while they watch).
        if self.attacks.is_none() && me.is_some() {
            return;
        }
        let checkpoints = self
            .attacks
            .as_ref()
            .map_or(&[][..], |a| a.ledger().checkpoints_us());
        let (ranking, drawn) = SETTLEMENT_V0.rank(&entrants, checkpoints, self.settling);
        self.drawn = drawn;
        self.winners_play_on(&ranking);
        self.decided = Some(ranking);
    }

    /// The stage decided with `ranking`: its CPU winners (alone, or first
    /// shared, and still in) play the song out without being put out,
    /// whatever their gauges do after it (the decision stands), as a
    /// person who wins has their fail turned off.
    fn winners_play_on(&mut self, ranking: &[Vec<Finisher>]) {
        let Some(first) = ranking.first() else {
            return;
        };
        for s in &mut self.seats {
            if s.opponent.out_at().is_none()
                && first.iter().any(|f| {
                    f.participant == s.seat.participant && f.finish == ddi_room::Finish::Survived
                })
            {
                s.plays_on = true;
                s.opponent.set_fail(ddi_engine::FailPolicy::Off);
            }
        }
    }

    /// The moments of this frame (before the stage is decided): the local
    /// player's life entering danger while still in (once), going out
    /// (once; it silences the same frame's critical life and players
    /// left), and the players still in becoming one of
    /// [`SURVIVOR_MILESTONES`] (only the count reached, never one passed
    /// over; not at one: that decides the stage; told to someone watching
    /// too, a spectator or a player out of the stage: it is nobody's
    /// "you"). For someone watching, each competitor's going out as well
    /// ([`WatchMoment::Out`]).
    fn tell(&mut self, human: Option<&Player>, decided: bool) {
        let out = human.is_some_and(|h| h.gauge_failed_at().is_some());
        let in_stage = human.is_some_and(|h| self.me_in(h));
        let danger = human.is_some_and(|h| {
            let rules = h.ruleset();
            in_stage && rules.gauge.danger() && !matches!(rules.fail, ddi_engine::FailPolicy::Off)
        });
        let live = usize::from(in_stage)
            + self
                .remote_live
                .as_ref()
                .map_or(usize::from(self.setup.others), live::Live::others_in)
            + self
                .seats
                .iter()
                .filter(|s| s.out_at().is_none())
                .count();
        let milestone = |live: usize, before: usize| {
            (live < before && !decided)
                .then(|| u8::try_from(live).ok())
                .flatten()
                .filter(|n| SURVIVOR_MILESTONES.contains(n))
        };
        if out && !self.out {
            self.out = true;
            self.events.push(Event::Eliminated);
        } else if in_stage {
            // Entering danger, not in it from the start (a gauge of one
            // life is in danger throughout: nothing crosses).
            if danger && self.danger == Some(false) && !self.critical && !decided {
                self.critical = true;
                self.events.push(Event::LifeCritical);
            }
            if let Some(n) = milestone(live, self.live) {
                self.events.push(Event::Survivors(n));
            }
        } else if let Some(n) = milestone(live, self.live) {
            // Watching (never in, or out of it since an earlier frame).
            self.events.push(Event::Survivors(n));
        }
        // Who went out, for someone watching.
        if !in_stage {
            self.outs_told.resize(self.seats.len(), false);
            for (seat, s) in self.seats.iter().enumerate() {
                if s.out_at().is_some() && !self.outs_told[seat] {
                    self.outs_told[seat] = true;
                    self.watch_moments.push(WatchMoment::Out { seat });
                }
            }
        } else {
            // Told once they watch: only what happens from then.
            self.outs_told = self
                .seats
                .iter()
                .map(|s| s.out_at().is_some())
                .collect();
        }
        self.danger = Some(danger);
        self.live = live;
    }

    /// The stage's moments for someone watching since the last call
    /// ([`WatchMoment`]), in the order they came.
    pub fn take_watch_moments(&mut self) -> Vec<WatchMoment> {
        std::mem::take(&mut self.watch_moments)
    }

    /// The local player's attack moments since the last call (an attack
    /// against them committed, one of their own launched), in order,
    /// each with its attack's first shock (judged song seconds).
    pub fn take_attack_moments(&mut self) -> Vec<(AttackMoment, f64)> {
        std::mem::take(&mut self.attack_moments)
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
        if let Some(l) = self.live_attacks() {
            return self.me_battling(human).then(|| l.ledger.bar(self.setup.me));
        }
        let a = self.attacks.as_ref()?;
        self.me_battling(human).then(|| a.bar(self.setup.me))
    }

    /// The live stage's part, where it has attacks (the room's ledger).
    fn live_attacks(&self) -> Option<&live::Live> {
        self.remote_live.as_ref().filter(|_| self.setup.battle.attacks())
    }

    /// The attack against the local player on its way or passing, as its
    /// warning shows it (none once out or after a confirmed win): the
    /// nearer where one more waits behind it (`attacks-v1`'s chain: said
    /// by `more`), the next once the first's last shock has passed, none
    /// once every one has (there is no recovery to wait for).
    pub fn incoming(&self, human: &Player) -> Option<Incoming> {
        if !self.me_battling(human) {
            return None;
        }
        self.incoming_of(self.setup.me)
    }

    /// The attack against the competitor in `seat` on its way or passing,
    /// as a watcher following them is shown it ([`Battle::incoming`] for
    /// that competitor): none once they are out or the stage is decided.
    pub fn incoming_for(&self, seat: usize) -> Option<Incoming> {
        if let Some(l) = self.live_attacks() {
            let r = l.seats.get(seat.checked_sub(self.seats.len())?)?;
            if self.decided.is_some() || r.gone() {
                return None;
            }
            return self.incoming_of(r.participant);
        }
        let s = self.seats.get(seat)?;
        if self.decided.is_some() || s.out_at().is_some() {
            return None;
        }
        self.incoming_of(s.seat.participant)
    }

    fn incoming_of(&self, participant: ParticipantId) -> Option<Incoming> {
        let now = attacks::micros(self.now);
        if let Some(l) = self.live_attacks() {
            // Another competitor's field is drawn the view's delay behind
            // the song: its attack's times are said on the song's clock
            // where that field shows them.
            let d = if participant == self.setup.me {
                0.0
            } else {
                l.field_delay_s(participant)
            };
            let mut all = l.ledger.incoming(participant, now - attacks::micros(d));
            let attack = all.next()?;
            let more = all.next().is_some();
            let plan_at = |k: usize| {
                l.learned
                    .iter()
                    .find(|(id, _)| *id == attack.id)
                    .map_or(k as f64, |(_, t)| *t)
            };
            return Some(Incoming {
                shocks: attack.cells.len().min(usize::from(u8::MAX)) as u8,
                committed: plan_at(0) + d,
                first: attack.cells.first().map_or(0, |c| c.at_us) as f64 / 1e6 + d,
                clear: attack.clear_us as f64 / 1e6 + d,
                more,
            });
        }
        let a = self.attacks.as_ref()?;
        let mut all = a
            .ledger()
            .incoming_all(participant)
            .filter(|x| x.clear_us >= now);
        let attack = all.next()?;
        let more = all.next().is_some();
        let committed = a
            .ledger()
            .checkpoints_us()
            .get(attack.checkpoint)
            .copied()
            .unwrap_or(0);
        Some(Incoming {
            shocks: attack.cells.len().min(usize::from(u8::MAX)) as u8,
            committed: committed as f64 / 1e6,
            first: attack.cells.first().map_or(0, |c| c.at_us) as f64 / 1e6,
            clear: attack.clear_us as f64 / 1e6,
            more,
        })
    }

    /// The local player's attack relationships now ([`Link`]): each attack
    /// they sent or were sent that has not cleared yet, in the order
    /// committed; none once out or after a confirmed win (as the bar and
    /// the warning).
    pub fn links(&self, human: &Player) -> Vec<Link> {
        if !self.me_battling(human) {
            return Vec::new();
        }
        if self.live_attacks().is_some() {
            return self
                .live_links()
                .iter()
                .filter_map(|x| x.of(usize::MAX))
                .collect();
        }
        let Some(a) = self.attacks.as_ref() else {
            return Vec::new();
        };
        let me = self.setup.me;
        let now = attacks::micros(self.now);
        let live: Vec<&ddi_room::attacks::Attack> = a
            .ledger()
            .attacks()
            .iter()
            .filter(|x| {
                matches!(
                    x.state,
                    ddi_room::attacks::AttackState::Active | ddi_room::attacks::AttackState::Cleared
                ) && x.clear_us >= now
            })
            .collect();
        live.iter()
            .filter(|x| x.sender == me || x.recipient == me)
            .filter_map(|x| {
                let other = if x.sender == me { x.recipient } else { x.sender };
                let seat = self.seats.iter().position(|s| s.seat.participant == other)?;
                // Out since: its miniature says so, nothing to point at.
                if x.sender == me && self.seats[seat].out_at().is_some() {
                    return None;
                }
                let committed = a
                    .ledger()
                    .checkpoints_us()
                    .get(x.checkpoint)
                    .copied()
                    .unwrap_or(0) as f64
                    / 1e6;
                Some(Link {
                    attack: x.id,
                    seat,
                    outgoing: x.sender == me,
                    shown_from: self
                        .seen
                        .iter()
                        .find(|(id, _)| *id == x.id)
                        .map_or(committed, |(_, t)| *t),
                    first: x.cells.first().map_or(0, |c| c.at_us) as f64 / 1e6,
                    clear: x.clear_us as f64 / 1e6,
                    chained: live
                        .iter()
                        .any(|y| y.recipient == x.recipient && y.id < x.id),
                })
            })
            .collect()
    }

    /// The seats a stage keeps in view where the opponents take turns
    /// (beyond the cap: `ddi_shell::opponents::rotate`): the senders of
    /// the attacks against the local player, then the recipient of their
    /// latest attack, while those attacks last and `after` seconds more
    /// (one rotation turn).
    pub fn kept_seats(&self, human: &Player, after: f64) -> Vec<usize> {
        let Some(a) = self.attacks.as_ref() else {
            return Vec::new();
        };
        if !self.me_battling(human) {
            return Vec::new();
        }
        let me = self.setup.me;
        let now = self.now;
        let lasts = |x: &&ddi_room::attacks::Attack| {
            x.state != ddi_room::attacks::AttackState::Expired
                && x.clear_us as f64 / 1e6 + after >= now
        };
        let seat = |p: ParticipantId| self.seats.iter().position(|s| s.seat.participant == p);
        let mut kept: Vec<usize> = Vec::new();
        let all = a.ledger().attacks();
        for x in all.iter().filter(|x| x.recipient == me).filter(lasts) {
            if let Some(s) = seat(x.sender).filter(|s| !kept.contains(s)) {
                kept.push(s);
            }
        }
        if let Some(x) = all.iter().rev().find(|x| x.sender == me).filter(lasts)
            && let Some(s) = seat(x.recipient).filter(|s| !kept.contains(s))
        {
            kept.push(s);
        }
        kept
    }

    /// Who attacks whom among the competitors, as someone watching sees
    /// it (`watch-v0`): each attack between two of them that has not
    /// cleared yet, in the order committed; none once the stage is
    /// decided, none against a recipient who went out.
    pub fn seen_links(&self) -> Vec<SeenLink> {
        if self.live_attacks().is_some() {
            if self.decided.is_some() {
                return Vec::new();
            }
            return self
                .live_links()
                .into_iter()
                .filter(|x| x.from != usize::MAX && x.to != usize::MAX)
                .collect();
        }
        let Some(a) = self.attacks.as_ref() else {
            return Vec::new();
        };
        if self.decided.is_some() {
            return Vec::new();
        }
        let now = attacks::micros(self.now);
        let seat = |p: ParticipantId| self.seats.iter().position(|s| s.seat.participant == p);
        let live: Vec<&ddi_room::attacks::Attack> = a
            .ledger()
            .attacks()
            .iter()
            .filter(|x| {
                matches!(
                    x.state,
                    ddi_room::attacks::AttackState::Active | ddi_room::attacks::AttackState::Cleared
                ) && x.clear_us >= now
            })
            .collect();
        live.iter()
            .filter_map(|x| {
                let (from, to) = (seat(x.sender)?, seat(x.recipient)?);
                if self.seats[to].out_at().is_some() {
                    return None;
                }
                let committed = a
                    .ledger()
                    .checkpoints_us()
                    .get(x.checkpoint)
                    .copied()
                    .unwrap_or(0) as f64
                    / 1e6;
                Some(SeenLink {
                    attack: x.id,
                    from,
                    to,
                    shown_from: self
                        .seen
                        .iter()
                        .find(|(id, _)| *id == x.id)
                        .map_or(committed, |(_, t)| *t),
                    first: x.cells.first().map_or(0, |c| c.at_us) as f64 / 1e6,
                    clear: x.clear_us as f64 / 1e6,
                    chained: live
                        .iter()
                        .any(|y| y.recipient == x.recipient && y.id < x.id),
                })
            })
            .collect()
    }

    /// A live stage's attacks between its competitors not cleared yet,
    /// as [`SeenLink`]s: the local player's seat is `usize::MAX` (what
    /// [`SeenLink::of`] of it makes their own [`Link`]s).
    fn live_links(&self) -> Vec<SeenLink> {
        let Some(l) = self.live_attacks() else {
            return Vec::new();
        };
        let now = attacks::micros(self.now);
        let base = self.seats.len();
        let me = self.setup.me;
        let delay = |p: ParticipantId| {
            if p == me {
                0
            } else {
                attacks::micros(l.field_delay_s(p))
            }
        };
        let seat = |p: ParticipantId| {
            if p == self.setup.me {
                Some(usize::MAX)
            } else {
                l.seats.iter().position(|s| s.participant == p).map(|i| base + i)
            }
        };
        let live: Vec<&ddi_room::attacks::Attack> = l
            .ledger
            .attacks
            .iter()
            .filter(|x| {
                matches!(
                    x.state,
                    ddi_room::attacks::AttackState::Active | ddi_room::attacks::AttackState::Cleared
                ) && x.clear_us >= now - delay(x.recipient)
            })
            .collect();
        live.iter()
            .filter_map(|x| {
                let (from, to) = (seat(x.sender)?, seat(x.recipient)?);
                if to != usize::MAX && l.seats[to - base].gone() {
                    return None;
                }
                // On the song's clock where the recipient's field shows
                // it (another's: the view's delay behind).
                let d = delay(x.recipient) as f64 / 1e6;
                Some(SeenLink {
                    attack: x.id,
                    from,
                    to,
                    shown_from: l
                        .learned
                        .iter()
                        .find(|(id, _)| *id == x.id)
                        .map_or(0.0, |(_, t)| *t)
                        + d,
                    first: x.cells.first().map_or(0, |c| c.at_us) as f64 / 1e6 + d,
                    clear: x.clear_us as f64 / 1e6 + d,
                    chained: live
                        .iter()
                        .any(|y| y.recipient == x.recipient && y.id < x.id),
                })
            })
            .collect()
    }

    /// The local player's full bar waiting, and why (none once out or
    /// after a confirmed win).
    pub fn waiting(&self, human: &Player) -> Option<Waiting> {
        if let Some(l) = self.live_attacks() {
            if !self.me_battling(human) {
                return None;
            }
            let me = self.setup.me;
            return l.ledger.waiting(me).map(|reason| Waiting {
                targeting: self.policy,
                reason,
            });
        }
        let a = self.attacks.as_ref()?;
        if !self.me_battling(human) {
            return None;
        }
        a.waiting(self.setup.me)
            .map(|(targeting, reason)| Waiting { targeting, reason })
    }

    /// The local player's attack policy as they have it now, where the
    /// battle has attacks: what the label by the power bar says (none
    /// once out or after a confirmed win, as the bar).
    pub fn policy(&self, human: &Player) -> Option<Targeting> {
        if self.attacks.is_none() && self.live_attacks().is_none() {
            return None;
        }
        self.me_battling(human).then_some(self.policy)
    }

    /// The local player changes their policy to `targeting` at judged
    /// song second `at` (D8: Tab, a pad's View, a tap on the label): the
    /// ledger launches by it from the next checkpoint. Whether it changed
    /// (nothing does without attacks, once out or after a confirmed win).
    pub fn set_targeting(&mut self, targeting: Targeting, at: f64, human: &Player) -> bool {
        if self.policy(human).is_none_or(|p| p == targeting) {
            return false;
        }
        self.retarget(targeting, at);
        true
    }

    /// A policy change as a log kept it (a stage judged again from its
    /// log): given to the ledger whatever the play's state.
    pub fn retarget(&mut self, targeting: Targeting, at: f64) {
        self.policy = targeting;
        if let Some(a) = self.attacks.as_mut() {
            a.retarget(self.setup.me, at, targeting);
        }
        if let Some(l) = self.remote_live.as_mut() {
            l.retarget(at, targeting);
        }
    }

    /// The commitments since the last call, worded for a log (English).
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    /// A live stage's attacks committed against the local player, as each
    /// came (taken, or refused and why).
    pub fn live_incoming(&self) -> &[live::Incoming] {
        self.remote_live.as_ref().map_or(&[], |l| &l.incoming)
    }

    /// Shocks a play refused so far (a frame stalled past the lead).
    pub fn refused(&self) -> u32 {
        self.refused + self.remote_live.as_ref().map_or(0, |l| l.refused)
    }

    /// Decided by a final attack's knockout (`SETTLEMENT_V0`).
    pub fn drawn_by_final_attack(&self) -> bool {
        self.drawn
    }

    /// Waiting on an attack against the last one in.
    pub fn settling(&self) -> Option<Settling> {
        self.settling.filter(|_| self.decided.is_none())
    }

    /// The stage waited for an attack against the last one in before it
    /// was decided (`SETTLEMENT_V0`).
    pub fn waited_to_settle(&self) -> bool {
        self.settling.is_some()
    }

    /// When the stage was decided during the play (the local player's
    /// judged song second).
    pub fn decided_at(&self) -> Option<f64> {
        self.decided_at
    }

    /// The chart's tap-only copy the CPUs play, as their fields draw it.
    pub fn track(&self) -> &feed::TapTrack {
        &self.track
    }

    /// Each opponent's field's stream, in their stable slots' order.
    pub fn feeds(&self) -> impl Iterator<Item = &feed::FieldFeed> {
        self.seats.iter().map(|s| &s.feed).chain(
            self.remote_live
                .iter()
                .flat_map(|l| l.seats.iter().map(|s| &s.feed)),
        )
    }

    /// The opponents in their stable slots (the order the room seated
    /// them in).
    pub fn views(&self) -> Vec<OpponentView> {
        let remote = self.remote_live.iter().flat_map(|l| {
            l.seats.iter().map(|s| {
                // As the view shows them: a delay behind.
                let shown = s.feed.shown(self.now);
                let view = s.feed.shown_score(self.now);
                OpponentView {
                    participant: s.participant,
                    name: s.name.clone(),
                    level: None,
                    life: shown.life,
                    out: shown.out,
                    // The stream tells DDI's sum alone: the percent is
                    // worked out from it, the W0s it told and the chart's
                    // most (`live::ddi_of_sum`); none without a most.
                    score: match (view.and_then(|v| v.ddi), self.most) {
                        (Some(d), Some(_)) if d.most > 0 => accuracy(&ScoreView {
                            ddi: Some(d),
                            ..ScoreView::default()
                        }),
                        (None, Some(_)) => 0.0,
                        _ => f64::NAN,
                    },
                    sum: Some(view.and_then(|v| v.ddi).map_or(0, |d| d.score)),
                    combo: shown.combo,
                    power: self
                        .live_attacks()
                        .filter(|_| !s.gone())
                        .map(|l| l.ledger.bar(s.participant)),
                }
            })
        });
        let mut views: Vec<OpponentView> = self
            .seats
            .iter()
            .map(|s| OpponentView {
                participant: s.seat.participant,
                name: s.name.clone(),
                level: Some(s.level),
                life: s.opponent.life(),
                out: s.out_at().is_some(),
                score: accuracy(&s.opponent.score()),
                sum: cpu_score(&s.opponent.score(), self.most)
                    .ddi
                    .map(|d| d.score),
                combo: s.opponent.combo(),
                power: self
                    .attacks
                    .as_ref()
                    .filter(|_| s.out_at().is_none())
                    .map(|a| a.bar(s.seat.participant)),
            })
            .collect();
        views.extend(remote);
        views
    }

    /// The stage's outcome now the play is over: the ranking decided
    /// during the song, else the opponents and the attacks between them
    /// played out ([`Battle::play_out`]: nothing they do depends on the
    /// player any more) and everyone ranked (`SETTLEMENT_V0`); `quit`,
    /// the local player left while still in (a forfeit; never after the
    /// stage was decided).
    pub fn outcome(&mut self, human: &Player, quit: bool) -> StageOutcome {
        let watched = !self.competes();
        let quit = quit || self.forfeit.is_some();
        // A live stage's stream ends before the run reported with it.
        if let Some(l) = self.remote_live.as_mut()
            && !watched
        {
            let at = human.judge().last_time().unwrap_or(0.0);
            let end = match human.gauge_failed_at() {
                Some(out) => ddi_room::live::StreamEnd::Out {
                    at_us: attacks::micros(out),
                },
                None if quit => ddi_room::live::StreamEnd::Forfeit,
                None => ddi_room::live::StreamEnd::Finished,
            };
            l.end(end, at);
        }
        let mine = if watched {
            Run::Forfeit
        } else {
            self.my_run(human, quit)
        };
        if self.decided.is_none() {
            self.play_out(human, quit);
        }
        let mut details = Vec::new();
        if !watched {
            let mut results = human.results_brief();
            results.score = self.on_stage_scale(human);
            let mut detail = run_detail(
                &results,
                human.ruleset(),
                human.ruleset().gauge.life(),
                self.launched(self.setup.me),
            );
            detail.chart = self.rung_name(human.steps().len()).map(str::to_string);
            details.push((self.setup.me, detail.bounded()));
        }
        details.extend(self.cpu_details());
        let summary = if watched {
            None
        } else {
            self.attack_summary(human)
        };
        if let Some(ranking) = &self.decided {
            return StageOutcome {
                stage: self.setup.stage,
                ranking: ranking.clone(),
                confirmed_early: true,
                mine,
                drawn_by_final_attack: self.drawn,
                details,
                targeting: self.policy,
                summary,
                watched,
            };
        }
        let checkpoints = self
            .attacks
            .as_ref()
            .map_or(&[][..], |a| a.ledger().checkpoints_us());
        let me = (!watched).then_some(mine);
        let (ranking, drawn) = SETTLEMENT_V0.rank(&self.entrants_as(me), checkpoints, self.settling);
        StageOutcome {
            stage: self.setup.stage,
            ranking,
            confirmed_early: false,
            mine,
            drawn_by_final_attack: drawn,
            details,
            targeting: self.policy,
            summary: summary.map(|mut s| {
                s.drawn_by_final_attack = drawn;
                s
            }),
            watched,
        }
    }

    /// The attacks `participant` launched and those sent against it,
    /// where the battle has attacks.
    fn launched(&self, participant: ParticipantId) -> Option<(u32, u32)> {
        if let Some(l) = self.live_attacks() {
            let all = &l.ledger.attacks;
            let sent = all.iter().filter(|a| a.sender == participant).count();
            let got = all.iter().filter(|a| a.recipient == participant).count();
            return Some((sent as u32, got as u32));
        }
        self.attacks.as_ref().map(|a| {
            let s = a.stats(participant);
            (s.launched, s.received)
        })
    }

    /// The local player's attacks of the stage so far, where it has
    /// them ([`AttackSummary`]): every attack committed by them, by
    /// recipient, and against them, by sender (those that expired
    /// included: they were launched), in the order first committed.
    pub fn attack_summary(&self, human: &Player) -> Option<AttackSummary> {
        let me = self.setup.me;
        if let Some(l) = self.live_attacks() {
            let mut summary = AttackSummary {
                shocks: human.results_brief().battle_mines.map_or(0, |b| b.hit),
                drawn_by_final_attack: self.drawn,
                ..AttackSummary::default()
            };
            for x in &l.ledger.attacks {
                let (list, other) = if x.sender == me {
                    (&mut summary.sent, x.recipient)
                } else if x.recipient == me {
                    (&mut summary.taken, x.sender)
                } else {
                    continue;
                };
                match list.iter_mut().find(|(q, _)| *q == other) {
                    Some((_, n)) => *n += 1,
                    None => list.push((other, 1)),
                }
            }
            return Some(summary);
        }
        let a = self.attacks.as_ref()?;
        let mut summary = AttackSummary {
            shocks: human.results_brief().battle_mines.map_or(0, |b| b.hit),
            drawn_by_final_attack: self.drawn,
            ..AttackSummary::default()
        };
        let count = |list: &mut Vec<(ParticipantId, u32)>, p: ParticipantId| {
            match list.iter_mut().find(|(q, _)| *q == p) {
                Some((_, n)) => *n += 1,
                None => list.push((p, 1)),
            }
        };
        for x in a.ledger().attacks() {
            if x.sender == me {
                count(&mut summary.sent, x.recipient);
            } else if x.recipient == me {
                count(&mut summary.taken, x.sender);
            }
        }
        Some(summary)
    }

    /// How each CPU performed, as judged by its own play.
    fn cpu_details(&self) -> Vec<(ParticipantId, RunDetail)> {
        self.seats
            .iter()
            .map(|s| {
                let player = s.opponent.player();
                // Its score on the chart's scale (`CPU_SCORE_V0`).
                let mut results = player.results_brief();
                results.score = cpu_score(&results.score, self.most);
                let mut detail = run_detail(
                    &results,
                    player.ruleset(),
                    s.opponent.life(),
                    self.launched(s.seat.participant),
                );
                detail.chart = self.rung_name(player.steps().len()).map(str::to_string);
                (s.seat.participant, detail.bounded())
            })
            .collect()
    }

    /// The stage's end as the local player's results tell it, from its
    /// outcome `o`: the winner's finished song after an early win (not
    /// after a quit); at the song's end, won alone, first shared or not
    /// won while still in; nothing for one who went out (that was told)
    /// or forfeited.
    pub fn end_event(&self, o: &StageOutcome, quit: bool) -> Option<Event> {
        // An online stage's outcome is the room's, not known here yet
        // (unless the room decided it live before the song's end); a
        // watcher has no end of their own.
        let live_decided = self.remote_live.is_some() && o.confirmed_early;
        if (self.setup.remote() && !live_decided) || !self.competes() {
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

    /// The local player's run now: a forfeit where they left while in
    /// (`quit`, or [`Battle::forfeit_now`]), else out or still in.
    fn my_run(&self, human: &Player, quit: bool) -> Run {
        if (quit || self.forfeit.is_some()) && human.gauge_failed_at().is_none() {
            Run::Forfeit
        } else {
            run_of(
                human.gauge_failed_at(),
                human.ruleset().gauge.life(),
                &self.on_stage_scale(human),
                human.steps().len(),
            )
        }
    }

    /// `player`'s score as the room compares it: on the stage chart's
    /// scale where it stepped down (each stretch counted against what it
    /// was shown; `CPU_SCORE_V0`'s way, as a CPU's).
    fn on_stage_scale(&self, player: &Player) -> ScoreView {
        let view = player.ruleset().score.view();
        if player.steps().is_empty() {
            view
        } else {
            cpu_score(&view, self.most)
        }
    }

    /// Every competitor's run, the local player's `me` first (none: a
    /// watcher's stage, the CPUs alone).
    fn entrants_as(&self, me: Option<Run>) -> Vec<Entrant> {
        let mut out: Vec<Entrant> = me
            .map(|run| Entrant {
                participant: self.setup.me,
                run,
            })
            .into_iter()
            .collect();
        out.extend(self.seats.iter().map(|s| Entrant {
            participant: s.seat.participant,
            run: run_of(
                s.out_at(),
                s.opponent.life(),
                &cpu_score(&s.opponent.score(), self.most),
                s.opponent.player().steps().len(),
            ),
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
    let ladder = stage_ladder(song, chart, &setup);
    let mut b = Battle::new(setup, song, chart, ruleset, 0.0, FORFEIT_HORIZON, &ladder)?;
    // A watcher left it: the CPUs alone, nobody forfeits.
    let me = b.competes().then_some(Run::Forfeit);
    if me.is_some()
        && let Some(a) = b.attacks.as_mut()
    {
        a.out_at(b.setup.me, f64::NEG_INFINITY);
    }
    b.play_out_as(me);
    let entrants = b.entrants_as(me);
    let checkpoints = b
        .attacks
        .as_ref()
        .map_or(&[][..], |a| a.ledger().checkpoints_us());
    let (ranking, drawn) = match &b.decided {
        Some(r) => (r.clone(), b.drawn),
        None => SETTLEMENT_V0.rank(&entrants, checkpoints, b.settling),
    };
    Ok(StageOutcome {
        stage: b.setup.stage,
        ranking,
        confirmed_early: false,
        mine: Run::Forfeit,
        drawn_by_final_attack: drawn,
        details: b.cpu_details(),
        targeting: b.policy,
        // Left at the prompt: nothing of their own to sum up.
        summary: None,
        watched: me.is_none(),
    })
}

/// A competitor's run as the room ranks it: out at `out_at`, else still
/// in with `life` and `score` (a CPU's on the chart's scale,
/// [`cpu_score`]): its proportional part and, under DDI, the score shown
/// (the sum, what the room ranks by after life) with its absolute score
/// and the most its scale gives.
/// `steps`, the steps down it made (a match's "At zero life: Step down").
pub fn run_of(out_at: Option<f64>, life: f32, score: &ScoreView, steps: usize) -> Run {
    match out_at {
        Some(at) => Run::Out { at },
        None => Run::Live {
            life: f64::from(life),
            accuracy: accuracy(score),
            // What an authority before round 3b breaks a tie by.
            absolute: score.ddi.as_ref().map(|d| Absolute {
                score: d.absolute,
                most: d.most,
            }),
            score: score.ddi.as_ref().map(|d| d.score),
            steps: u8::try_from(steps).unwrap_or(u8::MAX),
        },
    }
}

#[cfg(test)]
mod study;


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extreme_slow_speed_is_raised_to_fit_the_lead() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/songs/beatucada/Beatucada.ssc");
        let song = ddi_library::scan::parse_simfile(&std::fs::read_to_string(path).unwrap(), "ssc")
            .unwrap();
        let mut o = ddi_engine::player::PlayOptions::default();
        o.scroll.speed = ddi_engine::scroll::SpeedMod::XMod(2.5);
        assert_eq!(raised_speed(&song, 4, &o), None);
        o.scroll.speed = ddi_engine::scroll::SpeedMod::XMod(0.25);
        let raised = raised_speed(&song, 4, &o).expect("raised");
        let mut r = o;
        r.scroll.speed = raised;
        let timing = song.charts[4].timing(&song);
        let h = ddi_engine::battle::reading_horizon(&r, timing, 0.0, 60.0);
        assert!(h <= LEAD_V0.horizon_cap_us() as f64 / 1e6, "{h}");
    }
}
