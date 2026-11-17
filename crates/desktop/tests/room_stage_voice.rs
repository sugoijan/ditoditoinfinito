//! The announcer in a practice stage with attacks as the desktop window
//! runs it (`docs/announcer-lines.md`, "In battle";
//! `docs/plans/multiplayer-prototype-progress.md`, M4 and M8 round 3), on
//! this machine's GPU offscreen and a fake clock with no sound device:
//! an announcer chosen in the settings, its stage's sounds decoded behind
//! the prompt, an autoplayed stage against CPUs whose full bars launch
//! attacks; what the play said of the battle's moments and the attacks',
//! and what it showed in the status line instead, read from the host's
//! log. Without a GPU adapter (CI) the stage runs undrawn
//! (`common::rig::Draws`) and all of it is checked all the same: what is
//! said and shown is the session's, never the view's.

mod common;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use ddi_desktop::play::host::Host;
use ddi_platform::history::MemoryPlayStore;
use ddi_platform::storage::{self, MemoryStore};
use ddi_ui::screens::data::Difficulty;
use ddi_ui::state::history::Plays;
use ddi_ui::state::settings::Settings;

const SIZE: [u32; 2] = [1280, 720];

/// The seed of the practice room's CPUs (so their play, the attacks and
/// the stage's battle) and of the voice's takes: the same stage in every
/// run. Under it the player's fourth attack launches at 84.59 s of the
/// song, 16 s after the attack moments before it and two before the
/// next, where nothing is said: its line is ([`quiet_launches_are_said`]).
const SEED: u64 = 7;

/// Seconds before a moment within which no battle line was said for it to
/// fall in a quiet moment: longer than any of our packs' battle lines
/// (2.5 s at most) said at the end of a combo line it waited for (3.2 s
/// at most).
const QUIET: f64 = 8.0;

/// A moment as the play told it: its line's folder, whether a line said
/// it (else it was shown, or left to the results), and when (host
/// seconds).
#[derive(Debug)]
struct Told {
    line: String,
    said: bool,
    at: f64,
}

/// What a stage said and showed.
struct Heard {
    /// The lines spoken, by folder, in order.
    spoken: Vec<String>,
    /// The player's attack moments as told.
    attacks: Vec<Told>,
    /// The battle's moments as told.
    moments: Vec<Told>,
    /// The attacks the player launched, and those against them, as the
    /// stage committed them.
    launched: usize,
    against: usize,
}

/// A one-stage practice match under the attacks' battle on Beatucada's
/// Medium chart, autoplayed against the CPUs with announcer `key` chosen
/// (`folder`: a song folder it is found in; ours need none) in `language`
/// where given: what the play said.
fn stage_with(key_of: &str, language: Option<&str>, folder: Option<PathBuf>) -> Option<Heard> {
    common::indexes();
    storage::install(Rc::new(MemoryStore::default()));
    let mut settings = Settings {
        announcer: Some(key_of.into()),
        ..Settings::default()
    };
    if let Some(lang) = language {
        settings.set_announcer_param(
            key_of,
            ddi_library::announcer::pool::LANGUAGE,
            Some(ddi_ui::state::settings::ParamValue::Text(lang.into())),
            None,
        );
    }
    settings.save();
    if let Some(folder) = &folder {
        ddi_desktop::library::set_extra_folders(std::slice::from_ref(folder));
    }
    let (mut ui, env) = common::ui::theme("start", common::ui::lowercase(), SIZE, true);
    let store = Rc::new(MemoryPlayStore::default());
    let mut host = Host::new(Rc::default(), Some(Plays::new(store)));
    let clock = common::rig::fake_clock(&mut host);
    let draws = common::rig::Draws::here(&mut host, SIZE);
    // The same stage in every run: the room's CPUs and the voice's takes
    // drawn from fixed seeds, never from the time (a new room each time,
    // its voice's shuffles begun anew).
    common::practice::seed(SEED);
    let room = common::practice::open(&mut ui, &env, &host);
    let chart = 4;
    let entry = common::practice::song_entry("beatucada", "Beatucada", chart, Difficulty::Medium);
    {
        let mut r = room.borrow_mut();
        let rules = ddi_room::MatchRules {
            stages: 1,
            battle: ddi_room::BattlePreset::GAUGE_ATTACKS_V0.into(),
            ..r.rules().clone()
        };
        r.save_rules(rules).unwrap();
    }
    room.borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    ui.update(0.1);
    let mut request = host.requested.borrow_mut().take().expect("asked for");
    request.auto = true;
    *host.requested.borrow_mut() = Some(request);
    // Read, and the voice decoded (it never holds Start, and the play's
    // fake clock would outrun its thread): waited for by asking, never
    // by counting. The thread starts late here, as on a loaded machine,
    // so a wait that only counts fails (review tests-1).
    ddi_desktop::play::voice::set_test_delay(600);
    let mut waited = 0;
    while !host.prompt_ready() || !host.voice_loaded() {
        draws.poll(&mut host, &mut ui, &env);
        ui.update(0.05);
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 3000, "the song or the voice was never read");
    }
    draws.ready_and_start(&mut host, &mut ui, &env, &room);
    assert!(host.playing(), "{:?}", ui.layers());
    let mut frames = 0;
    while host.playing() {
        let now = clock.fetch_add(1_000_000_000 / 10, Ordering::SeqCst) + 1_000_000_000 / 10;
        draws.frame(&mut host, &mut ui, &env, now);
        if frames % 16 == 0 {
            draws.wait_gpu();
        }
        frames += 1;
        assert!(frames < 10 * 400, "the play never ended");
    }
    draws.wait_gpu();
    ui.update(1.0);
    // The results' line, once the room confirmed the stage.
    draws.poll(&mut host, &mut ui, &env);
    ui.update(0.5);
    eprintln!("{key_of}: {:#?}", host.log);
    // No line came due before its sound was decoded: what is asserted
    // below is of the voice, never of how fast its thread was.
    let report = host
        .log
        .iter()
        .find(|l| l.starts_with("announcer: spoke "))
        .expect("the voice's report");
    assert!(report.contains("· not decoded [] ·"), "{report}");
    let list = |prefix: &str| -> Vec<String> {
        host.log
            .iter()
            .filter_map(|l| l.strip_prefix(prefix))
            .flat_map(|l| l.split(" · ").next().unwrap_or("").split(", "))
            .map(str::to_string)
            .filter(|l| !l.is_empty())
            .collect()
    };
    // "battle attack launched (shown) at 123.456 s"
    let marked = |items: Vec<String>| -> Vec<Told> {
        items
            .into_iter()
            .map(|i| {
                let (line, rest) = i.split_once(" (").expect("how it was told");
                let (how, at) = rest.split_once(") at ").expect("when");
                Told {
                    line: line.to_string(),
                    said: how.starts_with("said"),
                    at: at.trim_end_matches(" s").parse().expect("seconds"),
                }
            })
            .collect()
    };
    let committed = |part: &str| {
        host.log
            .iter()
            .filter(|l| l.starts_with("attack: committed ") && l.contains(part))
            .count()
    };
    Some(Heard {
        spoken: list("announcer: spoke "),
        attacks: marked(list("attacks: ")),
        moments: marked(list("battle: ")),
        launched: committed(" you→"),
        against: committed("→you "),
    })
}

/// What `battle-voice-v3` promises of the player's attack moments,
/// whichever take of each line was said: each attack the player launched
/// told once and each against them at most once (one right behind
/// another is not told again), by its line or in the status line; a
/// launch told in the frame an attack against the player was said (a
/// line that matters more) shown; a launch where no battle line was said
/// for [`QUIET`] seconds said; and the stage ([`SEED`]) has such a quiet
/// launch.
fn quiet_launches_are_said(key: &str, heard: &Heard) {
    let launches: Vec<&Told> = heard
        .attacks
        .iter()
        .filter(|t| t.line == "battle attack launched")
        .collect();
    let incoming = || {
        heard
            .attacks
            .iter()
            .filter(|t| t.line == "battle attack incoming")
    };
    assert_eq!(launches.len(), heard.launched, "{key}: {:?}", heard.attacks);
    assert!(
        (1..=heard.against).contains(&incoming().count()),
        "{key}: {} against the player: {:?}",
        heard.against,
        heard.attacks
    );
    let mut quiet = 0;
    for launch in &launches {
        if incoming().any(|t| t.said && t.at == launch.at) {
            assert!(
                !launch.said,
                "{key}: said over an attack against the player: {:?}",
                heard.attacks
            );
        }
        let speaking = heard
            .attacks
            .iter()
            .chain(&heard.moments)
            .filter(|t| !std::ptr::eq(*t, *launch))
            .any(|t| t.said && t.at <= launch.at && t.at > launch.at - QUIET);
        if !speaking {
            quiet += 1;
            assert!(
                launch.said,
                "{key}: a quiet launch not said: {:?} {:?}",
                heard.attacks, heard.moments
            );
        }
    }
    assert!(
        quiet > 0,
        "{key}: no launch in a quiet moment: {:?} {:?}",
        heard.attacks,
        heard.moments
    );
}

/// `seconds` of a 16-bit mono WAV tone at 48 kHz.
fn wav(seconds: f64) -> Vec<u8> {
    let n = (48_000.0 * seconds) as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| ((i as f64 * 440.0 * std::f64::consts::TAU / 48_000.0).sin() * 0.25) as f32)
        .collect();
    ddi_library::wav::write(&[samples], 48_000)
}

/// A pack made in a scratch folder with the attack lines' two folders
/// (and nothing else of the battle's), found in a song folder's
/// `Announcers/` and chosen: a practice stage says `battle attack
/// incoming` and `battle attack launched` for the player's attack
/// moments, and each moment is said or shown, never neither, a launch in
/// a quiet moment said ([`quiet_launches_are_said`]); the battle's other
/// moments, which the pack lacks, are not said.
#[test]
fn a_scratch_pack_s_attack_lines_are_said_in_a_practice_stage() {
    let root = std::env::temp_dir()
        .join("ddi-desktop-tests")
        .join(format!("stage-voice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let tone = wav(0.6);
    for line in ["battle attack incoming", "Battle Attack Launched"] {
        let dir = root.join("Announcers/Tones").join(line);
        std::fs::create_dir_all(&dir).unwrap();
        for take in ["a.wav", "b.wav"] {
            std::fs::write(dir.join(take), &tone).unwrap();
        }
    }
    let Some(heard) = stage_with("tones", None, Some(root.clone())) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let _ = std::fs::remove_dir_all(&root);
    let said = |line: &str| heard.spoken.iter().filter(|l| *l == line).count();
    let told = |line: &str, how: bool| {
        heard
            .attacks
            .iter()
            .filter(|t| t.line == line && t.said == how)
            .count()
    };
    quiet_launches_are_said("tones", &heard);
    assert!(said("battle attack incoming") > 0, "{:?}", heard.spoken);
    for line in ["battle attack incoming", "battle attack launched"] {
        assert_eq!(said(line), told(line, true), "{line}");
    }
    // An attack against the player is always said here: nothing that
    // matters more speaks in this pack.
    assert_eq!(
        told("battle attack incoming", false),
        0,
        "{:?}",
        heard.attacks
    );
    // Nothing but the two lines: the pack has no other.
    assert!(
        heard.spoken.iter().all(|l| l.starts_with("battle attack ")),
        "{:?}",
        heard.spoken
    );
    assert!(heard.moments.iter().all(|t| !t.said), "{:?}", heard.moments);
}

/// Ours, which comes with the game, in each of its languages: each says a
/// practice stage's battle lines from the assets, the two attack lines
/// among them, each by its own line (never a play's routine line but the
/// combo lines, `battle-voice-v1`, and of the play's end the autoplay's
/// full combo before the results' outcome, with no cleared between them
/// since the packs have the outcomes' lines, `battle-voice-v3`; never one
/// in another's place), the
/// autoplayed player's steps up as their new charts begin, and the status
/// line shows only the moments passed over for a line that mattered as
/// much: a launch in a quiet moment is said, one over an attack against
/// the player shown ([`quiet_launches_are_said`]), never which happens to
/// win.
#[test]
fn our_announcers_say_the_battle_lines_in_a_practice_stage() {
    for key in ["en", "ja"] {
        let Some(heard) = stage_with(ddi_library::announcer::BUILT_IN, Some(key), None) else {
            eprintln!("no GPU adapter: skipped");
            return;
        };
        let said = |line: &str| heard.spoken.iter().filter(|l| *l == line).count();
        // The opening, here we go, the attacks' two, and the results'.
        assert_eq!(heard.spoken[0], "battle match begin", "{key}: {:?}", heard.spoken);
        assert_eq!(heard.spoken[1], "gameplay here we go normal", "{key}");
        assert!(said("battle attack incoming") > 0, "{key}: {:?}", heard.spoken);
        quiet_launches_are_said(key, &heard);
        assert_eq!(
            heard.spoken.last().map(String::as_str),
            Some("battle match won"),
            "{key}: {:?}",
            heard.spoken
        );
        // Nothing but the battle's lines, here we go, the combo lines
        // and the lines of the play's end.
        let combo = |l: &str| ddi_library::announcer::Line::parse(l).is_some_and(|l| l.is_combo());
        let end = |l: &str| {
            ddi_library::announcer::Line::parse(l)
                .is_some_and(ddi_session::announcer::battle::end_line)
        };
        assert!(
            heard.spoken.iter().all(|l| l.starts_with("battle ")
                || l == "gameplay here we go normal"
                || combo(l)
                || end(l)),
            "{key}: {:?}",
            heard.spoken
        );
        // The autoplay's full combo, then the outcome: no cleared (the
        // outcome's line follows), no failed.
        let n = heard.spoken.len();
        assert_eq!(heard.spoken[n - 2], "gameplay full combo w1", "{key}: {:?}", heard.spoken);
        assert_eq!(said("gameplay cleared") + said("gameplay failed"), 0, "{key}: {:?}", heard.spoken);
        assert!(heard.spoken.iter().any(|l| combo(l)), "{key}: a combo line: {:?}", heard.spoken);
        // The player, autoplayed, steps up: each said by its line.
        assert!(said("battle step up") > 0, "{key}: {:?}", heard.spoken);
        // Each attack moment said or shown, the said ones by their line.
        for line in ["battle attack incoming", "battle attack launched"] {
            let told = heard
                .attacks
                .iter()
                .filter(|t| t.line == line && t.said)
                .count();
            assert_eq!(said(line), told, "{key}: {line}");
        }
        assert!(
            heard
                .attacks
                .iter()
                .all(|t| t.said || t.line == "battle attack launched"),
            "{key}: an attack against the player not said: {:?}",
            heard.attacks
        );
        // The battle's moments in play were said too: nothing shown.
        assert!(
            heard
                .moments
                .iter()
                .any(|t| t.line == "battle match begin" && t.said),
            "{key}: {:?}",
            heard.moments
        );
    }
}
