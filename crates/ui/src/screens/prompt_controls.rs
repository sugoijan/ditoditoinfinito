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
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub controls: Rc<Controls>,
    pub size: [f32; 2],
    pub ready: Rc<std::cell::Cell<bool>>,
    pub time: f32,
    pub active: Option<usize>,
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
) -> (Vec<ddi_actor::Actor>, f32, [f32; 2]) {
    use crate::look::{call, named};
    use ddi_actor::{Actor, Kind, Sprite, State};
    let n = controls.keys.len();
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
        let y = top + row as f32 * (cell + 42.0);
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
        for i in start..start + count {
            let x = left + (i - start) as f32 * cell + cell / 2.0;
            let key = controls.keys[i].as_deref().map(keycap);
            let (mut hint, width) = match key {
                Some(key) => super::key_hint(env, &format!("KeyMotion{i}"), &[key], "", cx),
                None => (
                    env.text(&format!("KeyMotion{i}"), "unbound", 10.0, 400, "Muted"),
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
        }
    }
    (actors, rows as f32 * (cell + 42.0), [cell * n as f32, cell])
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
