//! Songs a source finds added to an online room without importing them
//! (`docs/plans/source-plugins.md`, stage 3), as the window does it but
//! headless: the host (alice) on the desktop's own room screen over the
//! room model's fake room (`ddi_room::fake`, no server), the server's
//! shares faked on a loopback port of this process
//! (`common::shares`), and a second member (bob) on a client of his own
//! whose downloads and loads are the desktop's (`ddi_desktop::room`).
//!
//! - The room's More → Find songs to add: "This computer" over a folder
//!   of generated songs in the build's scratch folder; Add to room: the
//!   song listed, uploaded from its item (the bytes the share got are the
//!   song's files), held while the room lists it;
//! - the example plugin (a real process): its songs added the same way;
//!   how far each of its songs may go (its items name each of the three
//!   steps for themselves): Add to room and Import on a song that may be
//!   kept, Add to room alone on one that may go to rooms, neither on one
//!   played here only, each line saying why; and a second plugin whose
//!   manifest says `reach = "play"` offers neither on any song, even one
//!   whose item names a higher step;
//! - the room's own refusals at the press (a full list; a member who may
//!   not add), in its words;
//! - a song removed from the list let go, then added again from My songs'
//!   "Fetched this session";
//! - bob downloads it from the room's share and plays it, alice plays it
//!   from the copy kept (nothing downloaded of her own upload);
//! - a song imported while listed: one entry, the library's copy read
//!   since and the fetched one let go;
//! - the room left: everything let go.
//!
//! The plugins' processes are this test's own executable under their
//! names (`main`), as in `find_songs_plugins.rs`. Pictures where
//! `DDI_OFFSCREEN_PPM` names a folder and a GPU is there.

mod common;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use common::online::{RoomHttp, signed_in};
use common::shares::FakeShares;
use ddi_desktop::play::host::Host;
use ddi_desktop::room::NewsTx;
use ddi_desktop::songs::Songs;
use ddi_room::fake::{FakeServer, FakeSocket, SharedFake};
use ddi_room::{
    ClientId, Clock, Command, ContentStatus, Identity, Intent, ListRules, ManualClock,
    RoomPolicy, RoomSongId, WhenFull,
};
use ddi_ui::screens::DESIGN;
use ddi_ui::state::room::{EntryState, RoomClient, SharedRoom};
use ddi_ui::testing::Harness;
use ddi_ui::{PointerKind, TextMode, Ui};

const SIZE: [u32; 2] = [1280, 720];
const ROOM: &str = common::online::ROOM;
/// The example source again under another id, its manifest letting its
/// songs be played on this computer only.
const PLAY_ONLY: &str = "play-only";

/// What a result's line says where its source lets it be played alone,
/// or played and added to rooms but not imported.
const SAYS_PLAY_ONLY: &str =
    "Play only: its source does not allow importing it or adding it to rooms";
const SAYS_NOT_FOR_KEEPING: &str =
    "Not for importing: its source allows playing it and adding it to rooms only";

fn main() {
    let me = std::env::current_exe().expect("this executable");
    match me.file_stem().and_then(|s| s.to_str()) {
        Some("example") => ddi_source_sdk::main(ddi_source_test_plugins::example::Example),
        Some(PLAY_ONLY) => {
            ddi_source_sdk::main(ddi_source_test_plugins::example::ExampleAs(PLAY_ONLY))
        }
        _ => {
            found_songs_go_into_a_room_without_being_imported();
            println!("room_find_songs: ok");
        }
    }
}

fn top(ui: &Ui) -> String {
    ui.layers()
        .last()
        .map(|l| l.name.clone())
        .unwrap_or_default()
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// A song's simfile, its notes `notes` (a chart's digest is its notes
/// and timing: songs of the same notes are one song to a room).
fn simfile(title: &str, meter: u32, notes: &str) -> String {
    format!(
        "#VERSION:0.83;\n#TITLE:{title};\n#ARTIST:Generated;\n#MUSIC:song.wav;\n#OFFSET:0;\n#BPMS:0=120;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:{meter};\n#NOTES:\n{notes};\n"
    )
}

/// Two seconds of silence: 8 kHz, mono, 16-bit.
fn silence() -> Vec<u8> {
    let data = 32_000u32;
    let mut w = Vec::new();
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&8000u32.to_le_bytes());
    w.extend_from_slice(&16000u32.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    w.resize(44 + data as usize, 0);
    w
}

fn song_at(dir: &Path, title: &str, meter: u32, notes: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("song.ssc"), simfile(title, meter, notes)).unwrap();
    std::fs::write(dir.join("song.wav"), silence()).unwrap();
}

/// This executable installed as plugin `id` in `plugins`, with its
/// manifest (`reach` how far its songs may go, where it states it).
fn install(plugins: &Path, id: &str, reach: Option<&str>) {
    let dir = plugins.join(id);
    let bin = dir.join("bin").join(ddi_source_host::installed::platform());
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(format!("{id}{}", std::env::consts::EXE_SUFFIX));
    let me = std::env::current_exe().unwrap();
    if std::fs::hard_link(&me, &exe).is_err() {
        std::fs::copy(&me, &exe).unwrap();
    }
    let reach = reach
        .map(|r| format!("reach = \"{r}\"\n"))
        .unwrap_or_default();
    std::fs::write(
        dir.join("plugin.toml"),
        format!(
            "id = \"{id}\"\nname = {{ en = \"Test {id}\" }}\npublisher = \"Tests\"\nversion = \"1.0.0\"\nprotocol = [1]\nexecutable = \"{id}\"\ncapabilities = [\"input\", \"search\", \"filters\", \"browse\", \"metadata\", \"fetch\"]\ninputs = [\"url\"]\n{reach}"
        ),
    )
    .unwrap();
}

/// A member's client of the fake room.
fn client(server: &SharedFake, who: &'static str) -> SharedRoom {
    Rc::new(RefCell::new(RoomClient::online(
        signed_in(RoomHttp::new(server, who)),
        ROOM,
        Box::new(FakeSocket::new(server.clone())),
        ClientId::new(format!("{who}-desktop")),
        1_000,
    )))
}

struct World {
    clock: ManualClock,
    server: SharedFake,
    shares: FakeShares,
    /// Shares the room was told of (as the server tells it of a finished
    /// upload).
    told: BTreeSet<String>,
    alice: SharedRoom,
    bob: SharedRoom,
    bob_tx: NewsTx,
    songs: Rc<Songs>,
    env: ddi_ui::screens::Env,
    h: Harness,
}

impl World {
    /// A twentieth of a second: the room's clock, both clients, bob's
    /// device's downloads, alice's screens and her library's loop.
    fn step(&mut self) {
        self.clock.advance(50);
        let now = self.clock.now().0;
        self.alice.borrow_mut().tick(now);
        self.bob.borrow_mut().tick(now);
        let loads = self.bob.borrow_mut().take_load_jobs();
        for job in loads {
            ddi_desktop::room::prepare_online_with(Some(&self.bob_tx), &self.bob, job, None);
        }
        let ahead = self.bob.borrow_mut().take_preload_jobs();
        for job in ahead {
            ddi_desktop::room::preload_with(&self.bob_tx, job);
        }
        let _ = self.bob.borrow_mut().take_upload_jobs();
        self.h.run(0.05);
        self.songs.poll(&mut self.h.ui);
        // A finished upload under an entry's grant: the room lists it as
        // there, as the server tells it.
        for (id, made) in self.shares.made() {
            if !made.finished || self.told.contains(&id) {
                continue;
            }
            let song = made
                .auth
                .rsplit("-song-")
                .next()
                .and_then(|n| n.parse::<u64>().ok())
                .expect("an entry's grant");
            self.server
                .borrow_mut()
                .authority(Command::SetSongContent {
                    song: RoomSongId(song),
                    status: ContentStatus::Available,
                    share: Some(id.clone()),
                    charts: None,
                })
                .unwrap();
            self.told.insert(id);
        }
    }

    /// Steps, real time between for the threads, until `done`.
    fn until(&mut self, what: &str, done: impl Fn(&World) -> bool) {
        let started = std::time::Instant::now();
        while !done(self) {
            assert!(
                started.elapsed().as_secs() < 60,
                "never {what}: {:?} {:?}",
                self.h.ui.layers(),
                self.h.texts()
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
            self.step();
        }
    }

    fn shows(&self, text: &str) -> bool {
        self.h.texts().iter().any(|t| t.contains(text))
    }

    /// The page's item `key` (`button:add:0`) tapped.
    fn press(&mut self, key: &str) {
        let items = self.env.state.borrow().page_items.clone();
        let i = items
            .iter()
            .position(|k| k == key)
            .unwrap_or_else(|| panic!("no {key} in {items:?}"));
        self.h.tap(PointerKind::Mouse, &format!("Item{i}"));
        self.step();
    }

    fn items(&self) -> Vec<String> {
        self.env.state.borrow().page_items.clone()
    }

    /// The room's entry of the song titled `title`.
    fn entry(&self, title: &str) -> Option<ddi_room::RoomSong> {
        self.alice
            .borrow()
            .room_songs()
            .iter()
            .find(|e| e.card.title == title)
            .cloned()
    }

    fn available(&self, title: &str) -> bool {
        self.entry(title).is_some_and(|e| e.available())
    }
}

/// The store's item of fetched song `id`, and whether it is held.
fn item(id: &str) -> ddi_source_host::ItemKey {
    let hex = id.strip_prefix("src:").unwrap().split(':').next().unwrap();
    ddi_source_host::ItemKey(u64::from_str_radix(hex, 16).unwrap())
}

fn held(id: &str) -> bool {
    let store = ddi_desktop::sources::fetched::store().unwrap();
    let s = store.lock().unwrap();
    s.held(item(id))
}

fn in_store(id: &str) -> bool {
    let store = ddi_desktop::sources::fetched::store().unwrap();
    let mut s = store.lock().unwrap();
    s.contains(item(id))
}

fn found_songs_go_into_a_room_without_being_imported() {
    common::indexes();
    assert!(ddi_l10n::set_language("en"));
    let dir = scratch("room-find-songs");
    // SAFETY: set before any thread of this run reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    ddi_desktop::audio::set_headless(true);
    let shares = FakeShares::start();
    ddi_desktop::share::set_base(&shares.base);

    // This computer's folder, the plugins, all switched on.
    let found = dir.join("found");
    let one = found.join("Pack A/Song One");
    song_at(&one, "Song One", 4, "1000\n0010\n0100\n0001\n");
    song_at(&found.join("Pack A/Song Two"), "Song Two", 7, "0001\n0010\n0100\n1000\n");
    let plugins = ddi_desktop::sources::plugins_dir().unwrap();
    install(&plugins, "example", None);
    install(&plugins, PLAY_ONLY, Some("play"));
    let switches = ddi_desktop::sources::switches_path().unwrap();
    std::fs::create_dir_all(switches.parent().unwrap()).unwrap();
    for id in ["example", PLAY_ONLY] {
        ddi_source_host::switches::Switches::set(&switches, id, true).unwrap();
    }
    ddi_desktop::sources::save_switch(true);
    let mut setting = ddi_desktop::sources::Setting::load();
    setting.folders = vec![found.clone()];
    setting.save();
    // A library of a pack of its own (enough songs for the list's packs).
    let library = dir.join("library");
    let patterns = ["1100", "0110", "0011", "1001", "1010", "0101", "1110", "0111"];
    for (n, p) in patterns.iter().enumerate() {
        let notes = format!("{p}\n0000\n{p}\n0000\n");
        song_at(
            &library.join(format!("Library Pack/Song L{n}")),
            &format!("Library Song {n}"),
            3,
            &notes,
        );
    }
    let songs = Songs::new(vec![library]).with_sources();
    assert!(songs.finding());
    songs.skip_offer();
    songs.start(Arc::new(|| {}));
    if let Some(l) = songs.library.borrow_mut().as_mut() {
        l.wait();
    }
    songs.autoplay.set(true);

    // The room: alice its owner, bob a member.
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(
        ROOM,
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "bob", Intent::Play)
        .unwrap();
    let alice = client(&server, "alice");
    alice
        .borrow_mut()
        .set_inbox(ddi_desktop::room::inbox());
    let bob = client(&server, "bob");
    let (bob_tx, bob_inbox) = ddi_desktop::room::news_inbox();
    bob.borrow_mut().set_inbox(bob_inbox);
    bob.borrow_mut().set_library(&[]);

    let sound = Rc::new(RefCell::new(ddi_desktop::preview::Player::with(
        ddi_desktop::audio::Audio::silent(0.0),
        ddi_desktop::library::preview_song,
    )));
    let (mut ui, env, _) =
        ddi_desktop::shell::library_ui("start", common::ui::nfkd(), sound.clone(), &songs);
    env.reduced_motion.set(true);
    ui.push(ddi_desktop::room::online_room_screen_for(
        &env,
        alice.clone(),
        Some(songs.clone()),
        Some(sound),
        Some(songs.play.clone()),
    ));
    let mut h = Harness::with_gestures(DESIGN, DESIGN, 1.0, TextMode::Own, false);
    h.ui = ui;
    let mut w = World {
        clock,
        server,
        shares,
        told: BTreeSet::new(),
        alice: alice.clone(),
        bob: bob.clone(),
        bob_tx,
        songs: songs.clone(),
        env: env.clone(),
        h,
    };
    let mut pictures = common::gpu::Pictures::new();
    w.until("connected", |w| {
        w.alice.borrow().link_state() == ddi_room::LinkState::Connected
            && w.bob.borrow().link_state() == ddi_room::LinkState::Connected
    });
    for _ in 0..20 {
        w.step();
    }
    assert_eq!(top(&w.h.ui), "select");
    // Two songs at most, refused when full.
    alice
        .borrow_mut()
        .set_song_list(ListRules {
            size: 2,
            when_full: WhenFull::Refuse,
        })
        .unwrap();
    w.until("the list's rules", |w| {
        w.alice.borrow().song_list_rules().when_full == WhenFull::Refuse
    });

    // The room's More → Find songs to add.
    w.h.tap(PointerKind::Mouse, "More");
    w.step();
    w.h.run(0.4);
    pictures.shot(&mut w.h.ui, SIZE, "room-more.png");
    w.h.tap(PointerKind::Mouse, "Item.room-find-songs");
    w.h.run(0.4);
    assert_eq!(top(&w.h.ui), "find-songs", "{:?}", w.h.ui.layers());
    w.until("found", |w| w.shows("Song One") && w.shows("Song Two"));
    assert!(w.shows("to add to the room"), "{:?}", w.h.texts());
    let items = w.items();
    assert!(!items.iter().any(|i| i.starts_with("button:play:")), "{items:?}");
    assert!(items.contains(&"button:add:0".to_string()), "{items:?}");
    pictures.shot(&mut w.h.ui, SIZE, "room-find.png");

    // Song One added: listed, uploaded from its item, held.
    let field = "Field.query";
    w.h.tap(PointerKind::Mouse, field);
    w.h.send(ddi_ui::UiEvent::Text(ddi_ui::event::TextEvent::Commit(
        "one".into(),
    )));
    w.h.key("Enter", "Enter");
    w.until("searched", |w| !w.shows("Song Two") && w.shows("Song One"));
    w.press("button:add:0");
    w.until("added", |w| w.shows("Added to the room"));
    w.until("uploaded", |w| w.available("Song One"));
    pictures.shot(&mut w.h.ui, SIZE, "room-added.png");
    let e1 = w.entry("Song One").unwrap();
    let id1 = alice
        .borrow()
        .library_song(&e1)
        .map(|s| s.id.clone())
        .expect("alice's copy");
    assert!(id1.starts_with("src:"), "{id1}");
    assert!(held(&id1), "held while the room lists it");
    assert_eq!(alice.borrow().entry_state(&e1), EntryState::Ready);
    assert!(alice.borrow().ahead_song(&e1).is_none(), "nothing downloaded of her own");
    let made = w.shares.made();
    let share1 = e1.share.clone().expect("its share");
    let sent = &made[&share1].files[0];
    for name in ["song.ssc", "song.wav"] {
        assert_eq!(
            sent.get(name),
            Some(&std::fs::read(one.join(name)).unwrap()),
            "{name}: {:?}",
            sent.keys()
        );
    }

    // The example plugin: Add to room and Import on its songs that may be
    // kept; song 23 may go to rooms and not be kept, song 24 only be
    // played here (their items say so); the play-only plugin: neither.
    let source = |w: &World, name: &str| -> String {
        let texts = w.h.texts();
        let names: Vec<&String> = texts
            .iter()
            .filter(|t| *t == "This computer" || t.starts_with("Test "))
            .collect();
        let i = names.iter().position(|t| *t == name).expect(name);
        format!("button:source:{i}")
    };
    // (The search field emptied: everything listed.)
    w.h.tap(PointerKind::Mouse, field);
    for _ in 0.."one".len() {
        w.h.key("Backspace", "Backspace");
    }
    w.h.key("Enter", "Enter");
    let example = source(&w, "Test example");
    w.press(&example);
    w.until("the example's songs", |w| w.shows("24 results."));
    let items = w.items();
    for i in ["add:0", "import:0", "add:21", "import:21", "add:22"] {
        assert!(items.contains(&format!("button:{i}")), "{i}: {items:?}");
    }
    for i in ["import:22", "add:23", "import:23"] {
        assert!(!items.contains(&format!("button:{i}")), "{i}: {items:?}");
    }
    // Each step alone: why a button is absent, on its line.
    let search = |w: &mut World, text: &str, erase: usize| {
        w.h.tap(PointerKind::Mouse, field);
        for _ in 0..erase {
            w.h.key("Backspace", "Backspace");
        }
        if !text.is_empty() {
            w.h.send(ddi_ui::UiEvent::Text(ddi_ui::event::TextEvent::Commit(
                text.into(),
            )));
        }
        w.h.key("Enter", "Enter");
    };
    search(&mut w, "22", 0);
    w.until("song 22", |w| {
        w.shows("One result") && w.shows("Generated Song 22")
    });
    let items = w.items();
    assert!(
        items.contains(&"button:add:0".to_string())
            && items.contains(&"button:import:0".to_string()),
        "{items:?}"
    );
    assert!(!w.shows(SAYS_PLAY_ONLY) && !w.shows(SAYS_NOT_FOR_KEEPING));
    pictures.shot(&mut w.h.ui, SIZE, "reach-room-keep.png");
    search(&mut w, "23", 2);
    w.until("song 23", |w| {
        w.shows("One result") && w.shows("Generated Song 23")
    });
    assert!(w.shows(SAYS_NOT_FOR_KEEPING), "{:?}", w.h.texts());
    let items = w.items();
    assert!(items.contains(&"button:add:0".to_string()), "{items:?}");
    assert!(!items.contains(&"button:import:0".to_string()), "{items:?}");
    pictures.shot(&mut w.h.ui, SIZE, "reach-room-rooms.png");
    search(&mut w, "24", 2);
    w.until("song 24", |w| {
        w.shows("One result") && w.shows("Generated Song 24")
    });
    assert!(w.shows(SAYS_PLAY_ONLY), "{:?}", w.h.texts());
    assert!(
        !w.items()
            .iter()
            .any(|i| i.starts_with("button:add:") || i.starts_with("button:import:")),
        "{:?}",
        w.items()
    );
    pictures.shot(&mut w.h.ui, SIZE, "reach-room-play.png");
    let play_only = source(&w, &format!("Test {PLAY_ONLY}"));
    w.press(&play_only);
    w.until("the play-only source's song", |w| {
        w.shows("Searching: Test play-only") && w.shows("One result")
    });
    // Song 22's item names `keep`: held to its source's `play`.
    search(&mut w, "22", 2);
    w.until("its song 22", |w| w.shows("Generated Song 22"));
    assert!(w.shows(SAYS_PLAY_ONLY), "{:?}", w.h.texts());
    search(&mut w, "", 2);
    w.until("the play-only source's songs", |w| w.shows("24 results."));
    let items = w.items();
    assert!(
        !items
            .iter()
            .any(|i| i.starts_with("button:add:") || i.starts_with("button:import:")),
        "{items:?}"
    );
    assert!(w.shows(SAYS_PLAY_ONLY));
    assert!(!w.shows(SAYS_NOT_FOR_KEEPING));
    pictures.shot(&mut w.h.ui, SIZE, "reach-room-play-only-source.png");

    // Generated Song 1 from the example (a real process) added.
    let example = source(&w, "Test example");
    w.press(&example);
    w.until("the example's songs", |w| w.shows("24 results."));
    w.press("button:add:0");
    w.until("uploaded", |w| w.available("Generated Song 1"));
    let e2 = w.entry("Generated Song 1").unwrap();
    let id2 = alice.borrow().library_song(&e2).unwrap().id.clone();
    assert!(id2.starts_with("src:") && held(&id2), "{id2}");

    // The list is full: refused at the press, in the room's words, and
    // nothing fetched.
    w.press("button:add:2");
    assert_eq!(top(&w.h.ui), "room-notice", "{:?}", w.h.ui.layers());
    assert!(w.shows("The room's song list is full."), "{:?}", w.h.texts());
    pictures.shot(&mut w.h.ui, SIZE, "room-full.png");
    w.h.key("Escape", "Escape");
    w.h.run(0.4);
    assert_eq!(top(&w.h.ui), "find-songs");

    // Bob may not add: his own Find songs refuses at the press. (One page
    // at a time: a page takes the sources' answers for itself.)
    w.h.key("Escape", "Escape");
    w.h.run(0.6);
    assert_eq!(top(&w.h.ui), "select");
    w.h.ui.push(ddi_desktop::sources::find::room_screen(
        &env,
        &songs,
        None,
        bob.clone(),
    ));
    w.h.run(0.4);
    w.until("bob's results", |w| w.shows("2 results."));
    assert!(w.shows("In the room"), "Song One: {:?}", w.h.texts());
    w.press("button:add:1");
    assert_eq!(top(&w.h.ui), "room-notice");
    assert!(
        w.shows("Only the hosts can add songs to this room now."),
        "{:?}",
        w.h.texts()
    );
    w.h.key("Escape", "Escape");
    w.h.run(0.3);
    w.h.key("Escape", "Escape");
    w.h.run(0.6);
    assert_eq!(top(&w.h.ui), "select");

    // Bob downloads Song One from the room's share and plays it there.
    w.until("bob has it", |w| {
        w.bob.borrow().entry_state(&e1) == EntryState::Ready
    });
    let bob_id = bob
        .borrow()
        .ahead_song(&e1)
        .map(|s| s.id.clone())
        .expect("bob's copy, from the share");
    assert!(bob_id.starts_with("shared:"), "{bob_id}");
    let (files, _) = ddi_desktop::play::load::Files::of(&bob_id).expect("bob's files");
    assert_eq!(
        files.read("song.ssc").unwrap(),
        std::fs::read(one.join("song.ssc")).unwrap()
    );

    // Both play it: bob from his download, alice from the copy kept (her
    // play kept in her history, with where its song came from).
    let mut host = Host::new(songs.play.clone(), songs.plays.clone());
    let fake = common::rig::fake_clock(&mut host);
    let draws = common::rig::Draws::here(&mut host, SIZE);
    for id in [&bob_id, &id1] {
        *songs.play.borrow_mut() = Some(ddi_desktop::play::Request::song(id, 0, true));
        draws.wait_prompt(&mut host, &mut w.h.ui, &env);
        w.h.ui.update(0.5);
        common::ui::key(&mut w.h.ui, "Enter", "Enter");
        w.h.ui.update(0.05);
        draws.poll(&mut host, &mut w.h.ui, &env);
        assert!(host.playing(), "{id} plays");
        let mut n = 0;
        while host.playing() {
            let now = fake.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
            draws.frame(&mut host, &mut w.h.ui, &env, now);
            n += 1;
            assert!(n < 60 * 120, "the play never ended");
        }
        draws.wait_gpu();
        w.h.ui.update(0.05);
        assert_eq!(top(&w.h.ui), "results", "{id}");
        w.h.ui.update(3.0);
        common::ui::key(&mut w.h.ui, "Escape", "Escape");
        w.h.ui.update(0.5);
        draws.poll(&mut host, &mut w.h.ui, &env);
        w.h.ui.update(1.0);
        assert!(host.idle());
        assert!(held(&id1), "still held: the room lists it");
    }
    assert_eq!(top(&w.h.ui), "select");

    // Song One removed from the list: let go, still fetched; then added
    // again from My songs' "Fetched this session", held again.
    alice.borrow_mut().remove_song(e1.id).unwrap();
    w.until("removed", |w| w.entry("Song One").is_none());
    w.step();
    assert!(!held(&id1), "let go once the room no longer lists it");
    assert!(in_store(&id1));
    let mine = songs.room_data_at(&ddi_ui::state::place::MenuPlace::default());
    let fetched: Vec<_> = mine
        .songs
        .iter()
        .filter(|s| s.pack.as_deref() == Some(ddi_ui::state::place::FETCHED_PACK))
        .collect();
    assert!(fetched.iter().any(|s| s.id == id1), "{fetched:?}");
    w.h.tap(PointerKind::Mouse, ddi_ui::screens::select::TAB_MINE_NAME);
    w.h.run(0.6);
    pictures.shot(&mut w.h.ui, SIZE, "room-my-songs.png");
    // What Enter on its row does (`room_start`): the song added.
    let row = fetched.iter().find(|s| s.id == id1).unwrap();
    alice.borrow_mut().add_song(row).unwrap();
    w.until("listed again", |w| w.available("Song One"));
    w.step();
    assert!(held(&id1), "held again");
    let e1 = w.entry("Song One").unwrap();
    assert_eq!(e1.card.pack, "", "the screen's own pack is no song's");
    w.h.tap(PointerKind::Mouse, ddi_ui::screens::select::TAB_ROOM_NAME);
    w.h.run(0.6);
    pictures.shot(&mut w.h.ui, SIZE, "room-songs.png");

    // Find songs from the room: Song One under "Played before", in the
    // room already (no button); removed from the list, Add to room in
    // Play's place, which adds it again from the copy fetched.
    w.h.tap(PointerKind::Mouse, "More");
    w.h.run(0.4);
    w.h.tap(PointerKind::Mouse, "Item.room-find-songs");
    w.h.run(0.4);
    w.until("played before", |w| {
        w.shows("Played before") && w.shows("Last played") && w.shows("Song Two")
    });
    let items = w.items();
    assert!(
        !items.iter().any(|i| i.starts_with("button:played")),
        "in the room already: {items:?}"
    );
    assert!(w.shows("In the room"), "{:?}", w.h.texts());
    alice.borrow_mut().remove_song(e1.id).unwrap();
    w.until("removed again", |w| w.entry("Song One").is_none());
    w.until("offered again", |w| {
        w.items().contains(&"button:played-add:0".to_string())
    });
    assert!(
        !w.items().iter().any(|i| i.starts_with("button:played:")),
        "nothing played over a room"
    );
    pictures.shot(&mut w.h.ui, SIZE, "room-played-before.png");
    w.press("button:played-add:0");
    w.until("added from played before", |w| w.shows("Added to the room"));
    w.until("listed from played before", |w| w.available("Song One"));
    w.step();
    assert!(held(&id1), "held for the room");
    assert_eq!(
        alice
            .borrow()
            .library_song(&w.entry("Song One").unwrap())
            .map(|s| s.id.clone()),
        Some(id1.clone()),
        "its copy fetched, the same song"
    );
    w.h.key("Escape", "Escape");
    w.h.run(0.5);
    assert_eq!(top(&w.h.ui), "select");

    // Generated Song 1 imported while listed: one entry, the library's
    // copy from then on, the fetched one let go.
    w.h.tap(PointerKind::Mouse, "More");
    w.h.run(0.4);
    w.h.tap(PointerKind::Mouse, "Item.room-find-songs");
    w.h.run(0.4);
    let example = source(&w, "Test example");
    w.press(&example);
    w.until("the example's songs", |w| w.shows("24 results."));
    assert!(w.shows("In the room"), "{:?}", w.h.texts());
    w.press("button:import:0");
    w.until("imported", |w| w.shows("Imported one song"));
    w.until("the library's copy", |w| {
        let a = w.alice.borrow();
        let e = w.entry("Generated Song 1").unwrap();
        a.library_song(&e).is_some_and(|s| !s.id.starts_with("src:"))
    });
    w.step();
    assert_eq!(alice.borrow().room_songs().len(), 2, "one entry a song");
    let e2 = w.entry("Generated Song 1").unwrap();
    assert_eq!(alice.borrow().entry_state(&e2), EntryState::Ready);
    assert!(!in_store(&id2), "the fetched copy let go");
    assert!(held(&id1));

    // The room left: everything let go.
    alice.borrow_mut().leave();
    for _ in 0..10 {
        w.step();
    }
    assert!(!held(&id1), "let go with the room");
    assert!(ddi_desktop::sources::rooms::kept().is_empty());
}
