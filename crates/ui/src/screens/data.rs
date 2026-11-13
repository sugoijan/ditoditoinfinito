//! What the theme's screens show, as plain data a shell fills (step 6
//! from the library, the engine's records and the settings; the
//! developer routes from [`super::sample`]).

use std::rc::Rc;

pub use ddi_chart::Difficulty;
use ddi_chart::Quantization;

/// A style's name as the screens show it (`common-style-*`): a layout of
/// ours by its message, a Dancing☆Onigiri mode by its keys, a generic
/// layout by its lanes, any other by its own name (`own`, data).
pub fn style_name(id: &str, own: &str) -> ddi_l10n::UiText {
    use ddi_l10n::msg::common as m;
    match id {
        "dance-single" => m::style_dance_single(),
        "dance-solo" => m::style_dance_solo(),
        "dance-double" => m::style_dance_double(),
        "techno-single8" => m::style_techno_single8(),
        "pump-single" => m::style_pump_single(),
        "pump-halfdouble" => m::style_pump_halfdouble(),
        "pump-double" => m::style_pump_double(),
        _ => {
            if let Some(mode) = id.strip_prefix("danoni-") {
                m::style_danoni(mode)
            } else if let Some(n) = id
                .strip_prefix("generic-")
                .and_then(|n| n.parse::<u32>().ok())
            {
                m::style_lanes(n)
            } else {
                ddi_l10n::UiText::data(own)
            }
        }
    }
}

/// Lane `i` of `layout`'s name as the screens show it (`common-lane-*`):
/// its arrow's direction or its panel; a Dancing☆Onigiri lane's place
/// before it ("2 · Down-left"); a glyph of a work's own by its name (data).
pub fn lane_name(layout: &ddi_chart::Layout, i: usize) -> ddi_l10n::UiText {
    use ddi_chart::Glyph;
    use ddi_l10n::msg::common as m;
    let Some(lane) = layout.lanes.get(i) else {
        return ddi_l10n::UiText::data("");
    };
    let name = match &lane.glyph {
        Glyph::Arrow { rotation_deg } => {
            match ((rotation_deg / 45.0).round() as i64).rem_euclid(8) {
                0 => m::lane_left(),
                1 => m::lane_up_left(),
                2 => m::lane_up(),
                3 => m::lane_up_right(),
                4 => m::lane_right(),
                5 => m::lane_down_right(),
                6 => m::lane_down(),
                _ => m::lane_down_left(),
            }
        }
        Glyph::Center => m::lane_center(),
        Glyph::Onigiri => m::lane_onigiri(),
        Glyph::Giko => m::lane_giko(),
        Glyph::Iyo => m::lane_iyo(),
        Glyph::Custom(name) => ddi_l10n::UiText::data(name),
    };
    if layout.id.starts_with("danoni-") {
        // Its place and its name, a label rather than a sentence.
        ddi_l10n::UiText::data(format!("{} · {name}", i + 1))
    } else if layout.pads() > 1 {
        m::lane_on_pad(&*name, u32::from(lane.pad) + 1)
    } else {
        name
    }
}

/// A style (a chart layout: `dance-single`, shown as "Single").
#[derive(Clone, Debug, PartialEq)]
pub struct StyleDef {
    pub id: String,
    pub name: String,
}

/// A chart of a song.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartEntry {
    /// Its style's id.
    pub style: String,
    pub difficulty: Difficulty,
    pub meter: u32,
    pub notes: u32,
    /// Its step artist.
    pub credit: String,
    /// Its place in the song (the play route's `chart=`).
    pub index: usize,
    /// Its name: an Edit's, a Dancing☆Onigiri chart's (they go by it).
    pub name: String,
    /// The player's best on it under the rules in effect, if any.
    pub best: Option<Best>,
    /// Its content's digest, when known (its kept plays).
    pub digest: Option<ddi_chart::ChartDigest>,
    /// What kind of version the game made it as, which the list marks
    /// ("Without center", "From Pump single"); `None` for an original.
    pub version: Option<ddi_chart::simplify::Kind>,
}

impl ChartEntry {
    /// Whether its version leaves notes out, its plays assisted (a
    /// conversion's are not).
    pub fn assisted(&self) -> bool {
        self.version
            .is_some_and(ddi_chart::simplify::Kind::assisted)
    }

    /// Named: an Edit (or a Dancing☆Onigiri chart) with a name, which it
    /// goes by.
    pub fn named(&self) -> bool {
        self.difficulty == Difficulty::Edit && !self.name.trim().is_empty()
    }

    /// What the ladder calls it: a named chart by its name, else its
    /// difficulty.
    pub fn label(&self) -> String {
        if self.named() {
            self.name.trim().to_string()
        } else {
            self.difficulty.name().to_string()
        }
    }

    /// Its meter as shown: none for a named chart of meter 0 (as the old
    /// song list had it).
    pub fn meter_text(&self) -> String {
        if self.named() && self.meter == 0 {
            String::new()
        } else {
            self.meter.to_string()
        }
    }
}

/// A best on a chart, as song select shows it (from the play history).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Best {
    /// The ruleset's grade ("AA", "★★★").
    pub grade: String,
    /// As the ruleset scores ("96.77%", "987650").
    pub score: String,
    /// Its lamp's name, as colors and commands name it (`Failed`,
    /// `Clear`, `FullCombo`, `GreatFullCombo`, `PerfectFullCombo`,
    /// `MarvelousFullCombo`), and how it is written short ("PFC").
    pub lamp: String,
    pub lamp_text: String,
    /// Plays under an older judge wait to be worked out again: the values
    /// are the old ones.
    pub pending: bool,
    pub plays: u32,
    /// The play the score is from (its replay).
    pub play: String,
    /// When the last of its plays was, milliseconds since the Unix epoch.
    pub last_played: f64,
}

/// A kept play of a chart, as song select's plays list shows it
/// (`docs/plans/play-history.md`, Replays).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayRow {
    /// The play's id (its replay, its results).
    pub id: String,
    /// When, in the player's time ("2026-10-16 14:03").
    pub when: String,
    pub grade: String,
    /// As the ruleset scores ("96.77%", "987650").
    pub score: String,
    /// The lamp's name and short text ([`Best::lamp`]).
    pub lamp: String,
    pub lamp_text: String,
    /// The rules it was judged by ("ITG"): those it was played by, or
    /// those it was judged again under (`rejudged`).
    pub ruleset: String,
    /// Judged again under rules other than those it was played by.
    pub rejudged: bool,
    /// The options as played ("×2 · reverse · mirror"); empty when not
    /// known (a play kept before summaries kept them).
    pub mods: String,
    pub autoplay: bool,
    /// The chart's best under the rules chosen now.
    pub best: bool,
    /// Waits to be worked out again by today's judge.
    pub pending: bool,
}

/// A pack of the song list and what can be done to it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackEntry {
    /// Its name; `None` for the bundled songs.
    pub name: Option<String>,
    /// Pack cover as a texture source, when supplied by the pack.
    pub banner: Option<String>,
    /// Imported: it can be removed.
    pub removable: bool,
    /// Imported from a folder the browser keeps: it can be read again.
    pub rereadable: bool,
}

/// A line song select shows over everything (an error, the storage
/// prompt, a controller without bindings), and what its link does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notice {
    pub text: String,
    /// The link's label and what it asks the shell
    /// ([`SelectCall::Notice`]).
    pub link: Option<(String, String)>,
    pub error: bool,
}

impl Notice {
    /// An error, without a link.
    pub fn error(text: impl Into<String>) -> Notice {
        Notice {
            text: text.into(),
            link: None,
            error: true,
        }
    }

    /// A quiet line, without a link.
    pub fn quiet(text: impl Into<String>) -> Notice {
        Notice {
            text: text.into(),
            link: None,
            error: false,
        }
    }
}

/// Where the game runs, for the screens' wording: the shell says (the
/// screens are the same; what keeping songs means is not).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Platform {
    /// A browser: songs imported into its storage (IndexedDB), removed
    /// for good; files dropped on the page.
    #[default]
    Web,
    /// The desktop game: song folders and zips read where they are,
    /// nothing copied; removing hides or forgets, never deletes.
    Desktop,
}

impl Platform {
    /// Its name as messages select on it (`$platform`: `web`, `desktop`;
    /// `docs/plans/localization.md`, "Message files").
    pub fn l10n(self) -> &'static str {
        match self {
            Platform::Web => "web",
            Platform::Desktop => "desktop",
        }
    }
}

/// The import dialog's state: the options, what runs, the last report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportView {
    /// The browser keeps a chosen folder's handle (Chromium): the copy
    /// option applies.
    pub folders: bool,
    /// Store a chosen folder's songs in the browser.
    pub copy: bool,
    /// Also store background videos.
    pub videos: bool,
    /// Details for pack authors.
    pub details: bool,
    /// The desktop build includes the DanOni URL importer.
    pub danoni_urls: bool,
    /// A running DanOni import can be cancelled.
    pub danoni_cancel: bool,
    /// Parameters and progress for a desktop URL import.
    pub danoni: DanoniImportView,
    /// What runs now ("reading files…").
    pub running: Option<ImportRunning>,
    /// The last import's report.
    pub report: Option<ImportReportView>,
}

/// What an import runs now, as the import dialog's status line says it.
#[derive(Clone, Debug, PartialEq)]
pub enum ImportRunning {
    /// An import of files at this step, worded by
    /// [`super::import_report`].
    Files(ddi_library::report::ImportStep),
    /// Worded by its importer (desktop's URL import), in messages of
    /// its own.
    Worded(String),
}

impl From<ddi_library::report::ImportStep> for ImportRunning {
    fn from(step: ddi_library::report::ImportStep) -> ImportRunning {
        ImportRunning::Files(step)
    }
}

impl ImportRunning {
    /// The status line.
    pub fn text(&self) -> String {
        match self {
            ImportRunning::Files(step) => super::import_report::step_text(step).into(),
            ImportRunning::Worded(text) => text.clone(),
        }
    }
}

/// The parameters and live state of a desktop DanOni URL import.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DanoniImportView {
    /// Last submitted URL, kept when Add songs closes.
    pub url: String,
    /// Use files already cached, without network access.
    pub offline: bool,
    /// The step under way, as the shell says it in the language now
    /// (“Converting music…”).
    pub stage: Option<String>,
    /// Completed and total items in the current stage, when known.
    pub progress: Option<(u64, u64)>,
    /// An additional address requires the user's decision.
    pub origin: Option<DanoniOriginRequest>,
}

/// A request to access one additional origin during an import.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DanoniOriginRequest {
    pub request_id: u64,
    /// The address asked for (data).
    pub origin: String,
}

/// An import's report, as the dialog shows it: a summary line ("3 songs
/// imported · 2 warnings"), whether nothing was imported, and titled
/// lists ("Not imported", …), each line "what: why".
#[derive(Clone, Debug, PartialEq)]
pub enum ImportReportView {
    /// An import of files: what it found, worded by
    /// [`super::import_report`]; the details for pack authors when asked
    /// for.
    Files {
        report: ddi_library::report::ImportReport,
        details: bool,
    },
    /// Worded by its importer (desktop's URL import), in messages of
    /// its own.
    Worded {
        summary: String,
        error: bool,
        sections: Vec<(String, Vec<String>)>,
    },
}

impl ImportReportView {
    /// The summary line (a line of tags, or why nothing was imported).
    pub fn summary(&self) -> String {
        match self {
            ImportReportView::Files { report, details } => {
                super::import_report::summary(report, *details)
            }
            ImportReportView::Worded { summary, .. } => summary.clone(),
        }
    }

    /// Nothing was imported.
    pub fn error(&self) -> bool {
        match self {
            ImportReportView::Files { report, .. } => super::import_report::failed(report),
            ImportReportView::Worded { error, .. } => *error,
        }
    }

    /// The titled lists, each line whole.
    pub fn sections(&self) -> Vec<(String, Vec<String>)> {
        match self {
            ImportReportView::Files { report, details } => {
                super::import_report::sections(report, *details)
                    .into_iter()
                    .map(|(title, lines)| {
                        (title.into(), lines.into_iter().map(String::from).collect())
                    })
                    .collect()
            }
            ImportReportView::Worded { sections, .. } => sections.clone(),
        }
    }
}

/// A song as song select shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SongEntry {
    /// Stable within the list (the library's id).
    pub id: String,
    /// With its subtitle, as the library's entries have it.
    pub title: String,
    pub artist: String,
    /// Its pack's name; `None` for a bundled song.
    pub pack: Option<String>,
    /// As shown ("160", "90–180").
    pub bpm_text: String,
    /// Seconds.
    pub length: f32,
    /// Texture sources ([`crate::Cx::texture`]).
    pub banner: Option<String>,
    pub background: Option<String>,
    /// Its jacket (square art) and CD title (a small mark), texture
    /// sources too.
    pub jacket: Option<String>,
    pub cd_title: Option<String>,
    /// Easiest first within a style.
    pub charts: Vec<ChartEntry>,
    /// Imported: it can be removed.
    pub removable: bool,
    /// Its offset (ms), when set.
    pub offset_ms: i32,
    /// The title and artist in Latin letters as the simfile spells them
    /// (`#TITLETRANSLIT`, `#ARTISTTRANSLIT`), searched too.
    pub title_translit: String,
    pub artist_translit: String,
    /// Its credit line (a bundled song's: the artist and the licence),
    /// shown and searched.
    pub credit: String,
}

impl SongEntry {
    /// Its charts in style `style`, easiest first.
    pub fn charts_in(&self, style: &str) -> Vec<&ChartEntry> {
        let mut v: Vec<&ChartEntry> = self.charts.iter().filter(|c| c.style == style).collect();
        v.sort_by_key(|c| (c.difficulty as usize, c.meter));
        v
    }

    /// The image behind the menus: its background, blurred (a step on
    /// the way into the song). A song without one shows the theme's
    /// plain backdrop, never its banner blown up (the maintainer's call).
    pub fn backdrop(&self) -> Option<&str> {
        self.background.as_deref()
    }

    /// "2:26"; "–" while unknown (a song imported before lengths were
    /// kept, until its chart is read again).
    pub fn length_text(&self) -> String {
        if self.length > 0.0 {
            clock(self.length)
        } else {
            "–".into()
        }
    }
}

/// A number with at most `decimals` decimals, for showing ("2.5", "3";
/// never "-0").
pub fn trim_number(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// An amount of data as the screens write it (`common-size-*`): B, kB,
/// MB or GB in steps of 1000, the largest unit it makes one of (counted
/// after rounding: 999,960 bytes are "1.0 MB"), to a tenth below 10 of
/// it ("4.2 MB"), else whole ("37 MB"); bytes always whole.
pub fn size_text(bytes: f64) -> ddi_l10n::UiText {
    use ddi_l10n::msg::common as m;
    let bytes = bytes.max(0.0);
    let mut unit = 0;
    let mut amount = bytes.round();
    while unit < 3 && amount >= 1000.0 {
        unit += 1;
        let v = bytes / 1000f64.powi(unit);
        amount = if (v * 10.0).round() < 100.0 {
            (v * 10.0).round() / 10.0
        } else {
            v.round()
        };
    }
    let tenths = if unit > 0 && amount < 10.0 {
        "yes"
    } else {
        "no"
    };
    match unit {
        0 => m::size_b(amount),
        1 => m::size_kb(amount, tenths),
        2 => m::size_mb(amount, tenths),
        _ => m::size_gb(amount, tenths),
    }
}

/// Seconds as "m:ss".
pub fn clock(seconds: f32) -> String {
    let s = seconds.max(0.0).round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The chart of `charts` (easiest first) closest to `preferred`: the
/// same difficulty, else the nearest, the easier on a tie (StepMania's
/// `ScreenSelectMusic::SwitchToPreferredDifficulty`, which keeps the
/// first of equal differences in its easiest-first list).
pub fn closest(charts: &[&ChartEntry], preferred: Difficulty) -> Option<usize> {
    let want = preferred as i32;
    let mut best: Option<(usize, i32)> = None;
    for (i, c) in charts.iter().enumerate() {
        let d = (c.difficulty as i32 - want).abs();
        if best.is_none_or(|(_, b)| d < b) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

/// Song select's data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectData {
    /// Every style the library has charts in, in the order the tabs show.
    pub styles: Vec<StyleDef>,
    /// In list order, grouped by pack.
    pub songs: Vec<SongEntry>,
    /// What is in effect, for the header ("×2.5", "Reverse", "ITG").
    pub mods: Vec<String>,
    /// The difficulty picked at first (the player's last; Hard when
    /// none).
    pub preferred: Option<Difficulty>,
    /// The packs, in list order, with what can be done to them (packs
    /// named by songs and missing here can do nothing).
    pub packs: Vec<PackEntry>,
    /// Packs closed at first, by name (`None`: the bundled songs).
    pub closed: Vec<Option<String>>,
    /// Saved browsing song and chart; restored once, including after a library scan.
    pub resume: Option<(String, Option<usize>)>,
    /// Saved open pack, including when its header was selected: its key
    /// ([`crate::state::place::pack_key`]).
    pub resume_pack: Option<String>,
    /// More songs may arrive from the initial library scan.
    pub loading: bool,
    /// The style shown at first (its id), and the search.
    pub style: Option<String>,
    pub search: String,
    /// Lines over everything.
    pub notices: Vec<Notice>,
    pub import: ImportView,
    /// Plays started here are autoplayed (for the session; not counted).
    pub autoplay: bool,
    /// Songs shared by link (`docs/plans/shared-songs.md`): the list is
    /// the share's songs alone, nothing is added, edited or managed.
    pub shared: Option<SharedView>,
}

/// How sharing a pack goes (`docs/plans/shared-songs.md`): the shell's
/// upload as the Share dialog shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShareView {
    pub state: ShareState,
    /// Songs and bytes uploaded, of all.
    pub done_songs: usize,
    pub songs: usize,
    pub done_bytes: f64,
    pub bytes: f64,
    /// The link, once the share is readable.
    pub link: String,
    /// Why it failed (worded by the dialog, in the player's language).
    pub error: Option<ddi_library::share::ShareError>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShareState {
    #[default]
    Idle,
    Uploading,
    Done,
    Failed,
}

/// The shares a token may see, as Options' Shares dialog shows them
/// (`docs/plans/shared-songs.md`): read by the shell when the dialog
/// opens, again after a deletion.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SharesView {
    pub state: SharesState,
    /// The token sees every share (an admin's).
    pub admin: bool,
    /// Each share with the link to it.
    pub shares: Vec<ShareRow>,
    /// Why they could not be read.
    pub error: Option<ddi_library::share::ShareError>,
}

impl SharesView {
    /// The shares could not be read, for `error`.
    pub fn failed(error: ddi_library::share::ShareError) -> SharesView {
        SharesView {
            state: SharesState::Failed,
            error: Some(error),
            ..SharesView::default()
        }
    }

    /// The server's `listing` read, each share with its link (`link`).
    pub fn from_listing(
        listing: ddi_library::share::ShareListing,
        link: impl Fn(&str) -> String,
    ) -> SharesView {
        SharesView {
            state: SharesState::Ready,
            admin: listing.role == "admin",
            shares: listing
                .shares
                .into_iter()
                .map(|summary| ShareRow {
                    link: link(&summary.id),
                    summary,
                })
                .collect(),
            error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SharesState {
    #[default]
    Loading,
    Ready,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShareRow {
    pub summary: ddi_library::share::ShareSummary,
    pub link: String,
}

/// What song select says of a share it shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SharedView {
    /// The pack's name, as the one who shared it named it.
    pub pack: String,
    /// "expires in 3 days"; `None`: it never does.
    pub expires: Option<String>,
}

impl SharedView {
    /// The share of `pack`, expiring at `expires_at` (`None`: never),
    /// at `now` (both Unix seconds).
    pub fn new(pack: String, expires_at: Option<f64>, now: f64) -> SharedView {
        let expires = expires_at.map(|at| super::options_dialogs::expires_in(Some(at), now).into());
        SharedView { pack, expires }
    }
}

/// A judgement row of results.
#[derive(Clone, Debug, PartialEq)]
pub struct JudgementCount {
    /// As the ruleset names it ("Fantastic").
    pub name: String,
    /// Its color's name (`Judgement.W1` … `Judgement.Miss`).
    pub color: String,
    pub count: u32,
    /// DDI's rules: how many of them were early and late (the name then
    /// says both words, "Awesome / Amazing").
    pub sides: Option<[u32; 2]>,
}

/// One hit of a play: when (seconds into the song) and how far off
/// (milliseconds, negative early); `None` for a miss.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub time: f32,
    pub offset: Option<f32>,
    pub lane: u8,
    /// Its note's place in the beat (a 4th on the beat, an 8th between);
    /// `None` where a play kept from before it was kept does not say.
    pub beat: Option<Quantization>,
}

/// The hits of one place in the beat (or of several rare ones, "Other"),
/// as the by-beat panel shows them.
#[derive(Clone, Debug, PartialEq)]
pub struct BeatGroup {
    /// "4th", "16th", "Other".
    pub name: String,
    /// Its notes: hit and missed.
    pub count: u32,
    pub misses: u32,
    /// The hits' mean offset and its standard deviation, ms.
    pub mean: f32,
    pub sd: f32,
}

/// A place in the beat as the results name it ("4th", "32nd").
pub fn beat_name(q: Quantization) -> String {
    let n: u32 = match q {
        Quantization::N4 => 4,
        Quantization::N8 => 8,
        Quantization::N12 => 12,
        Quantization::N16 => 16,
        Quantization::N24 => 24,
        Quantization::N32 => 32,
        Quantization::N48 => 48,
        Quantization::N64 => 64,
        Quantization::N192 => 192,
    };
    ddi_l10n::msg::results::beat(n).into()
}

/// The places in the beat in rhythmic order.
const BEATS: [Quantization; 9] = [
    Quantization::N4,
    Quantization::N8,
    Quantization::N12,
    Quantization::N16,
    Quantization::N24,
    Quantization::N32,
    Quantization::N48,
    Quantization::N64,
    Quantization::N192,
];

/// The mean and standard deviation of `v` (0 and 0 for none).
fn mean_sd(v: &[f32]) -> (f32, f32) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let n = v.len() as f32;
    let mean = v.iter().sum::<f32>() / n;
    let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n;
    (mean, var.sqrt())
}

/// A judgement window of the histogram: how early and how late (ms,
/// both positive) and its color's name, best first.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub early: f32,
    pub late: f32,
    pub color: String,
}

/// A lane as the by-lane panel names it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaneName {
    /// One character ("←", "■").
    pub symbol: String,
    /// "Left", "Up-left".
    pub label: String,
}

/// A results panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelKind {
    /// The timing histogram.
    Timing,
    /// Life over the song.
    Life,
    /// Each hit's offset over the song, misses marked.
    Offsets,
    /// Hits short of the best tier, by lane.
    Lanes,
    /// The hits' offsets by their notes' places in the beat.
    Beats,
}

/// A set of panels the player picks ("Overview": all four).
#[derive(Clone, Debug, PartialEq)]
pub struct PanelSet {
    pub name: String,
    pub panels: Vec<PanelKind>,
}

/// The per-song offset editor: the offset the play ran with, the one set
/// now, and this play's mean error as a new offset when it is worth
/// offering (seconds).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SongOffset {
    pub played: f64,
    pub current: f64,
    pub suggested: Option<f64>,
}

/// Results' data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResultsData {
    pub song: SongEntry,
    /// The chart and the rules ("Hard 13 · Single · ITG rules").
    pub line: String,
    /// What was played with ("×2.5 · reverse · shuffle ↓←→↑").
    pub mods: String,
    pub grade: String,
    /// As the ruleset scores: money ("987650"), percent ("96.77%") or
    /// "—".
    pub score: String,
    /// "EX 1234 / 1400" where the ruleset has EX.
    pub ex: Option<String>,
    pub failed: bool,
    /// The player quit: QUIT where the grade goes, no lamp ([`ddi_engine::player::Results::quit`]).
    pub quit: bool,
    /// "FULL COMBO", "PERFECT FULL COMBO · ASSIST", "ASSIST".
    pub lamp: Option<String>,
    /// The ruleset's tiers that have names, in order.
    pub judgements: Vec<JudgementCount>,
    /// (label, value) pairs under the table ("O.K. / N.G.", "13 / 1").
    pub stats: Vec<(String, String)>,
    /// Lines worth saying ("an audio device or the display changed
    /// during play").
    pub notes: Vec<String>,
    pub hits: Vec<Hit>,
    /// The ruleset's windows the histogram colors by.
    pub windows: Vec<Window>,
    /// Life over the song: (seconds, 0..=1).
    pub life: Vec<(f32, f32)>,
    /// The gauge's danger line, where it has one.
    pub danger: Option<f32>,
    pub lanes: Vec<LaneName>,
    /// The editor; `None`: none (not a song).
    pub offset: Option<SongOffset>,
    /// The panel sets offered, and the one shown.
    pub panel_sets: Vec<PanelSet>,
    pub panel_set: usize,
    /// A play just played, a replay just watched, or a kept play opened.
    pub kind: ResultsKind,
}

/// What a results screen is of (`docs/plans/play-history.md`, Replays).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResultsKind {
    /// A play just played: Retry, and its replay once it is kept.
    #[default]
    Play,
    /// A replay just watched: "Watch again".
    Replay,
    /// A kept play opened from its list: its replay.
    Kept,
    /// A multiplayer stage's play (`docs/plans/multiplayer-prototype-progress.md`,
    /// M3): no Retry (a stage is played once), "Back to room" for Song
    /// list, not kept.
    Stage,
}

impl ResultsData {
    /// The misses' color's name in the ruleset's set: its Miss row's
    /// (`Judgement.Ddi.Miss` under DDI), else `Judgement.Miss`.
    pub fn miss_color(&self) -> &str {
        self.judgements
            .iter()
            .map(|j| j.color.as_str())
            .find(|c| c.ends_with(".Miss"))
            .unwrap_or("Judgement.Miss")
    }

    /// The panels of the set shown.
    pub fn panels(&self) -> &[PanelKind] {
        self.panel_sets
            .get(self.panel_set)
            .map_or(&[], |s| s.panels.as_slice())
    }

    /// The mean and standard deviation of the hits' offsets, ms.
    pub fn timing(&self) -> (f32, f32) {
        let v: Vec<f32> = self.hits.iter().filter_map(|h| h.offset).collect();
        mean_sd(&v)
    }

    /// The hits by their notes' places in the beat, in rhythmic order, at
    /// most `rows`: a place with fewer than `fewest` notes, and past the
    /// room the least played, go together into "Other" (last; named as
    /// the place when it is only one), so every note whose place is known
    /// is counted.
    pub fn beat_groups(&self, rows: usize, fewest: u32) -> Vec<BeatGroup> {
        let notes = |q: Quantization| self.hits.iter().filter(move |h| h.beat == Some(q));
        let counts: Vec<(Quantization, u32)> = BEATS
            .iter()
            .map(|&q| (q, notes(q).count() as u32))
            .filter(|&(_, n)| n > 0)
            .collect();
        let mut kept: Vec<(Quantization, u32)> = counts
            .iter()
            .copied()
            .filter(|&(_, n)| n >= fewest)
            .collect();
        // Room for the kept and, when any is merged, Other: the least
        // played go first (of two as played, the finer).
        while !kept.is_empty() && kept.len() + usize::from(kept.len() < counts.len()) > rows.max(1)
        {
            let least = (0..kept.len())
                .min_by_key(|&i| (kept[i].1, std::cmp::Reverse(i)))
                .unwrap();
            kept.remove(least);
        }
        let kept: Vec<Quantization> = kept.into_iter().map(|(q, _)| q).collect();
        let group = |name: &str, qs: &[Quantization]| {
            let hits: Vec<&Hit> = self
                .hits
                .iter()
                .filter(|h| h.beat.is_some_and(|b| qs.contains(&b)))
                .collect();
            let offsets: Vec<f32> = hits.iter().filter_map(|h| h.offset).collect();
            let (mean, sd) = mean_sd(&offsets);
            BeatGroup {
                name: name.to_string(),
                count: hits.len() as u32,
                misses: (hits.len() - offsets.len()) as u32,
                mean,
                sd,
            }
        };
        let mut out: Vec<BeatGroup> = kept.iter().map(|&q| group(&beat_name(q), &[q])).collect();
        let rest: Vec<Quantization> = counts
            .iter()
            .map(|c| c.0)
            .filter(|q| !kept.contains(q))
            .collect();
        match rest.as_slice() {
            [] => {}
            [one] => out.push(group(&beat_name(*one), &[*one])),
            _ => out.push(group(&ddi_l10n::msg::results::beat_other(), &rest)),
        }
        out
    }

    /// The largest fall of life within `window` seconds: how much (a
    /// fraction of the gauge), from when, to when; `None` when it never
    /// fell. Life holds between its points.
    pub fn steepest_drop(&self, window: f32) -> Option<(f32, f32, f32)> {
        let life = &self.life;
        let mut best: Option<(f32, f32, f32)> = None;
        for (i, &(t, v)) in life.iter().enumerate() {
            // The highest the gauge stood in the window before it: its
            // points in it, and the value held from before it.
            let mut high: Option<(f32, f32)> = None;
            for &(pt, pv) in life[..i].iter().rev() {
                let at = pt.max(t - window);
                if high.is_none_or(|(_, hv)| pv > hv) {
                    high = Some((at, pv));
                }
                if pt <= t - window {
                    break;
                }
            }
            if let Some((from, hv)) = high
                && hv - v > best.map_or(0.0, |b| b.0)
            {
                best = Some((hv - v, from, t));
            }
        }
        best
    }
}

/// An option's values.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionKind {
    /// Named choices, one chosen.
    Choices(Vec<String>),
    /// A number stepped through `values` (shown as given; shared, since
    /// only the chosen one shows and long lists are formatted once).
    Stepper(Rc<[String]>),
    /// The same as a slider: a track the pointer drags or taps along,
    /// keys and pads stepping it (the volume).
    Slider(Rc<[String]>),
    /// Buttons (none: a row that only says something, its `note`): Left
    /// and Right move between them (`index`), Start or a tap presses one.
    Actions(Vec<String>),
}

/// An option row.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionDef {
    /// What the shell knows it by (`speed`, `keys.lane.2`); unique on the
    /// screen.
    pub id: String,
    pub label: String,
    pub kind: OptionKind,
    pub index: usize,
    /// A note beside a stepper ("400 at this song").
    pub note: String,
    /// What it does, in a sentence.
    pub description: String,
    /// A heading above it ("SCROLL").
    pub heading: Option<String>,
}

impl OptionDef {
    pub fn values(&self) -> &[String] {
        match &self.kind {
            OptionKind::Choices(v) | OptionKind::Actions(v) => v,
            OptionKind::Stepper(v) | OptionKind::Slider(v) => v,
        }
    }

    pub fn value(&self) -> &str {
        self.values().get(self.index).map_or("", String::as_str)
    }

    /// The index `by` values from this one, going round with `wrap`, else
    /// stopping at the ends; a row without values stays.
    pub fn stepped(&self, by: i32, wrap: bool) -> usize {
        let n = self.values().len() as i32;
        if n == 0 {
            return self.index;
        }
        let to = self.index as i32 + by;
        (if wrap {
            to.rem_euclid(n)
        } else {
            to.clamp(0, n - 1)
        }) as usize
    }
}

/// A section of options.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionSection {
    /// What the shell and the tests know it by, whatever the language
    /// (its English name: `Display`).
    pub id: String,
    pub name: String,
    pub options: Vec<OptionDef>,
}

/// Options' data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptionsData {
    pub sections: Vec<OptionSection>,
    /// The layout selector above the preview, shared by every section.
    pub preview_layout: Option<OptionDef>,
    /// Show the whole play composition at the current viewport aspect.
    pub preview_whole_play: bool,
    /// Under the field preview ("Infinito · Single · ×2.5 · Reverse").
    pub preview: String,
    /// What the last action did ("Settings reset.", "3 plays
    /// exported."), shown in the header.
    pub notice: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulties_in_order_by_name() {
        for (i, d) in Difficulty::ALL.into_iter().enumerate() {
            assert_eq!(d as usize, i);
            assert_eq!(d.name(), format!("{d:?}"));
            assert_eq!(Difficulty::from_name(d.name()), Some(d));
        }
        assert!(Difficulty::ALL.is_sorted());
        assert_eq!(Difficulty::from_name("hard"), None);
    }

    #[test]
    fn sizes_are_written_in_steps_of_1000_to_a_tenth_below_10() {
        ddi_l10n::set_language("en");
        let cases = [
            (0.0, "0 B"),
            (999.0, "999 B"),
            (999.6, "1.0 kB"),
            (4_200.0, "4.2 kB"),
            (4_000_000.0, "4.0 MB"),
            (9_960_000.0, "10 MB"),
            (37_400_000.0, "37 MB"),
            (999_600_000.0, "1.0 GB"),
            (1_300_000_000.0, "1.3 GB"),
            (12_000_000_000.0, "12 GB"),
            (2_500_000_000_000.0, "2,500 GB"),
        ];
        for (bytes, text) in cases {
            assert_eq!(&*size_text(bytes), text, "{bytes}");
        }
    }

    fn chart(d: Difficulty) -> ChartEntry {
        ChartEntry {
            style: "s".into(),
            difficulty: d,
            meter: 1,
            notes: 1,
            ..Default::default()
        }
    }

    fn hits(beats: &[(Quantization, usize)]) -> Vec<Hit> {
        beats
            .iter()
            .flat_map(|&(q, n)| {
                (0..n).map(move |i| Hit {
                    time: i as f32,
                    offset: (i > 0).then_some(if q == Quantization::N16 { -10.0 } else { 2.0 }),
                    lane: 0,
                    beat: Some(q),
                })
            })
            .collect()
    }

    #[test]
    fn beats_go_in_rhythmic_order_the_rare_and_the_least_played_together() {
        use Quantization::*;
        let d = ResultsData {
            hits: hits(&[(N16, 40), (N4, 100), (N8, 60), (N12, 3), (N24, 2)]),
            ..Default::default()
        };
        let names = |g: &[BeatGroup]| {
            g.iter()
                .map(|g| (g.name.clone(), g.count))
                .collect::<Vec<_>>()
        };
        let g = d.beat_groups(7, 5);
        assert_eq!(
            names(&g),
            [
                ("4th".into(), 100),
                ("8th".into(), 60),
                ("16th".into(), 40),
                ("Other".into(), 5)
            ]
        );
        // Each row's first note missed; 16ths 10 ms early.
        assert_eq!((g[2].misses, g[2].mean, g[2].sd), (1, -10.0, 0.0));
        // Room for three: the 16ths join Other.
        assert_eq!(
            names(&d.beat_groups(3, 5)),
            [
                ("4th".into(), 100),
                ("8th".into(), 60),
                ("Other".into(), 45)
            ]
        );
        // One rare place keeps its name.
        let d = ResultsData {
            hits: hits(&[(N4, 10), (N12, 2)]),
            ..Default::default()
        };
        assert_eq!(
            names(&d.beat_groups(7, 5)),
            [("4th".into(), 10), ("12th".into(), 2)]
        );
        // A play kept before the places were: none.
        let mut d = d;
        for h in &mut d.hits {
            h.beat = None;
        }
        assert!(d.beat_groups(7, 5).is_empty());
    }

    #[test]
    fn the_largest_drop_is_the_largest_fall_within_the_window() {
        let d = |life: &[(f32, f32)]| ResultsData {
            life: life.to_vec(),
            ..Default::default()
        };
        // A slow fall of 40% over 20 s, then 20% in a second.
        let slow: Vec<(f32, f32)> = (0..=20)
            .map(|t| (t as f32, 1.0 - 0.02 * t as f32))
            .collect();
        let mut life = slow.clone();
        life.extend([(30.0, 0.6), (31.0, 0.4), (40.0, 0.5)]);
        let (drop, from, to) = d(&life).steepest_drop(4.0).unwrap();
        assert!(
            (drop - 0.2).abs() < 1e-5 && from == 30.0 && to == 31.0,
            "{drop} {from} {to}"
        );
        // The slow fall alone: 4 s of it, from the window's start.
        let (drop, from, to) = d(&slow).steepest_drop(4.0).unwrap();
        assert!(
            (drop - 0.08).abs() < 1e-5 && to - from == 4.0,
            "{drop} {from} {to}"
        );
        // A value held from before the window counts at the window's start.
        let (drop, from, _) = d(&[(0.0, 1.0), (10.0, 0.7)]).steepest_drop(4.0).unwrap();
        assert!((drop - 0.3).abs() < 1e-5 && from == 6.0, "{drop} {from}");
        // Never falling: none.
        assert_eq!(d(&[(0.0, 0.5), (5.0, 0.8)]).steepest_drop(4.0), None);
    }

    #[test]
    fn the_closest_chart_prefers_the_easier_on_a_tie() {
        let m = chart(Difficulty::Medium);
        let c = chart(Difficulty::Challenge);
        let b = chart(Difficulty::Beginner);
        assert_eq!(closest(&[&m, &c], Difficulty::Hard), Some(0));
        assert_eq!(closest(&[&b, &c], Difficulty::Hard), Some(1));
        assert_eq!(closest(&[&b], Difficulty::Edit), Some(0));
        assert_eq!(closest(&[], Difficulty::Hard), None);
        assert_eq!(clock(146.4), "2:26");
        assert_eq!(clock(108.0), "1:48");
    }
}
