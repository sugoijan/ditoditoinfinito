//! Lane covers: DDR's LANE COVER (Sudden+, Hidden+, both), panels drawn
//! over the lanes that hide the notes beneath them.
//!
//! DanceDanceRevolution X2 brought them from beatmania IIDX (RemyWiki,
//! "DDR AC General Info", Options): Hidden+ is "a lane cover placed at the
//! top (or bottom if using REVERSE)", over the end the notes go to, Sudden+
//! one at the bottom (the top under Reverse), over the end they come from,
//! and both show the notes through a window between them; "adjusted by
//! using the Up and Down navigational buttons", and toggled "off and back
//! on by pressing the Start button during gameplay". StepMania 5.1 has no
//! such panel: its `Cover` option darkens the background
//! (`PlayerOptions::m_fCover`), and `HiddenOffset`/`SuddenOffset` move
//! where Hidden and Sudden fade notes ([`crate::appearance`]).
//!
//! A cover is presentation only: it is drawn over the notes and under the
//! receptors and judgements, and never changes what is judged. Its
//! heights are percentages of the lane, from the receptors' far edge to
//! the end of the field the notes come from (the renderer works out the
//! pixels: `ddi_render::scene`). A play keeps every change made during
//! it in its log ([`crate::history::PlayLog::covers`]), so a replay draws
//! the same.

use serde::{Deserialize, Serialize};

/// Which covers are on (DDR's LANE COVER option).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CoverMode {
    #[default]
    Off,
    /// Over the end the notes come from: they appear late.
    Sudden,
    /// Over the end the notes go to: they vanish before the receptors.
    Hidden,
    /// Both: the notes show through a window between them.
    HiddenSudden,
}

impl CoverMode {
    pub fn sudden(self) -> bool {
        matches!(self, CoverMode::Sudden | CoverMode::HiddenSudden)
    }

    pub fn hidden(self) -> bool {
        matches!(self, CoverMode::Hidden | CoverMode::HiddenSudden)
    }

    /// Its byte in a play log's binary form.
    pub fn to_byte(self) -> u8 {
        match self {
            CoverMode::Off => 0,
            CoverMode::Sudden => 1,
            CoverMode::Hidden => 2,
            CoverMode::HiddenSudden => 3,
        }
    }

    /// The mode of a play log's byte ([`CoverMode::to_byte`]).
    pub fn from_byte(byte: u8) -> Option<CoverMode> {
        Some(match byte {
            0 => CoverMode::Off,
            1 => CoverMode::Sudden,
            2 => CoverMode::Hidden,
            3 => CoverMode::HiddenSudden,
            _ => return None,
        })
    }
}

/// The lane covers and their heights.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct LaneCover {
    #[serde(deserialize_with = "crate::lenient")]
    pub mode: CoverMode,
    /// Percent of the lane Sudden+ covers, from the end the notes come
    /// from.
    pub sudden: u8,
    /// Percent of the lane Hidden+ covers, from the receptors.
    pub hidden: u8,
}

impl Default for LaneCover {
    fn default() -> LaneCover {
        LaneCover {
            mode: CoverMode::Off,
            sudden: LaneCover::DEFAULT_HEIGHT,
            hidden: LaneCover::DEFAULT_HEIGHT,
        }
    }
}

impl LaneCover {
    /// The whole lane.
    pub const FULL: u8 = 100;
    /// The heights a cover starts at.
    pub const DEFAULT_HEIGHT: u8 = 25;

    /// Whether anything is drawn.
    pub fn is_on(self) -> bool {
        self.mode != CoverMode::Off
    }

    /// The parts of the lane covered as drawn, `(hidden, sudden)` in
    /// `0..=1`, nothing for a cover that is off; together never more than
    /// the whole lane (Hidden+ keeps its height, Sudden+ gives way).
    pub fn drawn(self) -> (f32, f32) {
        let hidden = if self.mode.hidden() {
            self.hidden.min(LaneCover::FULL)
        } else {
            0
        };
        let sudden = if self.mode.sudden() {
            self.sudden.min(LaneCover::FULL - hidden)
        } else {
            0
        };
        (
            f32::from(hidden) / f32::from(LaneCover::FULL),
            f32::from(sudden) / f32::from(LaneCover::FULL),
        )
    }

    /// The cover moved `steps` percent towards the receptors (negative:
    /// away): Sudden+ grows, Hidden+ shrinks, and with both the window
    /// between them moves as one, stopping where either reaches its end
    /// (DDR's "shutter window"). Heights of a cover that is off stay.
    pub fn moved(self, steps: i32) -> LaneCover {
        let full = i32::from(LaneCover::FULL);
        let (s, h) = (i32::from(self.sudden), i32::from(self.hidden));
        let (sudden, hidden) = match self.mode {
            CoverMode::Off => (s, h),
            CoverMode::Sudden => ((s + steps).clamp(0, full), h),
            CoverMode::Hidden => (s, (h - steps).clamp(0, full)),
            CoverMode::HiddenSudden => {
                // Both drawn as they are now, then shifted together.
                let h = h.min(full);
                let s = s.min(full - h);
                let d = steps.clamp(-s, h);
                (s + d, h - d)
            }
        };
        LaneCover {
            sudden: sudden as u8,
            hidden: hidden as u8,
            ..self
        }
    }

    /// The heights shown for it, as a play's adjustment shows them and
    /// the mods line says them: "sudden+ 25%", "hidden+ 30%", "hidden+
    /// 30% sudden+ 25%"; `None` when it is off.
    pub fn label(self) -> Option<String> {
        let (hidden, sudden) = self.drawn();
        let pct = |f: f32| (f * 100.0).round() as u32;
        match self.mode {
            CoverMode::Off => None,
            CoverMode::Sudden => Some(format!("sudden+ {}%", pct(sudden))),
            CoverMode::Hidden => Some(format!("hidden+ {}%", pct(hidden))),
            CoverMode::HiddenSudden => {
                Some(format!("hidden+ {}% sudden+ {}%", pct(hidden), pct(sudden)))
            }
        }
    }
}

/// A play's cover as drawn in a frame ([`crate::frame::Frame::cover`]).
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct CoverView {
    pub cover: LaneCover,
    /// Seconds since the play last changed it, while that is recent
    /// enough to show its heights ([`COVER_LABEL_SECONDS`]).
    pub changed: Option<f32>,
}

/// Seconds a cover's heights show after a change.
pub const COVER_LABEL_SECONDS: f32 = 1.5;

/// A change to the cover during a play, at a judged song second (to the
/// microsecond, as a log's edges).
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverChange {
    pub time: f64,
    pub cover: LaneCover,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cover(mode: CoverMode, sudden: u8, hidden: u8) -> LaneCover {
        LaneCover {
            mode,
            sudden,
            hidden,
        }
    }

    #[test]
    fn sudden_grows_and_hidden_shrinks_towards_the_receptors() {
        let s = cover(CoverMode::Sudden, 25, 40);
        assert_eq!(s.moved(5), cover(CoverMode::Sudden, 30, 40));
        assert_eq!(s.moved(-30), cover(CoverMode::Sudden, 0, 40));
        assert_eq!(s.moved(200).sudden, 100);
        let h = cover(CoverMode::Hidden, 25, 40);
        assert_eq!(h.moved(5), cover(CoverMode::Hidden, 25, 35));
        assert_eq!(h.moved(-70).hidden, 100);
        let off = cover(CoverMode::Off, 25, 40);
        assert_eq!(off.moved(5), off);
    }

    #[test]
    fn both_move_as_one_window_and_stop_at_either_end() {
        let b = cover(CoverMode::HiddenSudden, 20, 30);
        assert_eq!(b.moved(10), cover(CoverMode::HiddenSudden, 30, 20));
        // Hidden+ reaches the receptors: the window stops there.
        assert_eq!(b.moved(50), cover(CoverMode::HiddenSudden, 50, 0));
        assert_eq!(b.moved(-50), cover(CoverMode::HiddenSudden, 0, 50));
        // Heights that overlap are drawn as they fit first.
        let over = cover(CoverMode::HiddenSudden, 80, 60);
        assert_eq!(over.moved(0), cover(CoverMode::HiddenSudden, 40, 60));
    }

    #[test]
    fn drawn_parts_never_cover_more_than_the_lane() {
        assert_eq!(cover(CoverMode::Off, 50, 50).drawn(), (0.0, 0.0));
        assert_eq!(cover(CoverMode::Sudden, 50, 50).drawn(), (0.0, 0.5));
        assert_eq!(cover(CoverMode::Hidden, 50, 50).drawn(), (0.5, 0.0));
        assert_eq!(cover(CoverMode::HiddenSudden, 70, 40).drawn(), (0.4, 0.6));
        assert_eq!(
            cover(CoverMode::HiddenSudden, 20, 30).label().as_deref(),
            Some("hidden+ 30% sudden+ 20%")
        );
        assert_eq!(cover(CoverMode::Off, 20, 30).label(), None);
    }

    #[test]
    fn unknown_modes_fall_back_to_off() {
        let c: LaneCover =
            serde_json::from_str(r#"{"mode":"Lifted","sudden":40,"hidden":10}"#).unwrap();
        assert_eq!(c, cover(CoverMode::Off, 40, 10));
        let c: LaneCover = serde_json::from_str("{}").unwrap();
        assert_eq!(c, LaneCover::default());
    }
}
