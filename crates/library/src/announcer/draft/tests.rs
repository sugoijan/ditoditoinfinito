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
    for (path, text) in &export.texts {
        files.push((path.clone(), text.clone().into_bytes()));
    }
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
        ..Adopted::default()
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
        ..Adopted::default()
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

/// Review studio-editor-4: an unreadable draft's name aside is never one
/// taken, and a draft begun anew gives no id a stored recording has.
#[test]
fn an_unreadable_draft_s_name_and_ids_reuse_nothing() {
    assert_eq!(aside_name(|_| false), "draft.unreadable.json");
    assert_eq!(
        aside_name(|n| n == "draft.unreadable.json"),
        "draft.unreadable.2.json"
    );
    assert_eq!(
        aside_name(|n| n == "draft.unreadable.json" || n == "draft.unreadable.2.json"),
        "draft.unreadable.3.json"
    );
    let mut d = Draft::default();
    d.start_past(41);
    assert_eq!(d.add("a.wav", 1, b"RIFF"), 42);
    d.start_past(3);
    assert_eq!(d.add("b.wav", 1, b"RIFF"), 43, "never back");
}

/// Review studio-editor-3: a recording past what a pack may hold is not
/// added, and the decoded sound kept is bounded, the oldest let go.
#[test]
fn recordings_and_their_decoded_sound_are_bounded() {
    let mut d = Draft::default();
    assert!(d.has_room(TAKE_MAX_BYTES));
    assert!(!d.has_room(TAKE_MAX_BYTES + 1), "a song, not a recording");
    for i in 0..DRAFT_MAX_BYTES / TAKE_MAX_BYTES {
        assert!(d.has_room(TAKE_MAX_BYTES));
        d.add(&format!("{i}.wav"), TAKE_MAX_BYTES, b"RIFF");
    }
    assert!(!d.has_room(1), "the draft's recordings in all");
    let mut d = Draft::default();
    for i in 0..MAX_FILES {
        d.add(&format!("{i}.wav"), 1, b"RIFF");
    }
    assert!(!d.has_room(1), "a pack's files at most");

    let mut kept: Decoded<&str> = Decoded::with_cap(100);
    kept.insert(1, "one", 40);
    kept.insert(2, "two", 40);
    kept.insert(3, "three", 40);
    assert!(!kept.contains(1), "the oldest let go");
    assert_eq!((kept.len(), kept.bytes()), (2, 80));
    // Put in again: the newest, its bytes counted once.
    kept.insert(2, "two again", 50);
    assert_eq!(kept.bytes(), 90);
    kept.insert(4, "four", 40);
    assert!(!kept.contains(3) && kept.contains(2) && kept.contains(4));
    // One larger than the cap is still kept, alone.
    kept.insert(5, "huge", 1000);
    assert_eq!(kept.sounds().keys().copied().collect::<Vec<_>>(), vec![5]);
    assert_eq!(kept.remove(5), Some("huge"));
    assert!(kept.is_empty() && kept.bytes() == 0);
}

/// Review studio-editor-10: an announcer named as a Windows device, or
/// with a dot or a space at its end, still gets a folder Windows makes.
#[test]
fn a_pack_s_folder_is_a_name_every_system_takes() {
    assert_eq!(file_safe("Con"), "_Con");
    assert_eq!(file_safe("aux"), "_aux");
    assert_eq!(file_safe("Voice . "), "Voice");
    assert_eq!(file_safe("Mr. Clank"), "Mr. Clank");
    let d = Draft {
        name: "NUL".into(),
        ..Draft::default()
    };
    assert_eq!(d.folder_name(), "_NUL");
}

#[test]
fn stadium_is_optional_persisted_and_forces_processing_of_settled_takes() {
    let mut d = Draft::default();
    let id = d.add("gameplay ready.mp3", 64, MP3);
    d.take_mut(id).unwrap().sound = Sound::Read(measure(1.0, 0.0, 1.0));
    assert!(!d.stadium);
    assert_eq!(d.write_of(d.take(id).unwrap()), Write::AsIs);
    let mut legacy = serde_json::to_value(&d).unwrap();
    legacy.as_object_mut().unwrap().remove("stadium");
    assert!(!serde_json::from_value::<Draft>(legacy).unwrap().stadium);
    d.stadium = true;
    assert!(matches!(
        d.write_of(d.take(id).unwrap()),
        Write::Opus { .. }
    ));
    assert_eq!(d.seconds(d.take(id).unwrap()), Some(1.0));
    let opening = d.processing(d.take(id).unwrap());
    d.take_mut(id).unwrap().line = Some(Line::Combo(100));
    let normal = d.processing(d.take(id).unwrap());
    assert_ne!(opening, normal);
    assert_eq!(d.seconds(d.take(id).unwrap()), Some(1.6));
    let saved = serde_json::to_string(&d).unwrap();
    assert!(serde_json::from_str::<Draft>(&saved).unwrap().stadium);
    d.tidy = false;
    assert!(matches!(
        d.write_of(d.take(id).unwrap()),
        Write::Opus { tidy: false }
    ));
    let input = vec![vec![0.0; 4800]];
    let off = Processing::default().apply(&input, 48000);
    assert_eq!(off, (input.clone(), 48000));
    let (processed, rate) = Processing {
        stadium: true,
        ..Processing::default()
    }
    .apply(&input, 48000);
    assert_eq!(rate, 48000);
    assert_eq!(processed[0].len(), 33600);
    assert!(processed[0].iter().all(|s| *s == 0.0));
    let mut cache = Processed::<Vec<Vec<f32>>>::default();
    cache.insert(id, opening, input.clone(), input[0].len() * 4);
    assert_eq!(cache.get(id, opening), Some(input));
    assert!(cache.get(id, normal).is_none());
}

/// FNV-1a over `bytes`: a pin of a file's bytes.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// A draft as the studio stored it before it had languages, tags or
/// takes on several lines (2026-11-18, written by that studio).
const OLD_DRAFT: &str = include_str!("old-draft.json");

/// A plain draft (one language, no tags, no take on two lines), stored
/// before this round, opens as it was and exports byte for byte what the
/// studio exported then: its line folders at the pack's top, no
/// `announcer.ini`, no script.
#[test]
fn a_plain_draft_stored_before_exports_as_it_did() {
    let d: Draft = serde_json::from_str(OLD_DRAFT).expect("an old draft");
    assert_eq!(d.name, "Old Voice");
    assert_eq!(d.takes.len(), 5);
    assert_eq!(d.scripts.len(), 1);
    let mut bytes = BTreeMap::new();
    for t in &d.takes {
        bytes.insert(t.id, format!("ID3 take {}", t.id).into_bytes());
    }
    let export = d.export();
    let paths: Vec<&str> = export.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "Announcers/Old Voice/gameplay ready/ready.mp3",
            "Announcers/Old Voice/gameplay ready/ready 2.mp3",
            "Announcers/Old Voice/gameplay 100 combo/hundred.mp3",
            "Announcers/Old Voice/evaluation cheer/cheer.wav",
        ]
    );
    assert!(export.texts.is_empty());
    let zip = zip_of(&d, &bytes);
    assert_eq!((zip.len(), fnv(&zip)), (OLD_ZIP_LEN, OLD_ZIP_FNV));
    // Kept again, it reads as it was: one language, not named.
    assert!(d.languages.is_empty() && d.working.is_none() && !d.mixed);
    assert!(d.takes.iter().all(|t| t.lang.is_none()));
    let again: Draft = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    assert_eq!(again, d);
    // Its one language named, it is still a plain pack.
    let mut named = d.clone();
    assert!(named.add_language("en"));
    assert!(!named.declares());
    assert_eq!(named.export(), export);
}

/// Two languages: each its own takes and text lines, the page working in
/// one; the pack written with a folder a language and an `announcer.ini`
/// the game's reader declares it by.
#[test]
fn a_draft_of_two_languages_writes_a_folder_a_language() {
    let mut d = Draft {
        name: "Two Voices".into(),
        tidy: false,
        ..Draft::default()
    };
    let ready = d.add("gameplay ready.mp3", 9, MP3);
    assert!(d.add_language("en"));
    assert!(!d.add_language("en") && !d.add_language("not a tag"));
    // Named, the first is worked in still; a second is worked in next.
    assert_eq!(d.working_language(), Some("en"));
    assert!(d.add_language("es"));
    assert_eq!(d.working_language(), Some("es"));
    assert_eq!(d.takes_of(Line::Ready).count(), 0);
    let listo = d.add("gameplay ready.mp3", 9, MP3);
    d.add("evaluation cheer.mp3", 9, MP3);
    assert_eq!(d.take(listo).unwrap().lang.as_deref(), Some("es"));
    assert_eq!(d.lang_of(d.take(ready).unwrap()), Some("en"));
    let es = d.write(Line::Cleared);
    assert_eq!(d.scripts_of(Line::Cleared).count(), 1);
    d.working = Some("en".into());
    assert_eq!(
        d.takes_of(Line::Ready).map(|t| t.id).collect::<Vec<_>>(),
        [ready]
    );
    assert_eq!(d.scripts_of(Line::Cleared).count(), 0);
    assert_eq!(d.languages_of(Line::Ready), ["en", "es"]);
    assert_eq!(d.languages_of(Line::Cheer), ["es"]);
    // A text line recorded keeps its language; removed, it goes back.
    let rec = d.add_to(es, "clear.mp3", 9, MP3);
    assert_eq!(d.take(rec).unwrap().lang.as_deref(), Some("es"));
    d.remove(rec);
    assert_eq!(d.scripts[0].lang.as_deref(), Some("es"));
    // Not removed while it speaks.
    assert!(!d.remove_language("es"));
    d.mixed = true;
    let export = d.export();
    let paths: Vec<&str> = export.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "Announcers/Two Voices/en/gameplay ready/gameplay ready.mp3",
            "Announcers/Two Voices/es/gameplay ready/gameplay ready.mp3",
            "Announcers/Two Voices/es/evaluation cheer/evaluation cheer.mp3",
        ]
    );
    assert_eq!(
        export.texts,
        [(
            "Announcers/Two Voices/announcer.ini".to_string(),
            "; announcer.ini, written by Dito Dito Infinito's announcer studio.\n\n\
             [Announcer]\nName=Two Voices\n\n[Parameters]\nList=Language\n\n\
             [Parameter Language]\nType=Language\nLanguages=en,es\n\
             Labels=English,Español\nMixed=1\n"
                .to_string()
        )]
    );
    // Read as the game reads a pack: one announcer, its two voices.
    let mut bytes = BTreeMap::new();
    for t in &d.takes {
        bytes.insert(t.id, MP3.to_vec());
    }
    let read = crate::zip::tests::read_all(&zip_of(&d, &bytes)).unwrap();
    let files: Vec<(String, u64)> = read
        .iter()
        .map(|(e, data)| (e.name.clone(), data.len() as u64))
        .collect();
    let found = find_announcers(&files, "root");
    assert_eq!(found.announcers.len(), 1);
    let mut kept = found.announcers[0].kept();
    kept.ini = export.text(crate::announcer::pool::DECLARATION).map(str::to_string);
    let offered = kept.offered(&Default::default(), "en");
    assert_eq!(
        offered.languages,
        [
            ("en".to_string(), Some("English".to_string())),
            ("es".to_string(), Some("Español".to_string()))
        ]
    );
    assert!(offered.mixed);
    assert_eq!(offered.name("en"), Some("Two Voices"));
    // Its first language, empty, can go; the other then is the first.
    d.takes.retain(|t| t.id != ready);
    assert!(d.remove_language("en"));
    assert_eq!(d.languages, ["es"]);
    assert!(!d.mixed && !d.declares());
}

/// The zip [`a_plain_draft_stored_before_exports_as_it_did`] pins, as
/// the studio made it before this round.
const OLD_ZIP_LEN: usize = 946;
const OLD_ZIP_FNV: u64 = 10_308_506_410_483_160_419;

/// Writes `old-draft.json` (run once, by the studio as it was).
#[test]
#[ignore = "writes the old draft's fixture"]
fn write_the_old_draft() {
    let mut d = Draft {
        name: "Old Voice".into(),
        author: "Someone".into(),
        license: "CC0".into(),
        tidy: false,
        ..Draft::default()
    };
    for (source, line, read) in [
        ("ready.mp3", Some(Line::Ready), true),
        ("other/ready.mp3", Some(Line::Ready), true),
        ("hundred.mp3", Some(Line::Combo(100)), true),
        ("cheer.wav", Some(Line::Cheer), false),
        ("New Recording.mp3", None, true),
    ] {
        let id = d.add(source, 9, if source.ends_with("wav") { b"RIFF\0\0\0\0WAVE" } else { MP3 });
        let t = d.take_mut(id).unwrap();
        t.line = line;
        if read {
            t.sound = Sound::Read(measure(1.0, 0.1, 0.9));
        }
    }
    let s = d.write(Line::Cleared);
    *d.words_mut(s).unwrap() = "All clear!".into();
    d.add_combo(64);
    let json = serde_json::to_string_pretty(&d).unwrap();
    std::fs::write(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/announcer/draft/old-draft.json"),
        json + "\n",
    )
    .unwrap();
}

/// `d`'s export read back as the game reads a pack: its announcer, with
/// its `announcer.ini` and `announcer.lua` as text.
fn read_back(d: &Draft) -> crate::announcer::Kept {
    let export = d.export();
    let mut bytes = BTreeMap::new();
    for t in &d.takes {
        bytes.insert(t.id, MP3.to_vec());
    }
    let read = crate::zip::tests::read_all(&zip_of(d, &bytes)).unwrap();
    let files: Vec<(String, u64)> = read
        .iter()
        .map(|(e, data)| (e.name.clone(), data.len() as u64))
        .collect();
    let found = find_announcers(&files, "root");
    assert_eq!(found.announcers.len(), 1, "{files:?}");
    assert_eq!(found.announcers[0].report.ignored, 0, "{files:?}");
    let mut kept = found.announcers[0].kept();
    kept.ini = export.text(crate::announcer::pool::DECLARATION).map(str::to_string);
    kept.script = export.text(crate::announcer::script::SCRIPT).map(str::to_string);
    kept
}

/// Tags: words on takes, one made switchable with a label and a first
/// state; the pack written with a readable script the game's reader
/// runs, its switch a row, a line with every take tagged going silent
/// when switched off (what the page says beforehand).
#[test]
fn tags_are_written_in_a_script_the_game_runs() {
    use crate::announcer::pool::Tag;
    use ddi_noteskin::params::Value;
    let mut d = Draft {
        name: "Tagged \"One\"".into(),
        tidy: false,
        ..Draft::default()
    };
    let ready = d.add("gameplay ready.mp3", 9, MP3);
    let calm = d.add("gameplay here we go normal.mp3", 9, MP3);
    let loud = d.add("gameplay here we go normal 2.mp3", 9, MP3);
    let hundred = d.add("gameplay 100 combo.mp3", 9, MP3);
    assert_eq!(d.toggle_tag(calm, "calm"), Some(true));
    assert_eq!(d.toggle_tag(hundred, "rude"), Some(true));
    assert_eq!(d.toggle_tag(loud, "not a tag"), None);
    assert_eq!(d.toggle_tag(loud, "calm"), Some(true));
    assert_eq!(d.toggle_tag(loud, "calm"), Some(false));
    assert_eq!(d.tags(), ["calm", "rude"]);
    assert_eq!(d.tagged("calm"), 1);
    assert!(d.toggle_switch("rude"));
    assert!(d.toggle_default("rude"));
    d.switches[0].label = "Rude \\ words\n".into();
    assert!(d.declares() && d.scripted());
    // Switched off, the 100 combo has nothing left: More combo has none
    // either, so it says nothing; here we go keeps its other take.
    assert_eq!(
        d.silenced("rude"),
        [Silenced {
            line: Line::Combo(100),
            lang: None,
            instead: None
        }]
    );
    assert!(d.silenced("calm").is_empty());
    // Restart's only take tagged: Ready is said in its place.
    let restart = d.add("gameplay restart.mp3", 9, MP3);
    d.toggle_tag(restart, "rude");
    assert!(d.silenced("rude").contains(&Silenced {
        line: Line::Restart,
        lang: None,
        instead: Some(Line::Ready)
    }));
    d.remove(restart);
    let export = d.export();
    assert_eq!(
        export.text(crate::announcer::script::SCRIPT),
        Some(
            "-- announcer.lua, written by Dito Dito Infinito's announcer studio.\n\
             -- Run once as the pack is chosen, it says what each recording may be\n\
             -- said for (\"A script\" in the game's docs/announcer-lines.md): a\n\
             -- recording in a line's folder is said for that line, in its language\n\
             -- folder's language; those listed say more of themselves.\n\
             local pack = Announcer.pack()\n\n\
             local listed = {\n  \
             [\"gameplay 100 combo/gameplay 100 combo.mp3\"] = { tags = { \"rude\" } },\n  \
             [\"gameplay here we go normal/gameplay here we go normal.mp3\"] = { tags = { \"calm\" } },\n\
             }\n\n\
             for _, file in ipairs(Announcer.files()) do\n  \
             pack:take(file, listed[file])\nend\n\n\
             pack:tag(\"rude\", { label = \"Rude \\\\ words\", default = false })\n\
             return pack\n"
        )
    );
    assert_eq!(
        export.text(crate::announcer::pool::DECLARATION),
        Some(
            "; announcer.ini, written by Dito Dito Infinito's announcer studio.\n\n\
             [Announcer]\nName=Tagged \"One\"\n"
        )
    );
    // The same draft, the same bytes.
    assert_eq!(d.export(), export);
    let kept = read_back(&d);
    let (pool, refused) = kept.pool_with(&Default::default(), "en");
    assert_eq!(refused, None);
    assert_eq!(pool.takes.len(), 4);
    let tags_of = |file: &str| -> Vec<String> {
        pool.takes
            .iter()
            .find(|t| t.file.as_str().ends_with(file))
            .unwrap()
            .tags
            .clone()
    };
    assert_eq!(tags_of("/gameplay 100 combo.mp3"), ["rude"]);
    assert_eq!(tags_of("/gameplay here we go normal.mp3"), ["calm"]);
    assert!(tags_of("/gameplay ready.mp3").is_empty());
    assert_eq!(
        pool.tags,
        [Tag {
            name: "rude".into(),
            label: "Rude \\ words".into(),
            default: false
        }]
    );
    // Off at first: the 100 combo is not said; switched on, it is.
    let off = kept.resolve(&Default::default(), "en");
    assert!(!off.lines.contains_key("gameplay 100 combo"));
    let mut values = ddi_noteskin::params::Values::new();
    values.insert(crate::announcer::pool::tag_value_name("rude"), Value::Bool(true));
    assert!(kept.resolve(&values, "en").lines.contains_key("gameplay 100 combo"));
    assert_eq!(off.lines["gameplay ready"].len(), 1);
    let _ = ready;
}

/// A take said for several lines: one sound, listed under each line,
/// written once in `shared/` with a script naming its lines and its
/// language; read back, the game says it for each line.
#[test]
fn a_take_on_several_lines_is_written_once() {
    let mut d = Draft {
        name: "Shared".into(),
        tidy: false,
        ..Draft::default()
    };
    d.add_language("en");
    let awesome = d.add("awesome.mp3", 9, MP3);
    d.set_line(awesome, Some(Line::Combo(100)));
    assert!(!d.share(awesome, Line::Combo(100)));
    assert!(d.share(awesome, Line::Cheer));
    assert!(d.share(awesome, Line::Combo(64)));
    assert!(d.combos.contains(&64));
    let cheer = d.add("evaluation cheer.mp3", 9, MP3);
    d.add_language("ja");
    let ja = d.add("gameplay 100 combo.mp3", 9, MP3);
    d.working = Some("en".into());
    assert_eq!(
        d.takes_of(Line::Cheer).map(|t| t.id).collect::<Vec<_>>(),
        [awesome, cheer]
    );
    assert_eq!(d.languages_of(Line::Cheer), ["en"]);
    assert!(!d.remove_combo(64), "said for it");
    assert!(d.unshare(awesome, Line::Combo(64)) && !d.unshare(awesome, Line::Combo(64)));
    assert!(d.scripted() && d.declares());
    let export = d.export();
    let paths: Vec<&str> = export.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "Announcers/Shared/shared/awesome.mp3",
            "Announcers/Shared/en/evaluation cheer/evaluation cheer.mp3",
            "Announcers/Shared/ja/gameplay 100 combo/gameplay 100 combo.mp3",
        ]
    );
    let script = export.text(crate::announcer::script::SCRIPT).unwrap();
    assert!(
        script.contains(
            "  [\"shared/awesome.mp3\"] = { events = { \"gameplay 100 combo\", \"evaluation cheer\" }, lang = \"en\" },\n"
        ),
        "{script}"
    );
    assert!(script.ends_with("pack:languages({ \"en\", \"ja\" })\nreturn pack\n"), "{script}");
    let kept = read_back(&d);
    let (pool, refused) = kept.pool_with(&Default::default(), "en");
    assert_eq!(refused, None);
    let shared = pool
        .takes
        .iter()
        .find(|t| t.file.as_str() == "shared/awesome.mp3")
        .unwrap();
    assert_eq!(shared.events, [Line::Combo(100), Line::Cheer]);
    assert_eq!(shared.lang.as_deref(), Some("en"));
    let en = kept.resolve(&Default::default(), "en");
    assert_eq!(en.lines["gameplay 100 combo"], [crate::path::RelPathBuf::from("shared/awesome.mp3")]);
    assert_eq!(en.lines["evaluation cheer"].len(), 2);
    let ja_lines = kept.resolve(&Default::default(), "ja");
    assert!(!ja_lines.lines.contains_key("evaluation cheer"));
    let _ = ja;
    // Moved onto a line it is said for besides: that line once, the one
    // it left no more.
    d.set_line(awesome, Some(Line::Cheer));
    assert!(d.take(awesome).unwrap().also.is_empty());
    d.share(awesome, Line::Ready);
    // Unsorted, it is said for none of them.
    d.set_line(awesome, None);
    assert!(d.take(awesome).unwrap().also.is_empty());
}

/// Every export is read back through the game's own reader and compared
/// with the draft before it is offered: plain, in two languages, tagged
/// and shared alike pass; a pack the reader takes otherwise (a script it
/// refuses, a declaration that says other languages, a take missing) is
/// refused, as a mistake of the studio's.
#[test]
fn an_export_is_checked_by_the_games_reader() {
    let sizes = |e: &Export| -> Vec<(String, u64)> {
        e.entries.iter().map(|x| (x.path.clone(), 9)).collect()
    };
    let old: Draft = serde_json::from_str(OLD_DRAFT).unwrap();
    let mut d = Draft {
        name: "Checked".into(),
        tidy: false,
        ..Draft::default()
    };
    let plain = d.add("gameplay ready.mp3", 9, MP3);
    let e = d.export();
    let plan = d.check(&e, &sizes(&e)).expect("plain");
    assert_eq!(plan.kept.name, "Checked");
    let e = old.export();
    assert!(old.check(&e, &sizes(&e)).is_ok());
    d.add_language("en");
    d.toggle_tag(plain, "calm");
    d.toggle_switch("calm");
    d.add_language("ja");
    let ja = d.add("gameplay ready.mp3", 9, MP3);
    d.add("evaluation cheer.mp3", 9, MP3);
    d.set_line(ja, Some(Line::Combo(100)));
    d.share(ja, Line::Cheer);
    d.mixed = true;
    let e = d.export();
    let plan = d.check(&e, &sizes(&e)).expect("declared, scripted, shared");
    assert!(plan.kept.ini.is_some() && plan.kept.script.is_some());
    // What the reader takes otherwise is refused.
    let mut broken = e.clone();
    broken.texts[1].1 = broken.texts[1].1.replace("shared/", "elsewhere/");
    assert!(d.check(&broken, &sizes(&broken)).unwrap_err().0.contains("refused"));
    let mut other = e.clone();
    other.texts[0].1 = other.texts[0].1.replace("Mixed=1\n", "");
    other.texts[1].1 = other.texts[1].1.replace(", { mixed = true }", "");
    assert!(d.check(&other, &sizes(&other)).unwrap_err().0.contains("languages"));
    let mut fewer = e.clone();
    fewer.entries.pop();
    assert!(d.check(&fewer, &sizes(&fewer)).is_err());
}

/// A pack made before with an `announcer.ini` and a script imported into
/// a draft: its pool read as the game reads it (languages, Mixed, tags,
/// switches, a sound said for several lines), and what the draft cannot
/// carry over said.
#[test]
fn a_scripted_pack_is_read_into_a_draft() {
    // The studio's own export first: read back as it was made.
    let mut d = Draft {
        name: "Round Trip".into(),
        tidy: false,
        ..Draft::default()
    };
    d.add_language("en");
    let awesome = d.add("awesome.mp3", 9, MP3);
    d.set_line(awesome, Some(Line::Combo(100)));
    d.share(awesome, Line::Cheer);
    let damn = d.add("evaluation cheer.mp3", 9, MP3);
    d.toggle_tag(damn, "rude");
    d.toggle_switch("rude");
    d.switches[0].label = "Rude words".into();
    d.add_language("ja");
    d.add("gameplay ready.mp3", 9, MP3);
    d.mixed = true;
    let export = d.export();
    let listed: Vec<(String, u64)> = export
        .listed(&export.entries.iter().map(|e| (format!("Zip/{}", e.path), 9)).collect::<Vec<_>>())
        .into_iter()
        .map(|(p, n)| if p.starts_with("Zip/") { (p, n) } else { (format!("Zip/{p}"), n) })
        .collect();
    let m = made(&listed).expect("one pack");
    assert_eq!(m.name, "Round Trip");
    assert_eq!(m.ini.as_deref(), Some("Zip/Announcers/Round Trip/announcer.ini"));
    assert!(m.is_text("Zip/Announcers/Round Trip/announcer.lua"));
    let plan = read_pack(
        &listed,
        export.text(crate::announcer::pool::DECLARATION),
        export.text(crate::announcer::script::SCRIPT),
    )
    .expect("a declared pack");
    assert_eq!(plan.languages, ["en", "ja"]);
    assert!(plan.mixed);
    assert_eq!(plan.switches, d.switches);
    assert_eq!(plan.lost, [Lost::Script]);
    assert_eq!(
        plan.takes["Zip/Announcers/Round Trip/shared/awesome.mp3"],
        Planned {
            line: Line::Combo(100),
            also: vec![Line::Cheer],
            lang: Some("en".into()),
            tags: Vec::new()
        }
    );
    assert_eq!(
        plan.takes["Zip/Announcers/Round Trip/en/evaluation cheer/evaluation cheer.mp3"].tags,
        ["rude"]
    );
    assert_eq!(
        plan.takes["Zip/Announcers/Round Trip/ja/gameplay ready/gameplay ready.mp3"].lang.as_deref(),
        Some("ja")
    );
    // Into a new draft, each sound in its place: the same pack again.
    let mut again = Draft {
        tidy: false,
        ..Draft::default()
    };
    again.adopt(Adopted::new(m.name.clone(), None).with_plan(Some(plan.clone())));
    for e in &export.entries {
        let path = format!("Zip/{}", e.path);
        let id = again.add(&path, 9, MP3);
        again.place(id, &plan.takes[&path]);
    }
    assert_eq!(again.languages, d.languages);
    assert_eq!(again.export().texts, export.texts);
    // A plain pack has no plan: its folders say.
    assert_eq!(read_pack(&pack_files("Name/"), None, None), None);
    // What a hand-written script does that the draft cannot keep.
    let files: Vec<(String, u64)> = [
        "P/announcer.ini",
        "P/announcer.lua",
        "P/en/gameplay ready/a.ogg",
        "P/gameplay cleared/b.ogg",
        "P/ja/gameplay ready/c.ogg",
    ]
    .iter()
    .map(|p| (p.to_string(), 1))
    .collect();
    let ini = "[Parameters]\nList=Language,Mood\n[Parameter Language]\nType=Language\nLanguages=en,ja\n\
               [Parameter Mood]\nType=Choice\nChoices=Calm,Loud\nDefault=Calm\n";
    let lua = "local pack = Announcer.pack()\n\
               for _, f in ipairs(Announcer.files()) do\n\
                 if Announcer.language() == 'ja' or f:sub(1, 3) ~= 'ja/' then\n\
                   pack:take(f, { weight = 2 })\n\
                 end\n\
               end\n\
               pack:take('P/x', {})\n\
               return pack\n";
    let plan = read_pack(&files, Some(ini), Some(lua)).unwrap();
    assert!(matches!(plan.lost[0], Lost::Refused(_)), "{:?}", plan.lost);
    let lua = lua.replace("pack:take('P/x', {})\n", "");
    let plan = read_pack(&files, Some(ini), Some(&lua)).unwrap();
    assert_eq!(
        plan.lost,
        [
            Lost::Script,
            Lost::ByGameLanguage,
            Lost::Settings(vec!["Mood".into()]),
            Lost::Weights,
            Lost::EveryLanguage(1, "en".into()),
        ]
    );
    assert_eq!(plan.takes["P/gameplay cleared/b.ogg"].lang.as_deref(), Some("en"));
}
