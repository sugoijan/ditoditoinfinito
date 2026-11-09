//! A scrollbar's arithmetic for what scrolls by hand, virtual pixels: the
//! thumb's length and place, and what a press or a drag on it means.

use ddi_actor::{Actor, Kind};

use crate::gesture::Gesture;
use crate::hit;
use crate::look::call;

/// A bar `height` long over `content` scrolled through `range` (more than
/// nothing: a bar with no range is not shown).
pub(crate) struct Scrollbar {
    height: f64,
    /// The thumb's length.
    thumb: f64,
    range: f64,
}

impl Scrollbar {
    /// The thumb the share shown, at least 24 (or the whole bar when
    /// shorter).
    pub(crate) fn new(height: f64, content: f64, range: f64) -> Scrollbar {
        Scrollbar {
            height,
            thumb: (height * height / content.max(1.0)).clamp(24.0_f64.min(height), height),
            range,
        }
    }

    /// `bar`'s `Thumb`, `width` wide, where `scroll` puts it (from the top
    /// of the range).
    pub(crate) fn place_thumb(&self, bar: &mut Actor, width: f32, scroll: f64) {
        let room = self.height - self.thumb;
        let y = (scroll / self.range * room).clamp(0.0, room);
        if let Some(t) = hit::find_mut(bar, "Thumb") {
            if let Kind::Sprite(s) = &mut t.kind {
                s.size = [width, self.thumb as f32];
            }
            call(t, "y", &[y as f32]);
        }
    }

    /// `gesture` at `y` from the bar's top: a press on the track jumps
    /// there, the thumb drags from where it was grabbed (`grabbed`, its
    /// top to the press). The scroll to go to, from the top of the range.
    pub(crate) fn gesture(
        &self,
        gesture: Gesture,
        y: f64,
        scroll: f64,
        grabbed: &mut Option<f64>,
    ) -> Option<f64> {
        let room = self.height - self.thumb;
        let to = |grab: f64| (y - grab).clamp(0.0, room.max(0.0)) / room.max(1.0) * self.range;
        match gesture {
            Gesture::Press => {
                let thumb_y = scroll / self.range * room;
                let grab = if (thumb_y..thumb_y + self.thumb).contains(&y) {
                    y - thumb_y
                } else {
                    self.thumb / 2.0
                };
                *grabbed = Some(grab);
                Some(to(grab))
            }
            Gesture::Drag { .. } | Gesture::DragStart => grabbed.map(to),
            _ => {
                *grabbed = None;
                None
            }
        }
    }
}
