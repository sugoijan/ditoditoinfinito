//! Plays imported from an export (`docs/plans/phase-10.md`, step 1): the
//! JSON lines `Plays::export` writes (`PlayEntry::to_json`, one a line),
//! read back on either shell, sans IO. The shell hands the file over in
//! chunks as it reads it ([`PlayImport::push`]); each line is a play,
//! checked whole before it is taken ([`read_line`]); what reads comes
//! out a batch at a time ([`PlayImport::take`]) for `Plays::import` (or a
//! shell's own writer and `Plays::adopt`); the [`ImportReport`] says what
//! was imported, already kept, refused and why.
//!
//! What a play keeps is taken as it is (the record's text, the settings
//! as stored in it, the log and the timing bytes); what is worked out
//! from it is not trusted: the summary is made again from the record,
//! with the score the line carried kept only as unconfirmed (an older
//! judge's, [`Worked::judge_version`] 0), so the play is judged again
//! from its log (`Reworker`) once its chart is in the library. Until
//! then it is kept, as plays of a removed song are.

use std::collections::HashSet;

use super::{
    JUDGE_VERSION, LogError, PlayEntry, PlayLog, PlayRecord, PlaySummary, PlayTiming,
    RECORD_VERSION, is_play_id,
};

/// The longest line read, in bytes: a long chart's play takes about
/// 20 KB; a line longer than this is no play of ours.
pub const MAX_LINE: usize = 4 << 20;
/// The most plays one import takes (5,004 plays exported to 40 MB).
pub const MAX_PLAYS: usize = 200_000;
/// The problems a report names (the rest are counted).
pub const MAX_PROBLEMS: usize = 5;
/// The longest time a play's log, offset or start may name, seconds.
const MAX_SECONDS: f64 = 86_400.0;
/// The longest song id taken, bytes.
const MAX_SONG_ID: usize = 1024;

/// A play read from a line: its entry as it will be kept (the summary
/// made again) and that summary.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    pub entry: PlayEntry,
    pub summary: PlaySummary,
}

/// Why a line was not taken.
#[derive(Clone, Debug, PartialEq)]
pub enum Refused {
    /// Kept by a newer version of the game (its record, judge, log or
    /// timing): this one cannot judge it.
    Newer(LineProblem),
    /// Not a play as the export writes one.
    Invalid(LineProblem),
}

/// A part of a play a line carries, read on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinePart {
    /// Its input log.
    Log,
    /// Its timing (the judgements and life as played).
    Timing,
}

/// What was wrong with a line of an export (`docs/plans/localization.md`,
/// step 4): a value Options words ([`LineProblem::text`], after "line
/// 3:"). `Display` is English, for the logs and the debug state.
#[derive(Clone, Debug, PartialEq)]
pub enum LineProblem {
    /// Longer than [`MAX_LINE`].
    TooLong,
    NotUtf8,
    /// Not a play's JSON: what the reader said.
    NotAPlay(String),
    NotAPlayId,
    RecordNotJson,
    RecordNoVersion,
    RecordVersionZero,
    /// Its record does not read: what the reader said.
    RecordUnreadable(String),
    RecordOutOfRange,
    RecordNoSong,
    LogOutOfRange,
    /// Kept by a newer version (its record's format).
    NewerRecord(u64),
    /// Judged by a newer judge (its version).
    NewerJudge(u32),
    /// A part of a newer format (its version).
    NewerPart(LinePart, u8),
    /// A part that does not read: what the reader said.
    PartUnreadable(LinePart, String),
}

impl LineProblem {
    /// A clause, after "line 3:" (`options-import-problem`).
    pub fn text(&self) -> ddi_l10n::UiText {
        use ddi_l10n::msg::errors as m;
        match self {
            LineProblem::TooLong => m::history_too_long(MAX_LINE >> 20),
            LineProblem::NotUtf8 => m::history_not_utf8(),
            LineProblem::NotAPlay(e) => m::history_not_a_play(e),
            LineProblem::NotAPlayId => m::history_not_a_play_id(),
            LineProblem::RecordNotJson => m::history_record_not_json(),
            LineProblem::RecordNoVersion => m::history_record_no_version(),
            LineProblem::RecordVersionZero => m::history_record_version_zero(),
            LineProblem::RecordUnreadable(e) => m::history_record_unreadable(e),
            LineProblem::RecordOutOfRange => m::history_record_out_of_range(),
            LineProblem::RecordNoSong => m::history_record_no_song(),
            LineProblem::LogOutOfRange => m::history_log_out_of_range(),
            LineProblem::NewerRecord(v) => m::history_newer_record(v.to_string()),
            LineProblem::NewerJudge(v) => m::history_newer_judge(v.to_string()),
            LineProblem::NewerPart(LinePart::Log, v) => m::history_newer_log(v.to_string()),
            LineProblem::NewerPart(LinePart::Timing, v) => m::history_newer_timing(v.to_string()),
            LineProblem::PartUnreadable(LinePart::Log, e) => m::history_log_unreadable(e),
            LineProblem::PartUnreadable(LinePart::Timing, e) => m::history_timing_unreadable(e),
        }
    }
}

impl std::fmt::Display for LineProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let part = |p: &LinePart| match p {
            LinePart::Log => "input log",
            LinePart::Timing => "timing",
        };
        match self {
            LineProblem::TooLong => write!(f, "longer than {} MB", MAX_LINE >> 20),
            LineProblem::NotUtf8 => f.write_str("not UTF-8 text"),
            LineProblem::NotAPlay(e) => write!(f, "not a play: {e}"),
            LineProblem::NotAPlayId => f.write_str("its id is not a play's"),
            LineProblem::RecordNotJson => f.write_str("its record is not JSON"),
            LineProblem::RecordNoVersion => f.write_str("its record has no version"),
            LineProblem::RecordVersionZero => f.write_str("its record's version is 0"),
            LineProblem::RecordUnreadable(e) => write!(f, "its record does not read: {e}"),
            LineProblem::RecordOutOfRange => {
                f.write_str("its record's time or offset is out of range")
            }
            LineProblem::RecordNoSong => f.write_str("its record names no song"),
            LineProblem::LogOutOfRange => f.write_str("its input log's times are out of range"),
            LineProblem::NewerRecord(v) => {
                write!(f, "kept by a newer version of the game (record format {v})")
            }
            LineProblem::NewerJudge(v) => {
                write!(f, "judged by a newer version of the game (judge {v})")
            }
            LineProblem::NewerPart(p, v) => {
                write!(f, "its {} is of a newer format ({v})", part(p))
            }
            LineProblem::PartUnreadable(p, e) => write!(f, "its {} does not read: {e}", part(p)),
        }
    }
}

/// What an import did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    /// Plays kept now.
    pub imported: usize,
    /// Plays already kept here (or twice in the file).
    pub duplicates: usize,
    /// Lines that are not plays.
    pub invalid: usize,
    /// Plays kept by a newer version of the game.
    pub newer: usize,
    /// The file had more than [`MAX_PLAYS`] plays: the rest were left.
    pub cut: bool,
    /// The first few problems: the line (from 1) and what it was.
    pub problems: Vec<(usize, LineProblem)>,
    /// Why the plays read could not be kept (the store failed).
    pub error: Option<String>,
}

impl ImportReport {
    /// The report as a line of tags (Options' notice): what was imported,
    /// what was left out and why, the first problem.
    pub fn notice(&self) -> String {
        use ddi_l10n::msg::options as m;
        let mut parts: Vec<String> = vec![m::import_imported(self.imported).into()];
        if self.duplicates > 0 {
            parts.push(m::import_duplicates(self.duplicates).into());
        }
        if self.newer > 0 {
            parts.push(m::import_newer(self.newer).into());
        }
        if self.invalid > 0 {
            parts.push(m::import_invalid(self.invalid).into());
        }
        if self.cut {
            parts.push(m::import_cut(MAX_PLAYS).into());
        }
        if let Some((line, why)) = self.problems.first() {
            parts.push(m::import_problem(*line, why.text()).into());
        }
        if let Some(e) = &self.error {
            parts.push(m::import_not_kept(crate::screens::errors::error_text(e)).into());
        }
        parts.join(" · ")
    }

    fn problem(&mut self, line: usize, why: LineProblem) {
        if self.problems.len() < MAX_PROBLEMS {
            self.problems.push((line, why));
        }
    }
}

/// An import under way: lines read as the file comes in.
pub struct PlayImport {
    /// Ids kept already, and those read so far.
    known: HashSet<String>,
    /// The line read so far (its start).
    pending: Vec<u8>,
    /// Lines begun so far.
    line: usize,
    /// The line is longer than [`MAX_LINE`]: skipped to its end.
    overlong: bool,
    /// Plays read in this file, kept or not.
    plays: usize,
    /// Bytes have come (a byte-order mark is looked for in the first).
    started: bool,
    ready: Vec<Imported>,
    report: ImportReport,
}

impl PlayImport {
    /// An import beside the plays `known` (their ids): those are left.
    pub fn new(known: impl IntoIterator<Item = String>) -> PlayImport {
        PlayImport {
            known: known.into_iter().collect(),
            pending: Vec::new(),
            line: 0,
            overlong: false,
            plays: 0,
            started: false,
            ready: Vec::new(),
            report: ImportReport::default(),
        }
    }

    /// The next part of the file.
    pub fn push(&mut self, mut bytes: &[u8]) {
        if !self.started && !bytes.is_empty() {
            self.started = true;
            self.line = 1;
        }
        while let Some(at) = bytes.iter().position(|&b| b == b'\n') {
            self.extend(&bytes[..at]);
            self.end_line();
            bytes = &bytes[at + 1..];
        }
        self.extend(bytes);
    }

    /// The file is all read: its last line, if it has no line break.
    pub fn finish(&mut self) {
        if !self.pending.is_empty() || self.overlong {
            self.end_line();
        }
    }

    /// The plays read since last asked.
    pub fn take(&mut self) -> Vec<Imported> {
        std::mem::take(&mut self.ready)
    }

    /// How many plays are waiting to be taken.
    pub fn ready(&self) -> usize {
        self.ready.len()
    }

    pub fn report(&self) -> &ImportReport {
        &self.report
    }

    pub fn report_mut(&mut self) -> &mut ImportReport {
        &mut self.report
    }

    /// Nothing more is read (the file had too many plays).
    pub fn is_full(&self) -> bool {
        self.report.cut
    }

    fn extend(&mut self, bytes: &[u8]) {
        if self.overlong || self.report.cut {
            return;
        }
        if self.pending.len() + bytes.len() > MAX_LINE {
            self.overlong = true;
            self.pending = Vec::new();
        } else {
            self.pending.extend_from_slice(bytes);
        }
    }

    fn end_line(&mut self) {
        let line = self.line;
        self.line += 1;
        let text = std::mem::take(&mut self.pending);
        if std::mem::take(&mut self.overlong) {
            self.report.invalid += 1;
            self.report.problem(line, LineProblem::TooLong);
            return;
        }
        if self.report.cut {
            return;
        }
        let text = match std::str::from_utf8(&text) {
            Ok(t) => t,
            Err(_) => {
                self.report.invalid += 1;
                self.report.problem(line, LineProblem::NotUtf8);
                return;
            }
        };
        // A byte-order mark an editor added; Windows' line breaks.
        let text = if line == 1 {
            text.trim_start_matches('\u{feff}')
        } else {
            text
        };
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if self.plays == MAX_PLAYS {
            self.report.cut = true;
            return;
        }
        self.plays += 1;
        match read_line(text) {
            Ok(play) => {
                if self.known.insert(play.entry.id.clone()) {
                    self.ready.push(play);
                } else {
                    self.report.duplicates += 1;
                }
            }
            Err(Refused::Newer(why)) => {
                self.report.newer += 1;
                self.report.problem(line, why);
            }
            Err(Refused::Invalid(why)) => {
                self.report.invalid += 1;
                self.report.problem(line, why);
            }
        }
    }
}

fn within(x: f64) -> bool {
    x.is_finite() && x.abs() <= MAX_SECONDS
}

fn log_error(part: LinePart, e: LogError) -> Refused {
    match e {
        LogError::Version(v) if v > 1 => Refused::Newer(LineProblem::NewerPart(part, v)),
        e => Refused::Invalid(LineProblem::PartUnreadable(part, e.to_string())),
    }
}

/// One line of an export, checked: the play it is, with its summary made
/// again from its record (the line's score kept as unconfirmed), or why
/// it is not taken.
pub fn read_line(text: &str) -> Result<Imported, Refused> {
    let invalid = Refused::Invalid;
    let mut entry =
        PlayEntry::from_json(text).map_err(|e| invalid(LineProblem::NotAPlay(e.to_string())))?;
    if !is_play_id(&entry.id) {
        return Err(invalid(LineProblem::NotAPlayId));
    }
    // The record: a JSON object, of a format this build reads.
    let value: serde_json::Value =
        serde_json::from_str(&entry.record).map_err(|_| invalid(LineProblem::RecordNotJson))?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(invalid(LineProblem::RecordNoVersion))?;
    if version > u64::from(RECORD_VERSION) {
        return Err(Refused::Newer(LineProblem::NewerRecord(version)));
    }
    if version == 0 {
        return Err(invalid(LineProblem::RecordVersionZero));
    }
    let record: PlayRecord = serde_json::from_value(value)
        .map_err(|e| invalid(LineProblem::RecordUnreadable(e.to_string())))?;
    if record.judge_version > JUDGE_VERSION {
        return Err(Refused::Newer(LineProblem::NewerJudge(
            record.judge_version,
        )));
    }
    if !(record.time.is_finite() && record.time >= 0.0) || !within(record.song_offset) {
        return Err(invalid(LineProblem::RecordOutOfRange));
    }
    if record.chart.song.is_empty() || record.chart.song.len() > MAX_SONG_ID {
        return Err(invalid(LineProblem::RecordNoSong));
    }
    // The log judges it again; the timing is what it showed.
    let log = PlayLog::decode(&entry.log).map_err(|e| log_error(LinePart::Log, e))?;
    if !within(log.start) || log.inputs.iter().any(|i| !within(i.time)) {
        return Err(invalid(LineProblem::LogOutOfRange));
    }
    PlayTiming::decode(&entry.timing).map_err(|e| log_error(LinePart::Timing, e))?;
    // The summary: made from the record; the score the line carried,
    // when it is of this play, kept as unconfirmed until judged again.
    let mut summary = PlaySummary::of(&record);
    let carried = entry
        .summary
        .as_deref()
        .and_then(|s| serde_json::from_str::<PlaySummary>(s).ok())
        .filter(|s| {
            s.chart == summary.chart && s.ruleset == summary.ruleset && s.time == summary.time
        });
    summary.worked = carried.and_then(|s| s.worked).map(|mut w| {
        w.judge_version = 0;
        w
    });
    entry.summary = Some(
        serde_json::to_string(&summary)
            .map_err(|e| invalid(LineProblem::NotAPlay(e.to_string())))?,
    );
    Ok(Imported { entry, summary })
}
