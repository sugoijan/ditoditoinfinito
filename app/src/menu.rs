//! The song list's place, kept across a play, and moving through the list
//! with the arrows, a controller or a dance pad.
//!
//! The place (open pack, last chart started, scroll) lives in
//! `sessionStorage`: it is per tab, survives the play page and a reload,
//! and is forgotten with the tab. Storage can be unavailable (private
//! windows, blocked site data); the list then simply starts fresh.
//!
//! Navigation works on the page as drawn: rows are elements marked
//! `data-nav-row` (a pack header, a song), their items `data-nav-item`
//! (the header itself, each chart button). Up and down move between rows
//! keeping the column, left and right between a row's items, confirm
//! clicks the item, back goes from a song to its pack's header and from an
//! open pack's header closes it.

use gloo::storage::{SessionStorage, Storage};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, ScrollIntoViewOptions, ScrollLogicalPosition};

const KEY: &str = "ddi.menu.v1";

/// Class of the item the last navigation moved to (drawn like keyboard
/// focus, which programmatic focus does not always show).
pub(crate) const CURRENT: &str = "nav-current";

/// Where the song list was.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MenuPlace {
    /// Whether `open` was ever set (false before the first visit).
    pub(crate) known: bool,
    /// Key of the open pack ([`pack_key`]), `None` when all are closed.
    pub(crate) open: Option<String>,
    /// Song and chart index of the last chart started.
    pub(crate) chart: Option<(String, usize)>,
    /// `window.scrollY` when it was left.
    pub(crate) scroll: f64,
}

impl MenuPlace {
    pub(crate) fn load() -> MenuPlace {
        SessionStorage::get(KEY).unwrap_or_default()
    }

    pub(crate) fn save(&self) {
        let _ = SessionStorage::set(KEY, self);
    }
}

/// Key of a group of the list: the bundled songs or an imported pack
/// (packs are told apart ignoring case).
pub(crate) fn pack_key(pack: Option<&str>) -> String {
    match pack {
        None => "bundled".into(),
        Some(p) => format!("pack:{}", p.to_lowercase()),
    }
}

pub(crate) use ddi_engine::MenuInput as Nav;

/// Directions repeat while held.
pub(crate) fn repeats(nav: Nav) -> bool {
    matches!(nav, Nav::Up | Nav::Down | Nav::Left | Nav::Right)
}

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

fn all(root: &web_sys::Document, selector: &str) -> Vec<Element> {
    let Ok(list) = root.query_selector_all(selector) else {
        return Vec::new();
    };
    (0..list.length())
        .filter_map(|i| list.item(i)?.dyn_into::<Element>().ok())
        .collect()
}

fn items(row: &Element) -> Vec<Element> {
    if row.has_attribute("data-nav-item") {
        return vec![row.clone()];
    }
    let Ok(list) = row.query_selector_all("[data-nav-item]") else {
        return Vec::new();
    };
    (0..list.length())
        .filter_map(|i| list.item(i)?.dyn_into::<Element>().ok())
        .collect()
}

/// Focuses `el`, marks it current and brings it into view.
pub(crate) fn focus(el: &Element, block: ScrollLogicalPosition) {
    if let Some(doc) = document() {
        for old in all(&doc, &format!(".{CURRENT}")) {
            let _ = old.class_list().remove_1(CURRENT);
        }
    }
    let _ = el.class_list().add_1(CURRENT);
    if let Some(h) = el.dyn_ref::<HtmlElement>() {
        let opts = web_sys::FocusOptions::new();
        opts.set_prevent_scroll(true);
        let _ = h.focus_with_options(&opts);
    }
    let opts = ScrollIntoViewOptions::new();
    opts.set_block(block);
    el.scroll_into_view_with_scroll_into_view_options(&opts);
}

/// What a move did.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Moved {
    /// Nothing to do (the page may scroll as usual).
    No,
    /// Focus moved, or an item was activated.
    Yes,
    /// Back on an open pack's header: the caller closes that pack.
    Close(String),
}

/// Clears the [`CURRENT`] mark (the mouse took over).
pub(crate) fn clear_current() {
    if let Some(doc) = document() {
        for old in all(&doc, &format!(".{CURRENT}")) {
            let _ = old.class_list().remove_1(CURRENT);
        }
    }
}

/// Moves through the list.
pub(crate) fn navigate(nav: Nav) -> Moved {
    let Some(doc) = document() else {
        return Moved::No;
    };
    let rows = all(&doc, "[data-nav-row]");
    if rows.is_empty() {
        return Moved::No;
    }
    let active = doc.active_element();
    let here = active.as_ref().and_then(|a| {
        let r = rows.iter().position(|row| row.contains(Some(a)))?;
        let c = items(&rows[r]).iter().position(|i| i == a).unwrap_or(0);
        Some((r, c))
    });
    let Some((r, c)) = here else {
        // Nothing chosen yet: up or down (or a controller's confirm)
        // enters the list at the open pack, else at the top.
        if matches!(nav, Nav::Left | Nav::Right | Nav::Back) {
            return Moved::No;
        }
        let start = rows
            .iter()
            .find(|row| row.get_attribute("aria-expanded").as_deref() == Some("true"))
            .unwrap_or(&rows[0]);
        return match items(start).first() {
            Some(first) => {
                focus(first, ScrollLogicalPosition::Nearest);
                Moved::Yes
            }
            None => Moved::No,
        };
    };
    let goto = |r: usize, c: usize| {
        let row_items = items(&rows[r]);
        match row_items.get(c.min(row_items.len().saturating_sub(1))) {
            Some(el) if Some(el) != active.as_ref() => {
                focus(el, ScrollLogicalPosition::Nearest);
                Moved::Yes
            }
            _ => Moved::No,
        }
    };
    let on_item = active
        .as_ref()
        .is_some_and(|a| a.has_attribute("data-nav-item"));
    match nav {
        Nav::Up if r > 0 => goto(r - 1, c),
        Nav::Down if r + 1 < rows.len() => goto(r + 1, c),
        Nav::Left if c > 0 => goto(r, c - 1),
        Nav::Right => goto(r, c + 1),
        // Only list items: a focused preview button would play silently,
        // without the gesture its audio needs.
        Nav::Confirm if on_item => match active.as_ref().and_then(|a| a.dyn_ref::<HtmlElement>()) {
            Some(h) => {
                h.click();
                Moved::Yes
            }
            None => Moved::No,
        },
        Nav::Back => {
            let row = &rows[r];
            if let Some(key) = row.get_attribute("data-pack-header") {
                if row.get_attribute("aria-expanded").as_deref() == Some("true") {
                    return Moved::Close(key);
                }
                Moved::No
            } else if let Some(header) = rows[..r]
                .iter()
                .rev()
                .find(|h| h.has_attribute("data-pack-header"))
            {
                focus(header, ScrollLogicalPosition::Nearest);
                Moved::Yes
            } else {
                Moved::No
            }
        }
        _ => Moved::No,
    }
}

/// The chart button of `song`'s chart `chart`, if drawn.
pub(crate) fn chart_button(song: &str, chart: usize) -> Option<Element> {
    let doc = document()?;
    let chart = chart.to_string();
    all(&doc, "[data-song]").into_iter().find(|el| {
        el.get_attribute("data-song").as_deref() == Some(song)
            && el.get_attribute("data-chart").as_deref() == Some(chart.as_str())
    })
}

/// The header of the pack with key `key`, if drawn.
pub(crate) fn pack_header(key: &str) -> Option<Element> {
    let doc = document()?;
    all(&doc, "[data-pack-header]")
        .into_iter()
        .find(|el| el.get_attribute("data-pack-header").as_deref() == Some(key))
}
