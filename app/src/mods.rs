//! Short labels for the note and scroll options of a play, shown on the
//! results and on the song list.

use ddi_chart::{Glyph, Layout};
use ddi_engine::{Appearance, ScrollAction, TimingCut, TransformOptions, Turn};

/// One label per option that differs from the default, empty when none
/// does. With `lane_map` (`take_from[new_lane] = old_lane`) and the layout
/// played, a shuffle also shows the resulting order: lane `i` shows the
/// symbol of old lane `take_from[i]`.
pub(crate) fn mods_summary(
    transform: &TransformOptions,
    appearance: Appearance,
    scroll_action: ScrollAction,
    lane_map: Option<(&[u8], &Layout)>,
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

/// The lanes' symbols after the lane map, a space between pads, or `None`
/// when the map does not fit the layout.
fn shuffle_order((take_from, layout): (&[u8], &Layout)) -> Option<String> {
    if take_from.len() != layout.lane_count() {
        return None;
    }
    let mut out = String::new();
    for (new, &old) in take_from.iter().enumerate() {
        if new > 0 && layout.lanes[new].pad != layout.lanes[new - 1].pad {
            out.push(' ');
        }
        out.push_str(symbol(&layout.lanes.get(usize::from(old))?.glyph));
    }
    Some(out)
}

/// A one-character symbol for a lane.
fn symbol(glyph: &Glyph) -> &'static str {
    match glyph {
        Glyph::Arrow { rotation_deg } => {
            // 0 = left, 90 = up, clockwise in 45° steps.
            const ARROWS: [&str; 8] = ["←", "↖", "↑", "↗", "→", "↘", "↓", "↙"];
            let step = (rotation_deg / 45.0).round() as i64;
            ARROWS[step.rem_euclid(8) as usize]
        }
        Glyph::Onigiri => "◉",
        _ => "●",
    }
}
