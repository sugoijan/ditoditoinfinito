//! Where a room's song select comes back after each action
//! (`docs/plans/song-navigation.md`, "In a room"; feedback round E2): the
//! picker's own list as they left it (the song, the chart, the style, the
//! open pack, the search) across the pop-ups, a rebuild, a resize, a
//! stage and a change of hands; the song followed is nobody's place.

use std::cell::RefCell;
use std::rc::Rc;

use super::data::{ChartEntry, Difficulty, SelectData, SongEntry};
use super::room::{RoomCall, RoomShell};
use super::select::{SelectCall, SelectScreen};
use super::{DESIGN, Env, sample};
use crate::look::Plain;
use crate::state::room::{RoomClient, SharedRoom, StageOrder};
use crate::testing::Harness;
use crate::{PointerKind, TextMode};

fn env() -> Env {
    Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    )
}

fn harness() -> Harness {
    Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own)
}

/// Three packs of six songs, each with Easy, Medium and Hard Single
/// charts and a Double chart.
fn packs() -> SelectData {
    let mut songs = Vec::new();
    for (p, pack) in [(0, None), (1, Some("Pack A")), (2, Some("Pack B"))] {
        for k in 0..6usize {
            let id = format!("p{p}s{k}");
            let charts = [
                ("dance-single", Difficulty::Easy, 3),
                ("dance-single", Difficulty::Medium, 6),
                ("dance-single", Difficulty::Hard, 9),
                ("dance-double", Difficulty::Hard, 10),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, (style, difficulty, meter))| ChartEntry {
                style: style.into(),
                difficulty,
                meter,
                notes: 300,
                index: i,
                digest: Some(ddi_chart::ChartDigest(
                    0x7000 + (p * 100 + k * 10 + i) as u128,
                )),
                ..ChartEntry::default()
            })
            .collect();
            songs.push(SongEntry {
                id: id.clone(),
                title: format!("Song {id}"),
                artist: "Artist".into(),
                bpm_text: "150".into(),
                length: 120.0,
                pack: pack.map(String::from),
                charts,
                ..SongEntry::default()
            });
        }
    }
    SelectData {
        styles: sample::styles(),
        songs,
        style: Some("dance-single".into()),
        ..SelectData::default()
    }
}

type Calls = Rc<RefCell<Vec<SelectCall>>>;
type Orders = Rc<RefCell<Vec<StageOrder>>>;

fn room_shell(room: &SharedRoom, orders: &Orders) -> RoomShell {
    let shared = room.clone();
    let orders = orders.clone();
    Rc::new(move |call, cx| match call {
        RoomCall::Leave | RoomCall::Discard => cx.close(true),
        RoomCall::Prepare(job) => shared.borrow_mut().finish_load(&job, Ok(())),
        RoomCall::PlayStage(order) => orders.borrow_mut().push(*order),
        _ => {}
    })
}

fn select(env: &Env, room: &SharedRoom, data: SelectData, orders: &Orders, calls: &Calls) -> SelectScreen {
    let c = calls.clone();
    sample::room_select(env, room.clone(), data)
        .with_shell(
            Rc::new(move |call, _cx| c.borrow_mut().push(call)),
            Default::default(),
        )
        .with_room(room.clone(), room_shell(room, orders))
}

/// What the room's song select shows of its place: the song, the chart,
/// the style, the packs closed, the search, the rows.
type Shown = (String, Option<String>, String, Vec<String>, String, usize);

fn shown(env: &Env) -> Shown {
    let s = env.state.borrow();
    (
        s.song_id.clone(),
        s.chart.clone(),
        s.style.clone(),
        s.closed.clone(),
        s.search.clone(),
        s.items,
    )
}

fn top(h: &Harness) -> String {
    h.ui.layers().last().map(|l| l.name.clone()).unwrap_or_default()
}

fn places(calls: &Calls) -> usize {
    calls
        .borrow()
        .iter()
        .filter(|c| matches!(c, SelectCall::Place(_)))
        .count()
}

/// Escape, then the pop-up's leaving.
fn close(h: &mut Harness) {
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(h), "select");
}

/// Practice's room keeps its place in one screen across everything that
/// covers it or builds it again: Rules (closed, and applied), Players,
/// More, a phone's size and back, a change of language's rebuild, a stage
/// picked from a search, its standings and Continue: the song picked
/// stays selected, with its chart and the search.
#[test]
fn a_practice_rooms_place_holds_through_pop_ups_a_rebuild_and_a_stage() {
    let env = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let calls = Calls::default();
    let mut data = packs();
    data.closed = vec![None, Some("Pack B".into())];
    data.resume = Some(("p1s4".into(), Some(1)));
    let mut h = harness();
    h.push(select(&env, &room, data, &orders, &calls));
    h.run(0.6);
    h.key("ArrowDown", "ArrowDown");
    h.run(0.8);
    let place = shown(&env);
    assert_eq!(place.0, "p1s5");
    assert_eq!(place.1.as_deref(), Some("Medium 6"));
    assert_eq!(place.3, ["Bundled songs", "Pack B"]);
    for name in ["Room.Rules", "KeyL", "More"] {
        if name == "KeyL" {
            // The players: L (a wide screen has no button for them).
            h.key("KeyL", "l");
        } else {
            h.tap(PointerKind::Mouse, name);
        }
        h.run(0.5);
        assert_ne!(top(&h), "select", "{name} opened");
        close(&mut h);
        assert_eq!(shown(&env), place, "after {name}");
    }
    // Rules applied (a stage more).
    h.tap(PointerKind::Mouse, "Room.Rules");
    h.run(0.5);
    h.tap(PointerKind::Mouse, "Inc.stages");
    h.run(0.2);
    h.tap(PointerKind::Mouse, "Apply");
    h.run(0.5);
    assert_eq!(top(&h), "select");
    assert_eq!(room.borrow().rules().stages, 5, "applied");
    assert_eq!(shown(&env), place, "after Rules applied");
    for size in [[844.0, 390.0], [1280.0, 720.0]] {
        h.send(crate::UiEvent::Resize { size, ratio: 1.0 });
        h.run(0.5);
        assert_eq!(shown(&env), place, "at {size:?}");
    }
    h.ui.rebuild_all();
    h.run(0.5);
    assert_eq!(shown(&env), place, "rebuilt");
    // A search, a song found, picked; the stage played and reported.
    h.key("Slash", "/");
    h.send(crate::UiEvent::Text(crate::event::TextEvent::Commit(
        "p2".into(),
    )));
    h.run(0.1);
    h.key("Enter", "Enter");
    h.run(0.5);
    h.key("ArrowDown", "ArrowDown");
    h.run(0.8);
    let picked = shown(&env);
    assert_eq!((picked.0.as_str(), picked.4.as_str()), ("p2s1", "p2"));
    h.key("Enter", "Enter");
    h.run(0.3);
    let order = orders.borrow_mut().pop().expect("a prompt");
    assert_eq!(order.song.id, "p2s1");
    room.borrow_mut().set_ready(true).unwrap();
    room.borrow_mut().tick(4_000);
    room.borrow_mut().tick(20_000);
    let mut ids: Vec<_> = order.cpus.iter().map(|c| c.participant).collect();
    ids.insert(0, order.me);
    let ranking = ids
        .into_iter()
        .map(|participant| {
            vec![ddi_room::Finisher {
                participant,
                finish: ddi_room::Finish::Survived,
            }]
        })
        .collect();
    room.borrow_mut()
        .report_stage(order.stage, ranking, false)
        .unwrap();
    h.run(0.5);
    assert_eq!(top(&h), "room-standings");
    h.tap(PointerKind::Mouse, "Continue");
    h.run(0.5);
    assert_eq!(top(&h), "select");
    assert_eq!(shown(&env), picked, "back from the stage");
    assert_eq!(h.ui.focused(), Some("Wheel"));
}

/// An online room's picker who hands the pick over and takes it back has
/// their list as they left it: the song, the chart, the style (Double,
/// which an online room without CPUs plays), the open pack. While they
/// follow, nothing of their list is told as the room's place, and a
/// rebuild shows no list.
#[test]
fn the_pickers_place_comes_back_whole_after_a_hand_over() {
    use super::room_tests::Three;
    let t = Three::new(true);
    let (ea, eb) = (env(), env());
    let orders = Orders::default();
    let (ca, cb) = (Calls::default(), Calls::default());
    let mut data = packs();
    data.closed = vec![None, Some("Pack A".into())];
    data.resume = Some(("p2s4".into(), Some(2)));
    let mut ha = harness();
    ha.push(select(&ea, &t.alice, data, &orders, &ca));
    let mut db = packs();
    db.closed = vec![None, Some("Pack B".into())];
    db.resume = Some(("p1s2".into(), Some(0)));
    let mut hb = harness();
    hb.push(select(&eb, &t.bob, db, &orders, &cb));
    t.settle(400, &mut [&mut ha, &mut hb]);
    ha.key("Tab", "Tab");
    ha.run(0.5);
    ha.key("ArrowDown", "ArrowDown");
    ha.run(0.8);
    let place = shown(&ea);
    assert_eq!(
        (place.0.as_str(), place.1.as_deref(), place.2.as_str()),
        ("p2s5", Some("Hard 10"), "Double")
    );
    // Bob takes the pick: his own list as he had it.
    hb.tap(PointerKind::Mouse, "Room.TakePick");
    t.settle(400, &mut [&mut ha, &mut hb]);
    t.settle(400, &mut [&mut ha, &mut hb]);
    assert_eq!(shown(&eb).0, "p1s2");
    assert_eq!(shown(&eb).1.as_deref(), Some("Easy 3"));
    let told = places(&ca);
    ha.ui.rebuild_all();
    ha.run(0.5);
    assert!(!ha.ui.actor("Wheel").is_some_and(|w| w.visible), "no list");
    assert_eq!(places(&ca), told, "the song followed is nobody's place");
    // Alice takes it back with T.
    ha.key("KeyT", "t");
    t.settle(400, &mut [&mut ha, &mut hb]);
    t.settle(400, &mut [&mut ha, &mut hb]);
    assert_eq!(shown(&ea), place, "as she left it");
    assert_eq!(ea.state.borrow().closed, ["Bundled songs", "Pack A"]);
}

/// A follower's screen built once the room is known (the web's room page
/// after a stage, a rebuild): the picker's song from its first frame,
/// never the follower's own songs told (no preview of theirs), no place.
#[test]
fn a_follower_built_in_a_known_room_never_shows_or_tells_their_own_list() {
    use super::room_tests::Three;
    let t = Three::new(false);
    let eb = env();
    let orders = Orders::default();
    let cb = Calls::default();
    t.step(400);
    t.step(400);
    let mut db = packs();
    db.resume = Some(("p1s2".into(), Some(0)));
    let mut hb = harness();
    hb.push(select(&eb, &t.bob, db, &orders, &cb));
    let mut seen_list = false;
    for _ in 0..20 {
        hb.run(0.05);
        seen_list |= hb.ui.actor("Wheel").is_some_and(|w| w.visible);
    }
    assert!(!seen_list, "the list never showed");
    let own = cb.borrow().iter().any(|c| {
        matches!(c, SelectCall::Selected { song: Some(s), .. } if s.starts_with('p'))
    });
    let said: Vec<String> = cb.borrow().iter().map(|c| format!("{c:?}").chars().take(80).collect()).collect();
    assert!(!own, "{said:?}");
    assert_eq!(places(&cb), 0);
}
