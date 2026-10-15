//! winit's keys as the DOM names them, which is what `ddi_ui` reads:
//! physical keys as `KeyboardEvent.code`, logical ones as
//! `KeyboardEvent.key`. winit's names are the W3C's for both, but for
//! the Super keys (the DOM's Meta) and Space's key (a space).

use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

/// `KeyboardEvent.code` of a physical key.
pub fn code(key: PhysicalKey) -> String {
    match key {
        PhysicalKey::Code(KeyCode::SuperLeft) => "MetaLeft".into(),
        PhysicalKey::Code(KeyCode::SuperRight) => "MetaRight".into(),
        PhysicalKey::Code(c) => format!("{c:?}"),
        PhysicalKey::Unidentified(_) => "Unidentified".into(),
    }
}

/// `KeyboardEvent.key` of a logical key.
pub fn key(key: &Key) -> String {
    match key {
        Key::Character(s) => s.to_string(),
        Key::Named(NamedKey::Space) => " ".into(),
        Key::Named(NamedKey::Super) => "Meta".into(),
        Key::Named(n) => format!("{n:?}"),
        Key::Dead(_) => "Dead".into(),
        Key::Unidentified(_) => "Unidentified".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_the_doms() {
        assert_eq!(code(PhysicalKey::Code(KeyCode::ArrowUp)), "ArrowUp");
        assert_eq!(code(PhysicalKey::Code(KeyCode::KeyQ)), "KeyQ");
        assert_eq!(code(PhysicalKey::Code(KeyCode::NumpadEnter)), "NumpadEnter");
        assert_eq!(code(PhysicalKey::Code(KeyCode::SuperLeft)), "MetaLeft");
        assert_eq!(code(PhysicalKey::Code(KeyCode::Slash)), "Slash");
        assert_eq!(key(&Key::Named(NamedKey::Enter)), "Enter");
        assert_eq!(key(&Key::Named(NamedKey::Escape)), "Escape");
        assert_eq!(key(&Key::Named(NamedKey::ArrowLeft)), "ArrowLeft");
        assert_eq!(key(&Key::Named(NamedKey::Backspace)), "Backspace");
        assert_eq!(key(&Key::Named(NamedKey::Space)), " ");
        assert_eq!(key(&Key::Character("/".into())), "/");
    }
}
