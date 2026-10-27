//! What a shortcut hint asks the theme to draw. A physical key carries its
//! identity; custom printing carries an explicit base shape and verbatim text.
//! These are presentation requests, independent of input bindings.

/// Base geometry for custom printing. Width still grows with the label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyShape {
    #[default]
    Normal,
    Space,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    Up,
    Down,
    Left,
    Right,
}

impl Arrow {
    pub fn symbol(self) -> char {
        match self {
            Self::Up => '↑',
            Self::Down => '↓',
            Self::Left => '←',
            Self::Right => '→',
        }
    }

    pub fn from_symbol(c: char) -> Option<Self> {
        match c {
            '↑' => Some(Self::Up),
            '↓' => Some(Self::Down),
            '←' => Some(Self::Left),
            '→' => Some(Self::Right),
            _ => None,
        }
    }
}

/// A named keyboard key. The theme chooses its legend, marks and placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardKey {
    Character(char),
    Arrow(Arrow),
    Enter,
    Tab,
    Escape,
    Space,
    Delete,
    Backspace,
    Shift,
    Control,
    Alt,
    Meta,
    Home,
    End,
    PageUp,
    PageDown,
}

impl KeyboardKey {
    /// Plain text for a look that cannot draw key-specific artwork.
    pub fn label(self) -> String {
        match self {
            Self::Character(c) => c.to_uppercase().collect(),
            Self::Arrow(a) => a.symbol().to_string(),
            Self::Enter => "enter".into(),
            Self::Tab => "tab".into(),
            Self::Escape => "esc".into(),
            Self::Space => "space".into(),
            Self::Delete => "del".into(),
            Self::Backspace => "backspace".into(),
            Self::Shift => "shift".into(),
            Self::Control => "ctrl".into(),
            Self::Alt => "alt".into(),
            Self::Meta => "meta".into(),
            Self::Home => "home".into(),
            Self::End => "end".into(),
            Self::PageUp => "pgup".into(),
            Self::PageDown => "pgdn".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Keycap {
    Controller(crate::controller_hints::Cap),
    Key(KeyboardKey),
    Custom { label: String, shape: KeyShape },
}

impl Keycap {
    pub fn custom(label: impl Into<String>, shape: KeyShape) -> Self {
        Self::Custom {
            label: label.into(),
            shape,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Key(key) => key.label(),
            Self::Controller(cap) => cap.label(),
            Self::Custom { label, .. } => label.clone(),
        }
    }

    pub fn shape(&self) -> KeyShape {
        match self {
            Self::Key(KeyboardKey::Space) => KeyShape::Space,
            Self::Custom { shape, .. } => *shape,
            Self::Key(_) | Self::Controller(_) => KeyShape::Normal,
        }
    }

    /// Adapter for existing screen hint strings. New callers can request a
    /// key or custom printing explicitly, without parsing labels. Composite
    /// strings ("↑↓", "2×↑↓←→", ranges) remain one custom normal cap.
    pub fn from_hint(label: &str) -> Self {
        use KeyboardKey as K;
        let key = match label.to_ascii_uppercase().as_str() {
            "ENTER" => K::Enter,
            "TAB" => K::Tab,
            "ESC" | "ESCAPE" => K::Escape,
            "SPACE" => K::Space,
            "DEL" | "DELETE" => K::Delete,
            "BACKSPACE" => K::Backspace,
            "SHIFT" => K::Shift,
            "CTRL" | "CONTROL" => K::Control,
            "ALT" => K::Alt,
            "META" => K::Meta,
            "HOME" => K::Home,
            "END" => K::End,
            "PGUP" | "PAGEUP" => K::PageUp,
            "PGDN" | "PAGEDOWN" => K::PageDown,
            _ => {
                let mut chars = label.chars();
                if let Some(c) = chars.next()
                    && chars.next().is_none()
                {
                    return Self::Key(Arrow::from_symbol(c).map_or(K::Character(c), K::Arrow));
                }
                return Self::custom(label, KeyShape::Normal);
            }
        };
        Self::Key(key)
    }
}

impl From<KeyboardKey> for Keycap {
    fn from(key: KeyboardKey) -> Self {
        Self::Key(key)
    }
}
