//! An import's report in the player's language
//! (`docs/plans/localization.md`, step 4): what runs, the summary line,
//! the sections and each line's "what: why", worded from the values the
//! library and the shells make where they find them
//! ([`ddi_library::report`]). File and folder names, layouts and formats
//! are data, as found; what another part of the game said in its own
//! words ([`Finding::Other`]) is shown as it is; a system's failure is
//! worded by [`super::errors`].

use ddi_l10n::UiText;
use ddi_l10n::msg::import as m;
use ddi_library::pack::MissingTag;
use ddi_library::report::{
    AnnouncerSummary, Finding, ImportFailure, ImportReport, ImportStep, SkinDrawn, SkinFiles,
    SkinSummary,
};
use ddi_library::swf::SwfError;
use ddi_library::zip::ZipError;
use ddi_platform::video::{VideoCodec, VideoContainer, VideoFormat};

use super::errors::error_text;

/// Data: what is not ours to translate (names as found).
fn d(text: impl AsRef<str>) -> UiText {
    UiText::data(text)
}

/// The status line while an import of files runs.
pub fn step_text(step: &ImportStep) -> UiText {
    match step {
        ImportStep::ReadingFiles => m::step_reading_files(),
        ImportStep::ReadingZip { path } => m::step_reading_zip(d(path)),
        ImportStep::StoringSharedImages { count } => m::step_shared_images(*count),
        ImportStep::StoringSharedVideos { count } => m::step_shared_videos(*count),
        ImportStep::ImportingSong { index, total, name } => m::step_song(*index, *total, d(name)),
        ImportStep::ImportingWork { label } => m::step_work(d(label)),
        ImportStep::ReadingLevel {
            index,
            total,
            title,
        } => m::step_level(*index, *total, d(title)),
        ImportStep::StoringSkin { name } => m::step_skin(d(name)),
        ImportStep::RunningSkinLua { name } => m::step_skin_lua(d(name)),
        ImportStep::StoringAnnouncer { name } => m::step_announcer(d(name)),
        ImportStep::TakingStills { done, total } => m::step_stills(*done, *total),
    }
}

/// Whether the import imported nothing at all.
pub fn failed(r: &ImportReport) -> bool {
    r.imported.is_empty()
        && r.noteskins.is_empty()
        && r.announcers.is_empty()
        && r.shared_images == 0
        && r.shared_videos == 0
        && r.shared_skipped == 0
        && r.videos_left_out == 0
        && r.removed == 0
}

/// The summary line: what the import did, a tag each, or why it did
/// nothing.
pub fn summary(r: &ImportReport, details: bool) -> String {
    let counts = [
        (r.imported.len(), m::summary_songs as fn(usize) -> UiText),
        (r.noteskins.len(), m::summary_noteskins),
        (r.announcers.len(), m::summary_announcers),
        (r.shared_images, m::summary_shared_images),
        (r.shared_videos, m::summary_shared_videos),
        (r.shared_skipped, m::summary_shared_skipped),
        (r.videos, m::summary_videos),
        (r.videos_left_out, m::summary_videos_left_out),
        (r.removed, m::summary_removed),
        (r.warnings.len(), m::summary_warnings),
        (r.skipped.len(), m::summary_skipped),
        (if details { r.notes.len() } else { 0 }, m::summary_details),
    ];
    let tags: Vec<String> = counts
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, tag)| tag(n).into())
        .collect();
    if !tags.is_empty() {
        return tags.join(" · ");
    }
    match &r.error {
        Some(ImportFailure::NothingFound) => m::nothing_found(),
        Some(ImportFailure::System(e)) => error_text(e),
        None => m::no_simfiles(),
    }
    .into()
}

/// The report's sections: a heading and its lines each, the details for
/// pack authors only with `details`.
pub fn sections(r: &ImportReport, details: bool) -> Vec<(UiText, Vec<UiText>)> {
    let mut out = Vec::new();
    let named = |name: &str, why: UiText| m::line(UiText::data(name), why);
    if !r.noteskins.is_empty() {
        out.push((
            m::section_noteskins(),
            r.noteskins
                .iter()
                .map(|(name, s)| named(name, skin_text(s)))
                .collect(),
        ));
    }
    if !r.announcers.is_empty() {
        out.push((
            m::section_announcers(),
            r.announcers
                .iter()
                .map(|(name, a)| named(name, announcer_text(a)))
                .collect(),
        ));
    }
    for (title, lines, shown) in [
        (m::section_skipped(), &r.skipped, true),
        (m::section_problems(), &r.warnings, true),
        (m::section_details(), &r.notes, details),
    ] {
        if shown && !lines.is_empty() {
            out.push((title, lines.iter().map(|(what, f)| line(what, f)).collect()));
        }
    }
    out
}

/// A report's line: what it is about (data), then why; a finding that
/// says what it is about (an empty `what`) alone.
pub fn line(what: &str, f: &Finding) -> UiText {
    if what.is_empty() {
        finding_text(f)
    } else {
        m::line(UiText::data(what), finding_text(f))
    }
}

/// Names as found, joined (data).
fn names(list: &[String]) -> UiText {
    UiText::data(list.join(", "))
}

/// A tag of the simfile as it is written (a chart's own `#MUSIC` with its
/// chart's place).
fn tag_text(tag: MissingTag) -> UiText {
    let tag = match tag {
        MissingTag::ChartMusic(i) => return m::tag_chart_music(i + 1),
        MissingTag::Music => "#MUSIC",
        MissingTag::Banner => "#BANNER",
        MissingTag::Background => "#BACKGROUND",
        MissingTag::Jacket => "#JACKET",
        MissingTag::CdTitle => "#CDTITLE",
        MissingTag::LyricsPath => "#LYRICSPATH",
        MissingTag::Preview => "#PREVIEW",
        MissingTag::PreviewVid => "#PREVIEWVID",
    };
    UiText::data(tag)
}

/// A movie's format: its codec in its container, as far as they are
/// known (their names are data).
pub fn video_format_text(f: &VideoFormat) -> UiText {
    let codec = match &f.codec {
        VideoCodec::H264 => "H.264",
        VideoCodec::Hevc => "HEVC",
        VideoCodec::Mpeg1 => "MPEG-1",
        VideoCodec::Mpeg2 => "MPEG-2",
        VideoCodec::Mpeg4 => "MPEG-4 Part 2",
        VideoCodec::Vp8 => "VP8",
        VideoCodec::Vp9 => "VP9",
        VideoCodec::Av1 => "AV1",
        VideoCodec::Other(name) => name.as_str(),
        VideoCodec::Unknown => "",
    };
    let named = |name: &str| Some(UiText::data(name));
    let container = match f.container {
        VideoContainer::Avi => named("AVI"),
        VideoContainer::Mp4 => named("MP4"),
        VideoContainer::Matroska => named("Matroska"),
        VideoContainer::Ogg => named("Ogg"),
        VideoContainer::Asf => named("ASF"),
        VideoContainer::Flv => named("FLV"),
        VideoContainer::MpegPs => Some(m::video_mpeg_ps()),
        VideoContainer::MpegTs => Some(m::video_mpeg_ts()),
        VideoContainer::MpegVideo => Some(m::video_mpeg_video()),
        VideoContainer::Unknown => None,
    };
    match (codec.is_empty(), container) {
        (true, None) => m::video_format_unknown(),
        (true, Some(container)) => container,
        (false, None) => UiText::data(codec),
        (false, Some(container)) => m::video_format(UiText::data(codec), container),
    }
}

/// The zip reader's reason.
fn zip_text(e: &ZipError) -> UiText {
    match e {
        ZipError::NotAZip => m::zip_not_a_zip(),
        ZipError::Corrupt(detail) => m::zip_corrupt(d(detail)),
        ZipError::MultiDisk => m::zip_multi_disk(),
        ZipError::Encrypted { name } => m::zip_encrypted(d(name)),
        ZipError::UnsupportedMethod { name, method } => {
            m::zip_method(d(name), d(method.to_string()))
        }
        ZipError::SizeMismatch {
            name,
            expected,
            actual,
        } => m::zip_size(d(name), *expected, *actual),
        ZipError::CrcMismatch {
            name,
            expected,
            actual,
        } => m::zip_crc(
            d(name),
            d(format!("{expected:08x}")),
            d(format!("{actual:08x}")),
        ),
        ZipError::Inflate(detail) => m::zip_inflate(d(detail)),
        ZipError::Decode {
            name,
            method,
            message,
        } => m::zip_decode(d(name), d(method), d(message)),
    }
}

/// Why a level's SWF could not be read.
fn swf_text(e: &SwfError) -> UiText {
    match e {
        SwfError::NotSwf => m::swf_not_swf(),
        SwfError::Inflate(detail) => m::swf_inflate(UiText::data(detail)),
        SwfError::Lzma => m::swf_lzma(),
        SwfError::CutShort => m::swf_cut_short(),
        SwfError::StringNotEnded => m::swf_string(),
        SwfError::MusicTooLong => m::swf_music_too_long(),
    }
}

/// A finding's reason (or, for a finding marked "whole line", the line).
pub fn finding_text(f: &Finding) -> UiText {
    match f {
        Finding::System(e) => error_text(e),
        Finding::Other(text) => d(text),
        Finding::FileFailed { file, error } => m::file_failed(d(file), error_text(error)),
        Finding::FileMissing { file } => m::file_missing(d(file)),
        Finding::Truncated { file } => m::truncated(d(file)),
        Finding::Zip(e) => zip_text(e),
        Finding::Encrypted => m::encrypted(),

        Finding::NoCharts => m::no_charts(),
        Finding::NoPlayableCharts { layouts } => m::no_playable_charts(names(layouts)),
        Finding::NoMusic => m::no_music(),
        Finding::SameFolderAs { first } => m::same_folder(d(first)),
        Finding::MusicWithoutSimfile { music } => m::music_without_simfile(d(music)),
        Finding::LayoutsLeftOut { layouts } => m::layouts_left_out(names(layouts)),
        Finding::Missing { tag, file } => m::missing(tag_text(*tag), d(file)),
        Finding::MissingPreview { tag, file } => m::missing_preview(tag_text(*tag), d(file)),
        Finding::MissingChartMusic { tag, file } => m::missing_chart_music(tag_text(*tag), d(file)),
        Finding::MissingWithout { tag, file } => match tag {
            MissingTag::Background => m::missing_background(tag_text(*tag), d(file)),
            _ => m::missing_banner(tag_text(*tag), d(file)),
        },
        Finding::MissingReplaced { tag, file, used } => {
            m::missing_replaced(tag_text(*tag), d(file), d(used))
        }
        Finding::TooManyBackgroundImages { count, kept } => m::too_many_bg_images(*count, *kept),
        Finding::PreviewVideoNotStored { file } => m::preview_video_not_stored(d(file)),
        Finding::PreviewVideoUnplayable { file } => m::preview_video_unplayable(d(file)),
        Finding::VideosUnplayable { videos } => {
            let list: Vec<String> = videos
                .iter()
                .map(|(file, format)| m::video_item(d(file), video_format_text(format)).into())
                .collect();
            m::videos_unplayable(list.join(", "))
        }
        Finding::VideosNotStored { files } => m::videos_not_stored(names(files)),
        Finding::TooManyScriptFiles { count, kept } => m::too_many_script_files(*count, *kept),
        Finding::SharedAnimations { names: n } => m::shared_animations(names(n)),
        Finding::MissingAnimations { names: n } => m::missing_animations(names(n)),
        Finding::MissingBackgrounds { names: n } => m::missing_backgrounds(names(n)),

        Finding::DividedDos => m::divided_dos(),
        Finding::DosMissing { file } => m::dos_missing(d(file)),
        Finding::DosNotRead { file } => m::dos_not_read(d(file)),
        Finding::DosScript { file } => m::dos_script(d(file)),
        Finding::WorkMusicMissing { url } => m::work_music_missing(d(url)),
        Finding::WorkWithoutMusic => m::work_without_music(),
        Finding::MusicScriptEmpty => m::music_script_empty(),
        Finding::LayerImagesMissing { names: n, more } => m::layer_images_missing(names(n), *more),
        Finding::TooManyLayerImages { count, kept } => m::too_many_layer_images(*count, *kept),
        Finding::CaptureWithoutLayers => m::capture_without_layers(),
        Finding::SameFileAs { first } => m::same_file(d(first)),
        Finding::MusicNotCaptured { titles } => m::music_not_captured(titles.len(), names(titles)),

        Finding::LevelWithoutMp3 => m::level_without_mp3(),
        Finding::LevelWithoutMusic => m::level_without_music(),
        Finding::Swf(e) => swf_text(e),

        Finding::SkinsForOtherGames { count, games } => m::skins_other_games(*count, names(games)),
        Finding::TooManySkins { count, kept } => m::too_many_skins(*count, *kept),
        Finding::SameSkinName => m::same_skin_name(),
        Finding::TooManySkinFiles { count, kept } => m::too_many_skin_files(*count, *kept),
        Finding::SkinFileTooLarge { file } => m::skin_file_too_large(d(file)),
        Finding::SkinTextsTooLarge { megabytes } => m::skin_texts_too_large(*megabytes),

        Finding::TooManyAnnouncers { count, kept } => m::too_many_announcers(*count, *kept),
        Finding::AnnouncerWithoutLines => m::announcer_without_lines(),
        Finding::SameAnnouncerName => m::same_announcer_name(),
        Finding::AnnouncerSoundsLeftOut {
            count,
            max_files,
            max_megabytes,
        } => m::announcer_sounds_left_out(*count, *max_files, *max_megabytes),
        Finding::AnnouncerOldFolders { count, names: n } => {
            m::announcer_old_folders(*count, names(n), more(*count, n))
        }
        Finding::AnnouncerUnknownFolders { count, names: n } => {
            m::announcer_unknown_folders(*count, names(n), more(*count, n))
        }
        Finding::AnnouncerEmptyFolders { count, names: n } => {
            m::announcer_empty_folders(*count, names(n), more(*count, n))
        }
        Finding::AnnouncerNestedCopies { count, names: n } => {
            m::announcer_nested_copies(*count, names(n), more(*count, n))
        }
        Finding::AnnouncerOtherFiles { count } => m::announcer_other_files(*count),

        Finding::SharedVideosFailed { error } => m::shared_videos_failed(error_text(error)),
        Finding::PackBannerFailed(why) => m::pack_banner_failed(finding_text(why)),
        Finding::PackBannersFailed { error } => m::pack_banners_failed(error_text(error)),
        Finding::RemovingGoneFailed { error } => m::removing_gone_failed(error_text(error)),
        Finding::TidyingFailed { error } => m::tidying_failed(error_text(error)),
        Finding::StillFailed { movie, reason } => m::still_failed(d(movie), d(reason)),
        Finding::StillLater { reason } => m::still_later(d(reason)),
    }
}

/// How many of `count` a list naming `named` of them leaves out.
fn more(count: usize, named: &[String]) -> usize {
    count.saturating_sub(named.len())
}

/// What of an imported noteskin draws.
pub fn skin_text(s: &SkinSummary) -> UiText {
    let why = |why: &[String]| UiText::data(why.join("; "));
    let text = match &s.drawn {
        SkinDrawn::Common => m::skin_common(),
        SkinDrawn::Failed(e) => UiText::data(e),
        SkinDrawn::Lua { own, total, why: w } => {
            let builtin = total.saturating_sub(*own);
            if builtin == 0 {
                m::skin_lua(*own, *total)
            } else if w.is_empty() {
                m::skin_lua_partly(*own, *total, builtin)
            } else {
                m::skin_lua_partly_why(*own, *total, builtin, why(w))
            }
        }
        SkinDrawn::NoLanes => m::skin_no_lanes(),
        SkinDrawn::LuaStopped { error, without } => {
            m::skin_lua_stopped(UiText::data(error), skin_files_text(without))
        }
        SkinDrawn::Legacy { own, total, why: w } => {
            let builtin = total.saturating_sub(*own);
            if builtin == 0 {
                m::skin_legacy(*own, *total)
            } else if w.is_empty() {
                m::skin_legacy_partly(*own, *total, builtin)
            } else {
                m::skin_legacy_partly_why(*own, *total, builtin, why(w))
            }
        }
        SkinDrawn::Files(files) => skin_files_text(files),
    };
    if s.pump { m::skin_pump(text) } else { text }
}

fn skin_files_text(f: &SkinFiles) -> UiText {
    m::skin_files(f.own, f.total, f.lua, f.builtin, f.other)
}

/// What an imported announcer speaks.
pub fn announcer_text(a: &AnnouncerSummary) -> UiText {
    m::announcer_summary(
        a.lines,
        a.in_play,
        a.on_results,
        a.on_select,
        a.sounds,
        super::data::size_text(a.bytes as f64),
    )
}

/// A report with every kind of line, every step an import runs (for
/// the walk of the screens' texts, `l10n_tests`).
#[cfg(test)]
pub(crate) fn every_line() -> (ImportReport, Vec<ImportStep>) {
    let n = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let s = String::from;
    let format = |container, codec| VideoFormat { container, codec };
    let findings = vec![
        Finding::System("No such file or directory (os error 2)".into()),
        Finding::System("odd".into()),
        Finding::Other("unknown difficulty `x`".into()),
        Finding::FileFailed {
            file: s("a.ogg"),
            error: s("NotReadableError"),
        },
        Finding::FileMissing { file: s("a.ogg") },
        Finding::Truncated { file: s("a.ogg") },
        Finding::Zip(ZipError::NotAZip),
        Finding::Zip(ZipError::Corrupt("bad")),
        Finding::Zip(ZipError::MultiDisk),
        Finding::Zip(ZipError::Encrypted { name: s("a") }),
        Finding::Zip(ZipError::UnsupportedMethod {
            name: s("a"),
            method: 99,
        }),
        Finding::Zip(ZipError::SizeMismatch {
            name: s("a"),
            expected: 2,
            actual: 1,
        }),
        Finding::Zip(ZipError::CrcMismatch {
            name: s("a"),
            expected: 2,
            actual: 1,
        }),
        Finding::Zip(ZipError::Inflate(s("bad"))),
        Finding::Zip(ZipError::Decode {
            name: s("a"),
            method: "LZMA",
            message: s("bad"),
        }),
        Finding::Encrypted,
        Finding::NoCharts,
        Finding::NoPlayableCharts {
            layouts: n(&["kb7-single"]),
        },
        Finding::NoMusic,
        Finding::SameFolderAs { first: s("P/S") },
        Finding::MusicWithoutSimfile { music: s("a.ogg") },
        Finding::LayoutsLeftOut {
            layouts: n(&["dance-couple", "kb7-single"]),
        },
        Finding::Missing {
            tag: MissingTag::Jacket,
            file: s("j.png"),
        },
        Finding::MissingPreview {
            tag: MissingTag::Preview,
            file: s("p.ogg"),
        },
        Finding::MissingChartMusic {
            tag: MissingTag::ChartMusic(2),
            file: s("c.ogg"),
        },
        Finding::MissingWithout {
            tag: MissingTag::Banner,
            file: s("b.png"),
        },
        Finding::MissingWithout {
            tag: MissingTag::Background,
            file: s("b.png"),
        },
        Finding::MissingReplaced {
            tag: MissingTag::Music,
            file: s("a.ogg"),
            used: s("b.ogg"),
        },
        Finding::TooManyBackgroundImages {
            count: 40,
            kept: 32,
        },
        Finding::PreviewVideoNotStored { file: s("p.mp4") },
        Finding::PreviewVideoUnplayable { file: s("p.mp4") },
        Finding::VideosUnplayable {
            videos: vec![
                (s("a.avi"), format(VideoContainer::Avi, VideoCodec::Av1)),
                (
                    s("b.mpg"),
                    format(VideoContainer::MpegPs, VideoCodec::Unknown),
                ),
                (s("c.ts"), format(VideoContainer::MpegTs, VideoCodec::Mpeg2)),
                (
                    s("d.m2v"),
                    format(VideoContainer::MpegVideo, VideoCodec::Mpeg2),
                ),
                (
                    s("e.bin"),
                    format(VideoContainer::Unknown, VideoCodec::Unknown),
                ),
                (s("f.bin"), format(VideoContainer::Unknown, VideoCodec::Vp9)),
            ],
        },
        Finding::VideosNotStored {
            files: n(&["a.mp4"]),
        },
        Finding::TooManyScriptFiles {
            count: 70,
            kept: 64,
        },
        Finding::SharedAnimations { names: n(&["A"]) },
        Finding::MissingAnimations { names: n(&["A"]) },
        Finding::MissingBackgrounds {
            names: n(&["a.png"]),
        },
        Finding::DividedDos,
        Finding::DosMissing { file: s("d.txt") },
        Finding::DosNotRead { file: s("d.txt") },
        Finding::DosScript { file: s("d.js") },
        Finding::WorkMusicMissing { url: s("m.mp3") },
        Finding::WorkWithoutMusic,
        Finding::MusicScriptEmpty,
        Finding::LayerImagesMissing {
            names: n(&["a.png"]),
            more: 0,
        },
        Finding::LayerImagesMissing {
            names: n(&["a.png"]),
            more: 3,
        },
        Finding::TooManyLayerImages {
            count: 130,
            kept: 128,
        },
        Finding::CaptureWithoutLayers,
        Finding::SameFileAs { first: s("w.html") },
        Finding::MusicNotCaptured {
            titles: n(&["A", "B"]),
        },
        Finding::LevelWithoutMp3,
        Finding::LevelWithoutMusic,
        Finding::Swf(SwfError::NotSwf),
        Finding::Swf(SwfError::Inflate(s("Failed"))),
        Finding::Swf(SwfError::Lzma),
        Finding::Swf(SwfError::CutShort),
        Finding::Swf(SwfError::StringNotEnded),
        Finding::Swf(SwfError::MusicTooLong),
        Finding::SkinsForOtherGames {
            count: 2,
            games: n(&["kb7"]),
        },
        Finding::TooManySkins {
            count: 40,
            kept: 32,
        },
        Finding::SameSkinName,
        Finding::TooManySkinFiles {
            count: 600,
            kept: 500,
        },
        Finding::SkinFileTooLarge { file: s("a.lua") },
        Finding::SkinTextsTooLarge { megabytes: 8 },
        Finding::TooManyAnnouncers {
            count: 60,
            kept: 50,
        },
        Finding::AnnouncerWithoutLines,
        Finding::SameAnnouncerName,
        Finding::AnnouncerSoundsLeftOut {
            count: 3,
            max_files: 2000,
            max_megabytes: 128,
        },
        Finding::AnnouncerOldFolders {
            count: 1,
            names: n(&["a"]),
        },
        Finding::AnnouncerUnknownFolders {
            count: 9,
            names: n(&["a"]),
        },
        Finding::AnnouncerEmptyFolders {
            count: 1,
            names: n(&["a"]),
        },
        Finding::AnnouncerNestedCopies {
            count: 1,
            names: n(&["a"]),
        },
        Finding::AnnouncerOtherFiles { count: 4 },
        Finding::SharedVideosFailed { error: s("odd") },
        Finding::PackBannerFailed(Box::new(Finding::FileMissing { file: s("b.png") })),
        Finding::PackBannersFailed { error: s("odd") },
        Finding::RemovingGoneFailed { error: s("odd") },
        Finding::TidyingFailed { error: s("odd") },
        Finding::StillFailed {
            movie: s("a.mp4"),
            reason: s("odd"),
        },
        Finding::StillLater { reason: s("odd") },
    ];
    let whole = |f: &Finding| {
        matches!(
            f,
            Finding::SkinsForOtherGames { .. }
                | Finding::TooManySkins { .. }
                | Finding::TooManyAnnouncers { .. }
                | Finding::SharedVideosFailed { .. }
                | Finding::PackBannersFailed { .. }
                | Finding::RemovingGoneFailed { .. }
                | Finding::TidyingFailed { .. }
        )
    };
    let lines: Vec<(String, Finding)> = findings
        .into_iter()
        .map(|f| (if whole(&f) { s("") } else { s("Pack/Song") }, f))
        .collect();
    let files = SkinFiles {
        total: 20,
        own: 15,
        lua: 2,
        builtin: 2,
        other: 1,
    };
    let skins = [
        SkinDrawn::Common,
        SkinDrawn::Failed(s("no metrics.ini")),
        SkinDrawn::Lua {
            own: 20,
            total: 20,
            why: Vec::new(),
        },
        SkinDrawn::Lua {
            own: 18,
            total: 20,
            why: Vec::new(),
        },
        SkinDrawn::Lua {
            own: 18,
            total: 20,
            why: n(&["text"]),
        },
        SkinDrawn::NoLanes,
        SkinDrawn::LuaStopped {
            error: s("nil"),
            without: files,
        },
        SkinDrawn::Legacy {
            own: 19,
            total: 19,
            why: Vec::new(),
        },
        SkinDrawn::Legacy {
            own: 18,
            total: 19,
            why: Vec::new(),
        },
        SkinDrawn::Legacy {
            own: 18,
            total: 19,
            why: n(&["text"]),
        },
        SkinDrawn::Files(files),
        SkinDrawn::Files(SkinFiles {
            total: 20,
            own: 20,
            ..SkinFiles::default()
        }),
    ];
    let noteskins = skins
        .into_iter()
        .enumerate()
        .map(|(i, drawn)| {
            (
                s("Skin"),
                SkinSummary {
                    pump: i % 2 == 1,
                    drawn,
                },
            )
        })
        .collect();
    let report = ImportReport {
        imported: n(&["A", "B"]),
        skipped: lines.clone(),
        warnings: lines.clone(),
        notes: lines,
        shared_images: 1,
        shared_videos: 2,
        shared_skipped: 3,
        videos: 1,
        videos_left_out: 2,
        removed: 1,
        noteskins,
        announcers: vec![(
            s("Voice"),
            AnnouncerSummary {
                lines: 1,
                in_play: 1,
                sounds: 1,
                bytes: 2_000_000,
                ..AnnouncerSummary::default()
            },
        )],
        error: None,
    };
    let steps = vec![
        ImportStep::ReadingFiles,
        ImportStep::ReadingZip { path: s("P.zip") },
        ImportStep::StoringSharedImages { count: 1 },
        ImportStep::StoringSharedVideos { count: 2 },
        ImportStep::ImportingSong {
            index: 1,
            total: 2,
            name: s("Song"),
        },
        ImportStep::ImportingWork { label: s("w.html") },
        ImportStep::ReadingLevel {
            index: 1,
            total: 2,
            title: s("Level"),
        },
        ImportStep::StoringSkin { name: s("Skin") },
        ImportStep::RunningSkinLua { name: s("Skin") },
        ImportStep::StoringAnnouncer { name: s("Voice") },
        ImportStep::TakingStills { done: 1, total: 2 },
    ];
    (report, steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each kind of finding has a line in [`every_line`]'s report (a new
    /// kind fails to compile here until it is added).
    #[test]
    fn the_sample_has_every_kind_of_line() {
        fn kind(f: &Finding) -> usize {
            match f {
                Finding::System(_) => 0,
                Finding::Other(_) => 1,
                Finding::FileFailed { .. } => 2,
                Finding::FileMissing { .. } => 3,
                Finding::Truncated { .. } => 4,
                Finding::Zip(_) => 5,
                Finding::Encrypted => 6,
                Finding::NoCharts => 7,
                Finding::NoPlayableCharts { .. } => 8,
                Finding::NoMusic => 9,
                Finding::SameFolderAs { .. } => 10,
                Finding::MusicWithoutSimfile { .. } => 11,
                Finding::LayoutsLeftOut { .. } => 12,
                Finding::Missing { .. } => 13,
                Finding::MissingPreview { .. } => 14,
                Finding::MissingChartMusic { .. } => 15,
                Finding::MissingWithout { .. } => 16,
                Finding::MissingReplaced { .. } => 17,
                Finding::TooManyBackgroundImages { .. } => 18,
                Finding::PreviewVideoNotStored { .. } => 19,
                Finding::PreviewVideoUnplayable { .. } => 20,
                Finding::VideosUnplayable { .. } => 21,
                Finding::VideosNotStored { .. } => 22,
                Finding::TooManyScriptFiles { .. } => 23,
                Finding::SharedAnimations { .. } => 24,
                Finding::MissingAnimations { .. } => 25,
                Finding::MissingBackgrounds { .. } => 26,
                Finding::DividedDos => 27,
                Finding::DosMissing { .. } => 28,
                Finding::DosNotRead { .. } => 29,
                Finding::DosScript { .. } => 30,
                Finding::WorkMusicMissing { .. } => 31,
                Finding::WorkWithoutMusic => 32,
                Finding::MusicScriptEmpty => 33,
                Finding::LayerImagesMissing { .. } => 34,
                Finding::TooManyLayerImages { .. } => 35,
                Finding::CaptureWithoutLayers => 36,
                Finding::SameFileAs { .. } => 37,
                Finding::MusicNotCaptured { .. } => 38,
                Finding::LevelWithoutMp3 => 39,
                Finding::LevelWithoutMusic => 40,
                Finding::Swf(_) => 41,
                Finding::SkinsForOtherGames { .. } => 42,
                Finding::TooManySkins { .. } => 43,
                Finding::SameSkinName => 44,
                Finding::TooManySkinFiles { .. } => 45,
                Finding::SkinFileTooLarge { .. } => 46,
                Finding::SkinTextsTooLarge { .. } => 47,
                Finding::TooManyAnnouncers { .. } => 48,
                Finding::AnnouncerWithoutLines => 49,
                Finding::SameAnnouncerName => 50,
                Finding::AnnouncerSoundsLeftOut { .. } => 51,
                Finding::AnnouncerOldFolders { .. } => 52,
                Finding::AnnouncerUnknownFolders { .. } => 53,
                Finding::AnnouncerEmptyFolders { .. } => 54,
                Finding::AnnouncerNestedCopies { .. } => 55,
                Finding::AnnouncerOtherFiles { .. } => 56,
                Finding::SharedVideosFailed { .. } => 57,
                Finding::PackBannerFailed(_) => 58,
                Finding::PackBannersFailed { .. } => 59,
                Finding::RemovingGoneFailed { .. } => 60,
                Finding::TidyingFailed { .. } => 61,
                Finding::StillFailed { .. } => 62,
                Finding::StillLater { .. } => 63,
            }
        }
        let (report, steps) = every_line();
        let kinds: std::collections::BTreeSet<usize> =
            report.skipped.iter().map(|(_, f)| kind(f)).collect();
        assert_eq!(kinds.len(), 64);
        assert_eq!(steps.len(), 11);
    }

    #[test]
    fn a_report_reads_as_it_did_in_english() {
        ddi_l10n::set_language("en");
        let report = ImportReport {
            imported: vec!["A".into(), "B".into()],
            skipped: vec![
                ("Pack/Song B".into(), Finding::NoMusic),
                (
                    String::new(),
                    Finding::SkinsForOtherGames {
                        count: 3,
                        games: vec!["kb7".into(), "techno".into()],
                    },
                ),
            ],
            warnings: vec![(
                "Pack/Song A".into(),
                Finding::MissingWithout {
                    tag: MissingTag::Banner,
                    file: "bn.png".into(),
                },
            )],
            notes: vec![(
                "Pack/Song A".into(),
                Finding::MissingChartMusic {
                    tag: MissingTag::ChartMusic(1),
                    file: "gone.ogg".into(),
                },
            )],
            noteskins: vec![(
                "Metal".into(),
                SkinSummary {
                    pump: true,
                    drawn: SkinDrawn::Files(SkinFiles {
                        total: 20,
                        own: 15,
                        lua: 2,
                        builtin: 3,
                        other: 0,
                    }),
                },
            )],
            announcers: vec![(
                "Voice".into(),
                AnnouncerSummary {
                    lines: 6,
                    in_play: 5,
                    on_results: 1,
                    on_select: 0,
                    sounds: 8,
                    bytes: 7_300_000,
                },
            )],
            ..ImportReport::default()
        };
        assert_eq!(
            summary(&report, false),
            "2 songs imported · 1 noteskin stored (choose it under Options, Arrows) · \
             1 announcer stored (choose it under Options, Sound) · 1 warning · 2 skipped"
        );
        assert!(!failed(&report));
        let s = sections(&report, true);
        let text = |t: &UiText| t.to_string();
        assert_eq!(
            s.iter().map(|(t, _)| text(t)).collect::<Vec<_>>(),
            [
                "Noteskins",
                "Announcers",
                "Not imported",
                "Imported with problems",
                "Details for pack authors (no difference in play)"
            ]
        );
        assert_eq!(
            text(&s[0].1[0]),
            "Metal: a pump skin, on Pump single: 15 of 20 elements are its own images and \
             models; 2 are Lua (drawn built in); 3 come from StepMania's own skins (drawn \
             built in)"
        );
        assert_eq!(
            text(&s[1].1[0]),
            "Voice: 6 lines (5 in play, 1 on the results, 0 on song select), 8 sounds, 7.3 MB"
        );
        assert_eq!(
            s[2].1.iter().map(text).collect::<Vec<_>>(),
            [
                "Pack/Song B: no audio file found next to the simfile",
                "3 noteskins: for other games (kb7, techno), not dance or pump",
            ]
        );
        assert_eq!(
            text(&s[3].1[0]),
            "Pack/Song A: #BANNER names bn.png, which is missing; the song has no banner"
        );
        assert_eq!(
            text(&s[4].1[0]),
            "Pack/Song A: #MUSIC (chart 2) names gone.ogg, which is missing; the song's \
             music plays instead"
        );
        assert_eq!(sections(&report, false).len(), 4);
        // Nothing found.
        let none = ImportReport::failed(ImportFailure::NothingFound);
        assert!(failed(&none));
        assert!(summary(&none, false).starts_with("No .sm, .ssc or .dwi files"));
        assert_eq!(
            summary(&ImportReport::default(), false),
            "No simfiles found in what was picked."
        );
        ddi_l10n::set_language("ja");
        assert_eq!(summary(&report, false).split(" · ").count(), 5);
        assert!(text(&s[0].1[0]).starts_with("Metal"));
        ddi_l10n::set_language("en");
    }

    #[test]
    fn lists_name_a_few_and_count_the_rest() {
        ddi_l10n::set_language("en");
        let n = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            &*finding_text(&Finding::AnnouncerOldFolders {
                count: 8,
                names: n(&["a", "b"]),
            }),
            "8 folders of lines no longer spoken: a, b and 6 more"
        );
        assert_eq!(
            &*finding_text(&Finding::AnnouncerUnknownFolders {
                count: 1,
                names: n(&["ssstyle intro"]),
            }),
            "1 folder no line uses: ssstyle intro"
        );
        assert_eq!(
            &*finding_text(&Finding::LayerImagesMissing {
                names: n(&["a.png"]),
                more: 0,
            }),
            "back and mask images not found (or not drawn, like SVG): a.png"
        );
        assert_eq!(
            &*finding_text(&Finding::VideosUnplayable {
                videos: vec![(
                    "bg.avi".into(),
                    VideoFormat {
                        container: VideoContainer::Avi,
                        codec: VideoCodec::Other("Indeo".into()),
                    }
                )],
            }),
            "background videos in a format the game cannot play (the song background shows \
             instead): bg.avi (Indeo in AVI)"
        );
        assert_eq!(
            &*step_text(&ImportStep::ImportingSong {
                index: 3,
                total: 12,
                name: "Song".into()
            }),
            "importing 3/12: Song"
        );
        assert_eq!(
            &*finding_text(&Finding::Zip(ZipError::NotAZip)),
            "not a zip archive (no end of central directory record)"
        );
    }
}
