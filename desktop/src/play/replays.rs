//! Replays on desktop (`docs/plans/play-history.md`, Replays): a kept
//! play read back, its chart found (where the record says, else in a song
//! with a chart of its content), the skin it was drawn with looked for;
//! the [`Request`] that watches it (or shows its results), or one the
//! prompt refuses with the reason.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_chart::ChartDigest;
use ddi_ui::state::history::{Plays, Replay};
use ddi_ui::state::settings::Settings;

use super::Request;
use super::load::read_chart;

/// The request for kept play `id`: its replay, or its results with
/// `view_results`. Refused (`Request::refused`) when it cannot be
/// replayed.
pub fn request(plays: &Plays, id: &str, view_results: bool) -> Request {
    request_with(plays, id, view_results, &crate::library::find_by_digest)
}

/// [`request`], `find` naming a song with a chart of a content.
pub fn request_with(
    plays: &Plays,
    id: &str,
    view_results: bool,
    find: &dyn Fn(ChartDigest) -> Option<String>,
) -> Request {
    let refused = |why: String| Request {
        refused: Some(why),
        view_results,
        ..Request::song("", 0, false)
    };
    let got: Rc<RefCell<Option<Result<_, String>>>> = Rc::default();
    let g = got.clone();
    plays.entry(id, move |r| *g.borrow_mut() = Some(r));
    let entry = got
        .borrow_mut()
        .take()
        .unwrap_or_else(|| Err("The play history did not answer.".into()));
    let replay = match entry.and_then(|e| Replay::from_entry(&e)) {
        Ok(r) => r,
        Err(e) => return refused(e),
    };
    let key = replay.record.chart.clone();
    // Where the record says; else a song with a chart of its content.
    let mut found = None;
    let mut why = Replay::missing_song();
    if let Ok(song) = read_chart(&key.song) {
        match replay.chart(&song) {
            Ok(i) => found = Some((key.song.clone(), i, song)),
            Err(e) => why = e,
        }
    }
    if found.is_none()
        && let Some(other) = find(key.digest)
        && let Ok(song) = read_chart(&other)
        && let Ok(i) = replay.chart(&song)
    {
        found = Some((other, i, song));
    }
    let Some((song_id, chart, song)) = found else {
        return refused(why);
    };
    // The skin it was drawn with (bundled ones ship with the game).
    let settings = replay.settings(&Settings::load());
    if let Some(skin) = replay.skin(&settings, &song.charts[chart].layout)
        && !skin.starts_with("bundled:")
        && !crate::library::noteskins::all(&crate::library::all_folders())
            .iter()
            .any(|s| s.key() == skin)
    {
        return refused(Replay::missing_skin(&skin));
    }
    Request {
        replay: Some(Rc::new(replay)),
        view_results,
        ..Request::song(&song_id, chart, false)
    }
}

/// Minutes east of UTC at a time (ms since the epoch), as the system
/// keeps the player's time zone (UTC where it cannot be read).
pub fn tz_minutes(ms: f64) -> i32 {
    #[cfg(unix)]
    {
        let t = (ms / 1000.0).floor() as libc::time_t;
        // SAFETY: `localtime_r` writes only into `tm`, which outlives it.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
        if ok {
            return (tm.tm_gmtoff / 60) as i32;
        }
    }
    let _ = ms;
    0
}
