//! The library on generated folder trees and zips in a scratch folder.

use super::*;

const SM: &str = "#TITLE:Song;#ARTIST:Me;#MUSIC:song.ogg;#BANNER:bn.png;#OFFSET:0;\
    #BPMS:0=120;#NOTES:dance-single:x:Easy:3:0,0,0,0,0:1000\n0100\n0010\n0001\n;";

pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("ddi-library-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, files: &[(&str, &[u8])]) {
    for (p, data) in files {
        let path = root.join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, data).unwrap();
    }
}

/// A zip of stored entries.
pub(crate) fn zip(path: &Path, files: &[(&str, &[u8])]) {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let crc = ddi_library::zip::crc32(data);
        let offset = out.len() as u32;
        let header = |sig: u32, central: bool| {
            let mut h = sig.to_le_bytes().to_vec();
            if central {
                h.extend(20u16.to_le_bytes());
            }
            h.extend(20u16.to_le_bytes());
            h.extend(0x0800u16.to_le_bytes()); // UTF-8 names
            h.extend(0u16.to_le_bytes()); // stored
            h.extend([0; 4]); // time, date
            h.extend(crc.to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            if central {
                h.extend([0; 6]); // comment, disk, internal attributes
                h.extend([0; 4]); // external attributes
                h.extend(offset.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        out.extend(header(0x0403_4b50, false));
        out.extend_from_slice(data);
        central.extend(header(0x0201_4b50, true));
    }
    let at = out.len() as u32;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(at.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    std::fs::write(path, out).unwrap();
}

/// [`scan_all`] to its end.
fn scan(
    folders: &[PathBuf],
    cache: Option<&Cache>,
    force: &[PathBuf],
) -> (Vec<LibSong>, Report, Cache) {
    let force = Force {
        folders: force.to_vec(),
        packs: Vec::new(),
    };
    scan_all(folders, cache, &force, &|| false).unwrap_or_else(|_| panic!("superseded"))
}

fn ids(songs: &[LibSong]) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = songs
        .iter()
        .map(|s| (s.pack.clone(), s.entry.title.clone()))
        .collect();
    v.sort();
    v
}

#[test]
fn a_stepmania_folder_with_a_zip_lists_as_the_web_imports() {
    let root = scratch("stepmania");
    write(
        &root,
        &[
            ("Songs/Pack/cover.PNG", b"pack cover"),
            ("Songs/Pack/Song A/a.sm", SM.as_bytes()),
            // Named in other cases than the simfile writes them.
            ("Songs/Pack/Song A/Song.OGG", b"music"),
            ("Songs/Pack/Song A/BN.png", b"img"),
            (
                "Songs/Pack/Song B/b.ssc",
                SM.replace("Song;", "B;").as_bytes(),
            ),
            ("Songs/Pack/Song B/song.ogg", b"music"),
            ("Songs/Pack/Song B/.DS_Store", b"x"),
            ("RandomMovies/m.avi", b"avi"),
        ],
    );
    zip(
        &root.join("Songs/Zipped.zip"),
        &[
            ("Zipped/cover.jpg", b"zip cover"),
            ("Zipped/Z/z.sm", SM.replace("Song;", "Z;").as_bytes()),
            ("Zipped/Z/song.ogg", b"music"),
            ("Zipped/Z/bn.png", b"img"),
        ],
    );
    let (songs, report, _) = scan(std::slice::from_ref(&root), None, &[]);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(report.missing.is_empty(), "{:?}", report.missing);
    assert_eq!(
        ids(&songs),
        vec![
            ("Pack".into(), "B".into()),
            ("Pack".into(), "Song".into()),
            ("Zipped".into(), "Z".into()),
        ]
    );
    // The web's ids for the same pack.
    let a = songs.iter().find(|s| s.entry.title == "Song").unwrap();
    assert_eq!(a.entry.id, pack::song_id("Pack", "Pack/Song A"));
    assert_eq!(a.entry.music, "Song.OGG");
    assert_eq!(
        a.pack_banner.as_deref().map(RelPath::as_str),
        Some("Pack/cover.PNG")
    );
    assert_eq!(
        a.root_source(a.pack_banner.as_deref().unwrap()),
        root.join("Songs")
            .join("Pack")
            .join("cover.PNG")
            .display()
            .to_string()
    );
    assert_eq!(
        a.source(a.entry.banner.as_deref().unwrap()),
        root.join("Songs")
            .join("Pack")
            .join("Song A")
            .join("BN.png")
            .display()
            .to_string()
    );
    let z = songs.iter().find(|s| s.zip).unwrap();
    assert_eq!(z.entry.id, pack::song_id("Zipped", "Zipped/Z"));
    assert_eq!(
        z.source("bn.png"),
        format!(
            "zip:{}!Zipped/Z/bn.png",
            root.join("Songs").join("Zipped.zip").display()
        )
    );
    assert_eq!(
        z.pack_banner.as_deref().map(RelPath::as_str),
        Some("Zipped/cover.jpg")
    );
    assert_eq!(
        z.root_source(z.pack_banner.as_deref().unwrap()),
        format!("zip:{}!Zipped/cover.jpg", z.root.display())
    );
    // The zip's files read in place.
    let r = ZipReader::open(&z.root, &shift_jis).unwrap();
    assert_eq!(r.read(&z.rooted("song.ogg")).unwrap(), b"music");
}

#[test]
fn the_cache_reuses_what_did_not_change() {
    let root = scratch("cache");
    write(
        &root,
        &[
            ("Pack/A/a.sm", SM.as_bytes()),
            ("Pack/A/song.ogg", b"m"),
            ("Pack/B/b.sm", SM.replace("Song;", "B;").as_bytes()),
            ("Pack/B/song.ogg", b"m"),
        ],
    );
    let folders = vec![root.clone()];
    let (songs, r1, cache) = scan(&folders, None, &[]);
    assert_eq!((songs.len(), r1.parsed, r1.reused), (2, 2, 0));
    // Again: everything from the cache, through its file.
    let file = root.join("cache.json");
    save_cache(&file, &cache);
    let cache = load_cache(&file).unwrap();
    let (again, r2, cache) = scan(&folders, Some(&cache), &[]);
    assert_eq!((r2.parsed, r2.reused), (0, 2));
    assert_eq!(ids(&again), ids(&songs));
    // A changed simfile is read again; a removed song is gone.
    write(
        &root,
        &[("Pack/A/a.sm", SM.replace("Song;", "Changed;").as_bytes())],
    );
    std::fs::remove_dir_all(root.join("Pack/B")).unwrap();
    let (now, r3, _) = scan(&folders, Some(&cache), &[]);
    assert_eq!((r3.parsed, r3.reused), (1, 0));
    assert_eq!(ids(&now), vec![("Pack".into(), "Changed".into())]);
    // "Read again" reads all.
    let (_, r4, _) = scan(&folders, Some(&cache), std::slice::from_ref(&root));
    assert_eq!(r4.reused, 0);
}

#[test]
fn the_first_folder_holding_a_pack_wins() {
    let a = scratch("first");
    let b = scratch("second");
    for root in [&a, &b] {
        write(
            root,
            &[("Pack/S/s.sm", SM.as_bytes()), ("Pack/S/song.ogg", b"m")],
        );
    }
    let (songs, report, _) = scan(&[a.clone(), b.clone()], None, &[]);
    assert_eq!(songs.len(), 1);
    assert_eq!(songs[0].root, a);
    assert_eq!(report.duplicates.len(), 1, "{:?}", report.duplicates);
    assert!(report.duplicates[0].contains("Pack"));
    // A folder that is gone is reported, not fatal.
    let (_, report, _) = scan(&[a.join("nothing")], None, &[]);
    assert_eq!(report.missing.len(), 1);
}

#[test]
fn a_single_pack_folder_names_its_pack_after_itself() {
    let root = scratch("My Pack");
    write(
        &root,
        &[("Song/s.sm", SM.as_bytes()), ("Song/song.ogg", b"m")],
    );
    let (songs, _, _) = scan(std::slice::from_ref(&root), None, &[]);
    let pack_name = root.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(songs[0].pack, pack_name);
    assert_eq!(songs[0].entry.id, pack::song_id(&pack_name, "Song"));
}

/// The ids the web's import gives the files of `paths` (named as the web
/// names a pick's files: the picked folder's name first, a zip's entries
/// under its name), through the same shared code.
fn web_ids(paths: &[&str], fallback: &str) -> Vec<(String, String)> {
    let mut paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    paths.sort();
    let mut v: Vec<(String, String)> = pack::find_songs(&paths, fallback)
        .iter()
        .map(|c| (c.pack.clone(), c.identity().0))
        .collect();
    v.sort();
    v
}

fn desktop_ids(songs: &[LibSong]) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = songs
        .iter()
        .map(|s| (s.pack.clone(), s.entry.id.clone()))
        .collect();
    v.sort();
    v
}

#[test]
fn every_layout_has_the_webs_ids() {
    let base = scratch("layouts");
    let song = |dir: &str| -> Vec<(String, Vec<u8>)> {
        vec![
            (format!("{dir}/s.sm"), SM.as_bytes().to_vec()),
            (format!("{dir}/song.ogg"), b"m".to_vec()),
        ]
    };
    let put = |root: &Path, files: Vec<(String, Vec<u8>)>| {
        for (p, d) in files {
            write(root, &[(p.as_str(), d.as_slice())]);
        }
    };
    // A pack folder whose name has a dot.
    let dotted = base.join("Pack v1.5");
    put(&dotted, song("Song"));
    let (songs, ..) = scan(std::slice::from_ref(&dotted), None, &[]);
    assert_eq!(
        desktop_ids(&songs),
        web_ids(
            &["Pack v1.5/Song/s.sm", "Pack v1.5/Song/song.ogg"],
            pack::LOOSE_PACK
        )
    );
    assert_eq!(songs[0].pack, "Pack v1.5");
    // A StepMania root with a song loose in Songs/ and a pack.
    let sm = base.join("StepMania 5.1");
    put(&sm, song("Songs/Loose"));
    put(&sm, song("Songs/Pack/In Pack"));
    let (songs, ..) = scan(std::slice::from_ref(&sm), None, &[]);
    assert_eq!(
        desktop_ids(&songs),
        web_ids(
            &[
                "StepMania 5.1/Songs/Loose/s.sm",
                "StepMania 5.1/Songs/Loose/song.ogg",
                "StepMania 5.1/Songs/Pack/In Pack/s.sm",
                "StepMania 5.1/Songs/Pack/In Pack/song.ogg",
            ],
            pack::LOOSE_PACK
        )
    );
    // A folder that is itself a song.
    let itself = base.join("My Song");
    put(&itself, song("."));
    let (songs, ..) = scan(std::slice::from_ref(&itself), None, &[]);
    assert_eq!(
        desktop_ids(&songs),
        web_ids(&["My Song/s.sm", "My Song/song.ogg"], pack::LOOSE_PACK)
    );
    assert_eq!(songs[0].dir, "");
    // A zip with one song at its root, added alone and lying in a songs
    // folder.
    let z = base.join("zips");
    std::fs::create_dir_all(z.join("Songs")).unwrap();
    let files: &[(&str, &[u8])] = &[("a.sm", SM.as_bytes()), ("song.ogg", b"m")];
    zip(&z.join("Foo v2.zip"), files);
    zip(&z.join("Songs/Foo v2.zip"), files);
    let (songs, ..) = scan(&[z.join("Foo v2.zip")], None, &[]);
    assert_eq!(
        desktop_ids(&songs),
        web_ids(&["Foo v2/a.sm", "Foo v2/song.ogg"], "Foo v2")
    );
    let (songs, ..) = scan(&[z.join("Songs")], None, &[]);
    assert_eq!(
        desktop_ids(&songs),
        web_ids(
            &["Songs/Foo v2/a.sm", "Songs/Foo v2/song.ogg"],
            pack::LOOSE_PACK
        )
    );
    // Its files read where they are.
    let r = ZipReader::open(&songs[0].root, &shift_jis).unwrap();
    assert_eq!(
        r.read(&songs[0].rooted(&songs[0].entry.music)).unwrap(),
        b"m"
    );
}

#[test]
fn a_pack_read_again_is_read_whole_and_the_rest_reused() {
    let root = scratch("reread");
    write(
        &root,
        &[
            ("A/S/s.sm", SM.as_bytes()),
            ("A/S/song.ogg", b"m"),
            ("B/S/s.sm", SM.as_bytes()),
            ("B/S/song.ogg", b"m"),
        ],
    );
    let folders = vec![root.clone()];
    let (_, _, cache) = scan(&folders, None, &[]);
    let force = Force {
        folders: Vec::new(),
        packs: vec!["a".into()],
    };
    let (_, r, _) = scan_all(&folders, Some(&cache), &force, &|| false)
        .unwrap_or_else(|_| panic!("superseded"));
    assert_eq!((r.parsed, r.reused), (1, 1));
}

/// A page work as the scan finds it: its song, from the cache while its
/// folder's listing is the same, read again when it changes; a page too
/// large to hold a work is not read.
#[test]
fn works_are_cached_by_their_folders_listing() {
    let root = scratch("works");
    let page = |title: &str| {
        format!(
            "<html><head><meta charset=\"UTF-8N\"></head><body>\
             <input type=\"hidden\" id=\"dos\" value='|musicTitle={title},Me|difData=5,Easy,3.5\
             |musicUrl=song.ogg|left_data=300,400|'></body></html>"
        )
    };
    write(
        &root,
        &[
            ("Pack/danoni/work.html", page("曲名").as_bytes()),
            ("Pack/music/song.ogg", b"m"),
        ],
    );
    let folders = vec![root.clone()];
    let (songs, r1, cache) = scan(&folders, None, &[]);
    assert_eq!(songs.len(), 1, "{:?}", r1.failed);
    // An unknown charset label: read by what the bytes are (UTF-8 here).
    assert_eq!(songs[0].entry.title, "曲名");
    assert_eq!(r1.parsed, 1);
    let (again, r2, cache) = scan(&folders, Some(&cache), &[]);
    assert_eq!((r2.parsed, r2.reused), (0, 1));
    assert_eq!(again, songs);
    write(
        &root,
        &[("Pack/danoni/work.html", page("Changed").as_bytes())],
    );
    let (now, r3, _) = scan(&folders, Some(&cache), &[]);
    assert_eq!((r3.parsed, r3.reused), (1, 0));
    assert_eq!(now[0].entry.title, "Changed");
    // Larger than a work's page: not read.
    let mut big = page("Big").into_bytes();
    big.resize(MAX_PAGE + 10, b' ');
    write(&root, &[("Pack/danoni/work.html", &big)]);
    let (none, _, _) = scan(&folders, None, &[]);
    assert!(none.is_empty());
}
