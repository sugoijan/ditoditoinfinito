//! The chart's keyboard mapping and the prompt's short demonstration.
//! Presentation only: never sends input to a play or changes its bindings.
use crate::keycaps::Keycap;
use crate::state::settings::Settings;
use ddi_chart::Layout;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub struct Controls {
    pub layout: Layout,
    pub settings: Settings,
    /// First saved binding per lane, or the layout's default. An explicitly
    /// unbound lane stays unbound; it must not claim a default still works.
    pub keys: Vec<Option<String>>,
}
impl Controls {
    pub fn new(layout: &Layout, settings: &Settings) -> Self {
        Self {
            layout: layout.clone(),
            settings: settings.clone(),
            keys: settings
                .controls
                .keyboard(layout)
                .into_iter()
                .map(|keys| keys.first().cloned())
                .collect(),
        }
    }
    /// The controls of chart `chart` (its index) of `entry`, as the
    /// prompt will show them: on the layout the song defines for it,
    /// else the built-in one; `None` for a chart or a layout the game
    /// does not know. Song select readies the diagram's skin with it
    /// while the chart is shown, before Start.
    pub fn for_chart(
        entry: &ddi_library::manifest::ManifestEntry,
        chart: usize,
        settings: &Settings,
    ) -> Option<Self> {
        let c = entry.charts.iter().find(|c| c.index == chart)?;
        let layout = entry
            .layouts
            .iter()
            .find(|l| l.id == c.layout)
            .cloned()
            .or_else(|| Layout::builtin(&c.layout))?;
        Some(Self::new(&layout, settings))
    }
    /// Each lane's cap: the active controller's control for it (the play's
    /// assignment, `hints`), else the key.
    pub fn caps(&self, hints: &crate::controller_hints::Hints) -> Vec<Option<Keycap>> {
        hints
            .lanes(&self.layout, &self.settings.controls)
            .unwrap_or_else(|| self.keys.iter().map(|k| k.as_deref().map(keycap)).collect())
    }
    pub fn duration(&self) -> f32 {
        0.2 + self.keys.len() as f32 * 0.14
    }
    pub fn active(&self, time: f32) -> Option<usize> {
        let t = time - 0.2;
        if t < 0.0 {
            return None;
        }
        let lane = (t / 0.14).floor() as usize;
        (lane < self.keys.len() && t % 0.14 < 0.11).then_some(lane)
    }
    /// A diagram in binding order. Spread split-scroll/multi-row layouts
    /// into one strip, which the UI wraps into rows without reordering lanes.
    pub fn diagram_layout(&self) -> Layout {
        let mut layout = self.layout.clone();
        for (i, lane) in layout.lanes.iter_mut().enumerate() {
            lane.column = i as f32;
            lane.row = 0;
            lane.scroll_sign = 1;
        }
        layout
    }
}

/// One receptor-only frame, drawn by the shells' existing skin preview.
/// Compared every frame: its controls by identity (the prompt's state
/// keeps one `Rc` while they are the same).
#[derive(Clone, Debug)]
pub struct Picture {
    pub controls: Rc<Controls>,
    pub size: [f32; 2],
    pub ready: Rc<std::cell::Cell<bool>>,
    pub time: f32,
    pub active: Option<usize>,
}

impl PartialEq for Picture {
    fn eq(&self, o: &Picture) -> bool {
        Rc::ptr_eq(&self.controls, &o.controls)
            && Rc::ptr_eq(&self.ready, &o.ready)
            && self.size == o.size
            && self.time == o.time
            && self.active == o.active
    }
}

/// Physical KeyboardEvent.code names, shared by both shells.
pub fn keycap(code: &str) -> Keycap {
    let label = match code {
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "Slash" => "/",
        "Backslash" => "\\",
        "IntlBackslash" => "intl \\",
        "Comma" => ",",
        "Period" => ".",
        "Semicolon" => ";",
        "Quote" => "'",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Minus" => "-",
        "Equal" => "=",
        "Backquote" => "`",
        "ShiftLeft" => "l shift",
        "ShiftRight" => "r shift",
        "ControlLeft" => "l ctrl",
        "ControlRight" => "r ctrl",
        "AltLeft" => "l alt",
        "AltRight" => "r alt",
        "MetaLeft" => "l meta",
        "MetaRight" => "r meta",
        _ => code
            .strip_prefix("Key")
            .or_else(|| code.strip_prefix("Digit"))
            .unwrap_or(code),
    };
    let label = if label.len() > 1 {
        label.to_lowercase()
    } else {
        label.to_string()
    };
    Keycap::from_hint(&label)
}

/// Receptors share one strip texture; crop it into balanced rows (more,
/// smaller lanes fit across a landscape phone). This also handles layouts whose gameplay receptors share columns.
pub(super) fn draw(
    env: &super::Env,
    controls: &Controls,
    inner: f32,
    top: f32,
    compact: bool,
    cx: &mut crate::Cx,
) -> (Vec<ddi_actor::Actor>, f32, [f32; 2], Vec<Option<Keycap>>) {
    use crate::look::{call, named};
    use ddi_actor::{Actor, Kind, Sprite, State};
    let caps = controls.caps(cx.hints());
    let n = caps.len();
    // On a touch screen, under each receptor the touch control it maps to
    // (`docs/plans/touch-controls.md`), as a cut of the controls: its
    // group or its pad, the lane's own lit.
    let touch = cx
        .hints()
        .source()
        .is_touch()
        .then(|| {
            let vp = cx.viewport().screen;
            crate::touch_controls::plan(
                &controls.layout,
                crate::touch_controls::Screen::new([vp.width, vp.height], [0.0; 4]),
                controls.settings.touch_choice(),
            )
        })
        .flatten();
    let side_labels = caps
        .iter()
        .any(|c| matches!(c,Some(Keycap::Controller(cap)) if cap.side.is_some()));
    let extra = if side_labels { 52.0 } else { 42.0 };
    let rows = n.div_ceil(if compact { 12 } else { 8 }).max(1);
    let cols = n.div_ceil(rows).max(1);
    let cell = (inner / cols as f32).min(if compact { 48.0 } else { 72.0 });
    let texture = cx.texture(super::options::FIELD_PREVIEW, cell * n as f32);
    let mut actors = Vec::new();
    for row in 0..rows {
        let start = row * cols;
        let count = (n - start).min(cols);
        let width = count as f32 * cell;
        let left = 24.0 + (inner - width) / 2.0;
        let y = top + row as f32 * (cell + extra);
        let mut image = named(
            Kind::Sprite(Sprite::new(
                texture,
                vec![State {
                    rect: [
                        start as f32 / n as f32,
                        0.0,
                        (start + count) as f32 / n as f32,
                        1.0,
                    ],
                    delay: 1.0,
                }],
                [width, cell],
            )),
            &format!("Receptors{row}"),
        );
        if let Kind::Sprite(sprite) = &mut image.kind {
            sprite.shape.hide_unloaded = true;
        }
        call(&mut image, "xy", &[left + width / 2.0, y + cell / 2.0]);
        actors.push(image);
        for (i, cap) in caps.iter().enumerate().skip(start).take(count) {
            let x = left + (i - start) as f32 * cell + cell / 2.0;
            if let Some(plan) = &touch {
                let mut cut = touch_cut(plan, i, (cell - 10.0).min(48.0));
                cut.name = format!("TouchCut{i}");
                call(&mut cut, "xy", &[x, y + cell + 18.0]);
                actors.push(cut);
                continue;
            }
            let key = cap.clone();
            let (mut hint, width) = match key {
                Some(key) => super::key_hint(env, &format!("KeyMotion{i}"), &[key], "", cx),
                None => (
                    env.text(
                        &format!("KeyMotion{i}"),
                        &ddi_l10n::msg::prompt::unbound(),
                        10.0,
                        400,
                        "Muted",
                    ),
                    42.0,
                ),
            };
            call(
                &mut hint,
                "zoom",
                &[(cell - 6.0).min(width) / width.max(1.0)],
            );
            let mut holder = Actor::new(Kind::Frame(vec![hint]));
            holder.name = format!("BindingKey{i}");
            call(&mut holder, "xy", &[x, y + cell + 18.0]);
            actors.push(holder);
            if let Some(Keycap::Controller(cap)) = cap
                && let Some(side) = cap.side
            {
                let mut label = env.text(
                    &format!("ControllerSide{i}"),
                    &ddi_l10n::msg::prompt::controller_pad(side),
                    10.0,
                    400,
                    "Muted",
                );
                call(&mut label, "xy", &[x, y + cell + 40.0]);
                actors.push(label);
            }
        }
    }
    (
        actors,
        rows as f32 * (cell + extra),
        [cell * n as f32, cell],
        caps,
    )
}

/// Lane `lane`'s touch control in `plan`, cut out of the controls: its
/// pad whole, or its group of buttons with a line for the screen's
/// bottom edge under it, at the side it is on; the lane's own target lit.
/// About `size` wide, around its middle.
pub fn touch_cut(plan: &crate::touch_controls::Controls, lane: usize, size: f32) -> ddi_actor::Actor {
    use crate::look::{call, quad};
    use crate::touch_controls::Kind;
    let own: Vec<&crate::touch_controls::Target> =
        plan.targets.iter().filter(|t| t.lane == Some(lane)).collect();
    let Some(first) = own.first() else {
        return super::frame("TouchCut", Vec::new());
    };
    // What is cut: the first target's group (its pad, or its buttons).
    let group = plan
        .groups
        .iter()
        .copied()
        .find(|g| {
            let [x, y] = first.center();
            x >= g[0] && x <= g[2] && y >= g[1] && y <= g[3]
        })
        .unwrap_or(first.rect);
    let gw = (group[2] - group[0]).max(1.0);
    let gh = (group[3] - group[1]).max(1.0);
    let k = size / gw.max(gh);
    let pad = matches!(first.kind, Kind::Tile { .. });
    // A group of buttons shows the side it is on: shifted that way.
    let shift = match first.kind {
        Kind::Button { group: 0 } => -size * 0.12,
        Kind::Button { .. } => size * 0.12,
        Kind::Tile { .. } => 0.0,
    };
    let mid = [(group[0] + group[2]) / 2.0, (group[1] + group[3]) / 2.0];
    let mut parts = Vec::new();
    for t in &plan.targets {
        let [x, y] = t.center();
        if x < group[0] || x > group[2] || y < group[1] || y > group[3] {
            continue;
        }
        let [w, h] = t.size();
        let lit = t.lane == Some(lane);
        let color = if lit {
            [1.0, 1.0, 1.0, 0.95]
        } else if t.lane.is_some() {
            [1.0, 1.0, 1.0, 0.32]
        } else {
            [1.0, 1.0, 1.0, 0.12]
        };
        let mut q = quad("", [w * k, h * k], color);
        call(&mut q, "cornerradius", &[(w.min(h) * k * 0.18).max(1.0)]);
        call(&mut q, "xy", &[(x - mid[0]) * k + shift, (y - mid[1]) * k]);
        parts.push(q);
    }
    if !pad {
        // The screen's corner the buttons sit in: its bottom edge under
        // them and its side beside them.
        let bottom = gh * k / 2.0 + 4.0;
        let mut edge = quad("", [size * 1.25, 1.5], [1.0, 1.0, 1.0, 0.35]);
        call(&mut edge, "xy", &[0.0, bottom]);
        parts.push(edge);
        let side = if shift < 0.0 {
            shift - gw * k / 2.0 - 4.0
        } else {
            shift + gw * k / 2.0 + 4.0
        };
        let tall = gh * k + 8.0;
        let mut wall = quad("", [1.5, tall], [1.0, 1.0, 1.0, 0.35]);
        call(&mut wall, "xy", &[side, bottom - tall / 2.0]);
        parts.push(wall);
    }
    let mut a = super::frame("TouchCut", parts);
    a.decoration = true;
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chart_selects_defaults_and_saved_primary_bindings() {
        let layout = Layout::builtin("danoni-7").unwrap();
        let mut settings = Settings::default(); // The menu can still be dance-single.
        let controls = Controls::new(&layout, &settings);
        assert_eq!(
            controls.keys,
            ["KeyS", "KeyD", "KeyF", "Space", "KeyJ", "KeyK", "KeyL"].map(|s| Some(s.into()))
        );
        let mut table = settings.controls.keyboard(&layout);
        table[0] = vec!["KeyA".into(), "KeyQ".into()];
        table[3].clear();
        settings.controls.set_keyboard(&layout, table);
        let controls = Controls::new(&layout, &settings);
        assert_eq!(controls.keys[0].as_deref(), Some("KeyA"));
        assert_eq!(controls.keys[3], None);
        assert_eq!(keycap("KeyA").label(), "A");
        assert_eq!(keycap("Space").shape(), crate::keycaps::KeyShape::Space);
        assert_eq!(keycap("Enter").label(), "enter");
    }

    #[test]
    fn a_touch_cut_shows_the_lanes_group_or_pad_its_own_lit() {
        use crate::touch_controls::{Choice, Screen, Style, plan};
        let layout = Layout::builtin("dance-single").unwrap();
        let screen = Screen::new([390.0, 844.0], [0.0; 4]);
        let parts = |style, lane| {
            let c = plan(&layout, screen, Choice { style, ..Choice::default() }).unwrap();
            let cut = touch_cut(&c, lane, 48.0);
            let ddi_actor::Kind::Frame(children) = &cut.kind else {
                panic!("a frame");
            };
            assert!(cut.decoration);
            children.len()
        };
        // The left group's two buttons and the screen's corner.
        assert_eq!(parts(Style::Buttons, 0), 4);
        // The whole pad.
        assert_eq!(parts(Style::Pad, 2), 9);
    }

    #[test]
    fn the_brief_demo_visits_every_lane_once_then_stops() {
        for id in ["dance-single", "danoni-7", "pump-double"] {
            let controls = Controls::new(&Layout::builtin(id).unwrap(), &Settings::default());
            assert!(controls.duration() < 3.0);
            assert_eq!(controls.active(0.0), None);
            for i in 0..controls.keys.len() {
                assert_eq!(controls.active(0.2 + i as f32 * 0.14 + 0.05), Some(i));
            }
            assert_eq!(controls.active(controls.duration()), None);
            let diagram = controls.diagram_layout();
            for (i, lane) in diagram.lanes.iter().enumerate() {
                assert_eq!(lane.glyph, controls.layout.lanes[i].glyph);
                assert_eq!((lane.column, lane.row, lane.scroll_sign), (i as f32, 0, 1));
            }
        }
    }
}
