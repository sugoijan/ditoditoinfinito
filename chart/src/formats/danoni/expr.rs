//! Arithmetic on effect values.
//!
//! danoniplus evaluates effect frames and values (speed, boost, colours,
//! gauges) as JavaScript (`new Function("return " + s)`, `js/danoni_main.js`
//! 399–405), so authors write things like `60*20` or `(4+1)*60`. Only
//! numbers, `+ - * / %` and parentheses are understood here; anything else
//! is `None`, which danoniplus would treat like a failed evaluation (the
//! field's default).

pub fn eval(s: &str) -> Option<f64> {
    let mut p = Parser {
        b: s.as_bytes(),
        i: 0,
        depth: 0,
    };
    let v = p.expr()?;
    p.ws();
    (p.i == p.b.len() && v.is_finite()).then_some(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
    /// Nesting of parentheses and unary signs, bounded so a long run of
    /// `(` cannot overflow the stack.
    depth: u32,
}

/// Deepest nesting evaluated; deeper input is `None`.
const MAX_DEPTH: u32 = 64;

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.b.get(self.i).copied()
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.i += 1;
                    v += self.term()?;
                }
                Some(b'-') => {
                    self.i += 1;
                    v -= self.term()?;
                }
                _ => return Some(v),
            }
        }
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.i += 1;
                    v *= self.unary()?;
                }
                Some(b'/') => {
                    self.i += 1;
                    v /= self.unary()?;
                }
                Some(b'%') => {
                    self.i += 1;
                    v %= self.unary()?;
                }
                _ => return Some(v),
            }
        }
    }

    fn unary(&mut self) -> Option<f64> {
        self.depth += 1;
        let v = if self.depth > MAX_DEPTH {
            None
        } else {
            self.unary_inner()
        };
        self.depth -= 1;
        v
    }

    fn unary_inner(&mut self) -> Option<f64> {
        match self.peek()? {
            b'-' => {
                self.i += 1;
                Some(-self.unary()?)
            }
            b'+' => {
                self.i += 1;
                self.unary()
            }
            b'(' => {
                self.i += 1;
                let v = self.expr()?;
                (self.peek()? == b')').then(|| self.i += 1)?;
                Some(v)
            }
            _ => self.number(),
        }
    }

    fn number(&mut self) -> Option<f64> {
        let start = self.i;
        while self.i < self.b.len() && (self.b[self.i].is_ascii_digit() || self.b[self.i] == b'.') {
            self.i += 1;
        }
        if self.i < self.b.len() && matches!(self.b[self.i], b'e' | b'E') {
            let save = self.i;
            self.i += 1;
            if self.i < self.b.len() && matches!(self.b[self.i], b'+' | b'-') {
                self.i += 1;
            }
            let digits = self.i;
            while self.i < self.b.len() && self.b[self.i].is_ascii_digit() {
                self.i += 1;
            }
            if self.i == digits {
                self.i = save;
            }
        }
        std::str::from_utf8(&self.b[start..self.i])
            .ok()?
            .parse()
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::eval;

    #[test]
    fn arithmetic() {
        assert_eq!(eval("1200"), Some(1200.0));
        assert_eq!(eval(" 60*20 "), Some(1200.0));
        assert_eq!(eval("(4+1)*60 - 2"), Some(298.0));
        assert_eq!(eval("-0.5"), Some(-0.5));
        assert_eq!(eval("7/2"), Some(3.5));
        assert_eq!(eval("1e3"), Some(1000.0));
        assert_eq!(eval(""), None);
        assert_eq!(eval("alert(1)"), None);
        assert_eq!(eval("1/0"), None);
        assert_eq!(eval("2*(3"), None);
        let deep = format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000));
        assert_eq!(eval(&deep), None);
        assert_eq!(eval(&"-".repeat(100_000)), None);
        assert_eq!(eval("((((1))))"), Some(1.0));
    }
}
