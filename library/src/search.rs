//! Text search over the song list: a query's words each found somewhere
//! in a song's text (title, artist, their transliterations, pack, credits,
//! chart names and step artists), in any order.
//!
//! Both sides are folded the same way so that what a player types matches
//! however the simfile spells it: Unicode compatibility decomposition
//! (full-width letters become ASCII, `é` becomes `e` plus an accent),
//! accents dropped, lower case, katakana as hiragana. The decomposition is
//! passed in ([`Fold`]): the app uses the browser's
//! `String.prototype.normalize("NFKD")`, which keeps Unicode's tables out of
//! the wasm; the tests use a stand-in.
//!
//! Each field is also kept romanised ([`romaji`]): kana in Hepburn, then
//! reduced to one spelling ([`canonical`]) so that the romanisations
//! people type meet (`shi`/`si`, `tsu`/`tu`, `ou`/`oo`/`ō`, doubled
//! letters). A word matches a song as typed or romanised, so `sakura`
//! finds さくら, and さくら finds a song transliterated `Sakura`.

/// Unicode NFKD of a string.
pub trait Fold {
    fn nfkd(&self, text: &str) -> String;
}

impl<F: Fn(&str) -> String> Fold for F {
    fn nfkd(&self, text: &str) -> String {
        self(text)
    }
}

/// `text` as searched: decomposed, without accents, lower case, katakana
/// as hiragana, runs of white space as one space. Kana keep their voicing
/// marks (`ば` is not `は`).
pub fn fold(text: &str, folder: &impl Fold) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in folder.nfkd(text).chars() {
        if is_accent(c) {
            continue;
        }
        if c.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        match hiragana(c) {
            Some(h) => out.push(h),
            None => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// Katakana ァ … ヶ as hiragana ぁ … ゖ.
fn hiragana(c: char) -> Option<char> {
    match c {
        '\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60),
        _ => None,
    }
}

/// Combining accents NFKD leaves after a base letter (the kana voicing
/// marks, U+3099 and U+309A, are not among them).
fn is_accent(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{FE20}'..='\u{FE2F}')
}

const DAKUTEN: char = '\u{3099}';
const HANDAKUTEN: char = '\u{309A}';

/// Hepburn for a hiragana letter (small ones as their full size; っ, ー and
/// the voicing marks are handled by [`romaji`]).
fn kana(c: char) -> Option<&'static str> {
    Some(match c {
        'あ' | 'ぁ' => "a",
        'い' | 'ぃ' => "i",
        'う' | 'ぅ' => "u",
        'え' | 'ぇ' => "e",
        'お' | 'ぉ' => "o",
        'か' | 'ゕ' => "ka",
        'き' => "ki",
        'く' => "ku",
        'け' | 'ゖ' => "ke",
        'こ' => "ko",
        'さ' => "sa",
        'し' => "shi",
        'す' => "su",
        'せ' => "se",
        'そ' => "so",
        'た' => "ta",
        'ち' => "chi",
        'つ' => "tsu",
        'て' => "te",
        'と' => "to",
        'な' => "na",
        'に' => "ni",
        'ぬ' => "nu",
        'ね' => "ne",
        'の' => "no",
        'は' => "ha",
        'ひ' => "hi",
        'ふ' => "fu",
        'へ' => "he",
        'ほ' => "ho",
        'ま' => "ma",
        'み' => "mi",
        'む' => "mu",
        'め' => "me",
        'も' => "mo",
        'や' | 'ゃ' => "ya",
        'ゆ' | 'ゅ' => "yu",
        'よ' | 'ょ' => "yo",
        'ら' => "ra",
        'り' => "ri",
        'る' => "ru",
        'れ' => "re",
        'ろ' => "ro",
        'わ' | 'ゎ' => "wa",
        'ゐ' => "wi",
        'ゑ' => "we",
        'を' => "wo",
        'ん' => "n",
        _ => return None,
    })
}

/// A kana's romanisation with a voicing mark: `か゛` is `ga`, `ふ゜` `pu`.
fn voiced(base: &str, mark: char) -> String {
    let semi = mark == HANDAKUTEN;
    match (base, semi) {
        ("shi" | "chi", false) => "ji".into(),
        ("tsu", false) => "zu".into(),
        ("fu", false) => "bu".into(),
        ("fu", true) => "pu".into(),
        ("u", false) => "vu".into(),
        _ => {
            let rest = &base[1..];
            match (&base[..1], semi) {
                ("k", false) => format!("g{rest}"),
                ("s", false) => format!("z{rest}"),
                ("t", false) => format!("d{rest}"),
                ("h", false) => format!("b{rest}"),
                ("h", true) => format!("p{rest}"),
                // ヷ ヸ ヹ ヺ.
                ("w", false) => format!("v{rest}"),
                _ => base.into(),
            }
        }
    }
}

/// `text` romanised: folded as [`fold`] does, its kana in Hepburn (`きゃ`
/// is `kya`, `っ` doubles the next consonant, `ファ` is `fa`, `ー` is
/// dropped, `ゝ` repeats), then [`canonical`].
pub fn romaji(text: &str, folder: &impl Fold) -> String {
    canonical(&hepburn(&fold(text, folder)))
}

/// Folded text with its kana in Hepburn; the rest as it is.
fn hepburn(folded: &str) -> String {
    let small_y = |c: char| match c {
        'ゃ' => Some('a'),
        'ゅ' => Some('u'),
        'ょ' => Some('o'),
        _ => None,
    };
    let small_vowel = |c: char| match c {
        'ぁ' => Some('a'),
        'ぃ' => Some('i'),
        'ぅ' => Some('u'),
        'ぇ' => Some('e'),
        'ぉ' => Some('o'),
        _ => None,
    };
    let chars: Vec<char> = folded.chars().collect();
    let mut out = String::with_capacity(folded.len());
    let mut double = false;
    // The last syllable, for the iteration marks.
    let mut last = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if c == 'っ' {
            double = true;
            continue;
        }
        if c == 'ー' || c == DAKUTEN || c == HANDAKUTEN {
            continue;
        }
        // ゝ and ヽ repeat the syllable before, ゞ and ヾ (NFKD: the same
        // with a voicing mark) voiced.
        let syllable = match c {
            'ゝ' | 'ヽ' if chars.get(i) == Some(&DAKUTEN) && !last.is_empty() => {
                i += 1;
                voiced(&last, DAKUTEN)
            }
            'ゝ' | 'ヽ' => last.clone(),
            'ゞ' | 'ヾ' if !last.is_empty() => voiced(&last, DAKUTEN),
            _ => {
                let Some(base) = kana(c) else {
                    double = false;
                    last.clear();
                    out.push(c);
                    continue;
                };
                let mut syllable = match chars.get(i) {
                    Some(&m) if m == DAKUTEN || m == HANDAKUTEN => {
                        i += 1;
                        voiced(base, m)
                    }
                    _ => base.to_string(),
                };
                // A small ゃ/ゅ/ょ: きゃ kya, しゃ sha, じゃ ja, and after
                // other kana デュ dyu, フュ fyu.
                if let Some(y) = chars.get(i).copied().and_then(small_y)
                    && syllable.len() > 1
                {
                    i += 1;
                    syllable.pop();
                    if !matches!(syllable.as_str(), "sh" | "ch" | "j") {
                        syllable.push('y');
                    }
                    syllable.push(y);
                }
                // A small vowel after a kana makes a sound kana lack: ファ
                // fa, ティ ti, ウィ wi, ヴァ va, シェ she, クァ kwa, イェ ye.
                if let Some(v) = chars.get(i).copied().and_then(small_vowel)
                    && (syllable.len() > 1 || syllable == "u" || syllable == "i")
                {
                    i += 1;
                    let w = matches!(syllable.as_str(), "ku" | "gu");
                    let y = syllable == "i";
                    syllable.pop();
                    if w || syllable.is_empty() {
                        syllable.push(if y { 'y' } else { 'w' });
                    }
                    syllable.push(v);
                }
                syllable
            }
        };
        if double {
            double = false;
            if let Some(first) = syllable.chars().next().filter(|c| !"aiueon".contains(*c)) {
                out.push(first);
            }
        }
        out.push_str(&syllable);
        last = syllable;
    }
    out
}

/// Romaji reduced to one spelling: Hepburn's `shi`, `chi`, `tsu`, `fu`,
/// `ji`, `sh`, `ch`, `j` as Kunrei's `si`, `ti`, `tu`, `hu`, `zi`, `sy`,
/// `ty`, `zy` (`tch` and `cch` as `ty`); `wo` as `o`; `m` before `b`/`p` as `n`; a letter repeated
/// once (`kk`, `aa`, `nn`); `ou` as `o`; apostrophes and hyphens dropped.
/// Applied to English too, consistently on both sides.
pub fn canonical(text: &str) -> String {
    const RULES: [(&str, &str); 19] = [
        // `っち`, `っちゃ` as Hepburn (`tchi`) and as typed (`cchi`).
        ("tchi", "ti"),
        ("cchi", "ti"),
        ("tch", "ty"),
        ("cch", "ty"),
        ("tsu", "tu"),
        ("shi", "si"),
        ("chi", "ti"),
        ("dzu", "zu"),
        ("sh", "sy"),
        ("ch", "ty"),
        ("ji", "zi"),
        ("fu", "hu"),
        ("du", "zu"),
        ("di", "zi"),
        ("wo", "o"),
        ("mb", "nb"),
        ("mm", "nm"),
        ("mp", "np"),
        ("j", "zy"),
    ];
    let mut spelled = String::with_capacity(text.len());
    let mut rest = text;
    'outer: while let Some(c) = rest.chars().next() {
        if c == '\'' || c == '-' || c == '’' {
            rest = &rest[c.len_utf8()..];
            continue;
        }
        for (from, to) in RULES {
            if let Some(after) = rest.strip_prefix(from) {
                spelled.push_str(to);
                rest = after;
                continue 'outer;
            }
        }
        spelled.push(c);
        rest = &rest[c.len_utf8()..];
    }
    let mut out = String::with_capacity(spelled.len());
    for c in spelled.chars() {
        if c.is_ascii_alphabetic() && out.ends_with(c) {
            continue;
        }
        if c == 'u' && out.ends_with('o') {
            continue;
        }
        out.push(c);
    }
    out
}

/// One word of a search, as typed and romanised (as typed, and as the
/// syllable a trailing partial one starts: `ash` as `as…` of `ashita`).
#[derive(Clone, Debug, PartialEq)]
struct Word {
    plain: String,
    romans: Vec<String>,
}

/// A word typed so far may end part-way through a syllable's spelling:
/// `sh`, `ch`, `ts`, `tc`, `cc`, `j`, `f` or `m`, which [`canonical`]
/// spells differently once complete. The spelling it starts.
fn complete_tail(roman: &str) -> Option<String> {
    for (tail, to) in [
        ("sh", "s"),
        ("ch", "t"),
        ("ts", "t"),
        ("tc", "t"),
        ("cc", "t"),
        ("j", "z"),
        ("f", "h"),
        ("m", "n"),
    ] {
        if let Some(head) = roman.strip_suffix(tail) {
            return Some(format!("{head}{to}"));
        }
    }
    None
}

/// A search: the words of what was typed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    words: Vec<Word>,
}

impl Query {
    pub fn new(text: &str, folder: &impl Fold) -> Query {
        Query {
            words: fold(text, folder)
                .split(' ')
                .filter(|w| !w.is_empty())
                .map(|w| {
                    let raw = hepburn(w);
                    let mut romans: Vec<String> = std::iter::once(canonical(&raw))
                        .chain(complete_tail(&raw).map(|t| canonical(&t)))
                        .collect();
                    romans.dedup();
                    // Not when reducing left a letter of more (`zzz` is
                    // `z`, found everywhere).
                    romans.retain(|r| {
                        let n = r.chars().count();
                        n >= 2 || n >= w.chars().count()
                    });
                    Word {
                        plain: w.to_string(),
                        romans,
                    }
                })
                .collect(),
        }
    }

    /// Nothing typed (or only spaces): everything matches.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Whether every word is in `haystack`, a [`haystack`].
    pub fn matches(&self, haystack: &str) -> bool {
        self.matches_in(&[haystack])
    }

    /// Whether every word is in one of `haystacks` (a song's own text and
    /// the text of the charts shown, say), as typed or romanised.
    pub fn matches_in(&self, haystacks: &[&str]) -> bool {
        self.words.iter().all(|w| {
            haystacks.iter().any(|h| {
                let (plain, roman) = h.split_once(ROMAN).unwrap_or((h, ""));
                plain.contains(w.plain.as_str())
                    || w.romans
                        .iter()
                        .any(|r| !r.is_empty() && roman.contains(r.as_str()))
            })
        })
    }
}

/// A song's searchable text, folded once: its fields as typed, joined by
/// line breaks so that no word matches across two of them, then (after
/// [`ROMAN`]) romanised the fields with kana and the transliterations (`translits`,
/// the simfile's own romaji). Latin fields are not romanised: reduced
/// spellings would find `Lemon` for `moon`.
pub fn haystack<'a>(
    fields: impl IntoIterator<Item = &'a str>,
    translits: impl IntoIterator<Item = &'a str>,
    folder: &impl Fold,
) -> String {
    let mut plain = Vec::new();
    let mut roman = Vec::new();
    let fields = fields.into_iter().map(|f| (f, false));
    let translits = translits.into_iter().map(|f| (f, true));
    for (field, translit) in fields.chain(translits) {
        if field.trim().is_empty() {
            continue;
        }
        let folded = fold(field, folder);
        if translit
            || folded
                .chars()
                .any(|c| kana(c).is_some() || "っーゝゞ".contains(c))
        {
            roman.push(canonical(&hepburn(&folded)));
        }
        plain.push(folded);
    }
    format!("{}{ROMAN}{}", plain.join("\n"), roman.join("\n"))
}

/// Between a [`haystack`]'s fields as typed and romanised: words typed are
/// looked for in the first, their romanisations in the second.
const ROMAN: char = '\u{0}';

#[cfg(test)]
mod tests {
    use super::*;

    /// NFKD for the characters these tests use.
    fn nfkd(text: &str) -> String {
        text.chars()
            .flat_map(|c| -> Vec<char> {
                match c {
                    'é' => vec!['e', '\u{0301}'],
                    'Ö' => vec!['O', '\u{0308}'],
                    'ö' => vec!['o', '\u{0308}'],
                    'ド' => vec!['ト', '\u{3099}'],
                    'バ' => vec!['ハ', '\u{3099}'],
                    'ば' => vec!['は', '\u{3099}'],
                    'ど' => vec!['と', '\u{3099}'],
                    'ぱ' => vec!['は', '\u{309A}'],
                    'パ' => vec!['ハ', '\u{309A}'],
                    'プ' => vec!['フ', '\u{309A}'],
                    'デ' => vec!['テ', '\u{3099}'],
                    'じ' => vec!['し', '\u{3099}'],
                    'ゞ' => vec!['ゝ', '\u{3099}'],
                    'が' => vec!['か', '\u{3099}'],
                    'ジ' => vec!['シ', '\u{3099}'],
                    'ヴ' => vec!['ウ', '\u{3099}'],
                    'ō' => vec!['o', '\u{0304}'],
                    'Ａ'..='Ｚ' => vec![char::from_u32(c as u32 - 0xFF21 + 'A' as u32).unwrap()],
                    '\u{3000}' => vec![' '],
                    _ => vec![c],
                }
            })
            .collect()
    }

    fn song(fields: &[&str]) -> String {
        haystack(fields.iter().copied(), [], &nfkd)
    }

    #[test]
    fn every_word_in_any_field_any_order() {
        let s = song(&[
            "Bad Apple!!",
            "Alstroemeria Records",
            "Touhou Fujinya",
            "",
            "nomu",
        ]);
        for q in [
            "bad",
            "APPLE bad",
            "touhou apple",
            "nomu",
            "records alst",
            "  bad   apple ",
        ] {
            assert!(Query::new(q, &nfkd).matches(&s), "{q}");
        }
        for q in ["bad orange", "apple!!!", "applealst"] {
            assert!(!Query::new(q, &nfkd).matches(&s), "{q}");
        }
        // A word is not found across two fields.
        assert!(!Query::new("xst", &nfkd).matches(&song(&["xs", "tx"])));
    }

    #[test]
    fn accents_width_and_kana_fold() {
        let s = song(&["Pokémon", "ＤＪ　ÖSTER", "バッド"]);
        for q in [
            "pokemon",
            "Pokémon",
            "dj öster",
            "dj oster",
            "ばっど",
            "バッド",
        ] {
            assert!(Query::new(q, &nfkd).matches(&s), "{q}");
        }
        // Voicing marks count: は is not ば.
        assert!(!Query::new("はっと", &nfkd).matches(&s));
    }

    #[test]
    fn kana_in_hepburn() {
        let r = |t: &str| romaji(t, &nfkd);
        assert_eq!(r("さくら"), "sakura");
        assert_eq!(r("サクラ"), "sakura");
        // Long vowels, doubled consonants and Hepburn's spellings reduced.
        assert_eq!(r("がっこう"), canonical("gakkou"));
        assert_eq!(r("がっこう"), "gako");
        assert_eq!(r("きゃりーぱみゅぱみゅ"), "kyaripamyupamyu");
        assert_eq!(r("シャドウ"), canonical("shadou"));
        assert_eq!(r("ジャンプ"), canonical("janpu"));
        assert_eq!(r("ティー"), "ti");
        assert_eq!(r("ファイナル"), "fainaru");
        assert_eq!(r("ヴァイオリン"), "vaiorin");
        assert_eq!(r("ウィッチ"), canonical("witchi"));
        assert_eq!(r("ちゅうしん"), canonical("chushin"));
        // Not kana: as folded, then reduced alike.
        assert_eq!(r("Tōkyō"), "tokyo");
    }

    #[test]
    fn spellings_meet() {
        for (a, b) in [
            ("shinkai", "sinkai"),
            ("tsukimi", "tukimi"),
            ("chikyuu", "tikyu"),
            ("fuji", "huzi"),
            ("toukyou", "tookyoo"),
            ("toukyou", "tokyo"),
            ("shimbun", "shinbun"),
            ("kan'i", "kani"),
            ("matcha", "maccha"),
            ("jojo", "zyozyo"),
        ] {
            assert_eq!(canonical(a), canonical(b), "{a} {b}");
        }
    }

    #[test]
    fn romaji_finds_kana_and_kana_finds_romaji() {
        // A kana title without a transliteration.
        let kana = song(&["さくら さくら", "がっこう"]);
        for q in ["sakura", "SAKURA", "gakkou", "gakko", "サクラ"] {
            assert!(Query::new(q, &nfkd).matches(&kana), "{q}");
        }
        assert!(!Query::new("sakuran", &nfkd).matches(&kana));
        // A song found by its kana through its transliterations.
        let latin = haystack(["桜", "東京"], ["Sakura Drops", "Tōkyō Jihen"], &nfkd);
        for q in [
            "さくら",
            "サクラ",
            "とうきょう",
            "tokyo",
            "toukyou",
            "drops",
        ] {
            assert!(Query::new(q, &nfkd).matches(&latin), "{q}");
        }
        assert!(!Query::new("きょうと", &nfkd).matches(&latin));
        // Reduced to almost nothing, a word is only taken as typed.
        let jojo = haystack(["ジョジョ"], ["JoJo", "Jinja"], &nfkd);
        for q in ["zzz", "jjj", "ooo"] {
            assert!(!Query::new(q, &nfkd).matches(&jojo), "{q}");
        }
        assert!(Query::new("zyozyo", &nfkd).matches(&jojo));
    }

    #[test]
    fn words_may_be_in_different_haystacks() {
        let (a, b) = (song(&["Bad Apple!!"]), song(&["nomu"]));
        assert!(Query::new("apple nomu", &nfkd).matches_in(&[&a, &b]));
        assert!(!Query::new("apple nomu", &nfkd).matches_in(&[&a]));
        assert!(Query::new("", &nfkd).matches_in(&[]));
    }

    #[test]
    fn latin_fields_are_not_romanised() {
        // Reduced spellings would find `Lemon` for `moon`.
        let s = song(&["Lemon", "Thunder", "Business", "Until", "Rain"]);
        for q in ["moon", "fun", "shine", "chill", "あい"] {
            assert!(!Query::new(q, &nfkd).matches(&s), "{q}");
        }
        assert!(Query::new("lemon", &nfkd).matches(&s));
    }

    #[test]
    fn a_word_typed_part_way_through_a_syllable() {
        let s = song(&[
            "あした",
            "なつ",
            "ふじ",
            "かち",
            "まっちゃ",
            "さんば",
            "さむらい",
        ]);
        for q in [
            "ash", "ashi", "nats", "natsu", "fuj", "kach", "matc", "sam", "samb", "samu",
        ] {
            assert!(Query::new(q, &nfkd).matches(&s), "{q}");
        }
    }

    #[test]
    fn three_letter_syllables_count() {
        let s = song(&["つき", "しんかい", "こうえん"]);
        for q in ["tsu", "shi", "kou", "chi"] {
            let found = Query::new(q, &nfkd).matches(&s);
            assert_eq!(found, q != "chi", "{q}");
        }
    }

    #[test]
    fn more_kana() {
        let r = |t: &str| romaji(t, &nfkd);
        assert_eq!(r("デュアル"), canonical("dyuaru"));
        assert_eq!(r("フュージョン"), canonical("fyujon"));
        assert_eq!(r("クァ"), "kwa");
        assert_eq!(r("イェス"), "yesu");
        assert_eq!(r("ヴァ"), "va");
        assert_eq!(r("いすゞ"), "isuzu");
        assert_eq!(r("いすゝ"), "isusu");
        assert_eq!(r("さんま"), canonical("samma"));
    }

    #[test]
    fn an_empty_query_is_empty() {
        assert!(Query::new("", &nfkd).is_empty());
        assert!(Query::new("  \u{3000} ", &nfkd).is_empty());
        assert!(Query::new("", &nfkd).matches(""));
    }
}
