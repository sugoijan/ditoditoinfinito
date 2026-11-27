//! Basic and Advanced in Options (`docs/plans/ui-levels.md`, stage 1):
//! the level of each row, in one table ([`LEVELS`]), and the one-time
//! decision for settings stored before the levels existed
//! ([`advanced_in_effect`]). Where each row shows, the Show advanced
//! options row first in Interface, is `super::options_tree`'s table.
//!
//! The level only hides: every setting keeps its effect at either level.
//! At Basic (Show advanced options Off), Options shows the Basic rows and
//! ends each section with a row counting its Advanced rows changed from
//! their defaults, with Show and Reset (`OptionsModel::data`); the start
//! prompt keeps an Advanced row that is changed (`super::prompt_options`).
//!
//! The levels follow the plan's rule as the maintainer leaned it on
//! 2026-11-26 ("erring more towards marking a simple option as advanced
//! than marking a complex option as basic"): a row is Basic only where a
//! person new to rhythm games understands it, would want it and cannot
//! quietly make their game worse with it; the table of why for each row
//! is in the plan's section 9. Change a row's level here and nowhere
//! else.
//!
//! Stage 2 brought six rows back to Basic under plain words (each
//! message's value; its `.advanced` the genre's words, Show advanced
//! options On): Fewer notes, Notes together and No long notes (Notes),
//! Clap on each note (Sound), Compare with your best and Darken behind
//! the arrows (Display); the plan's section 10.

use crate::state::settings::Level::{self, Advanced, Basic};
use crate::state::settings::{INFINITO, Settings};

/// Each row's level, by its id, or by its family's prefix (an entry
/// ending in `.`: a device's rows, a lane's, a skin's parameters). An id
/// not here shows at Advanced only (in doubt, Advanced; a test holds every
/// row the model makes to an entry).
///
/// The entries are grouped by the parts the model makes its rows in (the
/// sections before stage 3); where each shows is `super::options_tree`.
///
/// A skin's parameters (`skin.`) are Advanced unless the skin declares one
/// Basic (`Level=Basic` in its metrics, our extension; Infinito declares
/// its Note colors, since 2026-11-27); a pump skin's are Advanced as its
/// row is.
pub const LEVELS: &[(&str, Level)] = &[
    // Scroll.
    ("speed", Basic),
    ("speed_source", Advanced),
    ("reverse", Basic),
    ("scroll_action", Advanced),
    // Notes.
    ("turn", Advanced),
    ("appearance", Advanced),
    ("lane_cover", Advanced),
    ("cover_sudden", Advanced),
    ("cover_hidden", Advanced),
    ("cut", Basic),
    ("simplify_onigiri", Advanced),
    ("simplify_center", Advanced),
    ("at_once", Basic),
    ("no_stretch", Advanced),
    ("no_holds", Basic),
    ("convert_pump_single", Advanced),
    ("convert_five_keys", Advanced),
    // Rules.
    ("ruleset_mode", Advanced),
    ("ruleset", Advanced),
    ("ddi.difficulty", Basic),
    ("ddi.boos", Advanced),
    ("ddi.practice", Basic),
    ("ddi.score", Basic),
    ("ddi.words", Basic),
    ("holds", Advanced),
    ("fail", Advanced),
    ("life", Advanced),
    ("judge", Advanced),
    ("life_type", Advanced),
    ("lives", Advanced),
    ("ddr_gauge", Advanced),
    ("rejudge", Advanced),
    ("danoni.gauge", Advanced),
    ("danoni.range", Advanced),
    ("danoni.excessive", Advanced),
    // Timing. A device's offset is calibration's result (Basic), so it
    // has no default to count at Basic (`OptionsModel`'s defaults).
    ("calibrate", Basic),
    ("audio_offset", Advanced),
    ("visual_offset", Advanced),
    ("profile.", Advanced),
    ("forget.", Advanced),
    // Sound.
    ("volume", Basic),
    ("sample", Basic),
    ("assist_clap", Basic),
    ("mine_sound", Advanced),
    ("announcer", Basic),
    ("announcer.language", Basic),
    ("announcer.tag.", Advanced),
    ("announcer.param.", Advanced),
    ("announcer_volume", Basic),
    ("announcer.remove", Basic),
    ("announcer.studio", Basic),
    // Display.
    ("field_place", Advanced),
    ("show_deltas", Advanced),
    ("opponent_fields", Advanced),
    ("twitch_chat", Basic),
    ("chat_style", Advanced),
    ("chat_names", Advanced),
    ("chat_emotes_from", Basic),
    ("chat_opacity", Advanced),
    ("dancing_hand", Basic),
    ("dancing_hand_multiplayer", Advanced),
    ("hand_layout", Advanced),
    ("hand_frame_width", Advanced),
    ("hand_frame_height", Advanced),
    ("ghost", Basic),
    ("receptor_snap", Advanced),
    ("lyrics", Basic),
    ("bg_brightness", Basic),
    ("video", Advanced),
    ("random_movies", Advanced),
    ("generated_background", Advanced),
    ("generated_over_pictures", Advanced),
    ("generated_palette", Advanced),
    ("background_frame", Advanced),
    ("field_filter", Basic),
    (SHOW_ADVANCED, Basic),
    ("language", Basic),
    ("reduced_motion", Basic),
    ("cursor_style", Basic),
    ("cursor_size", Basic),
    ("start_countdown", Basic),
    ("chart_ratings", Advanced),
    ("graphics", Advanced),
    ("debug", Advanced),
    ("demo", Advanced),
    // Arrows.
    ("noteskin", Basic),
    ("skin.", Advanced),
    ("noteskin.remove", Basic),
    ("pump_noteskin", Advanced),
    ("pump_noteskin.remove", Advanced),
    ("pump_skin.", Advanced),
    ("preview_layout", Basic),
    // Controls.
    ("bind_layout", Basic),
    ("bind.any", Basic),
    ("keys", Basic),
    ("keys.lane.", Basic),
    ("touch_controls", Basic),
    ("touch_hand", Basic),
    ("touch_size", Basic),
    ("pad.", Basic),
    ("pad_style.", Basic),
    ("fast_pad_poll", Advanced),
    ("pads.none", Basic),
    // Data.
    ("history.export", Basic),
    ("reset.settings", Basic),
    ("reset.songs", Basic),
    ("room_ahead", Basic),
    ("twitch", Basic),
    ("privacy", Basic),
    ("usage_stats", Basic),
    ("error_reports", Basic),
    ("reset.everything", Basic),
    // Songs (desktop).
    ("folders.", Basic),
    // Sharing, behind its hidden switch: shown wherever the switch is on
    // until the hidden settings screen takes it (the plan's stage 4).
    ("share.", Basic),
    // Each section's count of what is changed at Advanced.
    (CHANGED, Basic),
];

/// The Show advanced options row's id (Off: Basic, On: Advanced).
pub const SHOW_ADVANCED: &str = "show_advanced";

/// The prefix of each section's "Advanced changes" row's id (then the
/// section's id).
pub const CHANGED: &str = "advanced.changed.";

/// Row `id`'s level as [`LEVELS`] gives it: its own entry, else its
/// family's (the longest prefix); `None` where it has neither.
pub fn level_of(id: &str) -> Option<Level> {
    LEVELS
        .iter()
        .find(|(k, _)| *k == id)
        .or_else(|| {
            LEVELS
                .iter()
                .filter(|(k, _)| k.ends_with('.') && id.starts_with(k))
                .max_by_key(|(k, _)| k.len())
        })
        .map(|(_, l)| *l)
}

/// Whether settings `s` have an Advanced setting changed from its default:
/// a row Options shows at Advanced only, or what such a row holds that the
/// shell's lists would show (a pump skin, a skin's or an announcer's
/// Advanced values). Settings stored before the levels existed start at
/// Advanced where this holds (the plan's open question 3), so that nobody
/// loses a row they had; device offsets are calibration's and do not
/// count.
pub fn advanced_in_effect(s: &Settings) -> bool {
    use super::options_model::{OptionsModel, World};
    let model = OptionsModel::new(
        Settings {
            level: Advanced,
            ..s.clone()
        },
        World::default(),
    );
    let rows = model.data_at(Advanced);
    let row_changed = rows
        .sections
        .iter()
        .flat_map(|s| &s.options)
        .any(|o| o.level == Advanced && o.changed());
    let infinito = ddi_noteskin::bundled::infinito_parameters();
    let skin_values = s.skin_params.iter().any(|(key, values)| {
        values.keys().any(|name| {
            key != INFINITO || !infinito.get(name).is_some_and(|p| p.basic)
        })
    });
    let announcer_values = s.announcer_params.values().any(|values| {
        values
            .keys()
            .any(|k| k != ddi_library::announcer::pool::LANGUAGE)
    });
    row_changed || s.pump_noteskin.is_some() || skin_values || announcer_values
}

#[cfg(test)]
pub(crate) mod tests {
    use std::rc::Rc;

    use ddi_platform::storage::{self, Area, MemoryStore};

    use super::*;
    use crate::screens::data::{OptionDef, OptionKind, OptionSection, OptionsData, Platform};
    use crate::screens::options_model::{Act, OptionsModel, Reset, World};
    use crate::state::settings::{OffsetProfile, ParamValue};

    fn rows(d: &OptionsData) -> impl Iterator<Item = &OptionDef> {
        d.sections.iter().flat_map(|s| &s.options)
    }

    fn section<'a>(d: &'a OptionsData, id: &str) -> Option<&'a OptionSection> {
        d.sections.iter().find(|s| s.id == id)
    }

    /// Options as full as the model makes them: every conditional row
    /// shown (devices, controllers, pump skins, an announcer's tags and
    /// parameters, sharing, song folders, the chat, the covers), under
    /// DDI and under StepMania's rules, on both shells.
    pub(crate) fn fullest() -> Vec<OptionsModel> {
        use ddi_library::announcer::Kept;
        let mut kept = Kept {
            name: "Mine".into(),
            ini: Some(
                "[Parameters]\nList=Mood\n[Parameter Mood]\nType=Choice\nLabel=Mood\nChoices=Normal,Calm\n"
                    .into(),
            ),
            script: Some(
                "local p = Announcer.pack() \
                 p:take('gameplay ready/a.ogg', { tags = { 'profanity' } }) \
                 p:tag('profanity', { label = 'Profanity', default = false }) return p"
                    .into(),
            ),
            ..Kept::default()
        };
        kept.lines
            .insert("gameplay ready".into(), vec!["gameplay ready/a.ogg".into()]);
        let profile = |id: &str| OffsetProfile {
            label: id.into(),
            id: id.into(),
            facts: None,
            offset: 0.01,
            calibrated: true,
            last_seen: 0.0,
        };
        let mut out = Vec::new();
        for platform in [Platform::Web, Platform::Desktop] {
            for ruleset in ["ddi", "itg"] {
                let world = World {
                    platform,
                    pads_supported: platform == Platform::Web,
                    sample: crate::screens::options_model::Sample::Ready,
                    plays: 2,
                    folders: (platform == Platform::Desktop).then(|| vec!["/Songs".into()]),
                    noteskins: vec![
                        ("dance/mine".into(), "Mine".into()),
                        ("pump/cmd".into(), "cmd".into()),
                    ],
                    connected: vec![ddi_engine::ConnectedPad {
                        vendor_product: None,
                        id: "Pad".into(),
                        index: 0,
                        standard: true,
                    }],
                    sharing: true,
                    ..World::default()
                };
                let settings = Settings {
                    level: Advanced,
                    ruleset: ruleset.into(),
                    noteskin: Some("dance/mine".into()),
                    pump_noteskin: Some("pump/cmd".into()),
                    lane_cover: ddi_engine::LaneCover {
                        mode: ddi_engine::CoverMode::HiddenSudden,
                        ..Default::default()
                    },
                    twitch_chat: ddi_chat::Level::Messages,
                    audio_profiles: vec![profile("audio:a")],
                    display_profiles: vec![profile("display:a")],
                    ..Settings::default()
                };
                let mut m = OptionsModel::new(settings, world);
                m.set_announcers_kept(&[kept.clone()], false);
                m.settings.announcer = Some("mine".into());
                out.push(m);
            }
        }
        out
    }

    /// Every row the model makes has a level in the table, its own or its
    /// family's (none falls to "in doubt, Advanced" unseen).
    #[test]
    fn every_row_has_a_level() {
        storage::install(Rc::new(MemoryStore::default()));
        for m in fullest() {
            let d = m.data_at(Advanced);
            let mut ids: Vec<&str> = rows(&d).map(|o| o.id.as_str()).collect();
            ids.extend(d.preview_layout.iter().map(|o| o.id.as_str()));
            for id in ["skin.Colors", "pump_skin.x", "announcer.tag.x", "announcer.param.x"] {
                ids.push(id);
            }
            for id in ids {
                assert!(level_of(id).is_some(), "{id} has no level");
            }
            // The table's families stay apart from the exact ids.
            assert_eq!(level_of("announcer"), Some(Basic));
            assert_eq!(level_of("announcer.tag.profanity"), Some(Advanced));
            assert_eq!(level_of("keys"), Some(Basic));
            assert_eq!(level_of("pad.0.lane.3"), Some(Basic));
            assert_eq!(level_of("nothing"), None);
        }
    }

    /// At Basic no Advanced row shows, and nothing is removed: the hidden
    /// setting keeps its effect (the play's options, the preview), its
    /// section says it is changed, Show brings it back at its place, and
    /// Reset (asked once) puts it back.
    #[test]
    fn basic_hides_never_removes() {
        storage::install(Rc::new(MemoryStore::default()));
        let mut m = OptionsModel::new(Settings::default(), World::default());
        assert_eq!(m.settings.level, Basic, "a new player's level");
        let d = m.data();
        assert_eq!(d.level, Basic);
        assert!(rows(&d).all(|o| o.level == Basic), "only Basic rows");
        assert!(rows(&d).all(|o| !o.id.starts_with(CHANGED)), "nothing changed yet");
        // Advanced's rows, changed there.
        m.settings.level = Advanced;
        m.change("turn", 1);
        m.change("lane_cover", 1);
        m.change("fast_pad_poll", 0);
        m.change(SHOW_ADVANCED, 0);
        assert_eq!(m.settings.level, Basic);
        let d = m.data();
        assert!(rows(&d).all(|o| o.id != "turn" && o.id != "fast_pad_poll"));
        let notes = section(&d, "Play").unwrap();
        let count = notes.options.last().unwrap();
        assert_eq!(count.id, format!("{CHANGED}Play"));
        assert_eq!(count.label, "Advanced changes: 2");
        assert_eq!(count.values(), ["Reset them", "Show"]);
        assert_eq!(count.index, 1, "Show lit first");
        let controls = section(&d, "Controls").unwrap();
        assert_eq!(controls.options.last().unwrap().label, "Advanced changes: 1");
        assert!(section(&d, "Screen").unwrap().options.iter().all(|o| !o.id.starts_with(CHANGED)));
        // Still in effect.
        assert_eq!(m.settings.transform.turn, ddi_engine::Turn::Mirror);
        assert_eq!(m.preview_settings().transform.turn, ddi_engine::Turn::Mirror);
        assert!(!m.settings.fast_pad_poll);
        let stored: Settings = storage::load(Area::Local, crate::state::settings::KEY).unwrap();
        assert_eq!(stored.transform.turn, ddi_engine::Turn::Mirror);
        // Show: Advanced, on the first of them.
        assert_eq!(m.action(&format!("{CHANGED}Play"), 1), Act::Done);
        let d = m.data();
        assert_eq!(d.level, Advanced);
        assert_eq!(d.focus, Some(("Play".into(), "turn".into())));
        assert_eq!(m.data().focus, None, "once");
        // Reset: asked, naming the count and the section, then done.
        m.change(SHOW_ADVANCED, 0);
        let id = format!("{CHANGED}Play");
        let act = m.action(&id, 0);
        assert_eq!(act, Act::Confirm(Reset::Advanced("Play".into())));
        let Act::Confirm(reset) = act else { unreachable!() };
        let (title, body, yes) = reset.texts(&m);
        assert_eq!(title, "Reset these settings?");
        assert_eq!(body, "The 2 changed advanced options of Play go back to their defaults.");
        assert_eq!(yes, "Reset");
        m.reset(&reset);
        assert_eq!(m.settings.transform.turn, ddi_engine::Turn::Off);
        assert_eq!(m.settings.lane_cover.mode, ddi_engine::CoverMode::Off);
        assert!(!m.settings.fast_pad_poll, "another section's kept");
        assert_eq!(m.notice, "2 settings back to their defaults.");
        assert_eq!(m.settings.level, Basic);
        // Play at Basic: Speed, Reverse and the plain assists (stage 2),
        // with nothing changed no count.
        let notes: Vec<String> = section(&m.data(), "Play")
            .unwrap()
            .options
            .iter()
            .map(|o| o.id.clone())
            .collect();
        assert_eq!(notes, ["speed", "reverse", "cut", "at_once", "no_holds"]);
        let stored: Settings = storage::load(Area::Local, crate::state::settings::KEY).unwrap();
        assert_eq!(stored.transform.turn, ddi_engine::Turn::Off);
    }

    /// At Basic a hidden row's heading goes to the next row shown under
    /// it; a section with no Basic row and nothing changed is left out
    /// (where an Advanced-only section, desktop's Plugins later, goes),
    /// and with something changed it shows only its count.
    #[test]
    fn headings_carry_and_advanced_sections_hide() {
        storage::install(Rc::new(MemoryStore::default()));
        // No controllers read: "Controllers" heads the notice after the
        // poll rate's hidden row.
        let world = World {
            pads_supported: false,
            sections: vec![OptionSection {
                id: "Test".into(),
                name: "TEST".into(),
                options: vec![OptionDef {
                    id: "test.switch".into(),
                    label: "Switch".into(),
                    kind: OptionKind::Choices(vec!["Off".into(), "On".into()]),
                    index: 0,
                    note: String::new(),
                    description: String::new(),
                    heading: None,
                    level: Advanced,
                    default: Some(0),
                }],
            }],
            ..World::default()
        };
        let mut m = OptionsModel::new(Settings::default(), world);
        let d = m.data();
        let controls = section(&d, "Controls").unwrap();
        let none = controls.options.iter().find(|o| o.id == "pads.none").unwrap();
        assert_eq!(none.heading.as_deref(), Some("CONTROLLERS"));
        // The Show advanced options row first in Interface.
        let display = section(&d, "Interface").unwrap();
        let at = display.options.iter().position(|o| o.id == SHOW_ADVANCED).unwrap();
        assert_eq!(at, 0);
        assert_eq!(display.options[at].heading, None);
        assert_eq!(display.options[at + 1].id, "language");
        assert_eq!(display.options[at + 1].heading, None);
        assert_eq!(display.options[at].label, "Show advanced options");
        assert_eq!(display.options[at].values(), ["Off", "On"]);
        assert_eq!(display.options[at].index, 0, "Off at Basic");
        assert!(section(&d, "Test").is_none());
        assert!(section(&m.data_at(Advanced), "Test").is_some());
        m.world.sections[0].options[0].index = 1;
        let d = m.data();
        let test = section(&d, "Test").unwrap();
        assert_eq!(test.options.len(), 1);
        assert_eq!(test.options[0].id, format!("{CHANGED}Test"));
    }

    /// Skins' parameters take the skin's level (Infinito declares its
    /// Note colors Basic, since 2026-11-27), announcers' are Advanced,
    /// each counted against its own declared default.
    #[test]
    fn parameters_take_their_declared_level_and_default() {
        storage::install(Rc::new(MemoryStore::default()));
        let mut m = OptionsModel::new(Settings::default(), World::default());
        let d = m.data();
        let arrows: Vec<&str> = section(&d, "Arrows")
            .unwrap()
            .options
            .iter()
            .map(|o| o.id.as_str())
            .collect();
        assert_eq!(arrows, ["noteskin", "skin.Colors"], "Infinito's Basic one");
        m.settings
            .set_infinito_param("ReceptorFill", ParamValue::Bool(false));
        m.settings
            .set_infinito_param("Colors", ParamValue::Text("Rainbow".into()));
        let d = m.data();
        let arrows = section(&d, "Arrows").unwrap();
        assert_eq!(arrows.options.last().unwrap().label, "Advanced changes: 1");
    }

    /// Settings stored before the levels: Advanced where an Advanced
    /// setting differs from its default, else Basic, decided once.
    #[test]
    fn those_who_play_already_keep_what_they_see() {
        let plain = Settings::default();
        assert!(!advanced_in_effect(&plain));
        let mut basic_only = Settings {
            speed: 3.0,
            reverse: true,
            lyrics: false,
            dancing_hand: false,
            ..Settings::default()
        };
        basic_only.audio_profiles.push(OffsetProfile {
            label: "a".into(),
            id: "audio:a".into(),
            facts: None,
            offset: 0.02,
            calibrated: true,
            last_seen: 0.0,
        });
        assert!(!advanced_in_effect(&basic_only), "calibration is Basic's");
        let changed: [fn(&mut Settings); 16] = [
            |s| s.transform.turn = ddi_engine::Turn::Mirror,
            |s| s.ruleset = "itg".into(),
            |s| s.audio_offset = 0.01,
            |s| s.fast_pad_poll = false,
            |s| s.pump_noteskin = Some("pump/cmd".into()),
            |s| s.set_infinito_param("ReceptorFill", ParamValue::Bool(false)),
            |s| {
                s.set_announcer_param("techno", "Mood", Some(ParamValue::Text("Calm".into())), None)
            },
            // The rows Basic left on 2026-11-26 (the maintainer's lean)
            // and stage 2 did not bring back.
            |s| s.mine_sound = false,
            |s| s.field_place = ddi_engine::FieldPlace::Left,
            |s| s.opponent_fields = crate::state::settings::OpponentFields::UpToEight,
            |s| {
                s.twitch_chat = ddi_chat::Level::Messages;
                s.chat_names = true;
            },
            |s| s.dancing_hand_multiplayer = true,
            |s| s.video = crate::state::settings::VideoMode::Off,
            |s| s.generated_background = crate::state::settings::GeneratedBackground::Off,
            |s| s.generated_palette = crate::state::settings::Palette::Mono,
            |s| s.set_infinito_param("Explosions", ParamValue::Bool(false)),
        ];
        for (i, change) in changed.into_iter().enumerate() {
            let mut s = Settings::default();
            change(&mut s);
            assert!(advanced_in_effect(&s), "change {i}");
        }
        // The rows stage 2 brought back to Basic under plain words no
        // longer count (`docs/plans/ui-levels.md`, section 10).
        let back: [fn(&mut Settings); 7] = [
            |s| s.transform.cut = ddi_engine::TimingCut::Quarters,
            |s| s.transform.at_once = ddi_engine::AtOnce::Two,
            |s| s.transform.no_holds = true,
            |s| s.assist_clap = true,
            |s| s.ghost = true,
            |s| s.field_filter = 0.0,
            // Infinito's Note colors, back on 2026-11-27.
            |s| s.set_infinito_param("Colors", ParamValue::Text("Rainbow".into())),
        ];
        for (i, change) in back.into_iter().enumerate() {
            let mut s = Settings::default();
            change(&mut s);
            assert!(!advanced_in_effect(&s), "brought back {i}");
        }
        // As stored: no mark, decided at load and kept.
        for (blob, level) in [
            (r#"{"speed": 3.0}"#, Basic),
            (r#"{"transform": {"turn": "Mirror"}}"#, Advanced),
            (r#"{"show_deltas": true}"#, Advanced),
            // Rows Basic left on 2026-11-26 and kept at Advanced.
            (r#"{"mine_sound": false}"#, Advanced),
            (r#"{"skin_params": {"bundled:dance/infinito": {"Colors": "Rainbow"}}}"#, Basic),
            (r#"{"skin_params": {"bundled:dance/infinito": {"Explosions": false}}}"#, Advanced),
            (r#"{"lyrics": false, "reverse": true}"#, Basic),
            // Rows stage 2 brought back: Basic where not yet placed.
            (r#"{"assist_clap": true}"#, Basic),
            (r#"{"transform": {"no_holds": true}}"#, Basic),
            (r#"{"ghost": true, "speed": 3.0}"#, Basic),
            // A choice made after the decision stays, and those placed
            // before stage 2 for a row it brought back stay where they
            // were placed: nobody moves.
            (r#"{"show_deltas": true, "level": "Basic", "level_decided": true}"#, Basic),
            (r#"{"assist_clap": true, "level": "Advanced", "level_decided": true}"#, Advanced),
            (
                r#"{"transform": {"cut": "Quarters"}, "level": "Advanced", "level_decided": true}"#,
                Advanced,
            ),
        ] {
            storage::install(Rc::new(MemoryStore::default()));
            storage::set(Area::Local, crate::state::settings::KEY, blob).unwrap();
            let s = Settings::load();
            assert_eq!(s.level, level, "{blob}");
            assert!(s.level_decided);
            let stored: Settings = storage::load(Area::Local, crate::state::settings::KEY).unwrap();
            assert_eq!((stored.level, stored.level_decided), (level, true), "{blob}");
        }
    }

    /// Every row the model makes, once, with its section, label,
    /// description and level, as tab-separated lines: the table the
    /// maintainer marks up (`cargo test -p ddi-ui --lib rows_table --
    /// --ignored --nocapture`).
    #[test]
    #[ignore]
    fn rows_table() {
        storage::install(Rc::new(MemoryStore::default()));
        let mut seen = std::collections::HashSet::new();
        for m in fullest() {
            let d = m.data_at(Advanced);
            for s in &d.sections {
                for o in &s.options {
                    if seen.insert(o.id.clone()) {
                        let level = level_of(&o.id).unwrap_or(Advanced);
                        let description = o.description.replace('\n', " ");
                        println!("{}\t{}\t{}\t{description}\t{level:?}", s.id, o.id, o.label);
                    }
                }
            }
        }
    }

    /// The rows each section shows at each level, for the plan and the
    /// report (`cargo test -p ddi-ui --lib rows_per_section -- --ignored
    /// --nocapture`).
    #[test]
    #[ignore]
    fn rows_per_section() {
        storage::install(Rc::new(MemoryStore::default()));
        for platform in [Platform::Web, Platform::Desktop] {
            let world = World {
                platform,
                pads_supported: true,
                sample: crate::screens::options_model::Sample::Ready,
                folders: (platform == Platform::Desktop).then(|| vec!["/Songs".into()]),
                ..World::default()
            };
            let m = OptionsModel::new(Settings::default(), world);
            let (basic, advanced) = (m.data_at(Basic), m.data_at(Advanced));
            println!("{platform:?}: section | Basic | Advanced");
            for s in &advanced.sections {
                let b = section(&basic, &s.id).map_or(0, |b| b.options.len());
                println!("{} | {b} | {}", s.id, s.options.len());
            }
            println!(
                "total | {} | {}",
                rows(&basic).count(),
                rows(&advanced).count()
            );
        }
    }
}
