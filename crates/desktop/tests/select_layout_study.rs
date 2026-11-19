//! TESTING AID (the select-layout study, 2026-11-19): song select's song
//! side drawn offscreen in today's layout and in each candidate of
//! `ddi_ui::screens::select::SelectLayout`, at the sizes, cases and
//! languages the study compares, for the maintainer to choose from.
//! Ignored: run by hand with
//!
//! `DDI_SL_SHOTS=<dir> cargo test -p ddi-desktop --test select_layout_study -- --ignored`
//!
//! - `DDI_SL_SHOTS`: where the pictures go (nothing is drawn without it);
//! - `DDI_SL_ART`: a folder of synthetic banners and covers (the study's
//!   own drawings: `lantern-cover.png`, `lantern-banner.png`, …); without
//!   it those songs name files that are not there (the placeholder shows);
//! - `DDI_SL_SONGS`: a JSON list of local songs (`id`, `title`, `artist`,
//!   `simfile`, `banner`, `jacket`, `background`), each read and rated as
//!   a library song; without it, none.

mod common;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ddi_ui::screens::data::{ChartEntry, SelectData, SongEntry};
use ddi_ui::screens::select::{SelectLayout, SelectScreen, set_select_layout};
use ddi_ui::screens::{Env, sample};
use ddi_ui::state::ratings;
use ddi_ui::{Ui, UiEvent};

fn out_dir() -> Option<PathBuf> {
    std::env::var_os("DDI_SL_SHOTS").map(PathBuf::from)
}

fn art(name: &str) -> String {
    let dir = std::env::var("DDI_SL_ART").unwrap_or_else(|_| "/nonexistent".into());
    format!("{dir}/{name}")
}

/// A simfile read as the library reads it.
fn read_song(path: &Path) -> Option<ddi_chart::Song> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    ddi_library::scan::parse_simfile(&text, &ext).ok()
}

/// `song`'s charts as song select has them, each named by its digest and
/// rated (MSD and the DDI estimate) where it can be.
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

fn bpm_text(song: &ddi_chart::Song) -> String {
    let Some(c) = song.charts.first() else {
        return String::new();
    };
    let (lo, hi) = c.timing(song).bpm_range();
    if !lo.is_finite() {
        return String::new();
    }
    if (hi - lo).abs() < 0.5 {
        format!("{}", lo.round())
    } else {
        format!("{}–{}", lo.round(), hi.round())
    }
}

/// A song made of a simfile at `path`, with the art given.
fn song_entry(id: &str, path: &Path, title: Option<&str>, artist: Option<&str>) -> Option<SongEntry> {
    let song = read_song(path)?;
    let mut charts = rated_charts(&song);
    charts.sort_by_key(|c| (c.style.clone(), c.difficulty as usize, c.meter));
    Some(SongEntry {
        id: id.into(),
        title: title.map_or_else(|| song.title.clone(), str::to_string),
        artist: artist.map_or_else(|| song.artist.clone(), str::to_string),
        bpm_text: bpm_text(&song),
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

/// The study's library: the sample's, its bundled songs read and rated,
/// the synthetic cases and the local songs (`DDI_SL_SONGS`).
fn study_data() -> SelectData {
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
        e.pack = Some("Study".into());
        e
    };
    let mut extra = vec![
        add("sl-both", "Lantern Festival", "Kana Lab", Some(art("lantern-banner.png")), Some(art("lantern-cover.png"))),
        add("sl-cover", "Glass Orbit", "Rinne", None, Some(art("orbit-cover.png"))),
        add("sl-neither", "Untitled Signal", "The Arrows", None, None),
        add(
            "sl-long",
            "The Extraordinarily Long Title of a Song That Never Seems to End (Extended Mix)",
            "Somebody feat. Another Person & The Long Name Orchestra",
            Some(art("long-banner.png")),
            None,
        ),
        add("sl-tall", "Tall Cover", "Okami", Some(art("lantern-banner.png")), Some(art("tall-cover.png"))),
        add("sl-wide", "Wide Cover", "Leafy", None, Some(art("wide-cover.png"))),
        add("sl-tiny", "Tiny Cover", "Café Sound", None, Some(art("tiny-cover.png"))),
        add("sl-alpha", "Clear Skies", "Kana Lab", Some(art("alpha-banner.png")), None),
        add("sl-missing", "Missing Files", "Nobody", Some(art("not-there-bn.png")), Some(art("not-there-jk.png"))),
        add("sl-ja", "夏祭りの夜に咲く花火", "おにぎり楽団", Some(art("matsuri-banner.png")), Some(art("matsuri-cover.png"))),
        add("sl-ja-plain", "ほしぞらの歌", "ジャン", None, None),
    ];
    // Seven Single charts: an Edit beside the song's six.
    let mut seven = add("sl-seven", "Seven Charts", "Kana Lab", Some(art("lantern-banner.png")), Some(art("lantern-cover.png")));
    if let Some(mut edit) = seven
        .charts
        .iter()
        .find(|c| c.style == "dance-single" && c.difficulty == ddi_chart::Difficulty::Challenge)
        .cloned()
    {
        edit.difficulty = ddi_chart::Difficulty::Edit;
        edit.index = 99;
        edit.digest = None;
        edit.credit = "Somebody's edit".into();
        seven.charts.push(edit);
    }
    extra.push(seven);
    // The local songs.
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
                eprintln!("{id}: not read");
                continue;
            };
            e.banner = get("banner");
            e.jacket = get("jacket");
            e.background = get("background");
            e.pack = Some("Library".into());
            extra.push(e);
        }
    }
    data.songs.splice(0..0, extra);
    data
}

/// What is drawn: a layout (`None`: today's), the song, the ratings, the
/// size and the language.
struct Shot<'a> {
    layout: Option<SelectLayout>,
    song: &'a str,
    radar: Option<ratings::Source>,
    size: [u32; 2],
    lang: &'a str,
    case: &'a str,
}

fn layout_name(l: Option<SelectLayout>) -> String {
    l.map_or_else(|| "today".into(), |l| l.name().to_string())
}

fn radar_name(r: Option<ratings::Source>) -> &'static str {
    match r {
        None => "off",
        Some(ratings::Source::Estimate) => "ddi",
        Some(_) => "msd",
    }
}

fn draw(pictures: &mut common::gpu::Pictures, data: &SelectData, s: &Shot) {
    let Some(dir) = out_dir() else {
        return;
    };
    set_select_layout(s.layout);
    assert!(ddi_l10n::force(s.lang), "{}", s.lang);
    match s.radar {
        Some(source) => ratings::set_mode(source, true),
        None => ratings::set_mode(ratings::Source::Song, false),
    }
    let css = [s.size[0] as f32, s.size[1] as f32];
    let look = Rc::new(ddi_theme::Infinito::new());
    let env = Env::new(look.clone(), Rc::new(|s: &str| s.to_string()));
    env.reduced_motion.set(true);
    let mut ui = Ui::new(ddi_ui::screens::design_for(&*look, css), ddi_ui::TextMode::Own, false);
    ui.set_focus_ring(false);
    ui.push(SelectScreen::new(
        env.clone(),
        data.clone(),
        Some(s.song),
        Rc::new(|_, _, _, _| {}),
        Rc::new(|_, _| {}),
    ));
    ui.handle(UiEvent::Resize { size: css, ratio: 1.0 });
    common::ui::frames(&mut ui, 1.0);
    let Some(p) = pictures.draw(&mut ui, s.size) else {
        return;
    };
    // Twice: the images asked for in the first are in by the second.
    let p = pictures.draw(&mut ui, s.size).unwrap_or(p);
    let name = format!(
        "{}-{}-{}-{}x{}-{}.png",
        layout_name(s.layout),
        s.case,
        radar_name(s.radar),
        s.size[0],
        s.size[1],
        s.lang
    );
    std::fs::create_dir_all(&dir).unwrap();
    image::save_buffer(dir.join(name), &p, s.size[0], s.size[1], image::ColorType::Rgba8).unwrap();
}

const LAYOUTS: [Option<SelectLayout>; 5] = [
    None,
    Some(SelectLayout::Beside),
    Some(SelectLayout::Twins),
    Some(SelectLayout::Column),
    Some(SelectLayout::Card),
];

const CASES: [(&str, &str); 12] = [
    ("both", "sl-both"),
    ("banner", "into-my-dream"),
    ("cover", "sl-cover"),
    ("neither", "sl-neither"),
    ("long", "sl-long"),
    ("seven", "sl-seven"),
    ("tall", "sl-tall"),
    ("wide", "sl-wide"),
    ("tiny", "sl-tiny"),
    ("alpha", "sl-alpha"),
    ("missing", "sl-missing"),
    ("bundled2", "sweeteners"),
];

const RADARS: [Option<ratings::Source>; 3] = [None, Some(ratings::Source::Msd), Some(ratings::Source::Estimate)];

#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn the_song_side_in_each_candidate() {
    if out_dir().is_none() {
        eprintln!("DDI_SL_SHOTS unset: nothing drawn");
        return;
    }
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let data = study_data();
    let mut pictures = common::gpu::Pictures::new();
    let only = std::env::var("DDI_SL_ONLY").ok();
    let wanted = |l: Option<SelectLayout>| only.as_deref().is_none_or(|o| o.contains(&layout_name(l)));
    for layout in LAYOUTS.into_iter().filter(|l| wanted(*l)) {
        // Every case at the design's size, each radar.
        for (case, song) in CASES {
            for radar in RADARS {
                draw(&mut pictures, &data, &Shot { layout, song, radar, size: [1280, 720], lang: "en", case });
            }
        }
        // The other sizes: a song with both, the bundled banner, neither,
        // the long one; the radar On (MSD) and Off.
        for size in [[1440u32, 900u32], [1920, 1080], [1024, 768], [844, 390], [390, 844]] {
            for (case, song) in [("both", "sl-both"), ("banner", "into-my-dream"), ("neither", "sl-neither"), ("long", "sl-long")] {
                for radar in [None, Some(ratings::Source::Msd)] {
                    draw(&mut pictures, &data, &Shot { layout, song, radar, size, lang: "en", case });
                }
            }
        }
        // Japanese.
        for (case, song) in [("ja", "sl-ja"), ("ja-plain", "sl-ja-plain"), ("banner", "into-my-dream"), ("long", "sl-long")] {
            for radar in [None, Some(ratings::Source::Estimate)] {
                for size in [[1280u32, 720u32], [844, 390]] {
                    draw(&mut pictures, &data, &Shot { layout, song, radar, size, lang: "ja", case });
                }
            }
        }
    }
    set_select_layout(None);
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}

/// The local songs (`DDI_SL_SONGS`), each in each layout at 1280×720 with
/// the DDI estimate's radar.
#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn local_songs_in_each_candidate() {
    if out_dir().is_none() || std::env::var_os("DDI_SL_SONGS").is_none() {
        eprintln!("DDI_SL_SHOTS or DDI_SL_SONGS unset: nothing drawn");
        return;
    }
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let data = study_data();
    let ids: Vec<String> = data
        .songs
        .iter()
        .filter(|s| s.pack.as_deref() == Some("Library"))
        .map(|s| s.id.clone())
        .collect();
    let mut pictures = common::gpu::Pictures::new();
    let only = std::env::var("DDI_SL_ONLY").ok();
    for layout in LAYOUTS
        .into_iter()
        .filter(|l| only.as_deref().is_none_or(|o| o.contains(&layout_name(*l))))
    {
        for id in &ids {
            let radar = Some(ratings::Source::Estimate);
            draw(&mut pictures, &data, &Shot { layout, song: id, radar, size: [1280, 720], lang: "en", case: id });
        }
    }
    set_select_layout(None);
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}

/// A room's short side (the owner's and a member's My songs, the players
/// panel under it) in today's layout and each candidate's, at 1280×720
/// and a phone held sideways, over the room model's fake room.
#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn a_rooms_short_side_in_each_candidate() {
    use common::online::{ROOM, Scene, Songs, person};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Identity, Intent, ManualClock, RoomPolicy};
    let Some(dir) = out_dir() else {
        eprintln!("DDI_SL_SHOTS unset: nothing drawn");
        return;
    };
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut data = study_data();
    let keep = ["sl-both", "into-my-dream", "sl-neither", "sl-long", "sl-cover"];
    data.songs.retain(|s| keep.contains(&s.id.as_str()));
    data.songs.sort_by_key(|s| keep.iter().position(|k| *k == s.id));
    for (i, s) in data.songs.iter_mut().enumerate() {
        s.pack = None;
        for (k, c) in s.charts.iter_mut().enumerate() {
            c.digest = Some(ddi_chart::ChartDigest(0x51_0000 + (i as u128) * 64 + k as u128));
        }
    }
    let mut pictures = common::gpu::Pictures::new();
    for layout in LAYOUTS {
        set_select_layout(layout);
        for (size, lang) in [([1280u32, 720u32], "en"), ([844, 390], "en"), ([1280, 720], "ja")] {
            assert!(ddi_l10n::force(lang));
            ratings::set_mode(ratings::Source::Msd, true);
            let css = [size[0] as f32, size[1] as f32];
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
                .join(Identity::new("dev:carol"), "carol", Intent::Play)
                .unwrap();
            let all: Songs = Rc::new(data.songs.clone());
            let mut scene = Scene {
                clock,
                server: server.clone(),
                people: vec![
                    person(&server, "alice", data.clone(), &all, css),
                    person(&server, "carol", data.clone(), &all, css),
                ],
            };
            scene.run(5.0);
            for p in &mut scene.people {
                let ui = &mut p.ui;
                ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::down("KeyM", "m")));
                ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::up("KeyM", "m")));
            }
            scene.run(1.0);
            for step in 0..2 {
                for (who, name) in [(0, "host"), (1, "member")] {
                    if let Some(p) = pictures.draw(&mut scene.people[who].ui, size) {
                        let p = pictures.draw(&mut scene.people[who].ui, size).unwrap_or(p);
                        let file = format!(
                            "{}-room-{name}{step}-msd-{}x{}-{lang}.png",
                            layout_name(layout),
                            size[0],
                            size[1]
                        );
                        image::save_buffer(dir.join(file), &p, size[0], size[1], image::ColorType::Rgba8).unwrap();
                    }
                    let ui = &mut scene.people[who].ui;
                    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::down("ArrowDown", "ArrowDown")));
                    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::up("ArrowDown", "ArrowDown")));
                }
                scene.run(1.0);
            }
        }
    }
    set_select_layout(None);
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}

// Round 2: B refined (`SelectLayout::TwinsTwo`, `b2`).

/// The list's variants round 2 compares: the rows' heights (pitch and the
/// selected row's) and the song side's and the list's widths.
const LISTS: [(&str, [(&str, f32); 5]); 4] = [
    ("rows66", [("Select.Wheel.Row", 66.0), ("Select.Wheel.Selected", 88.0), ("Select.Song.Width", 560.0), ("Select.Wheel.X", 640.0), ("Select.Wheel.Width", 596.0)]),
    ("rows56", [("Select.Wheel.Row", 56.0), ("Select.Wheel.Selected", 76.0), ("Select.Song.Width", 560.0), ("Select.Wheel.X", 640.0), ("Select.Wheel.Width", 596.0)]),
    ("narrow", [("Select.Wheel.Row", 66.0), ("Select.Wheel.Selected", 88.0), ("Select.Song.Width", 640.0), ("Select.Wheel.X", 720.0), ("Select.Wheel.Width", 516.0)]),
    ("both", [("Select.Wheel.Row", 56.0), ("Select.Wheel.Selected", 76.0), ("Select.Song.Width", 640.0), ("Select.Wheel.X", 720.0), ("Select.Wheel.Width", 516.0)]),
];

/// The list variant round 2 recommends (`DDI_SL_LIST` overrides it).
fn chosen_list() -> &'static [(&'static str, f32); 5] {
    let want = std::env::var("DDI_SL_LIST").unwrap_or_else(|_| "both".into());
    &LISTS.iter().find(|(n, _)| *n == want).unwrap_or(&LISTS[3]).1
}

fn look_with(metrics: &[(&str, f32)]) -> Rc<ddi_theme::Infinito> {
    let mut look = ddi_theme::Infinito::new();
    for (k, v) in metrics {
        look.set_metric(k, *v);
    }
    Rc::new(look)
}

/// Song select drawn as round 2 has it: `file` names the picture.
#[allow(clippy::too_many_arguments)]
fn draw2(
    pictures: &mut common::gpu::Pictures,
    data: &SelectData,
    song: &str,
    source: ratings::Source,
    size: [u32; 2],
    lang: &str,
    metrics: &[(&str, f32)],
    file: &str,
) -> Option<Ui> {
    let dir = out_dir()?;
    assert!(ddi_l10n::force(lang), "{lang}");
    ratings::set_mode(source, true);
    let css = [size[0] as f32, size[1] as f32];
    let look = look_with(metrics);
    let env = Env::new(look.clone(), Rc::new(|s: &str| s.to_string()));
    env.reduced_motion.set(true);
    let mut ui = Ui::new(ddi_ui::screens::design_for(&*look, css), ddi_ui::TextMode::Own, false);
    ui.set_focus_ring(false);
    ui.push(SelectScreen::new(
        env.clone(),
        data.clone(),
        Some(song),
        Rc::new(|_, _, _, _| {}),
        Rc::new(|_, _| {}),
    ));
    ui.handle(UiEvent::Resize { size: css, ratio: 1.0 });
    common::ui::frames(&mut ui, 1.0);
    let p = pictures.draw(&mut ui, size)?;
    let p = pictures.draw(&mut ui, size).unwrap_or(p);
    std::fs::create_dir_all(&dir).unwrap();
    image::save_buffer(dir.join(format!("{file}.png")), &p, size[0], size[1], image::ColorType::Rgba8).unwrap();
    Some(ui)
}

/// The local songs' preview stills (`preview` in `DDI_SL_SONGS`'s list).
fn previews() -> Vec<(String, String)> {
    let Some(list) = std::env::var_os("DDI_SL_SONGS") else {
        return Vec::new();
    };
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(list).unwrap()).unwrap();
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| Some((s.get("id")?.as_str()?.to_string(), s.get("preview")?.as_str()?.to_string())))
        .collect()
}

#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn round_two_b2() {
    use ddi_ui::screens::select::{PreviewStill, set_preview_still};
    if out_dir().is_none() {
        eprintln!("DDI_SL_SHOTS unset: nothing drawn");
        return;
    }
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let data = study_data();
    let mut pictures = common::gpu::Pictures::new();
    let est = ratings::Source::Estimate;
    set_select_layout(Some(SelectLayout::TwinsTwo));
    let list = chosen_list();
    // Main cases, the list as recommended; MSD once for comparison.
    for (case, song) in CASES {
        draw2(&mut pictures, &data, song, est, [1280, 720], "en", list, &format!("b2-{case}-ddi-1280x720-en"));
    }
    for (case, song) in [("both", "sl-both"), ("banner", "into-my-dream")] {
        draw2(&mut pictures, &data, song, ratings::Source::Msd, [1280, 720], "en", list, &format!("b2-{case}-msd-1280x720-en"));
    }
    // Sizes.
    for size in [[1440u32, 900u32], [1920, 1080], [1024, 768], [844, 390], [390, 844]] {
        for (case, song) in [("both", "sl-both"), ("banner", "into-my-dream"), ("long", "sl-long")] {
            draw2(&mut pictures, &data, song, est, size, "en", list, &format!("b2-{case}-ddi-{}x{}-en", size[0], size[1]));
        }
    }
    // Japanese.
    for (case, song) in [("ja", "sl-ja"), ("ja-plain", "sl-ja-plain"), ("long", "sl-long")] {
        for size in [[1280u32, 720u32], [844, 390]] {
            draw2(&mut pictures, &data, song, est, size, "ja", list, &format!("b2-{case}-ddi-{}x{}-ja", size[0], size[1]));
        }
    }
    // The list's variants.
    for (name, metrics) in LISTS {
        for size in [[1280u32, 720u32], [1440, 900]] {
            for (case, song) in [("both", "sl-both"), ("long", "sl-long"), ("ja", "sl-ja")] {
                let lang = if case == "ja" { "ja" } else { "en" };
                draw2(&mut pictures, &data, song, est, size, lang, &metrics, &format!("b2-list-{name}-{case}-{}x{}-{lang}", size[0], size[1]));
            }
        }
    }
    // The preview movie's options (a still of it at 3 s; the synthetic
    // song's a drawn frame): before, and playing as a, b, c, d.
    let mut movies = vec![("sl-both".to_string(), art("lantern-movie.png"))];
    movies.extend(previews());
    for (song, still) in &movies {
        set_preview_still(None);
        draw2(&mut pictures, &data, song, est, [1280, 720], "en", list, &format!("b2-video-{song}-0before-1280x720-en"));
        for mode in ['a', 'b', 'c', 'd'] {
            set_preview_still(Some(PreviewStill { song: song.clone(), still: still.clone(), mode }));
            draw2(&mut pictures, &data, song, est, [1280, 720], "en", list, &format!("b2-video-{song}-{mode}-1280x720-en"));
        }
        for size in [[844u32, 390u32], [390, 844]] {
            set_preview_still(Some(PreviewStill { song: song.clone(), still: still.clone(), mode: 'a' }));
            draw2(&mut pictures, &data, song, est, size, "en", list, &format!("b2-video-{song}-a-{}x{}-en", size[0], size[1]));
        }
        set_preview_still(None);
    }
    // The library sample.
    let ids: Vec<String> = data.songs.iter().filter(|s| s.pack.as_deref() == Some("Library")).map(|s| s.id.clone()).collect();
    for id in &ids {
        draw2(&mut pictures, &data, id, est, [1280, 720], "en", list, &format!("b2-lib-{id}-ddi-1280x720-en"));
    }
    set_select_layout(None);
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}

/// Round 2's room: an online room of fourteen (twelve listed and a count),
/// its owner and a member on both tabs, a room song not on the member's
/// device yet, a stage picked and waiting; each form at each size.
#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn round_two_room() {
    use common::online::{ROOM, Scene, Songs, person};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Command, Identity, Intent, ManualClock, ParticipantId, RoomPolicy};
    use ddi_ui::screens::select::{RoomForm, set_room_form};
    let Some(dir) = out_dir() else {
        eprintln!("DDI_SL_SHOTS unset: nothing drawn");
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let all_data = study_data();
    ratings::set_mode(ratings::Source::Estimate, true);
    let pick = |ids: &[&str], salt: u128| {
        let mut d = all_data.clone();
        d.songs.retain(|s| ids.contains(&s.id.as_str()));
        d.songs.sort_by_key(|s| ids.iter().position(|k| *k == s.id));
        for (i, s) in d.songs.iter_mut().enumerate() {
            s.pack = Some("Mine".into());
            for (k, c) in s.charts.iter_mut().enumerate() {
                let was = c.digest;
                let d = ddi_chart::ChartDigest(salt + (i as u128) * 64 + k as u128);
                c.digest = Some(d);
                // The ratings kept under the new digest.
                if let Some(Some(e)) = was.and_then(ratings::estimate) {
                    ratings::remember_estimate(d, e);
                }
                if let Some(v) = was.and_then(ratings::get) {
                    ratings::remember(d, v);
                }
            }
        }
        d
    };
    let alice = pick(&["sl-both", "sl-cover", "into-my-dream", "sl-long"], 0x61_0000);
    let carol = pick(&["sl-neither", "sweeteners"], 0x63_0000);
    let mut pictures = common::gpu::Pictures::new();
    set_select_layout(Some(SelectLayout::TwinsTwo));
    let forms = [(RoomForm::Beside, "r1"), (RoomForm::OnDemand, "r2"), (RoomForm::OnDemandOpen, "r2open")];
    for size in [[1280u32, 720u32], [1440, 900], [844, 390], [390, 844]] {
        for (form, fname) in forms {
            // A phone's room has one form.
            if size[0] < 1000 && form != RoomForm::Beside {
                continue;
            }
            set_room_form(form);
            assert!(ddi_l10n::force("en"));
            ratings::set_mode(ratings::Source::Estimate, true);
            let css = [size[0] as f32, size[1] as f32];
            let clock = ManualClock::new(1_000);
            let server = FakeServer::hosted(ROOM, Identity::new("dev:alice"), "alice", RoomPolicy::default(), clock.clone()).shared();
            server.borrow_mut().join(Identity::new("dev:carol"), "carol", Intent::Play).unwrap();
            for n in 0..12 {
                let who = format!("player{:02}", n + 1);
                server.borrow_mut().join(Identity::new(format!("dev:{who}")), &who, Intent::Play).unwrap();
            }
            let _ = server.borrow_mut().authority(Command::SetCoHost { participant: ParticipantId(2), on: true });
            let all: Songs = Rc::new(alice.songs.iter().chain(&carol.songs).cloned().collect());
            let mut scene = Scene {
                clock,
                server: server.clone(),
                people: vec![
                    person(&server, "alice", alice.clone(), &all, css),
                    person(&server, "carol", carol.clone(), &all, css),
                ],
            };
            let mut shot = |scene: &mut Scene, who: usize, name: &str| {
                let ui = &mut scene.people[who].ui;
                if let Some(p) = pictures.draw(ui, size) {
                    let p = pictures.draw(ui, size).unwrap_or(p);
                    let file = format!("b2-room-{fname}-{name}-{}x{}-en.png", size[0], size[1]);
                    image::save_buffer(dir.join(file), &p, size[0], size[1], image::ColorType::Rgba8).unwrap();
                }
            };
            let key = |scene: &mut Scene, who: usize, code: &str, k: &str| {
                let ui = &mut scene.people[who].ui;
                ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::down(code, k)));
                ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::up(code, k)));
            };
            scene.run(5.0);
            // Both on My songs.
            key(&mut scene, 0, "KeyM", "m");
            key(&mut scene, 1, "KeyM", "m");
            scene.run(1.0);
            shot(&mut scene, 0, "1-host-mine");
            shot(&mut scene, 1, "2-member-mine");
            // The owner adds three of hers to the room's list.
            for _ in 0..3 {
                key(&mut scene, 0, "Enter", "Enter");
                scene.run(0.6);
                key(&mut scene, 0, "ArrowDown", "ArrowDown");
                scene.run(0.6);
            }
            scene.run(2.0);
            key(&mut scene, 0, "KeyM", "m");
            key(&mut scene, 1, "KeyM", "m");
            scene.run(1.0);
            shot(&mut scene, 0, "3-host-room");
            // The member's Room songs: songs her device does not have yet.
            shot(&mut scene, 1, "4-member-room-away");
            // The owner picks the first for stage 1: picked and waiting.
            key(&mut scene, 0, "Enter", "Enter");
            scene.run(1.5);
            shot(&mut scene, 0, "5-host-picked");
            shot(&mut scene, 1, "6-member-picked");
        }
    }
    set_room_form(RoomForm::Beside);
    set_select_layout(None);
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}

// Round 3: the room in three columns, and the preview movie's a and d.

/// The names round 3's rooms seat after alice and carol (Japanese and long
/// Latin ones among them).
const NAMES: [&str; 28] = [
    "さくら", "Maximiliana-Constance", "Okami", "ジャン・ニコラ", "Rinne", "Kana Lab", "Leafy",
    "おにぎり団長", "The Arrows", "Café Sound", "Bartholomew_the_Third", "みどり", "Dancer07", "Nobody",
    "ほしぞら", "Quentin", "Ysolde", "Takumi", "Zephyrine", "ゆめ", "Ostrava", "Pip", "Marguerite",
    "Ichigo", "Fennel", "Hanabi", "Juniper", "Kestrel",
];

const ROWS56: [(&str, f32); 2] = [("Select.Wheel.Row", 56.0), ("Select.Wheel.Selected", 76.0)];

/// An online room of `n` (alice the owner, carol a co-host, the rest of
/// [`NAMES`]), alice's library `mine` and carol's `theirs`, at `size`.
fn room3(n: usize, size: [u32; 2], mine: &SelectData, theirs: &SelectData) -> common::online::Scene {
    use common::online::{ROOM, Scene, Songs, person};
    use ddi_room::fake::FakeServer;
    use ddi_room::{Command, Identity, Intent, ManualClock, ParticipantId, RoomPolicy};
    let css = [size[0] as f32, size[1] as f32];
    let clock = ManualClock::new(1_000);
    let server = FakeServer::hosted(ROOM, Identity::new("dev:alice"), "alice", RoomPolicy::default(), clock.clone()).shared();
    server.borrow_mut().join(Identity::new("dev:carol"), "carol", Intent::Play).unwrap();
    for (k, who) in NAMES.iter().take(n.saturating_sub(2)).enumerate() {
        server.borrow_mut().join(Identity::new(format!("dev:p{k}")), who, Intent::Play).unwrap();
    }
    let _ = server.borrow_mut().authority(Command::SetCoHost { participant: ParticipantId(2), on: true });
    let all: Songs = Rc::new(mine.songs.iter().chain(&theirs.songs).cloned().collect());
    common::online::LOOK_METRICS.with(|m| *m.borrow_mut() = ROWS56.to_vec());
    let mut scene = Scene {
        clock,
        server: server.clone(),
        people: vec![
            person(&server, "alice", mine.clone(), &all, css),
            person(&server, "carol", theirs.clone(), &all, css),
        ],
    };
    scene.run(5.0);
    scene
}

fn key3(scene: &mut common::online::Scene, who: usize, code: &str, k: &str) {
    let ui = &mut scene.people[who].ui;
    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::up(code, k)));
}

fn shot3(pictures: &mut common::gpu::Pictures, ui: &mut Ui, size: [u32; 2], file: &str) {
    let Some(dir) = out_dir() else { return };
    if let Some(p) = pictures.draw(ui, size) {
        let p = pictures.draw(ui, size).unwrap_or(p);
        image::save_buffer(dir.join(format!("{file}.png")), &p, size[0], size[1], image::ColorType::Rgba8).unwrap();
    }
}

/// Alice's songs added to the room's list (three), then both on Room songs.
fn add_three(scene: &mut common::online::Scene) {
    key3(scene, 0, "KeyM", "m");
    key3(scene, 1, "KeyM", "m");
    scene.run(1.0);
    for _ in 0..3 {
        key3(scene, 0, "Enter", "Enter");
        scene.run(0.6);
        key3(scene, 0, "ArrowDown", "ArrowDown");
        scene.run(0.6);
    }
    scene.run(2.0);
    key3(scene, 0, "KeyM", "m");
    key3(scene, 1, "KeyM", "m");
    scene.run(1.0);
}

/// `ids` of the study's library as one person's songs, their charts named
/// anew (`salt`) with their ratings kept.
fn library_of(all: &SelectData, ids: &[&str], salt: u128) -> SelectData {
    let mut d = all.clone();
    d.songs.retain(|s| ids.contains(&s.id.as_str()));
    d.songs.sort_by_key(|s| ids.iter().position(|k| *k == s.id));
    for (i, s) in d.songs.iter_mut().enumerate() {
        s.pack = Some("Mine".into());
        for (k, c) in s.charts.iter_mut().enumerate() {
            let was = c.digest;
            let dg = ddi_chart::ChartDigest(salt + (i as u128) * 64 + k as u128);
            c.digest = Some(dg);
            if let Some(Some(e)) = was.and_then(ratings::estimate) {
                ratings::remember_estimate(dg, e);
            }
        }
    }
    d
}

#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn round_three_room() {
    use ddi_ui::screens::select::{PlayerIcon, RoomForm, set_column_width, set_room_form, set_staged_icons};
    let Some(dir) = out_dir() else {
        eprintln!("DDI_SL_SHOTS unset: nothing drawn");
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let all = study_data();
    assert!(ddi_l10n::force("en"));
    ratings::set_mode(ratings::Source::Estimate, true);
    let mine = library_of(&all, &["sl-both", "sl-cover", "into-my-dream", "sl-long"], 0x71_0000);
    let theirs = library_of(&all, &["sl-neither", "sweeteners"], 0x73_0000);
    let mut pictures = common::gpu::Pictures::new();
    set_select_layout(Some(SelectLayout::TwinsTwo));
    set_room_form(RoomForm::Columns);
    let s720 = [1280u32, 720u32];
    // The column's widths, fourteen in the room.
    for pw in [150.0, 180.0, 210.0] {
        set_column_width(pw);
        let mut sc = room3(14, s720, &mine, &theirs);
        add_three(&mut sc);
        shot3(&mut pictures, &mut sc.people[0].ui, s720, &format!("b3-room-w{pw}-host-room-1280x720-en"));
        shot3(&mut pictures, &mut sc.people[1].ui, s720, &format!("b3-room-w{pw}-member-away-1280x720-en"));
    }
    set_column_width(180.0);
    // How many: 3, 8, 14, 30.
    for n in [3usize, 8, 14, 30] {
        let mut sc = room3(n, s720, &mine, &theirs);
        add_three(&mut sc);
        shot3(&mut pictures, &mut sc.people[0].ui, s720, &format!("b3-room-n{n}-host-room-1280x720-en"));
    }
    // The flow at 180 with fourteen; R1 at the same moments.
    for (form, fname) in [(RoomForm::Columns, "cols"), (RoomForm::Beside, "r1")] {
        set_room_form(form);
        let mut sc = room3(14, s720, &mine, &theirs);
        key3(&mut sc, 0, "KeyM", "m");
        key3(&mut sc, 1, "KeyM", "m");
        sc.run(1.0);
        shot3(&mut pictures, &mut sc.people[0].ui, s720, &format!("b3-flow-{fname}-1-host-mine-1280x720-en"));
        shot3(&mut pictures, &mut sc.people[1].ui, s720, &format!("b3-flow-{fname}-2-member-mine-1280x720-en"));
        for _ in 0..3 {
            key3(&mut sc, 0, "Enter", "Enter");
            sc.run(0.6);
            key3(&mut sc, 0, "ArrowDown", "ArrowDown");
            sc.run(0.6);
        }
        sc.run(2.0);
        key3(&mut sc, 0, "KeyM", "m");
        key3(&mut sc, 1, "KeyM", "m");
        sc.run(1.0);
        shot3(&mut pictures, &mut sc.people[0].ui, s720, &format!("b3-flow-{fname}-3-host-room-1280x720-en"));
        shot3(&mut pictures, &mut sc.people[1].ui, s720, &format!("b3-flow-{fname}-4-member-away-1280x720-en"));
        key3(&mut sc, 0, "Enter", "Enter");
        sc.run(1.5);
        shot3(&mut pictures, &mut sc.people[0].ui, s720, &format!("b3-flow-{fname}-5-host-picked-1280x720-en"));
        // The ready states filling in (staged: the fake room's others are
        // not connected).
        set_staged_icons(Some(vec![
            PlayerIcon::Ready,
            PlayerIcon::Loading(0.35),
            PlayerIcon::NotReady,
            PlayerIcon::Ready,
            PlayerIcon::Loading(0.8),
            PlayerIcon::WaitingSong,
            PlayerIcon::Ready,
            PlayerIcon::Away,
        ]));
        refill(&mut sc);
        shot3(&mut pictures, &mut sc.people[0].ui, s720, &format!("b3-flow-{fname}-6-host-filling-1280x720-en"));
        shot3(&mut pictures, &mut sc.people[1].ui, s720, &format!("b3-flow-{fname}-7-member-filling-1280x720-en"));
        // A stage in play as one who sat it out sees it (staged).
        set_staged_icons(Some(vec![
            PlayerIcon::Playing,
            PlayerIcon::Playing,
            PlayerIcon::Out,
            PlayerIcon::FailedPlaying,
            PlayerIcon::Playing,
            PlayerIcon::SatOut,
            PlayerIcon::Watching,
            PlayerIcon::Left,
        ]));
        refill(&mut sc);
        shot3(&mut pictures, &mut sc.people[1].ui, s720, &format!("b3-flow-{fname}-8-member-satout-1280x720-en"));
        set_staged_icons(None);
    }
    set_room_form(RoomForm::Columns);
    // Sizes and Japanese.
    for (size, lang) in [
        ([1440u32, 900u32], "en"),
        ([1920, 1080], "en"),
        ([1024, 768], "en"),
        ([2560, 1080], "en"),
        ([844, 390], "en"),
        ([390, 844], "en"),
        ([1280, 720], "ja"),
    ] {
        assert!(ddi_l10n::force(lang));
        let mut sc = room3(14, size, &mine, &theirs);
        add_three(&mut sc);
        shot3(&mut pictures, &mut sc.people[0].ui, size, &format!("b3-room-size-host-room-{}x{}-{lang}", size[0], size[1]));
        shot3(&mut pictures, &mut sc.people[1].ui, size, &format!("b3-room-size-member-away-{}x{}-{lang}", size[0], size[1]));
    }
    assert!(ddi_l10n::force("en"));
    // Every icon, in the column, at its size and drawn at 4×.
    set_column_width(210.0);
    set_staged_icons(Some(PlayerIcon::ALL.to_vec()));
    {
        use ddi_ui::screens::select::{PlayerMark, set_staged_marks};
        set_staged_marks(Some(vec![
            Some(PlayerMark::Chooser),
            Some(PlayerMark::Host),
            Some(PlayerMark::CoHost),
            Some(PlayerMark::Bot),
            Some(PlayerMark::Level(1)),
            Some(PlayerMark::Level(3)),
            Some(PlayerMark::Level(5)),
            Some(PlayerMark::Level(6)),
            Some(PlayerMark::Level(8)),
            Some(PlayerMark::Level(10)),
            None,
        ]));
    }
    let mut sc = room3(14, s720, &mine, &theirs);
    refill(&mut sc);
    shot3(&mut pictures, &mut sc.people[0].ui, s720, "b3-icons-1x");
    sc.people[0].ui.handle(UiEvent::Resize { size: [1280.0, 720.0], ratio: 4.0 });
    sc.run(0.5);
    shot3(&mut pictures, &mut sc.people[0].ui, [5120, 2880], "b3-icons-4x");
    set_staged_icons(None);
    ddi_ui::screens::select::set_staged_marks(None);
    set_column_width(180.0);
    // CPU practice: the same, the CPUs in the column (pips beside R1's stars).
    for (form, fname) in [(RoomForm::Columns, "cols"), (RoomForm::Beside, "r1")] {
        set_room_form(form);
        let look = look_with(&ROWS56);
        let env = Env::new(look.clone(), Rc::new(|s: &str| s.to_string()));
        env.reduced_motion.set(true);
        let room = ddi_ui::state::room::practice_room(1_000);
        let shell: ddi_ui::screens::room::RoomShell = Rc::new(|_, _| {});
        let mut ui = Ui::new(ddi_ui::screens::design_for(&*look, [1280.0, 720.0]), ddi_ui::TextMode::Own, false);
        ui.set_focus_ring(false);
        ui.push(sample::room_select(&env, room.clone(), mine.clone()).with_room(room.clone(), shell));
        ui.handle(UiEvent::Resize { size: [1280.0, 720.0], ratio: 1.0 });
        for k in 0..60 {
            room.borrow_mut().tick(1_000 + k * 16);
            ui.update(1.0 / 60.0);
        }
        shot3(&mut pictures, &mut ui, s720, &format!("b3-practice-{fname}-1280x720-en"));
    }
    set_room_form(RoomForm::Beside);
    set_select_layout(None);
    common::online::LOOK_METRICS.with(|m| m.borrow_mut().clear());
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}

/// The room's parts filled again on both screens (what a staged change
/// needs to show): the tabs changed there and back.
fn refill(sc: &mut common::online::Scene) {
    for who in 0..2 {
        key3(sc, who, "KeyM", "m");
    }
    sc.run(0.4);
    for who in 0..2 {
        key3(sc, who, "KeyM", "m");
    }
    sc.run(0.6);
}

/// The preview movie as round 3 compares it: a (in the square) and d
/// (the side's backdrop, dimmed, or cheaply softened), for songs with a
/// banner only (the library's, all of them) and a cover (synthetic), with
/// the darkest and the brightest frames; solo and in the room.
#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn round_three_preview() {
    use ddi_ui::screens::select::{PreviewStill, RoomForm, set_preview_still, set_room_form};
    if out_dir().is_none() {
        eprintln!("DDI_SL_SHOTS unset: nothing drawn");
        return;
    }
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let data = study_data();
    let mut pictures = common::gpu::Pictures::new();
    let est = ratings::Source::Estimate;
    set_select_layout(Some(SelectLayout::TwinsTwo));
    let list = chosen_list();
    let stills = std::env::var("DDI_SL_STILLS").ok();
    // (song, frames: name and file)
    let mut cases: Vec<(String, Vec<(String, String)>)> = vec![(
        "sl-both".into(),
        vec![("frame".into(), art("lantern-movie.png"))],
    )];
    if let Some(dir) = &stills {
        for (k, id) in ["song-32", "song-33", "song-34", "song-35"].iter().enumerate() {
            let n = k + 1;
            cases.push((
                id.to_string(),
                ["", "-dark", "-bright"]
                    .iter()
                    .map(|t| (format!("frame{t}"), format!("{dir}/preview-{n}{t}.png")))
                    .collect(),
            ));
        }
    }
    for (song, frames) in &cases {
        set_preview_still(None);
        draw2(&mut pictures, &data, song, est, [1280, 720], "en", list, &format!("b3-video-{song}-0before-1280x720-en"));
        for (fname, file) in frames {
            for mode in ['a', 'd'] {
                set_preview_still(Some(PreviewStill { song: song.clone(), still: file.clone(), mode }));
                draw2(&mut pictures, &data, song, est, [1280, 720], "en", list, &format!("b3-video-{song}-{fname}-{mode}-1280x720-en"));
            }
            // d, cheaply softened (a small copy of the frame scaled up).
            let soft = file.replace(".png", "-soft.png");
            if std::path::Path::new(&soft).exists() {
                set_preview_still(Some(PreviewStill { song: song.clone(), still: soft, mode: 'd' }));
                draw2(&mut pictures, &data, song, est, [1280, 720], "en", list, &format!("b3-video-{song}-{fname}-dsoft-1280x720-en"));
            }
        }
        set_preview_still(None);
    }
    // In the three-column room: alice's first song playing its preview.
    set_room_form(RoomForm::Columns);
    assert!(ddi_l10n::force("en"));
    ratings::set_mode(est, true);
    for (song, frames) in cases.iter().take(2) {
        let mine = library_of(&data, &[song.as_str(), "sl-cover", "into-my-dream"], 0x81_0000);
        let theirs = library_of(&data, &["sl-neither"], 0x83_0000);
        let file = &frames[0].1;
        for mode in [None, Some('a'), Some('d')] {
            set_preview_still(mode.map(|mode| PreviewStill { song: song.clone(), still: file.clone(), mode }));
            let mut sc = room3(14, [1280, 720], &mine, &theirs);
            key3(&mut sc, 0, "KeyM", "m");
            sc.run(1.0);
            let m = mode.map_or("0before".to_string(), |c| c.to_string());
            shot3(&mut pictures, &mut sc.people[0].ui, [1280, 720], &format!("b3-video-room-{song}-{m}-1280x720-en"));
        }
    }
    set_preview_still(None);
    set_room_form(RoomForm::Beside);
    set_select_layout(None);
    common::online::LOOK_METRICS.with(|m| m.borrow_mut().clear());
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}

// Round 4: smaller squares, and the players column after the list.

/// Solo B2's tunables for a song side `side` wide (rows of 56, the list
/// from 48 past the side to round 2's right edge).
fn solo_side(side: f32) -> Vec<(&'static str, f32)> {
    vec![
        ("Select.Wheel.Row", 56.0),
        ("Select.Wheel.Selected", 76.0),
        ("Select.Song.Width", side),
        ("Select.Wheel.X", side + 80.0),
        ("Select.Wheel.Width", 1156.0 - side),
    ]
}

const SQUARES: [f32; 4] = [220.0, 240.0, 256.0, 276.0];

#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn round_four_solo() {
    if out_dir().is_none() {
        eprintln!("DDI_SL_SHOTS unset: nothing drawn");
        return;
    }
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let data = study_data();
    let mut pictures = common::gpu::Pictures::new();
    let est = ratings::Source::Estimate;
    set_select_layout(Some(SelectLayout::TwinsTwo));
    let local: Vec<&str> = ["song-01", "song-05", "song-07", "song-12", "song-19", "song-22", "song-26", "song-29"]
        .into_iter()
        .filter(|id| data.songs.iter().any(|s| s.id == *id))
        .collect();
    for sq in SQUARES {
        let side = 2.0 * sq + 16.0;
        let m = solo_side(side);
        let tag = format!("s{}", sq as u32);
        for (case, song) in [("both", "sl-both"), ("banner", "into-my-dream"), ("long", "sl-long"), ("seven", "sl-seven"), ("neither", "sl-neither")] {
            draw2(&mut pictures, &data, song, est, [1280, 720], "en", &m, &format!("b4-solo-{tag}-{case}-1280x720-en"));
        }
        draw2(&mut pictures, &data, "sl-ja", est, [1280, 720], "ja", &m, &format!("b4-solo-{tag}-ja-1280x720-ja"));
        for size in [[1440u32, 900u32], [1024, 768]] {
            draw2(&mut pictures, &data, "sl-both", est, size, "en", &m, &format!("b4-solo-{tag}-both-{}x{}-en", size[0], size[1]));
        }
        for id in &local {
            draw2(&mut pictures, &data, id, est, [1280, 720], "en", &m, &format!("b4-solo-{tag}-{id}-1280x720-en"));
        }
    }
    set_select_layout(None);
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}

#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn round_four_room() {
    use ddi_ui::screens::select::{PlayerIcon, RoomForm, set_column_width, set_room_form, set_room_side, set_staged_icons};
    let Some(dir) = out_dir() else {
        eprintln!("DDI_SL_SHOTS unset: nothing drawn");
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let all = study_data();
    assert!(ddi_l10n::force("en"));
    ratings::set_mode(ratings::Source::Estimate, true);
    let mine = library_of(&all, &["sl-both", "sl-cover", "into-my-dream", "sl-long"], 0x91_0000);
    let theirs = library_of(&all, &["sl-neither", "sweeteners"], 0x93_0000);
    let mut pictures = common::gpu::Pictures::new();
    set_select_layout(Some(SelectLayout::TwinsTwo));
    set_room_form(RoomForm::Columns);
    // The players column as round 4 recommends it.
    {
        use ddi_ui::screens::select::{ColumnStyle, set_column_style};
        set_column_style(ColumnStyle {
            mono: true,
            accents: true,
            marks_before: true,
            reserved: true,
            two: false,
            crown_color: false,
        });
    }
    let best: f32 = std::env::var("DDI_SL_SQUARE").ok().and_then(|v| v.parse().ok()).unwrap_or(240.0);
    // Each square size, the players at 210 on the left (and 180 once).
    for (pw, sq) in SQUARES.iter().map(|s| (210.0, *s)).chain([(180.0, best)]) {
        set_column_width(pw);
        set_room_side(2.0 * sq + 16.0, false);
        let mut sc = room3(14, [1280, 720], &mine, &theirs);
        add_three(&mut sc);
        shot3(&mut pictures, &mut sc.people[0].ui, [1280, 720], &format!("b4-room-p{}-s{}-host-room-1280x720-en", pw as u32, sq as u32));
    }
    // Both orders at the recommended size, the same moments.
    set_column_width(210.0);
    for (right, order) in [(false, "left"), (true, "right")] {
        set_room_side(2.0 * best + 16.0, right);
        for size in [[1280u32, 720u32], [1440, 900]] {
            let sz = format!("{}x{}", size[0], size[1]);
            let mut sc = room3(14, size, &mine, &theirs);
            add_three(&mut sc);
            shot3(&mut pictures, &mut sc.people[0].ui, size, &format!("b4-order-{order}-1-host-room-{sz}-en"));
            key3(&mut sc, 0, "Enter", "Enter");
            sc.run(1.5);
            set_staged_icons(Some(vec![
                PlayerIcon::Ready,
                PlayerIcon::Loading(0.35),
                PlayerIcon::NotReady,
                PlayerIcon::Ready,
                PlayerIcon::Loading(0.8),
                PlayerIcon::WaitingSong,
                PlayerIcon::Ready,
                PlayerIcon::Away,
            ]));
            refill(&mut sc);
            shot3(&mut pictures, &mut sc.people[0].ui, size, &format!("b4-order-{order}-2-host-filling-{sz}-en"));
            set_staged_icons(Some(vec![
                PlayerIcon::Playing,
                PlayerIcon::Playing,
                PlayerIcon::Out,
                PlayerIcon::FailedPlaying,
                PlayerIcon::Playing,
                PlayerIcon::SatOut,
                PlayerIcon::Watching,
                PlayerIcon::Left,
            ]));
            refill(&mut sc);
            shot3(&mut pictures, &mut sc.people[1].ui, size, &format!("b4-order-{order}-3-member-satout-{sz}-en"));
            set_staged_icons(None);
            // CPU practice.
            let look = look_with(&ROWS56);
            let env = Env::new(look.clone(), Rc::new(|s: &str| s.to_string()));
            env.reduced_motion.set(true);
            let room = ddi_ui::state::room::practice_room(1_000);
            let shell: ddi_ui::screens::room::RoomShell = Rc::new(|_, _| {});
            let css = [size[0] as f32, size[1] as f32];
            let mut ui = Ui::new(ddi_ui::screens::design_for(&*look, css), ddi_ui::TextMode::Own, false);
            ui.set_focus_ring(false);
            ui.push(sample::room_select(&env, room.clone(), mine.clone()).with_room(room.clone(), shell));
            ui.handle(UiEvent::Resize { size: css, ratio: 1.0 });
            for k in 0..60 {
                room.borrow_mut().tick(1_000 + k * 16);
                ui.update(1.0 / 60.0);
            }
            shot3(&mut pictures, &mut ui, size, &format!("b4-order-{order}-4-practice-{sz}-en"));
        }
    }
    // The recommended combination's main cases (DDI_SL_ORDER=right|left).
    let right = std::env::var("DDI_SL_ORDER").map(|v| v == "right").unwrap_or(true);
    set_room_side(2.0 * best + 16.0, right);
    let mut sc = room3(14, [1280, 720], &mine, &theirs);
    key3(&mut sc, 0, "KeyM", "m");
    key3(&mut sc, 1, "KeyM", "m");
    sc.run(1.0);
    shot3(&mut pictures, &mut sc.people[0].ui, [1280, 720], "b4-best-room-1-host-mine-1280x720-en");
    shot3(&mut pictures, &mut sc.people[1].ui, [1280, 720], "b4-best-room-2-member-mine-1280x720-en");
    key3(&mut sc, 0, "KeyM", "m");
    key3(&mut sc, 1, "KeyM", "m");
    sc.run(0.5);
    add_three(&mut sc);
    key3(&mut sc, 0, "KeyM", "m");
    sc.run(0.5);
    key3(&mut sc, 0, "KeyM", "m");
    sc.run(0.5);
    shot3(&mut pictures, &mut sc.people[0].ui, [1280, 720], "b4-best-room-3-host-room-1280x720-en");
    shot3(&mut pictures, &mut sc.people[1].ui, [1280, 720], "b4-best-room-4-member-away-1280x720-en");
    set_room_side(568.0, false);
    ddi_ui::screens::select::set_column_style(ddi_ui::screens::select::ColumnStyle::ROUND_3);
    set_room_form(RoomForm::Beside);
    set_select_layout(None);
    common::online::LOOK_METRICS.with(|m| m.borrow_mut().clear());
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}

/// Round 4's players column styles, by name.
fn column_styles() -> Vec<(&'static str, ddi_ui::screens::select::ColumnStyle)> {
    use ddi_ui::screens::select::ColumnStyle;
    let mono = ColumnStyle {
        mono: true,
        accents: false,
        marks_before: true,
        reserved: true,
        two: false,
        crown_color: false,
    };
    vec![
        ("round3", ColumnStyle::ROUND_3),
        ("mono", mono),
        ("mono-accents", ColumnStyle { accents: true, ..mono }),
        ("mono-gold", ColumnStyle { crown_color: true, ..mono }),
        ("mono-shift", ColumnStyle { reserved: false, ..mono }),
        ("mono-two", ColumnStyle { two: true, ..mono }),
    ]
}

#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn round_four_icons() {
    use ddi_ui::screens::select::{
        PlayerIcon, PlayerMark, RoomForm, set_column_style, set_column_width, set_room_form, set_room_side,
        set_staged_icons, set_staged_marks,
    };
    if out_dir().is_none() {
        eprintln!("DDI_SL_SHOTS unset: nothing drawn");
        return;
    }
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let all = study_data();
    assert!(ddi_l10n::force("en"));
    ratings::set_mode(ratings::Source::Estimate, true);
    let mine = library_of(&all, &["sl-both", "sl-cover", "into-my-dream"], 0xa1_0000);
    let theirs = library_of(&all, &["sl-neither"], 0xa3_0000);
    let mut pictures = common::gpu::Pictures::new();
    set_select_layout(Some(SelectLayout::TwinsTwo));
    set_room_form(RoomForm::Columns);
    set_column_width(210.0);
    set_room_side(496.0, false);
    let marks = vec![
        Some(PlayerMark::Chooser),
        Some(PlayerMark::Host),
        Some(PlayerMark::CoHost),
        Some(PlayerMark::Bot),
        Some(PlayerMark::Level(1)),
        Some(PlayerMark::Level(3)),
        Some(PlayerMark::Level(5)),
        Some(PlayerMark::Level(6)),
        Some(PlayerMark::Level(8)),
        Some(PlayerMark::Level(10)),
        None,
        None,
        None,
        None,
    ];
    // A realistic room of fourteen in the Ready window.
    let mixed = vec![
        PlayerIcon::Ready,
        PlayerIcon::Ready,
        PlayerIcon::NotReady,
        PlayerIcon::Loading(0.45),
        PlayerIcon::Ready,
        PlayerIcon::Ready,
        PlayerIcon::Away,
        PlayerIcon::Ready,
        PlayerIcon::NotReady,
        PlayerIcon::Loading(0.9),
        PlayerIcon::Ready,
        PlayerIcon::LoadFailed,
        PlayerIcon::Ready,
    ];
    let mixed_marks = vec![
        Some(PlayerMark::CoHost),
        None,
        None,
        Some(PlayerMark::Level(4)),
        None,
        None,
        None,
        Some(PlayerMark::Level(9)),
        None,
        None,
        Some(PlayerMark::Bot),
        None,
        None,
        None,
    ];
    for (name, style) in column_styles() {
        set_column_style(style);
        set_staged_icons(Some(PlayerIcon::ALL.to_vec()));
        set_staged_marks(Some(marks.clone()));
        let mut sc = room3(14, [1280, 720], &mine, &theirs);
        refill(&mut sc);
        shot3(&mut pictures, &mut sc.people[0].ui, [1280, 720], &format!("b4-icons-{name}-1x"));
        sc.people[0].ui.handle(UiEvent::Resize { size: [1280.0, 720.0], ratio: 4.0 });
        sc.run(0.5);
        shot3(&mut pictures, &mut sc.people[0].ui, [5120, 2880], &format!("b4-icons-{name}-4x"));
        set_staged_icons(Some(mixed.clone()));
        set_staged_marks(Some(mixed_marks.clone()));
        let mut sc = room3(14, [1280, 720], &mine, &theirs);
        refill(&mut sc);
        shot3(&mut pictures, &mut sc.people[0].ui, [1280, 720], &format!("b4-icons-{name}-room"));
    }
    set_staged_icons(None);
    set_staged_marks(None);
    set_column_style(ddi_ui::screens::select::ColumnStyle::ROUND_3);
    set_room_side(568.0, false);
    set_room_form(RoomForm::Beside);
    set_select_layout(None);
    common::online::LOOK_METRICS.with(|m| m.borrow_mut().clear());
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}

// Round 5: the players as a panel of the frame (foldable), names displaced
// by their marks, Length · Notes · BPM under the title, the radar's labels
// at its edges.

/// A synthetic Single chart: `rows` repeated to `n`, 16 a measure.
fn pattern5(bpm: u32, rows: &[&str], n: usize) -> ddi_chart::Song {
    let mut r: Vec<&str> = rows.iter().cycle().take(n).copied().collect();
    r.resize(n.next_multiple_of(16), "0000");
    let measures: Vec<String> = r.chunks(16).map(|c| c.join("\n")).collect();
    let text = format!("#BPMS:0={bpm};#NOTES:dance-single::Hard:13::{};", measures.join(",\n"));
    ddi_chart::formats::sm::parse_sm(&text).expect("parsed")
}

/// The study's library with three radar cases (a flat octagon, a typical
/// chart, a spiky one) and a song with a BPM range.
fn data5() -> SelectData {
    let mut data = study_data();
    let dream = data.songs.iter().find(|s| s.id == "into-my-dream").cloned().unwrap();
    let only = |id: &str, title: &str, d: ddi_chart::Difficulty| {
        let mut s = dream.clone();
        s.id = id.into();
        s.title = title.into();
        s.pack = Some("Study".into());
        s.charts.retain(|c| c.style != "dance-single" || c.difficulty == d);
        s
    };
    let flat = only("sl-flat", "A flat octagon", ddi_chart::Difficulty::Beginner);
    let typical = only("sl-typical", "A typical chart", ddi_chart::Difficulty::Hard);
    const JACK: [&str; 16] = [
        "1000", "1000", "1000", "1000", "0010", "0010", "0010", "0010", "0100", "0100", "0100", "0100", "0001", "0001",
        "0001", "0001",
    ];
    let song = pattern5(130, &JACK, 480);
    let d = song.chart_digest(0);
    if let Some(e) = ratings::estimate_chart(&song, 0) {
        ratings::remember_estimate(d, e);
    }
    if let Some(v) = ratings::rate_song(&song, &[0], &[]).first().and_then(|r| r.values) {
        ratings::remember(d, v);
    }
    let mut c = ChartEntry::of_song(&song, 0).unwrap();
    c.digest = Some(d);
    let spiky = SongEntry {
        id: "sl-spiky".into(),
        title: "A spiky chart".into(),
        artist: "Synthetic".into(),
        bpm_text: "130".into(),
        length: 60.0,
        pack: Some("Study".into()),
        banner: Some(art("lantern-banner.png")),
        jacket: Some(art("orbit-cover.png")),
        charts: vec![c],
        ..SongEntry::default()
    };
    let mut range = data.songs.iter().find(|s| s.id == "sl-both").cloned().unwrap();
    range.id = "sl-range".into();
    range.title = "Tempo Changes".into();
    range.bpm_text = "100–200".into();
    data.songs.splice(0..0, [flat, typical, spiky, range]);
    data
}

fn r5(fold: f32, rail_icons: bool, leaders: u8) -> ddi_ui::screens::select::Round5 {
    ddi_ui::screens::select::Round5 { on: true, fold, rail_icons, leaders }
}

/// Every radar label and the caption apart from each other and inside the
/// panel, as the platform's text measures them.
fn radar_apart(ui: &Ui, what: &str) {
    let mut boxes: Vec<(String, [f32; 4])> =
        (0..8).filter_map(|k| ui.bounds_of("Radar", &format!("Label{k}")).map(|b| (format!("Label{k}"), b))).collect();
    let n = boxes.len();
    if let Some(c) = ui.bounds_of("Radar", "Caption") {
        boxes.push(("Caption".into(), c));
    }
    let panel = ui.bounds_of("Radar", "Panel").expect("the radar");
    for (i, (a, ra)) in boxes.iter().enumerate() {
        assert!(
            ra[0] >= panel[0] - 0.5 && ra[2] <= panel[2] + 0.5 && ra[1] >= panel[1] - 0.5 && ra[3] <= panel[3] + 0.5,
            "{what}: {a} outside"
        );
        for (b, rb) in &boxes[i + 1..] {
            // The centered caption sits over the shape, apart from labels.
            let apart = ra[2] <= rb[0] || rb[2] <= ra[0] || ra[3] <= rb[1] || rb[3] <= ra[1];
            assert!(apart, "{what}: {a} and {b} overlap ({n} labels)");
        }
    }
}

#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn round_five_solo_and_radar() {
    use ddi_ui::radar::{Caption, set_edge_layout};
    use ddi_ui::screens::select::set_round5;
    let Some(dir) = out_dir() else {
        eprintln!("DDI_SL_SHOTS unset: nothing drawn");
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let data = data5();
    let mut pictures = common::gpu::Pictures::new();
    let est = ratings::Source::Estimate;
    set_select_layout(Some(SelectLayout::TwinsTwo));
    set_round5(r5(0.0, true, 0));
    // The radar before and after, at 240 and 276, three shapes, both
    // captions; MSD's seven once.
    let mut radii = String::new();
    for sq in [240.0f32, 276.0] {
        let m = solo_side(2.0 * sq + 16.0);
        for (mode, cap) in [("before", None), ("corner", Some(Caption::Corner)), ("center", Some(Caption::Center))] {
            set_edge_layout(cap);
            for (case, song, source) in [
                ("flat", "sl-flat", est),
                ("typical", "sl-typical", est),
                ("spiky", "sl-spiky", est),
                ("msd", "sl-typical", ratings::Source::Msd),
            ] {
                let file = format!("b5-radar-{}-{mode}-{case}-1280x720-en", sq as u32);
                if let Some(ui) = draw2(&mut pictures, &data, song, source, [1280, 720], "en", &m, &file)
                    && ui.bounds("Radar").is_some()
                {
                    radar_apart(&ui, &file);
                }
            }
            // The rule's numbers at this size (8 spokes, English).
            use ddi_l10n::msg::select as s;
            let full: Vec<String> = [
                s::radar_stream_full(), s::radar_jumpstream_full(), s::radar_handstream_full(), s::radar_stamina_full(),
                s::radar_jackspeed_full(), s::radar_chordjack_full(), s::radar_technical_full(), s::radar_evasion_full(),
            ].map(String::from).to_vec();
            let short: Vec<String> = [
                s::radar_stream(), s::radar_jumpstream(), s::radar_handstream(), s::radar_stamina(),
                s::radar_jackspeed(), s::radar_chordjack(), s::radar_technical(), s::radar_evasion(),
            ].map(String::from).to_vec();
            let words = ddi_ui::radar::Words { full: &full, short: &short, overall: "DDI" };
            let numbers: Vec<String> = std::iter::repeat_n("11.8".to_string(), 9).collect();
            let p = ddi_ui::radar::place([sq, sq], &words, &numbers);
            radii.push_str(&format!("square {sq}: {mode}: radius {} text {} full {}\n", p.radius, p.text, p.full));
        }
    }
    std::fs::write(dir.join("b5-radar-radii.txt"), &radii).unwrap();
    // The labels apart in Japanese and the long pseudo-language.
    for lang in ["ja", "qps-ploc"] {
        for cap in [Caption::Corner, Caption::Center] {
            set_edge_layout(Some(cap));
            let file = format!("b5-radar-240-{cap:?}-typical-1280x720-{lang}").to_lowercase();
            if let Some(ui) = draw2(&mut pictures, &data, "sl-typical", est, [1280, 720], lang, &solo_side(496.0), &file)
                && ui.bounds("Radar").is_some()
            {
                radar_apart(&ui, &file);
            }
        }
    }
    // Solo's main cases with round 5 (caption in the corner, the base).
    set_edge_layout(Some(Caption::Corner));
    let m = solo_side(496.0);
    for (case, song) in [
        ("both", "sl-both"),
        ("banner", "into-my-dream"),
        ("neither", "sl-neither"),
        ("long", "sl-long"),
        ("seven", "sl-seven"),
        ("range", "sl-range"),
        ("bundled2", "sweeteners"),
    ] {
        draw2(&mut pictures, &data, song, est, [1280, 720], "en", &m, &format!("b5-solo-{case}-1280x720-en"));
    }
    for (case, song) in [("ja", "sl-ja"), ("banner", "into-my-dream"), ("range", "sl-range")] {
        draw2(&mut pictures, &data, song, est, [1280, 720], "ja", &m, &format!("b5-solo-{case}-1280x720-ja"));
    }
    draw2(&mut pictures, &data, "sl-both", est, [1440, 900], "en", &m, "b5-solo-both-1440x900-en");
    set_edge_layout(None);
    set_round5(ddi_ui::screens::select::Round5::OFF);
    set_select_layout(None);
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}

#[test]
#[ignore = "the select-layout study's pictures: run by hand"]
fn round_five_room() {
    use ddi_ui::radar::{Caption, set_edge_layout};
    use ddi_ui::screens::select::{
        ColumnStyle, PlayerIcon, PlayerMark, RoomForm, Round5, set_column_style, set_column_width, set_room_form,
        set_room_side, set_round5, set_staged_icons, set_staged_marks, set_staged_places,
    };
    let Some(dir) = out_dir() else {
        eprintln!("DDI_SL_SHOTS unset: nothing drawn");
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let all = data5();
    assert!(ddi_l10n::force("en"));
    ratings::set_mode(ratings::Source::Estimate, true);
    let mine = library_of(&all, &["sl-both", "sl-cover", "into-my-dream", "sl-long"], 0xb1_0000);
    let theirs = library_of(&all, &["sl-neither", "sweeteners"], 0xb3_0000);
    let mut pictures = common::gpu::Pictures::new();
    set_select_layout(Some(SelectLayout::TwinsTwo));
    set_room_form(RoomForm::Columns);
    set_column_width(210.0);
    set_room_side(496.0, true);
    set_edge_layout(Some(Caption::Corner));
    let shifting = |accents: bool| ColumnStyle {
        mono: true,
        accents,
        marks_before: true,
        reserved: false,
        two: true,
        crown_color: false,
    };
    set_column_style(shifting(true));
    let filling = vec![
        PlayerIcon::Ready,
        PlayerIcon::Loading(0.35),
        PlayerIcon::NotReady,
        PlayerIcon::Ready,
        PlayerIcon::Loading(0.8),
        PlayerIcon::WaitingSong,
        PlayerIcon::Ready,
        PlayerIcon::Away,
    ];
    // Open, folding, folded (icons), folded (the count alone): host and
    // member, 1280×720 and 1440×900.
    for size in [[1280u32, 720u32], [1440, 900]] {
        let sz = format!("{}x{}", size[0], size[1]);
        for (fold, icons, name) in [(0.0, true, "open"), (0.5, true, "folding"), (1.0, true, "rail"), (1.0, false, "rail-count")] {
            set_round5(r5(fold, icons, 0));
            set_staged_icons(None);
            let mut sc = room3(14, size, &mine, &theirs);
            add_three(&mut sc);
            shot3(&mut pictures, &mut sc.people[0].ui, size, &format!("b5-panel-{name}-1-host-room-{sz}-en"));
            shot3(&mut pictures, &mut sc.people[1].ui, size, &format!("b5-panel-{name}-2-member-away-{sz}-en"));
            key3(&mut sc, 0, "Enter", "Enter");
            sc.run(1.5);
            set_staged_icons(Some(filling.clone()));
            refill(&mut sc);
            shot3(&mut pictures, &mut sc.people[0].ui, size, &format!("b5-panel-{name}-3-host-filling-{sz}-en"));
            set_staged_icons(None);
        }
        // CPU practice, open and folded.
        for (fold, name) in [(0.0, "open"), (1.0, "rail")] {
            set_round5(r5(fold, true, 0));
            let look = look_with(&ROWS56);
            let env = Env::new(look.clone(), Rc::new(|s: &str| s.to_string()));
            env.reduced_motion.set(true);
            let room = ddi_ui::state::room::practice_room(1_000);
            let shell: ddi_ui::screens::room::RoomShell = Rc::new(|_, _| {});
            let css = [size[0] as f32, size[1] as f32];
            let mut ui = Ui::new(ddi_ui::screens::design_for(&*look, css), ddi_ui::TextMode::Own, false);
            ui.set_focus_ring(false);
            ui.push(sample::room_select(&env, room.clone(), mine.clone()).with_room(room.clone(), shell));
            ui.handle(UiEvent::Resize { size: css, ratio: 1.0 });
            for k in 0..60 {
                room.borrow_mut().tick(1_000 + k * 16);
                ui.update(1.0 / 60.0);
            }
            shot3(&mut pictures, &mut ui, size, &format!("b5-panel-{name}-4-practice-{sz}-en"));
        }
    }
    // The column of fourteen: names displaced by their marks, with and
    // without the two accents; the places at the right or the leaders
    // marked (the match under way: staged).
    let marks = vec![
        Some(PlayerMark::Chooser),
        Some(PlayerMark::CoHost),
        None,
        Some(PlayerMark::Level(4)),
        None,
        None,
        None,
        Some(PlayerMark::Level(9)),
        None,
        None,
        Some(PlayerMark::Bot),
        None,
        None,
        None,
    ];
    let mixed = vec![
        PlayerIcon::Ready,
        PlayerIcon::Ready,
        PlayerIcon::NotReady,
        PlayerIcon::Loading(0.45),
        PlayerIcon::Ready,
        PlayerIcon::Ready,
        PlayerIcon::Away,
        PlayerIcon::Ready,
        PlayerIcon::NotReady,
        PlayerIcon::Loading(0.9),
        PlayerIcon::Ready,
        PlayerIcon::LoadFailed,
        PlayerIcon::Ready,
    ];
    let places: Vec<Option<u16>> = [4, 1, 7, 2, 9, 3, 12, 5, 11, 6, 13, 8, 10, 14].map(Some).to_vec();
    for accents in [true, false] {
        set_column_style(shifting(accents));
        for leaders in [0u8, 1, 2] {
            set_round5(r5(0.0, true, leaders));
            set_staged_icons(Some(mixed.clone()));
            set_staged_marks(Some(marks.clone()));
            set_staged_places(Some(places.clone()));
            let mut sc = room3(14, [1280, 720], &mine, &theirs);
            refill(&mut sc);
            let name = format!("b5-column-{}-places{leaders}", if accents { "accents" } else { "mono" });
            shot3(&mut pictures, &mut sc.people[0].ui, [1280, 720], &name);
            sc.people[0].ui.handle(UiEvent::Resize { size: [1280.0, 720.0], ratio: 3.0 });
            sc.run(0.5);
            shot3(&mut pictures, &mut sc.people[0].ui, [3840, 2160], &format!("{name}-3x"));
        }
    }
    set_staged_places(None);
    set_staged_marks(None);
    set_staged_icons(None);
    // The whole together: the room's main moments and Japanese.
    set_column_style(shifting(true));
    set_round5(r5(0.0, true, 1));
    let mut sc = room3(14, [1280, 720], &mine, &theirs);
    key3(&mut sc, 0, "KeyM", "m");
    key3(&mut sc, 1, "KeyM", "m");
    sc.run(1.0);
    shot3(&mut pictures, &mut sc.people[0].ui, [1280, 720], "b5-best-1-host-mine-1280x720-en");
    shot3(&mut pictures, &mut sc.people[1].ui, [1280, 720], "b5-best-2-member-mine-1280x720-en");
    assert!(ddi_l10n::force("ja"));
    let mut sc = room3(14, [1280, 720], &mine, &theirs);
    add_three(&mut sc);
    shot3(&mut pictures, &mut sc.people[0].ui, [1280, 720], "b5-best-3-host-room-1280x720-ja");
    assert!(ddi_l10n::force("en"));
    set_room_side(568.0, false);
    set_column_style(ColumnStyle::ROUND_3);
    set_round5(Round5::OFF);
    set_edge_layout(None);
    set_room_form(RoomForm::Beside);
    set_select_layout(None);
    common::online::LOOK_METRICS.with(|m| m.borrow_mut().clear());
    ratings::set_mode(ratings::Source::Song, false);
    ddi_l10n::unforce();
}
