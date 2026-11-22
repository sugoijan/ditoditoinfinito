//! Each player's own starting chart under Charts: Adaptive (room model
//! revision 23, round OC: "each player will play their preferred
//! difficulty on that song"), through the reducer: chosen, defaulted to
//! the stage's own, changed and taken back while the stage gathers
//! readiness, locked at its start, a late entrant's chosen until they
//! enter, refused under As picked (and read as none once a stage is
//! played as picked), a chart not of the song or not of its style
//! refused, a new pick taking every start with it, the audience told the
//! starters' charts, and a room stored by revision 22 read with none.
//! Every revision's events are applied to a mirror, which must stay
//! equal.

mod common;

use common::Room;
use ddi_room::*;

fn line(n: u32, k: u32, style: &str, difficulty: &str, meter: u32) -> ChartLine {
    ChartLine {
        chart: chart(n, k),
        style: style.into(),
        difficulty: difficulty.into(),
        meter,
    }
}

fn chart(n: u32, k: u32) -> ChartKey {
    ChartKey::new(format!("d{n}-{k}"))
}

/// Song `n` with an Easy, a Medium and a Hard single chart and a Hard
/// double one.
fn new_song(n: u32) -> NewSong {
    NewSong {
        key: SongKey::new(format!("s{n:015x}")),
        card: SongCard {
            title: format!("Song {n}"),
            artist: "Artist".into(),
            pack: "Pack".into(),
        },
        charts: vec![
            line(n, 0, "dance-single", "Easy", 3),
            line(n, 1, "dance-single", "Medium", 6),
            line(n, 2, "dance-single", "Hard", 9),
            line(n, 3, "dance-double", "Hard", 10),
        ],
    }
}

/// A room playing live under `charts`, two songs listed (the host's,
/// there to load), Ana and Bob who play, Wes who watches.
struct Starts {
    r: Room,
    ana: ParticipantId,
    bob: ParticipantId,
    wes: ParticipantId,
    ids: Vec<RoomSongId>,
}

fn room(charts: Charts) -> Starts {
    let mut rules = MatchRules::hosted();
    rules.stages = 2;
    rules.charts = charts;
    let mut policy = RoomPolicy::default().with_live().with_audience();
    policy.readiness.window_ms = 15_000;
    let mut r = Room::open("host", "Host", policy, rules, ContentSource::Shared);
    let host = r.host;
    let ana = r.join_as("ana", Intent::Play, false).unwrap();
    let bob = r.join_as("bob", Intent::Play, false).unwrap();
    let wes = r.join_as("wes", Intent::Watch, false).unwrap();
    let mut ids = Vec::new();
    for n in 1..=2 {
        r.by(host, Command::AddSong { song: Box::new(new_song(n)) }).unwrap();
        let id = r.s.songs.by_key(&new_song(n).key).unwrap().id;
        r.sys(Command::SetSongContent {
            song: id,
            status: ContentStatus::Available,
            share: Some(format!("share{:03}", id.0)),
            charts: None,
        })
        .unwrap();
        ids.push(id);
    }
    Starts { r, ana, bob, wes, ids }
}

impl Starts {
    /// The host picks song `n`'s Hard single chart.
    fn pick(&mut self, n: u32) -> StageId {
        let host = self.r.host;
        let song = self.ids[(n - 1) as usize];
        self.r
            .by(host, Command::PickListed { song, chart: chart(n, 2) })
            .unwrap();
        self.r.s.upcoming_stage().unwrap()
    }

    fn start(&mut self, who: ParticipantId, stage: StageId, chart: Option<ChartKey>) -> Result<Vec<Event>, Rejection> {
        let readiness_rev = self.r.rev(stage);
        self.r.by(
            who,
            Command::SetStart {
                stage,
                readiness_rev,
                chart,
            },
        )
    }

    fn start_of(&self, stage: StageId, who: ParticipantId) -> Option<ChartKey> {
        self.r.s.start_of(stage, who).cloned()
    }
}

/// Chosen, changed, defaulted: Ana starts on Easy, then Medium; the
/// stage's own chart named is no start of her own; none takes it back.
/// Bob, who chose none, starts on the stage's own chart.
#[test]
fn a_player_chooses_their_own_start_and_none_is_the_stages_chart() {
    let mut s = room(Charts::Adaptive);
    let (ana, bob) = (s.ana, s.bob);
    let stage = s.pick(1);
    let events = s.start(ana, stage, Some(chart(1, 0))).unwrap();
    assert_eq!(
        events,
        vec![Event::StartChosen {
            stage,
            participant: ana,
            chart: Some(chart(1, 0)),
        }]
    );
    assert_eq!(s.start_of(stage, ana), Some(chart(1, 0)));
    assert_eq!(s.start_of(stage, bob), None, "none chosen: the stage's chart");
    // The same again changes nothing.
    assert!(s.start(ana, stage, Some(chart(1, 0))).unwrap().is_empty());
    s.start(ana, stage, Some(chart(1, 1))).unwrap();
    assert_eq!(s.start_of(stage, ana), Some(chart(1, 1)));
    // The stage's own chart named is kept as none.
    s.start(ana, stage, Some(chart(1, 2))).unwrap();
    assert_eq!(s.r.s.stage(stage).unwrap().prep(ana).start, None);
    assert_eq!(s.start_of(stage, ana), None);
    s.start(ana, stage, Some(chart(1, 0))).unwrap();
    s.start(ana, stage, None).unwrap();
    assert_eq!(s.start_of(stage, ana), None);
    // Readiness is nobody else's: a start changes no one's.
    s.r.load_for(bob, stage, Loading::Loaded).unwrap();
    s.r.ready_for(bob, stage, true).unwrap();
    s.start(ana, stage, Some(chart(1, 0))).unwrap();
    assert!(s.r.s.stage(stage).unwrap().prep(bob).ready);
}

/// What may not be a start: a chart not of the stage's song, one of
/// another style (the ladder is the stage chart's style's), a stage
/// that is not the one gathering readiness, a stale readiness, a watcher.
#[test]
fn a_start_is_a_chart_of_the_stages_song_in_its_style() {
    let mut s = room(Charts::Adaptive);
    let (ana, wes) = (s.ana, s.wes);
    let stage = s.pick(1);
    assert_eq!(s.start(ana, stage, Some(chart(2, 0))), Err(Rejection::ChartNotInSong));
    assert_eq!(s.start(ana, stage, Some(chart(1, 3))), Err(Rejection::ChartNotInSong), "a double chart of a single stage");
    assert_eq!(s.start(wes, stage, Some(chart(1, 0))), Err(Rejection::NotPlaying));
    assert_eq!(
        s.r.by(
            ana,
            Command::SetStart {
                stage,
                readiness_rev: s.r.rev(stage) + 7,
                chart: Some(chart(1, 0)),
            }
        ),
        Err(Rejection::StaleReadiness { current: s.r.rev(stage) })
    );
    assert_eq!(
        s.r.by(
            ana,
            Command::SetStart {
                stage: StageId(99),
                readiness_rev: 0,
                chart: Some(chart(1, 0)),
            }
        ),
        Err(Rejection::UnknownStage)
    );
}

/// Locked at the start: once the stage's start commits a start is
/// refused, and every device reads the one each starter had.
#[test]
fn a_start_is_locked_once_the_stage_starts() {
    let mut s = room(Charts::Adaptive);
    let (host, ana, bob) = (s.r.host, s.ana, s.bob);
    let stage = s.pick(1);
    s.start(ana, stage, Some(chart(1, 0))).unwrap();
    s.start(bob, stage, Some(chart(1, 1))).unwrap();
    // Round LO: before the start the audience is told them as chosen (its
    // page shows each player's chart, as the members' Players pop-up
    // does); Bob's taken back and chosen again changes it there too.
    let stage_view = |s: &Starts| {
        audience::PublicView::of(&s.r.s, "Room")
            .stages
            .into_iter()
            .find(|p| p.id == stage)
            .unwrap()
    };
    assert_eq!(stage_view(&s).starts, vec![(ana, chart(1, 0)), (bob, chart(1, 1))]);
    assert!(stage_view(&s).play.is_none());
    s.start(bob, stage, None).unwrap();
    assert_eq!(stage_view(&s).starts, vec![(ana, chart(1, 0))]);
    s.start(bob, stage, Some(chart(1, 1))).unwrap();
    for who in [host, ana, bob] {
        s.r.load_for(who, stage, Loading::Loaded).unwrap();
        s.r.ready_for(who, stage, true).unwrap();
    }
    assert!(matches!(s.r.s.phase, Phase::Countdown { .. }), "{:?}", s.r.s.phase);
    // Committed: the play's, locked; the stage's own list empty.
    assert!(stage_view(&s).starts.is_empty());
    assert_eq!(s.start(ana, stage, Some(chart(1, 1))), Err(Rejection::StartCommitted));
    assert_eq!(s.start(ana, stage, None), Err(Rejection::StartCommitted));
    assert_eq!(s.start_of(stage, ana), Some(chart(1, 0)));
    assert_eq!(s.start_of(stage, bob), Some(chart(1, 1)));
    assert_eq!(s.start_of(stage, host), None);
    // The audience is told the starters' own charts, the host's none.
    let view = audience::PublicView::of(&s.r.s, "Room");
    let play = view
        .stages
        .iter()
        .find(|p| p.id == stage)
        .and_then(|p| p.play.as_ref())
        .unwrap();
    assert_eq!(play.starts, vec![(ana, chart(1, 0)), (bob, chart(1, 1))]);
}

/// A late entrant (counted out of the start, entering within the late
/// join) chooses their start until they enter; then it is locked too.
#[test]
fn a_late_entrant_chooses_their_start_until_they_enter() {
    let mut s = room(Charts::Adaptive);
    let (host, ana, bob) = (s.r.host, s.ana, s.bob);
    let stage = s.pick(1);
    for who in [host, bob] {
        s.r.load_for(who, stage, Loading::Loaded).unwrap();
        s.r.ready_for(who, stage, true).unwrap();
    }
    s.r.load_for(ana, stage, Loading::Loaded).unwrap();
    for _ in 0..400 {
        if matches!(s.r.s.phase, Phase::Countdown { .. }) {
            break;
        }
        let next = s.r.now + 100;
        s.r.wait_to(next);
    }
    assert!(matches!(s.r.s.phase, Phase::Countdown { .. }), "{:?}", s.r.s.phase);
    assert!(!s.r.s.stage(stage).unwrap().play.as_ref().unwrap().lineup.contains(&ana));
    // Bob, a starter, may not; Ana, counted out, may.
    assert_eq!(s.start(bob, stage, Some(chart(1, 0))), Err(Rejection::StartCommitted));
    s.start(ana, stage, Some(chart(1, 0))).unwrap();
    assert_eq!(s.start_of(stage, ana), Some(chart(1, 0)));
    s.r.ready_for(ana, stage, true).unwrap();
    assert!(s.r.s.stage(stage).unwrap().play.as_ref().unwrap().lineup.contains(&ana));
    assert_eq!(s.start(ana, stage, Some(chart(1, 1))), Err(Rejection::StartCommitted));
    assert_eq!(s.start_of(stage, ana), Some(chart(1, 0)));
}

/// Under As picked there is one chart for everyone: a start is refused,
/// and one kept from before a change of the rule to As picked is read as
/// none.
#[test]
fn as_picked_has_no_starts_of_ones_own() {
    let mut s = room(Charts::AsPicked);
    let ana = s.ana;
    let stage = s.pick(1);
    assert_eq!(s.start(ana, stage, Some(chart(1, 0))), Err(Rejection::ChartsAsPicked));
    assert_eq!(s.start(ana, stage, None), Err(Rejection::ChartsAsPicked));
    assert_eq!(s.start_of(stage, ana), None);

    // Adaptive for stage 1, a start chosen; As picked from stage 2.
    let mut s = room(Charts::Adaptive);
    let (host, ana) = (s.r.host, s.ana);
    let one = s.pick(1);
    s.start(ana, one, Some(chart(1, 0))).unwrap();
    let from = s.r.s.rules_land_on().unwrap();
    assert_ne!(from, one);
    let m = s.r.s.current.as_ref().unwrap();
    let rules = PlayRules {
        charts: Charts::AsPicked,
        ..m.play_rules_for(from)
    };
    s.r.by(host, Command::ChangeStageRules { from, rules }).unwrap();
    assert_eq!(s.start_of(one, ana), Some(chart(1, 0)), "stage 1 is still Adaptive");
    assert_eq!(s.r.s.charts_for(from), Charts::AsPicked);
}

/// A new pick for the stage (its song replaced) takes every start with
/// it: they named the old song's charts.
#[test]
fn a_new_pick_takes_the_starts_with_it() {
    let mut s = room(Charts::Adaptive);
    let (host, ana) = (s.r.host, s.ana);
    let stage = s.pick(1);
    s.start(ana, stage, Some(chart(1, 0))).unwrap();
    s.r.by(host, Command::CancelStart { stage }).unwrap();
    let song = s.ids[1];
    s.r.by(host, Command::PickListed { song, chart: chart(2, 2) }).unwrap();
    let again = s.r.s.upcoming_stage().unwrap();
    assert_eq!(again, stage);
    assert_eq!(s.start_of(stage, ana), None);
    s.start(ana, stage, Some(chart(2, 1))).unwrap();
    assert_eq!(s.start_of(stage, ana), Some(chart(2, 1)));
}

/// A room stored by revision 22 (`room-revision-22.json`, written by
/// revision 22's own reducer: a match under Adaptive between stages, a
/// list, Song choice: Group with a standing choice) reads with no starts,
/// writes back what it read, plays as it did and takes starts from its
/// next stage.
#[test]
fn a_room_stored_by_revision_22_reads_with_no_starts() {
    let text = include_str!("room-revision-22.json");
    let mut state: RoomState = serde_json::from_str(text).unwrap();
    let back = serde_json::to_value(&state).unwrap();
    assert_eq!(back, serde_json::from_str::<serde_json::Value>(text).unwrap(), "written as it was read");
    let m = state.current.as_ref().unwrap();
    let played = m.stages[0].id;
    for id in m.roster.keys() {
        assert_eq!(state.start_of(played, *id), None);
    }
    assert!(m.stages.iter().all(|s| s.prep.values().all(|p| p.start.is_none())));
    let next = state.upcoming_stage().unwrap();
    assert_eq!(state.charts_for(next), Charts::Adaptive);
    // The host's choice opens the draw as before; drawn, a start is taken.
    let host = ParticipantId(1);
    let song = state.songs.entries[0].id;
    let chart = state.songs.entries[0].charts[1].chart.clone();
    state
        .apply(
            Actor::Member(host),
            Some(state.revision),
            &Command::Choose { song: Some(song), chart: Some(chart) },
            Millis(400_000),
        )
        .unwrap();
    let ends = state.draw_ends_at().expect("a countdown");
    state.advance(ends);
    state.apply(Actor::Authority, None, &Command::Draw { roll: 0 }, ends).unwrap();
    let stage = state.upcoming_stage().unwrap();
    let rev = state.stage(stage).unwrap().readiness_rev;
    let entry = state.stage(stage).unwrap().content.as_ref().unwrap().song.unwrap();
    let easiest = state.songs.get(entry).unwrap().charts[0].chart.clone();
    let ana = state.member_by_identity(&Identity::new("twitch:ana")).unwrap().id;
    state
        .apply(
            Actor::Member(ana),
            Some(state.revision),
            &Command::SetStart { stage, readiness_rev: rev, chart: Some(easiest.clone()) },
            ends,
        )
        .unwrap();
    assert_eq!(state.start_of(stage, ana), Some(&easiest));
}

/// A snapshot's step status (`live::StepStatus`) of a competitor who
/// started on a rung of their own: their first step is from it, never
/// from the stage's chart.
#[test]
fn a_first_step_is_from_ones_own_start() {
    use ddi_room::live::{Moment, MomentKind, StepStatus};
    let step = |rung: u8, at_ms: i32| Moment {
        at_ms,
        tick: i64::from(at_ms),
        lane: 0,
        kind: MomentKind::Step { rung },
        life_ppm: 500_000,
        combo: 0,
        score: 0,
    };
    let s = StepStatus::after_from(None, 2, &[step(3, 10_000)]).unwrap();
    assert_eq!((s.rung, s.from), (3, 2));
    let s = StepStatus::after_from(Some(s), 2, &[step(0, 20_000)]).unwrap();
    assert_eq!((s.rung, s.from), (0, 3));
    // From the stage's chart, as before.
    let s = StepStatus::after(None, &[step(1, 10_000)]).unwrap();
    assert_eq!((s.rung, s.from), (1, 0));
}
