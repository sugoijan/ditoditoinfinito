//! StepMania's `cmd` keyword, rewritten into plain Lua before compiling.
//!
//! StepMania's Lua 5.1 has `cmd` as a keyword (`extern/lua-5.1/src/llex.c`,
//! `lparser.c` `cmd`, `cmd_body`, `cmd_funcargs`): `cmd(a,b;c;d,e,f)` is an
//! expression, the function `function(self, ...) self:a(b) self:c()
//! self:d(e, f) end`. Each `;`-separated part is a method name, then
//! nothing or `,` and an expression list; empty parts are allowed.
//!
//! The rewrite works on tokens and replaces them in place, so every token
//! keeps its line and errors point where StepMania's would: `cmd` becomes
//! `function`, its `(` becomes `(self,...)`, a name `self:name`, the `,`
//! after it `(`, and each part ends with `)`; the closing `)` becomes
//! ` end`. `cmd` not followed by `(` is left as a name (StepMania rejects
//! it). Strings, comments and everything else are copied unchanged.

/// A malformed `cmd(...)`, with the line it was found on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmdError {
    pub line: usize,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Whitespace and comments.
    Trivia,
    Name,
    /// Strings and numbers.
    Literal,
    Symbol,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
    line: usize,
}

/// Rewrites every `cmd(...)` of `source`. Sources without `cmd(` come back
/// unchanged (and are not copied).
pub fn rewrite(source: &[u8]) -> Result<Option<Vec<u8>>, CmdError> {
    if !contains_cmd(source) {
        return Ok(None);
    }
    let tokens = tokenize(source);
    let mut out = Vec::with_capacity(source.len().saturating_add(source.len() / 4));
    let mut w = Writer {
        source,
        tokens: &tokens,
        out: &mut out,
        depth: 0,
    };
    let mut i = 0;
    while i < tokens.len() {
        i = w.token(i)?;
    }
    Ok(Some(out))
}

fn contains_cmd(source: &[u8]) -> bool {
    source.windows(3).any(|w| w == b"cmd")
}

struct Writer<'a> {
    source: &'a [u8],
    tokens: &'a [Token],
    out: &'a mut Vec<u8>,
    /// `cmd(...)`s open around the current token.
    depth: usize,
}

/// `cmd(...)`s nested in one another at most (the rewrite recurses per
/// level; the parser stops at the same depth).
const MAX_DEPTH: usize = 200;

impl<'a> Writer<'a> {
    fn text(&self, t: Token) -> &'a [u8] {
        self.source.get(t.start..t.end).unwrap_or_default()
    }

    fn is(&self, i: usize, kind: Kind, text: &[u8]) -> bool {
        self.tokens
            .get(i)
            .is_some_and(|&t| t.kind == kind && self.text(t) == text)
    }

    fn copy(&mut self, i: usize) {
        if let Some(&t) = self.tokens.get(i) {
            self.out.extend_from_slice(self.text(t));
        }
    }

    /// The next token that is not trivia, from `i` on, copying the trivia.
    fn skip_trivia(&mut self, mut i: usize) -> usize {
        while self.tokens.get(i).is_some_and(|t| t.kind == Kind::Trivia) {
            self.copy(i);
            i += 1;
        }
        i
    }

    /// The line of token `i`; past the end, the last line (as the lexer
    /// counts it at `<eof>`).
    fn line(&self, i: usize) -> usize {
        match self.tokens.get(i) {
            Some(t) => t.line,
            None => 1 + self.source.iter().filter(|&&b| b == b'\n').count(),
        }
    }

    fn near(&self, i: usize) -> String {
        match self.tokens.get(i) {
            Some(&t) => format!("'{}'", String::from_utf8_lossy(self.text(t))),
            None => "<eof>".into(),
        }
    }

    fn error(&self, i: usize, what: &str) -> CmdError {
        CmdError {
            line: self.line(i),
            message: format!("{what} near {}", self.near(i)),
        }
    }

    /// Copies token `i`, rewriting it if it starts a `cmd(...)`; returns
    /// the index after what it consumed.
    fn token(&mut self, i: usize) -> Result<usize, CmdError> {
        if self.is(i, Kind::Name, b"cmd") {
            let mut j = i + 1;
            while self.tokens.get(j).is_some_and(|t| t.kind == Kind::Trivia) {
                j += 1;
            }
            if self.is(j, Kind::Symbol, b"(") {
                return self.cmd(i, j);
            }
        }
        self.copy(i);
        Ok(i + 1)
    }

    /// `cmd` at `i`, its `(` at `open`.
    fn cmd(&mut self, i: usize, open: usize) -> Result<usize, CmdError> {
        if self.depth >= MAX_DEPTH {
            return Err(self.error(i, "cmd nested too deeply"));
        }
        self.depth += 1;
        let r = self.cmd_body(i, open);
        self.depth -= 1;
        r
    }

    fn cmd_body(&mut self, i: usize, open: usize) -> Result<usize, CmdError> {
        let line = self.line(i);
        self.out.extend_from_slice(b"function");
        for t in i + 1..open {
            self.copy(t);
        }
        self.out.extend_from_slice(b"(self,...)");
        let mut p = open + 1;
        loop {
            p = self.skip_trivia(p);
            let Some(&t) = self.tokens.get(p) else {
                return Err(CmdError {
                    line: self.line(p),
                    message: format!("')' expected (to close 'cmd' at line {line}) near <eof>"),
                });
            };
            match (t.kind, self.text(t)) {
                (Kind::Symbol, b")") => {
                    self.out.extend_from_slice(b" end");
                    return Ok(p + 1);
                }
                (Kind::Symbol, b";") => {
                    self.out.push(b' ');
                    p += 1;
                }
                (Kind::Name, _) => p = self.part(p)?,
                _ => return Err(self.error(p, "<name> expected")),
            }
        }
    }

    /// One `name [, explist]` part, at its name; returns the index of the
    /// `;` or `)` that ends it.
    fn part(&mut self, name: usize) -> Result<usize, CmdError> {
        self.out.extend_from_slice(b"self:");
        self.copy(name);
        let mut p = self.skip_trivia(name + 1);
        if self.is(p, Kind::Symbol, b";") || self.is(p, Kind::Symbol, b")") {
            self.out.extend_from_slice(b"()");
            return Ok(p);
        }
        if !self.is(p, Kind::Symbol, b",") {
            return Err(self.error(p, "')' expected"));
        }
        self.out.push(b'(');
        p += 1;
        let first = self.skip_trivia(p);
        if self.is(first, Kind::Symbol, b";") || self.is(first, Kind::Symbol, b")") {
            return Err(self.error(first, "unexpected symbol"));
        }
        p = first;
        // The expression list ends at a `;` or `)` outside brackets and
        // blocks (a `function ... end` argument has statements of its own).
        let mut depth = 0usize;
        loop {
            let Some(&t) = self.tokens.get(p) else {
                return Err(self.error(p, "')' expected"));
            };
            let text = self.text(t);
            match t.kind {
                Kind::Symbol if depth == 0 && (text == b";" || text == b")") => {
                    self.out.push(b')');
                    return Ok(p);
                }
                Kind::Symbol if matches!(text, b"(" | b"[" | b"{") => depth += 1,
                Kind::Symbol if matches!(text, b")" | b"]" | b"}") => {
                    depth = depth.saturating_sub(1)
                }
                Kind::Name if matches!(text, b"function" | b"do" | b"if" | b"repeat") => depth += 1,
                Kind::Name if matches!(text, b"end" | b"until") => depth = depth.saturating_sub(1),
                _ => {}
            }
            p = self.token(p)?;
        }
    }
}

/// Splits `source` into Lua 5.1 tokens, enough to tell names, strings,
/// comments and symbols apart (literals are not validated: the compiler
/// does that on the rewritten text).
fn tokenize(source: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    let mut line = 1;
    while pos < source.len() {
        let start = pos;
        let start_line = line;
        let at = |i: usize| source.get(i).copied().unwrap_or(0);
        let c = at(pos);
        let kind = if c == b'\n' || c.is_ascii_whitespace() || c == 0x0b {
            while pos < source.len() && (at(pos).is_ascii_whitespace() || at(pos) == 0x0b) {
                pos += 1;
            }
            Kind::Trivia
        } else if c == b'-' && at(pos + 1) == b'-' {
            pos += 2;
            match long_bracket(source, pos) {
                Some(level) => pos = long_end(source, pos, level),
                None => {
                    while pos < source.len() && at(pos) != b'\n' {
                        pos += 1;
                    }
                }
            }
            Kind::Trivia
        } else if c == b'[' && long_bracket(source, pos).is_some() {
            let level = long_bracket(source, pos).unwrap_or(0);
            pos = long_end(source, pos, level);
            Kind::Literal
        } else if c == b'"' || c == b'\'' {
            pos += 1;
            while pos < source.len() {
                match at(pos) {
                    b'\\' => pos += 2,
                    b'\n' => break,
                    q if q == c => {
                        pos += 1;
                        break;
                    }
                    _ => pos += 1,
                }
            }
            pos = pos.min(source.len());
            Kind::Literal
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while pos < source.len() && (at(pos).is_ascii_alphanumeric() || at(pos) == b'_') {
                pos += 1;
            }
            Kind::Name
        } else if c.is_ascii_digit() || (c == b'.' && at(pos + 1).is_ascii_digit()) {
            // As `read_numeral`: digits, dots, an exponent sign, then any
            // alphanumerics.
            while pos < source.len() {
                let d = at(pos);
                let sign = (d == b'+' || d == b'-') && matches!(at(pos - 1), b'e' | b'E');
                if sign || d.is_ascii_alphanumeric() || d == b'.' || d == b'_' {
                    pos += 1;
                } else {
                    break;
                }
            }
            Kind::Literal
        } else {
            pos += [b"...".as_slice(), b"..", b"==", b"~=", b"<=", b">=", b"::"]
                .iter()
                .find(|op| source.get(pos..).is_some_and(|rest| rest.starts_with(op)))
                .map_or(1, |op| op.len());
            Kind::Symbol
        };
        line += source
            .get(start..pos)
            .unwrap_or_default()
            .iter()
            .filter(|&&b| b == b'\n')
            .count();
        tokens.push(Token {
            kind,
            start,
            end: pos,
            line: start_line,
        });
    }
    tokens
}

/// The level of a long bracket (`[`, `=`s, `[`) at `pos`.
fn long_bracket(source: &[u8], pos: usize) -> Option<usize> {
    if source.get(pos) != Some(&b'[') {
        return None;
    }
    let level = source
        .get(pos + 1..)
        .unwrap_or_default()
        .iter()
        .take_while(|&&b| b == b'=')
        .count();
    (source.get(pos + 1 + level) == Some(&b'[')).then_some(level)
}

/// The end of the long bracket of `level` opened at `pos` (or the end of
/// the source, which the compiler then reports).
fn long_end(source: &[u8], pos: usize, level: usize) -> usize {
    let mut i = pos + level + 2;
    while i < source.len() {
        if source.get(i) == Some(&b']')
            && source
                .get(i + 1..i + 1 + level)
                .is_some_and(|eq| eq.iter().all(|&b| b == b'='))
            && source.get(i + 1 + level) == Some(&b']')
        {
            return i + level + 2;
        }
        i += 1;
    }
    source.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rw(s: &str) -> String {
        match rewrite(s.as_bytes()) {
            Ok(Some(out)) => String::from_utf8(out).unwrap_or_default(),
            Ok(None) => s.to_string(),
            Err(e) => format!("error {}: {}", e.line, e.message),
        }
    }

    #[test]
    fn parts() {
        assert_eq!(
            rw("f = cmd(diffusealpha,0;zoom,1.1;playcommand,\"Set\")"),
            "f = function(self,...)self:diffusealpha(0) self:zoom(1.1) self:playcommand(\"Set\") end"
        );
        assert_eq!(rw("cmd()"), "function(self,...) end");
        assert_eq!(
            rw("cmd(;;finishtweening;)"),
            "function(self,...)  self:finishtweening()  end"
        );
        assert_eq!(
            rw("cmd(x,1,2,...)"),
            "function(self,...)self:x(1,2,...) end"
        );
    }

    #[test]
    fn nested_and_brackets() {
        assert_eq!(
            rw("cmd(diffuse,color(\"1,1,1,1\");addx,t[1];sleep,(a or 1))"),
            "function(self,...)self:diffuse(color(\"1,1,1,1\")) self:addx(t[1]) self:sleep((a or 1)) end"
        );
        assert_eq!(
            rw("cmd(queuecommand,cmd(zoom,2))"),
            "function(self,...)self:queuecommand(function(self,...)self:zoom(2) end) end"
        );
        assert_eq!(
            rw("cmd(x,function(s) a(); if b then c() end; return 1 end;y)"),
            "function(self,...)self:x(function(s) a(); if b then c() end; return 1 end) self:y() end"
        );
    }

    #[test]
    fn strings_and_comments_untouched() {
        let s = "a = \"cmd(x;y)\" b = [[cmd(]] -- cmd(\n--[==[ cmd( ]==] c = 'cmd(\\'' d = t.cmd";
        assert_eq!(rw(s), s);
        assert_eq!(rw("local cmd = 1"), "local cmd = 1");
    }

    #[test]
    fn lines_kept() {
        let src = "local t = cmd(\n  zoom,1;\n  -- a comment\n  x,\n    2\n)\nerror('here')";
        let out = rw(src);
        assert_eq!(out.matches('\n').count(), src.matches('\n').count());
        assert_eq!(out.lines().last(), Some("error('here')"));
        assert_eq!(
            out,
            "local t = function(self,...)\n  self:zoom(1) \n  -- a comment\n  self:x(\n    2\n) end\nerror('here')"
        );
    }

    #[test]
    fn errors() {
        assert_eq!(rw("x = 1\ncmd(zoom,1"), "error 2: ')' expected near <eof>");
        assert_eq!(rw("cmd(zoom 1)"), "error 1: ')' expected near '1'");
        assert_eq!(rw("cmd(1)"), "error 1: <name> expected near '1'");
        assert_eq!(rw("cmd(zoom,)"), "error 1: unexpected symbol near ')'");
        assert_eq!(
            rw("f = cmd(\nzoom;\n"),
            "error 3: ')' expected (to close 'cmd' at line 1) near <eof>"
        );
    }

    #[test]
    fn nesting_is_bounded() {
        let deep = format!("x = {}{}", "cmd(a,".repeat(100_000), ")".repeat(100_000));
        assert!(
            rewrite(deep.as_bytes()).is_err_and(|e| e.message.starts_with("cmd nested too deeply"))
        );
        let ok = format!("x = {}1{}", "cmd(a,".repeat(150), ")".repeat(150));
        assert!(rewrite(ok.as_bytes()).is_ok());
    }

    #[test]
    fn unterminated_literals_do_not_panic() {
        for s in [
            "cmd(x,\"abc",
            "cmd(x,[[abc",
            "cmd(x,--[[",
            "cmd(x,'\\",
            "cmd(x,1e",
            "cmd(",
            "cmd",
        ] {
            let _ = rewrite(s.as_bytes());
        }
    }
}
