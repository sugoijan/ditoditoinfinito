//! Skin parameters, our extension (`docs/plans/skin-parameters.md`): the
//! controls a skin declares in its `metrics.ini` (choices, toggles,
//! numbers in a range, colors; their names, descriptions and defaults),
//! which Options shows under Arrows for the chosen skin and which its Lua
//! reads (`NOTESKIN:GetParameter`). A choice may name the skin that draws
//! it: a skin falling back on this one, loaded in its place ([`head`]),
//! the way StepMania's own skins share a base. A skin without
//! declarations behaves exactly as before.
//!
//! ```ini
//! [Parameters]
//! List=Colors,Explosions
//!
//! [Parameter Colors]
//! Type=Choice
//! Label=Note colors
//! Description=How notes are colored.
//! Choices=Vivid,Flat
//! ChoiceLabels=Vivid,Flat
//! Skins=Dito Vivid,Dito Flat
//! Default=Vivid
//!
//! [Parameter Explosions]
//! Type=Toggle
//! Default=1
//! ```
//!
//! Declarations are merged under the fallbacks' like every metric, so a
//! skin's variants declare what it declares. Everything is bounded: a
//! declaration past a limit is left out with a warning, and a value is
//! only ever one its declaration accepts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ini::{Ini, string_to_float};
use crate::skin::{Folder, Skin};

/// Parameters a skin may declare.
pub const MAX_PARAMETERS: usize = 16;
/// Characters of a parameter's name or a choice's id.
pub const MAX_NAME: usize = 32;
/// Characters of a label.
pub const MAX_LABEL: usize = 48;
/// Characters of a description.
pub const MAX_DESCRIPTION: usize = 400;
/// Choices of a choice or a color's palette.
pub const MAX_CHOICES: usize = 32;
/// Values between a number's minimum and maximum, both included.
pub const MAX_STEPS: usize = 1001;
/// Characters of a number's unit.
pub const MAX_UNIT: usize = 8;
/// Warnings kept about a skin's declarations.
const MAX_WARNINGS: usize = 32;

/// A parameter's value: a choice's id or a color (`#RRGGBB`, `#RRGGBBAA`),
/// a toggle's, a number's. Stored in the settings as plain JSON (`"Vivid"`,
/// `true`, `0.5`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Number(f64),
    Text(String),
}

/// Values by parameter name.
pub type Values = BTreeMap<String, Value>;

/// One declared parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    /// As `[Parameters] List` names it; what the settings and the Lua know
    /// it by.
    pub name: String,
    /// What Options calls it (its name when not given).
    pub label: String,
    /// What it does, in a sentence or two.
    pub description: String,
    pub kind: Kind,
    /// Always one [`Parameter::accept`] takes.
    pub default: Value,
}

/// What a parameter is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// One of named choices.
    Choice(Vec<Choice>),
    /// On or off.
    Toggle,
    /// A number from `min` to `max` by `step`, shown with `unit` after it.
    Number {
        min: f64,
        max: f64,
        step: f64,
        unit: String,
    },
    /// A color from a palette (its ids `#RRGGBB`); a stored color off the
    /// palette is kept too.
    Color(Vec<Choice>),
}

/// A choice, or a color of a palette.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub id: String,
    pub label: String,
    /// The skin that draws the skin with this choice (a choice's `Skins`
    /// entry): one falling back on it.
    pub skin: Option<String>,
}

/// A skin's declarations, and what was wrong with them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Declared {
    pub parameters: Vec<Parameter>,
    pub warnings: Vec<String>,
}

/// The palette a color parameter without `Choices` offers.
const PALETTE: [(&str, &str); 12] = [
    ("#59D9FF", "Cyan"),
    ("#4D7CFF", "Blue"),
    ("#A066FF", "Violet"),
    ("#FF66CC", "Pink"),
    ("#FF4D4D", "Red"),
    ("#FF9940", "Orange"),
    ("#FFE04D", "Yellow"),
    ("#8CE64D", "Lime"),
    ("#40CC80", "Green"),
    ("#FFFFFF", "White"),
    ("#A6A6B3", "Gray"),
    ("#26262E", "Black"),
];

/// `s` cut to `max` characters.
fn cut(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// A comma-separated list, each entry trimmed.
fn list(s: &str) -> Vec<String> {
    s.split(',').map(|p| p.trim().to_string()).collect()
}

/// A name or id: letters, digits, `_` and `-`, at most [`MAX_NAME`].
fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().count() <= MAX_NAME
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `#RRGGBB` or `#RRGGBBAA`, in upper case.
pub fn hex_color(s: &str) -> Option<String> {
    let hex = s.trim().strip_prefix('#')?;
    ((hex.len() == 6 || hex.len() == 8) && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| format!("#{}", hex.to_ascii_uppercase()))
}

/// A metric read as a boolean, as `GetMetricB` reads one (`1`, `true`).
fn truthy(s: &str) -> bool {
    let s = s.trim();
    s.eq_ignore_ascii_case("true") || string_to_float(s) != 0.0
}

impl Parameter {
    /// `value` as this parameter takes it (a choice's id in its own case,
    /// a number clamped to the range and its step, a color in upper case),
    /// or `None`.
    pub fn accept(&self, value: &Value) -> Option<Value> {
        match (&self.kind, value) {
            (Kind::Choice(choices), Value::Text(t)) => choices
                .iter()
                .find(|c| c.id.eq_ignore_ascii_case(t))
                .map(|c| Value::Text(c.id.clone())),
            (Kind::Toggle, Value::Bool(b)) => Some(Value::Bool(*b)),
            (Kind::Number { min, max, step, .. }, Value::Number(n)) if n.is_finite() => {
                let steps = ((n.clamp(*min, *max) - min) / step).round();
                Some(Value::Number((min + steps * step).min(*max)))
            }
            (Kind::Color(_), Value::Text(t)) => hex_color(t).map(Value::Text),
            _ => None,
        }
    }

    /// The numbers a number parameter offers, from its minimum.
    pub fn numbers(&self) -> Vec<f64> {
        match &self.kind {
            Kind::Number { min, max, step, .. } => {
                let n = ((max - min) / step + 1e-9).floor() as usize + 1;
                (0..n.min(MAX_STEPS))
                    .map(|i| min + i as f64 * step)
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// The skin drawing value `value` (a choice's `Skins` entry).
    fn skin_for(&self, value: &Value) -> Option<&str> {
        match (&self.kind, value) {
            (Kind::Choice(choices), Value::Text(t)) => choices
                .iter()
                .find(|c| &c.id == t)
                .and_then(|c| c.skin.as_deref()),
            _ => None,
        }
    }
}

impl Declared {
    /// The declarations of a skin's merged metrics.
    pub fn from_ini(ini: &Ini) -> Declared {
        let mut d = Declared::default();
        let Some(names) = ini.get("Parameters", "List") else {
            return d;
        };
        for name in list(names).into_iter().filter(|n| !n.is_empty()) {
            if !is_name(&name) {
                d.warn(format!(
                    "parameter \"{}\": a name is up to {MAX_NAME} letters, digits, _ or -",
                    cut(&name, MAX_NAME)
                ));
                continue;
            }
            if d.parameters
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&name))
            {
                d.warn(format!("parameter {name} is listed twice"));
                continue;
            }
            if d.parameters.len() == MAX_PARAMETERS {
                d.warn(format!(
                    "more than {MAX_PARAMETERS} parameters: {name} and the rest left out"
                ));
                break;
            }
            match parameter(ini, &name) {
                Ok(p) => d.parameters.push(p),
                Err(e) => d.warn(format!("parameter {name}: {e}")),
            }
        }
        d
    }

    /// The declarations of skin `name` of `game` among `folders` (its
    /// fallbacks' merged under its own); none when it does not load.
    pub fn of_skin(folders: &[Folder], game: &str, name: &str) -> Declared {
        Skin::load(folders, game, name)
            .map(|s| Declared::from_ini(&s.metrics))
            .unwrap_or_default()
    }

    fn warn(&mut self, w: String) {
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(w);
        }
    }

    /// The parameter named `name`.
    pub fn get(&self, name: &str) -> Option<&Parameter> {
        self.parameters.iter().find(|p| p.name == name)
    }

    /// Every parameter's value: `stored`'s where its declaration takes it,
    /// else the default. Names not declared are left out.
    pub fn resolve(&self, stored: &Values) -> Values {
        self.parameters
            .iter()
            .map(|p| {
                let v = stored
                    .get(&p.name)
                    .and_then(|v| p.accept(v))
                    .unwrap_or_else(|| p.default.clone());
                (p.name.clone(), v)
            })
            .collect()
    }

    /// The skin drawing it with `values` (resolved): the first choice that
    /// names one.
    pub fn skin_for(&self, values: &Values) -> Option<&str> {
        self.parameters
            .iter()
            .find_map(|p| p.skin_for(values.get(&p.name)?))
    }
}

/// One `[Parameter <name>]` section.
fn parameter(ini: &Ini, name: &str) -> Result<Parameter, String> {
    let section = format!("Parameter {name}");
    let get = |key: &str| ini.get(&section, key).map(str::trim);
    let label = get("Label")
        .filter(|l| !l.is_empty())
        .map_or_else(|| name.to_string(), |l| cut(l, MAX_LABEL));
    let description = get("Description").map_or_else(String::new, |d| cut(d, MAX_DESCRIPTION));
    let default = get("Default").unwrap_or("");
    let ty = get("Type").ok_or_else(|| format!("no [{section}] Type"))?;
    let (kind, default) = if ty.eq_ignore_ascii_case("Choice") {
        let choices = choices(get("Choices"), get("ChoiceLabels"), get("Skins"), |id| {
            is_name(id).then(|| id.to_string())
        })?;
        let default = choices
            .iter()
            .find(|c| c.id.eq_ignore_ascii_case(default))
            .unwrap_or(&choices[0])
            .id
            .clone();
        (Kind::Choice(choices), Value::Text(default))
    } else if ty.eq_ignore_ascii_case("Toggle") {
        (Kind::Toggle, Value::Bool(truthy(default)))
    } else if ty.eq_ignore_ascii_case("Number") {
        let number = |key: &str| {
            get(key)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
        };
        let (Some(min), Some(max)) = (number("Min"), number("Max")) else {
            return Err("a number needs a Min and a Max".into());
        };
        let step = number("Step").unwrap_or(1.0);
        if max < min || step <= 0.0 {
            return Err("a number's Max is below its Min, or its Step is not above 0".into());
        }
        if (max - min) / step + 1.0 > MAX_STEPS as f64 {
            return Err(format!("a number offers at most {MAX_STEPS} values"));
        }
        let unit = cut(get("Unit").unwrap_or(""), MAX_UNIT);
        let kind = Kind::Number {
            min,
            max,
            step,
            unit,
        };
        let probe = Parameter {
            name: String::new(),
            label: String::new(),
            description: String::new(),
            kind,
            default: Value::Number(min),
        };
        let default = default
            .parse::<f64>()
            .ok()
            .and_then(|v| probe.accept(&Value::Number(v)))
            .unwrap_or(Value::Number(min));
        (probe.kind, default)
    } else if ty.eq_ignore_ascii_case("Color") {
        let mut palette = match get("Choices") {
            Some(_) => choices(get("Choices"), get("ChoiceLabels"), None, hex_color)?,
            None => PALETTE
                .iter()
                .map(|(id, label)| Choice {
                    id: id.to_string(),
                    label: label.to_string(),
                    skin: None,
                })
                .collect(),
        };
        let default = hex_color(default).ok_or("a color needs a Default, as #RRGGBB")?;
        if !palette.iter().any(|c| c.id == default) {
            if palette.len() == MAX_CHOICES {
                palette.pop();
            }
            palette.insert(
                0,
                Choice {
                    label: default.clone(),
                    id: default.clone(),
                    skin: None,
                },
            );
        }
        (Kind::Color(palette), Value::Text(default))
    } else {
        return Err(format!(
            "Type is Choice, Toggle, Number or Color, not \"{}\"",
            cut(ty, MAX_NAME)
        ));
    };
    Ok(Parameter {
        name: name.to_string(),
        label,
        description,
        kind,
        default,
    })
}

/// A choice's or a palette's entries: `ids` (each read by `id`), with
/// their `labels` and `skins` where given.
fn choices(
    ids: Option<&str>,
    labels: Option<&str>,
    skins: Option<&str>,
    id: impl Fn(&str) -> Option<String>,
) -> Result<Vec<Choice>, String> {
    let ids = match ids.map(str::trim) {
        Some(ids) if !ids.is_empty() => list(ids),
        _ => return Err("no Choices".into()),
    };
    if ids.len() > MAX_CHOICES {
        return Err(format!("at most {MAX_CHOICES} choices"));
    }
    let labels = labels.map(list).unwrap_or_default();
    let skins = skins.map(list).unwrap_or_default();
    let mut out: Vec<Choice> = Vec::new();
    for (i, raw) in ids.iter().enumerate() {
        let Some(id) = id(raw) else {
            return Err(format!("\"{}\" is not a choice's id", cut(raw, MAX_NAME)));
        };
        if out.iter().any(|c| c.id.eq_ignore_ascii_case(&id)) {
            return Err(format!("choice {id} is there twice"));
        }
        let label = labels
            .get(i)
            .filter(|l| !l.is_empty())
            .map_or_else(|| id.clone(), |l| cut(l, MAX_LABEL));
        let skin = skins.get(i).filter(|s| !s.is_empty()).map(|s| cut(s, 128));
        out.push(Choice { id, label, skin });
    }
    Ok(out)
}

/// The parameters the skins keyed `keys` ([`crate::bundled::key`]) among
/// `skins` declare, by key: each skin's folder and whether it is bundled
/// (a bundled skin's fallbacks are looked for among the bundled ones
/// only, as a play loads it). Skins that declare none are left out.
pub fn catalog<'a>(
    skins: impl IntoIterator<Item = (&'a Folder, bool)>,
    keys: &[String],
) -> BTreeMap<String, Vec<Parameter>> {
    let skins: Vec<(&Folder, bool)> = skins.into_iter().collect();
    let all: Vec<Folder> = skins.iter().map(|(f, _)| (*f).clone()).collect();
    let bundled: Vec<Folder> = skins
        .iter()
        .filter(|(_, b)| *b)
        .map(|(f, _)| (*f).clone())
        .collect();
    // No folder declares any: nothing to load.
    if !all.iter().any(|f| {
        f.text("metrics.ini")
            .is_some_and(|t| t.contains("[Parameters]"))
    }) {
        return BTreeMap::new();
    }
    skins
        .iter()
        .filter_map(|(f, is_bundled)| {
            let key = crate::bundled::key(&f.game, &f.name, *is_bundled);
            if !keys.contains(&key) {
                return None;
            }
            let among = if *is_bundled { &bundled } else { &all };
            let declared = Declared::of_skin(among, &f.game, &f.name);
            (!declared.parameters.is_empty()).then_some((key, declared.parameters))
        })
        .collect()
}

/// The skin to load for skin `name` of `game` among `folders` with the
/// parameter values `stored`: the one its choices name for them (a skin
/// that loads and falls back on it), else `name` itself.
pub fn head(folders: &[Folder], game: &str, name: &str, stored: &Values) -> String {
    let Ok(base) = Skin::load(folders, game, name) else {
        return name.to_string();
    };
    let declared = Declared::from_ini(&base.metrics);
    let Some(variant) = declared.skin_for(&declared.resolve(stored)) else {
        return name.to_string();
    };
    match Skin::load(folders, game, variant) {
        Ok(v) if v.chain.contains(&base.chain[0]) => v.name,
        _ => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(text: &str) -> Declared {
        Declared::from_ini(&Ini::parse(text))
    }

    #[test]
    fn every_kind_is_read_with_its_default() {
        let d = declared(
            "[Parameters]\nList=Colors, Boom ,Size,Tint\n\
             [Parameter Colors]\nType=Choice\nLabel=Note colors\nDescription= How. \n\
             Choices=Vivid,Flat\nChoiceLabels=Vivid!\nSkins=,X Flat\nDefault=flat\n\
             [Parameter Boom]\nType=toggle\nDefault=1\n\
             [Parameter Size]\nType=Number\nMin=0.5\nMax=2\nStep=0.25\nDefault=1.1\nUnit=×\n\
             [Parameter Tint]\nType=Color\nDefault=#ff8800\n",
        );
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);
        let names: Vec<&str> = d.parameters.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Colors", "Boom", "Size", "Tint"]);
        let colors = d.get("Colors").unwrap();
        assert_eq!(colors.label, "Note colors");
        assert_eq!(colors.description, "How.");
        assert_eq!(colors.default, Value::Text("Flat".into()));
        let Kind::Choice(c) = &colors.kind else {
            panic!()
        };
        assert_eq!(
            (c[0].label.as_str(), c[1].label.as_str()),
            ("Vivid!", "Flat")
        );
        assert_eq!(
            (c[0].skin.as_deref(), c[1].skin.as_deref()),
            (None, Some("X Flat"))
        );
        assert_eq!(d.get("Boom").unwrap().default, Value::Bool(true));
        assert_eq!(d.get("Boom").unwrap().label, "Boom");
        let size = d.get("Size").unwrap();
        assert_eq!(size.default, Value::Number(1.0));
        assert_eq!(size.numbers(), [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0]);
        let tint = d.get("Tint").unwrap();
        assert_eq!(tint.default, Value::Text("#FF8800".into()));
        let Kind::Color(palette) = &tint.kind else {
            panic!()
        };
        // The default is put first in the standard palette.
        assert_eq!(palette.len(), PALETTE.len() + 1);
        assert_eq!(palette[0].id, "#FF8800");
    }

    #[test]
    fn values_are_only_ever_what_a_declaration_takes() {
        let d = declared(
            "[Parameters]\nList=C,T,N,K\n[Parameter C]\nType=Choice\nChoices=A,B\n\
             [Parameter T]\nType=Toggle\n[Parameter N]\nType=Number\nMin=0\nMax=10\nStep=2\n\
             [Parameter K]\nType=Color\nDefault=#000000\nChoices=#000000,#FFFFFF\n",
        );
        let stored: Values = [
            ("C", Value::Text("b".into())),
            ("T", Value::Number(1.0)),
            ("N", Value::Number(7.2)),
            ("K", Value::Text("#abcdef".into())),
            ("Unknown", Value::Bool(true)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let v = d.resolve(&stored);
        assert_eq!(v.len(), 4);
        assert_eq!(v["C"], Value::Text("B".into()));
        // A toggle stored as a number is not a toggle: its default.
        assert_eq!(v["T"], Value::Bool(false));
        assert_eq!(v["N"], Value::Number(8.0));
        // A color off the palette is kept (a hand edit, another version's).
        assert_eq!(v["K"], Value::Text("#ABCDEF".into()));
        let n = d.get("N").unwrap();
        assert_eq!(n.accept(&Value::Number(99.0)), Some(Value::Number(10.0)));
        assert_eq!(n.accept(&Value::Number(f64::NAN)), None);
        assert_eq!(d.get("K").unwrap().accept(&Value::Text("red".into())), None);
        assert_eq!(d.resolve(&Values::new())["C"], Value::Text("A".into()));
    }

    #[test]
    fn bad_declarations_are_left_out_with_a_warning() {
        let many: Vec<String> = (0..40).map(|i| format!("P{i}")).collect();
        let mut text = format!("[Parameters]\nList={},bad name,P1\n", many.join(","));
        for p in &many {
            text.push_str(&format!("[Parameter {p}]\nType=Toggle\n"));
        }
        let d = declared(&text);
        assert_eq!(d.parameters.len(), MAX_PARAMETERS);
        assert!(d.warnings.iter().any(|w| w.contains("more than")));
        // A long list of bad names: the warnings are bounded.
        let bad = vec!["no good"; 10_000].join(",");
        let d = declared(&format!("[Parameters]\nList={bad}\n"));
        assert_eq!(d.warnings.len(), MAX_WARNINGS);
        for (text, why) in [
            ("[Parameter X]\nType=Slider\n", "Type is"),
            ("[Parameter X]\n", "no [Parameter X] Type"),
            ("[Parameter X]\nType=Choice\n", "no Choices"),
            ("[Parameter X]\nType=Choice\nChoices=a,A\n", "twice"),
            (
                "[Parameter X]\nType=Choice\nChoices=a b\n",
                "not a choice's id",
            ),
            ("[Parameter X]\nType=Number\nMin=0\n", "Min and a Max"),
            ("[Parameter X]\nType=Number\nMin=0\nMax=1\nStep=0\n", "Step"),
            ("[Parameter X]\nType=Number\nMin=0\nMax=1e9\n", "at most"),
            ("[Parameter X]\nType=Color\nDefault=red\n", "#RRGGBB"),
            (
                "[Parameter X]\nType=Color\nDefault=#000000\nChoices=#00\n",
                "not a choice",
            ),
        ] {
            let d = declared(&format!("[Parameters]\nList=X\n{text}"));
            assert!(d.parameters.is_empty(), "{text}");
            assert!(d.warnings[0].contains(why), "{text}: {:?}", d.warnings);
        }
        // Long texts are cut; a skin with no list declares nothing.
        let d = declared(&format!(
            "[Parameters]\nList=X\n[Parameter X]\nType=Toggle\nLabel={}\nDescription={}\n",
            "l".repeat(500),
            "d".repeat(5000)
        ));
        assert_eq!(d.parameters[0].label.len(), MAX_LABEL);
        assert_eq!(d.parameters[0].description.len(), MAX_DESCRIPTION);
        assert_eq!(
            declared("[Parameter X]\nType=Toggle\n"),
            Declared::default()
        );
    }

    fn folder(name: &str, metrics: &str) -> Folder {
        Folder {
            game: "dance".into(),
            name: name.into(),
            files: vec!["metrics.ini".into()],
            texts: vec![("metrics.ini".into(), metrics.into())],
        }
    }

    #[test]
    fn a_choice_loads_the_skin_it_names_when_that_falls_back_on_this_one() {
        let base = "[Parameters]\nList=Look\n[Parameter Look]\nType=Choice\n\
                    Choices=Plain,Gold,Other,Gone\nSkins=,Base Gold,Other,Missing\n";
        let folders = vec![
            folder("Base", base),
            folder("Base Gold", "[Global]\nFallbackNoteSkin=Base\n"),
            folder("Other", ""),
        ];
        let pick = |look: &str| {
            let stored: Values = [("Look".to_string(), Value::Text(look.into()))].into();
            head(&folders, "dance", "Base", &stored)
        };
        assert_eq!(pick("Plain"), "Base");
        assert_eq!(pick("gold"), "Base Gold");
        // Not one of its own: the skin itself.
        assert_eq!(pick("Other"), "Base");
        assert_eq!(pick("Gone"), "Base");
        assert_eq!(head(&folders, "dance", "Nope", &Values::new()), "Nope");
        // The variant declares what its base declares.
        let d = Declared::of_skin(&folders, "dance", "Base Gold");
        assert_eq!(d.parameters.len(), 1);
    }

    #[test]
    fn the_catalog_has_the_listed_skins_that_declare_some() {
        let folders = [
            folder("Base", "[Parameters]\nList=B\n[Parameter B]\nType=Toggle\n"),
            folder("Plain", ""),
            folder("On base", "[Global]\nFallbackNoteSkin=Base\n"),
        ];
        let keys: Vec<String> = ["bundled:dance/base", "dance/plain", "dance/on base"]
            .into_iter()
            .map(str::to_string)
            .collect();
        // Base bundled, the others the player's (falling back on it).
        let skins = folders.iter().zip([true, false, false]);
        let c = catalog(skins.clone(), &keys);
        assert_eq!(
            c.keys().collect::<Vec<_>>(),
            ["bundled:dance/base", "dance/on base"]
        );
        // Unlisted: left out.
        assert!(catalog(skins, &keys[1..2]).is_empty());
    }

    #[test]
    fn values_are_stored_as_plain_json() {
        let v: Values = [
            ("A".to_string(), Value::Bool(true)),
            ("B".to_string(), Value::Number(0.5)),
            ("C".to_string(), Value::Text("Vivid".into())),
        ]
        .into();
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, r#"{"A":true,"B":0.5,"C":"Vivid"}"#);
        assert_eq!(serde_json::from_str::<Values>(&json).unwrap(), v);
    }
}
