//! The options screen on the player's settings (`docs/plans/ui-core.md`,
//! step 6.3): every setting as a row of [`OptionsData`], each change
//! applied to [`Settings`] and saved, and the actions (bind, reset,
//! export, the volume sample) named for the shell. Pure: the shell says
//! what only it knows ([`World`]: the noteskins, the controllers
//! connected, the library's counts) and carries out what only it can
//! ([`Act`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::state::settings::HandLayout;
use ddi_chart::Layout;
use ddi_engine::rules::danoni::JudgeRange;
use ddi_engine::rules::ddr::DdrGauge;
use ddi_engine::rules::presets::{HoldStyle, RulesetMode};
use ddi_engine::rules::{FailType, JUDGE_SCALES, LIFE_SCALES, LifeType, StepManiaOptions};
use ddi_engine::scroll::SpeedSource;
use ddi_engine::{
    Appearance, AtOnce, ConnectedPad, CoverMode, FieldPlace, LaneCover, PadUse, ScrollAction,
    TimingCut, Turn,
};
use ddi_platform::DeviceId;

use super::data::{
    OptionDef, OptionKind, OptionSection, OptionsData, Platform, lane_name, style_name, trim_number,
};
use super::options::CALIBRATE;
use crate::state::bind::Bound;
use crate::state::settings::{
    BUNDLED, BackgroundFrame, CenterVersions, CursorStyle, FIELD_FILTERS, GeneratedBackground,
    Graphics, INFINITO, OpponentFields, PadBindings, Palette, ParamValue, ReducedMotion, START_COUNTDOWNS,
    JudgmentWords, ScoreDisplay, Settings, VideoMode, slider_to_volume, volume_to_slider,
};
use ddi_l10n::UiText;
use ddi_l10n::msg::{common, options as m};
use ddi_noteskin::params::{Kind as ParamKind, Parameter};

/// The row whose button judges the kept plays again under the rules
/// chosen ([`Act::Rejudge`]).
pub const REJUDGE: &str = "rejudge";

/// The volume sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sample {
    /// No song to play it with.
    #[default]
    None,
    Ready,
    Playing,
}

/// What the shell knows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    /// Whose wording (the shell's platform: desktop keeps nothing of
    /// its own to remove, and saves the export to a file).
    pub platform: super::data::Platform,
    /// The noteskins one can choose `(key, name)`: bundled, then imported.
    pub noteskins: Vec<(String, String)>,
    /// Whether [`World::noteskins`] lists them all (desktop's folders'
    /// are walked after the screen opens).
    pub noteskins_listed: bool,
    /// The announcers one can choose `(key, name)`: ours (their keys
    /// `bundled:`, `ddi_library::announcer::bundled_key`), then the
    /// imported (the web) or the song folders' (desktop), by name.
    pub announcers: Vec<(String, String)>,
    /// Those made in the announcer studio on desktop (their keys), which
    /// can be removed.
    pub made_announcers: Vec<String>,
    /// The default skin's Lua is off for the session.
    pub default_lua_off: bool,
    /// The parameters skins declare (`docs/plans/skin-parameters.md`),
    /// by key; Infinito's are known without it.
    pub skin_parameters: std::collections::BTreeMap<String, Vec<Parameter>>,
    /// Layouts the library has charts for (offered for bindings).
    pub library_styles: Vec<Layout>,
    /// The controllers connected, one by one.
    pub connected: Vec<ConnectedPad>,
    /// What controller hints are drawn from (the UI's, `options_shell`
    /// keeps it fresh): the lane cover's help names the Start bound.
    pub hints: std::rc::Rc<crate::controller_hints::Hints>,
    /// Whether the platform reads controllers at all.
    pub pads_supported: bool,
    pub sample: Sample,
    /// Imported songs, shared background images, plays kept.
    pub imported: usize,
    pub shared: usize,
    pub plays: usize,
    /// An import runs (nothing is removed meanwhile).
    pub importing: bool,
    /// Plays are being imported (the history's row says so).
    pub importing_plays: bool,
    /// The song folders the library reads in place (desktop), and what
    /// the last scan found; `None` where songs are imported instead (the
    /// web).
    pub folders: Option<Vec<String>>,
    pub library_note: String,
    /// Sharing by link is switched on (`Settings::sharing`): the
    /// Sharing section shows, with the server the uploads go to.
    pub sharing: bool,
    pub share_url: String,
}

/// What a confirmation is for.
#[derive(Clone, Debug, PartialEq)]
pub enum Reset {
    /// Every setting (device calibrations too) but the songs' offsets.
    Settings,
    /// Imported songs, their files and offsets, imported noteskins and
    /// announcers, and shared images.
    Songs,
    /// Both, and the bundled songs' offsets: like a first visit.
    Everything,
    /// An imported noteskin (its key).
    Noteskin(String),
    /// An imported announcer (its key).
    Announcer(String),
    /// Every play kept (the bests go with them).
    History,
}

impl Reset {
    /// The confirmation's title, text and button.
    pub fn texts(&self, model: &OptionsModel) -> (String, String, String) {
        let plays = model.world.plays;
        let (title, body, yes) = match self {
            Reset::Settings => (
                m::confirm_settings(),
                m::confirm_settings_body(),
                m::confirm_settings_button(),
            ),
            // Desktop reads song folders in place: they are forgotten,
            // never deleted.
            Reset::Songs if model.world.folders.is_some() => (
                m::confirm_folders(),
                m::confirm_folders_body(model.world.folders.as_ref().map_or(0, Vec::len)),
                m::confirm_folders_button(),
            ),
            Reset::Songs => (
                m::confirm_songs(),
                m::confirm_songs_body(model.world.imported, model.world.shared),
                m::confirm_songs_button(),
            ),
            Reset::Everything => (
                m::confirm_everything(),
                m::confirm_everything_body(model.songs_wording(), plays),
                m::confirm_everything_button(),
            ),
            Reset::History => (
                m::confirm_history(),
                m::confirm_history_body(plays),
                m::confirm_history_button(),
            ),
            Reset::Noteskin(key) => {
                let skin = model.skin_name(Some(key));
                (
                    m::confirm_noteskin(),
                    if crate::state::settings::is_pump_skin(key) {
                        m::confirm_pump_noteskin_body(&skin)
                    } else {
                        m::confirm_noteskin_body(&skin)
                    },
                    m::confirm_noteskin_button(),
                )
            }
            Reset::Announcer(key) => (
                m::confirm_announcer(),
                m::confirm_announcer_body(model.announcer_name(key), model.world.platform.l10n()),
                m::confirm_announcer_button(),
            ),
        };
        (title.into(), body.into(), yes.into())
    }
}

/// The play history's buttons: Export and Reset with plays kept, Import
/// unless one runs.
fn history_buttons(plays: usize, importing: bool) -> &'static [HistoryButton] {
    use HistoryButton::*;
    match (plays, importing) {
        (0, false) => &[Import],
        (0, true) => &[],
        (_, false) => &[Export, Import, Reset],
        (_, true) => &[Export],
    }
}

/// A button of the play history's row, as [`OptionsModel::action`] tells
/// them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryButton {
    Export,
    Import,
    Reset,
}

impl HistoryButton {
    fn label(self) -> UiText {
        match self {
            HistoryButton::Export => m::history_export(),
            HistoryButton::Import => m::history_import(),
            HistoryButton::Reset => m::history_reset(),
        }
    }
}

/// A button of a controller's row, as [`OptionsModel::action`] tells them
/// apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PadButton {
    Bind,
    Clear,
    Defaults,
    Forget,
}

impl PadButton {
    fn label(self) -> UiText {
        match self {
            PadButton::Bind => m::pad_bind(),
            PadButton::Clear => m::pad_clear(),
            PadButton::Defaults => m::pad_defaults(),
            PadButton::Forget => m::pad_forget(),
        }
    }
}

/// A button of a controller's lane row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaneButton {
    /// One more control for the lane (its next press).
    Add,
    Clear,
}

impl LaneButton {
    fn label(self) -> UiText {
        match self {
            LaneButton::Add => m::pad_lane_add(),
            LaneButton::Clear => m::pad_lane_clear(),
        }
    }
}

/// What a change asks of the shell, beyond showing the rows again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Nothing more.
    Plain,
    /// The volume changed: the sample follows it.
    Volume,
    /// The announcer or its volume changed: a line is spoken.
    Announce,
    /// The UI's language changed: the screens are built again in it.
    Language,
    /// Reduced motion changed: followed at once.
    Motion,
}

/// What an action asks of the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// Done here (the rows changed).
    Done,
    /// Nothing (a row that only says something).
    Nothing,
    /// The guided binding, for a device or the next one pressed.
    Bind(Option<DeviceId>),
    /// One more key for a lane of the keyboard.
    AddKey(usize),
    /// One more control for a lane of controller `id` (its next press).
    AddButton {
        id: String,
        lane: usize,
    },
    /// Start or stop the volume sample (in the gesture).
    Sample,
    /// Download the play history.
    Export,
    /// Judge the kept plays again under the rules chosen now.
    Rejudge,
    /// Import plays from an export (the platform's picker, inside the
    /// press).
    ImportPlays,
    /// Ask first, then [`OptionsModel::reset`] (and the shell's part).
    Confirm(Reset),
    /// Calibration (its own screen).
    Calibrate,
    /// The announcer studio (its own page).
    Studio,
    /// Autoplay a bundled song (on WebGL2).
    Demo {
        gl: bool,
    },
    /// Add a song folder (the platform's picker), forget one (by its
    /// place in the list), or read them all again (desktop).
    AddFolder,
    RemoveFolder(usize),
    Rescan,
    /// Ask for the upload token (a dialog with a text field).
    ShareToken,
    /// Open the shares dialog (the shell reads the listing into it).
    ListShares,
}

pub struct OptionsModel {
    pub settings: Settings,
    pub world: World,
    /// The layout whose bindings the controls show (not saved).
    pub bind_layout: Layout,
    /// What the last action did.
    pub notice: String,
    /// The layout the field's preview shows (not saved; `None`: the
    /// style chosen).
    pub preview_layout: Option<String>,
    /// Changes are not saved (the start prompt's play options,
    /// `super::prompt_options`).
    pub transient: bool,
}

impl OptionsModel {
    pub fn new(settings: Settings, world: World) -> OptionsModel {
        let mut m = OptionsModel {
            bind_layout: Layout::dance_single(),
            settings,
            world,
            notice: String::new(),
            preview_layout: None,
            transient: false,
        };
        m.follow_style();
        m
    }

    /// The bindings shown are the style's, once the layouts are known.
    pub fn follow_style(&mut self) {
        let style = self.settings.style.clone();
        if let Some(l) = self.bind_layouts().into_iter().find(|l| l.id == style) {
            self.bind_layout = l;
        }
    }

    /// Layouts the bindings offer: StepMania's, then the library's.
    pub fn bind_layouts(&self) -> Vec<Layout> {
        let mut layouts = Layout::builtins();
        for l in &self.world.library_styles {
            if !layouts.iter().any(|x| x.id == l.id) {
                layouts.push(l.clone());
            }
        }
        if !layouts.iter().any(|l| l.id == self.bind_layout.id) {
            layouts.push(self.bind_layout.clone());
        }
        layouts
    }

    /// The controllers the controls list: connected ones (identical ones
    /// once, with how many), then saved ones, most recently used first.
    /// `(id, standard, count)`.
    pub fn pads(&self) -> Vec<(String, bool, usize)> {
        let mut pads: Vec<(String, bool, usize)> = Vec::new();
        for p in &self.world.connected {
            match pads.iter_mut().find(|(id, _, _)| *id == p.id) {
                Some(c) => c.2 += 1,
                None => pads.push((p.id.clone(), p.standard, 1)),
            }
        }
        let mut saved: Vec<&PadBindings> = self
            .settings
            .controls
            .pads
            .iter()
            .filter(|p| !pads.iter().any(|(id, _, _)| *id == p.id))
            .collect();
        saved.sort_by(|a, b| b.last_seen.total_cmp(&a.last_seen));
        pads.extend(saved.into_iter().map(|p| (p.id.clone(), false, 0)));
        for id in self.settings.controller_styles.keys() {
            if !pads.iter().any(|p| &p.0 == id) {
                pads.push((id.clone(), false, 0));
            }
        }
        pads
    }

    /// The noteskin chosen, by name.
    pub fn skin_name(&self, key: Option<&str>) -> String {
        match key {
            None | Some(INFINITO) => "Infinito".into(),
            Some(k) => self
                .world
                .noteskins
                .iter()
                .find(|(key, _)| key == k)
                .map_or_else(|| k.to_string(), |(_, n)| n.clone()),
        }
    }

    /// An announcer's name (its key when it is not listed).
    pub fn announcer_name(&self, key: &str) -> String {
        self.world
            .announcers
            .iter()
            .find(|(k, _)| k == key)
            .map_or_else(|| key.to_string(), |(_, n)| n.clone())
    }

    /// The rows.
    pub fn data(&self) -> OptionsData {
        let s = &self.settings;
        let shown = self.preview_layout.as_deref().unwrap_or(&s.style);
        let layouts = self.bind_layouts();
        let style = layouts
            .iter()
            .find(|l| l.id == shown)
            .map_or_else(|| UiText::data(shown), |l| style_name(&l.id, &l.name));
        // A line of tags, " · " between them.
        let mut preview: Vec<String> = vec![
            UiText::data(self.skin_name(s.noteskin_for(shown))).into(),
            style.into(),
            speed_text(s.speed),
        ];
        if s.reverse {
            preview.push(m::preview_reverse().into());
        }
        if s.dancing_hand_for(shown) {
            preview.push(m::preview_standing_hand().into());
        }
        OptionsData {
            sections: vec![
                self.scroll(),
                self.notes(),
                self.rules(),
                self.timing(),
                self.sound(),
                self.display(),
                self.arrows(),
                self.controls(&layouts),
                self.data_section(),
            ]
            .into_iter()
            .chain(self.folders())
            .chain(self.sharing())
            .collect(),
            preview_layout: Some(self.preview_layout_option(&layouts)),
            preview_whole_play: s.dancing_hand_for(shown),
            preview: preview.join(" · "),
            notice: self.notice.clone(),
        }
    }

    /// The rows a play's start prompt picks from, as Options shows them:
    /// its Scroll, Notes and Arrows sections' alone.
    pub fn prompt_rows(&self) -> impl Iterator<Item = OptionDef> {
        [self.scroll(), self.notes(), self.arrows()]
            .into_iter()
            .flat_map(|s| s.options)
    }

    /// The speeds offered: ×0.25 to ×10 by 0.25, and the one stored when
    /// it is between (an older setting, a hand edit), until changed.
    fn speeds(&self) -> Vec<f64> {
        let mut v: Vec<f64> = (1..=40).map(|q| q as f64 * 0.25).collect();
        let s = self.settings.speed;
        if !v.iter().any(|x| (x - s).abs() < 1e-6) {
            let at = v.iter().position(|x| *x > s).unwrap_or(v.len());
            v.insert(at, s);
        }
        v
    }

    /// The settings the field's preview shows: these, on the layout
    /// chosen for it, without a chosen skin that is not there (as a play
    /// draws it, and the Arrows row shows it).
    pub fn preview_settings(&self) -> Settings {
        let mut s = self.settings.clone();
        if let Some(l) = &self.preview_layout {
            s.style = l.clone();
        }
        if self.world.noteskins_listed {
            s.let_go_missing_skins(|k| self.world.noteskins.iter().any(|(key, _)| key == k));
        }
        s
    }

    fn scroll(&self) -> OptionSection {
        let s = &self.settings;
        let values = self.speeds();
        let speeds: Vec<String> = values.iter().map(|v| speed_text(*v)).collect();
        let at = values
            .iter()
            .position(|v| (v - s.speed).abs() < 1e-6)
            .unwrap_or(0);
        section(
            "Scroll",
            m::section_scroll(),
            vec![
                row(
                    "speed",
                    m::speed(),
                    OptionKind::Stepper(speeds.into()),
                    at,
                    m::speed_description(),
                ),
                choice(
                    "speed_source",
                    m::speed_source(),
                    labels(&SPEED_SOURCES),
                    pos(&SPEED_SOURCES, s.speed_source),
                    m::speed_source_description(),
                ),
                choice(
                    "reverse",
                    m::reverse(),
                    on_off(),
                    s.reverse as usize,
                    m::reverse_description(),
                ),
                choice(
                    "scroll_action",
                    m::scroll_action(),
                    labels(&SCROLL_ACTIONS),
                    pos(&SCROLL_ACTIONS, s.scroll_action),
                    m::scroll_action_description(),
                ),
            ],
        )
    }

    /// The lane cover's description: how it moves in play, on the
    /// keyboard or the controller the hints follow.
    fn cover_description(&self) -> UiText {
        if !self.world.hints.active() {
            m::lane_cover_description_keys()
        } else {
            match self.world.hints.cover_start() {
                Some(start) => m::lane_cover_description_pad(start),
                None => m::lane_cover_description_unbound(),
            }
        }
    }

    fn notes(&self) -> OptionSection {
        let s = &self.settings;
        let t = &s.transform;
        let mut cut = choice(
            "cut",
            m::cut(),
            labels(&CUTS),
            pos(&CUTS, t.cut),
            m::cut_description(),
        );
        cut.heading = Some(m::heading_simplify().into());
        let cover = s.lane_cover;
        let mut rows = vec![
            choice(
                "turn",
                m::turn(),
                labels(&TURNS),
                pos(&TURNS, t.turn),
                m::turn_description(),
            ),
            choice(
                "appearance",
                m::appearance(),
                labels(&APPEARANCES),
                pos(&APPEARANCES, s.appearance),
                m::appearance_description(),
            ),
            choice(
                "lane_cover",
                m::lane_cover(),
                labels(&COVER_MODES),
                pos(&COVER_MODES, cover.mode),
                self.cover_description(),
            ),
        ];
        if cover.mode.sudden() {
            rows.push(cover_height(
                "cover_sudden",
                m::cover_sudden_height(),
                m::cover_sudden_description(),
                cover.sudden,
            ));
        }
        if cover.mode.hidden() {
            rows.push(cover_height(
                "cover_hidden",
                m::cover_hidden_height(),
                m::cover_hidden_description(),
                cover.hidden,
            ));
        }
        rows.extend([
            cut,
            choice(
                "simplify_onigiri",
                m::simplify_onigiri(),
                on_off(),
                s.simplify_onigiri as usize,
                m::simplify_onigiri_description(),
            ),
            choice(
                "simplify_center",
                m::simplify_center(),
                labels(&CENTER_VERSIONS),
                pos(&CENTER_VERSIONS, s.simplify_center),
                m::simplify_center_description(),
            ),
            choice(
                "at_once",
                m::at_once(),
                labels(&AT_ONCE),
                pos(&AT_ONCE, t.at_once),
                m::at_once_description(),
            ),
            choice(
                "no_stretch",
                m::no_stretch(),
                on_off(),
                t.no_stretch as usize,
                m::no_stretch_description(),
            ),
            choice(
                "no_holds",
                m::no_holds(),
                on_off(),
                t.no_holds as usize,
                m::no_holds_description(),
            ),
        ]);
        let mut pump_single = choice(
            "convert_pump_single",
            m::convert_pump_single(),
            on_off(),
            s.convert_pump_single as usize,
            m::convert_pump_single_description(),
        );
        pump_single.heading = Some(m::heading_convert().into());
        rows.extend([
            pump_single,
            choice(
                "convert_five_keys",
                m::convert_five_keys(),
                on_off(),
                s.convert_five_keys as usize,
                m::convert_five_keys_description(),
            ),
        ]);
        section("Notes", m::section_notes(), rows)
    }

    fn rules(&self) -> OptionSection {
        let s = &self.settings;
        let presets = ddi_engine::rules::presets::all();
        // The rulesets keep the names their games give them.
        let names: Vec<String> = presets
            .iter()
            .map(|r| {
                if r.approximate {
                    m::ruleset_approximate(&r.name).into()
                } else {
                    UiText::data(&r.name).into()
                }
            })
            .collect();
        let at = presets.iter().position(|r| r.id == s.ruleset).unwrap_or(0);
        let gauge_at = DANONI_GAUGES
            .iter()
            .position(|(g, _)| *g == s.danoni.gauge.as_deref())
            .unwrap_or(0);
        // Dancing☆Onigiri's gauges and judge ranges, and DDR's gauges,
        // keep their games' names.
        let gauges = DANONI_GAUGES.map(|(gauge, border)| match (gauge, border) {
            (None, _) => m::danoni_gauge_first(),
            (Some(g), true) => m::danoni_gauge_border(g),
            (Some(g), false) => UiText::data(g),
        });
        let mut gauge = choice(
            "danoni.gauge",
            m::danoni_gauge(),
            gauges,
            gauge_at,
            m::danoni_gauge_description(),
        );
        gauge.heading = Some(m::heading_danoni().into());
        // DDI's own rows under DDI; StepMania's under the presets that
        // take them (DDI has its own difficulty, life and holds).
        let ddi = s.ruleset == ddi_engine::rules::ddi::ID;
        let mut rows = vec![
            choice(
                "ruleset_mode",
                m::ruleset_mode(),
                labels(&RULESET_MODES),
                pos(&RULESET_MODES, s.ruleset_mode),
                m::ruleset_mode_description(),
            ),
            row(
                "ruleset",
                m::ruleset(),
                OptionKind::Choices(names),
                at,
                m::ruleset_description(),
            ),
        ];
        if ddi {
            rows.extend([
                choice(
                    "ddi.difficulty",
                    m::ddi_difficulty(),
                    labels(&DDI_DIFFICULTIES),
                    pos(&DDI_DIFFICULTIES, s.ddi.difficulty),
                    m::ddi_difficulty_description(),
                ),
                choice(
                    "ddi.boos",
                    m::ddi_boos(),
                    on_off(),
                    s.ddi.boos as usize,
                    m::ddi_boos_description(),
                ),
                choice(
                    "ddi.practice",
                    m::ddi_practice(),
                    on_off(),
                    s.ddi.practice as usize,
                    m::ddi_practice_description(),
                ),
                choice(
                    "ddi.score",
                    m::ddi_score(),
                    labels(&SCORE_DISPLAYS),
                    pos(&SCORE_DISPLAYS, s.ddi_score),
                    m::ddi_score_description(),
                ),
                choice(
                    "ddi.words",
                    m::ddi_words(),
                    labels(&JUDGMENT_WORDS),
                    pos(&JUDGMENT_WORDS, s.judgment_words),
                    m::ddi_words_description(),
                ),
            ]);
        } else {
            rows.extend([
                choice(
                    "holds",
                    m::holds(),
                    labels(&HOLD_STYLES),
                    pos(&HOLD_STYLES, s.holds),
                    m::holds_description(),
                ),
                choice(
                    "fail",
                    m::fail(),
                    labels(&FAIL_TYPES),
                    pos(&FAIL_TYPES, s.stepmania.fail),
                    m::fail_description(),
                ),
                row(
                    "life",
                    m::life_difficulty(),
                    OptionKind::Stepper(levels(LIFE_SCALES.len())),
                    level_index(s.stepmania.life),
                    m::life_difficulty_description(),
                ),
                row(
                    "judge",
                    m::judge_difficulty(),
                    // StepMania's levels, the last its Justice.
                    OptionKind::Stepper(
                        (1..=JUDGE_SCALES.len() as u8)
                            .map(|l| UiText::data(StepManiaOptions::judge_name(l)).into())
                            .collect(),
                    ),
                    level_index(s.stepmania.judge),
                    m::judge_difficulty_description(),
                ),
                choice(
                    "life_type",
                    m::life_type(),
                    labels(&LIFE_TYPES),
                    pos(&LIFE_TYPES, s.stepmania.life_type),
                    m::life_type_description(),
                ),
                row(
                    "lives",
                    m::lives(),
                    OptionKind::Stepper(levels(usize::from(StepManiaOptions::MAX_LIVES))),
                    level_index(s.stepmania.lives),
                    m::lives_description(),
                ),
                choice(
                    "ddr_gauge",
                    m::ddr_gauge(),
                    DdrGauge::ALL.iter().map(|g| match g {
                        DdrGauge::Normal => m::ddr_gauge_normal(),
                        g => UiText::data(g.name()),
                    }),
                    DdrGauge::ALL
                        .iter()
                        .position(|g| *g == s.stepmania.gauge.sanitized())
                        .unwrap_or(0),
                    m::ddr_gauge_description(),
                ),
            ]);
        }
        // The kept plays judged again under the rules chosen now.
        let plays = self.world.plays;
        rows.push(actions(
            REJUDGE,
            m::rejudge(),
            if plays == 0 {
                vec![]
            } else {
                vec![m::rejudge_button()]
            },
            if plays == 0 {
                m::history_none()
            } else {
                m::history_plays(plays)
            },
            m::rejudge_description(),
        ));
        rows.extend([
            gauge,
            choice(
                "danoni.range",
                m::danoni_range(),
                JudgeRange::ALL.map(|r| UiText::data(r.name())),
                JudgeRange::ALL
                    .iter()
                    .position(|r| *r == s.danoni.range)
                    .unwrap_or(0),
                m::danoni_range_description(),
            ),
            choice(
                "danoni.excessive",
                m::danoni_excessive(),
                [m::danoni_excessive_chart(), common::off(), common::on()],
                match s.danoni.excessive {
                    None => 0,
                    Some(false) => 1,
                    Some(true) => 2,
                },
                m::danoni_excessive_description(),
            ),
        ]);
        section("Rules", m::section_rules(), rows)
    }

    fn timing(&self) -> OptionSection {
        let s = &self.settings;
        let mut rows = vec![
            actions(
                CALIBRATE,
                m::calibration(),
                [common::calibrate()],
                "",
                m::calibration_description(),
            ),
            offset(
                "audio_offset",
                m::audio_offset(),
                s.audio_offset,
                "",
                m::audio_offset_description(),
            ),
            offset(
                "visual_offset",
                m::visual_offset(),
                s.visual_offset,
                "",
                m::visual_offset_description(),
            ),
        ];
        let mut first = true;
        for (kind, list) in [
            ("audio", &s.audio_profiles),
            ("display", &s.display_profiles),
        ] {
            for (i, p) in list.iter().enumerate() {
                let mut r = offset(
                    &format!("profile.{kind}.{i}"),
                    if kind == "audio" {
                        m::profile_audio(device_name(p))
                    } else {
                        m::profile_display(device_name(p))
                    },
                    p.offset,
                    if p.calibrated {
                        m::profile_calibrated()
                    } else {
                        m::profile_default()
                    },
                    m::profile_description(),
                );
                if first {
                    r.heading = Some(m::heading_known_devices().into());
                    first = false;
                }
                rows.push(r);
                rows.push(actions(
                    &format!("forget.{kind}.{i}"),
                    "",
                    [m::profile_forget()],
                    "",
                    m::profile_forget_description(),
                ));
            }
        }
        section("Timing", m::section_timing(), rows)
    }

    fn sound(&self) -> OptionSection {
        let s = &self.settings;
        let percent = (volume_to_slider(s.volume) * 100.0).round() as usize;
        let mut volume = row(
            "volume",
            m::volume(),
            OptionKind::Slider(percents()),
            percent.min(100),
            m::volume_description(),
        );
        volume.note = decibels(s.volume);
        let mut v = vec![volume];
        let label = match self.world.sample {
            Sample::None => None,
            Sample::Ready => Some(m::sample_play()),
            Sample::Playing => Some(m::sample_stop()),
        };
        if let Some(l) = label {
            v.push(actions(
                "sample",
                m::sample(),
                [l],
                "",
                m::sample_description(),
            ));
        }
        v.push(choice(
            "assist_clap",
            m::assist_clap(),
            on_off(),
            s.assist_clap as usize,
            m::assist_clap_description(),
        ));
        v.push(choice(
            "mine_sound",
            m::mine_sound(),
            on_off(),
            s.mine_sound as usize,
            m::mine_sound_description(),
        ));
        v.extend(self.announcer_rows());
        section("Sound", m::section_sound(), v)
    }

    /// The announcer, its volume, and removing it (the web's, imported).
    fn announcer_rows(&self) -> Vec<OptionDef> {
        let s = &self.settings;
        let desktop = self.world.platform == Platform::Desktop;
        let mut names: Vec<String> = vec![common::off().into()];
        names.extend(
            self.world
                .announcers
                .iter()
                .map(|(_, n)| UiText::data(n).into()),
        );
        let listed = |k: &str| self.world.announcers.iter().position(|(key, _)| key == k);
        let at = match s.announcer.as_deref() {
            None => 0,
            Some(k) => listed(k).map_or_else(
                // Not listed yet (desktop's folders are walked after the
                // screen opens): shown by its key meanwhile.
                || {
                    names.push(UiText::data(k).into());
                    names.len() - 1
                },
                |i| i + 1,
            ),
        };
        let studio = Some(actions(
            "announcer.studio",
            m::announcer_studio(),
            [m::announcer_studio_button()],
            "",
            m::announcer_studio_description(),
        ));
        let mut rows = vec![row(
            "announcer",
            m::announcer(),
            OptionKind::Choices(names),
            at,
            m::announcer_description(self.world.platform.l10n()),
        )];
        let Some(key) = s.announcer.as_deref() else {
            rows.extend(studio);
            return rows;
        };
        let percent = (volume_to_slider(s.announcer_volume) * 100.0).round() as usize;
        let mut volume = row(
            "announcer_volume",
            m::announcer_volume(),
            OptionKind::Slider(percents()),
            percent.min(100),
            m::announcer_volume_description(),
        );
        volume.note = decibels(s.announcer_volume);
        rows.push(volume);
        // Ours come with the game; on desktop, what is in a song folder
        // stays there, and only those made in the studio can go.
        let removable = if desktop {
            self.world.made_announcers.iter().any(|k| k == key)
        } else {
            listed(key).is_some() && !ddi_library::announcer::is_bundled(key)
        };
        if removable {
            rows.push(actions(
                "announcer.remove",
                m::announcer_remove(),
                [m::announcer_remove_button()],
                "",
                m::announcer_remove_description(self.world.platform.l10n()),
            ));
        }
        rows.extend(studio);
        rows
    }

    fn display(&self) -> OptionSection {
        let s = &self.settings;
        let bg = (s.bg_brightness * 20.0).round() as usize;
        let filter = FIELD_FILTERS
            .iter()
            .position(|f| (f - s.field_filter).abs() < 0.01)
            .unwrap_or(0);
        let mut demo = actions(
            "demo",
            m::demo(),
            [m::demo_play(), m::demo_webgl2()],
            "",
            m::demo_description(),
        );
        demo.heading = Some(m::heading_diagnostics().into());
        let mut brightness = row(
            "bg_brightness",
            m::bg_brightness(),
            OptionKind::Stepper(
                (0..=20)
                    .map(|v| {
                        if v == 0 {
                            common::off().into()
                        } else {
                            m::percent(v * 5).into()
                        }
                    })
                    .collect(),
            ),
            bg.min(20),
            m::bg_brightness_description(),
        );
        brightness.heading = Some(m::heading_backgrounds().into());
        let mut menus = self.language_row();
        menus.heading = Some(m::heading_menus().into());
        let motion = choice(
            "reduced_motion",
            m::reduced_motion(),
            labels(&REDUCED_MOTION),
            pos(&REDUCED_MOTION, s.reduced_motion),
            m::reduced_motion_description(),
        );
        let mut rows = vec![
                choice(
                    "field_place",
                    m::field_place(),
                    labels(&FIELD_PLACES),
                    pos(&FIELD_PLACES, s.field_place),
                    m::field_place_description(),
                ),
                choice(
                    "show_deltas",
                    m::show_deltas(),
                    on_off(),
                    s.show_deltas as usize,
                    m::show_deltas_description(),
                ),
                choice(
                    "opponent_fields",
                    m::opponent_fields(),
                    labels(&OPPONENT_FIELDS),
                    pos(&OPPONENT_FIELDS, s.opponent_fields),
                    m::opponent_fields_description(),
                ),
                choice(
                    "dancing_hand",
                    m::dancing_hand(),
                    on_off(),
                    s.dancing_hand as usize,
                    m::dancing_hand_description(),
                ),
                choice(
                    "hand_layout",
                    m::hand_layout(),
                    labels(&HAND_LAYOUTS),
                    pos(&HAND_LAYOUTS, s.hand_layout),
                    m::hand_layout_description(),
                ),
                choice(
                    "hand_frame_width",
                    m::hand_frame_width(),
                    (3..=10).map(|t| m::percent(t * 10)),
                    usize::from(s.hand_frame_width.clamp(30, 100) - 30) / 10,
                    m::hand_frame_width_description(),
                ),
                choice(
                    "hand_frame_height",
                    m::hand_frame_height(),
                    (5..=10).map(|t| m::percent(t * 10)),
                    usize::from(s.hand_frame_height.clamp(50, 100) - 50) / 10,
                    m::hand_frame_height_description(),
                ),
                choice(
                    "ghost",
                    m::ghost(),
                    on_off(),
                    s.ghost as usize,
                    m::ghost_description(),
                ),
                choice(
                    "receptor_snap",
                    m::receptor_snap(),
                    on_off(),
                    s.receptor_snap as usize,
                    m::receptor_snap_description(),
                ),
                choice(
                    "lyrics",
                    m::lyrics(),
                    on_off(),
                    s.lyrics as usize,
                    m::lyrics_description(),
                ),
                brightness,
                choice(
                    "video",
                    m::video(),
                    labels(&VIDEOS),
                    pos(&VIDEOS, s.video),
                    m::video_description(),
                ),
                choice(
                    "random_movies",
                    m::random_movies(),
                    on_off(),
                    s.random_movies as usize,
                    m::random_movies_description(),
                ),
                choice(
                    "generated_background",
                    m::generated_background(),
                    labels(&GENERATED),
                    pos(&GENERATED, s.generated_background),
                    m::generated_background_description(),
                ),
                choice(
                    "generated_over_pictures",
                    m::generated_over_pictures(),
                    on_off(),
                    s.generated_over_pictures as usize,
                    m::generated_over_pictures_description(),
                ),
                choice(
                    "generated_palette",
                    m::generated_palette(),
                    labels(&PALETTES),
                    pos(&PALETTES, s.generated_palette),
                    m::generated_palette_description(),
                ),
                choice(
                    "background_frame",
                    m::background_frame(),
                    labels(&FRAMES),
                    pos(&FRAMES, s.background_frame),
                    m::background_frame_description(),
                ),
                choice(
                    "field_filter",
                    m::field_filter(),
                    std::iter::once(common::off()).chain(
                        FIELD_FILTERS[1..]
                            .iter()
                            .map(|f| m::percent((f * 100.0).round())),
                    ),
                    filter,
                    m::field_filter_description(),
                ),
                menus,
                motion,
                choice(
                    "cursor_style",
                    m::cursor(),
                    labels(&CURSORS),
                    pos(&CURSORS, s.cursor_style),
                    m::cursor_description(),
                ),
                row(
                    "cursor_size",
                    m::cursor_size(),
                    OptionKind::Slider((16..=64).map(|v| m::px(v).into()).collect()),
                    (s.cursor_size.clamp(16.0, 64.0).round() as usize).saturating_sub(16),
                    m::cursor_size_description(),
                ),
                choice(
                    "start_countdown",
                    m::start_countdown(),
                    START_COUNTDOWNS.iter().map(|&c| {
                        if c == 0.0 {
                            common::off()
                        } else {
                            m::seconds(c)
                        }
                    }),
                    START_COUNTDOWNS
                        .iter()
                        .position(|c| (c - s.start_countdown).abs() < 0.01)
                        .unwrap_or(0),
                    m::start_countdown_description(),
                ),
                choice(
                    "graphics",
                    m::graphics(),
                    labels(&GRAPHICS),
                    pos(&GRAPHICS, s.graphics),
                    m::graphics_description(),
                ),
                choice(
                    "debug",
                    m::debug(),
                    on_off(),
                    s.debug as usize,
                    m::debug_description(),
                ),
                demo,
        ];
        // The hand in a multiplayer stage, under the hand's own row while
        // the hand is on at all.
        if s.dancing_hand {
            let at = rows
                .iter()
                .position(|o| o.id == "dancing_hand")
                .map_or(rows.len(), |i| i + 1);
            rows.insert(
                at,
                choice(
                    "dancing_hand_multiplayer",
                    m::dancing_hand_multiplayer(),
                    on_off(),
                    s.dancing_hand_multiplayer as usize,
                    m::dancing_hand_multiplayer_description(),
                ),
            );
        }
        section("Display", m::section_display(), rows)
    }

    fn arrows(&self) -> OptionSection {
        let s = &self.settings;
        // Skins keep the names their folders give them.
        let infinito = self.skin_name(None);
        let mut names: Vec<String> = vec![if self.world.default_lua_off {
            m::noteskin_lua_off(&infinito).into()
        } else {
            UiText::data(&infinito).into()
        }];
        let dance = self.dance_skins();
        names.extend(dance.iter().map(|(_, n)| UiText::data(n).into()));
        let at = match s.noteskin.as_deref() {
            None => 0,
            Some(k) => dance
                .iter()
                .position(|(key, _)| key == k)
                .map_or(0, |i| i + 1),
        };
        let mut rows = vec![row(
            "noteskin",
            m::noteskin(),
            OptionKind::Choices(names),
            at,
            m::noteskin_description(self.world.platform.l10n()),
        )];
        // The chosen skin's parameters.
        let key = s.noteskin.as_deref().unwrap_or(INFINITO);
        rows.extend(self.param_rows(SKIN_ROW, key));
        let desktop = self.world.platform == Platform::Desktop;
        // Desktop reads skins in place (bundled ones, the song folders'):
        // none is stored to remove.
        if let Some(k) = s.noteskin.as_deref()
            && !k.starts_with(BUNDLED)
            && !desktop
        {
            rows.push(actions(
                "noteskin.remove",
                m::noteskin_remove(),
                [m::noteskin_remove_button()],
                "",
                m::noteskin_remove_description(),
            ));
        }
        // Pump skins (StepMania's `NoteSkins/pump/*`) draw the pump
        // layouts when one is chosen.
        let pump = self.pump_skins();
        if !pump.is_empty() {
            let mut names: Vec<String> = vec![m::pump_noteskin_same().into()];
            names.extend(pump.iter().map(|(_, n)| UiText::data(n).into()));
            let at = s
                .pump_noteskin
                .as_deref()
                .and_then(|k| pump.iter().position(|(key, _)| key == k))
                .map_or(0, |i| i + 1);
            rows.push(row(
                "pump_noteskin",
                m::pump_noteskin(),
                OptionKind::Choices(names),
                at,
                m::pump_noteskin_description(),
            ));
            if s.pump_noteskin
                .as_deref()
                .is_some_and(|k| !k.starts_with(BUNDLED))
                && !desktop
            {
                rows.push(actions(
                    "pump_noteskin.remove",
                    m::pump_noteskin_remove(),
                    [m::pump_noteskin_remove_button()],
                    "",
                    m::pump_noteskin_remove_description(),
                ));
            }
            if let Some(k) = s.pump_noteskin.as_deref() {
                rows.extend(self.param_rows(PUMP_SKIN_ROW, k));
            }
        }
        section("Arrows", m::section_arrows(), rows)
    }

    /// What skin `key` declares (Infinito's built in).
    fn declared(&self, key: &str) -> &[Parameter] {
        if key == INFINITO {
            &ddi_noteskin::bundled::infinito_parameters().parameters
        } else {
            self.world
                .skin_parameters
                .get(key)
                .map_or(&[], Vec::as_slice)
        }
    }

    /// Skin `key`'s parameters as rows, their ids `prefix` and the name:
    /// their labels, descriptions and choices as the skin's metrics write
    /// them (data).
    fn param_rows(&self, prefix: &str, key: &str) -> Vec<OptionDef> {
        let stored = self.settings.skin_params(key);
        self.declared(key)
            .iter()
            .map(|p| {
                let (labels, _, at) = param_values(p, stored.get(&p.name));
                row(
                    &format!("{prefix}{}", p.name),
                    UiText::data(&p.label),
                    match p.kind {
                        ParamKind::Number { .. } => OptionKind::Stepper(labels.into()),
                        _ => OptionKind::Choices(labels),
                    },
                    at,
                    UiText::data(&p.description),
                )
            })
            .collect()
    }

    /// Sets parameter `name` of skin `key` to its row's value `i`.
    fn set_param(&mut self, key: &str, name: &str, i: usize) {
        let Some(p) = self.declared(key).iter().find(|p| p.name == name).cloned() else {
            return;
        };
        let stored = self.settings.skin_params(key);
        let (_, values, _) = param_values(&p, stored.get(name));
        if let Some(v) = values.get(i) {
            self.settings
                .set_skin_param(key, name, Some(v.clone()), Some(&p.default));
        }
    }

    fn preview_layout_option(&self, layouts: &[Layout]) -> OptionDef {
        let shown = self
            .preview_layout
            .as_deref()
            .unwrap_or(&self.settings.style);
        row(
            "preview_layout",
            m::preview_on(),
            OptionKind::Choices(
                layouts
                    .iter()
                    .map(|l| style_name(&l.id, &l.name).into())
                    .collect(),
            ),
            layouts.iter().position(|l| l.id == shown).unwrap_or(0),
            m::preview_on_description(),
        )
    }

    fn controls(&self, layouts: &[Layout]) -> OptionSection {
        let s = &self.settings;
        let layout = &self.bind_layout;
        let at = layouts.iter().position(|l| l.id == layout.id).unwrap_or(0);
        let mut rows = vec![
            row(
                "bind_layout",
                m::bind_layout(),
                OptionKind::Choices(
                    layouts
                        .iter()
                        .map(|l| style_name(&l.id, &l.name).into())
                        .collect(),
                ),
                at,
                match layout.one_pad_layout() {
                    Some(one) => m::bind_layout_description_pads(style_name(&one.id, &one.name)),
                    None => m::bind_layout_description(),
                },
            ),
            actions(
                "bind.any",
                m::bind_any(),
                [m::bind_any_button()],
                "",
                m::bind_any_description(),
            ),
        ];
        let mut keyboard = actions(
            "keys",
            m::keyboard(),
            [m::keyboard_bind(), m::keyboard_reset()],
            if s.controls.keyboard_is_default(layout) {
                m::bindings_defaults()
            } else {
                m::bindings_saved()
            },
            m::keyboard_description(),
        );
        keyboard.heading = Some(m::heading_keyboard().into());
        rows.push(keyboard);
        let keys = s.controls.keyboard(layout);
        for lane in 0..layout.lanes.len() {
            let k = keys.get(lane).cloned().unwrap_or_default();
            rows.push(actions(
                &format!("keys.lane.{lane}"),
                lane_name(layout, lane),
                [m::key_add(), m::key_clear()],
                // The keys' codes, as the shells name them.
                UiText::data(if k.is_empty() {
                    "—".to_string()
                } else {
                    k.join(", ")
                }),
                m::key_description(),
            ));
        }
        let one_pad = self.layout_is_one_pad();
        for (j, (id, standard, count)) in self.pads().into_iter().enumerate() {
            rows.extend(self.pad_rows(j, &id, standard, count, one_pad));
        }
        let mut poll = choice(
            "fast_pad_poll",
            m::fast_pad_poll(),
            on_off(),
            s.fast_pad_poll as usize,
            m::fast_pad_poll_description(),
        );
        poll.heading = Some(m::heading_controllers().into());
        rows.push(poll);
        if !self.world.pads_supported {
            rows.push(actions(
                "pads.none",
                m::pads_none(),
                [],
                m::pads_none_note(),
                m::pads_none_description(self.world.platform.l10n()),
            ));
        }
        section("Controls", m::section_controls(), rows)
    }

    /// The controllers connected and, with `count` of them connected
    /// none, a placeholder for saved controller `id`.
    fn pads_with(&self, id: &str, standard: bool, count: usize) -> Vec<ConnectedPad> {
        let mut pads = self.world.connected.clone();
        if count == 0 {
            pads.push(ConnectedPad {
                vendor_product: None,
                id: id.to_string(),
                index: u32::MAX,
                standard,
            });
        }
        pads
    }

    /// What controller `id` plays in the layout shown, of `pads`.
    fn pad_use(&self, id: &str, pads: &[ConnectedPad]) -> PadUse {
        self.settings
            .controls
            .plan(&self.bind_layout, pads)
            .into_iter()
            .find(|(c, _)| c.id == id)
            .map_or(PadUse::Unused, |(_, u)| u)
    }

    /// Controller `id`'s saved entry, and whether it has a table for the
    /// layout shown.
    fn saved_pad(&self, id: &str) -> (Option<&PadBindings>, bool) {
        let saved = self.settings.controls.pads.iter().find(|p| p.id == id);
        let has_table = saved.is_some_and(|p| p.table(&self.bind_layout.id).is_some());
        (saved, has_table)
    }

    /// Whether the layout shown is the one-pad layout of a built-in one.
    fn layout_is_one_pad(&self) -> bool {
        Layout::builtins().iter().any(|l| {
            l.one_pad_layout()
                .is_some_and(|one| one.id == self.bind_layout.id)
        })
    }

    /// Controller `id`'s buttons, in their order.
    fn pad_buttons(
        &self,
        id: &str,
        standard: bool,
        pad_use: &PadUse,
        is_one_pad: bool,
    ) -> Vec<PadButton> {
        let (saved, has_table) = self.saved_pad(id);
        let mut buttons = vec![PadButton::Bind];
        if pad_use.plays() || has_table {
            buttons.push(PadButton::Clear);
        }
        if has_table && (!is_one_pad || standard) {
            buttons.push(PadButton::Defaults);
        }
        if saved.is_some() || self.settings.controller_styles.contains_key(id) {
            buttons.push(PadButton::Forget);
        }
        buttons
    }

    /// A controller's lane rows' buttons: one more control (while it is
    /// connected) and Clear, or none where it plays one pad of the
    /// layout with its bindings of the one-pad layout (changed there).
    fn pad_lane_buttons(pad_use: &PadUse, count: usize) -> &'static [LaneButton] {
        let own = !matches!(pad_use, PadUse::OnePad { .. });
        match (own, count > 0) {
            (false, _) => &[],
            (true, true) => &[LaneButton::Add, LaneButton::Clear],
            (true, false) => &[LaneButton::Clear],
        }
    }

    /// A controller: what it plays, its bindings, and what can be done
    /// with them.
    fn pad_rows(
        &self,
        j: usize,
        id: &str,
        standard: bool,
        count: usize,
        is_one_pad: bool,
    ) -> Vec<OptionDef> {
        let layout = &self.bind_layout;
        let (_, has_table) = self.saved_pad(id);
        let bindings = self.settings.pad_bindings(id, standard);
        let pads = self.pads_with(id, standard, count);
        let pad_use = self.pad_use(id, &pads);
        let one_name = layout
            .one_pad_layout()
            .map(|l| style_name(&l.id, &l.name))
            .unwrap_or_else(|| UiText::data(""));
        let status = match &pad_use {
            PadUse::OnePad { .. } if count > 1 => m::pad_one_pad_each(&one_name),
            PadUse::OnePad { .. } => m::pad_one_pad(&one_name),
            u if !u.plays() && has_table => m::pad_nothing_bound(),
            PadUse::Table { defaults: true, .. } if pad_use.plays() => m::bindings_defaults(),
            PadUse::Table { .. } if pad_use.plays() => m::bindings_saved(),
            _ => m::pad_not_bound(),
        };
        let presence = match count {
            0 => m::pad_not_connected(),
            n => m::pad_connected(n),
        };
        let mut head = actions(
            &format!("pad.{j}"),
            "",
            self.pad_buttons(id, standard, &pad_use, is_one_pad)
                .into_iter()
                .map(PadButton::label),
            // Two tags.
            format!("{status} · {presence}"),
            m::pad_description(),
        );
        // The controller's name as it reports it.
        head.heading = Some(UiText::data(id.to_uppercase()).into());
        let mut rows = vec![head];
        let style = self
            .settings
            .controller_styles
            .get(id)
            .copied()
            .unwrap_or_default();
        let detected = pads
            .iter()
            .find(|p| p.id == id)
            .map(crate::controller_hints::detect);
        let auto = detected.map_or_else(m::pad_style_auto, |(f, _)| {
            m::pad_style_auto_detected(family_name(f))
        });
        use crate::controller_hints::Style;
        rows.push(choice(
            &format!("pad_style.{j}"),
            m::pad_style(),
            Style::ALL.map(|s| match s {
                Style::Auto => auto.clone(),
                Style::Generic => m::pad_style_generic(),
                // Brands, by their names.
                s => UiText::data(s.label()),
            }),
            Style::ALL.iter().position(|s| *s == style).unwrap_or(0),
            m::pad_style_description(),
        ));
        let device = pads
            .iter()
            .find(|p| p.id == id)
            .expect("connected or saved placeholder");
        let family = crate::controller_hints::presentation(device, style);
        // Controls as their buttons are named: legends as printed (data),
        // else in words.
        let label = |c: &String| crate::controller_hints::Cap::control(device, family, c).name();
        let list = |controls: &[String]| {
            crate::controller_hints::control_list(controls.iter().map(label))
                .unwrap_or_else(|| UiText::data("—"))
        };
        let lanes: Vec<UiText> = match (&pad_use, layout.one_pad_layout()) {
            (PadUse::OnePad { pad, .. }, Some(one)) => layout
                .pad_lanes_in(*pad, &one)
                .into_iter()
                .zip(layout.lanes_of_pad(*pad))
                .map(|(l, own)| match l {
                    Some(l) => lane_name(&one, l),
                    None => lane_name(layout, own),
                })
                .collect(),
            _ => (0..layout.lanes.len())
                .map(|i| lane_name(layout, i))
                .collect(),
        };
        let own = !matches!(pad_use, PadUse::OnePad { .. });
        let lane_buttons = Self::pad_lane_buttons(&pad_use, count);
        let table = match pad_use {
            PadUse::Table { table, .. } | PadUse::OnePad { single: table, .. } => table,
            PadUse::Unused => Vec::new(),
        };
        let b = bindings.unwrap_or_default();
        for (lane, name) in lanes.into_iter().enumerate() {
            rows.push(actions(
                &format!("pad.{j}.lane.{lane}"),
                name,
                lane_buttons.iter().map(|b| b.label()),
                list(table.get(lane).map(Vec::as_slice).unwrap_or_default()),
                if own {
                    m::pad_lane_description()
                } else {
                    m::pad_lane_shared_description()
                },
            ));
        }
        for (what, name, c) in [
            ("start", m::pad_start(), &b.start),
            ("back", m::pad_back(), &b.back),
        ] {
            rows.push(actions(
                &format!("pad.{j}.{what}"),
                name,
                [],
                c.as_ref().map(label).unwrap_or_else(|| UiText::data("—")),
                m::pad_start_back_description(),
            ));
        }
        rows
    }

    /// The song folders (desktop), after the rest.
    fn folders(&self) -> Option<OptionSection> {
        let folders = self.world.folders.as_ref()?;
        let mut rows: Vec<OptionDef> = folders
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let name = f
                    .trim_end_matches(['/', '\\'])
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(f);
                actions(
                    &format!("folders.{i}"),
                    UiText::data(name),
                    [m::folder_remove()],
                    UiText::data(f),
                    m::folder_description(),
                )
            })
            .collect();
        rows.push(actions(
            "folders.add",
            m::folders(),
            [m::folders_add(), m::folders_rescan()],
            // The shell's line of tags, made of messages.
            self.world.library_note.as_str(),
            m::folders_description(),
        ));
        Some(section("Songs", m::section_songs(), rows))
    }

    /// Sharing songs by link (`docs/plans/shared-songs.md`), while the
    /// hidden switch is on: the token the uploads carry (never shown),
    /// the server they go to, and what sharing publishes.
    fn sharing(&self) -> Option<OptionSection> {
        if !self.world.sharing {
            return None;
        }
        let set = !self.settings.share_token.is_empty();
        let rows = vec![
            actions(
                "share.token",
                m::share_token(),
                if set {
                    vec![m::share_token_change(), m::share_token_clear()]
                } else {
                    vec![m::share_token_set()]
                },
                if set {
                    m::share_token_is_set()
                } else {
                    m::share_token_not_set()
                },
                m::share_token_description(),
            ),
            actions(
                "share.shares",
                m::share_shares(),
                if set { vec![m::share_list()] } else { vec![] },
                if set {
                    m::share_shares_note()
                } else {
                    m::share_shares_no_token()
                },
                m::share_shares_description(),
            ),
            actions(
                "share.server",
                m::share_server(),
                [],
                UiText::data(&self.world.share_url),
                m::share_server_description(),
            ),
            actions(
                "share.about",
                m::share_about(),
                [],
                m::share_about_note(),
                m::share_about_description(),
            ),
        ];
        Some(section("Sharing", m::section_sharing(), rows))
    }

    /// The play history and the resets ("Data").
    fn data_section(&self) -> OptionSection {
        let w = &self.world;
        let nothing = w.imported == 0 && w.shared == 0 && !self.has_imported_skins();
        let songs = match &w.folders {
            // Desktop: the song folders are forgotten, never deleted.
            Some(folders) => actions(
                "reset.songs",
                m::reset_folders(),
                if folders.is_empty() {
                    vec![]
                } else {
                    vec![m::reset_folders_button()]
                },
                if folders.is_empty() {
                    m::reset_folders_none()
                } else {
                    m::reset_folders_count(folders.len())
                },
                m::reset_folders_description(),
            ),
            None => actions(
                "reset.songs",
                m::reset_songs(),
                if nothing || w.importing {
                    vec![]
                } else {
                    vec![m::reset_songs_button()]
                },
                if w.importing {
                    m::reset_importing()
                } else if nothing {
                    m::reset_songs_none()
                } else {
                    m::reset_songs_count(w.imported)
                },
                m::reset_songs_description(),
            ),
        };
        let plays = self.world.plays;
        let wording = self.songs_wording();
        section(
            "Data",
            m::section_data(),
            vec![
                actions(
                    "history.export",
                    m::history(),
                    history_buttons(plays, w.importing_plays)
                        .iter()
                        .map(|b| b.label()),
                    if w.importing_plays {
                        m::history_importing()
                    } else if plays == 0 {
                        m::history_none()
                    } else {
                        m::history_plays(plays)
                    },
                    m::history_description(self.world.platform.l10n()),
                ),
                actions(
                    "reset.settings",
                    m::reset_settings(),
                    [m::reset_settings_button()],
                    "",
                    m::reset_settings_description(wording),
                ),
                songs,
                actions(
                    "reset.everything",
                    m::reset_everything(),
                    if w.importing {
                        vec![]
                    } else {
                        vec![m::reset_everything_button()]
                    },
                    if w.importing {
                        m::reset_importing().into()
                    } else {
                        String::new()
                    },
                    m::reset_everything_description(wording),
                ),
            ],
        )
    }

    /// Whose wording the songs' rows take (`$platform`): desktop's where
    /// song folders are read in place, the web's where songs are
    /// imported.
    fn songs_wording(&self) -> &'static str {
        if self.world.folders.is_some() {
            Platform::Desktop.l10n()
        } else {
            Platform::Web.l10n()
        }
    }

    fn has_imported_skins(&self) -> bool {
        self.world
            .noteskins
            .iter()
            .any(|(k, _)| !k.starts_with(BUNDLED))
    }

    /// Applies a row's new value and saves; what the shell does after,
    /// or `None` when nothing changed.
    pub fn change(&mut self, id: &str, i: usize) -> Option<Effect> {
        let before = self.settings.clone();
        let layout_before = self.bind_layout.id.clone();
        let style_pad = id
            .strip_prefix("pad_style.")
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| self.pads().into_iter().nth(n))
            .map(|p| p.0);
        let languages = self.languages();
        let s = &mut self.settings;
        let on = i == 1;
        match id {
            _ if style_pad.is_some() => {
                let id = style_pad.unwrap();
                let style = crate::controller_hints::Style::ALL
                    .get(i)
                    .copied()
                    .unwrap_or_default();
                if style == crate::controller_hints::Style::Auto {
                    s.controller_styles.remove(&id);
                } else {
                    s.controller_styles.insert(id, style);
                }
            }
            "speed" => {
                if let Some(v) = self.speeds().get(i) {
                    self.settings.speed = *v;
                }
            }
            "speed_source" => s.speed_source = at(&SPEED_SOURCES, i),
            "reverse" => s.reverse = on,
            "field_place" => {
                s.field_place = at(&FIELD_PLACES, i);
                s.hand_layout = HandLayout::FollowField;
            }
            "hand_layout" => {
                s.hand_layout = at(&HAND_LAYOUTS, i);
                if let Some(place) = s.hand_layout.field() {
                    s.field_place = place;
                }
            }
            "hand_frame_width" => s.hand_frame_width = 30 + i.min(7) as u8 * 10,
            "hand_frame_height" => s.hand_frame_height = 50 + i.min(5) as u8 * 10,
            "scroll_action" => s.scroll_action = at(&SCROLL_ACTIONS, i),
            "turn" => s.transform.turn = at(&TURNS, i),
            "appearance" => s.appearance = at(&APPEARANCES, i),
            "lane_cover" => s.lane_cover.mode = at(&COVER_MODES, i),
            "cover_sudden" => s.lane_cover.sudden = i.min(usize::from(LaneCover::FULL)) as u8,
            "cover_hidden" => s.lane_cover.hidden = i.min(usize::from(LaneCover::FULL)) as u8,
            "cut" => s.transform.cut = at(&CUTS, i),
            "at_once" => s.transform.at_once = at(&AT_ONCE, i),
            "simplify_onigiri" => s.simplify_onigiri = on,
            "simplify_center" => s.simplify_center = at(&CENTER_VERSIONS, i),
            "convert_pump_single" => s.convert_pump_single = on,
            "convert_five_keys" => s.convert_five_keys = on,
            "no_holds" => s.transform.no_holds = on,
            "no_stretch" => s.transform.no_stretch = on,
            "ruleset_mode" => s.ruleset_mode = at(&RULESET_MODES, i),
            "ruleset" => {
                if let Some(r) = ddi_engine::rules::presets::all().get(i) {
                    s.ruleset = r.id.clone();
                }
            }
            "holds" => s.holds = at(&HOLD_STYLES, i),
            "fail" => s.stepmania.fail = at(&FAIL_TYPES, i),
            "life" => s.stepmania.life = level_at(i, LIFE_SCALES.len()),
            "judge" => s.stepmania.judge = level_at(i, JUDGE_SCALES.len()),
            "life_type" => s.stepmania.life_type = at(&LIFE_TYPES, i),
            "lives" => s.stepmania.lives = level_at(i, usize::from(StepManiaOptions::MAX_LIVES)),
            "ddr_gauge" => s.stepmania.gauge = DdrGauge::ALL.get(i).copied().unwrap_or_default(),
            "ddi.difficulty" => s.ddi.difficulty = at(&DDI_DIFFICULTIES, i),
            "ddi.boos" => s.ddi.boos = i == 1,
            "ddi.practice" => s.ddi.practice = i == 1,
            "ddi.score" => s.ddi_score = at(&SCORE_DISPLAYS, i),
            "ddi.words" => {
                s.judgment_words = at(&JUDGMENT_WORDS, i);
                // Shown so from the next frame (the preview's, a play's).
                ddi_l10n::judgments::set_localized(s.judgment_words == JudgmentWords::Localized);
            }
            "danoni.gauge" => {
                s.danoni.gauge = DANONI_GAUGES
                    .get(i)
                    .and_then(|(g, _)| *g)
                    .map(str::to_string)
            }
            "danoni.range" => s.danoni.range = JudgeRange::ALL.get(i).copied().unwrap_or_default(),
            "danoni.excessive" => {
                s.danoni.excessive = match i {
                    1 => Some(false),
                    2 => Some(true),
                    _ => None,
                }
            }
            "audio_offset" => s.audio_offset = offset_at(i),
            "visual_offset" => s.visual_offset = offset_at(i),
            "volume" => s.volume = slider_to_volume(i as f32 / 100.0),
            "assist_clap" => s.assist_clap = on,
            "mine_sound" => s.mine_sound = on,
            "announcer" => {
                if i == 0 {
                    s.announcer = None;
                } else if let Some((k, _)) = self.world.announcers.get(i - 1) {
                    s.announcer = Some(k.clone());
                }
            }
            "announcer_volume" => s.announcer_volume = slider_to_volume(i as f32 / 100.0),
            "show_deltas" => s.show_deltas = on,
            "ghost" => s.ghost = on,
            "dancing_hand" => s.dancing_hand = on,
            "dancing_hand_multiplayer" => s.dancing_hand_multiplayer = on,
            "opponent_fields" => s.opponent_fields = at(&OPPONENT_FIELDS, i),
            "receptor_snap" => s.receptor_snap = on,
            "lyrics" => s.lyrics = on,
            "random_movies" => s.random_movies = on,
            "generated_background" => s.generated_background = at(&GENERATED, i),
            "generated_over_pictures" => s.generated_over_pictures = on,
            "generated_palette" => s.generated_palette = at(&PALETTES, i),
            "bg_brightness" => s.bg_brightness = i as f32 / 20.0,
            "video" => s.video = at(&VIDEOS, i),
            "background_frame" => s.background_frame = at(&FRAMES, i),
            "field_filter" => s.field_filter = FIELD_FILTERS.get(i).copied().unwrap_or(0.0),
            "cursor_size" => s.cursor_size = (i.min(48) + 16) as f32,
            "cursor_style" => s.cursor_style = at(&CURSORS, i),
            "reduced_motion" => s.reduced_motion = at(&REDUCED_MOTION, i),
            "language" => {
                s.language = i
                    .checked_sub(1)
                    .and_then(|i| languages.get(i))
                    .map(|l| l.tag.to_string())
            }
            "start_countdown" => {
                s.start_countdown = START_COUNTDOWNS.get(i).copied().unwrap_or(0.0)
            }
            "graphics" => s.graphics = at(&GRAPHICS, i),
            "debug" => s.debug = on,
            "fast_pad_poll" => s.fast_pad_poll = on,
            "noteskin" => {
                s.noteskin = i.checked_sub(1).and_then(|n| {
                    skins_of(&self.world.noteskins, false)
                        .get(n)
                        .map(|(k, _)| k.clone())
                })
            }
            id if id.starts_with(SKIN_ROW) => {
                let key = s.noteskin.clone().unwrap_or_else(|| INFINITO.to_string());
                self.set_param(&key, &id[SKIN_ROW.len()..], i);
            }
            id if id.starts_with(PUMP_SKIN_ROW) => {
                if let Some(key) = s.pump_noteskin.clone() {
                    self.set_param(&key, &id[PUMP_SKIN_ROW.len()..], i);
                }
            }
            "pump_noteskin" => {
                s.pump_noteskin = i.checked_sub(1).and_then(|n| {
                    skins_of(&self.world.noteskins, true)
                        .get(n)
                        .map(|(k, _)| k.clone())
                });
            }
            "preview_layout" => {
                if let Some(l) = self.bind_layouts().into_iter().nth(i) {
                    self.preview_layout = Some(l.id);
                }
            }
            "bind_layout" => {
                if let Some(l) = self.bind_layouts().into_iter().nth(i) {
                    self.bind_layout = l;
                }
            }
            other => {
                // A device's offset.
                let mut parts = other.split('.');
                if let (Some("profile"), Some(kind), Some(n)) =
                    (parts.next(), parts.next(), parts.next())
                    && let Ok(n) = n.parse::<usize>()
                {
                    let list = if kind == "audio" {
                        &mut s.audio_profiles
                    } else {
                        &mut s.display_profiles
                    };
                    if let Some(p) = list.get_mut(n) {
                        p.offset = offset_at(i);
                        p.calibrated = true;
                    }
                }
            }
        }
        let changed = self.settings != before
            || self.bind_layout.id != layout_before
            || id == "preview_layout";
        if self.settings != before && !self.transient {
            self.settings.save();
        }
        let s = &self.settings;
        changed.then(|| {
            if s.volume != before.volume {
                Effect::Volume
            } else if s.announcer != before.announcer
                || s.announcer_volume != before.announcer_volume
            {
                Effect::Announce
            } else if s.language != before.language {
                Effect::Language
            } else if s.reduced_motion != before.reduced_motion {
                Effect::Motion
            } else {
                Effect::Plain
            }
        })
    }

    /// What pressing button `button` of row `id` does: done here, or
    /// what the shell must do.
    pub fn action(&mut self, id: &str, button: usize) -> Act {
        let before = self.settings.clone();
        let layout = self.bind_layout.clone();
        let act = match (id, button) {
            (CALIBRATE, _) => Act::Calibrate,
            ("sample", _) => Act::Sample,
            ("demo", b) => Act::Demo { gl: b == 1 },
            ("noteskin.remove", _) => match self.settings.noteskin.clone() {
                Some(k) => Act::Confirm(Reset::Noteskin(k)),
                None => Act::Nothing,
            },
            ("pump_noteskin.remove", _) => match self.settings.pump_noteskin.clone() {
                Some(k) => Act::Confirm(Reset::Noteskin(k)),
                None => Act::Nothing,
            },
            ("announcer.studio", _) => Act::Studio,
            ("announcer.remove", _) => match self.settings.announcer.clone() {
                Some(k) => Act::Confirm(Reset::Announcer(k)),
                None => Act::Nothing,
            },
            ("bind.any", _) => Act::Bind(None),
            ("keys", 0) => Act::Bind(Some(DeviceId::Keyboard)),
            ("keys", _) => {
                self.settings.controls.reset_keyboard(&layout);
                self.notice = m::notice_keys_default(style_name(&layout.id, &layout.name)).into();
                Act::Done
            }
            ("history.export", b) => {
                let buttons = history_buttons(self.world.plays, self.world.importing_plays);
                match buttons.get(b) {
                    Some(HistoryButton::Export) => Act::Export,
                    Some(HistoryButton::Import) => Act::ImportPlays,
                    Some(HistoryButton::Reset) => Act::Confirm(Reset::History),
                    None => Act::Nothing,
                }
            }
            (REJUDGE, _) if self.world.plays > 0 => {
                self.notice = m::notice_rejudge(self.world.plays).into();
                Act::Rejudge
            }
            ("share.token", 1) => {
                self.settings.share_token.clear();
                self.notice = m::notice_token_cleared().into();
                Act::Done
            }
            ("share.token", _) => Act::ShareToken,
            ("share.shares", _) if !self.settings.share_token.is_empty() => Act::ListShares,
            ("folders.add", 0) => Act::AddFolder,
            ("folders.add", _) => Act::Rescan,
            ("reset.settings", _) => Act::Confirm(Reset::Settings),
            ("reset.songs", _) => Act::Confirm(Reset::Songs),
            ("reset.everything", _) => Act::Confirm(Reset::Everything),
            (other, b) => {
                let parts: Vec<&str> = other.split('.').collect();
                match parts.as_slice() {
                    ["folders", n] => n.parse().map_or(Act::Nothing, Act::RemoveFolder),
                    ["keys", "lane", n] => match (n.parse::<usize>(), b) {
                        (Ok(lane), 0) => Act::AddKey(lane),
                        (Ok(lane), _) => {
                            let mut table = self.settings.controls.keyboard(&layout);
                            if let Some(k) = table.get_mut(lane) {
                                k.clear();
                            }
                            self.settings.controls.set_keyboard(&layout, table);
                            Act::Done
                        }
                        _ => Act::Nothing,
                    },
                    ["forget", kind, n] => {
                        let list = if *kind == "audio" {
                            &mut self.settings.audio_profiles
                        } else {
                            &mut self.settings.display_profiles
                        };
                        if let Ok(n) = n.parse::<usize>()
                            && n < list.len()
                        {
                            list.remove(n);
                        }
                        Act::Done
                    }
                    ["pad", n] => self.pad_action(n.parse().unwrap_or(usize::MAX), b),
                    ["pad", n, "lane", l] => match (n.parse::<usize>(), l.parse::<usize>()) {
                        (Ok(j), Ok(lane)) => self.pad_lane_action(j, lane, b),
                        _ => Act::Nothing,
                    },
                    _ => Act::Nothing,
                }
            }
        };
        if self.settings != before {
            self.settings.save();
        }
        act
    }

    fn pad_action(&mut self, j: usize, button: usize) -> Act {
        let Some((id, standard, count)) = self.pads().into_iter().nth(j) else {
            return Act::Nothing;
        };
        let layout = self.bind_layout.clone();
        let pad_use = self.pad_use(&id, &self.pads_with(&id, standard, count));
        let is_one_pad = self.layout_is_one_pad();
        match self
            .pad_buttons(&id, standard, &pad_use, is_one_pad)
            .get(button)
        {
            Some(PadButton::Bind) => Act::Bind(Some(DeviceId::Gamepad(id))),
            Some(PadButton::Clear) => {
                let mut pad = self.pad_entry(&id, standard);
                pad.set_table(&layout.id, vec![Vec::new(); layout.lane_count()]);
                self.settings.set_pad_bindings(pad);
                Act::Done
            }
            Some(PadButton::Defaults) => {
                self.settings
                    .controls
                    .reset_pad_table(&id, &layout.id, standard);
                Act::Done
            }
            Some(PadButton::Forget) => {
                self.settings.controls.forget_pad(&id);
                self.settings.controller_styles.remove(&id);
                self.notice = m::notice_pad_forgotten(&id).into();
                Act::Done
            }
            None => Act::Nothing,
        }
    }

    /// A lane row's button of the `j`th controller listed: Add button or
    /// Clear.
    fn pad_lane_action(&mut self, j: usize, lane: usize, button: usize) -> Act {
        let Some((id, standard, count)) = self.pads().into_iter().nth(j) else {
            return Act::Nothing;
        };
        let pad_use = self.pad_use(&id, &self.pads_with(&id, standard, count));
        match Self::pad_lane_buttons(&pad_use, count).get(button) {
            Some(LaneButton::Add) => Act::AddButton { id, lane },
            Some(LaneButton::Clear) => {
                let layout = self.bind_layout.clone();
                let mut table = self.pad_lanes(&id, standard);
                if let Some(c) = table.get_mut(lane) {
                    c.clear();
                }
                let mut pad = self.pad_entry(&id, standard);
                pad.set_table(&layout.id, table);
                self.settings.set_pad_bindings(pad);
                Act::Done
            }
            None => Act::Nothing,
        }
    }

    /// Controller `id`'s table for the layout shown: saved, else the
    /// standard defaults, else nothing on every lane.
    fn pad_lanes(&self, id: &str, standard: bool) -> Vec<Vec<String>> {
        let layout = &self.bind_layout;
        let mut table = self
            .settings
            .controls
            .pad_table(id, standard, layout)
            .unwrap_or_default();
        table.resize_with(layout.lane_count(), Vec::new);
        table
    }

    /// A control added to a lane of controller `id` for the layout shown
    /// (taken from any other lane: a control plays one lane only), as the
    /// keyboard's [`Self::add_key`]. The dialog never takes its Start or
    /// Back (they act before any lane).
    pub fn add_pad_control(&mut self, id: &str, lane: usize, control: &str) {
        let layout = self.bind_layout.clone();
        let standard = self.standard(id);
        let mut table = self.pad_lanes(id, standard);
        let mut from = None;
        for (i, controls) in table.iter_mut().enumerate() {
            if i != lane && controls.iter().any(|c| c == control) {
                from = Some(i);
            }
            controls.retain(|c| c != control);
        }
        if let Some(controls) = table.get_mut(lane) {
            controls.push(control.to_string());
        }
        let mut pad = self.pad_entry(id, standard);
        pad.set_table(&layout.id, table);
        self.settings.set_pad_bindings(pad);
        let shown = crate::controller_hints::binding_name(control);
        self.notice = moved_notice(&layout, &shown, from, lane);
        self.settings.save();
    }

    /// A controller's saved entry, else its standard defaults (so a table
    /// saved for one layout keeps the others' defaults), else a new one.
    fn pad_entry(&self, id: &str, standard: bool) -> PadBindings {
        self.settings
            .pad_bindings(id, standard)
            .unwrap_or_else(|| PadBindings {
                id: id.to_string(),
                ..PadBindings::default()
            })
    }

    /// Whether controller `id` has the standard mapping (as connected).
    pub fn standard(&self, id: &str) -> bool {
        self.world
            .connected
            .iter()
            .any(|p| p.id == id && p.standard)
    }

    /// A finished binding: the device's table for the layout shown (and a
    /// controller's Start and Back, kept when skipped).
    pub fn bound(&mut self, bound: Bound) {
        let layout = self.bind_layout.clone();
        let style = style_name(&layout.id, &layout.name);
        match bound {
            Bound::Keyboard(keys) => {
                self.settings.controls.set_keyboard(&layout, keys);
                self.notice = m::notice_keyboard_bound(&style).into();
            }
            Bound::Pad {
                id,
                table,
                start,
                back,
            } => {
                let mut pad = self.pad_entry(&id, self.standard(&id));
                pad.set_table(&layout.id, table);
                pad.start = start.or(pad.start);
                pad.back = back.or(pad.back);
                self.settings.set_pad_bindings(pad);
                self.notice = m::notice_pad_bound(&id, &style).into();
            }
        }
        self.settings.save();
    }

    /// The upload token typed into its dialog, kept.
    pub fn set_share_token(&mut self, token: &str) {
        self.settings.share_token = token.trim().to_string();
        self.notice = if self.settings.share_token.is_empty() {
            m::notice_token_cleared()
        } else {
            m::notice_token_set()
        }
        .into();
        self.settings.save();
    }

    /// A key added to a lane of the keyboard (taken from any other lane:
    /// a key plays one lane only).
    pub fn add_key(&mut self, lane: usize, code: &str) {
        let layout = self.bind_layout.clone();
        let mut table = self.settings.controls.keyboard(&layout);
        let mut from = None;
        for (i, keys) in table.iter_mut().enumerate() {
            if i != lane && keys.iter().any(|k| k == code) {
                from = Some(i);
            }
            keys.retain(|k| k != code);
        }
        if let Some(keys) = table.get_mut(lane) {
            keys.push(code.to_string());
        }
        self.settings.controls.set_keyboard(&layout, table);
        self.notice = moved_notice(&layout, code, from, lane);
        self.settings.save();
    }

    /// The settings' part of a reset (the shell removes the songs).
    pub fn reset(&mut self, r: &Reset) {
        match r {
            Reset::Settings | Reset::Everything => {
                let offsets = std::mem::take(&mut self.settings.song_offsets);
                self.settings = Settings::default();
                if *r == Reset::Settings {
                    self.settings.song_offsets = offsets;
                }
                self.bind_layout = Layout::dance_single();
            }
            Reset::Songs => {
                self.settings
                    .song_offsets
                    .retain(|id, _| !id.starts_with("u-"));
                if self
                    .settings
                    .noteskin
                    .as_deref()
                    .is_some_and(|k| !k.starts_with(BUNDLED))
                {
                    self.settings.noteskin = None;
                }
                if self
                    .settings
                    .pump_noteskin
                    .as_deref()
                    .is_some_and(|k| !k.starts_with(BUNDLED))
                {
                    self.settings.pump_noteskin = None;
                }
                self.settings
                    .skin_params
                    .retain(|k, _| k.starts_with(BUNDLED));
                // The web's are removed; desktop's folders forgotten.
                // Ours stay.
                if self
                    .settings
                    .announcer
                    .as_deref()
                    .is_some_and(|k| !ddi_library::announcer::is_bundled(k))
                {
                    self.settings.announcer = None;
                }
            }
            Reset::History => {}
            Reset::Announcer(k) => {
                if self.settings.announcer.as_deref() == Some(k.as_str()) {
                    self.settings.announcer = None;
                }
            }
            Reset::Noteskin(k) => {
                if self.settings.noteskin.as_deref() == Some(k.as_str()) {
                    self.settings.noteskin = None;
                }
                if self.settings.pump_noteskin.as_deref() == Some(k.as_str()) {
                    self.settings.pump_noteskin = None;
                }
                self.settings.skin_params.remove(k);
            }
        }
        self.settings.save();
        if *r == Reset::Settings {
            self.notice = m::notice_settings_reset().into();
        }
    }

    /// The noteskins changed: a chosen one no longer there is let go
    /// (when every skin could be read: `complete`).
    pub fn set_noteskins(&mut self, skins: Vec<(String, String)>, lua_off: bool, complete: bool) {
        if complete
            && self
                .settings
                .let_go_missing_skins(|k| skins.iter().any(|(key, _)| key == k))
        {
            self.settings.save();
        }
        self.world.noteskins = skins;
        self.world.noteskins_listed = true;
        self.world.default_lua_off = lua_off;
    }

    /// The announcers one can choose `(key, name)`, listed ours first,
    /// then by name: a chosen one no longer there is let go (when every
    /// one could be read: `complete`).
    pub fn set_announcers(&mut self, mut announcers: Vec<(String, String)>, complete: bool) {
        announcers.sort_by_cached_key(|(key, name)| {
            (
                !ddi_library::announcer::is_bundled(key),
                name.to_lowercase(),
            )
        });
        if complete
            && self
                .settings
                .let_go_missing_announcer(|k| announcers.iter().any(|(key, _)| key == k))
        {
            self.settings.save();
        }
        self.world.announcers = announcers;
    }

    /// The parameters the skins declare, by key (the shell reads them
    /// with the skins, [`ddi_noteskin::params::Declared::of_skin`]).
    pub fn set_skin_parameters(
        &mut self,
        parameters: std::collections::BTreeMap<String, Vec<Parameter>>,
    ) {
        self.world.skin_parameters = parameters;
    }
}

/// The ids of the chosen skin's parameters' rows (then the name), and of
/// the chosen pump skin's.
const SKIN_ROW: &str = "skin.";
const PUMP_SKIN_ROW: &str = "pump_skin.";

/// A parameter's row: its values' labels, the values, and which is
/// `stored`'s (as its declaration takes it, else the default's). A color
/// off the palette is offered after it. The labels are the skin's own
/// (data), but for a switch's Off and On.
fn param_values(
    p: &Parameter,
    stored: Option<&ParamValue>,
) -> (Vec<String>, Vec<ParamValue>, usize) {
    let current = stored
        .and_then(|v| p.accept(v))
        .unwrap_or_else(|| p.default.clone());
    let data = |s: &str| -> String { UiText::data(s).into() };
    let (labels, values): (Vec<String>, Vec<ParamValue>) = match &p.kind {
        ParamKind::Choice(choices) => choices
            .iter()
            .map(|c| (data(&c.label), ParamValue::Text(c.id.clone())))
            .unzip(),
        ParamKind::Toggle => (
            on_off().map(String::from).to_vec(),
            vec![ParamValue::Bool(false), ParamValue::Bool(true)],
        ),
        ParamKind::Number { unit, .. } => p
            .numbers()
            .into_iter()
            .map(|n| {
                (
                    data(&format!("{}{unit}", trim_number(n, 2))),
                    ParamValue::Number(n),
                )
            })
            .unzip(),
        ParamKind::Color(palette) => {
            let mut pairs: Vec<(String, ParamValue)> = palette
                .iter()
                .map(|c| (data(&c.label), ParamValue::Text(c.id.clone())))
                .collect();
            if let ParamValue::Text(t) = &current
                && !pairs.iter().any(|(_, v)| v == &current)
            {
                pairs.push((data(t), current.clone()));
            }
            pairs.into_iter().unzip()
        }
    };
    let at = match &current {
        ParamValue::Number(n) => values
            .iter()
            .position(|v| matches!(v, ParamValue::Number(x) if (x - n).abs() < 1e-9))
            .unwrap_or(0),
        _ => values.iter().position(|v| *v == current).unwrap_or(0),
    };
    (labels, values, at)
}

impl OptionsModel {
    /// The skins of [`World::noteskins`] for every layout (dance's).
    fn dance_skins(&self) -> Vec<(String, String)> {
        skins_of(&self.world.noteskins, false)
    }

    /// The pump skins of [`World::noteskins`].
    fn pump_skins(&self) -> Vec<(String, String)> {
        skins_of(&self.world.noteskins, true)
    }
}

impl OptionsModel {
    /// The languages the Language row offers after the system's: the
    /// shipped ones and the previews, the pseudo-languages too with the
    /// debug overlay on, and the one chosen whatever it is.
    fn languages(&self) -> Vec<&'static ddi_l10n::Language> {
        let mut offered = ddi_l10n::offered(self.settings.debug);
        if let Some(chosen) = self.settings.language.as_deref().and_then(ddi_l10n::find)
            && !offered.contains(&chosen)
        {
            offered.push(chosen);
        }
        offered
    }

    /// The UI's language (`docs/plans/localization.md`): each language
    /// named in itself, the system's first.
    fn language_row(&self) -> OptionDef {
        let languages = self.languages();
        let mut names: Vec<String> =
            vec![m::language_system(ddi_l10n::system_language().name).into()];
        names.extend(languages.iter().map(|l| UiText::data(l.name).into()));
        let index = self
            .settings
            .language
            .as_deref()
            .and_then(|t| languages.iter().position(|l| l.tag.eq_ignore_ascii_case(t)))
            .map_or(0, |i| i + 1);
        row(
            "language",
            m::language(),
            OptionKind::Choices(names),
            index,
            m::language_description(),
        )
    }
}

/// The pump skins of `skins`, or the others.
fn skins_of(skins: &[(String, String)], pump: bool) -> Vec<(String, String)> {
    skins
        .iter()
        .filter(|(k, _)| crate::state::settings::is_pump_skin(k) == pump)
        .cloned()
        .collect()
}

/// A section: `id` what the shell and the tests know it by (its English
/// name), `name` as it shows.
fn section(id: &str, name: UiText, options: Vec<OptionDef>) -> OptionSection {
    OptionSection {
        id: id.into(),
        name: name.into(),
        options,
    }
}

fn row(
    id: &str,
    label: impl Into<String>,
    kind: OptionKind,
    index: usize,
    description: impl Into<String>,
) -> OptionDef {
    OptionDef {
        id: id.into(),
        label: label.into(),
        kind,
        index,
        note: String::new(),
        description: description.into(),
        heading: None,
    }
}

fn choice(
    id: &str,
    label: impl Into<String>,
    names: impl IntoIterator<Item = UiText>,
    index: usize,
    description: impl Into<String>,
) -> OptionDef {
    row(
        id,
        label,
        OptionKind::Choices(names.into_iter().map(String::from).collect()),
        index,
        description,
    )
}

/// A switch's values.
fn on_off() -> [UiText; 2] {
    [common::off(), common::on()]
}

fn actions(
    id: &str,
    label: impl Into<String>,
    buttons: impl IntoIterator<Item = UiText>,
    note: impl Into<String>,
    description: impl Into<String>,
) -> OptionDef {
    let mut r = row(
        id,
        label,
        OptionKind::Actions(buttons.into_iter().map(String::from).collect()),
        0,
        description,
    );
    r.note = note.into();
    r
}

/// An offset stepper, −500 to +500 ms by 1.
fn offset(
    id: &str,
    label: impl Into<String>,
    seconds: f64,
    note: impl Into<String>,
    description: impl Into<String>,
) -> OptionDef {
    let ms = (seconds * 1000.0).round().clamp(-500.0, 500.0) as i32;
    let mut r = row(
        id,
        label,
        OptionKind::Stepper(formatted("offsets", || {
            (-500..=500)
                .map(|v| m::ms(format!("{v:+}")).into())
                .collect()
        })),
        (ms + 500) as usize,
        description,
    );
    r.note = note.into();
    r
}

fn offset_at(i: usize) -> f64 {
    (i as f64 - 500.0).clamp(-500.0, 500.0) / 1000.0
}

/// A row's value and the words for it: one table, in the order shown.
type Labeled<T> = (T, fn() -> UiText);

/// The words for each of a table's values, in its order.
fn labels<T>(table: &[Labeled<T>]) -> impl Iterator<Item = UiText> + '_ {
    table.iter().map(|(_, label)| label())
}

fn pos<T: PartialEq>(table: &[Labeled<T>], v: T) -> usize {
    table.iter().position(|(x, _)| *x == v).unwrap_or(0)
}

fn at<T: Copy + Default>(table: &[Labeled<T>], i: usize) -> T {
    table.get(i).map_or_else(T::default, |(v, _)| *v)
}

/// Levels `1..=n` as a stepper shows them.
fn levels(n: usize) -> Rc<[String]> {
    (1..=n).map(|l| l.to_string()).collect()
}

/// A level `1..=n`'s place in its stepper.
fn level_index(level: u8) -> usize {
    usize::from(level.max(1) - 1)
}

/// The level `1..=n` at a stepper's place.
fn level_at(i: usize, n: usize) -> u8 {
    (i.min(n - 1) + 1) as u8
}

/// A scroll speed as the rows show it ("×2.5"), to two places.
fn speed_text(speed: f64) -> String {
    m::speed_value((speed * 100.0).round() / 100.0).into()
}

/// Percentages 0 to 100 as a slider's or a stepper's values.
fn percents() -> Rc<[String]> {
    formatted("percents", || {
        (0..=100).map(|p| m::percent(p).into()).collect()
    })
}

/// A long list of values (offsets, percentages) formatted once per
/// language: the rows are made again at every change, and a stepper or a
/// slider shows only the chosen one.
fn formatted(list: &'static str, make: impl FnOnce() -> Rc<[String]>) -> Rc<[String]> {
    /// By language and list.
    type Lists = HashMap<(&'static str, &'static str), Rc<[String]>>;
    thread_local! {
        static LISTS: RefCell<Lists> = RefCell::default();
    }
    let key = (ddi_l10n::current().tag, list);
    if let Some(values) = LISTS.with_borrow(|l| l.get(&key).cloned()) {
        return values;
    }
    let values = make();
    LISTS.with_borrow_mut(|l| l.insert(key, values.clone()));
    values
}

/// A volume's loudness, beside its slider ("-10 dB", "muted").
fn decibels(volume: f32) -> String {
    if volume > 0.0 {
        m::db((20.0 * volume.log10()).round()).into()
    } else {
        m::muted().into()
    }
}

/// What a key or control added to lane `to` of `layout` did: moved from
/// lane `from`, or added.
fn moved_notice(layout: &Layout, control: &str, from: Option<usize>, to: usize) -> String {
    match from {
        Some(f) => m::notice_moved(control, lane_name(layout, f), lane_name(layout, to)),
        None => m::notice_added(control, lane_name(layout, to)),
    }
    .into()
}

/// A controller's kind, as the Button hints row names it: by its brand,
/// or the word for one of none.
fn family_name(f: crate::controller_hints::Family) -> UiText {
    match f {
        crate::controller_hints::Family::Generic => m::pad_style_generic(),
        f => UiText::data(f.label()),
    }
}

/// A device the offsets are kept by, as the device rows (and
/// calibration's page) name it: worded from what its fingerprint said,
/// else from what its id says (a profile kept before), else its
/// description as stored (data).
pub(crate) fn device_name(p: &crate::state::settings::OffsetProfile) -> UiText {
    use ddi_l10n::msg::devices as d;
    use ddi_platform::{AudioLink, DeviceFacts};
    let facts = p
        .facts
        .clone()
        .filter(|f| !matches!(f, DeviceFacts::Described(_)))
        .or_else(|| DeviceFacts::from_id(&p.id));
    match facts {
        Some(DeviceFacts::Audio {
            name,
            rate,
            latency_ms,
            link,
        }) => {
            let khz = f64::from(rate) / 1000.0;
            match name {
                // The device's own name, as the system gives it.
                Some(name) => d::audio_named(UiText::data(name), khz, latency_ms),
                None => d::audio(
                    khz,
                    latency_ms,
                    match link {
                        Some(AudioLink::Wired) => "wired",
                        Some(AudioLink::Wireless) => "wireless",
                        Some(AudioLink::Unknown) => "unknown",
                        None => "none",
                    },
                ),
            }
        }
        Some(DeviceFacts::Display {
            name,
            width,
            height,
            scale_percent,
            refresh_hz,
        }) => {
            // Sizes as strings: a pixel count is never grouped (1,920).
            let (w, h) = (width.to_string(), height.to_string());
            let scale = f64::from(scale_percent) / 100.0;
            match (name, refresh_hz) {
                (Some(name), _) => d::display_named(UiText::data(name), w, h, scale),
                (None, Some(hz)) => d::display_refresh(w, h, scale, hz),
                (None, None) => d::display(w, h, scale),
            }
        }
        Some(DeviceFacts::Described(_)) | None => UiText::data(&p.label),
    }
}

const SPEED_SOURCES: [Labeled<SpeedSource>; 2] = [
    (SpeedSource::Player, m::speed_source_mine),
    (SpeedSource::Chart, m::speed_source_chart),
];
const SCROLL_ACTIONS: [Labeled<ScrollAction>; 4] = [
    (ScrollAction::Normal, m::scroll_action_normal),
    (ScrollAction::Boost, m::scroll_action_boost),
    (ScrollAction::Brake, m::scroll_action_brake),
    (ScrollAction::Wave, m::scroll_action_wave),
];
const TURNS: [Labeled<Turn>; 5] = [
    (Turn::Off, common::off),
    (Turn::Mirror, m::turn_mirror),
    (Turn::Left, m::turn_left),
    (Turn::Right, m::turn_right),
    (Turn::Shuffle, m::turn_shuffle),
];
const APPEARANCES: [Labeled<Appearance>; 5] = [
    (Appearance::Visible, m::appearance_normal),
    (Appearance::Hidden, m::appearance_hidden),
    (Appearance::Sudden, m::appearance_sudden),
    (Appearance::HiddenSudden, m::appearance_both),
    (Appearance::Stealth, m::appearance_stealth),
];
const CENTER_VERSIONS: [Labeled<CenterVersions>; 3] = [
    (CenterVersions::Off, common::off),
    (CenterVersions::Missing, m::simplify_center_missing),
    (CenterVersions::All, m::simplify_center_all),
];

const COVER_MODES: [Labeled<CoverMode>; 4] = [
    (CoverMode::Off, common::off),
    (CoverMode::Sudden, m::lane_cover_sudden),
    (CoverMode::Hidden, m::lane_cover_hidden),
    (CoverMode::HiddenSudden, m::lane_cover_both),
];

/// A cover's height row: its heights 0 to 100% by 1.
fn cover_height(id: &str, label: UiText, description: UiText, height: u8) -> OptionDef {
    row(
        id,
        label,
        OptionKind::Stepper(percents()),
        usize::from(height.min(LaneCover::FULL)),
        description,
    )
}

// A lane cover's stepper is its percentages.
const _: () = assert!(LaneCover::FULL == 100);

const CUTS: [Labeled<TimingCut>; 3] = [
    (TimingCut::Off, m::cut_all),
    (TimingCut::Quarters, m::cut_quarters),
    (TimingCut::Eighths, m::cut_eighths),
];
const AT_ONCE: [Labeled<AtOnce>; 3] = [
    (AtOnce::Any, m::at_once_any),
    (AtOnce::Two, m::at_once_two),
    (AtOnce::One, m::at_once_one),
];
const RULESET_MODES: [Labeled<RulesetMode>; 3] = [
    (RulesetMode::AllStepMania, m::ruleset_mode_ruleset),
    (RulesetMode::Original, m::ruleset_mode_game),
    (RulesetMode::AllDanoni, m::ruleset_mode_danoni),
];
const DDI_DIFFICULTIES: [Labeled<ddi_engine::rules::ddi::Difficulty>; 3] = [
    (ddi_engine::rules::ddi::Difficulty::Easy, m::ddi_easy),
    (ddi_engine::rules::ddi::Difficulty::Normal, m::ddi_normal),
    (ddi_engine::rules::ddi::Difficulty::Hard, m::ddi_hard),
];
const SCORE_DISPLAYS: [Labeled<ScoreDisplay>; 2] = [
    (ScoreDisplay::Score, m::ddi_score_score),
    (ScoreDisplay::Percent, m::ddi_score_percent),
];
const JUDGMENT_WORDS: [Labeled<JudgmentWords>; 2] = [
    (JudgmentWords::Localized, m::ddi_words_localized),
    (JudgmentWords::English, m::ddi_words_english),
];
const HOLD_STYLES: [Labeled<HoldStyle>; 3] = [
    (HoldStyle::ByGame, m::holds_by_game),
    (HoldStyle::Dance, m::holds_dance),
    (HoldStyle::Ticked, m::holds_ticked),
];
/// Dancing☆Onigiri's gauges by their names (`None`: the chart's first),
/// and whether each fails below a border.
const DANONI_GAUGES: [(Option<&str>, bool); 10] = [
    (None, false),
    (Some("Original"), false),
    (Some("Light"), false),
    (Some("Heavy"), false),
    (Some("NoRecovery"), false),
    (Some("SuddenDeath"), false),
    (Some("Practice"), false),
    (Some("Normal"), true),
    (Some("Easy"), true),
    (Some("Hard"), true),
];
const VIDEOS: [Labeled<VideoMode>; 3] = [
    (VideoMode::Auto, m::video_auto),
    (VideoMode::On, common::on),
    (VideoMode::Off, common::off),
];
const FIELD_PLACES: [Labeled<FieldPlace>; 5] = [
    (FieldPlace::Center, m::field_place_center),
    (FieldPlace::Left, m::field_place_left),
    (FieldPlace::Right, m::field_place_right),
    (FieldPlace::NearLeft, m::field_place_near_left),
    (FieldPlace::NearRight, m::field_place_near_right),
];

const GENERATED: [Labeled<GeneratedBackground>; 5] = [
    (GeneratedBackground::Off, common::off),
    (GeneratedBackground::Bars, m::generated_background_bars),
    (
        GeneratedBackground::Kaleidoscope,
        m::generated_background_kaleidoscope,
    ),
    (GeneratedBackground::Field, m::generated_background_field),
    (GeneratedBackground::Rain, m::generated_background_rain),
];
const FRAMES: [Labeled<BackgroundFrame>; 4] = [
    (BackgroundFrame::Auto, m::background_frame_auto),
    (BackgroundFrame::FourThree, || UiText::data("4:3")),
    (BackgroundFrame::SixteenNine, || UiText::data("16:9")),
    (BackgroundFrame::Screen, m::background_frame_screen),
];
const REDUCED_MOTION: [Labeled<ReducedMotion>; 3] = [
    (ReducedMotion::Auto, m::reduced_motion_system),
    (ReducedMotion::On, common::on),
    (ReducedMotion::Off, common::off),
];
const CURSORS: [Labeled<CursorStyle>; 3] = [
    (CursorStyle::InfinitoArrow, m::cursor_infinito),
    (CursorStyle::PointerTip, m::cursor_pointer),
    (CursorStyle::System, m::cursor_system),
];
const OPPONENT_FIELDS: [Labeled<OpponentFields>; 2] = [
    (OpponentFields::AsManyAsFit, m::opponent_fields_all),
    (OpponentFields::UpToEight, m::opponent_fields_eight),
];
const GRAPHICS: [Labeled<Graphics>; 2] = [
    (Graphics::Auto, m::graphics_auto),
    (Graphics::WebGl2, m::graphics_webgl2),
];
const FAIL_TYPES: [Labeled<FailType>; 4] = [
    (FailType::Immediate, m::fail_immediate),
    (FailType::ImmediateContinue, m::fail_delayed),
    (FailType::EndOfSong, m::fail_at_end),
    (FailType::Off, common::off),
];
const LIFE_TYPES: [Labeled<LifeType>; 2] = [
    (LifeType::Bar, m::life_type_bar),
    (LifeType::Battery, m::life_type_battery),
];
const HAND_LAYOUTS: [Labeled<HandLayout>; 7] = [
    (HandLayout::FollowField, m::hand_layout_follow),
    (HandLayout::FieldRight, m::hand_layout_right_left),
    (HandLayout::FieldLeft, m::hand_layout_left_right),
    (HandLayout::CenterHandRight, m::hand_layout_center_right),
    (HandLayout::CenterHandLeft, m::hand_layout_center_left),
    (HandLayout::NearRight, m::hand_layout_near_right_left),
    (HandLayout::NearLeft, m::hand_layout_near_left_right),
];
const PALETTES: [Labeled<Palette>; 5] = [
    (Palette::Ink, m::generated_palette_ink),
    (Palette::Dusk, m::generated_palette_dusk),
    (Palette::Moss, m::generated_palette_moss),
    (Palette::Ember, m::generated_palette_ember),
    (Palette::Mono, m::generated_palette_mono),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    use ddi_platform::storage::{self, Area, MemoryStore};

    #[test]
    fn devices_are_named_from_their_facts_their_ids_or_as_kept() {
        use crate::state::settings::OffsetProfile;
        use ddi_platform::{AudioLink, DeviceFacts, DeviceFingerprint};
        let kept = |fp: DeviceFingerprint| {
            let mut list = Vec::new();
            Settings::touch_profile(&mut list, &fp, 0.0);
            list.remove(0)
        };
        let audio = |name: Option<&str>, link| DeviceFacts::Audio {
            name: name.map(str::to_string),
            rate: 44100,
            latency_ms: 20,
            link,
        };
        let web = kept(DeviceFingerprint::new(
            "audio:44100:5:15",
            audio(None, Some(AudioLink::Wired)),
        ));
        // The English kept for older builds and the history says the same.
        assert_eq!(
            web.label,
            "44.1 kHz · ~20 ms output latency (wired/built-in?)"
        );
        let names = |tag: &str, profiles: &[OffsetProfile]| {
            ddi_l10n::set_language(tag);
            let names: Vec<String> = profiles.iter().map(|p| device_name(p).into()).collect();
            ddi_l10n::set_language("en");
            names
        };
        let desktop = kept(DeviceFingerprint::new(
            "audio:CoreAudio:Speakers:44100:20",
            audio(Some("Speakers"), None),
        ));
        let unknown = kept(DeviceFingerprint::new(
            "audio:44100:0:0",
            audio(None, Some(AudioLink::Unknown)),
        ));
        let screen = kept(DeviceFingerprint::new(
            "display:1512x982@2",
            DeviceFacts::Display {
                name: None,
                width: 1512,
                height: 982,
                scale_percent: 200,
                refresh_hz: Some(120),
            },
        ));
        // Kept before the facts were: worded from the id where it says,
        // else as described.
        let old = |id: &str, label: &str| OffsetProfile {
            id: id.into(),
            label: label.into(),
            facts: None,
            offset: 0.0,
            calibrated: false,
            last_seen: 0.0,
        };
        let profiles = [
            web,
            desktop,
            unknown,
            screen,
            old(
                "audio:48000:5:170",
                "48 kHz · ~175 ms output latency (wireless?)",
            ),
            old(
                "display:DELL U2720Q:3840x2160@1.5",
                "DELL U2720Q · 3840×2160 · ×1.5",
            ),
            old("display:9", "Headphones"),
            kept(DeviceFingerprint::new("audio:1", "Described")),
        ];
        assert_eq!(
            names("en", &profiles),
            [
                "44.1 kHz · ~20 ms output latency (wired/built-in?)",
                "Speakers · 44.1 kHz · ~20 ms output latency",
                "44.1 kHz · output latency unknown",
                "1512×982 (2×), ~120 Hz",
                "48 kHz · ~175 ms output latency (wireless?)",
                "DELL U2720Q · 3840×2160 · ×1.5",
                "Headphones",
                "Described",
            ]
        );
        assert_eq!(
            names("ja", &profiles)[..4],
            [
                "44.1 kHz · 出力遅延 約20 ms（有線・内蔵？）",
                "Speakers · 44.1 kHz · 出力遅延 約20 ms",
                "44.1 kHz · 出力遅延 不明",
                "1512×982（2×）、約120 Hz",
            ]
        );
        // Sizes are never grouped as counts are.
        let wide = kept(DeviceFingerprint::new(
            "display:x",
            DeviceFacts::Display {
                name: None,
                width: 5120,
                height: 2880,
                scale_percent: 125,
                refresh_hz: None,
            },
        ));
        assert_eq!(names("en", &[wide]), ["5120×2880 (1.25×)"]);
        // The rows say the device so.
        let mut m = model();
        m.settings.display_profiles = vec![profiles[3].clone()];
        assert_eq!(
            find(&m.data(), "profile.display.0").label,
            "Display: 1512×982 (2×), ~120 Hz"
        );
    }

    /// The hand in multiplayer is a row under the hand's, while it is on.
    #[test]
    fn the_hand_in_multiplayer_shows_under_the_hand_while_it_is_on() {
        let mut m = model();
        let ids = |m: &OptionsModel| -> Vec<String> {
            m.data()
                .sections
                .iter()
                .flat_map(|s| s.options.iter().map(|o| o.id.clone()))
                .collect()
        };
        let shown = ids(&m);
        let at = shown.iter().position(|i| i == "dancing_hand").unwrap();
        assert_eq!(shown[at + 1], "dancing_hand_multiplayer");
        assert_eq!(find(&m.data(), "dancing_hand_multiplayer").value(), "Off");
        m.change("dancing_hand_multiplayer", 1);
        assert!(m.settings.dancing_hand_multiplayer);
        assert!(Settings::load().dancing_hand_multiplayer);
        m.change("dancing_hand", 0);
        assert!(!ids(&m).iter().any(|i| i == "dancing_hand_multiplayer"));
        // Kept for when the hand is on again.
        assert!(m.settings.dancing_hand_multiplayer);
        m.change("dancing_hand", 1);
        assert_eq!(find(&m.data(), "dancing_hand_multiplayer").value(), "On");
    }

    #[test]
    fn dancing_hand_option_defaults_on_and_saved_off_persists() {
        let mut m = model();
        assert_eq!(find(&m.data(), "dancing_hand").value(), "On");
        m.change("dancing_hand", 0);
        assert!(!m.settings.dancing_hand);
        assert_eq!(find(&m.data(), "dancing_hand").value(), "Off");
        assert!(!Settings::load().dancing_hand);
    }

    #[test]

    fn hand_layout_presets_change_both_views_and_manual_field_resets_recipe() {
        let mut m = model();
        m.change("hand_layout", 5);
        assert_eq!(m.settings.field_place, FieldPlace::NearRight);
        assert_eq!(m.settings.hand_frame().center, 0.375);
        assert!(m.data().preview_whole_play);
        m.change("hand_frame_width", 4);
        m.change("hand_frame_height", 1);
        let saved = Settings::load();
        assert_eq!((saved.hand_frame_width, saved.hand_frame_height), (70, 60));
        m.change("field_place", 1);
        assert_eq!(m.settings.hand_layout, HandLayout::FollowField);
        assert_eq!(m.settings.hand_frame().center, 0.75);
        m.change("dancing_hand", 0);
        assert!(!m.data().preview_whole_play);
    }

    fn model() -> OptionsModel {
        storage::install(Rc::new(MemoryStore::default()));
        OptionsModel::new(
            Settings::default(),
            World {
                noteskins: vec![
                    ("bundled:dance/delta".into(), "Delta (with the game)".into()),
                    ("dance/mine".into(), "Mine".into()),
                ],
                pads_supported: true,
                sample: Sample::Ready,
                connected: vec![ConnectedPad {
                    vendor_product: None,
                    id: "Pad".into(),
                    index: 0,
                    standard: true,
                }],
                ..Default::default()
            },
        )
    }

    #[test]
    fn cursor_choices_and_size_round_trip_and_old_settings_get_the_arrow() {
        let mut m = model();
        assert_eq!(find(&m.data(), "cursor_style").value(), "Infinito arrow");
        for (i, (style, _)) in CURSORS.into_iter().enumerate() {
            m.change("cursor_style", i);
            assert_eq!(m.settings.cursor_style, style);
            let text = serde_json::to_string(&m.settings).unwrap();
            assert_eq!(
                serde_json::from_str::<Settings>(&text)
                    .unwrap()
                    .cursor_style,
                style
            );
        }
        m.change("cursor_size", 20);
        assert_eq!(m.settings.cursor_size, 36.0);
        let mut s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.cursor_style, CursorStyle::InfinitoArrow);
        assert_eq!(s.cursor_size, 28.0);
        s.cursor_size = f32::NAN;
        s.sanitize();
        assert_eq!(s.cursor_size, 28.0);
        s.cursor_size = 1000.0;
        s.sanitize();
        assert_eq!(s.cursor_size, 64.0);
        let s: Settings = serde_json::from_str(r#"{"cursor_style":"unknown"}"#).unwrap();
        assert_eq!(s.cursor_style, CursorStyle::InfinitoArrow);
    }

    /// Pump skins get a row of their own, which draws the pump layouts
    /// only; dance's row never lists them.
    #[test]
    fn the_start_countdown_defaults_to_three_seconds_and_takes_the_offered_ones() {
        let mut m = model();
        assert_eq!(find(&m.data(), "start_countdown").value(), "3 s");
        m.change("start_countdown", 0);
        assert_eq!(m.settings.start_countdown, 0.0);
        assert_eq!(find(&m.data(), "start_countdown").value(), "Off");
        // Stored before it: on, at 3 s; a hand edit: the nearest offered.
        let s: Settings = serde_json::from_str(r#"{"speed": 2.0}"#).unwrap();
        assert_eq!(s.start_countdown, 3.0);
        let mut s: Settings = serde_json::from_str(r#"{"start_countdown": 4.2}"#).unwrap();
        s.sanitize();
        assert_eq!(s.start_countdown, 5.0);
    }

    #[test]
    fn pump_skins_are_chosen_for_the_pump_layouts() {
        let mut m = model();
        assert!(
            !m.data()
                .sections
                .iter()
                .any(|s| s.options.iter().any(|o| o.id == "pump_noteskin"))
        );
        let mut skins = m.world.noteskins.clone();
        skins.push(("pump/cmd".into(), "cmd".into()));
        m.set_noteskins(skins.clone(), false, true);
        let d = m.data();
        let dance = find(&d, "noteskin");
        let OptionKind::Choices(names) = &dance.kind else {
            panic!("{dance:?}")
        };
        assert!(!names.iter().any(|n| n == "cmd"), "{names:?}");
        let pump = find(&d, "pump_noteskin");
        let OptionKind::Choices(names) = &pump.kind else {
            panic!("{pump:?}")
        };
        assert_eq!(names, &["As above", "cmd"]);
        m.change("pump_noteskin", 1);
        assert_eq!(m.settings.pump_noteskin.as_deref(), Some("pump/cmd"));
        m.change("noteskin", 1);
        assert_eq!(m.settings.noteskin_for("pump-single"), Some("pump/cmd"));
        assert_eq!(
            m.settings.noteskin_for("dance-single"),
            Some("bundled:dance/delta")
        );
        // An imported pump skin can be removed from its row.
        let _ = find(&m.data(), "pump_noteskin.remove");
        let remove = m.action("pump_noteskin.remove", 0);
        assert_eq!(remove, Act::Confirm(Reset::Noteskin("pump/cmd".into())));
        // A dance skin's key there is never used for the pump layouts.
        m.settings.pump_noteskin = Some("dance/mine".into());
        assert_eq!(
            m.settings.noteskin_for("pump-single"),
            Some("bundled:dance/delta")
        );
        m.settings.pump_noteskin = Some("pump/cmd".into());
        // Gone from the library: let go.
        skins.pop();
        m.set_noteskins(skins, false, true);
        assert_eq!(m.settings.pump_noteskin, None);
    }

    /// A chosen skin not among those listed (desktop's, chosen on another
    /// machine, stays chosen) is previewed as the Arrows row shows it.
    #[test]
    fn a_skin_not_listed_previews_as_not_chosen() {
        let mut m = model();
        m.settings.noteskin = Some("dance/elsewhere".into());
        // Not listed yet: as chosen.
        m.world.noteskins_listed = false;
        assert_eq!(
            m.preview_settings().noteskin.as_deref(),
            Some("dance/elsewhere")
        );
        let skins = m.world.noteskins.clone();
        m.set_noteskins(skins, false, false);
        assert_eq!(m.settings.noteskin.as_deref(), Some("dance/elsewhere"));
        assert_eq!(m.preview_settings().noteskin, None);
        assert_eq!(find(&m.data(), "noteskin").index, 0);
    }

    #[test]
    fn an_announcer_is_chosen_its_volume_set_and_removed() {
        let has = |m: &OptionsModel, id: &str| {
            m.data()
                .sections
                .iter()
                .flat_map(|s| &s.options)
                .any(|o| o.id == id)
        };
        let mut m = model();
        // Off, none listed: the row alone.
        assert_eq!(find(&m.data(), "announcer").values(), ["Off"]);
        assert!(!has(&m, "announcer_volume"));
        m.set_announcers(
            vec![
                ("ddr 1st mix".into(), "DDR 1st Mix".into()),
                ("techno".into(), "Techno".into()),
            ],
            true,
        );
        assert_eq!(m.change("announcer", 2), Some(Effect::Announce));
        assert_eq!(m.settings.announcer.as_deref(), Some("techno"));
        assert_eq!(find(&m.data(), "announcer").index, 2);
        // Its volume, a share of the music's, on the music's scale.
        assert_eq!(find(&m.data(), "announcer_volume").value(), "100%");
        m.change("announcer_volume", 50);
        assert!((m.settings.announcer_volume - 0.125).abs() < 1e-6);
        assert_eq!(find(&m.data(), "announcer_volume").note, "-18 dB");
        // Removed from its row on the web; desktop reads them in place.
        assert_eq!(
            m.action("announcer.remove", 0),
            Act::Confirm(Reset::Announcer("techno".into()))
        );
        m.world.platform = super::super::data::Platform::Desktop;
        assert!(!has(&m, "announcer.remove"));
        m.world.platform = super::super::data::Platform::Web;
        let (_, body, _) = Reset::Announcer("techno".into()).texts(&m);
        assert!(body.starts_with("Techno is removed"), "{body}");
        m.reset(&Reset::Announcer("techno".into()));
        assert_eq!(m.settings.announcer, None);
        // One chosen and not listed yet shows by its key; gone once all
        // are listed.
        m.settings.announcer = Some("elsewhere".into());
        let row = find(&m.data(), "announcer").clone();
        assert_eq!(row.values()[row.index], "elsewhere");
        let listed = m.world.announcers.clone();
        m.set_announcers(listed.clone(), false);
        assert_eq!(m.settings.announcer.as_deref(), Some("elsewhere"));
        m.set_announcers(listed, true);
        assert_eq!(m.settings.announcer, None);
        // Removing the imported songs lets it go too.
        m.change("announcer", 1);
        m.reset(&Reset::Songs);
        assert_eq!(m.settings.announcer, None);
    }

    #[test]
    fn our_announcers_come_first_and_stay() {
        let has = |m: &OptionsModel, id: &str| {
            m.data()
                .sections
                .iter()
                .flat_map(|s| &s.options)
                .any(|o| o.id == id)
        };
        let mut m = model();
        // Off by default.
        assert_eq!(m.settings.announcer, None);
        m.set_announcers(
            vec![
                ("techno".into(), "Techno".into()),
                ("bundled:mr. clank".into(), "Mr. Clank".into()),
                ("ddr".into(), "ddr".into()),
            ],
            true,
        );
        assert_eq!(
            find(&m.data(), "announcer").values(),
            ["Off", "Mr. Clank", "ddr", "Techno"]
        );
        assert_eq!(m.settings.announcer, None);
        m.change("announcer", 1);
        assert_eq!(m.settings.announcer.as_deref(), Some("bundled:mr. clank"));
        // Never removed, nor let go with the imported songs.
        assert!(has(&m, "announcer_volume"));
        assert!(!has(&m, "announcer.remove"));
        m.reset(&Reset::Songs);
        assert_eq!(m.settings.announcer.as_deref(), Some("bundled:mr. clank"));
        // The studio opens from here, Off or not, on both shells.
        assert!(has(&m, "announcer.studio"));
        assert_eq!(m.action("announcer.studio", 0), Act::Studio);
        m.change("announcer", 0);
        assert!(has(&m, "announcer.studio"));
        m.world.platform = super::super::data::Platform::Desktop;
        assert!(has(&m, "announcer.studio"));
        // Desktop removes only those made in the studio.
        m.world.announcers = vec![
            ("made".into(), "Made".into()),
            ("found".into(), "Found".into()),
        ];
        m.world.made_announcers = vec!["made".into()];
        m.settings.announcer = Some("found".into());
        assert!(!has(&m, "announcer.remove"));
        m.settings.announcer = Some("made".into());
        assert!(has(&m, "announcer.remove"));
    }

    fn find<'a>(d: &'a OptionsData, id: &str) -> &'a OptionDef {
        d.sections
            .iter()
            .flat_map(|s| &s.options)
            .find(|o| o.id == id)
            .unwrap_or_else(|| panic!("no {id}"))
    }

    /// DDI, first in the list and the default, shows its own rows and
    /// none of StepMania's; another preset shows StepMania's and none of
    /// DDI's. The kept plays' row judges them again when there are any.
    #[test]
    fn ddi_shows_its_own_rows_and_hides_stepmanias() {
        use ddi_engine::rules::ddi::Difficulty;
        let has = |m: &OptionsModel, id: &str| {
            m.data()
                .sections
                .iter()
                .flat_map(|s| &s.options)
                .any(|o| o.id == id)
        };
        const DDI: [&str; 4] = ["ddi.difficulty", "ddi.boos", "ddi.practice", "ddi.score"];
        const STEPMANIA: [&str; 7] = [
            "holds",
            "fail",
            "life",
            "judge",
            "life_type",
            "lives",
            "ddr_gauge",
        ];
        let mut m = model();
        let d = m.data();
        assert_eq!(find(&d, "ruleset").index, 0);
        assert_eq!(find(&d, "ruleset").value(), "DDI");
        assert_eq!(find(&d, "ruleset").values()[1], "ITG (Simply Love)");
        for id in DDI {
            assert!(has(&m, id), "{id}");
        }
        for id in STEPMANIA {
            assert!(!has(&m, id), "{id}");
        }
        // The defaults: Normal, Boos off, Practice off, the score.
        assert_eq!(find(&d, "ddi.difficulty").value(), "Normal");
        assert_eq!(find(&d, "ddi.boos").value(), "Off");
        assert_eq!(find(&d, "ddi.practice").value(), "Off");
        assert_eq!(find(&d, "ddi.score").value(), "Score");
        m.change("ddi.difficulty", 2);
        m.change("ddi.boos", 1);
        m.change("ddi.practice", 1);
        m.change("ddi.score", 1);
        let s = Settings::load();
        assert_eq!(s.ddi.difficulty, Difficulty::Hard);
        assert!(s.ddi.boos && s.ddi.practice);
        assert_eq!(s.ddi_score, ScoreDisplay::Percent);
        assert_eq!(find(&m.data(), "ddi.difficulty").value(), "Hard");
        // Dancing☆Onigiri's rows stay under either.
        assert!(has(&m, "danoni.gauge") && has(&m, "ruleset_mode"));
        // ITG: StepMania's rows, none of DDI's (its options are kept).
        m.change("ruleset", 1);
        assert_eq!(m.settings.ruleset, "itg");
        for id in DDI {
            assert!(!has(&m, id), "{id}");
        }
        for id in STEPMANIA {
            assert!(has(&m, id), "{id}");
        }
        m.change("ruleset", 0);
        assert_eq!(m.settings.ruleset, "ddi");
        assert_eq!(m.settings.ddi.difficulty, Difficulty::Hard);
        // No play kept: nothing to judge again.
        assert!(find(&m.data(), REJUDGE).values().is_empty());
        assert_eq!(m.action(REJUDGE, 0), Act::Nothing);
        m.world.plays = 3;
        assert_eq!(find(&m.data(), REJUDGE).values(), ["Judge again"]);
        assert_eq!(m.action(REJUDGE, 0), Act::Rejudge);
        assert_eq!(m.notice, "3 kept plays are being judged again.");
    }

    #[test]
    fn every_row_shows_its_setting_and_changes_it() {
        let mut m = model();
        let d = m.data();
        // Ids are unique.
        let mut ids: Vec<&str> = d
            .sections
            .iter()
            .flat_map(|s| &s.options)
            .map(|o| o.id.as_str())
            .collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
        assert_eq!(find(&d, "speed").value(), "×2.5");
        // A fresh profile's volume: 69% exactly, a step from 68% and 70%.
        assert_eq!(find(&d, "volume").value(), "69%");
        assert_eq!(find(&d, "volume").note, "-10 dB");
        for (i, shown) in [(68, "68%"), (70, "70%"), (69, "69%")] {
            m.change("volume", i);
            assert_eq!(find(&m.data(), "volume").value(), shown);
        }
        assert!((m.settings.volume - Settings::default().volume).abs() < 1e-6);
        // Every choice row, every value: applied and shown again.
        for o in d.sections.iter().flat_map(|s| &s.options) {
            if matches!(o.kind, OptionKind::Actions(_)) || o.id == "bind_layout" {
                continue;
            }
            for i in [o.values().len() - 1, 0] {
                m.change(&o.id, i);
                let again = m.data();
                assert_eq!(find(&again, &o.id).index, i, "{}", o.id);
            }
            // (The hand's rows under it show while it is on.)
            if o.id == "dancing_hand" {
                m.change("dancing_hand", 1);
            }
        }
        m.change("dancing_hand", 0);
        // Saved as changed.
        m.change("reverse", 1);
        m.change("speed", 11);
        m.change("field_place", 2);
        let stored: Settings = storage::load(Area::Local, crate::state::settings::KEY).unwrap();
        assert!(stored.reverse);
        assert_eq!(stored.speed, 3.0);
        assert_eq!(stored.field_place, FieldPlace::Right);
        assert_eq!(m.data().preview, "Infinito · Single · ×3 · Reverse");
        m.change("noteskin", 2);
        assert_eq!(m.settings.noteskin.as_deref(), Some("dance/mine"));
        assert!(
            m.data()
                .sections
                .iter()
                .any(|s| s.options.iter().any(|o| o.id == "noteskin.remove"))
        );
        assert_eq!(
            m.action("noteskin.remove", 0),
            Act::Confirm(Reset::Noteskin("dance/mine".into()))
        );
    }

    /// The chosen skin's parameters are rows under Arrows: Infinito's (built
    /// in), or those the shell read for another skin; set values are
    /// kept per skin, a default forgotten, and a removed skin's let go.
    #[test]
    fn a_skins_parameters_are_rows_under_arrows() {
        let mut m = model();
        let d = m.data();
        // Infinito's, with the arrows' row first; no Built-in choice.
        let arrows = &d
            .sections
            .iter()
            .find(|s| s.name == "Arrows")
            .unwrap()
            .options;
        let ids: Vec<&str> = arrows.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "noteskin",
                "skin.Colors",
                "skin.NoteTimedCharge",
                "skin.Onigiri",
                "skin.ShockArrows",
                "skin.Explosions",
                "skin.ReceptorFill",
                "skin.HitVariation"
            ]
        );
        assert!(
            !arrows[0]
                .values()
                .iter()
                .any(|v| v.contains("Built") || v.contains("Spiral"))
        );
        assert_eq!(find(&d, "skin.Colors").value(), "Vivid");
        assert_eq!(
            find(&d, "skin.Colors").values(),
            ["Vivid", "Rainbow", "Flat"]
        );
        for (i, name) in [(1, "Rainbow"), (2, "Flat")] {
            m.change("skin.Colors", i);
            assert_eq!(
                m.settings.skin_params(INFINITO).get("Colors"),
                Some(&ParamValue::Text(name.into()))
            );
            assert_eq!(
                Some(m.settings.play_skin("dance-single").as_str()),
                Some(INFINITO)
            );
        }
        m.change("skin.Colors", 0);
        assert!(
            m.settings.skin_params.is_empty(),
            "Vivid is the unstored default"
        );
        assert_eq!(find(&d, "skin.ShockArrows").value(), "Off");
        assert_eq!(find(&d, "skin.Explosions").value(), "On");
        m.change("skin.ShockArrows", 1);
        let stored: Settings = storage::load(Area::Local, crate::state::settings::KEY).unwrap();
        assert_eq!(
            stored.skin_params(INFINITO).get("ShockArrows"),
            Some(&ParamValue::Bool(true))
        );
        m.change("skin.ShockArrows", 0);
        assert!(m.settings.skin_params.is_empty());
        m.change("skin.ShockArrows", 1);
        assert_eq!(
            Some(m.settings.play_skin("dance-single").as_str()),
            Some(INFINITO)
        );
        // Another skin's, as the shell read them.
        let declared = ddi_noteskin::params::Declared::from_ini(&ddi_noteskin::ini::Ini::parse(
            "[Parameters]\nList=Size,Tint\n\
             [Parameter Size]\nType=Number\nMin=0.5\nMax=1.5\nStep=0.25\nDefault=1\nUnit=×\n\
             [Parameter Tint]\nType=Color\nDefault=#FF8800\nChoices=#FF8800,#0088FF\n\
             ChoiceLabels=Orange,Blue\n",
        ));
        m.set_skin_parameters([("dance/mine".to_string(), declared.parameters)].into());
        m.change("noteskin", 2);
        let d = m.data();
        assert!(
            d.sections
                .iter()
                .all(|s| s.options.iter().all(|o| o.id != "skin.ShockArrows"))
        );
        let size = find(&d, "skin.Size");
        assert!(matches!(size.kind, OptionKind::Stepper(_)));
        assert_eq!(size.values(), ["0.5×", "0.75×", "1×", "1.25×", "1.5×"]);
        assert_eq!(size.value(), "1×");
        m.change("skin.Size", 4);
        m.change("skin.Tint", 1);
        assert_eq!(
            m.settings.skin_params("dance/mine"),
            [
                ("Size".to_string(), ParamValue::Number(1.5)),
                ("Tint".to_string(), ParamValue::Text("#0088FF".into())),
            ]
            .into()
        );
        // Infinito's stay its own.
        assert_eq!(
            m.settings.skin_params(INFINITO).get("ShockArrows"),
            Some(&ParamValue::Bool(true))
        );
        // A color off the palette (a hand edit) shows as itself.
        m.settings.set_skin_param(
            "dance/mine",
            "Tint",
            Some(ParamValue::Text("#123456".into())),
            None,
        );
        assert_eq!(find(&m.data(), "skin.Tint").value(), "#123456");
        // Removed: its values go with it.
        m.reset(&Reset::Noteskin("dance/mine".into()));
        assert_eq!(m.settings.noteskin, None);
        assert!(m.settings.skin_params("dance/mine").is_empty());
        assert_eq!(
            m.settings.skin_params(INFINITO).get("ShockArrows"),
            Some(&ParamValue::Bool(true))
        );
    }

    #[test]
    fn no_stretch_is_a_saved_play_simplification() {
        let mut m = model();
        assert_eq!(find(&m.data(), "no_stretch").value(), "Off");
        assert!(m.change("no_stretch", 1).is_some());
        assert_eq!(find(&m.data(), "no_stretch").value(), "On");
        let stored: Settings = storage::load(Area::Local, crate::state::settings::KEY).unwrap();
        assert!(stored.transform.no_stretch);
        let options = stored.play_options(None);
        assert!(options.transform.is_assist());
        assert!(super::super::play::options_line(&options).contains("no stretch"));
        assert!(m.change("no_stretch", 0).is_some());
        assert!(!Settings::load().transform.no_stretch);
        assert!(
            !super::super::play::options_line(&m.settings.play_options(None))
                .contains("no stretch")
        );
    }

    #[test]
    fn lane_cover_rows_follow_the_covers_chosen() {
        let mut m = model();
        let has = |m: &OptionsModel, id: &str| {
            m.data()
                .sections
                .iter()
                .any(|s| s.options.iter().any(|o| o.id == id))
        };
        assert!(has(&m, "lane_cover"));
        assert!(!has(&m, "cover_sudden") && !has(&m, "cover_hidden"));
        m.change("lane_cover", 1);
        assert_eq!(m.settings.lane_cover.mode, CoverMode::Sudden);
        assert!(has(&m, "cover_sudden") && !has(&m, "cover_hidden"));
        assert_eq!(find(&m.data(), "cover_sudden").value(), "25%");
        m.change("cover_sudden", 40);
        assert_eq!(m.settings.lane_cover.sudden, 40);
        m.change("lane_cover", 3);
        assert!(has(&m, "cover_sudden") && has(&m, "cover_hidden"));
        m.change("cover_hidden", 100);
        assert_eq!(find(&m.data(), "cover_hidden").value(), "100%");
        let stored: Settings = storage::load(Area::Local, crate::state::settings::KEY).unwrap();
        assert_eq!(
            stored.lane_cover,
            LaneCover {
                mode: CoverMode::HiddenSudden,
                sudden: 40,
                hidden: 100,
            }
        );
        // Played with: the start prompt and song select say it.
        assert!(
            super::super::play::options_line(&m.settings.play_options(None))
                .ends_with("hidden+ 100% sudden+ 0%"),
        );
        m.change("lane_cover", 0);
        assert!(!has(&m, "cover_sudden") && !has(&m, "cover_hidden"));
    }

    #[test]
    fn controller_artwork_preferences_never_materialize_or_replace_bindings() {
        use crate::controller_hints::Style;
        let mut m = model();
        let before = m.settings.controls.clone();
        assert!(m.change("pad_style.0", 2).is_some());
        assert_eq!(m.settings.controls, before);
        assert_eq!(
            Settings::load().controller_styles["Pad"],
            Style::PlayStation
        );
        let mut saved = PadBindings::standard("Pad");
        saved.single[0] = vec!["button:14".into()];
        saved.single[1].clear();
        m.settings.set_pad_bindings(saved);
        let before = m.settings.controls.clone();
        m.change("pad_style.0", 3);
        assert_eq!(m.settings.controls, before);
        let reset = find(&m.data(), "pad.0")
            .values()
            .iter()
            .position(|s| s == "Use defaults")
            .unwrap();
        m.action("pad.0", reset);
        assert_eq!(m.settings.controller_styles["Pad"], Style::Nintendo);
        m.world.connected.clear();
        assert_eq!(find(&m.data(), "pad_style.0").index, 3);
        let forget = find(&m.data(), "pad.0")
            .values()
            .iter()
            .position(|s| s == "Forget")
            .unwrap();
        m.action("pad.0", forget);
        assert!(m.settings.controller_styles.is_empty());
        assert!(m.settings.controls.pads.is_empty());
        // A future/invalid artwork value cannot discard unrelated custom input.
        let mut json = serde_json::to_value(Settings {
            controls: before.clone(),
            ..Settings::default()
        })
        .unwrap();
        json["controller_styles"] = serde_json::json!({"Pad":"future_style"});
        let loaded: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(loaded.controls, before);
    }

    #[test]
    fn keys_bindings_and_resets() {
        let mut m = model();
        assert_eq!(m.action("keys.lane.0", 0), Act::AddKey(0));
        let down = m.settings.controls.keyboard(&Layout::dance_single())[1][0].clone();
        m.add_key(0, &down);
        assert!(m.notice.contains("moved from Down to Left"), "{}", m.notice);
        let keys = m.settings.controls.keyboard(&Layout::dance_single());
        assert!(keys[0].contains(&down) && !keys[1].contains(&down));
        assert_eq!(m.action("keys.lane.0", 1), Act::Done);
        assert!(m.settings.controls.keyboard(&Layout::dance_single())[0].is_empty());
        assert_eq!(m.action("keys", 1), Act::Done);
        assert!(
            m.settings
                .controls
                .keyboard_is_default(&Layout::dance_single())
        );
        // The connected standard pad: bind, use defaults after a binding.
        assert_eq!(
            m.action("pad.0", 0),
            Act::Bind(Some(DeviceId::Gamepad("Pad".into())))
        );
        m.bound(Bound::Pad {
            id: "Pad".into(),
            table: vec![vec!["button:1".into()]; 4],
            start: None,
            back: None,
        });
        let d = m.data();
        assert_eq!(find(&d, "pad.0.lane.0").note, "East");
        assert_eq!(find(&d, "pad.0").note, "saved · connected");
        assert!(
            find(&d, "pad.0")
                .values()
                .contains(&"Use defaults".to_string())
        );
        // Resets: asked first; the settings' part keeps the songs' offsets.
        assert_eq!(m.action("reset.settings", 0), Act::Confirm(Reset::Settings));
        m.settings.set_song_offset("u-1", 0.01);
        m.change("reverse", 1);
        m.reset(&Reset::Settings);
        assert!(!m.settings.reverse);
        assert_eq!(m.settings.song_offset("u-1"), 0.01);
        m.reset(&Reset::Songs);
        assert_eq!(m.settings.song_offset("u-1"), 0.0);
        // The bindings follow another layout.
        let doubles = m
            .bind_layouts()
            .iter()
            .position(|l| l.id == "dance-double")
            .unwrap();
        m.change("bind_layout", doubles);
        assert_eq!(find(&m.data(), "keys.lane.7").label, "Right (pad 2)");
    }

    #[test]
    fn a_speed_off_the_steps_shows_as_it_is_and_the_preview_takes_a_layout() {
        let mut m = model();
        m.settings.speed = 2.3;
        let d = m.data();
        let speed = find(&d, "speed");
        assert_eq!(speed.value(), "×2.3");
        m.change("speed", speed.index + 1);
        assert_eq!(m.settings.speed, 2.5);
        assert_eq!(find(&m.data(), "speed").value(), "×2.5");
        assert!(
            !find(&m.data(), "speed")
                .values()
                .contains(&"×2.3".to_string())
        );
        // The preview's layout: the style's until another is picked.
        let d = m.data();
        let p = d.preview_layout.as_ref().unwrap();
        assert_eq!(p.value(), "Single");
        let solo = p.values().iter().position(|v| v == "Solo").unwrap();
        assert!(m.change("preview_layout", solo).is_some());
        assert_eq!(m.preview_settings().style, "dance-solo");
        assert!(m.data().preview.contains(" · Solo · "));
        assert_eq!(m.settings.style, "dance-single");
    }
}
