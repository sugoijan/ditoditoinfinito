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
    // The built-in announcer's voices, a folder a language.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/announcers")
        .join(BUILT_IN_FOLDER);
    let mut voices = 0;
    for pack in std::fs::read_dir(&root).unwrap() {
        let pack = pack.unwrap().path();
        if !pack.is_dir() {
            continue;
        }
        voices += 1;
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
    assert_eq!(voices, 2);
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
            "battle last song",
            "battle life critical",
            "battle attack incoming",
            "battle attack launched",
            "battle step up",
            "battle step down",
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
    // Round AC: the last song's opening, the final stage's before it.
    assert_eq!(
        b(Battle::LastSong).chain(),
        [
            b(Battle::LastSong),
            b(Battle::FinalStage),
            b(Battle::StageBegin),
            Line::Ready
        ]
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
        Battle::AttackIncoming,
        Battle::AttackLaunched,
        Battle::StepUp,
        Battle::StepDown,
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
        // An attack's two: their folders, any case, and their own ends.
        (
            "Battle Attack Incoming.wav",
            Some(Line::Battle(Battle::AttackIncoming)),
        ),
        (
            "attack launched 3.wav",
            Some(Line::Battle(Battle::AttackLaunched)),
        ),
        ("incoming.wav", Some(Line::Battle(Battle::AttackIncoming))),
        ("launched.wav", Some(Line::Battle(Battle::AttackLaunched))),
        // A step's two: their folders and their own ends.
        ("battle step up.wav", Some(Line::Battle(Battle::StepUp))),
        ("step down 2.wav", Some(Line::Battle(Battle::StepDown))),
        // No line's end.
        ("attack.wav", None),
        // Two lines' ends.
        ("begin.wav", None),
        ("drawn.wav", None),
    ] {
        assert_eq!(match_line(path), line, "{path}");
    }
    // The studio lists them, as every line the game speaks.
    assert!(draft::listed().contains(&Line::Battle(Battle::MatchBegin)));
}

/// The combo lines, exactly: the counts, the overflow and a broken combo;
/// never the full combos (the play's end) nor any other line.
#[test]
fn the_combo_lines_are_the_counts_the_overflow_and_the_stop() {
    let combos: Vec<Line> = Line::all().into_iter().filter(|l| l.is_combo()).collect();
    let mut want: Vec<Line> = (1..=10).map(|n| Line::Combo(n * 100)).collect();
    want.extend([Line::ComboOverflow, Line::ComboStopped]);
    assert_eq!(combos, want);
    assert!(Line::Combo(64).is_combo() && Line::Combo(1500).is_combo());
    assert!(!Line::FullCombo.is_combo() && !Line::EvaluationFullCombo(1).is_combo());
}

/// A declared pack (`announcer.ini`): its language folders are its
/// voices, never announcers of their own; without the file, each is one.
#[test]
fn a_declared_pack_is_one_announcer_its_language_folders_its_voices() {
    let mut listed = vec!["Announcers/Clank/announcer.ini".to_string()];
    for lang in ["en", "ja"] {
        for f in small("") {
            listed.push(format!("Announcers/Clank/{lang}/{f}"));
        }
    }
    listed.push("Announcers/Clank/cheer/x.ogg".into());
    let found = find_announcers(&files(&listed, 10), "root");
    assert_eq!(found.announcers.len(), 1);
    let a = &found.announcers[0];
    assert_eq!((a.dir.as_str(), a.name.as_str()), ("Announcers/Clank", "Clank"));
    assert_eq!(
        a.declaration.as_ref().map(|d| d.as_str()),
        Some("Announcers/Clank/announcer.ini")
    );
    assert!(a.lines.is_empty());
    assert_eq!(a.voices.keys().collect::<Vec<_>>(), ["en", "ja"]);
    assert_eq!(
        paths(&a.voices["ja"][&Line::Cleared]),
        [
            "Announcers/Clank/ja/gameplay cleared/cleared 1.ogg",
            "Announcers/Clank/ja/gameplay cleared/cleared 2.mp3"
        ]
    );
    assert_eq!(a.unknown.keys().collect::<Vec<_>>(), ["cheer"]);
    let summary = a.said().summary;
    assert_eq!((summary.lines, summary.sounds), (3, 8));
    let kept = a.kept();
    assert_eq!(kept.files, 8);
    assert_eq!(
        paths(&kept.voices["en"]["gameplay ready"]),
        ["en/gameplay ready/ready.ogg"]
    );
    // The same folders without the declaration: two announcers.
    listed.remove(0);
    let names: Vec<String> = find_announcers(&files(&listed, 10), "root")
        .announcers
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(names, ["en", "ja"]);
}

const DECLARED: &str = "[Announcer]\nName=Mr. Clank\nName.ja=ガキンさん\n\n\
    [Parameters]\nList=Language,Mood\n\n\
    [Parameter Language]\nType=Language\nLanguages=ja,en,xx-, de\nLabels=,,,Deutsch\nMixed=1\n\n\
    [Parameter Mood]\nType=Choice\nChoices=Normal,Calm\n";

#[test]
fn a_declaration_names_the_pack_and_its_languages() {
    let d = pool::Declaration::parse(DECLARED);
    assert_eq!(d.name("ja"), Some("ガキンさん"));
    assert_eq!(d.name("ja-JP"), Some("ガキンさん"));
    assert_eq!(d.name("en"), Some("Mr. Clank"));
    assert_eq!(d.name("qps-ploc"), Some("Mr. Clank"));
    assert_eq!(d.languages, ["ja", "en", "de"]);
    assert_eq!(d.language_labels["de"], "Deutsch");
    assert!(d.mixed);
    // The other parameters as a skin's; the Language one is ours.
    let names: Vec<&str> = d.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Mood"]);
    assert_eq!(d.warnings, ["\"xx-\" is not a language's tag"]);
    assert_eq!(pool::Declaration::parse("").name("en"), None);
}

/// A declared pack of two voices, as kept.
fn two_voices() -> Kept {
    let mut listed = vec!["Two/announcer.ini".to_string()];
    for lang in ["en", "ja"] {
        for f in small("") {
            listed.push(format!("Two/{lang}/{f}"));
        }
    }
    listed.push("Two/gameplay failed/f.ogg".into());
    let a = &find_announcers(&files(&listed, 10), "root").announcers[0];
    Kept {
        ini: Some(DECLARED.into()),
        ..a.kept()
    }
}

#[test]
fn the_language_follows_the_game_unless_chosen() {
    use ddi_noteskin::params::{Value, Values};
    use pool::{LANGUAGE, MIXED};
    let kept = two_voices();
    let none = Values::new();
    let ready = |k: &Kept| paths(&k.lines["gameplay ready"]).join(" ");
    // The game's language, its primary subtag, else English, else the
    // first; what has no language in every one.
    for (game, said) in [
        ("ja", "ja/gameplay ready/ready.ogg"),
        ("ja-JP", "ja/gameplay ready/ready.ogg"),
        ("en", "en/gameplay ready/ready.ogg"),
        ("qps-ploc", "en/gameplay ready/ready.ogg"),
        ("fr", "en/gameplay ready/ready.ogg"),
    ] {
        let k = kept.resolve(&none, game);
        assert_eq!(ready(&k), said, "{game}");
        assert_eq!(paths(&k.lines["gameplay failed"]), ["gameplay failed/f.ogg"]);
        assert!(k.voices.is_empty() && k.ini.is_none());
        assert_eq!(k.key(), kept.key());
    }
    assert_eq!(kept.resolve(&none, "ja").bag_key(), "two#ja");
    // Chosen: one language whatever the game's; one the pack lacks is the
    // game's.
    let chosen = |v: &str| Values::from([(LANGUAGE.to_string(), Value::Text(v.into()))]);
    assert_eq!(ready(&kept.resolve(&chosen("ja"), "en")), "ja/gameplay ready/ready.ogg");
    assert_eq!(ready(&kept.resolve(&chosen("de"), "ja")), "ja/gameplay ready/ready.ogg");
    // Mixed: both, each take once.
    let mixed = kept.resolve(&chosen(MIXED), "en");
    assert_eq!(
        ready(&mixed),
        "en/gameplay ready/ready.ogg ja/gameplay ready/ready.ogg"
    );
    assert_eq!(mixed.bag_key(), "two#ja+en");
    let offered = kept.offered(&none, "ja-JP");
    assert!(offered.has_languages() && offered.mixed);
    assert_eq!(offered.game.as_deref(), Some("ja"));
    assert_eq!(
        offered.languages,
        [("ja".to_string(), None), ("en".to_string(), None)]
    );
    assert_eq!(kept.display_name("ja"), "ガキンさん");
    // A pack of line folders alone reads as it always did.
    let plain = find_announcers(&files(&small("Plain"), 10), "root").announcers[0].kept();
    assert_eq!(plain.resolve(&chosen("ja"), "ja"), plain);
    assert!(!plain.offered(&none, "ja").has_languages());
    // The voice key changes with the choice and the game's language.
    assert_ne!(
        pool::voice_key("two", &none, "en"),
        pool::voice_key("two", &chosen("ja"), "en")
    );
}

#[test]
fn a_bundled_voice_reads_as_a_pack_of_its_own() {
    let kept = Kept {
        bundled: true,
        ..two_voices()
    };
    let b = Bundled {
        path: "Two".into(),
        hashes: [("ja/gameplay ready/ready.ogg".to_string(), "abc".to_string())].into(),
        said: [("ja/gameplay ready/ready.ogg".to_string(), "準備！".to_string())].into(),
        kept,
    };
    let ja = b.voice("ja").unwrap();
    assert_eq!(ja.path, "Two/ja");
    assert_eq!(ja.kept.name, "ガキンさん");
    assert_eq!(paths(&ja.kept.lines["gameplay ready"]), ["gameplay ready/ready.ogg"]);
    assert_eq!(ja.hashes["gameplay ready/ready.ogg"], "abc");
    assert_eq!(ja.said["gameplay ready/ready.ogg"], "準備！");
    assert!(b.voice("de").is_none());
}

/// A scripted pack: its sounds of two languages, a take outside its line
/// folders said for two lines, tagged.
fn scripted(script: &str) -> Kept {
    let mut listed = vec!["S/announcer.ini".to_string(), "S/announcer.lua".to_string()];
    for lang in ["en", "ja"] {
        for f in small("") {
            listed.push(format!("S/{lang}/{f}"));
        }
    }
    listed.push("S/shared/awesome.ogg".into());
    let a = &find_announcers(&files(&listed, 10), "root").announcers[0];
    assert_eq!(a.script.as_ref().map(|s| s.as_str()), Some("S/announcer.lua"));
    Kept {
        ini: Some(
            "[Parameters]\nList=Language,Mood\n[Parameter Language]\nType=Language\nLanguages=en,ja\n\
             [Parameter Mood]\nType=Choice\nChoices=Normal,Calm\n"
                .into(),
        ),
        script: Some(script.into()),
        ..a.kept()
    }
}

const SCRIPT: &str = r#"
local pack = Announcer.pack()
for _, f in ipairs(Announcer.files()) do
  if f:sub(1, 7) ~= "shared/" then pack:take(f) end
end
pack:take("shared/awesome.ogg", {
  events = { "gameplay 100 combo", "evaluation cheer" },
  lang = "en",
  tags = { "loud" },
})
if Announcer.param("Mood") == "Calm" then
  pack:take("en/gameplay ready/ready.ogg", { events = { "evaluation cheer" } })
end
pack:tag("loud", { label = "Loud lines", default = true })
pack:languages({ "en", "ja" }, { mixed = true })
return pack
"#;

#[test]
fn a_script_reuses_a_take_tags_it_and_mixes_its_languages() {
    use ddi_noteskin::params::{Value, Values};
    let kept = scripted(SCRIPT);
    assert_eq!(paths(&kept.others), ["shared/awesome.ogg"]);
    let none = Values::new();
    let en = kept.resolve(&none, "en");
    // One take, two lines.
    assert_eq!(paths(&en.lines["gameplay 100 combo"]), ["shared/awesome.ogg"]);
    assert_eq!(paths(&en.lines["evaluation cheer"]), ["shared/awesome.ogg"]);
    assert_eq!(paths(&en.lines["gameplay ready"]), ["en/gameplay ready/ready.ogg"]);
    // In Japanese the English take is not said.
    let ja = kept.resolve(&none, "ja");
    assert!(!ja.lines.contains_key("evaluation cheer"));
    // Its tag switched off: its take is not said; the row's default on.
    let off = Values::from([(pool::tag_value_name("loud"), Value::Bool(false))]);
    assert!(!kept.resolve(&off, "en").lines.contains_key("gameplay 100 combo"));
    let offered = kept.offered(&none, "en");
    assert_eq!(offered.refused, None);
    assert_eq!(offered.tags.len(), 1);
    assert_eq!((offered.tags[0].label.as_str(), offered.tags[0].default), ("Loud lines", true));
    assert!(offered.mixed && offered.has_languages());
    // Mixed: both languages.
    let mixed = Values::from([(pool::LANGUAGE.to_string(), Value::Text(pool::MIXED.into()))]);
    assert_eq!(
        paths(&kept.resolve(&mixed, "en").lines["gameplay ready"]),
        ["en/gameplay ready/ready.ogg", "ja/gameplay ready/ready.ogg"]
    );
    // Its parameters read by the script.
    let calm = Values::from([("Mood".to_string(), Value::Text("Calm".into()))]);
    assert_eq!(
        paths(&kept.resolve(&calm, "en").lines["evaluation cheer"]),
        ["shared/awesome.ogg", "en/gameplay ready/ready.ogg"]
    );
    // The same each time.
    assert_eq!(kept.resolve(&none, "en"), en);
}

/// Scripts that loop, grow without end, reach outside the pack, name a
/// line the game does not know or break are refused whole, each with
/// its reason, and the pack's folders speak instead: what its plain
/// layout says.
#[test]
fn a_hostile_or_broken_script_is_refused_and_the_folders_speak() {
    use ddi_noteskin::params::Values;
    let none = Values::new();
    let plain = Kept {
        script: None,
        others: Vec::new(),
        ..scripted(SCRIPT)
    };
    let fallback = plain.resolve(&none, "en");
    for (case, script, said) in [
        ("loop", "while true do end", "fuel"),
        (
            "huge table",
            "local t = {} for i = 1, 1e9 do t[i] = ('x'):rep(64) .. i end return Announcer.pack()",
            "",
        ),
        ("huge string", "local s = string.rep('x', 1e9) return Announcer.pack()", ""),
        (
            "recursion",
            "local function f(n) return f(n + 1) + 1 end f(1) return Announcer.pack()",
            "",
        ),
        (
            "outside the pack",
            "local p = Announcer.pack() p:take('../Other/gameplay ready/r.ogg') return p",
            "no sound of this pack",
        ),
        (
            "an unknown line",
            "local p = Announcer.pack() p:take('shared/awesome.ogg', { events = { 'gameplay victory dance' } }) return p",
            "not a line",
        ),
        (
            "a folder of no line",
            "local p = Announcer.pack() p:take('shared/awesome.ogg') return p",
            "names no line",
        ),
        ("syntax", "local p = Announcer.pack( return p", ""),
        ("an error", "error('broken')", "broken"),
        ("no pack", "return 5", "must return its pack"),
        ("the system", "os.execute('true') return Announcer.pack()", ""),
        ("a file read", "io.open('/etc/passwd') return Announcer.pack()", ""),
        (
            "a bad tag",
            "local p = Announcer.pack() p:tag('no spaces please') return p",
            "not a tag's name",
        ),
        (
            "a bad weight",
            "local p = Announcer.pack() p:take('en/gameplay ready/ready.ogg', { weight = -1 }) return p",
            "weight",
        ),
    ] {
        let kept = scripted(script);
        let started = std::time::Instant::now();
        let offered = kept.offered(&none, "en");
        let refused = offered.refused.unwrap_or_else(|| panic!("{case}: not refused"));
        assert!(refused.contains(said), "{case}: {refused}");
        let (resolved, why) = kept.resolve_said(&none, "en");
        assert!(why.is_some(), "{case}");
        assert_eq!(resolved, fallback, "{case}");
        assert!(started.elapsed().as_secs_f64() < 5.0, "{case}: {:?}", started.elapsed());
    }
}

/// The built-in announcer on its script says what its folders say, in
/// every choice and for every game language.
#[test]
fn the_built_in_announcer_s_script_says_what_its_folders_say() {
    use ddi_noteskin::params::{Value, Values};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/announcers");
    let mut listed = Vec::new();
    let mut walk = vec![root.join(BUILT_IN_FOLDER)];
    while let Some(dir) = walk.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk.push(p);
            } else {
                let rel = p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                listed.push((rel, 1u64));
            }
        }
    }
    let a = &find_announcers(&listed, "root").announcers[0];
    let read = |name: &str| std::fs::read_to_string(root.join(BUILT_IN_FOLDER).join(name)).ok();
    let kept = Kept {
        ini: read(pool::DECLARATION),
        script: read(script::SCRIPT),
        ..a.kept()
    };
    assert!(kept.script.is_some(), "the built-in has its script");
    let plain = Kept {
        script: None,
        ..kept.clone()
    };
    for choice in [None, Some("en"), Some("ja"), Some(pool::MIXED)] {
        let values: Values = choice
            .map(|c| (pool::LANGUAGE.to_string(), Value::Text(c.into())))
            .into_iter()
            .collect();
        for game in ["en", "ja", "ja-JP", "qps-ploc", "qps-wide", "fr"] {
            let (by_script, refused) = kept.resolve_said(&values, game);
            assert_eq!(refused, None, "{choice:?} {game}");
            assert_eq!(by_script, plain.resolve(&values, game), "{choice:?} {game}");
        }
    }
    assert_eq!(kept.offered(&Values::new(), "en"), plain.offered(&Values::new(), "en"));
}
