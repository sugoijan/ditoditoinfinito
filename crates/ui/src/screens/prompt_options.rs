//! The start prompt's play options (`docs/plans/start-prompt.md`): the
//! few options a player changes between songs, as StepMania's player
//! options come before each song (`ScreenPlayerOptions`): Speed, Reverse,
//! Turn, Appearance, Lane cover and the noteskin. Their rows are Options'
//! own ([`OptionsModel`], never saving), so names, choices and ranges
//! match; a change applies to this play and its retry only
//! ([`PlayOverrides`] over the stored settings), and "Save as default"
//! writes those rows, and only those, to the settings.

use ddi_engine::{Appearance, CoverMode, Turn};

use super::data::{OptionDef, Platform};
use super::options_model::{OptionsModel, World};
use crate::state::settings::Settings;

/// The rows, in their order; `noteskin` is `pump_noteskin` on a pump
/// layout when pump skins can be chosen.
const ROWS: [&str; 6] = [
    "speed",
    "reverse",
    "turn",
    "appearance",
    "lane_cover",
    "noteskin",
];

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
    }

    /// The same over a settings snapshot as stored (a play record's,
    /// which keeps fields this version does not know): each field written
    /// where the settings keep it.
    pub fn apply_json(&self, v: &mut serde_json::Value) {
        let Some(o) = v.as_object_mut() else {
            return;
        };
        let json = |x: &dyn erased::Ser| x.json();
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

/// Serialising values of different types through one closure.
mod erased {
    pub trait Ser {
        fn json(&self) -> serde_json::Value;
    }
    impl<T: serde::Serialize> Ser for T {
        fn json(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap_or_default()
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
        }
    }

    /// The skins one can choose (`(key, name)`), and whether that is all
    /// of them.
    pub fn set_noteskins(&mut self, skins: Vec<(String, String)>, complete: bool) {
        // Not `complete` for the model: a chosen skin stays chosen.
        self.model.set_noteskins(skins, false, false);
        self.listed |= complete;
    }

    /// The chart's layout (an id), once the song is read.
    pub fn set_layout(&mut self, layout: &str) {
        self.pump = layout.starts_with("pump");
    }

    /// The skin's row: the pump one on a pump layout when pump skins can
    /// be chosen.
    fn skin_row(&self, rows: &[OptionDef]) -> &'static str {
        if self.pump && rows.iter().any(|o| o.id == "pump_noteskin") {
            "pump_noteskin"
        } else {
            "noteskin"
        }
    }

    /// The rows, as Options shows them.
    pub fn rows(&self) -> Vec<OptionDef> {
        let all: Vec<OptionDef> = self
            .model
            .data()
            .sections
            .into_iter()
            .flat_map(|s| s.options)
            .collect();
        let skin = self.skin_row(&all);
        ROWS.iter()
            .map(|id| if *id == "noteskin" { skin } else { id })
            .filter(|id| self.listed || !id.ends_with("noteskin"))
            .filter_map(|id| all.iter().find(|o| o.id == id).cloned())
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
    use crate::state::settings::DITO;

    fn options() -> PlayOptions {
        let mut o = PlayOptions::new(Settings::default(), "dance-single", Platform::Web);
        o.set_noteskins(
            vec![
                (DITO.into(), "Dito".into()),
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
        // The skins not all listed yet: no skin row.
        let o = PlayOptions::new(Settings::default(), "dance-single", Platform::Web);
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
}
