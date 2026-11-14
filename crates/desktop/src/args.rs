//! The command line's flags as [`crate::run`] reads them: a flag, the
//! word after it, and the switches kept in the settings.

use std::str::FromStr;

use ddi_ui::state::settings::Settings;

/// The flags that open a play, a test, the editor or a share: song select
/// under them, and no first start's offer over them.
pub const OPENS_SELECT: [&str; 6] = [
    "--play",
    "--replay",
    "--calibrate",
    "--edit",
    "--shared",
    "--cpu",
];

/// The command line's words (the program's name left out).
#[derive(Clone, Copy)]
pub struct Args<'a>(pub &'a [String]);

impl<'a> Args<'a> {
    /// Whether `flag` is given.
    pub fn has(&self, flag: &str) -> bool {
        self.0.iter().any(|a| a == flag)
    }

    /// Whether any of `flags` is given.
    pub fn any(&self, flags: &[&str]) -> bool {
        self.0.iter().any(|a| flags.contains(&a.as_str()))
    }

    /// The word after the first `flag`, whatever it is, where both are
    /// given.
    pub fn value(&self, flag: &str) -> Option<&'a str> {
        let i = self.0.iter().position(|a| a == flag)?;
        self.0.get(i + 1).map(String::as_str)
    }

    /// The word after each `flag`.
    pub fn values(&self, flag: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        let words = self.0;
        words
            .iter()
            .enumerate()
            .filter(move |(_, a)| *a == flag)
            .filter_map(move |(i, _)| words.get(i + 1).map(String::as_str))
    }

    /// [`Args::value`] read as a `T`: none where it is not given or does
    /// not read as one.
    pub fn parsed<T: FromStr>(&self, flag: &str) -> Option<T> {
        self.value(flag)?.parse().ok()
    }

    /// `flag on|1` or `flag off|0`: none where the flag or its word is
    /// not given; anything else is refused (the process ends, status 2).
    pub fn on_off(&self, flag: &str) -> Option<bool> {
        match self.value(flag)? {
            "on" | "1" => Some(true),
            "off" | "0" => Some(false),
            _ => {
                eprintln!("{flag} takes on or off");
                std::process::exit(2);
            }
        }
    }
}

/// A switch kept in the settings (`field`, `--sharing`'s and the
/// multiplayer's hidden options) set to `on`, and written at once: a flag
/// refused later ends the process before the shell writes anything.
pub fn save_switch(on: bool, field: fn(&mut Settings) -> &mut bool) {
    let mut settings = Settings::load();
    if *field(&mut settings) != on {
        *field(&mut settings) = on;
        settings.save();
        let _ = ddi_platform::storage::flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_platform::storage::{Area, MemoryStore, Store};
    use std::cell::Cell;
    use std::rc::Rc;

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn flags_and_their_words_are_read_as_before() {
        let w = words("--play x --chart 2 --songs a --songs b --bias nope --sharing maybe --join");
        let a = Args(&w);
        assert!(a.has("--play") && !a.has("--edit"));
        assert!(a.any(&OPENS_SELECT));
        assert_eq!(a.value("--play"), Some("x"));
        assert_eq!(a.parsed::<usize>("--chart"), Some(2));
        assert_eq!(a.parsed::<f64>("--bias"), None);
        assert_eq!(a.value("--join"), None);
        assert_eq!(a.value("--edit"), None);
        assert_eq!(a.values("--songs").collect::<Vec<_>>(), ["a", "b"]);
        // A flag's word is whatever follows it, another flag too.
        let w = words("--screen --dev");
        assert_eq!(Args(&w).value("--screen"), Some("--dev"));
        let w = words("--sharing on --mp-rules-first 0 --mp-watch-link");
        let a = Args(&w);
        assert_eq!(a.on_off("--sharing"), Some(true));
        assert_eq!(a.on_off("--mp-rules-first"), Some(false));
        assert_eq!(a.on_off("--mp-watch-link"), None);
    }

    /// Memory, counting its flushes.
    #[derive(Default)]
    struct Counted {
        values: MemoryStore,
        flushes: Cell<u32>,
    }

    impl Store for Counted {
        fn get(&self, area: Area, key: &str) -> Option<String> {
            self.values.get(area, key)
        }
        fn set(&self, area: Area, key: &str, value: &str) -> Result<(), String> {
            self.values.set(area, key, value)
        }
        fn remove(&self, area: Area, key: &str) {
            self.values.remove(area, key)
        }
        fn flush(&self) -> Result<(), String> {
            self.flushes.set(self.flushes.get() + 1);
            Ok(())
        }
    }

    /// Each switch is written at once, as `--sharing` always was: a flag
    /// refused after it ends the process before the shell flushes.
    #[test]
    fn the_switches_are_written_at_once() {
        let store = Rc::new(Counted::default());
        ddi_platform::storage::install(store.clone());
        let switches: [fn(&mut Settings) -> &mut bool; 3] = [
            |s| &mut s.sharing,
            |s| &mut s.multiplayer_rules_first,
            |s| &mut s.multiplayer_watch_link,
        ];
        for (n, field) in switches.into_iter().enumerate() {
            save_switch(true, field);
            assert!(*field(&mut Settings::load()), "switch {n} on");
            assert_eq!(store.flushes.get(), n as u32 + 1, "switch {n} written");
        }
        // Unchanged: nothing to write.
        save_switch(true, |s| &mut s.sharing);
        assert_eq!(store.flushes.get(), 3);
    }
}
