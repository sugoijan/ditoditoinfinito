//! Wide characters, one place for the crate, its build script and the
//! UI's text metrics.

/// Whether `c` is wide (CJK, kana, Hangul, full-width forms): a full em
/// in the theme's metrics (`ddi_ui::look::mono_width`), drawn by a
/// language's face where the stack names one, never by the theme's.
pub fn wide(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
}
