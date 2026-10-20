//! The globals a script sees: Lua 5.1's base, `string`, `table`, `math`
//! and `coroutine` libraries as the fork provides them, plus the 5.1 names
//! skins use that the fork leaves to the host. Nothing reaches outside the
//! VM: no `io`, `os`, `debug`, `package`, `require`, `dofile`, `load` or
//! `collectgarbage` (the host adds its own `loadfile` over the skin's
//! files, and `print`).

use lunita::limits::LimitError;
use lunita::stdlib::{load_base, load_coroutine, load_math, load_string, load_table};
use lunita::{Callback, CallbackReturn, Context, Table, Value};

/// Globals the fork's libraries define that a script must not have.
const REMOVED: &[&str] = &["load", "collectgarbage"];

/// Loads the globals; refused only under a memory limit too small for them.
pub fn load(ctx: Context<'_>) -> Result<(), LimitError> {
    load_base(ctx)?;
    load_coroutine(ctx)?;
    load_math(ctx)?;
    load_string(ctx)?;
    load_table(ctx)?;
    let g = ctx.globals();
    for name in REMOVED {
        ctx.set_global(name, Value::Nil)?;
    }
    ctx.set_global("_VERSION", "Lua 5.1")?;
    ctx.set_global("_G", g)?;

    // Lua 5.1's names for what the fork has under other ones
    // (lbaselib.c, ltablib.c, lmathlib.c, lstrlib.c with the 5.1
    // compatibility options StepMania builds with).
    let table: Option<Table> = ctx.get_global("table").ok();
    let math: Option<Table> = ctx.get_global("math").ok();
    let string: Option<Table> = ctx.get_global("string").ok();
    if let Some(table) = table {
        ctx.set_global("unpack", table.get_value(ctx, "unpack"))?;
        // `luaL_getn`: the raw length.
        let _ = table.set(
            ctx,
            "getn",
            Callback::from_fn(&ctx, |ctx, _, mut stack| {
                let t: Table = stack.consume(ctx)?;
                stack.replace(ctx, t.length());
                Ok(CallbackReturn::Return)
            }),
        );
    }
    if let Some(math) = math {
        let _ = math.set(ctx, "mod", math.get_value(ctx, "fmod"));
        let _ = math.set(
            ctx,
            "pow",
            Callback::from_fn(&ctx, |ctx, _, mut stack| {
                let (x, y): (f64, f64) = stack.consume(ctx)?;
                stack.replace(ctx, x.powf(y));
                Ok(CallbackReturn::Return)
            }),
        );
    }
    if let Some(string) = string {
        let _ = string.set(ctx, "gfind", string.get_value(ctx, "gmatch"));
    }
    Ok(())
}
