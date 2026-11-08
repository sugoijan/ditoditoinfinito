//! A play staged for the announcer studio (`docs/plans/announcer-studio.md`,
//! step 4): the real play, autoplayed, from a few seconds before the
//! moment a line is said for. What came before is given ([`Staging`]):
//! the notes before its start judged at once as the autoplayer would
//! have pressed them (`Player::start_staged`), a combo on top, where the
//! announcer's comment gap stands; chosen notes are missed or pressed
//! into a lower window ([`Touch`]). Presentation only: such a play is
//! never kept.

use ddi_engine::autoplay::Press;
use ddi_library::announcer::Line;

/// What a staged play starts with, and how it is played.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Staging {
    /// The song second the play starts at, every note before it judged at
    /// once (0: the song's start, as any play).
    pub from: f64,
    /// Notes the autoplayer misses or presses off, before the start and
    /// after it.
    pub touches: Vec<Touch>,
    /// The combo at the start: at least this (the notes before it may
    /// make more), given where they make less.
    pub combo: u32,
    /// Seconds since the announcer's last gated line at the start: its
    /// periodic comment comes once [`crate::announcer::COMMENT_GAP`] has
    /// passed (a combo's line once [`crate::announcer::COMBO_GAP`] has).
    pub since: f64,
    /// The song second the scene is over at (`SessionEvent::SceneOver`):
    /// `None` plays on to the end, into the results.
    pub until: Option<f64>,
    /// The song second the player restarts at (R twice).
    pub restart: Option<f64>,
    /// The line the scene is for: the example, what else is said around
    /// it quieter.
    pub example: Option<Line>,
}

/// Notes whose judged second is in `from..to` pressed as `how` says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touch {
    pub from: f64,
    pub to: f64,
    pub how: How,
}

/// How a touched note is played.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum How {
    /// Not pressed at all.
    Miss,
    /// Pressed this many seconds off (into a lower window).
    Off(f64),
}

impl Touch {
    pub fn miss(from: f64, to: f64) -> Touch {
        Touch {
            from,
            to,
            how: How::Miss,
        }
    }

    pub fn off(from: f64, to: f64, seconds: f64) -> Touch {
        Touch {
            from,
            to,
            how: How::Off(seconds),
        }
    }
}

/// The autoplayer's `presses` (`autoplay::script`), each at a note's
/// judged second, as `touches` say (the first that covers a press):
/// missed ones left out, others moved, held as long.
pub fn touched(presses: Vec<Press>, touches: &[Touch]) -> Vec<Press> {
    let mut out: Vec<Press> = presses
        .into_iter()
        .filter_map(|p| {
            let how = touches
                .iter()
                .find(|t| p.press >= t.from && p.press < t.to)
                .map(|t| t.how);
            match how {
                None => Some(p),
                Some(How::Miss) => None,
                Some(How::Off(d)) => Some(Press {
                    press: p.press + d,
                    release: p.release + d,
                    lane: p.lane,
                }),
            }
        })
        .collect();
    out.sort_by(|a, b| a.press.total_cmp(&b.press));
    out
}
