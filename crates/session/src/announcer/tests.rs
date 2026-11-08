use super::*;

use ddi_engine::rules::presets;
use ddi_platform::{ClockSample, DeviceFingerprint, SoundId};

/// Plays nothing: a buffer is its length in seconds; the voice's starts
/// and gains are kept.
#[derive(Default)]
pub(super) struct Fake {
    pub(super) voiced: Vec<(f64, f64, f32)>,
}

impl AudioBackend for Fake {
    type Buffer = f64;
    type Error = String;
    fn sample_rate(&self) -> f64 {
        48_000.0
    }
    fn master_ramp(&mut self, _: Option<f32>, _: f32, _: Option<f64>, _: f64) {}
    fn stop_all(&mut self) {}
    fn stop_music(&mut self) {}
    fn running(&self) -> bool {
        true
    }
    fn buffer_from_pcm(&self, _: Vec<Vec<f32>>, _: f64) -> Result<f64, String> {
        Ok(0.0)
    }
    fn fingerprint(&self) -> Option<DeviceFingerprint> {
        None
    }
    fn now(&self) -> ClockSample {
        ClockSample::default()
    }
    fn play(&mut self, _: &f64, _: f64, _: f64, _: f32) -> Result<SoundId, String> {
        Ok(1)
    }
    fn duration(&self, b: &f64) -> f64 {
        *b
    }
    fn play_voice(&mut self, b: &f64, at: f64, volume: f32) -> Result<SoundId, String> {
        self.voiced.push((*b, at, volume));
        Ok(1)
    }
}

/// Song second `t` on the audio clock at `100 + t` (the music started
/// at context time 100).
const TIMING: Timing = Timing {
    start: 100.0,
    offset: 0.0,
    rate: 1.0,
};

/// A voice of `lines`, each one sound `len` seconds long.
fn voice(lines: &[(Line, f64)]) -> SharedVoice<f64> {
    let mut v = Voice::default();
    for &(line, len) in lines {
        v.expect([line]);
        v.add(line, len);
    }
    Rc::new(RefCell::new(v))
}

fn announcer(lines: &[(Line, f64)], first_note: f64, prior: Option<Prior>) -> Announcer<f64> {
    let mut a = Announcer::new(true, 0.5, prior, TIMING, Some(first_note));
    a.set_voice(voice(lines));
    a
}

/// The folders spoken, in order.
fn spoken(a: &Announcer<f64>) -> Vec<String> {
    a.report.spoken.iter().map(|(f, _)| f.clone()).collect()
}

/// The play at song second `t` (the audio clock with it), `dt` after the
/// last frame.
fn at(t: f64, dt: f64) -> Moment {
    Moment {
        now: TIMING.at(t),
        dt,
        song_time: t,
        life: 0.5,
        ..Moment::default()
    }
}

/// Frames every 1/60 s from song second `from` to `to`, `with` giving
/// each its state.
fn run(a: &mut Announcer<f64>, audio: &mut Fake, from: f64, to: f64, with: impl Fn(&mut Moment)) {
    let mut t = from;
    while t < to {
        let mut m = at(t, 1.0 / 60.0);
        with(&mut m);
        a.tick(audio, &m);
        t += 1.0 / 60.0;
    }
}

#[test]
fn bags_give_every_sound_once_before_any_again() {
    let mut bags = Bags::new(7);
    let line = Line::CommentGood;
    let mut seen = Vec::new();
    for _ in 0..3 {
        let take = bags.take("voice", line, 10, 4);
        assert!(take.iter().all(|&i| i < 10));
        let mut unique = take.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), take.len(), "{take:?}");
        seen.extend(take);
    }
    // Ten in the first two and a half takes: each once.
    let mut first: Vec<usize> = seen[..10].to_vec();
    first.sort();
    assert_eq!(first, (0..10).collect::<Vec<_>>());
    // Fewer sounds than asked for: each of them.
    let mut few = bags.take("voice", Line::Ready, 2, 4);
    few.sort();
    assert_eq!(few, [0, 1]);
    // Bags are an announcer's: another's line starts its own.
    assert_eq!(bags.take("other", line, 1, 4), [0]);
}

#[test]
fn a_play_decodes_the_lines_it_can_speak_a_few_sounds_each() {
    let mut kept = Kept {
        name: "Voice".into(),
        ..Kept::default()
    };
    let files = |dir: &str, n: usize| -> Vec<RelPathBuf> {
        (0..n).map(|i| format!("{dir}/{i}.ogg").into()).collect()
    };
    for (folder, n) in [
        ("gameplay ready", 3),
        ("gameplay comment good", 9),
        ("gameplay combo stopped", 5),
        ("gameplay 100 combo", 1),
        ("gameplay 500 combo", 1),
        ("gameplay combo overflow", 3),
        ("evaluation aa", 2),
        // Song select's: not a play's.
        ("select music intro", 2),
        ("gameplay restart", 2),
    ] {
        kept.lines.insert(folder.into(), files(folder, n));
    }
    let mut bags = Bags::new(1);
    let got: BTreeMap<String, usize> = wanted(&kept, 300, &mut bags)
        .into_iter()
        .map(|(line, files)| (line.folder(), files.len()))
        .collect();
    let expected: BTreeMap<String, usize> = [
        ("evaluation aa", 1),
        ("gameplay 100 combo", 1),
        ("gameplay combo stopped", 2),
        ("gameplay comment good", 4),
        ("gameplay ready", 1),
        ("gameplay restart", 1),
    ]
    .into_iter()
    .map(|(f, n)| (f.to_string(), n))
    .collect();
    assert_eq!(got, expected);
    // The opening's first, as they are said first.
    let order: Vec<Line> = wanted(&kept, 300, &mut bags)
        .into_iter()
        .map(|(line, _)| line)
        .collect();
    assert_eq!(&order[..2], [Line::Ready, Line::Restart]);
    // A chart past 1000: the overflow and every hundred.
    let long = wanted(&kept, 1500, &mut bags);
    assert!(long.iter().any(|(l, _)| *l == Line::ComboOverflow));
    assert!(long.iter().any(|(l, _)| *l == Line::Combo(500)));
    // Song select's: its intro, one sound, the next of its bag each time.
    let first = menu(&kept, &mut bags);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].0, Line::SelectMusicIntro);
    let second = menu(&kept, &mut bags);
    assert_ne!(first[0].1, second[0].1);
}

#[test]
fn a_voice_gives_each_lines_sounds_in_turn_and_follows_the_chains() {
    let mut v: Voice<f64> = Voice::default();
    v.add(Line::Ready, 1.0);
    v.add(Line::Ready, 2.0);
    assert_eq!(v.peek(Line::Ready), Some((Line::Ready, &1.0)));
    assert_eq!(v.next(Line::Ready), Some((Line::Ready, &1.0)));
    assert_eq!(v.next(Line::Restart), Some((Line::Ready, &2.0)));
    assert_eq!(v.next(Line::Ready), Some((Line::Ready, &1.0)));
    assert_eq!(v.next(Line::Cleared), None);
}

#[test]
fn a_voice_speaks_sounds_decoded_out_of_order_in_their_ranks() {
    let mut v: Voice<f64> = Voice::default();
    v.add_ranked(Line::Ready, 2, 3.0);
    v.add_ranked(Line::Ready, 0, 1.0);
    v.add_ranked(Line::Ready, 1, 2.0);
    let spoken: Vec<f64> = (0..4).map(|_| *v.next(Line::Ready).unwrap().1).collect();
    assert_eq!(spoken, [1.0, 2.0, 3.0, 1.0]);
}

#[test]
fn a_sound_decoded_late_does_not_repeat_the_one_spoken() {
    let mut v: Voice<f64> = Voice::default();
    v.add_ranked(Line::Ready, 1, 2.0);
    assert_eq!(v.next(Line::Ready), Some((Line::Ready, &2.0)));
    v.add_ranked(Line::Ready, 0, 1.0);
    assert_eq!(v.next(Line::Ready), Some((Line::Ready, &1.0)));
}

#[test]
fn ready_and_here_we_go_fit_the_lead_in_or_give_way() {
    let lines = [(Line::Ready, 1.0), (Line::HereWeGo, 1.5)];
    // Room for both: ready at once, here we go ending half a second
    // before the first note (song second 5, context time 105).
    let mut audio = Fake::default();
    let mut a = announcer(&lines, 5.0, None);
    a.tick(&mut audio, &at(-0.8, 0.0));
    assert_eq!(spoken(&a), ["gameplay ready", "gameplay here we go normal"]);
    let go = a.report.spoken[1].1;
    assert!((go + 1.5 - (105.0 - GO_BEFORE)).abs() < 1e-9, "{go}");
    assert_eq!(audio.voiced[0].2, 0.5);
    // Room for here we go alone.
    let mut a = announcer(&lines, 1.5, None);
    a.tick(&mut audio, &at(-0.8, 0.0));
    assert_eq!(spoken(&a), ["gameplay here we go normal"]);
    // None at all.
    let mut a = announcer(&lines, 0.5, None);
    a.tick(&mut audio, &at(-0.8, 0.0));
    assert!(spoken(&a).is_empty());
    // An announcer with no here we go: ready alone.
    let mut a = announcer(&[(Line::Ready, 1.0)], 5.0, None);
    a.tick(&mut audio, &at(-0.8, 0.0));
    assert_eq!(spoken(&a), ["gameplay ready"]);
    // Here we go decoded only later: still placed, after ready.
    let v = voice(&[(Line::Ready, 1.0)]);
    v.borrow_mut().expect([Line::HereWeGo]);
    let mut a = Announcer::new(true, 0.5, None, TIMING, Some(5.0));
    a.set_voice(v.clone());
    a.tick(&mut audio, &at(-0.8, 0.0));
    v.borrow_mut().add(Line::HereWeGo, 1.5);
    a.tick(&mut audio, &at(-0.7, 0.1));
    assert_eq!(spoken(&a), ["gameplay ready", "gameplay here we go normal"]);
}

#[test]
fn comments_come_every_ten_seconds_by_the_life() {
    let lines = [
        (Line::CommentHot, 1.0),
        (Line::CommentGood, 1.0),
        (Line::CommentDanger, 1.0),
    ];
    let mut audio = Fake::default();
    let mut a = announcer(&lines, 2.0, None);
    // Ten seconds from the play's start (5.1 counts the lead-in), none
    // before the first note; then one each ten seconds.
    run(&mut a, &mut audio, 0.0, 12.5, |_| {});
    assert_eq!(spoken(&a), ["gameplay comment good"]);
    let first = a.report.spoken[0].1;
    assert!((first - TIMING.at(10.0)).abs() < 0.05, "{first}");
    run(&mut a, &mut audio, 12.5, 22.5, |m| m.life = 1.0);
    run(&mut a, &mut audio, 22.5, 32.5, |m| {
        m.life = 0.1;
        m.danger = true;
    });
    // In danger without failing (fail off): good.
    run(&mut a, &mut audio, 32.5, 42.5, |m| m.life = 0.1);
    assert_eq!(
        spoken(&a),
        [
            "gameplay comment good",
            "gameplay comment hot",
            "gameplay comment danger",
            "gameplay comment good"
        ]
    );
    // A lead-in past ten seconds: the first comment as the first note
    // comes, none before.
    let mut a = announcer(&lines, 15.0, None);
    run(&mut a, &mut audio, 0.0, 14.9, |_| {});
    assert!(spoken(&a).is_empty());
    run(&mut a, &mut audio, 14.9, 15.2, |_| {});
    assert_eq!(spoken(&a), ["gameplay comment good"]);
    // On a battery gauge: `oni`, else the life's line.
    let mut a = announcer(&[(Line::CommentOni, 1.0)], 0.0, None);
    run(&mut a, &mut audio, 0.0, 10.5, |m| m.lives = Some(3));
    assert_eq!(spoken(&a), ["gameplay comment oni"]);
    let mut a = announcer(&lines, 0.0, None);
    run(&mut a, &mut audio, 0.0, 10.5, |m| m.lives = Some(3));
    assert_eq!(spoken(&a), ["gameplay comment good"]);
}

#[test]
fn a_comment_not_decoded_when_due_is_said_once_it_is() {
    let mut audio = Fake::default();
    let v = voice(&[]);
    v.borrow_mut().expect([Line::CommentGood]);
    let mut a = Announcer::new(true, 1.0, None, TIMING, Some(0.0));
    a.set_voice(v.clone());
    // Due from ten seconds, its sound not there yet: missing.
    run(&mut a, &mut audio, 0.0, 11.0, |m| m.life = 0.5);
    assert!(spoken(&a).is_empty());
    assert_eq!(a.report.missing, ["gameplay comment good"]);
    // Decoded: said at the next frame.
    v.borrow_mut().add(Line::CommentGood, 1.0);
    run(&mut a, &mut audio, 11.0, 11.05, |m| m.life = 0.5);
    assert_eq!(spoken(&a), ["gameplay comment good"]);
    let said = a.report.spoken[0].1;
    assert!((said - TIMING.at(11.0)).abs() < 0.05, "{said}");
}

#[test]
fn combos_speak_behind_the_shared_gate() {
    let lines = [
        (Line::Combo(100), 1.0),
        (Line::Combo(200), 1.0),
        (Line::ComboOverflow, 1.0),
        (Line::ComboStopped, 1.0),
        (Line::CommentGood, 1.0),
    ];
    let mut audio = Fake::default();
    let mut a = announcer(&lines, 0.0, None);
    run(&mut a, &mut audio, 0.0, 3.0, |m| m.combo = 99);
    run(&mut a, &mut audio, 3.0, 3.1, |m| m.combo = 100);
    // 200 within two seconds of it: dropped.
    run(&mut a, &mut audio, 3.1, 6.0, |m| m.combo = 200);
    // Broken from above 50: combo stopped, the gate open again.
    run(&mut a, &mut audio, 6.0, 6.1, |m| m.combo = 0);
    assert_eq!(spoken(&a), ["gameplay 100 combo", "gameplay combo stopped"]);
    assert_eq!(a.report.dropped, 1);
    // A comment ten seconds after the last gated line, not before.
    run(&mut a, &mut audio, 6.1, 15.5, |m| m.combo = 0);
    assert_eq!(spoken(&a).len(), 2);
    run(&mut a, &mut audio, 15.5, 16.5, |m| m.combo = 0);
    assert_eq!(spoken(&a)[2], "gameplay comment good");
    // Past 1000: the pack's hundred, else the overflow.
    let mut a = announcer(&lines, 0.0, None);
    run(&mut a, &mut audio, 0.0, 3.0, |m| m.combo = 1050);
    run(&mut a, &mut audio, 3.0, 3.1, |m| m.combo = 1100);
    assert_eq!(spoken(&a), ["gameplay combo overflow"]);
    // Broken from 40: nothing.
    let mut a = announcer(&lines, 0.0, None);
    run(&mut a, &mut audio, 0.0, 3.0, |m| m.combo = 40);
    run(&mut a, &mut audio, 3.0, 3.1, |m| m.combo = 0);
    assert!(spoken(&a).is_empty());
}

#[test]
fn the_end_is_cleared_or_failed_once() {
    let lines = [(Line::Cleared, 1.0), (Line::Failed, 1.0)];
    let mut audio = Fake::default();
    let mut a = announcer(&lines, 0.0, None);
    run(&mut a, &mut audio, 0.0, 1.0, |m| m.ended = true);
    assert_eq!(spoken(&a), ["gameplay cleared"]);
    let mut a = announcer(&lines, 0.0, None);
    run(&mut a, &mut audio, 0.0, 1.0, |m| {
        m.ended = true;
        m.failed = true;
    });
    assert_eq!(spoken(&a), ["gameplay failed"]);
}

#[test]
fn a_restart_opens_with_its_own_line_or_ready() {
    let mut audio = Fake::default();
    let mut a = announcer(&[(Line::Ready, 1.0), (Line::Restart, 1.2)], 5.0, None).restarted();
    a.tick(&mut audio, &at(-0.8, 0.0));
    assert_eq!(spoken(&a), ["gameplay restart"]);
    // A pack without it: ready.
    let mut a = announcer(&[(Line::Ready, 1.0)], 5.0, None).restarted();
    a.tick(&mut audio, &at(-0.8, 0.0));
    assert_eq!(spoken(&a), ["gameplay ready"]);
}

#[test]
fn a_full_combo_is_said_as_the_last_note_is_judged_then_cleared() {
    let lines = [
        (Line::FullComboW(1), 0.7),
        (Line::FullCombo, 0.9),
        (Line::Cleared, 1.0),
    ];
    let end = |a: &mut Announcer<f64>, audio: &mut Fake, full_combo: FullCombo| {
        run(a, audio, 0.0, 0.1, |m| {
            m.ended = true;
            m.full_combo = full_combo;
        });
    };
    let mut audio = Fake::default();
    // Its kind's line, cleared once it is said.
    let mut a = announcer(&lines, 0.0, None);
    end(&mut a, &mut audio, FullCombo::MarvelousFC);
    assert_eq!(spoken(&a), ["gameplay full combo w1", "gameplay cleared"]);
    let (fc, cleared) = (a.report.spoken[0].1, a.report.spoken[1].1);
    assert!((cleared - fc - 0.7).abs() < 1e-9);
    // A kind the pack lacks: the plain line; a plain full combo too.
    for kind in [FullCombo::PerfectFC, FullCombo::FC] {
        let mut a = announcer(&lines, 0.0, None);
        end(&mut a, &mut audio, kind);
        assert_eq!(spoken(&a), ["gameplay full combo", "gameplay cleared"]);
    }
    // None of them: cleared alone, at once.
    let mut a = announcer(&[(Line::Cleared, 1.0)], 0.0, None);
    end(&mut a, &mut audio, FullCombo::GreatFC);
    assert_eq!(spoken(&a), ["gameplay cleared"]);
    // No full combo: cleared.
    let mut a = announcer(&lines, 0.0, None);
    end(&mut a, &mut audio, FullCombo::None);
    assert_eq!(spoken(&a), ["gameplay cleared"]);
}

#[test]
fn a_battery_gauges_lives_lost_are_said_the_last_whatever_the_gate() {
    let lines = [
        (Line::LifeLost, 0.5),
        (Line::LastLife, 0.5),
        (Line::ComboStopped, 0.5),
        (Line::Failed, 0.5),
    ];
    let mut audio = Fake::default();
    let mut a = announcer(&lines, 0.0, None);
    run(&mut a, &mut audio, 0.0, 3.0, |m| {
        m.lives = Some(4);
        m.combo = 80;
    });
    // A life lost breaking the combo: its line, not `combo stopped`.
    run(&mut a, &mut audio, 3.0, 3.1, |m| m.lives = Some(3));
    // Another within two seconds: dropped; the last one left: said.
    run(&mut a, &mut audio, 3.1, 3.2, |m| m.lives = Some(2));
    run(&mut a, &mut audio, 3.2, 3.3, |m| m.lives = Some(1));
    // The one that fails the play: the fail's line.
    run(&mut a, &mut audio, 6.0, 6.1, |m| {
        m.lives = Some(0);
        m.failed = true;
        m.ended = true;
    });
    assert_eq!(
        spoken(&a),
        [
            "gameplay life lost",
            "gameplay last life",
            "gameplay failed"
        ]
    );
    assert_eq!(a.report.dropped, 1);
}

fn ending(grade: &str, full_combo: FullCombo, score: f64) -> Ending<'_> {
    Ending {
        quit: false,
        failed: false,
        grade,
        full_combo,
        score,
    }
}

#[test]
fn the_results_say_one_line_by_precedence_and_cheer() {
    let sm5 = presets::sm5().grades;
    let all = [
        (Line::Quit, 1.0),
        (Line::FirstClear, 1.0),
        (Line::NewRecord, 1.0),
        (Line::EvaluationFullCombo(1), 1.0),
        (Line::EvaluationFullCombo(4), 1.0),
        (Line::Grade(Grade::Aa), 1.0),
        (Line::Grade(Grade::B), 1.0),
        (Line::Grade(Grade::F), 1.0),
        (Line::Cheer, 1.0),
    ];
    let line = |prior: Option<Prior>, e: &Ending, w4: bool| {
        let mut audio = Fake::default();
        let mut a = announcer(&all, 0.0, prior);
        a.results(&mut audio, 200.0, e, &sm5, w4);
        spoken(&a)
    };
    let before = Prior {
        played: true,
        cleared: true,
        best: 0.9,
    };
    // A quit: its line alone.
    let quit = Ending {
        quit: true,
        ..ending("AA", FullCombo::None, 0.95)
    };
    assert_eq!(line(Some(before), &quit, false), ["evaluation quit"]);
    // The first clear, even of a record; then cheered.
    let first = Prior {
        played: true,
        cleared: false,
        best: 0.5,
    };
    assert_eq!(
        line(Some(first), &ending("AA", FullCombo::None, 0.95), false),
        ["evaluation first clear", "evaluation cheer"]
    );
    // A new best after an earlier play.
    assert_eq!(
        line(Some(before), &ending("B", FullCombo::None, 0.95), false),
        ["evaluation new record"]
    );
    // Not a best: the full combo, then the grade.
    assert_eq!(
        line(
            Some(before),
            &ending("AAAA", FullCombo::MarvelousFC, 0.8),
            false
        ),
        ["evaluation full combo w1", "evaluation cheer"]
    );
    assert_eq!(
        line(Some(before), &ending("B", FullCombo::None, 0.8), false),
        ["evaluation b"]
    );
    // A plain full combo is W4's only where the combo keeps on it.
    assert_eq!(
        line(Some(before), &ending("B", FullCombo::FC, 0.8), true),
        ["evaluation full combo w4"]
    );
    assert_eq!(
        line(Some(before), &ending("B", FullCombo::FC, 0.8), false),
        ["evaluation b"]
    );
    // A missing full combo line gives way to the grade (5.1 says nothing).
    assert_eq!(
        line(Some(before), &ending("AA", FullCombo::GreatFC, 0.8), false),
        ["evaluation aa", "evaluation cheer"]
    );
    // Failed: `e`, read as Etterna's `f`; no cheer.
    let failed = Ending {
        failed: true,
        ..ending("F", FullCombo::None, 0.3)
    };
    assert_eq!(line(Some(before), &failed, false), ["evaluation f"]);
    // A first play: a first clear, else the grade; its record only where
    // nothing else speaks.
    let none = Prior::default();
    assert_eq!(
        line(Some(none), &failed, false),
        ["evaluation f"],
        "a failed first play is no first clear"
    );
    let mut audio = Fake::default();
    let mut a = announcer(&[(Line::NewRecord, 1.0)], 0.0, Some(none));
    a.results(&mut audio, 200.0, &failed, &sm5, false);
    assert_eq!(spoken(&a), ["evaluation new record"]);
    // Autoplay or a replay (nothing known before): no clear, no record.
    assert_eq!(
        line(None, &ending("AA", FullCombo::None, 0.95), false),
        ["evaluation aa", "evaluation cheer"]
    );
    // The cheer comes 2.5 s after the line.
    let mut audio = Fake::default();
    let mut a = announcer(&all, 0.0, None);
    a.results(
        &mut audio,
        200.0,
        &ending("AA", FullCombo::None, 0.9),
        &sm5,
        false,
    );
    assert!((a.report.spoken[1].1 - a.report.spoken[0].1 - CHEER_DELAY).abs() < 1e-9);
}

#[test]
fn grades_become_the_old_seven_by_letter_or_by_place() {
    let g = |grades: &GradeTable, name: &str| old_grade(grades, name, false);
    // SM5's are the old seven.
    let sm5 = presets::sm5().grades;
    assert_eq!(g(&sm5, "AAAA"), Some(Grade::Aaaa));
    assert_eq!(g(&sm5, "D"), Some(Grade::D));
    assert_eq!(old_grade(&sm5, "F", true), Some(Grade::E));
    // DDR's by letter.
    let ddr = presets::ddr_a().grades;
    assert_eq!(g(&ddr, "AAA"), Some(Grade::Aaa));
    assert_eq!(g(&ddr, "AA-"), Some(Grade::Aa));
    assert_eq!(g(&ddr, "D+"), Some(Grade::D));
    // ITG's seventeen by place, rounded as 5.1 does.
    let itg = presets::itg().grades;
    let by_place: Vec<Option<Grade>> = [
        "★★★★",
        "★★★",
        "★★",
        "★",
        "S+",
        "S-",
        "A+",
        "B",
        "B-",
        "C-",
        "D",
    ]
    .iter()
    .map(|n| g(&itg, n))
    .collect();
    assert_eq!(
        by_place,
        [
            Some(Grade::Aaaa),
            Some(Grade::Aaaa),
            Some(Grade::Aaa),
            Some(Grade::Aaa),
            Some(Grade::Aa),
            Some(Grade::Aa),
            Some(Grade::A),
            Some(Grade::B),
            Some(Grade::C),
            Some(Grade::D),
            Some(Grade::D),
        ]
    );
    // Dancing☆Onigiri's eleven, AP first.
    let danoni = presets::danoni().grades;
    assert_eq!(g(&danoni, "AP"), Some(Grade::Aaaa));
    assert_eq!(g(&danoni, "SS"), Some(Grade::Aaa));
    assert_eq!(g(&danoni, "D"), Some(Grade::D));
    // FFR has none.
    assert_eq!(g(&presets::ffr().grades, ""), None);
}

#[test]
fn what_is_not_there_is_reported_and_an_announcer_off_says_nothing() {
    let mut audio = Fake::default();
    // Being decoded, not yet: missing; never there: absent.
    let v = voice(&[]);
    v.borrow_mut().expect([Line::Cleared]);
    let mut a = Announcer::new(true, 1.0, None, TIMING, Some(0.0));
    a.set_voice(v);
    run(&mut a, &mut audio, 0.0, 0.1, |m| m.ended = true);
    a.results(
        &mut audio,
        200.0,
        &ending("B", FullCombo::None, 0.5),
        &presets::sm5().grades,
        false,
    );
    assert_eq!(a.report.missing, ["gameplay cleared"]);
    assert_eq!(a.report.absent, ["gameplay ready", "evaluation b"]);
    // Off (calibration), or without a voice: nothing at all.
    let mut a = Announcer::new(false, 1.0, None, TIMING, Some(0.0));
    a.set_voice(voice(&[(Line::Cleared, 1.0)]));
    run(&mut a, &mut audio, 0.0, 0.1, |m| m.ended = true);
    let mut b: Announcer<f64> = Announcer::new(true, 1.0, None, TIMING, Some(0.0));
    run(&mut b, &mut audio, 0.0, 0.1, |m| m.ended = true);
    assert!(a.report == Report::default() && b.report == Report::default());
    assert!(audio.voiced.is_empty());
}

#[test]
fn a_menu_line_waits_for_its_sound_while_it_is_due() {
    let v: SharedVoice<f64> = Rc::new(RefCell::new(Voice::default()));
    let mut m = MenuVoice::new("bundled:x".into(), v.clone());
    // Not decoded yet: it waits.
    assert_eq!(m.ask(Line::SelectMusicIntro, 10.0), None);
    assert!(m.waiting());
    assert_eq!(m.poll(10.5), None);
    // Decoded while due: spoken once.
    v.borrow_mut().add(Line::SelectMusicIntro, 2.0);
    assert_eq!(m.poll(11.0), Some((Line::SelectMusicIntro, 2.0)));
    assert!(!m.waiting());
    assert_eq!(m.poll(11.1), None);
    // There at once: spoken, nothing waits.
    assert_eq!(
        m.ask(Line::SelectMusicIntro, 20.0),
        Some((Line::SelectMusicIntro, 2.0))
    );
    assert!(!m.waiting());
    // Decoded too late: dropped.
    let comment = Line::SelectMusic(ddi_library::announcer::Comment::New);
    assert_eq!(m.ask(comment, 30.0), None);
    v.borrow_mut().add(comment, 1.0);
    assert_eq!(m.poll(30.0 + MENU_WAIT + 0.01), None);
    assert!(!m.waiting());
    // A line asked for replaces the one waiting.
    let hard = Line::SelectMusic(ddi_library::announcer::Comment::Hard);
    assert_eq!(m.ask(hard, 40.0), None);
    assert_eq!(m.ask(comment, 40.5), Some((comment, 1.0)));
    assert!(!m.waiting());
    v.borrow_mut().add(hard, 1.0);
    assert_eq!(m.poll(41.0), None);
}
