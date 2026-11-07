//! The pseudo-languages' transforms, applied to a message's own text
//! (never to its variables' values).

use std::borrow::Cow;

/// `qps-ploc`'s letters: accented where Latin-1 and Latin Extended-A
/// have an accented form (the theme's Latin subset draws them).
pub fn accents(s: &str) -> Cow<'_, str> {
    s.chars()
        .map(|c| match c {
            'a' => 'á',
            'c' => 'ç',
            'd' => 'ď',
            'e' => 'é',
            'g' => 'ĝ',
            'h' => 'ĥ',
            'i' => 'í',
            'j' => 'ĵ',
            'k' => 'ķ',
            'l' => 'ļ',
            'n' => 'ñ',
            'o' => 'ó',
            'r' => 'ŕ',
            's' => 'š',
            't' => 'ţ',
            'u' => 'ú',
            'w' => 'ŵ',
            'y' => 'ý',
            'z' => 'ž',
            'A' => 'Á',
            'C' => 'Ç',
            'D' => 'Ď',
            'E' => 'É',
            'G' => 'Ĝ',
            'H' => 'Ĥ',
            'I' => 'Í',
            'J' => 'Ĵ',
            'K' => 'Ķ',
            'L' => 'Ļ',
            'N' => 'Ñ',
            'O' => 'Ó',
            'R' => 'Ŕ',
            'S' => 'Š',
            'T' => 'Ţ',
            'U' => 'Ú',
            'W' => 'Ŵ',
            'Y' => 'Ý',
            'Z' => 'Ž',
            c => c,
        })
        .collect::<String>()
        .into()
}

/// `qps-ploc`'s whole message: in « », 40% longer (as German and Russian
/// run), so that a message cut short or two messages glued into one run
/// show.
pub fn wrap(text: &str) -> String {
    let n = text.chars().count();
    let pad = (n * 2).div_ceil(5).max(1);
    format!("«{text}{}»", "~".repeat(pad))
}

/// `qps-wide`'s letters: ASCII as its full-width forms, so CJK widths
/// show.
pub fn wide(s: &str) -> Cow<'_, str> {
    s.chars()
        .map(|c| match c {
            ' ' => '\u{3000}',
            '!'..='~' => char::from_u32(c as u32 - 0x21 + 0xFF01).unwrap_or(c),
            c => c,
        })
        .collect::<String>()
        .into()
}
