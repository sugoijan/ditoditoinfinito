//! Watching a stage (`watch-v0`,
//! `docs/plans/multiplayer-prototype-progress.md`, "Watching a stage"):
//! what a session keeps of someone who watches rather than plays. A
//! spectator (a member who chose Watch: no field of their own, everyone's
//! fields over the whole screen, or one competitor followed, shown large),
//! and a player out of a stage that goes on between the others (their
//! life ran out, or they left its play: a forfeit), who watches the rest
//! of it from their own dimmed field until it is decided, the song ends
//! or they skip to the results.
//!
//! Nothing here judges anything: the fields come from the competitors'
//! streams of judged moments (`crate::battle::feed`), the stage's outcome
//! from its battle.

use ddi_engine::bindings::MenuInput;
use ddi_platform::{DeviceId, RawInput};

use crate::battle::WatchMoment;

/// The control a tap or a click on a watched field sends: `watch:<seat>`
/// follows that competitor, [`TOUCH_WATCH_ALL`] goes back to every field.
pub const TOUCH_WATCH_CONTROL: &str = "watch:";
pub const TOUCH_WATCH_ALL: &str = "watch:all";
/// The seat [`TOUCH_WATCH_CONTROL`] names for a tap on a paged grid's
/// ladder: the next page ([`GRID_PAGE`]).
pub const TOUCH_PAGE_SEAT: usize = usize::MAX;
/// The control a tap on "Skip to results" sends.
pub const TOUCH_SKIP_CONTROL: &str = "skip";

/// PROVISIONAL (`watch-v1`, 2026-11-13; `watch-v0` ended it this long
/// after the decision): a watched stage goes on after it is decided while
/// anyone still plays its song (the winner finishing it, anyone playing
/// it out; the decision said on the status line as it comes, and kept),
/// and is shown this long after the last of them ended before its
/// results, the music fading over as long as a fail's (or at the song's
/// end); how long the grid's hint shows; how long a moment stays in the
/// status line.
pub const WATCH_END_HOLD: f64 = 2.5;
pub const WATCH_END_FADE: f64 = super::FAIL_FADE;
pub const WATCH_HINT_SECONDS: f64 = 6.0;
pub const WATCH_CUE_SECONDS: f64 = 2.5;

/// PROVISIONAL (`watch-pages-v0`, 2026-11-13): a spectator's grid shows at
/// most this many fields a page, by standing; past it, Page Up and Page
/// Down (a pad's D-pad Up and Down, which mean nothing else there: its
/// shoulders follow a competitor) turn the pages.
pub const GRID_PAGE: usize = 12;

/// Why the stage is watched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Watching {
    /// Never in it: a member who chose Watch.
    Spectator,
    /// A player whose life ran out.
    Out,
    /// A player who left the stage's play while still in (a forfeit).
    Left,
}

/// What a frame shows of the watching ([`super::PlaySession::watch`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WatchView {
    pub why: Watching,
    /// The competitor a spectator follows (their seat: shown large, the
    /// others beside); none: every field (the grid), or a player's own
    /// dimmed field.
    pub focus: Option<usize>,
    /// Host seconds since the watching began.
    pub age: f64,
    /// The stage's latest moment for the status line, and how old it is.
    pub cue: Option<(WatchMoment, f64)>,
    /// Nobody plays the stage any more (decided, the winner done): its
    /// results follow.
    pub over: bool,
    /// The grid's page ([`GRID_PAGE`] fields a page, by standing), from 0.
    pub page: usize,
    /// The seat it is watched through ended (an audience's: the audience
    /// closed, the room ended, its link replaced): the view stopped, why
    /// is said over it, and Enter, Start or a tap goes back.
    pub ended: bool,
}

/// What an edge asks of the watching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ask {
    /// The next (or previous) competitor followed, the grid past the last.
    Cycle(i32),
    /// That competitor followed; none: the grid.
    Focus(Option<usize>),
    /// The grid's next (or previous) page, round.
    Page(i32),
    /// The results now.
    Skip,
}

/// A session's watching.
#[derive(Clone, Debug)]
pub(super) struct Watch {
    pub why: Watching,
    pub focus: Option<usize>,
    /// Host seconds it began at.
    pub since: f64,
    /// Host seconds the stage was seen decided with nobody playing it any
    /// more.
    pub over_at: Option<f64>,
    pub skip: bool,
    shift: bool,
    pub cue: Option<(WatchMoment, f64)>,
    pub page: usize,
    /// The seat ended ([`WatchView::ended`]).
    pub ended: bool,
}

impl Watch {
    pub fn new(why: Watching, host: f64) -> Watch {
        Watch {
            why,
            focus: None,
            since: host,
            over_at: None,
            skip: false,
            shift: false,
            cue: None,
            page: 0,
            ended: false,
        }
    }

    /// What `raw` asks: Tab and Shift+Tab or a pad's shoulders cycle the
    /// focus (a spectator's round the seats and the grid, a player out of
    /// the stage's round the seats and their own field), a tap on a field
    /// follows it; Enter, a pad's Start or South, or a tap on its button
    /// skips a stage the player is out of to its results. `menu`: the
    /// edge as a menu reads a pad's.
    pub fn read(&mut self, raw: &RawInput, menu: Option<MenuInput>) -> Option<Ask> {
        let spectator = self.why == Watching::Spectator;
        // Who may skip (to the results, or back where the seat ended).
        let skips = !spectator || self.ended;
        match &raw.device {
            DeviceId::Keyboard => {
                if matches!(raw.control.as_str(), "ShiftLeft" | "ShiftRight") {
                    self.shift = raw.pressed;
                    return None;
                }
                if !raw.pressed {
                    return None;
                }
                match raw.control.as_str() {
                    "Tab" => Some(Ask::Cycle(if self.shift { -1 } else { 1 })),
                    "PageDown" if spectator => Some(Ask::Page(1)),
                    "PageUp" if spectator => Some(Ask::Page(-1)),
                    "Enter" | "NumpadEnter" if skips => Some(Ask::Skip),
                    _ => None,
                }
            }
            DeviceId::Touch if raw.pressed => {
                if raw.control == TOUCH_SKIP_CONTROL {
                    return skips.then_some(Ask::Skip);
                }
                if raw.control == TOUCH_WATCH_ALL {
                    return Some(Ask::Focus(None));
                }
                let seat = raw.control.strip_prefix(TOUCH_WATCH_CONTROL)?;
                match seat.parse().ok()? {
                    TOUCH_PAGE_SEAT if spectator => Some(Ask::Page(1)),
                    TOUCH_PAGE_SEAT => None,
                    s => Some(Ask::Focus(Some(s))),
                }
            }
            DeviceId::Gamepad(_) if raw.pressed => match menu? {
                MenuInput::Previous => Some(Ask::Cycle(-1)),
                MenuInput::Next => Some(Ask::Cycle(1)),
                MenuInput::Down if spectator => Some(Ask::Page(1)),
                MenuInput::Up if spectator => Some(Ask::Page(-1)),
                MenuInput::Confirm | MenuInput::Start if skips => Some(Ask::Skip),
                _ => None,
            },
            _ => None,
        }
    }

    /// Does what was asked, among `seats` competitors.
    pub fn apply(&mut self, ask: Ask, seats: usize) {
        match ask {
            Ask::Skip => self.skip = true,
            Ask::Focus(seat) => self.focus = seat.filter(|s| *s < seats),
            // Pages turn on the grid alone (following someone, the
            // grid's page is kept for coming back to it).
            Ask::Page(by) if self.focus.is_none() => {
                let pages = seats.div_ceil(GRID_PAGE).max(1) as i32;
                self.page = (self.page as i32 + by).rem_euclid(pages) as usize;
            }
            Ask::Page(_) => {}
            Ask::Cycle(by) => {
                // The grid, then each seat, then the grid again.
                let places = seats as i32 + 1;
                let at = self.focus.map_or(0, |s| s as i32 + 1);
                let next = (at + by).rem_euclid(places);
                self.focus = (next > 0).then(|| (next - 1) as usize);
            }
        }
    }

    /// A moment of the stage for the status line, at host second `host`:
    /// it takes the line unless a moment that matters more still shows
    /// (the stage's decision over someone going out, that over an
    /// attack).
    pub fn tell(&mut self, moment: WatchMoment, host: f64) {
        let weight = |m: &WatchMoment| match m {
            WatchMoment::Won { .. } | WatchMoment::Drawn => 3,
            WatchMoment::Out { .. } | WatchMoment::LastStanding { .. } => 2,
            WatchMoment::Attack { .. } => 1,
        };
        let kept = self
            .cue
            .as_ref()
            .is_some_and(|(m, at)| host - at < WATCH_CUE_SECONDS && weight(m) > weight(&moment));
        if !kept {
            self.cue = Some((moment, host));
        }
    }

    pub fn view(&self, host: f64) -> WatchView {
        WatchView {
            why: self.why,
            focus: self.focus,
            age: host - self.since,
            // The decision stays to the end, and who is last standing
            // until it comes.
            cue: self.cue.map(|(m, at)| (m, host - at)).filter(|(m, age)| {
                *age < WATCH_CUE_SECONDS
                    || matches!(
                        m,
                        WatchMoment::Won { .. }
                            | WatchMoment::Drawn
                            | WatchMoment::LastStanding { .. }
                    )
            }),
            over: self.over_at.is_some(),
            page: self.page,
            ended: self.ended,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_platform::HostTime;

    fn key(code: &str, pressed: bool) -> RawInput {
        RawInput {
            device: DeviceId::Keyboard,
            control: code.into(),
            pressed,
            host_time: HostTime(0.0),
            slot: 0,
        }
    }

    #[test]
    fn tab_goes_round_the_seats_and_back_to_the_grid() {
        let mut w = Watch::new(Watching::Spectator, 0.0);
        let mut seen = Vec::new();
        for _ in 0..5 {
            let ask = w.read(&key("Tab", true), None).unwrap();
            w.apply(ask, 3);
            seen.push(w.focus);
            assert_eq!(w.read(&key("Tab", false), None), None);
        }
        assert_eq!(seen, [Some(0), Some(1), Some(2), None, Some(0)]);
        // Shift+Tab the other way, the grid before the first.
        w.read(&key("ShiftLeft", true), None);
        for want in [None, Some(2), Some(1)] {
            let ask = w.read(&key("Tab", true), None).unwrap();
            w.apply(ask, 3);
            assert_eq!(w.focus, want);
        }
        w.read(&key("ShiftLeft", false), None);
        // A tap on a field, and back; a seat that is not there is none.
        let tap = |control: &str| RawInput {
            device: DeviceId::Touch,
            control: control.into(),
            pressed: true,
            host_time: HostTime(0.0),
            slot: 0,
        };
        let ask = w.read(&tap("watch:2"), None).unwrap();
        w.apply(ask, 3);
        assert_eq!(w.focus, Some(2));
        let ask = w.read(&tap(TOUCH_WATCH_ALL), None).unwrap();
        w.apply(ask, 3);
        assert_eq!(w.focus, None);
        w.apply(Ask::Focus(Some(7)), 3);
        assert_eq!(w.focus, None);
        // A spectator has nothing to skip; Enter is not theirs.
        assert_eq!(w.read(&key("Enter", true), None), None);
        assert_eq!(w.read(&tap(TOUCH_SKIP_CONTROL), None), None);
        // Until the seat they watch through ends: then it goes back.
        w.ended = true;
        assert_eq!(w.read(&key("Enter", true), None), Some(Ask::Skip));
        assert_eq!(w.read(&tap(TOUCH_SKIP_CONTROL), None), Some(Ask::Skip));
        assert!(w.view(0.0).ended);
    }

    #[test]
    fn page_keys_and_the_d_pad_turn_the_grids_pages() {
        let mut w = Watch::new(Watching::Spectator, 0.0);
        // A room of nine: one page, nothing turns.
        let ask = w.read(&key("PageDown", true), None).unwrap();
        w.apply(ask, 8);
        assert_eq!(w.page, 0);
        // 72 competitors: six pages of twelve, round.
        for want in [1, 2, 3, 4, 5, 0] {
            let ask = w.read(&key("PageDown", true), None).unwrap();
            w.apply(ask, 72);
            assert_eq!(w.page, want);
        }
        let ask = w.read(&key("PageUp", true), None).unwrap();
        w.apply(ask, 72);
        assert_eq!(w.view(0.0).page, 5);
        let pad = RawInput {
            device: DeviceId::Gamepad("pad".into()),
            control: "button:13".into(),
            pressed: true,
            host_time: HostTime(0.0),
            slot: 0,
        };
        assert_eq!(w.read(&pad, Some(MenuInput::Down)), Some(Ask::Page(1)));
        assert_eq!(w.read(&pad, Some(MenuInput::Up)), Some(Ask::Page(-1)));
        // The shoulders still follow a competitor; while one is followed
        // the page stays.
        assert_eq!(w.read(&pad, Some(MenuInput::Next)), Some(Ask::Cycle(1)));
        w.apply(Ask::Focus(Some(3)), 72);
        w.apply(Ask::Page(1), 72);
        assert_eq!(w.page, 5);
        // A tap on the ladder turns the page.
        let tap = RawInput {
            device: DeviceId::Touch,
            control: format!("{TOUCH_WATCH_CONTROL}{TOUCH_PAGE_SEAT}"),
            pressed: true,
            host_time: HostTime(0.0),
            slot: 0,
        };
        assert_eq!(w.read(&tap, None), Some(Ask::Page(1)));
        // A player out of the stage has no pages.
        let mut out = Watch::new(Watching::Out, 0.0);
        assert_eq!(out.read(&key("PageDown", true), None), None);
    }

    #[test]
    fn a_player_out_of_the_stage_skips_and_follows_one_large() {
        for why in [Watching::Out, Watching::Left] {
            let mut w = Watch::new(why, 0.0);
            // Tab round the seats and back to their own field; Shift+Tab
            // the other way; a tap on a field, and on their own.
            let mut seen = Vec::new();
            for _ in 0..4 {
                let ask = w.read(&key("Tab", true), None).unwrap();
                w.apply(ask, 3);
                seen.push(w.focus);
            }
            assert_eq!(seen, [Some(0), Some(1), Some(2), None]);
            w.read(&key("ShiftLeft", true), None);
            let ask = w.read(&key("Tab", true), None).unwrap();
            w.apply(ask, 3);
            assert_eq!(w.focus, Some(2));
            w.read(&key("ShiftLeft", false), None);
            let tap = |control: &str| RawInput {
                device: DeviceId::Touch,
                control: control.into(),
                pressed: true,
                host_time: HostTime(0.0),
                slot: 0,
            };
            let ask = w.read(&tap("watch:1"), None).unwrap();
            w.apply(ask, 3);
            assert_eq!(w.focus, Some(1));
            let ask = w.read(&tap(TOUCH_WATCH_ALL), None).unwrap();
            w.apply(ask, 3);
            assert_eq!(w.focus, None);
            // No pages.
            assert_eq!(w.read(&key("PageDown", true), None), None);
            let ask = w.read(&key("Enter", true), None).unwrap();
            assert_eq!(ask, Ask::Skip);
            w.apply(ask, 3);
            assert!(w.skip);
            let pad = RawInput {
                device: DeviceId::Gamepad("pad".into()),
                control: "button:9".into(),
                pressed: true,
                host_time: HostTime(0.0),
                slot: 0,
            };
            assert_eq!(w.read(&pad, Some(MenuInput::Start)), Some(Ask::Skip));
            assert_eq!(w.read(&pad, Some(MenuInput::Next)), Some(Ask::Cycle(1)));
            assert_eq!(w.read(&pad, Some(MenuInput::Previous)), Some(Ask::Cycle(-1)));
        }
    }

    #[test]
    fn the_decision_takes_the_line_and_keeps_it() {
        let mut w = Watch::new(Watching::Spectator, 0.0);
        w.tell(WatchMoment::Out { seat: 1 }, 10.0);
        // An attack does not take the line from someone going out.
        w.tell(WatchMoment::Attack { from: 0, to: 2 }, 10.5);
        assert_eq!(w.view(11.0).cue.unwrap().0, WatchMoment::Out { seat: 1 });
        // Once it has had its time, the next moment shows.
        w.tell(WatchMoment::Attack { from: 0, to: 2 }, 13.0);
        assert_eq!(
            w.view(13.1).cue.unwrap().0,
            WatchMoment::Attack { from: 0, to: 2 }
        );
        assert_eq!(w.view(16.0).cue, None);
        w.tell(WatchMoment::Won { seat: 0 }, 20.0);
        assert_eq!(w.view(40.0).cue.unwrap().0, WatchMoment::Won { seat: 0 });
    }
}
