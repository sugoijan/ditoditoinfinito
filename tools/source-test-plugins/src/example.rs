//! The well-behaved example source, written on the SDK (the
//! `ddi-example-source` plugin's whole; in the library so that a test
//! binary elsewhere can run as it, `crates/desktop/tests/find_songs.rs`):
//! the host's tests' model plugin, and an example of every capability.
//!
//! - 24 songs, `song-1` to `song-24`, in two packs; searched by title
//!   with a level range, a style choice and an artist filter, a page at a
//!   time;
//! - browsed by pack; opened from a link (`https://songs.invalid/<pack>`);
//! - fetched as streamed files, `pack-a.zip` as a zip, `big-song` into the
//!   cache folder the game gives (its size is past the memory budget), and
//!   `local` where it is, a folder in the plugin's data folder;
//! - `slow-song` takes a while, telling progress and checking for a
//!   cancel;
//! - `song-24` narrows the room rule for itself: its songs are not for
//!   online rooms (`rooms`), whatever the manifest allows.
//!
//! [`ExampleAs`] is the same source under another id (a manifest of its
//! own beside it: another room rule, say).

use crate::{meter, simfile, song_files, zip};
use ddi_source_sdk::*;

const SONGS: u32 = 24;

/// The example source.
pub struct Example;

fn pack_of(n: u32) -> &'static str {
    if n <= 12 { "pack-a" } else { "pack-b" }
}

fn item(n: u32) -> ItemMeta {
    let mut charts = vec![ChartMeta {
        style: "dance-single".into(),
        difficulty: Difficulty::Hard,
        meter: Some(meter(n)),
        name: None,
        author: None,
    }];
    if n.is_multiple_of(2) {
        charts.push(ChartMeta {
            style: "dance-double".into(),
            difficulty: Difficulty::Medium,
            meter: Some(meter(n).saturating_sub(1).max(1)),
            name: None,
            author: None,
        });
    }
    ItemMeta {
        id: format!("song-{n}"),
        kind: Some(ItemKind::Song),
        title: format!("Generated Song {n}"),
        artist: Some(format!("Test Artist {}", n % 4)),
        pack: Some(pack_of(n).into()),
        charts,
        length: Some(0.25),
        bpm: Some([120.0, 120.0]),
        size: Some((simfile(n).len() + 4044) as u64),
        license: Some("CC0 1.0".into()),
        revision: Some("r1".into()),
        // An item may narrow its source's room rule, never widen it.
        rooms: (n == SONGS).then_some(RoomRule::None),
        ..ItemMeta::default()
    }
}

fn number(item: &str) -> Option<u32> {
    item.strip_prefix("song-")?
        .parse()
        .ok()
        .filter(|n| (1..=SONGS).contains(n))
}

impl Source for Example {
    fn info(&self) -> Info {
        Info::new(
            "example",
            "1.0.0",
            [
                Capability::Input,
                Capability::Search,
                Capability::Filters,
                Capability::Browse,
                Capability::Metadata,
                Capability::Fetch,
            ],
        )
        .inputs([InputKind::Url])
        .filters([
            Filter {
                id: "level".into(),
                label: Text::en("Level").with("ja", "レベル"),
                kind: FilterKind::Range {
                    min: 1.0,
                    max: 20.0,
                    step: 1.0,
                    unit: Unit::Meter,
                },
            },
            Filter {
                id: "style".into(),
                label: Text::en("Style"),
                kind: FilterKind::Choice {
                    multiple: true,
                    choices: vec![
                        Choice {
                            id: "single".into(),
                            label: Text::en("Single"),
                        },
                        Choice {
                            id: "double".into(),
                            label: Text::en("Double"),
                        },
                    ],
                },
            },
            Filter {
                id: "artist".into(),
                label: Text::en("Artist"),
                kind: FilterKind::Text {
                    max_chars: Some(64),
                },
            },
        ])
    }

    fn open(&self, _cx: &Context, input: Input) -> Outcome<Opened> {
        let pack = input
            .value
            .strip_prefix("https://songs.invalid/")
            .filter(|p| matches!(*p, "pack-a" | "pack-b"))
            .ok_or_else(|| Failure::with(FailCode::InvalidInput, "not a pack's link"))?;
        Ok(Opened {
            scope: pack.into(),
            title: Some(Text::en(format!("Pack {pack}"))),
        })
    }

    fn search(&self, cx: &Context, q: SearchQuery) -> Outcome<Page> {
        cx.progress(Step::Searching, None, None, None);
        let query = q.query.to_lowercase();
        let matching: Vec<u32> = (1..=SONGS)
            .filter(|&n| q.scope.as_deref().is_none_or(|s| s == pack_of(n)))
            .filter(|&n| format!("generated song {n}").contains(&query))
            .filter(|&n| {
                q.filters.iter().all(|(id, value)| match (id.as_str(), value) {
                    ("level", FilterValue::Range { min, max }) => {
                        let m = f64::from(meter(n));
                        min.is_none_or(|lo| m >= lo) && max.is_none_or(|hi| m <= hi)
                    }
                    ("style", FilterValue::Choice { values }) => values.iter().any(|v| {
                        v == "single" || (v == "double" && n.is_multiple_of(2))
                    }),
                    ("artist", FilterValue::Text { value }) => {
                        format!("test artist {}", n % 4).contains(&value.to_lowercase())
                    }
                    _ => true,
                })
            })
            .collect();
        let start: usize = q
            .cursor
            .as_deref()
            .map(|c| c.parse().map_err(|_| Failure::new(FailCode::InvalidInput)))
            .transpose()?
            .unwrap_or(0);
        let end = (start + q.page_size as usize).min(matching.len());
        Ok(Page {
            items: matching[start.min(end)..end].iter().map(|&n| item(n)).collect(),
            cursor: (end < matching.len()).then(|| end.to_string()),
            total: Some(matching.len() as u64),
        })
    }

    fn browse(&self, _cx: &Context, q: BrowseQuery) -> Outcome<BrowsePage> {
        let node = q.node.or(q.scope);
        match node.as_deref() {
            None => Ok(BrowsePage {
                nodes: ["pack-a", "pack-b"]
                    .iter()
                    .map(|p| Node {
                        id: (*p).into(),
                        title: format!("Pack {p}"),
                        items: Some(12),
                        nodes: Some(0),
                    })
                    .collect(),
                ..BrowsePage::default()
            }),
            Some(pack @ ("pack-a" | "pack-b")) => Ok(BrowsePage {
                items: (1..=SONGS)
                    .filter(|&n| pack_of(n) == pack)
                    .take(q.page_size as usize)
                    .map(item)
                    .collect(),
                total: Some(12),
                ..BrowsePage::default()
            }),
            Some(_) => Err(Failure::new(FailCode::NotFound)),
        }
    }

    fn describe(&self, _cx: &Context, id: &str) -> Outcome<ItemMeta> {
        match id {
            "big-song" => Ok(ItemMeta {
                id: id.into(),
                kind: Some(ItemKind::Song),
                title: "A Big Song".into(),
                // Past the game's memory budget: it asks for a folder.
                size: Some(200 << 20),
                ..ItemMeta::default()
            }),
            _ => number(id)
                .map(item)
                .ok_or_else(|| Failure::new(FailCode::NotFound)),
        }
    }

    fn fetch(&self, cx: &Context, id: &str, out: &mut Fetch) -> Outcome<Delivery> {
        cx.progress(Step::Downloading, Some(0), Some(1), None);
        match id {
            "pack-a.zip" => {
                let files: Vec<(String, Vec<u8>)> = (1..=3)
                    .flat_map(|n| song_files(&format!("Pack A/Song {n}"), n))
                    .collect();
                out.archive("pack-a.zip", &zip(&files))
            }
            "big-song" => {
                let dir = out
                    .cache_dir()
                    .ok_or_else(|| Failure::with(FailCode::TooLarge, "expected a cache folder"))?
                    .to_path_buf();
                for (path, data) in song_files("Big Song", 7) {
                    let file = dir.join(&path);
                    std::fs::create_dir_all(file.parent().expect("a folder"))?;
                    std::fs::write(file, data)?;
                }
                out.cached()
            }
            "local" => {
                let dir = cx.dirs().data.join("local");
                for (path, data) in song_files("Local Song", 9) {
                    let file = dir.join(&path);
                    std::fs::create_dir_all(file.parent().expect("a folder"))?;
                    std::fs::write(file, data)?;
                }
                Ok(Delivery::InPlace { path: dir })
            }
            "slow-song" => {
                for i in 0..100 {
                    cx.check()?;
                    cx.progress(Step::Downloading, Some(i), Some(100), None);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                for (path, data) in song_files("Slow Song", 3) {
                    out.file(&path, &data)?;
                }
                Ok(out.files())
            }
            _ => {
                let n = number(id).ok_or_else(|| Failure::new(FailCode::NotFound))?;
                for (path, data) in song_files(&format!("Generated Song {n}"), n) {
                    out.file(&path, &data)?;
                }
                Ok(out.files())
            }
        }
    }
}

/// [`Example`] under the id `.0`: installed beside it with a manifest of
/// its own.
pub struct ExampleAs(pub &'static str);

impl Source for ExampleAs {
    fn info(&self) -> Info {
        Info {
            id: self.0.to_string(),
            ..Example.info()
        }
    }

    fn open(&self, cx: &Context, input: Input) -> Outcome<Opened> {
        Example.open(cx, input)
    }

    fn search(&self, cx: &Context, q: SearchQuery) -> Outcome<Page> {
        Example.search(cx, q)
    }

    fn browse(&self, cx: &Context, q: BrowseQuery) -> Outcome<BrowsePage> {
        Example.browse(cx, q)
    }

    fn describe(&self, cx: &Context, id: &str) -> Outcome<ItemMeta> {
        Example.describe(cx, id)
    }

    fn fetch(&self, cx: &Context, id: &str, out: &mut Fetch) -> Outcome<Delivery> {
        Example.fetch(cx, id, out)
    }
}
