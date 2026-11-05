//! Coordinated field/hand recipes. Presentation only; contacts stay in mat space.
use super::*;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandLayout {
    FieldRight,
    FieldLeft,
    CenterHandRight,
    CenterHandLeft,
    NearRight,
    NearLeft,
    #[default]
    #[serde(other)]
    FollowField,
}
impl HandLayout {
    pub const ALL: [Self; 7] = [
        Self::FollowField,
        Self::FieldRight,
        Self::FieldLeft,
        Self::CenterHandRight,
        Self::CenterHandLeft,
        Self::NearRight,
        Self::NearLeft,
    ];
    pub fn field(self) -> Option<FieldPlace> {
        Some(match self {
            Self::FollowField => return None,
            Self::FieldRight => FieldPlace::Right,
            Self::FieldLeft => FieldPlace::Left,
            Self::CenterHandRight | Self::CenterHandLeft => FieldPlace::Center,
            Self::NearRight => FieldPlace::NearRight,
            Self::NearLeft => FieldPlace::NearLeft,
        })
    }
    fn center(self, field: FieldPlace) -> f32 {
        match self {
            Self::FieldRight => 0.25,
            Self::FieldLeft | Self::CenterHandRight => 0.75,
            Self::CenterHandLeft => 0.25,
            Self::NearRight => 0.375,
            Self::NearLeft => 0.625,
            Self::FollowField => match field {
                FieldPlace::Right => 0.25,
                FieldPlace::NearRight => 0.375,
                FieldPlace::NearLeft => 0.625,
                _ => 0.75,
            },
        }
    }
}
impl Settings {
    pub fn hand_frame(&self) -> ddi_character::appearance::HandFrame {
        ddi_character::appearance::HandFrame {
            center: self.hand_layout.center(self.field_place),
            width: f32::from(self.hand_frame_width.clamp(30, 100)) / 100.,
            height: f32::from(self.hand_frame_height.clamp(50, 100)) / 100.,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipes_are_mirrored_and_saved_choices_survive() {
        for (a, b) in [
            (HandLayout::FieldRight, HandLayout::FieldLeft),
            (HandLayout::CenterHandRight, HandLayout::CenterHandLeft),
            (HandLayout::NearRight, HandLayout::NearLeft),
        ] {
            assert_eq!(
                a.center(FieldPlace::Center) + b.center(FieldPlace::Center),
                1.
            );
        }
        let s: Settings = serde_json::from_str(
            r#"{"dancing_hand":false,"hand_layout":"NearLeft","hand_frame_width":60}"#,
        )
        .unwrap();
        assert!(!s.dancing_hand);
        let round: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(round.hand_layout, HandLayout::NearLeft);
        assert_eq!(round.hand_frame().width, 0.6);
        assert!(serde_json::from_str::<Settings>("{}").unwrap().dancing_hand);
    }
}
