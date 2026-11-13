//! An online room's live play from the client's side
//! (`docs/plans/multiplayer-online-play.md`), over the fake room
//! (`ddi_room::fake`): the live link on only where the room's policy
//! plays live, the room's clock, the battle plan offered by the picker and
//! compared by everyone, readiness to play live said before Loaded.

use super::tests::room::{client, person, tick};
use crate::state::room::RoomClient;
use ddi_room::fake::{FakeServer, SharedFake};
use ddi_room::{Clock, Identity, Intent, LinkState, ManualClock, RoomPolicy};

pub(crate) fn room_with(clock: &ManualClock, others: &[&str], policy: RoomPolicy) -> SharedFake {
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        policy,
        clock.clone(),
    )
    .shared();
    for who in others {
        server
            .borrow_mut()
            .join(Identity::new(format!("dev:{who}")), who, Intent::Play)
            .unwrap();
    }
    server
}

/// A room that does not play live (an older server) is never sent a live
/// message: the socket stays open (a live frame there closes it with
/// 4400) and the client says it does not play live. One that does says
/// hello, and the room's clock is known within a few pings.
#[test]
fn the_live_link_is_on_only_where_the_room_plays_live() {
    let clock = ManualClock::new(5_000);
    let old = room_with(&clock, &["bob"], RoomPolicy::default());
    let mut alice = client(&old, person(&old, "alice"), "alice", 5_000);
    let mut bob = client(&old, person(&old, "bob"), "bob", 5_000);
    tick(&clock, &mut [&mut alice, &mut bob], 2_000);
    for r in [&alice, &bob] {
        assert_eq!(r.link_state(), LinkState::Connected);
        assert!(!r.plays_live());
        assert!(!r.live_link().unwrap().is_on());
        assert_eq!(r.room_now(), None);
    }
    assert_eq!(old.borrow().sockets(), 2, "nobody closed with 4400");

    let clock = ManualClock::new(5_000);
    let live = room_with(&clock, &["bob"], RoomPolicy::default().with_live());
    let mut alice = client(&live, person(&live, "alice"), "alice", 5_000);
    let mut bob = client(&live, person(&live, "bob"), "bob", 5_000);
    // Ticked as a frame loop does (every 10 ms): a ping's answer is read
    // on the next tick.
    for _ in 0..50 {
        tick(&clock, &mut [&mut alice, &mut bob], 40);
    }
    for r in [&alice, &bob] {
        assert_eq!(r.link_state(), LinkState::Connected);
        assert!(r.plays_live());
        assert!(r.live_link().unwrap().is_on());
        let now = r.room_now().expect("the room's clock").0;
        assert!(now.abs_diff(clock.now().0) <= 5, "{now} {}", clock.now().0);
    }
    // Practice never plays live.
    assert!(!RoomClient::new(5_000, 1).plays_live());
}

/// The bundled Beatucada and its first chart of four lanes, as read.
pub(crate) fn beatucada() -> (ddi_chart::Song, usize) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs/beatucada");
    let path = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "ssc"))
        .unwrap();
    let song =
        ddi_library::scan::parse_simfile(&std::fs::read_to_string(path).unwrap(), "ssc").unwrap();
    let chart = (0..song.charts.len())
        .find(|&i| {
            song.layout_of(&song.charts[i])
                .is_some_and(|l| l.lanes.len() == 4 && l.players == 1)
        })
        .unwrap();
    (song, chart)
}

/// A library entry naming `song`'s `chart` by its digest.
pub(crate) fn entry_of(song: &ddi_chart::Song, chart: usize) -> crate::screens::data::SongEntry {
    let mut s = super::tests::room::song();
    s.charts[0].digest = Some(song.chart_digest(chart));
    s.charts[0].index = chart;
    s
}

/// A live room with attacks: the picker's device offers the battle plan
/// it worked out, the room keeps its digest, every device that worked out
/// the same says it is ready to play live (its horizon and the digest)
/// before its Loaded; one that worked out another reports the mismatch
/// and sits the stage out, said in a plain sentence. The plan is the same
/// on every device from the same chart and seed.
#[test]
fn the_picker_offers_the_plan_and_every_device_compares_it_before_loaded() {
    use crate::state::room::{LoadProblem, Picked, prepare_stage};
    use ddi_room::{Command, ContentStatus, Loading};
    let clock = ManualClock::new(1_000);
    let server = room_with(&clock, &["bob", "carol"], RoomPolicy::default().with_live());
    let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
    let mut carol = client(&server, person(&server, "carol"), "carol", 1_000);
    let mut all = [&mut alice, &mut bob, &mut carol];
    for _ in 0..20 {
        tick(&clock, &mut all, 40);
    }
    // Attacks are offered in a room that plays live, and set.
    assert!(all[0].attacks_offered() && all[1].attacks_offered());
    let mut rules = all[0].rules().clone();
    rules.set_attacks(true);
    all[0].save_rules(rules).unwrap();
    tick(&clock, &mut all, 400);
    assert!(all[1].attacks_on());
    let (song, chart) = beatucada();
    let s = entry_of(&song, chart);
    all[0].pick(&s, &s.charts[0]).unwrap();
    tick(&clock, &mut all, 400);
    let stage = all[0].stages()[0].id;
    // The share, as the server makes it available.
    for j in all[0].take_upload_jobs() {
        server
            .borrow_mut()
            .authority(Command::SetContentStatus {
                stage: j.stage,
                status: ContentStatus::Available,
                share: Some("abc12345".into()),
            })
            .unwrap();
        all[0].upload_finished(j.stage, Ok("abc12345".into()));
    }
    tick(&clock, &mut all, 400);
    let jobs: Vec<_> = all.iter_mut().map(|r| r.take_load_jobs()).collect();
    let asks: Vec<_> = jobs
        .iter()
        .map(|j| j[0].live.clone().expect("a live ask"))
        .collect();
    assert!(asks.iter().all(|a| a.attacks && a.seed == asks[0].seed));
    // Each works the plan out as a shell does; Carol's differs (another
    // seed: another version of the game, say).
    let prepared: Vec<_> = jobs
        .iter()
        .map(|j| prepare_stage(&j[0], &song, &mut |_| {}).unwrap())
        .collect();
    assert_eq!(prepared[0], prepared[1], "the same plan everywhere");
    let plan = prepared[0].live.clone().unwrap();
    assert!(plan.horizon_us > 0);
    let digest = plan.plan.as_ref().unwrap().digest.clone();
    let mut other = plan.clone();
    other.plan = Some(crate::state::battle_plan::plan(
        &song,
        chart,
        &asks[2].rules,
        asks[2].seed ^ 1,
    ));
    let picked = |id: &str| Picked {
        song: crate::screens::data::SongEntry {
            id: id.into(),
            ..s.clone()
        },
        chart: s.charts[0].clone(),
    };
    // Bob and Carol before the picker: they wait for the room's plan.
    all[1].finish_shared_load_live(&jobs[1][0], Ok((picked("shared:abc12345:0"), Some(plan.clone()))));
    all[2].finish_shared_load_live(&jobs[2][0], Ok((picked("shared:abc12345:0"), Some(other))));
    tick(&clock, &mut all, 400);
    let loading = |r: &RoomClient| r.state().stage(stage).unwrap().prep(r.me()).loading.clone();
    assert!(matches!(loading(all[1]), Loading::Loading { .. }), "waits for the plan");
    assert_eq!(all[1].state().stage(stage).unwrap().content.as_ref().unwrap().battle, None);
    all[0].finish_load_live(&jobs[0][0], Ok(Some(plan.clone())));
    for _ in 0..4 {
        tick(&clock, &mut all, 200);
    }
    let room = all[0].state().stage(stage).unwrap().clone();
    assert_eq!(room.content.as_ref().unwrap().battle.as_ref(), Some(&digest));
    assert_eq!(loading(all[0]), Loading::Loaded);
    assert_eq!(loading(all[1]), Loading::Loaded);
    assert_eq!(all[2].load_problem(stage), Some(LoadProblem::PlanMismatch));
    // The room heard Alice and Bob say they play live, with the digest.
    let durable = server.borrow().live().unwrap().durable();
    let said = &durable.prepared[&stage];
    assert_eq!(said.len(), 2);
    assert!(said.iter().all(|(_, p)| p.plan.as_ref() == Some(&digest)
        && p.horizon_us == plan.horizon_us));
    assert_eq!(all[1].live_prep(stage), Some(&plan));
    // The stage's order carries it, and the others of the stage.
    let order = all[1].take_stage_order().expect("Bob's prompt");
    assert_eq!(order.battle, ddi_room::BattlePreset::GaugeAttacksV0);
    let live = order.live.expect("played live");
    assert_eq!(live.prep, Some(plan));
    assert_eq!(live.people.len(), 2);
    assert!(all[2].take_stage_order().is_none(), "Carol sits it out");
}
