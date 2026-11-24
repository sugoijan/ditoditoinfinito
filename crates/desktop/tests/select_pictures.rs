//! Song select's pictures (round SLB, 2026-11-20; `docs/plans/song-navigation.md`,
//! "The song's side" and "In a room"), drawn offscreen for a person to
//! look at: the wide song side under each Chart ratings source (the
//! Groove radar under From the song), its sizes, a phone both ways up, a
//! room's three parts (the players panel open, folded, a row's pop-over,
//! the marks, a stage filling in) and CPU practice, in English and
//! Japanese. Ignored: run by hand with
//!
//! `DDI_SLB_SHOTS=<dir> cargo test -p ddi-desktop --test select_pictures -- --ignored`
//!
//! - `DDI_SLB_SHOTS`: where the pictures go (nothing is drawn without it);
//! - `DDI_SL_ART`: a folder of synthetic covers and banners
//!   (`lantern-cover.png`, `lantern-banner.png`, …; without it those
//!   songs name files that are not there, and the title plate shows);
//! - `DDI_SL_SONGS`: a JSON list of local songs (`id`, `title`, `artist`,
//!   `simfile`, `banner`, `jacket`), read in place; without it, none.

mod common;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ddi_ui::screens::data::{ChartEntry, SelectData, SongEntry};
use ddi_ui::screens::select::SelectScreen;
use ddi_ui::screens::{Env, sample};
use ddi_ui::state::ratings;
use ddi_ui::{Ui, UiEvent};

fn out_dir() -> Option<PathBuf> {
    std::env::var_os("DDI_SLB_SHOTS").map(PathBuf::from)
}

fn art(name: &str) -> String {
    let dir = std::env::var("DDI_SL_ART").unwrap_or_else(|_| "/nonexistent".into());
    format!("{dir}/{name}")
}

fn read_song(path: &Path) -> Option<ddi_chart::Song> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let bytes = std::fs::read(path).ok()?;
    ddi_library::scan::parse_simfile(&String::from_utf8_lossy(&bytes), &ext).ok()
}

/// `song`'s charts as song select has them: named by digest, rated (MSD
/// and the DDI estimate) where they can be, their Groove values.
fn rated_charts(song: &ddi_chart::Song) -> Vec<ChartEntry> {
    let mut out = Vec::new();
    for i in 0..song.charts.len() {
        let Some(mut c) = ChartEntry::of_song(song, i) else {
            continue;
        };
        let d = song.chart_digest(i);
        c.digest = Some(d);
        if song.charts[i].layout == "dance-single" {
            if let Some(v) = ratings::rate_song(song, &[i], &[]).first().and_then(|r| r.values) {
                ratings::remember(d, v);
            }
            if let Some(e) = ratings::estimate_chart(song, i) {
                ratings::remember_estimate(d, e);
            }
        }
        out.push(c);
    }
    out
}

fn song_entry(id: &str, path: &Path, title: Option<&str>, artist: Option<&str>) -> Option<SongEntry> {
    let song = read_song(path)?;
    let mut charts = rated_charts(&song);
    charts.sort_by_key(|c| (c.style.clone(), c.difficulty as usize, c.meter));
    let (lo, hi) = song.charts.first().map_or((f64::NAN, f64::NAN), |c| c.timing(&song).bpm_range());
    let bpm = if !lo.is_finite() {
        String::new()
    } else if (hi - lo).abs() < 0.5 {
        format!("{}", lo.round())
    } else {
        format!("{}–{}", lo.round(), hi.round())
    };
    Some(SongEntry {
        id: id.into(),
        title: title.map_or_else(|| song.title.clone(), str::to_string),
        artist: artist.map_or_else(|| song.artist.clone(), str::to_string),
        bpm_text: bpm,
        length: ddi_library::manifest::song_length(&song) as f32,
        charts,
        ..SongEntry::default()
    })
}

fn bundled(id: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs").join(id);
    std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "ssc" || e == "sm"))
        .expect("the bundled simfile")
}

/// The sample's library with its bundled songs read and rated, a few
/// cases of art and the local songs.
fn library() -> SelectData {
    let mut data = sample::select();
    for s in data.songs.iter_mut().filter(|s| s.pack.is_none()) {
        if let Some(e) = song_entry(&s.id, &bundled(&s.id), None, None) {
            s.charts = e.charts;
            s.credit = String::new();
        }
    }
    let dream = bundled("into-my-dream");
    let add = |id: &str, title: &str, artist: &str, banner: Option<String>, jacket: Option<String>| {
        let mut e = song_entry(id, &dream, Some(title), Some(artist)).expect("the bundled song");
        e.banner = banner;
        e.jacket = jacket;
        e.pack = Some("Cases".into());
        e
    };
    let mut extra = vec![
        add("sl-both", "Lantern Festival", "Kana Lab", Some(art("lantern-banner.png")), Some(art("lantern-cover.png"))),
        add("sl-neither", "Untitled Signal", "The Arrows", None, None),
        add(
            "sl-long",
            "The Extraordinarily Long Title of a Song That Never Seems to End (Extended Mix)",
            "Somebody feat. Another Person & The Long Name Orchestra",
            Some(art("long-banner.png")),
            None,
        ),
        add("sl-ja", "夏祭りの夜に咲く花火", "おにぎり楽団", Some(art("matsuri-banner.png")), Some(art("matsuri-cover.png"))),
    ];
    if let Some(list) = std::env::var_os("DDI_SL_SONGS") {
        let text = std::fs::read_to_string(&list).expect("the songs' list");
        let v: serde_json::Value = serde_json::from_str(&text).expect("JSON");
        for s in v.as_array().into_iter().flatten() {
            let get = |k: &str| s.get(k).and_then(|x| x.as_str()).map(str::to_string);
            let (Some(id), Some(sim)) = (get("id"), get("simfile")) else {
                continue;
            };
            let Some(mut e) = song_entry(&id, Path::new(&sim), get("title").as_deref(), get("artist").as_deref())
            else {
                continue;
            };
            e.banner = get("banner");
            e.jacket = get("jacket");
            e.pack = Some("Library".into());
            extra.push(e);
        }
    }
    data.songs.splice(0..0, extra);
    data
}

fn save(dir: &Path, name: &str, p: &[u8], size: [u32; 2]) {
    std::fs::create_dir_all(dir).unwrap();
    image::save_buffer(dir.join(format!("{name}.png")), p, size[0], size[1], image::ColorType::Rgba8).unwrap();
}

/// Twice: the images asked for in the first are in by the second.
fn shoot(pictures: &mut common::gpu::Pictures, ui: &mut Ui, size: [u32; 2], name: &str) {
    let Some(dir) = out_dir() else { return };
    if let Some(p) = pictures.draw(ui, size) {
        let p = pictures.draw(ui, size).unwrap_or(p);
        save(&dir, name, &p, size);
    }
}

fn source_name(s: ratings::Source) -> &'static str {
    match s {
        ratings::Source::Song => "song",
        ratings::Source::Msd => "msd",
        ratings::Source::Estimate => "ddi",
    }
}

/// Song select alone on `song`, at `size`, in `lang`, under `source`.
fn solo(pictures: &mut common::gpu::Pictures, data: &SelectData, song: &str, source: ratings::Source, size: [u32; 2], lang: &str) {
    assert!(ddi_l10n::force(lang), "{lang}");
    ratings::set_mode(source);
    let css = [size[0] as f32, size[1] as f32];
    let look = Rc::new(ddi_theme::Infinito::new());
    let env = Env::new(look.clone(), Rc::new(|s: &str| s.to_string()));
    env.reduced_motion.set(true);
    let mut ui = Ui::new(ddi_ui::screens::design_for(&*look, css), ddi_ui::TextMode::Own, false);
    ui.set_focus_ring(false);
    ui.push(SelectScreen::new(env, data.clone(), Some(song), Rc::new(|_, _, _, _| {}), Rc::new(|_, _| {})));
    ui.handle(UiEvent::Resize { size: css, ratio: 1.0 });
    common::ui::frames(&mut ui, 1.0);
    let name = format!("solo-{song}-{}-{}x{}-{lang}", source_name(source), size[0], size[1]);
    shoot(pictures, &mut ui, size, &name);
}

#[test]
#[ignore = "song select's pictures: run by hand"]
fn solo_sources_sizes_and_phones() {
    if out_dir().is_none() {
        eprintln!("DDI_SLB_SHOTS unset: nothing drawn");
        return;
    }
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let mut pictures = common::gpu::Pictures::new();
    let data = library();
    for source in [ratings::Source::Song, ratings::Source::Msd, ratings::Source::Estimate] {
        for song in ["sl-both", "into-my-dream", "sl-neither", "sweeteners"] {
            solo(&mut pictures, &data, song, source, [1280, 720], "en");
        }
    }
    for size in [[1440, 900], [1920, 1080], [1024, 768], [844, 390], [390, 844]] {
        solo(&mut pictures, &data, "sl-both", ratings::Source::Song, size, "en");
    }
    for (song, lang) in [("sl-long", "en"), ("sl-ja", "ja"), ("sl-both", "ja")] {
        solo(&mut pictures, &data, song, ratings::Source::Song, [1280, 720], lang);
    }
    solo(&mut pictures, &data, "sl-ja", ratings::Source::Song, [844, 390], "ja");
    for s in data.songs.iter().filter(|s| s.pack.as_deref() == Some("Library")) {
        solo(&mut pictures, &data, &s.id, ratings::Source::Song, [1280, 720], "en");
    }
    ddi_l10n::unforce();
    ratings::set_mode(ratings::Source::Song);
}

/// The names an online room seats after alice and carol.
const NAMES: [&str; 10] = [
    "さくら", "Maximiliana-Constance", "Okami", "ジャン・ニコラ", "Rinne", "Kana Lab", "Leafy", "おにぎり団長",
    "The Arrows", "Café Sound",
];

fn key(ui: &mut Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::up(code, k)));
}

fn hover(ui: &mut Ui, name: &str) {
    let Some(b) = ui.bounds(name) else { return };
    let v = ui.viewport();
    let ph = v.to_physical([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]);
    ui.handle(UiEvent::Pointer(ddi_ui::PointerEvent {
        id: 1,
        kind: ddi_ui::PointerKind::Mouse,
        phase: ddi_ui::PointerPhase::Move,
        pos: [ph[0] / v.ratio, ph[1] / v.ratio],
        buttons: 0,
        modifiers: ddi_ui::Modifiers::default(),
        time: 0.0,
    }));
}

/// An online room of twelve (alice the owner and the one choosing, carol
/// a co-host and the server's administrator, Okami an administrator too),
/// drawn from alice's side: the panel open, a row's words, folded; a
/// stage picked and filling in; Japanese.
#[test]
#[ignore = "song select's pictures: run by hand"]
fn an_online_rooms_three_parts() {
    use common::online::{ROOM, Scene, Songs, person};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Command, Identity, Intent, ManualClock, ParticipantId, RoomPolicy};
    if out_dir().is_none() {
        eprintln!("DDI_SLB_SHOTS unset: nothing drawn");
        return;
    }
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let mut pictures = common::gpu::Pictures::new();
    let all = library();
    for lang in ["en", "ja"] {
        assert!(ddi_l10n::force(lang));
        ratings::set_mode(ratings::Source::Estimate);
        let size = [1280u32, 720];
        let css = [size[0] as f32, size[1] as f32];
        let clock = ManualClock::new(1_000);
        let server =
            FakeServer::hosted(ROOM, Identity::new("dev:alice"), "alice", RoomPolicy::default(), clock.clone()).shared();
        server.borrow_mut().join(Identity::new("dev:carol"), "carol", Intent::Play).unwrap();
        for (k, who) in NAMES.iter().enumerate() {
            server.borrow_mut().join(Identity::new(format!("dev:p{k}")), who, Intent::Play).unwrap();
        }
        let _ = server.borrow_mut().authority(Command::SetCoHost { participant: ParticipantId(2), on: true });
        let mut mine = all.clone();
        mine.songs.retain(|s| ["sl-both", "into-my-dream", "sweeteners"].contains(&s.id.as_str()));
        let songs: Songs = Rc::new(mine.songs.clone());
        let mut scene = Scene {
            clock,
            server: server.clone(),
            people: vec![
                person(&server, "alice", mine.clone(), &songs, css),
                person(&server, "carol", mine.clone(), &songs, css),
            ],
        };
        scene.run(5.0);
        server.borrow_mut().rewrite(|s| {
            for id in [2, 5] {
                if let Some(m) = s.members.get_mut(&ParticipantId(id)) {
                    m.admin = true;
                }
            }
        });
        scene.run(1.0);
        // Alice's songs into the room's list, then Room songs.
        key(&mut scene.people[0].ui, "KeyM", "m");
        scene.run(1.0);
        for _ in 0..3 {
            key(&mut scene.people[0].ui, "Enter", "Enter");
            scene.run(0.6);
            key(&mut scene.people[0].ui, "ArrowDown", "ArrowDown");
            scene.run(0.6);
        }
        scene.run(2.0);
        key(&mut scene.people[0].ui, "KeyM", "m");
        scene.run(1.5);
        let ui = &mut scene.people[0].ui;
        shoot(&mut pictures, ui, size, &format!("room-open-{lang}"));
        hover(ui, "Players.Row1");
        common::ui::frames(ui, 0.3);
        shoot(&mut pictures, ui, size, &format!("room-popover-{lang}"));
        hover(ui, "Header");
        key(ui, "KeyL", "l");
        common::ui::frames(ui, 0.5);
        shoot(&mut pictures, ui, size, &format!("room-folded-{lang}"));
        key(ui, "KeyL", "l");
        common::ui::frames(ui, 0.5);
        // The song shown picked: the stage gathers, the rows fill in.
        key(&mut scene.people[0].ui, "Enter", "Enter");
        scene.run(3.0);
        shoot(&mut pictures, &mut scene.people[0].ui, size, &format!("room-filling-{lang}"));
        shoot(&mut pictures, &mut scene.people[1].ui, size, &format!("room-member-{lang}"));
    }
    ddi_l10n::unforce();
    ratings::set_mode(ratings::Source::Song);
}

/// CPU practice as the desktop opens it (`--screen room`): open and
/// folded, and a phone sideways with its strip.
#[test]
#[ignore = "song select's pictures: run by hand"]
fn cpu_practice() {
    if out_dir().is_none() {
        eprintln!("DDI_SLB_SHOTS unset: nothing drawn");
        return;
    }
    let mut pictures = common::gpu::Pictures::new();
    for size in [[1280u32, 720], [844, 390]] {
        let (mut ui, _env) = common::ui::theme("room", common::ui::nfkd(), size, true);
        common::ui::frames(&mut ui, 1.0);
        shoot(&mut pictures, &mut ui, size, &format!("practice-{}x{}", size[0], size[1]));
        if size[0] >= 1000 {
            key(&mut ui, "KeyL", "l");
            common::ui::frames(&mut ui, 0.5);
            shoot(&mut pictures, &mut ui, size, "practice-folded");
            key(&mut ui, "KeyL", "l");
            common::ui::frames(&mut ui, 0.5);
        }
    }
}
