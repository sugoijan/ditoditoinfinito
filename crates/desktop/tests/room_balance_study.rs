//! Round DC's study (`docs/plans/song-navigation.md`, "In a room"): a
//! room's song select drawn offscreen with each balance between its song's
//! side, its list and its players panel (`ddi_ui::screens::select::
//! ROOM_BALANCES`, picked by the testing hook `study_room_balance`), over
//! the room model's fake room: its owner's Room songs with a short, a long
//! and a very long title, a room of three and of twelve people with long
//! names, the panel open and folded, at 1280×720, 1024×768, 1440×900 and
//! 1920×1080 and a phone held sideways, in English and Japanese; and
//! singleplayer's song select at the same sizes, for the rows' spacing.
//! Ignored: `DDI_DC_STUDY=<dir> cargo test -p ddi-desktop --test
//! room_balance_study -- --ignored` writes the pictures there
//! (`room-<balance>-<w>x<h>-<lang>-p<n>-<open|folded>-<title>.png`,
//! `solo-<w>x<h>-<lang>-<title>.png`); `DDI_DC_TAG=<tag>` puts a tag
//! before each name (`before`, `after`).

mod common;
use std::rc::Rc;

use common::online::{ROOM, Scene, Songs, library, person};
use ddi_room::fake::FakeServer;
use ddi_room::{Identity, Intent, ManualClock, RoomPolicy};
use ddi_ui::screens::select::{ROOM_BALANCES, SelectScreen, study_room_balance};
use ddi_ui::screens::{Env, sample};
use ddi_ui::{Ui, UiEvent};

/// The people of a room of twelve beyond its owner, long names first.
const NAMES: [&str; 11] = [
    "Bartholomew Featherstonehaugh",
    "ひまわり畑のミルクティー",
    "Xiomara Quintanilla-Vasquez",
    "Konstantinos Papadopoulos",
    "MidnightMetronomeMaster",
    "ana",
    "Wolfgang Amadeus Groovemaker",
    "星空ステップ研究会",
    "TheRealSlimShadyStepper99",
    "bo",
    "Guinevere Oyelaran-Whitlock",
];

/// The selected song's titles: short, long (the one round SF's picture
/// cut to "Som…"), very long; the very long one in Japanese in a
/// Japanese run.
fn titles(lang: &str) -> [(&'static str, &'static str); 3] {
    [
        ("short", "Lumen"),
        ("long", "Some Things Must"),
        (
            "verylong",
            if lang == "ja" {
                "夜が明けるまで踊り続けるための長い長い歌"
            } else {
                "The Long Way Home Through Every Single Night (Extended Mix)"
            },
        ),
    ]
}

/// Every chart of `songs` rated: its MSD and its DDI estimate with a
/// skill profile (the radar's labels at their fullest), the first song
/// given eight Single charts (the chart row's fixed places at their
/// narrowest).
fn rate(songs: &mut [ddi_ui::screens::data::SongEntry], salt: u128) {
    use ddi_library::manifest::{ChartEstimate, ProfileSpoke};
    use ddi_ui::state::ratings;
    if let Some(first) = songs.first_mut() {
        let single: Vec<_> = first.charts.iter().filter(|c| c.style == "dance-single").cloned().collect();
        let more = 8usize.saturating_sub(single.len());
        let from = first.charts.len();
        for (k, base) in single.iter().cycle().take(more).enumerate() {
            let mut c = base.clone();
            c.index = from + k;
            c.difficulty = ddi_ui::screens::data::Difficulty::Edit;
            c.meter = 13 + k as u32;
            c.name = format!("Edit {}", k + 1);
            c.digest = Some(ddi_chart::ChartDigest(salt + 0xE000 + k as u128));
            first.charts.push(c);
        }
    }
    for song in songs.iter() {
        for c in &song.charts {
            let Some(d) = c.digest else { continue };
            let m = c.meter as f32;
            ratings::remember(d, [m + 0.4, m + 0.1, m - 0.8, m - 2.3, m - 0.2, m - 3.1, m - 4.0, m + 0.6]);
            ratings::remember_estimate(
                d,
                ChartEstimate {
                    decimal: m + 0.37,
                    level: c.meter as u8,
                    scalar: true,
                    studied: true,
                    profile: Some([4.1, 2.6, 1.2, 3.3, 0.8, 2.0, 1.5, 0.6].map(ProfileSpoke::Numeric)),
                },
            );
        }
    }
}

fn dir() -> Option<String> {
    std::env::var("DDI_DC_STUDY").ok()
}

fn save(name: &str, pixels: &[u8], size: [u32; 2]) {
    let Some(dir) = dir() else { return };
    let tag = std::env::var("DDI_DC_TAG").map(|t| format!("{t}-")).unwrap_or_default();
    let path = std::path::Path::new(&dir).join(format!("{tag}{name}.png"));
    std::fs::create_dir_all(&dir).unwrap();
    image::save_buffer(path, pixels, size[0], size[1], image::ColorType::Rgba8).unwrap();
}

fn key(ui: &mut Ui, code: &str, k: &str) {
    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::down(code, k)));
    ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::up(code, k)));
}

/// A key pressed on the owner's song select, then `seconds` pass.
fn press(scene: &mut Scene, code: &str, k: &str, seconds: f32) {
    key(&mut scene.people[0].ui, code, k);
    scene.run(seconds);
}

/// Which of the titles the selected song has.
fn which(env: &Env, lang: &str) -> &'static str {
    let song = env.state.borrow().song.clone();
    titles(lang)
        .iter()
        .find(|(_, t)| *t == song)
        .map_or("other", |(k, _)| k)
}

#[test]
#[ignore]
fn room_balance_study() {
    if dir().is_none() {
        return;
    }
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    // The DDI estimate's radar (the default source), or MSD's with
    // `DDI_DC_RATINGS=msd`.
    let source = match std::env::var("DDI_DC_RATINGS").as_deref() {
        Ok("msd") => ddi_ui::state::ratings::Source::Msd,
        _ => ddi_ui::state::ratings::Source::Estimate,
    };
    ddi_ui::state::ratings::set_mode(source);
    let mut pictures = common::gpu::Pictures::new();
    let sizes: [[u32; 2]; 5] = [[1280, 720], [1024, 768], [1440, 900], [1920, 1080], [844, 390]];
    for size in sizes {
        for lang in ["en", "ja"] {
            let full = size == [1280, 720];
            let phone = size[1] < 480;
            if phone && lang == "ja" {
                continue;
            }
            for people in [3usize, 12] {
                if !full && people == 3 {
                    continue;
                }
                assert!(ddi_l10n::set_language(lang));
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
                for (i, name) in NAMES.iter().take(people - 1).enumerate() {
                    server
                        .borrow_mut()
                        .join(Identity::new(format!("dev:p{i}")), name, Intent::Play)
                        .unwrap();
                }
                // Her library: the three songs alone, retitled.
                let mut a = library(0, 0x1000);
                a.songs.truncate(3);
                for (s, (_, t)) in a.songs.iter_mut().zip(titles(lang)) {
                    s.title = t.into();
                    s.pack = Some("Alice's Pack".into());
                }
                rate(&mut a.songs, 0x1000);
                let all: Songs = Rc::new(a.songs.clone());
                let mut scene = Scene {
                    clock,
                    server: server.clone(),
                    people: vec![person(&server, "alice", a, &all, css)],
                };
                scene.run(5.0);
                // Her three songs added from My songs, then Room songs.
                press(&mut scene, "KeyM", "m", 0.5);
                for _ in 0..3 {
                    press(&mut scene, "Enter", "Enter", 0.6);
                    press(&mut scene, "ArrowDown", "ArrowDown", 0.6);
                }
                scene.run(2.0);
                press(&mut scene, "KeyM", "m", 1.0);
                let env = scene.people[0].env.clone();
                assert_eq!(env.state.borrow().room.as_ref().map(|r| r.songs.len()), Some(3));
                for balance in ROOM_BALANCES {
                    study_room_balance(Some(balance));
                    for folded in [false, true] {
                        if folded && !full {
                            continue;
                        }
                        // Laid out again (a resize to the same size is
                        // nothing: one a pixel smaller first).
                        let ui = &mut scene.people[0].ui;
                        ui.handle(UiEvent::Resize { size: [css[0] - 1.0, css[1]], ratio: 1.0 });
                        ui.update(1.0 / 60.0);
                        ui.handle(UiEvent::Resize { size: css, ratio: 1.0 });
                        if folded {
                            key(ui, "KeyL", "l");
                        }
                        scene.run(0.6);
                        // Each song from the top (a key a second apart:
                        // two in a moment are the chart gesture).
                        for _ in 0..4 {
                            press(&mut scene, "ArrowUp", "ArrowUp", 1.0);
                        }
                        let mut seen: Vec<&str> = Vec::new();
                        for k in 0..6 {
                            if k > 0 {
                                press(&mut scene, "ArrowDown", "ArrowDown", 1.0);
                            }
                            let title = which(&env, lang);
                            if seen.contains(&title) {
                                continue;
                            }
                            seen.push(title);
                            if phone && title != "long" {
                                continue;
                            }
                            let ui = &mut scene.people[0].ui;
                            ui.forget_metrics();
                            if let Some(p) = pictures.draw(ui, size) {
                                let fold = if folded { "folded" } else { "open" };
                                save(
                                    &format!(
                                        "room-{}-{}x{}-{lang}-p{people}-{fold}-{title}",
                                        balance.id, size[0], size[1]
                                    ),
                                    &p,
                                    size,
                                );
                            }
                        }
                        if folded {
                            let ui = &mut scene.people[0].ui;
                            key(ui, "KeyL", "l");
                            scene.run(0.6);
                        }
                    }
                }
                study_room_balance(None);
            }
            // Singleplayer's song select (the sample library, three songs
            // retitled, each selected in turn): the rows' spacing.
            for (k, (what, _)) in titles(lang).iter().enumerate() {
                if phone && *what != "long" {
                    continue;
                }
                let css = [size[0] as f32, size[1] as f32];
                let look = Rc::new(ddi_theme::Infinito::new());
                let env = Env::new(look.clone(), Rc::new(|s: &str| s.to_string()));
                env.reduced_motion.set(true);
                let mut ui = Ui::new(ddi_ui::screens::design_for(&*look, css), ddi_ui::TextMode::Own, false);
                ui.set_focus_ring(false);
                let mut data = sample::select();
                for (s, (_, t)) in data.songs.iter_mut().zip(titles(lang)) {
                    s.title = t.into();
                }
                for (i, song) in data.songs.iter_mut().enumerate() {
                    for (k, c) in song.charts.iter_mut().enumerate() {
                        c.digest = Some(ddi_chart::ChartDigest(0x5000 + (i as u128) * 64 + k as u128));
                    }
                }
                rate(&mut data.songs, 0x5000);
                let id = data.songs[k].id.clone();
                ui.push(SelectScreen::new(
                    env.clone(),
                    data,
                    Some(&id),
                    Rc::new(|_, _, _, _| {}),
                    Rc::new(|_, _| {}),
                ));
                ui.handle(UiEvent::Resize { size: css, ratio: 1.0 });
                common::ui::frames(&mut ui, 1.5);
                ui.forget_metrics();
                if let Some(p) = pictures.draw(&mut ui, size) {
                    save(&format!("solo-{}x{}-{lang}-{what}", size[0], size[1]), &p, size);
                }
            }
        }
    }
    ddi_l10n::set_language("en");
}

