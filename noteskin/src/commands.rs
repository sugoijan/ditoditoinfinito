//! Actor commands as metrics write them: `zoom,.75;linear,0.11;zoom,1`.
//! StepMania compiles such a string to a Lua function
//! (`LuaHelpers::ParseCommandList`, `src/LuaManager.cpp:898-984`;
//! `ParseCommands`, `src/Command.cpp:40-104`): commands split at `;`
//! (quotes respected), each at every `,`, the first part the method and the
//! rest rejoined as its Lua arguments, after a leading `+` is dropped and a
//! `#rrggbb[aa]` part becomes four numbers. The arguments are evaluated here
//! without Lua when they are what skins write: numbers, strings, booleans,
//! `nil`, `color("…")` and the enum and alignment globals. Anything else
//! needs the Lua step.

use std::fmt;

/// A value passed to an actor method.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Num(f32),
    Str(String),
    Bool(bool),
    Nil,
    /// A colour table, as `color(…)` returns.
    Color([f32; 4]),
}

/// `self:<method>(<args>)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub method: String,
    pub args: Vec<Arg>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    /// Lua beyond what is evaluated here (a function call, arithmetic, `%`
    /// raw Lua): needs the Lua step.
    NeedsLua(String),
    /// Not valid Lua: StepMania warns and the command does nothing.
    Invalid(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::NeedsLua(s) => write!(f, "needs Lua: {s}"),
            CommandError::Invalid(s) => write!(f, "not valid: {s}"),
        }
    }
}

/// Parses a command string into calls.
pub fn parse(commands: &str) -> Result<Vec<Call>, CommandError> {
    if commands.starts_with('%') || commands.starts_with('\u{1b}') {
        return Err(CommandError::NeedsLua(commands.to_string()));
    }
    let mut calls = Vec::new();
    for command in split_with_quotes(commands, b';') {
        let mut parts = command.split(',');
        let method = parts.next().unwrap_or("").trim_matches(LUA_SPACE);
        if !is_name(method) {
            return Err(CommandError::Invalid(command.to_string()));
        }
        let args: Vec<String> = parts.map(argument).collect();
        let args = evaluate(&args.join(","))?;
        calls.push(Call {
            method: method.to_string(),
            args,
        });
    }
    Ok(calls)
}

/// `SplitWithQuotes(…, ';', …, true)`: a quote runs to the next same quote
/// (or the end), empty pieces are dropped.
fn split_with_quotes(s: &str, delimiter: u8) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    while start <= b.len() {
        let mut pos = start;
        while pos < b.len() && b[pos] != delimiter {
            if b[pos] == b'"' || b[pos] == b'\'' {
                pos = match b[pos + 1..].iter().position(|&c| c == b[pos]) {
                    Some(p) => pos + 1 + p + 1,
                    None => b.len(),
                };
            } else {
                pos += 1;
            }
        }
        if pos > start {
            out.push(&s[start..pos]);
        }
        start = pos + 1;
    }
    out
}

/// One `,`-separated part as `ParseCommandList` writes it out.
fn argument(part: &str) -> String {
    let part = part.strip_prefix('+').unwrap_or(part);
    if part.starts_with('#') {
        // `RageColor::FromString` on the part, written as four numbers
        // (white when it does not parse).
        let c = color_from_string(part);
        return format!("{},{},{},{}", c[0], c[1], c[2], c[3]);
    }
    part.to_string()
}

/// `RageColor::FromString` (`src/RageTypes.h:231-256`): `r,g,b[,a]` as
/// `sscanf("%f,%f,%f,%f")`, else `#rrggbb[aa]`, else white.
pub fn color_from_string(s: &str) -> [f32; 4] {
    let mut floats = Vec::new();
    let mut rest = s;
    for i in 0..4 {
        if i > 0 {
            match rest.strip_prefix(',') {
                Some(r) => rest = r,
                None => break,
            }
        }
        match scan_float(rest) {
            Some((v, r)) => {
                floats.push(v);
                rest = r;
            }
            None => break,
        }
    }
    match floats.len() {
        3 => return [floats[0], floats[1], floats[2], 1.0],
        4 => return [floats[0], floats[1], floats[2], floats[3]],
        _ => {}
    }
    if let Some(hex) = s.strip_prefix('#') {
        let mut values = Vec::new();
        let mut h = hex;
        for _ in 0..4 {
            // `%2x`: up to two hex digits.
            let n = h.bytes().take(2).take_while(u8::is_ascii_hexdigit).count();
            if n == 0 {
                break;
            }
            values.push(u32::from_str_radix(&h[..n], 16).unwrap_or(0));
            h = &h[n..];
        }
        if values.len() >= 3 {
            let a = values.get(3).map_or(1.0, |&v| v as f32 / 255.0);
            return [
                values[0] as f32 / 255.0,
                values[1] as f32 / 255.0,
                values[2] as f32 / 255.0,
                a,
            ];
        }
    }
    [1.0; 4]
}

/// `%f`: optional white space, then a decimal number (C syntax).
fn scan_float(s: &str) -> Option<(f32, &str)> {
    let t = s.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let digits = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut n = i - digits;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let f = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        n += i - f;
    }
    if n == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let e = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > e {
            i = j;
        }
    }
    Some((t[..i].parse().ok()?, &t[i..]))
}

/// Lua's white space.
const LUA_SPACE: &[char] = &[' ', '\t', '\n', '\r', '\x0b', '\x0c'];

fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !LUA_KEYWORDS.contains(&s)
}

const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// Evaluates a Lua expression list made of the values skins write.
fn evaluate(list: &str) -> Result<Vec<Arg>, CommandError> {
    let mut lexer = Lexer { s: list, pos: 0 };
    let mut out = Vec::new();
    if lexer.peek().is_none() {
        return Ok(out);
    }
    loop {
        out.push(lexer.expression(list)?);
        match lexer.next_token()? {
            None => return Ok(out),
            Some(Token::Comma) => {}
            Some(_) => return Err(CommandError::NeedsLua(list.to_string())),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Num(f64),
    Str(String),
    Name(String),
    Comma,
    Minus,
    Open,
    Close,
    /// Anything else Lua has: operators, `:`, `.`, `[`, `{`…
    Other,
}

struct Lexer<'a> {
    s: &'a str,
    pos: usize,
}

impl Lexer<'_> {
    fn skip_space(&mut self) {
        let b = self.s.as_bytes();
        loop {
            while self.pos < b.len() && b[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            // A `--` comment runs to the end of the line.
            if b[self.pos..].starts_with(b"--") {
                while self.pos < b.len() && b[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_space();
        self.s.as_bytes().get(self.pos).copied()
    }

    fn next_token(&mut self) -> Result<Option<Token>, CommandError> {
        let Some(c) = self.peek() else {
            return Ok(None);
        };
        let b = self.s.as_bytes();
        let start = self.pos;
        let token = match c {
            b',' => {
                self.pos += 1;
                Token::Comma
            }
            b'-' => {
                self.pos += 1;
                Token::Minus
            }
            b'(' => {
                self.pos += 1;
                Token::Open
            }
            b')' => {
                self.pos += 1;
                Token::Close
            }
            b'"' | b'\'' => Token::Str(self.string(c)?),
            b'0'..=b'9' | b'.'
                if c != b'.' || b.get(self.pos + 1).is_some_and(u8::is_ascii_digit) =>
            {
                Token::Num(self.number()?)
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                while self.pos < b.len()
                    && (b[self.pos].is_ascii_alphanumeric() || b[self.pos] == b'_')
                {
                    self.pos += 1;
                }
                Token::Name(self.s[start..self.pos].to_string())
            }
            _ => {
                self.pos += 1;
                Token::Other
            }
        };
        Ok(Some(token))
    }

    /// A short string with Lua's escapes.
    fn string(&mut self, quote: u8) -> Result<String, CommandError> {
        let b = self.s.as_bytes();
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            let Some(&c) = b.get(self.pos) else {
                return Err(CommandError::Invalid(self.s.to_string()));
            };
            self.pos += 1;
            match c {
                c if c == quote => break,
                b'\n' => return Err(CommandError::Invalid(self.s.to_string())),
                b'\\' => {
                    let Some(&e) = b.get(self.pos) else {
                        return Err(CommandError::Invalid(self.s.to_string()));
                    };
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'a' => out.push(7),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'v' => out.push(11),
                        b'0'..=b'9' => {
                            let mut v: u32 = u32::from(e - b'0');
                            for _ in 0..2 {
                                match b.get(self.pos) {
                                    Some(d) if d.is_ascii_digit() => {
                                        v = v * 10 + u32::from(d - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            if v > 255 {
                                return Err(CommandError::Invalid(self.s.to_string()));
                            }
                            out.push(v as u8);
                        }
                        other => out.push(other),
                    }
                }
                c => out.push(c),
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// A Lua 5.1 numeral (`lua_str2number`: `strtod`, or hex with `0x`).
    fn number(&mut self) -> Result<f64, CommandError> {
        let b = self.s.as_bytes();
        let start = self.pos;
        // Lua reads a numeral as everything alphanumeric, `.` and an
        // exponent sign, then converts it.
        while self.pos < b.len() {
            let c = b[self.pos];
            let exponent_sign = (c == b'+' || c == b'-') && matches!(b[self.pos - 1], b'e' | b'E');
            if c.is_ascii_alphanumeric() || c == b'.' || c == b'_' || exponent_sign {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = &self.s[start..self.pos];
        let parsed = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            Some(hex) => u64::from_str_radix(hex, 16).ok().map(|v| v as f64),
            None => text
                .parse::<f64>()
                .ok()
                .filter(|_| !text.contains(['_']) && !text.ends_with(['e', 'E'])),
        };
        parsed.ok_or_else(|| CommandError::Invalid(format!("malformed number {text}")))
    }

    fn expression(&mut self, list: &str) -> Result<Arg, CommandError> {
        let needs_lua = || CommandError::NeedsLua(list.to_string());
        let Some(token) = self.next_token()? else {
            return Err(CommandError::Invalid(list.to_string()));
        };
        let value = match token {
            Token::Minus => match self.expression(list)? {
                Arg::Num(n) => Arg::Num(-n),
                _ => return Err(needs_lua()),
            },
            Token::Num(n) => Arg::Num(n as f32),
            Token::Str(s) => Arg::Str(s),
            Token::Open => {
                let inner = self.expression(list)?;
                if self.next_token()? != Some(Token::Close) {
                    return Err(needs_lua());
                }
                inner
            }
            Token::Name(name) => match name.as_str() {
                "true" => Arg::Bool(true),
                "false" => Arg::Bool(false),
                "nil" => Arg::Nil,
                "color" => {
                    if self.next_token()? != Some(Token::Open) {
                        return Err(needs_lua());
                    }
                    let Some(Token::Str(s)) = self.next_token()? else {
                        return Err(needs_lua());
                    };
                    if self.next_token()? != Some(Token::Close) {
                        return Err(needs_lua());
                    }
                    Arg::Color(color_from_string(&s))
                }
                // `02 Actor.lua`'s alignment globals.
                "left" => Arg::Str("HorizAlign_Left".into()),
                "center" => Arg::Str("HorizAlign_Center".into()),
                "right" => Arg::Str("HorizAlign_Right".into()),
                "top" => Arg::Str("VertAlign_Top".into()),
                "middle" => Arg::Str("VertAlign_Middle".into()),
                "bottom" => Arg::Str("VertAlign_Bottom".into()),
                // Enum values are globals holding their names
                // (`BlendMode_Add`).
                n if ENUM_PREFIXES.iter().any(|p| {
                    n.strip_prefix(p)
                        .is_some_and(|r| r.starts_with('_') && r.len() > 1)
                }) =>
                {
                    Arg::Str(name)
                }
                n if LUA_KEYWORDS.contains(&n) => {
                    return Err(CommandError::Invalid(list.to_string()));
                }
                // Any other global: a skin's or the theme's, needs Lua.
                _ => return Err(needs_lua()),
            },
            Token::Comma | Token::Close => return Err(CommandError::Invalid(list.to_string())),
            Token::Other => return Err(needs_lua()),
        };
        // An operator or call after a value: arithmetic, needs Lua.
        match self.peek() {
            None | Some(b',') | Some(b')') => Ok(value),
            Some(_) => Err(needs_lua()),
        }
    }
}

/// The enums actor methods take whose values skins name as globals.
const ENUM_PREFIXES: &[&str] = &[
    "BlendMode",
    "HorizAlign",
    "VertAlign",
    "EffectMode",
    "CullMode",
    "ZTestMode",
    "TextureMode",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(s: &str) -> Vec<Call> {
        parse(s).unwrap()
    }

    fn call(method: &str, args: Vec<Arg>) -> Call {
        Call {
            method: method.into(),
            args,
        }
    }

    #[test]
    fn commands_as_metrics_write_them() {
        use Arg::*;
        assert_eq!(
            calls("stoptweening;zoom,.75;linear,0.11;zoom,1"),
            [
                call("stoptweening", vec![]),
                call("zoom", vec![Num(0.75)]),
                call("linear", vec![Num(0.11)]),
                call("zoom", vec![Num(1.0)])
            ]
        );
        assert_eq!(
            calls(
                "effectclock,'beat';diffuseramp;effectcolor1,color(\"0.35,0.35,0.35,1\");effectcolor2,color(\"1,1,1,1\");"
            ),
            [
                call("effectclock", vec![Str("beat".into())]),
                call("diffuseramp", vec![]),
                call("effectcolor1", vec![Color([0.35, 0.35, 0.35, 1.0])]),
                call("effectcolor2", vec![Color([1.0; 4])])
            ]
        );
        assert_eq!(
            calls(
                "blend,\"BlendMode_Add\";diffuse,1,1,1,1;rotationz,-90;x,+5;playcommand,\"HoldingOff\";visible,false"
            ),
            [
                call("blend", vec![Str("BlendMode_Add".into())]),
                call("diffuse", vec![Num(1.0), Num(1.0), Num(1.0), Num(1.0)]),
                call("rotationz", vec![Num(-90.0)]),
                call("x", vec![Num(5.0)]),
                call("playcommand", vec![Str("HoldingOff".into())]),
                call("visible", vec![Bool(false)])
            ]
        );
        // `#rrggbb` parts become four numbers; enum and alignment globals.
        assert_eq!(
            calls("diffuse,#FF0080;vertalign,bottom;blend,BlendMode_Add;effecttiming,.2,0,.8,0"),
            [
                call(
                    "diffuse",
                    vec![Num(1.0), Num(0.0), Num(128.0 / 255.0), Num(1.0)]
                ),
                call("vertalign", vec![Str("VertAlign_Bottom".into())]),
                call("blend", vec![Str("BlendMode_Add".into())]),
                call("effecttiming", vec![Num(0.2), Num(0.0), Num(0.8), Num(0.0)])
            ]
        );
        // A `;` inside quotes stays.
        assert_eq!(
            calls("queuecommand,'a;b'"),
            [call("queuecommand", vec![Str("a;b".into())])]
        );
        assert_eq!(calls(""), []);
        // Empty commands are dropped; a blank one is `self: ()`, not Lua.
        assert_eq!(calls(";;"), []);
        assert!(matches!(parse(" ;; "), Err(CommandError::Invalid(_))));
        assert_eq!(calls("zoom , 2"), [call("zoom", vec![Num(2.0)])]);
    }

    #[test]
    fn what_needs_lua_and_what_is_not_valid() {
        assert!(matches!(
            parse("diffuse,JudgmentLineToColor(\"JudgmentLine_W5\")"),
            Err(CommandError::NeedsLua(_))
        ));
        assert!(matches!(
            parse("x,PREFSMAN:GetPreference('GlobalOffsetSeconds')"),
            Err(CommandError::NeedsLua(_))
        ));
        assert!(matches!(
            parse("zoom,0.5*2"),
            Err(CommandError::NeedsLua(_))
        ));
        assert!(matches!(
            parse("%function(self) end"),
            Err(CommandError::NeedsLua(_))
        ));
        assert!(matches!(parse("x,somevar"), Err(CommandError::NeedsLua(_))));
        assert!(matches!(parse("x,,1"), Err(CommandError::Invalid(_))));
        assert!(matches!(parse("1x"), Err(CommandError::Invalid(_))));
        assert!(matches!(parse("x,'open"), Err(CommandError::Invalid(_))));
        assert!(matches!(parse("x,1e"), Err(CommandError::Invalid(_))));
        assert!(matches!(parse("end"), Err(CommandError::Invalid(_))));
    }

    #[test]
    fn colours_parse_as_stepmania_does() {
        assert_eq!(color_from_string("1,.5,0"), [1.0, 0.5, 0.0, 1.0]);
        assert_eq!(color_from_string(" 1, 0.5 ,0,0.25"), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(color_from_string("1, 0.5,0,0.25"), [1.0, 0.5, 0.0, 0.25]);
        assert_eq!(color_from_string("#FFFFFF"), [1.0; 4]);
        assert_eq!(
            color_from_string("#00000080"),
            [0.0, 0.0, 0.0, 128.0 / 255.0]
        );
        assert_eq!(color_from_string("1,1"), [1.0; 4]);
        assert_eq!(color_from_string("junk"), [1.0; 4]);
        assert_eq!(color_from_string("#12"), [1.0; 4]);
    }

    #[test]
    fn strings_and_numbers_follow_lua() {
        use Arg::*;
        assert_eq!(
            evaluate("'a\\'b', \"\\65\\n\", 0x10, 1e2, -(-.5)").unwrap(),
            [
                Str("a'b".into()),
                Str("A\n".into()),
                Num(16.0),
                Num(100.0),
                Num(0.5)
            ]
        );
        assert!(evaluate("1 -- comment").unwrap() == [Num(1.0)]);
        assert!(evaluate("'\\300'").is_err());
    }
}
