//! A multiplayer stage's CPU opponents beside the field
//! (`docs/plans/multiplayer-prototype-progress.md`, M3;
//! `docs/plans/multiplayer-ui.md`, "Play and spectator layout"): a quiet
//! column in the side space, drawn by the play's 2D pass over the frame.
//! One stable slot per opponent in the order the room seated them, each
//! with its name and level, its life (its own engine-judged gauge) and a
//! quiet Out once it is gone; never moved when one is out. A heading says
//! the CPUs play a tap-only copy of the chart, and "Stage won" shows once
//! the player has won the stage before the song's end. A battle's moment
//! that no announcer line said (`ddi_session::announcer::battle`: the
//! stage's opening, life critical, the players left, out) takes the
//! heading's place for a few seconds, fading in and out (at once under
//! reduced motion); with bars only there is no room for it. Nothing else
//! moves but the bars. The faithful miniature fields of the design are
//! later work.
//!
//! The column goes on the side of the field with more room, clear of the
//! HUD's areas; on a narrow side only the numbered bars, on none at all
//! nothing (it never covers the field).

use ddi_actor::text::{TextMeasure, TextStyle};
use ddi_render::FieldGeometry;
use ddi_render::draw2d::{Affine, Blend, Command, DrawList, Mask, Quad, Shape, TextRun};
use ddi_session::battle::OpponentView;

/// The theme's text face (`crates/theme/src/infinito.toml`), with the
/// language's faces after it ([`ddi_l10n::font_stack`]).
const FACE: &str = "Iosevka Custom, Iosevka Term, monospace";

/// The column's width, logical pixels, at most, and the least a side
/// must have for names (else bars only), and for bars.
const WIDE: f32 = 172.0;
const NAMES: f32 = 96.0;
const BARS: f32 = 20.0;

const TEXT: [f32; 4] = [0.92, 0.92, 0.95, 1.0];
const MUTED: [f32; 4] = [0.72, 0.72, 0.78, 1.0];
const PANEL: [f32; 4] = [0.02, 0.01, 0.05, 0.55];
const TRACK: [f32; 4] = [1.0, 1.0, 1.0, 0.14];
/// Life: the HUD's mint, and its coral in danger.
const LIFE: [f32; 4] = [0.45, 0.86, 0.72, 0.95];
const LOW: [f32; 4] = [0.96, 0.49, 0.42, 0.95];
const WON: [f32; 4] = [0.98, 0.84, 0.45, 1.0];

/// What the column shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Column {
    pub opponents: Vec<OpponentView>,
    /// "CPUs · taps-only copy", "Out" and "Stage won", worded.
    pub heading: String,
    pub out: String,
    pub won: Option<String>,
    /// A battle's moment shown in the heading's place, worded, and how
    /// opaque, `0..=1`.
    pub cue: Option<(String, f32)>,
}

/// Where it went, for tests and the debug state: the column's rectangle
/// in the target's pixels (`[x, y, width, height]`) and whether it has
/// names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub rect: [f32; 4],
    pub names: bool,
    /// A moment's cue was drawn.
    pub cue: bool,
}

/// Draws `column` beside the field `geo` (`span` lanes wide) at `ratio`
/// target pixels to a logical one, clear of `keep_out` (`[left, top,
/// right, bottom]`); where it went, if anywhere.
pub fn draw(
    list: &mut DrawList,
    column: &Column,
    geo: &FieldGeometry,
    span: f32,
    ratio: f32,
    keep_out: &[[f32; 4]],
    measure: &mut dyn TextMeasure,
) -> Option<Placed> {
    let n = column.opponents.len();
    if n == 0 {
        return None;
    }
    let field = [geo.left, geo.left + geo.arrow * span];
    let left_room = field[0];
    let right_room = geo.width - field[1];
    let on_right = right_room >= left_room;
    let room = left_room.max(right_room) / ratio;
    if room < BARS {
        return None;
    }
    let names = room >= NAMES;
    let gap = 12.0_f32.min(room * 0.1);
    let w = (room - 2.0 * gap).min(WIDE);
    let row = if names { 34.0 } else { 14.0 };
    let head = if names { 22.0 } else { 0.0 } + if column.won.is_some() { 20.0 } else { 0.0 };
    let pad = if names { 10.0 } else { 4.0 };
    let h = head + row * n as f32 + 2.0 * pad;
    let x = if on_right {
        field[1] / ratio + gap
    } else {
        field[0] / ratio - gap - w
    };
    // In the middle of the screen's height, moved below what it would
    // cover of the HUD.
    let mut y = (geo.height / ratio - h) / 2.0;
    for k in keep_out {
        let k = k.map(|v| v / ratio);
        if x < k[2] && x + w > k[0] && y < k[3] && y + h > k[1] {
            y = k[3] + 8.0;
        }
    }
    let px = |v: f32| v * ratio;
    let rect = [px(x), px(y), px(w), px(h)];
    let mut panel = Quad::plain(
        [rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]],
        PANEL,
    );
    panel.shape = Shape {
        radius: [8.0 / w, 8.0 / h],
        ..Shape::NONE
    };
    list.push(Command::Quad(panel));
    let face = ddi_l10n::font_stack(FACE);
    let style = |size: f32, weight: u16| {
        let mut s = TextStyle::new(face.as_str(), px(size));
        s.weight = weight;
        s
    };
    let mut text =
        |list: &mut DrawList, s: &str, style: &TextStyle, at: [f32; 2], right: bool, color| {
            let mut s = s.to_string();
            let room = px(w - 2.0 * pad);
            // Cut to the room, an ellipsis at the end.
            while !s.is_empty() && measure.measure(&s, style).advance > room {
                s.pop();
                while s.ends_with(' ') {
                    s.pop();
                }
                if s.is_empty() {
                    break;
                }
                s.pop();
                s.push('…');
            }
            let advance = measure.measure(&s, style).advance;
            let x = if right { at[0] - advance } else { at[0] };
            list.push(Command::Text(TextRun {
                curve: None,
                text: s.into(),
                style: style.clone(),
                transform: Affine::translate(x, at[1]),
                color,
                blend: Blend::Normal,
                mask: Mask::None,
                perspective: None,
                blur: [0.0; 2],
            }));
        };
    let (x0, x1) = (x + pad, x + w - pad);
    let mut top = y + pad;
    if let Some(won) = &column.won {
        text(list, won, &style(12.0, 700), [px(x0), px(top)], false, WON);
        top += 20.0;
    }
    let mut cued = false;
    if names {
        // The cue alone while it shows (fading in and out), never over
        // the heading.
        match column.cue.as_ref().filter(|(_, a)| *a > 0.0) {
            None => text(
                list,
                &column.heading,
                &style(10.5, 400),
                [px(x0), px(top)],
                false,
                MUTED,
            ),
            Some((cue, alpha)) => {
                let mut color = TEXT;
                color[3] *= alpha.clamp(0.0, 1.0);
                text(
                    list,
                    cue,
                    &style(11.5, 600),
                    [px(x0), px(top - 1.0)],
                    false,
                    color,
                );
                cued = true;
            }
        }
        top += 22.0;
    }
    for (i, o) in column.opponents.iter().enumerate() {
        let ry = top + row * i as f32;
        let dim = if o.out { 0.5 } else { 1.0 };
        let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * dim];
        let bar_y = if names { ry + 18.0 } else { ry + 4.0 };
        let mut bar_x = x0;
        if names {
            text(
                list,
                &o.name,
                &style(12.0, 600),
                [px(x0), px(ry)],
                false,
                fade(TEXT),
            );
            let right = if o.out { &column.out } else { &o.level };
            text(
                list,
                right,
                &style(11.0, 400),
                [px(x1), px(ry + 1.0)],
                true,
                fade(MUTED),
            );
        } else {
            // Bars only: the CPU's number before its bar.
            let number = (i + 1).to_string();
            text(
                list,
                &number,
                &style(10.0, 600),
                [px(x0), px(ry - 3.0)],
                false,
                fade(MUTED),
            );
            bar_x = x0 + 9.0;
        }
        let bw = (x1 - bar_x).max(1.0);
        list.rect(
            [px(bar_x), px(bar_y), px(bar_x + bw), px(bar_y + 4.0)],
            TRACK,
        );
        let life = o.life.clamp(0.0, 1.0);
        if life > 0.0 && !o.out {
            let color = if life < 0.25 { LOW } else { LIFE };
            list.rect(
                [px(bar_x), px(bar_y), px(bar_x + bw * life), px(bar_y + 4.0)],
                color,
            );
        }
    }
    Some(Placed {
        rect,
        names,
        cue: cued,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_actor::text::FixedMeasure;
    use ddi_room::ParticipantId;

    fn column(n: usize) -> Column {
        Column {
            opponents: (0..n)
                .map(|i| OpponentView {
                    participant: ParticipantId(i as u64 + 2),
                    name: format!("CPU {}", i + 1),
                    level: "Lv 3".into(),
                    life: 0.5,
                    out: i == 1,
                    score: 0.9,
                    combo: 10,
                })
                .collect(),
            heading: "CPUs · taps-only copy".into(),
            out: "Out".into(),
            won: None,
            cue: None,
        }
    }

    fn texts(list: &DrawList) -> Vec<(String, f32)> {
        list.commands
            .iter()
            .filter_map(|c| match c {
                Command::Text(t) => Some((t.text.to_string(), t.color[3])),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_moment_takes_the_heading_s_place_while_it_shows() {
        let g = geo(1280.0, 720.0);
        let plain = draw(
            &mut DrawList::new(),
            &column(3),
            &g,
            4.0,
            1.0,
            &[],
            &mut FixedMeasure,
        )
        .unwrap();
        assert!(!plain.cue);
        // Shown whole: the heading gone, the column where it was.
        let mut c = column(3);
        c.cue = Some(("Final stage".into(), 1.0));
        let mut list = DrawList::new();
        let placed = draw(&mut list, &c, &g, 4.0, 1.0, &[], &mut FixedMeasure).unwrap();
        assert!(placed.cue);
        assert_eq!(placed.rect, plain.rect);
        let t = texts(&list);
        assert!(t.iter().any(|(s, a)| s == "Final stage" && *a > 0.9));
        assert!(!t.iter().any(|(s, _)| s.starts_with("CPUs")));
        // Fading: the cue alone, partly.
        c.cue = Some(("Final stage".into(), 0.5));
        let mut list = DrawList::new();
        draw(&mut list, &c, &g, 4.0, 1.0, &[], &mut FixedMeasure).unwrap();
        let t = texts(&list);
        assert!(t.iter().any(|(s, a)| s == "Final stage" && *a < 0.9));
        assert!(!t.iter().any(|(s, _)| s.starts_with("CPUs")));
        // Gone: the heading back.
        c.cue = Some(("Final stage".into(), 0.0));
        let mut list = DrawList::new();
        assert!(
            !draw(&mut list, &c, &g, 4.0, 1.0, &[], &mut FixedMeasure)
                .unwrap()
                .cue
        );
        assert!(texts(&list).iter().any(|(s, _)| s.starts_with("CPUs")));
    }

    fn geo(w: f32, h: f32) -> FieldGeometry {
        FieldGeometry::placed(w, h, 4.0, false, Default::default())
    }

    #[test]
    fn beside_the_field_never_over_it() {
        let g = geo(1280.0, 720.0);
        let mut list = DrawList::new();
        let placed = draw(&mut list, &column(3), &g, 4.0, 1.0, &[], &mut FixedMeasure).unwrap();
        assert!(placed.names);
        let [x, _, w, _] = placed.rect;
        let field = [g.left, g.left + g.arrow * 4.0];
        assert!(x >= field[1] || x + w <= field[0], "{placed:?} {field:?}");
        // The Out one says so instead of its level; the others their level.
        let texts: Vec<String> = list
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::Text(t) => Some(t.text.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(texts.iter().filter(|t| *t == "Out").count(), 1);
        assert_eq!(texts.iter().filter(|t| *t == "Lv 3").count(), 2);
        assert!(texts.iter().any(|t| t.starts_with("CPUs")));
    }

    #[test]
    fn a_phone_shows_bars_or_nothing() {
        // Upright: little room either side of the field.
        let g = geo(390.0, 844.0);
        let mut list = DrawList::new();
        let placed = draw(&mut list, &column(7), &g, 4.0, 1.0, &[], &mut FixedMeasure);
        if let Some(p) = placed {
            assert!(!p.names);
            let field = [g.left, g.left + g.arrow * 4.0];
            assert!(p.rect[0] >= field[1] || p.rect[0] + p.rect[2] <= field[0]);
        }
        assert!(
            draw(
                &mut DrawList::new(),
                &column(0),
                &g,
                4.0,
                1.0,
                &[],
                &mut FixedMeasure
            )
            .is_none()
        );
    }

    #[test]
    fn it_moves_below_the_hud_it_would_cover() {
        let g = geo(1280.0, 720.0);
        let first = draw(
            &mut DrawList::new(),
            &column(7),
            &g,
            4.0,
            1.0,
            &[],
            &mut FixedMeasure,
        )
        .unwrap();
        let [x, y, w, _] = first.rect;
        let keep = [x, y - 10.0, x + w, y + 30.0];
        let moved = draw(
            &mut DrawList::new(),
            &column(7),
            &g,
            4.0,
            1.0,
            &[keep],
            &mut FixedMeasure,
        )
        .unwrap();
        assert!(moved.rect[1] >= keep[3]);
    }
}
