//! Short labels for the note and scroll options of a play, shown on the
//! results and on the song list.

use ddi_engine::{Appearance, ScrollAction, TimingCut, TransformOptions, Turn};

/// Arrows of the `dance-single` lanes, left to right.
const ARROWS: [&str; 4] = ["←", "↓", "↑", "→"];

/// One label per option that differs from the default, empty when none
/// does. With `lane_map` (`take_from[new_lane] = old_lane`) a shuffle on a
/// four-lane field also shows the resulting order: lane `i` shows the arrow
/// of old lane `take_from[i]`.
pub(crate) fn mods_summary(
    transform: &TransformOptions,
    appearance: Appearance,
    scroll_action: ScrollAction,
    lane_map: Option<&[u8]>,
) -> Vec<String> {
    let mut out = Vec::new();
    match transform.turn {
        Turn::Off => {}
        Turn::Mirror => out.push("mirror".into()),
        Turn::Left => out.push("left".into()),
        Turn::Right => out.push("right".into()),
        Turn::Shuffle => out.push(match lane_map.and_then(shuffle_order) {
            Some(order) => format!("shuffle {order}"),
            None => "shuffle".into(),
        }),
    }
    match appearance {
        Appearance::Visible => {}
        Appearance::Hidden => out.push("hidden".into()),
        Appearance::Sudden => out.push("sudden".into()),
        Appearance::HiddenSudden => out.push("hidden + sudden".into()),
        Appearance::Stealth => out.push("stealth".into()),
    }
    match scroll_action {
        ScrollAction::Normal => {}
        ScrollAction::Boost => out.push("boost".into()),
        ScrollAction::Brake => out.push("brake".into()),
        ScrollAction::Wave => out.push("wave".into()),
    }
    match transform.cut {
        TimingCut::Off => {}
        TimingCut::Quarters => out.push("quarter notes".into()),
        TimingCut::Eighths => out.push("quarters and eighths".into()),
    }
    if transform.no_jumps {
        out.push("no jumps".into());
    }
    if transform.no_holds {
        out.push("no holds".into());
    }
    out
}

/// The arrows of a four-lane field after the lane map, or `None` for other
/// lane counts.
fn shuffle_order(take_from: &[u8]) -> Option<String> {
    if take_from.len() != ARROWS.len() {
        return None;
    }
    take_from
        .iter()
        .map(|&old| ARROWS.get(usize::from(old)).copied())
        .collect()
}
