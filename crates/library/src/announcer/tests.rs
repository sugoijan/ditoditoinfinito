use super::*;

/// Files of `size` bytes at `paths`.
fn files<S: AsRef<str>>(paths: &[S], size: u64) -> Vec<(String, u64)> {
    paths
        .iter()
        .map(|p| (p.as_ref().to_string(), size))
        .collect()
}

fn paths(list: &[RelPathBuf]) -> Vec<&str> {
    list.iter().map(|p| p.as_str()).collect()
}

/// A small announcer under `dir` (`""`: the root): three lines.
fn small(dir: &str) -> Vec<String> {
    let at = |rest: &str| {
        if dir.is_empty() {
            rest.to_string()
        } else {
            format!("{dir}/{rest}")
        }
    };
    vec![
        at("gameplay ready/ready.ogg"),
        at("gameplay cleared/cleared 1.ogg"),
        at("gameplay cleared/cleared 2.mp3"),
        at("evaluation aa/aa.wav"),
    ]
}

#[test]
fn every_line_reads_back_from_its_folder_in_any_case() {
    for line in Line::all() {
        let folder = line.folder();
        assert_eq!(Line::parse(&folder), Some(line), "{folder}");
        assert_eq!(Line::parse(&folder.to_uppercase()), Some(line), "{folder}");
        assert_eq!(classify(&folder), Kind::Line(line));
    }
    // StepMania's alias, and our combos of any other count.
    assert_eq!(Line::parse("Gameplay Combo 300"), Some(Line::Combo(300)));
    assert_eq!(Line::parse("gameplay 1500 combo"), Some(Line::Combo(1500)));
    assert_eq!(Line::parse("gameplay 64 combo"), Some(Line::Combo(64)));
    assert_eq!(Line::parse("gameplay combo 222"), Some(Line::Combo(222)));
    assert_eq!(Line::parse("gameplay 1 combo"), Some(Line::Combo(1)));
    assert_eq!(Line::Combo(1500).origin(), Origin::Ours);
    assert_eq!(Line::Combo(64).origin(), Origin::Ours);
    assert_eq!(Line::Combo(1000).origin(), Origin::StepMania);
    for not in [
        "gameplay 0 combo",
        "gameplay 0100 combo",
        "gameplay 99999999999 combo",
        "gameplay combo",
        "gameplay  100 combo",
        "gameplay -100 combo",
    ] {
        assert_eq!(Line::parse(not), None, "{not}");
    }
}

#[test]
fn lines_say_where_they_are_spoken_and_who_named_them() {
    assert_eq!(Line::Ready.spoken(), Spoken::Play);
    assert_eq!(Line::Grade(Grade::A).spoken(), Spoken::Results);
    assert_eq!(Line::Quit.spoken(), Spoken::Results);
    assert_eq!(Line::SelectMusic(Comment::New).spoken(), Spoken::Menu);
    assert_eq!(Line::Grade(Grade::Aaaaa).origin(), Origin::Etterna);
    assert_eq!(Line::Grade(Grade::F).origin(), Origin::Etterna);
    assert_eq!(Line::Grade(Grade::E).origin(), Origin::StepMania);
    for ours in [
        Line::Restart,
        Line::FullCombo,
        Line::FullComboW(2),
        Line::LastLife,
        Line::LifeLost,
        Line::Quit,
        Line::FirstClear,
    ] {
        assert_eq!(ours.origin(), Origin::Ours, "{ours:?}");
    }
}

#[test]
fn a_missing_line_falls_back_along_its_chain() {
    assert_eq!(
        Line::Combo(1200).chain(),
        [Line::Combo(1200), Line::ComboOverflow]
    );
    assert_eq!(Line::Combo(500).chain(), [Line::Combo(500)]);
    // Only a hundred past 1000 is More combo's.
    assert_eq!(Line::Combo(1024).chain(), [Line::Combo(1024)]);
    assert_eq!(Line::Combo(64).chain(), [Line::Combo(64)]);
    assert_eq!(Line::Restart.chain(), [Line::Restart, Line::Ready]);
    assert_eq!(
        Line::FullComboW(1).chain(),
        [Line::FullComboW(1), Line::FullCombo]
    );
    assert_eq!(
        Line::Grade(Grade::Aaaaa).chain(),
        [Line::Grade(Grade::Aaaaa), Line::Grade(Grade::Aaaa)]
    );
    assert_eq!(
        Line::Grade(Grade::E).chain(),
        [Line::Grade(Grade::E), Line::Grade(Grade::F)]
    );
    assert_eq!(
        Line::Grade(Grade::F).chain(),
        [Line::Grade(Grade::F), Line::Grade(Grade::E)]
    );
    assert_eq!(Line::FirstClear.chain(), [Line::FirstClear]);

    let list = [
        "A/gameplay combo overflow/o.ogg",
        "A/gameplay ready/r.ogg",
        "A/evaluation f/f.ogg",
    ];
    let found = find_announcers(&files(&list, 1), "x");
    let a = &found.announcers[0];
    let (line, sounds) = a.files(Line::Combo(1300)).unwrap();
    assert_eq!(line, Line::ComboOverflow);
    assert_eq!(paths(sounds), ["A/gameplay combo overflow/o.ogg"]);
    assert_eq!(a.files(Line::Restart).unwrap().0, Line::Ready);
    assert_eq!(
        a.files(Line::Grade(Grade::E)).unwrap().0,
        Line::Grade(Grade::F)
    );
    assert!(a.files(Line::Cleared).is_none());
}

#[test]
fn old_names_screen_lines_and_unknown_folders_are_kept_apart() {
    assert_eq!(
        classify("ScreenSelectStyle comment Single"),
        Kind::Unused("select style comment single".into())
    );
    assert_eq!(
        classify("ScreenSelectDifficulty intro"),
        Kind::Unused("select difficulty intro".into())
    );
    assert_eq!(classify("stage Final"), Kind::Unused("stage final".into()));
    assert_eq!(classify("stage 7"), Kind::Unknown("stage 7".into()));
    assert_eq!(
        classify("evaluation final aaa"),
        Kind::Unused("evaluation final aaa".into())
    );
    assert_eq!(
        classify("ScreenSelectMaster comment heavy"),
        Kind::Unused("screenselectmaster comment heavy".into())
    );
    assert_eq!(
        classify("ScreenSelectPlayMode IdleComment"),
        Kind::Unused("screenselectplaymode idlecomment".into())
    );
    assert_eq!(
        classify("gameplayNonStop ready"),
        Kind::Unknown("gameplaynonstop ready".into())
    );
    assert_eq!(
        classify("SSStyle intro"),
        Kind::Unknown("ssstyle intro".into())
    );
}

#[test]
fn announcers_are_found_whatever_wraps_them() {
    // A `.smzip`: its control file, then `Announcers/<Name>/`.
    let mut list = vec!["smzip.ctl".to_string()];
    list.extend(small("Announcers/Techno"));
    let found = find_announcers(&files(&list, 10), "techno.smzip");
    assert_eq!(found.announcers.len(), 1);
    let a = &found.announcers[0];
    assert_eq!(
        (a.dir.as_str(), a.name.as_str()),
        ("Announcers/Techno", "Techno")
    );
    assert_eq!(
        paths(&a.lines[&Line::Cleared]),
        [
            "Announcers/Techno/gameplay cleared/cleared 1.ogg",
            "Announcers/Techno/gameplay cleared/cleared 2.mp3"
        ]
    );
    assert_eq!((a.report.files, a.report.bytes), (4, 40));

    // `<Name>/`, and an odd top folder.
    for top in ["DDR 1st Mix", "Dance Dance Revolution MEGA Announcer Pack!"] {
        let list = small(top);
        let found = find_announcers(&files(&list, 1), "pack.zip");
        assert_eq!(found.announcers[0].name, top);
    }

    // The line folders at the root: named after the archive or folder.
    let list = small("");
    let found = find_announcers(&files(&list, 1), "Ashens");
    let a = &found.announcers[0];
    assert_eq!((a.dir.as_str(), a.name.as_str()), ("", "Ashens"));
    assert_eq!(a.lines.len(), 3);

    // Several in one `Announcers` folder; one there needs a single line.
    let mut list = small("Announcers/One");
    list.extend(small("Announcers/Two"));
    list.push("Announcers/Tiny/gameplay ready/r.ogg".into());
    let found = find_announcers(&files(&list, 1), "x");
    let names: Vec<&str> = found.announcers.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["One", "Tiny", "Two"]);
}

#[test]
fn what_is_not_an_announcer_is_not_found() {
    let list = [
        // Numbered sounds, no line folders (a game's voices as ripped).
        "DDR A voices/00000000.wav",
        "DDR A voices/0000000a.wav",
        // Songs.
        "Pack/Song/song.sm",
        "Pack/Song/song.ogg",
        // Two lines alone, outside an `Announcers` folder.
        "Loose/gameplay ready/r.ogg",
        "Loose/gameplay failed/f.ogg",
        // A theme's sounds, and StepMania's template.
        "Themes/T/Sounds/gameplay ready/r.ogg",
        "Themes/T/Sounds/gameplay failed/f.ogg",
        "Themes/T/Sounds/gameplay cleared/c.ogg",
        "Announcers/Empty/gameplay ready/r.ogg",
        // Line folders without sounds.
        "Silent/gameplay ready/readme.txt",
        "Silent/gameplay failed/x.pk",
        "Silent/gameplay cleared/notes.doc",
        // Screen lines alone (a theme's BGAnimations has such names).
        "BG/ScreenTitleMenu intro/a.ogg",
        "BG/ScreenGameplay intro/a.ogg",
        "BG/ScreenEvaluation intro/a.ogg",
    ];
    let found = find_announcers(&files(&list, 1), "x");
    assert_eq!(found, Found::default());
    assert!(announcer_dirs(&list).is_empty());
}

#[test]
fn a_messy_pack_reports_what_it_passed_over() {
    let top = "Mega";
    let mut list: Vec<String> = small(top);
    list.extend(
        [
            // Old names and a screen line, kept unused; an unknown folder.
            "select style intro/intro.mp3",
            "ScreenSelectStyle Intro/intro 2.mp3",
            "ScreenSelectMaster comment heavy/heavy.mp3",
            "SSStyle intro/s.mp3",
            "gameplay 1500 combo/1500.mp3",
            // Two spellings of one line, and the alias of another.
            "Gameplay Ready/ready 2.ogg",
            "gameplay combo 100/100.ogg",
            // Not sounds: a readme, a peak file alone, loose sounds.
            "readme.txt",
            "New Record.mp3",
            "stage 6/CARD_0557.pk",
            "gameplay failed/failed.ogg",
            "gameplay failed/failed.pk",
            // A copy of a whole announcer inside a line folder.
            "evaluation aa/evaluation a/a.mp3",
            "evaluation aa/gameplay ready/r.mp3",
            "evaluation aa/gameplay failed/f.mp3",
            "evaluation aa/gameplay cleared/c.mp3",
            // A line folder holding only folders.
            "evaluation b/evaluation b/b.mp3",
            // Hidden files.
            "gameplay ready/._ready.ogg",
            ".DS_Store",
        ]
        .map(|p| format!("{top}/{p}")),
    );
    let found = find_announcers(&files(&list, 1), "x");
    assert_eq!(found.announcers.len(), 1);
    let a = &found.announcers[0];
    assert_eq!(
        paths(&a.lines[&Line::Ready]),
        [
            "Mega/Gameplay Ready/ready 2.ogg",
            "Mega/gameplay ready/ready.ogg"
        ]
    );
    assert_eq!(
        paths(&a.lines[&Line::Combo(100)]),
        ["Mega/gameplay combo 100/100.ogg"]
    );
    assert_eq!(
        paths(&a.lines[&Line::Combo(1500)]),
        ["Mega/gameplay 1500 combo/1500.mp3"]
    );
    assert_eq!(
        paths(&a.lines[&Line::Failed]),
        ["Mega/gameplay failed/failed.ogg"]
    );
    assert_eq!(
        paths(&a.unused["select style intro"]),
        [
            "Mega/ScreenSelectStyle Intro/intro 2.mp3",
            "Mega/select style intro/intro.mp3"
        ]
    );
    assert!(a.unused.contains_key("screenselectmaster comment heavy"));
    assert!(a.unknown.contains_key("ssstyle intro"));
    assert_eq!(a.report.nested, [RelPathBuf::from("Mega/evaluation aa")]);
    assert_eq!(a.report.empty, ["evaluation b", "stage 6"]);
    // readme, New Record, the peak files, the nested copy's four, the
    // folder in `evaluation b`.
    assert_eq!(a.report.ignored, 9);
    assert_eq!(a.report.left_out, 0);
    // What the shells keep: the lines alone, relative to the folder.
    let kept = a.kept();
    assert_eq!(kept.key(), "mega");
    assert_eq!(kept.lines.len(), a.lines.len());
    assert_eq!(
        paths(&kept.lines["gameplay ready"]),
        ["Gameplay Ready/ready 2.ogg", "gameplay ready/ready.ogg"]
    );
    assert_eq!(kept.files, 8);
    assert_eq!(kept.bytes, 8);
    assert_eq!(a.report.bytes, 12);
    let said = a.said();
    assert_eq!(
        said.summary,
        crate::report::AnnouncerSummary {
            lines: 6,
            in_play: 5,
            on_results: 1,
            on_select: 0,
            sounds: 8,
            bytes: 8,
        }
    );
    assert!(said.warnings.is_empty());
    let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    use crate::report::Finding;
    assert_eq!(
        said.details,
        [
            Finding::AnnouncerOldFolders {
                count: 2,
                names: names(&["screenselectmaster comment heavy", "select style intro"]),
            },
            Finding::AnnouncerUnknownFolders {
                count: 1,
                names: names(&["ssstyle intro"]),
            },
            Finding::AnnouncerEmptyFolders {
                count: 2,
                names: names(&["evaluation b", "stage 6"]),
            },
            Finding::AnnouncerNestedCopies {
                count: 1,
                names: names(&["Mega/evaluation aa"]),
            },
            Finding::AnnouncerOtherFiles { count: 9 },
        ]
    );
}

#[test]
fn what_is_kept_reads_back_and_follows_the_chains() {
    let mut list = small("Voice");
    list.push("Voice/gameplay 1000 combo/1000.ogg".into());
    list.push("Voice/gameplay combo overflow/more.ogg".into());
    list.push("Voice/gameplay full combo/fc.ogg".into());
    let found = find_announcers(&files(&list, 1 << 20), "x");
    let kept = found.announcers[0].kept();
    let json = serde_json::to_string(&kept).unwrap();
    let back: Kept = serde_json::from_str(&json).unwrap();
    assert_eq!(back, kept);
    assert_eq!(back.bytes, 7 << 20);
    let first = |line| back.files(line).map(|(l, f)| (l, paths(f)[0].to_string()));
    assert_eq!(
        first(Line::Restart),
        Some((Line::Ready, "gameplay ready/ready.ogg".into()))
    );
    assert_eq!(
        first(Line::Combo(1200)),
        Some((
            Line::ComboOverflow,
            "gameplay combo overflow/more.ogg".into()
        ))
    );
    assert_eq!(
        first(Line::FullComboW(1)),
        Some((Line::FullCombo, "gameplay full combo/fc.ogg".into()))
    );
    assert_eq!(first(Line::Combo(200)), None);
    // Options' sample: any of a line's sounds (no `select music intro`).
    assert_eq!(
        back.sample(0).map(|p| p.as_str()),
        Some("evaluation aa/aa.wav")
    );
    let mut intro = back.clone();
    intro.lines.insert(
        "select music intro".into(),
        vec![
            "select music intro/a.ogg".into(),
            "select music intro/b.ogg".into(),
        ],
    );
    assert_eq!(
        intro.sample(3).map(|p| p.as_str()),
        Some("select music intro/b.ogg")
    );
    assert_eq!(Kept::default().sample(0), None);
    // A later version's line is passed over, not an error.
    let later = json.replace("gameplay ready", "gameplay someday");
    let read: Kept = serde_json::from_str(&later).unwrap();
    assert_eq!(read.files(Line::Ready), None);
    let summary = found.announcers[0].said().summary;
    assert_eq!(
        (
            summary.lines,
            summary.in_play,
            summary.on_results,
            summary.sounds
        ),
        (6, 5, 1, 7)
    );
    assert_eq!(summary.bytes / 100_000, 73);
}

#[test]
fn the_caps_keep_the_spoken_lines_first() {
    // An unknown folder sorted first, then more sounds than an announcer
    // keeps: the lines take them in their folders' order.
    let mut list: Vec<String> = (0..10).map(|i| format!("A/aaa custom/{i}.ogg")).collect();
    list.extend((0..MAX_FILES).map(|i| format!("A/gameplay comment hot/{i:04}.ogg")));
    list.extend(small("A"));
    let found = find_announcers(&files(&list, 1), "x");
    let a = &found.announcers[0];
    assert_eq!(a.report.files, MAX_FILES);
    assert_eq!(a.report.left_out, 14);
    assert!(a.unknown.is_empty());
    assert_eq!(a.lines.len(), 3);
    assert_eq!(a.lines[&Line::CommentHot].len(), MAX_FILES - 3);
    assert!(!a.lines.contains_key(&Line::Ready));

    // Bytes: the second big sound does not fit.
    let list = files(
        &[
            "B/gameplay ready/a.ogg",
            "B/gameplay ready/b.ogg",
            "B/gameplay failed/f.ogg",
            "B/gameplay cleared/c.ogg",
        ],
        MAX_BYTES / 2 + 1,
    );
    let a = &find_announcers(&list, "x").announcers[0];
    assert_eq!((a.report.files, a.report.left_out), (1, 3));

    // Announcers: at most so many from one import.
    let list: Vec<String> = (0..MAX_ANNOUNCERS + 2)
        .flat_map(|i| small(&format!("Announcers/{i:03}")))
        .collect();
    let found = find_announcers(&files(&list, 1), "x");
    assert_eq!(
        (found.announcers.len(), found.left_out),
        (MAX_ANNOUNCERS, 2)
    );
}

#[test]
fn an_announcer_beside_songs_is_no_song_without_a_simfile() {
    let mut list = small("Announcers/Techno");
    list.extend(small("Pack Voice"));
    list.extend([
        "Pack/Song/song.sm".to_string(),
        "Pack/Song/song.ogg".to_string(),
        "Pack/Orphan/track.ogg".to_string(),
    ]);
    assert_eq!(
        crate::pack::folders_without_simfile(&list),
        [(
            RelPathBuf::from("Pack/Orphan"),
            RelPathBuf::from("Pack/Orphan/track.ogg")
        )]
    );
}

#[test]
fn a_bundled_announcers_key_is_apart_from_an_imported_ones() {
    let imported = Kept {
        name: "Mr. Clank".into(),
        ..Kept::default()
    };
    let ours = Kept {
        bundled: true,
        ..imported.clone()
    };
    assert_eq!(imported.key(), "mr. clank");
    assert_eq!(ours.key(), "bundled:mr. clank");
    assert!(is_bundled(&ours.key()) && !is_bundled(&imported.key()));
    assert_eq!(bundled_key("ガキンさん"), "bundled:ガキンさん");
    // Stored as before (the web's records): the flag only when set.
    let json = serde_json::to_string(&imported).unwrap();
    assert!(!json.contains("bundled"));
    let back: Kept = serde_json::from_str(&serde_json::to_string(&ours).unwrap()).unwrap();
    assert_eq!(back, ours);
}

#[test]
fn our_combos_are_the_combo_lines_our_packs_have() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/announcers");
    for pack in std::fs::read_dir(&root).unwrap() {
        let pack = pack.unwrap().path();
        if !pack.is_dir() {
            continue;
        }
        let mut combos: Vec<u32> = std::fs::read_dir(&pack)
            .unwrap()
            .filter_map(|e| match Line::parse(e.ok()?.file_name().to_str()?) {
                Some(Line::Combo(n)) => Some(n),
                _ => None,
            })
            .collect();
        combos.sort();
        assert_eq!(combos, OUR_COMBOS, "{}", pack.display());
    }
}

#[test]
fn the_battle_lines_have_their_own_folders_and_category() {
    let folders: Vec<String> = Battle::ALL
        .iter()
        .map(|&b| Line::Battle(b).folder())
        .collect();
    assert_eq!(
        folders,
        [
            "battle match begin",
            "battle stage begin",
            "battle final stage",
            "battle life critical",
            "battle survivors 4",
            "battle survivors 2",
            "battle eliminated",
            "battle stage won",
            "battle stage not won",
            "battle stage drawn",
            "battle song finished",
            "battle match won",
            "battle match not won",
            "battle match drawn",
        ]
    );
    for b in Battle::ALL {
        let line = Line::Battle(b);
        assert_eq!(line.spoken(), Spoken::Battle, "{b:?}");
        assert_eq!(line.origin(), Origin::Ours, "{b:?}");
        assert!(Line::all().contains(&line), "{b:?}");
    }
    // Only the milestones are lines; StepMania's old battle names stay
    // read and unused, never these.
    assert_eq!(Line::parse("battle survivors 3"), None);
    assert_eq!(
        classify("gameplay battle trick level1"),
        Kind::Unused("gameplay battle trick level1".into())
    );
    assert_eq!(
        classify("evaluation win"),
        Kind::Unused("evaluation win".into())
    );
    // A pack's battle folders make it an announcer as other lines do.
    let found = find_announcers(
        &files(
            &[
                "Pack/battle match begin/a.ogg",
                "Pack/battle stage won/b.ogg",
                "Pack/battle eliminated/c.ogg",
            ],
            10,
        ),
        "root",
    );
    assert_eq!(found.announcers.len(), 1);
    assert_eq!(found.announcers[0].lines.len(), 3);
}

#[test]
fn a_missing_battle_line_falls_back_on_the_play_line_of_its_moment() {
    let b = Line::Battle;
    assert_eq!(
        b(Battle::MatchBegin).chain(),
        [b(Battle::MatchBegin), b(Battle::StageBegin), Line::Ready]
    );
    assert_eq!(
        b(Battle::FinalStage).chain(),
        [b(Battle::FinalStage), b(Battle::StageBegin), Line::Ready]
    );
    assert_eq!(
        b(Battle::StageBegin).chain(),
        [b(Battle::StageBegin), Line::Ready]
    );
    assert_eq!(
        b(Battle::LifeCritical).chain(),
        [b(Battle::LifeCritical), Line::CommentDanger]
    );
    assert_eq!(
        b(Battle::Eliminated).chain(),
        [b(Battle::Eliminated), Line::Failed]
    );
    assert_eq!(
        b(Battle::SongFinished).chain(),
        [b(Battle::SongFinished), Line::Cleared]
    );
    // Outcomes and milestones have nothing to fall back on: a chart's
    // clear is not a stage's win.
    for alone in [
        Battle::Survivors(4),
        Battle::Survivors(2),
        Battle::StageWon,
        Battle::StageNotWon,
        Battle::StageDrawn,
        Battle::MatchWon,
        Battle::MatchNotWon,
        Battle::MatchDrawn,
    ] {
        assert_eq!(b(alone).chain(), [b(alone)], "{alone:?}");
    }
    // A kept pack with only the play lines speaks them in the battle's
    // place.
    let mut kept = Kept::default();
    kept.lines
        .insert("gameplay ready".into(), vec!["r.ogg".into()]);
    kept.lines
        .insert("gameplay failed".into(), vec!["f.ogg".into()]);
    assert_eq!(
        kept.files(b(Battle::FinalStage)).map(|(l, _)| l),
        Some(Line::Ready)
    );
    assert_eq!(
        kept.files(b(Battle::Eliminated)).map(|(l, _)| l),
        Some(Line::Failed)
    );
    assert_eq!(kept.files(b(Battle::StageWon)), None);
}

#[test]
fn the_battle_lines_leave_the_studio_s_names_for_other_lines_alone() {
    use draft::match_line;
    // Ends of the play lines' folders still find them alone.
    for (path, line) in [
        ("lost.wav", Some(Line::LifeLost)),
        ("danger 2.wav", Some(Line::CommentDanger)),
        ("failed.wav", Some(Line::Failed)),
        ("cleared.wav", Some(Line::Cleared)),
        ("ready.wav", Some(Line::Ready)),
        // The battle's, by their folder or an end of their own.
        (
            "battle stage won 2.wav",
            Some(Line::Battle(Battle::StageWon)),
        ),
        ("eliminated.wav", Some(Line::Battle(Battle::Eliminated))),
        ("final stage.wav", Some(Line::Battle(Battle::FinalStage))),
        // Two lines' ends.
        ("begin.wav", None),
        ("drawn.wav", None),
    ] {
        assert_eq!(match_line(path), line, "{path}");
    }
    // The studio lists them, as every line the game speaks.
    assert!(draft::listed().contains(&Line::Battle(Battle::MatchBegin)));
}
