//! Approved cursor masks with Vivid band paint and hotspot-anchored feedback.
use ddi_actor::{Actor, Blend, Kind, Sprite, State, Vivid};
use ddi_ui::cursor::Spec;
use ddi_ui::look::{call, named};
use ddi_ui::state::settings::CursorStyle;

fn mask(row: usize, column: usize, side: f32, pos: [f32; 2]) -> Actor {
    let mut s = Sprite::new(
        0,
        vec![State {
            rect: [
                column as f32 / 8.0,
                row as f32 / 2.0,
                (column + 1) as f32 / 8.0,
                (row + 1) as f32 / 2.0,
            ],
            delay: 1.0,
        }],
        [side; 2],
    );
    s.shape.hide_unloaded = true;
    let mut a = named(Kind::Sprite(s), "PointerMask");
    a.set_position([pos[0], pos[1], 0.0]);
    a
}

pub(super) fn actor(spec: &Spec) -> Option<Actor> {
    let row = match spec.style {
        CursorStyle::InfinitoArrow => 0,
        CursorStyle::PointerTip => 1,
        CursorStyle::System => return None,
    };
    let side = spec.size * 96.0 / 56.0;
    // Both designs' sharp tip is at (16,16) in its 96-unit atlas cell.
    let offset = side * (0.5 - 16.0 / 96.0);
    let center = [spec.pos[0] + offset, spec.pos[1] + offset];
    let mut pieces = vec![mask(row, 4 + usize::from(spec.stage), side, center)];
    let mut band = mask(row, 0, side, center);
    if let Kind::Sprite(s) = &mut band.kind {
        s.shape.vivid = Some(Vivid {
            time: (2.0 * (5.04 + spec.seconds)) as f32,
            phase_offset: 2.146,
            evolution_speed: 0.5,
            screen_space: false,
            rect: [-48.0, -20.0, 48.0, 76.0],
        });
    }
    pieces.push(band);
    if spec.stage > 0 {
        pieces.push(mask(row, usize::from(spec.stage), side, center));
    }
    if let Some(hit) = spec.hit {
        let (duration, from, to, life, alpha, color) = match hit.stage {
            3 => (0.175, 0.85, 1.25, 0.175, 1.0, [1.0; 4]),
            2 => (0.15, 0.54, 0.70, 5.0 / 60.0, 0.9, [1.0, 0.94, 0.39, 1.0]),
            1 => (0.15, 0.40, 0.56, 0.125, 0.8, [0.25, 1.0, 0.25, 1.0]),
            _ => (0.15, 0.26, 0.40, 0.10, 0.65, [0.24, 1.0, 0.75, 1.0]),
        };
        if hit.age < duration {
            let t = hit.age / duration;
            let zoom = if spec.reduced_motion {
                1.0
            } else {
                1.0 + 0.25 * t
            };
            let mut echo = mask(
                row,
                0,
                side * zoom,
                [hit.pos[0] + offset * zoom, hit.pos[1] + offset * zoom],
            );
            echo.blend = Blend::Add;
            call(&mut echo, "diffuse", &color);
            call(
                &mut echo,
                "diffusealpha",
                &[if hit.stage == 3 { 0.25 } else { 0.9 } * (1.0 - t)],
            );
            pieces.push(echo);
        }
        if hit.age < life {
            let t = if hit.stage == 3 {
                hit.age / life
            } else {
                (hit.age / 0.05).min(1.0)
            };
            let diameter = spec.size * 96.0 / 43.1335
                * if spec.reduced_motion {
                    from
                } else {
                    from + (to - from) * t
                };
            let fade = if hit.stage == 3 {
                (1.0 - (hit.age - 0.025).max(0.0) / 0.15).max(0.0)
            } else {
                1.0 - hit.age / life
            };
            let gains = if hit.stage == 3 {
                [1.0, 0.8]
            } else {
                [alpha, 0.0]
            };
            for gain in gains.into_iter().filter(|g| *g > 0.0) {
                let mut s = Sprite::new(0, vec![], [diameter; 2]);
                s.shape.hide_unloaded = true;
                let mut burst = named(Kind::Sprite(s), "PointerBurst");
                burst.blend = Blend::Add;
                burst.set_position([hit.pos[0], hit.pos[1], 0.0]);
                call(&mut burst, "rotationz", &[hit.rotation]);
                call(&mut burst, "diffusealpha", &[fade * gain]);
                pieces.push(burst);
            }
        }
    }
    let mut a = named(Kind::Frame(pieces), "PointerOverlay");
    a.decoration = true;
    Some(a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_actor::{Drawn, Kind};
    use ddi_ui::look::{Look, Presentation};
    use ddi_ui::request::Cursor;
    use ddi_ui::screens::Env;
    use ddi_ui::testing::Harness;
    use ddi_ui::{
        Cx, Handled, Input, PointerKind, PointerPhase, Screen, TextMode, UiEvent, UiRequest,
    };
    use std::cell::Cell;
    use std::rc::Rc;

    struct Page {
        env: Env,
        actions: Rc<Cell<u32>>,
    }
    impl Screen for Page {
        fn name(&self) -> &str {
            "cursor-test"
        }
        fn build(&mut self, _: &mut Cx) -> Actor {
            let mut parts = Vec::new();
            for (name, pos) in [
                ("Button", [100.0, 100.0]),
                ("Disabled", [300.0, 100.0]),
                ("Text", [500.0, 100.0]),
            ] {
                let mut a = named(Kind::Sprite(Sprite::quad([100.0, 40.0])), name);
                a.set_position([pos[0], pos[1], 0.0]);
                parts.push(a);
            }
            named(Kind::Frame(parts), "Page")
        }
        fn presentation(&self) -> Option<Presentation<'_>> {
            Some(self.env.presentation())
        }
        fn focusables(&self) -> Vec<ddi_ui::Focusable> {
            vec![ddi_ui::Focusable::new("Button")]
        }
        fn cursor(&self, target: &ddi_ui::Target) -> Option<Cursor> {
            target.within("Text").then_some(Cursor::Text)
        }
        fn input(&mut self, input: &Input, _: &mut Actor, _: &mut Cx) -> Handled {
            if let Input::Activate { name } = input
                && name == "Button"
            {
                self.actions.set(self.actions.get() + 1);
                return Handled::Yes;
            }
            Handled::No
        }
    }
    fn rig() -> (Harness, Env, Rc<Cell<u32>>) {
        let env = Env::new(
            Rc::new(super::super::Infinito::default()),
            Rc::new(str::to_lowercase),
        );
        let actions = Rc::new(Cell::new(0));
        let mut h = Harness::new([800.0, 400.0], [800.0, 400.0], 2.0, TextMode::Own);
        h.push(Page {
            env: env.clone(),
            actions: actions.clone(),
        });
        h.run(0.2);
        (h, env, actions)
    }
    fn shines(h: &Harness) -> usize {
        h.drawn
            .iter()
            .filter(|d| matches!(d, Drawn::Quad(q) if q.blend==Blend::Add))
            .count()
    }
    #[test]
    fn mouse_down_shines_but_only_release_activates_and_empty_space_does_not_shine() {
        let (mut h, _, actions) = rig();
        assert!(
            h.hover([100.0, 100.0])
                .contains(&UiRequest::SetCursor(Cursor::Hidden))
        );
        h.frame(0.0);
        h.pointer(1, PointerKind::Mouse, PointerPhase::Down, [100.0, 100.0]);
        h.frame(0.0);
        assert!(shines(&h) > 0);
        assert_eq!(actions.get(), 0);
        let burst = h
            .drawn
            .iter()
            .filter_map(|d| match d {
                Drawn::Quad(q)
                    if h.ui
                        .texture_source(q.texture)
                        .is_some_and(|s| &*s.source == "ui/cursor-burst.svg") =>
                {
                    Some(q.transform)
                }
                _ => None,
            })
            .next_back()
            .unwrap();
        h.pointer(1, PointerKind::Mouse, PointerPhase::Move, [200.0, 200.0]);
        h.frame(0.02);
        let fixed = h
            .drawn
            .iter()
            .filter_map(|d| match d {
                Drawn::Quad(q)
                    if h.ui
                        .texture_source(q.texture)
                        .is_some_and(|s| &*s.source == "ui/cursor-burst.svg") =>
                {
                    Some(q.transform)
                }
                _ => None,
            })
            .next_back()
            .unwrap();
        assert_eq!([burst[4], burst[5]], [fixed[4], fixed[5]]);
        h.pointer(1, PointerKind::Mouse, PointerPhase::Up, [200.0, 200.0]);
        assert_eq!(actions.get(), 0);
        h.run(0.2);
        h.tap_at(PointerKind::Mouse, [100.0, 100.0]);
        assert_eq!(actions.get(), 1);
        h.run(0.2);
        h.pointer(1, PointerKind::Mouse, PointerPhase::Down, [300.0, 100.0]);
        h.frame(0.0);
        assert_eq!(shines(&h), 0);
    }
    #[test]
    fn system_text_touch_blur_and_reduced_motion_keep_their_expected_behavior() {
        let (mut h, env, _) = rig();
        h.hover([100.0, 100.0]);
        h.frame(0.0);
        env.cursor.set((CursorStyle::System, 28.0));
        assert!(
            h.frame(0.0)
                .contains(&UiRequest::SetCursor(Cursor::Pointer))
        );
        assert!(
            !h.drawn
                .iter()
                .any(|d| matches!(d,Drawn::Quad(q) if q.shape.vivid.is_some() && h.ui.texture_source(q.texture).is_some_and(|s| &*s.source == "ui/cursors.svg")))
        );
        env.cursor.set((CursorStyle::PointerTip, 28.0));
        h.frame(0.0);
        assert!(
            h.hover([500.0, 100.0])
                .contains(&UiRequest::SetCursor(Cursor::Text))
        );
        env.reduced_motion.set(true);
        h.hover([100.0, 100.0]);
        h.frame(0.0);
        assert!(!h.ui.needs_redraw());
        h.pointer(2, PointerKind::Touch, PointerPhase::Down, [100.0, 100.0]);
        h.frame(0.0);
        assert_eq!(shines(&h), 0);
        h.send(UiEvent::Focus(false));
        h.frame(0.0);
        assert!(
            !h.drawn
                .iter()
                .any(|d| matches!(d,Drawn::Quad(q) if q.shape.vivid.is_some() && h.ui.texture_source(q.texture).is_some_and(|s| &*s.source == "ui/cursors.svg")))
        );
    }
    #[test]
    fn the_artwork_size_is_in_css_pixels_and_variants_keep_the_tip_at_the_origin() {
        let (mut h, env, _) = rig();
        h.send(UiEvent::Resize {
            size: [400.0, 200.0],
            ratio: 2.0,
        });
        for style in [CursorStyle::InfinitoArrow, CursorStyle::PointerTip] {
            env.cursor.set((style, 28.0));
            h.hover([100.0, 100.0]);
            h.frame(0.0);
            let q = h
                .drawn
                .iter()
                .find_map(|d| match d {
                    Drawn::Quad(q)
                        if q.shape.vivid.is_some()
                            && h.ui
                                .texture_source(q.texture)
                                .is_some_and(|s| &*s.source == "ui/cursors.svg") =>
                    {
                        Some(q)
                    }
                    _ => None,
                })
                .unwrap();
            let css_width = q.size[0] * h.ui.viewport().scale / h.ui.viewport().ratio;
            assert!((css_width - 48.0).abs() < 0.01);
        }
        assert!(
            super::super::Infinito::default()
                .pointer(&Spec {
                    style: CursorStyle::System,
                    size: 28.0,
                    pos: [0.0; 2],
                    seconds: 0.0,
                    stage: 0,
                    reduced_motion: false,
                    hit: None
                })
                .is_none()
        );
    }
}
