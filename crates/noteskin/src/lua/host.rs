//! The host functions `prelude.lua` wraps (its `H`), and the actors'
//! native methods. Each charges fuel for its work beyond the callback's
//! own cost; none reaches anything but the skin's folders.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::commands::{Arg, Call, color_from_string, to_lua};
use ddi_actor::{METHODS, Problems, Value as Got};
use ddi_lua::lunita::{
    Callback, CallbackReturn, Context, Error, Execution, Table, UserData, Value,
};

use super::{Host, Registries};
use crate::ini::string_to_float;

/// The actor a userdata stands for.
pub(crate) struct ActorRef(pub u32);

/// A render target's texture a userdata stands for
/// (`ActorFrameTexture:GetTexture`), by its index.
pub(crate) struct TextureRef(pub u32);

/// The pixels a song's render targets may hold, all together: ten
/// screens' worth (`ActorFrameTexture`s are mostly screen-sized).
pub const MAX_TARGET_PIXELS: u64 = 4 << 20;

/// The largest side a render target may have.
pub const MAX_TARGET_SIDE: u32 = 4096;

/// Actor classes `Def` accepts (`ActorUtil::IsRegisteredClass`, the ones
/// StepMania registers that a skin could name).
const CLASSES: &[&str] = &[
    "Actor",
    "ActorFrame",
    "Sprite",
    "Quad",
    "Model",
    "BitmapText",
    "Sound",
    "BGAnimation",
    "ActorFrameTexture",
    "ActorMultiVertex",
    "ActorProxy",
    "ActorScroller",
    "RollingNumbers",
];

/// Getters (`Actor::getter`).
const GETTERS: &[&str] = &[
    "GetName",
    "GetX",
    "GetY",
    "GetZ",
    "GetZoom",
    "GetZoomX",
    "GetZoomY",
    "GetZoomZ",
    "GetRotationX",
    "GetRotationY",
    "GetRotationZ",
    "GetCropLeft",
    "GetCropTop",
    "GetCropRight",
    "GetCropBottom",
    "GetBaseZoomX",
    "GetBaseZoomY",
    "GetBaseZoomZ",
    "GetBaseRotationX",
    "GetBaseRotationY",
    "GetBaseRotationZ",
    "GetDiffuse",
    "GetDiffuseAlpha",
    "GetGlow",
    "GetVisible",
    "GetWidth",
    "GetHeight",
    "GetZoomedWidth",
    "GetZoomedHeight",
    "GetTweenTimeLeft",
    "GetSecsIntoEffect",
    "GetEffectDelta",
    "GetEffectMagnitude",
    "GetHAlign",
    "GetVAlign",
    "GetNumStates",
    "GetState",
    "GetAnimationLengthSeconds",
    "GetNumChildren",
];

/// Methods only Models have (`Model.cpp:783-803`), run natively.
const MODEL_ONLY: &[&str] = &[
    "rate",
    "playanimation",
    "loop",
    "SetDefaultAnimation",
    "position",
];

/// `PREFSMAN:GetPreference` for what skins read; anything else is nil.
fn preference<'gc>(ctx: Context<'gc>, name: &str) -> Value<'gc> {
    match name {
        // The game applies the offsets itself; skins see none.
        "GlobalOffsetSeconds" | "VisualDelaySeconds" => Value::Number(0.0),
        "Center1Player" => Value::Boolean(false),
        "EasterEggs" => Value::Boolean(false),
        "ShowNativeLanguage" => Value::Boolean(true),
        "Theme" => Value::String(ctx.intern(b"default")),
        _ => Value::Nil,
    }
}

pub(crate) fn error<'gc>(ctx: Context<'gc>, message: impl AsRef<str>) -> Error<'gc> {
    Value::String(ctx.intern(message.as_ref().as_bytes())).into()
}

fn string(v: Value<'_>) -> Option<String> {
    match v {
        Value::String(s) => Some(s.display_lossy().to_string()),
        Value::Integer(_) | Value::Number(_) => Some(v.display().to_string()),
        _ => None,
    }
}

fn str_arg<'gc>(
    ctx: Context<'gc>,
    stack: &ddi_lua::lunita::Stack<'gc, '_>,
    i: usize,
    what: &str,
) -> Result<String, Error<'gc>> {
    string(stack.get(i))
        .ok_or_else(|| error(ctx, format!("{what}: argument {} must be a string", i + 1)))
}

/// What reading the timing costs, in bytes' worth of fuel: its maths
/// walks every segment.
fn segments(t: &ddi_chart::timing::TimingMap) -> usize {
    (t.bpms.len() + t.stops.len() + t.delays.len() + t.warps.len()) * 16
}

fn charge(exec: &mut Execution<'_, '_>, bytes: usize) {
    let fuel = i32::try_from(4 + bytes / 64).unwrap_or(i32::MAX);
    exec.fuel().consume(fuel);
}

/// The longest string an actor method takes.
const MAX_ARG: usize = 256;

/// A Lua value as an actor method's argument.
fn arg<'gc>(ctx: Context<'gc>, v: Value<'gc>, method: &str, i: usize) -> Result<Arg, Error<'gc>> {
    Ok(match v {
        Value::Nil => Arg::Nil,
        Value::Boolean(b) => Arg::Bool(b),
        Value::Integer(n) => Arg::Num(n as f32),
        Value::Number(n) => Arg::Num(n as f32),
        // Actor methods take short strings (command names, enum values);
        // a long one would be kept per tween.
        Value::String(s) if s.as_bytes().len() > MAX_ARG => {
            return Err(error(
                ctx,
                format!("{method}: argument {i} is longer than {MAX_ARG} bytes"),
            ));
        }
        Value::String(s) => Arg::Str(s.display_lossy().to_string()),
        // `RageColor::FromStack`: the table's first four entries.
        Value::Table(t) => {
            let mut c = [1.0f32; 4];
            for (k, slot) in c.iter_mut().enumerate() {
                if let Some(n) = t.get_value(ctx, k as i64 + 1).to_number() {
                    *slot = n as f32;
                }
            }
            Arg::Color(c)
        }
        other => {
            return Err(error(
                ctx,
                format!("{method}: argument {i} cannot be a {}", other.type_name()),
            ));
        }
    })
}

fn got<'gc>(ctx: Context<'gc>, v: Got, out: &mut Vec<Value<'gc>>) {
    match v {
        Got::Num(n) => out.push(Value::Number(f64::from(n))),
        Got::Str(s) => out.push(Value::String(ctx.intern(s.as_bytes()))),
        Got::Bool(b) => out.push(Value::Boolean(b)),
        Got::Color(c) => {
            let t = Table::new(&ctx);
            for (i, x) in c.iter().enumerate() {
                let _ = t.set(ctx, i as i64 + 1, f64::from(*x));
            }
            out.push(Value::Table(t));
        }
        Got::Many(vs) => {
            for v in vs {
                got(ctx, v, out);
            }
        }
    }
}

fn actor_id<'gc>(ctx: Context<'gc>, v: Value<'gc>) -> Result<u32, Error<'gc>> {
    match v {
        Value::UserData(ud) => ud
            .downcast_static::<ActorRef>()
            .map(|a| a.0)
            .map_err(|_| error(ctx, "not an actor")),
        _ => Err(error(
            ctx,
            "calling an actor method on something that is not an actor",
        )),
    }
}

/// The host table `H` and the registries the converter fills.
pub(crate) fn table<'gc>(ctx: Context<'gc>, host: &Rc<RefCell<Host>>) -> (Table<'gc>, Registries) {
    let h = Table::new(&ctx);
    let set = |name: &'static str, v: Value<'gc>| {
        let _ = h.set(ctx, name, v);
    };
    let func = |f: Callback<'gc>| Value::Function(f.into());

    let shared = host.clone();
    set(
        "warn",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let s = string(stack.get(0)).unwrap_or_default();
            charge(&mut exec, s.len());
            shared.borrow_mut().warn(s);
            stack.clear();
            let _ = ctx;
            Ok(CallbackReturn::Return)
        })),
    );

    set(
        "color",
        func(Callback::from_fn(&ctx, |ctx, mut exec, mut stack| {
            let s = string(stack.get(0)).unwrap_or_default();
            charge(&mut exec, s.len());
            let c = color_from_string(&s);
            let t = Table::new(&ctx);
            for (i, x) in c.iter().enumerate() {
                let _ = t.set(ctx, i as i64 + 1, f64::from(*x));
            }
            stack.replace(ctx, t);
            Ok(CallbackReturn::Return)
        })),
    );

    set(
        "is_class",
        func(Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let c = string(stack.get(0)).unwrap_or_default();
            stack.replace(ctx, CLASSES.contains(&c.as_str()));
            Ok(CallbackReturn::Return)
        })),
    );

    // The source, line and folder of the nearest skin file up the stack
    // (`Def`'s `debug.getinfo` loop).
    set(
        "caller",
        func(Callback::from_fn(&ctx, |ctx, exec, mut stack| {
            stack.clear();
            for level in 1..64 {
                let Some(frame) = exec.lua_frame(level) else {
                    continue;
                };
                let name = frame.chunk_name.display_lossy().to_string();
                if !name.starts_with(crate::skin::SKINS_ROOT)
                    && !name.starts_with(crate::skin::SONGS_ROOT)
                {
                    continue;
                }
                let dir = name.rfind('/').map_or("/", |i| &name[..=i]).to_string();
                stack.push_back(Value::String(ctx.intern(format!("@{name}").as_bytes())));
                stack.push_back(Value::Integer(frame.current_line.0 as i64 + 1));
                stack.push_back(Value::String(ctx.intern(dir.as_bytes())));
                break;
            }
            Ok(CallbackReturn::Return)
        })),
    );

    let shared = host.clone();
    set(
        "resolve",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let path = str_arg(ctx, &stack, 0, "ResolvePath")?;
            let optional = stack.get(1).to_bool();
            charge(&mut exec, path.len() * 4);
            let mut host = shared.borrow_mut();
            let r = host.skin.resolve_path(&path);
            match r {
                Ok(f) => {
                    let p = host.skin.file_path(&f);
                    stack.replace(ctx, ctx.intern(p.as_bytes()));
                }
                Err(e) => {
                    if !optional {
                        host.warn(e);
                    }
                    stack.replace(ctx, Value::Nil);
                }
            }
            Ok(CallbackReturn::Return)
        })),
    );

    let shared = host.clone();
    set(
        "file_type",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let path = str_arg(ctx, &stack, 0, "GetFileType")?;
            charge(&mut exec, path.len());
            let lower = path.to_ascii_lowercase();
            let ext = lower.rsplit_once('.').map_or("", |(_, e)| e);
            let t = match ext {
                "lua" => "FileType_Lua",
                "xml" => "FileType_Xml",
                "ini" => "FileType_Ini",
                _ if crate::is_image(&path) => "FileType_Bitmap",
                "mp3" | "oga" | "ogg" | "wav" => "FileType_Sound",
                "avi" | "f4v" | "flv" | "mkv" | "mp4" | "mpeg" | "mpg" | "mov" | "ogv" | "webm"
                | "wmv" => "FileType_Movie",
                "sprite" => "FileType_Sprite",
                "txt" => "FileType_Model",
                _ if path.ends_with('/') => "FileType_Directory",
                _ => {
                    let host = shared.borrow();
                    match host.skin.resolve_path(&path) {
                        Ok(f) if f.is_dir => "FileType_Directory",
                        _ => "FileType_Invalid",
                    }
                }
            };
            stack.replace(ctx, ctx.intern(t.as_bytes()));
            Ok(CallbackReturn::Return)
        })),
    );

    // `loadfile` over the skin's own files: the chunk, or nil and why.
    let shared = host.clone();
    set(
        "loadfile",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let path = str_arg(ctx, &stack, 0, "loadfile")?;
            charge(&mut exec, path.len() * 4);
            let host = shared.borrow();
            let found = host.skin.resolve_path(&path).and_then(|f| {
                host.skin
                    .lua_text(&f)
                    .map(|t| (host.skin.file_path(&f), t.to_string()))
                    .ok_or_else(|| format!("cannot open {path}"))
            });
            let settings = host.settings;
            drop(host);
            stack.clear();
            match found {
                Ok((name, text)) => {
                    match ddi_lua::compile(ctx, &settings, &format!("@{name}"), text.as_bytes()) {
                        Ok(f) => stack.push_back(Value::Function(f)),
                        Err(ddi_lua::Failure::Limit(l)) => {
                            return Err(ddi_lua::lunita::RuntimeError::new(l).into());
                        }
                        Err(e) => {
                            stack.push_back(Value::Nil);
                            stack.push_back(Value::String(ctx.intern(e.to_string().as_bytes())));
                        }
                    }
                }
                Err(e) => {
                    stack.push_back(Value::Nil);
                    stack.push_back(Value::String(ctx.intern(e.as_bytes())));
                }
            }
            Ok(CallbackReturn::Return)
        })),
    );

    let shared = host.clone();
    set(
        "get_path",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let button = str_arg(ctx, &stack, 0, "GetPath")?;
            let element = str_arg(ctx, &stack, 1, "GetPath")?;
            charge(&mut exec, (button.len() + element.len()) * 8);
            let mut host = shared.borrow_mut();
            let path = match host.skin.get_path(&button, &element) {
                Ok(f) => host.skin.file_path(&f),
                Err(e) => {
                    // StepMania reports it and returns "".
                    host.warn(format!("{button} {element}: {e}"));
                    String::new()
                }
            };
            stack.replace(ctx, ctx.intern(path.as_bytes()));
            Ok(CallbackReturn::Return)
        })),
    );

    // `NoteSkinManager::GetMetric`: `[button]`, then `[NoteDisplay]`;
    // reported and "" when in neither.
    fn metric(host: &mut Host, section: &str, name: &str) -> String {
        match host.skin.metric(section, name) {
            Some(m) => m.to_string(),
            None => {
                host.warn(format!(
                    "Could not read metric \"{section}::{name}\" or \"NoteDisplay::{name}\""
                ));
                String::new()
            }
        }
    }

    let shared = host.clone();
    set(
        "metric",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let section = str_arg(ctx, &stack, 0, "GetMetric")?;
            let name = str_arg(ctx, &stack, 1, "GetMetric")?;
            let m = metric(&mut shared.borrow_mut(), &section, &name);
            charge(&mut exec, m.len());
            stack.replace(ctx, ctx.intern(m.as_bytes()));
            Ok(CallbackReturn::Return)
        })),
    );

    let shared = host.clone();
    set(
        "metric_number",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let section = str_arg(ctx, &stack, 0, "GetMetric")?;
            let name = str_arg(ctx, &stack, 1, "GetMetric")?;
            let int = string(stack.get(2)).as_deref() == Some("i");
            let m = metric(&mut shared.borrow_mut(), &section, &name);
            charge(&mut exec, m.len());
            let v = f64::from(string_to_float(&m));
            stack.replace(
                ctx,
                if int {
                    Value::Integer(v as i64)
                } else {
                    Value::Number(v)
                },
            );
            Ok(CallbackReturn::Return)
        })),
    );

    // The metric compiled as `ParseCommandList` compiles it; the chunk
    // returns the command's function.
    let shared = host.clone();
    let shared2 = host.clone();
    set(
        "metric_function",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let section = str_arg(ctx, &stack, 0, "GetMetricA")?;
            let name = str_arg(ctx, &stack, 1, "GetMetricA")?;
            let mut host = shared.borrow_mut();
            let m = metric(&mut host, &section, &name);
            let settings = host.settings;
            drop(host);
            let source = to_lua(&m);
            charge(&mut exec, source.len());
            match ddi_lua::compile(
                ctx,
                &settings,
                &format!("={section}::{name}"),
                source.as_bytes(),
            ) {
                Ok(f) => {
                    stack.replace(ctx, Value::Function(f));
                    Ok(CallbackReturn::Return)
                }
                Err(ddi_lua::Failure::Limit(l)) => {
                    Err(ddi_lua::lunita::RuntimeError::new(l).into())
                }
                // StepMania warns and the command is nil.
                Err(e) => {
                    shared2
                        .borrow_mut()
                        .warn(format!("compiling [{section}] {name}: {e}"));
                    stack.replace(ctx, Value::Nil);
                    Ok(CallbackReturn::Return)
                }
            }
        })),
    );

    // A song's folder as its scripts name it (`Song::GetSongDir`), and
    // StepMania's screen for them (`SCREEN_WIDTH`, `SCREEN_HEIGHT`).
    let shared = host.clone();
    set(
        "song_dir",
        func(Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let host = shared.borrow();
            let dir = host.skin.chain.first().map(|&d| host.skin.dir_path(d));
            stack.replace(ctx, ctx.intern(dir.unwrap_or_default().as_bytes()));
            Ok(CallbackReturn::Return)
        })),
    );
    // `ActorSound::play`: the actor's file noted for the shell to play;
    // whether it is a `Sound` actor.
    let shared = host.clone();
    set(
        "sound",
        func(Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let id = actor_id(ctx, stack.get(0))?;
            let mut host = shared.borrow_mut();
            let file = host.sounds.get(&id).cloned();
            let is = file.is_some();
            if let Some(f) = file
                && host.played.len() < 256
            {
                host.played.push(f);
            }
            stack.replace(ctx, is);
            Ok(CallbackReturn::Return)
        })),
    );
    let shared = host.clone();
    set(
        "screen",
        func(Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let [w, h] = shared.borrow().env.screen;
            stack.replace(ctx, (w, h));
            Ok(CallbackReturn::Return)
        })),
    );

    let shared = host.clone();
    set(
        "skin_name",
        func(Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let name = shared.borrow().skin.name.clone();
            stack.replace(ctx, ctx.intern(name.as_bytes()));
            Ok(CallbackReturn::Return)
        })),
    );

    let style = {
        let host = host.borrow();
        let (style_type, steps_type) = host.env.style_types();
        let t = Table::new(&ctx);
        let _ = t.set(ctx, "columns", i64::from(host.env.columns));
        let _ = t.set(ctx, "style_type", ctx.intern(style_type.as_bytes()));
        let _ = t.set(ctx, "steps_type", ctx.intern(steps_type.as_bytes()));
        let _ = t.set(ctx, "name", ctx.intern(host.env.style.as_bytes()));
        let _ = t.set(ctx, "game", ctx.intern(host.env.game.as_bytes()));
        t
    };
    set(
        "style",
        func(Callback::from_fn_with(
            &ctx,
            style,
            |style, ctx, _, mut stack| {
                stack.replace(ctx, *style);
                Ok(CallbackReturn::Return)
            },
        )),
    );

    let shared = host.clone();
    set(
        "player_options",
        func(Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let o = shared.borrow().env.player_options.clone();
            stack.replace(ctx, ctx.intern(o.as_bytes()));
            Ok(CallbackReturn::Return)
        })),
    );

    let shared = host.clone();
    set(
        "difficulty",
        func(Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            let d = shared.borrow().env.difficulty.clone();
            stack.replace(ctx, ctx.intern(d.as_bytes()));
            Ok(CallbackReturn::Return)
        })),
    );

    // The song's position: the start while loading, the frame's during a
    // play (`LuaSkin::begin_frame`).
    let shared = host.clone();
    set(
        "song_position",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let kind = string(stack.get(0)).unwrap_or_default();
            let host = shared.borrow();
            let timing = &host.env.timing;
            charge(&mut exec, segments(timing));
            let (seconds, beat) = match host.position {
                Some(p) => (p.seconds, p.beat),
                None => (0.0, timing.beat_at(0.0)),
            };
            let v = match kind.as_str() {
                "seconds" => Value::Number(seconds),
                "beat" => Value::Number(beat),
                "bps" => Value::Number(timing.bpm_at_beat(beat) / 60.0),
                "lead_in" => Value::Boolean(host.position.is_none_or(|p| p.seconds < 0.0)),
                _ => Value::Nil,
            };
            stack.replace(ctx, v);
            Ok(CallbackReturn::Return)
        })),
    );

    let shared = host.clone();
    set(
        "timing",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let kind = string(stack.get(0)).unwrap_or_default();
            let x = stack.get(1).to_number().unwrap_or(0.0);
            let host = shared.borrow();
            let env = &host.env;
            charge(&mut exec, segments(&env.timing));
            // Beats and seconds a script passes may be anything; the
            // timing's maths wants finite ones.
            let x = if x.is_finite() {
                x.clamp(-1e9, 1e9)
            } else {
                0.0
            };
            let v = match kind.as_str() {
                "beat" => env.timing.beat_at(x),
                "seconds" => env.timing.seconds_at_beat(x),
                "bps" => env.timing.bpm_at_beat(x) / 60.0,
                "first_beat" => env.first_beat,
                "last_beat" => env.last_beat,
                _ => 0.0,
            };
            stack.replace(ctx, v);
            Ok(CallbackReturn::Return)
        })),
    );

    let env = Table::new(&ctx);
    set(
        "env",
        func(Callback::from_fn_with(
            &ctx,
            env,
            |env, ctx, _, mut stack| {
                stack.replace(ctx, *env);
                Ok(CallbackReturn::Return)
            },
        )),
    );

    let shared = host.clone();
    set(
        "field_reversed",
        func(Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            stack.replace(ctx, shared.borrow().env.reversed);
            Ok(CallbackReturn::Return)
        })),
    );

    // A parameter's value (our extension, `crate::params`): a copy of
    // what the host resolved at load, and its kind; nil for a name the
    // skin does not declare.
    let shared = host.clone();
    set(
        "parameter",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let name = string(stack.get(0)).unwrap_or_default();
            charge(&mut exec, name.len());
            let host = shared.borrow();
            let kind = host.declared.get(&name).map(|p| match p.kind {
                crate::params::Kind::Choice(_) => "choice",
                crate::params::Kind::Toggle => "toggle",
                crate::params::Kind::Number { .. } => "number",
                crate::params::Kind::Color(_) => "color",
            });
            let value = match host.env.params.get(&name) {
                Some(crate::params::Value::Bool(b)) => Value::Boolean(*b),
                Some(crate::params::Value::Number(n)) => Value::Number(*n),
                Some(crate::params::Value::Text(t)) => {
                    charge(&mut exec, t.len());
                    Value::String(ctx.intern(t.as_bytes()))
                }
                None => Value::Nil,
            };
            stack.clear();
            stack.push_back(value);
            stack.push_back(kind.map_or(Value::Nil, |k| Value::String(ctx.intern(k.as_bytes()))));
            Ok(CallbackReturn::Return)
        })),
    );

    let shared = host.clone();
    set(
        "trace_missing",
        func(Callback::from_fn(&ctx, move |ctx, _, mut stack| {
            stack.replace(ctx, shared.borrow().env.trace_missing);
            Ok(CallbackReturn::Return)
        })),
    );

    set(
        "preference",
        func(Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let name = string(stack.get(0)).unwrap_or_default();
            stack.replace(ctx, preference(ctx, &name));
            Ok(CallbackReturn::Return)
        })),
    );

    // Actors.
    let actors = Table::new(&ctx);
    let commands = Table::new(&ctx);
    let children = Table::new(&ctx);
    set("actors", Value::Table(actors));
    set("commands", Value::Table(commands));
    set("children", Value::Table(children));

    set(
        "actor_id",
        func(Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let id = actor_id(ctx, stack.get(0))?;
            stack.replace(ctx, i64::from(id));
            Ok(CallbackReturn::Return)
        })),
    );

    let methods = Table::new(&ctx);
    for &m in METHODS {
        let _ = methods.set(ctx, m, native_method(ctx, host, m));
    }
    for &g in GETTERS {
        let shared = host.clone();
        let _ = methods.set(
            ctx,
            g,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let id = actor_id(ctx, stack.get(0))?;
                let v = shared
                    .borrow()
                    .stage
                    .get(id)
                    .and_then(|a| a.getter(g))
                    .ok_or_else(|| error(ctx, format!("{g}: the actor is gone")))?;
                let mut out = Vec::new();
                got(ctx, v, &mut out);
                stack.clear();
                for v in out {
                    stack.push_back(v);
                }
                Ok(CallbackReturn::Return)
            }),
        );
    }
    // `name`: sets the actor's name.
    let shared = host.clone();
    let _ = methods.set(
        ctx,
        "name",
        Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let this = stack.get(0);
            let id = actor_id(ctx, this)?;
            if matches!(stack.get(1), Value::String(s) if s.as_bytes().len() > MAX_ARG) {
                return Err(error(ctx, format!("name: longer than {MAX_ARG} bytes")));
            }
            let name = str_arg(ctx, &stack, 1, "name")?;
            exec.fuel().consume(4);
            if let Some(a) = shared.borrow_mut().stage.get_mut(id) {
                a.name = name;
            }
            stack.replace(ctx, this);
            Ok(CallbackReturn::Return)
        }),
    );
    // Children (the prelude builds `GetChild`/`GetChildren` on them) and
    // the parent, as the actors' userdata.
    let shared = host.clone();
    set(
        "child_ids",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let id = actor_id(ctx, stack.get(0))?;
            let kids = shared.borrow().stage.children(id);
            charge(&mut exec, kids.len() * 64);
            let t = Table::new(&ctx);
            for (i, c) in kids.iter().enumerate() {
                t.set(ctx, i as i64 + 1, i64::from(*c))?;
            }
            stack.replace(ctx, t);
            Ok(CallbackReturn::Return)
        })),
    );
    // `SortByDrawOrder` (the prelude's): whether the children moved, their
    // places then recorded again.
    let shared = host.clone();
    set(
        "sort_children",
        func(Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let id = actor_id(ctx, stack.get(0))?;
            let mut host = shared.borrow_mut();
            let moved = match host.stage.get_mut(id) {
                Some(a) => {
                    charge(&mut exec, a.work("SortByDrawOrder") * 16);
                    a.sort_by_draw_order()
                }
                None => false,
            };
            if moved {
                host.stage.reindex(id);
            }
            stack.replace(ctx, moved);
            Ok(CallbackReturn::Return)
        })),
    );
    let shared = host.clone();
    let _ = methods.set(
        ctx,
        "GetParent",
        Callback::from_fn_with(&ctx, actors, move |actors, ctx, _, mut stack| {
            let id = actor_id(ctx, stack.get(0))?;
            let parent = shared.borrow().stage.parent(id);
            stack.replace(
                ctx,
                parent.map_or(Value::Nil, |p| actors.get_value(ctx, i64::from(p))),
            );
            Ok(CallbackReturn::Return)
        }),
    );
    let class = |parent: Table<'gc>| {
        let t = Table::new(&ctx);
        let mt = Table::new(&ctx);
        let _ = mt.set(ctx, "__index", parent);
        let _ = t.set_metatable(&ctx, Some(mt));
        t
    };
    let sprite = class(methods);
    let frame = class(methods);
    let model = class(methods);
    // `SetUpdateFunction` (frames only): called with the frame and the
    // seconds after its children's update, during a play.
    let updates = Table::new(&ctx);
    set("update_functions", Value::Table(updates));
    let shared = host.clone();
    let _ = frame.set(
        ctx,
        "SetUpdateFunction",
        Callback::from_fn_with(&ctx, updates, move |updates, ctx, _, mut stack| {
            let this = stack.get(0);
            let id = actor_id(ctx, this)?;
            let f = stack.get(1);
            if !matches!(f, Value::Nil | Value::Function(_)) {
                return Err(error(
                    ctx,
                    "SetUpdateFunction: argument 1 must be a function",
                ));
            }
            updates.set(ctx, i64::from(id), f)?;
            let mut host = shared.borrow_mut();
            if f.is_nil() {
                host.updating.remove(&id);
            } else {
                host.updating.insert(id);
            }
            stack.replace(ctx, this);
            Ok(CallbackReturn::Return)
        }),
    );
    for &m in MODEL_ONLY {
        let _ = model.set(ctx, m, native_method(ctx, host, m));
    }
    // `SetStateProperties`: new states, written as `Frames`.
    let shared = host.clone();
    let _ = sprite.set(
        ctx,
        "SetStateProperties",
        Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let this = stack.get(0);
            let id = actor_id(ctx, this)?;
            let Value::Table(list) = stack.get(1) else {
                return Err(error(ctx, "SetStateProperties: argument 1 must be a table"));
            };
            let mut host = shared.borrow_mut();
            let mut notes = Vec::new();
            if let Some(ddi_actor::Actor {
                kind: ddi_actor::Kind::Sprite(sprite),
                ..
            }) = host.stage.get_mut(id)
            {
                let states =
                    super::convert::frames(ctx, list, sprite, "SetStateProperties", &mut notes)
                        .map_err(|e| error(ctx, e.to_string()))?;
                charge(&mut exec, states.len() * 16);
                super::convert::set_states(sprite, states);
            }
            for n in notes {
                host.warn(n);
            }
            stack.replace(ctx, this);
            Ok(CallbackReturn::Return)
        }),
    );
    // `ActorFrameTexture` (a song's): `Create` within the song's pixels,
    // `GetTexture`; `SetTextureName` names nothing here (sprites show a
    // target through `SetTexture`).
    let aft = class(frame);
    let texture_class = Table::new(&ctx);
    for (name, image) in [
        ("GetImageWidth", 0),
        ("GetImageHeight", 1),
        ("GetSourceWidth", 0),
        ("GetSourceHeight", 1),
        ("GetTextureWidth", 0),
        ("GetTextureHeight", 1),
    ] {
        let shared = host.clone();
        let _ = texture_class.set(
            ctx,
            name,
            Callback::from_fn(&ctx, move |ctx, _, mut stack| {
                let index = texture_index(ctx, stack.get(0))?;
                let size = shared.borrow().targets.get(index as usize).copied();
                let size = size.ok_or_else(|| error(ctx, format!("{name}: no such texture")))?;
                stack.replace(ctx, i64::from(size[image]));
                Ok(CallbackReturn::Return)
            }),
        );
    }
    let _ = texture_class.set(
        ctx,
        "GetNumFrames",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            stack.replace(ctx, 1);
            Ok(CallbackReturn::Return)
        }),
    );
    let texture_mt = Table::new(&ctx);
    let _ = texture_mt.set(ctx, "__index", texture_class);
    let shared = host.clone();
    let _ = aft.set(
        ctx,
        "Create",
        Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let this = stack.get(0);
            let id = actor_id(ctx, this)?;
            exec.fuel().consume(64);
            let mut host = shared.borrow_mut();
            let host = &mut *host;
            let made = host.targets.len() as u32;
            let pixels = host.target_pixels;
            let problem = match host.stage.get_mut(id).and_then(|a| a.target.as_deref_mut()) {
                None => Some("Create: not an ActorFrameTexture".to_string()),
                Some(t) if t.index.is_some() => {
                    Some("Can't Create an already created ActorFrameTexture".into())
                }
                Some(t) => {
                    let [w, h] = t.size.map(|v| if v.is_finite() { v as i64 } else { 0 });
                    if w < 1 || h < 1 {
                        Some("ActorFrameTexture: Cannot have width or height less than 1".into())
                    } else if w > i64::from(MAX_TARGET_SIDE) || h > i64::from(MAX_TARGET_SIDE) {
                        Some(format!(
                            "ActorFrameTexture: {w} × {h} is larger than {MAX_TARGET_SIDE} pixels a side"
                        ))
                    } else if pixels + (w * h) as u64 > MAX_TARGET_PIXELS {
                        Some(format!(
                            "ActorFrameTexture: the song's render targets would hold more than {MAX_TARGET_PIXELS} pixels"
                        ))
                    } else {
                        t.index = Some(made);
                        host.targets.push([w as u32, h as u32]);
                        host.target_pixels += (w * h) as u64;
                        None
                    }
                }
            };
            if let Some(p) = problem {
                // `LuaHelpers::ReportScriptError`: reported, the script
                // goes on.
                host.warn(p);
            }
            stack.replace(ctx, this);
            Ok(CallbackReturn::Return)
        }),
    );
    let _ = aft.set(
        ctx,
        "SetTextureName",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let this = stack.get(0);
            actor_id(ctx, this)?;
            stack.replace(ctx, this);
            Ok(CallbackReturn::Return)
        }),
    );
    let shared = host.clone();
    let _ = aft.set(
        ctx,
        "GetTexture",
        Callback::from_fn_with(&ctx, texture_mt, move |&mt, ctx, _, mut stack| {
            let id = actor_id(ctx, stack.get(0))?;
            let index = shared
                .borrow()
                .stage
                .get(id)
                .and_then(|a| a.target.as_ref()?.index);
            let v = match index {
                Some(i) => {
                    let ud = UserData::new_static(&ctx, TextureRef(i));
                    let _ = ud.set_metatable(&ctx, Some(mt));
                    Value::UserData(ud)
                }
                None => Value::Nil,
            };
            stack.replace(ctx, v);
            Ok(CallbackReturn::Return)
        }),
    );
    // `Sprite::SetTexture` of a render target's texture: the sprite shows
    // it, at its size.
    let shared = host.clone();
    let _ = sprite.set(
        ctx,
        "SetTexture",
        Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
            let this = stack.get(0);
            let id = actor_id(ctx, this)?;
            let index = texture_index(ctx, stack.get(1))?;
            exec.fuel().consume(16);
            let mut host = shared.borrow_mut();
            let size = host
                .targets
                .get(index as usize)
                .copied()
                .ok_or_else(|| error(ctx, "SetTexture: no such texture"))?;
            let size = size.map(|v| v as f32);
            let texture = ddi_actor::target_texture(index);
            if let Some(a) = host.stage.get_mut(id) {
                match &mut a.kind {
                    ddi_actor::Kind::Sprite(s) => {
                        s.texture = texture;
                        s.size = size;
                        s.frame_size = size;
                        s.padded_to_image = [1.0; 2];
                    }
                    kind @ ddi_actor::Kind::Actor => {
                        *kind = ddi_actor::Kind::Sprite(ddi_actor::Sprite::new(
                            texture,
                            Vec::new(),
                            size,
                        ));
                    }
                    _ => return Err(error(ctx, "SetTexture: not a sprite")),
                }
            }
            stack.replace(ctx, this);
            Ok(CallbackReturn::Return)
        }),
    );
    set("actor_methods", Value::Table(methods));
    set("sprite_methods", Value::Table(sprite));
    set("frame_methods", Value::Table(frame));
    set("model_methods", Value::Table(model));
    set("frame_texture_methods", Value::Table(aft));
    // An actor is its methods plus fields of its own (`self.n = 1`), as
    // StepMania's actor objects are.
    let fields = Table::new(&ctx);
    let metatable = |class: Table<'gc>| {
        let mt = Table::new(&ctx);
        let shared = host.clone();
        let _ = mt.set(
            ctx,
            "__index",
            Callback::from_fn_with(
                &ctx,
                (fields, class),
                move |&(fields, class), ctx, _, mut stack| {
                    let id = actor_id(ctx, stack.get(0))?;
                    let key = stack.get(1);
                    let mut v = match fields.get_value(ctx, i64::from(id)) {
                        Value::Table(t) => t.get_raw(key),
                        _ => Value::Nil,
                    };
                    // The class, then the classes it extends (`Sprite`, then
                    // `Actor`).
                    let mut c = Some(class);
                    // A few levels: a skin can rewire the class tables into
                    // a cycle.
                    for _ in 0..8 {
                        if !v.is_nil() {
                            break;
                        }
                        let Some(t) = c else { break };
                        v = t.get_raw(key);
                        c = t.metatable().and_then(|m| {
                            match m.get_raw(Value::String(ctx.intern(b"__index"))) {
                                Value::Table(up) => Some(up),
                                _ => None,
                            }
                        });
                    }
                    if v.is_nil() && shared.borrow().env.trace_missing {
                        shared
                            .borrow_mut()
                            .warn(format!("actor method {} is not available", key.display()));
                    }
                    stack.replace(ctx, v);
                    Ok(CallbackReturn::Return)
                },
            ),
        );
        let _ = mt.set(
            ctx,
            "__newindex",
            Callback::from_fn_with(&ctx, fields, |&fields, ctx, _, mut stack| {
                let id = actor_id(ctx, stack.get(0))?;
                let own = match fields.get_value(ctx, i64::from(id)) {
                    Value::Table(t) => t,
                    _ => {
                        let t = Table::new(&ctx);
                        fields.set(ctx, i64::from(id), t)?;
                        t
                    }
                };
                own.set(ctx, stack.get(1), stack.get(2))?;
                stack.clear();
                Ok(CallbackReturn::Return)
            }),
        );
        ctx.stash(mt)
    };
    let registries = Registries {
        actors: ctx.stash(actors),
        commands: ctx.stash(commands),
        children: ctx.stash(children),
        metatables: [
            metatable(methods),
            metatable(sprite),
            metatable(frame),
            metatable(model),
            metatable(aft),
        ],
    };
    (h, registries)
}

/// The index of the render target's texture `v` stands for.
fn texture_index<'gc>(ctx: Context<'gc>, v: Value<'gc>) -> Result<u32, Error<'gc>> {
    match v {
        Value::UserData(ud) => ud
            .downcast_static::<TextureRef>()
            .map(|t| t.0)
            .map_err(|_| error(ctx, "not a texture")),
        _ => Err(error(ctx, "not a texture")),
    }
}

/// A new userdata for actor `id` with its class's metatable.
pub(crate) fn actor_userdata<'gc>(
    ctx: Context<'gc>,
    id: u32,
    metatable: Table<'gc>,
) -> UserData<'gc> {
    let ud = UserData::new_static(&ctx, ActorRef(id));
    let _ = ud.set_metatable(&ctx, Some(metatable));
    ud
}

/// An actor method run natively (`Actor::call`): the call's arguments
/// converted, the actor looked up by its handle, the work it does paid for.
fn native_method<'gc>(
    ctx: Context<'gc>,
    host: &Rc<RefCell<Host>>,
    m: &'static str,
) -> Callback<'gc> {
    let shared = host.clone();
    Callback::from_fn(&ctx, move |ctx, mut exec, mut stack| {
        let this = stack.get(0);
        let id = actor_id(ctx, this)?;
        let mut args = Vec::new();
        for i in 1..stack.len().min(17) {
            args.push(arg(ctx, stack.get(i), m, i)?);
        }
        let call = Call {
            method: m.to_string(),
            args,
        };
        let mut host = shared.borrow_mut();
        let Some(actor) = host.stage.get_mut(id) else {
            return Err(error(ctx, format!("{m}: the actor is gone")));
        };
        // Methods that walk a tree or a sprite's states pay for what they
        // visit.
        charge(&mut exec, actor.work(m) * 64);
        let mut problems = Problems::default();
        let r = actor.call(&call, &mut problems, 0);
        for p in problems.messages {
            host.warn(p);
        }
        r.map_err(|e| error(ctx, e))?;
        stack.replace(ctx, this);
        Ok(CallbackReturn::Return)
    })
}
