//! Options' sections, their order and the headings in them, in one table
//! ([`TREE`]; `docs/plans/ui-levels.md`, section 3.3, built in stage 3).
//!
//! The model makes its rows in parts by topic (`OptionsModel`'s `scroll`,
//! `notes`, `display` and the rest, each row with its id, value and
//! level); [`arrange`] takes every row out of them and puts it where the
//! tree says, in the tree's order, with the tree's headings. A row keeps
//! everything else: its id (which the settings, the shells and the saved
//! position know it by), its words, its values, its level and what a
//! change of it does. Change where a row shows here and nowhere else.
//!
//! The same sections at both levels: a path in instructions ("Options →
//! Interface → Show advanced options") stays true at either. A section
//! with no Basic row shows at Basic only where something in it is
//! changed (`OptionsModel::data`). Sections a shell adds
//! (`World::sections`: desktop's Plugins page) come after the tree's.
//!
//! Each choice rests on one of the plan's recommendations for its open
//! questions (section 8, question 9: "section 3.3"); the plan's section 11
//! lists them.

use ddi_l10n::UiText;
use ddi_l10n::msg::options as m;

use super::data::{OptionDef, OptionSection};
use super::options_levels::{EXPERIMENTAL, SHOW_ADVANCED};

/// What a section holds, in its order.
#[derive(Clone, Copy, Debug)]
pub enum Entry {
    /// A heading over the next row placed (dropped where none follows
    /// before the next heading or the section's end).
    Heading(fn() -> UiText),
    /// The row with this id, where the model makes it.
    Row(&'static str),
    /// The rows whose ids start with any of these prefixes, in the order
    /// the model makes them (a device's offset and its Forget, a
    /// controller's rows): a family of rows that may be any number.
    Family(&'static [&'static str]),
}

/// A section: its id (its English name, which the saved position, the
/// count's row and the tests know it by), its name as it shows, the ids
/// of the sections whose rows it took (`was`: a position saved before
/// lands here when its row is not shown), and its rows.
#[derive(Clone, Copy, Debug)]
pub struct Branch {
    pub id: &'static str,
    pub name: fn() -> UiText,
    pub was: &'static [&'static str],
    pub entries: &'static [Entry],
}

use Entry::{Family, Heading, Row};

/// Options' sections, in their order (`docs/plans/ui-levels.md`, 3.3).
pub const TREE: &[Branch] = &[
    // Per play: the start prompt's rows come from here.
    Branch {
        id: "Play",
        name: m::section_play,
        was: &["Scroll", "Notes"],
        entries: &[
            Row("speed"),
            Row("reverse"),
            Row("speed_source"),
            Row("scroll_action"),
            Row("turn"),
            Row("appearance"),
            Row("lane_cover"),
            Row("cover_sudden"),
            Row("cover_hidden"),
            Heading(m::heading_easier),
            Row("cut"),
            Row("at_once"),
            Row("no_holds"),
            Row("no_stretch"),
        ],
    },
    // How the notes look; the preview's home.
    Branch {
        id: "Arrows",
        name: m::section_arrows,
        was: &[],
        entries: &[
            Row("noteskin"),
            Family(&["skin."]),
            Row("noteskin.remove"),
            Row("pump_noteskin"),
            Row("pump_noteskin.remove"),
            Family(&["pump_skin."]),
        ],
    },
    // What a play draws around the notes.
    Branch {
        id: "Screen",
        name: m::section_screen,
        was: &["Display"],
        entries: &[
            Row("field_place"),
            Row("ddi.words"),
            Row("ddi.score"),
            Row("ghost"),
            Row("lyrics"),
            Row("show_deltas"),
            Row("receptor_snap"),
            Heading(m::heading_dancing_hand),
            Row("dancing_hand"),
            Row("hand_layout"),
            Row("hand_frame_width"),
            Row("hand_frame_height"),
        ],
    },
    Branch {
        id: "Backgrounds",
        name: m::section_backgrounds,
        was: &[],
        entries: &[
            Row("bg_brightness"),
            Row("field_filter"),
            Row("video"),
            Row("random_movies"),
            Row("generated_background"),
            Row("generated_over_pictures"),
            Row("generated_palette"),
            Row("background_frame"),
        ],
    },
    Branch {
        id: "Rules",
        name: m::section_rules,
        was: &[],
        entries: &[
            Row("ddi.difficulty"),
            Row("ddi.practice"),
            Row("ddi.boos"),
            Row("ruleset_mode"),
            Row("ruleset"),
            Row("holds"),
            Row("fail"),
            Row("life"),
            Row("judge"),
            Row("life_type"),
            Row("lives"),
            Row("ddr_gauge"),
            Row("rejudge"),
            Heading(m::heading_danoni),
            Row("danoni.gauge"),
            Row("danoni.range"),
            Row("danoni.excessive"),
        ],
    },
    Branch {
        id: "Timing",
        name: m::section_timing,
        was: &[],
        entries: &[
            Row("calibrate"),
            Row("audio_offset"),
            Row("visual_offset"),
            Heading(m::heading_known_devices),
            Family(&["profile.", "forget."]),
        ],
    },
    Branch {
        id: "Sound",
        name: m::section_sound,
        was: &[],
        entries: &[
            Row("volume"),
            Row("sample"),
            Row("assist_clap"),
            Row("mine_sound"),
            Row("announcer"),
            Row("announcer.language"),
            Family(&["announcer.tag."]),
            Family(&["announcer.param."]),
            Row("announcer_volume"),
            Row("announcer.remove"),
            Row("announcer.studio"),
        ],
    },
    // As before: each controller's rows under its own name.
    Branch {
        id: "Controls",
        name: m::section_controls,
        was: &[],
        entries: &[
            Row("bind_layout"),
            Row("bind.any"),
            Heading(m::heading_keyboard),
            Row("keys"),
            Family(&["keys.lane."]),
            Heading(m::heading_touch),
            Row("touch_controls"),
            Row("touch_hand"),
            Row("touch_size"),
            Family(&["pad.", "pad_style."]),
            Heading(m::heading_controllers),
            Row("fast_pad_poll"),
            Row("pads.none"),
        ],
    },
    // What song select offers and how it starts a song.
    Branch {
        id: "Song list",
        name: m::section_song_list,
        was: &["Songs"],
        entries: &[
            Row("start_countdown"),
            Row("chart_ratings"),
            Heading(m::heading_simplify),
            Row("simplify_onigiri"),
            Row("simplify_center"),
            Heading(m::heading_convert),
            Row("convert_pump_single"),
            Row("convert_five_keys"),
            Heading(m::heading_song_folders),
            Family(&["folders."]),
        ],
    },
    Branch {
        id: "Multiplayer",
        name: m::section_multiplayer,
        was: &[],
        entries: &[
            Row("opponent_fields"),
            Row("dancing_hand_multiplayer"),
            Row("room_ahead"),
            Row("twitch_chat"),
            Row("chat_style"),
            Row("chat_names"),
            Row("chat_emotes_from"),
            Row("chat_opacity"),
        ],
    },
    // The menus, and the switch between the two levels first.
    Branch {
        id: "Interface",
        name: m::section_interface,
        was: &[],
        entries: &[
            Row(SHOW_ADVANCED),
            Row("language"),
            Row("reduced_motion"),
            Row("cursor_style"),
            Row("cursor_size"),
            Heading(m::heading_troubleshooting),
            Row("graphics"),
            Row("debug"),
            Row("demo"),
            Row(EXPERIMENTAL),
        ],
    },
    // Sharing (behind its switch on the experimental settings screen,
    // the plan's stage 4) last, at Advanced.
    Branch {
        id: "Data",
        name: m::section_data,
        was: &["Sharing"],
        entries: &[
            Row("history.export"),
            Row("twitch"),
            Row("privacy"),
            Row("terms"),
            Row("usage_stats"),
            Row("error_reports"),
            Row("reset.settings"),
            Row("reset.songs"),
            Row("reset.everything"),
            Heading(m::heading_sharing),
            Family(&["share."]),
        ],
    },
];

/// The section a position saved under section `id` lands in: `id`'s own,
/// or the one that took its rows.
pub fn section_for(id: &str) -> Option<&'static str> {
    TREE.iter()
        .find(|b| b.id == id || b.was.contains(&id))
        .map(|b| b.id)
}

/// Whether row `id` is what `entry` places.
fn takes(entry: &Entry, id: &str) -> bool {
    match entry {
        Heading(_) => false,
        Row(r) => *r == id,
        Family(prefixes) => prefixes.iter().any(|p| id.starts_with(p)),
    }
}

/// The tree's sections from the model's parts: each row where [`TREE`]
/// places it, with the tree's heading where it gives one (a row keeps its
/// own otherwise: a controller's name), a section the tree has that ends
/// up empty left out. A row the tree does not place (none should: a test
/// holds every row to it) goes to the end of the section that took its
/// part, so that nothing is ever lost.
pub fn arrange(parts: Vec<OptionSection>) -> Vec<OptionSection> {
    let mut pool: Vec<(String, Option<OptionDef>)> = parts
        .into_iter()
        .flat_map(|s| {
            let id = s.id;
            s.options.into_iter().map(move |o| (id.clone(), Some(o)))
        })
        .collect();
    let mut out: Vec<OptionSection> = TREE
        .iter()
        .map(|branch| {
            let mut rows = Vec::new();
            let mut heading: Option<String> = None;
            for entry in branch.entries {
                if let Heading(h) = entry {
                    heading = Some(h().into());
                    continue;
                }
                for (_, slot) in pool.iter_mut() {
                    if slot.as_ref().is_some_and(|o| takes(entry, &o.id)) {
                        let mut o = slot.take().expect("checked");
                        if let Some(h) = heading.take() {
                            o.heading = Some(h);
                        }
                        rows.push(o);
                    }
                }
            }
            OptionSection {
                id: branch.id.into(),
                name: (branch.name)().into(),
                options: rows,
            }
        })
        .collect();
    for (part, slot) in pool {
        let Some(o) = slot else { continue };
        let to = section_for(&part).unwrap_or("Data");
        if let Some(s) = out.iter_mut().find(|s| s.id == to) {
            s.options.push(o);
        }
    }
    out.retain(|s| !s.options.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::rc::Rc;

    use ddi_platform::storage::{self, MemoryStore};

    use super::*;
    use crate::state::settings::Level;

    /// Every row the model makes is placed by an entry of the tree (none
    /// falls to the end of a section unseen), and no entry places a row
    /// of another's: an id is in one place.
    #[test]
    fn the_tree_places_every_row_once() {
        storage::install(Rc::new(MemoryStore::default()));
        for m in super::super::options_levels::tests::fullest() {
            let d = m.data_at(Level::Advanced);
            for s in &d.sections {
                let branch = TREE.iter().find(|b| b.id == s.id);
                for o in &s.options {
                    let Some(branch) = branch else {
                        continue;
                    };
                    let placed: Vec<&Entry> =
                        branch.entries.iter().filter(|e| takes(e, &o.id)).collect();
                    assert_eq!(placed.len(), 1, "{} in {}: {placed:?}", o.id, s.id);
                    let elsewhere = TREE
                        .iter()
                        .filter(|b| b.id != s.id)
                        .any(|b| b.entries.iter().any(|e| takes(e, &o.id)));
                    assert!(!elsewhere, "{} placed twice", o.id);
                }
            }
        }
        // Ids and the sections' former ids are distinct.
        let mut seen = HashSet::new();
        for b in TREE {
            assert!(seen.insert(b.id), "{}", b.id);
            for w in b.was {
                assert!(seen.insert(*w), "{w}");
            }
        }
    }

    /// The plan's tree at Advanced on the web, a fresh player's: twelve
    /// sections in its order, the switch first in Interface.
    #[test]
    fn the_sections_are_the_plans() {
        storage::install(Rc::new(MemoryStore::default()));
        let m = super::super::options_model::OptionsModel::new(
            crate::state::settings::Settings::default(),
            super::super::options_model::World::default(),
        );
        let d = m.data_at(Level::Advanced);
        let ids: Vec<&str> = d.sections.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "Play",
                "Arrows",
                "Screen",
                "Backgrounds",
                "Rules",
                "Timing",
                "Sound",
                "Controls",
                "Song list",
                "Multiplayer",
                "Interface",
                "Data"
            ]
        );
        let names: Vec<&str> = d.sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names[11], "Data and privacy");
        let interface = &d.sections[10].options;
        assert_eq!(interface[0].id, SHOW_ADVANCED);
        assert_eq!(interface[0].heading, None);
        let demo = interface.iter().position(|o| o.id == "graphics").unwrap();
        assert_eq!(interface[demo].heading.as_deref(), Some("TROUBLESHOOTING"));
        // Positions saved before land in the section that took the rows.
        assert_eq!(section_for("Notes"), Some("Play"));
        assert_eq!(section_for("Display"), Some("Screen"));
        assert_eq!(section_for("Songs"), Some("Song list"));
        assert_eq!(section_for("Sharing"), Some("Data"));
        assert_eq!(section_for("Data"), Some("Data"));
        assert_eq!(section_for("Plugins"), None);
    }

    /// The tree as it shows, for the maintainer's map (`cargo test -p
    /// ddi-ui --lib tree_map -- --ignored --nocapture`): each world's
    /// sections at each level, a line a heading and a row (id, label,
    /// level), tab-separated.
    #[test]
    #[ignore]
    fn tree_map() {
        storage::install(Rc::new(MemoryStore::default()));
        use super::super::data::Platform;
        use super::super::options_model::{OptionsModel, Sample, World};
        use crate::state::settings::Settings;
        let mut worlds: Vec<(String, OptionsModel)> = Vec::new();
        for platform in [Platform::Web, Platform::Desktop] {
            let world = World {
                platform,
                pads_supported: true,
                sample: Sample::Ready,
                folders: (platform == Platform::Desktop).then(|| vec!["/Songs".into()]),
                ..World::default()
            };
            worlds.push((
                format!("new-{platform:?}"),
                OptionsModel::new(Settings::default(), world),
            ));
        }
        for m in super::super::options_levels::tests::fullest() {
            let name = format!("full-{:?}-{}", m.world.platform, m.settings.ruleset);
            worlds.push((name, m));
        }
        for (name, m) in &worlds {
            for level in [Level::Basic, Level::Advanced] {
                for s in &m.data_at(level).sections {
                    for o in &s.options {
                        if let Some(h) = &o.heading {
                            println!("{name}\t{level:?}\t{}\t{}\theading\t{h}", s.id, s.name);
                        }
                        println!(
                            "{name}\t{level:?}\t{}\t{}\trow\t{}\t{}\t{:?}",
                            s.id, s.name, o.id, o.label, o.level
                        );
                    }
                }
            }
        }
    }
}
