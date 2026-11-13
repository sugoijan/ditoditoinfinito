//! Room policy and match rules. The room's policy holds operational values
//! (the competitor cap, readiness timing); a match's rules are locked when
//! the match starts. The next match's rules are an explicit draft the room
//! keeps beside the running match's (`state::NextMatchDraft`).

use crate::awards::{AwardScheme, PointsBias};
use crate::member::OpponentSpec;
use serde::{Deserialize, Serialize};

/// Operational limits and timing for one room. These are policy values,
/// checked in one place each, not array sizes or UI timers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomPolicy {
    /// The competitor cap, humans and CPUs together. 73 (the maintainer,
    /// 2026-11-13: "from 1+8 to 1+72 players"; nine before,
    /// `INITIAL_COMPETITOR_CAP`); nothing else in the model assumes it.
    pub max_competitors: u16,
    /// Room members who are not administrators, watchers included: every
    /// one listed in the room's state, so a bound on what the state and
    /// its writes carry (`DEFAULT_MAX_MEMBERS`, PROVISIONAL). An audience
    /// beyond it is counted and not listed ([`RoomPolicy::audience`]).
    pub max_members: u16,
    /// Extra room for administrators entering to help, so a full room never
    /// shuts them out. Bounded; the real limit is a server-capacity decision.
    pub admin_reserve: u16,
    /// The longest match a rules draft may ask for.
    pub max_stages: u16,
    /// Finished matches kept as records in the room.
    pub history: u16,
    pub readiness: ReadinessPolicy,
    /// When an online stage settles on its members' reports (M5d).
    #[serde(default)]
    pub outcomes: OutcomePolicy,
    /// Live play (`crate::live`, `docs/plans/multiplayer-online-play.md`):
    /// a stage started at one instant on the room's clock, its competitors'
    /// plays streamed, its attacks decided by the room. `None`: an authority
    /// that predates it (or local practice, which judges every play on one
    /// device). ADDITIVE to protocol 2: a client sends live frames only to a
    /// room whose policy has it, so never to an authority that would close
    /// the socket on a frame it cannot read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<LivePolicy>,
    /// The authority reads `crate::Command::RestartMatch` (2026-11-13: the
    /// next match's rules saved and the running match stopped as one
    /// change). True for every room this build makes; false (left out of
    /// the JSON) for a room whose authority predates it, or a room stored
    /// before it. ADDITIVE to protocol 2: a client sends the command only
    /// to a room whose policy says so, so never to an authority that would
    /// close the socket on a command it cannot read.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restart: bool,
    /// The authority awards fields of nine and more by the top-biased
    /// tables at the match's [`MatchRules::points`] (2026-11-13). True for
    /// every room this build makes; false (left out of the JSON) for an
    /// authority that predates it, which ignores the rule (it awards nine
    /// by its own retired rule and seats no more), so the Points row is
    /// offered only where this says so. ADDITIVE to protocol 2.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub points_bias: bool,
    /// An audience beyond the members (`crate::audience`,
    /// `docs/plans/multiplayer-audience.md`): people who watch, counted and
    /// not listed, served by relays. `None`: the authority has none (one
    /// that predates it, or local practice). ADDITIVE to protocol 2: a
    /// client offers the audience's link, count and control only where it
    /// is said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<crate::audience::AudiencePolicy>,
    /// The authority honors the match's "At zero life: Step down"
    /// ([`MatchRules::at_zero`]): it keeps the rule and ranks a stage by
    /// the steps its runs report ([`crate::battle::rank`]). True for every
    /// room this build makes; false (left out of the JSON) for an
    /// authority that predates it, which would drop the rule and rank a
    /// stepped-down run as any run still in, so the row is offered only
    /// where this says so. ADDITIVE to protocol 2.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub step_down: bool,
    /// The authority reads `crate::Command::ChangeStageRules` (2026-11-13:
    /// a running match's per-stage rules, [`PlayRules`], changed from a
    /// later stage on) and keeps the changes on the match
    /// (`state::Match::changes`). True for every room this build makes;
    /// false (left out of the JSON) for a room whose authority predates
    /// it, where the Rules pop-up keeps its form of before and the command
    /// is never sent. ADDITIVE to protocol 2.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stage_rules: bool,
    /// The authority seats a match's CPUs in a room that plays live and
    /// takes their reports from the devices that compute them
    /// (`crate::live::cpus`, `online-cpus-v0`, 2026-11-13): its live side
    /// keeps each CPU in the ledger and the ranking, the first report of
    /// each checkpoint taken. True for every room this build makes (it
    /// matters only with [`RoomPolicy::live`]: practice judges its CPUs
    /// on its one device); false (left out of the JSON) for an authority
    /// that predates it, which would never hear from a CPU, so a client
    /// offers the roster online only where this says so. ADDITIVE to
    /// protocol 2.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cpus: bool,
}

/// What a room that plays live tells its clients of how
/// (`crate::live::LiveRules` by name, and when a stage's song begins).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LivePolicy {
    /// The live contract's id (`crate::live::LIVE_V1`'s `live-v1`): the
    /// messages and their meaning, which every live build speaks. A build
    /// that knows this id plays live in the room.
    pub id: String,
    /// From a stage's committed start (the countdown's end) to its song's
    /// time zero, on the room's clock: room for every device's own lead-in
    /// (a play is drawn from song second −0.8). PROVISIONAL.
    pub prelude_ms: u64,
    /// The room's timing, where it is not the contract's own
    /// (`crate::live::LIVE_V2`'s `live-v2`: a frame and a batch every
    /// 100 ms, 2026-11-13). ADDITIVE: a build that predates it reads the
    /// id alone and plays live by `live-v1`'s numbers (batches every
    /// 250 ms, its views a second behind), which a room ticking faster
    /// serves as well; a build that knows it follows it. A room without
    /// it (one made before it) runs `live-v1`'s numbers for every build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<String>,
}

impl LivePolicy {
    /// `live-v1` (PROVISIONAL): song zero one second after the countdown,
    /// a frame and a batch every 250 ms.
    pub fn v1() -> Self {
        Self {
            id: crate::live::LIVE_V1.id.into(),
            prelude_ms: 1_000,
            timing: None,
        }
    }

    /// `live-v1`'s contract with `live-v2`'s timing (PROVISIONAL, what a
    /// room this build makes plays): a frame and a batch every 100 ms.
    pub fn v2() -> Self {
        Self {
            timing: Some(crate::live::LIVE_V2.id.into()),
            ..Self::v1()
        }
    }

    /// The live rules it names, if this build knows them: its timing's
    /// where it names one this build knows, else its contract's.
    pub fn rules(&self) -> Option<crate::live::LiveRules> {
        let base = crate::live::LiveRules::named(&self.id)?;
        Some(
            self.timing
                .as_deref()
                .and_then(crate::live::LiveRules::named)
                .unwrap_or(base),
        )
    }
}

impl RoomPolicy {
    /// This policy with an audience (`AudiencePolicy::v0`): what an online
    /// room's authority that serves one sets.
    pub fn with_audience(mut self) -> Self {
        self.audience = Some(crate::audience::AudiencePolicy::v0());
        self
    }

    /// This policy with live play (`LivePolicy::v2`): what an online
    /// room's authority that plays live sets.
    pub fn with_live(mut self) -> Self {
        self.live = Some(LivePolicy::v2());
        self
    }
}

impl RoomPolicy {
    /// Whether a room under this policy whose content is `source` can
    /// seat CPUs: practice always (its one device judges them), an online
    /// room where it plays live and its authority seats them
    /// ([`RoomPolicy::cpus`]).
    pub fn seats_cpus(&self, source: ContentSource) -> bool {
        match source {
            ContentSource::LocalLibrary => true,
            ContentSource::Shared => self.cpus && self.live.is_some(),
        }
    }

    /// The operational cap (`docs/plans/multiplayer.md`, "Eight first and
    /// larger parties later"; raised to nine, eight opponents beside the
    /// player, M6b; to 73, 1+72, on 2026-11-13,
    /// `docs/plans/multiplayer-prototype-progress.md`, "Rooms of 1+72
    /// players"). The top-biased award tables go as far
    /// (`crate::TopBiased::MAX_FIELD`).
    pub const INITIAL_COMPETITOR_CAP: u16 = 73;

    /// The member bound, PROVISIONAL (`member-bound-v0`, 2026-11-13): a
    /// full field and about three hundred members watching it, every one
    /// listed in the state the server writes on each change and sends in
    /// each snapshot (measured: `tests/scale.rs`, `what_a_large_room_costs`;
    /// the server's own load test carried 73 and 300 watchers).
    pub const DEFAULT_MAX_MEMBERS: u16 = 384;

    /// DEVELOPMENT ONLY: a practice room's cap for trying more CPUs
    /// than a match allows (kept for stress runs past 73) (`docs/plans/multiplayer-prototype-progress.md`,
    /// M6b). Never a hosted room's, never reachable from the game's menus.
    pub const DEVELOPMENT_COMPETITOR_CAP: u16 = 129;

    /// DEVELOPMENT ONLY: this policy with the cap raised to
    /// [`RoomPolicy::DEVELOPMENT_COMPETITOR_CAP`].
    pub fn development() -> Self {
        Self {
            max_competitors: Self::DEVELOPMENT_COMPETITOR_CAP,
            ..Self::default()
        }
    }

    /// The one place the competitor cap is checked: reservations, CPU seats
    /// and rules all come through here.
    pub fn check_competitors(&self, requested: usize) -> Result<(), CapExceeded> {
        if requested > usize::from(self.max_competitors) {
            Err(CapExceeded {
                requested: requested.try_into().unwrap_or(u16::MAX),
                cap: self.max_competitors,
            })
        } else {
            Ok(())
        }
    }
}

impl Default for RoomPolicy {
    fn default() -> Self {
        Self {
            max_competitors: Self::INITIAL_COMPETITOR_CAP,
            max_members: Self::DEFAULT_MAX_MEMBERS,
            admin_reserve: 4,
            max_stages: 16,
            history: 8,
            readiness: ReadinessPolicy::default(),
            outcomes: OutcomePolicy::default(),
            live: None,
            restart: true,
            points_bias: true,
            audience: None,
            step_down: true,
            stage_rules: true,
            cpus: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapExceeded {
    pub requested: u16,
    pub cap: u16,
}

/// How a start treats unready people (`docs/plans/multiplayer.md`, "Hosted
/// readiness without an indefinite wait"): a pick (or Start) opens a window
/// when someone eligible is unready; at its deadline the ready and loaded
/// start and the others sit out the stage with zero points. Nobody's
/// unreadiness vetoes the start. `hosted-deadline-v1` (M7b, the maintainer's
/// flow): the window counts from when the song is available to everyone,
/// and the stage starts as soon as every connected player who could still
/// join is ready and loaded (v0 never shortened it). PROVISIONAL: every
/// number here is a working value, not a decided one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessPolicy {
    /// Names this policy and its version in records and on the wire.
    pub id: String,
    /// The readiness window. 15 s is the studies' test value.
    pub window_ms: u64,
    /// The committed start's countdown.
    pub countdown_ms: u64,
    /// Ready and loaded people needed to start (a window or at once).
    pub min_ready_humans: u16,
    /// Competitors needed to start, CPUs included.
    pub min_competitors: u16,
}

impl ReadinessPolicy {
    pub const HOSTED_DEADLINE_V0: &'static str = "hosted-deadline-v0";
    pub const HOSTED_DEADLINE_V1: &'static str = "hosted-deadline-v1";
}

impl Default for ReadinessPolicy {
    fn default() -> Self {
        Self {
            id: Self::HOSTED_DEADLINE_V1.into(),
            // Long enough for a member to download a song not shared ahead
            // (the spotlight's upload usually makes it ready at once).
            window_ms: 30_000,
            countdown_ms: 3_000,
            min_ready_humans: 1,
            min_competitors: 2,
        }
    }
}

/// When a stage played on its members' own devices (an online room's: no
/// CPUs) settles: once every starter reported its run
/// (`Command::ReportOutcome`), or at a deadline, those who never reported
/// then counted as forfeits. PROVISIONAL (`self-reported-v0`): the song's
/// length is not the room's to know, so the deadline is a wait after the
/// first report of a song played to its end (each device starts its play
/// on its own Start press after the countdown, so it covers a late press
/// too), and a cap from the start for a stage nobody finished.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomePolicy {
    pub id: String,
    /// After the first report of the song played to its end.
    pub after_song_end_ms: u64,
    /// After the stage's start, whatever was reported.
    pub stage_cap_ms: u64,
}

impl OutcomePolicy {
    pub const SELF_REPORTED_V0: &'static str = "self-reported-v0";
}

impl Default for OutcomePolicy {
    fn default() -> Self {
        Self {
            id: Self::SELF_REPORTED_V0.into(),
            after_song_end_ms: 60_000,
            stage_cap_ms: 20 * 60_000,
        }
    }
}

/// Where a stage's content comes from, which decides whether choosing it
/// makes it available at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentSource {
    /// Local practice: the player's own library. Chosen content is available.
    LocalLibrary,
    /// A hosted room: chosen content is pending until the authority confirms
    /// it was shared.
    Shared,
}

/// Room access: future admission only. Changing it never removes members,
/// changes match rules or clears readiness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Access {
    pub admission: Admission,
    /// Bumped to replace the invite; the authority keeps the secret itself.
    pub invite_generation: u32,
}

impl Default for Access {
    fn default() -> Self {
        Self {
            admission: Admission::Open,
            invite_generation: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Admission {
    /// Newcomers with the invite may enter.
    Open,
    /// No new members; present and returning members are unaffected.
    Closed,
}

/// One match's rules, locked from its start to its end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRules {
    /// Songs in the match. Four is a working default, not a limit.
    pub stages: u16,
    /// Playing seats, humans and CPUs. At most the room's competitor cap.
    pub seats: u16,
    pub cpu: CpuRules,
    /// Everyone returns at the next stage after an elimination (default).
    /// Off: elimination, a forfeit or a missed stage ends the match for them.
    pub return_each_stage: bool,
    /// Newcomers may enter at a later stage's boundary into a vacant seat,
    /// with zero points for the stages they missed. Default Off.
    pub mid_match_entry: bool,
    /// The battle preset, opaque to the room (attacks, defense, gauge): the
    /// Attacks rule chooses it ([`MatchRules::attacks`],
    /// [`MatchRules::set_attacks`]).
    pub battle: String,
    pub awards: AwardScheme,
    /// The ruleset everyone plays the match's stages under (the
    /// maintainer, 2026-11-11, D0; DDI alone since the DDI ruleset's round
    /// 3, `docs/plans/ddi-ruleset.md`), AS STORED OR RECEIVED: an id of
    /// [`MatchRules::RULESETS`], or one a room could be played under before
    /// ([`MatchRules::LEGACY_RULESETS`]), kept as it came so that rules sent
    /// back to an authority one build behind are ones it knows. Read it
    /// through [`MatchRules::ruleset_id`]: every one of them is DDI now.
    /// Never a player's own Options ruleset.
    #[serde(default = "MatchRules::default_ruleset")]
    pub ruleset: String,
    /// DDI's difficulty for the match, everyone's (Normal by default): the
    /// timing grid and what a loss costs. A plain value; the stage's runner
    /// builds the engine's rules from it (`ddi_session::battle::stage_ruleset`).
    /// ADDITIVE: left out of the JSON while Normal, read as Normal where
    /// missing (an authority one build behind drops it).
    #[serde(default, skip_serializing_if = "MatchDifficulty::is_normal")]
    pub difficulty: MatchDifficulty,
    /// DDI's Boos for the match (Off by default): a press matched to no
    /// note counts against everyone alike. ADDITIVE, as `difficulty`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub boos: bool,
    /// How strongly a field of nine and more favors its top places
    /// (Strong by default; a field of eight or fewer never reads it).
    /// ADDITIVE, as `difficulty`: left out while Strong, read as Strong
    /// where missing; an authority that predates it drops it and awards
    /// by its own rule (`RoomPolicy::points_bias` says which).
    #[serde(default, skip_serializing_if = "PointsBias::is_strong")]
    pub points: PointsBias,
    /// What zero life does in a stage (Out by default): out of the stage,
    /// or a step down to the song's next easier chart while it has one
    /// (`ddi_engine::ladder`, PROVISIONAL `step-down-v0`). ADDITIVE, as
    /// `difficulty`: left out while Out, read as Out where missing; an
    /// authority that predates it drops it (`RoomPolicy::step_down` says
    /// whether it is honored).
    #[serde(default, skip_serializing_if = "AtZero::is_out")]
    pub at_zero: AtZero,
}

/// The rules each stage is played under on its own (2026-11-13): those of
/// a match's rules that may change from a later stage of a running match
/// (`crate::Command::ChangeStageRules`, `state::Match::changes`): Timing
/// difficulty, Boos, Attacks (the battle preset) and At zero life. The
/// stage count, the seats and the CPUs, the ruleset and the award curve
/// (`MatchRules::points`) stay the match's: they say who is seated and
/// what every stage is worth against the others. Its fields are
/// [`MatchRules`]'s, read and written as there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayRules {
    #[serde(default, skip_serializing_if = "MatchDifficulty::is_normal")]
    pub difficulty: MatchDifficulty,
    #[serde(default, skip_serializing_if = "is_false")]
    pub boos: bool,
    /// The battle preset's id ([`MatchRules::battle`]).
    pub battle: String,
    #[serde(default, skip_serializing_if = "AtZero::is_out")]
    pub at_zero: AtZero,
}

impl PlayRules {
    /// The Attacks rule ([`MatchRules::attacks`]).
    pub fn attacks(&self) -> bool {
        crate::battle::BattlePreset::parse(&self.battle)
            .is_some_and(crate::battle::BattlePreset::attacks)
    }

    /// Whether a room under `policy` can play them: a battle's id of a
    /// sane length (the room keeps it opaque, as the match's), and Step
    /// down only where its authority honors it.
    pub fn check(&self, policy: &RoomPolicy) -> Result<(), RulesProblem> {
        if self.battle.is_empty() || self.battle.len() > 64 {
            return Err(RulesProblem::BattleId);
        }
        if self.at_zero == AtZero::StepDown && !policy.step_down {
            return Err(RulesProblem::StepDownNotHonored);
        }
        Ok(())
    }
}

/// What zero life does in a stage ([`MatchRules::at_zero`]).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AtZero {
    /// Out of the stage (as ever).
    #[default]
    Out,
    /// A step down to the song's next easier chart, out on the lowest.
    StepDown,
}

impl AtZero {
    pub const ALL: [AtZero; 2] = [Self::Out, Self::StepDown];

    pub fn is_out(&self) -> bool {
        *self == Self::Out
    }

    /// Its place in [`AtZero::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Out => "out",
            Self::StepDown => "step_down",
        }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// DDI's difficulty as a match rule (`MatchRules::difficulty`): the room
/// carries the choice, never the engine's type.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum MatchDifficulty {
    Easy,
    #[default]
    Normal,
    Hard,
}

impl MatchDifficulty {
    pub const ALL: [MatchDifficulty; 3] = [Self::Easy, Self::Normal, Self::Hard];

    pub fn is_normal(&self) -> bool {
        *self == Self::Normal
    }

    /// Its place in [`MatchDifficulty::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

impl MatchRules {
    pub const PROVISIONAL_BATTLE: &'static str = "provisional-standard";

    /// A room's ruleset: DDI, our own (D0).
    pub const DEFAULT_RULESET: &'static str = "ddi";

    /// The rulesets a match may be played under: DDI alone
    /// (`docs/plans/ddi-ruleset.md`, "What it means for multiplayer").
    pub const RULESETS: [&'static str; 1] = ["ddi"];

    /// The ids a room's rules could name before DDI was the only one
    /// (M8a's list): still read, from a stored room or an authority one
    /// build behind, and played as DDI ([`MatchRules::ruleset_id`]).
    pub const LEGACY_RULESETS: [&'static str; 7] = [
        "itg",
        "itg-fa+",
        "sm5",
        "ddr-a",
        "ddr-x",
        "ddr-extreme",
        "ffr",
    ];

    fn default_ruleset() -> String {
        Self::DEFAULT_RULESET.into()
    }

    /// The ruleset the match is played under, by the engine's id: DDI,
    /// whatever older id the rules carry.
    pub fn ruleset_id(&self) -> &'static str {
        Self::DEFAULT_RULESET
    }

    /// The Attacks rule: the stage's battle has attack power, launches
    /// and shocks ([`crate::BattlePreset::GaugeAttacksV0`]). On by default
    /// where attacks are offered, which is CPU practice
    /// ([`MatchRules::practice`]; the maintainer, 2026-11-12); a hosted
    /// room's rules name no such battle ([`MatchRules::hosted`]), so an
    /// online room, whoever made it, has none until online attacks exist.
    pub fn attacks(&self) -> bool {
        crate::battle::BattlePreset::parse(&self.battle)
            .is_some_and(crate::battle::BattlePreset::attacks)
    }

    /// The rules a stage is played under on its own ([`PlayRules`]).
    pub fn play_rules(&self) -> PlayRules {
        PlayRules {
            difficulty: self.difficulty,
            boos: self.boos,
            battle: self.battle.clone(),
            at_zero: self.at_zero,
        }
    }

    /// These rules with a stage's own `play` rules in place of theirs.
    pub fn with_play(&self, play: &PlayRules) -> MatchRules {
        MatchRules {
            difficulty: play.difficulty,
            boos: play.boos,
            battle: play.battle.clone(),
            at_zero: play.at_zero,
            ..self.clone()
        }
    }

    /// Sets the Attacks rule: the battle with attacks, or survival alone.
    pub fn set_attacks(&mut self, on: bool) {
        let preset = if on {
            crate::battle::BattlePreset::GaugeAttacksV0
        } else {
            crate::battle::BattlePreset::GaugeSurvivalV0
        };
        self.battle = preset.id().into();
    }

    /// Local CPU practice: one player and three CPUs, four stages, the
    /// battle the practice stages are played under (PROVISIONAL:
    /// `BattlePreset::GaugeAttacksV0`, the Attacks rule On: anyone turns
    /// it Off in Rules, [`MatchRules::set_attacks`]).
    pub fn practice(opponent: OpponentSpec) -> Self {
        Self {
            stages: 4,
            seats: 4,
            cpu: CpuRules {
                mode: CpuMode::Fixed { count: 3 },
                opponent,
                roster: Vec::new(),
            },
            battle: crate::battle::BattlePreset::GAUGE_ATTACKS_V0.into(),
            ..Self::hosted()
        }
    }

    /// A hosted room's starting rules: four stages, a seat for each of the
    /// cap's competitors, no CPUs.
    pub fn hosted() -> Self {
        Self {
            stages: 4,
            seats: RoomPolicy::INITIAL_COMPETITOR_CAP,
            cpu: CpuRules {
                mode: CpuMode::Off,
                opponent: OpponentSpec {
                    level: 0,
                    seed: 0,
                    personality: None,
                    name: None,
                    targeting: None,
                },
                roster: Vec::new(),
            },
            return_each_stage: true,
            mid_match_entry: false,
            battle: Self::PROVISIONAL_BATTLE.into(),
            awards: AwardScheme::PlacementPointsV1,
            ruleset: Self::DEFAULT_RULESET.into(),
            difficulty: MatchDifficulty::Normal,
            boos: false,
            points: PointsBias::Strong,
            at_zero: AtZero::Out,
        }
    }

    /// A hosted room's starting rules under `policy`, and what Rules'
    /// Reset restores there: [`MatchRules::hosted`], with the Attacks rule
    /// On where the room plays live (`RoomPolicy::live`: its authority
    /// runs the attack ledger), as practice has it (the maintainer,
    /// 2026-11-13: "Attacks should be on by default on private rooms").
    /// Off where it does not, which plays none whatever the rule says.
    /// Only a new room starts so: a room's stored rules stay as its host
    /// left them.
    pub fn hosted_for(policy: &RoomPolicy) -> Self {
        let mut rules = Self::hosted();
        // An authority one build behind seats fewer (nine).
        rules.seats = rules.seats.min(policy.max_competitors);
        if policy.live.is_some() {
            rules.set_attacks(true);
        }
        rules
    }

    /// How many CPUs a match with `humans` admitted people would seat.
    pub fn cpu_seats(&self, humans: usize) -> usize {
        match self.cpu.mode {
            CpuMode::Off => 0,
            CpuMode::Fixed { count } => usize::from(count),
            CpuMode::Fill => usize::from(self.seats).saturating_sub(humans),
            CpuMode::Roster => self.cpu.roster.len(),
        }
    }

    /// The `ordinal`th CPU seat's profile (from 1): the roster's entry, or
    /// the common profile with its own seed.
    pub fn cpu_for_seat(&self, ordinal: u16) -> OpponentSpec {
        match self.cpu.mode {
            CpuMode::Roster => self
                .cpu
                .roster
                .get(usize::from(ordinal).saturating_sub(1))
                .cloned()
                .unwrap_or_else(|| self.cpu.opponent.for_seat(ordinal)),
            _ => self.cpu.opponent.for_seat(ordinal),
        }
    }

    /// Check these rules for a match of the `humans` people who want to
    /// play. Run when the draft is saved, again at the match boundary and
    /// at the start, since reservations change in between.
    pub fn validate(&self, policy: &RoomPolicy, humans: usize) -> Result<(), RulesProblem> {
        if self.stages == 0 || self.stages > policy.max_stages {
            return Err(RulesProblem::StageCount {
                stages: self.stages,
                max: policy.max_stages,
            });
        }
        if let Err(cap) = policy.check_competitors(self.seats.into()) {
            return Err(RulesProblem::OverCap(cap));
        }
        if self.seats < policy.readiness.min_competitors {
            return Err(RulesProblem::TooFewSeats {
                seats: self.seats,
                min: policy.readiness.min_competitors,
            });
        }
        if !self.return_each_stage && self.mid_match_entry {
            return Err(RulesProblem::EliminationWithLateEntry);
        }
        if !self.cpu.opponent.fits() || !self.cpu.roster.iter().all(OpponentSpec::fits) {
            return Err(RulesProblem::CpuText);
        }
        let id = self.ruleset.as_str();
        if !Self::RULESETS.contains(&id) && !Self::LEGACY_RULESETS.contains(&id) {
            return Err(RulesProblem::UnknownRuleset);
        }
        let seats = usize::from(self.seats);
        if humans > seats {
            return Err(RulesProblem::HumansExceedSeats {
                humans: humans.try_into().unwrap_or(u16::MAX),
                seats: self.seats,
            });
        }
        let cpus = self.cpu_seats(humans);
        if humans + cpus > seats {
            return Err(RulesProblem::CpusExceedSeats {
                humans: humans.try_into().unwrap_or(u16::MAX),
                cpus: cpus.try_into().unwrap_or(u16::MAX),
                seats: self.seats,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuRules {
    pub mode: CpuMode,
    /// The common profile of the match's CPUs; each seat gets its own seed.
    pub opponent: OpponentSpec,
    /// [`CpuMode::Roster`]'s CPUs, each its own profile (M7b's named CPUs),
    /// in seat order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roster: Vec<OpponentSpec>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuMode {
    Off,
    Fixed {
        count: u16,
    },
    /// Fill the seats people leave open at the roster's lock.
    Fill,
    /// The CPUs [`CpuRules::roster`] lists, each its own (M7b).
    Roster,
}

/// Why a set of rules cannot be used (now).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RulesProblem {
    StageCount {
        stages: u16,
        max: u16,
    },
    OverCap(CapExceeded),
    TooFewSeats {
        seats: u16,
        min: u16,
    },
    /// Persistent elimination and mid-match entry cannot be combined: a
    /// late entrant would skip the elimination that defines the format.
    EliminationWithLateEntry,
    /// More people want to play than the rules seat.
    HumansExceedSeats {
        humans: u16,
        seats: u16,
    },
    /// A fixed CPU count leaves no room for the people who want to play.
    CpusExceedSeats {
        humans: u16,
        cpus: u16,
        seats: u16,
    },
    /// A CPU's name or personality is too long.
    CpuText,
    /// Neither one of [`MatchRules::RULESETS`] nor an id read as one
    /// ([`MatchRules::LEGACY_RULESETS`]).
    UnknownRuleset,
    /// A stage's rules ([`PlayRules`]) name no battle, or one too long.
    BattleId,
    /// Step down where the room's authority does not honor it
    /// ([`RoomPolicy::step_down`]).
    StepDownNotHonored,
    /// CPUs in a room whose authority cannot seat them: an online room
    /// that does not play live, or whose authority predates CPUs online
    /// ([`RoomPolicy::cpus`]).
    CpusNotHonored,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_zero_is_an_additive_match_rule_and_the_policy_says_it_is_honored() {
        // Out by default, left out of the JSON then, read as Out where
        // missing (rules from an authority that predates it).
        let rules = MatchRules::hosted();
        assert_eq!(rules.at_zero, AtZero::Out);
        let json = serde_json::to_value(&rules).unwrap();
        assert!(json.get("at_zero").is_none());
        let mut on = rules.clone();
        on.at_zero = AtZero::StepDown;
        let json = serde_json::to_value(&on).unwrap();
        assert_eq!(json["at_zero"], "step_down");
        assert_eq!(serde_json::from_value::<MatchRules>(json).unwrap(), on);
        // This build's authority honors it; one that predates it says
        // nothing, read as not.
        let policy = serde_json::to_value(RoomPolicy::default()).unwrap();
        assert_eq!(policy["step_down"], true);
        let mut old = policy.clone();
        old.as_object_mut().unwrap().remove("step_down");
        assert!(!serde_json::from_value::<RoomPolicy>(old).unwrap().step_down);
    }

    #[test]
    fn ddi_is_the_only_ruleset_and_old_values_read_as_it() {
        assert_eq!(MatchRules::RULESETS, ["ddi"]);
        assert_eq!(MatchRules::hosted().ruleset, "ddi");
        assert_eq!(MatchRules::practice(spec()).ruleset, "ddi");
        assert_eq!(MatchRules::hosted().ruleset_id(), "ddi");
        // Rules kept before the ruleset was a rule.
        let mut json = serde_json::to_value(MatchRules::hosted()).unwrap();
        json.as_object_mut().unwrap().remove("ruleset");
        let old: MatchRules = serde_json::from_value(json).unwrap();
        assert_eq!(old, MatchRules::hosted());
        // Every id a room could name before is read, valid, kept as it
        // came (an authority one build behind knows it) and played as DDI.
        for id in MatchRules::LEGACY_RULESETS {
            let mut json = serde_json::to_value(MatchRules::hosted()).unwrap();
            json["ruleset"] = id.into();
            let r: MatchRules = serde_json::from_value(json).unwrap();
            assert_eq!(r.ruleset, id);
            assert_eq!(r.ruleset_id(), "ddi", "{id}");
            assert!(r.validate(&RoomPolicy::default(), 1).is_ok(), "{id}");
            assert_eq!(serde_json::to_value(&r).unwrap()["ruleset"], id);
        }
    }

    #[test]
    fn difficulty_and_boos_are_additive_match_rules() {
        let rules = MatchRules::hosted();
        assert_eq!(rules.difficulty, MatchDifficulty::Normal);
        assert!(!rules.boos);
        // The defaults are left out: the rules are on the wire what they
        // were, so an authority or a client one build behind reads them.
        let json = serde_json::to_value(&rules).unwrap();
        assert!(json.get("difficulty").is_none() && json.get("boos").is_none());
        let mut set = rules.clone();
        set.difficulty = MatchDifficulty::Hard;
        set.boos = true;
        let json = serde_json::to_value(&set).unwrap();
        assert_eq!(json["difficulty"], "hard");
        assert_eq!(json["boos"], true);
        assert_eq!(serde_json::from_value::<MatchRules>(json).unwrap(), set);
        assert!(set.validate(&RoomPolicy::default(), 1).is_ok());
        assert_eq!(
            MatchDifficulty::ALL.map(MatchDifficulty::index),
            [0, 1, 2]
        );
    }

    #[test]
    fn the_points_bias_is_an_additive_match_rule() {
        let rules = MatchRules::hosted();
        assert_eq!(rules.points, PointsBias::Strong);
        let json = serde_json::to_value(&rules).unwrap();
        assert!(json.get("points").is_none(), "left out at Strong");
        let mut set = rules.clone();
        set.points = PointsBias::Mild;
        let json = serde_json::to_value(&set).unwrap();
        assert_eq!(json["points"], "mild");
        assert_eq!(serde_json::from_value::<MatchRules>(json).unwrap(), set);
        // The policy says the authority reads it; an older one's does not.
        let policy = serde_json::to_value(RoomPolicy::default()).unwrap();
        assert_eq!(policy["points_bias"], true);
        let mut old = policy.clone();
        old.as_object_mut().unwrap().remove("points_bias");
        assert!(!serde_json::from_value::<RoomPolicy>(old).unwrap().points_bias);
    }

    #[test]
    fn attacks_are_a_match_rule_that_chooses_the_battle() {
        // On by default where attacks are offered: CPU practice.
        let mut rules = MatchRules::practice(spec());
        assert!(rules.attacks(), "on by default in practice");
        assert_eq!(rules.battle, "gauge-attacks-v0");
        rules.set_attacks(false);
        assert!(!rules.attacks());
        assert_eq!(rules.battle, "gauge-survival-v0");
        rules.set_attacks(true);
        assert!(rules.attacks());
        assert_eq!(rules.battle, "gauge-attacks-v0");
        // A hosted room's battle is not one with attacks, whoever made
        // the room (an older client's and an older server's rules name
        // the same battle).
        assert!(!MatchRules::hosted().attacks());
        assert_eq!(MatchRules::hosted().battle, MatchRules::PROVISIONAL_BATTLE);
    }

    fn spec() -> OpponentSpec {
        OpponentSpec {
            level: 2,
            seed: 1,
            personality: None,
            name: None,
            targeting: None,
        }
    }

    #[test]
    fn the_cap_is_one_policy_check() {
        let policy = RoomPolicy::default();
        assert!(policy.check_competitors(73).is_ok());
        assert_eq!(
            policy.check_competitors(74),
            Err(CapExceeded {
                requested: 74,
                cap: 73
            })
        );
        let mut rules = MatchRules::hosted();
        rules.seats = 74;
        assert_eq!(
            rules.validate(&policy, 1),
            Err(RulesProblem::OverCap(CapExceeded {
                requested: 74,
                cap: 73
            }))
        );
        // A larger policy is a value, not an architecture change.
        let large = RoomPolicy {
            max_competitors: 128,
            ..RoomPolicy::default()
        };
        assert!(rules.validate(&large, 1).is_ok());
    }

    #[test]
    fn rules_are_checked_against_the_people_who_want_to_play() {
        let policy = RoomPolicy::default();
        let practice = MatchRules::practice(spec());
        assert!(practice.validate(&policy, 1).is_ok());
        assert_eq!(
            practice.validate(&policy, 2),
            Err(RulesProblem::CpusExceedSeats {
                humans: 2,
                cpus: 3,
                seats: 4
            })
        );
        let mut fill = MatchRules::hosted();
        fill.cpu.mode = CpuMode::Fill;
        fill.seats = 4;
        assert_eq!(fill.cpu_seats(3), 1);
        assert_eq!(
            fill.validate(&policy, 5),
            Err(RulesProblem::HumansExceedSeats {
                humans: 5,
                seats: 4
            })
        );
        let mut both = MatchRules::hosted();
        both.return_each_stage = false;
        both.mid_match_entry = true;
        assert_eq!(
            both.validate(&policy, 1),
            Err(RulesProblem::EliminationWithLateEntry)
        );
        let mut odd = MatchRules::hosted();
        odd.ruleset = "danoni".into();
        assert_eq!(odd.validate(&policy, 1), Err(RulesProblem::UnknownRuleset));
        let mut none = MatchRules::hosted();
        none.stages = 0;
        assert!(matches!(
            none.validate(&policy, 1),
            Err(RulesProblem::StageCount { .. })
        ));
    }
}
