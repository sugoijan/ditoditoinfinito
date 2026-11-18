//! A pack's `announcer.lua` (`docs/plans/announcers.md`, "A pack's
//! script"): run once when its sounds are chosen, on the sandboxed Lua
//! (`ddi_lua`, Lua 5.1's globals and nothing that reaches outside), it
//! builds the pack's pool of takes ([`super::pool::Pool`]) and returns
//! it. Nothing scripted runs during a play: the session speaks the pool
//! as it speaks any pack's lines. A script that fails (an error, its fuel
//! or memory spent, a take of a file the pack lacks or of a line the game
//! does not know) is refused whole, and the pack's folders speak instead.
//!
//! ```lua
//! local pack = Announcer.pack()
//! pack:take("shared/awesome.ogg", {
//!   events = { "gameplay 100 combo", "evaluation cheer" },
//!   lang = "en",
//! })
//! pack:take("en/gameplay 200 combo/damn.ogg", { tags = { "profanity" } })
//! pack:tag("profanity", { label = "Profanity", default = true })
//! pack:languages({ "en", "ja" }, { mixed = true })
//! return pack
//! ```
//!
//! What a script reads: the pack's sound files (`Announcer.files()`,
//! sorted, `Announcer.files("en/")` those under a folder), its
//! parameters' values as `announcer.ini` declares them
//! (`Announcer.param(name)`, the Language's the tag chosen or `mixed`,
//! nothing for Same as game language) and the game's language
//! (`Announcer.language()`); nothing of the player, the play or anything
//! outside the pack.

use std::collections::BTreeSet;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_lua::lunita::{Table, Value};
use ddi_lua::{Budget, Outcome, Settings, Vm};
use ddi_noteskin::params::{Value as ParamValue, Values};

use super::Line;
use super::pool::{MAX_LANGUAGES, Tag, Take, is_language, language_tag};
use crate::path::RelPathBuf;

/// The file a pack's script is.
pub const SCRIPT: &str = "announcer.lua";
/// A script's source, at most, in bytes.
pub const MAX_SOURCE: usize = 64 << 10;
/// Fuel a script's one run may spend (about a quarter of a second of the
/// slowest work at most).
pub const FUEL: u64 = 1_000_000;
/// Bytes a script's VM may hold.
pub const MEMORY: usize = 4 << 20;
/// Its longest string.
const STRING_LEN: usize = 64 << 10;
/// Wall-clock seconds a run may take, where the shell gives a clock
/// ([`set_host`]): the backstop behind the fuel.
pub const DEADLINE: f64 = 0.25;
/// Takes a pool may hold, at most.
pub const MAX_TAKES: usize = 8000;
/// Lines a take may be said for, at most.
pub const MAX_EVENTS: usize = 64;
/// Tags a take may carry, and switchable tags a pack may declare.
pub const MAX_TAGS: usize = 16;
/// Characters of a tag's name (letters, digits, `_`, `-`).
pub const MAX_TAG_NAME: usize = 24;
/// What a take's weight may be, at most.
pub const MAX_WEIGHT: f64 = 100.0;

/// The prelude a script runs inside: `Announcer`, the pack it builds,
/// and the pool handed back as records (`take`, `tag`, `languages`, the
/// fields apart by NUL, a list's items by US) that Rust reads strictly.
const PRELUDE: &str = r#"
local script, files, values, game = ...
local concat, ipairs, type, tostring, error, setmetatable, select =
  table.concat, ipairs, type, tostring, error, setmetatable, select
local SEP, LIST = "\0", "\31"
local function str(v, what)
  if type(v) ~= "string" then error(what .. " is not a string", 3) end
  return v
end
local function strings(v, what)
  if v == nil then return {} end
  if type(v) == "string" then return { v } end
  if type(v) ~= "table" then error(what .. " is not a list", 3) end
  local out = {}
  for i, s in ipairs(v) do out[i] = str(s, what) end
  return out
end
local takes, tags, langs, made = {}, {}, nil, nil
local Pack = {}
Pack.__index = Pack
function Pack:take(file, o)
  str(file, "a take's file")
  if o == nil then o = {} end
  if type(o) ~= "table" then error("a take's options are not a table", 2) end
  local w = o.weight
  if w ~= nil and type(w) ~= "number" then error("a take's weight is not a number", 2) end
  if o.lang ~= nil then str(o.lang, "a take's language") end
  takes[#takes + 1] = concat({ "take", file, concat(strings(o.events, "an event"), LIST),
    o.lang or "", concat(strings(o.tags, "a tag"), LIST), tostring(w or 1) }, SEP)
  return self
end
function Pack:tag(name, o)
  str(name, "a tag's name")
  if o == nil then o = {} end
  if type(o) ~= "table" then error("a tag's options are not a table", 2) end
  local default = o.default
  if default == nil then default = true end
  if default == "on" then default = true elseif default == "off" then default = false end
  if type(default) ~= "boolean" then error("a tag's default is true or false", 2) end
  tags[#tags + 1] = concat({ "tag", name, o.label == nil and name or str(o.label, "a tag's label"),
    default and "1" or "0" }, SEP)
  return self
end
function Pack:languages(list, o)
  if o ~= nil and type(o) ~= "table" then error("the languages' options are not a table", 2) end
  langs = concat({ "languages", concat(strings(list, "a language"), LIST),
    (o and o.mixed) and "1" or "0" }, SEP)
  return self
end
Announcer = {
  pack = function()
    if made == nil then made = setmetatable({}, Pack) end
    return made
  end,
  files = function(prefix)
    local out = {}
    for _, f in ipairs(files) do
      if prefix == nil or f:sub(1, #prefix) == prefix then out[#out + 1] = f end
    end
    return out
  end,
  param = function(name) return values[name] end,
  language = function() return game end,
}
local result = script()
if made == nil or result ~= made then
  error("announcer.lua must return its pack (Announcer.pack())", 0)
end
local out = {}
for i = 1, #takes do out[#out + 1] = takes[i] end
for i = 1, #tags do out[#out + 1] = tags[i] end
if langs then out[#out + 1] = langs end
return out
"#;

/// What a script built.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scripted {
    pub takes: Vec<Take>,
    pub tags: Vec<Tag>,
    /// The languages it offers, in order, and whether mixed (`None`: it
    /// did not say).
    pub languages: Option<(Vec<String>, bool)>,
}

/// The shell's clock and a seed only it knows, for scripts' runs: the
/// deadline behind the fuel, and the hashing a script cannot aim
/// collisions at (`ddi_lua::Settings::hash_seed`). Without them (tests)
/// fuel alone bounds a run.
static CLOCK: OnceLock<fn() -> f64> = OnceLock::new();
static SEED: AtomicU64 = AtomicU64::new(0);

/// Gives scripts' runs the shell's clock (seconds) and a seed of its
/// randomness; once a run of the game.
pub fn set_host(now: fn() -> f64, seed: u64) {
    let _ = CLOCK.set(now);
    SEED.store(seed, Ordering::Relaxed);
}

/// Whether `s` is a tag's name: letters, digits, `_` and `-`, at most
/// [`MAX_TAG_NAME`].
pub fn is_tag(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_TAG_NAME
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Runs `source` over the pack's sound files `files` (paths in it), its
/// parameters' `values` and the game's language `game`; what it built,
/// or why it was refused.
pub fn run(source: &str, files: &[String], values: &Values, game: &str) -> Result<Scripted, String> {
    if source.len() > MAX_SOURCE {
        return Err(format!("{SCRIPT} is longer than {} KB", MAX_SOURCE >> 10));
    }
    let mut vm = Vm::new(Settings {
        memory: MEMORY,
        string_len: STRING_LEN,
        source_len: MAX_SOURCE,
        random_seed: 0,
        hash_seed: SEED.load(Ordering::Relaxed),
        ..Settings::default()
    });
    let prelude = vm
        .load("=announcer prelude", PRELUDE.as_bytes())
        .map_err(|e| e.to_string())?;
    let script = vm
        .load(&format!("@{SCRIPT}"), source.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut task = vm.start(&prelude, |ctx| {
        let list = Table::new(&ctx);
        for (i, f) in files.iter().enumerate() {
            let _ = list.set(ctx, i as i64 + 1, ctx.intern(f.as_bytes()));
        }
        let params = Table::new(&ctx);
        for (name, v) in values {
            let v: Value = match v {
                ParamValue::Bool(b) => Value::Boolean(*b),
                ParamValue::Number(n) => Value::Number(*n),
                ParamValue::Text(t) => Value::String(ctx.intern(t.as_bytes())),
            };
            let _ = params.set(ctx, ctx.intern(name.as_bytes()), v);
        }
        vec![
            Value::Function(ctx.fetch(&script)),
            Value::Table(list),
            Value::Table(params),
            Value::String(ctx.intern(game.as_bytes())),
        ]
    });
    let clock = CLOCK.get().copied();
    let start = clock.map_or(0.0, |c| c());
    let budget = Budget {
        fuel: FUEL,
        deadline: if clock.is_some() {
            start + DEADLINE
        } else {
            f64::INFINITY
        },
    };
    let mut now = || clock.map_or(0.0, |c| c());
    match vm.run(&mut task, budget, &mut now) {
        Outcome::Done => {}
        Outcome::Paused => return Err(format!("{SCRIPT} used too much fuel")),
        Outcome::Failed(f) => return Err(f.to_string()),
    }
    let records: Vec<Vec<u8>> = {
        let results = task.results().to_vec();
        vm.lua().enter(|ctx| {
            let Some(Value::Table(t)) = results.first().map(|r| ctx.fetch(r)) else {
                return Err("its prelude returned nothing".to_string());
            };
            let mut out = Vec::new();
            for i in 1.. {
                match t.get_value(ctx, i) {
                    Value::Nil => break,
                    Value::String(s) => out.push(s.as_bytes().to_vec()),
                    _ => return Err("its prelude returned something else".to_string()),
                }
                if out.len() > MAX_TAKES + MAX_TAGS + 1 {
                    return Err(format!("more than {MAX_TAKES} takes"));
                }
            }
            Ok(out)
        })?
    };
    read(&records, files)
}

/// The records a run handed back, read and checked.
fn read(records: &[Vec<u8>], files: &[String]) -> Result<Scripted, String> {
    let known: BTreeSet<&str> = files.iter().map(String::as_str).collect();
    let mut out = Scripted::default();
    for record in records {
        let record = std::str::from_utf8(record).map_err(|_| "a name is not UTF-8".to_string())?;
        let fields: Vec<&str> = record.split('\0').collect();
        let list = |s: &str| -> Vec<String> {
            s.split('\u{1f}')
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect()
        };
        match fields.as_slice() {
            ["take", file, events, lang, tags, weight] => {
                out.takes.push(take(&known, file, &list(events), lang, &list(tags), weight)?);
                if out.takes.len() > MAX_TAKES {
                    return Err(format!("more than {MAX_TAKES} takes"));
                }
            }
            ["tag", name, label, default] => {
                if !is_tag(name) {
                    return Err(format!("\"{}\" is not a tag's name", cut(name)));
                }
                if out.tags.iter().any(|t| t.name == *name) {
                    return Err(format!("tag {name} is declared twice"));
                }
                if out.tags.len() == MAX_TAGS {
                    return Err(format!("more than {MAX_TAGS} tags"));
                }
                out.tags.push(Tag {
                    name: name.to_string(),
                    label: label
                        .chars()
                        .take(ddi_noteskin::params::MAX_LABEL)
                        .collect(),
                    default: *default == "1",
                });
            }
            ["languages", langs, mixed] => {
                let mut tags = Vec::new();
                for l in list(langs) {
                    if !is_language(&l) {
                        return Err(format!("\"{}\" is not a language's tag", cut(&l)));
                    }
                    let l = language_tag(&l);
                    if !tags.contains(&l) {
                        tags.push(l);
                    }
                }
                if tags.len() > MAX_LANGUAGES {
                    return Err(format!("more than {MAX_LANGUAGES} languages"));
                }
                out.languages = Some((tags, *mixed == "1"));
            }
            _ => return Err("a name holds a character no name may".to_string()),
        }
    }
    // A switchable tag no take carries is no row; one a take carries is
    // only switchable when declared.
    Ok(out)
}

/// `s`, cut for a message.
fn cut(s: &str) -> String {
    s.chars().take(48).collect()
}

/// A take, checked: its file one of the pack's, each event a line the
/// game knows (its folder's where it names none), its language a tag
/// (its first folder's where it names none and that is one), its tags
/// names.
fn take(
    known: &BTreeSet<&str>,
    file: &str,
    events: &[String],
    lang: &str,
    tags: &[String],
    weight: &str,
) -> Result<Take, String> {
    if !known.contains(file) {
        return Err(format!("\"{}\" is no sound of this pack", cut(file)));
    }
    let path = RelPathBuf::from(file);
    let mut lines = Vec::new();
    if events.is_empty() {
        let folder = path.parent().file_name().to_string();
        let line = Line::parse(&folder)
            .ok_or_else(|| format!("{file}: names no line, and its folder is none"))?;
        lines.push(line);
    }
    for e in events {
        let line = Line::parse(e).ok_or_else(|| format!("\"{}\" is not a line", cut(e)))?;
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    if lines.len() > MAX_EVENTS {
        return Err(format!("{file}: more than {MAX_EVENTS} lines"));
    }
    let lang = if lang.is_empty() {
        file.split('/')
            .next()
            .filter(|first| file.contains('/') && is_language(first))
            .map(language_tag)
    } else if is_language(lang) {
        Some(language_tag(lang))
    } else {
        return Err(format!("\"{}\" is not a language's tag", cut(lang)));
    };
    if tags.len() > MAX_TAGS {
        return Err(format!("{file}: more than {MAX_TAGS} tags"));
    }
    if let Some(t) = tags.iter().find(|t| !is_tag(t)) {
        return Err(format!("\"{}\" is not a tag's name", cut(t)));
    }
    let weight: f64 = weight
        .parse()
        .ok()
        .filter(|w: &f64| w.is_finite() && (0.0..=MAX_WEIGHT).contains(w))
        .ok_or_else(|| format!("{file}: a weight is from 0 to {MAX_WEIGHT}"))?;
    Ok(Take {
        file: path,
        events: lines,
        lang,
        tags: tags.to_vec(),
        weight,
    })
}
