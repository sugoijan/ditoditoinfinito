//! A message made into an item (C6, C9, C11): a row of emotes (an
//! emote-only item: up to five, a run of the same one drawn once with a
//! count), or one line of text with its emotes inline, cut at 60
//! graphemes. What C11 leaves out never becomes an item: commands (a
//! leading `!`), messages with a link, cheers, and messages from another
//! channel's shared chat.

use unicode_segmentation::UnicodeSegmentation;

use crate::Level;
use crate::emotes::{Emote, EmoteSets};
use crate::eventsub::{ChatMessage, Fragment};

/// A piece of an item, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    /// An emote drawn once; `count` times in a row in the message.
    Emote { emote: Emote, count: u16 },
}

/// What an item shows, before it is placed.
#[derive(Clone, Debug, PartialEq)]
pub struct Content {
    /// The message's id and its chatter's, for the moderation events.
    pub message_id: String,
    pub chatter_id: String,
    /// The chatter's display name leading it ("Chatter names: On").
    pub name: Option<String>,
    pub pieces: Vec<Piece>,
}

impl Content {
    /// Its emotes, each once.
    pub fn emotes(&self) -> impl Iterator<Item = &Emote> {
        self.pieces.iter().filter_map(|p| match p {
            Piece::Emote { emote, .. } => Some(emote),
            Piece::Text(_) => None,
        })
    }

    /// Whether it has emotes alone.
    pub fn emote_only(&self) -> bool {
        self.pieces.iter().all(|p| match p {
            Piece::Emote { .. } => true,
            Piece::Text(t) => t.trim().is_empty(),
        })
    }
}

/// Why a message is no item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Left {
    Command,
    Link,
    Cheer,
    Shared,
    /// Under "Emotes", a message without one; or nothing left to show.
    Empty,
}

/// The limits an item is cut to (part of `chat-overlay-v0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    /// Runs of emotes an emote-only item shows.
    pub emotes: usize,
    /// Graphemes of a line (an emote counts as one).
    pub graphemes: usize,
}

/// A message's words, each a run of text or an emote.
enum Word {
    Text(String),
    Emote(Emote),
}

/// Splits `text` into words and the spaces between them, matching
/// `sets`' words as emotes.
fn split_text(text: &str, sets: &EmoteSets, out: &mut Vec<Word>) {
    for token in text.split_keeping_spaces() {
        match sets.word(token).filter(|_| !token.trim().is_empty()) {
            Some(e) => out.push(Word::Emote(e.clone())),
            None => out.push(Word::Text(token.to_string())),
        }
    }
}

/// Splitting at whitespace while keeping it (a word of a third-party set
/// is a whole whitespace-separated token: `catJAM`, `:tf:`).
trait SplitWs {
    fn split_keeping_spaces(&self) -> Vec<&str>;
}

impl SplitWs for str {
    fn split_keeping_spaces(&self) -> Vec<&str> {
        let mut out = Vec::new();
        let mut start = 0;
        let mut in_space = None;
        for (i, c) in self.char_indices() {
            let space = c.is_whitespace();
            if in_space.is_some_and(|s| s != space) {
                out.push(&self[start..i]);
                start = i;
            }
            in_space = Some(space);
        }
        if start < self.len() {
            out.push(&self[start..]);
        }
        out
    }
}

/// Whether `word` reads as a link: a scheme, `www.`, or a dotted name
/// ending in a top-level domain chats see links in (with or without a
/// path). PROVISIONAL: what Twitch itself makes clickable is wider.
pub fn is_link(word: &str) -> bool {
    let w = word.trim_matches(|c: char| "()[]<>\"'.,!?".contains(c));
    let lower = w.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("www.") {
        return true;
    }
    let host = lower.split(['/', '?', '#']).next().unwrap_or_default();
    let mut parts = host.rsplit('.');
    let (Some(tld), Some(name)) = (parts.next(), parts.next()) else {
        return false;
    };
    const TLDS: &[&str] = &[
        "com", "net", "org", "tv", "gg", "io", "ly", "me", "co", "be", "jp", "uk", "de", "fr",
        "ru", "info", "xyz", "app", "dev", "link", "live", "shop", "site", "online", "to", "cc",
        "us", "ca", "au", "kr", "cn", "br", "es", "it", "nl", "pl", "se", "gl", "ms", "ai",
    ];
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        && TLDS.contains(&tld)
}

/// `message` as an item under `level` (C11's filters, then the cut).
pub fn content(
    message: &ChatMessage,
    sets: &EmoteSets,
    level: Level,
    names: bool,
    cut: Cut,
) -> Result<Content, Left> {
    if level == Level::Off {
        return Err(Left::Empty);
    }
    if message.shared {
        return Err(Left::Shared);
    }
    if message.cheer {
        return Err(Left::Cheer);
    }
    if message.text.trim_start().starts_with('!') {
        return Err(Left::Command);
    }
    let mut words = Vec::new();
    for f in &message.fragments {
        match f {
            Fragment::Text(t) => split_text(t, sets, &mut words),
            Fragment::Emote { id, still, text } => {
                if *still {
                    words.push(Word::Emote(Emote::twitch(id.clone())));
                } else {
                    words.push(Word::Text(text.clone()));
                }
            }
            Fragment::Mention(t) | Fragment::Other(t) => words.push(Word::Text(t.clone())),
            Fragment::Cheermote(_) => return Err(Left::Cheer),
        }
    }
    // A message without fragments (a shape to come): its text.
    if message.fragments.is_empty() {
        split_text(&message.text, sets, &mut words);
    }
    if words.iter().any(|w| match w {
        Word::Text(t) => t.split_whitespace().any(is_link),
        Word::Emote(_) => false,
    }) {
        return Err(Left::Link);
    }
    // Runs of the same emote, the spaces between them passed over.
    let mut pieces: Vec<Piece> = Vec::new();
    for w in words {
        match w {
            Word::Emote(e) => {
                // The same emote after only spaces: one more of the run.
                let gap = matches!(pieces.last(), Some(Piece::Text(t)) if t.trim().is_empty());
                let before = if gap { pieces.len().checked_sub(2) } else { pieces.len().checked_sub(1) };
                if let Some(Piece::Emote { emote, count }) = before.and_then(|i| pieces.get_mut(i))
                    && *emote == e
                {
                    *count = count.saturating_add(1);
                    if gap {
                        pieces.pop();
                    }
                    continue;
                }
                pieces.push(Piece::Emote { emote: e, count: 1 });
            }
            Word::Text(t) => match pieces.last_mut() {
                Some(Piece::Text(had)) => had.push_str(&t),
                _ => pieces.push(Piece::Text(t)),
            },
        }
    }
    let emote_only = pieces.iter().all(|p| match p {
        Piece::Emote { .. } => true,
        Piece::Text(t) => t.trim().is_empty(),
    });
    let pieces = if level == Level::Emotes || emote_only {
        let emotes: Vec<Piece> = pieces
            .into_iter()
            .filter(|p| matches!(p, Piece::Emote { .. }))
            .take(cut.emotes)
            .collect();
        if emotes.is_empty() {
            return Err(Left::Empty);
        }
        emotes
    } else {
        cut_line(pieces, cut.graphemes)
    };
    if pieces.is_empty() {
        return Err(Left::Empty);
    }
    Ok(Content {
        message_id: message.id.clone(),
        chatter_id: message.chatter_id.clone(),
        name: (names && !message.chatter_name.is_empty()).then(|| message.chatter_name.clone()),
        pieces,
    })
}

/// One line: whitespace runs made one space, the ends trimmed, cut at
/// `most` graphemes (an emote one) with an ellipsis.
fn cut_line(pieces: Vec<Piece>, most: usize) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut left = most;
    let n = pieces.len();
    for (i, p) in pieces.into_iter().enumerate() {
        if left == 0 {
            break;
        }
        match p {
            Piece::Text(t) => {
                let mut s = t.split_whitespace().collect::<Vec<_>>().join(" ");
                if t.starts_with(char::is_whitespace) && i > 0 {
                    s.insert(0, ' ');
                }
                if t.ends_with(char::is_whitespace) && i + 1 < n && !s.is_empty() && !s.ends_with(' ') {
                    s.push(' ');
                }
                if s.trim().is_empty() && (i == 0 || i + 1 == n) {
                    continue;
                }
                let g: Vec<&str> = s.graphemes(true).collect();
                if g.len() > left {
                    let mut kept: String = g[..left].concat().trim_end().to_string();
                    kept.push('…');
                    out.push(Piece::Text(kept));
                    left = 0;
                } else {
                    left -= g.len();
                    out.push(Piece::Text(s));
                }
            }
            e @ Piece::Emote { .. } => {
                left -= 1;
                out.push(e);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emotes::{Image, Provider, Source};

    const CUT: Cut = Cut {
        emotes: 5,
        graphemes: 60,
    };

    fn message(fragments: Vec<Fragment>) -> ChatMessage {
        let text = fragments
            .iter()
            .map(|f| match f {
                Fragment::Text(t)
                | Fragment::Cheermote(t)
                | Fragment::Mention(t)
                | Fragment::Other(t) => t.clone(),
                Fragment::Emote { text, .. } => text.clone(),
            })
            .collect();
        ChatMessage {
            id: "m1".into(),
            chatter_id: "u1".into(),
            chatter_name: "Ann".into(),
            text,
            fragments,
            kind: "text".into(),
            ..ChatMessage::default()
        }
    }

    fn kappa() -> Fragment {
        Fragment::Emote {
            text: "Kappa".into(),
            id: "25".into(),
            still: true,
        }
    }

    fn sets() -> EmoteSets {
        let mut s = EmoteSets::default();
        s.add(
            Source::Channel(Provider::SevenTv),
            vec![(
                "catJAM".into(),
                Emote::new(Provider::SevenTv, "c"),
                Image {
                    url: "https://cdn.7tv.app/emote/c/2x_static.webp".into(),
                    proxy: None,
                },
            )],
        );
        s
    }

    fn t(s: &str) -> Fragment {
        Fragment::Text(s.into())
    }

    #[test]
    fn a_message_keeps_its_emotes_inline() {
        let m = message(vec![t("Hi chat "), kappa(), t(" catJAM  nice")]);
        let c = content(&m, &sets(), Level::Messages, false, CUT).unwrap();
        assert_eq!(
            c.pieces,
            [
                Piece::Text("Hi chat ".into()),
                Piece::Emote {
                    emote: Emote::twitch("25"),
                    count: 1
                },
                Piece::Text(" ".into()),
                Piece::Emote {
                    emote: Emote::new(Provider::SevenTv, "c"),
                    count: 1
                },
                Piece::Text(" nice".into()),
            ]
        );
        assert_eq!(c.name, None);
        assert_eq!(
            content(&m, &sets(), Level::Messages, true, CUT).unwrap().name.as_deref(),
            Some("Ann")
        );
    }

    #[test]
    fn under_emotes_only_the_emotes_show_and_runs_count() {
        let m = message(vec![
            kappa(),
            t(" "),
            kappa(),
            t(" "),
            kappa(),
            t(" catJAM hello"),
        ]);
        let c = content(&m, &sets(), Level::Emotes, false, CUT).unwrap();
        assert_eq!(
            c.pieces,
            [
                Piece::Emote {
                    emote: Emote::twitch("25"),
                    count: 3
                },
                Piece::Emote {
                    emote: Emote::new(Provider::SevenTv, "c"),
                    count: 1
                },
            ]
        );
        // No emote: no item.
        assert_eq!(
            content(&message(vec![t("hello")]), &sets(), Level::Emotes, false, CUT),
            Err(Left::Empty)
        );
    }

    #[test]
    fn an_emote_only_item_shows_five_runs() {
        let mut f = Vec::new();
        for i in 0..8 {
            f.push(Fragment::Emote {
                text: format!("e{i}"),
                id: i.to_string(),
                still: true,
            });
            f.push(t(" "));
        }
        let c = content(&message(f), &sets(), Level::Messages, false, CUT).unwrap();
        assert_eq!(c.pieces.len(), 5);
        assert!(c.emote_only());
    }

    #[test]
    fn a_line_is_cut_at_sixty_graphemes() {
        let long = "あ".repeat(70);
        let c = content(&message(vec![t(&long)]), &sets(), Level::Messages, false, CUT).unwrap();
        let Piece::Text(s) = &c.pieces[0] else {
            panic!()
        };
        assert_eq!(s.graphemes(true).count(), 61);
        assert!(s.ends_with('…'));
        // A family emoji is one grapheme.
        let family = "👨‍👩‍👧".repeat(65);
        let c = content(&message(vec![t(&family)]), &sets(), Level::Messages, false, CUT).unwrap();
        let Piece::Text(s) = &c.pieces[0] else {
            panic!()
        };
        assert_eq!(s.graphemes(true).count(), 61);
    }

    #[test]
    fn commands_links_cheers_and_shared_chat_are_left_out() {
        let s = sets();
        let c = |m: &ChatMessage| content(m, &s, Level::Messages, false, CUT);
        assert_eq!(c(&message(vec![t("!song")])), Err(Left::Command));
        assert_eq!(c(&message(vec![t("  !drop me")])), Err(Left::Command));
        for link in [
            "see https://example.test/x",
            "www.example.com",
            "go to example.com/abc now",
            "(twitch.tv/foo)",
        ] {
            assert_eq!(c(&message(vec![t(link)])), Err(Left::Link), "{link}");
        }
        for fine in ["3.14 is pi", "lol...", "e.g. this", "v1.2"] {
            assert!(c(&message(vec![t(fine)])).is_ok(), "{fine}");
        }
        let mut cheer = message(vec![Fragment::Cheermote("cheer100".into()), t(" gl")]);
        cheer.cheer = true;
        assert_eq!(c(&cheer), Err(Left::Cheer));
        let mut shared = message(vec![t("hi")]);
        shared.shared = true;
        assert_eq!(c(&shared), Err(Left::Shared));
    }

    #[test]
    fn every_script_stays_text() {
        let s = sets();
        for line in ["مرحبا بالجميع", "こんにちは、みなさん！", "🎉🎉 nice 🎶", "Привет"] {
            let c = content(&message(vec![t(line)]), &s, Level::Messages, false, CUT).unwrap();
            assert_eq!(c.pieces, [Piece::Text(line.into())]);
        }
    }
}
