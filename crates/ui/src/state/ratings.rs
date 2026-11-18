//! Chart ratings of the game's own (`docs/plans/msd-ratings.md`): for now
//! MSD, Etterna's chart difficulty (`ddi_msd`), shown in place of the
//! simfiles' numbers and as a skill radar, each behind its option
//! ([`Settings::chart_ratings`](crate::state::settings::Settings::chart_ratings),
//! `skill_radar`), both Off by default.
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
use ddi_platform::storage::{self, Area};
use serde::{Deserialize, Serialize};

/// Where the ratings are kept.
pub const KEY: &str = "ddi.msd.v1";

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

/// A song to rate: its library id and the charts of it (their places in
/// the song) that want numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub song: String,
    pub charts: Vec<usize>,
}

/// What a shell's rating of a [`Job`] found: each chart's digest and its
/// numbers (`None` where it could not be rated: not four lanes after all,
/// or a file the calculator skips).
#[derive(Clone, Debug, PartialEq)]
pub struct Rated {
    pub index: usize,
    pub digest: ChartDigest,
    pub values: Option<Values>,
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
}

#[derive(Default)]
struct State {
    msd: bool,
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
    fn wanted(&self) -> bool {
        self.msd || self.radar
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

/// What the settings show: MSD in the numbers' place, the radar.
pub fn set_mode(msd: bool, radar: bool) {
    with(|s| {
        s.msd = msd;
        s.radar = radar;
        s.ever |= msd;
        if !s.wanted() {
            s.pending.clear();
        }
    });
}

/// Whether a rated chart shows its MSD in its number's place.
pub fn msd_shown() -> bool {
    with(|s| s.msd)
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
    with(|s| if s.wanted() { s.get(digest) } else { None })
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
            if !job.charts.is_empty() {
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
pub fn want_first(song: &str, index: usize, digest: Option<ChartDigest>, layout: &str) {
    if !wanted() || !ddi_msd::chart::four_keys(layout) || digest.is_some_and(|d| get(d).is_some()) {
        return;
    }
    with(|s| {
        if s.failed.contains(song) || s.rating.as_deref() == Some(song) {
            return;
        }
        if let Some(at) = s.pending.iter().position(|j| j.song == song) {
            let mut job = s.pending.remove(at).expect("there");
            if !job.charts.contains(&index) {
                job.charts.push(index);
            }
            s.pending.push_front(job);
        } else {
            s.pending.push_front(Job {
                song: song.to_string(),
                charts: vec![index],
            });
        }
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
        let mut any = false;
        for r in rated {
            if let Some(v) = r.values {
                s.kept.insert(r.digest.0, v.map(|x| (x * 100.0).round() / 100.0));
                s.dirty = true;
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
        }
    });
}

/// Writes what is not kept yet.
pub fn flush() {
    with(State::save);
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
    })
}

/// The MSD shown in the number's place of `song`'s chart `index`: where
/// Chart ratings shows MSD and the chart has one known.
pub fn shown_msd(song: &ddi_chart::Song, index: usize) -> Option<f32> {
    if !msd_shown() {
        return None;
    }
    let chart = song.charts.get(index)?;
    if !ddi_msd::chart::four_keys(&chart.layout) {
        return None;
    }
    get(song.chart_digest(index)).map(|v| v[0])
}

/// The eight numbers of `chart` of `song`, rated now, kept and returned
/// (a room's chart, read for its stage before the play). `None` where it
/// has none, or ratings are not wanted. Known ones are not rated again.
pub fn rate_now(song: &ddi_chart::Song, index: usize) -> Option<Values> {
    if !wanted() {
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

/// Rates every chart of a song that wants it (`job`): what a shell's
/// worker or thread runs, from the song's parsed simfile.
pub fn rate_song(song: &ddi_chart::Song, job_charts: &[usize]) -> Vec<Rated> {
    job_charts
        .iter()
        .filter_map(|&index| {
            let chart = song.charts.get(index)?;
            Some(Rated {
                index,
                digest: song.chart_digest(index),
                values: ddi_msd::chart::chart_msd(song, chart),
            })
        })
        .collect()
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
            Some(Rated {
                index,
                digest,
                values: r.values,
            })
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
        let own = rate_song(&song, &four);
        assert!(own.iter().all(|r| r.values.is_some_and(|v| v[0] > 0.0)), "{own:?}");
        // As the web's worker does it.
        let (text, map) = rating_simfile(&song, &four);
        let mut read = ddi_chart::formats::sm::parse_simfile(&text, "ssc").unwrap();
        read.add_chart_versions();
        let written: Vec<usize> = (0..map.len()).collect();
        let worker = placed(rate_song(&read, &written), &map);
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
        set_mode(false, false);
        offer([Job {
            song: "a".into(),
            charts: vec![0],
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
        set_mode(true, false);
        offer([
            Job {
                song: "a".into(),
                charts: vec![0],
            },
            Job {
                song: "b".into(),
                charts: vec![1],
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
        set_mode(false, true);
        assert_eq!(get(digest(7)), Some([10.0; 8]));
        offer([a]);
        assert_eq!(status().pending, 1);
    }
}
