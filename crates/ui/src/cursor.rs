//! Pointer presentation shared by the shells. It observes UI hit-testing;
//! feedback never changes activation, judging or the gesture tracker.
use crate::state::settings::CursorStyle;

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub pos: [f32; 2],
    pub age: f32,
    pub stage: u8,
    pub rotation: f32,
}

/// Look input, in virtual screen pixels, already corrected for UI scale.
pub struct Spec {
    pub style: CursorStyle,
    pub size: f32,
    pub pos: [f32; 2],
    pub seconds: f64,
    pub stage: u8,
    pub reduced_motion: bool,
    pub hit: Option<Hit>,
}

pub fn charge(seconds: f64) -> u8 {
    [3, 0, 1, 2][((seconds * 2.0).rem_euclid(1.0) * 4.0).floor() as usize]
}

#[derive(Default)]
pub(crate) struct State {
    pub visible: bool,
    pub displayed: u8,
    pub hit: Option<Hit>,
    serial: u32,
}

impl State {
    pub fn press(&mut self, pos: [f32; 2]) {
        self.serial = self.serial.wrapping_add(1);
        self.hit = Some(Hit {
            pos,
            age: 0.0,
            stage: self.displayed,
            rotation: (self.serial as f32 * 137.508).rem_euclid(360.0),
        });
    }

    pub fn update(&mut self, dt: f32) {
        if let Some(hit) = &mut self.hit {
            hit.age += dt;
            if hit.age >= 0.175 {
                self.hit = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_matches_the_study_and_press_keeps_the_last_visible_stage_and_point() {
        assert_eq!([0.0, 0.125, 0.25, 0.375, 0.5].map(charge), [3, 0, 1, 2, 3]);
        let mut s = State {
            displayed: 2,
            ..State::default()
        };
        s.press([20.0, 30.0]);
        s.displayed = 0;
        s.update(0.05);
        let hit = s.hit.unwrap();
        assert_eq!(hit.stage, 2);
        assert_eq!(hit.pos, [20.0, 30.0]);
        s.update(0.13);
        assert!(s.hit.is_none());
    }
}
