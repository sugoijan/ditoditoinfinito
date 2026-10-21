//! What both shells (the web's `crates/web`, desktop's `crates/desktop`)
//! do between the player's settings (`ddi_ui::state::settings`) and the
//! renderer (`ddi_render`), so neither keeps a copy of its own:
//!
//! - the field's options for a play ([`render_options`]) and for Options'
//!   field preview ([`field_preview_options`], [`field_preview_skin`]),
//!   with the note colors as the renderer has them ([`note_scheme`]);
//! - the ghost's lead as the HUD shows it ([`score_delta`]);
//! - [`stage`]: a Dancing☆Onigiri work's layers' textures, over a content
//!   each shell decodes its own way;
//! - [`scripted`]: a song's scripted backgrounds and foregrounds, their
//!   images' textures (the same contents) and quads.
//!
//! Platform-free: built natively and for wasm, with no platform code.

pub mod scripted;
pub mod stage;

use ddi_chart::Layout;
use ddi_render::field_preview::{BPM, FieldPreviewOptions, SkinWant};
use ddi_render::{ColorScheme, RenderOptions, ScoreDelta};
use ddi_ui::state::history::GhostDelta;
use ddi_ui::state::settings::{BackgroundFrame, NoteColors, Settings};

/// The renderer's color scheme for a [`NoteColors`] setting.
pub fn note_scheme(colors: NoteColors) -> ColorScheme {
    match colors {
        NoteColors::Quantized => ColorScheme::Quantized,
        NoteColors::Vivid => ColorScheme::Vivid,
        NoteColors::Flat => ColorScheme::Flat,
        NoteColors::Rainbow => ColorScheme::Rainbow,
        NoteColors::Single => ColorScheme::Single([0.35, 0.85, 1.0, 1.0]),
    }
}

/// What a play's field draws, from the settings. What changes during
/// the play (the backdrop, the frame, the reveal, what the session says
/// of the notes and the cancel gesture, the ghost) is set each frame by
/// the shell.
pub fn render_options(settings: &Settings) -> RenderOptions {
    RenderOptions {
        reverse: settings.reverse,
        hide_notes: false,
        show_deltas: settings.show_deltas,
        cancel_progress: 0.0,
        cancel_hint: "",
        note_colors: note_scheme(settings.note_colors),
        chart_colors: settings.chart_colors,
        background: settings.bg_brightness,
        backdrop: Default::default(),
        frame_aspect: None,
        background_reveal: None,
        field_filter: settings.field_filter,
        preview: None,
        ghost: None,
    }
}

/// The ghost's lead as the HUD shows it: a percentage, or whole points.
pub fn score_delta(delta: GhostDelta) -> ScoreDelta {
    match delta {
        GhostDelta::Percent(p) => ScoreDelta::Percent(p as f32),
        GhostDelta::Points(p) => ScoreDelta::Points(p.round() as i64),
    }
}

/// The background frames' shapes in the field preview, as a play fits a
/// background; the automatic one fits the preview's plate, which is 16:9.
fn preview_frame_aspect(frame: BackgroundFrame) -> Option<f32> {
    match frame {
        BackgroundFrame::Screen => None,
        BackgroundFrame::FourThree => Some(4.0 / 3.0),
        BackgroundFrame::SixteenNine | BackgroundFrame::Auto => Some(16.0 / 9.0),
    }
}

/// Options' field preview's options under `settings`, as a play's
/// ([`render_options`]): the demo chart on the chosen layout (a single
/// one for a layout that is not built in), under the rules its chart
/// would be played with.
pub fn field_preview_options(settings: &Settings) -> FieldPreviewOptions {
    let layout = if Layout::builtin(&settings.style).is_some() {
        settings.style.clone()
    } else {
        "dance-single".to_string()
    };
    let rules = ddi_engine::demo::song(&layout, BPM)
        .and_then(|s| s.charts.first().map(|c| settings.ruleset_for(c).key()))
        .unwrap_or_else(|| settings.ruleset().key());
    FieldPreviewOptions {
        autoplay_flashes: false,
        layout,
        play: settings.play_options(None),
        rules,
        note_colors: note_scheme(settings.note_colors),
        field_filter: settings.field_filter,
        show_deltas: settings.show_deltas,
        background: settings.bg_brightness,
        frame_aspect: preview_frame_aspect(settings.background_frame),
    }
}

/// The noteskin play would draw the preview's layout with under
/// `settings`, and what it is readied for (its key, the layout, the
/// direction): key `None` for the built-in arrows. The demo has no
/// colors of its own.
pub fn field_preview_skin(settings: &Settings, options: &FieldPreviewOptions) -> SkinWant {
    SkinWant::new(settings.play_skin(&options.layout, None), options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_color_is_the_cyan() {
        assert_eq!(
            note_scheme(NoteColors::Single),
            ColorScheme::Single([0.35, 0.85, 1.0, 1.0])
        );
    }

    #[test]
    fn the_field_takes_the_settings() {
        let settings = Settings {
            reverse: true,
            show_deltas: true,
            bg_brightness: 0.25,
            ..Settings::default()
        };
        let render = render_options(&settings);
        assert!(render.reverse && render.show_deltas && !render.hide_notes);
        assert_eq!(render.background, 0.25);
        assert_eq!(render.note_colors, note_scheme(settings.note_colors));
        assert!(render.ghost.is_none() && render.frame_aspect.is_none());
    }

    #[test]
    fn the_ghost_shows_whole_points() {
        assert_eq!(
            score_delta(GhostDelta::Points(-1229.6)),
            ScoreDelta::Points(-1230)
        );
        assert_eq!(
            score_delta(GhostDelta::Percent(0.42)),
            ScoreDelta::Percent(0.42)
        );
    }

    #[test]
    fn the_preview_fits_the_frame_its_plate_allows() {
        let mut settings = Settings::default();
        for (frame, aspect) in [
            (BackgroundFrame::Screen, None),
            (BackgroundFrame::FourThree, Some(4.0 / 3.0)),
            (BackgroundFrame::SixteenNine, Some(16.0 / 9.0)),
            (BackgroundFrame::Auto, Some(16.0 / 9.0)),
        ] {
            settings.background_frame = frame;
            assert_eq!(field_preview_options(&settings).frame_aspect, aspect);
        }
    }

    #[test]
    fn a_layout_not_built_in_previews_as_single() {
        let settings = Settings {
            style: "no-such-layout".into(),
            ..Settings::default()
        };
        assert_eq!(field_preview_options(&settings).layout, "dance-single");
    }
}
