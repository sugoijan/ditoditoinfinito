//! Our own reports of what went wrong, as the screens say them
//! (`docs/plans/localization.md`, step 4): why a song could not be
//! played ([`LoadFailure`]), why a background video could not
//! ([`video_text`], of `ddi_platform::video::VideoError`), what went wrong
//! with a share ([`share_text`], of `ddi_library::share::ShareError`).
//! Each is a value made where the trouble is found and worded here in
//! the player's language; what a system said goes through
//! [`super::errors::error_text`], what a reader or a decoder said in its
//! own technical words stays as data. Their `Display` is English, for
//! the logs.

use ddi_l10n::UiText;
use ddi_l10n::msg::errors as m;
use ddi_library::share::ShareError;
use ddi_platform::video::{VideoError, VideoTrouble};

use super::errors::error_text;

/// Why a song could not be played, found by the shells' loading (both
/// shells' play, a replay's chart, the chart editor's playtest): worded
/// after "The song failed to load:" ([`LoadFailure::text`]).
#[derive(Clone, Debug, PartialEq)]
pub enum LoadFailure {
    /// The library has no song of this id.
    NoSong,
    /// One of its files could not be read: the file, what the system said.
    File { file: String, error: String },
    /// Web: a file of an imported song is no longer stored.
    Missing { file: String },
    /// Web: the folder the song plays from is not known.
    FolderUnknown,
    /// Web: the folder the song plays from is no longer kept.
    FolderGone,
    /// Web: a file is no longer in the folder the song plays from.
    NotInFolder { file: String, folder: String },
    /// Its chart file does not read: the file, what the reader said.
    Chart { file: String, error: String },
    /// Its music could not be decoded: what the decoder said.
    Music(String),
    /// Web: its Dancing☆Onigiri work is no longer stored.
    WorkGone,
    /// Its Dancing☆Onigiri work could not be read: what was wrong.
    Work(String),
    /// Its share could not be read.
    Share(ShareError),
    /// The sound could not start: what the system said.
    Audio(String),
    /// Anything else: what was said (a system's error, or technical).
    Other(String),
}

impl LoadFailure {
    /// A clause, after "The song failed to load:" (`prompt-song-failed`).
    pub fn text(&self) -> UiText {
        match self {
            LoadFailure::NoSong => m::load_no_song(),
            LoadFailure::File { file, error } => m::load_file(file, error_text(error)),
            LoadFailure::Missing { file } => m::load_missing(file),
            LoadFailure::FolderUnknown => m::load_folder_unknown(),
            LoadFailure::FolderGone => m::load_folder_gone(),
            LoadFailure::NotInFolder { file, folder } => m::load_not_in_folder(file, folder),
            LoadFailure::Chart { file, error } => m::load_chart(file, UiText::data(error)),
            LoadFailure::Music(e) => m::load_music(error_text(e)),
            LoadFailure::WorkGone => m::load_work_gone(),
            LoadFailure::Work(e) => m::load_work(UiText::data(e)),
            LoadFailure::Share(e) => m::load_share(share_text(e)),
            LoadFailure::Audio(e) => m::load_audio(error_text(e)),
            LoadFailure::Other(e) => error_text(e),
        }
    }

    /// The sentence the start prompt shows.
    pub fn sentence(&self) -> UiText {
        ddi_l10n::msg::prompt::song_failed(self.text())
    }
}

impl std::fmt::Display for LoadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadFailure::NoSong => f.write_str("no such song"),
            LoadFailure::File { file, error } => write!(f, "{file}: {error}"),
            LoadFailure::Missing { file } => {
                write!(f, "{file} is missing from the stored song; import it again")
            }
            LoadFailure::FolderUnknown => {
                f.write_str("the folder this song plays from is unknown; import it again")
            }
            LoadFailure::FolderGone => {
                f.write_str("the folder this song plays from is no longer kept; import it again")
            }
            LoadFailure::NotInFolder { file, folder } => write!(
                f,
                "{file} is no longer in the folder “{folder}”; re-read the folder or import the \
                 song again"
            ),
            LoadFailure::Chart { file, error } => write!(f, "{file}: {error}"),
            LoadFailure::Music(e) => write!(f, "the music: {e}"),
            LoadFailure::WorkGone => f.write_str("the work is no longer stored; import it again"),
            LoadFailure::Work(e) => write!(f, "the work: {e}"),
            LoadFailure::Share(e) => write!(f, "the share: {e}"),
            LoadFailure::Audio(e) => write!(f, "the sound: {e}"),
            LoadFailure::Other(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for LoadFailure {}

impl From<String> for LoadFailure {
    /// What a system (or a reader) said, of no kind of ours.
    fn from(e: String) -> LoadFailure {
        LoadFailure::Other(e)
    }
}

impl From<&str> for LoadFailure {
    fn from(e: &str) -> LoadFailure {
        LoadFailure::Other(e.to_string())
    }
}

impl From<ShareError> for LoadFailure {
    fn from(e: ShareError) -> LoadFailure {
        LoadFailure::Share(e)
    }
}

impl From<LoadFailure> for String {
    /// The English, for what keeps errors as text (the logs, the tests).
    fn from(e: LoadFailure) -> String {
        e.to_string()
    }
}

/// Why a background video could not play, a clause (after "could not
/// play:" in the results' note, after "background videos cannot play:"
/// on the web's prompt).
pub fn video_text(e: &VideoError) -> UiText {
    match e {
        VideoError::NotIncluded => m::video_not_included(),
        VideoError::NotLoaded(e) => m::video_not_loaded(error_text(e)),
        VideoError::Crashed(e) => m::video_crashed(e),
        VideoError::UnknownFormat => m::video_unknown_format(),
        VideoError::Broken(n) => m::video_broken(*n),
        VideoError::Unseekable => m::video_unseekable(),
        VideoError::TooLarge {
            width,
            height,
            limit,
        } => m::video_too_large(width.to_string(), height.to_string(), limit.to_string()),
        VideoError::BadFrame => m::video_bad_frame(),
        VideoError::File(e) => error_text(e),
        VideoError::Decoder(e) => UiText::data(e),
    }
}

/// The results' note on the background videos that could not play.
pub fn video_trouble_text(t: &VideoTrouble) -> UiText {
    use ddi_l10n::msg::results as rm;
    let why = video_text(&t.why);
    if t.count <= 1 {
        rm::note_video_trouble(&t.name, why)
    } else if t.same {
        rm::note_videos_trouble(t.count, why)
    } else {
        rm::note_videos_trouble_first(t.count, &t.name, why)
    }
}

/// What went wrong with a share, a clause (after a colon: the shares
/// dialog's "The shares could not be read:", [`share_load_text`]'s,
/// [`share_upload_text`]'s, a shared song's load).
pub fn share_text(e: &ShareError) -> UiText {
    match e {
        ShareError::Gone { .. } => m::share_gone(),
        ShareError::NoToken => m::share_no_token(),
        ShareError::Nothing => m::share_nothing(),
        ShareError::TokenRefused => m::share_token_refused(),
        ShareError::TooLarge => m::share_too_large(),
        ShareError::Status { status, body } => match body {
            Some(body) => m::share_status_body(status.to_string(), UiText::data(body)),
            None => m::share_status(status.to_string()),
        },
        ShareError::Failed { error, .. } => error_text(error),
        ShareError::Answer(e) => m::share_answer(e),
        ShareError::NoChart(title) => m::share_no_chart(title),
        ShareError::In { name, error } => m::share_in(name, share_text(error)),
    }
}

/// Why a share could not be read, a sentence on its own (the web's page
/// of a link that is gone, desktop's notice over the shared songs).
pub fn share_load_text(e: &ShareError) -> UiText {
    match e {
        ShareError::Gone { .. } => m::share_link_gone(),
        e => m::share_not_loaded(share_text(e)),
    }
}

/// Why an upload failed, a sentence under the share dialog's title.
pub fn share_upload_text(e: &ShareError) -> UiText {
    m::share_not_shared(share_text(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_reports_are_worded_in_the_player_s_language() {
        ddi_l10n::set_language("en");
        assert_eq!(
            &*LoadFailure::NoSong.sentence(),
            "The song failed to load: the library has no such song"
        );
        assert_eq!(
            &*LoadFailure::File {
                file: "a.ogg".into(),
                error: "No such file or directory (os error 2)".into(),
            }
            .text(),
            "a.ogg could not be read: not found (No such file or directory (os error 2))"
        );
        let t = VideoTrouble::of(&[
            ("a.avi".into(), VideoError::NotIncluded),
            ("b.avi".into(), VideoError::NotIncluded),
        ])
        .unwrap();
        assert_eq!(
            &*video_trouble_text(&t),
            "2 videos could not play: the video decoder is not included in this build; the \
             song background showed."
        );
        let e = ShareError::of("x.png", ddi_library::share::refusal(403, None));
        assert_eq!(&*share_text(&e), "x.png: the upload token was refused");
        assert_eq!(
            &*share_load_text(&ShareError::Gone { expired: true }),
            "This link doesn't exist or has expired."
        );
        ddi_l10n::set_language("ja");
        assert!(LoadFailure::NoSong.sentence().contains("ライブラリ"));
        assert!(video_text(&VideoError::NotIncluded).contains("動画デコーダー"));
        ddi_l10n::set_language("en");
    }
}
