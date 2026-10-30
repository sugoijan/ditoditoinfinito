//! The chart editor's field in the player's skin, between the editor
//! screen's picture (`ddi_ui::screens::editor_field`) and the renderer
//! (`ddi_render::editor_field`): what the screen is told of the player's
//! scroll, the skin a play of the chart would be drawn with (readied for
//! the direction drawn), and the picture's geometry as the renderer
//! places a field.

use ddi_chart::{Chart, Layout, Song};
use ddi_noteskin::lua::Env;
use ddi_render::editor_field::FieldDraw;
use ddi_render::field_preview::SkinWant;
use ddi_render::{ColorScheme, FieldGeometry};
use ddi_ui::screens::editor_field::{FieldPicture, SkinField};
use ddi_ui::state::settings::Settings;

use crate::note_scheme;

/// What the editor screen is told: the player's scroll as a play of an
/// `.sm` or `.ssc` chart takes it (their speed is the player's).
pub fn skin_field(settings: &Settings) -> SkinField {
    SkinField {
        scroll: settings.play_options(None).scroll,
    }
}

/// The picture's layout (dance single for one that is not built in).
pub fn layout(picture: &FieldPicture) -> Layout {
    Layout::builtin(&picture.layout).unwrap_or_else(Layout::dance_single)
}

/// The picture's field as the renderer places one, in the texture's
/// pixels.
pub fn geometry(picture: &FieldPicture) -> FieldGeometry {
    FieldGeometry {
        arrow: picture.arrow,
        left: picture.left,
        receptor_y: picture.receptor_y,
        direction: if picture.reverse { -1.0 } else { 1.0 },
        width: picture.rect[2],
        height: picture.rect[3],
    }
}

/// The renderer's frame of `picture` (its layout `layout`) in `colors`
/// ([`note_colors`]).
pub fn draw<'a>(
    picture: &'a FieldPicture,
    layout: &'a Layout,
    colors: NoteColors,
) -> FieldDraw<'a> {
    FieldDraw {
        frame: &picture.frame,
        layout,
        geometry: geometry(picture),
        running: picture.running,
        events: &picture.events,
        note_colors: colors.scheme,
        chart_colors: colors.chart,
        background: picture.background,
    }
}

/// How the field's notes are colored where the skin leaves it to the
/// game (the built-in arrows'): a play's [`crate::render_options`]'
/// `note_colors` and `chart_colors`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NoteColors {
    pub scheme: ColorScheme,
    /// A song's own colors.
    pub chart: bool,
}

/// The note colors the field is drawn in under `settings` (a play's).
pub fn note_colors(settings: &Settings) -> NoteColors {
    NoteColors {
        scheme: note_scheme(settings.note_colors()),
        chart: crate::render_options(settings).chart_colors,
    }
}

/// The texture's size, whole pixels.
pub fn size(picture: &FieldPicture) -> [u32; 2] {
    [
        picture.rect[2].round().max(1.0) as u32,
        picture.rect[3].round().max(1.0) as u32,
    ]
}

/// The skin a play would draw `picture`'s layout with under `settings`,
/// readied for its direction, with its parameters' values.
pub fn skin(settings: &Settings, picture: &FieldPicture) -> SkinWant {
    let key = settings.play_skin(&picture.layout);
    SkinWant {
        params: settings.skin_params(&key),
        key: Some(key),
        layout: picture.layout.clone(),
        reverse: picture.reverse,
    }
}

/// What the skin's Lua is told: `song`'s chart `chart` on the wanted
/// layout and direction, with the parameters' values.
pub fn skin_env(want: &SkinWant, song: &Song, chart: &Chart) -> Env {
    let layout = want.layout();
    Env {
        params: want.params.clone(),
        ..ddi_noteskin::lua::chart_env(song, chart, &layout, want.reverse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sources_are_one() {
        assert_eq!(
            ddi_ui::screens::editor_field::SOURCE,
            ddi_render::editor_field::SOURCE
        );
    }

    #[test]
    fn the_picture_is_placed_as_drawn() {
        let editor = ddi_ui::state::editor::Editor::new_song(None, "T");
        let picture = FieldPicture {
            rect: [10.0, 20.0, 300.0, 500.0],
            layout: "dance-single".into(),
            arrow: 64.0,
            left: 22.0,
            receptor_y: 140.0,
            reverse: true,
            frame: ddi_ui::screens::editor_field::frame(
                editor.chart(),
                editor.timing(),
                &ddi_ui::screens::editor_field::Mapping::edit(140.0, 0.0, 64.0, 64.0),
                0.0,
                500.0,
                0.0,
                &[None; 4],
            ),
            running: false,
            events: Vec::new(),
            background: [0.0, 0.0, 0.0, 1.0],
        };
        let g = geometry(&picture);
        assert_eq!(g.y(1.5), picture.y(1.5));
        assert_eq!(g.lane_x(2.0), picture.lane_x(2.0));
        assert_eq!(size(&picture), [300, 500]);
        let settings = Settings {
            reverse: true,
            ..Settings::default()
        };
        assert!(skin_field(&settings).scroll.reverse);
        let want = skin(&settings, &picture);
        assert!(want.reverse && want.layout == "dance-single");
    }

    #[test]
    fn the_notes_are_colored_as_a_plays() {
        // The editor uses the same fallback palette as play.
        let mut settings = Settings::default();
        settings.set_infinito_param(
            ddi_noteskin::bundled::infinito::COLORS,
            ddi_ui::state::settings::ParamValue::Text("Flat".into()),
        );
        for settings in [settings, Settings::default()] {
            let play = crate::render_options(&settings);
            let colors = note_colors(&settings);
            assert_eq!(
                (colors.scheme, colors.chart),
                (play.note_colors, play.chart_colors)
            );
        }
    }
}

/// What the editor shows, as each shell keeps it for its field: its
/// picture, the skin it wants and what that skin's Lua is told, the note
/// colors (read from the settings for each layout and direction), and
/// whether it changed since it was drawn.
#[derive(Default)]
pub struct Shown {
    pub picture: Option<FieldPicture>,
    pub want: Option<(SkinWant, Env)>,
    pub colors: NoteColors,
    read_for: Option<(String, bool)>,
    pub changed: bool,
}

impl Shown {
    /// The editor's picture from now on (`None`: none; the texture goes),
    /// the settings read (`settings`) when its layout or direction is new.
    pub fn show(
        &mut self,
        picture: Option<&FieldPicture>,
        editor: &ddi_ui::state::editor::Editor,
        settings: impl FnOnce() -> Settings,
    ) {
        match picture {
            Some(p)
                if self
                    .read_for
                    .as_ref()
                    .is_none_or(|(l, r)| *l != p.layout || *r != p.reverse) =>
            {
                let settings = settings();
                let want = skin(&settings, p);
                if self.want.as_ref().is_none_or(|(w, _)| *w != want) {
                    let env = skin_env(&want, editor.song(), editor.chart());
                    self.want = Some((want, env));
                }
                self.colors = note_colors(&settings);
                self.read_for = Some((p.layout.clone(), p.reverse));
            }
            Some(_) => {}
            // The settings read again for the next editor.
            None => self.read_for = None,
        }
        self.picture = picture.cloned();
        self.changed = true;
    }
}
