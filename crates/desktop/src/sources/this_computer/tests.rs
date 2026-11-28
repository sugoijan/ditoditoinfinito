//! This computer over generated folders (in the build's scratch folder,
//! nothing of anyone's), through the host as the game asks it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ddi_library::path::RelPath;
use ddi_source_host::host::{Event, Request};
use ddi_source_host::protocol::{FilterValue, Input, InputKind, ItemMeta, Node, OpId};
use ddi_source_host::store::{Store, StoreLimits, Stored};
use ddi_source_host::{Host, HostConfig};

use super::*;

/// A generated song's simfile: `title`, a Single chart at `meter`, and a
/// Double one where `double`.
pub fn simfile(title: &str, artist: &str, meter: u32, double: bool) -> String {
    let mut s = format!(
        "#VERSION:0.83;\n#TITLE:{title};\n#ARTIST:{artist};\n#MUSIC:song.wav;\n#OFFSET:0;\n#BPMS:0=120;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:{meter};\n#NOTES:\n1000\n0100\n0010\n0001\n;\n"
    );
    if double {
        s.push_str("#NOTEDATA:;\n#STEPSTYPE:dance-double;\n#DIFFICULTY:Medium;\n#METER:3;\n#NOTES:\n10000000\n01000000\n00100000\n00010000\n;\n");
    }
    s
}

/// A quarter second of silence: 8 kHz, mono, 16-bit.
pub fn silence() -> Vec<u8> {
    let data = 4000u32;
    let mut w = Vec::new();
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    for v in [1u16, 1] {
        w.extend_from_slice(&v.to_le_bytes());
    }
    for v in [8000u32, 16000] {
        w.extend_from_slice(&v.to_le_bytes());
    }
    for v in [2u16, 16] {
        w.extend_from_slice(&v.to_le_bytes());
    }
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    w.resize(44 + data as usize, 0);
    w
}

/// A song folder written at `dir`.
pub fn song_at(dir: &Path, title: &str, artist: &str, meter: u32, double: bool) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("song.ssc"), simfile(title, artist, meter, double)).unwrap();
    std::fs::write(dir.join("song.wav"), silence()).unwrap();
}

/// A Dancing☆Onigiri work's page: `title`'s one song, its music
/// `music` (where danoniplus looks for it: `../music/` from the page),
/// eight notes from its frame 200.
pub fn work_page(title: &str, music: &str) -> String {
    let left: Vec<String> = (230..470).step_by(60).map(|f| f.to_string()).collect();
    let down: Vec<String> = (260..500).step_by(60).map(|f| f.to_string()).collect();
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>{title}</title></head><body>\n\
         <input type=\"hidden\" name=\"dos\" id=\"dos\" value='\n\
         |musicTitle={title},Generated,,,120|\n|difData=5,Easy,3.5|\n|musicUrl={music}|\n\
         |blankFrame=200|\n|left_data={}|\n|down_data={}|\n'>\n</body></html>\n",
        left.join(","),
        down.join(",")
    )
}

/// A work written in pack folder `pack`: its page in `danoni/`, its
/// music in `music/` beside it.
pub fn work_at(pack: &Path, title: &str) {
    std::fs::create_dir_all(pack.join("danoni")).unwrap();
    std::fs::create_dir_all(pack.join("music")).unwrap();
    std::fs::write(pack.join("music/work.wav"), silence()).unwrap();
    std::fs::write(pack.join("danoni/work.html"), work_page(title, "work.wav")).unwrap();
}

/// An FFR level of `notes` notes (from frame 30, a sixth of a second
/// apart), its music the platform's MP3 fixture.
pub fn level(notes: usize) -> Vec<u8> {
    let mp3 = include_bytes!("../../../../platform/tests/fixtures/sine-441-bare.mp3").repeat(8);
    let notes: Vec<(f64, &str, &str)> = (0..notes)
        .map(|i| (30.0 + 5.0 * i as f64, ["L", "D", "U", "R"][i % 4], "blue"))
        .collect();
    ddi_library::swf::write_level(&notes, &mp3, true)
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch/this-computer")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// A folder of songs: two in a pack, one in a zip, one too deep, and
/// (on Unix) a link to a folder outside with a song in it.
fn folder(root: &Path) -> PathBuf {
    let songs = root.join("songs");
    song_at(&songs.join("Pack A/Song One"), "Song One", "Alpha", 4, false);
    song_at(&songs.join("Pack A/Song Two"), "Song Two", "Beta", 9, true);
    let zipped: Vec<(&str, Vec<u8>)> = vec![
        (
            "Zipped/Song Three/song.ssc",
            simfile("Song Three", "Gamma", 6, false).into_bytes(),
        ),
        ("Zipped/Song Three/song.wav", silence()),
    ];
    let borrowed: Vec<(&str, &[u8])> = zipped.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    std::fs::write(
        songs.join("Zipped.zip"),
        ddi_library::zip::write_stored(&borrowed).unwrap(),
    )
    .unwrap();
    // Deeper than the bounds allow (depth 2 in the tests).
    song_at(&songs.join("a/b/c/Pack/Too Deep"), "Too Deep", "Delta", 2, false);
    let outside = root.join("outside");
    song_at(&outside.join("Pack O/Song Out"), "Song Out", "Omega", 3, false);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, songs.join("Linked")).unwrap();
    songs
}

fn host(root: &Path, source: Arc<ThisComputer>) -> Host {
    let store = Store::open(root.join("cache/sources"), StoreLimits::default()).shared();
    let mut config = HostConfig::new(root.join("cache/plugins"), root.join("data/plugin-data"));
    // As the game's: works and levels read as its library reads them.
    config.more = crate::library::item_songs;
    let mut host = Host::new(config, Vec::new(), &BTreeSet::new(), store, None);
    host.add_builtin(source);
    host
}

fn bounds() -> Bounds {
    Bounds {
        depth: 2,
        ..Bounds::default()
    }
}

/// `op`'s events to its end.
fn answer(host: &mut Host, op: OpId) -> Vec<Event> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut out = Vec::new();
    while Instant::now() < deadline {
        let Some(e) = host.wait_event(Duration::from_millis(20)) else {
            continue;
        };
        let end = e.ends() == Some(op);
        if !matches!(e, Event::Progress { .. }) {
            out.push(e);
        }
        if end {
            return out;
        }
    }
    panic!("operation {op} did not end: {out:?}");
}

fn search(host: &mut Host, query: &str, filters: BTreeMap<String, FilterValue>) -> Vec<ItemMeta> {
    let op = host.request(
        ID,
        Request::Search {
            scope: None,
            query: query.into(),
            filters,
            cursor: None,
            page_size: 50,
        },
    );
    let events = answer(host, op);
    let mut items = Vec::new();
    for e in events {
        match e {
            Event::Results { items: i, .. } => items.extend(i),
            Event::Done { .. } => {}
            other => panic!("{other:?}"),
        }
    }
    items
}

fn titles(items: &[ItemMeta]) -> Vec<&str> {
    items.iter().map(|i| i.title.as_str()).collect()
}

#[test]
fn it_finds_folders_and_zips_songs_never_through_a_link_or_past_its_depth() {
    let root = scratch("finds");
    let songs = folder(&root);
    let source = Arc::new(ThisComputer::with_bounds(vec![songs.clone()], bounds()));
    let mut host = host(&root, source.clone());
    let all = search(&mut host, "", BTreeMap::new());
    assert_eq!(titles(&all), ["Song One", "Song Two", "Song Three"]);
    assert!(source.cut(), "the folder too deep was cut and said so");
    let one = &all[0];
    assert_eq!(one.pack.as_deref(), Some("Pack A"));
    assert_eq!(one.artist.as_deref(), Some("Alpha"));
    assert_eq!(one.charts[0].style, "dance-single");
    assert_eq!(one.charts[0].meter, Some(4));
    assert!(one.song_key.as_deref().is_some_and(
        ddi_source_host::protocol::check::is_song_key
    ));
    assert!(one.revision.is_some() && one.size.is_some_and(|s| s > 44));
    // By text (title, artist, pack), and by the filters.
    assert_eq!(titles(&search(&mut host, "two", BTreeMap::new())), ["Song Two"]);
    assert_eq!(titles(&search(&mut host, "gamma", BTreeMap::new())), ["Song Three"]);
    assert_eq!(
        titles(&search(&mut host, "pack a", BTreeMap::new())),
        ["Song One", "Song Two"]
    );
    let style = BTreeMap::from([(
        "style".to_string(),
        FilterValue::Choice {
            values: vec!["dance-double".into()],
        },
    )]);
    assert_eq!(titles(&search(&mut host, "", style)), ["Song Two"]);
    let level = BTreeMap::from([(
        "level".to_string(),
        FilterValue::Range {
            min: Some(5.0),
            max: Some(8.0),
        },
    )]);
    assert_eq!(titles(&search(&mut host, "", level)), ["Song Three"]);

    // Browsed: the folders, a folder's packs, a pack's songs.
    let browse = |host: &mut Host, node: Option<String>| {
        let op = host.request(
            ID,
            Request::Browse {
                scope: None,
                node,
                cursor: None,
                page_size: 50,
            },
        );
        let mut nodes: Vec<Node> = Vec::new();
        let mut items = Vec::new();
        for e in answer(host, op) {
            match e {
                Event::Nodes { nodes: n, .. } => nodes.extend(n),
                Event::Results { items: i, .. } => items.extend(i),
                Event::Done { .. } => {}
                other => panic!("{other:?}"),
            }
        }
        (nodes, items)
    };
    let (top, _) = browse(&mut host, None);
    assert_eq!(top.len(), 1);
    let (packs, _) = browse(&mut host, Some(top[0].id.clone()));
    let names: Vec<&str> = packs.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(names, ["Pack A", "Zipped"]);
    let (_, items) = browse(&mut host, Some(packs[0].id.clone()));
    assert_eq!(titles(&items), ["Song One", "Song Two"]);

    // Opened: a folder named by the folder picker is a scope.
    let op = host.request(
        ID,
        Request::Open(Input {
            kind: InputKind::Folder,
            value: songs.display().to_string(),
        }),
    );
    assert!(matches!(answer(&mut host, op).pop(), Some(Event::Opened { .. })));

    // Fetched: a folder's song read where it is; a zip's into memory.
    let fetch = |host: &mut Host, item: &ItemMeta| {
        let op = host.request(
            ID,
            Request::Fetch {
                item: item.id.clone(),
                revision: item.revision.clone(),
                size: item.size,
                reach: None,
            },
        );
        match answer(host, op).pop() {
            Some(Event::Fetched { songs, .. }) => songs,
            other => panic!("{other:?}"),
        }
    };
    let fetched = fetch(&mut host, one);
    assert_eq!(fetched.len(), 1);
    assert!(fetched[0].id.starts_with("src:"));
    assert_eq!(fetched[0].entry.title, "Song One");
    assert_eq!(fetched[0].song_key, one.song_key);
    // The player's own files: everything, as far as a song goes.
    let key = fetched[0].key;
    assert_eq!(
        host.store().lock().unwrap().origin(key).map(|o| o.reach),
        Some(ddi_source_host::protocol::Reach::Keep)
    );
    let (song, files) = host.store().lock().unwrap().song(&fetched[0].id).unwrap();
    let wav = song.dir.join("song.wav");
    assert!(
        matches!(files.get(&wav), Some(Stored::Disk(p)) if p.starts_with(&songs)),
        "read in place"
    );
    assert_eq!(files.read(&wav).unwrap(), silence());
    assert_eq!(host.store().lock().unwrap().usage().memory, 0);
    let zipped = fetch(&mut host, &all[2]);
    let (song, files) = host.store().lock().unwrap().song(&zipped[0].id).unwrap();
    assert!(matches!(
        files.get(RelPath::new(&format!("{}/song.ssc", song.dir))),
        Some(Stored::Memory(_))
    ));
    assert!(host.store().lock().unwrap().usage().memory > 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn past_its_entries_a_folder_is_read_in_part_and_says_so() {
    let root = scratch("bounds");
    let songs = root.join("songs");
    for n in 0..12 {
        song_at(
            &songs.join(format!("Pack/Song {n:02}")),
            &format!("Song {n:02}"),
            "Many",
            3,
            false,
        );
    }
    let few = Bounds {
        entries: 16,
        ..Bounds::default()
    };
    let source = Arc::new(ThisComputer::with_bounds(vec![songs.clone()], few));
    let mut host = host(&root, source.clone());
    let found = search(&mut host, "", BTreeMap::new());
    assert!(source.cut());
    assert!(!found.is_empty() && found.len() < 12, "{}", found.len());
    // Read again from the start once the page opens again.
    source.refresh();
    assert!(!source.cut());
    // No folder named: nothing found, nothing read.
    source.set_folders(Vec::new());
    assert!(search(&mut host, "", BTreeMap::new()).is_empty());
    let _ = std::fs::remove_dir_all(root);
}

/// Importing in place: what is added to the library's song folders for a
/// song of a pack (its pack's folder), a second of the same pack once the
/// first is in (nothing), a loose song and one directly in a folder
/// searched (its own folder), a zip's song (the zip), and a song a song
/// folder holds already (nothing); a pack holding
/// a listed song folder is not added whole (its songs would be read twice).
#[test]
fn an_import_adds_the_pack_or_the_song_or_the_zip_in_place_once() {
    let named = PathBuf::from("/songs");
    let folder = |dir: &str, pack: &str| Place::Folder {
        dir: PathBuf::from(dir),
        pack: pack.into(),
    };
    let one = folder("/songs/Pack A/Song One", "Pack A");
    let two = folder("/songs/Pack A/Song Two", "Pack A");
    let add = |p: &str| InPlace::Add(PathBuf::from(p));
    let covered = |p: &str| InPlace::Covered(PathBuf::from(p));
    let named = std::slice::from_ref(&named);
    // One of a pack: the pack's folder; then the second: in it already.
    assert_eq!(in_place_of(&one, named, &[]), add("/songs/Pack A"));
    let listed = [PathBuf::from("/songs/Pack A")];
    assert_eq!(in_place_of(&two, named, &listed), covered("/songs/Pack A"));
    // A loose song, the folder named itself: its own folder.
    let loose = folder("/elsewhere/Loose Song", "elsewhere");
    let here = [PathBuf::from("/elsewhere/Loose Song")];
    assert_eq!(in_place_of(&loose, &here, &[]), add("/elsewhere/Loose Song"));
    // A song directly in the folder named: its own folder, never the
    // folder named; listed, nothing.
    let top = folder("/songs/Top Song", "songs");
    assert_eq!(in_place_of(&top, named, &[]), add("/songs/Top Song"));
    assert_eq!(
        in_place_of(&top, named, &[PathBuf::from("/songs/Top Song")]),
        covered("/songs/Top Song")
    );
    // A pack folder named itself: its songs are directly in it, each its
    // own folder.
    let pack_named = [PathBuf::from("/songs/Pack A")];
    assert_eq!(
        in_place_of(&one, &pack_named, &[]),
        add("/songs/Pack A/Song One")
    );
    // A zip's song: the zip.
    let zipped = Place::Zip {
        zip: PathBuf::from("/songs/Zipped.zip"),
        dir: ddi_library::path::RelPathBuf::from("Zipped/Song Three"),
    };
    assert_eq!(in_place_of(&zipped, named, &[]), add("/songs/Zipped.zip"));
    assert_eq!(
        in_place_of(&zipped, named, &[PathBuf::from("/songs/Zipped.zip")]),
        covered("/songs/Zipped.zip")
    );
    // A song folder of the library above it (a StepMania root): nothing.
    let root = [PathBuf::from("/songs")];
    assert_eq!(in_place_of(&one, named, &root), covered("/songs"));
    assert_eq!(in_place_of(&zipped, named, &root), covered("/songs"));
    // The library lists the first song's folder alone: the second is
    // added by its own folder, the first is in already.
    let alone = [PathBuf::from("/songs/Pack A/Song One")];
    assert_eq!(in_place_of(&two, named, &alone), add("/songs/Pack A/Song Two"));
    assert_eq!(
        in_place_of(&one, named, &alone),
        covered("/songs/Pack A/Song One")
    );
}

/// Works and FFR levels, in a folder and in a zip, found, fetched and
/// imported in place as the library reads them: a work an item (its page,
/// its music beside), a level one; fetched, each read again by the
/// library's own readers into the songs it lists once its folder (or the
/// zip) is added.
#[test]
fn it_finds_works_and_levels_and_reads_them_as_the_library_does() {
    let root = scratch("works");
    let songs = root.join("songs");
    work_at(&songs.join("WorkPack"), "Work Test");
    std::fs::create_dir_all(songs.join("FFR Levels")).unwrap();
    std::fs::write(songs.join("FFR Levels/level_1.swf"), level(8)).unwrap();
    // A zip of a work and a level.
    let page = work_page("Zip Work", "zip.wav");
    let swf = level(4);
    let wav = silence();
    let entries: Vec<(&str, &[u8])> = vec![
        ("Inside/danoni/work.html", page.as_bytes()),
        ("Inside/music/zip.wav", &wav),
        ("Inside/Levels/level_2.swf", &swf),
    ];
    std::fs::write(
        songs.join("Bundle.zip"),
        ddi_library::zip::write_stored(&entries).unwrap(),
    )
    .unwrap();
    let source = Arc::new(ThisComputer::new(vec![songs.clone()]));
    let mut host = host(&root, source.clone());
    let all = search(&mut host, "", BTreeMap::new());
    assert_eq!(titles(&all), ["level_2", "Zip Work", "level_1", "Work Test"]);
    assert!(!source.cut());
    let packs: Vec<Option<&str>> = all.iter().map(|i| i.pack.as_deref()).collect();
    assert_eq!(
        packs,
        [Some("Bundle"), Some("Bundle"), Some("FFR Levels"), Some("WorkPack")]
    );
    for item in &all {
        assert!(item.revision.is_some() && item.size.is_some_and(|s| s > 0), "{item:?}");
        assert!(!item.charts.is_empty(), "{item:?}");
        assert!(item.song_key.is_some(), "{item:?}");
    }
    assert_eq!(titles(&search(&mut host, "work test", BTreeMap::new())), ["Work Test"]);

    let fetch = |host: &mut Host, item: &ItemMeta| {
        let op = host.request(
            ID,
            Request::Fetch {
                item: item.id.clone(),
                revision: item.revision.clone(),
                size: item.size,
                reach: None,
            },
        );
        match answer(host, op).pop() {
            Some(Event::Fetched { songs, .. }) => songs,
            other => panic!("{other:?}"),
        }
    };
    // Each fetched is what the library lists once its folder is added:
    // the same entry but for its id, the same pack.
    let listed = |folder: &Path| {
        let (songs, report) = crate::library::scan_folders(&[folder.to_path_buf()]);
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        songs
    };
    let same = |fetched: &ddi_source_host::FetchedSong, lib: &crate::library::LibSong| {
        let mut entry = fetched.entry.clone();
        entry.id = lib.entry.id.clone();
        assert_eq!(entry, lib.entry);
        assert_eq!(fetched.pack, lib.pack);
        assert!(fetched.song_key.is_some());
    };
    let added = |item: &ItemMeta| match source.in_place(&item.id, &[]) {
        Some(InPlace::Add(p)) => p,
        other => panic!("{other:?}"),
    };
    // The work: read where it is, its page and music; played as a work.
    let work = fetch(&mut host, &all[3]);
    assert_eq!(work.len(), 1);
    let w = work[0].work.clone().expect("a work's song");
    assert_eq!(w.source.as_str(), "WorkPack/danoni/work.html");
    assert_eq!(w.music.as_str(), "WorkPack/music/work.wav");
    let (_, files) = host.store().lock().unwrap().song(&work[0].id).unwrap();
    assert!(matches!(files.get(&w.music), Some(Stored::Disk(p)) if p.starts_with(&songs)));
    assert_eq!(added(&all[3]), songs.join("WorkPack"));
    let lib = listed(&songs.join("WorkPack"));
    assert_eq!(lib.len(), 1);
    same(&work[0], &lib[0]);
    // Its files where the library has them, under the folder added.
    let lib_work = lib[0].work.as_ref().expect("the library's work");
    assert_eq!(RelPath::new("WorkPack").join(&lib_work.source), w.source);
    assert_eq!(RelPath::new("WorkPack").join(&lib_work.music), w.music);
    assert_eq!(work[0].song_key, all[3].song_key);
    // The level: its file, its music in it.
    let level = fetch(&mut host, &all[2]);
    assert_eq!(level.len(), 1);
    assert!(level[0].work.is_none());
    assert!(ddi_library::ffr::is_level_chart(&level[0].entry.chart));
    assert_eq!(added(&all[2]), songs.join("FFR Levels"));
    let lib = listed(&songs.join("FFR Levels"));
    assert_eq!(lib.len(), 1);
    same(&level[0], &lib[0]);
    // A zip's: into memory, as the library reads the zip once it is added.
    let zipped = listed(&songs.join("Bundle.zip"));
    assert_eq!(zipped.len(), 2);
    for item in &all[..2] {
        let got = fetch(&mut host, item);
        assert_eq!(got.len(), 1);
        let lib = zipped
            .iter()
            .find(|s| s.entry.title == item.title)
            .expect("listed");
        same(&got[0], lib);
        let (song, files) = host.store().lock().unwrap().song(&got[0].id).unwrap();
        let chart = song.dir.join(song.entry.chart.as_str());
        assert!(matches!(files.get(&chart), Some(Stored::Memory(_))), "{chart}");
        assert_eq!(added(item), songs.join("Bundle.zip"));
    }
    // The same ids, read again (the page opened again).
    let ids: Vec<String> = all.iter().map(|i| i.id.clone()).collect();
    source.refresh();
    let again = search(&mut host, "", BTreeMap::new());
    assert_eq!(again.iter().map(|i| i.id.clone()).collect::<Vec<_>>(), ids);
    // Covered once a song folder of the library holds it.
    assert_eq!(
        source.in_place(&all[3].id, std::slice::from_ref(&songs)),
        Some(InPlace::Covered(songs.clone()))
    );
    let _ = std::fs::remove_dir_all(root);
}
