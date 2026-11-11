use super::*;
use crate::announcer::find_announcers;
use crate::announcer::tidy;

const MP3: &[u8] = b"ID3\x04\x00\x00\x00\x00\x00\x00rest";
const M4A: &[u8] = b"\x00\x00\x00\x20ftypM4A \x00\x00\x00\x00";

fn ogg(kind: &[u8]) -> Vec<u8> {
    let mut head = b"OggS".to_vec();
    head.resize(28, 0);
    head.extend_from_slice(kind);
    head
}

#[test]
fn recordings_are_known_by_their_first_bytes() {
    let mut wav = b"RIFF\x24\x00\x00\x00WAVEfmt ".to_vec();
    wav.resize(64, 0);
    assert_eq!(Format::sniff(&wav), Format::Wav);
    assert_eq!(Format::sniff(MP3), Format::Mp3);
    assert_eq!(Format::sniff(&[0xFF, 0xFB, 0x90, 0x64]), Format::Mp3);
    // ADTS AAC shares the sync, not the layer.
    assert_eq!(Format::sniff(&[0xFF, 0xF1, 0x50, 0x80]), Format::Other);
    assert_eq!(Format::sniff(&ogg(b"OpusHead")), Format::Opus);
    assert_eq!(Format::sniff(&ogg(b"\x01vorbis")), Format::Vorbis);
    assert_eq!(Format::sniff(b"fLaC\x00\x00"), Format::Flac);
    assert_eq!(Format::sniff(M4A), Format::Other);
    assert_eq!(Format::sniff(b""), Format::Other);
    assert!(Format::Opus.kept(false) && !Format::Opus.kept(true));
    assert!(Format::Vorbis.kept(true) && !Format::Other.kept(false));
}

#[test]
fn files_are_put_on_the_lines_their_names_say() {
    let cases = [
        ("New Recording 12.m4a", None),
        ("Voice Memo.m4a", None),
        ("gameplay ready.wav", Some(Line::Ready)),
        ("Gameplay_Ready_2.m4a", Some(Line::Ready)),
        ("ready take 3.wav", Some(Line::Ready)),
        ("ready-take3.wav", Some(Line::Ready)),
        ("here we go normal (2).mp3", Some(Line::HereWeGo)),
        ("100 combo.wav", Some(Line::Combo(100))),
        ("1500 combo.wav", Some(Line::Combo(1500))),
        ("gameplay combo 300.ogg", Some(Line::Combo(300))),
        ("cheer.wav", Some(Line::Cheer)),
        ("evaluation aa 2.wav", Some(Line::Grade(Grade::Aa))),
        // Too short to be told apart, or two lines' ends.
        ("aa.wav", None),
        ("full combo w1.wav", None),
        // A pack added whole: its line folders.
        ("Mine/gameplay cleared/yay.mp3", Some(Line::Cleared)),
        (
            "Announcers/X/Gameplay Combo 200/a 1.wav",
            Some(Line::Combo(200)),
        ),
        ("gameplay failed/ready.wav", Some(Line::Failed)),
        // A folder named for its line holding takes named anyhow.
        ("takes/100 combo/1.wav", Some(Line::Combo(100))),
        ("Recordings/12.wav", None),
    ];
    for (path, line) in cases {
        assert_eq!(match_line(path), line, "{path}");
    }
}

fn measure(seconds: f64, start: f64, end: f64) -> Measure {
    Measure {
        rate: 48_000,
        channels: 1,
        seconds,
        clipped: false,
        start,
        end,
        gain: 1.0,
    }
}

#[test]
fn takes_are_sorted_moved_and_removed() {
    let mut d = Draft::default();
    let a = d.add("gameplay ready.wav", 10, MP3);
    let b = d.add("New Recording.m4a", 20, M4A);
    let c = d.add("gameplay 1100 combo/x.mp3", 30, MP3);
    assert_eq!(
        d.takes_of(Line::Ready).map(|t| t.id).collect::<Vec<_>>(),
        [a]
    );
    assert_eq!(d.unsorted().map(|t| t.id).collect::<Vec<_>>(), [b]);
    d.take_mut(b).unwrap().line = Some(Line::Cheer);
    assert_eq!(d.unsorted().count(), 0);
    assert_eq!(d.remove(a).map(|t| t.id), Some(a));
    assert!(d.take(a).is_none());
    // A combo past 1000 is listed once a take came on it, for good;
    // Etterna's two only while one is.
    assert!(d.combos.contains(&1100));
    let lines = d.lines();
    assert!(lines.contains(&Line::Combo(1100)));
    assert!(!lines.contains(&Line::Grade(Grade::Aaaaa)));
    assert!(!lines.contains(&Line::Grade(Grade::F)));
    let at = |l| lines.iter().position(|x| *x == l).unwrap();
    assert!(at(Line::Combo(1000)) < at(Line::Combo(1100)));
    assert!(at(Line::Combo(1100)) < at(Line::ComboOverflow));
    assert_ne!(c, b);
    // Ids are not given twice, even after a removal.
    assert_eq!(d.add("y.wav", 1, MP3), c + 1);
}

#[test]
fn the_combo_lines_are_the_authors_to_add_and_remove() {
    let mut d = Draft::default();
    let combos = |d: &Draft| -> Vec<u32> {
        d.lines()
            .into_iter()
            .filter_map(|l| match l {
                Line::Combo(n) => Some(n),
                _ => None,
            })
            .collect()
    };
    // Our packs' to begin with.
    assert_eq!(combos(&d), OUR_COMBOS);
    // A pack counting by powers of two.
    for n in [1024, 64, 128, 256, 512] {
        assert!(d.add_combo(n));
    }
    assert!(!d.add_combo(64), "listed already");
    assert!(!d.add_combo(0));
    for n in OUR_COMBOS {
        assert!(d.remove_combo(n));
    }
    assert!(!d.remove_combo(100), "not listed");
    assert_eq!(combos(&d), [64, 128, 256, 512, 1024]);
    // In the game's order, before More combo.
    let lines = d.lines();
    let at = |l| lines.iter().position(|x| *x == l).unwrap();
    assert!(at(Line::Combo(1024)) < at(Line::ComboOverflow));
    // A recording named for a count lists its line; a line with a
    // recording on it stays.
    let t = d.add("gameplay 32 combo/a.wav", 1, MP3);
    assert_eq!(d.take(t).unwrap().line, Some(Line::Combo(32)));
    assert_eq!(combos(&d), [32, 64, 128, 256, 512, 1024]);
    assert!(!d.remove_combo(32));
    d.remove(t);
    assert!(d.remove_combo(32));
    // Kept with the draft; a draft from before lists ours.
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(serde_json::from_str::<Draft>(&json).unwrap(), d);
    let mut old: serde_json::Value = serde_json::from_str(&json).unwrap();
    old.as_object_mut().unwrap().remove("combos");
    let old: Draft = serde_json::from_value(old).unwrap();
    assert_eq!(old.combos, OUR_COMBOS);
}

#[test]
fn a_take_is_kept_as_it_came_unless_tidied_or_unread_by_the_game() {
    let mut d = Draft::default();
    let mp3 = d.add("ready.mp3", 1, MP3);
    let m4a = d.add("cheer.m4a", 1, M4A);
    let opus = d.add("cleared.opus", 1, &ogg(b"OpusHead"));
    let write = |d: &Draft, id| d.write_of(d.take(id).unwrap());
    assert_eq!(write(&d, mp3), Write::Opus { tidy: true });
    d.tidy = false;
    assert_eq!(write(&d, mp3), Write::AsIs);
    assert_eq!(write(&d, m4a), Write::Opus { tidy: false });
    assert_eq!(write(&d, opus), Write::AsIs);
    d.stepmania = true;
    assert_eq!(write(&d, opus), Write::Wav { tidy: false });
    d.take_mut(mp3).unwrap().tidy = Some(true);
    assert_eq!(write(&d, mp3), Write::Wav { tidy: true });
}

#[test]
fn a_compressed_take_tidied_before_is_not_encoded_again() {
    let mut d = Draft::default();
    let opus = d.add("ready.opus", 1, &ogg(b"OpusHead"));
    let mut wav = b"RIFF\x24\x00\x00\x00WAVEfmt ".to_vec();
    wav.resize(64, 0);
    let wave = d.add("cheer.wav", 1, &wav);
    let with = |d: &mut Draft, id, m| {
        d.take_mut(id).unwrap().sound = Sound::Read(m);
        d.write_of(d.take(id).unwrap())
    };
    let tidied = measure(1.0, 0.01, 0.97);
    let opus_as = Write::Opus { tidy: true };
    assert_eq!(with(&mut d, opus, tidied), Write::AsIs);
    // A WAV is ten times smaller encoded, and loses next to nothing.
    assert_eq!(with(&mut d, wave, tidied), opus_as);
    // Something for tidying to do.
    assert_eq!(with(&mut d, opus, measure(1.0, 0.1, 0.97)), opus_as);
    assert_eq!(with(&mut d, opus, measure(1.0, 0.01, 0.8)), opus_as);
    let louder = Measure {
        gain: 1.2,
        ..tidied
    };
    assert_eq!(with(&mut d, opus, louder), opus_as);
    // StepMania reads no Opus, however tidy.
    d.stepmania = true;
    assert_eq!(with(&mut d, opus, tidied), Write::Wav { tidy: true });
}

#[test]
fn a_take_of_more_channels_than_opus_here_stays_a_wav() {
    let mut d = Draft::default();
    let id = d.add("ready.mp3", 1, MP3);
    let with = |d: &mut Draft, channels| {
        d.take_mut(id).unwrap().sound = Sound::Read(Measure {
            channels,
            ..measure(2.0, 0.5, 1.5)
        });
        d.write_of(d.take(id).unwrap())
    };
    assert_eq!(with(&mut d, 1), Write::Opus { tidy: true });
    assert_eq!(with(&mut d, 2), Write::Opus { tidy: true });
    assert_eq!(with(&mut d, 6), Write::Wav { tidy: true });
    assert!(d.export().entries[0].path.ends_with(".wav"));
}

#[test]
fn notes_tell_of_long_clipped_and_unreadable_takes() {
    let mut d = Draft::default();
    let combo = d.add("100 combo.wav", 1, MP3);
    let comment = d.add("comment good.wav", 1, MP3);
    let bad = d.add("cheer.wav", 1, MP3);
    let clipped = d.add("cleared.wav", 1, MP3);
    // 3 s as recorded, 1.5 s once tidied.
    d.take_mut(combo).unwrap().sound = Sound::Read(measure(3.0, 1.0, 2.5));
    d.take_mut(comment).unwrap().sound = Sound::Read(measure(9.0, 0.0, 9.0));
    d.take_mut(bad).unwrap().sound = Sound::Unreadable("no decoder".into());
    d.take_mut(clipped).unwrap().sound = Sound::Read(Measure {
        clipped: true,
        ..measure(1.0, 0.0, 1.0)
    });
    let notes = |d: &Draft, id| d.notes(d.take(id).unwrap());
    assert!(notes(&d, combo).is_empty());
    assert!(notes(&d, comment).is_empty());
    assert_eq!(notes(&d, bad), [Note::Unreadable("no decoder".into())]);
    assert_eq!(notes(&d, clipped), [Note::Clipped]);
    d.take_mut(combo).unwrap().tidy = Some(false);
    assert_eq!(
        notes(&d, combo),
        [Note::TooLong {
            seconds: 3.0,
            limit: COMBO_GAP
        }]
    );
    // Ready and here we go: the longest of each, and the half second
    // before the first note.
    assert_eq!(d.opening(), None);
    for (name, s) in [
        ("ready.wav", 1.0),
        ("ready 2.wav", 1.5),
        ("here we go normal.wav", 1.25),
    ] {
        let id = d.add(name, 1, MP3);
        d.take_mut(id).unwrap().sound = Sound::Read(measure(s, 0.0, s));
    }
    assert_eq!(d.opening(), Some(1.5 + 1.25 + GO_BEFORE));
}

/// A second of a tone, as the shells hand the studio decoded takes.
fn tone() -> Vec<f32> {
    (0..48_000)
        .map(|i| {
            let t = f64::from(i) / 48_000.0;
            let on = (0.2..0.7).contains(&t);
            if on {
                (0.2 * (t * 440.0 * std::f64::consts::TAU).sin()) as f32
            } else {
                0.0
            }
        })
        .collect()
}

/// The zip an export writes, the shell's part done here: each take's
/// bytes as they came, its tone tidied and written as a WAV, or a stand-in
/// for what an encoder makes of it.
fn zip_of(d: &Draft, bytes: &BTreeMap<u64, Vec<u8>>) -> Vec<u8> {
    let export = d.export();
    let mut files: Vec<(String, Vec<u8>)> = export
        .entries
        .iter()
        .map(|e| {
            let data = match e.write {
                Write::AsIs => bytes[&e.take].clone(),
                Write::Opus { .. } => ddi_platform::ogg_opus::write(
                    ddi_platform::ogg_opus::Stream {
                        channels: 1,
                        pre_skip: 0,
                        frames: 960,
                    },
                    &[[31 << 3, 0]],
                )
                .unwrap(),
                Write::Wav { tidy } => {
                    let t = tone();
                    let out = if tidy {
                        tidy::apply(&[&t], 48_000, &tidy::measure(&[&t], 48_000))
                    } else {
                        vec![t]
                    };
                    crate::wav::write(&out, 48_000)
                }
            };
            (e.path.clone(), data)
        })
        .collect();
    files.push(("README.txt".into(), export.readme.into_bytes()));
    crate::zip::write_stored(&files).unwrap()
}

/// What an export writes, read back as an import reads it: the same
/// lines and sounds, nothing passed over, the same bytes each time.
#[test]
fn an_export_imports_as_it_was_made() {
    let mut d = Draft {
        name: "ガキン/さん: 2".into(),
        author: "Someone".into(),
        license: "CC0".into(),
        tidy: false,
        ..Draft::default()
    };
    let mut bytes = BTreeMap::new();
    for (source, head) in [
        ("gameplay ready.mp3", MP3.to_vec()),
        ("Gameplay_Ready_2.mp3", MP3.to_vec()),
        ("ready.mp3", MP3.to_vec()),
        ("100 combo.opus", ogg(b"OpusHead")),
        ("cheer.m4a", M4A.to_vec()),
        ("New Recording.m4a", M4A.to_vec()),
        ("cleared.wav", MP3.to_vec()),
    ] {
        let id = d.add(source, head.len() as u64, &head);
        bytes.insert(id, head);
    }
    let cleared = d.takes_of(Line::Cleared).next().unwrap().id;
    d.take_mut(cleared).unwrap().sound = Sound::Unreadable("no".into());
    let export = d.export();
    assert_eq!(export.file_name, "ガキン さん 2.zip");
    let paths: Vec<&str> = export.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "Announcers/ガキン さん 2/gameplay ready/gameplay ready.mp3",
            "Announcers/ガキン さん 2/gameplay ready/Gameplay_Ready_2.mp3",
            "Announcers/ガキン さん 2/gameplay ready/ready.mp3",
            "Announcers/ガキン さん 2/gameplay 100 combo/100 combo.opus",
            "Announcers/ガキン さん 2/evaluation cheer/cheer.opus",
        ]
    );
    assert_eq!(
        export.readme,
        "ガキン さん 2\nBy Someone\nLicense: CC0\n\nAn announcer for Dito Dito Infinito, made with its announcer studio.\n"
    );
    // Imported again, the README gives the credits back.
    assert_eq!(
        readme_credits(&export.readme),
        (Some("Someone".into()), Some("CC0".into()))
    );
    assert_eq!(readme_credits("Name\n\nBy nobody\n"), (None, None));
    let zip = zip_of(&d, &bytes);
    assert_eq!(zip_of(&d, &bytes), zip);
    let read = crate::zip::tests::read_all(&zip).unwrap();
    let files: Vec<(String, u64)> = read
        .iter()
        .map(|(e, data)| (e.name.clone(), data.len() as u64))
        .collect();
    let found = find_announcers(&files, "root");
    assert_eq!(found.announcers.len(), 1);
    let a = &found.announcers[0];
    assert_eq!(a.name, "ガキン さん 2");
    let lines: BTreeMap<Line, usize> = a.lines.iter().map(|(l, f)| (*l, f.len())).collect();
    assert_eq!(
        lines,
        BTreeMap::from([(Line::Ready, 3), (Line::Combo(100), 1), (Line::Cheer, 1)])
    );
    assert_eq!(a.report.ignored, 0);
    assert!(a.report.empty.is_empty() && a.unknown.is_empty());
    // Tidied, every take is Opus, and a WAV for StepMania.
    d.tidy = true;
    assert!(d.export().entries.iter().all(|e| e.path.ends_with(".opus")));
    d.stepmania = true;
    assert!(d.export().entries.iter().all(|e| e.path.ends_with(".wav")));
    d.stepmania = false;
    // Unnamed: a folder all the same.
    assert_eq!(Draft::default().export().file_name, "Announcer.zip");
}

#[test]
fn a_takes_name_is_numbered_where_two_would_share_it() {
    let mut d = Draft {
        tidy: false,
        ..Draft::default()
    };
    for source in ["a/ready.mp3", "b/Ready.mp3", "c/ready.mp3"] {
        let id = d.add(source, 1, MP3);
        d.take_mut(id).unwrap().line = Some(Line::Ready);
    }
    let names: Vec<String> = d
        .export()
        .entries
        .iter()
        .map(|e| e.path.rsplit('/').next().unwrap().to_string())
        .collect();
    assert_eq!(names, ["ready.mp3", "Ready 2.mp3", "ready 3.mp3"]);
}

#[test]
fn a_draft_is_kept_as_json() {
    let mut d = Draft::default();
    let id = d.add("100 combo.wav", 5, MP3);
    d.take_mut(id).unwrap().sound = Sound::Read(measure(1.0, 0.1, 0.9));
    d.add("x.m4a", 5, M4A);
    let json = serde_json::to_string(&d).unwrap();
    assert!(json.contains("\"gameplay 100 combo\""), "{json}");
    let back: Draft = serde_json::from_str(&json).unwrap();
    assert_eq!(back, d);
    // The next id survives too.
    let mut back = back;
    assert_eq!(back.add("y.wav", 1, MP3), 3);
}

#[test]
fn the_size_counts_what_is_written_anew_by_its_length() {
    let mut d = Draft::default();
    let id = d.add("ready.mp3", 1000, MP3);
    d.take_mut(id).unwrap().sound = Sound::Read(measure(2.0, 0.5, 1.5));
    // Tidied: a second of Opus…
    assert_eq!(d.size(), (1, ddi_platform::ogg_opus::len(1.0, 1)));
    // …or, for StepMania, a second at 48 kHz, mono, 16-bit.
    d.stepmania = true;
    assert_eq!(d.size(), (1, 44 + 96_000));
    d.stepmania = false;
    d.tidy = false;
    assert_eq!(d.size(), (1, 1000));
    assert!(!d.too_big());
}

/// The files of a pack `Name` in a folder of its own.
fn pack_files(folder: &str) -> Vec<(String, u64)> {
    let mut files: Vec<(String, u64)> = ["gameplay ready", "evaluation aaa", "evaluation cheer"]
        .iter()
        .map(|line| (format!("{folder}{line}/a.ogg"), 1))
        .collect();
    files.push((format!("{folder}README.txt"), 1));
    files
}

#[test]
fn a_pack_made_before_gives_what_the_draft_has_not() {
    let (name, readme) = made_before(&pack_files("Name/")).expect("a pack");
    assert_eq!(name, "Name");
    assert_eq!(readme.as_deref(), Some("Name/README.txt"));
    assert_eq!(made_before(&[("a.ogg".to_string(), 1)]), None);

    let mut draft = Draft::default();
    draft.adopt(Adopted {
        name: "Name".into(),
        by: Some("Someone".into()),
        license: None,
    });
    assert_eq!(
        (draft.name.as_str(), draft.author.as_str()),
        ("Name", "Someone")
    );
    // What the draft says stays.
    draft.adopt(Adopted {
        name: "Other".into(),
        by: Some("Another".into()),
        license: Some("CC0".into()),
    });
    assert_eq!(
        (
            draft.name.as_str(),
            draft.author.as_str(),
            draft.license.as_str()
        ),
        ("Name", "Someone", "CC0")
    );
}

#[test]
fn a_finished_packs_files_install_under_its_own_folder() {
    let plan = install_plan(&pack_files("Name/")).expect("a plan");
    assert_eq!(plan.kept.name, "Name");
    assert_eq!(plan.folder, "Name");
    assert_eq!(
        plan.inside("Name/gameplay ready/a.ogg"),
        "gameplay ready/a.ogg"
    );
    assert_eq!(plan.inside("elsewhere.ogg"), "elsewhere.ogg");
    assert_eq!(
        install_plan(&[("a.ogg".to_string(), 1)]).err(),
        Some("it has no lines yet")
    );
}

#[test]
fn a_text_line_is_recorded_and_its_words_kept() {
    let mut d = Draft::default();
    let line = Line::Combo(64);
    assert!(d.add_combo(64));
    let s = d.write(line);
    *d.words_mut(s).unwrap() = "Sixty-four!".into();
    // Listed, and the combo line stays while it is there.
    assert!(!d.remove_combo(64));
    assert_eq!(d.scripts_of(line).count(), 1);
    // A recording for it takes its place, on its line with its words.
    let t = d.add_to(s, "take.mp3", 5, MP3);
    assert_ne!(t, s);
    assert_eq!(d.scripts_of(line).count(), 0);
    let take = d.take(t).unwrap();
    assert_eq!(take.line, Some(line));
    assert_eq!(take.words.as_deref(), Some("Sixty-four!"));
    // Its words are still the author's to change.
    *d.words_mut(t).unwrap() = "Sixty-four combo!".into();
    // Kept as JSON.
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(serde_json::from_str::<Draft>(&json).unwrap(), d);
    // Removed, it leaves its words as a text line again.
    d.remove(t);
    let back: Vec<&Script> = d.scripts_of(line).collect();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].words, "Sixty-four combo!");
    assert!(d.take(back[0].id).is_none());
    // A recording without words leaves nothing; a text line gone, the
    // combo line goes too.
    let plain = d.add("other.mp3", 5, MP3);
    d.take_mut(plain).unwrap().line = Some(Line::Ready);
    d.remove(plain);
    assert_eq!(d.scripts.len(), 1);
    assert!(d.unwrite(back_id(&d)).is_some());
    assert!(d.remove_combo(64));
    // A take added for a text line already gone is added as any other.
    let gone = d.add_to(999, "gameplay ready.wav", 5, MP3);
    assert_eq!(d.take(gone).unwrap().line, Some(Line::Ready));
    assert_eq!(d.take(gone).unwrap().words, None);
}

fn back_id(d: &Draft) -> u64 {
    d.scripts[0].id
}
