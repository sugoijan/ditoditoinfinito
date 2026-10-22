//! Simfile text in the legacy code pages packs were written in, told
//! apart where the bytes are not UTF-8 (`docs/plans/format-gaps.md`,
//! "Legacy encodings"). StepMania itself tries UTF-8 and then the
//! system's code page, so a Korean pack shows its titles only on a Korean
//! Windows; here the text decides.
//!
//! The decoders are the platform's (the browser's `TextDecoder` on the
//! web, `encoding_rs` on desktop), each strict: `None` on any sequence the
//! code page lacks. What they give is then judged:
//!
//! 1. Shift-JIS read as Japanese ([`crate::pack::reads_as_japanese`]) with
//!    kana in it is taken at once: Korean and Chinese text, whose bytes
//!    are mostly pairs from 0xA1 up, decodes in Shift-JIS as half-width
//!    katakana and kanji, never as full-width kana.
//! 2. Otherwise EUC-KR (Windows-949), GBK and Big5 are each read, and a
//!    reading counts when every character is of its script (Hangul with
//!    few hanja; ideographs, kana and CJK punctuation), its bytes are
//!    those its text mostly uses (KS X 1001's and GB 2312's rows, Big5's
//!    frequent characters), and enough of its letters are among the most
//!    common of its language ([`COMMON_HANGUL`], [`COMMON_HANZI`]): the
//!    three code pages share their byte ranges, so the same bytes read in
//!    the wrong one give rare syllables or characters. The reading with
//!    the most common letters wins; Shift-JIS read as Japanese without
//!    kana (kanji or half-width katakana only) stands unless one does
//!    clearly better.
//! 3. Else Windows-1252, byte for byte.

/// A legacy code page a simfile may be in, by its WHATWG label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Legacy {
    ShiftJis,
    /// EUC-KR as browsers have it: Windows-949 (Unified Hangul Code).
    EucKr,
    /// GBK as browsers have it: the gb18030 decoder.
    Gbk,
    /// Big5 as browsers have it: with the HKSCS extensions.
    Big5,
}

impl Legacy {
    pub const ALL: [Legacy; 4] = [Legacy::ShiftJis, Legacy::EucKr, Legacy::Gbk, Legacy::Big5];

    /// Its label for `TextDecoder` and `encoding_rs::Encoding::for_label`.
    pub const fn label(self) -> &'static str {
        match self {
            Legacy::ShiftJis => "shift_jis",
            Legacy::EucKr => "euc-kr",
            Legacy::Gbk => "gbk",
            Legacy::Big5 => "big5",
        }
    }
}

/// A strict decoder of the platform: the text of `bytes` in a code page,
/// `None` when they are not valid in it (or the platform has none).
pub type Decoder<'a> = &'a dyn Fn(Legacy, &[u8]) -> Option<String>;

/// No legacy decoder: text that is not UTF-8 is Windows-1252.
pub fn none(_: Legacy, _: &[u8]) -> Option<String> {
    None
}

/// The share of its letters a reading needs among the most common of
/// its language to be taken, and to be taken over Shift-JIS read as
/// Japanese without kana.
const TAKEN: f32 = 0.35;
const OVER_KANJI: f32 = 0.5;

/// `bytes` (not UTF-8) in the legacy code page their text reads in, and
/// which, as the module explains; `None` when none reads.
pub fn decode(bytes: &[u8], decode: Decoder) -> Option<(Legacy, String)> {
    let sjis = decode(Legacy::ShiftJis, bytes).filter(|s| crate::pack::reads_as_japanese(s));
    if sjis.as_ref().is_some_and(|s| s.chars().any(is_kana)) {
        return sjis.map(|s| (Legacy::ShiftJis, s));
    }
    let mut best: Option<(f32, Legacy, String)> = None;
    for legacy in [Legacy::EucKr, Legacy::Gbk, Legacy::Big5] {
        if share(bytes, legacy) < 0.8 {
            continue;
        }
        let Some(text) = decode(legacy, bytes) else {
            continue;
        };
        let score = match legacy {
            Legacy::EucKr => korean_score(&text),
            _ => chinese_score(&text),
        };
        if let Some(score) = score.filter(|s| *s >= TAKEN)
            && best.as_ref().is_none_or(|(b, ..)| score > *b)
        {
            best = Some((score, legacy, text));
        }
    }
    match (best, sjis) {
        (Some((score, legacy, text)), sjis) if sjis.is_none() || score >= OVER_KANJI => {
            Some((legacy, text))
        }
        (_, sjis) => sjis.map(|s| (Legacy::ShiftJis, s)),
    }
}

/// Of the two-byte characters of `bytes` read in `legacy`, the share in
/// the rows its text mostly uses: KS X 1001's (lead and trail from 0xA1;
/// Windows-949's extra Hangul, lead below 0xC7, also), GB 2312's (lead
/// 0xA1–0xF7, trail from 0xA1), Big5's symbols and frequent characters
/// (0xA140–0xC67E). 0 without any.
fn share(bytes: &[u8], legacy: Legacy) -> f32 {
    let (mut all, mut inside) = (0u32, 0u32);
    let mut i = 0;
    while i < bytes.len() {
        let lead = bytes[i];
        if lead < 0x80 {
            i += 1;
            continue;
        }
        let Some(&trail) = bytes.get(i + 1) else {
            all += 1;
            break;
        };
        all += 1;
        let ok = match legacy {
            Legacy::EucKr => {
                (lead >= 0xA1 && trail >= 0xA1)
                    || ((0x81..=0xC6).contains(&lead)
                        && matches!(trail, 0x41..=0x5A | 0x61..=0x7A | 0x81..=0xFE))
            }
            Legacy::Gbk => (0xA1..=0xF7).contains(&lead) && trail >= 0xA1,
            Legacy::Big5 => {
                let code = u16::from(lead) << 8 | u16::from(trail);
                (0xA140..=0xC67E).contains(&code) && matches!(trail, 0x40..=0x7E | 0xA1..=0xFE)
            }
            Legacy::ShiftJis => true,
        };
        inside += u32::from(ok);
        i += 2;
    }
    if all == 0 {
        0.0
    } else {
        inside as f32 / all as f32
    }
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{30FF}')
}

fn is_ideograph(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}')
}

fn is_hangul(c: char) -> bool {
    matches!(c, '\u{AC00}'..='\u{D7A3}')
}

/// Punctuation and symbols the three code pages share with their text.
fn is_symbol(c: char) -> bool {
    matches!(c,
        '\u{00A1}' | '\u{00A7}' | '\u{00A8}' | '\u{00B0}' | '\u{00B1}' | '\u{00B4}' | '\u{00B6}'
        | '\u{00B7}' | '\u{00BF}' | '\u{00D7}' | '\u{00F7}'
        | '\u{0391}'..='\u{03C9}' // Greek
        | '\u{0401}'..='\u{0451}' // Cyrillic
        | '\u{2010}'..='\u{27BF}' // punctuation, arrows, maths, box drawing, ★, ♪
        | '\u{3000}'..='\u{303F}' // CJK punctuation
        | '\u{3131}'..='\u{318E}' // Hangul letters
        | '\u{3200}'..='\u{33FF}' // enclosed letters, units
        | '\u{FF01}'..='\u{FF5E}' // full-width forms
        | '\u{FFE0}'..='\u{FFE6}'
    )
}

/// The share of a Korean reading's Hangul among the most common
/// syllables, when it reads as Korean: every other character ASCII (no
/// control but line breaks and tabs), a symbol or a hanja, at most half
/// as many hanja as syllables. `None` otherwise.
fn korean_score(s: &str) -> Option<f32> {
    let (mut hangul, mut common, mut hanja) = (0u32, 0u32, 0u32);
    for c in s.chars() {
        if is_hangul(c) {
            hangul += 1;
            common += u32::from(COMMON_HANGUL.contains(c));
        } else if is_ideograph(c) {
            hanja += 1;
        } else if !(plain_ascii(c) || is_symbol(c)) {
            return None;
        }
    }
    (hangul > 0 && hanja * 2 <= hangul).then(|| common as f32 / hangul as f32)
}

/// The share of a Chinese reading's letters (ideographs and kana) among
/// the most common, when it reads as Chinese: every other character
/// ASCII or a symbol, and no ideograph alone between ASCII letters or
/// digits (Western text, accented letters read as ideographs). `None`
/// otherwise.
fn chinese_score(s: &str) -> Option<f32> {
    let chars: Vec<char> = s.chars().collect();
    let latin = |i: Option<usize>| {
        i.and_then(|i| chars.get(i))
            .is_some_and(char::is_ascii_alphanumeric)
    };
    let (mut letters, mut common) = (0u32, 0u32);
    for (i, &c) in chars.iter().enumerate() {
        if is_ideograph(c) {
            let lone = !chars.get(i + 1).copied().is_some_and(is_ideograph)
                && !i
                    .checked_sub(1)
                    .and_then(|j| chars.get(j))
                    .copied()
                    .is_some_and(is_ideograph);
            if lone && (latin(i.checked_sub(1)) || latin(Some(i + 1))) {
                return None;
            }
            letters += 1;
            common += u32::from(COMMON_HANZI.contains(c));
        } else if is_kana(c) {
            letters += 1;
            common += 1;
        } else if !(plain_ascii(c) || is_symbol(c)) {
            return None;
        }
    }
    (letters > 0).then(|| common as f32 / letters as f32)
}

fn plain_ascii(c: char) -> bool {
    c.is_ascii() && (!c.is_ascii_control() || matches!(c, '\n' | '\r' | '\t'))
}

/// Some 300 of the most used Hangul syllables (particles, endings, the
/// words of song titles), written for this check.
const COMMON_HANGUL: &str = "\
가각간갈감갑강같개거걸게겠겨결경계고곡곳공과관광괜교구국군굴권귀그근글금기길김까깨꺼께꼬꽃꿈끝나난날남내너널넘네녀년노놀누눈뉴느는늘니님다단달담당대더던데도독돌동되된될두둘뒤드든들듯등디따때또뜨라락란람랑래러런럼렇레려력련렬령로록론롭료루류르른를름리린림마막만많말맘망매머먹멀메며면명모목몸못무문물뭐므미민밀바박반받발밤방배백버번벌범베벽별병보복본봄봐부북분불브비빛빠사산살삼상새색생서석선설성세소속손송수숙순술숨쉬스슬습시식신실심싶아안않알암았앞애야약양어억언얼엄없었에여역연열영예오온올와완왜외요용우운울움웃워원월위유으은을음응의이인일임입있잊자작잘잠장재저적전절점정제조좀종좋주죽준줄중즈즘지진질짐집짜쪽쯤찾처천철청체초촌추축춘출춤충치친칠카커케코크키타탄태터테토통트티파판퍼페편평포표푸프플피하학한할함합항해했행향허헤현형호혼화확환활회후훈휘흐흔희히힘";

/// Some 600 of the most used Chinese characters, simplified and
/// traditional, with those song titles favor, written for this check.
const COMMON_HANZI: &str = "\
的一是不了人我在有他这中大来上国个到说们为子和你地出道也时年得就那要下以生会自着去之过家学对可她里后小么心多天而能好都然没日于起还发成事只作当想看文无开手十用主行方又如前所本见经头面公同三已老从动两长知民样现分将外但身些与高意进把法此实回二理美点月明其种声全工己话儿者向情部正名定女问力机给等几很业最间新什打便位因重被走电四第门相次东政海口使教西再平真听世气信北少关并内加化由却代军产入先山五太水万市眼体别处总才场师书比住员九笑性通目华报立马命张活难神数件安表原车白应路期叫死常提感金何更反合放做系计或司利受光王果亲界及今京务制解各任至清物台象记边共风战干接它许八特觉望直服毛林题建南度统色字请交爱让认算论百吃义科怎元社术结六功指思非流每青管夫连远资队跟带花快条院变联言权往展该领传近留红治决周保达办运武半候七必城父强步完革深区即求品士转量空甚众技轻程告江语英基派满式李息写呢识极令黄德收脸钱党倒未持取设始版双历越史商千片容研像找友孩站广改议形委早房音火际则首单据导影失拿网香似斯专石若兵弟谁校读志飞观争究包组造落视济喜离虽坐集编宝谈府拉黑且随格尽剑讲布杀微怕母调局根曾准团段终乐切级克精哪官示冷域病啊乱\
這來國個說們為時會著過學對裡後麼沒於還發當無開見經頭從動兩長樣現將與進點種聲話兒問機給幾業間電門東聽氣關內軍產萬體別處總場師書員華報馬張難數車應麗紅決雙寫識極黃錢黨設歷戰覺樂聯傳歲廣議單據導網專兵飛觀爭組視濟離雖編寶談隨盡劍講殺調團級隊帶條變聯權領遠資親記邊風許統請愛讓認論義術結這戀夢淚櫻雲藍幸傷憶鬥鳳龍貓鳥魚閃燈雙響鐘號輪蝶\
梦恋歌舞星夜雨雪春秋冬夏泪樱云蓝幸福伤悲忆念吻魂鬼龙凤狐猫狗鸟鱼曲唱跳闪灯响钟号轮蝶翼羽恋梦泪樱蝶舞歌姬君吾汝妳俺僕";

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::pack::{decode_legacy_names, decode_text_with};

    /// The platform's strict decoder, as desktop has it.
    pub(crate) fn decoder(legacy: Legacy, bytes: &[u8]) -> Option<String> {
        let encoding = encoding_rs::Encoding::for_label(legacy.label().as_bytes())?;
        let (text, had_errors) = encoding.decode_without_bom_handling(bytes);
        (!had_errors).then(|| text.into_owned())
    }

    fn encode(label: &str, text: &str) -> Vec<u8> {
        let encoding = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
        let (bytes, _, unmappable) = encoding.encode(text);
        assert!(!unmappable, "{text} in {label}");
        bytes.into_owned()
    }

    #[test]
    fn each_language_reads_in_its_code_page() {
        let cases: [(&str, &[&str]); 4] = [
            (
                "euc-kr",
                &[
                    "#TITLE:사랑해요;\n#ARTIST:가수;",
                    "#TITLE:너를 위해;",
                    "#TITLE:눈의 꽃;\n#ARTIST:박효신;\n#CREDIT:만든 사람;",
                    "#TITLE:하늘 (Remix);",
                ],
            ),
            (
                "gbk",
                &[
                    "#TITLE:青花瓷;\n#ARTIST:周杰伦;",
                    "#TITLE:七里香;",
                    "#TITLE:我们的歌;\n#CREDIT:制作;",
                    "#TITLE:千本桜;\n#SUBTITLE:初音ミク;",
                ],
            ),
            (
                "big5",
                &[
                    "#TITLE:青花瓷;\n#ARTIST:周杰倫;",
                    "#TITLE:夜曲;",
                    "#TITLE:愛的飛行日記;",
                    "#TITLE:說好的幸福呢;",
                ],
            ),
            (
                "shift_jis",
                &[
                    "#TITLE:恋のソナタ;",
                    "#TITLE:桜;",
                    "#TITLE:千本桜;",
                    "#TITLE:ｶﾞﾝﾀﾞﾑ;",
                    "#TITLE:夜明けと蛍;\n#ARTIST:ナブナ;",
                ],
            ),
        ];
        for (label, texts) in cases {
            for text in texts {
                let bytes = encode(label, text);
                assert_eq!(decode_text_with(&bytes, decoder), *text, "{label}");
            }
        }
    }

    #[test]
    fn western_text_stays_windows_1252() {
        for text in [
            "#TITLE:Don’t Stop;",
            "#TITLE:Pokémon;",
            "#TITLE:Ökostrom;",
            "#TITLE:Straße;",
            "#TITLE:Ça va;",
            "#CREDIT:© 2005;",
            "#TITLE:10°C;",
            "#TITLE:½;",
            "#TITLE:Résumé;",
            "#TITLE:Señor Café;\n#ARTIST:Björk;",
            "#TITLE:Ærø;",
            "#TITLE:Ñandú;",
            "#TITLE:Déjà vu;",
            "#ARTIST:Motörhead;",
        ] {
            let cp = encode("windows-1252", text);
            assert_eq!(decode_text_with(&cp, decoder), text, "{text}");
        }
    }

    #[test]
    fn readings_are_told_apart() {
        let korean = encode("euc-kr", "#TITLE:사랑의 노래;");
        assert_eq!(decode(&korean, &decoder).unwrap().0, Legacy::EucKr);
        let chinese = encode("gbk", "#TITLE:我爱你中国;");
        assert_eq!(decode(&chinese, &decoder).unwrap().0, Legacy::Gbk);
        let big5 = encode("big5", "#TITLE:我愛你，台灣;");
        assert_eq!(decode(&big5, &decoder).unwrap().0, Legacy::Big5);
        // Nothing reads: none.
        assert_eq!(decode(b"\xff\xfe\xfd", &decoder), None);
        assert_eq!(decode(b"\xc0\xc1", &none), None);
    }

    /// `DDI_ENCODING_CORPUS=<dir>`: every simfile under it that is not
    /// UTF-8, with the code page it reads in and its title (local packs
    /// stay local).
    #[test]
    #[ignore]
    fn local_corpus() {
        let Ok(root) = std::env::var("DDI_ENCODING_CORPUS") else {
            return;
        };
        let mut dirs = vec![std::path::PathBuf::from(root)];
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        while let Some(dir) = dirs.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !["sm", "ssc", "dwi"]
                    .iter()
                    .any(|x| ext.eq_ignore_ascii_case(x))
                {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&path) else {
                    continue;
                };
                if std::str::from_utf8(&bytes).is_ok() {
                    continue;
                }
                let read = decode(&bytes, &decoder);
                let page = read.as_ref().map_or("windows-1252", |(l, _)| l.label());
                *counts.entry(page.to_string()).or_default() += 1;
                let text = decode_text_with(&bytes, decoder);
                let title = text.lines().find(|l| l.starts_with("#TITLE")).unwrap_or("");
                println!("{page}\t{title}\t{}", path.display());
            }
        }
        println!("{counts:?}");
    }

    #[test]
    fn zip_names_in_korean_and_chinese() {
        let a = encode("euc-kr", "노래 모음/사랑/song.sm");
        let b = encode("euc-kr", "노래 모음/사랑/음악.ogg");
        assert_eq!(
            decode_legacy_names(&[&a, &b], decoder),
            ["노래 모음/사랑/song.sm", "노래 모음/사랑/음악.ogg"]
        );
        let a = encode("gbk", "中文歌曲/青花瓷/青花瓷.sm");
        assert_eq!(
            decode_legacy_names(&[&a], decoder),
            ["中文歌曲/青花瓷/青花瓷.sm"]
        );
    }
}
