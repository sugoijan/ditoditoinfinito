//! Numbers in messages, written as the language writes them: its decimal
//! mark and its grouping (`locales/languages.toml`), without ICU's data.

use fluent_bundle::FluentValue;

/// fluent-bundle's formatter hook: numbers in the language
/// [`super::FORMATTING`] names.
pub fn format<M>(value: &FluentValue, _: &M) -> Option<String> {
    let FluentValue::Number(n) = value else {
        return None;
    };
    let language = super::FORMATTING.get();
    Some(write(
        &n.as_string(),
        n.options.use_grouping,
        language.decimal,
        language.group,
        language.min_grouping,
    ))
}

/// `plain` (`-1234.5`, as Rust writes it) with `decimal` for its decimal
/// point and `group` between each three digits of an integer part of at
/// least `3 + min_grouping` digits.
pub fn write(plain: &str, grouping: bool, decimal: &str, group: &str, min_grouping: u32) -> String {
    let (sign, digits) = plain
        .strip_prefix('-')
        .map_or(("", plain), |rest| ("-", rest));
    let (integer, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let mut out = String::from(sign);
    let n = integer.len();
    let grouped =
        grouping && integer.bytes().all(|b| b.is_ascii_digit()) && n >= 3 + min_grouping as usize;
    for (i, c) in integer.chars().enumerate() {
        if grouped && i > 0 && (n - i) % 3 == 0 {
            out.push_str(group);
        }
        out.push(c);
    }
    if !fraction.is_empty() {
        out.push_str(decimal);
        out.push_str(fraction);
    }
    out
}
