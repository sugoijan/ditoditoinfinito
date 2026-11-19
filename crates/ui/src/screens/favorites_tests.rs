//! Favorite songs on song select (`docs/plans/song-navigation.md`,
//! "Favorites"): the heart by a key, a click and More; the favorites' pack
//! at the top, its order, its place, its emptying; search; a share and a
//! room's follower without them; what is kept.

use std::cell::RefCell;
use std::rc::Rc;

use super::data::{ChartEntry, Difficulty, SelectData, SharedView, SongEntry};
use super::select::{
    FEED, Feed, OpenFn, PlayFn, SelectCall, SelectPlace, SelectScreen, SharedFeed, ShellFn,
};
use super::test_kit::{Orders, env, top};
use super::{DESIGN, Env, SharedTheme, sample};
use crate::PointerKind;
use crate::TextMode;
use crate::state::favorites;
use crate::state::place::FAVORITES_KEY;
use crate::testing::Harness;

type Calls = Rc<RefCell<Vec<SelectCall>>>;

/// Three packs of six songs (the bundled ones, Pack A, Pack B), each with
/// a Single and a Double chart; their titles out of their ids' order.
fn packs() -> SelectData {
    let mut songs = Vec::new();
    for (p, pack) in [(0, None), (1, Some("Pack A")), (2, Some("Pack B"))] {
        for k in 0..6usize {
            let id = format!("p{p}s{k}");
            let charts = [
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
                ..ChartEntry::default()
            })
            .collect();
            songs.push(SongEntry {
                id: id.clone(),
                // Pack B's titles sort before the others', backwards
                // ("A0 song" its last).
                title: if p == 2 {
                    format!("A{} song", 5 - k)
                } else {
                    format!("Song {id}")
                },
                artist: "Artist".into(),
                bpm_text: "150".into(),
                length: 120.0,
                pack: pack.map(String::from),
                removable: pack.is_some(),
                charts,
                ..SongEntry::default()
            });
        }
    }
    SelectData {
        styles: sample::styles(),
        songs,
        style: Some("dance-single".into()),
        closed: vec![Some("Pack A".into()), Some("Pack B".into())],
        ..SelectData::default()
    }
}

/// Song select over `data` with a shell that keeps what it is told, at the
/// design's size or `size`.
fn shelled(data: SelectData, size: [f32; 2]) -> (Harness, SharedTheme, Calls, SharedFeed) {
    let env = env();
    let state = env.state.clone();
    let design = if size[1] < 500.0 {
        super::COMPACT
    } else {
        DESIGN
    };
    let mut h = Harness::new(design, size, 1.0, TextMode::Mirror);
    let play: PlayFn = Rc::new(|_, _, _: &Env, _| {});
    let open: OpenFn = Rc::new(|_, _| {});
    let calls: Calls = Rc::default();
    let c = calls.clone();
    let shell: ShellFn = Rc::new(move |call, _| c.borrow_mut().push(call));
    let feed: SharedFeed = Rc::new(RefCell::new(Feed::default()));
    h.push(
        SelectScreen::new(env, data, None, play, open)
            .with_shell(shell, feed.clone())
            .with_sharing(),
    );
    h.run(0.5);
    (h, state, calls, feed)
}

fn last_place(calls: &Calls) -> SelectPlace {
    calls
        .borrow()
        .iter()
        .rev()
        .find_map(|c| match c {
            SelectCall::Place(p) => Some(p.clone()),
            _ => None,
        })
        .expect("a place")
}

/// One arrow, settled (past the double tap's window).
fn arrow(h: &mut Harness, key: &str) {
    h.key(key, key);
    h.run(0.45);
}

fn song(s: &SharedTheme) -> String {
    s.borrow().song_id.clone()
}

/// More closed, the focus back on the list (Down from the header's More).
fn close_more(h: &mut Harness) {
    h.key("Escape", "Escape");
    h.run(0.3);
    h.key("ArrowDown", "ArrowDown");
    h.run(0.3);
    assert_eq!(h.ui.focused(), Some("Wheel"));
}

/// Up to the favorites' header (the list's first row).
fn to_top(h: &mut Harness, s: &SharedTheme) {
    for _ in 0..40 {
        if s.borrow().header.as_deref() == Some("Favorites") {
            return;
        }
        h.key("ArrowUp", "ArrowUp");
        h.run(0.45);
    }
    panic!("no favorites' header");
}

/// The heart turned by F, a click on it and More's item, each said by its
/// state, its footer word and More's label; the songs in the favorites'
/// pack at the top, sorted by title; kept at once in its own key.
#[test]
fn a_song_is_made_a_favorite_by_a_key_a_click_and_more() {
    ddi_l10n::set_language("en");
    let (mut h, s, _, _) = shelled(packs(), [1280.0, 720.0]);
    assert_eq!(song(&s), "p0s0");
    assert_eq!(s.borrow().favorite, Some(false));
    assert!(
        s.borrow().hints.iter().any(|t| t == "F Favorite"),
        "{:?}",
        s.borrow().hints
    );
    assert!(s.borrow().favorites.is_empty());
    // F.
    h.key("KeyF", "f");
    h.run(0.3);
    assert_eq!(s.borrow().favorite, Some(true));
    assert!(s.borrow().hints.iter().any(|t| t == "F Unfavorite"));
    assert_eq!(s.borrow().favorites, ["p0s0"]);
    assert_eq!(favorites::all(), ["p0s0"]);
    // The selected row stays: the favorites' header came in above it.
    assert_eq!(song(&s), "p0s0");
    // A click on the heart.
    arrow(&mut h, "ArrowDown");
    assert_eq!(song(&s), "p0s1");
    h.tap(PointerKind::Mouse, "Favorite");
    h.run(0.3);
    assert_eq!(s.borrow().favorite, Some(true));
    assert_eq!(h.ui.focused(), Some("Wheel"), "the list keeps the focus");
    // More's item, as a controller reaches it.
    arrow(&mut h, "ArrowDown");
    h.tap(PointerKind::Mouse, "More");
    h.run(0.3);
    assert!(
        h.texts().iter().any(|t| t == "Add to favorites"),
        "{:?}",
        h.texts()
    );
    h.tap(PointerKind::Mouse, "Item.Favorite");
    h.run(0.4);
    assert_eq!(top(&h), "select");
    assert_eq!(s.borrow().favorite, Some(true));
    assert_eq!(favorites::all(), ["p0s0", "p0s1", "p0s2"]);
    h.tap(PointerKind::Mouse, "More");
    h.run(0.3);
    assert!(h.texts().iter().any(|t| t == "Remove from favorites"));
    close_more(&mut h);
    // A song of Pack B, whose title sorts first.
    for _ in 0..5 {
        arrow(&mut h, "ArrowDown");
    }
    assert_eq!(s.borrow().header.as_deref(), Some("Pack B"));
    h.key("Enter", "Enter");
    h.run(0.3);
    arrow(&mut h, "ArrowDown");
    assert_eq!(song(&s), "p2s0");
    h.key("KeyF", "f");
    h.run(0.3);
    assert_eq!(s.borrow().favorites, ["p2s0", "p0s0", "p0s1", "p0s2"]);
    // F again: off, and gone from the pack.
    h.key("KeyF", "f");
    h.run(0.3);
    assert_eq!(s.borrow().favorite, Some(false));
    assert_eq!(s.borrow().favorites, ["p0s0", "p0s1", "p0s2"]);
    assert_eq!(
        song(&s),
        "p2s0",
        "outside the favorites' pack the song stays"
    );
    assert_eq!(
        crate::state::place::pack_key(Some(crate::state::place::FAVORITES_PACK)),
        FAVORITES_KEY
    );
}

/// The favorites' pack is the list's first row, opens and closes as a
/// pack, keeps its place (a song opened from it comes back in it), offers
/// none of a pack's actions, and taken empty song by song gives the focus
/// to the next song, then its header, then the first pack's header.
#[test]
fn the_favorites_pack_opens_keeps_its_place_and_empties() {
    ddi_l10n::set_language("en");
    favorites::set_favorite("p1s3", true);
    favorites::set_favorite("p0s1", true);
    // One whose song is not in the library: kept, not shown.
    favorites::set_favorite("u-gone", true);
    let (mut h, s, calls, _) = shelled(packs(), [1280.0, 720.0]);
    assert_eq!(s.borrow().favorites, ["p0s1", "p1s3"]);
    to_top(&mut h, &s);
    assert_eq!(s.borrow().header.as_deref(), Some("Favorites"));
    // Nothing to manage or share on it.
    h.key("Delete", "Delete");
    h.run(0.3);
    assert_eq!(top(&h), "select", "no manage dialog");
    h.tap(PointerKind::Mouse, "More");
    h.run(0.3);
    let texts = h.texts();
    assert!(
        !texts
            .iter()
            .any(|t| t.starts_with("Share") || t.starts_with("Manage")),
        "{texts:?}"
    );
    close_more(&mut h);
    // Opened: its songs, the place's open pack its key.
    h.key("Enter", "Enter");
    h.run(0.3);
    arrow(&mut h, "ArrowDown");
    assert_eq!(song(&s), "p0s1");
    arrow(&mut h, "ArrowDown");
    assert_eq!(song(&s), "p1s3");
    let place = last_place(&calls);
    assert_eq!(place.open.as_deref(), Some(FAVORITES_KEY));
    assert_eq!(place.song.as_deref(), Some("p1s3"));
    // No closed pack is the favorites' (a real pack's name only).
    assert_eq!(
        place.closed,
        [None, Some("Pack A".into()), Some("Pack B".into())]
    );
    // The place comes back in it (as a new screen from the kept place).
    let mut kept = crate::state::place::MenuPlace::default();
    kept.remember(&place);
    let mut data = packs();
    data.closed = place.closed.clone();
    data.resume = Some(("p1s3".into(), None));
    data.resume_pack = kept.open.clone();
    let (mut h2, s2, calls2, _) = shelled(data, [1280.0, 720.0]);
    assert_eq!(song(&s2), "p1s3");
    h2.key("ArrowUp", "ArrowUp");
    h2.run(0.45);
    assert_eq!(song(&s2), "p0s1", "within the favorites' pack");
    assert_eq!(last_place(&calls2).open.as_deref(), Some(FAVORITES_KEY));
    // Emptied: the next song, then the header after the last.
    arrow(&mut h, "ArrowUp");
    assert_eq!(song(&s), "p0s1");
    h.key("KeyF", "f");
    h.run(0.3);
    assert_eq!(song(&s), "p1s3", "the next song");
    assert_eq!(s.borrow().favorites, ["p1s3"]);
    h.key("KeyF", "f");
    h.run(0.3);
    // The pack gone: the first pack's header, nothing open.
    assert!(s.borrow().favorites.is_empty());
    assert_eq!(s.borrow().header.as_deref(), Some("Bundled songs"));
    assert_eq!(last_place(&calls).open, None);
    assert_eq!(favorites::all(), ["u-gone"], "the missing song's kept");
}

/// The last song of the open favorites' pack taken out: its header.
#[test]
fn the_last_favorite_taken_out_leaves_the_focus_on_the_pack() {
    favorites::set_favorite("p0s1", true);
    favorites::set_favorite("p1s3", true);
    let (mut h, s, _, _) = shelled(packs(), [1280.0, 720.0]);
    to_top(&mut h, &s);
    h.key("Enter", "Enter");
    h.run(0.3);
    arrow(&mut h, "ArrowDown");
    arrow(&mut h, "ArrowDown");
    assert_eq!(song(&s), "p1s3");
    h.key("KeyF", "f");
    h.run(0.3);
    assert_eq!(s.borrow().header.as_deref(), Some("Favorites"));
    assert_eq!(s.borrow().favorites, ["p0s1"]);
}

/// While searching a song shows once, under its own pack, with its heart:
/// the list as without favorites; the search cleared, the favorites' pack
/// is back where it was.
#[test]
fn a_search_lists_each_song_once_as_without_favorites() {
    let search = |favorite: bool| {
        if favorite {
            favorites::set_favorite("p0s3", true);
            favorites::set_favorite("p2s1", true);
            favorites::set_favorite("p1s4", true);
        }
        let mut data = packs();
        data.search = "song".into();
        let (mut h, s, _, _) = shelled(data, [1280.0, 720.0]);
        h.run(0.3);
        let st = s.borrow();
        let seen = (st.items, st.matches, st.song_id.clone(), st.closed.clone());
        drop(st);
        let texts: Vec<String> = h
            .texts()
            .into_iter()
            .filter(|t| !t.starts_with("F "))
            .collect();
        let _ = &mut h;
        (seen, texts)
    };
    let without = search(false);
    // A fresh thread's store: none kept yet.
    let with = std::thread::spawn(move || search(true)).join().unwrap();
    assert_eq!(without, with);
    // 18 songs and their three headers; no favorites' header.
    assert_eq!(without.0.0, 21);
}

/// A library of fewer than ten songs has no packs, so no favorites' pack:
/// the heart and the rows' alone.
#[test]
fn a_small_library_has_hearts_without_a_pack() {
    let mut data = packs();
    data.songs.truncate(6);
    let (mut h, s, _, _) = shelled(data, [1280.0, 720.0]);
    h.key("KeyF", "f");
    h.run(0.3);
    assert_eq!(s.borrow().favorite, Some(true));
    assert!(s.borrow().favorites.is_empty());
    assert_eq!(s.borrow().items, 6);
}

/// A share's songs have no heart and no pack, whatever is kept.
#[test]
fn a_share_has_no_favorites() {
    favorites::set_favorite("p0s0", true);
    let mut data = packs();
    data.shared = Some(SharedView {
        pack: "A friend's pack".into(),
        expires: None,
    });
    let (mut h, s, _, _) = shelled(data, [1280.0, 720.0]);
    assert_eq!(s.borrow().favorite, None);
    assert!(s.borrow().favorites.is_empty());
    assert!(!s.borrow().hints.iter().any(|t| t.starts_with("F ")));
    h.key("KeyF", "f");
    h.run(0.3);
    assert_eq!(favorites::all(), ["p0s0"]);
    assert!(h.ui.actor("Favorite").is_none());
    h.tap(PointerKind::Mouse, "More");
    h.run(0.3);
    assert!(!h.texts().iter().any(|t| t.contains("favorites")));
}

/// The heart's key is the one that types F on the person's layout (round
/// AD): an event whose code is `KeyU` and key `f` turns it, one whose code
/// is `KeyF` and key `u` does not; Shift's F too, a modifier's not; while
/// the search is typed into, f types.
#[test]
fn the_hearts_key_is_the_typed_letter_f() {
    ddi_l10n::set_language("en");
    let (mut h, s, _, _) = shelled(packs(), [1280.0, 720.0]);
    assert_eq!(s.borrow().favorite, Some(false));
    h.key("KeyF", "u");
    h.run(0.3);
    assert_eq!(s.borrow().favorite, Some(false), "the physical KeyF typing u");
    h.key("KeyU", "f");
    h.run(0.3);
    assert_eq!(s.borrow().favorite, Some(true), "the key typing f");
    h.key("KeyU", "F");
    h.run(0.3);
    assert_eq!(s.borrow().favorite, Some(false), "Shift's F");
    h.key_with(
        "KeyF",
        "f",
        crate::event::Modifiers {
            ctrl: true,
            ..Default::default()
        },
    );
    h.run(0.3);
    assert_eq!(s.borrow().favorite, Some(false), "a modifier's: not the heart");
    // Typing: f is the field's.
    h.key("Slash", "/");
    h.run(0.3);
    h.key("KeyU", "f");
    h.run(0.3);
    assert_eq!(s.borrow().favorite, Some(false));
}

/// A favorite whose song is gone shows again with it (a library read
/// again); the list's rows follow.
#[test]
fn a_favorite_comes_back_with_its_song() {
    favorites::set_favorite("p2s2", true);
    let mut data = packs();
    data.songs.retain(|s| s.id != "p2s2");
    let (mut h, s, _, feed) = shelled(data, [1280.0, 720.0]);
    assert!(s.borrow().favorites.is_empty());
    feed.borrow_mut().data = Some(packs());
    h.ui.broadcast(crate::Message::new(FEED));
    h.run(0.3);
    assert_eq!(s.borrow().favorites, ["p2s2"]);
    assert_eq!(song(&s), "p0s0", "the song shown kept");
}

/// The heart pops when turned, but under reduced motion; a phone's layout
/// has it too (a tap), without the footer's word.
#[test]
fn the_heart_pops_but_under_reduced_motion_and_shows_on_a_phone() {
    let (mut h, s, _, _) = shelled(packs(), [844.0, 390.0]);
    assert!(s.borrow().compact);
    assert!(!s.borrow().hints.iter().any(|t| t.starts_with("F ")));
    h.tap(PointerKind::Touch, "Favorite");
    h.run(0.05);
    assert_eq!(s.borrow().favorite, Some(true));
    let heart = |h: &Harness| {
        h.ui.actor("Favorite")
            .and_then(|f| crate::hit::find(f, "Heart").and_then(|p| crate::hit::actor_at(f, &p)))
            .map(|a| a.tween_time_left())
            .unwrap()
    };
    assert!(heart(&h) > 0.0, "it pops");
    assert_eq!(
        crate::heart::shown(h.ui.actor("Favorite").unwrap()),
        Some(true)
    );
    // Reduced motion: at once.
    let e = env();
    e.follow_motion(crate::state::settings::ReducedMotion::On, false);
    let state = e.state.clone();
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(SelectScreen::new(
        e,
        packs(),
        None,
        Rc::new(|_, _, _, _| {}),
        Rc::new(|_, _| {}),
    ));
    h.run(0.5);
    h.key("KeyF", "f");
    h.run(0.01);
    assert_eq!(state.borrow().favorite, Some(false), "p0s0 was one: off");
    assert_eq!(heart(&h), 0.0);
}

/// In a room the picker's own list has the hearts and the pack (More's
/// item among the room's); a follower's side neither.
#[test]
fn a_rooms_picker_has_favorites_and_a_follower_none() {
    use super::room_tests::Three;
    use crate::state::room::RoomClient;
    ddi_l10n::set_language("en");
    favorites::set_favorite("p1s0", true);
    // Practice: the picker.
    let e = env();
    let room = RoomClient::shared(0, 1);
    room.borrow_mut().set_rules_seen(true);
    let orders = Orders::default();
    let mut h = super::test_kit::harness();
    h.push(
        sample::room_select(&e, room.clone(), packs())
            .with_room(room.clone(), super::test_kit::shell(&room, &orders)),
    );
    h.run(0.6);
    assert_eq!(e.state.borrow().favorites, ["p1s0"]);
    assert_eq!(e.state.borrow().favorite, Some(false));
    h.tap(PointerKind::Mouse, "More");
    h.run(0.3);
    assert!(
        h.texts().iter().any(|t| t == "Add to favorites"),
        "{:?}",
        h.texts()
    );
    h.tap(PointerKind::Mouse, "Item.Favorite");
    h.run(0.3);
    assert_eq!(e.state.borrow().favorite, Some(true));
    assert_eq!(e.state.borrow().favorites.len(), 2);
    // Online (revision 17): Room songs, a flat list, has no favorites'
    // pack; My songs has the hearts and the pack, for Alice who picks and
    // Bob who does not alike.
    let t = Three::new(false);
    let (ea, eb) = (env(), env());
    let mut ha = super::test_kit::harness();
    ha.push(
        sample::room_select(&ea, t.alice.clone(), packs())
            .with_room(t.alice.clone(), super::test_kit::shell(&t.alice, &orders)),
    );
    let mut hb = super::test_kit::harness();
    hb.push(
        sample::room_select(&eb, t.bob.clone(), packs())
            .with_room(t.bob.clone(), super::test_kit::shell(&t.bob, &orders)),
    );
    t.settle(400, &mut [&mut ha, &mut hb]);
    ha.run(0.5);
    hb.run(0.5);
    assert!(ea.state.borrow().favorites.is_empty());
    assert!(eb.state.borrow().favorites.is_empty());
    for h in [&mut ha, &mut hb] {
        h.key("KeyM", "m");
        h.run(0.5);
    }
    assert!(!ea.state.borrow().favorites.is_empty());
    assert!(ea.state.borrow().favorite.is_some());
    assert!(!eb.state.borrow().favorites.is_empty());
    assert!(eb.state.borrow().favorite.is_some());
}

/// A player whose Favorites pack was open on My songs has it open again
/// after Room songs (and a hand-over of the pick meanwhile), on the song
/// they left.
#[test]
fn an_open_favorites_pack_comes_back_after_room_songs() {
    use super::room_tests::Three;
    ddi_l10n::set_language("en");
    favorites::set_favorite("p1s2", true);
    favorites::set_favorite("p2s4", true);
    let t = Three::new(true);
    let (ea, eb) = (env(), env());
    let orders = Orders::default();
    let mut ha = super::test_kit::harness();
    ha.push(
        sample::room_select(&ea, t.alice.clone(), packs())
            .with_room(t.alice.clone(), super::test_kit::shell(&t.alice, &orders)),
    );
    let mut hb = super::test_kit::harness();
    hb.push(
        sample::room_select(&eb, t.bob.clone(), packs())
            .with_room(t.bob.clone(), super::test_kit::shell(&t.bob, &orders)),
    );
    t.settle(400, &mut [&mut ha, &mut hb]);
    ha.key("KeyM", "m");
    ha.run(0.5);
    to_top(&mut ha, &ea.state);
    ha.key("Enter", "Enter");
    ha.run(0.3);
    arrow(&mut ha, "ArrowDown");
    arrow(&mut ha, "ArrowDown");
    assert_eq!(song(&ea.state), "p1s2");
    // Bob takes the pick, Alice takes it back (T).
    hb.tap(PointerKind::Mouse, "Room.TakePick");
    t.settle(400, &mut [&mut ha, &mut hb]);
    t.settle(400, &mut [&mut ha, &mut hb]);
    ha.key("KeyM", "m");
    ha.run(0.5);
    assert!(ea.state.borrow().favorites.is_empty(), "Room songs");
    ha.key("KeyT", "t");
    t.settle(400, &mut [&mut ha, &mut hb]);
    t.settle(400, &mut [&mut ha, &mut hb]);
    ha.key("KeyM", "m");
    ha.run(0.5);
    assert_eq!(song(&ea.state), "p1s2");
    assert_eq!(ea.state.borrow().favorites, ["p2s4", "p1s2"]);
    arrow(&mut ha, "ArrowUp");
    assert_eq!(song(&ea.state), "p2s4", "within the open favorites");
}
