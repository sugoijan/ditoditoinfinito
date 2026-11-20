//! Chart ratings of the game's own (`docs/plans/msd-ratings.md`): MSD,
//! Etterna's chart difficulty (`ddi_msd`), and the DDI estimate, our own
//! (`ddi_rating`, a try since round Z2: "The DDI estimate as a source"),
//! shown in place of the simfiles' numbers and as song select's radar
//! ([`Settings::chart_ratings`](crate::state::settings::Settings::chart_ratings):
//! the song's, MSD or the DDI estimate, the estimate by default). The
//! radar is always shown (round SLB) and follows the choice: the
//! estimate's profile under the estimate, MSD's skillsets under MSD, the
//! song's own Groove values under the song's.
//!
//! The estimate, where shown, is kept as [`ChartEstimate`] (the bundled
//! songs' in their manifest, the rest under [`ESTIMATE_KEY`] beside MSD's,
//! with the estimator's, the scale's and the profile's rule's names: a new
//! one forgets them), made by the same shells one song at a time
//! ([`Job::estimates`], [`rate_song`]).
//!
//! - What is shown: [`set_mode`] from the settings (at boot and when
//!   Options changes them), read by the screens ([`msd_shown`],
//!   [`radar_shown`]). With both Off nothing here reads or writes storage
//!   and nothing is rated.
//! - What is known: a chart's eight numbers by its digest
//!   ([`ddi_chart::ChartDigest`]) and the calculator's version. The bundled
//!   songs' come in their manifest (`xtask gen-songs`) and are only
//!   remembered ([`remember`]); every other chart's are rated in the
//!   background and kept under [`KEY`] in the device's local storage, a
//!   chart 64 characters (its digest and its eight numbers in hundredths,
//!   hex), some 640 kB at 10,000 charts. A new calculator version
//!   ([`ddi_msd::CALC_VERSION`]) forgets them and rates again.
//! - What is rated: song select offers its songs whose four-lane charts
//!   have none yet ([`offer`]), the focused song and its neighbours first
//!   ([`first`]); a shell takes one song at a time ([`next`]), rates it
//!   off its main thread (a worker on the web, a thread on desktop) and
//!   hands the numbers back ([`done`]). Nothing is taken while a play runs
//!   ([`set_playing`], from the prompt's Start to the results), as the
//!   maintainer asked, even though it could run beside it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

use ddi_chart::ChartDigest;
pub use ddi_library::manifest::{ChartEstimate, PROFILE_RULE};
use ddi_platform::storage::{self, Area};
use serde::{Deserialize, Serialize};

/// Where the ratings are kept.
pub const KEY: &str = "ddi.msd.v1";
/// Where the DDI estimates are kept.
pub const ESTIMATE_KEY: &str = "ddi.estimate.v1";

/// What a chart's number is (`Settings::chart_ratings`, Options →
/// Display): the song's own, MSD, or the DDI estimate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    #[default]
    Song,
    Msd,
    Estimate,
}

/// The estimates as stored: who made them, and a chart after another
/// ([`encode_estimate`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct StoredEstimates {
    estimator: String,
    scale: String,
    rule: String,
    charts: String,
}

/// The names the kept estimates must carry to be read: the estimator's,
/// its scale's and the profile's rule's.
fn estimate_names() -> (&'static str, &'static str, &'static str) {
    (ddi_rating::ESTIMATOR, ddi_rating::SCALE, PROFILE_RULE)
}

/// A chart's estimate as kept: its digest (32 hex digits), the decimal in
/// hundredths (4), the level (2), the flags (1: the number qualified, the
/// profile qualified, a profile, none at all), then with a profile its
/// eight costs in hundredths (4 each); `None`, the chart estimated and
/// none made, kept as its flag alone. 39 or 71 characters.
fn encode_estimate(digest: u128, e: Option<&ChartEstimate>) -> String {
    let hundredths = |x: f32| (x.max(0.0) * 100.0).round().min(65535.0) as u16;
    let Some(e) = e else {
        return format!("{digest:032x}{:04x}{:02x}{:x}", 0, 0, 8);
    };
    let flags = u8::from(e.scalar) | u8::from(e.studied) << 1 | u8::from(e.profile.is_some()) << 2;
    let mut s = format!("{digest:032x}{:04x}{:02x}{flags:x}", hundredths(e.decimal), e.level);
    for c in e.profile.iter().flatten() {
        s.push_str(&format!("{:04x}", hundredths(*c)));
    }
    s
}

/// Every estimate of `text` ([`encode_estimate`]), read until one does not
/// read.
fn decode_estimates(text: &str) -> Vec<(u128, Option<ChartEstimate>)> {
    let mut out = Vec::new();
    let mut at = 0;
    let hex = |s: &str| u32::from_str_radix(s, 16).ok();
    while let Some(head) = text.get(at..at + 39) {
        let (Ok(digest), Some(decimal), Some(level), Some(flags)) = (
            u128::from_str_radix(&head[..32], 16),
            hex(&head[32..36]),
            hex(&head[36..38]),
            hex(&head[38..39]),
        ) else {
            break;
        };
        at += 39;
        if flags & 8 != 0 {
            out.push((digest, None));
            continue;
        }
        let profile = if flags & 4 != 0 {
            let Some(body) = text.get(at..at + 32) else {
                break;
            };
            at += 32;
            let mut p = [0f32; 8];
            for (k, c) in p.iter_mut().enumerate() {
                let Some(v) = hex(&body[k * 4..k * 4 + 4]) else {
                    return out;
                };
                *c = v as f32 / 100.0;
            }
            Some(p)
        } else {
            None
        };
        out.push((
            digest,
            Some(ChartEstimate {
                decimal: decimal as f32 / 100.0,
                level: level as u8,
                scalar: flags & 1 != 0,
                studied: flags & 2 != 0,
                profile,
            }),
        ));
    }
    out
}

/// A chart's eight numbers: the overall rating, then Stream, Jumpstream,
/// Handstream, Stamina, JackSpeed, Chordjack and Technical
/// (`ddi_msd::Skillset`'s order).
pub type Values = [f32; 8];

/// The ratings as stored.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    /// The calculator version they were made by.
    version: u32,
    /// A chart after another: 32 hex digits of digest, then eight numbers
    /// in hundredths as four hex digits each.
    charts: String,
}

const RECORD: usize = 32 + 8 * 4;

fn encode(digest: u128, v: &Values) -> String {
    let mut s = format!("{digest:032x}");
    for x in v {
        let h = (x.max(0.0) * 100.0).round().min(65535.0) as u16;
        s.push_str(&format!("{h:04x}"));
    }
    s
}

fn decode(record: &str) -> Option<(u128, Values)> {
    if record.len() != RECORD || !record.is_ascii() {
        return None;
    }
    let digest = u128::from_str_radix(&record[..32], 16).ok()?;
    let mut v = [0f32; 8];
    for (k, x) in v.iter_mut().enumerate() {
        let at = 32 + k * 4;
        *x = f32::from(u16::from_str_radix(&record[at..at + 4], 16).ok()?) / 100.0;
    }
    Some((digest, v))
}

/// A song to rate: its library id, the charts of it (their places in the
/// song) that want MSD, and those that want the DDI estimate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Job {
    pub song: String,
    pub charts: Vec<usize>,
    pub estimates: Vec<usize>,
}

/// What a shell's rating of a [`Job`] found for a chart: its digest, its
/// MSD (`None` where not asked or it could not be rated: not four lanes
/// after all, or a file the calculator skips), and where it was asked
/// (`estimated`) its estimate (`None`: none could be made).
#[derive(Clone, Debug, PartialEq)]
pub struct Rated {
    pub index: usize,
    pub digest: ChartDigest,
    pub values: Option<Values>,
    pub estimated: bool,
    pub estimate: Option<ChartEstimate>,
}

impl Default for Rated {
    fn default() -> Rated {
        Rated {
            index: 0,
            digest: ChartDigest(0),
            values: None,
            estimated: false,
            estimate: None,
        }
    }
}

/// Where the rating stands, for tests (`__DDI_DEBUG.ratings`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Status {
    /// Whether either option is On.
    pub wanted: bool,
    /// A play runs: nothing is taken.
    pub paused: bool,
    /// Songs waiting.
    pub pending: usize,
    /// The song being rated, if any.
    pub rating: Option<String>,
    /// Charts rated (and kept) this session.
    pub rated: usize,
    /// Charts known, kept and bundled.
    pub known: usize,
    /// Charts estimated, kept and bundled.
    pub estimates: usize,
}

#[derive(Default)]
struct State {
    source: Source,
    radar: bool,
    /// MSD has been shown this session: numbers drawn fainter may need
    /// drawing plainly again.
    ever: bool,
    /// The kept ratings, read (once wanted) from this store generation.
    loaded: Option<u64>,
    kept: HashMap<u128, Values>,
    /// The bundled songs' (from their manifest): never stored.
    bundled: HashMap<u128, Values>,
    dirty: bool,
    unsaved: usize,
    /// The DDI estimates: as MSD's, read from their own store
    /// ([`ESTIMATE_KEY`]); `None` where the chart has none.
    est_loaded: Option<u64>,
    est_kept: HashMap<u128, Option<ChartEstimate>>,
    est_bundled: HashMap<u128, ChartEstimate>,
    est_dirty: bool,
    /// Bumped at each new rating, so screens show it.
    generation: u64,
    pending: VecDeque<Job>,
    failed: HashSet<String>,
    rating: Option<String>,
    playing: bool,
    rated: usize,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
    STATE.with_borrow_mut(f)
}

impl State {
    /// MSD is made: shown as the number, or on the radar, which shows
    /// MSD's skillsets but under the DDI estimate.
    fn wants_msd(&self) -> bool {
        self.source == Source::Msd || (self.radar && self.source == Source::Song)
    }

    fn wants_estimate(&self) -> bool {
        self.source == Source::Estimate
    }

    fn wanted(&self) -> bool {
        self.wants_msd() || self.wants_estimate()
    }

    /// The kept estimates, read where they were not.
    fn load_estimates(&mut self) {
        let generation = storage::generation();
        if self.est_loaded == Some(generation) {
            return;
        }
        self.est_loaded = Some(generation);
        self.est_kept.clear();
        self.est_dirty = false;
        let stored: StoredEstimates = storage::load(Area::Local, ESTIMATE_KEY).unwrap_or_default();
        let (estimator, scale, rule) = estimate_names();
        if (stored.estimator.as_str(), stored.scale.as_str(), stored.rule.as_str())
            != (estimator, scale, rule)
        {
            return;
        }
        self.est_kept.extend(decode_estimates(&stored.charts));
    }

    fn save_estimates(&mut self) {
        if !self.est_dirty {
            return;
        }
        let mut digests: Vec<_> = self.est_kept.keys().copied().collect();
        digests.sort_unstable();
        let charts: String = digests
            .iter()
            .map(|d| encode_estimate(*d, self.est_kept[d].as_ref()))
            .collect();
        let (estimator, scale, rule) = estimate_names();
        let stored = StoredEstimates {
            estimator: estimator.into(),
            scale: scale.into(),
            rule: rule.into(),
            charts,
        };
        let _ = storage::save(Area::Local, ESTIMATE_KEY, &stored);
        self.est_dirty = false;
    }

    /// A chart's estimate: `Some(None)` estimated and none made, `None`
    /// not estimated yet.
    fn estimate(&mut self, digest: ChartDigest) -> Option<Option<ChartEstimate>> {
        if let Some(e) = self.est_bundled.get(&digest.0) {
            return Some(Some(e.clone()));
        }
        self.load_estimates();
        self.est_kept.get(&digest.0).cloned()
    }

    /// The kept ratings, read where they were not.
    fn load(&mut self) {
        let generation = storage::generation();
        if self.loaded == Some(generation) {
            return;
        }
        self.loaded = Some(generation);
        self.kept.clear();
        self.dirty = false;
        let stored: Stored = storage::load(Area::Local, KEY).unwrap_or_default();
        if stored.version != ddi_msd::CALC_VERSION {
            return;
        }
        let bytes = stored.charts.as_bytes();
        for chunk in bytes.chunks(RECORD) {
            if let Some((d, v)) = std::str::from_utf8(chunk).ok().and_then(decode) {
                self.kept.insert(d, v);
            }
        }
    }

    fn save(&mut self) {
        if !self.dirty {
            return;
        }
        let mut digests: Vec<_> = self.kept.keys().copied().collect();
        digests.sort_unstable();
        let mut charts = String::with_capacity(digests.len() * RECORD);
        for d in digests {
            charts.push_str(&encode(d, &self.kept[&d]));
        }
        let stored = Stored {
            version: ddi_msd::CALC_VERSION,
            charts,
        };
        let _ = storage::save(Area::Local, KEY, &stored);
        self.dirty = false;
        self.unsaved = 0;
    }

    fn get(&mut self, digest: ChartDigest) -> Option<Values> {
        if let Some(v) = self.bundled.get(&digest.0) {
            return Some(*v);
        }
        self.load();
        self.kept.get(&digest.0).copied()
    }
}

/// What the settings show: a rating in the numbers' place (`source`), the
/// radar. A change of what is made forgets the songs waiting (song select
/// offers them again for what is wanted now).
pub fn set_mode(source: Source, radar: bool) {
    with(|s| {
        let before = (s.wants_msd(), s.wants_estimate());
        s.source = source;
        s.radar = radar;
        s.ever |= source != Source::Song;
        if before != (s.wants_msd(), s.wants_estimate()) {
            s.pending.clear();
            s.failed.clear();
        }
    });
}

/// What the numbers' place shows.
pub fn source() -> Source {
    with(|s| s.source)
}

/// Whether a rated chart shows its MSD in its number's place.
pub fn msd_shown() -> bool {
    with(|s| s.source == Source::Msd)
}

/// Whether MSD is made (shown, or on the radar).
pub fn wants_msd() -> bool {
    with(|s| s.wants_msd())
}

/// Whether the DDI estimate is made (and shown, and on the radar).
pub fn wants_estimate() -> bool {
    with(|s| s.wants_estimate())
}

/// Whether MSD has been shown this session (a screen then draws every
/// number's faintness, so one shown fainter is plain again once Off).
pub fn ever_shown() -> bool {
    with(|s| s.ever)
}

/// Whether song select shows the chart's skill radar.
pub fn radar_shown() -> bool {
    with(|s| s.radar)
}

/// Whether either is On (ratings are made and read only then).
pub fn wanted() -> bool {
    with(|s| s.wanted())
}

/// A chart's numbers, where known and wanted.
pub fn get(digest: ChartDigest) -> Option<Values> {
    with(|s| if s.wants_msd() { s.get(digest) } else { None })
}

/// A chart's DDI estimate where wanted: `Some(None)` estimated and none
/// made, `None` not estimated yet (or not wanted).
pub fn estimate(digest: ChartDigest) -> Option<Option<ChartEstimate>> {
    with(|s| if s.wants_estimate() { s.estimate(digest) } else { None })
}

/// Whether a chart is one the DDI estimate is made for: an original
/// Single chart (`ddi_rating::estimate`'s own condition).
pub fn estimable(layout: &str, original: bool) -> bool {
    original && layout == "dance-single"
}

/// A bundled chart's estimate (from its manifest): known for the session,
/// never stored.
pub fn remember_estimate(digest: ChartDigest, e: ChartEstimate) {
    with(|s| {
        s.est_bundled.insert(digest.0, e);
    });
}

/// A rating in a chart's number's place, where the source chosen has one
/// for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shown {
    Msd(f32),
    /// The estimate's level and decimal.
    Estimate(u8, f32),
}

impl Shown {
    /// As the chart buttons show it: MSD whole, the
    /// estimate's decimal to a tenth (its rounded level is shown nowhere:
    /// round AD, 2026-11-19, the maintainer: "A tenth everywhere").
    pub fn whole(&self) -> String {
        match self {
            Shown::Msd(m) => format!("{}", m.round() as i64),
            Shown::Estimate(_, d) => format!("{d:.1}"),
        }
    }

    /// As a ladder's row and the prompt show it: both to a tenth.
    pub fn fine(&self) -> String {
        match self {
            Shown::Msd(m) => format!("{m:.1}"),
            Shown::Estimate(_, d) => format!("{d:.1}"),
        }
    }

    /// As the radar's side's detail shows it: to a tenth.
    pub fn detail(&self) -> String {
        match self {
            Shown::Msd(m) => format!("{m:.1}"),
            Shown::Estimate(_, d) => format!("{d:.1}"),
        }
    }
}

/// What chart `digest` (of `layout`, an original or a generated version)
/// shows in its number's place: its MSD where Chart ratings is MSD (four
/// lanes), its estimate's level where it is the DDI estimate (an original
/// Single chart whose number is qualified, `within_scalar_domain`); `None`
/// otherwise (the song's own number then, fainter).
pub fn shown(digest: Option<ChartDigest>, layout: &str, original: bool) -> Option<Shown> {
    let digest = digest?;
    match source() {
        Source::Song => None,
        Source::Msd => {
            if !ddi_msd::chart::four_keys(layout) {
                return None;
            }
            get(digest).map(|v| Shown::Msd(v[0]))
        }
        Source::Estimate => {
            if !estimable(layout, original) {
                return None;
            }
            estimate(digest)
                .flatten()
                .filter(|e| e.scalar)
                .map(|e| Shown::Estimate(e.level, e.decimal))
        }
    }
}

/// A bundled chart's numbers (from its manifest): known for the session,
/// never stored.
pub fn remember(digest: ChartDigest, values: Values) {
    with(|s| {
        s.bundled.insert(digest.0, values);
    });
}

/// Bumped at every new rating.
pub fn generation() -> u64 {
    with(|s| s.generation)
}

/// Songs to rate: those of `jobs` with a chart not known yet, after any
/// already waiting (a song offered again keeps its place).
pub fn offer(jobs: impl IntoIterator<Item = Job>) {
    // Made before the state is borrowed (making them may read it).
    let jobs: Vec<Job> = jobs.into_iter().collect();
    with(|s| {
        if !s.wanted() {
            return;
        }
        s.load();
        for job in jobs {
            if s.failed.contains(&job.song)
                || s.rating.as_deref() == Some(job.song.as_str())
                || s.pending.iter().any(|j| j.song == job.song)
            {
                continue;
            }
            if !job.charts.is_empty() || !job.estimates.is_empty() {
                s.pending.push_back(job);
            }
        }
    });
}

/// Rates these songs first (the focused one, then its neighbours), in
/// their order.
pub fn first(songs: &[&str]) {
    with(|s| {
        for id in songs.iter().rev() {
            if let Some(at) = s.pending.iter().position(|j| j.song == *id) {
                let job = s.pending.remove(at).expect("there");
                s.pending.push_front(job);
            }
        }
    });
}

/// A chart wanted soon (a room's picked chart, which a follower sees):
/// its song rated next, where ratings are wanted and it has none yet.
pub fn want_first(
    song: &str,
    index: usize,
    digest: Option<ChartDigest>,
    layout: &str,
    original: bool,
) {
    let msd = wants_msd()
        && ddi_msd::chart::four_keys(layout)
        && !digest.is_some_and(|d| get(d).is_some());
    let est = wants_estimate()
        && estimable(layout, original)
        && !digest.is_some_and(|d| estimate(d).is_some());
    if !msd && !est {
        return;
    }
    with(|s| {
        if s.failed.contains(song) || s.rating.as_deref() == Some(song) {
            return;
        }
        let mut job = match s.pending.iter().position(|j| j.song == song) {
            Some(at) => s.pending.remove(at).expect("there"),
            None => Job {
                song: song.to_string(),
                ..Job::default()
            },
        };
        if msd && !job.charts.contains(&index) {
            job.charts.push(index);
        }
        if est && !job.estimates.contains(&index) {
            job.estimates.push(index);
        }
        s.pending.push_front(job);
    });
}

/// A play runs (from its Start to its results): nothing new is taken.
pub fn set_playing(playing: bool) {
    with(|s| s.playing = playing);
}

/// Whether a play runs.
pub fn playing() -> bool {
    with(|s| s.playing)
}

/// The next song to rate, if any is waiting and none is being rated (one
/// at a time), and no play runs.
pub fn next() -> Option<Job> {
    with(|s| {
        if !s.wanted() || s.rating.is_some() {
            return None;
        }
        if s.playing {
            return None;
        }
        let job = s.pending.pop_front()?;
        s.rating = Some(job.song.clone());
        Some(job)
    })
}

/// `song` rated (`rated` empty where it could not be read at all).
pub fn done(song: &str, rated: Vec<Rated>) {
    with(|s| {
        if s.rating.as_deref() == Some(song) {
            s.rating = None;
        }
        s.load();
        s.load_estimates();
        let mut any = false;
        for r in rated {
            if let Some(v) = r.values {
                s.kept.insert(r.digest.0, v.map(|x| (x * 100.0).round() / 100.0));
                s.dirty = true;
                s.unsaved += 1;
                s.rated += 1;
                any = true;
            }
            if r.estimated {
                // Kept whether one was made or not: a chart is estimated
                // once (until a new estimator).
                s.est_kept.insert(r.digest.0, r.estimate);
                s.est_dirty = true;
                s.unsaved += 1;
                s.rated += 1;
                any = true;
            }
        }
        if !any {
            s.failed.insert(song.to_string());
        }
        if any {
            s.generation += 1;
        }
        // Kept in batches: every 50 charts, and when nothing waits.
        if s.unsaved >= 50 || s.pending.is_empty() {
            s.save();
            s.save_estimates();
            s.unsaved = 0;
        }
    });
}

/// Writes what is not kept yet.
pub fn flush() {
    with(|s| {
        s.save();
        s.save_estimates();
    });
}

/// Where the rating stands.
pub fn status() -> Status {
    with(|s| Status {
        wanted: s.wanted(),
        paused: s.playing,
        pending: s.pending.len(),
        rating: s.rating.clone(),
        rated: s.rated,
        known: s.kept.len() + s.bundled.len(),
        estimates: s.est_kept.len() + s.est_bundled.len(),
    })
}

/// Where a chart's rating stands, for what song select says in the
/// radar's place (round Z1; the estimate's since round Z2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Standing {
    /// Its MSD's eight numbers (the radar's skillsets).
    Rated(Values),
    /// Its DDI estimate's profile ([`ChartEstimate::profile`]): the eight
    /// skills' costs, in percentage points (all small: a regular octagon,
    /// round Z3, `crate::radar::estimate_spokes`).
    Profile([f32; 8]),
    /// Its DDI estimate is outside what it was studied on (no profile).
    Outside,
    /// Waiting to be rated, or being rated (its song is offered, the
    /// shown one first): it comes by itself.
    Rating,
    /// Not a layout the radar's source rates (MSD: not four lanes for one
    /// player; the estimate: not an original Single chart).
    NoLayout,
    /// None, nor any to come (it could not be rated, or nothing rates it).
    Unrated,
    /// Under "From the song": its Groove values (stream, voltage, air,
    /// freeze, chaos; `ddi_chart::groove`), raw, read from the song list
    /// (song select's `ChartEntry::groove`), never rated in the
    /// background.
    Groove([f32; 5]),
}

/// Where chart `digest` (of layout `layout`, an original or a generated
/// version) of song `song` stands for the radar, whose source follows
/// Chart ratings: the DDI estimate's profile under the estimate, MSD's
/// skillsets otherwise.
pub fn standing(song: &str, digest: Option<ChartDigest>, layout: &str, original: bool) -> Standing {
    let waiting = || {
        with(|s| {
            s.wanted()
                && (s.rating.as_deref() == Some(song) || s.pending.iter().any(|j| j.song == song))
        })
    };
    if source() == Source::Estimate {
        if !estimable(layout, original) {
            return Standing::NoLayout;
        }
        return match digest.and_then(estimate) {
            Some(Some(e)) => match e.profile {
                Some(p) => Standing::Profile(p),
                None if !e.studied => Standing::Outside,
                None => Standing::Unrated,
            },
            Some(None) => Standing::Unrated,
            None if waiting() => Standing::Rating,
            None => Standing::Unrated,
        };
    }
    if !ddi_msd::chart::four_keys(layout) {
        return Standing::NoLayout;
    }
    if let Some(v) = digest.and_then(get) {
        return Standing::Rated(v);
    }
    if waiting() { Standing::Rating } else { Standing::Unrated }
}

/// The rating shown in the number's place of `song`'s chart `index` (as a
/// ladder's row and the prompt show it, [`Shown::fine`]): where Chart
/// ratings shows one and the chart has it.
pub fn shown_number(song: &ddi_chart::Song, index: usize) -> Option<String> {
    let chart = song.charts.get(index)?;
    shown(
        Some(song.chart_digest(index)),
        &chart.layout,
        chart.derivation.is_none(),
    )
    .map(|s| s.fine())
}

/// The eight numbers of `chart` of `song`, rated now, kept and returned
/// (a room's chart, read for its stage before the play). `None` where it
/// has none, or ratings are not wanted. Known ones are not rated again.
pub fn rate_now(song: &ddi_chart::Song, index: usize) -> Option<Values> {
    if !wants_msd() {
        return None;
    }
    let chart = song.charts.get(index)?;
    if !ddi_msd::chart::four_keys(&chart.layout) {
        return None;
    }
    let digest = song.chart_digest(index);
    if let Some(v) = get(digest) {
        return Some(v);
    }
    let values = ddi_msd::chart::chart_msd(song, chart)?;
    with(|s| {
        s.load();
        let v = values.map(|x| (x * 100.0).round() / 100.0);
        s.kept.insert(digest.0, v);
        s.dirty = true;
        s.rated += 1;
        s.generation += 1;
        s.save();
    });
    get(digest)
}

/// Rates every chart of a song that wants it: MSD for `charts`, the DDI
/// estimate for `estimates` (a job's, [`Job`]); what a shell's thread or
/// the web's worker runs, from the song's parsed simfile. One entry a
/// chart.
pub fn rate_song(song: &ddi_chart::Song, charts: &[usize], estimates: &[usize]) -> Vec<Rated> {
    let mut out: Vec<Rated> = Vec::new();
    for &index in charts {
        let Some(chart) = song.charts.get(index) else {
            continue;
        };
        out.push(Rated {
            index,
            digest: song.chart_digest(index),
            values: ddi_msd::chart::chart_msd(song, chart),
            ..Rated::default()
        });
    }
    for &index in estimates {
        if index >= song.charts.len() {
            continue;
        }
        let e = estimate_chart(song, index);
        match out.iter_mut().find(|r| r.index == index) {
            Some(r) => {
                r.estimated = true;
                r.estimate = e;
            }
            None => out.push(Rated {
                index,
                digest: song.chart_digest(index),
                estimated: true,
                estimate: e,
                ..Rated::default()
            }),
        }
    }
    out
}

/// Chart `index` of `song` estimated (`ddi_rating::estimate`) and read as
/// the game keeps it (through the estimator's JSON, as the web's worker
/// hands it over: [`ChartEstimate::from_rating_json`]).
pub fn estimate_chart(song: &ddi_chart::Song, index: usize) -> Option<ChartEstimate> {
    let e = ddi_rating::estimate(song, index).ok()?;
    ChartEstimate::from_rating_json(&serde_json::to_string(&e).ok()?)
}

/// A song whose own file is no simfile (a Dancing☆Onigiri work, an FFR
/// level) as one the web's rating worker reads (round Y3: they were never
/// rated there, the worker reading simfiles alone): charts `charts` of
/// `song`, read as a play reads it (generated versions included), written
/// as an `.ssc` of those charts alone (`ddi_chart`'s writer: what is read
/// is written back equal), and what each of its charts is in `song` (its
/// place and its digest, which a rating is kept by). The worker's answer
/// is put back with [`placed`].
pub fn rating_simfile(song: &ddi_chart::Song, charts: &[usize]) -> (String, Vec<(usize, ChartDigest)>) {
    let mut alone = song.clone();
    alone.charts = Vec::new();
    // Written from the model alone (what the file said is not this file's),
    // a generated version as a chart of its own (the writer leaves
    // generated ones out, the reader makes them again).
    alone.source.unknown_tags.clear();
    let mut map = Vec::new();
    for &index in charts {
        if let Some(c) = song.charts.get(index) {
            let mut c = c.clone();
            c.derivation = None;
            alone.charts.push(c);
            map.push((index, song.chart_digest(index)));
        }
    }
    (ddi_chart::formats::sm_write::write_ssc(&alone), map)
}

/// The worker's answer for a [`rating_simfile`]: each of its charts
/// (numbered as written) put back in its place in the song, by the digest
/// the song knows it by.
pub fn placed(rated: Vec<Rated>, map: &[(usize, ChartDigest)]) -> Vec<Rated> {
    rated
        .into_iter()
        .filter_map(|r| {
            let &(index, digest) = map.get(r.index)?;
            Some(Rated { index, digest, ..r })
        })
        .collect()
}

/// Forgets the session's queue and what was read (tests).
#[cfg(test)]
pub(crate) fn reset() {
    with(|s| *s = State::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    fn digest(n: u128) -> ChartDigest {
        ChartDigest(n)
    }

    /// Round Y3: a Dancing☆Onigiri work's generated four-lane version
    /// (Without onigiri, on Single) is rated as any four-lane chart, its
    /// notes as generated, kept by its own digest: written as the web's
    /// worker reads it, parsed back there and rated, it gives exactly the
    /// numbers the song's own chart gives (as desktop rates it, from the
    /// song as a play reads it).
    #[test]
    fn a_works_generated_four_lane_version_rates_as_its_own_chart() {
        use ddi_chart::formats::danoni::{self, Dos, DosFlags};
        // Enough notes for every skillset: a stream over the four arrows,
        // jumps now and then, and the onigiri between.
        let lane = |k: usize, every: usize, from: usize| {
            (0..120)
                .filter(|i| i % every == k)
                .map(|i| (from + i * 12).to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        let dos = format!(
            "|musicTitle=Test,Artist|musicUrl=test.ogg|blankFrame=0|difData=5,Hard,3.5\
             |left_data={}|down_data={}|up_data={}|right_data={}|space_data={}|",
            lane(0, 4, 60),
            lane(1, 4, 60),
            lane(2, 4, 60),
            lane(3, 4, 60),
            lane(0, 8, 66),
        );
        let mut song = danoni::import(&Dos::parse(&dos, DosFlags::default()))
            .unwrap()
            .songs
            .remove(0);
        song.add_chart_versions();
        let four: Vec<usize> = (0..song.charts.len())
            .filter(|&i| ddi_msd::chart::four_keys(&song.charts[i].layout))
            .collect();
        assert!(!four.is_empty(), "a generated four-lane version");
        assert!(four.iter().all(|&i| song.charts[i].derivation.is_some()));
        let own = rate_song(&song, &four, &[]);
        assert!(own.iter().all(|r| r.values.is_some_and(|v| v[0] > 0.0)), "{own:?}");
        // As the web's worker does it.
        let (text, map) = rating_simfile(&song, &four);
        let mut read = ddi_chart::formats::sm::parse_simfile(&text, "ssc").unwrap();
        read.add_chart_versions();
        let written: Vec<usize> = (0..map.len()).collect();
        let worker = placed(rate_song(&read, &written, &[]), &map);
        assert_eq!(worker, own);
    }

    #[test]
    fn a_record_reads_back_to_the_hundredth() {
        let v = [24.31, 22.0, 1.5, 0.0, 30.99, 18.25, 19.5, 21.03];
        let r = encode(0xabc, &v);
        assert_eq!(r.len(), RECORD);
        let (d, back) = decode(&r).unwrap();
        assert_eq!(d, 0xabc);
        for (a, b) in v.iter().zip(back) {
            assert!((a - b).abs() < 0.006, "{a} {b}");
        }
    }

    #[test]
    fn nothing_is_read_written_or_queued_with_both_options_off() {
        reset();
        let store = Rc::new(storage::MemoryStore::default());
        storage::install(store.clone());
        set_mode(Source::Song, false);
        offer([Job {
            song: "a".into(),
            charts: vec![0],
            ..Job::default()
        }]);
        assert_eq!(next(), None);
        assert_eq!(get(digest(1)), None);
        flush();
        assert!(storage::get(Area::Local, KEY).is_none());
        assert_eq!(status().pending, 0);
    }

    #[test]
    fn songs_wait_for_a_play_and_are_kept_once_rated() {
        reset();
        let store = Rc::new(storage::MemoryStore::default());
        storage::install(store.clone());
        set_mode(Source::Msd, false);
        offer([
            Job {
                song: "a".into(),
                charts: vec![0],
                ..Job::default()
            },
            Job {
                song: "b".into(),
                charts: vec![1],
                ..Job::default()
            },
        ]);
        first(&["b"]);
        set_playing(true);
        assert_eq!(next(), None, "paused during a play");
        assert!(status().paused);
        set_playing(false);
        let job = next().expect("b first");
        assert_eq!(job.song, "b");
        assert_eq!(next(), None, "one at a time");
        let g = generation();
        done(
            "b",
            vec![Rated {
                index: 1,
                digest: digest(7),
                values: Some([10.0; 8]),
                ..Rated::default()
            }],
        );
        assert!(generation() > g);
        assert_eq!(get(digest(7)), Some([10.0; 8]));
        // A song that could not be rated is not offered again.
        let a = next().expect("a");
        done("a", Vec::new());
        offer([a.clone()]);
        assert_eq!(status().pending, 0);
        // Kept in the store, and read back by another session.
        assert!(storage::get(Area::Local, KEY).is_some());
        reset();
        storage::install(store);
        set_mode(Source::Song, true);
        assert_eq!(get(digest(7)), Some([10.0; 8]));
        offer([a]);
        assert_eq!(status().pending, 1);
    }

    /// Round Z1: what song select says in the radar's place.
    #[test]
    fn a_chart_stands_rated_rating_or_unrated() {
        reset();
        storage::install(Rc::new(storage::MemoryStore::default()));
        set_mode(Source::Song, true);
        assert_eq!(standing("x", Some(digest(1)), "dance-double", true), Standing::NoLayout);
        assert_eq!(standing("a", Some(digest(1)), "dance-single", true), Standing::Unrated);
        offer([Job {
            song: "a".into(),
            charts: vec![0, 1],
            ..Job::default()
        }]);
        assert_eq!(standing("a", Some(digest(1)), "dance-single", true), Standing::Rating);
        let job = next().expect("a");
        assert_eq!(standing("a", Some(digest(1)), "dance-single", true), Standing::Rating);
        // Chart 0 rated, chart 1 not (the calculator skipped it).
        done(
            &job.song,
            vec![
                Rated {
                    index: 0,
                    digest: digest(1),
                    values: Some([9.0; 8]),
                    ..Rated::default()
                },
                Rated {
                    index: 1,
                    digest: digest(2),
                    values: None,
                    ..Rated::default()
                },
            ],
        );
        assert_eq!(standing("a", Some(digest(1)), "dance-single", true), Standing::Rated([9.0; 8]));
        assert_eq!(standing("a", Some(digest(2)), "dance-single", true), Standing::Unrated);
        assert_eq!(standing("a", None, "dance-single", true), Standing::Unrated);
    }

    fn estimate_of(decimal: f32, profile: Option<[f32; 8]>) -> ChartEstimate {
        ChartEstimate {
            decimal,
            level: decimal.round() as u8,
            scalar: true,
            studied: profile.is_some(),
            profile,
        }
    }

    /// Round Z2: estimates kept by digest under the estimator's, the
    /// scale's and the rule's names, read back to the hundredth, a chart
    /// without one kept as such, and forgotten when a name changes.
    #[test]
    fn estimates_are_kept_by_digest_and_forgotten_by_a_new_estimator() {
        let p = [1.25, 0.0, 0.0, 3.5, 12.29, 0.0, 0.01, 0.0];
        let kept = [
            (5u128, Some(estimate_of(14.27, Some(p)))),
            (6, Some(estimate_of(19.0, None))),
            (7, None),
        ];
        let text: String = kept.iter().map(|(d, e)| encode_estimate(*d, e.as_ref())).collect();
        assert_eq!(decode_estimates(&text), kept.to_vec());
        reset();
        let store = Rc::new(storage::MemoryStore::default());
        storage::install(store.clone());
        set_mode(Source::Estimate, false);
        offer([Job {
            song: "a".into(),
            estimates: vec![0, 1],
            ..Job::default()
        }]);
        let job = next().expect("a");
        assert_eq!(job.estimates, vec![0, 1]);
        done(
            "a",
            vec![
                Rated {
                    index: 0,
                    digest: digest(5),
                    estimated: true,
                    estimate: Some(estimate_of(14.27, Some(p))),
                    ..Rated::default()
                },
                Rated {
                    index: 1,
                    digest: digest(7),
                    estimated: true,
                    estimate: None,
                    ..Rated::default()
                },
            ],
        );
        assert_eq!(estimate(digest(7)), Some(None), "estimated, none made");
        // Read back by another session.
        reset();
        storage::install(store.clone());
        set_mode(Source::Estimate, true);
        assert_eq!(estimate(digest(5)), Some(Some(estimate_of(14.27, Some(p)))));
        assert_eq!(
            shown(Some(digest(5)), "dance-single", true),
            Some(Shown::Estimate(14, 14.27))
        );
        // To a tenth everywhere it shows (round AD).
        let e = Shown::Estimate(14, 14.27);
        assert_eq!((e.whole(), e.fine(), e.detail()), ("14.3".into(), "14.3".into(), "14.3".into()));
        let m = Shown::Msd(21.44);
        assert_eq!((m.whole(), m.fine()), ("21".into(), "21.4".into()));
        assert_eq!(shown(Some(digest(5)), "dance-single", false), None, "a version");
        assert_eq!(shown(Some(digest(5)), "dance-double", true), None);
        assert_eq!(standing("a", Some(digest(5)), "dance-single", true), Standing::Profile(p));
        assert_eq!(standing("a", Some(digest(7)), "dance-single", true), Standing::Unrated);
        assert_eq!(standing("a", Some(digest(5)), "dance-double", true), Standing::NoLayout);
        // MSD is not made under the estimate, nor read.
        assert!(!wants_msd());
        // Another estimator's: forgotten.
        let mut stored: StoredEstimates = storage::load(Area::Local, ESTIMATE_KEY).unwrap();
        stored.rule = "matched-level-profile-v-1".into();
        storage::save(Area::Local, ESTIMATE_KEY, &stored).unwrap();
        reset();
        storage::install(store);
        set_mode(Source::Estimate, false);
        assert_eq!(estimate(digest(5)), None);
    }

    /// Round Z2: the estimate's profile in the radar's place: shown (even
    /// where no skill stands out: round Z3's octagon), in words where the
    /// chart is outside what the estimate was studied on; a bundled one
    /// from its manifest; and the radar shows MSD's under the song's
    /// numbers.
    #[test]
    fn the_radar_s_source_follows_chart_ratings() {
        reset();
        storage::install(Rc::new(storage::MemoryStore::default()));
        set_mode(Source::Estimate, true);
        remember_estimate(digest(1), estimate_of(9.0, Some([0.1, 0.0, 0.0, 0.2, 0.0, 0.0, 0.0, 0.0])));
        let mut outside = estimate_of(18.7, None);
        outside.studied = false;
        remember_estimate(digest(2), outside);
        assert_eq!(
            standing("a", Some(digest(1)), "dance-single", true),
            Standing::Profile([0.1, 0.0, 0.0, 0.2, 0.0, 0.0, 0.0, 0.0])
        );
        assert_eq!(standing("a", Some(digest(2)), "dance-single", true), Standing::Outside);
        remember(digest(3), [10.0; 8]);
        assert_eq!(standing("a", Some(digest(3)), "dance-single", true), Standing::Unrated, "no MSD");
        set_mode(Source::Song, true);
        assert_eq!(standing("a", Some(digest(3)), "dance-single", true), Standing::Rated([10.0; 8]));
        assert_eq!(shown(Some(digest(3)), "dance-single", true), None, "the song's number");
    }

    /// A synthetic Single chart: `rows` repeated to `n` rows, 16 a measure,
    /// at `bpm`, as the coordinator's pattern study wrote them
    /// (`rating-anchor-patterns.rs`).
    fn pattern(bpm: u32, rows: &[&str], n: usize) -> ddi_chart::Song {
        let mut r: Vec<&str> = rows.iter().cycle().take(n).copied().collect();
        r.resize(n.next_multiple_of(16), "0000");
        let measures: Vec<String> = r.chunks(16).map(|c| c.join("\n")).collect();
        let text = format!("#BPMS:0={bpm};#NOTES:dance-single::Hard:1::{};", measures.join(",\n"));
        ddi_chart::formats::sm::parse_sm(&text).expect("parsed")
    }

    const RND: [&str; 12] = [
        "1000", "0010", "0100", "0001", "0100", "1000", "0001", "0010", "1000", "0001", "0100", "0010",
    ];
    const JS: [&str; 12] = [
        "1100", "0010", "0001", "0100", "1001", "0100", "0010", "1000", "0011", "1000", "0100", "0001",
    ];
    const JACK: [&str; 16] = [
        "1000", "1000", "1000", "1000", "0010", "0010", "0010", "0010", "0100", "0100", "0100", "0100",
        "0001", "0001", "0001", "0001",
    ];
    const HS: [&str; 8] = ["1110", "0001", "0111", "1000", "1011", "0100", "1101", "0010"];
    const SLOW: [&str; 8] = ["1000", "0000", "0100", "0000", "0010", "0000", "0001", "0000"];

    /// The profile of a pattern chart and its largest skill's place
    /// (`ddi_engine::cpu::SKILL_NAMES`).
    fn profile_of(song: &ddi_chart::Song) -> ([f32; 8], usize) {
        let e = estimate_chart(song, 0).expect("estimated");
        assert!(e.studied, "inside the studied domain: {e:?}");
        let p = e.profile.expect("a profile");
        let largest = (0..8).max_by(|&a, &b| p[a].total_cmp(&p[b])).unwrap();
        (p, largest)
    }

    fn share(p: &[f32; 8], k: usize) -> f32 {
        p[k] / p.iter().copied().fold(0.0, f32::max).max(1e-6)
    }

    /// Round Z2, `matched-level-profile-v0` on synthetic pattern charts:
    /// a 16th stream's largest spoke is Stream or Stamina, 16th jacks'
    /// JackSpeed, jumpstream's Jumpstream share and handstream's Handstream
    /// share above a stream's, a slow chart of quarter notes small.
    #[test]
    fn the_matched_level_profile_reads_patterns() {
        use ddi_engine::cpu::SKILL_NAMES;
        let k = |name: &str| SKILL_NAMES.iter().position(|n| *n == name).unwrap();
        let (stream, largest) = profile_of(&pattern(100, &RND, 400));
        assert!([k("Stream"), k("Stamina")].contains(&largest), "{stream:?}");
        for bpm in [90, 130] {
            let (p, largest) = profile_of(&pattern(bpm, &JACK, 480));
            assert_eq!(largest, k("JackSpeed"), "jacks at {bpm}: {p:?}");
        }
        let (stream150, _) = profile_of(&pattern(150, &RND, 600));
        let (js, _) = profile_of(&pattern(130, &JS, 560));
        assert!(
            share(&js, k("Jumpstream")) > share(&stream150, k("Jumpstream")) + 0.2,
            "{js:?} {stream150:?}"
        );
        let (hs, _) = profile_of(&pattern(130, &HS, 400));
        assert!(
            share(&hs, k("Handstream")) > share(&stream150, k("Handstream")) + 0.2,
            "{hs:?}"
        );
        let (slow, _) = profile_of(&pattern(120, &SLOW, 480));
        assert!(slow.iter().copied().fold(0.0, f32::max) < 2.5, "{slow:?}");
    }

    /// The matched level: a chart at the scale's top (19) reads level 10's
    /// row, at its bottom level 1's, between them the two rows about it.
    #[test]
    fn the_matched_level_runs_from_level_one_to_ten() {
        use ddi_library::manifest::matched_profile;
        let means: [[f64; 9]; 10] =
            std::array::from_fn(|i| std::array::from_fn(|j| if j == 0 { 90.0 } else { (i + 1) as f64 - 3.0 }));
        assert_eq!(matched_profile(19.0, &means), [7.0; 8]);
        assert_eq!(matched_profile(25.0, &means), [7.0; 8], "clamped");
        assert_eq!(matched_profile(1.0, &means), [0.0; 8], "level 1's, negatives as zero");
        // 9.5 is level 5: row 5's cost, 2.
        assert_eq!(matched_profile(9.5, &means), [2.0; 8]);
        // 10.45 is level 5.5: halfway between rows 5 and 6.
        let p = matched_profile(10.45, &means);
        assert!((p[0] - 2.5).abs() < 1e-9, "{p:?}");
    }

    /// Estimates wait for a play as MSD does.
    #[test]
    fn estimates_wait_for_a_play() {
        reset();
        storage::install(Rc::new(storage::MemoryStore::default()));
        set_mode(Source::Estimate, false);
        offer([Job {
            song: "a".into(),
            estimates: vec![0],
            ..Job::default()
        }]);
        set_playing(true);
        assert_eq!(next(), None);
        set_playing(false);
        assert_eq!(next().map(|j| j.estimates), Some(vec![0]));
    }

    /// Round Z3: a jacks pattern's radar under the estimate: its JackSpeed
    /// spoke the longest, every other at least the chart's base octagon.
    #[test]
    fn a_jack_pattern_bulges_at_jack_speed() {
        use ddi_engine::cpu::SKILL_NAMES;
        let song = pattern(130, &JACK, 480);
        let e = estimate_chart(&song, 0).expect("estimated");
        let spokes = crate::radar::estimate_spokes(e.decimal, &e.profile.expect("a profile"));
        let jack = SKILL_NAMES.iter().position(|n| *n == "JackSpeed").unwrap();
        let base = crate::radar::estimate_base(e.decimal);
        assert!(spokes.iter().all(|s| *s >= base - 1e-6), "{spokes:?}");
        assert!(spokes.iter().all(|s| *s <= spokes[jack]), "{spokes:?}");
    }

    /// Round Z4: a stream pattern's largest spoke is Stream or Stamina.
    #[test]
    fn a_stream_pattern_bulges_at_stream_or_stamina() {
        use ddi_engine::cpu::SKILL_NAMES;
        let song = pattern(150, &RND, 600);
        let e = estimate_chart(&song, 0).expect("estimated");
        let spokes = crate::radar::estimate_spokes(e.decimal, &e.profile.expect("a profile"));
        let largest = (0..8).max_by(|&a, &b| spokes[a].total_cmp(&spokes[b])).unwrap();
        assert!(["Stream", "Stamina"].contains(&SKILL_NAMES[largest]), "{spokes:?}");
    }
}
