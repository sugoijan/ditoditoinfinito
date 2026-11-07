//! What an import reports, as values (`docs/plans/localization.md`,
//! step 4): why a song, a skin or an announcer was left out, what is
//! wrong with what was imported, what only a pack author would want to
//! know, and what runs meanwhile. Each is made where it is found (the
//! library's scan, a work's or a level's reading, each shell's import)
//! with its data (file and folder names exactly as found, counts), and
//! worded where it is shown, in the player's language, by
//! `ddi_ui::screens::import_report`; nothing here is a sentence.

use ddi_platform::video::VideoFormat;
use serde::{Deserialize, Serialize};

use crate::pack::MissingTag;
use crate::swf::SwfError;
use crate::zip::ZipError;

/// One thing an import found: why something was not imported, what an
/// imported song lacks, or a detail for pack authors. A report's line is
/// `(what, finding)`, `what` the file or folder it is about (data); the
/// findings marked "whole line" say what they are about themselves, and
/// come with an empty `what`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Finding {
    // What failed outside the game, and other parts' own words.
    /// Something outside the game failed (a file's read, the browser's
    /// storage): the system's words.
    System(String),
    /// Another part of the game said why in its own words (a chart's
    /// parser, a work's reader, the sprites' importer): shown as written.
    Other(String),
    /// A file of the import could not be read: the system's words.
    FileFailed { file: String, error: String },
    /// A file the import listed is not there any more.
    FileMissing { file: String },
    /// A zip entry ends past its archive's end.
    Truncated { file: String },
    /// A zip, or an entry in one, could not be read. Never kept (only
    /// the web's import, which keeps nothing of its report, makes it).
    #[serde(skip)]
    Zip(ZipError),
    /// An encrypted zip entry, not read.
    Encrypted,

    // Simfiles.
    /// The simfile has no charts.
    NoCharts,
    /// No chart in a layout the game plays; the layouts found (as the
    /// simfile names them).
    NoPlayableCharts { layouts: Vec<String> },
    /// No audio file beside the simfile.
    NoMusic,
    /// A song folder of the same pack and name came first (where).
    SameFolderAs { first: String },
    /// A folder with music (its file) but no simfile.
    MusicWithoutSimfile { music: String },
    /// Charts in layouts the game does not play, left out (a detail).
    LayoutsLeftOut { layouts: Vec<String> },
    /// `tag` names `file`, which is missing (a detail).
    Missing { tag: MissingTag, file: String },
    /// The same, the music's sample playing instead (a detail).
    MissingPreview { tag: MissingTag, file: String },
    /// The same, the song's music playing instead (a detail).
    MissingChartMusic { tag: MissingTag, file: String },
    /// The same, with nothing in its place: the song has no banner or
    /// background (which, `tag` says).
    MissingWithout { tag: MissingTag, file: String },
    /// The same, `used` (a file name) standing in (a detail).
    MissingReplaced {
        tag: MissingTag,
        file: String,
        used: String,
    },
    /// More background-change images than are kept.
    TooManyBackgroundImages { count: usize, kept: usize },
    /// The preview movie was not stored (background videos are not
    /// stored).
    PreviewVideoNotStored { file: String },
    /// The preview movie is in a format the game cannot play.
    PreviewVideoUnplayable { file: String },
    /// Background movies the game cannot play: each with its format.
    VideosUnplayable { videos: Vec<(String, VideoFormat)> },
    /// Background movies not stored (background videos are not stored).
    VideosNotStored { files: Vec<String> },
    /// More files in scripted backgrounds than are kept.
    TooManyScriptFiles { count: usize, kept: usize },
    /// Background changes use shared folders' animations, not played.
    SharedAnimations { names: Vec<String> },
    /// Background changes name scripted animations that are missing.
    MissingAnimations { names: Vec<String> },
    /// Background changes name images or movies that were not found.
    MissingBackgrounds { names: Vec<String> },

    // Dancing☆Onigiri works.
    /// Its charts are split over several dos files.
    DividedDos,
    /// The dos file it names is missing.
    DosMissing { file: String },
    /// The dos file it names was not read.
    DosNotRead { file: String },
    /// The dos file builds its dos with a script.
    DosScript { file: String },
    /// The music its data names was not found (as the data names it).
    WorkMusicMissing { url: String },
    /// The work names no music.
    WorkWithoutMusic,
    /// Its music script holds no audio.
    MusicScriptEmpty,
    /// Back and mask images not found (or not drawn): the first names,
    /// and how many more.
    LayerImagesMissing { names: Vec<String>, more: usize },
    /// More back and mask images than are kept.
    TooManyLayerImages { count: usize, kept: usize },
    /// A capture holds no back and mask images.
    CaptureWithoutLayers,
    /// A work's song of the same pack and file name came first (where).
    SameFileAs { first: String },
    /// Songs of a captured work without their music: their titles.
    MusicNotCaptured { titles: Vec<String> },

    // FFR levels.
    /// The level has no MP3 music.
    LevelWithoutMp3,
    /// No audio file beside a text level.
    LevelWithoutMusic,
    /// The level's SWF could not be read.
    Swf(SwfError),

    // Noteskins (the web's import).
    /// Whole line: skins for games the game does not play.
    SkinsForOtherGames { count: usize, games: Vec<String> },
    /// Whole line: more skins than an import keeps.
    TooManySkins { count: usize, kept: usize },
    /// Another skin of the import has the same name.
    SameSkinName,
    /// More images and scripts than a skin keeps.
    TooManySkinFiles { count: usize, kept: usize },
    /// A text file of the skin too large, left out.
    SkinFileTooLarge { file: String },
    /// The skin's texts pass `megabytes`; the rest left out.
    SkinTextsTooLarge { megabytes: u64 },

    // Announcers.
    /// Whole line: more announcers than an import keeps.
    TooManyAnnouncers { count: usize, kept: usize },
    /// An announcer with none of the lines the game speaks.
    AnnouncerWithoutLines,
    /// Another announcer of the import has the same name.
    SameAnnouncerName,
    /// Sounds left out past the limits.
    AnnouncerSoundsLeftOut {
        count: usize,
        max_files: usize,
        max_megabytes: u64,
    },
    /// Folders of lines no longer spoken (a detail): the first names
    /// and how many folders in all.
    AnnouncerOldFolders { count: usize, names: Vec<String> },
    /// Folders no line uses (a detail).
    AnnouncerUnknownFolders { count: usize, names: Vec<String> },
    /// Folders without a sound (a detail).
    AnnouncerEmptyFolders { count: usize, names: Vec<String> },
    /// Copies inside its line folders, passed over (a detail).
    AnnouncerNestedCopies { count: usize, names: Vec<String> },
    /// Other files passed over (a detail).
    AnnouncerOtherFiles { count: usize },

    // Storing (the web's import).
    /// Whole line: the shared background videos could not be kept.
    SharedVideosFailed { error: String },
    /// The pack's banner could not be stored, and why.
    PackBannerFailed(Box<Finding>),
    /// Whole line: the packs' banners could not be stored.
    PackBannersFailed { error: String },
    /// Whole line: songs gone from a folder read again could not be
    /// removed.
    RemovingGoneFailed { error: String },
    /// Whole line: stored works and folders could not be tidied.
    TidyingFailed { error: String },
    /// The song's picture could not be taken from its background movie
    /// (the decoder's words); its background image shows (a detail).
    StillFailed { movie: String, reason: String },
    /// The song's picture could not be taken now (why); it is taken at
    /// its first play (a detail).
    StillLater { reason: String },
}

/// A failure of the system's, said by its words.
impl From<String> for Finding {
    fn from(error: String) -> Finding {
        Finding::System(error)
    }
}

impl From<&str> for Finding {
    fn from(error: &str) -> Finding {
        Finding::System(error.to_string())
    }
}

impl From<ZipError> for Finding {
    fn from(e: ZipError) -> Finding {
        Finding::Zip(e)
    }
}

impl From<SwfError> for Finding {
    fn from(e: SwfError) -> Finding {
        Finding::Swf(e)
    }
}

/// Why an import of files imported nothing at all.
#[derive(Clone, Debug, PartialEq)]
pub enum ImportFailure {
    /// Nothing among the files was a song, a work, a level, a skin or an
    /// announcer.
    NothingFound,
    /// It could not run: the system's words.
    System(String),
}

/// What an import of files (the web's) did: what its summary counts,
/// its lines by section.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    /// Titles of the songs stored.
    pub imported: Vec<String>,
    /// `(what, why)` for everything left out: simfiles that could not be
    /// imported, folders with music but no simfile.
    pub skipped: Vec<(String, Finding)>,
    /// `(what, problem)` for songs imported with something missing.
    pub warnings: Vec<(String, Finding)>,
    /// `(what, detail)` for what makes no difference in play: shown to
    /// pack authors who ask for details.
    pub notes: Vec<(String, Finding)>,
    /// Images stored from shared background folders (`RandomMovies`,
    /// `SongMovies`).
    pub shared_images: usize,
    /// Movies from shared background folders kept (copied or linked).
    pub shared_videos: usize,
    /// Other files in shared folders, left out.
    pub shared_skipped: usize,
    /// Background-change movies of the imported songs that will play.
    pub videos: usize,
    /// Movies left out because background videos are not stored.
    pub videos_left_out: usize,
    /// Songs removed because their folder is gone from a folder read
    /// again.
    pub removed: usize,
    /// `(name, what of it draws)` for the noteskins imported.
    pub noteskins: Vec<(String, SkinSummary)>,
    /// `(name, what it speaks)` for the announcers imported.
    pub announcers: Vec<(String, AnnouncerSummary)>,
    /// Set when the import imported nothing, and why.
    pub error: Option<ImportFailure>,
}

impl ImportReport {
    pub fn failed(error: ImportFailure) -> ImportReport {
        ImportReport {
            error: Some(error),
            ..ImportReport::default()
        }
    }
}

/// What a running import of files does now.
#[derive(Clone, Debug, PartialEq)]
pub enum ImportStep {
    /// Listing what was picked.
    ReadingFiles,
    /// Reading a zip's directory (its path).
    ReadingZip {
        path: String,
    },
    StoringSharedImages {
        count: usize,
    },
    StoringSharedVideos {
        count: usize,
    },
    /// Song `index` (from 1) of `total`, named.
    ImportingSong {
        index: usize,
        total: usize,
        name: String,
    },
    /// A Dancing☆Onigiri work, by its page or manifest.
    ImportingWork {
        label: String,
    },
    /// FFR level `index` (from 1) of `total`, titled.
    ReadingLevel {
        index: usize,
        total: usize,
        title: String,
    },
    StoringSkin {
        name: String,
    },
    /// Running a skin's Lua to see what of it draws.
    RunningSkinLua {
        name: String,
    },
    StoringAnnouncer {
        name: String,
    },
    /// Taking the imported songs' pictures from their videos, after the
    /// import: `done` of `total`.
    TakingStills {
        done: usize,
        total: usize,
    },
}

/// What of an imported noteskin draws, as the import found it (a dance
/// skin probed on dance single's Down, a pump skin on pump single's
/// UpLeft).
#[derive(Clone, Debug, PartialEq)]
pub struct SkinSummary {
    /// A pump skin, probed on Pump single.
    pub pump: bool,
    pub drawn: SkinDrawn,
}

/// [`SkinSummary`]'s finding.
#[derive(Clone, Debug, PartialEq)]
pub enum SkinDrawn {
    /// StepMania's shared files (`common`), which other skins fall back
    /// on.
    Common,
    /// It could not be loaded: the loader's words.
    Failed(String),
    /// Its Lua ran: `own` of `total` elements are its own; why the first
    /// others are drawn built in (the loader's words).
    Lua {
        own: usize,
        total: usize,
        why: Vec<String>,
    },
    /// Its Lua cannot draw the layout's lanes.
    NoLanes,
    /// Its Lua stopped (the error's words); drawn without it.
    LuaStopped { error: String, without: SkinFiles },
    /// Made for StepMania 3.9 or OpenITG: `own` of `total` elements its
    /// own; why the first others are drawn built in.
    Legacy {
        own: usize,
        total: usize,
        why: Vec<String>,
    },
    /// Drawn without Lua.
    Files(SkinFiles),
}

/// What a skin drawn without Lua is made of, of `total` elements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkinFiles {
    pub total: usize,
    /// Its own images and models.
    pub own: usize,
    /// Lua (drawn built in).
    pub lua: usize,
    /// StepMania's own skins' (drawn built in).
    pub builtin: usize,
    /// What cannot be drawn.
    pub other: usize,
}

/// What an imported announcer speaks: its lines (where they are
/// spoken), its sounds and their size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnnouncerSummary {
    pub lines: usize,
    /// Of them, spoken in play, on the results and on song select.
    pub in_play: usize,
    pub on_results: usize,
    pub on_select: usize,
    pub sounds: usize,
    pub bytes: u64,
}
