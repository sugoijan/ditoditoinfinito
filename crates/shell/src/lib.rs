//! What both shells (the web's `crates/web`, desktop's `crates/desktop`)
//! do between the player's settings (`ddi_ui::state::settings`) and the
//! renderer (`ddi_render`), so neither keeps a copy of its own:
//!
//! - the field's options for a play ([`render_options`]) and for Options'
//!   field preview ([`field_preview_options`]), with the note colors as
//!   the renderer has them ([`note_scheme`]); the skin a play readies
//!   ([`play_skin`], and what it draws when that is not there,
//!   [`play_skins`]) and the preview's ([`field_preview_skin`]), with the
//!   same values of its parameters;
//! - the ghost's lead as the HUD shows it ([`score_delta`]);
//! - [`stage`]: a Dancing☆Onigiri work's layers' textures, over a content
//!   each shell decodes its own way;
//! - [`scripted`]: a song's scripted backgrounds and foregrounds, their
//!   images' textures (the same contents) and quads;
//! - [`editor_field`]: the chart editor's field in the player's skin,
//!   its skin and its place.
//!
//! Platform-free: built natively and for wasm, with no platform code.

pub mod editor_field;
pub mod scripted;
pub mod stage;

use ddi_chart::Layout;
use ddi_noteskin::lua::Env;
use ddi_noteskin::params::Values;
use ddi_render::field_preview::{BPM, FieldPreviewOptions, SkinWant};
use ddi_render::{ColorScheme, RenderOptions, ScoreDelta};
use ddi_ui::state::history::GhostDelta;
use ddi_ui::state::settings::{BackgroundFrame, INFINITO, NoteColors, Settings};

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
        reduced_motion: settings.reduced_motion.reduced(false),
        hud_time: None,
        hide_notes: false,
        show_deltas: settings.show_deltas,
        cancel_progress: 0.0,
        cancel_hint: "",
        note_colors: note_scheme(settings.note_colors()),
        // No skin's setting asks for a chart's own colors now.
        chart_colors: false,
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
        note_colors: note_scheme(settings.note_colors()),
        field_filter: settings.field_filter,
        show_deltas: settings.show_deltas,
        background: settings.bg_brightness,
        frame_aspect: preview_frame_aspect(settings.background_frame),
    }
}

/// The noteskin play would draw the preview's layout with under
/// `settings`, and what it is readied for (its key, the layout, the
/// direction, its parameters' values as [`play_skin`] passes them): key
/// `None` for the built-in arrows. The demo has no colors of its own.
pub fn field_preview_skin(settings: &Settings, options: &FieldPreviewOptions) -> SkinWant {
    let key = settings.play_skin(&options.layout);
    SkinWant {
        params: key
            .as_deref()
            .map(|k| settings.skin_params(k))
            .unwrap_or_default(),
        ..SkinWant::new(key, options)
    }
}

/// The noteskin a play draws `song`'s chart `chart` (of layout `layout`)
/// with under `settings`, and what its Lua is told: the play's timing
/// shifted by `shift` (the song offset), its direction and the values of
/// the skin's parameters. `None` for the built-in arrows.
pub fn play_skin(
    settings: &Settings,
    song: &ddi_chart::Song,
    chart: &ddi_chart::Chart,
    layout: &Layout,
    shift: f64,
) -> Option<(String, Env)> {
    let key = settings.play_skin(&layout.id)?;
    let env = Env {
        params: settings.skin_params(&key),
        ..ddi_noteskin::lua::chart_env(song, chart, layout, settings.reverse).shifted(shift)
    };
    Some((key, env))
}

/// The skins a play may draw with, best first ([`play_skins`]).
#[derive(Clone)]
pub struct SkinWants {
    /// Each one's key with its parameters' values; `None`, the built-in
    /// arrows, ends the list.
    pub keys: Vec<Option<(String, Values)>>,
    /// What the Lua of the one drawn with is told, but its parameters'
    /// values.
    pub env: Env,
}

/// The skins a play may draw `song`'s chart `chart` with, best first
/// ([`play_skin`]): the chosen one, then what is drawn when it is not
/// there (gone, or chosen on another machine), the choice let go as
/// Options lets it go ([`Settings::let_go_missing_skins`]). `None` for
/// the built-in arrows; [`first_there`] picks.
pub fn play_skins(
    settings: &Settings,
    song: &ddi_chart::Song,
    chart: &ddi_chart::Chart,
    layout: &Layout,
    shift: f64,
) -> Option<SkinWants> {
    let mut settings = settings.clone();
    let want = |s: &Settings| {
        s.play_skin(&layout.id).map(|k| {
            let params = s.skin_params(&k);
            (k, params)
        })
    };
    let mut keys = vec![Some(want(&settings)?)];
    while let Some(Some((key, _))) = keys.last()
        && settings.let_go_missing_skins(|k| k != key)
    {
        keys.push(want(&settings));
    }
    let env = ddi_noteskin::lua::chart_env(song, chart, layout, settings.reverse).shifted(shift);
    Some(SkinWants { keys, env })
}

/// The first of `wants` ([`play_skins`]) among the skins one can pick,
/// `there` (`(key, name)`; Infinito always is): `None` for the built-in
/// arrows.
pub fn first_there(wants: SkinWants, there: &[(String, String)]) -> Option<(String, Env)> {
    let (key, params) = wants
        .keys
        .into_iter()
        .find(|w| {
            w.as_ref()
                .is_none_or(|(k, _)| k == INFINITO || there.iter().any(|(t, _)| t == k))
        })
        .flatten()?;
    Some((
        key,
        Env {
            params,
            ..wants.env
        },
    ))
}

/// The pre-start diagram's skin for `controls` (a chart's, following it
/// rather than the menu's style): the preview's options, the skin
/// wanted and the layout it is readied for (the diagram's, one strip).
/// Both hosts use it, for the prompt's picture
/// ([`receptor_preview_options`]) and to ready the skin ahead, while
/// song select shows the chart.
pub fn receptor_skin(
    controls: &ddi_ui::screens::prompt_controls::Controls,
) -> (FieldPreviewOptions, SkinWant, Layout) {
    let mut settings = controls.settings.clone();
    settings.style = controls.layout.id.clone();
    let mut options = field_preview_options(&settings);
    options.layout = controls.layout.id.clone();
    let want = field_preview_skin(&settings, &options);
    (options, want, controls.diagram_layout())
}

/// Settings for the pre-start diagram's picture ([`receptor_skin`] with
/// the picture's pose). Both hosts use the same skin and pose.
pub fn receptor_preview_options(
    p: &ddi_ui::screens::prompt_controls::Picture,
) -> (
    FieldPreviewOptions,
    SkinWant,
    ddi_render::field_preview::ReceptorPreview,
) {
    let (options, want, layout) = receptor_skin(&p.controls);
    let pose = ddi_render::field_preview::ReceptorPreview {
        layout,
        time: p.time,
        active: p.active,
    };
    (options, want, pose)
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
        assert_eq!(render.note_colors, note_scheme(settings.note_colors()));
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

    /// The preview and a play pass the skin the same values of its
    /// parameters: Infinito's whether chosen or not, a chosen skin's own.
    #[test]
    fn the_preview_and_a_play_ready_the_skin_with_the_same_values() {
        use ddi_ui::state::settings::{INFINITO, ParamValue};
        let song = ddi_engine::demo::song("dance-single", BPM).unwrap();
        let chart = &song.charts[0];
        let layout = Layout::dance_single();
        let mut settings = Settings::default();
        settings.set_infinito_param("Onigiri", ParamValue::Bool(false));
        settings.set_infinito_param("Explosions", ParamValue::Bool(false));
        settings.set_skin_param("dance/mine", "Size", Some(ParamValue::Number(2.0)), None);
        for chosen in [None, Some("dance/mine")] {
            settings.noteskin = chosen.map(str::to_string);
            let preview = field_preview_skin(&settings, &field_preview_options(&settings));
            let (key, env) = play_skin(&settings, &song, chart, &layout, 0.0).unwrap();
            assert_eq!(preview.key.as_deref(), Some(key.as_str()));
            assert_eq!(preview.env(&layout).params, env.params);
            assert_eq!(env.params, settings.skin_params(chosen.unwrap_or(INFINITO)));
            assert!(!env.params.is_empty());
        }
        // Explicit internal fallback: the built-in arrows, in both.
        settings.noteskin = Some(ddi_ui::state::settings::BUILT_IN.into());
        assert!(play_skin(&settings, &song, chart, &layout, 0.0).is_none());
        let preview = field_preview_skin(&settings, &field_preview_options(&settings));
        assert_eq!(preview.key, None);
        assert_eq!(
            render_options(&settings).note_colors,
            note_scheme(NoteColors::Vivid)
        );
    }

    /// A chosen skin that is not there plays as Options shows it: let
    /// go, the pump skin before the dance one.
    #[test]
    fn a_skin_not_there_plays_as_if_not_chosen() {
        use ddi_ui::state::settings::ParamValue;
        let song = ddi_engine::demo::song("pump-single", BPM).unwrap();
        let chart = &song.charts[0];
        let layout = song.layout_of(chart).unwrap();
        let mut settings = Settings {
            noteskin: Some("dance/mine".into()),
            pump_noteskin: Some("pump/mine".into()),
            ..Settings::default()
        };
        settings.set_infinito_param("Onigiri", ParamValue::Bool(false));
        let wants = play_skins(&settings, &song, chart, &layout, 0.0).unwrap();
        let keys: Vec<_> = wants
            .keys
            .iter()
            .map(|w| w.as_ref().map(|(k, _)| k.as_str()))
            .collect();
        assert_eq!(
            keys,
            [Some("pump/mine"), Some("dance/mine"), Some(INFINITO)]
        );
        let pick = |there: &[&str]| {
            let there: Vec<(String, String)> = there
                .iter()
                .map(|k| (k.to_string(), String::new()))
                .collect();
            first_there(wants.clone(), &there).map(|(k, env)| (k, env.params))
        };
        assert_eq!(pick(&["pump/mine", "dance/mine"]).unwrap().0, "pump/mine");
        assert_eq!(pick(&["dance/mine"]).unwrap().0, "dance/mine");
        // Infinito, with its parameters.
        assert_eq!(
            pick(&[]),
            Some((INFINITO.into(), settings.skin_params(INFINITO)))
        );
        // Explicit internal fallback: the built-in arrows.
        settings.noteskin = Some(ddi_ui::state::settings::BUILT_IN.into());
        settings.pump_noteskin = None;
        assert!(play_skins(&settings, &song, chart, &layout, 0.0).is_none());
        // Nothing chosen: Infinito alone.
        let wants = play_skins(&Settings::default(), &song, chart, &layout, 0.0).unwrap();
        assert_eq!(wants.keys.len(), 1);
        // Nothing chosen readies the new default.
        (settings.noteskin, settings.pump_noteskin) = (None, None);
        assert!(play_skins(&settings, &song, chart, &layout, 0.0).is_some());
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
