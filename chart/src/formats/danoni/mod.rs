//! Dancing☆Onigiri ("DanOni") works, as danoniplus reads them.
//!
//! A work is a set of dos fields (`dos`): one or more charts (`difData`),
//! each with one note list per lane (`left_data`, `frzLeft_data`, `left2_data`
//! for the second chart, …), music (`musicUrl`, `musicNo`), timing
//! (`blankFrame`, `adjustment`) and effects. Behaviour follows danoniplus
//! v51.2.2 (`80c3c47`; `scoreConvert` in `js/lib/dataLoader.js` 712–1170,
//! `headerConvert` in `js/lib/dosConverter.js`), as recorded in
//! `docs/research/formats-ffr-danoni.md` B8.
//!
//! Notes are integer frames at 60 fps and land on the tick grid through the
//! synthetic tempo of decision 4: at 75 BPM one tick is one frame. A frame
//! `F` sounds `(F − blankFrame + floor(adjustment)) / 60` seconds into the
//! music, so each chart's timing is a constant 75 BPM with that offset.
//! `speed_data`/`speed_change` become scroll segments (danoniplus moves notes
//! by the speed of each frame, which at a constant tempo is exactly a
//! displayed-beat ratio), and `boost_data` a per-note scroll multiplier by
//! arrival frame. Neither changes when a note is judged.
//!
//! Deviations, each reported as a warning where a work uses it:
//! - `playbackRate` is not applied: charts play at the music's own speed,
//!   which is the timing the author wrote against.
//! - Notes after `endFrame` are left out (danoniplus ends the play there
//!   without judging them).
//! - Divided and locked external dos (`externalDosDivide`, `dosNo`) and key
//!   patterns other than the first are not read.
//! - A tap on the same frame and lane as a hold start is dropped (the hold
//!   remains); danoniplus judges neither of the two in that case.

mod colors;
pub mod dos;
pub mod expr;
mod keycodes;
pub mod keys;
pub mod source;

use crate::layout::danoni::layout_id;
use crate::model::{
    Chart, DanoniChart, Difficulty, DisplayBpm, EffectEvent, EffectTime, Note, NoteKind, Song,
    SourceFormat, Tick,
};
use crate::timing::{ScrollSegment, TimingMap};

pub use dos::{Dos, DosFlags};
use dos::{js_round, parse_float, parse_int, split_lf, split_lf2};

/// The synthetic tempo of frame-based charts: one tick per 60 fps frame.
pub const SYNTHETIC_BPM: f64 = 75.0;

/// Largest frame read (about 580 years at 60 fps): anything beyond is
/// dropped rather than saturating the tick arithmetic.
const MAX_FRAME: f64 = (1u64 << 40) as f64;

/// Most charts read from one work (the largest known package has 55).
const MAX_CHARTS: usize = 512;

/// Longest value a field reference may expand to.
const MAX_REF_BYTES: usize = 8 << 20;

/// Rule headers kept per work, and their bytes in all.
const MAX_RULE_HEADERS: usize = 256;
const MAX_RULE_HEADER_BYTES: usize = 64 << 10;

/// Default `blankFrame`.
const DEFAULT_BLANK_FRAME: i64 = 200;

/// Headers kept on each chart for the rulesets, besides every `gauge…`
/// and `customGauge…` header (per-gauge and per-chart settings).
const RULE_HEADERS: [&str; 6] = [
    "wordAutoReverse",
    "maxLifeVal",
    "frzStartjdgUse",
    "frzAttempt",
    "excessiveUse",
    "excessiveJdgUse",
];

/// Whether a header carries rules the rulesets read.
fn is_rule_header(key: &str) -> bool {
    RULE_HEADERS.contains(&key)
        || ((key.starts_with("gauge") || key.starts_with("customGauge"))
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// The songs of a work (one per music file it uses) and what could not be
/// imported as danoniplus would play it.
#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub songs: Vec<Song>,
    /// The music number (`musicNo`) of each song in `songs`.
    pub music: Vec<usize>,
    /// Per author chart (`difData` row): `(song, chart)` in `songs`, or
    /// `None` when it could not be imported.
    pub charts: Vec<Option<(usize, usize)>>,
    /// One line per problem, for the import report.
    pub warnings: Vec<String>,
}

/// Imports a work from its merged dos fields.
pub fn import(dos: &Dos) -> Result<Import, String> {
    let mut warnings = Vec::new();
    let mut difs = match dos.val("difData") {
        Some(d) => split_lf2(d),
        None => vec!["7,Normal,3.5".to_string()],
    };
    if difs.len() > MAX_CHARTS {
        warnings.push(format!(
            "{} charts; only the first {MAX_CHARTS} are imported",
            difs.len()
        ));
        difs.truncate(MAX_CHARTS);
    }
    let music_nos: Vec<usize> = match dos.val("musicNo") {
        Some(m) => split_lf2(m)
            .iter()
            .map(|v| dos::parse_int(v).filter(|n| *n >= 0).unwrap_or(0) as usize)
            .collect(),
        None => vec![0; difs.len()],
    };
    let titles: Vec<String> = dos.val("musicTitle").map(split_lf2).unwrap_or_default();
    let urls: Vec<String> = dos
        .val("musicUrl")
        .map(|u| {
            split_lf2(u)
                .iter()
                .map(|v| v.split(',').next().unwrap_or_default().trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    let blank_frames: Vec<i64> = match dos.get("blankFrame").and_then(parse_float) {
        Some(_) => split_lf2(dos.get("blankFrame").unwrap_or_default())
            .iter()
            .map(|v| dos::parse_int(v).unwrap_or(DEFAULT_BLANK_FRAME))
            .collect(),
        None => vec![DEFAULT_BLANK_FRAME],
    };
    let adjustments: Vec<String> = dos
        .val("adjustment")
        .map(|a| a.split('$').map(str::to_string).collect())
        .unwrap_or_else(|| vec!["0".into()]);
    let rate = dos.val("playbackRate").and_then(parse_float).unwrap_or(1.0);
    if rate > 0.0 && rate != 1.0 {
        warnings.push(format!(
            "playbackRate {rate} is not applied: the charts play at the music's own speed"
        ));
    }
    let tuning = dos
        .val("tuning")
        .map(|t| {
            split_lf2(t)[0]
                .split(',')
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default();
    let dummy_ids: Vec<String> = dos
        .val("dummyId")
        .map(|d| d.split('$').map(str::to_string).collect())
        .unwrap_or_default();
    // Every chart keeps a copy, so the whole set is bounded.
    let mut header_bytes = 0;
    let gauge_headers: Vec<(String, String)> = dos
        .fields()
        .filter(|(k, _)| is_rule_header(k))
        .take_while(|(k, v)| {
            header_bytes += k.len() + v.len();
            header_bytes <= MAX_RULE_HEADER_BYTES
        })
        .take(MAX_RULE_HEADERS)
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    if header_bytes > MAX_RULE_HEADER_BYTES {
        warnings.push("gauge and rule headers beyond the first 64 KB were left out".into());
    }

    let stage = stage_size(dos, &difs);
    let mut songs: Vec<(usize, Song)> = Vec::new();
    let mut word_budget = MAX_WORK_WORD_BYTES;
    let mut sprite_budget = MAX_WORK_SPRITE_BYTES;
    let mut index: Vec<Option<(usize, usize)>> = Vec::new();
    for (i, dif) in difs.iter().enumerate() {
        let fields: Vec<&str> = dif.split(',').collect();
        let field = |k: usize| fields.get(k).copied().filter(|v| !v.is_empty());
        let label = keys::canonical_label(field(0).unwrap_or("7")).to_string();
        let (name, maker, level) = match field(1) {
            Some(n) => {
                let mut parts = n.split("::");
                (
                    parts.next().unwrap_or("Normal").to_string(),
                    parts.next().filter(|m| !m.is_empty()).map(str::to_string),
                    parts.next().and_then(dos::parse_int).unwrap_or(0),
                )
            }
            None => ("Normal".to_string(), None, 0),
        };
        let chart_label = format!("chart {} ({label} keys, {name})", i + 1);
        if !crate::layout::PLAYED_DANONI_MODES.contains(&label.as_str()) {
            warnings.push(format!(
                "{chart_label}: {label} keys is not a key mode this game plays (it plays 5, 7, 9A and 9B keys)"
            ));
            if crate::layout::FORMER_DANONI_MODES.contains(&label.as_str()) {
                // Kept in its place, unplayable: no layout resolves its id.
                let music = music_nos.get(i).copied().unwrap_or(0);
                let k = match songs.iter().position(|(m, _)| *m == music) {
                    Some(k) => k,
                    None => {
                        songs.push((music, new_song(music, &titles, &urls, &tuning)));
                        songs.len() - 1
                    }
                };
                songs[k].1.charts.push(Chart {
                    layout: layout_id(&label),
                    difficulty: Difficulty::Edit,
                    meter: level.clamp(0, 999) as u32,
                    name: name.clone(),
                    description: name,
                    credit: maker.unwrap_or_else(|| tuning.clone()),
                    notes: Vec::new(),
                    timing: None,
                    display_bpm: None,
                    danoni: None,
                });
            }
            continue;
        }
        let (def, from_header) = match keys::key_def(dos, &label) {
            Ok(Some(d)) => d,
            Ok(None) => {
                warnings.push(format!("{chart_label}: unknown key mode {label}"));
                continue;
            }
            Err(e) => {
                warnings.push(format!("{chart_label}: key mode {label} {e}"));
                continue;
            }
        };
        if from_header {
            // A work redefining a key mode changes lanes, positions or keys
            // in ways the game does not follow (decision 27).
            warnings.push(format!(
                "{chart_label}: the work redefines {label} keys, which is not supported"
            ));
            continue;
        }
        let id = layout_id(&label);

        let suffix = chart_suffix(i);
        let blank = blank_frames
            .get(i)
            .or(blank_frames.first())
            .copied()
            .unwrap_or(DEFAULT_BLANK_FRAME);
        let adjustment = adjustments
            .get(i)
            .filter(|a| !a.is_empty())
            .or(adjustments.first())
            .and_then(|a| parse_float(a))
            .filter(|a| a.is_finite())
            .unwrap_or(0.0)
            .floor()
            .clamp(-MAX_FRAME, MAX_FRAME);
        let blank = (blank as f64).clamp(-MAX_FRAME, MAX_FRAME);
        let mut timing = TimingMap::constant(SYNTHETIC_BPM, (blank - adjustment) / 60.0);

        let mut notes = Vec::new();
        let mut collisions = 0;
        let mut odd_holds = 0;
        // `parseInt`: 0 or 1 is the first chart's arrays, n is chart n's,
        // anything else is no dummy chart (`settings.js` 1343,
        // `dataLoader.js` 327–334).
        let dummy = dummy_ids
            .get(i)
            .and_then(|d| dos::parse_int(d))
            .filter(|n| *n >= 0)
            .map(|n| if n <= 1 { String::new() } else { n.to_string() });
        let lanes = lane_frames(dos, &def.chara, &suffix);
        let dummy_lanes = dummy.map(|d| lane_frames(dos, &def.chara, &d));
        for (lane, l) in lanes.iter().enumerate() {
            let lane = lane as u8;
            if l.holds.len() % 2 == 1 {
                odd_holds += 1;
            }
            let heads: std::collections::HashSet<i64> =
                l.holds.as_chunks::<2>().0.iter().map(|p| p[0]).collect();
            for p in l.holds.as_chunks::<2>().0 {
                let kind = if p[1] > p[0] {
                    NoteKind::HoldHead { end: Tick(p[1]) }
                } else {
                    NoteKind::Tap
                };
                notes.push(Note::new(Tick(p[0]), lane, kind));
            }
            let mut last = None;
            for &t in &l.taps {
                if heads.contains(&t) || last == Some(t) {
                    collisions += 1;
                    continue;
                }
                last = Some(t);
                notes.push(Note::new(Tick(t), lane, NoteKind::Tap));
            }
            if let Some(d) = dummy_lanes.as_ref().and_then(|d| d.get(usize::from(lane))) {
                for t in d
                    .taps
                    .iter()
                    .copied()
                    .chain(d.holds.as_chunks::<2>().0.iter().map(|p| p[0]))
                {
                    notes.push(Note::new(Tick(t), lane, NoteKind::Dummy));
                }
            }
        }
        if collisions > 0 {
            warnings.push(format!(
                "{chart_label}: {collisions} tap(s) on the frame of a hold start or another tap in the same lane were dropped"
            ));
        }
        if odd_holds > 0 {
            warnings.push(format!(
                "{chart_label}: {odd_holds} lane(s) with an unpaired hold end"
            ));
        }

        // Where the play starts and the music ends (startFrame, endFrame,
        // fadeFrame); notes after the end are never reached.
        let span = PlaySpan::of(dos, i, &timing, blank);
        if let Some(end) = span.end_frame {
            let before = notes.len();
            notes.retain(|n| n.kind == NoteKind::Dummy || n.end_tick().0 <= end);
            notes.retain(|n| n.tick.0 <= end);
            let dropped = before - notes.len();
            if dropped > 0 {
                warnings.push(format!(
                    "{chart_label}: {dropped} note(s) after endFrame were left out"
                ));
            }
        }

        for (frame, ratio) in speed_pairs(dos, &suffix) {
            timing.scrolls.push(ScrollSegment {
                tick: Tick(frame),
                ratio,
            });
        }
        timing.scrolls.sort_by_key(|s| s.tick);
        let boosts = boost_pairs(dos, &suffix);
        if !boosts.is_empty() {
            for n in &mut notes {
                // The last boost at or before the note's arrival.
                let k = boosts.partition_point(|(f, _)| *f <= n.tick.0);
                let m = k.checked_sub(1).map(|k| boosts[k].1);
                if let Some(m) = m.filter(|m| *m != 1.0) {
                    n.speed_mul = Some(m as f32);
                }
            }
        }
        // The work's own colours, used when the player asks for them.
        let palette = colors::Palette::new(dos, &suffix, &def.color);
        for n in &mut notes {
            n.color = match n.kind {
                // danoniplus draws dummy arrows in `setDummyColor`'s greys.
                NoteKind::Dummy => {
                    let grey = [0x77, 0x44][usize::from(def.color[usize::from(n.lane)]) % 2];
                    let c = f32::from(grey as u8) / 255.0;
                    Some(crate::model::Color {
                        r: c,
                        g: c,
                        b: c,
                        a: 1.0,
                    })
                }
                kind => palette.color(
                    usize::from(n.lane),
                    n.tick.0,
                    matches!(kind, NoteKind::HoldHead { .. }),
                ),
            };
        }
        notes.sort_by_key(|n| (n.tick, n.lane));

        let gauge = [3usize, 4, 5, 6].map(|k| {
            field(k)
                .map(str::to_string)
                .unwrap_or_else(|| ["x", "6", "40", "25"][k - 3].to_string())
        });
        let chart = Chart {
            layout: id,
            difficulty: Difficulty::Edit,
            meter: level.clamp(0, 999) as u32,
            name: name.clone(),
            description: name,
            credit: maker.unwrap_or_else(|| tuning.clone()),
            notes,
            timing: Some(timing),
            display_bpm: None,
            danoni: Some(DanoniChart {
                init_speed: field(2).and_then(parse_float).unwrap_or(3.5),
                gauge,
                headers: gauge_headers.clone(),
                index: i,
                start: span.start,
                music_start: span.music_start,
                end: span.end,
                fade: span.fade,
                stage: Some(stage),
            }),
        };

        let music = music_nos.get(i).copied().unwrap_or(0);
        let song_index = match songs.iter().position(|(m, _)| *m == music) {
            Some(k) => k,
            None => {
                songs.push((music, new_song(music, &titles, &urls, &tuning)));
                songs.len() - 1
            }
        };
        index.resize(i, None);
        index.push(Some((song_index, songs[song_index].1.charts.len())));
        let song = &mut songs[song_index].1;
        // The first playable chart's (a former key mode's has none).
        if song.charts.iter().all(|c| c.timing.is_none()) {
            song.timing = chart.timing.clone().expect("set above");
        }
        // Lyrics and the other effects stay as raw events for later layers,
        // timed in seconds (charts of one song may differ in blankFrame).
        // The layer names the chart; a work's lyrics are bounded in all.
        let chart_timing = chart.timing.as_ref().expect("set above");
        let layer = u8::try_from(song.charts.len()).ok();
        if let Some(layer) = layer.filter(|_| word_budget > 0) {
            for (frame, fields) in words(dos, &suffix) {
                let cost = fields.iter().map(String::len).sum::<usize>() + 32;
                if cost > word_budget {
                    word_budget = 0;
                    warnings.push("lyrics beyond the first 4 MB were left out".into());
                    break;
                }
                word_budget -= cost;
                song.effects.push(EffectEvent {
                    at: EffectTime::Seconds(chart_timing.seconds_at(Tick(frame))),
                    kind: "danoni:lyric".into(),
                    layer,
                    fields,
                });
            }
        }
        // Back and mask layers, and their versions for Reverse.
        if let Some(layer) = layer.filter(|_| sprite_budget > 0) {
            'layers: for (kind, header) in [(SPRITE_BACK, "back"), (SPRITE_MASK, "mask")] {
                for variant in ["", "Rev"] {
                    let Some(cues) = sprites(dos, header, variant, &suffix) else {
                        continue;
                    };
                    // Reverse's data replaces the other even with nothing
                    // shown: a cue of its variant alone says it is there.
                    if variant == "Rev" {
                        song.effects.push(EffectEvent {
                            at: EffectTime::Seconds(0.0),
                            kind: kind.into(),
                            layer,
                            fields: vec![variant.to_string()],
                        });
                    }
                    for (frame, fields) in cues {
                        let cost = fields.iter().map(String::len).sum::<usize>() + 32;
                        if cost > sprite_budget {
                            sprite_budget = 0;
                            warnings.push(
                                "back and mask layers beyond the first 4 MB were left out".into(),
                            );
                            break 'layers;
                        }
                        sprite_budget -= cost;
                        song.effects.push(EffectEvent {
                            at: EffectTime::Seconds(chart_timing.seconds_at(Tick(frame))),
                            kind: kind.into(),
                            layer,
                            fields,
                        });
                    }
                }
            }
        }
        song.charts.push(chart);
    }
    // Songs left with former key modes' charts only: not songs.
    let keep: Vec<bool> = songs
        .iter()
        .map(|(_, s)| s.charts.iter().any(|c| c.timing.is_some()))
        .collect();
    if keep.contains(&false) {
        let mut at = 0;
        let renumber: Vec<Option<usize>> = keep
            .iter()
            .map(|&k| {
                k.then(|| {
                    at += 1;
                    at - 1
                })
            })
            .collect();
        for e in &mut index {
            *e = e.and_then(|(s, c)| renumber[s].map(|s| (s, c)));
        }
        let mut k = keep.iter();
        songs.retain(|_| *k.next().unwrap_or(&false));
    }
    if songs.is_empty() {
        // Every reason, so the one that matters (a key mode the game does
        // not play) is not hidden behind a header note.
        return Err(if warnings.is_empty() {
            "no playable chart".into()
        } else {
            format!("no playable chart: {}", warnings.join("; "))
        });
    }
    index.resize(difs.len(), None);
    Ok(Import {
        music: songs.iter().map(|(m, _)| *m).collect(),
        songs: songs.into_iter().map(|(_, s)| s).collect(),
        charts: index,
        warnings,
    })
}

/// A song for music number `music`.
fn new_song(music: usize, titles: &[String], urls: &[String], tuning: &str) -> Song {
    let mut song = super::sm::empty_song(SourceFormat::Danoni);
    // `splitComma`: ", " is not a separator.
    let split = |row: &str| -> Vec<String> {
        row.replace(", ", "\u{1}")
            .split(',')
            .map(|p| p.replace('\u{1}', ", "))
            .collect()
    };
    let own = split(titles.get(music).map_or("", String::as_str));
    let first = split(titles.first().map_or("", String::as_str));
    // A field the song's row leaves empty comes from the first row, as
    // `headerConvert` fills them (`dosConverter.js` 1121–1130).
    let get = |k: usize| {
        [&own, &first].into_iter().find_map(|parts| {
            parts
                .get(k)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
    };
    song.title = get(0)
        .map(|t| music_name(&t))
        .unwrap_or_else(|| "musicName".into());
    song.artist = get(1).map(|a| unescape(&a)).unwrap_or_default();
    song.credit = tuning.to_string();
    song.music = urls
        .get(music)
        .or(urls.first())
        .filter(|u| !u.is_empty())
        .cloned();
    song.display_bpm = get(4)
        .and_then(|b| display_bpm(&b))
        .unwrap_or(DisplayBpm::Actual);
    song.timing = TimingMap::constant(SYNTHETIC_BPM, DEFAULT_BLANK_FRAME as f64 / 60.0);
    song
}

/// A title as shown on one line (`getMusicNameSimple`).
fn music_name(s: &str) -> String {
    unescape(
        &s.replace("<br>", " ")
            .replace("<nbr>", "")
            .replace("<dbr>", "\u{3000}"),
    )
}

/// danoniplus `escapeTag` placeholders back to characters.
fn unescape(s: &str) -> String {
    [
        ("*amp*", "&"),
        ("*pipe*", "|"),
        ("*dollar*", "$"),
        ("*rsquo*", "’"),
        ("*quot*", "\""),
        ("*comma*", ","),
        ("*squo*", "'"),
        ("*bkquo*", "`"),
        ("*lt*", "<"),
        ("*gt*", ">"),
        ("*lbrace*", "{"),
        ("*rbrace*", "}"),
    ]
    .iter()
    .fold(s.to_string(), |acc, (from, to)| acc.replace(from, to))
}

/// `180` or `168-325`.
fn display_bpm(s: &str) -> Option<DisplayBpm> {
    match s.split_once('-') {
        Some((a, b)) => Some(DisplayBpm::Range(parse_float(a)?, parse_float(b)?)),
        None => parse_float(s).map(DisplayBpm::Single),
    }
}

/// danoniplus `storeArrowData`: lines joined, comma-separated `parseFloat`
/// values (others dropped), rounded like `Math.round`, sorted.
fn frames(data: Option<&str>) -> Vec<i64> {
    let Some(data) = data.filter(|d| !d.is_empty()) else {
        return Vec::new();
    };
    let joined: String = split_lf(data).collect();
    let mut v: Vec<i64> = joined
        .split(',')
        .filter_map(parse_float)
        .filter(|f| f.abs() <= MAX_FRAME)
        .map(|f| js_round(f) as i64)
        .collect();
    v.sort_unstable();
    v
}

/// One lane's frames as danoniplus parses them, before play preparation:
/// sorted taps and sorted hold endpoints (start, end, start, end, …; an odd
/// last value is kept here and dropped when building notes).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaneFrames {
    pub taps: Vec<i64>,
    pub holds: Vec<i64>,
}

/// The frames of every lane of a chart: the arrays named `<chara><suffix>_data`
/// and `<frz name><suffix>_data` (the first chart's suffix is empty, then
/// `2`, `3`, …).
pub fn lane_frames(dos: &Dos, chara: &[String], suffix: &str) -> Vec<LaneFrames> {
    chara
        .iter()
        .map(|c| LaneFrames {
            taps: frames(dos.get(&format!("{c}{suffix}_data"))),
            holds: frames(dos.get(&format!("{}{suffix}_data", frz_name(c)))),
        })
        .collect()
}

/// A chart's speed changes (`speed_change` wins over `speed_data`), as
/// `(frame, multiplier)` sorted by frame.
pub fn speed_pairs(dos: &Dos, suffix: &str) -> Vec<(i64, f64)> {
    let name = if dos.val(&format!("speed{suffix}_change")).is_some() {
        format!("{suffix}_change")
    } else {
        format!("{suffix}_data")
    };
    pairs(dos, "speed", &name)
}

/// A chart's boosts, as `(frame, multiplier)` sorted by frame.
pub fn boost_pairs(dos: &Dos, suffix: &str) -> Vec<(i64, f64)> {
    pairs(dos, "boost", &format!("{suffix}_data"))
}

/// The suffix of chart `index`'s arrays (`""`, `"2"`, `"3"`, …).
pub fn chart_suffix(index: usize) -> String {
    if index == 0 {
        String::new()
    } else {
        (index + 1).to_string()
    }
}

/// The hold array name of a lane (`g_escapeStr.frzName`, then `frz` and the
/// capitalised name when nothing matched).
pub fn frz_name(chara: &str) -> String {
    const PAIRS: [(&str, &str); 10] = [
        ("leftdia", "frzLdia"),
        ("rightdia", "frzRdia"),
        ("left", "frzLeft"),
        ("down", "frzDown"),
        ("up", "frzUp"),
        ("right", "frzRight"),
        ("space", "frzSpace"),
        ("iyo", "frzIyo"),
        ("gor", "frzGor"),
        ("oni", "foni"),
    ];
    let mut s = chara.to_string();
    for (from, to) in PAIRS {
        s = s.replace(from, to);
    }
    if !s.contains("frz") && !s.contains("foni") {
        let mut c = s.chars();
        s = match c.next() {
            Some(f) => format!("frz{}{}", f.to_uppercase(), c.as_str()),
            None => "frz".into(),
        };
    }
    s
}

/// danoniplus `getRefData`: a value whose lines name another field with
/// the same prefix take that field's value.
///
/// Built line by line, at most [`MAX_REF_BYTES`] long: a value naming a
/// large field on many lines must not grow without bound.
fn ref_data(dos: &Dos, header: &str, name: &str) -> Option<String> {
    let data = dos.get(&format!("{header}{name}"))?;
    let mut out = String::new();
    for (k, line) in split_lf(data).enumerate() {
        if k > 0 {
            out.push('\n');
        }
        let text = match dos.get(line) {
            Some(v) if line.starts_with(header) => v,
            _ => line,
        };
        if out.len() + text.len() > MAX_REF_BYTES {
            break;
        }
        out.push_str(text);
    }
    Some(out)
}

/// `setSpeedData`: `frame,value` pairs, values evaluated, sorted by frame;
/// a `-` value makes the rest of its line a comment.
fn pairs(dos: &Dos, header: &str, name: &str) -> Vec<(i64, f64)> {
    let Some(data) = ref_data(dos, header, name).filter(|d| !d.is_empty()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in split_lf(&data).filter(|l| !l.is_empty()) {
        let items: Vec<&str> = line.split(',').collect();
        for k in (0..items.len()).step_by(2) {
            if items[k].is_empty() {
                continue;
            }
            if items.get(k + 1) == Some(&"-") {
                break;
            }
            let Some(frame) = expr::eval(items[k]).filter(|f| f.abs() <= MAX_FRAME) else {
                continue;
            };
            let value = items
                .get(k + 1)
                .filter(|v| !v.is_empty())
                .and_then(|v| expr::eval(v))
                .unwrap_or(1.0);
            out.push((js_round(frame) as i64, value));
        }
    }
    out.sort_by_key(|p| p.0);
    out
}

/// danoniplus's music after a fade-out before the play ends, frames
/// (`C_FRM_AFTERFADE`).
const AFTER_FADE_FRAMES: f64 = 420.0;

/// `transTimerToFrame` (`danoni_main.js` 2505–2525): `m:ss` or `m:ss.ff`
/// (the part after the dot counts frames) as frames, else the number.
fn timer_frames(s: &str) -> Option<f64> {
    let s = s.trim();
    // `Number("")` is 0; the result goes through `parseInt`, which
    // truncates.
    let num = |t: &str| {
        let t = t.trim();
        if t.is_empty() {
            Some(0.0)
        } else {
            t.parse::<f64>().ok()
        }
    };
    let frames = match s.split_once(':') {
        Some((m, rest)) => {
            let (sec, ff) = match rest.split_once('.') {
                Some((sec, ff)) => (num(sec)?, num(ff)?),
                None => (num(rest)?, 0.0),
            };
            60.0 * (num(m)? * 60.0 + sec) + ff
        }
        None => parse_int(s)? as f64,
    };
    frames
        .is_finite()
        .then_some(frames.trunc().clamp(-MAX_FRAME, MAX_FRAME))
}

/// Where a chart's play starts and its music ends.
struct PlaySpan {
    /// `endFrame`, frames.
    end_frame: Option<i64>,
    start: Option<f64>,
    music_start: Option<f64>,
    end: Option<f64>,
    fade: Option<(f64, f64)>,
}

impl PlaySpan {
    /// Chart `i`'s entries (`dosConverter.js` 1434–1446, `mainWindow.js`
    /// 198–225, `dataLoader.js` 1351–1357): `startFrame` and `endFrame`
    /// fall back to the first chart's, `fadeFrame` (`frame,length`) does
    /// not.
    fn of(dos: &Dos, i: usize, timing: &TimingMap, blank: f64) -> PlaySpan {
        let list = |key: &str| dos.val(key).map(split_lf2).unwrap_or_default();
        let own_or_first = |l: &[String]| {
            l.get(i)
                .filter(|v| !v.trim().is_empty())
                .or(l.first())
                .and_then(|v| timer_frames(v))
        };
        let seconds = |f: f64| timing.seconds_at(Tick(f.round() as i64));
        let start_frame = own_or_first(&list("startFrame")).filter(|f| *f > 0.0);
        let start = start_frame.map(seconds);
        // The music joins `blankFrame` frames later, at the start frame's
        // position (`mainWindow.js` 175–176, 1825–1830).
        let music_start = start_frame.map(|f| seconds(f + blank));
        let end_frame = own_or_first(&list("endFrame")).map(|f| f as i64);
        let fade = list("fadeFrame").get(i).and_then(|v| {
            let mut parts = v.split(',');
            let at = timer_frames(parts.next()?)?;
            let length = parts
                .next()
                .and_then(parse_float)
                .filter(|l| l.is_finite() && *l >= 0.0)
                .unwrap_or(AFTER_FADE_FRAMES);
            // The volume drops by 1.26 of itself per `length` frames
            // (`C_FRM_AFTERFADE / fadeOutTerm × 3/1000` a frame).
            Some((seconds(at), length.min(MAX_FRAME) / 60.0 / 1.26))
        });
        PlaySpan {
            end_frame,
            start,
            music_start,
            end: end_frame.map(|f| seconds(f as f64)),
            fade,
        }
    }
}

/// danoniplus's default lyric fade, frames (`C_WOD_FRAME`).
const WORD_FADE_FRAMES: i64 = 30;

/// Lyric cues per chart, at most, and the longest lyric kept.
const MAX_WORDS: usize = 20_000;
const MAX_WORD_BYTES: usize = 1024;

/// Lyric bytes kept per work, all charts together.
const MAX_WORK_WORD_BYTES: usize = 4 << 20;

/// `word_data` as `makeSpriteWordData` reads it (`dataLoader.js`
/// 1081–1136): per line `frame,depth,text` triples, the first text taking
/// in the fields after it up to the next number, or a single
/// `frame,depth,command,fadeFrames`; a depth of `-` ends the line. Each cue
/// is `(frame, [depth, text, fade frames])`, its text plain (see
/// [`lyric_text`]). A chart without its own (`word2_data`) takes the
/// work's (`word_data`), as `getPriorityVal` orders them.
fn words(dos: &Dos, suffix: &str) -> Vec<(i64, Vec<String>)> {
    let Some(data) = [format!("{suffix}_data"), "_data".to_string()]
        .iter()
        .find_map(|name| ref_data(dos, "word", name))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in split_lf(&data).filter(|l| !l.trim().is_empty()) {
        let mut f: Vec<&str> = line.split(',').map(str::trim).collect();
        let mut k = 0;
        while k < f.len() && out.len() < MAX_WORDS {
            if f[k].is_empty() {
                k += 3;
                continue;
            }
            let depth = f.get(k + 1).copied().unwrap_or("");
            if depth == "-" {
                break;
            }
            let Some(frame) = expr::eval(f[k]).filter(|v| v.abs() <= MAX_FRAME) else {
                k += 3;
                continue;
            };
            let depth = expr::eval(depth)
                .filter(|d| (0.0..=255.0).contains(d))
                .map_or(0, |d| d as u8);
            let mut text = f.get(k + 2).copied().unwrap_or("").to_string();
            if k == 0 && f.len() > 3 {
                // Text containing commas: take fields until a number.
                let end = (3..f.len())
                    .find(|&j| f[j].is_empty() || parse_int(f[j]).is_some())
                    .unwrap_or(f.len());
                for part in f.drain(3..end) {
                    text.push(',');
                    text.push_str(part);
                }
            }
            if text.len() > MAX_WORD_BYTES {
                let mut cut = MAX_WORD_BYTES;
                while !text.is_char_boundary(cut) {
                    cut -= 1;
                }
                text.truncate(cut);
            }
            let frame = js_round(frame) as i64;
            if f.len() > 3 && f.len() < 6 {
                let fade = parse_int(f[3])
                    .unwrap_or(WORD_FADE_FRAMES)
                    .clamp(0, 1 << 20);
                out.push((
                    frame,
                    vec![depth.to_string(), lyric_text(&text), fade.to_string()],
                ));
                break;
            }
            out.push((frame, vec![depth.to_string(), lyric_text(&text)]));
            k += 3;
        }
    }
    out
}

/// The stage's width and height in danoniplus's px (`dosConverter.js`,
/// "自動横幅拡張設定" and "横幅設定"): the page's frame (500 px in its
/// templates; a page import does not read it), widened to `windowWidth`
/// and, unless `autoSpread=false`, to the widest minimum of the work's key
/// modes (`g_keyObj.minWidth*`, `danoni_constants.js`: 500 at least, 600
/// for a key mode without its own); 500 high, or `windowHeight`.
fn stage_size(dos: &Dos, difs: &[String]) -> (f64, f64) {
    const FRAME: f64 = 500.0;
    const MIN_WIDTH: f64 = 500.0;
    const MIN_WIDTH_DEFAULT: f64 = 600.0;
    const MIN_WIDTHS: [(&str, f64); 10] = [
        ("5", 500.0),
        ("7i", 550.0),
        ("11", 650.0),
        ("11i", 650.0),
        ("11j", 650.0),
        ("12i", 675.0),
        ("13", 750.0),
        ("16i", 650.0),
        ("17", 825.0),
        ("23", 900.0),
    ];
    let header = |k: &str| dos.val(k).and_then(parse_float).filter(|v| v.is_finite());
    let mut width = header("windowWidth").map_or(FRAME, |w| w.max(FRAME));
    if dos
        .val("autoSpread")
        .is_none_or(|v| !v.trim().eq_ignore_ascii_case("false"))
    {
        width = width.max(MIN_WIDTH);
        for dif in difs {
            let label = dif.split(',').next().unwrap_or("7");
            let key = keys::canonical_label(if label.is_empty() { "7" } else { label });
            let min = MIN_WIDTHS
                .iter()
                .find(|(k, _)| *k == key)
                .map_or(MIN_WIDTH_DEFAULT, |(_, w)| *w);
            width = width.max(min);
        }
    }
    let height = header("windowHeight").map_or(FRAME, |h| h.max(FRAME));
    (width.min(4096.0), height.min(4096.0))
}

/// [`EffectEvent::kind`]s of the back and mask layers' cues
/// (`docs/plans/overlays.md`).
pub const SPRITE_BACK: &str = "danoni:back";
pub const SPRITE_MASK: &str = "danoni:mask";

/// Whether a back or mask source names an image (by extension, as
/// danoniplus's `checkImage` with `g_imgExtensions`); anything else is
/// text.
pub fn is_sprite_image(source: &str) -> bool {
    let name = source.rsplit('/').next().unwrap_or(source);
    name.rsplit_once('.').is_some_and(|(_, e)| {
        ["png", "gif", "bmp", "jpg", "jpeg", "svg"]
            .iter()
            .any(|x| x.eq_ignore_ascii_case(e))
    })
}

/// Back and mask cues per chart and layer, at most, and their bytes per
/// work, all charts together.
const MAX_SPRITES: usize = 20_000;
const MAX_WORK_SPRITE_BYTES: usize = 4 << 20;

/// A back or mask layer's data as the play reads it (`makeBackgroundData`
/// and `makeSpriteData`, `dataLoader.js`, `dosConverter.js`): for chart
/// `suffix`, `<header><variant><suffix>_data`, else the work's
/// `<header><variant>_data` (`variant` is `""`, or `"Rev"` for Reverse,
/// which danoniplus tries first). Each line is
/// `frame,depth,source,class,x,y,w,h,opacity,animation,frames,fillMode`;
/// a line without a depth, or with `-`, is skipped, and so are the title
/// and results screens' `[loop]` and `[jump]`. Each cue is
/// `(frame, [variant, depth, source, class, x, y, w, h, opacity,
/// animation, seconds, fill])`: `depth` a number (0 when it does not
/// evaluate) or `ALL`; `source` empty for a clear, else as danoniplus
/// writes it into the page (`escapeHtml` with `escapeCode`: `<script>`
/// removed, the `*name*` escapes as HTML references, tags kept); `x`, `y`
/// and `h` as written, `w` an integer, `opacity` a number (1 by default),
/// `animation` `none` by default and `seconds` its length (`frames /
/// 60`). Fields are trimmed as `trimStr` trims them (spaces at the ends,
/// every tab). `None` when the work has no such data; `Some` of nothing
/// for data whose lines all are skipped (it still takes the place of the
/// data after it in danoniplus's order).
fn sprites(
    dos: &Dos,
    header: &str,
    variant: &str,
    suffix: &str,
) -> Option<Vec<(i64, Vec<String>)>> {
    let data = [format!("{variant}{suffix}_data"), format!("{variant}_data")]
        .iter()
        .find_map(|name| ref_data(dos, header, name))?;
    let mut out = Vec::new();
    for line in split_lf(&data).filter(|l| !l.trim().is_empty()) {
        if out.len() >= MAX_SPRITES {
            break;
        }
        let f: Vec<String> = line
            .split(',')
            .map(|v| v.replace('\t', "").trim_matches(' ').to_string())
            .collect();
        let field = |k: usize| f.get(k).map_or("", String::as_str);
        let depth = field(1);
        if depth.is_empty() || depth == "-" || matches!(field(2), "[loop]" | "[jump]") {
            continue;
        }
        let frame = if parse_int(field(0)) == Some(0) {
            0.0
        } else {
            expr::eval(field(0)).unwrap_or(200.0)
        };
        if frame.abs() > MAX_FRAME {
            continue;
        }
        let depth = if depth == "ALL" {
            depth.to_string()
        } else {
            match expr::eval(depth).unwrap_or(0.0) {
                d if (0.0..=255.0).contains(&d) => (d as u8).to_string(),
                _ => continue,
            }
        };
        let source = sprite_text(&field(2).replace("<script>", "").replace("</script>", ""));
        let opacity = parse_float(field(8)).unwrap_or(1.0);
        let animation = Some(field(9)).filter(|a| !a.is_empty()).unwrap_or("none");
        let seconds = parse_int(field(10)).unwrap_or(0) as f64 / 60.0;
        let text = |k: usize| field(k).chars().take(MAX_WORD_BYTES).collect::<String>();
        out.push((
            js_round(frame) as i64,
            vec![
                variant.to_string(),
                depth,
                source,
                text(3),
                text(4),
                text(5),
                parse_int(field(6)).unwrap_or(0).to_string(),
                text(7),
                opacity.to_string(),
                animation.chars().take(MAX_WORD_BYTES).collect(),
                seconds.to_string(),
                text(11),
            ],
        ));
    }
    Some(out)
}

/// danoniplus's `*name*` escapes in a back or mask source as it writes
/// them into the page (`escapeHtmlForEnabledTag`, `g_escapeStr.escapeTag`):
/// HTML references the page decodes after its tags.
fn sprite_text(s: &str) -> String {
    const ESCAPE_TAG: [(&str, &str); 12] = [
        ("*amp*", "&amp;"),
        ("*pipe*", "|"),
        ("*dollar*", "$"),
        ("*rsquo*", "&rsquo;"),
        ("*quot*", "&quot;"),
        ("*comma*", "&sbquo;"),
        ("*squo*", "&#39;"),
        ("*bkquo*", "&#96;"),
        ("*lt*", "&lt;"),
        ("*gt*", "&gt;"),
        ("*lbrace*", "{"),
        ("*rbrace*", "}"),
    ];
    let s: String = s.chars().take(MAX_WORD_BYTES).collect();
    ESCAPE_TAG
        .iter()
        .fold(s, |acc, (from, to)| acc.replace(from, to))
}

/// danoniplus's `*name*` escapes (`g_escapeStr.escapeTag`).
const ESCAPES: [(&str, &str); 12] = [
    ("*amp*", "&"),
    ("*pipe*", "|"),
    ("*dollar*", "$"),
    ("*rsquo*", "\u{2019}"),
    ("*quot*", "\""),
    ("*comma*", ","),
    ("*squo*", "'"),
    ("*bkquo*", "`"),
    ("*lt*", "<"),
    ("*gt*", ">"),
    ("*lbrace*", "{"),
    ("*rbrace*", "}"),
];

/// A lyric as plain text: danoniplus's `*name*` escapes, `<br>` as a line
/// break, other tags dropped, character references decoded.
fn lyric_text(s: &str) -> String {
    let mut plain = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('<') {
        plain.push_str(&rest[..p]);
        let Some(end) = rest[p..].find('>') else {
            plain.push_str(&rest[p..]);
            rest = "";
            break;
        };
        let tag = rest[p + 1..p + end].trim().to_ascii_lowercase();
        if tag == "br" || tag == "br/" || tag == "br /" {
            plain.push('\n');
        }
        rest = &rest[p + end + 1..];
    }
    plain.push_str(rest);
    let plain = source::decode_entities(&plain);
    ESCAPES
        .iter()
        .fold(plain, |acc, (from, to)| acc.replace(from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charts_of_a_former_key_mode_keep_their_places() {
        // 7i, played until 2026-10-13: works stored before refer to the
        // charts after it by position.
        let text = "|difData=7,A$7i,B$7,C|left_data=300|left3_data=320|";
        let i = import(&Dos::parse(text, DosFlags::default())).unwrap();
        let song = &i.songs[0];
        assert_eq!(song.charts.len(), 3);
        assert_eq!(song.charts[2].name, "C");
        assert_eq!(song.layout_of(&song.charts[1]), None);
        assert_eq!(song.playable_charts().count(), 2);
        assert_eq!(i.charts, vec![Some((0, 0)), None, Some((0, 2))]);
        assert!(i.warnings.iter().any(|w| w.contains("7i keys")));
        // A work of 7i charts only is not playable.
        assert!(
            import(&Dos::parse(
                "|difData=7i,A|left_data=300|",
                DosFlags::default()
            ))
            .is_err()
        );
        // Nor is a song (music) of them alone.
        let text =
            "|musicUrl=a.ogg$b.ogg|musicNo=0$1|difData=7,A$7i,B|left_data=300|left2_data=300|";
        let i = import(&Dos::parse(text, DosFlags::default())).unwrap();
        assert_eq!(i.songs.len(), 1);
        assert_eq!(i.charts, vec![Some((0, 0)), None]);
    }

    fn import_text(text: &str) -> Import {
        import(&Dos::parse(text, DosFlags::default())).unwrap()
    }

    #[test]
    fn frames_map_onto_ticks_with_the_blank_offset() {
        let i = import_text(
            "|musicTitle=Song,Artist,https://example.org,,150|musicUrl=song.mp3|\
             |difData=5,Normal::maker::7,4.5|blankFrame=180|adjustment=2.7|\
             |left_data=300,240.5\n,360|space_data=300|frzDown_data=400,460,500,520|",
        );
        assert!(i.warnings.is_empty(), "{:?}", i.warnings);
        let song = &i.songs[0];
        assert_eq!(song.title, "Song");
        assert_eq!(song.artist, "Artist");
        assert_eq!(song.music.as_deref(), Some("song.mp3"));
        assert_eq!(song.display_bpm, DisplayBpm::Single(150.0));
        let c = &song.charts[0];
        assert_eq!(c.layout, "danoni-5");
        assert_eq!(c.meter, 7);
        assert_eq!(c.credit, "maker");
        assert_eq!(c.danoni.as_ref().unwrap().init_speed, 4.5);
        let ticks: Vec<(i64, u8)> = c.notes.iter().map(|n| (n.tick.0, n.lane)).collect();
        // 240.5 rounds up; lines join before splitting ("360" stays).
        assert_eq!(
            ticks,
            [(241, 0), (300, 0), (300, 4), (360, 0), (400, 1), (500, 1)]
        );
        assert_eq!(c.notes[4].kind, NoteKind::HoldHead { end: Tick(460) });
        // Frame F sounds at (F − blank + floor(adjustment)) / 60 s.
        let t = c.timing(song);
        for f in [0, 241, 300, 460] {
            let expected = (f as f64 - 180.0 + 2.0) / 60.0;
            assert!((t.seconds_at(Tick(f)) - expected).abs() < 1e-9, "{f}");
        }
    }

    #[test]
    fn several_charts_suffixes_and_music() {
        let i = import_text(
            "|musicTitle=A,x$B,y|musicUrl=a.ogg$b.ogg|musicNo=0$1$0|\
             |difData=5,Easy$7,Hard$9A,Expert|blankFrame=200$100|\
             |left_data=210|left2_data=220|left3_data=230|space2_data=240|",
        );
        assert_eq!(i.songs.len(), 2);
        let (a, b) = (&i.songs[0], &i.songs[1]);
        assert_eq!((a.title.as_str(), b.title.as_str()), ("A", "B"));
        assert_eq!(b.music.as_deref(), Some("b.ogg"));
        assert_eq!(a.charts.len(), 2);
        assert_eq!(a.charts[0].notes[0].tick, Tick(210));
        assert_eq!(a.charts[1].layout, "danoni-9A");
        assert_eq!(a.charts[1].notes[0].tick, Tick(230));
        let seven = &b.charts[0];
        assert_eq!(seven.notes.len(), 2);
        // The second chart has its own blankFrame.
        let t = seven.timing.as_ref().unwrap();
        assert!((t.seconds_at(Tick(100))).abs() < 1e-9);
    }

    #[test]
    fn back_and_mask_layers_are_read_as_the_play_reads_them() {
        // danoniplus's sample work 1, with a mask, a second chart, a
        // Reverse version and lines the play skips.
        let i = import_text(
            "|difData=5,N$5,H|left_data=300|left2_data=300|\
             |back_data=200,0,../img/classic/c.png,,20,20,200,200,0.5,leftToRightFade,120\n\
             400,2,文字*comma*テスト,,100,140,36,0,0,fromBig,100\n\
             300,0\n0,ALL\n500,-,x.png\n600,,[loop],0\n|\
             |backRev_data=10,0,rev.png|mask2_data=700,1,m.png,cls,{g_sWidth/2},5|",
        );
        let song = &i.songs[0];
        let cues = |kind: &str, chart: u8| -> Vec<Vec<String>> {
            song.effects
                .iter()
                .filter(|e| e.kind == kind && e.layer == chart)
                .map(|e| e.fields.clone())
                .collect()
        };
        let back = cues(SPRITE_BACK, 0);
        // Four cues, Reverse's marker and its cue.
        assert_eq!(back.len(), 6);
        assert_eq!(
            back[0],
            [
                "",
                "0",
                "../img/classic/c.png",
                "",
                "20",
                "20",
                "200",
                "200",
                "0.5",
                "leftToRightFade",
                "2",
                ""
            ]
        );
        // Escapes as the page gets them.
        assert_eq!(back[1][2], "文字&sbquo;テスト");
        assert_eq!((back[2][1].as_str(), back[2][2].as_str()), ("0", ""));
        assert_eq!(back[3][1], "ALL");
        assert_eq!(
            (back[5][0].as_str(), back[5][2].as_str()),
            ("Rev", "rev.png")
        );
        assert_eq!(back[4], ["Rev"]);
        // The second chart: the shared back data, and its own mask.
        assert_eq!(cues(SPRITE_BACK, 1).len(), 6);
        let mask = cues(SPRITE_MASK, 1);
        assert_eq!(mask.len(), 1);
        assert_eq!(mask[0][4], "{g_sWidth/2}");
        assert_eq!(mask[0][9], "none");
        assert!(cues(SPRITE_MASK, 0).is_empty());
        // The stage: 5 keys keep the 500 px frame; a 7-key chart widens
        // it to 600, `windowWidth` further.
        let stage = |i: &Import| i.songs[0].charts[0].danoni.as_ref().unwrap().stage;
        assert_eq!(stage(&i), Some((500.0, 500.0)));
        let seven = import_text("|difData=5,N$7,H|left_data=300|left2_data=300|");
        assert_eq!(stage(&seven), Some((600.0, 500.0)));
        let wide = import_text("|difData=5,N|windowWidth=800|windowHeight=600|left_data=300|");
        assert_eq!(stage(&wide), Some((800.0, 600.0)));
    }

    #[test]
    fn speed_boost_dummy_and_names() {
        let i = import_text(
            "|difData=7,N|dummyId=2|\
             |left_data=500,900|leftdia_data=600|frzLdia_data=700,800|\
             |left2_data=550|\
             |speed_data=speed_x|speed_x=60*5,0.5,600,-\n700,2|\
             |boost_data=650,3|",
        );
        let c = &i.songs[0].charts[0];
        let t = c.timing.as_ref().unwrap();
        assert_eq!(
            t.scrolls
                .iter()
                .map(|s| (s.tick.0, s.ratio))
                .collect::<Vec<_>>(),
            [(300, 0.5), (700, 2.0)]
        );
        let at = |tick: i64, lane: u8| {
            c.notes
                .iter()
                .find(|n| n.tick.0 == tick && n.lane == lane)
                .unwrap()
        };
        assert_eq!(at(550, 0).kind, NoteKind::Dummy);
        assert_eq!(at(600, 1).speed_mul, None);
        assert_eq!(at(700, 1).speed_mul, Some(3.0));
        assert_eq!(at(700, 1).kind, NoteKind::HoldHead { end: Tick(800) });
        assert_eq!(frz_name("sleft"), "sfrzLeft");
        assert_eq!(frz_name("leftdia"), "frzLdia");
        assert_eq!(frz_name("oni"), "foni");
        assert_eq!(frz_name("1x"), "frz1x");
        assert_eq!(frz_name("siyo"), "sfrzIyo");
    }

    #[test]
    fn speed_change_wins_and_only_played_modes_import() {
        let i = import_text(
            "|difData=5,N|speed_data=100,2|speed_change=200,3|left_data=300|frzDown_data=310,320|",
        );
        let c = &i.songs[0].charts[0];
        assert_eq!(c.timing.as_ref().unwrap().scrolls[0].tick, Tick(200));
        assert_eq!(c.notes.len(), 2);
        // A work's own key mode, a redefined one and a mode the game does
        // not play are reported, not imported.
        let r = import(&Dos::parse(
            "|difData=4x,N$7,R$11,E$9B,O|keyCtrl4x=D,F,J,K|keyCtrl7=S,D,F,G,H,J,K|\
             |left4_data=300|",
            DosFlags::default(),
        ))
        .unwrap();
        assert_eq!(r.charts, [None, None, None, Some((0, 0))]);
        assert!(r.warnings[0].contains("4x keys is not a key mode this game plays"));
        assert!(r.warnings[1].contains("redefines 7 keys"));
        assert!(r.warnings[2].contains("11 keys is not"));
    }

    #[test]
    fn review_fixes() {
        // Fields a song's musicTitle row leaves empty come from the first row.
        let i = import_text(
            "|musicTitle=A,Art,,,180$B|musicUrl=a.ogg$b.ogg|musicNo=0$1|difData=5,E$5,H|\
             |left_data=300|left2_data=400|",
        );
        assert_eq!(i.songs[1].artist, "Art");
        assert_eq!(i.songs[1].display_bpm, DisplayBpm::Single(180.0));
        // Huge adjustments and frames neither overflow nor saturate.
        let i = import_text(
            "|difData=5,N|adjustment=-1e300|blankFrame=99999999999999999999|\
             |left_data=1e300,300,-1e300|speed_data=1e300,2|word_data=1e300,0,x|",
        );
        let c = &i.songs[0].charts[0];
        assert_eq!(c.notes.len(), 1);
        assert!(c.timing.as_ref().unwrap().offset_seconds.is_finite());
        assert!(c.timing.as_ref().unwrap().scrolls.is_empty());
        // dummyId is a number: `02` is chart 2's arrays, text is none.
        let i = import_text("|difData=5,N|dummyId=02|left_data=300|left2_data=310|");
        assert!(
            i.songs[0].charts[0]
                .notes
                .iter()
                .any(|n| n.kind == NoteKind::Dummy)
        );
        let i = import_text("|difData=5,N|dummyId=abc|left_data=300|leftabc_data=310|");
        assert!(
            i.songs[0].charts[0]
                .notes
                .iter()
                .all(|n| n.kind != NoteKind::Dummy)
        );
        // A long lyric is cut; its comma-separated parts are joined once.
        let long = "あ,".repeat(1000);
        let i = import_text(&format!(
            "|difData=5,N|left_data=300|word_data=200,0,{long}|"
        ));
        let text = &i.songs[0].effects[0].fields[1];
        assert!(text.len() <= MAX_WORD_BYTES && text.starts_with("あ,あ"));

        // Where a play starts and the music ends: per chart, `m:ss.ff`
        // timers, the first chart's start and end for the others, but not
        // its fade.
        let i = import_text(
            "|difData=5,A$5,B|blankFrame=140|startFrame=0$200|endFrame=0:05.30|\
             |fadeFrame=$440,120|left_data=300,500|left2_data=300,800|",
        );
        let span = |k: usize| {
            let d = i.songs[0].charts[k].danoni.clone().unwrap();
            (d.start, d.end, d.fade)
        };
        // endFrame 5 s 30 frames = 330 frames: (330 − 140) / 60 s.
        let end = Some(190.0 / 60.0);
        assert_eq!(span(0), (None, end, None));
        assert_eq!(span(1), (Some(1.0), end, Some((5.0, 2.0 / 1.26))));
        let music = i.songs[0].charts[1].danoni.as_ref().unwrap().music_start;
        assert!(music.is_some_and(|m| (m - 200.0 / 60.0).abs() < 1e-9));
        assert_eq!(timer_frames(":30"), Some(1800.0));
        assert_eq!(timer_frames("330.6"), Some(330.0));
        assert_eq!(i.songs[0].charts[0].notes.len(), 1);
        assert!(i.warnings.iter().any(|w| w.contains("after endFrame")));

        // Lyrics are timed in seconds of their chart.
        let i = import_text("|difData=5,N|blankFrame=140|left_data=300|word_data=200,0,hello|");
        assert_eq!(i.songs[0].effects[0].at, EffectTime::Seconds(1.0));
        // A chart without lyrics of its own takes the work's
        // (`getPriorityVal`); one with its own keeps them.
        let i = import_text(
            "|difData=5,N$5,H$5,X|blankFrame=140|left_data=300|left2_data=300|\
             left3_data=300|word_data=200,0,shared|word3_data=200,0,own|",
        );
        let lyric = |chart: u8| -> Vec<String> {
            i.songs[0]
                .effects
                .iter()
                .filter(|e| e.kind == "danoni:lyric" && e.layer == chart)
                .map(|e| e.fields[1].clone())
                .collect()
        };
        assert_eq!(lyric(0), ["shared"]);
        assert_eq!(lyric(1), ["shared"]);
        assert_eq!(lyric(2), ["own"]);
        // Several cues on a line, commas in a text, commands with a fade,
        // markup and escapes.
        let i = import_text(
            "|difData=5,N|blankFrame=140|left_data=300|word_data=\
             200,0,a, b,260,1,c<br>d*amp*e\n\
             230,1,[fadeout],60\n\
             240,0,*lt*x*gt*<span>y</span>,320,-\n|",
        );
        // Fields are trimmed (`trimStr`); times in milliseconds.
        let cues: Vec<(i64, Vec<&str>)> = i.songs[0]
            .effects
            .iter()
            .map(|e| match e.at {
                EffectTime::Seconds(s) => (
                    (s * 1000.0).round() as i64,
                    e.fields.iter().map(String::as_str).collect(),
                ),
                EffectTime::Beat(_) => unreachable!(),
            })
            .collect();
        assert_eq!(
            cues,
            vec![
                (1000, vec!["0", "a,b"]),
                (2000, vec!["1", "c\nd&e"]),
                (1500, vec!["1", "[fadeout]", "60"]),
                // Four or five fields read as a command and its fade,
                // as in danoniplus.
                (1667, vec!["0", "<x>y", "320"]),
            ]
        );
        // A reference repeated on many lines stays bounded.
        let big = "1,2,".repeat(100_000);
        let many = "speed_x\n".repeat(1000);
        let i = import_text(&format!(
            "|difData=5,N|left_data=300|speed_x={big}|speed_data={many}|"
        ));
        assert!(
            !i.songs[0].charts[0]
                .timing
                .as_ref()
                .unwrap()
                .scrolls
                .is_empty()
        );
        assert!(
            ref_data(
                &Dos::parse(
                    &format!("|speed_x={big}|speed_data={many}|"),
                    DosFlags::default()
                ),
                "speed",
                "_data"
            )
            .unwrap()
            .len()
                <= MAX_REF_BYTES
        );
    }

    #[test]
    fn unplayable_charts_are_reported() {
        let r = import(&Dos::parse("|difData=99q,N|", DosFlags::default()));
        assert!(
            r.unwrap_err()
                .contains("99q keys is not a key mode this game plays")
        );
        let i = import_text("|difData=5,N$99q,M|playbackRate=1.5|left_data=300|left_data=310|");
        assert_eq!(i.songs[0].charts.len(), 1);
        assert!(i.warnings.iter().any(|w| w.contains("99q")));
        assert!(i.warnings.iter().any(|w| w.contains("playbackRate")));
        // The later field wins.
        assert_eq!(i.songs[0].charts[0].notes[0].tick, Tick(310));
    }
}
