//! Sample data for the theme's screens on the developer routes
//! (`#/ui-dev?screen=select|results|options`, the desktop's `--screen`):
//! the four bundled songs with their real charts, then made-up packs,
//! some with pump charts only (so they grey out in the dance styles) and
//! some with Japanese titles. The results are a made-up ITG play.

use std::rc::Rc;

use ddi_chart::Difficulty;

use super::Env;
use super::data::{
    ChartEntry, Hit, JudgementCount, OptionDef, OptionKind, OptionSection, OptionsData,
    ResultsData, SelectData, SongEntry, StyleDef,
};
use super::options::{ChangeFn, OptionsScreen};
use super::results::ResultsScreen;
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

/// The bundled songs' pack, as the song list names it.
pub const BUNDLED_PACK: &str = "Bundled songs";

/// The styles, as `ddi_chart`'s built-in layouts name them.
pub fn styles() -> Vec<StyleDef> {
    ddi_chart::layout::Layout::builtins()
        .into_iter()
        .map(|l| StyleDef {
            id: l.id.to_string(),
            name: l.name.to_string(),
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
                pack: BUNDLED_PACK.into(),
                bpm: *bpm,
                bpm_text: format!("{bpm}"),
                length: *length,
                banner: Some(format!("songs/{id}/{banner}")),
                background: background.map(|b| format!("songs/{id}/{b}")),
                charts: charts
                    .iter()
                    .map(|(style, difficulty, meter, credit, notes)| ChartEntry {
                        style: style.to_string(),
                        difficulty: *difficulty,
                        meter: *meter,
                        notes: *notes,
                        credit: credit.to_string(),
                    })
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
                pack: pack.to_string(),
                bpm,
                bpm_text: format!("{bpm}"),
                length: 90.0 + rng.below(90) as f32,
                banner: None,
                background: None,
                charts,
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
    SelectData {
        styles,
        songs,
        mods: vec!["×2.5".into(), "Reverse".into(), "ITG".into()],
        preferred: Some(Difficulty::Hard),
    }
}

/// Results' sample: Some Things Must on Hard, an ITG play.
pub fn results() -> ResultsData {
    let song = bundled()
        .into_iter()
        .find(|s| s.id == "some-things-must")
        .unwrap_or_default();
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let length = song.length;
    // 282 hits about −2.1 ms with σ 9.8, one miss at 95 s.
    let mut hits: Vec<Hit> = (0..282)
        .map(|i| Hit {
            time: 4.0 + (length - 10.0) * i as f32 / 282.0,
            offset: Some(-2.1 + rng.normal() * 9.8),
            lane: rng.below(4) as u8,
        })
        .collect();
    hits.push(Hit {
        time: 95.0,
        offset: None,
        lane: 2,
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
    let j = |name: &str, color: &str, count: u32| JudgementCount {
        name: name.into(),
        color: color.into(),
        count,
    };
    ResultsData {
        line: "Hard 13 · Single · ITG rules · ×2.5 Reverse".into(),
        grade: "★".into(),
        score: 96.77,
        ex: 94.10,
        judgements: vec![
            j("Fantastic", "Judgement.W1", counts[0]),
            j("Excellent", "Judgement.W2", counts[1]),
            j("Great", "Judgement.W3", counts[2]),
            j("Decent", "Judgement.W4", counts[3]),
            j("Way Off", "Judgement.W5", counts[4]),
            j("Miss", "Judgement.Miss", counts[5]),
        ],
        stats: vec![
            ("Held / let go".into(), "13 / 1".into()),
            ("Max combo".into(), "187".into()),
            ("Mines hit".into(), "0 / 6".into()),
            ("Fast / slow".into(), format!("{fast} / {slow}")),
        ],
        hits,
        windows: vec![
            (21.5, "Judgement.W1".into()),
            (43.0, "Judgement.W2".into()),
            (102.0, "Judgement.W3".into()),
            (135.0, "Judgement.W4".into()),
            (180.0, "Judgement.W5".into()),
        ],
        life,
        lanes: vec!["Left".into(), "Down".into(), "Up".into(), "Right".into()],
        song_offset: 0,
        note: "Sample play: the numbers are illustrative.".into(),
        song,
    }
}

fn option(
    heading: Option<&str>,
    label: &str,
    kind: OptionKind,
    index: usize,
    note: &str,
    description: &str,
) -> OptionDef {
    OptionDef {
        label: label.into(),
        kind,
        index,
        note: note.into(),
        description: description.into(),
        heading: heading.map(str::to_string),
    }
}

fn choices(v: &[&str]) -> OptionKind {
    OptionKind::Choices(v.iter().map(|s| s.to_string()).collect())
}

/// Options' sample, with reduced motion as the settings hold it
/// (`Auto`, `On`, `Off`: 0, 1, 2).
pub fn options(reduced_motion: usize) -> OptionsData {
    let speeds: Vec<String> = (2..=32)
        .map(|q| {
            let v = q as f32 * 0.25;
            format!("×{}", trim(v))
        })
        .collect();
    let speed_at = speeds.iter().position(|s| s == "×2.5").unwrap_or(0);
    let offsets: Vec<String> = (-50..=50).map(|ms| format!("{ms:+} ms")).collect();
    let section = |name: &str, options: Vec<OptionDef>| OptionSection {
        name: name.into(),
        options,
    };
    OptionsData {
        sections: vec![
            section(
                "Scroll and notes",
                vec![
                    option(
                        Some("SCROLL"),
                        "Speed",
                        OptionKind::Stepper(speeds),
                        speed_at,
                        "400 at this song",
                        "How fast the arrows scroll: the chart's tempo times this.",
                    ),
                    option(
                        None,
                        "Charts with their own speed",
                        choices(&["the speed above", "the chart's"]),
                        0,
                        "",
                        "Some charts set their own scroll speed; keep yours or follow theirs.",
                    ),
                    option(
                        None,
                        "Reverse",
                        choices(&["Off", "On"]),
                        1,
                        "",
                        "Arrows scroll down to targets at the bottom.",
                    ),
                    option(
                        None,
                        "Scroll action",
                        choices(&["Normal", "Boost", "Brake", "Wave"]),
                        0,
                        "",
                        "Arrows speed up, slow down or sway as they come.",
                    ),
                    option(
                        Some("NOTES"),
                        "Turn",
                        choices(&["Off", "Mirror", "Left", "Right", "Shuffle"]),
                        0,
                        "",
                        "Moves every arrow to another lane, the same way for the whole chart.",
                    ),
                    option(
                        None,
                        "Appearance",
                        choices(&["Normal", "Hidden", "Sudden", "Both", "Stealth"]),
                        0,
                        "",
                        "Hides the arrows near the targets, far from them, or everywhere.",
                    ),
                    option(
                        None,
                        "Notes",
                        choices(&["All", "Quarters", "Quarters and eighths"]),
                        0,
                        "",
                        "Leaves out the faster notes.",
                    ),
                    option(
                        None,
                        "No jumps",
                        choices(&["Off", "On"]),
                        0,
                        "",
                        "Two arrows at once become one.",
                    ),
                    option(
                        None,
                        "No holds",
                        choices(&["Off", "On"]),
                        0,
                        "",
                        "Holds and rolls become single taps.",
                    ),
                ],
            ),
            section(
                "Rules",
                vec![option(
                    None,
                    "Rules",
                    choices(&["ITG", "StepMania", "DDR A"]),
                    0,
                    "",
                    "How timing is judged and scored, after the game that set the rules.",
                )],
            ),
            section(
                "Timing",
                vec![
                    option(
                        None,
                        "Audio offset",
                        OptionKind::Stepper(offsets.clone()),
                        50,
                        "",
                        "Moves judgement against the music; calibrate to set it.",
                    ),
                    option(
                        None,
                        "Visual offset",
                        OptionKind::Stepper(offsets),
                        50,
                        "",
                        "Moves only what is drawn.",
                    ),
                ],
            ),
            section(
                "Sound",
                vec![option(
                    None,
                    "Volume",
                    OptionKind::Stepper((0..=20).map(|v| format!("{} %", v * 5)).collect()),
                    16,
                    "",
                    "How loud the music plays.",
                )],
            ),
            section(
                "Display",
                vec![option(
                    None,
                    "Reduced motion",
                    choices(&["As the system", "On", "Off"]),
                    reduced_motion.min(2),
                    "",
                    "No beat pulse and no slides in the menus.",
                )],
            ),
            section(
                "Arrows",
                vec![option(
                    None,
                    "Noteskin",
                    choices(&["Dito", "Built-in"]),
                    0,
                    "",
                    "How the arrows look.",
                )],
            ),
            section(
                "Controls",
                vec![option(
                    None,
                    "Menu keys",
                    choices(&["Arrows", "WASD"]),
                    0,
                    "",
                    "Which keys move through the menus.",
                )],
            ),
            section(
                "Reset",
                vec![option(
                    None,
                    "Reset",
                    choices(&["Keep", "Reset everything"]),
                    0,
                    "",
                    "Puts every option back as it came.",
                )],
            ),
        ],
        preview: "Dito · Single · ×2.5 · Reverse".into(),
    }
}

/// "2.5", "3".
fn trim(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
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
    ResultsScreen::new(env.clone(), results(), None)
}

/// Options over [`options`], reduced motion as the shell has it.
pub fn options_screen(env: &Env, changed: Option<ChangeFn>) -> OptionsScreen {
    let reduced = env.state.borrow().reduced_motion_setting;
    OptionsScreen::new(env.clone(), options(reduced), changed)
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
        assert_eq!(options(0).sections.len(), 8);
    }
}
