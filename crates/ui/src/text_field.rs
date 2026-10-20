//! A single line of editable text (`docs/plans/ui-core.md`, P5): its
//! text, selection and composition, in two modes.
//!
//! - **Mirror** (the web): the platform edits. A hidden textarea holds the
//!   text, and every event re-reads its value, selection and composition
//!   ([`TextField::sync`]); the field only shows them. Clipboard, undo and
//!   the IME come with the textarea.
//! - **Own** (desktop): the field edits, from keys ([`TextField::key`])
//!   and winit's IME events ([`TextField::set_preedit`],
//!   [`TextField::commit`]). Editing moves by grapheme clusters
//!   (unicode-segmentation's extended clusters: `が` typed as か and a
//!   combining mark, an emoji with its modifiers, are one step), and by
//!   words with Ctrl or Alt (⌥ on macOS), ⌘ going to either end.
//!
//! Offsets are UTF-8 byte offsets into the text, always on character
//! boundaries (own mode keeps them on grapheme boundaries too); the DOM's
//! UTF-16 offsets are converted at the edge ([`utf16_to_byte`],
//! [`byte_to_utf16`]). The composition (preedit) is shown in the text,
//! marked ([`Display::preedit`]) so it can be drawn distinctly (an
//! underline, as IMEs expect).

use std::ops::Range;

use ddi_actor::text::{TextMeasure, TextStyle};
use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use crate::event::{KeyEvent, TextSync};

/// Who edits the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextMode {
    /// The platform (the web's hidden textarea); the field mirrors it.
    Mirror,
    /// We do (desktop).
    Own,
}

/// What a key did to a field ([`TextField::key`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyResult {
    /// The text changed.
    Edited,
    /// Only the caret or the selection moved.
    Moved,
    /// The field's key, with nothing to do (Left at the start).
    Consumed,
    /// Enter.
    Submit,
    /// Escape.
    Cancel,
    /// Copy (or cut, which also edited): this text to the clipboard.
    Copy { text: String, cut: bool },
    /// Paste: the shell is to read the clipboard.
    Paste,
    /// Not the field's (Up, Down, Tab, a key it does not know): for the
    /// screen, an autocomplete list say.
    Ignored,
}

/// What is shown: the text with any preedit in it, and where the caret,
/// the selection and the preedit are in it (byte offsets).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub text: String,
    pub caret: usize,
    pub selection: Range<usize>,
    pub preedit: Option<Range<usize>>,
}

/// Where the parts of [`Display`] are along the run, in the style's units
/// from the pen (the run's left edge).
#[derive(Clone, Debug, PartialEq)]
pub struct FieldLayout {
    pub caret: f32,
    /// `[from, to]`; `None` when nothing is selected.
    pub selection: Option<[f32; 2]>,
    pub preedit: Option<[f32; 2]>,
    /// The whole run's advance.
    pub width: f32,
}

/// A single-line text field.
#[derive(Clone, Debug, PartialEq)]
pub struct TextField {
    mode: TextMode,
    text: String,
    anchor: usize,
    caret: usize,
    /// Mirror mode: the composition's range in `text`.
    composition: Option<Range<usize>>,
    /// Own mode: the preedit shown at the caret, and its cursor.
    preedit: String,
    preedit_cursor: Option<(usize, usize)>,
    /// Characters (scalar values) allowed; own mode enforces it.
    pub max_chars: Option<usize>,
}

impl TextField {
    /// A field holding `text`, the caret at its end.
    pub fn new(mode: TextMode, text: &str) -> TextField {
        let text = single_line(text);
        let end = text.len();
        TextField {
            mode,
            text,
            anchor: end,
            caret: end,
            composition: None,
            preedit: String::new(),
            preedit_cursor: None,
            max_chars: None,
        }
    }

    pub fn mode(&self) -> TextMode {
        self.mode
    }

    /// The text, without an own-mode preedit (the web's composition is
    /// in its text: the textarea holds it).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The selection, ordered; empty at the caret.
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }

    /// `[anchor, caret]`, bytes.
    pub fn anchor_caret(&self) -> [usize; 2] {
        [self.anchor, self.caret]
    }

    /// `[anchor, caret]` in UTF-16 code units, for the textarea.
    pub fn selection_utf16(&self) -> [usize; 2] {
        [
            byte_to_utf16(&self.text, self.anchor),
            byte_to_utf16(&self.text, self.caret),
        ]
    }

    pub fn is_composing(&self) -> bool {
        self.composition.is_some() || !self.preedit.is_empty()
    }

    /// Replaces the text (not typed: cleared, an autocomplete picked);
    /// the caret goes to its end and any composition ends.
    pub fn set_text(&mut self, text: &str) {
        self.text = single_line(text);
        self.caret = self.text.len();
        self.anchor = self.caret;
        self.composition = None;
        self.preedit.clear();
        self.preedit_cursor = None;
    }

    /// Sets the selection (bytes; snapped to boundaries).
    pub fn set_selection(&mut self, anchor: usize, caret: usize) {
        self.anchor = self.snap(anchor);
        self.caret = self.snap(caret);
    }

    fn snap(&self, i: usize) -> usize {
        let i = floor_char(&self.text, i);
        match self.mode {
            TextMode::Mirror => i,
            TextMode::Own => floor_grapheme(&self.text, i),
        }
    }

    /// Mirror mode: the textarea's state. Whether anything changed.
    pub fn sync(&mut self, s: &TextSync) -> bool {
        let before = (
            self.text.clone(),
            self.anchor,
            self.caret,
            self.composition.clone(),
        );
        self.text = s.text.clone();
        self.anchor = utf16_to_byte(&self.text, s.selection[0]);
        self.caret = utf16_to_byte(&self.text, s.selection[1]);
        self.composition = s.composition.and_then(|[a, b]| {
            let (a, b) = (
                utf16_to_byte(&self.text, a.min(b)),
                utf16_to_byte(&self.text, a.max(b)),
            );
            (a < b).then_some(a..b)
        });
        before
            != (
                self.text.clone(),
                self.anchor,
                self.caret,
                self.composition.clone(),
            )
    }

    /// What is shown.
    pub fn display(&self) -> Display {
        match self.mode {
            TextMode::Mirror => Display {
                text: self.text.clone(),
                caret: self.caret,
                selection: self.selection(),
                preedit: self.composition.clone(),
            },
            TextMode::Own if self.preedit.is_empty() => Display {
                text: self.text.clone(),
                caret: self.caret,
                selection: self.selection(),
                preedit: None,
            },
            TextMode::Own => {
                let at = self.caret;
                let mut text = String::with_capacity(self.text.len() + self.preedit.len());
                text.push_str(&self.text[..at]);
                text.push_str(&self.preedit);
                text.push_str(&self.text[at..]);
                let (a, b) = self
                    .preedit_cursor
                    .unwrap_or((self.preedit.len(), self.preedit.len()));
                Display {
                    text,
                    caret: at + b,
                    selection: at + a.min(b)..at + a.max(b),
                    preedit: Some(at..at + self.preedit.len()),
                }
            }
        }
    }

    /// Where the caret, the selection and the preedit are along the run
    /// drawn in `style`.
    pub fn layout(&self, measure: &mut dyn TextMeasure, style: &TextStyle) -> FieldLayout {
        let d = self.display();
        let mut x = |i: usize| measure.measure(&d.text[..i], style).advance;
        let caret = x(d.caret);
        let selection = (!d.selection.is_empty() && d.preedit.is_none())
            .then(|| [x(d.selection.start), x(d.selection.end)]);
        let preedit = d.preedit.as_ref().map(|r| [x(r.start), x(r.end)]);
        let width = x(d.text.len());
        FieldLayout {
            caret,
            selection,
            preedit,
            width,
        }
    }

    // Own mode: editing.

    /// Replaces the selection with `s` (newlines and tabs become spaces,
    /// other control characters are dropped; cut short at a grapheme to
    /// keep within [`TextField::max_chars`]). Whether the text changed.
    pub fn insert(&mut self, s: &str) -> bool {
        let mut s = single_line(s);
        let sel = self.selection();
        if let Some(max) = self.max_chars {
            let kept = self.text.chars().count() - self.text[sel.clone()].chars().count();
            let room = max.saturating_sub(kept);
            if s.chars().count() > room {
                let mut end = 0;
                let mut count = 0;
                for g in s.graphemes(true) {
                    let n = g.chars().count();
                    if count + n > room {
                        break;
                    }
                    count += n;
                    end += g.len();
                }
                s.truncate(end);
            }
        }
        if s.is_empty() && sel.is_empty() {
            return false;
        }
        self.text.replace_range(sel.clone(), &s);
        self.caret = sel.start + s.len();
        self.anchor = self.caret;
        true
    }

    /// Deletes the selection, else the grapheme (or word) before the
    /// caret.
    pub fn backspace(&mut self, word: bool) -> bool {
        if self.selection().is_empty() {
            let to = if word {
                word_left(&self.text, self.caret)
            } else {
                prev_grapheme(&self.text, self.caret)
            };
            self.anchor = to;
        }
        self.delete_selection()
    }

    /// Deletes the selection, else the grapheme (or word) after the
    /// caret.
    pub fn delete(&mut self, word: bool) -> bool {
        if self.selection().is_empty() {
            let to = if word {
                word_right(&self.text, self.caret)
            } else {
                next_grapheme(&self.text, self.caret)
            };
            self.anchor = to;
        }
        self.delete_selection()
    }

    fn delete_selection(&mut self) -> bool {
        let sel = self.selection();
        self.text.replace_range(sel.clone(), "");
        self.caret = sel.start;
        self.anchor = sel.start;
        !sel.is_empty()
    }

    /// Moves the caret; with `extend`, the selection's far end. Without,
    /// a selection collapses to its side first. Whether anything moved.
    pub fn move_caret(&mut self, motion: Motion, extend: bool) -> bool {
        let before = (self.anchor, self.caret);
        let sel = self.selection();
        let to = match motion {
            Motion::Left if !extend && !sel.is_empty() => sel.start,
            Motion::Right if !extend && !sel.is_empty() => sel.end,
            Motion::Left => prev_grapheme(&self.text, self.caret),
            Motion::Right => next_grapheme(&self.text, self.caret),
            Motion::WordLeft => word_left(&self.text, self.caret),
            Motion::WordRight => word_right(&self.text, self.caret),
            Motion::Home => 0,
            Motion::End => self.text.len(),
        };
        self.caret = to;
        if !extend {
            self.anchor = to;
        }
        before != (self.anchor, self.caret)
    }

    pub fn select_all(&mut self) -> bool {
        let before = (self.anchor, self.caret);
        self.anchor = 0;
        self.caret = self.text.len();
        before != (self.anchor, self.caret)
    }

    /// winit's `Ime::Preedit`: the composition shown at the caret (it
    /// replaces the selection once it starts), its cursor as byte offsets
    /// into it (snapped to its graphemes); empty text ends it.
    pub fn set_preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) -> bool {
        let text = single_line(text);
        if !text.is_empty() && self.preedit.is_empty() {
            self.delete_selection();
        }
        let cursor = cursor.map(|(a, b)| {
            let snap = |i: usize| floor_grapheme(&text, floor_char(&text, i));
            (snap(a), snap(b))
        });
        let changed = text != self.preedit || cursor != self.preedit_cursor;
        self.preedit_cursor = if text.is_empty() { None } else { cursor };
        self.preedit = text;
        changed
    }

    /// winit's `Ime::Commit`, or typed text: ends the preedit and inserts.
    /// Control characters are dropped (a shell passing Tab's, Enter's or
    /// Backspace's text by mistake inserts nothing).
    pub fn commit(&mut self, text: &str) -> bool {
        let had = !self.preedit.is_empty();
        self.preedit.clear();
        self.preedit_cursor = None;
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        (!text.is_empty() && self.insert(&text)) || had
    }

    /// Mirror mode: the text on one line, as [`TextField::insert`] keeps
    /// it (a paste into the textarea brings line breaks and tabs), the
    /// selection moved with it. Never while composing. Whether it changed.
    pub fn strip_controls(&mut self) -> bool {
        if self.is_composing() || !self.text.chars().any(char::is_control) {
            return false;
        }
        let at = |i: usize| single_line(&self.text[..i]).len();
        let (anchor, caret) = (at(self.anchor), at(self.caret));
        self.text = single_line(&self.text);
        self.anchor = floor_char(&self.text, anchor);
        self.caret = floor_char(&self.text, caret);
        true
    }

    /// Cuts the text to [`TextField::max_chars`] scalar values at a
    /// grapheme boundary (mirror mode: the textarea's `maxlength` counts
    /// UTF-16 units, so we count ourselves), the selection kept within.
    /// Never while composing. Whether it cut.
    pub fn enforce_max(&mut self) -> bool {
        let Some(max) = self.max_chars else {
            return false;
        };
        if self.is_composing() || self.text.chars().count() <= max {
            return false;
        }
        let mut end = 0;
        let mut count = 0;
        for g in self.text.graphemes(true) {
            let n = g.chars().count();
            if count + n > max {
                break;
            }
            count += n;
            end += g.len();
        }
        self.text.truncate(end);
        self.anchor = self.anchor.min(end);
        self.caret = self.caret.min(end);
        true
    }

    /// Own mode: a key. Keys while composing are the IME's.
    pub fn key(&mut self, k: &KeyEvent) -> KeyResult {
        if !k.down {
            return KeyResult::Ignored;
        }
        if k.composing || self.is_composing() || k.key == "Process" {
            return KeyResult::Consumed;
        }
        let m = k.modifiers;
        let word = m.ctrl || m.alt;
        let moved = |b: bool| {
            if b {
                KeyResult::Moved
            } else {
                KeyResult::Consumed
            }
        };
        let edited = |b: bool| {
            if b {
                KeyResult::Edited
            } else {
                KeyResult::Consumed
            }
        };
        match k.key.as_str() {
            "Enter" => return KeyResult::Submit,
            "Escape" => return KeyResult::Cancel,
            "Backspace" if m.meta => {
                self.anchor = 0;
                return edited(self.delete_selection());
            }
            "Backspace" => return edited(self.backspace(word)),
            "Delete" => return edited(self.delete(word)),
            "ArrowLeft" | "ArrowRight" | "Home" | "End" => {
                let left = matches!(k.key.as_str(), "ArrowLeft" | "Home");
                let motion = match (k.key.as_str(), m.meta, word) {
                    ("Home", ..) | ("ArrowLeft", true, _) => Motion::Home,
                    ("End", ..) | ("ArrowRight", true, _) => Motion::End,
                    (_, _, true) if left => Motion::WordLeft,
                    (_, _, true) => Motion::WordRight,
                    _ if left => Motion::Left,
                    _ => Motion::Right,
                };
                return moved(self.move_caret(motion, m.shift));
            }
            _ => {}
        }
        if m.command() && !m.alt {
            match k.shortcut_char() {
                Some('a') => return moved(self.select_all()),
                Some('c') | Some('x') => {
                    let sel = self.selection();
                    if sel.is_empty() {
                        return KeyResult::Consumed;
                    }
                    let text = self.text[sel].to_string();
                    let cut = k.shortcut_char() == Some('x');
                    if cut {
                        self.delete_selection();
                    }
                    return KeyResult::Copy { text, cut };
                }
                Some('v') => return KeyResult::Paste,
                _ => {}
            }
        }
        KeyResult::Ignored
    }
}

/// A caret motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Home,
    End,
}

/// One line: line breaks and tabs become spaces, other control characters
/// go.
fn single_line(s: &str) -> String {
    let s = s.replace("\r\n", " ");
    s.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

fn floor_char(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn floor_grapheme(s: &str, i: usize) -> usize {
    let mut c = GraphemeCursor::new(i, s.len(), true);
    match c.is_boundary(s, 0) {
        Ok(true) => i,
        _ => c.prev_boundary(s, 0).ok().flatten().unwrap_or(0),
    }
}

/// The grapheme boundary before `i` (0 at the start).
pub fn prev_grapheme(s: &str, i: usize) -> usize {
    let mut c = GraphemeCursor::new(i, s.len(), true);
    c.prev_boundary(s, 0).ok().flatten().unwrap_or(0)
}

/// The grapheme boundary after `i` (the end at the end).
pub fn next_grapheme(s: &str, i: usize) -> usize {
    let mut c = GraphemeCursor::new(i, s.len(), true);
    c.next_boundary(s, 0).ok().flatten().unwrap_or(s.len())
}

/// The start of the word before `i`, spaces skipped.
pub fn word_left(s: &str, i: usize) -> usize {
    s.split_word_bound_indices()
        .rev()
        .find(|(start, w)| *start < i && !w.trim().is_empty())
        .map_or(0, |(start, _)| start)
}

/// The end of the word after `i`, spaces skipped.
pub fn word_right(s: &str, i: usize) -> usize {
    s.split_word_bound_indices()
        .find(|(start, w)| start + w.len() > i && !w.trim().is_empty())
        .map_or(s.len(), |(start, w)| start + w.len())
}

/// The composition's range in the textarea's `value` (UTF-16 units), for
/// [`TextSync::composition`]: what differs from `before` (the value at
/// `compositionstart`), its common start and end left out; when nothing
/// differs (Android reconverting a word already typed), `data` (the last
/// `compositionupdate`'s) where it ends at `caret`.
pub fn composition_range(
    before: &str,
    value: &str,
    data: &str,
    caret: usize,
) -> Option<[usize; 2]> {
    let b: Vec<u16> = before.encode_utf16().collect();
    let v: Vec<u16> = value.encode_utf16().collect();
    let prefix = b.iter().zip(&v).take_while(|(x, y)| x == y).count();
    let suffix = b[prefix..]
        .iter()
        .rev()
        .zip(v[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let end = v.len() - suffix;
    if prefix < end {
        return Some([prefix, end]);
    }
    let d: Vec<u16> = data.encode_utf16().collect();
    let caret = caret.min(v.len());
    (!d.is_empty() && caret >= d.len() && v[caret - d.len()..caret] == d[..])
        .then(|| [caret - d.len(), caret])
}

/// A UTF-16 offset (the DOM's) as a byte offset into `s`; one inside a
/// surrogate pair goes before it, one past the end to the end.
pub fn utf16_to_byte(s: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        if units + c.len_utf16() > utf16 {
            return i;
        }
        units += c.len_utf16();
    }
    s.len()
}

/// A byte offset into `s` as UTF-16 code units.
pub fn byte_to_utf16(s: &str, byte: usize) -> usize {
    s[..floor_char(s, byte)].encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Modifiers;
    use ddi_actor::text::FixedMeasure;

    fn key(f: &mut TextField, k: &str, m: Modifiers) -> KeyResult {
        let mut e = KeyEvent::down(k, k);
        e.modifiers = m;
        f.key(&e)
    }

    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ctrl: false,
        alt: false,
        meta: false,
    };
    const CTRL: Modifiers = Modifiers {
        shift: false,
        ctrl: true,
        alt: false,
        meta: false,
    };
    const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    };

    #[test]
    fn utf16_offsets() {
        // 𝄞 is two UTF-16 units and four bytes; ジ three bytes, one unit.
        let s = "a𝄞ジ";
        assert_eq!(utf16_to_byte(s, 0), 0);
        assert_eq!(utf16_to_byte(s, 1), 1);
        assert_eq!(utf16_to_byte(s, 2), 1, "inside the pair: before it");
        assert_eq!(utf16_to_byte(s, 3), 5);
        assert_eq!(utf16_to_byte(s, 4), 8);
        assert_eq!(utf16_to_byte(s, 99), 8);
        assert_eq!(byte_to_utf16(s, 5), 3);
        assert_eq!(byte_to_utf16(s, 8), 4);
    }

    #[test]
    fn edits_by_grapheme() {
        // か + combining dakuten, a family emoji (one cluster of 7 scalars),
        // and a flag.
        let mut f = TextField::new(TextMode::Own, "か\u{3099}👨‍👩‍👧x🇯🇵");
        assert!(f.backspace(false));
        assert_eq!(f.text(), "か\u{3099}👨‍👩‍👧x");
        assert_eq!(key(&mut f, "ArrowLeft", NONE), KeyResult::Moved);
        assert_eq!(key(&mut f, "ArrowLeft", NONE), KeyResult::Moved);
        // Before the family: a Delete removes it whole.
        assert!(f.delete(false));
        assert_eq!(f.text(), "か\u{3099}x");
        assert_eq!(key(&mut f, "ArrowLeft", NONE), KeyResult::Moved);
        assert_eq!(f.anchor_caret(), [0, 0]);
        assert_eq!(key(&mut f, "ArrowLeft", NONE), KeyResult::Consumed);
        // Selections: shift extends, an arrow collapses.
        assert_eq!(key(&mut f, "End", SHIFT), KeyResult::Moved);
        assert_eq!(f.selection(), 0..f.text().len());
        assert_eq!(key(&mut f, "ArrowLeft", NONE), KeyResult::Moved);
        assert_eq!(f.anchor_caret(), [0, 0]);
        // Typing replaces the selection.
        f.select_all();
        assert!(f.commit("ジャン"));
        assert_eq!(f.text(), "ジャン");
        // A selection set mid-cluster snaps to its start.
        let mut g = TextField::new(TextMode::Own, "か\u{3099}");
        g.set_selection(3, 4);
        assert_eq!(g.anchor_caret(), [0, 0]);
    }

    #[test]
    fn words_and_shortcuts() {
        let mut f = TextField::new(TextMode::Own, "dito  dito infinito");
        assert_eq!(key(&mut f, "ArrowLeft", CTRL), KeyResult::Moved);
        assert_eq!(f.anchor_caret(), [11, 11]);
        assert_eq!(key(&mut f, "ArrowLeft", CTRL), KeyResult::Moved);
        assert_eq!(f.anchor_caret(), [6, 6]);
        assert_eq!(key(&mut f, "Backspace", CTRL), KeyResult::Edited);
        assert_eq!(f.text(), "dito infinito");
        assert_eq!(key(&mut f, "ArrowRight", CTRL), KeyResult::Moved);
        assert_eq!(f.anchor_caret(), [4, 4]);
        let mut e = KeyEvent::down("KeyA", "a");
        e.modifiers.meta = true;
        assert_eq!(f.key(&e), KeyResult::Moved);
        let mut e = KeyEvent::down("KeyX", "x");
        e.modifiers.ctrl = true;
        assert_eq!(
            f.key(&e),
            KeyResult::Copy {
                text: "dito infinito".into(),
                cut: true
            }
        );
        assert_eq!(f.text(), "");
        let mut e = KeyEvent::down("KeyV", "м");
        e.modifiers.ctrl = true;
        assert_eq!(f.key(&e), KeyResult::Paste);
        assert_eq!(key(&mut f, "ArrowDown", NONE), KeyResult::Ignored);
        assert_eq!(key(&mut f, "Enter", NONE), KeyResult::Submit);
    }

    #[test]
    fn preedit_is_shown_and_marked() {
        let mut f = TextField::new(TextMode::Own, "ab");
        f.set_selection(1, 2);
        // The preedit replaces the selection; the cursor, a byte offset
        // inside a character, snaps to it.
        assert!(f.set_preedit("にほ", Some((4, 4))));
        assert!(f.is_composing());
        let d = f.display();
        assert_eq!(d.text, "aにほ");
        assert_eq!(d.preedit, Some(1..7));
        assert_eq!(d.caret, 4);
        // Keys are the IME's meanwhile.
        assert_eq!(key(&mut f, "Backspace", NONE), KeyResult::Consumed);
        let l = f.layout(&mut FixedMeasure, &TextStyle::new("sans-serif", 20.0));
        assert_eq!(l.preedit, Some([10.0, 30.0]));
        assert_eq!(l.caret, 20.0);
        assert_eq!(l.selection, None);
        assert!(f.commit("日本"));
        assert_eq!(f.text(), "a日本");
        assert!(!f.is_composing());
        assert_eq!(f.display().preedit, None);
    }

    #[test]
    fn mirror_follows_the_textarea() {
        let mut f = TextField::new(TextMode::Mirror, "");
        // Composing "にほ" after "a", the caret at its end (UTF-16).
        assert!(f.sync(&TextSync {
            text: "aにほ".into(),
            selection: [3, 3],
            composition: Some([1, 3]),
        }));
        let d = f.display();
        assert_eq!(d.preedit, Some(1..7));
        assert_eq!(d.caret, 7);
        assert!(f.is_composing());
        // The same state again changes nothing.
        assert!(!f.sync(&TextSync {
            text: "aにほ".into(),
            selection: [3, 3],
            composition: Some([1, 3]),
        }));
        // Committed, then selected backwards.
        assert!(f.sync(&TextSync {
            text: "a日本".into(),
            selection: [3, 1],
            composition: None,
        }));
        assert_eq!(f.anchor_caret(), [7, 1]);
        assert_eq!(f.selection_utf16(), [3, 1]);
        assert!(!f.is_composing());
    }

    #[test]
    fn compositions_are_found_by_difference() {
        // "a|b" composing に at the caret.
        assert_eq!(composition_range("ab", "aにb", "に", 2), Some([1, 2]));
        // The selection "bc" replaced by にほ.
        assert_eq!(composition_range("abcd", "aにほd", "にほ", 3), Some([1, 3]));
        // Typing a letter that repeats the one after: the range is the new
        // one, wherever the difference is placed.
        assert_eq!(
            composition_range("aa", "aaa", "a", 1).map(|r| r[1] - r[0]),
            Some(1)
        );
        // Reconversion: the value unchanged, the word before the caret.
        assert_eq!(
            composition_range("日本語", "日本語", "本語", 3),
            Some([1, 3])
        );
        assert_eq!(composition_range("日本語", "日本語", "", 3), None);
        // Astral characters count two units.
        assert_eq!(composition_range("x", "x𝄞", "𝄞", 3), Some([1, 3]));
    }

    #[test]
    fn commits_drop_control_characters() {
        let mut f = TextField::new(TextMode::Own, "ab");
        assert!(!f.commit("\t"));
        assert!(!f.commit("\r\n"));
        assert!(!f.commit("\u{8}"));
        assert_eq!(f.text(), "ab");
        assert!(f.commit("c\u{7f}"));
        assert_eq!(f.text(), "abc");
        // Ending a preedit with nothing still changes what is shown.
        f.set_preedit("に", None);
        assert!(f.commit(""));
    }

    #[test]
    fn mirrored_pastes_become_one_line() {
        let mut f = TextField::new(TextMode::Mirror, "");
        f.sync(&TextSync {
            text: "a\r\nb\tc".into(),
            selection: [6, 6],
            composition: None,
        });
        assert!(f.strip_controls());
        assert_eq!(f.text(), "a b c");
        assert_eq!(f.anchor_caret(), [5, 5]);
        assert!(!f.strip_controls());
    }

    #[test]
    fn mirrored_text_is_cut_to_the_limit() {
        let mut f = TextField::new(TextMode::Mirror, "");
        f.max_chars = Some(3);
        // 𝄞 is two UTF-16 units but one character.
        f.sync(&TextSync {
            text: "𝄞ab".into(),
            selection: [4, 4],
            composition: None,
        });
        assert!(!f.enforce_max());
        f.sync(&TextSync {
            text: "𝄞abc".into(),
            selection: [5, 5],
            composition: None,
        });
        assert!(f.enforce_max());
        assert_eq!(f.text(), "𝄞ab");
        assert_eq!(f.selection_utf16(), [4, 4]);
    }

    #[test]
    fn inserts_stay_on_one_line_and_within_the_limit() {
        let mut f = TextField::new(TextMode::Own, "");
        f.max_chars = Some(5);
        assert!(f.insert("a\nb\tc\u{7}"));
        assert_eq!(f.text(), "a b c");
        assert!(!f.insert("more"));
        f.set_text("abc");
        // A cluster that does not fit (👍🏽 is two scalars) is left out whole.
        assert!(f.insert("x👍🏽y"));
        assert_eq!(f.text(), "abcx");
    }
}
