//! Dancing☆Onigiri's lyrics over a play, drawn by the 2D pass (decision
//! 29): the text through the platform's runs (the browser's on the web,
//! cosmic-text with the system's fonts natively), so Japanese draws.
//! Placed as danoniplus places its `lblword` labels (`mainWindow.js`,
//! "歌詞表示"; `g_lblPosObj.lblWord`): on its stage, mapped onto the screen
//! as the work's back and mask layers are ([`crate::stage`]: the stage's
//! height the screen's, centered on the field), a box 100 px in from either
//! side, its top 10 px down for even depths and 60 px up from the bottom
//! for odd ones; the text flows from the box's top, wrapped to its width
//! and aligned in it; in danoniplus's `title_base` grey, a soft shadow
//! under it for the backgrounds behind (danoniplus's stage is dark). The
//! timing, fades, alignment and sizes are the track's
//! ([`ddi_engine::lyrics`]).

use ddi_actor::text::{TextMeasure, TextStyle};
use ddi_engine::lyrics::{FIELD_HEIGHT, LyricAlign, LyricLine, LyricLook};

use crate::draw2d::{Affine, Blend, Command, DrawList, Mask, TextRun};

/// danoniplus's basic font (`C_LBL_BASICFONT`), then the system's.
pub const FONT: &str = "\"Meiryo UI\", system-ui, sans-serif";

/// danoniplus's `--title-base` (`#cccccc`, its default skin's).
pub const COLOR: [f32; 3] = [0.8, 0.8, 0.8];

/// The label box in the stage's px: in from either side, from the top
/// (even depths) and from the bottom (odd ones).
const SIDE: f32 = 100.0;
const TOP: f32 = 10.0;
const BOTTOM: f32 = 60.0;

/// Where lyrics go on the target: its size, the work's stage (danoniplus's
/// px) centered on the field at `center_x`, what they keep clear of if they
/// can (`keep_out`, `[left, top, right, bottom]` in the target's px: the
/// HUD's gauge and score, the touch quit button), what they never cover
/// (`receptors`: the receptor rows and the lanes' approach near them),
/// and the smallest size a line may shrink to, the target's px.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place {
    pub size: [f32; 2],
    pub stage: (f32, f32),
    pub center_x: f32,
    pub keep_out: Vec<[f32; 4]>,
    pub receptors: Vec<[f32; 4]>,
    pub min_size: f32,
}

impl Place {
    /// Where a play's lyrics go (both shells): the target `geo` (its
    /// pixels, `ratio` of them to a CSS or logical pixel), the field of
    /// `layout` and the work's `stage`; clear of the HUD's gauge and score
    /// and of the quit button where one shows, never over the receptors;
    /// no smaller than 10 px (at `ratio`).
    pub fn for_play(
        geo: &crate::FieldGeometry,
        layout: &ddi_chart::Layout,
        stage: (f32, f32),
        quit: Option<[f32; 3]>,
        ratio: f32,
    ) -> Place {
        let span = crate::scene::field_span(layout);
        let mut keep_out = crate::scene::hud_areas(geo);
        keep_out.extend(quit.map(|[x, y, r]| [x - r, y - r, x + r, y + r]));
        Place {
            size: [geo.width, geo.height],
            stage,
            center_x: geo.center_x(span),
            keep_out,
            receptors: crate::scene::receptor_areas(geo, layout),
            min_size: 10.0 * ratio,
        }
    }
}

/// Room kept between lyrics and what they keep clear of, and from the
/// target's sides, in the target's px per stage px.
const GAP: f32 = 6.0;

/// Draws `lines` at `place`; `measure` the runs as they will be drawn.
///
/// The stage is scaled to fit the target (its height, or its width on a
/// portrait phone, where the height would run it off the sides), top
/// lines from the target's top, bottom lines from its bottom, the box
/// kept on the target. Our adjustments to danoniplus's placement: a line
/// never covers the receptors ([`Place::receptors`]); it keeps clear of
/// the HUD and the quit button too (a top line moves below them, a bottom
/// one narrows past them) while it can, shrinking to
/// [`Place::min_size`] to fit; when even then it cannot, it covers them
/// instead, at its own place by the screen's edge (its shadow keeps it
/// legible).
pub fn draw(
    list: &mut DrawList,
    lines: &[LyricLine],
    place: &Place,
    measure: &mut dyn TextMeasure,
) {
    let (sw, sh) = (place.stage.0.max(1.0), place.stage.1.max(1.0));
    let [w, h] = place.size;
    let s = (h / sh).min(w / sw);
    let gap = GAP * s;
    let box_w = (sw - 2.0 * SIDE).max(SIDE) * s;
    let x0 = (place.center_x - box_w / 2.0).max(gap);
    let x1 = (x0 + box_w).min(w - gap);
    for line in lines {
        let alpha = line.opacity.clamp(0.0, 1.0);
        if alpha <= 0.0 || line.text.is_empty() {
            continue;
        }
        if let LyricLook::StepMania { color, wipe } = line.look {
            draw_stepmania(list, line, color, wipe, place, measure);
            continue;
        }
        let full = (line.size * s).max(1.0);
        let min = place.min_size.min(full).max(1.0);
        // Sizes from its own down to the smallest, a tenth at a time.
        let sizes = (0..10)
            .map(|k| full - (full - min) * k as f32 / 9.0)
            .collect::<Vec<_>>();
        let fit = |size: f32, avoid: bool, tight: bool, measure: &mut dyn TextMeasure| {
            settle(line, size, avoid, tight, place, [x0, x1], gap, s, measure)
        };
        // At its place, else pressed to the screen's edge; each size down
        // to the smallest; else over the HUD, by the edge.
        let laid = sizes
            .iter()
            .find_map(|&size| {
                [false, true].into_iter().find_map(|tight| {
                    let l = fit(size, true, tight, measure);
                    (!l.covers(&place.receptors)).then_some(l)
                })
            })
            .unwrap_or_else(|| fit(min, false, true, measure));
        let Laid {
            size,
            rows,
            x: [bx0, bx1],
            mut y,
            ..
        } = laid;
        let style = TextStyle::new(FONT, size);
        let line_h = style.line_height();
        let box_w = bx1 - bx0;
        let shadow = (size * 0.06).max(1.0);
        for part in rows {
            let advance = measure.measure(&part, &style).advance;
            let x = match line.align {
                LyricAlign::Left => bx0,
                LyricAlign::Center => bx0 + (box_w - advance) / 2.0,
                LyricAlign::Right => bx0 + box_w - advance,
            };
            for (dx, color) in [
                (shadow, [0.0, 0.0, 0.0, 0.6 * alpha]),
                (0.0, [COLOR[0], COLOR[1], COLOR[2], alpha]),
            ] {
                list.push(Command::Text(TextRun {
                    curve: None,
                    text: part.as_str().into(),
                    style: style.clone(),
                    transform: Affine::translate(x + dx, y + dx),
                    color,
                    blend: Blend::Normal,
                    mask: Mask::None,
                    perspective: None,
                    blur: [0.0; 2],
                }));
            }
            y += line_h;
        }
    }
}

/// Where StepMania's default theme puts a lyric's middle, a share of the
/// screen's height down (`LyricDisplaySetNoReverseCommand`,
/// `…SetReverseCommand`: 360 + 260 and 360 − 260 of 720).
const STEPMANIA_Y: [f32; 2] = [620.0 / 720.0, 100.0 / 720.0];
/// Its shadow, straight down (`shadowlengthy(5)` on 720 lines), and the
/// shadow's color (an actor's default, black at half).
const STEPMANIA_SHADOW: f32 = 5.0 / 720.0;

/// A `.lrc` line as StepMania's `LyricDisplay` draws it
/// ([`LyricLook::StepMania`]): centered on the screen at its place, two
/// copies, the dim one (half its color) to the right of the wipe and the
/// bright one (0.9 of it) to its left, the wipe crossing the widest row.
/// StepMania squeezes a line wider than the screen; it shrinks here (to
/// [`Place::min_size`], then wraps). It keeps clear of the HUD like
/// Dancing☆Onigiri's lines, moving away from the screen's edge.
fn draw_stepmania(
    list: &mut DrawList,
    line: &LyricLine,
    color: [f32; 3],
    wipe: f32,
    place: &Place,
    measure: &mut dyn TextMeasure,
) {
    let [w, h] = place.size;
    let alpha = line.opacity.clamp(0.0, 1.0);
    let gap = GAP * h / FIELD_HEIGHT;
    let room = (w - 2.0 * gap).max(1.0);
    let full = (line.size * h / FIELD_HEIGHT).max(1.0);
    let min = place.min_size.min(full).max(1.0);
    let widest = |size: f32, measure: &mut dyn TextMeasure| {
        let style = TextStyle::new(FONT, size);
        line.text
            .split('\n')
            .map(|r| measure.measure(r, &style).advance)
            .fold(0.0, f32::max)
    };
    let size = {
        let at_full = widest(full, measure);
        if at_full <= room {
            full
        } else {
            (full * room / at_full).max(min)
        }
    };
    let style = TextStyle::new(FONT, size);
    let line_h = style.line_height();
    let rows = layout(&line.text, room, &style, measure);
    let advances: Vec<f32> = rows
        .iter()
        .map(|r| measure.measure(r, &style).advance)
        .collect();
    let block_w = advances.iter().copied().fold(0.0, f32::max);
    let block_h = rows.len() as f32 * line_h;
    let mut y = h * STEPMANIA_Y[usize::from(line.top)] - block_h / 2.0;
    y = y.clamp(gap, (h - gap - block_h).max(gap));
    for r in &place.keep_out {
        let block = [(w - block_w) / 2.0, y, (w + block_w) / 2.0, y + block_h];
        if overlaps(&block, r) {
            y = if line.top {
                r[3] + gap
            } else {
                r[1] - gap - block_h
            };
        }
    }
    let left = (w - block_w) / 2.0;
    let split = left + block_w * wipe.clamp(0.0, 1.0);
    let shadow = (h * STEPMANIA_SHADOW).max(1.0);
    for (clip, share) in [([split, 0.0, w, h], 0.5), ([0.0, 0.0, split, h], 0.9)] {
        if clip[2] <= clip[0] {
            continue;
        }
        list.push_clip(clip);
        for (row, (text, advance)) in rows.iter().zip(&advances).enumerate() {
            let x = (w - advance) / 2.0;
            let top = y + row as f32 * line_h;
            for (dy, tint) in [
                (shadow, [0.0, 0.0, 0.0, 0.5 * alpha]),
                (
                    0.0,
                    [color[0] * share, color[1] * share, color[2] * share, alpha],
                ),
            ] {
                list.push(Command::Text(TextRun {
                    curve: None,
                    text: text.as_str().into(),
                    style: style.clone(),
                    transform: Affine::translate(x, top + dy),
                    color: tint,
                    blend: Blend::Normal,
                    mask: Mask::None,
                    perspective: None,
                    blur: [0.0; 2],
                }));
            }
        }
        list.pop_clip();
    }
}

/// A line laid out: its size, rows, box's sides and top.
struct Laid {
    size: f32,
    rows: Vec<String>,
    x: [f32; 2],
    y: f32,
    line_h: f32,
}

impl Laid {
    fn block(&self) -> [f32; 4] {
        [
            self.x[0],
            self.y,
            self.x[1],
            self.y + self.rows.len() as f32 * self.line_h,
        ]
    }

    fn covers(&self, areas: &[[f32; 4]]) -> bool {
        let b = self.block();
        areas.iter().any(|r| overlaps(&b, r))
    }
}

/// `line` at `size` in the box `[x0, x1]`: at danoniplus's place (a
/// bottom line kept on the screen), or pressed to the screen's edge when
/// `tight`; moved clear of `place`'s kept-clear areas when `avoid`.
#[allow(clippy::too_many_arguments)]
fn settle(
    line: &LyricLine,
    size: f32,
    avoid: bool,
    tight: bool,
    place: &Place,
    [x0, x1]: [f32; 2],
    gap: f32,
    s: f32,
    measure: &mut dyn TextMeasure,
) -> Laid {
    let h = place.size[1];
    let style = TextStyle::new(FONT, size);
    let line_h = style.line_height();
    let (mut bx0, mut bx1) = (x0, x1);
    let mut rows = layout(&line.text, bx1 - bx0, &style, measure);
    let edge = |rows: usize| {
        let bottom = h - gap - rows as f32 * line_h;
        match (line.top, tight) {
            (true, false) => TOP * s,
            (true, true) => gap,
            // Its rows flow down from there, on the screen.
            (false, false) => (h - BOTTOM * s).min(bottom),
            (false, true) => bottom,
        }
    };
    let mut y = edge(rows.len());
    // Clear of what it would meet: a few rounds, each one moving it.
    for _ in 0..4 {
        if !avoid {
            break;
        }
        let block = [bx0, y, bx1, y + rows.len() as f32 * line_h];
        let Some(r) = place.keep_out.iter().find(|r| overlaps(&block, r)) else {
            break;
        };
        if line.top {
            y = r[3] + gap;
        } else if (r[0] + r[2]) / 2.0 < (bx0 + bx1) / 2.0 {
            bx0 = r[2] + gap;
        } else {
            bx1 = r[0] - gap;
        }
        if bx1 - bx0 < size * 2.0 {
            break;
        }
        rows = layout(&line.text, bx1 - bx0, &style, measure);
        if !line.top {
            y = y.min(edge(rows.len()));
        }
    }
    Laid {
        size,
        rows,
        x: [bx0, bx1],
        y,
        line_h,
    }
}

/// A line's rows: its line breaks, each wrapped to `width`.
fn layout(text: &str, width: f32, style: &TextStyle, measure: &mut dyn TextMeasure) -> Vec<String> {
    text.split('\n')
        .flat_map(|row| wrap(row, width, style, measure))
        .collect()
}

fn overlaps(a: &[f32; 4], b: &[f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// `text` in lines no wider than `width`, as a browser wraps a label's
/// text: between words, and anywhere in Japanese (each wide character
/// may start a line); a word longer than a line is kept whole.
fn wrap(text: &str, width: f32, style: &TextStyle, measure: &mut dyn TextMeasure) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    for c in text.chars() {
        let wide = (c as u32) >= 0x2E80;
        match tokens.last_mut() {
            Some(t) if !wide && !c.is_whitespace() && !t.ends_with(' ') && !is_wide(t) => t.push(c),
            _ => tokens.push(c.to_string()),
        }
    }
    let mut out = Vec::new();
    let mut line = String::new();
    for t in tokens {
        let candidate = format!("{line}{t}");
        if line.is_empty() || measure.measure(candidate.trim_end(), style).advance <= width {
            line = candidate;
        } else {
            out.push(line.trim_end().to_string());
            line = t.trim_start().to_string();
        }
    }
    if !line.trim().is_empty() || out.is_empty() {
        out.push(line.trim_end().to_string());
    }
    out
}

fn is_wide(t: &str) -> bool {
    t.chars().last().is_some_and(|c| (c as u32) >= 0x2E80)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_actor::text::FixedMeasure;
    use std::rc::Rc;

    fn line(text: &str, top: bool, align: LyricAlign) -> LyricLine {
        LyricLine {
            depth: u8::from(!top),
            top,
            text: Rc::from(text),
            align,
            size: 14.0,
            opacity: 1.0,
            look: LyricLook::Danoni,
        }
    }

    fn runs(list: &DrawList) -> Vec<(&str, [f32; 2], f32, f32)> {
        list.commands
            .iter()
            .filter_map(|c| match c {
                Command::Text(t) if t.color[0] > 0.5 => Some((
                    &*t.text,
                    [t.transform.0[4], t.transform.0[5]],
                    t.style.size(),
                    t.color[3],
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn lines_sit_where_danoniplus_puts_its_labels() {
        // A 500 px stage shown 1000 px tall around x = 800: 2 px a px.
        let mut list = DrawList::new();
        let lines = [
            line("hello", true, LyricAlign::Left),
            line("below", false, LyricAlign::Right),
        ];
        draw(
            &mut list,
            &lines,
            &Place {
                size: [1600.0, 1000.0],
                stage: (500.0, 500.0),
                center_x: 800.0,
                keep_out: Vec::new(),
                receptors: Vec::new(),
                min_size: 10.0,
            },
            &mut FixedMeasure,
        );
        let r = runs(&list);
        // The stage spans 300–1300; the box, its x 100–400, 500–1100.
        assert_eq!(r[0], ("hello", [500.0, 20.0], 28.0, 1.0));
        // Odd depths 60 px up from the bottom; right aligned in the box.
        let width = 5.0 * 0.5 * 28.0;
        assert_eq!(r[1].1, [1100.0 - width, 880.0]);
        // Phones upright: lines stay on the screen and never cover the
        // receptors; the gauge, the score and the quit button they keep
        // clear of where they can.
        for (size, reverse) in [
            ([390.0, 844.0], false),
            ([390.0, 844.0], true),
            ([360.0, 740.0], false),
            ([360.0, 740.0], true),
        ] {
            let layout = ddi_chart::Layout::dance_single();
            let geo = crate::FieldGeometry::new(size[0], size[1], 4.0, reverse);
            let quit = crate::scene::quit_button(&geo, &layout, 1.0, [0.0; 4]);
            let phone = Place::for_play(&geo, &layout, (500.0, 500.0), Some(quit), 1.0);
            let mut list = DrawList::new();
            draw(&mut list, &lines, &phone, &mut FixedMeasure);
            let r = runs(&list);
            assert_eq!(r.len(), 2, "{size:?}");
            for (text, [x, y], size_px, _) in &r {
                let w = text.chars().count() as f32 * 0.5 * size_px;
                assert!(*x >= 0.0 && x + w <= size[0], "{size:?} {text} at {x}");
                assert!(*size_px >= 10.0 - 0.02, "{size:?} {text} {size_px}");
                let block = [*x, *y, x + w, y + size_px * 1.25];
                for k in &phone.receptors {
                    assert!(
                        !overlaps(&block, k),
                        "{size:?} {reverse} {text} {block:?} on {k:?}"
                    );
                }
                assert!(
                    *y >= 0.0 && y + size_px * 1.25 <= size[1],
                    "{size:?} {text}"
                );
            }
        }
        // A shadow under each.
        assert_eq!(list.commands.len(), 4);
    }

    #[test]
    fn lrc_lines_sit_low_and_wipe_from_the_left() {
        let mut l = line("hello", false, LyricAlign::Center);
        l.size = FIELD_HEIGHT / 30.0;
        l.opacity = 0.75;
        l.look = LyricLook::StepMania {
            color: [0.0, 1.0, 0.0],
            wipe: 0.5,
        };
        let place = Place {
            size: [1280.0, 720.0],
            stage: (500.0, 500.0),
            center_x: 400.0,
            keep_out: Vec::new(),
            receptors: Vec::new(),
            min_size: 10.0,
        };
        let mut list = DrawList::new();
        draw(&mut list, &[l.clone()], &place, &mut FixedMeasure);
        // Two clips (dim right, bright left), a shadow and a run in each.
        let clips: Vec<[f32; 4]> = list
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::PushClip(r) => Some(*r),
                _ => None,
            })
            .collect();
        // 24 px, five characters half as wide: 60 px, centered on 640.
        assert_eq!(
            clips,
            vec![[640.0, 0.0, 1280.0, 720.0], [0.0, 0.0, 640.0, 720.0]]
        );
        let bright = |list: &DrawList| -> Vec<[f32; 2]> {
            list.commands
                .iter()
                .filter_map(|c| match c {
                    Command::Text(t) if t.color[1] > 0.6 => {
                        Some([t.transform.0[4], t.transform.0[5]])
                    }
                    _ => None,
                })
                .collect()
        };
        let r = bright(&list);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0][0], 610.0);
        let middle = r[0][1] + 24.0 * 1.25 / 2.0;
        assert!((middle - 620.0).abs() < 0.5, "{middle}");
        let tints: Vec<[f32; 4]> = list
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::Text(t) if t.color[1] > 0.0 => Some(t.color),
                _ => None,
            })
            .collect();
        assert_eq!(tints, vec![[0.0, 0.5, 0.0, 0.75], [0.0, 0.9, 0.0, 0.75]]);
        // Under Reverse, high; a HUD there pushes it down.
        l.top = true;
        let mut list = DrawList::new();
        let mut hud = place.clone();
        hud.keep_out.push([500.0, 0.0, 800.0, 140.0]);
        draw(&mut list, &[l], &hud, &mut FixedMeasure);
        let y = bright(&list)[0][1];
        assert!(y >= 140.0, "{y}");
    }

    #[test]
    fn long_lines_wrap_in_the_box_and_fades_show() {
        let mut list = DrawList::new();
        let mut l = line(
            "あいうえおかきくけこさしすせそたちつてと",
            true,
            LyricAlign::Center,
        );
        l.opacity = 0.5;
        // 300 px of box at 1 px a px; 14 px wide characters: 21 a line.
        draw(
            &mut list,
            &[l],
            &Place {
                size: [500.0, 500.0],
                stage: (500.0, 500.0),
                center_x: 250.0,
                keep_out: Vec::new(),
                receptors: Vec::new(),
                min_size: 10.0,
            },
            &mut FixedMeasure,
        );
        let r = runs(&list);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].3, 0.5);
        let mut list = DrawList::new();
        let l = line(
            "a fairly long line of words that runs past the box",
            true,
            LyricAlign::Left,
        );
        draw(
            &mut list,
            &[l],
            &Place {
                size: [500.0, 500.0],
                stage: (500.0, 500.0),
                center_x: 250.0,
                keep_out: Vec::new(),
                receptors: Vec::new(),
                min_size: 10.0,
            },
            &mut FixedMeasure,
        );
        let r = runs(&list);
        assert!(r.len() > 1, "{r:?}");
        for (text, _, size, _) in &r {
            assert!(text.chars().count() as f32 * 0.5 * size <= 300.0, "{text}");
        }
        // One under another.
        assert!(r[1].1[1] > r[0].1[1]);
    }
}
