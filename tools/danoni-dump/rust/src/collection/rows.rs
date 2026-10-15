//! Bounded structural evidence for reviewed static list layouts, not a DOM.
use crate::{Error, Result};
use html5ever::{
    tendril::StrTendril,
    tokenizer::{
        BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, states::RawKind,
    },
};
use std::cell::RefCell;
#[derive(Clone, Default)]
pub(super) struct Cell {
    pub text: String,
    pub links: Vec<(String, String)>,
}
#[derive(Default)]
struct Table {
    row: Vec<Cell>,
    cell: Option<Cell>,
}
#[derive(Default)]
struct State {
    tables: Vec<Table>,
    rows: Vec<Vec<Cell>>,
    music: Vec<Cell>,
    active_music: Option<Cell>,
    music_depth: usize,
    anchor: Option<(String, String)>,
    raw: Option<String>,
    tags: usize,
    links: usize,
    overflow: bool,
}
impl State {
    fn text(&mut self, s: &str) {
        if let Some((_, text)) = &mut self.anchor {
            text.push_str(s);
            if text.len() > 4096 {
                self.overflow = true;
            }
        }
        if let Some(c) = self.tables.last_mut().and_then(|t| t.cell.as_mut()) {
            c.text.push_str(s);
            if c.text.len() > 16384 {
                self.overflow = true;
            }
        }
        if let Some(c) = &mut self.active_music {
            c.text.push_str(s);
            if c.text.len() > 16384 {
                self.overflow = true;
            }
        }
    }
    fn finish_anchor(&mut self) {
        if let Some(a) = self.anchor.take() {
            if let Some(c) = self.tables.last_mut().and_then(|t| t.cell.as_mut()) {
                c.links.push(a.clone());
            }
            if let Some(c) = &mut self.active_music {
                c.links.push(a);
            }
        }
    }
    fn cell(&mut self) {
        self.finish_anchor();
        if let Some(t) = self.tables.last_mut()
            && let Some(c) = t.cell.take()
        {
            t.row.push(c);
            if t.row.len() > 64 {
                self.overflow = true;
            }
        }
    }
    fn row(&mut self) {
        self.cell();
        if let Some(t) = self.tables.last_mut()
            && !t.row.is_empty()
        {
            self.rows.push(std::mem::take(&mut t.row));
            if self.rows.len() > 2048 {
                self.overflow = true;
            }
        }
    }
}
#[derive(Default)]
struct Sink(RefCell<State>);
impl TokenSink for Sink {
    type Handle = ();
    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        let mut s = self.0.borrow_mut();
        if s.overflow {
            return TokenSinkResult::Continue;
        }
        if let Token::CharacterTokens(chars) = &token {
            if s.raw.is_none() {
                s.text(chars);
            }
            return TokenSinkResult::Continue;
        }
        let Token::TagToken(tag) = token else {
            return TokenSinkResult::Continue;
        };
        s.tags += 1;
        if s.tags > 32768 {
            s.overflow = true;
            return TokenSinkResult::Continue;
        }
        let name = tag.name.as_ref();
        if let Some(raw) = &s.raw {
            if tag.kind == TagKind::EndTag && raw == name {
                s.raw = None;
            }
            return TokenSinkResult::Continue;
        }
        if tag.kind == TagKind::EndTag {
            match name {
                "a" => s.finish_anchor(),
                "td" | "th" => s.cell(),
                "tr" => s.row(),
                "table" => {
                    s.row();
                    s.tables.pop();
                }
                "div" if s.active_music.is_some() => {
                    s.music_depth -= 1;
                    if s.music_depth == 0 {
                        s.finish_anchor();
                        let cell = s.active_music.take().unwrap();
                        s.music.push(cell);
                    }
                }
                _ => {}
            }
            return TokenSinkResult::Continue;
        }
        let attr = |key: &str| {
            tag.attrs
                .iter()
                .find(|a| &*a.name.local == key)
                .map(|a| a.value.to_string())
        };
        match name {
            "table" => {
                if s.tables.len() >= 64 {
                    s.overflow = true;
                } else {
                    s.tables.push(Table::default());
                }
            }
            "tr" => s.row(),
            "td" | "th" => {
                s.cell();
                if let Some(t) = s.tables.last_mut() {
                    t.cell = Some(Cell::default());
                }
            }
            "a" => {
                s.finish_anchor();
                if let Some(href) = attr("href") {
                    s.links += 1;
                    if s.links > 2048 {
                        s.overflow = true;
                    } else {
                        s.anchor = Some((href, String::new()));
                    }
                }
            }
            "div" => {
                if s.active_music.is_some() {
                    s.music_depth += 1;
                    if s.music_depth > 64 {
                        s.overflow = true;
                    }
                } else if attr("id").as_deref() == Some("music") {
                    s.active_music = Some(Cell::default());
                    s.music_depth = 1;
                }
            }
            "br" => s.text(" "),
            "script" => {
                s.raw = Some(name.into());
                return TokenSinkResult::RawData(RawKind::ScriptData);
            }
            "style" | "xmp" | "noembed" | "noframes" => {
                s.raw = Some(name.into());
                return TokenSinkResult::RawData(RawKind::Rawtext);
            }
            "title" | "textarea" => {
                s.raw = Some(name.into());
                return TokenSinkResult::RawData(RawKind::Rcdata);
            }
            "plaintext" => {
                s.raw = Some(name.into());
                return TokenSinkResult::Plaintext;
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}
pub(super) struct Rows {
    pub rows: Vec<Vec<Cell>>,
    pub music: Vec<Cell>,
}
pub(super) fn parse(text: &str) -> Result<Rows> {
    let q = BufferQueue::default();
    q.push_back(StrTendril::from_slice(text));
    let parser = Tokenizer::new(Sink::default(), Default::default());
    let _ = parser.feed(&q);
    parser.end();
    let s = parser.sink.0.into_inner();
    if s.overflow {
        return Err(Error::new(
            "index-structure-limit",
            "Index rows, cells, text or nesting exceed limits",
        ));
    }
    if s.anchor.is_some()
        || s.active_music.is_some()
        || s.tables
            .iter()
            .any(|t| t.cell.is_some() || !t.row.is_empty())
    {
        return Err(Error::new(
            "index-structure-incomplete",
            "Incomplete work containers cannot authorize child URLs",
        ));
    }
    Ok(Rows {
        rows: s.rows,
        music: s.music,
    })
}
pub(super) fn label(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(512)
        .collect()
}
