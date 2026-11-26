//! The start prompt's play options (`docs/plans/start-prompt.md`): the
//! few options a player changes between songs, as StepMania's player
//! options come before each song (`ScreenPlayerOptions`): Speed, Reverse,
//! Turn, Appearance, Lane cover and the noteskin, and on a touch screen
//! the touch controls (`docs/plans/touch-controls.md`). Their rows are Options'
//! own ([`OptionsModel`], never saving), so names, choices and ranges
//! match; a change applies to this play and its retry only
//! ([`PlayOverrides`] over the stored settings), and "Save as default"
//! writes those rows, and only those, to the settings.

use ddi_engine::{Appearance, CoverMode, Turn};

use super::data::{OptionDef, Platform};
use super::options_model::{OptionsModel, World};
use crate::state::settings::{Level, Settings};

/// The rows, in their order; `noteskin` is `pump_noteskin` on a pump
/// layout when pump skins can be chosen.
const ROWS: [&str; 8] = [
    "speed",
    "reverse",
    "turn",
    "appearance",
    "lane_cover",
    "noteskin",
    "touch_controls",
    "touch_size",
];

/// The rows shown on a touch screen alone.
const TOUCH_ROWS: [&str; 2] = ["touch_controls", "touch_size"];

/// What a play runs with beyond the stored settings: the prompt's rows
/// the player changed, field by field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayOverrides {
    pub speed: Option<f64>,
    pub reverse: Option<bool>,
    pub turn: Option<Turn>,
    pub appearance: Option<Appearance>,
    pub cover: Option<CoverMode>,
    pub noteskin: Option<Option<String>>,
    pub pump_noteskin: Option<Option<String>>,
    pub touch_controls: Option<crate::touch_controls::Style>,
    pub touch_size: Option<crate::touch_controls::Size>,
}

impl PlayOverrides {
    /// What `played` changes of `stored`, in the prompt's rows.
    pub fn between(stored: &Settings, played: &Settings) -> PlayOverrides {
        fn differs<T: PartialEq + Clone>(a: &T, b: &T) -> Option<T> {
            (a != b).then(|| b.clone())
        }
        PlayOverrides {
            speed: ((stored.speed - played.speed).abs() > 1e-9).then_some(played.speed),
            reverse: differs(&stored.reverse, &played.reverse),
            turn: differs(&stored.transform.turn, &played.transform.turn),
            appearance: differs(&stored.appearance, &played.appearance),
            cover: differs(&stored.lane_cover.mode, &played.lane_cover.mode),
            noteskin: differs(&stored.noteskin, &played.noteskin),
            pump_noteskin: differs(&stored.pump_noteskin, &played.pump_noteskin),
            touch_controls: differs(&stored.touch_controls, &played.touch_controls),
            touch_size: differs(&stored.touch_size, &played.touch_size),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == PlayOverrides::default()
    }

    /// Whether the skin a play reads changes (its key, or what its Lua
    /// is told: the scroll direction).
    pub fn skin(&self) -> bool {
        self.noteskin.is_some() || self.pump_noteskin.is_some() || self.reverse.is_some()
    }

    /// The play's settings: `s` with these over it.
    pub fn apply(&self, s: &mut Settings) {
        if let Some(v) = self.speed {
            s.speed = v;
        }
        if let Some(v) = self.reverse {
            s.reverse = v;
        }
        if let Some(v) = self.turn {
            s.transform.turn = v;
        }
        if let Some(v) = self.appearance {
            s.appearance = v;
        }
        if let Some(v) = self.cover {
            s.lane_cover.mode = v;
        }
        if let Some(v) = &self.noteskin {
            s.noteskin = v.clone();
        }
        if let Some(v) = &self.pump_noteskin {
            s.pump_noteskin = v.clone();
        }
        if let Some(v) = self.touch_controls {
            s.touch_controls = v;
        }
        if let Some(v) = self.touch_size {
            s.touch_size = v;
        }
    }

    /// The same over a settings snapshot as stored (a play record's,
    /// which keeps fields this version does not know): each field written
    /// where the settings keep it.
    pub fn apply_json(&self, v: &mut serde_json::Value) {
        let Some(o) = v.as_object_mut() else {
            return;
        };
        fn json<T: serde::Serialize>(x: &T) -> serde_json::Value {
            serde_json::to_value(x).unwrap_or_default()
        }
        if let Some(x) = self.speed {
            o.insert("speed".into(), json(&x));
        }
        if let Some(x) = self.reverse {
            o.insert("reverse".into(), json(&x));
        }
        if let Some(x) = &self.noteskin {
            o.insert("noteskin".into(), json(x));
        }
        if let Some(x) = &self.pump_noteskin {
            o.insert("pump_noteskin".into(), json(x));
        }
        if let Some(x) = self.appearance {
            o.insert("appearance".into(), json(&x));
        }
        if let Some(x) = self.touch_controls {
            o.insert("touch_controls".into(), json(&x));
        }
        if let Some(x) = self.touch_size {
            o.insert("touch_size".into(), json(&x));
        }
        let mut within = |outer: &str, inner: &str, x: serde_json::Value| {
            let e = o
                .entry(outer.to_string())
                .or_insert_with(|| serde_json::Value::Object(Default::default()));
            if !e.is_object() {
                *e = serde_json::Value::Object(Default::default());
            }
            if let Some(e) = e.as_object_mut() {
                e.insert(inner.into(), x);
            }
        };
        if let Some(x) = self.turn {
            within("transform", "turn", json(&x));
        }
        if let Some(x) = self.cover {
            within("lane_cover", "mode", json(&x));
        }
    }
}

/// The prompt's play options: the stored settings as the prompt opened,
/// and the play's.
pub struct PlayOptions {
    model: OptionsModel,
    stored: Settings,
    /// The chart's layout is a pump one: its skin is the pump row's.
    pump: bool,
    /// The skins are all listed (until then the skin's row is not shown:
    /// a skin from a folder not listed yet would read as Dito).
    listed: bool,
    /// Saved as the defaults since the last change.
    saved: bool,
    /// A touch screen plays: the touch controls' row shows.
    touch: bool,
}

impl PlayOptions {
    /// On `stored`, for a chart of layout `layout` (an id), on `platform`.
    pub fn new(stored: Settings, layout: &str, platform: Platform) -> PlayOptions {
        let world = World {
            platform,
            ..World::default()
        };
        let mut model = OptionsModel::new(stored.clone(), world);
        model.transient = true;
        PlayOptions {
            model,
            stored,
            pump: layout.starts_with("pump"),
            listed: false,
            saved: false,
            touch: false,
        }
    }

    /// The skins one can choose (`(key, name)`), and whether that is all
    /// of them.
    pub fn set_noteskins(&mut self, skins: Vec<(String, String)>, complete: bool) {
        // Not `complete` for the model: a chosen skin stays chosen.
        self.model.set_noteskins(skins, false, false);
        self.listed |= complete;
    }

    /// A touch screen plays (the touch controls' row shows).
    pub fn set_touch(&mut self, touch: bool) {
        self.touch = touch;
    }

    /// The chart's layout (an id), once the song is read.
    pub fn set_layout(&mut self, layout: &str) {
        self.pump = layout.starts_with("pump");
    }

    /// The rows, as Options shows them: at Basic
    /// (`docs/plans/ui-levels.md`) the Basic ones and each Advanced one
    /// changed from its default, in the stored settings or for this play
    /// (Turn, Appearance, Lane cover; a pump skin), so that a modifier in
    /// effect shows where it can be undone and never goes from under the
    /// player's hands. The skin's row is the pump one on a pump layout
    /// when pump skins can be chosen (and it shows).
    pub fn rows(&self) -> Vec<OptionDef> {
        let all: Vec<OptionDef> = self.model.prompt_rows().collect();
        let basic = self.model.settings.level == Level::Basic;
        let stored: Vec<OptionDef> = if basic {
            OptionsModel::new(self.stored.clone(), self.model.world.clone())
                .prompt_rows()
                .filter(OptionDef::changed)
                .collect()
        } else {
            Vec::new()
        };
        let shown = |o: &OptionDef| {
            !basic
                || o.level == Level::Basic
                || o.changed()
                || stored.iter().any(|s| s.id == o.id)
        };
        let skin = if self.pump && all.iter().any(|o| o.id == "pump_noteskin" && shown(o)) {
            "pump_noteskin"
        } else {
            "noteskin"
        };
        ROWS.iter()
            .map(|id| if *id == "noteskin" { skin } else { id })
            .filter(|id| self.listed || !id.ends_with("noteskin"))
            .filter(|id| self.touch || !TOUCH_ROWS.contains(id))
            .filter_map(|id| all.iter().find(|o| o.id == id).cloned())
            .filter(shown)
            .map(|mut o| {
                o.heading = None;
                o
            })
            .collect()
    }

    /// Row `id` to value `index` (this play's only); what changed, if
    /// anything did.
    pub fn change(&mut self, id: &str, index: usize) -> Option<PlayOverrides> {
        if !ROWS.contains(&id) && id != "pump_noteskin" {
            return None;
        }
        let before = self.model.settings.clone();
        self.model.change(id, index);
        let changed = PlayOverrides::between(&before, &self.model.settings);
        if changed.is_empty() {
            return None;
        }
        self.saved = false;
        Some(changed)
    }

    /// What the play runs with beyond the stored settings.
    pub fn overrides(&self) -> PlayOverrides {
        PlayOverrides::between(&self.stored, &self.model.settings)
    }

    /// The play's settings: `s` (the stored ones, read again) with the
    /// rows changed over it.
    pub fn over(&self, s: &mut Settings) {
        self.overrides().apply(s);
    }

    /// The rows differ from the stored settings.
    pub fn differs(&self) -> bool {
        !self.overrides().is_empty()
    }

    pub fn saved(&self) -> bool {
        self.saved
    }

    /// Keeps the rows as the defaults: the settings stored now (another
    /// screen may have changed others) with these over them.
    pub fn save(&mut self) {
        let mut s = Settings::load();
        self.over(&mut s);
        s.save();
        self.stored = s;
        self.saved = true;
    }

    /// What the prompt shows of them.
    pub fn show(&self, state: &mut super::prompt::PromptState) {
        state.set_options(self.rows(), self.differs(), self.saved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::settings::INFINITO;

    /// The prompt's options at Advanced: every row it offers.
    fn options() -> PlayOptions {
        options_at(Level::Advanced, Settings::default())
    }

    fn options_at(level: Level, stored: Settings) -> PlayOptions {
        let mut o = PlayOptions::new(Settings { level, ..stored }, "dance-single", Platform::Web);
        o.set_noteskins(
            vec![
                (INFINITO.into(), "Infinito".into()),
                ("bundled:dance/Peters-DDR".into(), "Peters-DDR".into()),
                ("pump/cmd".into(), "cmd".into()),
            ],
            true,
        );
        o
    }

    #[test]
    fn the_rows_are_options_own_in_the_prompts_order() {
        let o = options();
        let ids: Vec<String> = o.rows().into_iter().map(|r| r.id).collect();
        assert_eq!(
            ids,
            [
                "speed",
                "reverse",
                "turn",
                "appearance",
                "lane_cover",
                "noteskin"
            ]
        );
        assert_eq!(o.rows()[0].value(), "×2.5");
        // A pump chart: the pump skins' row.
        let mut p = options();
        p.set_layout("pump-single");
        assert_eq!(p.rows()[5].id, "pump_noteskin");
        // Each as Options shows it, without its heading.
        for o in [o, p] {
            let all: Vec<OptionDef> = o
                .model
                .data()
                .sections
                .into_iter()
                .flat_map(|s| s.options)
                .collect();
            for row in o.rows() {
                let mut shown = all.iter().find(|a| a.id == row.id).unwrap().clone();
                shown.heading = None;
                assert_eq!(shown, row);
            }
        }
        // The skins not all listed yet: no skin row.
        let o = PlayOptions::new(
            Settings {
                level: Level::Advanced,
                ..Settings::default()
            },
            "dance-single",
            Platform::Web,
        );
        assert_eq!(o.rows().len(), 5);
    }

    #[test]
    fn a_change_is_this_plays_until_saved() {
        ddi_platform::storage::install(std::rc::Rc::new(
            ddi_platform::storage::MemoryStore::default(),
        ));
        let mut o = options();
        let speed = o.rows()[0].index;
        let changed = o.change("speed", speed + 2).unwrap();
        assert_eq!(changed.speed, Some(3.0));
        assert!(!changed.skin());
        let changed = o.change("reverse", 1).unwrap();
        assert!(changed.skin());
        assert!(o.change("reverse", 1).is_none());
        o.change("turn", 1);
        assert!(o.differs());
        // Not saved.
        assert!(
            ddi_platform::storage::get(
                ddi_platform::storage::Area::Local,
                crate::state::settings::KEY
            )
            .is_none()
        );
        let mut s = Settings::load();
        o.over(&mut s);
        assert_eq!((s.speed, s.reverse), (3.0, true));
        assert_eq!(s.transform.turn, Turn::Mirror);
        // Kept in a record's snapshot, unknown fields with them.
        let mut v = serde_json::json!({"speed": 2.5, "future": 1, "transform": {"cut": "All"}});
        o.overrides().apply_json(&mut v);
        assert_eq!(v["speed"], 3.0);
        assert_eq!(v["reverse"], true);
        assert_eq!(v["future"], 1);
        assert_eq!(v["transform"]["cut"], "All");
        assert_eq!(v["transform"]["turn"], "Mirror");
        // Saved when asked: those rows, nothing else.
        let mut other = Settings::load();
        other.lyrics = false;
        other.save();
        o.save();
        let s = Settings::load();
        assert_eq!((s.speed, s.reverse, s.lyrics), (3.0, true, false));
        assert!(o.saved() && !o.differs());
    }

    /// At Basic (`docs/plans/ui-levels.md`) the prompt offers Speed,
    /// Reverse and the arrows; an Advanced row changed, stored or for this
    /// play, stays shown so the modifier can be undone where it acts, and
    /// does not go when put back; a pump skin chosen shows on a pump
    /// layout, else the arrows' row.
    #[test]
    fn basic_shows_its_rows_and_a_changed_advanced_one() {
        ddi_platform::storage::install(std::rc::Rc::new(
            ddi_platform::storage::MemoryStore::default(),
        ));
        let ids = |o: &PlayOptions| -> Vec<String> { o.rows().into_iter().map(|r| r.id).collect() };
        let o = options_at(Level::Basic, Settings::default());
        assert_eq!(ids(&o), ["speed", "reverse", "noteskin"]);
        let mut mirror = Settings::default();
        mirror.transform.turn = Turn::Mirror;
        let mut o = options_at(Level::Basic, mirror);
        assert_eq!(ids(&o), ["speed", "reverse", "turn", "noteskin"]);
        o.change("turn", 0);
        let turn = o.rows().into_iter().find(|r| r.id == "turn").unwrap();
        assert_eq!(turn.index, 0, "put back for this play, still shown");
        // Changed for this play alone shows too.
        let mut o = options_at(Level::Basic, Settings::default());
        assert!(o.change("appearance", 1).is_some());
        assert!(ids(&o).contains(&"appearance".to_string()));
        let pump = Settings {
            pump_noteskin: Some("pump/cmd".into()),
            ..Settings::default()
        };
        let mut o = options_at(Level::Basic, pump);
        o.set_layout("pump-single");
        assert_eq!(ids(&o).last().map(String::as_str), Some("pump_noteskin"));
        let mut o = options_at(Level::Basic, Settings::default());
        o.set_layout("pump-single");
        assert_eq!(ids(&o).last().map(String::as_str), Some("noteskin"));
    }

    /// On a touch screen the prompt offers the touch controls and, since
    /// round DE, their size, Options' own rows; a size changed is this
    /// play's, in its record's snapshot, and saved when asked.
    #[test]
    fn a_touch_screen_offers_the_controls_and_their_size() {
        ddi_platform::storage::install(std::rc::Rc::new(
            ddi_platform::storage::MemoryStore::default(),
        ));
        let mut o = options();
        assert!(o.rows().iter().all(|r| !r.id.starts_with("touch")));
        o.set_touch(true);
        let ids: Vec<String> = o.rows().into_iter().map(|r| r.id).collect();
        assert_eq!(ids[6..], ["touch_controls", "touch_size"]);
        let size = o.rows().into_iter().find(|r| r.id == "touch_size").unwrap();
        assert_eq!(size.index, 1, "Standard");
        let changed = o.change("touch_size", 2).unwrap();
        assert_eq!(changed.touch_size, Some(crate::touch_controls::Size::Larger));
        let mut s = Settings::load();
        o.over(&mut s);
        assert_eq!(s.touch_choice().size, crate::touch_controls::Size::Larger);
        let mut v = serde_json::json!({});
        o.overrides().apply_json(&mut v);
        assert_eq!(v["touch_size"], "Larger");
        o.save();
        assert_eq!(Settings::load().touch_size, crate::touch_controls::Size::Larger);
    }
}
