//! The experimental settings (`docs/plans/ui-levels.md`, section 5 and
//! stage 4): switches for developing and testing the game and those kept
//! for a few people, listed by their own screen
//! (`crate::screens::experimental`, `#/experimental`, `--screen
//! experimental`) rather than in Options. Each entry is written in
//! `experimental.toml` (made a table at build time, [`ENTRIES`]) and bound
//! here to the setting it is ([`switch`]): the settings keep their keys,
//! values and effects, and the flags and addresses that set them keep
//! working (an address's value for the page alone, never stored:
//! [`given`]).
//!
//! Nothing here acts where a player cannot find it: an entry changed from
//! its default counts among Interface's changes at Basic
//! ([`changed_count`]), and Options lists the screen under Interface's
//! Troubleshooting once it was opened or anything on it is changed
//! ([`in_options`]).

use crate::screens::data::Platform;
use crate::state::settings::{self, Settings};

/// An entry of `experimental.toml` (its comment says what each field is).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: &'static str,
    pub group: &'static str,
    pub name: &'static str,
    pub line: &'static str,
    pub default: bool,
    pub web: bool,
    pub desktop: bool,
    pub desktop_flag: &'static str,
    pub web_address: &'static str,
    pub play: bool,
}

include!(concat!(env!("OUT_DIR"), "/experimental.rs"));

impl Entry {
    /// It exists on `platform`.
    pub fn on(&self, platform: Platform) -> bool {
        match platform {
            Platform::Web => self.web,
            Platform::Desktop => self.desktop,
        }
    }
}

/// The entries on `platform`, in the file's order.
pub fn entries(platform: Platform) -> impl Iterator<Item = &'static Entry> {
    ENTRIES.iter().filter(move |e| e.on(platform))
}

/// Which hidden setting an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    RulesFirst,
    WatchLink,
    Sharing,
}

/// The setting entry `key` is bound to (`None`: no such entry in code,
/// which the tests refuse).
pub fn switch(key: &str) -> Option<Switch> {
    Some(match key {
        "rooms.rules_first" => Switch::RulesFirst,
        "rooms.watch_link" => Switch::WatchLink,
        "share.uploader" => Switch::Sharing,
        _ => return None,
    })
}

impl Switch {
    fn field(self, s: &mut Settings) -> &mut bool {
        match self {
            Switch::RulesFirst => &mut s.multiplayer_rules_first,
            Switch::WatchLink => &mut s.multiplayer_watch_link,
            Switch::Sharing => &mut s.sharing,
        }
    }

    /// As stored.
    pub fn stored(self, s: &Settings) -> bool {
        match self {
            Switch::RulesFirst => s.multiplayer_rules_first,
            Switch::WatchLink => s.multiplayer_watch_link,
            Switch::Sharing => s.sharing,
        }
    }

    /// What an address gave for the page, where it did
    /// ([`settings::given_hidden`]).
    pub fn given(self) -> Option<bool> {
        let (rules_first, watch_link, sharing) = settings::given_hidden();
        match self {
            Switch::RulesFirst => rules_first,
            Switch::WatchLink => watch_link,
            Switch::Sharing => sharing,
        }
    }

    /// In effect: the address's for the page, else as stored.
    pub fn value(self, s: &Settings) -> bool {
        self.given().unwrap_or_else(|| self.stored(s))
    }

    /// Set by the player on the screen: stored, and the page follows it
    /// from now (an address's value for it let go).
    pub fn set(self, s: &mut Settings, on: bool) {
        *self.field(s) = on;
        settings::forget_given(
            self == Switch::RulesFirst,
            self == Switch::WatchLink,
            self == Switch::Sharing,
        );
    }
}

/// Entry `e`'s value in effect.
pub fn value(s: &Settings, e: &Entry) -> bool {
    switch(e.key).is_some_and(|w| w.value(s))
}

/// Entry `e` differs from its default (in effect: an address's value
/// counts, it acts on the page).
pub fn changed(s: &Settings, e: &Entry) -> bool {
    value(s, e) != e.default
}

/// How many of `platform`'s entries differ from their defaults.
pub fn changed_count(s: &Settings, platform: Platform) -> usize {
    entries(platform).filter(|e| changed(s, e)).count()
}

/// Entry `key` set to `on` (stored; the caller saves).
pub fn set(s: &mut Settings, key: &str, on: bool) {
    if let Some(w) = switch(key) {
        w.set(s, on);
    }
}

/// Entry `key` back to its default.
pub fn reset(s: &mut Settings, key: &str) {
    if let Some(e) = ENTRIES.iter().find(|e| e.key == key) {
        set(s, key, e.default);
    }
}

/// Every entry back to its default (on every platform: they are
/// settings).
pub fn reset_all(s: &mut Settings) {
    for e in ENTRIES {
        set(s, e.key, e.default);
    }
}

/// Options lists the screen (under Interface's Troubleshooting, at
/// Advanced): once it was opened on this device, or while anything on it
/// differs from its default.
pub fn in_options(s: &Settings, platform: Platform) -> bool {
    s.experimental_seen || changed_count(s, platform) > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_given() {
        settings::forget_given(true, true, true);
    }

    /// Every entry of the file is bound to a setting, the keys are unique,
    /// each is somewhere and its default is the setting's own default.
    #[test]
    fn every_entry_is_a_setting_with_its_default() {
        clear_given();
        assert!(!ENTRIES.is_empty());
        let defaults = Settings::default();
        for e in ENTRIES {
            let w = switch(e.key).unwrap_or_else(|| panic!("{} bound in code", e.key));
            assert_eq!(w.stored(&defaults), e.default, "{}", e.key);
            assert!(e.web || e.desktop, "{} is somewhere", e.key);
            assert!(!e.name.is_empty() && !e.line.is_empty(), "{}", e.key);
            assert!(!e.play, "{}: none changes a play today", e.key);
            // Its own field, and no other.
            let mut s = Settings::default();
            set(&mut s, e.key, !e.default);
            assert_eq!(changed_count(&s, Platform::Web), 1, "{}", e.key);
            assert_eq!(changed_count(&s, Platform::Desktop), 1, "{}", e.key);
            reset(&mut s, e.key);
            assert_eq!(s, Settings::default(), "{}", e.key);
        }
        let keys: std::collections::HashSet<_> = ENTRIES.iter().map(|e| e.key).collect();
        assert_eq!(keys.len(), ENTRIES.len());
    }

    /// The three hidden switches the settings had before the screen are
    /// on it, by the fields the flags set.
    #[test]
    fn the_hidden_switches_are_listed() {
        clear_given();
        let mut s = Settings::default();
        set(&mut s, "share.uploader", true);
        set(&mut s, "rooms.rules_first", true);
        set(&mut s, "rooms.watch_link", true);
        assert!(s.sharing && s.multiplayer_rules_first && s.multiplayer_watch_link);
        assert!(s.sharing_on() && s.rules_first() && s.watch_link());
        reset_all(&mut s);
        assert_eq!(s, Settings::default());
    }

    /// An address's value is the page's: it shows and counts as in
    /// effect, is never stored, and gives way once the player sets the
    /// switch on the screen.
    #[test]
    fn an_address_value_counts_and_gives_way() {
        clear_given();
        let mut s = Settings::default();
        settings::give_hidden(Some(true), None);
        settings::give_sharing(Some(true));
        assert_eq!(Switch::RulesFirst.given(), Some(true));
        assert!(Switch::RulesFirst.value(&s) && !Switch::RulesFirst.stored(&s));
        assert_eq!(changed_count(&s, Platform::Web), 2);
        assert!(in_options(&s, Platform::Web));
        // Set off by the player: stored off, the address's let go.
        set(&mut s, "rooms.rules_first", false);
        assert_eq!(Switch::RulesFirst.given(), None);
        assert!(!s.rules_first());
        assert_eq!(Switch::Sharing.given(), Some(true), "the others' kept");
        reset_all(&mut s);
        assert_eq!(settings::given_hidden(), (None, None, None));
        assert_eq!(changed_count(&s, Platform::Web), 0);
        assert!(!in_options(&s, Platform::Web));
    }

    /// Options lists the screen once it was opened, or while anything
    /// differs.
    #[test]
    fn options_lists_it_once_seen_or_changed() {
        clear_given();
        let mut s = Settings::default();
        assert!(!in_options(&s, Platform::Desktop));
        s.multiplayer_watch_link = true;
        assert!(in_options(&s, Platform::Desktop));
        s.multiplayer_watch_link = false;
        s.experimental_seen = true;
        assert!(in_options(&s, Platform::Desktop));
    }

    /// The seen mark is left out of the stored settings while off, so
    /// settings stored before it read and write the same.
    #[test]
    fn the_seen_mark_is_stored_only_when_set() {
        let off = serde_json::to_string(&Settings::default()).unwrap();
        assert!(!off.contains("experimental_seen"));
        let on = Settings {
            experimental_seen: true,
            ..Settings::default()
        };
        let text = serde_json::to_string(&on).unwrap();
        assert!(text.contains("\"experimental_seen\":true"));
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert!(back.experimental_seen);
    }
}
