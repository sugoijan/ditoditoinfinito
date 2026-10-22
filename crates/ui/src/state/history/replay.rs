//! Replays (`docs/plans/play-history.md`, "Replays"): a kept play read
//! back for watching again ([`Replay`]): what a play session needs to
//! play it ([`Replaying`]), the settings it is drawn with, its results
//! judged again from its log, the check that a replay's results are the
//! play's ([`ReplayCheck`]), and the reasons one is refused. Also the
//! rows of song select's plays list ([`play_rows`]).

use ddi_chart::{ChartDigest, Song};
use ddi_engine::clock::ClockOptions;
use ddi_engine::player::{PlayOptions, Results};
use ddi_platform::DeviceProfile;
use serde::Deserialize;

use super::{
    History, JUDGE_VERSION, PlayEntry, PlayLog, PlayRecord, PlaySummary, PlayTiming,
    RECORD_VERSION, RulesetKey, Worked, find_chart, replay, score_text,
};
use crate::screens::data::PlayRow;
use crate::state::settings::{BUILT_IN, BUNDLED, DITO, Settings};

/// A kept play, read back to be watched again or its results shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Replay {
    pub id: String,
    pub record: PlayRecord,
    pub log: PlayLog,
    /// The judgements and life as played (`None`: they do not read; the
    /// replay plays, its results are then not compared with them).
    pub timing: Option<PlayTiming>,
    pub summary: Option<PlaySummary>,
}

/// What a play session needs to replay a play: its log, the options as
/// played (the clock's chosen for this device, [`Replay::replaying`]) and
/// its rules.
#[derive(Clone, Debug, PartialEq)]
pub struct Replaying {
    pub log: PlayLog,
    pub options: PlayOptions,
    pub ruleset: RulesetKey,
}

/// How a replay's results compare with the play's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReplayCheck {
    /// Nothing differed.
    pub matches: bool,
    /// The record was judged by today's judge, so its timing and summary
    /// were compared too (else only the log judged at once was).
    pub with_record: bool,
    /// What differed, said plainly.
    pub problems: Vec<String>,
}

impl ReplayCheck {
    /// The line the results show when something differed.
    pub fn note(&self) -> Option<String> {
        (!self.matches).then(|| {
            format!(
                "This replay judged differently from the play: {}.",
                self.problems.join("; ")
            )
        })
    }
}

/// The results with what only a live play has left out: edges judged
/// later than they happened (`Results::late_edges`), which a replay never
/// has.
fn comparable(r: &Results) -> Results {
    Results {
        late_edges: 0,
        ..r.clone()
    }
}

/// A summary's `worked`, `late_edges` aside.
fn comparable_worked(w: &Worked) -> Worked {
    Worked {
        late_edges: 0,
        ..w.clone()
    }
}

impl Replay {
    /// Reads a stored play for replaying. Refused (with a sentence the
    /// screens show) when its record or log is of a format this build
    /// does not read, or it was judged by a newer judge than this build's.
    pub fn from_entry(entry: &PlayEntry) -> Result<Replay, String> {
        #[derive(Deserialize)]
        struct Version {
            version: u32,
        }
        let version: Version = serde_json::from_str(&entry.record)
            .map_err(|_| "This play's record does not read.".to_string())?;
        if version.version > RECORD_VERSION {
            return Err(
                "This play was kept by a newer version of the game: this one cannot replay it."
                    .into(),
            );
        }
        let record: PlayRecord = serde_json::from_str(&entry.record)
            .map_err(|e| format!("This play's record does not read ({e})."))?;
        if record.judge_version > JUDGE_VERSION {
            return Err(
                "This play was judged by a newer version of the game: this one would judge it \
                 differently."
                    .into(),
            );
        }
        let log = PlayLog::decode(&entry.log)
            .map_err(|e| format!("This play's presses do not read ({e})."))?;
        let timing = PlayTiming::decode(&entry.timing).ok();
        let summary = entry
            .summary
            .as_deref()
            .and_then(|s| serde_json::from_str::<PlaySummary>(s).ok());
        Ok(Replay {
            id: entry.id.clone(),
            record,
            log,
            timing,
            summary,
        })
    }

    /// The chart's content, as the play was of it.
    pub fn digest(&self) -> ChartDigest {
        self.record.chart.digest
    }

    /// Where the play's chart is in `song` (as stored): where it was, or
    /// where a chart of its content is now. Refused when none is: the
    /// chart changed since.
    pub fn chart(&self, song: &Song) -> Result<usize, String> {
        find_chart(song, &self.record.chart).ok_or_else(|| {
            "This chart changed since the play (its notes or timing are not the ones played): \
             its replay would be wrong."
                .to_string()
        })
    }

    /// `song` (as stored) as it was played: shifted by the play's song
    /// offset.
    pub fn played_song(&self, song: &Song) -> Song {
        song.shifted(self.record.song_offset).into_owned()
    }

    /// The play judged again from its log on `song` (as stored): its
    /// results. Refused when the chart changed.
    pub fn results(&self, song: &Song) -> Result<Results, String> {
        let index = self.chart(song)?;
        let song = self.played_song(song);
        Ok(replay(
            &song,
            index,
            &self.record.ruleset,
            &self.record.options,
            &self.log,
        ))
    }

    /// Played on these devices: the audio path and the display the
    /// record names.
    fn same_devices(&self, devices: Option<&DeviceProfile>) -> bool {
        let clock = &self.record.setup.clock;
        match (devices, &clock.audio, &clock.display) {
            (Some(d), Some(a), Some(v)) => d.audio.id == a.id && d.display.id == v.id,
            _ => false,
        }
    }

    /// The clock options a replay on `devices` runs with: the record's on
    /// the devices it was played on, else `here` (this device's): the
    /// record's corrected other hardware. Judging does not read them.
    pub fn clock(&self, devices: Option<&DeviceProfile>, here: ClockOptions) -> ClockOptions {
        if self.same_devices(devices) {
            let c = &self.record.setup.clock;
            ClockOptions {
                audio_offset: c.audio_offset,
                visual_offset: c.visual_offset,
                ..self.record.options.clock
            }
        } else {
            ClockOptions {
                rate: self.record.options.clock.rate,
                ..here
            }
        }
    }

    /// What a session replays the play with, on `devices` whose own
    /// clock options are `here`.
    pub fn replaying(&self, devices: Option<&DeviceProfile>, here: ClockOptions) -> Replaying {
        Replaying {
            log: self.log.clone(),
            options: PlayOptions {
                clock: self.clock(devices, here),
                ..self.record.options
            },
            ruleset: self.record.ruleset.clone(),
        }
    }

    /// The settings a replay is drawn with: those the record kept (the
    /// skin, the colors, the filter, the background, the videos, the
    /// lyrics, the timing-error display; the options as played), with
    /// this device's volume, bindings, offsets' profiles, graphics and
    /// the rest of what is not the play's from `current`.
    pub fn settings(&self, current: &Settings) -> Settings {
        let snapshot = Some(&self.record.settings)
            .filter(|v| v.is_object())
            .and_then(|v| serde_json::from_value::<Settings>(v.clone()).ok());
        let read = snapshot.is_some();
        let mut s = snapshot.unwrap_or_else(|| current.clone());
        s.sanitize();
        let drawn = &self.record.setup.drawn;
        let named = |name: &str| serde_json::Value::String(name.to_string());
        // The snapshot not read: what the record says it was drawn with,
        // over this device's (never today's look for the play's).
        if !read {
            s.noteskin = drawn.noteskin.clone();
            s.pump_noteskin = None;
            // The skin's parameters as drawn, and Dito's colors (older
            // records' only: settings of their own then).
            let key = drawn.noteskin.clone().unwrap_or_else(|| DITO.to_string());
            s.skin_params.remove(&key);
            if !drawn.skin_params.is_empty() {
                s.skin_params.insert(key, drawn.skin_params.clone());
            }
            s.old_note_colors = serde_json::from_value(named(&drawn.note_colors)).ok();
            s.old_chart_colors = Some(drawn.chart_colors);
            s.migrate();
            if let Ok(v) = serde_json::from_value(named(&drawn.background_frame)) {
                s.background_frame = v;
            }
            if let Ok(v) = serde_json::from_value(named(&drawn.video)) {
                s.video = v;
            }
            s.field_filter = drawn.field_filter;
            s.bg_brightness = drawn.bg_brightness;
            s.show_deltas = drawn.show_deltas;
        }
        // The options as played.
        let o = &self.record.options;
        if let ddi_engine::scroll::SpeedMod::XMod(x) = o.scroll.speed {
            s.speed = x;
        }
        s.reverse = o.scroll.reverse;
        s.scroll_action = o.scroll.scroll_action;
        s.transform = o.transform;
        s.appearance = o.appearance;
        s.lane_cover = o.cover;
        s.receptor_snap = o.receptor_snap;
        // This device's.
        s.volume = current.volume;
        s.controls = current.controls.clone();
        s.fast_pad_poll = current.fast_pad_poll;
        s.graphics = current.graphics;
        s.debug = current.debug;
        s.reduced_motion = current.reduced_motion;
        s.results_panels = current.results_panels.clone();
        s.audio_offset = current.audio_offset;
        s.visual_offset = current.visual_offset;
        s.audio_profiles = current.audio_profiles.clone();
        s.display_profiles = current.display_profiles.clone();
        s.song_offsets = current.song_offsets.clone();
        s.style = current.style.clone();
        s.import_details = current.import_details;
        s.import_copy = current.import_copy;
        s.import_videos = current.import_videos;
        s
    }

    /// The noteskin the replay draws the play's chart (of layout
    /// `layout`) with, when it is one that must be installed: not the
    /// built-in arrows, not a bundled one (they ship with the game; Dito,
    /// chosen by no choice, among them).
    pub fn skin(&self, settings: &Settings, layout: &str) -> Option<String> {
        settings
            .noteskin_for(layout)
            .filter(|k| *k != BUILT_IN && !k.starts_with(BUNDLED))
            .map(str::to_string)
    }

    /// The line a replay's (or a kept play's) results say when the play
    /// was judged by an older judge: today's judges it, and may differ.
    pub fn older_judge_note(&self) -> Option<String> {
        (self.record.judge_version < JUDGE_VERSION).then(|| {
            "This play was judged by an older version of the game; it is judged here by today's, \
             which may differ from what it showed."
                .to_string()
        })
    }

    /// What a missing skin's refusal says.
    pub fn missing_skin(key: &str) -> String {
        format!(
            "This play was drawn with the noteskin {key}, which is no longer installed: add it again \
             to watch the replay."
        )
    }

    /// What a missing song's refusal says.
    pub fn missing_song() -> String {
        "The song of this play is no longer in the library (no song has a chart with its notes)."
            .into()
    }

    /// Compares a watched replay's results with the log judged at once
    /// (`offline`, which must be equal) and, when the record was judged
    /// by today's judge, with the timing and summary it kept.
    pub fn check(&self, watched: &Results, offline: &Results) -> ReplayCheck {
        let mut problems = Vec::new();
        if comparable(watched) != comparable(offline) {
            problems.push("the replay and its inputs judged at once differ".to_string());
        }
        let with_record = self.record.judge_version == JUDGE_VERSION;
        if with_record {
            match &self.timing {
                Some(t) if *t != PlayTiming::from_results(watched) => {
                    problems.push("its judgements or life are not the play's".into())
                }
                _ => {}
            }
            // Both read from JSON text, as the kept one was (its numbers
            // the nearest floats to the text, whatever wrote it: `0` or
            // `0.0`, JavaScript or serde).
            let reread = |w: &Worked| -> Option<Worked> {
                serde_json::from_str(&serde_json::to_string(w).ok()?).ok()
            };
            if let Some(w) = self.summary.as_ref().and_then(|s| s.worked.as_ref())
                && w.judge_version == JUDGE_VERSION
                && let Some(now) = reread(&Worked::from_results(watched))
                && comparable_worked(w) != comparable_worked(&now)
            {
                problems.push("its score or counts are not the play's".into());
            }
        }
        ReplayCheck {
            matches: problems.is_empty(),
            with_record,
            problems,
        }
    }

    /// [`Self::check`] of a watched replay against the log judged at once
    /// on `song` (as stored); a refusal of that is what differed.
    pub fn check_on(&self, song: &Song, watched: &Results) -> ReplayCheck {
        match self.results(song) {
            Ok(offline) => self.check(watched, &offline),
            Err(e) => ReplayCheck {
                problems: vec![e],
                ..Default::default()
            },
        }
    }

    /// The results' line under a replay's or a kept play's results: when
    /// it was played (`tz` as [`play_rows`]'s).
    pub fn when_line(&self, tz: &dyn Fn(f64) -> i32) -> String {
        format!(
            "Played {}",
            local_time(self.record.time, tz(self.record.time))
        )
    }
}

/// `ms` since the Unix epoch as "2026-10-16 14:03", `tz_minutes` east of
/// UTC.
pub fn local_time(ms: f64, tz_minutes: i32) -> String {
    if !ms.is_finite() {
        return String::new();
    }
    let minutes = (ms / 60_000.0).floor() as i64 + i64::from(tz_minutes);
    let days = minutes.div_euclid(1440);
    let in_day = minutes.rem_euclid(1440);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", in_day / 60, in_day % 60)
}

/// A ruleset's name as the plays list shows it.
fn ruleset_name(key: &RulesetKey) -> String {
    match key.id.as_str() {
        "itg" => "ITG".into(),
        "itg-fa+" => "ITG FA+".into(),
        "sm5" => "StepMania 5".into(),
        "ddr-a" => "DDR A".into(),
        "ddr-extreme" => "DDR EXTREME".into(),
        "ffr" => "FFR".into(),
        "danoni" => "Dancing☆Onigiri".into(),
        other => other.to_string(),
    }
}

/// The plays kept of a chart's content (under any song id), newest
/// first, as song select's plays list shows them; `best` is the chart's
/// best play's id under the rules chosen now. Times in the player's
/// time: `tz` gives the minutes east of UTC at a time (ms since the
/// epoch).
pub fn play_rows(
    history: &History,
    digest: ChartDigest,
    best: Option<&str>,
    tz: &dyn Fn(f64) -> i32,
) -> Vec<PlayRow> {
    let mut rows: Vec<PlayRow> = history
        .iter()
        .filter(|(_, s)| s.chart.digest == digest)
        .map(|(id, s)| {
            let (lamp, lamp_text) = match &s.worked {
                Some(w) => w.lamp.names(),
                None => ("None", ""),
            };
            PlayRow {
                id: id.clone(),
                when: local_time(s.time, tz(s.time)),
                grade: s.worked.as_ref().map_or(String::new(), |w| w.grade.clone()),
                score: s
                    .worked
                    .as_ref()
                    .map_or("…".into(), |w| score_text(&w.score)),
                lamp: lamp.into(),
                lamp_text: lamp_text.into(),
                ruleset: ruleset_name(&s.ruleset),
                mods: s
                    .options
                    .as_ref()
                    .map(crate::screens::play::options_line)
                    .into_iter()
                    .chain(s.ruleset.stepmania.summary())
                    .collect::<Vec<_>>()
                    .join(" · "),
                autoplay: s.autoplay,
                best: best == Some(id.as_str()),
                pending: s.is_stale(),
            }
        })
        .collect();
    // Ids begin with the time: newest first.
    rows.reverse();
    rows
}

/// Song select's plays list over the history kept in `plays`
/// (`crate::screens::select::SelectScreen::with_plays`): a chart's plays
/// by its digest, its best marked; `tz` as [`play_rows`]'s.
pub fn plays_list(
    plays: std::rc::Rc<super::Plays>,
    tz: std::rc::Rc<dyn Fn(f64) -> i32>,
) -> crate::screens::select::PlaysFn {
    std::rc::Rc::new(move |_song, chart| {
        let Some(digest) = chart.digest else {
            return Vec::new();
        };
        let best = chart.best.as_ref().map(|b| b.play.as_str());
        play_rows(&plays.history(), digest, best, &*tz)
    })
}

/// The ghost of a chart's best play (`docs/plans/play-history.md`,
/// Replays): its chart as played, its rules, options and log, played
/// again beside a play ([`Ghost::run`]) so it can show how far ahead of
/// it or behind it the play is at the same point. Off by default
/// (`Settings::ghost`).
#[derive(Clone, Debug, PartialEq)]
pub struct Ghost {
    song: Song,
    chart: usize,
    ruleset: RulesetKey,
    options: PlayOptions,
    log: PlayLog,
    /// Its final score's value.
    last: f64,
    /// Seconds from the play's song time to the ghost's: the song
    /// offsets each was played with differ (`Ghost::for_play`).
    shift: f64,
}

/// [`GhostRun::delta`]'s answer: percentage points, or points (money,
/// EX), as the ruleset scores.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GhostDelta {
    Percent(f64),
    Points(f64),
}

impl Ghost {
    /// `replay`'s ghost on its chart in `song` (as stored).
    pub fn of(replay: &Replay, song: &Song) -> Result<Ghost, String> {
        let chart = replay.chart(song)?;
        let last = super::score_value(&replay.results(song)?.score);
        Ok(Ghost {
            song: replay.played_song(song),
            chart,
            ruleset: replay.record.ruleset.clone(),
            options: replay.record.options,
            log: replay.log.clone(),
            last,
            shift: replay.record.song_offset,
        })
    }

    /// For a play shifted by song offset `played` (seconds): the same
    /// point of the chart is that much apart in the two plays' times.
    pub fn for_play(self, played: f64) -> Ghost {
        Ghost {
            shift: self.shift - played,
            ..self
        }
    }

    /// Its final score's value.
    pub fn last(&self) -> f64 {
        self.last
    }

    /// The ghost played from its start, to be advanced with a play.
    pub fn run(&self) -> GhostRun {
        let ruleset = self.ruleset.build(&self.song.charts[self.chart]);
        let mut player =
            ddi_engine::player::Player::new(&self.song, self.chart, ruleset, self.options);
        player.start(0.0, self.log.start);
        let view = player.ruleset().score.view();
        GhostRun {
            player,
            inputs: self.log.inputs.clone(),
            next: 0,
            percent: view.money.is_none() && view.percent.is_some(),
            shift: self.shift,
        }
    }
}

/// A ghost being played beside a play ([`Ghost::run`]).
pub struct GhostRun {
    player: ddi_engine::player::Player,
    inputs: Vec<super::LoggedInput>,
    next: usize,
    percent: bool,
    shift: f64,
}

impl GhostRun {
    /// How far `score`, of a play judged up to song second `t` (its
    /// edges up to `t`, the rest up to `t` less the judge's lag, as
    /// `Player::update_at` judges), is ahead of the ghost judged the same
    /// way (behind: negative). `t` only goes forward.
    pub fn delta(&mut self, score: &ddi_engine::rules::ScoreView, t: f64) -> GhostDelta {
        let t = t + self.shift;
        while let Some(i) = self.inputs.get(self.next).filter(|i| i.time <= t) {
            self.player.input_at(i.lane, i.pressed, i.time);
            self.next += 1;
        }
        self.player.update_at(t);
        let d = super::score_value(score) - super::score_value(&self.player.ruleset().score.view());
        if self.percent {
            GhostDelta::Percent(d * 100.0)
        } else {
            GhostDelta::Points(d)
        }
    }
}
