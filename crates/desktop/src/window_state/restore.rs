//! Putting a window left fullscreen back into fullscreen (round WN2,
//! `docs/plans/desktop.md`, "The window as it was left").
//!
//! macOS refuses a window's `toggleFullScreen:` while a Space switch is
//! under way, with its error sound, and nothing asks again: launched
//! while every display shows another app's fullscreen Space, the new
//! window appears on a desktop Space, macOS animates a switch to it, and
//! a request made at once is lost (the maintainer's run, 2026-11-28). So
//! the request waits until the window is settled ([`Signals::settled`]:
//! focused, visible, on the active Space, nothing of that changed for
//! [`QUIET`]), is confirmed by the window's own state afterwards, and is
//! made again after a growing wait ([`WAITS`]) where it did not take,
//! then given up. It stops at once where it took, where the player
//! changed the window themselves (the fullscreen key, a resize or a move
//! of their own), or where a play starts. Everything is decided from the
//! signals and times the shell gives each turn of its loop ([`Restore::see`]);
//! nothing here waits.

/// How long the window's signals must stay as they are before it is
/// asked (s): a Space switch changes them as it goes.
pub const QUIET: f64 = 0.3;

/// After each request, how long until it is confirmed and, where it did
/// not take, asked again (s); after the last, given up. Four requests in
/// all, over about six seconds: a Space switch takes about one.
pub const WAITS: [f64; 4] = [0.4, 0.8, 1.6, 3.0];

/// How often the signals are read while the window is not settled yet
/// (s): a Space switch may end without an event that wakes the loop.
pub const POLL: f64 = 0.25;

/// A window that never settles is asked anyway this long after it was
/// made, or after a request's wait ended (s): a window behind another
/// app's may never be focused.
pub const SETTLE_LIMIT: f64 = 5.0;

/// The window's own changes this soon after a request are the system's
/// (its transition, a refusal putting the frame back), not the
/// player's (s).
pub const TRANSITION: f64 = 1.5;

/// What the shell reads of the window each turn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Signals {
    /// It has the keyboard's focus (winit's `Focused`).
    pub focused: bool,
    /// It is not occluded (winit's `Occluded`) and is visible.
    pub visible: bool,
    /// It is on the active Space (macOS's `isOnActiveSpace`; true
    /// elsewhere).
    pub on_space: bool,
    /// It is fullscreen, as the window itself says (macOS: its style
    /// mask, not winit's own notion, which a refused request leaves
    /// saying fullscreen).
    pub fullscreen: bool,
    /// A play is on.
    pub playing: bool,
}

impl Signals {
    fn steady(&self) -> (bool, bool, bool) {
        (self.focused, self.visible, self.on_space)
    }
}

/// Why a restore stopped without taking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The player pressed the fullscreen key.
    Toggled,
    /// The player resized or moved the window.
    Frame,
    /// A play started.
    Play,
}

/// What the shell does after [`Restore::see`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Nothing,
    /// Ask the window for fullscreen; `tries` is its number, from 1.
    Ask {
        tries: usize,
    },
    /// It is fullscreen: fullscreen is no longer only wanted.
    Took {
        tries: usize,
    },
    /// Every request was refused: no more are made; fullscreen is still
    /// what was left (the shell keeps it wanted).
    GaveUp {
        tries: usize,
    },
    /// The player chose otherwise ([`Why::Toggled`], [`Why::Frame`]:
    /// fullscreen is no longer wanted), or a play started ([`Why::Play`]:
    /// no more requests, fullscreen still what was left).
    Stopped(Why),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    /// Waiting for the window to settle before the first request.
    Waiting,
    /// Asked `tries` times, the last at `at`.
    Asked { tries: usize, at: f64 },
    /// No more requests (all refused, or a play started); the player may
    /// still choose otherwise.
    GaveUp,
    /// Over.
    Done,
}

/// One restore of fullscreen.
#[derive(Clone, Debug)]
pub struct Restore {
    state: State,
    /// When it began (the window was made).
    began: f64,
    /// The signals last seen, and since when they are so.
    last: Option<(bool, bool, bool)>,
    since: f64,
}

impl Restore {
    /// A window just made at `now`, left fullscreen.
    pub fn new(now: f64) -> Restore {
        Restore {
            state: State::Waiting,
            began: now,
            last: None,
            since: now,
        }
    }

    /// Whether it still acts on what it is told.
    pub fn active(&self) -> bool {
        self.state != State::Done
    }

    /// Whether it may still make a request.
    pub fn asking(&self) -> bool {
        matches!(self.state, State::Waiting | State::Asked { .. })
    }

    /// How many requests were made.
    pub fn tries(&self) -> usize {
        match self.state {
            State::Asked { tries, .. } => tries,
            _ => 0,
        }
    }

    /// The window settled at `now`: steady for [`QUIET`] with focus, in
    /// view and on the active Space.
    fn settled(&self, s: &Signals, now: f64) -> bool {
        s.focused && s.visible && s.on_space && now - self.since >= QUIET
    }

    /// The player pressed the fullscreen key: their choice from now.
    pub fn toggled(&mut self) -> Act {
        self.stop(Why::Toggled)
    }

    /// The window's normal frame changed at `now`: the player's where
    /// the window had settled and no request was made lately (else the
    /// system's: a transition, a refusal, a Space switch).
    pub fn frame_changed(&mut self, s: &Signals, now: f64) -> Act {
        let recent = match self.state {
            State::Asked { at, .. } => now - at < TRANSITION,
            State::Waiting => !self.settled(s, now),
            State::GaveUp => false,
            State::Done => return Act::Nothing,
        };
        if recent {
            Act::Nothing
        } else {
            self.stop(Why::Frame)
        }
    }

    fn stop(&mut self, why: Why) -> Act {
        if self.state == State::Done {
            return Act::Nothing;
        }
        self.state = State::Done;
        Act::Stopped(why)
    }

    /// The loop's turn at `now`, the window as `s`: what to do.
    pub fn see(&mut self, s: &Signals, now: f64) -> Act {
        if self.state == State::Done {
            return Act::Nothing;
        }
        if self.last != Some(s.steady()) {
            self.last = Some(s.steady());
            self.since = now;
        }
        if s.fullscreen {
            let tries = self.tries();
            self.state = State::Done;
            return Act::Took { tries };
        }
        if s.playing && self.asking() {
            // No request over a play (its frames, its keys); fullscreen
            // is still what was left, until the player chooses otherwise.
            self.state = State::GaveUp;
            return Act::Stopped(Why::Play);
        }
        match self.state {
            State::Waiting => {
                if self.settled(s, now) || now - self.began >= SETTLE_LIMIT {
                    self.state = State::Asked { tries: 1, at: now };
                    Act::Ask { tries: 1 }
                } else {
                    Act::Nothing
                }
            }
            State::Asked { tries, at } => {
                let waited = at + WAITS[tries - 1];
                if now < waited {
                    return Act::Nothing;
                }
                // Not taken: again once the window is settled (never into
                // its own transition), or given up after the last.
                if tries == WAITS.len() {
                    self.state = State::GaveUp;
                    return Act::GaveUp { tries };
                }
                if self.settled(s, now) || now - waited >= SETTLE_LIMIT {
                    self.state = State::Asked {
                        tries: tries + 1,
                        at: now,
                    };
                    Act::Ask { tries: tries + 1 }
                } else {
                    Act::Nothing
                }
            }
            State::GaveUp | State::Done => Act::Nothing,
        }
    }

    /// When the loop should come back without an event (the caller's
    /// seconds), if it should.
    pub fn wake(&self, now: f64) -> Option<f64> {
        match self.state {
            State::Waiting => {
                let quiet = self.since + QUIET;
                Some(if quiet > now {
                    quiet.min(now + POLL)
                } else {
                    now + POLL
                })
            }
            State::Asked { tries, at } => {
                let waited = at + WAITS[tries - 1];
                Some(if now < waited { waited } else { now + POLL })
            }
            State::GaveUp | State::Done => None,
        }
    }
}

#[cfg(test)]
mod tests;
