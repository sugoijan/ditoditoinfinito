//! StepMania `.sm` and `.ssc` importers.
//!
//! Follows StepMania 5.1 (`NotesLoaderSM.cpp`, `NotesLoaderSSC.cpp`,
//! `NoteDataUtil.cpp`) as documented in `docs/research/formats-sm-ssc-dwi.md`:
//!
//! - `#OFFSET` is kept as-is (`time(beat 0) = -offset`).
//! - `.sm` negative BPMs and negative stops are converted to warps with
//!   `SMLoader::ProcessBPMsAndStops`; `.ssc` runs the same conversion only
//!   when negatives appear (StepMania would drop them).
//! - `.ssc` split timing (`#VERSION >= 0.70`): a chart with any timing tag
//!   gets its own [`TimingMap`] seeded with the song offset only.
//! - `.ssc` `#WARPS` before version 0.70 are absolute end beats.
//! - Note data: 4-beat measures split by `,`, rows by newline, one character
//!   per column, `[n]` keysound and `{...}` attack suffixes, `&` routine
//!   separator (only the first player is kept for now).

use std::collections::HashMap;

use crate::formats::ParseError;
use crate::formats::msd::{MsdTag, parse_msd};
use crate::layout::Layout;
use crate::model::{
    Chart, Difficulty, DisplayBpm, EffectEvent, EffectTime, Note, NoteKind, Song, SourceFormat,
    SourceInfo, Tick,
};
use crate::timing::{
    BpmSegment, ComboSegment, FakeSegment, Label, ScrollSegment, SpeedSegment, SpeedUnit,
    StopSegment, TickCount, TimeSignature, TimingMap, WarpSegment,
};

/// `NotesLoaderSM.h`: BPMs above this are SM4-style "infinite" warps.
const FAST_BPM_WARP: f64 = 9_999_999.0;
/// `NotesLoaderSSC.h` thresholds.
const VERSION_SPLIT_TIMING: f32 = 0.70;
const VERSION_CHART_NAME_TAG: f32 = 0.74;
/// `STEPFILE_VERSION_NUMBER`: what StepMania assumes when `#VERSION` is absent.
const CURRENT_SSC_VERSION: f32 = 0.83;
/// StepMania's `ROWS_PER_BEAT` upper bound for `#TICKCOUNTS`.
const MAX_TICKS_PER_BEAT: u32 = 48;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flavor {
    Sm,
    Ssc,
}

/// Parse a StepMania `.sm` file.
pub fn parse_sm(text: &str) -> Result<Song, ParseError> {
    parse(text, Flavor::Sm)
}

/// Parse a StepMania `.ssc` file.
pub fn parse_ssc(text: &str) -> Result<Song, ParseError> {
    parse(text, Flavor::Ssc)
}

/// Dispatch on the file extension (`sm`/`ssc`, with or without the dot,
/// any case).
pub fn parse_simfile(text: &str, extension: &str) -> Result<Song, ParseError> {
    let ext = extension.trim_start_matches('.').to_ascii_lowercase();
    match ext.as_str() {
        "sm" => parse_sm(text),
        "ssc" => parse_ssc(text),
        other => Err(ParseError::Malformed(format!(
            "unsupported simfile extension `{other}`"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Number parsing with StepMania (`strtof`/`atoi`) semantics.
// ---------------------------------------------------------------------------

/// Longest numeric prefix as `f64`, `0.0` when none (`StringToFloat`).
fn sm_float(s: &str) -> f64 {
    let s = s.trim();
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let digits_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut int_digits = i - digits_start;
    if i < b.len() && b[i] == b'.' {
        let mut j = i + 1;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        let frac = j - (i + 1);
        if int_digits + frac > 0 {
            i = j;
            int_digits += frac;
        }
    }
    if int_digits == 0 {
        return 0.0;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    s[..i].parse().unwrap_or(0.0)
}

/// Longest integer prefix, `0` when none (`atoi`).
fn sm_int(s: &str) -> i64 {
    let s = s.trim();
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    s[..i].parse().unwrap_or(0)
}

/// `SMLoader::RowToBeat`: a trailing `r`/`R` means the value is in rows
/// (48 per beat) instead of beats.
fn row_to_beat(s: &str) -> f64 {
    let t = s.trim();
    if let Some(rows) = t.strip_suffix(['r', 'R']) {
        sm_float(rows) / 48.0
    } else {
        sm_float(t)
    }
}

/// Split a `beat=v1[=v2...]` comma list into trimmed field vectors.
fn split_entries(value: &str) -> Vec<Vec<&str>> {
    value
        .split(',')
        .map(|e| e.split('=').map(str::trim).collect::<Vec<_>>())
        .filter(|fields| !(fields.len() == 1 && fields[0].is_empty()))
        .collect()
}

// ---------------------------------------------------------------------------
// Timing accumulation.
// ---------------------------------------------------------------------------

/// Raw timing tags of a song or chart before conversion into a [`TimingMap`].
#[derive(Clone, Debug, Default)]
struct TimingAcc {
    offset: f64,
    /// `(beat, bpm)` with zero BPMs already dropped.
    bpms: Vec<(f64, f64)>,
    /// `(beat, seconds)` with zero stops already dropped.
    stops: Vec<(f64, f64)>,
    delays: Vec<StopSegment>,
    warps: Vec<WarpSegment>,
    speeds: Vec<SpeedSegment>,
    scrolls: Vec<ScrollSegment>,
    fakes: Vec<FakeSegment>,
    time_signatures: Vec<TimeSignature>,
    tick_counts: Vec<TickCount>,
    combos: Vec<ComboSegment>,
    labels: Vec<Label>,
    /// Whether any timing tag (including `#OFFSET`) was seen.
    touched: bool,
}

impl TimingAcc {
    /// Whether at least one timing *segment* was read. StepMania only uses a
    /// chart's own timing when `TimingData::empty()` is false, so a chart
    /// with just `#OFFSET` falls back to the song timing.
    fn has_segments(&self) -> bool {
        !(self.bpms.is_empty()
            && self.stops.is_empty()
            && self.delays.is_empty()
            && self.warps.is_empty()
            && self.speeds.is_empty()
            && self.scrolls.is_empty()
            && self.fakes.is_empty()
            && self.time_signatures.is_empty()
            && self.tick_counts.is_empty()
            && self.combos.is_empty()
            && self.labels.is_empty())
    }

    /// Returns `true` when `name` was a timing tag and has been consumed.
    fn apply(&mut self, name: &str, value: &str, flavor: Flavor, version: f32) -> bool {
        match name {
            "OFFSET" => self.offset = sm_float(value),
            "BPMS" => {
                self.bpms.clear();
                for f in split_entries(value) {
                    if f.len() != 2 {
                        continue;
                    }
                    let bpm = sm_float(f[1]);
                    if bpm != 0.0 {
                        self.bpms.push((row_to_beat(f[0]), bpm));
                    }
                }
            }
            "STOPS" | "FREEZES" => {
                self.stops.clear();
                for f in split_entries(value) {
                    if f.len() != 2 {
                        continue;
                    }
                    let secs = sm_float(f[1]);
                    if secs != 0.0 {
                        self.stops.push((row_to_beat(f[0]), secs));
                    }
                }
            }
            "DELAYS" => {
                for f in split_entries(value) {
                    if f.len() != 2 {
                        continue;
                    }
                    let seconds = sm_float(f[1]);
                    if seconds > 0.0 {
                        self.delays.push(StopSegment {
                            tick: Tick::from_beat_f64(row_to_beat(f[0])),
                            seconds,
                        });
                    }
                }
            }
            "WARPS" => {
                for f in split_entries(value) {
                    if f.len() != 2 {
                        continue;
                    }
                    let beat = sm_float(f[0]);
                    let v = sm_float(f[1]);
                    // Before split timing the second value was the absolute
                    // destination beat.
                    let length =
                        if flavor == Flavor::Ssc && version < VERSION_SPLIT_TIMING && v > beat {
                            v - beat
                        } else {
                            v
                        };
                    if length > 0.0 {
                        self.warps.push(WarpSegment {
                            tick: Tick::from_beat_f64(beat),
                            length: Tick::from_beat_f64(length),
                        });
                    }
                }
            }
            "SPEEDS" => {
                for mut f in split_entries(value) {
                    // 2- and 3-value entries are padded with 0.
                    if f.len() == 2 {
                        f.push("0");
                    }
                    if f.len() == 3 {
                        f.push("0");
                    }
                    if f.len() < 4 {
                        continue;
                    }
                    let beat = row_to_beat(f[0]);
                    let ratio = sm_float(f[1]);
                    let delay = sm_float(f[2]);
                    let unit = if sm_int(f[3]) == 0 {
                        SpeedUnit::Beats
                    } else {
                        SpeedUnit::Seconds
                    };
                    if beat < 0.0 || delay < 0.0 {
                        continue;
                    }
                    self.speeds.push(SpeedSegment {
                        tick: Tick::from_beat_f64(beat),
                        ratio,
                        delay,
                        unit,
                    });
                }
            }
            "SCROLLS" => {
                for f in split_entries(value) {
                    if f.len() < 2 {
                        continue;
                    }
                    let beat = sm_float(f[0]);
                    if beat < 0.0 {
                        continue;
                    }
                    self.scrolls.push(ScrollSegment {
                        tick: Tick::from_beat_f64(beat),
                        ratio: sm_float(f[1]),
                    });
                }
            }
            "FAKES" => {
                for f in split_entries(value) {
                    if f.len() != 2 {
                        continue;
                    }
                    let length = sm_float(f[1]);
                    if length > 0.0 {
                        self.fakes.push(FakeSegment {
                            tick: Tick::from_beat_f64(row_to_beat(f[0])),
                            length: Tick::from_beat_f64(length),
                        });
                    }
                }
            }
            "TIMESIGNATURES" => {
                let mut segs = Vec::new();
                for f in split_entries(value) {
                    if f.len() < 3 {
                        continue;
                    }
                    let beat = row_to_beat(f[0]);
                    let num = sm_int(f[1]);
                    let den = sm_int(f[2]);
                    if beat < 0.0 || num < 1 || den < 1 {
                        continue;
                    }
                    segs.push(TimeSignature {
                        tick: Tick::from_beat_f64(beat),
                        numerator: num as u32,
                        denominator: den as u32,
                    });
                }
                if segs.first().is_some_and(|s| s.tick > Tick::ZERO) {
                    self.time_signatures.push(TimeSignature {
                        tick: Tick::ZERO,
                        numerator: 4,
                        denominator: 4,
                    });
                }
                self.time_signatures.extend(segs);
            }
            "TICKCOUNTS" => {
                for f in split_entries(value) {
                    if f.len() != 2 {
                        continue;
                    }
                    let ticks = sm_int(f[1]).clamp(0, MAX_TICKS_PER_BEAT as i64) as u32;
                    self.tick_counts.push(TickCount {
                        tick: Tick::from_beat_f64(row_to_beat(f[0])),
                        ticks_per_beat: ticks,
                    });
                }
            }
            "COMBOS" => {
                for f in split_entries(value) {
                    if f.len() < 2 {
                        continue;
                    }
                    let combo = sm_int(f[1]).max(0) as u32;
                    let miss_combo = if f.len() == 2 {
                        combo
                    } else {
                        sm_int(f[2]).max(0) as u32
                    };
                    self.combos.push(ComboSegment {
                        tick: Tick::from_beat_f64(sm_float(f[0])),
                        combo,
                        miss_combo,
                    });
                }
            }
            "LABELS" => {
                for f in split_entries(value) {
                    if f.len() != 2 {
                        continue;
                    }
                    let beat = sm_float(f[0]);
                    if beat < 0.0 {
                        continue;
                    }
                    self.labels.push(Label {
                        tick: Tick::from_beat_f64(beat),
                        text: f[1].to_string(),
                    });
                }
            }
            _ => return false,
        }
        self.touched = true;
        true
    }

    /// Convert into a tidy [`TimingMap`].
    fn build(self, flavor: Flavor) -> TimingMap {
        let mut map = TimingMap {
            offset_seconds: self.offset,
            bpms: Vec::new(),
            stops: Vec::new(),
            delays: self.delays,
            warps: self.warps,
            speeds: self.speeds,
            scrolls: self.scrolls,
            fakes: self.fakes,
            time_signatures: self.time_signatures,
            tick_counts: self.tick_counts,
            combos: self.combos,
            labels: self.labels,
        };
        let has_negative = self
            .bpms
            .iter()
            .any(|&(_, bpm)| bpm < 0.0 || bpm > FAST_BPM_WARP)
            || self.stops.iter().any(|&(_, s)| s < 0.0);
        if flavor == Flavor::Sm || has_negative {
            process_bpms_and_stops(&mut map, self.bpms, self.stops);
        } else {
            // `SSCLoader::ProcessBPMs/ProcessStops`: beat >= 0 and value > 0.
            for (beat, bpm) in self.bpms {
                if beat >= 0.0 && bpm > 0.0 {
                    map.bpms.push(BpmSegment {
                        tick: Tick::from_beat_f64(beat),
                        bpm,
                    });
                }
            }
            for (beat, seconds) in self.stops {
                if beat >= 0.0 && seconds > 0.0 {
                    map.stops.push(StopSegment {
                        tick: Tick::from_beat_f64(beat),
                        seconds,
                    });
                }
            }
        }
        map.tidy();
        map
    }
}

/// `SMLoader::ProcessBPMsAndStops`: merge BPM changes and stops in beat
/// order (BPMs first on ties) and turn negative BPMs, "infinite" BPMs and
/// negative stops into warps.
fn process_bpms_and_stops(
    out: &mut TimingMap,
    mut bpms: Vec<(f64, f64)>,
    mut stops: Vec<(f64, f64)>,
) {
    bpms.sort_by(|a, b| a.0.total_cmp(&b.0));
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));

    let is_warp_bpm = |bpm: f64| bpm < 0.0 || bpm > FAST_BPM_WARP;

    // Stops before beat 0 only move the offset.
    let mut istop = 0;
    while istop < stops.len() && stops[istop].0 < 0.0 {
        out.offset_seconds -= stops[istop].1;
        istop += 1;
    }

    // BPMs at or before beat 0: the last one wins.
    let mut bpm = 0.0;
    let mut ibpm = 0;
    while ibpm < bpms.len() && bpms[ibpm].0 <= 0.0 {
        bpm = bpms[ibpm].1;
        ibpm += 1;
    }
    if bpm == 0.0 {
        if ibpm >= bpms.len() {
            bpm = 60.0;
        } else {
            // StepMania skips one entry here and takes the following BPM
            // (a quirk we reproduce, including its fallback).
            ibpm += 1;
            bpm = bpms.get(ibpm).map(|b| b.1).unwrap_or(60.0);
        }
    }
    if bpm > 0.0 && bpm <= FAST_BPM_WARP {
        out.bpms.push(BpmSegment {
            tick: Tick::ZERO,
            bpm,
        });
    }

    let mut prevbeat = 0.0;
    let mut warpstart = -1.0_f64;
    let mut prewarpbpm = 0.0;
    let mut timeofs = 0.0;

    while ibpm < bpms.len() || istop < stops.len() {
        let change_is_bpm =
            istop >= stops.len() || (ibpm < bpms.len() && bpms[ibpm].0 <= stops[istop].0);
        let (beat, value) = if change_is_bpm {
            bpms[ibpm]
        } else {
            stops[istop]
        };

        // Time elapsed at the current BPM (none for "infinite" BPMs).
        if bpm <= FAST_BPM_WARP {
            timeofs += (beat - prevbeat) * 60.0 / bpm;
            if warpstart >= 0.0 && bpm > 0.0 && timeofs > 0.0 {
                let warpend = beat - timeofs * bpm / 60.0;
                out.warps.push(WarpSegment {
                    tick: Tick::from_beat_f64(warpstart),
                    length: Tick::from_beat_f64(warpend - warpstart),
                });
                if bpm != prewarpbpm {
                    out.bpms.push(BpmSegment {
                        tick: Tick::from_beat_f64(warpstart),
                        bpm,
                    });
                }
                warpstart = -1.0;
            }
        }
        prevbeat = beat;

        if change_is_bpm {
            if warpstart < 0.0 && is_warp_bpm(value) {
                warpstart = beat;
                prewarpbpm = bpm;
                timeofs = 0.0;
            } else if warpstart < 0.0 {
                out.bpms.push(BpmSegment {
                    tick: Tick::from_beat_f64(beat),
                    bpm: value,
                });
            }
            bpm = value;
            ibpm += 1;
        } else {
            if warpstart < 0.0 && value < 0.0 {
                warpstart = beat;
                prewarpbpm = bpm;
                timeofs = value;
            } else if warpstart < 0.0 {
                out.stops.push(StopSegment {
                    tick: Tick::from_beat_f64(beat),
                    seconds: value,
                });
            } else {
                timeofs += value;
                if value > 0.0 && timeofs > 0.0 {
                    // The stop overshoots the deficit: the warp ends here and
                    // the excess becomes a real stop.
                    out.warps.push(WarpSegment {
                        tick: Tick::from_beat_f64(warpstart),
                        length: Tick::from_beat_f64(beat - warpstart),
                    });
                    out.stops.push(StopSegment {
                        tick: Tick::from_beat_f64(beat),
                        seconds: timeofs,
                    });
                    if is_warp_bpm(bpm) {
                        warpstart = beat;
                        timeofs = 0.0;
                    } else {
                        if bpm != prewarpbpm {
                            out.bpms.push(BpmSegment {
                                tick: Tick::from_beat_f64(warpstart),
                                bpm,
                            });
                        }
                        warpstart = -1.0;
                    }
                }
            }
            istop += 1;
        }
    }

    if warpstart >= 0.0 {
        let warpend = if is_warp_bpm(bpm) {
            99_999_999.0
        } else {
            prevbeat - timeofs * bpm / 60.0
        };
        out.warps.push(WarpSegment {
            tick: Tick::from_beat_f64(warpstart),
            length: Tick::from_beat_f64(warpend - warpstart),
        });
        if bpm != prewarpbpm {
            out.bpms.push(BpmSegment {
                tick: Tick::from_beat_f64(warpstart),
                bpm,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Difficulty, steps types.
// ---------------------------------------------------------------------------

/// `OldStyleStringToDifficulty` (DWI-compatible aliases) plus the plain names.
fn parse_difficulty(s: &str) -> Result<Difficulty, ParseError> {
    Ok(match s.trim().to_ascii_lowercase().as_str() {
        "beginner" => Difficulty::Beginner,
        "easy" | "basic" | "light" => Difficulty::Easy,
        "medium" | "another" | "trick" | "standard" | "difficult" => Difficulty::Medium,
        "hard" | "ssr" | "maniac" | "heavy" => Difficulty::Hard,
        "challenge" | "smaniac" | "expert" | "oni" => Difficulty::Challenge,
        "edit" => Difficulty::Edit,
        other => {
            return Err(ParseError::Malformed(format!(
                "unknown difficulty `{other}`"
            )));
        }
    })
}

/// Legacy `.sm` rule: a Hard chart described as `smaniac`/`challenge` is a
/// Challenge chart.
fn apply_legacy_challenge(difficulty: Difficulty, description: &str) -> Difficulty {
    if difficulty == Difficulty::Hard
        && (description.eq_ignore_ascii_case("smaniac")
            || description.eq_ignore_ascii_case("challenge"))
    {
        Difficulty::Challenge
    } else {
        difficulty
    }
}

/// Normalise legacy steps-type spellings (`SMLoader::LoadFromTokens`).
fn normalize_steps_type(s: &str) -> String {
    match s.trim() {
        "ez2-single-hard" => "ez2-single".to_string(),
        "para" => "para-single".to_string(),
        other => other.to_string(),
    }
}

/// Column count of StepMania steps types without a built-in [`Layout`]
/// (`GameManager.cpp`), used to truncate over-long rows like StepMania does.
fn known_lane_count(steps_type: &str) -> Option<usize> {
    if let Some(l) = Layout::builtin(steps_type) {
        return Some(l.lane_count());
    }
    Some(match steps_type {
        "dance-couple" | "dance-routine" => 8,
        "dance-threepanel" => 3,
        "pump-single" => 5,
        "pump-halfdouble" => 6,
        "pump-double" | "pump-couple" | "pump-routine" => 10,
        "kb7-single" => 7,
        "ez2-single" => 5,
        "ez2-double" => 10,
        "ez2-real" => 7,
        "para-single" => 5,
        "ds3ddx-single" => 8,
        "bm-single5" => 6,
        "bm-versus5" => 6,
        "bm-double5" => 12,
        "bm-single7" => 8,
        "bm-versus7" => 8,
        "bm-double7" => 16,
        "maniax-single" => 4,
        "maniax-double" => 8,
        "techno-single4" => 4,
        "techno-single5" => 5,
        "techno-single8" => 8,
        "techno-double4" => 8,
        "techno-double5" => 10,
        "techno-double8" => 16,
        "pnm-five" => 5,
        "pnm-nine" => 9,
        "lights-cabinet" => 6,
        "kickbox-human" => 4,
        "kickbox-quadarm" => 4,
        "kickbox-insect" => 6,
        "kickbox-arachnid" => 8,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Note data.
// ---------------------------------------------------------------------------

/// Parse SM note data (`NoteDataUtil::LoadFromSMNoteDataString`).
///
/// `lanes` limits the columns read per row; `None` reads every character.
/// Routine data (`&`-separated players) keeps only the first player.
fn parse_note_data(data: &str, lanes: Option<usize>) -> Vec<Note> {
    // Routine charts separate players with `&`; keep player 1 only.
    let data = data.split('&').next().unwrap_or("");

    let mut notes: Vec<Note> = Vec::new();
    // Per lane: index of the last note placed (for same-row replacement)
    // and indices of hold/roll heads (for `3` matching).
    let mut last_in_lane: HashMap<u8, usize> = HashMap::new();
    let mut heads_in_lane: HashMap<u8, Vec<usize>> = HashMap::new();
    let open_end = Tick(i64::MAX);

    let mut measure: i64 = 0;
    for measure_text in data.split(',') {
        // StepMania's split ignores *empty* pieces, so `,,` does not count
        // as a measure, but a whitespace-only measure does (with no rows).
        if measure_text.is_empty() {
            continue;
        }
        let rows: Vec<&str> = measure_text
            .split('\n')
            .map(|l| l.trim_matches([' ', '\t', '\r', '\n']))
            .filter(|l| !l.is_empty())
            .collect();
        let row_count = rows.len() as i64;
        for (index, row) in rows.iter().enumerate() {
            let tick = Tick::from_measure_row(measure, index as i64, row_count);
            let chars: Vec<char> = row.chars().collect();
            let mut p = 0;
            let mut lane: usize = 0;
            while p < chars.len() && lanes.is_none_or(|n| lane < n) {
                let ch = chars[p];
                p += 1;
                let kind = match ch {
                    // `A` (attack note) is commented out of StepMania 5.1's
                    // parser and so reads as empty; we match that.
                    '1' => Some(NoteKind::Tap),
                    '2' => Some(NoteKind::HoldHead { end: open_end }),
                    '4' => Some(NoteKind::RollHead { end: open_end }),
                    'M' => Some(NoteKind::Mine),
                    'L' => Some(NoteKind::Lift),
                    'F' => Some(NoteKind::Fake),
                    'K' => Some(NoteKind::AutoKeysound),
                    // '0', '3' and anything unknown place nothing.
                    _ => None,
                };

                // Optional attack suffix `{mods:seconds}`: skipped.
                if p < chars.len() && chars[p] == '{' {
                    while p < chars.len() {
                        let c = chars[p];
                        p += 1;
                        if c == '}' {
                            break;
                        }
                    }
                }
                // Optional keysound suffix `[n]`.
                let mut keysound = None;
                if p < chars.len() && chars[p] == '[' {
                    p += 1;
                    let start = p;
                    while p < chars.len() && chars[p] != ']' {
                        p += 1;
                    }
                    let digits: String = chars[start..p].iter().collect();
                    let n = sm_int(&digits);
                    if digits
                        .trim_start()
                        .starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+')
                        && (0..=u16::MAX as i64).contains(&n)
                    {
                        keysound = Some(n as u16);
                    }
                    if p < chars.len() {
                        p += 1; // the `]`
                    }
                }

                let lane_u8 = lane as u8;
                if ch == '3' {
                    // Tail: close the most recent head in this lane whose
                    // span covers this row.
                    if let Some(heads) = heads_in_lane.get(&lane_u8) {
                        for &hi in heads.iter().rev() {
                            let covers = match notes[hi].kind {
                                NoteKind::HoldHead { end } | NoteKind::RollHead { end } => {
                                    notes[hi].tick <= tick && end >= tick
                                }
                                _ => false,
                            };
                            if covers {
                                match &mut notes[hi].kind {
                                    NoteKind::HoldHead { end } | NoteKind::RollHead { end } => {
                                        *end = tick;
                                    }
                                    _ => {}
                                }
                                break;
                            }
                        }
                    }
                } else if let Some(kind) = kind {
                    let mut note = Note::new(tick, lane_u8, kind);
                    note.keysound = keysound;
                    let is_head =
                        matches!(kind, NoteKind::HoldHead { .. } | NoteKind::RollHead { .. });
                    // Same (tick, lane) twice (odd row counts rounding onto
                    // one tick): the later note replaces the earlier one.
                    let replaced = last_in_lane
                        .get(&lane_u8)
                        .copied()
                        .filter(|&li| notes[li].tick == tick);
                    let idx = if let Some(li) = replaced {
                        notes[li] = note;
                        li
                    } else {
                        notes.push(note);
                        notes.len() - 1
                    };
                    last_in_lane.insert(lane_u8, idx);
                    if is_head {
                        heads_in_lane.entry(lane_u8).or_default().push(idx);
                    }
                }
                lane += 1;
            }
        }
        measure += 1;
    }

    // Drop unterminated heads (StepMania removes them).
    notes.retain(|n| !matches!(n.kind, NoteKind::HoldHead { end } | NoteKind::RollHead { end } if end == open_end));
    notes.sort_by_key(|n| (n.tick, n.lane));
    notes
}

// ---------------------------------------------------------------------------
// Song-level tags.
// ---------------------------------------------------------------------------

fn opt_string(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn parse_display_bpm(tag: &MsdTag) -> Option<DisplayBpm> {
    let first = tag.param(0);
    if first == "*" {
        return Some(DisplayBpm::Random);
    }
    if first.is_empty() {
        return None;
    }
    let min = sm_float(first);
    let second = tag.param(1);
    let max = if second.is_empty() {
        min
    } else {
        sm_float(second)
    };
    Some(if min == max {
        DisplayBpm::Single(min)
    } else {
        DisplayBpm::Range(min, max)
    })
}

/// `#BGCHANGES`/`#FGCHANGES`/`#ANIMATIONS` value → effect events.
fn parse_bg_changes(value: &str, kind: &str, layer: u8, out: &mut Vec<EffectEvent>) {
    let joined: String = value.chars().filter(|c| *c != '\r' && *c != '\n').collect();
    if joined.is_empty() {
        return;
    }
    for entry in joined.split(',') {
        let fields: Vec<String> = entry.split('=').map(|s| s.to_string()).collect();
        if fields.len() < 2 {
            continue;
        }
        out.push(EffectEvent {
            at: EffectTime::Beat(Tick::from_beat_f64(sm_float(&fields[0]))),
            kind: kind.to_string(),
            layer,
            fields,
        });
    }
}

/// `#ATTACKS:TIME=s:LEN=s|END=s:MODS=mods:...;` → `attack` events with
/// `fields = [mods, length_seconds]`.
fn parse_attacks(tag: &MsdTag, out: &mut Vec<EffectEvent>) {
    let mut start = 0.0;
    let mut len = 0.0;
    let mut end: Option<f64> = None;
    for p in &tag.params {
        let Some((k, v)) = p.split_once('=') else {
            continue;
        };
        let k = k.trim();
        if k.eq_ignore_ascii_case("TIME") {
            start = sm_float(v);
        } else if k.eq_ignore_ascii_case("LEN") {
            len = sm_float(v);
        } else if k.eq_ignore_ascii_case("END") {
            end = Some(sm_float(v));
        } else if k.eq_ignore_ascii_case("MODS") {
            if let Some(e) = end.take() {
                len = e - start;
            }
            if len < 0.0 {
                len = 0.0;
            }
            out.push(EffectEvent {
                at: EffectTime::Seconds(start),
                kind: "attack".to_string(),
                layer: 0,
                fields: vec![v.trim().to_string(), format!("{len}")],
            });
        }
    }
}

/// Tags understood but deliberately dropped (cache-only or not modelled).
fn is_ignored_tag(name: &str) -> bool {
    matches!(
        name,
        "SELECTABLE"
            | "LYRICSPATH"
            | "MUSICLENGTH"
            | "MUSICBYTES"
            | "LASTSECONDHINT"
            | "FIRSTSECOND"
            | "LASTSECOND"
            | "FIRSTBEAT"
            | "LASTBEAT"
            | "LASTBEATHINT"
            | "SONGFILENAME"
            | "STEPFILENAME"
            | "HASMUSIC"
            | "HASBANNER"
            | "SAMPLEPATH"
            | "LEADTRACK"
            | "RADARVALUES"
    )
}

// ---------------------------------------------------------------------------
// Main parser.
// ---------------------------------------------------------------------------

/// Per-chart state while reading an SSC `#NOTEDATA` block.
#[derive(Default)]
struct ChartAcc {
    name: String,
    steps_type: String,
    description: String,
    credit: Option<String>,
    difficulty: Option<String>,
    meter: Option<String>,
    display_bpm: Option<DisplayBpm>,
    timing: TimingAcc,
    unknown: Vec<(String, String)>,
}

fn empty_song(format: SourceFormat) -> Song {
    Song {
        title: String::new(),
        subtitle: String::new(),
        artist: String::new(),
        title_translit: String::new(),
        subtitle_translit: String::new(),
        artist_translit: String::new(),
        genre: String::new(),
        credit: String::new(),
        music: None,
        preview_start: 0.0,
        preview_length: 0.0,
        banner: None,
        background: None,
        jacket: None,
        cd_title: None,
        timing: TimingMap::constant(60.0, 0.0),
        display_bpm: DisplayBpm::Actual,
        charts: Vec::new(),
        effects: Vec::new(),
        keysounds: Vec::new(),
        source: SourceInfo {
            format,
            unknown_tags: Vec::new(),
        },
    }
}

fn build_chart(
    flavor: Flavor,
    steps_type: &str,
    description: &str,
    difficulty: &str,
    meter: &str,
    note_data: &str,
) -> Result<Chart, ParseError> {
    let steps_type = normalize_steps_type(steps_type);
    let description = description.trim().to_string();
    let mut diff = parse_difficulty(difficulty)?;
    if flavor == Flavor::Sm {
        diff = apply_legacy_challenge(diff, &description);
    }
    let meter = if meter.trim().is_empty() {
        1
    } else {
        sm_int(meter).max(0) as u32
    };
    let lanes = known_lane_count(&steps_type);
    let notes = parse_note_data(note_data.trim(), lanes);
    Ok(Chart {
        layout: steps_type,
        difficulty: diff,
        meter,
        name: description.clone(),
        description: description.clone(),
        credit: description,
        notes,
        timing: None,
        display_bpm: None,
    })
}

fn parse(text: &str, flavor: Flavor) -> Result<Song, ParseError> {
    let tags = parse_msd(text);
    let mut song = empty_song(match flavor {
        Flavor::Sm => SourceFormat::Sm,
        Flavor::Ssc => SourceFormat::Ssc {
            version: CURRENT_SSC_VERSION,
        },
    });
    let mut version: f32 = CURRENT_SSC_VERSION;
    let mut song_timing = TimingAcc::default();
    let mut current: Option<ChartAcc> = None;

    for tag in &tags {
        let name = tag.name.trim().to_ascii_uppercase();
        let value = tag.param(0);

        // --- Inside an SSC chart block -----------------------------------
        if let Some(chart) = current.as_mut() {
            match name.as_str() {
                "CHARTNAME" => chart.name = value.to_string(),
                "STEPSTYPE" => chart.steps_type = value.to_string(),
                "DESCRIPTION" => chart.description = value.to_string(),
                "CHARTSTYLE" => {}
                "CREDIT" => chart.credit = Some(value.to_string()),
                "DIFFICULTY" => chart.difficulty = Some(value.to_string()),
                "METER" => chart.meter = Some(value.to_string()),
                "RADARVALUES" => {}
                "MUSIC" => chart.unknown.push(("MUSIC".to_string(), tag.raw_value())),
                "DISPLAYBPM" => chart.display_bpm = parse_display_bpm(tag),
                "ATTACKS" => chart.unknown.push(("ATTACKS".to_string(), tag.raw_value())),
                "NOTES" | "NOTES2" => {
                    let chart = current.take().unwrap();
                    let idx = song.charts.len();
                    let mut built = build_chart(
                        flavor,
                        &chart.steps_type,
                        &chart.description,
                        chart.difficulty.as_deref().unwrap_or(""),
                        chart.meter.as_deref().unwrap_or(""),
                        value,
                    )?;
                    built.credit = chart.credit.unwrap_or_default();
                    // `#CHARTNAME` (0.74+) falls back to the description.
                    let name = chart.name.trim();
                    built.name = if name.is_empty() || version < VERSION_CHART_NAME_TAG {
                        if name.is_empty() {
                            built.description.clone()
                        } else {
                            name.to_string()
                        }
                    } else {
                        name.to_string()
                    };
                    built.display_bpm = chart.display_bpm;
                    if chart.timing.touched
                        && chart.timing.has_segments()
                        && version >= VERSION_SPLIT_TIMING
                    {
                        built.timing = Some(chart.timing.build(flavor));
                    }
                    for (k, v) in chart.unknown {
                        song.source
                            .unknown_tags
                            .push((format!("NOTEDATA[{idx}].{k}"), v));
                    }
                    song.charts.push(built);
                }
                "STEPFILENAME" => {
                    // Cache-only block without note data: drop it.
                    current = None;
                }
                "NOTEDATA" => {
                    // A block that never reached `#NOTES`: start over.
                    *chart = ChartAcc {
                        timing: TimingAcc {
                            offset: song_timing.offset,
                            ..TimingAcc::default()
                        },
                        ..ChartAcc::default()
                    };
                }
                _ if is_timing_tag(&name) => {
                    // Split timing exists from 0.70; older files' per-chart
                    // timing tags are ignored like StepMania does.
                    if version >= VERSION_SPLIT_TIMING {
                        chart.timing.apply(&name, value, flavor, version);
                    }
                }
                _ => chart.unknown.push((name.clone(), tag.raw_value())),
            }
            continue;
        }

        // --- Song level -------------------------------------------------
        match name.as_str() {
            "VERSION" => {
                version = sm_float(value) as f32;
                if flavor == Flavor::Ssc {
                    song.source.format = SourceFormat::Ssc { version };
                }
            }
            "TITLE" => song.title = value.to_string(),
            "SUBTITLE" => song.subtitle = value.to_string(),
            "ARTIST" => song.artist = value.to_string(),
            "TITLETRANSLIT" => song.title_translit = value.to_string(),
            "SUBTITLETRANSLIT" => song.subtitle_translit = value.to_string(),
            "ARTISTTRANSLIT" => song.artist_translit = value.to_string(),
            "GENRE" => song.genre = value.to_string(),
            "CREDIT" => song.credit = value.to_string(),
            "MUSIC" => song.music = opt_string(value),
            "BANNER" => song.banner = opt_string(value),
            "BACKGROUND" => song.background = opt_string(value),
            "JACKET" => song.jacket = opt_string(value),
            "CDTITLE" => song.cd_title = opt_string(value),
            "SAMPLESTART" => song.preview_start = sm_float(value),
            "SAMPLELENGTH" => song.preview_length = sm_float(value),
            "DISPLAYBPM" => song.display_bpm = parse_display_bpm(tag).unwrap_or(DisplayBpm::Actual),
            "KEYSOUNDS" => {
                // StepMania strips a leading `\#` left by old writers.
                let v = value.strip_prefix("\\#").unwrap_or(value);
                song.keysounds = if v.is_empty() {
                    Vec::new()
                } else {
                    v.split(',').map(|s| s.trim().to_string()).collect()
                };
            }
            "ATTACKS" => parse_attacks(tag, &mut song.effects),
            "FGCHANGES" => parse_bg_changes(value, "fgchange", 0, &mut song.effects),
            "ANIMATIONS" => parse_bg_changes(value, "bgchange", 0, &mut song.effects),
            "NOTEDATA" => {
                current = Some(ChartAcc {
                    timing: TimingAcc {
                        offset: song_timing.offset,
                        ..TimingAcc::default()
                    },
                    ..ChartAcc::default()
                });
            }
            "NOTES" | "NOTES2" => {
                // `.sm`: stepstype:description:difficulty:meter:radar:notedata
                if tag.params.len() < 6 {
                    // StepMania logs and skips such blocks.
                    continue;
                }
                let built = build_chart(
                    flavor,
                    tag.param(0),
                    tag.param(1),
                    tag.param(2),
                    tag.param(3),
                    tag.param(5),
                )?;
                song.charts.push(built);
            }
            _ if name.starts_with("BGCHANGES") => {
                let layer_no = sm_int(&name["BGCHANGES".len()..]);
                let layer = if name.len() == "BGCHANGES".len() {
                    0
                } else {
                    (layer_no - 1).clamp(0, u8::MAX as i64) as u8
                };
                parse_bg_changes(value, "bgchange", layer, &mut song.effects);
            }
            _ if song_timing.apply(&name, value, flavor, version) => {}
            _ if is_ignored_tag(&name) => {}
            _ => song
                .source
                .unknown_tags
                .push((name.clone(), tag.raw_value())),
        }
    }

    song.timing = song_timing.build(flavor);
    Ok(song)
}

fn is_timing_tag(name: &str) -> bool {
    matches!(
        name,
        "OFFSET"
            | "BPMS"
            | "STOPS"
            | "FREEZES"
            | "DELAYS"
            | "WARPS"
            | "SPEEDS"
            | "SCROLLS"
            | "FAKES"
            | "TIMESIGNATURES"
            | "TICKCOUNTS"
            | "COMBOS"
            | "LABELS"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn float_and_int_prefixes() {
        assert_eq!(sm_float("-0.611"), -0.611);
        assert_eq!(sm_float(" 120abc"), 120.0);
        assert_eq!(sm_float("abc"), 0.0);
        assert_eq!(sm_float(".5"), 0.5);
        assert_eq!(sm_float("1e2"), 100.0);
        assert_eq!(sm_float("1e"), 1.0);
        assert_eq!(sm_float(""), 0.0);
        assert_eq!(sm_int("12x"), 12);
        assert_eq!(sm_int("-3"), -3);
        assert_eq!(sm_int(""), 0);
        assert!(approx(row_to_beat("96r"), 2.0));
    }

    #[test]
    fn negative_bpm_becomes_warp() {
        let song = parse_sm("#BPMS:0=120,4=-120,6=120;").unwrap();
        let t = &song.timing;
        assert_eq!(
            t.warps,
            vec![WarpSegment {
                tick: Tick::from_beats(4),
                length: Tick::from_beats(4),
            }]
        );
        assert_eq!(t.bpms.len(), 1);
        assert!(approx(t.seconds_at_beat(8.0), 2.0));
        assert!(!t.judgeable(Tick::from_beats(5)));
        assert!(t.judgeable(Tick::from_beats(8)));
    }

    #[test]
    fn negative_stop_becomes_warp() {
        let song = parse_sm("#BPMS:0=120;#STOPS:4=-1;").unwrap();
        assert_eq!(
            song.timing.warps,
            vec![WarpSegment {
                tick: Tick::from_beats(4),
                length: Tick::from_beats(2),
            }]
        );
        assert!(song.timing.stops.is_empty());
    }

    #[test]
    fn stop_overshoot_ends_warp_with_remaining_stop() {
        let song = parse_sm("#BPMS:0=60;#STOPS:4=-3,5=5;").unwrap();
        let t = &song.timing;
        assert_eq!(
            t.warps,
            vec![WarpSegment {
                tick: Tick::from_beats(4),
                length: Tick::from_beats(1),
            }]
        );
        assert_eq!(t.stops.len(), 1);
        assert_eq!(t.stops[0].tick, Tick::from_beats(5));
        assert!(approx(t.stops[0].seconds, 3.0));
        assert!(approx(t.seconds_at_beat(6.0), 8.0));
    }

    #[test]
    fn bpm_change_inside_warp_is_emitted_at_warp_start() {
        // 60 → -60 at beat 4 → 120 at beat 6: deficit 2 s recovered at 120 BPM.
        let song = parse_sm("#BPMS:0=60,4=-60,6=120;").unwrap();
        let t = &song.timing;
        assert_eq!(t.warps.len(), 1);
        assert_eq!(t.warps[0].tick, Tick::from_beats(4));
        assert_eq!(t.warps[0].length, Tick::from_beats(6)); // ends at beat 10
        assert_eq!(
            t.bpms,
            vec![
                BpmSegment {
                    tick: Tick::ZERO,
                    bpm: 60.0
                },
                BpmSegment {
                    tick: Tick::from_beats(4),
                    bpm: 120.0
                },
            ]
        );
    }

    #[test]
    fn stops_before_beat_zero_fold_into_offset() {
        let song = parse_sm("#OFFSET:0.5;#BPMS:0=120;#STOPS:-1=0.25;").unwrap();
        assert!(approx(song.timing.offset_seconds, 0.25));
        assert!(song.timing.stops.is_empty());
    }

    #[test]
    fn zero_bpm_entries_are_ignored_and_default_is_60() {
        let song = parse_sm("#BPMS:0=0;").unwrap();
        assert_eq!(song.timing.bpms[0].bpm, 60.0);
    }

    #[test]
    fn ssc_drops_negative_beats_without_conversion() {
        let song = parse_ssc("#VERSION:0.83;#BPMS:-1=100,0=150;#STOPS:-2=1,4=0.5;").unwrap();
        assert_eq!(song.timing.bpms.len(), 1);
        assert_eq!(song.timing.bpms[0].bpm, 150.0);
        assert_eq!(song.timing.stops.len(), 1);
        assert!(approx(song.timing.offset_seconds, 0.0));
    }

    #[test]
    fn ssc_with_negative_bpm_still_converts() {
        let song = parse_ssc("#VERSION:0.83;#BPMS:0=120,4=-120,6=120;").unwrap();
        assert_eq!(song.timing.warps.len(), 1);
    }

    #[test]
    fn note_data_basics() {
        let notes = parse_note_data("1000\n0100\n0010\n0001\n,\n2000\n0000\n3000\n0000", Some(4));
        assert_eq!(notes.len(), 5);
        assert_eq!(notes[0].tick, Tick::ZERO);
        assert_eq!(notes[1].tick, Tick::from_beats(1));
        assert_eq!(notes[3].lane, 3);
        assert_eq!(notes[4].tick, Tick::from_beats(4));
        assert_eq!(
            notes[4].kind,
            NoteKind::HoldHead {
                end: Tick::from_beats(6)
            }
        );
    }

    #[test]
    fn classic_negative_bpm_gimmick_warp_length() {
        // 120 BPM, −120 for two beats, back to 120: the two "negative" beats
        // rewind one second, which 120 BPM replays in two beats, so four
        // beats are skipped in total and no extra BPM segment is emitted.
        let song =
            parse_sm("#BPMS:0=120,4=-120,6=120;#NOTES:dance-single:x:Hard:1:0:1000;").unwrap();
        let t = &song.timing;
        assert_eq!(t.warps.len(), 1);
        assert_eq!(t.warps[0].tick, Tick::from_beats(4));
        assert_eq!(t.warps[0].length, Tick::from_beats(4));
        assert_eq!(t.bpms.len(), 1);
        assert!(approx(t.seconds_at_beat(4.0), 2.0));
        assert!(approx(t.seconds_at_beat(8.0), 2.0));
        assert!(approx(t.seconds_at_beat(10.0), 3.0));
        assert!(t.in_warp(Tick::from_beats(7)));
        assert!(!t.in_warp(Tick::from_beats(8)));
    }

    #[test]
    fn attack_note_char_is_empty_like_stepmania() {
        let notes = parse_note_data("A000\n0000\n0000\n0001", Some(4));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].lane, 3);
    }

    #[test]
    fn crlf_notes_block_and_trailing_comma() {
        let text = "#TITLE:crlf;\r\n#BPMS:0=120;\r\n#NOTES:\r\n     dance-single:\r\n     :\r\n     Hard:\r\n     5:\r\n     0,0,0,0,0:\r\n1000\r\n0000\r\n0000\r\n0000\r\n,\r\n0100\r\n0000\r\n0000\r\n0000\r\n,\r\n;\r\n";
        let song = parse_sm(text).unwrap();
        assert_eq!(song.title, "crlf");
        let c = &song.charts[0];
        assert_eq!(c.notes.len(), 2);
        assert_eq!((c.notes[0].tick, c.notes[0].lane), (Tick::ZERO, 0));
        assert_eq!((c.notes[1].tick, c.notes[1].lane), (Tick::from_beats(4), 1));
        // The trailing comma leaves a whitespace-only measure with no rows.
        let notes = parse_note_data("1000\n0000\n0000\n0000\n,\n", Some(4));
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn hold_pairs_across_measures_and_rolls_keep_kind() {
        let notes = parse_note_data(
            "2000\n0000\n0000\n0400\n,\n0000\n0000\n3000\n0000\n,\n0300\n0000\n0000\n0000",
            Some(4),
        );
        assert_eq!(notes.len(), 2);
        assert_eq!(
            notes[0].kind,
            NoteKind::HoldHead {
                end: Tick::from_beats(6)
            }
        );
        assert_eq!(
            notes[1].kind,
            NoteKind::RollHead {
                end: Tick::from_beats(8)
            }
        );
    }

    #[test]
    fn ssc_chart_with_only_offset_uses_song_timing() {
        let song = parse_ssc(
            "#VERSION:0.83;#OFFSET:-0.5;#BPMS:0=150;#STOPS:4=1;\
             #NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:5;#OFFSET:0.2;\
             #NOTES:1000;\
             #NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Easy;#METER:1;#BPMS:0=100;\
             #NOTES:1000;",
        )
        .unwrap();
        assert_eq!(song.charts.len(), 2);
        // Only #OFFSET: StepMania's TimingData::empty() is true → song timing.
        assert!(song.charts[0].timing.is_none());
        // A real segment makes the chart's timing its own, seeded with the
        // song offset and nothing else.
        let own = song.charts[1].timing.as_ref().unwrap();
        assert!(approx(own.offset_seconds, -0.5));
        assert_eq!(own.bpms[0].bpm, 100.0);
        assert!(own.stops.is_empty());
    }

    #[test]
    fn unterminated_head_dropped_unmatched_tail_ignored() {
        let notes = parse_note_data("2300\n0000\n0000\n0000", Some(4));
        assert!(notes.is_empty());
    }

    #[test]
    fn empty_measure_counts_but_double_comma_does_not() {
        // 1st measure, whitespace-only 2nd measure, note in 3rd measure.
        let notes = parse_note_data("1000\n,\n\n,\n1000", Some(4));
        assert_eq!(notes[1].tick, Tick::from_beats(8));
        // `,,` collapses.
        let notes = parse_note_data("1000\n,,1000", Some(4));
        assert_eq!(notes[1].tick, Tick::from_beats(4));
    }

    #[test]
    fn keysound_and_attack_suffixes() {
        let notes = parse_note_data("1[3]0K[12]0\n0000\n1{drunk:2.5}000\n0000", Some(4));
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0].keysound, Some(3));
        assert_eq!(notes[1].lane, 2);
        assert_eq!(notes[1].kind, NoteKind::AutoKeysound);
        assert_eq!(notes[1].keysound, Some(12));
        assert_eq!(notes[2].lane, 0);
        assert_eq!(notes[2].tick, Tick::from_beats(2));
        assert_eq!(notes[2].keysound, None);
    }

    #[test]
    fn extra_columns_truncated_routine_keeps_first_player() {
        let notes = parse_note_data("10001000\n0000\n0000\n0000", Some(4));
        assert_eq!(notes.len(), 1);
        let notes = parse_note_data("1000\n0000\n0000\n0000\n&\n0100\n0000\n0000\n0000", Some(4));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].lane, 0);
    }

    #[test]
    fn difficulty_aliases() {
        assert_eq!(parse_difficulty("BASIC").unwrap(), Difficulty::Easy);
        assert_eq!(parse_difficulty("Trick").unwrap(), Difficulty::Medium);
        assert_eq!(parse_difficulty("heavy").unwrap(), Difficulty::Hard);
        assert_eq!(parse_difficulty("oni").unwrap(), Difficulty::Challenge);
        assert_eq!(parse_difficulty("Edit").unwrap(), Difficulty::Edit);
        assert!(parse_difficulty("insane").is_err());
        assert_eq!(
            apply_legacy_challenge(Difficulty::Hard, "SMANIAC"),
            Difficulty::Challenge
        );
        assert_eq!(
            apply_legacy_challenge(Difficulty::Medium, "challenge"),
            Difficulty::Medium
        );
    }

    #[test]
    fn display_bpm_forms() {
        let song = parse_sm("#DISPLAYBPM:*;").unwrap();
        assert_eq!(song.display_bpm, DisplayBpm::Random);
        let song = parse_sm("#DISPLAYBPM:150;").unwrap();
        assert_eq!(song.display_bpm, DisplayBpm::Single(150.0));
        let song = parse_sm("#DISPLAYBPM:100:200;").unwrap();
        assert_eq!(song.display_bpm, DisplayBpm::Range(100.0, 200.0));
        let song = parse_sm("#DISPLAYBPM:;").unwrap();
        assert_eq!(song.display_bpm, DisplayBpm::Actual);
    }

    #[test]
    fn attacks_preserved() {
        let song = parse_sm(
            "#BPMS:0=120;#ATTACKS:TIME=1.5:LEN=2:MODS=drunk:TIME=5:END=7.5:MODS=*2 dizzy;",
        )
        .unwrap();
        assert_eq!(song.effects.len(), 2);
        assert_eq!(song.effects[0].at, EffectTime::Seconds(1.5));
        assert_eq!(
            song.effects[0].fields,
            vec!["drunk".to_string(), "2".to_string()]
        );
        assert_eq!(
            song.effects[1].fields,
            vec!["*2 dizzy".to_string(), "2.5".to_string()]
        );
    }

    #[test]
    fn unknown_tags_are_kept() {
        let song = parse_sm("#BPMS:0=120;#MENUCOLOR:red;#FOO:a:b;#SELECTABLE:YES;").unwrap();
        assert_eq!(
            song.source.unknown_tags,
            vec![
                ("MENUCOLOR".to_string(), "red".to_string()),
                ("FOO".to_string(), "a:b".to_string()),
            ]
        );
    }

    #[test]
    fn simfile_dispatch() {
        assert!(parse_simfile("#TITLE:x;", ".SM").is_ok());
        assert!(parse_simfile("#TITLE:x;", "ssc").is_ok());
        assert!(parse_simfile("#TITLE:x;", "dwi").is_err());
    }
}
