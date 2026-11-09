//! Sample data for the theme's screens on the developer routes
//! (`#/ui-dev?screen=select|results|options`, the desktop's `--screen`):
//! the four bundled songs with their real charts, then made-up packs,
//! some with pump charts only (so they grey out in the dance styles) and
//! some with Japanese titles. The results are a made-up ITG play.

use std::rc::Rc;

use ddi_chart::Difficulty;

use super::Env;
use super::data::{
    Best, ChartEntry, Hit, JudgementCount, OptionsData, ResultsData, SelectData, SongEntry,
    SongOffset, StyleDef, Window,
};
use super::options::{ChangeFn, OptionsScreen};
use super::results::{ResultsActions, ResultsScreen};
use super::select::{OpenFn, PlayFn, SelectScreen};
use crate::screen::Cx;

/// The bundled songs: id, title, subtitle, artist, BPM, length (s),
/// banner, background, charts (style, difficulty, meter, step artist,
/// notes), as `songs/index.json` lists them.
type Bundled = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    f32,
    f32,
    &'static str,
    Option<&'static str>,
    &'static [(&'static str, Difficulty, u32, &'static str, u32)],
);

use Difficulty::{Beginner as B, Challenge as C, Easy as E, Hard as H, Medium as M};

const BUNDLED: [Bundled; 4] = [
    (
        "beatucada",
        "Beatucada",
        "",
        "Kurio Prokos",
        122.0,
        108.0,
        "Banner.png",
        Some("Background.png"),
        &[
            ("dance-single", B, 1, "Daniel Rotwind", 38),
            ("dance-single", E, 3, "Timo Kitsune", 112),
            ("dance-single", M, 7, "Timo Kitsune", 217),
            ("dance-single", H, 11, "Chriszo", 283),
            ("dance-single", C, 11, "Timo Kitsune", 276),
            ("dance-double", B, 2, "Marukomuru", 77),
            ("pump-single", B, 2, "DJGRS", 155),
            ("pump-single", M, 10, "DJGRS", 317),
            ("pump-halfdouble", H, 12, "DJGRS", 311),
            ("pump-double", E, 6, "DJGRS", 156),
        ],
    ),
    (
        "into-my-dream",
        "Into My Dream",
        "",
        "Lagoona",
        140.0,
        147.0,
        "Into My Dream-bn.png",
        None,
        &[
            ("dance-single", B, 1, "48productions", 50),
            ("dance-single", B, 2, "Daniel Rotwind", 65),
            ("dance-single", E, 3, "48productions", 86),
            ("dance-single", M, 8, "48productions", 280),
            ("dance-single", H, 12, "-YOSEFU-", 507),
            ("dance-single", C, 15, "-YOSEFU- in a Dream", 726),
            ("dance-double", B, 1, "Marukomuru", 66),
            ("pump-single", B, 3, "Chriszo", 112),
            ("pump-single", E, 5, "Chriszo", 164),
            ("pump-single", M, 8, "Chriszo", 379),
        ],
    ),
    (
        "some-things-must",
        "Some Things Must",
        "(OutFox Edit)",
        "Sevish",
        160.0,
        146.0,
        "sevish-bn.png",
        Some("sevish-bg.png"),
        &[
            ("dance-single", B, 2, "Daniel Rotwind", 85),
            ("dance-single", E, 9, "Sevish", 314),
            ("dance-single", M, 10, "-YOSEFU-", 377),
            ("dance-single", H, 13, "-YOSEFU-", 595),
            ("dance-single", C, 16, "Sudospective", 831),
            ("dance-double", B, 2, "Marukomuru", 70),
            ("pump-single", B, 1, "Kaede", 82),
            ("pump-single", E, 4, "Kaede", 258),
            ("pump-single", M, 8, "Kaede", 338),
            ("pump-single", M, 10, "Kaede", 570),
            ("pump-single", H, 16, "Kaede", 800),
            ("pump-single", C, 19, "Kaede", 1134),
            ("pump-double", M, 11, "Kaede", 570),
            ("pump-double", H, 17, "Ario", 801),
        ],
    ),
    (
        "sweeteners",
        "Sweeteners",
        "",
        "Jack5",
        153.0,
        157.0,
        "banner.jpg",
        Some("background.jpg"),
        &[
            ("dance-single", B, 3, "Daniel Rotwind", 105),
            ("dance-single", M, 10, "Jack5 (edit DanielRotwind)", 329),
            ("dance-single", H, 13, "Jack5 (edit DanielRotwind)", 424),
            ("dance-double", B, 1, "Marukomuru", 81),
        ],
    ),
];

/// The styles, as `ddi_chart`'s built-in layouts name them.
pub fn styles() -> Vec<StyleDef> {
    ddi_chart::layout::Layout::builtins()
        .into_iter()
        .map(|l| StyleDef {
            id: l.id,
            name: l.name,
        })
        .collect()
}

/// The bundled songs.
pub fn bundled() -> Vec<SongEntry> {
    BUNDLED
        .iter()
        .map(
            |(id, title, subtitle, artist, bpm, length, banner, background, charts)| SongEntry {
                id: id.to_string(),
                title: title.to_string(),
                subtitle: subtitle.to_string(),
                artist: artist.to_string(),
                pack: None,
                bpm_text: format!("{bpm}"),
                length: *length,
                banner: Some(format!("songs/{id}/{banner}")),
                background: background.map(|b| format!("songs/{id}/{b}")),
                jacket: None,
                cd_title: None,
                removable: false,
                offset_ms: 0,
                title_translit: String::new(),
                artist_translit: String::new(),
                credit: String::new(),
                charts: charts
                    .iter()
                    .enumerate()
                    .map(
                        |(index, (style, difficulty, meter, credit, notes))| ChartEntry {
                            index,
                            style: style.to_string(),
                            difficulty: *difficulty,
                            meter: *meter,
                            notes: *notes,
                            credit: credit.to_string(),
                            ..Default::default()
                        },
                    )
                    .collect(),
            },
        )
        .collect()
}

/// A small deterministic generator (xorshift), in `0..1`.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    fn below(&mut self, n: usize) -> usize {
        ((self.next() * n as f32) as usize).min(n.saturating_sub(1))
    }

    /// About a standard normal (the sum of twelve uniforms).
    fn normal(&mut self) -> f32 {
        (0..12).map(|_| self.next()).sum::<f32>() - 6.0
    }
}

const WORDS: [&str; 16] = [
    "Neon", "Drift", "Signal", "Paper", "Comet", "Velvet", "Orbit", "Harbor", "Lantern", "Static",
    "Bloom", "Cascade", "Ember", "Glass", "Tide", "Summit",
];
const JAPANESE: [&str; 8] = [
    "夏祭り",
    "ほしぞらの歌",
    "東京ナイト",
    "さくら色",
    "花火",
    "ゆめの続き",
    "カナリア",
    "雨上がり",
];
const ARTISTS: [&str; 8] = [
    "Kana Lab",
    "おにぎり",
    "The Arrows",
    "Rinne",
    "Café Sound",
    "ジャン",
    "Leafy",
    "Okami",
];

/// Made-up packs: (name, songs, styles their charts are in, Japanese
/// titles).
const PACKS: [(&str, usize, &[&str], bool); 5] = [
    ("Arrow Basics", 9, &["dance-single", "dance-double"], false),
    ("Pump Station", 7, &["pump-single", "pump-double"], false),
    ("夏のパック", 8, &["dance-single", "pump-single"], true),
    (
        "Long Night",
        12,
        &["dance-single", "dance-double", "pump-halfdouble"],
        false,
    ),
    ("Only Pump Doubles", 5, &["pump-double"], false),
];

/// Song select's sample: the bundled songs, then the made-up packs.
pub fn select() -> SelectData {
    let mut songs = bundled();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut n = 0usize;
    for (pack, count, pack_styles, japanese) in PACKS {
        for i in 0..count {
            let title = if japanese && i % 3 != 2 {
                JAPANESE[i % JAPANESE.len()].to_string()
            } else {
                format!(
                    "{} {}",
                    WORDS[rng.below(WORDS.len())],
                    WORDS[rng.below(WORDS.len())]
                )
            };
            let bpm = (90 + rng.below(110)) as f32;
            let mut charts = Vec::new();
            for style in pack_styles {
                // Three to five charts a style, rising.
                let k = 3 + rng.below(3);
                let start = rng.below(2);
                for (j, d) in [B, E, M, H, C].into_iter().skip(start).take(k).enumerate() {
                    let meter = 1 + j as u32 * 3 + rng.below(3) as u32;
                    charts.push(ChartEntry {
                        style: style.to_string(),
                        difficulty: d,
                        meter,
                        notes: 80 + meter * 45 + rng.below(40) as u32,
                        credit: ARTISTS[rng.below(ARTISTS.len())].to_string(),
                        ..Default::default()
                    });
                }
            }
            // Made up, so no art: a bundled song's banner on a made-up
            // title reads as a mislabelled song. The placeholder shows.
            songs.push(SongEntry {
                id: format!("sample-{n}"),
                title,
                subtitle: String::new(),
                artist: ARTISTS[rng.below(ARTISTS.len())].to_string(),
                pack: Some(pack.to_string()),
                bpm_text: format!("{bpm}"),
                length: 90.0 + rng.below(90) as f32,
                banner: None,
                background: None,
                charts,
                ..Default::default()
            });
            n += 1;
        }
    }
    let all = styles();
    let styles = all
        .into_iter()
        .filter(|s| {
            songs
                .iter()
                .any(|song| song.charts.iter().any(|c| c.style == s.id))
        })
        .collect();
    // A few bests, to show them.
    for (song, difficulty, best) in [
        (
            "some-things-must",
            Difficulty::Hard,
            sample_best("AA", "96.12%", "FullCombo", "FC"),
        ),
        (
            "some-things-must",
            Difficulty::Medium,
            sample_best("★★", "99.20%", "PerfectFullCombo", "PFC"),
        ),
        (
            "sweeteners",
            Difficulty::Hard,
            sample_best("C", "71.03%", "Failed", "FAILED"),
        ),
    ] {
        if let Some(c) = songs.iter_mut().find(|s| s.id == song).and_then(|s| {
            s.charts
                .iter_mut()
                .find(|c| c.style == "dance-single" && c.difficulty == difficulty)
        }) {
            c.best = Some(best);
        }
    }
    SelectData {
        styles,
        songs,
        // A speed, a mod and a ruleset by its name.
        mods: vec![
            "×2.5".into(),
            ddi_l10n::msg::mods::reverse().into(),
            ddi_l10n::UiText::data(SAMPLE_RULESET).into(),
        ],
        preferred: Some(Difficulty::Hard),
        ..Default::default()
    }
}

fn sample_best(grade: &str, score: &str, lamp: &str, lamp_text: &str) -> Best {
    Best {
        grade: grade.into(),
        score: score.into(),
        lamp: lamp.into(),
        lamp_text: lamp_text.into(),
        pending: false,
        plays: 3,
        play: String::new(),
        last_played: 0.0,
    }
}

/// The ruleset the samples are played by, by its name (data).
const SAMPLE_RULESET: &str = "ITG";

/// Results' sample: Some Things Must on Hard, an ITG play.
pub fn results() -> ResultsData {
    let song = bundled()
        .into_iter()
        .find(|s| s.id == "some-things-must")
        .unwrap_or_default();
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let length = song.length;
    // 282 hits about −2.1 ms with σ 9.8, one miss at 95 s; on the beat,
    // 8ths and 16ths, a few 12ths and 24ths.
    let beat = |i: usize| {
        use ddi_chart::Quantization::*;
        Some(match (i % 8, i % 24) {
            (0 | 4, _) => N4,
            (2 | 6, _) => N8,
            (_, 7) => N12,
            (_, 15) => N24,
            _ => N16,
        })
    };
    let mut hits: Vec<Hit> = (0..282)
        .map(|i| Hit {
            time: 4.0 + (length - 10.0) * i as f32 / 282.0,
            offset: Some(-2.1 + rng.normal() * 9.8),
            lane: rng.below(4) as u8,
            beat: beat(i),
        })
        .collect();
    hits.push(Hit {
        time: 95.0,
        offset: None,
        lane: 2,
        beat: beat(1),
    });
    hits.sort_by(|a, b| a.time.total_cmp(&b.time));
    // Life: rising, a dip to 71 % around the miss, recovering.
    let life = (0..=146)
        .map(|t| {
            let t = t as f32;
            let base = (0.75 + t * 0.02).min(1.0);
            let dip = if (95.0..110.0).contains(&t) {
                0.29 * (1.0 - (t - 95.0) / 15.0)
            } else {
                0.0
            };
            (t, (base - dip).clamp(0.0, 1.0))
        })
        .collect();
    // The table and fast/slow counted from the same hits, by ITG's windows.
    let windows = [21.5, 43.0, 102.0, 135.0, 180.0];
    let mut counts = [0u32; 6];
    let (mut fast, mut slow) = (0, 0);
    for h in &hits {
        let tier = match h.offset {
            Some(o) => windows.iter().position(|&w| o.abs() <= w).unwrap_or(5),
            None => 5,
        };
        counts[tier] += 1;
        if let (Some(o), 1..=4) = (h.offset, tier) {
            if o < 0.0 { fast += 1 } else { slow += 1 }
        }
    }
    // Names as the ruleset and the chart give them: data.
    let data = |s: &str| String::from(ddi_l10n::UiText::data(s));
    let j = |name: &str, color: &str, count: u32| JudgementCount {
        name: data(name),
        color: color.into(),
        count,
    };
    use ddi_l10n::msg::results as rm;
    ResultsData {
        line: format!(
            "{} · {} · {}",
            data("Hard 13"),
            super::data::style_name("dance-single", "Single"),
            data(SAMPLE_RULESET)
        ),
        mods: format!("×2.5 · {}", ddi_l10n::msg::mods::reverse()),
        grade: "★".into(),
        score: "96.77%".into(),
        ex: Some(data("EX 1680 / 1785")),
        failed: false,
        quit: false,
        lamp: None,
        judgements: vec![
            j("Fantastic", "Judgement.W1", counts[0]),
            j("Excellent", "Judgement.W2", counts[1]),
            j("Great", "Judgement.W3", counts[2]),
            j("Decent", "Judgement.W4", counts[3]),
            j("Way Off", "Judgement.W5", counts[4]),
            j("Miss", "Judgement.Miss", counts[5]),
        ],
        stats: vec![
            (data("Held / Let Go"), "13 / 1".into()),
            (rm::mines_hit().into(), "0".into()),
            (rm::max_combo().into(), "187".into()),
            (rm::fast_slow().into(), format!("{fast} / {slow}")),
            (rm::mean_offset().into(), data("−2.1 ms (σ 9.8)")),
        ],
        notes: vec![rm::sample_note().into()],
        hits,
        windows: windows
            .iter()
            .enumerate()
            .map(|(i, &w)| Window {
                early: w,
                late: w,
                color: format!("Judgement.W{}", i + 1),
            })
            .collect(),
        life,
        danger: Some(0.25),
        lanes: ddi_chart::Layout::builtin("dance-single")
            .map(|l| super::play::lanes(&l))
            .unwrap_or_default(),
        offset: Some(SongOffset {
            played: 0.0,
            current: 0.0,
            suggested: Some(-0.002),
        }),
        panel_sets: super::play::panel_sets(),
        panel_set: 0,
        song,
        kind: Default::default(),
    }
}

/// Options' sample: the default settings' rows (reduced motion as the
/// settings hold it, `Auto`, `On`, `Off`: 0, 1, 2), with a few
/// noteskins, a controller and a song for the volume sample.
pub fn options(reduced_motion: usize) -> OptionsData {
    use super::options_model::{OptionsModel, Sample, World};
    use crate::state::settings::{ReducedMotion, Settings};
    let settings = Settings {
        reduced_motion: [ReducedMotion::Auto, ReducedMotion::On, ReducedMotion::Off]
            [reduced_motion.min(2)],
        ..Settings::default()
    };
    let world = World {
        noteskins: vec![
            (
                ddi_noteskin::bundled::key("dance", "Delta", true),
                "Delta (with the game)".into(),
            ),
            (
                ddi_noteskin::bundled::key("dance", "Metal", false),
                "Metal".into(),
            ),
        ],
        pads_supported: true,
        connected: vec![ddi_engine::ConnectedPad {
            vendor_product: None,
            id: "Xbox Wireless Controller (STANDARD GAMEPAD)".into(),
            index: 0,
            standard: true,
        }],
        sample: Sample::Ready,
        imported: 12,
        plays: 3,
        ..World::default()
    };
    OptionsModel::new(settings, world).data()
}

/// The developer routes' screens: song select over [`select`], whose
/// Start shows [`results`] and whose Options link shows [`options`]
/// (`changed` hears each option changed, reduced motion among them).
pub fn select_screen(env: &Env, changed: Option<ChangeFn>) -> SelectScreen {
    let play: PlayFn = Rc::new(|_, _, env: &Env, cx: &mut Cx| cx.push(results_screen(env)));
    let open: OpenFn =
        Rc::new(move |env: &Env, cx: &mut Cx| cx.push(options_screen(env, changed.clone())));
    SelectScreen::new(env.clone(), select(), Some("some-things-must"), play, open)
}

/// Results over [`results`]; Retry goes back to the songs (no play
/// here).
pub fn results_screen(env: &Env) -> ResultsScreen {
    ResultsScreen::new(env.clone(), results(), ResultsActions::default())
}

/// The credits over song select: `credits` (the generated file where
/// the shell has it, else [`super::credits::sample`]).
/// The credits' sample (`credits::sample`), as `xtask gen-credits` writes
/// `credits.json`: made-up entries, an announcer's Japanese name among
/// them (data, drawn by the system's fonts as song titles are).
pub const CREDITS: &str = r#"{
  "songs": [{
    "title": "Beatucada", "artist": "Kurio Prokos",
    "source_url": "https://example.org/songs", "source_path": "Beatucada",
    "source_commit": "0123456789abcdef", "retrieved": "2026-10-01",
    "parts": [
      {"part": "music", "spdx": "CC-BY-4.0", "holders": ["Kurio Prokos"], "url": "https://creativecommons.org/licenses/by/4.0/"},
      {"part": "charts", "spdx": "CC-BY-4.0", "holders": ["Someone With A Rather Long Name", "Another"], "url": "https://creativecommons.org/licenses/by/4.0/"}
    ],
    "chart_authors": {"Daniel Rotwind": ["Beginner", "Easy"]},
    "excluded": [], "modifications": ["banner resized: magick banner.png -resize 50% banner.png"]
  }],
  "noteskins": [{
    "name": "Delta", "author": "StepMania", "source_url": "https://example.org/skins",
    "source_path": "NoteSkins/dance/delta", "source_commit": "fedcba9876543210",
    "parts": [{"part": "art", "spdx": "MIT", "holders": ["StepMania contributors"], "url": "https://opensource.org/license/mit"}]
  }],
  "announcers": [{
    "name": "ガキンさん", "author": "すごいジャン", "language": "Japanese",
    "made_with": "Kokoro-82M (hexgrad, Apache-2.0), voice jm_kumo",
    "model_url": "https://example.org/kokoro", "revision": "1939ad2a8e416c0a", "made": "2026-11-04",
    "parts": [{"part": "all", "spdx": "CC0-1.0", "holders": ["すごいジャン"], "url": "https://creativecommons.org/publicdomain/zero/1.0/"}],
    "trained_on": [{"voice": "jm_kumo", "what": "A reading (a corpus)", "url": "https://example.org/reading", "spdx": "CC-BY-3.0", "holders": ["A broadcaster"], "license_url": "https://creativecommons.org/licenses/by/3.0/"}]
  }],
  "fonts": [{"file": "IosevkaTerm-Regular.woff2", "spdx": "OFL-1.1", "holders": "Renzhi Li", "source": "https://github.com/be5invis/Iosevka", "modified": "subset"}],
  "video": {
    "name": "FFmpeg", "version": "8.0", "spdx": "LGPL-2.1-or-later",
    "homepage": "https://ffmpeg.org", "source_url": "https://example.org/ffmpeg/tag",
    "tarball_url": "https://ffmpeg.org/releases/ffmpeg-8.0.tar.xz",
    "tarball": "video/ffmpeg-8.0.tar.xz",
    "tarball_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
    "configure": ["--disable-everything", "--enable-decoder=h264", "--enable-small"],
    "components": [{"name": "libavcodec", "spdx": "LGPL-2.1-or-later", "url": "https://ffmpeg.org/libavcodec.html"}],
    "notices": "video/ddivideo.LICENSE.txt"
  },
  "code": [{
    "name": "StepMania noteskin metrics", "what": "Metrics of the default noteskins.",
    "license": "MIT", "holders": ["Chris Danford", "Glenn Maynard"],
    "source": "https://github.com/stepmania/stepmania", "commit": "abcdef0123456789",
    "license_url": "https://github.com/stepmania/stepmania/blob/master/Docs/Licenses.txt",
    "notice": "Copyright (c) 2001-2015 Chris Danford, Glenn Maynard and others.\nPermission is hereby granted…"
  }],
  "rust": {
    "crates": [
      {"name": "rusty-opus", "version": "1.0.1", "license": "BSD-3-Clause", "chosen": "BSD-3-Clause", "repository": "", "notices": [0]},
      {"name": "serde", "version": "1.0.0", "license": "MIT OR Apache-2.0", "chosen": "MIT", "repository": "", "notices": [1]}
    ],
    "notices": [
      {"license": "BSD-3-Clause", "text": "Copyright (c) 2026 Someone\nRedistribution and use in source and binary forms…", "crates": ["rusty-opus 1.0.1"]},
      {"license": "MIT", "text": "Permission is hereby granted, free of charge…", "crates": ["serde 1.0.0"]}
    ]
  }
}"#;

pub fn credits_screen(env: &Env, credits: &super::credits::Credits) -> super::page::PageScreen {
    super::page::PageScreen::new(
        env.clone(),
        "credits",
        super::credits::doc(credits, env.platform),
    )
}

/// Calibration's menu over song select, on the stored settings (its runs
/// are the web's play route: here the buttons do nothing).
pub fn calibration_screen(env: &Env) -> super::page::PageScreen {
    let settings = crate::state::settings::Settings::load();
    super::page::PageScreen::new(
        env.clone(),
        "calibration",
        super::calibration::doc(&settings, 250.0, &[]),
    )
}

/// The announcer studio's draft and view with a recording in each state
/// the page shows (sorted and not, being read, unreadable, clipped, too
/// long, tidied each way), the details on, a status and an export made,
/// and the examples (one of our pack's sounds, then the studio's): the
/// developer routes, tests.
pub fn studio() -> (
    ddi_library::announcer::draft::Draft,
    super::studio::View,
    std::collections::BTreeMap<ddi_library::announcer::Line, Vec<super::studio::Example>>,
) {
    use ddi_library::announcer::draft::{Draft, Sound};
    use ddi_library::announcer::examples::Set;
    use ddi_library::announcer::tidy::Measure;
    use ddi_library::announcer::{Bundled, Comment, Grade, Kept, Line, MAX_BYTES};
    const MP3: &[u8] = b"ID3\x04\x00";
    let read = |seconds: f64, start: f64, end: f64, gain: f32, clipped: bool| {
        Sound::Read(Measure {
            rate: 48_000,
            channels: 1,
            seconds,
            clipped,
            start,
            end,
            gain,
        })
    };
    let mut d = Draft::default();
    let mut take = |source: &str, line: Option<Line>, sound: Sound, tidy: Option<bool>| {
        let id = d.add(source, 40_000, MP3);
        let t = d.take_mut(id).expect("added");
        t.line = line;
        t.sound = sound;
        t.tidy = tidy;
        id
    };
    // Cut and made louder; quieter; cut only; nothing to change.
    let ready = take(
        "ready.mp3",
        Some(Line::Ready),
        read(3.0, 0.4, 2.9, 2.0, false),
        None,
    );
    take(
        "go.mp3",
        Some(Line::HereWeGo),
        read(1.5, 0.0, 1.5, 0.5, false),
        None,
    );
    take(
        "cheer.mp3",
        Some(Line::Cheer),
        read(2.0, 0.3, 2.0, 1.0, false),
        None,
    );
    take(
        "hot.mp3",
        Some(Line::CommentHot),
        read(1.0, 0.0, 1.0, 1.0, false),
        None,
    );
    // As recorded: too long for its moment, and clipped.
    take(
        "one hundred.mp3",
        Some(Line::Combo(100)),
        read(3.0, 0.0, 3.0, 1.0, true),
        Some(false),
    );
    take(
        "double a.m4a",
        Some(Line::Grade(Grade::Aa)),
        Sound::Unreadable("EncodingError".into()),
        None,
    );
    take(
        "intro.mp3",
        Some(Line::SelectMusicIntro),
        Sound::Unread,
        None,
    );
    take(
        "hard.mp3",
        Some(Line::SelectMusic(Comment::Hard)),
        read(1.2, 0.0, 1.2, 1.0, false),
        None,
    );
    take("New Recording 12.m4a", None, Sound::Unread, None);
    // Past what the game keeps of an announcer.
    let big = take(
        "cleared.mp3",
        Some(Line::Cleared),
        Sound::Unread,
        Some(false),
    );
    d.take_mut(big).expect("added").bytes = MAX_BYTES + 1;
    let view = super::studio::View {
        examples: Set::English,
        details: true,
        playing: Some(format!("take:{ready}")),
        status: ddi_l10n::msg::studio::status_added(9, 1).into(),
        ready: Some("Announcer.zip".into()),
        busy: false,
    };
    let mut kept = Kept {
        name: Set::English.pack().into(),
        ..Default::default()
    };
    kept.lines.insert(
        "gameplay 100 combo".into(),
        vec!["gameplay 100 combo/one.mp3".into()],
    );
    let ours = Bundled {
        path: Set::English.pack().into(),
        kept,
        hashes: Default::default(),
        said: [(
            "gameplay 100 combo/one.mp3".to_string(),
            "One hundred combo!".to_string(),
        )]
        .into(),
    };
    let examples = super::studio::examples(Set::English, Some(&ours));
    (d, view, examples)
}

/// The announcer studio's page on the [`studio`] sample (its presses do
/// nothing).
pub fn studio_screen(env: &Env) -> super::page::PageScreen {
    let (draft, view, examples) = studio();
    super::page::PageScreen::new(
        env.clone(),
        super::studio::NAME,
        super::studio::doc(&draft, &view, &examples, env.platform),
    )
}

/// The start prompt for Some Things Must's Hard chart at `step`, over
/// its background, its play options changed (never saved), the other
/// choices ignored (the developer routes, tests).
pub fn prompt_screen(env: &Env, step: super::prompt::PromptStep) -> super::prompt::PromptScreen {
    use super::prompt::{
        Badge, PromptChart, PromptChoice, PromptInfo, PromptPicture, PromptScreen, PromptState,
        timing_hint,
    };
    use super::prompt_options::PlayOptions;
    use crate::state::history::timing::Verdict;
    // Played at ×3, reversed and mirrored (the mods line says so).
    let mut played = crate::state::settings::Settings {
        speed: 3.0,
        reverse: true,
        ..Default::default()
    };
    played.transform.turn = ddi_engine::Turn::Mirror;
    let info = PromptInfo {
        // The sample's setup is not calibrated: the quiet line shows.
        timing: Some(timing_hint(
            &Verdict::Inconclusive { plays: 0, hits: 0 },
            false,
        )),
        ..PromptInfo::for_chart(
            "Some Things Must (OutFox Edit)".into(),
            "Sevish".into(),
            Some(PromptChart {
                layout: "dance-single".into(),
                difficulty: ddi_chart::Difficulty::Hard,
                meter: 13,
                style: "Single".into(),
            }),
            &played,
            Badge::Auto,
        )
    };
    let state = Rc::new(std::cell::RefCell::new(PromptState {
        info,
        step,
        notes: vec![ddi_l10n::msg::prompt::video_progress(40).into()],
        picture: Some(PromptPicture {
            source: "songs/some-things-must/sevish-bg.png".into(),
            aspect: 16.0 / 9.0,
            frame: Some(4.0 / 3.0),
            brightness: 0.6,
            filter: Some(([0.5, 0.3], 0.4)),
        }),
        ..PromptState::default()
    }));
    state.borrow_mut().set_bindings(
        Some(&ddi_chart::Layout::dance_single()),
        &crate::state::settings::Settings::default(),
    );
    let mut options = PlayOptions::new(
        crate::state::settings::Settings::default(),
        "dance-single",
        super::data::Platform::Web,
    );
    options.set_noteskins(
        vec![
            ("infinito".into(), "Infinito".into()),
            ("bundled:dance/Peters-DDR".into(), "Peters-DDR".into()),
        ],
        true,
    );
    options.show(&mut state.borrow_mut());
    let options = Rc::new(std::cell::RefCell::new(options));
    let shown = state.clone();
    PromptScreen::new(
        env.clone(),
        state,
        Rc::new(move |c, _| {
            if let PromptChoice::Option { id, index } = c {
                options.borrow_mut().change(&id, index);
                options.borrow().show(&mut shown.borrow_mut());
                let mut settings = crate::state::settings::Settings::default();
                options.borrow().over(&mut settings);
                shown
                    .borrow_mut()
                    .set_bindings(Some(&ddi_chart::Layout::dance_single()), &settings);
            }
        }),
    )
}

/// What the sample's options do when changed: reduced motion is the
/// settings' (saved, and the screens follow it at once, `system` the
/// system's preference); the rest is the sample's.
pub fn motion_change(env: &Env, system: fn() -> bool) -> ChangeFn {
    use crate::state::settings::{ReducedMotion, Settings};
    let env = env.clone();
    Rc::new(move |_section: &str, o: &super::data::OptionDef| {
        if o.id != "reduced_motion" {
            return;
        }
        let mut settings = Settings::load();
        settings.reduced_motion = match o.index {
            1 => ReducedMotion::On,
            2 => ReducedMotion::Off,
            _ => ReducedMotion::Auto,
        };
        settings.save();
        env.follow_motion(settings.reduced_motion, system());
    })
}

/// Options over [`options`], reduced motion as the shell has it.
pub fn options_screen(env: &Env, changed: Option<ChangeFn>) -> OptionsScreen {
    let reduced = env.state.borrow().reduced_motion_setting;
    OptionsScreen::new(env.clone(), options(reduced), changed)
}

/// A complete title/menu flow for developer routes and headless tests.
pub fn start_screen(env: &Env, desktop: bool) -> super::start::StartScreen {
    let e = env.clone();
    super::start::StartScreen::new(
        env.clone(),
        desktop,
        Rc::new(move |choice, cx| {
            use super::start::Choice;
            match choice {
                Choice::Singleplayer => cx.push(select_screen(&e, None)),
                Choice::Options => cx.push(options_screen(&e, None)),
                Choice::Credits => cx.push(credits_screen(&e, &super::credits::sample())),
                Choice::Quit => cx.request(crate::UiRequest::Quit),
            }
        }),
    )
}

/// The chart editor on a made-up song, unsaved, for headless tests (the
/// coverage walk, `l10n_tests.rs`): notes of each kind, a change of each
/// kind of timing at beat 4 where the cursor is, beats 2 to 4 selected,
/// a second chart; saving asks first, or the
/// song is `new_song`, never saved.
pub fn editor_screen(env: &Env, new_song: bool) -> super::editor::EditorScreen {
    use crate::state::editor::{Editor, NoteTool, Retime, Segment};
    use ddi_chart::{SpeedUnit, TICKS_PER_BEAT, Tick};
    let beat = |b: f64| Tick((b * TICKS_PER_BEAT as f64) as i64);
    let mut e = Editor::new_song(Some("song.ogg".into()), "Some Things Must");
    let mut info = e.song_info();
    info.artist = "Ayaka".into();
    info.banner = Some("banner.png".into());
    e.set_song_info(info);
    for (tool, lane, at) in [
        (NoteTool::Tap, 0, 0.0),
        (NoteTool::Mine, 1, 1.0),
        (NoteTool::Lift, 2, 1.5),
        (NoteTool::Fake, 3, 2.0),
    ] {
        e.set_tool(tool);
        e.toggle_at(lane, beat(at));
    }
    e.set_tool(NoteTool::Tap);
    e.place_long(1, beat(3.0), beat(4.0));
    let keep = Retime::KeepBeats;
    for segment in [
        Segment::Stop(0.5),
        Segment::Delay(0.25),
        Segment::Warp(beat(1.0)),
        Segment::Speed {
            ratio: 1.5,
            delay: 2.0,
            unit: SpeedUnit::Beats,
        },
        Segment::Scroll(2.0),
        Segment::Fake(beat(0.5)),
        Segment::TimeSignature(3, 4),
        Segment::TickCount(4),
        Segment::Combo(2, 1),
        // No letters: the walk opens its field, which draws its value as
        // it is.
        Segment::Label("2".into()),
    ] {
        e.set_segment(beat(4.0), segment, keep);
    }
    e.set_segment(beat(8.0), Segment::Bpm(180.0), keep);
    e.set_segment(
        beat(8.0),
        Segment::Speed {
            ratio: 1.0,
            delay: 0.5,
            unit: SpeedUnit::Seconds,
        },
        keep,
    );
    e.add_chart("dance-double", Difficulty::Edit);
    e.select_chart(0);
    // The cursor at the selection's end.
    e.select(beat(2.0), beat(4.0));
    let place = super::editor::EditorPlace {
        save: Some("Songs/Pack/Some Things Must/song.ssc".into()),
        confirm: (!new_song).then(|| "Songs/Pack/Some Things Must/song.ssc".into()),
        new_song,
    };
    super::editor::EditorScreen::new(env.clone(), e, place)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_has_the_bundled_songs_and_greyed_ones() {
        let d = select();
        assert_eq!(d.songs[2].title, "Some Things Must");
        assert_eq!(d.songs[2].length_text(), "2:26");
        assert_eq!(d.songs[0].length_text(), "1:48");
        assert_eq!(d.songs[1].length_text(), "2:27");
        assert_eq!(d.songs[3].length_text(), "2:37");
        // Songs with no Single chart, and Japanese titles.
        assert!(
            d.songs
                .iter()
                .any(|s| s.charts_in("dance-single").is_empty())
        );
        assert!(
            d.songs
                .iter()
                .any(|s| s.title.chars().any(|c| c as u32 > 0x3000))
        );
        let names: Vec<&str> = d.styles.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Single",
                "Double",
                "Pump single",
                "Pump half-double",
                "Pump double"
            ]
        );
        let r = results();
        let (mean, sd) = r.timing();
        assert!(
            (mean + 2.1).abs() < 1.5 && (sd - 9.8).abs() < 1.5,
            "{mean} {sd}"
        );
        assert_eq!(options(0).sections.len(), 9);
    }
}
