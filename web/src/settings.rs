//! Player settings: the data and its rules are shared with every shell
//! (`ddi_ui::state::settings`, P12); they are kept in `localStorage` under
//! the same key as ever through the store `crate::web::kv` installs.

pub(crate) use ddi_ui::state::settings::*;

/// The renderer's color scheme for a [`NoteColors`] setting.
pub(crate) fn note_scheme(colors: NoteColors) -> ddi_render::ColorScheme {
    use ddi_render::ColorScheme;
    match colors {
        NoteColors::Quantized => ColorScheme::Quantized,
        NoteColors::Vivid => ColorScheme::Vivid,
        NoteColors::Flat => ColorScheme::Flat,
        NoteColors::Rainbow => ColorScheme::Rainbow,
        NoteColors::Single => ColorScheme::Single([0.35, 0.85, 1.0, 1.0]),
    }
}
