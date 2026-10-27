//! The options screen on the player's settings (`docs/plans/ui-core.md`,
//! step 6.3): every setting as a row of [`OptionsData`], each change
//! applied to [`Settings`] and saved, and the actions (bind, reset,
//! export, the volume sample) named for the shell. Pure: the shell says
//! what only it knows ([`World`]: the noteskins, the controllers
//! connected, the library's counts) and carries out what only it can
//! ([`Act`]).

use ddi_chart::Layout;
use ddi_engine::rules::danoni::JudgeRange;
use ddi_engine::rules::ddr::DdrGauge;
use ddi_engine::rules::presets::{HoldStyle, RulesetMode};
use ddi_engine::rules::{FailType, JUDGE_SCALES, LIFE_SCALES, LifeType, StepManiaOptions};
use ddi_engine::scroll::SpeedSource;
use ddi_engine::{
    Appearance, AtOnce, ConnectedPad, CoverMode, LaneCover, PadUse, ScrollAction, TimingCut, Turn,
};
use ddi_platform::DeviceId;
use ddi_platform::gamepad::Control;

use super::data::{OptionDef, OptionKind, OptionSection, OptionsData, trim_number};
use super::options::CALIBRATE;
use crate::state::bind::Bound;
use crate::state::settings::{
    BUILT_IN, BUNDLED, BackgroundFrame, CenterVersions, FIELD_FILTERS, Graphics, INFINITO,
    PadBindings, ParamValue, ReducedMotion, START_COUNTDOWNS, Settings, VideoMode,
    slider_to_volume, volume_to_slider,
};
use ddi_noteskin::params::{Kind as ParamKind, Parameter};

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
    /// The default skin's Lua is off for the session.
    pub default_lua_off: bool,
    /// The parameters skins declare (`docs/plans/skin-parameters.md`),
    /// by key; Infinito's are known without it.
    pub skin_parameters: std::collections::BTreeMap<String, Vec<Parameter>>,
    /// Layouts the library has charts for (offered for bindings).
    pub library_styles: Vec<Layout>,
    /// The controllers connected, one by one.
    pub connected: Vec<ConnectedPad>,
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
}

/// What a confirmation is for.
#[derive(Clone, Debug, PartialEq)]
pub enum Reset {
    /// Every setting (device calibrations too) but the songs' offsets.
    Settings,
    /// Imported songs, their files and offsets, imported noteskins and
    /// shared images.
    Songs,
    /// Both, and the bundled songs' offsets: like a first visit.
    Everything,
    /// An imported noteskin (its key).
    Noteskin(String),
    /// Every play kept (the bests go with them).
    History,
}

impl Reset {
    /// The confirmation's title, text and button.
    pub fn texts(&self, model: &OptionsModel) -> (String, String, String) {
        match self {
            Reset::Settings => (
                "Reset settings?".into(),
                "Speed, scroll and note options, rules, volume, keys, controller bindings, \
                 display options and device calibrations go back to their defaults. Imported \
                 songs and per-song offsets are kept."
                    .into(),
                "Reset".into(),
            ),
            // Desktop reads song folders in place: they are forgotten,
            // never deleted.
            Reset::Songs if model.world.folders.is_some() => (
                "Forget the song folders?".into(),
                format!(
                    "The game stops reading {} and forgets the songs and packs taken off the \
                     list and the offsets of the folders' songs. Nothing in the folders is \
                     deleted; settings are kept.",
                    count(
                        model.world.folders.as_ref().map_or(0, Vec::len),
                        "song folder",
                        "song folders"
                    ),
                ),
                "Forget".into(),
            ),
            Reset::Everything if model.world.folders.is_some() => (
                "Reset everything?".into(),
                format!(
                    "Settings back to their defaults, the song folders forgotten (nothing in them \
                     is deleted), {}, and the offsets of the bundled songs too: like a first \
                     start.",
                    plays_deleted(model.world.plays)
                ),
                "Reset everything".into(),
            ),
            Reset::Songs => (
                "Remove imported songs?".into(),
                format!(
                    "Deletes the songs imported into this browser ({}) and their offsets, the \
                     imported noteskins and the shared background images ({}). Settings are kept.",
                    count(model.world.imported, "imported song", "imported songs"),
                    count(model.world.shared, "image", "images")
                ),
                "Remove".into(),
            ),
            Reset::Everything => (
                "Reset everything?".into(),
                format!(
                    "Settings back to their defaults, imported songs and noteskins removed, {}, \
                     and the offsets of the bundled songs too: like a first visit.",
                    plays_deleted(model.world.plays)
                ),
                "Reset everything".into(),
            ),
            Reset::History => (
                "Reset the play history?".into(),
                format!(
                    "Deletes {} kept here, and the bests and lamps they give; songs and settings \
                     are kept. Export them first to keep a copy.",
                    match model.world.plays {
                        1 => "the 1 play".to_string(),
                        n => format!("the {n} plays"),
                    }
                ),
                "Delete".into(),
            ),
            Reset::Noteskin(key) => (
                "Remove this noteskin?".into(),
                format!(
                    "{} is removed from this browser; {}",
                    model.skin_name(Some(key)),
                    if crate::state::settings::is_pump_skin(key) {
                        "the pump styles are drawn with the arrows above again."
                    } else {
                        "songs play with Infinito again."
                    }
                ),
                "Remove".into(),
            ),
        }
    }
}

/// The play history's buttons: Export and Reset with plays kept, Import
/// unless one runs.
fn history_buttons(plays: usize, importing: bool) -> &'static [&'static str] {
    match (plays, importing) {
        (0, false) => &["Import…"],
        (0, true) => &[],
        (_, false) => &["Export", "Import…", "Reset"],
        (_, true) => &["Export"],
    }
}

/// "the 3 plays kept deleted" (a reset's part).
fn plays_deleted(n: usize) -> String {
    match n {
        0 => "the play history (empty) cleared".to_string(),
        1 => "the 1 play kept deleted".to_string(),
        n => format!("the {n} plays kept deleted"),
    }
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
    /// Import plays from an export (the platform's picker, inside the
    /// press).
    ImportPlays,
    /// Ask first, then [`OptionsModel::reset`] (and the shell's part).
    Confirm(Reset),
    /// Calibration (its own screen).
    Calibrate,
    /// Autoplay a bundled song (on WebGL2).
    Demo {
        gl: bool,
    },
    /// Add a song folder (the platform's picker), forget one (by its
    /// place in the list), or read them all again (desktop).
    AddFolder,
    RemoveFolder(usize),
    Rescan,
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
            Some(BUILT_IN) => "Built-in arrows".into(),
            Some(k) => self
                .world
                .noteskins
                .iter()
                .find(|(key, _)| key == k)
                .map_or_else(|| k.to_string(), |(_, n)| n.clone()),
        }
    }

    /// The rows.
    pub fn data(&self) -> OptionsData {
        let s = &self.settings;
        let shown = self.preview_layout.as_deref().unwrap_or(&s.style);
        let style = self
            .bind_layouts()
            .into_iter()
            .find(|l| l.id == shown)
            .map_or_else(|| shown.to_string(), |l| l.name);
        let mut preview = vec![
            self.skin_name(s.noteskin_for(shown)),
            style,
            format!("×{}", trim_number(s.speed, 2)),
        ];
        if s.reverse {
            preview.push("Reverse".into());
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
                self.controls(),
                self.data_section(),
            ]
            .into_iter()
            .chain(self.folders())
            .collect(),
            preview_layout: Some(self.preview_layout_option()),
            preview: preview.join(" · "),
            notice: self.notice.clone(),
        }
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
        let speeds: Vec<String> = values
            .iter()
            .map(|v| format!("×{}", trim_number(*v, 2)))
            .collect();
        let at = values
            .iter()
            .position(|v| (v - s.speed).abs() < 1e-6)
            .unwrap_or(0);
        section(
            "Scroll",
            vec![
                row(
                    "speed",
                    "Speed",
                    OptionKind::Stepper(speeds),
                    at,
                    "How fast the arrows scroll: the chart's tempo times this.",
                ),
                choice(
                    "speed_source",
                    "Charts with their own speed",
                    &["Mine", "The chart's"],
                    pos(&SPEED_SOURCES, s.speed_source),
                    "Some charts set their own scroll speed (Dancing☆Onigiri works): play them \
                     at the speed above, or at theirs.",
                ),
                choice(
                    "reverse",
                    "Reverse",
                    &["Off", "On"],
                    s.reverse as usize,
                    "Arrows scroll down to receptors at the bottom.",
                ),
                choice(
                    "scroll_action",
                    "Scroll action",
                    &["Normal", "Boost", "Brake", "Wave"],
                    pos(&SCROLL_ACTIONS, s.scroll_action),
                    "Boost speeds the arrows up near the receptors, Brake slows them down, Wave \
                     sways them.",
                ),
            ],
        )
    }

    fn notes(&self) -> OptionSection {
        let s = &self.settings;
        let t = &s.transform;
        let mut cut = choice(
            "cut",
            "Notes",
            &["All", "Quarters", "Quarters and eighths"],
            pos(&CUTS, t.cut),
            "Leaves out the faster notes. Simplified plays are marked as assisted on the \
             results.",
        );
        cut.heading = Some("SIMPLIFY".into());
        let cover = s.lane_cover;
        let mut rows = vec![
            choice(
                "turn",
                "Turn",
                &["Off", "Mirror", "Left", "Right", "Shuffle"],
                pos(&TURNS, t.turn),
                "Moves every arrow to another lane, the same way for the whole chart; Shuffle \
                     makes a new lane order each play.",
            ),
            choice(
                "appearance",
                "Appearance",
                &["Normal", "Hidden", "Sudden", "Both", "Stealth"],
                pos(&APPEARANCES, s.appearance),
                "Hidden: notes vanish before the receptors. Sudden: they appear late. \
                     Stealth: no notes at all.",
            ),
            choice(
                "lane_cover",
                "Lane cover",
                &["Off", "Sudden+", "Hidden+", "Both"],
                pos(&COVER_MODES, cover.mode),
                &format!(
                    "A panel over the lanes. Sudden+ covers the end the notes come from, Hidden+ the end they go to. {}",
                    if crate::controller_hints::active() {
                        crate::controller_hints::cover_instruction().unwrap_or_else(|| "Bind a controller Start control to move or toggle the cover during play.".into())
                    } else {
                        "In play, Page Up and Page Down move it and Enter hides or shows it.".into()
                    }
                ),
            ),
        ];
        if cover.mode.sudden() {
            rows.push(cover_height("cover_sudden", "Sudden+", cover.sudden));
        }
        if cover.mode.hidden() {
            rows.push(cover_height("cover_hidden", "Hidden+", cover.hidden));
        }
        rows.extend([
            cut,
            choice(
                "simplify_onigiri",
                "Without onigiri",
                &["Off", "On"],
                s.simplify_onigiri as usize,
                "Adds separate DanOni charts to Single, Solo, Double and Techno 8-panel, \
                     without the onigiri notes. Only these versions are marked as assisted; \
                     the original charts stay available.",
            ),
            choice(
                "simplify_center",
                "Without center",
                &["Off", "Where missing", "All"],
                pos(&CENTER_VERSIONS, s.simplify_center),
                "Adds Pump charts to Single and Double without the center notes, each pad \
                     turned an eighth clockwise, marked as assisted. Where missing: only at \
                     difficulties the song has no chart of there.",
            ),
            choice(
                "at_once",
                "At once",
                &["Any", "Up to two", "One"],
                pos(&AT_ONCE, t.at_once),
                "The most panels pressed at once, holds included: Up to two keeps jumps but \
                     leaves out arrows meant for a hand or a keyboard, One leaves out jumps too. \
                     The leftmost arrows go first.",
            ),
            choice(
                "no_holds",
                "No holds",
                &["Off", "On"],
                t.no_holds as usize,
                "Holds and rolls become single taps.",
            ),
        ]);
        let mut pump_single = choice(
            "convert_pump_single",
            "Pump on 5 keys",
            &["Off", "On"],
            s.convert_pump_single as usize,
            "Adds Pump single charts to 5 keys: the center on the onigiri, the corners turned \
             an eighth clockwise. Marked as converted, played by the original's rules.",
        );
        pump_single.heading = Some("CONVERT".into());
        rows.extend([
            pump_single,
            choice(
                "convert_five_keys",
                "5 keys on Pump",
                &["Off", "On"],
                s.convert_five_keys as usize,
                "Adds DanOni 5-key charts to Pump single: the onigiri on the center, the \
                 arrows turned onto the corners. Marked as converted, played by the original's \
                 rules.",
            ),
        ]);
        section("Notes", rows)
    }

    fn rules(&self) -> OptionSection {
        let s = &self.settings;
        let presets = ddi_engine::rules::presets::all();
        let names: Vec<String> = presets
            .iter()
            .map(|r| {
                if r.approximate {
                    format!("{} (approximate)", r.name)
                } else {
                    r.name.clone()
                }
            })
            .collect();
        let at = presets.iter().position(|r| r.id == s.ruleset).unwrap_or(0);
        let gauge_at = DANONI_GAUGES
            .iter()
            .position(|(g, _)| *g == s.danoni.gauge.as_deref())
            .unwrap_or(0);
        let mut gauge = choice(
            "danoni.gauge",
            "Gauge",
            &DANONI_GAUGES.map(|(_, n)| n),
            gauge_at,
            "Used for the charts played by Dancing☆Onigiri rules. A chart may offer its own \
             gauges; one it does not offer plays its first.",
        );
        gauge.heading = Some("DANCING☆ONIGIRI RULES".into());
        section(
            "Rules",
            vec![
                choice(
                    "ruleset_mode",
                    "Rules for each chart",
                    &["The ruleset below", "The chart's game", "Dancing☆Onigiri"],
                    pos(&RULESET_MODES, s.ruleset_mode),
                    "The ruleset below for every chart, the rules of the game each chart was made \
                     for (Dancing☆Onigiri works and FFR levels by theirs, the rest by the ruleset \
                     below), or Dancing☆Onigiri rules for every chart.",
                ),
                row(
                    "ruleset",
                    "Ruleset",
                    OptionKind::Choices(names),
                    at,
                    "How timing is judged and scored, after the game that set the rules.",
                ),
                choice(
                    "holds",
                    "Holds",
                    &["By game", "Dance", "Ticked"],
                    pos(&HOLD_STYLES, s.holds),
                    "Ticked holds count at steady ticks while held, each adding to the combo and \
                     the score; dance-style holds give one result at their end. By game: ticked \
                     on pump charts. Dancing☆Onigiri charts keep their own.",
                ),
                choice(
                    "fail",
                    "Fail",
                    &["Immediate", "Delayed", "Fail at end", "Off"],
                    pos(&FailType::ALL, s.stepmania.fail),
                    "When an empty life bar fails the play: at once, ending the song; at once \
                     but playing on to the end; only if it is still empty at the end; or never. \
                     The results say when the bar emptied. FFR's rules always fail at once.",
                ),
                row(
                    "life",
                    "Life difficulty",
                    OptionKind::Stepper(levels(LIFE_SCALES.len())),
                    level_index(s.stepmania.life),
                    "Higher drains the life bar faster and refills it slower (StepMania's levels; \
                     4 is the ruleset's own). DDR A's gauge and FFR's life keep their own.",
                ),
                row(
                    "judge",
                    "Judge difficulty",
                    OptionKind::Stepper(
                        (1..=JUDGE_SCALES.len() as u8)
                            .map(StepManiaOptions::judge_name)
                            .collect(),
                    ),
                    level_index(s.stepmania.judge),
                    "Higher narrows every timing window (StepMania's levels; 4 is the ruleset's \
                     own, Justice the narrowest). Bests are kept apart for each level. FFR's \
                     rules keep their own windows.",
                ),
                choice(
                    "life_type",
                    "Life",
                    &["Bar", "Battery"],
                    pos(&LifeType::ALL, s.stepmania.life_type),
                    "The ruleset's life bar, or a battery of lives: one lost for each step worse \
                     than a Great, each mine stepped on and each hold let go. Not under FFR's \
                     rules.",
                ),
                row(
                    "lives",
                    "Battery lives",
                    OptionKind::Stepper(levels(usize::from(StepManiaOptions::MAX_LIVES))),
                    level_index(s.stepmania.lives),
                    "How many lives the battery starts with.",
                ),
                row(
                    "ddr_gauge",
                    "DDR gauge",
                    OptionKind::Choices(DdrGauge::ALL.iter().map(|g| g.name()).collect()),
                    DdrGauge::ALL
                        .iter()
                        .position(|g| *g == s.stepmania.gauge.sanitized())
                        .unwrap_or(0),
                    "DDR A's gauge: LIFE4 has four lives, RISKY one. FLARE never refills and \
                     keeps its level as a rank; FLOATING FLARE drops a level when one empties.",
                ),
                gauge,
                choice(
                    "danoni.range",
                    "Judge range",
                    &JudgeRange::ALL.map(|r| r.name()),
                    JudgeRange::ALL
                        .iter()
                        .position(|r| *r == s.danoni.range)
                        .unwrap_or(0),
                    "How wide the judgement windows are under Dancing☆Onigiri rules.",
                ),
                choice(
                    "danoni.excessive",
                    "Excessive",
                    &["As the chart says", "Off", "On"],
                    match s.danoni.excessive {
                        None => 0,
                        Some(false) => 1,
                        Some(true) => 2,
                    },
                    "Early stray presses cost life.",
                ),
            ],
        )
    }

    fn timing(&self) -> OptionSection {
        let s = &self.settings;
        let mut rows = vec![
            actions(
                CALIBRATE,
                "Calibration",
                &["Calibrate"],
                "",
                "Measures your audio and display offsets by playing along.",
            ),
            offset(
                "audio_offset",
                "Default audio offset",
                s.audio_offset,
                "",
                "For audio without a profile. Positive: you hear the music later than the clock \
                 says (typical for Bluetooth).",
            ),
            offset(
                "visual_offset",
                "Default visual offset",
                s.visual_offset,
                "",
                "For displays without a profile. Positive: arrows are drawn as if the song were \
                 slightly ahead.",
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
                    &format!("{}: {}", super::capitalized(kind), p.label),
                    p.offset,
                    if p.calibrated {
                        "calibrated"
                    } else {
                        "default"
                    },
                    "Offsets are kept per audio path and per display; the game picks the matching \
                     one when a song starts.",
                );
                if first {
                    r.heading = Some("KNOWN DEVICES".into());
                    first = false;
                }
                rows.push(r);
                rows.push(actions(
                    &format!("forget.{kind}.{i}"),
                    "",
                    &["Forget"],
                    "",
                    "Forgets this device: the game asks to calibrate when it meets it again.",
                ));
            }
        }
        section("Timing", rows)
    }

    fn sound(&self) -> OptionSection {
        let s = &self.settings;
        let percent = (volume_to_slider(s.volume) * 100.0).round() as usize;
        let db = if s.volume > 0.0 {
            format!("{:.0} dB", 20.0 * s.volume.log10())
        } else {
            "muted".into()
        };
        let mut volume = row(
            "volume",
            "Music volume",
            OptionKind::Slider((0..=100).map(|v| format!("{v}%")).collect()),
            percent.min(100),
            "How loud the music plays (equal steps are about equal in loudness).",
        );
        volume.note = db;
        let mut v = vec![volume];
        let label = match self.world.sample {
            Sample::None => None,
            Sample::Ready => Some("Play a sample"),
            Sample::Playing => Some("Stop"),
        };
        if let Some(l) = label {
            v.push(actions(
                "sample",
                "Sample",
                &[l],
                "",
                "Plays a song's preview at this volume.",
            ));
        }
        v.push(choice(
            "assist_clap",
            "Assist clap",
            &["Off", "On"],
            s.assist_clap as usize,
            "A clap on every row of notes, right on the beat: to hear where the steps fall.",
        ));
        v.push(choice(
            "mine_sound",
            "Mine sound",
            &["Off", "On"],
            s.mine_sound as usize,
            "A sound when you step on a mine or a shock arrow.",
        ));
        section("Sound", v)
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
            "Autoplay demo",
            &["Play", "On WebGL2"],
            "",
            "Diagnostics: a bundled song played by itself (WebGL2 forces the fallback \
             renderer).",
        );
        demo.heading = Some("DIAGNOSTICS".into());
        let mut brightness = row(
            "bg_brightness",
            "Background brightness",
            OptionKind::Stepper(
                (0..=20)
                    .map(|v| {
                        if v == 0 {
                            "Off".into()
                        } else {
                            format!("{}%", v * 5)
                        }
                    })
                    .collect(),
            ),
            bg.min(20),
            "How bright songs' backgrounds are behind the field; off shows none.",
        );
        brightness.heading = Some("BACKGROUNDS".into());
        let mut menus = choice(
            "reduced_motion",
            "Reduced motion",
            &["As the system", "On", "Off"],
            pos(&REDUCED_MOTION, s.reduced_motion),
            "No beat pulse and no slides in the menus.",
        );
        menus.heading = Some("MENUS".into());
        section(
            "Display",
            vec![
                choice(
                    "show_deltas",
                    "Timing error of each hit",
                    &["Off", "On"],
                    s.show_deltas as usize,
                    "Shows how many milliseconds early or late each hit was.",
                ),
                choice(
                    "ghost",
                    "Ghost of your best",
                    &["Off", "On"],
                    s.ghost as usize,
                    "Shows, under the score, how far ahead of your best play of the chart you are, \
                     or behind it.",
                ),
                choice(
                    "receptor_snap",
                    "Receptor snap",
                    &["Off", "On"],
                    s.receptor_snap as usize,
                    "Draws a note exactly on the receptor in its closest frame.",
                ),
                choice(
                    "lyrics",
                    "Lyrics",
                    &["Off", "On"],
                    s.lyrics as usize,
                    "Shows the lyrics of songs that have them (Dancing☆Onigiri works, songs with a \
                     lyrics file).",
                ),
                brightness,
                choice(
                    "video",
                    "Background videos",
                    &["Automatic", "On", "Off"],
                    pos(&VIDEOS, s.video),
                    "Videos play behind the field in time with the music; automatic turns one off \
                     for the rest of a song when it costs frames. Off never loads the decoder.",
                ),
                choice(
                    "background_frame",
                    "Background frame",
                    &["Automatic", "4:3", "16:9", "Whole screen"],
                    pos(&FRAMES, s.background_frame),
                    "Backgrounds and videos fill this shape, as on the screen they were made for \
                     (automatic: the song's video, else its background); the rest shows a blur \
                     of them.",
                ),
                choice(
                    "field_filter",
                    "Field filter",
                    &["Off", "40%", "60%", "80%"],
                    filter,
                    "Dims the song's background behind the lanes.",
                ),
                menus,
                choice(
                    "start_countdown",
                    "Start countdown",
                    &["Off", "3 s", "5 s", "10 s"],
                    START_COUNTDOWNS
                        .iter()
                        .position(|c| (c - s.start_countdown).abs() < 0.01)
                        .unwrap_or(0),
                    "A song chosen in song select starts by itself after this long. Enter (or \
                     Start) starts it at once, any other key or a tap waits.",
                ),
                choice(
                    "graphics",
                    "Graphics",
                    &["Automatic", "WebGL2"],
                    pos(&GRAPHICS, s.graphics),
                    "Automatic: WebGPU where it works, else WebGL2. Takes effect when the page is \
                     loaded again.",
                ),
                choice(
                    "debug",
                    "Debug overlay",
                    &["Off", "On"],
                    s.debug as usize,
                    "Shows the backend, frames a second, clock drift and timing error in play.",
                ),
                demo,
            ],
        )
    }

    fn arrows(&self) -> OptionSection {
        let s = &self.settings;
        let mut names = vec![if self.world.default_lua_off {
            "Infinito (its Lua is off)".to_string()
        } else {
            "Infinito".to_string()
        }];
        let dance = self.dance_skins();
        names.extend(dance.iter().map(|(_, n)| n.clone()));
        let at = match s.noteskin.as_deref() {
            None => 0,
            Some(k) => dance
                .iter()
                .position(|(key, _)| key == k)
                .map_or(0, |i| i + 1),
        };
        let mut rows = vec![row(
            "noteskin",
            "Arrows",
            OptionKind::Choices(names),
            at,
            if self.world.platform == super::data::Platform::Desktop {
                "Infinito: the game's own arrows, its options below. Or a bundled noteskin, or one \
                 from the song folders."
            } else {
                "Infinito: the game's own arrows, its options below. Or a bundled or an imported \
                 noteskin."
            },
        )];
        // The chosen skin's parameters.
        let key = s.noteskin.as_deref().unwrap_or(INFINITO);
        rows.extend(self.param_rows(SKIN_ROW, key));
        let desktop = self.world.platform == super::data::Platform::Desktop;
        // Desktop reads skins in place (bundled ones, the song folders'):
        // none is stored to remove.
        if let Some(k) = s.noteskin.as_deref()
            && k != BUILT_IN
            && !k.starts_with(BUNDLED)
            && !desktop
        {
            rows.push(actions(
                "noteskin.remove",
                "This noteskin",
                &["Remove"],
                "",
                "Removes the imported noteskin from this browser.",
            ));
        }
        // Pump skins (StepMania's `NoteSkins/pump/*`) draw the pump
        // layouts when one is chosen.
        let pump = self.pump_skins();
        if !pump.is_empty() {
            let mut names = vec!["As above".to_string()];
            names.extend(pump.iter().map(|(_, n)| n.clone()));
            let at = s
                .pump_noteskin
                .as_deref()
                .and_then(|k| pump.iter().position(|(key, _)| key == k))
                .map_or(0, |i| i + 1);
            rows.push(row(
                "pump_noteskin",
                "Pump arrows",
                OptionKind::Choices(names),
                at,
                "The noteskin the pump styles are drawn with: as above, or a pump noteskin \
                 (its own art for each panel).",
            ));
            if s.pump_noteskin
                .as_deref()
                .is_some_and(|k| !k.starts_with(BUNDLED))
                && !desktop
            {
                rows.push(actions(
                    "pump_noteskin.remove",
                    "This pump noteskin",
                    &["Remove"],
                    "",
                    "Removes the imported pump noteskin from this browser.",
                ));
            }
            if let Some(k) = s.pump_noteskin.as_deref() {
                rows.extend(self.param_rows(PUMP_SKIN_ROW, k));
            }
        }
        section("Arrows", rows)
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

    /// Skin `key`'s parameters as rows, their ids `prefix` and the name.
    fn param_rows(&self, prefix: &str, key: &str) -> Vec<OptionDef> {
        let stored = self.settings.skin_params(key);
        self.declared(key)
            .iter()
            .map(|p| {
                let (labels, _, at) = param_values(p, stored.get(&p.name));
                row(
                    &format!("{prefix}{}", p.name),
                    &p.label,
                    match p.kind {
                        ParamKind::Number { .. } => OptionKind::Stepper(labels),
                        _ => OptionKind::Choices(labels),
                    },
                    at,
                    &p.description,
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

    fn preview_layout_option(&self) -> OptionDef {
        let layouts = self.bind_layouts();
        let shown = self
            .preview_layout
            .as_deref()
            .unwrap_or(&self.settings.style);
        row(
            "preview_layout",
            "Preview on",
            OptionKind::Choices(layouts.iter().map(|l| l.name.clone()).collect()),
            layouts.iter().position(|l| l.id == shown).unwrap_or(0),
            "The style the preview shows the arrows on (the style chosen until you pick \
             another; not kept).",
        )
    }

    fn controls(&self) -> OptionSection {
        let s = &self.settings;
        let layout = &self.bind_layout;
        let layouts = self.bind_layouts();
        let at = layouts.iter().position(|l| l.id == layout.id).unwrap_or(0);
        let mut rows = vec![
            row(
                "bind_layout",
                "Bindings for",
                OptionKind::Choices(layouts.iter().map(|l| l.name.clone()).collect()),
                at,
                &match layout.one_pad_layout() {
                    Some(one) => format!(
                        "Each device keeps its own bindings for each style. With two controllers, \
                         each plays one pad of {} with its {} bindings (the one connected first \
                         on the left), unless it has {} bindings of its own.",
                        layout.name, one.name, layout.name
                    ),
                    None => "Each device keeps its own bindings for each style.".into(),
                },
            ),
            actions(
                "bind.any",
                "Any device",
                &["Bind…"],
                "",
                "Press a key or a controller's button, then each lane in turn. Controllers show \
                 up once one of their buttons has been pressed.",
            ),
        ];
        let mut keyboard = actions(
            "keys",
            "Keyboard",
            &["Bind", "Reset keys"],
            if s.controls.keyboard_is_default(layout) {
                "defaults"
            } else {
                "saved"
            },
            "Bind asks for each lane in turn; Reset keys puts this style's defaults back.",
        );
        keyboard.heading = Some("KEYBOARD".into());
        rows.push(keyboard);
        let keys = s.controls.keyboard(layout);
        for (lane, l) in layout.lanes.iter().enumerate() {
            let k = keys.get(lane).cloned().unwrap_or_default();
            rows.push(actions(
                &format!("keys.lane.{lane}"),
                l.label(),
                &["Add key", "Clear"],
                &if k.is_empty() {
                    "—".to_string()
                } else {
                    k.join(", ")
                },
                "Add key takes the next key you press for this lane (Escape cancels); a key \
                 plays one lane only.",
            ));
        }
        for (j, (id, standard, count)) in self.pads().into_iter().enumerate() {
            rows.extend(self.pad_rows(j, &id, standard, count));
        }
        let mut poll = choice(
            "fast_pad_poll",
            "Poll every millisecond",
            &["Off", "On"],
            s.fast_pad_poll as usize,
            "Polls controllers every millisecond during play: smoother timing, more CPU.",
        );
        poll.heading = Some("CONTROLLERS".into());
        rows.push(poll);
        if !self.world.pads_supported {
            rows.push(actions(
                "pads.none",
                "Controllers",
                &[],
                "not supported here",
                if self.world.platform == super::data::Platform::Desktop {
                    "Controllers are not read here."
                } else {
                    "This browser does not read controllers."
                },
            ));
        }
        section("Controls", rows)
    }

    /// A controller: what it plays, its bindings, and what can be done
    /// with them.
    fn pad_rows(&self, j: usize, id: &str, standard: bool, count: usize) -> Vec<OptionDef> {
        let layout = &self.bind_layout;
        let saved = self.settings.controls.pads.iter().find(|p| p.id == id);
        let has_table = saved.is_some_and(|p| p.table(&layout.id).is_some());
        let bindings = self.settings.pad_bindings(id, standard);
        let mut pads = self.world.connected.clone();
        if count == 0 {
            pads.push(ConnectedPad {
                vendor_product: None,
                id: id.to_string(),
                index: u32::MAX,
                standard,
            });
        }
        let pad_use = self
            .settings
            .controls
            .plan(layout, &pads)
            .into_iter()
            .find(|(c, _)| c.id == id)
            .map_or(PadUse::Unused, |(_, u)| u);
        let one_name = layout.one_pad_layout().map(|l| l.name).unwrap_or_default();
        let status = match &pad_use {
            PadUse::OnePad { .. } if count > 1 => format!("one pad each, {one_name} bindings"),
            PadUse::OnePad { .. } => format!("one pad, {one_name} bindings"),
            u if !u.plays() && has_table => "nothing bound".into(),
            PadUse::Table { defaults: true, .. } if pad_use.plays() => "defaults".into(),
            PadUse::Table { .. } if pad_use.plays() => "saved".into(),
            _ => "not bound yet".into(),
        };
        let presence = match count {
            0 => "not connected".to_string(),
            1 => "connected".to_string(),
            n => format!("{n} connected"),
        };
        let is_one_pad = Layout::builtins()
            .iter()
            .any(|l| l.one_pad_layout().is_some_and(|one| one.id == layout.id));
        let mut buttons = vec!["Bind"];
        if pad_use.plays() || has_table {
            buttons.push("Clear");
        }
        if has_table && (!is_one_pad || standard) {
            buttons.push("Use defaults");
        }
        if saved.is_some() || self.settings.controller_styles.contains_key(id) {
            buttons.push("Forget");
        }
        let mut head = actions(
            &format!("pad.{j}"),
            "",
            &buttons,
            &format!("{status} · {presence}"),
            "Bind asks for each lane, then Start and Back; Clear unbinds this style; Use \
             defaults puts the standard ones back; Forget drops everything saved for it.",
        );
        head.heading = Some(id.to_uppercase());
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
        let auto = detected.map_or("Auto".to_string(), |(f, _)| format!("Auto ({})", f.label()));
        rows.push(choice(&format!("pad_style.{j}"),"Button hints",&[&auto,"Xbox","PlayStation","Nintendo","Generic"],crate::controller_hints::Style::ALL.iter().position(|s|*s==style).unwrap_or(0),"Changes artwork only. Unmapped controls retain numbered button/axis labels. Identical controller models share this preference."));
        let device = pads
            .iter()
            .find(|p| p.id == id)
            .expect("connected or saved placeholder");
        let family = crate::controller_hints::presentation(device, style);
        let label = |c: &String| crate::controller_hints::Cap::control(device, family, c).label();
        let list = |controls: &[String]| {
            if controls.is_empty() {
                "—".to_string()
            } else {
                controls.iter().map(label).collect::<Vec<_>>().join(", ")
            }
        };
        let lanes: Vec<String> = match (&pad_use, layout.one_pad_layout()) {
            (PadUse::OnePad { pad, .. }, Some(one)) => layout
                .pad_lanes_in(*pad, &one)
                .into_iter()
                .zip(layout.lanes_of_pad(*pad))
                .map(|(l, own)| match l {
                    Some(l) => one.lanes[l].label().to_string(),
                    None => layout.lanes[own].label().to_string(),
                })
                .collect(),
            _ => layout.lanes.iter().map(|l| l.label().to_string()).collect(),
        };
        let own = !matches!(pad_use, PadUse::OnePad { .. });
        let table = match pad_use {
            PadUse::Table { table, .. } | PadUse::OnePad { single: table, .. } => table,
            PadUse::Unused => Vec::new(),
        };
        let b = bindings.unwrap_or_default();
        // A lane's own controls, as the keyboard's keys: one more (from
        // the controller, so only while it is connected) or none. A
        // controller playing one pad of the layout plays it with its
        // bindings of the one-pad layout: changed there.
        let lane_buttons: &[&str] = match (own, count > 0) {
            (false, _) => &[],
            (true, true) => &["Add button", "Clear"],
            (true, false) => &["Clear"],
        };
        for (lane, name) in lanes.iter().enumerate() {
            rows.push(actions(
                &format!("pad.{j}.lane.{lane}"),
                name,
                lane_buttons,
                &list(table.get(lane).map(Vec::as_slice).unwrap_or_default()),
                if own {
                    "Add button takes the next button, stick or hat direction you press on this \
                     controller for this lane (Escape, its Back or holding a button a second \
                     cancels); a control plays one lane only. Clear empties the lane."
                } else {
                    "What plays this lane; its bindings for the one-pad style change it."
                },
            ));
        }
        for (what, c) in [("Start", &b.start), ("Back", &b.back)] {
            rows.push(actions(
                &format!("pad.{j}.{}", what.to_lowercase()),
                what,
                &[],
                &c.as_ref().map(label).unwrap_or_else(|| "—".into()),
                "Start and Back are shared by every style.",
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
                    name,
                    &["Remove"],
                    f,
                    "Songs read in place from this folder (StepMania's Songs folder, its \
                     folder, a pack, or a zip). Removing it only takes it off the list: no file \
                     is touched.",
                )
            })
            .collect();
        rows.push(actions(
            "folders.add",
            "Song folders",
            &["Add…", "Read again"],
            &self.world.library_note,
            "Add a folder of songs (StepMania's, ITGmania's or OutFox's, a Songs folder, a pack) \
             or a pack's zip; nothing is copied. Read again reads every folder whole.",
        ));
        Some(section("Songs", rows))
    }

    /// The play history and the resets ("Data").
    fn data_section(&self) -> OptionSection {
        let w = &self.world;
        let nothing = w.imported == 0 && w.shared == 0 && !self.has_imported_skins();
        let songs = match &w.folders {
            // Desktop: the song folders are forgotten, never deleted.
            Some(folders) => actions(
                "reset.songs",
                "Song folders",
                if folders.is_empty() { &[] } else { &["Forget"] },
                &if folders.is_empty() {
                    "none".to_string()
                } else {
                    count(folders.len(), "folder", "folders")
                },
                "Stops reading every song folder and forgets the songs taken off the list and \
                 their offsets; nothing in the folders is deleted, settings are kept.",
            ),
            None => actions(
                "reset.songs",
                "Imported songs",
                if nothing || w.importing {
                    &[]
                } else {
                    &["Remove"]
                },
                &if w.importing {
                    "an import is running".to_string()
                } else if nothing {
                    "none".to_string()
                } else {
                    count(w.imported, "song", "songs")
                },
                "Deletes the imported songs, their offsets, the imported noteskins and the \
                     shared background images; settings are kept.",
            ),
        };
        let plays = self.world.plays;
        section(
            "Data",
            vec![
                actions(
                    "history.export",
                    "Play history",
                    history_buttons(plays, w.importing_plays),
                    &if w.importing_plays {
                        "importing plays…".to_string()
                    } else {
                        match plays {
                            0 => "no plays kept yet".to_string(),
                            1 => "1 play kept".to_string(),
                            n => format!("{n} plays kept"),
                        }
                    },
                    if self.world.platform == super::data::Platform::Desktop {
                        "Every song played to the end or failed is kept, with every press, so \
                         its score can be worked out again. Export saves them as JSON lines to a \
                         file you choose; Import adds a file's plays; Reset deletes them."
                    } else {
                        "Every song played to the end or failed is kept, with every press, so \
                         its score can be worked out again. Export downloads them as JSON lines; \
                         Import (or a drop here) adds a file's plays; Reset deletes them."
                    },
                ),
                actions(
                    "reset.settings",
                    "Settings",
                    &["Reset"],
                    "",
                    if w.folders.is_some() {
                        "Every setting back to its default; the song folders and per-song offsets \
                         are kept."
                    } else {
                        "Every setting back to its default; imported songs and per-song offsets \
                         are kept."
                    },
                ),
                songs,
                actions(
                    "reset.everything",
                    "Everything",
                    if w.importing { &[] } else { &["Reset"] },
                    if w.importing {
                        "an import is running"
                    } else {
                        ""
                    },
                    if w.folders.is_some() {
                        "The settings, the song folders and the play history above, and the \
                         offsets of the bundled songs: like a first start (nothing in the folders \
                         is deleted)."
                    } else {
                        "The settings, the songs and the play history above, and the offsets of \
                         the bundled songs: like a first visit."
                    },
                ),
            ],
        )
    }

    fn has_imported_skins(&self) -> bool {
        self.world
            .noteskins
            .iter()
            .any(|(k, _)| !k.starts_with(BUNDLED))
    }

    /// Applies a row's new value and saves; whether anything changed.
    pub fn change(&mut self, id: &str, i: usize) -> bool {
        let before = self.settings.clone();
        let layout_before = self.bind_layout.id.clone();
        let style_pad = id
            .strip_prefix("pad_style.")
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| self.pads().into_iter().nth(n))
            .map(|p| p.0);
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
            "ruleset_mode" => s.ruleset_mode = at(&RULESET_MODES, i),
            "ruleset" => {
                if let Some(r) = ddi_engine::rules::presets::all().get(i) {
                    s.ruleset = r.id.clone();
                }
            }
            "holds" => s.holds = at(&HOLD_STYLES, i),
            "fail" => s.stepmania.fail = at(&FailType::ALL, i),
            "life" => s.stepmania.life = level_at(i, LIFE_SCALES.len()),
            "judge" => s.stepmania.judge = level_at(i, JUDGE_SCALES.len()),
            "life_type" => s.stepmania.life_type = at(&LifeType::ALL, i),
            "lives" => s.stepmania.lives = level_at(i, usize::from(StepManiaOptions::MAX_LIVES)),
            "ddr_gauge" => s.stepmania.gauge = DdrGauge::ALL.get(i).copied().unwrap_or_default(),
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
            "show_deltas" => s.show_deltas = on,
            "ghost" => s.ghost = on,
            "receptor_snap" => s.receptor_snap = on,
            "lyrics" => s.lyrics = on,
            "bg_brightness" => s.bg_brightness = i as f32 / 20.0,
            "video" => s.video = at(&VIDEOS, i),
            "background_frame" => s.background_frame = at(&FRAMES, i),
            "field_filter" => s.field_filter = FIELD_FILTERS.get(i).copied().unwrap_or(0.0),
            "reduced_motion" => s.reduced_motion = at(&REDUCED_MOTION, i),
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
        changed
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
            ("bind.any", _) => Act::Bind(None),
            ("keys", 0) => Act::Bind(Some(DeviceId::Keyboard)),
            ("keys", _) => {
                self.settings.controls.reset_keyboard(&layout);
                self.notice = format!(
                    "The keyboard's {} keys are the defaults again.",
                    layout.name
                );
                Act::Done
            }
            ("history.export", b) => {
                let buttons = history_buttons(self.world.plays, self.world.importing_plays);
                match buttons.get(b) {
                    Some(&"Export") => Act::Export,
                    Some(&"Import…") => Act::ImportPlays,
                    Some(&"Reset") => Act::Confirm(Reset::History),
                    _ => Act::Nothing,
                }
            }
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
        let Some((id, standard, _)) = self.pads().into_iter().nth(j) else {
            return Act::Nothing;
        };
        let data = self.data();
        let label = data
            .sections
            .iter()
            .flat_map(|s| &s.options)
            .find(|o| o.id == format!("pad.{j}"))
            .and_then(|o| o.values().get(button).cloned())
            .unwrap_or_default();
        let layout = self.bind_layout.clone();
        match label.as_str() {
            "Bind" => Act::Bind(Some(DeviceId::Gamepad(id))),
            "Clear" => {
                let mut pad = self.pad_entry(&id, standard);
                pad.set_table(&layout.id, vec![Vec::new(); layout.lane_count()]);
                self.settings.set_pad_bindings(pad);
                Act::Done
            }
            "Use defaults" => {
                self.settings
                    .controls
                    .reset_pad_table(&id, &layout.id, standard);
                Act::Done
            }
            "Forget" => {
                self.settings.controls.forget_pad(&id);
                self.settings.controller_styles.remove(&id);
                self.notice = format!("{id} forgotten.");
                Act::Done
            }
            _ => Act::Nothing,
        }
    }

    /// A lane row's button of the `j`th controller listed: Add button or
    /// Clear.
    fn pad_lane_action(&mut self, j: usize, lane: usize, button: usize) -> Act {
        let Some((id, standard, _)) = self.pads().into_iter().nth(j) else {
            return Act::Nothing;
        };
        let label = self
            .data()
            .sections
            .iter()
            .flat_map(|s| &s.options)
            .find(|o| o.id == format!("pad.{j}.lane.{lane}"))
            .and_then(|o| o.values().get(button).cloned())
            .unwrap_or_default();
        match label.as_str() {
            "Add button" => Act::AddButton { id, lane },
            "Clear" => {
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
            _ => Act::Nothing,
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
        let name = |i: usize| layout.lanes.get(i).map_or("", |l| l.label()).to_string();
        let shown = control
            .parse::<Control>()
            .map(|c| c.label())
            .unwrap_or_else(|_| control.to_string());
        self.notice = match from {
            Some(f) => format!("{shown} moved from {} to {}.", name(f), name(lane)),
            None => format!("{shown} added to {}.", name(lane)),
        };
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
        match bound {
            Bound::Keyboard(keys) => {
                self.settings.controls.set_keyboard(&layout, keys);
                self.notice = format!("The keyboard is bound for {}.", layout.name);
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
                self.notice = format!("{id} is bound for {}.", layout.name);
            }
        }
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
        let name = |i: usize| layout.lanes.get(i).map_or("", |l| l.label()).to_string();
        self.notice = match from {
            Some(f) => format!("{code} moved from {} to {}.", name(f), name(lane)),
            None => format!("{code} added to {}.", name(lane)),
        };
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
                    .is_some_and(|k| k != BUILT_IN && !k.starts_with(BUNDLED))
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
            }
            Reset::History => {}
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
            self.notice = "Settings reset.".into();
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
/// off the palette is offered after it.
fn param_values(
    p: &Parameter,
    stored: Option<&ParamValue>,
) -> (Vec<String>, Vec<ParamValue>, usize) {
    let current = stored
        .and_then(|v| p.accept(v))
        .unwrap_or_else(|| p.default.clone());
    let (labels, values): (Vec<String>, Vec<ParamValue>) = match &p.kind {
        ParamKind::Choice(choices) => choices
            .iter()
            .map(|c| (c.label.clone(), ParamValue::Text(c.id.clone())))
            .unzip(),
        ParamKind::Toggle => (
            vec!["Off".into(), "On".into()],
            vec![ParamValue::Bool(false), ParamValue::Bool(true)],
        ),
        ParamKind::Number { unit, .. } => p
            .numbers()
            .into_iter()
            .map(|n| {
                (
                    format!("{}{unit}", trim_number(n, 2)),
                    ParamValue::Number(n),
                )
            })
            .unzip(),
        ParamKind::Color(palette) => {
            let mut pairs: Vec<(String, ParamValue)> = palette
                .iter()
                .map(|c| (c.label.clone(), ParamValue::Text(c.id.clone())))
                .collect();
            if let ParamValue::Text(t) = &current
                && !pairs.iter().any(|(_, v)| v == &current)
            {
                pairs.push((t.clone(), current.clone()));
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

/// The pump skins of `skins`, or the others.
fn skins_of(skins: &[(String, String)], pump: bool) -> Vec<(String, String)> {
    skins
        .iter()
        .filter(|(k, _)| crate::state::settings::is_pump_skin(k) == pump)
        .cloned()
        .collect()
}

fn section(name: &str, options: Vec<OptionDef>) -> OptionSection {
    OptionSection {
        name: name.into(),
        options,
    }
}

fn row(id: &str, label: &str, kind: OptionKind, index: usize, description: &str) -> OptionDef {
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

fn choice(id: &str, label: &str, names: &[&str], index: usize, description: &str) -> OptionDef {
    row(
        id,
        label,
        OptionKind::Choices(names.iter().map(|s| s.to_string()).collect()),
        index,
        description,
    )
}

fn actions(id: &str, label: &str, buttons: &[&str], note: &str, description: &str) -> OptionDef {
    let mut r = row(
        id,
        label,
        OptionKind::Actions(buttons.iter().map(|s| s.to_string()).collect()),
        0,
        description,
    );
    r.note = note.into();
    r
}

/// An offset stepper, −500 to +500 ms by 1.
fn offset(id: &str, label: &str, seconds: f64, note: &str, description: &str) -> OptionDef {
    let ms = (seconds * 1000.0).round().clamp(-500.0, 500.0) as i32;
    let mut r = row(
        id,
        label,
        OptionKind::Stepper((-500..=500).map(|v| format!("{v:+} ms")).collect()),
        (ms + 500) as usize,
        description,
    );
    r.note = note.into();
    r
}

fn offset_at(i: usize) -> f64 {
    (i as f64 - 500.0).clamp(-500.0, 500.0) / 1000.0
}

fn pos<T: PartialEq>(table: &[T], v: T) -> usize {
    table.iter().position(|x| *x == v).unwrap_or(0)
}

fn at<T: Copy + Default>(table: &[T], i: usize) -> T {
    table.get(i).copied().unwrap_or_default()
}

/// Levels `1..=n` as a stepper shows them.
fn levels(n: usize) -> Vec<String> {
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

fn count(n: usize, one: &str, many: &str) -> String {
    match n {
        0 => format!("no {many}"),
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    }
}

const SPEED_SOURCES: [SpeedSource; 2] = [SpeedSource::Player, SpeedSource::Chart];
const SCROLL_ACTIONS: [ScrollAction; 4] = [
    ScrollAction::Normal,
    ScrollAction::Boost,
    ScrollAction::Brake,
    ScrollAction::Wave,
];
const TURNS: [Turn; 5] = [
    Turn::Off,
    Turn::Mirror,
    Turn::Left,
    Turn::Right,
    Turn::Shuffle,
];
const APPEARANCES: [Appearance; 5] = [
    Appearance::Visible,
    Appearance::Hidden,
    Appearance::Sudden,
    Appearance::HiddenSudden,
    Appearance::Stealth,
];
const CENTER_VERSIONS: [CenterVersions; 3] = [
    CenterVersions::Off,
    CenterVersions::Missing,
    CenterVersions::All,
];

const COVER_MODES: [CoverMode; 4] = [
    CoverMode::Off,
    CoverMode::Sudden,
    CoverMode::Hidden,
    CoverMode::HiddenSudden,
];

/// A cover's height row: its heights 0 to 100% by 1.
fn cover_height(id: &str, name: &str, height: u8) -> OptionDef {
    row(
        id,
        &format!("{name} height"),
        OptionKind::Stepper((0..=LaneCover::FULL).map(|v| format!("{v}%")).collect()),
        usize::from(height.min(LaneCover::FULL)),
        &format!("How much of the lane {name} covers. A play keeps the height you leave it at."),
    )
}

const CUTS: [TimingCut; 3] = [TimingCut::Off, TimingCut::Quarters, TimingCut::Eighths];
const AT_ONCE: [AtOnce; 3] = [AtOnce::Any, AtOnce::Two, AtOnce::One];
const RULESET_MODES: [RulesetMode; 3] = [
    RulesetMode::AllStepMania,
    RulesetMode::Original,
    RulesetMode::AllDanoni,
];
const HOLD_STYLES: [HoldStyle; 3] = [HoldStyle::ByGame, HoldStyle::Dance, HoldStyle::Ticked];
const DANONI_GAUGES: [(Option<&str>, &str); 10] = [
    (None, "The chart's first"),
    (Some("Original"), "Original"),
    (Some("Light"), "Light"),
    (Some("Heavy"), "Heavy"),
    (Some("NoRecovery"), "NoRecovery"),
    (Some("SuddenDeath"), "SuddenDeath"),
    (Some("Practice"), "Practice"),
    (Some("Normal"), "Normal (border)"),
    (Some("Easy"), "Easy (border)"),
    (Some("Hard"), "Hard (border)"),
];
const VIDEOS: [VideoMode; 3] = [VideoMode::Auto, VideoMode::On, VideoMode::Off];
const FRAMES: [BackgroundFrame; 4] = [
    BackgroundFrame::Auto,
    BackgroundFrame::FourThree,
    BackgroundFrame::SixteenNine,
    BackgroundFrame::Screen,
];
const REDUCED_MOTION: [ReducedMotion; 3] =
    [ReducedMotion::Auto, ReducedMotion::On, ReducedMotion::Off];
const GRAPHICS: [Graphics; 2] = [Graphics::Auto, Graphics::WebGl2];

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    use ddi_platform::storage::{self, Area, MemoryStore};

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

    fn find<'a>(d: &'a OptionsData, id: &str) -> &'a OptionDef {
        d.sections
            .iter()
            .flat_map(|s| &s.options)
            .find(|o| o.id == id)
            .unwrap_or_else(|| panic!("no {id}"))
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
        }
        // Saved as changed.
        m.change("reverse", 1);
        m.change("speed", 11);
        let stored: Settings = storage::load(Area::Local, crate::state::settings::KEY).unwrap();
        assert!(stored.reverse);
        assert_eq!(stored.speed, 3.0);
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
                m.settings.play_skin("dance-single").as_deref(),
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
            m.settings.play_skin("dance-single").as_deref(),
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
        assert!(m.change("pad_style.0", 2));
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
        assert!(m.change("preview_layout", solo));
        assert_eq!(m.preview_settings().style, "dance-solo");
        assert!(m.data().preview.contains(" · Solo · "));
        assert_eq!(m.settings.style, "dance-single");
    }
}
