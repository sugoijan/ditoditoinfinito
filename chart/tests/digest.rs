//! Chart digests (`ddi_chart::digest`) on the bundled songs: stable, blind
//! to metadata, changed by notes and timing. `DDI_DIFF_CORPUS=<dir>` also
//! checks every `.sm`/`.ssc` under a local folder (third-party songs stay
//! local): the same digest from two parses and from a copy with other
//! metadata, and how many charts share one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ddi_chart::formats::sm::parse_simfile;
use ddi_chart::{ChartDigest, NoteKind, Song, Tick};

fn bundled() -> Vec<(String, String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/songs");
    let mut out = Vec::new();
    let mut dirs: Vec<_> = std::fs::read_dir(&root)
        .expect("bundled songs")
        .flatten()
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for dir in dirs {
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .expect("song folder")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "ssc" || e == "sm"))
            .collect();
        files.sort();
        for f in files {
            let ext = f.extension().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(&f).expect("simfile");
            out.push((f.display().to_string(), text, ext));
        }
    }
    out
}

fn parse(text: &str, ext: &str) -> Song {
    parse_simfile(text, ext).expect("parses")
}

fn digests(song: &Song) -> Vec<ChartDigest> {
    (0..song.charts.len())
        .map(|i| song.chart_digest(i))
        .collect()
}

/// `text` with its song-level metadata tags given other values.
fn other_metadata(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let tag = line
            .trim_start()
            .strip_prefix('#')
            .and_then(|l| l.split(':').next())
            .map(|t| t.to_ascii_uppercase());
        match tag.as_deref() {
            Some(
                "TITLE" | "SUBTITLE" | "ARTIST" | "TITLETRANSLIT" | "ARTISTTRANSLIT" | "GENRE"
                | "CREDIT" | "BANNER" | "BACKGROUND" | "JACKET" | "CDTITLE" | "MUSIC"
                | "SAMPLESTART" | "SAMPLELENGTH" | "DESCRIPTION" | "CHARTNAME" | "METER"
                | "DIFFICULTY" | "DISPLAYBPM",
            ) => {
                let name = line.split(':').next().unwrap_or(line);
                let value = match tag.as_deref() {
                    Some("METER") => "13",
                    Some("DIFFICULTY") => "Edit",
                    Some("SAMPLESTART" | "SAMPLELENGTH") => "7.5",
                    Some("DISPLAYBPM") => "*",
                    _ => "Something else",
                };
                out.push_str(&format!("{name}:{value};"));
            }
            _ => out.push_str(line),
        }
        // Windows line endings, as a pack edited elsewhere might have.
        out.push_str("\r\n");
    }
    out
}

#[test]
fn a_bundled_charts_digest_is_pinned() {
    let songs = bundled();
    let (path, text, ext) = songs
        .iter()
        .find(|(p, ..)| p.contains("sweeteners"))
        .expect("Sweeteners is bundled");
    let song = parse(text, ext);
    let d = song.chart_digest(0);
    // Pinned: a change here splits every record of this chart from its
    // bests. Raise it only with a deliberate change to the digest.
    assert_eq!(
        d.to_string(),
        "3922a9bf780ad31fbe9a4b37814a40fc",
        "{path}: chart 0 of {} ({} notes, {})",
        song.charts.len(),
        song.charts[0].notes.len(),
        song.charts[0].layout,
    );
    // A dance-single chart, whose layout is the game's own.
    let i = song
        .charts
        .iter()
        .position(|c| c.layout == "dance-single")
        .expect("a dance-single chart");
    assert_eq!(
        song.chart_digest(i).to_string(),
        "89ed02e973ec6973e20ceacfe551f3f5",
        "{path}: chart {i}"
    );
}

#[test]
fn bundled_digests_are_stable_distinct_and_blind_to_metadata() {
    let mut seen: HashMap<ChartDigest, String> = HashMap::new();
    for (path, text, ext) in bundled() {
        let song = parse(&text, &ext);
        let ds = digests(&song);
        // Two parses agree, and a copy with other titles, art, credits,
        // meters and line endings.
        assert_eq!(ds, digests(&parse(&text, &ext)), "{path}");
        let other = parse(&other_metadata(&text), &ext);
        assert_ne!(other.title, song.title, "{path}: the metadata changed");
        assert_eq!(ds, digests(&other), "{path}");
        for (i, d) in ds.iter().enumerate() {
            let label = format!("{path} chart {i} ({})", song.charts[i].layout);
            if let Some(before) = seen.insert(*d, label.clone()) {
                panic!("{label} has the digest of {before}");
            }
        }
    }
    assert!(seen.len() >= 8, "{} charts", seen.len());
}

#[test]
fn notes_and_timing_change_the_digest() {
    let (_, text, ext) = bundled().into_iter().next().expect("a bundled song");
    let song = parse(&text, &ext);
    let base = song.chart_digest(0);
    let changed = |f: &dyn Fn(&mut Song)| {
        let mut s = song.clone();
        f(&mut s);
        s.chart_digest(0)
    };
    // A note moved a tick, to another lane, made a mine, a hold longer.
    assert_ne!(changed(&|s| s.charts[0].notes[3].tick.0 += 1), base);
    assert_ne!(
        changed(&|s| {
            let n = &mut s.charts[0].notes[3];
            n.lane = (n.lane + 1) % 4;
        }),
        base
    );
    assert_ne!(
        changed(&|s| s.charts[0].notes[3].kind = NoteKind::Mine),
        base
    );
    assert_ne!(
        changed(&|s| {
            s.charts[0].notes.pop();
        }),
        base
    );
    let hold = song.charts[0]
        .notes
        .iter()
        .position(|n| matches!(n.kind, NoteKind::HoldHead { .. }));
    if let Some(h) = hold {
        assert_ne!(
            changed(&|s| {
                if let NoteKind::HoldHead { end } = &mut s.charts[0].notes[h].kind {
                    *end = Tick(end.0 + 12);
                }
            }),
            base
        );
    }
    // Timing: a BPM, the offset, a stop, a split timing of the chart's own.
    assert_ne!(
        changed(&|s| {
            let t = s.charts[0].timing.as_mut().unwrap_or(&mut s.timing);
            t.bpms[0].bpm += 0.001;
        }),
        base
    );
    assert_ne!(
        changed(&|s| {
            let t = s.charts[0].timing.as_mut().unwrap_or(&mut s.timing);
            t.offset_seconds += 0.001;
        }),
        base
    );
    assert_ne!(
        changed(&|s| {
            let t = s.charts[0].timing.as_mut().unwrap_or(&mut s.timing);
            t.stops.push(ddi_chart::StopSegment {
                tick: Tick::from_beats(8),
                seconds: 0.5,
            });
        }),
        base
    );
    assert_ne!(
        changed(&|s| s.charts[0].layout = "dance-double".into()),
        base
    );
    // A layout the song defines: what turns read of it counts.
    let own = |f: &dyn Fn(&mut ddi_chart::Layout)| {
        changed(&|s| {
            let mut l = ddi_chart::Layout::builtin("dance-single").expect("a built-in layout");
            l.id = "own".into();
            f(&mut l);
            s.layouts.push(l);
            s.charts[0].layout = "own".into();
        })
    };
    let plain = own(&|_| {});
    assert_ne!(own(&|l| l.lanes[0].shuffle_group = 9), plain);
    assert_ne!(own(&|l| l.turn_left = None), plain);
    assert_eq!(own(&|l| l.lanes[0].label = "Other".into()), plain);
    // Not: the name, the meter, a note's color or keysound, the art.
    assert_eq!(
        changed(&|s| {
            s.charts[0].name = "x".into();
            s.charts[0].meter += 1;
            s.charts[0].notes[0].keysound = Some(1);
            s.banner = None;
            s.title = "y".into();
        }),
        base
    );
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("sm") || e.eq_ignore_ascii_case("ssc"))
        {
            out.push(path);
        }
    }
}

/// Every `.sm`/`.ssc` under `DDI_DIFF_CORPUS`: the same digests from two
/// parses and from a copy with other metadata; how many charts share a
/// digest (the same chart in two packs, or charts with no notes).
#[test]
fn local_corpus_digests() {
    let Some(dir) = std::env::var_os("DDI_DIFF_CORPUS") else {
        return;
    };
    let mut files = Vec::new();
    collect(Path::new(&dir), &mut files);
    files.sort();
    let mut by_digest: HashMap<ChartDigest, Vec<String>> = HashMap::new();
    let mut failures = Vec::new();
    let mut charts = 0;
    for path in &files {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes).to_string();
        let ext = path
            .extension()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        let Ok(song) = parse_simfile(&text, &ext) else {
            continue;
        };
        let ds = digests(&song);
        if Ok(ds.clone()) != parse_simfile(&text, &ext).map(|s| digests(&s)) {
            failures.push(format!("{}: two parses differ", path.display()));
        }
        if Ok(ds.clone()) != parse_simfile(&other_metadata(&text), &ext).map(|s| digests(&s)) {
            failures.push(format!("{}: metadata changed the digest", path.display()));
        }
        for (i, d) in ds.into_iter().enumerate() {
            charts += 1;
            by_digest
                .entry(d)
                .or_default()
                .push(format!("{} #{i}", path.display()));
        }
    }
    let shared: Vec<_> = by_digest.values().filter(|v| v.len() > 1).collect();
    eprintln!(
        "{} files, {charts} charts, {} digests; {} shared by several charts",
        files.len(),
        by_digest.len(),
        shared.len()
    );
    for v in shared.iter().take(10) {
        eprintln!("  {}", v.join(" = "));
    }
    for f in &failures {
        eprintln!("{f}");
    }
    assert!(failures.is_empty(), "{} files fell short", failures.len());
}
