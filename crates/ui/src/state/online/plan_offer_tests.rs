//! Room model revision 24 (round R24): a stage's battle plan is offered by
//! any device that prepared it once the room still has none
//! (`ddi_room::live::plan::PLAN_OFFERS_V1`), not by the picker's alone,
//! over the fake room: the picker away at the pick, and the picker's load
//! failing; the stage gathers everyone's Loaded on the plan another
//! device offered, as soon as the offers open to everyone.

use super::live_tests::{beatucada, entry_of, room_with};
use super::tests::room::{client, person, tick};
use crate::state::room::{LoadProblem, Picked, RoomClient, prepare_stage};
use ddi_room::live::plan::PLAN_OFFERS_V1;
use ddi_room::{Command, ContentStatus, Loading, ManualClock, RoomPolicy, StageId};

/// Alice (the host and picker), Bob and Carol in a live room with
/// attacks; Alice picks Beatucada and its share is made available. The
/// load jobs each device was handed, and the stage.
struct Picked3 {
    clock: ManualClock,
    server: ddi_room::fake::SharedFake,
    rooms: Vec<RoomClient>,
    jobs: Vec<Vec<crate::state::room::LoadJob>>,
    stage: StageId,
}

fn picked() -> Picked3 {
    let clock = ManualClock::new(1_000);
    let server = room_with(&clock, &["bob", "carol"], RoomPolicy::default().with_live());
    let mut rooms = vec![
        client(&server, person(&server, "alice"), "alice", 1_000),
        client(&server, person(&server, "bob"), "bob", 1_000),
        client(&server, person(&server, "carol"), "carol", 1_000),
    ];
    let mut all: Vec<&mut RoomClient> = rooms.iter_mut().collect();
    for _ in 0..20 {
        tick(&clock, &mut all, 40);
    }
    let mut rules = all[0].rules().clone();
    rules.set_attacks(true);
    all[0].save_rules(rules).unwrap();
    tick(&clock, &mut all, 400);
    let (song, chart) = beatucada();
    let s = entry_of(&song, chart);
    all[0].pick(&s, &s.charts[0]).unwrap();
    tick(&clock, &mut all, 400);
    let stage = all[0].stages()[0].id;
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
    Picked3 { clock, server, rooms, jobs, stage }
}

impl Picked3 {
    /// Bob's and Carol's devices finish preparing the stage, as a shell
    /// does: the same plan on both.
    fn bob_and_carol_prepare(&mut self) {
        let (song, chart) = beatucada();
        let s = entry_of(&song, chart);
        let picked = Picked {
            song: crate::screens::data::SongEntry { id: "shared:abc12345:0".into(), ..s.clone() },
            chart: s.charts[0].clone(),
        };
        for i in [1, 2] {
            let prep = prepare_stage(&self.jobs[i][0], &song, &mut |_| {}).unwrap();
            let live = prep.live.clone();
            self.rooms[i].finish_shared_load_live(&self.jobs[i][0], Ok((picked.clone(), live)));
        }
    }

    fn loading(&self, i: usize) -> Loading {
        let r = &self.rooms[i];
        r.state().stage(self.stage).unwrap().prep(r.me()).loading.clone()
    }

    fn room_plan(&self) -> Option<ddi_room::live::PlanDigest> {
        self.server.borrow().state().stage(self.stage)?.content.as_ref()?.battle.clone()
    }

    /// `ms` pass, the devices in `who` ticked.
    fn run(&mut self, who: &[usize], ms: u64) {
        let mut all: Vec<&mut RoomClient> = self
            .rooms
            .iter_mut()
            .enumerate()
            .filter(|(i, _)| who.contains(i))
            .map(|(_, r)| r)
            .collect();
        for _ in 0..ms / 200 {
            tick(&self.clock, &mut all, 200);
        }
    }
}

/// The picker's device is gone at the pick (its socket dropped, nothing
/// of it ticked): Bob's and Carol's devices prepared the stage and wait
/// for the room's plan; before revision 24 nobody offered it and nobody's
/// Loaded was sent until the window ran out. Now, once the offers open
/// to everyone (`PLAN_OFFERS_V1.wait_ms` after the window opened), one of
/// them offers it, the room takes it and both are Loaded.
#[test]
fn a_picker_away_at_the_pick_holds_nobodys_loaded_up() {
    let mut p = picked();
    let alice = p.rooms[0].me();
    p.server.borrow_mut().drop_socket(alice);
    p.bob_and_carol_prepare();
    p.run(&[1, 2], 2_000);
    assert_eq!(p.room_plan(), None, "nobody offers before the wait");
    assert!(matches!(p.loading(1), Loading::Loading { .. }));
    p.run(&[1, 2], PLAN_OFFERS_V1.wait_ms);
    assert!(p.room_plan().is_some(), "a prepared device offered the plan");
    assert_eq!(p.loading(1), Loading::Loaded);
    assert_eq!(p.loading(2), Loading::Loaded);
    assert_eq!(p.rooms[1].load_problem(p.stage), None);
}

/// The picker's song fails to load (a download that failed): the others'
/// devices offer the plan once the offers open to everyone, and play.
#[test]
fn a_pickers_failed_load_holds_nobodys_loaded_up() {
    let mut p = picked();
    let job = p.jobs[0][0].clone();
    p.rooms[0].finish_load_live(&job, Err(LoadProblem::Download));
    p.bob_and_carol_prepare();
    p.run(&[0, 1, 2], 2_000);
    assert_eq!(p.room_plan(), None);
    assert!(matches!(p.loading(0), Loading::Failed { .. }));
    p.run(&[0, 1, 2], PLAN_OFFERS_V1.wait_ms);
    assert!(p.room_plan().is_some());
    assert_eq!(p.loading(1), Loading::Loaded);
    assert_eq!(p.loading(2), Loading::Loaded);
}

/// A person's offer before the offers open is refused as too soon and
/// offered again once they are: the room's live side says so, and the
/// picker's own offer is taken at once.
#[test]
fn an_offer_too_soon_is_refused_and_the_pickers_taken_at_once() {
    use ddi_room::live::{ClientLive, LiveRefusal, LiveReply, ServerLive};
    let p = picked();
    let (song, _) = beatucada();
    let plan = prepare_stage(&p.jobs[1][0], &song, &mut |_| {}).unwrap().live.unwrap().plan.unwrap();
    let bob = p.rooms[1].me();
    let alice = p.rooms[0].me();
    let now = ddi_room::Millis(ddi_room::Clock::now(&p.clock).0);
    let state = p.server.borrow().state().clone();
    let mut live = ddi_room::live::LiveServer::new(ddi_room::live::LIVE_V2);
    let offer = ClientLive::OfferPlan { stage: p.stage, plan: Box::new(plan.clone()) };
    let out = live.receive(&state, bob, offer.clone(), now);
    assert_eq!(
        out.reply,
        vec![LiveReply::Message(ServerLive::Refused { refusal: LiveRefusal::OfferTooSoon { stage: p.stage } })]
    );
    assert!(!state.plan_offers_open(p.stage, now));
    let later = now.after(PLAN_OFFERS_V1.wait_ms);
    assert!(state.plan_offers_open(p.stage, later));
    let out = live.receive(&state, bob, offer.clone(), later);
    assert!(out.reply.is_empty(), "{:?}", out.reply);
    assert_eq!(out.commands.len(), 1, "the plan set");
    // The picker's, at once.
    let mut live = ddi_room::live::LiveServer::new(ddi_room::live::LIVE_V2);
    let out = live.receive(&state, alice, offer, now);
    assert!(out.reply.is_empty() && out.commands.len() == 1);
}
