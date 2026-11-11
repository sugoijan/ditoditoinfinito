//! What both shells (the web's `crates/web`, desktop's `crates/desktop`)
//! do between the player's settings (`ddi_ui::state::settings`) and the
//! renderer (`ddi_render`), so neither keeps a copy of its own:
//!
//! - the field's options for a play ([`render_options`]) and for Options'
//!   field preview ([`field_preview_options`]), with the note colors as
//!   the renderer has them ([`note_scheme`]); the skins a play may
//!   ready ([`play_skins`]) and the preview's ([`field_preview_skin`]),
//!   with the same values of their parameters;
//! - the ghost's lead as the HUD shows it ([`score_delta`]);
//! - [`stage`]: a Dancing☆Onigiri work's layers' textures, over a content
//!   each shell decodes its own way;
//! - [`scripted`]: a song's scripted backgrounds and foregrounds, their
//!   images' textures (the same contents) and quads;
//! - [`editor_field`]: the chart editor's field in the player's skin,
//!   its skin and its place;
//! - [`generative`]: a play's generated background, the texture it is
//!   drawn into and its place in the backdrop;
//! - [`play_view`]: a play drawn, in one frame body over each shell's
//!   platform parts;
//! - [`opponents`]: a multiplayer stage's CPU opponents in a quiet column
//!   beside the field, drawn by the play's 2D pass.
//! - [`battle_cue`]: a battle's incoming attack warning by the receptors
//!   and the waiting line under the power bar, drawn by the same pass.
//!
//! Platform-free: built natively and for wasm, with no platform code.

pub mod battle_cue;
pub mod dance;
pub mod editor_field;
pub mod field_preview;
pub mod generative;

pub mod hand;
pub mod opponents;
pub mod play_view;
pub mod scripted;
pub mod stage;

use ddi_chart::Layout;
use ddi_noteskin::lua::Env;
use ddi_noteskin::params::Values;
use ddi_render::field_preview::{BPM, FieldPreviewOptions, SkinWant};
use ddi_render::noteskin::NoteskinPlay;
use ddi_render::{ColorScheme, DrawList, Painter, RenderOptions, ScoreDelta, TextureKey};
use ddi_ui::state::history::GhostDelta;
use ddi_ui::state::settings::{BackgroundFrame, INFINITO, NoteColors, Settings};

/// Where the renderer showed the HUD's parts, as the announcer studio's
/// picture of a play rings them.
pub fn play_spots(s: ddi_render::HudSpots) -> ddi_ui::screens::stage::PlaySpots {
    ddi_ui::screens::stage::PlaySpots {
        life: s.life,
        combo: s.combo,
        receptors: s.receptors,
    }
}

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
        place: settings.field_place,
        reduced_motion: settings.reduced_motion.reduced(false),
        hud_time: None,
        hide_notes: false,
        show_deltas: settings.show_deltas,
        cancel_progress: 0.0,
        cancel_hint: "",
        restarting: false,
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

/// The field's filter on the start prompt's picture: the band `song`'s
/// chart `chart` will take on a screen of `size` (logical pixels), as
/// fractions of its width, and how dark (`None`: no filter, or no lanes).
pub fn prompt_filter(
    song: &ddi_chart::Song,
    chart: usize,
    settings: &Settings,
    size: [f32; 2],
) -> Option<([f32; 2], f32)> {
    if settings.field_filter <= 0.0 {
        return None;
    }
    let layout = song
        .charts
        .get(chart)
        .and_then(|c| song.layout_of(c))
        .filter(|l| !l.lanes.is_empty())?;
    let band = ddi_render::scene::filter_fraction(
        &layout,
        size[0],
        size[1],
        settings.reverse,
        settings.field_place,
    );
    Some((band, settings.field_filter))
}

/// The ghost's lead as the HUD shows it: a percentage, or whole points.
pub fn score_delta(delta: GhostDelta) -> ScoreDelta {
    match delta {
        GhostDelta::Percent(p) => ScoreDelta::Percent(p as f32),
        GhostDelta::Points(p) => ScoreDelta::Points(p.round() as i64),
    }
}

/// What the session says of the notes and the cancel gesture, and the
/// ghost's lead, as a frame of the play draws them.
pub fn apply_view(
    render: &mut RenderOptions,
    view: ddi_session::play::SessionView,
    ghost: Option<GhostDelta>,
) {
    render.hide_notes = view.hide_notes;
    render.cancel_progress = view.cancel_progress;
    render.cancel_hint = view.cancel_hint;
    render.restarting = view.restarting;
    render.ghost = ghost.map(score_delta);
}

/// The noteskin `play` for `frame`, with `player`'s events since its last.
/// Why its Lua stopped, in the frame it did, and whether by a stall rather
/// than the skin's fault ([`NoteskinPlay::take_lua_stop`]).
pub fn update_noteskin<'a>(
    play: &'a mut NoteskinPlay,
    frame: &ddi_engine::frame::Frame,
    player: &ddi_engine::player::Player,
    reverse: bool,
) -> Option<(&'a str, bool)> {
    let from = play.next_seq();
    play.update(frame, player.skin_events_since(from), reverse);
    play.take_lua_stop()
}

/// Draws `ui` into `list`, shifted to its viewport and measured by
/// `painter`, each drawn image asked of `painter` by its source and the
/// size drawn (only what is drawn is asked for).
pub fn ui_list(ui: &mut ddi_ui::Ui, painter: &mut Painter, list: &mut DrawList) {
    let vp = *ui.viewport();
    let scale = vp.scale;
    let shift = [
        1.0,
        0.0,
        0.0,
        1.0,
        vp.offset[0] / scale,
        vp.offset[1] / scale,
    ];
    let drawn = {
        let mut measure = painter.measure(scale);
        ui.draw_in(&mut measure, shift)
    };
    list.clear();
    ddi_render::actors2d::append(&drawn, list, &mut |t| {
        let image = ui.texture_source(t)?;
        let key = if image.blurred {
            TextureKey::blurred(image.source, image.side)
        } else {
            TextureKey::drawn(image.source, image.side * scale)
        };
        Some(painter.texture(key))
    });
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
    let layout = if Layout::is_builtin(&settings.style) {
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
        place: settings.field_place,
        show_deltas: settings.show_deltas,
        background: settings.bg_brightness,
        frame_aspect: preview_frame_aspect(settings.background_frame),
    }
}

/// The noteskin play would draw the preview's layout with under
/// `settings`, and what it is readied for (its key, the layout, the
/// direction, its parameters' values as [`play_skins`] passes them). The
/// demo has no colors of its own.
pub fn field_preview_skin(settings: &Settings, options: &FieldPreviewOptions) -> SkinWant {
    let key = settings.play_skin(&options.layout);
    SkinWant {
        params: settings.skin_params(&key),
        ..SkinWant::new(Some(key), options)
    }
}

/// The skins a play may draw with, best first ([`play_skins`]).
#[derive(Clone)]
pub struct SkinWants {
    /// Each one's key with its parameters' values; Infinito, always
    /// there, ends the list.
    pub keys: Vec<(String, Values)>,
    /// What the Lua of the one drawn with is told, but its parameters'
    /// values.
    pub env: Env,
}

/// The skins a play may draw `song`'s chart `chart` (of layout `layout`)
/// with under `settings`, best first, and what their Lua is told: the
/// play's timing shifted by `shift` (the song offset), its direction and
/// each skin's parameters' values. The chosen one, then what is drawn when
/// it is not there (gone, or chosen on another machine), the choice let
/// go as Options lets it go ([`Settings::let_go_missing_skins`]);
/// [`first_there`] picks.
pub fn play_skins(
    settings: &Settings,
    song: &ddi_chart::Song,
    chart: &ddi_chart::Chart,
    layout: &Layout,
    shift: f64,
) -> SkinWants {
    let mut settings = settings.clone();
    let want = |s: &Settings| {
        let k = s.play_skin(&layout.id);
        let params = s.skin_params(&k);
        (k, params)
    };
    let mut keys = vec![want(&settings)];
    while let Some((key, _)) = keys.last()
        && settings.let_go_missing_skins(|k| k != key)
    {
        keys.push(want(&settings));
    }
    let env = ddi_noteskin::lua::chart_env(song, chart, layout, settings.reverse).shifted(shift);
    SkinWants { keys, env }
}

/// The first of `wants` ([`play_skins`]) among the skins one can pick,
/// `there` (`(key, name)`; Infinito always is).
pub fn first_there(wants: SkinWants, there: &[(String, String)]) -> (String, Env) {
    let (key, params) = wants
        .keys
        .into_iter()
        .find(|(k, _)| k == INFINITO || there.iter().any(|(t, _)| t == k))
        .unwrap_or_else(|| (INFINITO.to_string(), Values::default()));
    (
        key,
        Env {
            params,
            ..wants.env
        },
    )
}

/// The pre-start diagram's skin for `controls` (a chart's, following it
/// rather than the menu's style): the preview's options, the skin
/// wanted and the layout it is readied for (the diagram's, one strip).
/// Both hosts use it, for the prompt's picture
/// ([`ReceptorSkin`]) and to ready the skin ahead, while
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

/// The pre-start diagram's picture for a host drawing it every frame:
/// the preview's options and skin ([`receptor_skin`]) worked out again
/// only when the picture's controls change, the pose (its time, the key
/// pressed) every frame. Both hosts use the same skin and pose.
#[derive(Default)]
pub struct ReceptorSkin {
    last: Option<(
        std::rc::Rc<ddi_ui::screens::prompt_controls::Controls>,
        FieldPreviewOptions,
        SkinWant,
        std::rc::Rc<Layout>,
    )>,
}

impl ReceptorSkin {
    pub fn options(
        &mut self,
        p: &ddi_ui::screens::prompt_controls::Picture,
    ) -> (
        &FieldPreviewOptions,
        &SkinWant,
        ddi_render::field_preview::ReceptorPreview,
    ) {
        let fresh = !self
            .last
            .as_ref()
            .is_some_and(|(c, ..)| std::rc::Rc::ptr_eq(c, &p.controls));
        if fresh {
            let (options, want, layout) = receptor_skin(&p.controls);
            self.last = Some((p.controls.clone(), options, want, layout.into()));
        }
        let (_, options, want, layout) = self.last.as_ref().expect("set above");
        let pose = ddi_render::field_preview::ReceptorPreview {
            layout: layout.clone(),
            time: p.time,
            active: p.active,
        };
        (options, want, pose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_engine::FieldPlace;

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
            field_place: FieldPlace::Right,
            show_deltas: true,
            bg_brightness: 0.25,
            ..Settings::default()
        };
        let render = render_options(&settings);
        assert!(render.reverse && render.show_deltas && !render.hide_notes);
        assert_eq!(render.place, FieldPlace::Right);
        assert_eq!(render.background, 0.25);
        assert_eq!(render.note_colors, note_scheme(settings.note_colors()));
        assert!(render.ghost.is_none() && render.frame_aspect.is_none());
    }

    #[test]
    fn the_prompts_filter_is_the_plays_band() {
        let song = ddi_chart::formats::sm::parse_simfile(
            "#TITLE:T;#OFFSET:0;#BPMS:0=120;\n\
             #NOTES:\n dance-single:\n:\n Hard:\n 9:\n 0,0,0,0,0:\n1000\n0100\n0010\n0001\n;\n",
            "sm",
        )
        .unwrap();
        let layout = song.layout_of(&song.charts[0]).unwrap();
        let size = [1280.0, 720.0];
        let off = Settings {
            field_filter: 0.0,
            ..Settings::default()
        };
        assert_eq!(prompt_filter(&song, 0, &off, size), None);
        let mut middles = Vec::new();
        for (reverse, place) in [
            (false, FieldPlace::Center),
            (true, FieldPlace::Center),
            (false, FieldPlace::Left),
            (false, FieldPlace::Right),
        ] {
            let on = Settings {
                field_filter: 0.6,
                reverse,
                field_place: place,
                ..Settings::default()
            };
            let band =
                ddi_render::scene::filter_fraction(&layout, size[0], size[1], reverse, place);
            assert_eq!(prompt_filter(&song, 0, &on, size), Some((band, 0.6)));
            middles.push(band[0]);
            // A chart the song does not have: none.
            assert_eq!(prompt_filter(&song, 1, &on, size), None);
        }
        // A side's band is on that half, centered on it.
        assert_eq!(middles[..2], [0.5, 0.5]);
        assert_eq!(middles[2..], [0.25, 0.75]);
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
        let there = [("dance/mine".to_string(), "mine".to_string())];
        for chosen in [None, Some("dance/mine")] {
            settings.noteskin = chosen.map(str::to_string);
            let preview = field_preview_skin(&settings, &field_preview_options(&settings));
            let wants = play_skins(&settings, &song, chart, &layout, 0.0);
            let (key, env) = first_there(wants, &there);
            assert_eq!(preview.key.as_deref(), Some(key.as_str()));
            assert_eq!(preview.env(&layout).params, env.params);
            assert_eq!(env.params, settings.skin_params(chosen.unwrap_or(INFINITO)));
            assert!(!env.params.is_empty());
        }
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
        let wants = play_skins(&settings, &song, chart, &layout, 0.0);
        let keys: Vec<_> = wants.keys.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["pump/mine", "dance/mine", INFINITO]);
        let pick = |there: &[&str]| {
            let there: Vec<(String, String)> = there
                .iter()
                .map(|k| (k.to_string(), String::new()))
                .collect();
            let (k, env) = first_there(wants.clone(), &there);
            (k, env.params)
        };
        assert_eq!(pick(&["pump/mine", "dance/mine"]).0, "pump/mine");
        assert_eq!(pick(&["dance/mine"]).0, "dance/mine");
        // Infinito, with its parameters.
        assert_eq!(pick(&[]), (INFINITO.into(), settings.skin_params(INFINITO)));
        // Nothing chosen: Infinito alone.
        let wants = play_skins(&Settings::default(), &song, chart, &layout, 0.0);
        assert_eq!(wants.keys.len(), 1);
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

pub mod dance_runtime;

pub mod hand_field_preview;
