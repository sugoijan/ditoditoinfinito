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
    RECORD_VERSION,
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
    Newer(String),
    /// Not a play as the export writes one.
    Invalid(String),
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
    pub problems: Vec<(usize, String)>,
    /// Why the plays read could not be kept (the store failed).
    pub error: Option<String>,
}

impl ImportReport {
    /// The report in a sentence or two (Options' notice).
    pub fn notice(&self) -> String {
        let plays = |n: usize| match n {
            1 => "1 play".to_string(),
            n => format!("{n} plays"),
        };
        let mut parts = vec![format!("{} imported", plays(self.imported))];
        if self.duplicates > 0 {
            parts.push(format!("{} already kept", plays(self.duplicates)));
        }
        if self.newer > 0 {
            parts.push(format!(
                "{} from a newer version of the game left out",
                plays(self.newer)
            ));
        }
        if self.invalid > 0 {
            parts.push(match self.invalid {
                1 => "1 line that is not a play left out".to_string(),
                n => format!("{n} lines that are not plays left out"),
            });
        }
        let mut out = parts.join("; ");
        out.push('.');
        if self.cut {
            out.push_str(&format!(
                " Only the first {MAX_PLAYS} plays were read: import the rest from another file."
            ));
        }
        if let Some((line, why)) = self.problems.first() {
            out.push_str(&format!(" Line {line}: {why}"));
            if !why.ends_with('.') {
                out.push('.');
            }
        }
        if let Some(e) = &self.error {
            out.push_str(&format!(" The plays could not be kept: {e}"));
        }
        out
    }

    fn problem(&mut self, line: usize, why: String) {
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
            self.report
                .problem(line, format!("longer than {} MB", MAX_LINE >> 20));
            return;
        }
        if self.report.cut {
            return;
        }
        let text = match std::str::from_utf8(&text) {
            Ok(t) => t,
            Err(_) => {
                self.report.invalid += 1;
                self.report.problem(line, "not UTF-8 text".into());
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

/// Whether `id` is a play id as [`super::play_id`] writes them: the time
/// in 15 digits, a dash, 8 lowercase hex digits. Nothing else is taken
/// (an id names a file on desktop).
pub fn is_play_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 24
        && b[..15].iter().all(u8::is_ascii_digit)
        && b[15] == b'-'
        && b[16..]
            .iter()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
}

fn within(x: f64) -> bool {
    x.is_finite() && x.abs() <= MAX_SECONDS
}

fn log_error(what: &str, e: LogError) -> Refused {
    match e {
        LogError::Version(v) if v > 1 => {
            Refused::Newer(format!("its {what} is of a newer format ({v})"))
        }
        e => Refused::Invalid(format!("its {what} does not read: {e}")),
    }
}

/// One line of an export, checked: the play it is, with its summary made
/// again from its record (the line's score kept as unconfirmed), or why
/// it is not taken.
pub fn read_line(text: &str) -> Result<Imported, Refused> {
    let invalid = |why: String| Refused::Invalid(why);
    let mut entry = PlayEntry::from_json(text).map_err(|e| invalid(format!("not a play: {e}")))?;
    if !is_play_id(&entry.id) {
        return Err(invalid("its id is not a play's".into()));
    }
    // The record: a JSON object, of a format this build reads.
    let value: serde_json::Value = serde_json::from_str(&entry.record)
        .map_err(|_| invalid("its record is not JSON".into()))?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| invalid("its record has no version".into()))?;
    if version > u64::from(RECORD_VERSION) {
        return Err(Refused::Newer(format!(
            "kept by a newer version of the game (record format {version})"
        )));
    }
    if version == 0 {
        return Err(invalid("its record's version is 0".into()));
    }
    let record: PlayRecord = serde_json::from_value(value)
        .map_err(|e| invalid(format!("its record does not read: {e}")))?;
    if record.judge_version > JUDGE_VERSION {
        return Err(Refused::Newer(format!(
            "judged by a newer version of the game (judge {})",
            record.judge_version
        )));
    }
    if !(record.time.is_finite() && record.time >= 0.0) || !within(record.song_offset) {
        return Err(invalid(
            "its record's time or offset is out of range".into(),
        ));
    }
    if record.chart.song.is_empty() || record.chart.song.len() > MAX_SONG_ID {
        return Err(invalid("its record names no song".into()));
    }
    // The log judges it again; the timing is what it showed.
    let log = PlayLog::decode(&entry.log).map_err(|e| log_error("input log", e))?;
    if !within(log.start) || log.inputs.iter().any(|i| !within(i.time)) {
        return Err(invalid("its input log's times are out of range".into()));
    }
    PlayTiming::decode(&entry.timing).map_err(|e| log_error("timing", e))?;
    // The summary: made from the record; the score the line carried,
    // when it is of this play, kept as unconfirmed until judged again.
    let mut summary = PlaySummary::from_record(&entry.record).map_err(invalid)?;
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
    entry.summary = Some(serde_json::to_string(&summary).map_err(|e| invalid(e.to_string()))?);
    Ok(Imported { entry, summary })
}
