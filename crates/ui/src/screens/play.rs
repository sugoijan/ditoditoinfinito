//! A play's results as the results screen shows them
//! (`docs/plans/ui-core.md`, step 6.1): the engine's [`Results`] and the
//! ruleset it was played by, made [`ResultsData`]. Counts come from the
//! ruleset's tally (a per-row ruleset, DDR A, counts a jump once); the
//! panels from the timeline (every note). Also the mods line the results
//! and the song list share.

use ddi_chart::{Glyph, Layout};
use ddi_engine::judge::JudgeEventKind;
use ddi_engine::rules::FailType;
use ddi_engine::rules::ddr::flare_name;
use ddi_engine::{
    Appearance, Judgement, Results, Ruleset, ScrollAction, TimingCut, TransformOptions, Turn,
};

use crate::state::history::{Lamp, Replay, ReplayCheck};
use crate::state::settings::Settings;

use super::data::{
    Hit, JudgementCount, LaneName, PanelKind, PanelSet, ResultsData, SongEntry, SongOffset, Window,
};

/// Hits needed before the mean error is offered as the song's offset.
pub const MIN_HITS_FOR_SONG_OFFSET: u32 = 20;

/// What a play's results are shown with, besides the engine's.
pub struct PlayInfo<'a> {
    pub results: &'a Results,
    /// The ruleset played by (its names, windows and gauge).
    pub ruleset: &'a Ruleset,
    pub layout: &'a Layout,
    pub song: SongEntry,
    /// The chart ("Hard 13 · Single").
    pub chart: String,
    /// Mods the summary leaves out (the speed, "reverse"), first.
    pub mods: Vec<String>,
    pub device_changed: bool,
    /// What happened to the background videos, as lines.
    pub video_notes: Vec<String>,
    /// The song's offset: as played, as set now (seconds); `None`: no
    /// editor (not a song).
    pub offset: Option<(f64, f64)>,
    /// The panel set the player chose last.
    pub panels: String,
    /// Autoplayed (`&auto=1`, the song list's switch): never counted.
    pub autoplay: bool,
}

/// The color name of a Fantastic outside ITG FA+'s 15 ms window (Simply
/// Love's white Fantastic).
const W1_OUTSIDE_W0: &str = "Judgement.W1White";

/// The EX score as the ruleset keeps it: its points of the chart's most,
/// or Simply Love's percentage (ITG FA+).
pub fn ex_text(score: &ddi_engine::ScoreView) -> Option<String> {
    match (score.ex, score.max_ex, score.ex_percent) {
        (Some(ex), Some(max), _) => Some(format!("EX {ex} / {max}")),
        (_, _, Some(p)) => Some(format!("EX {:.2}%", p * 100.0)),
        _ => None,
    }
}

/// The color names of the tiers, best first.
const TIER_COLORS: [&str; 6] = [
    "Judgement.W1",
    "Judgement.W2",
    "Judgement.W3",
    "Judgement.W4",
    "Judgement.W5",
    "Judgement.Miss",
];

/// The panel sets every ruleset offers, its default first.
pub fn panel_sets() -> Vec<PanelSet> {
    let set = |name: &str, panels: &[PanelKind]| PanelSet {
        name: name.into(),
        panels: panels.to_vec(),
    };
    vec![
        set(
            "Overview",
            &[
                PanelKind::Timing,
                PanelKind::Life,
                PanelKind::Offsets,
                PanelKind::Lanes,
            ],
        ),
        set("Timing", &[PanelKind::Timing, PanelKind::Offsets]),
        set("Song", &[PanelKind::Life, PanelKind::Lanes]),
    ]
}

/// The options a play was played with, as the start prompt says them
/// ("×2 · reverse · mirror"; a constant speed "C400").
pub fn options_line(o: &ddi_engine::player::PlayOptions) -> String {
    let speed = match o.scroll.speed {
        ddi_engine::scroll::SpeedMod::XMod(x) => format!("×{}", (x * 100.0).round() / 100.0),
        ddi_engine::scroll::SpeedMod::CMod(c) => format!("C{}", c.round()),
    };
    let mut parts = vec![speed];
    if o.scroll.reverse {
        parts.push("reverse".into());
    }
    parts.extend(mods_summary(
        &o.transform,
        o.appearance,
        o.scroll.scroll_action,
        None,
    ));
    parts.join(" · ")
}

/// The score as the ruleset gives it: money, else percent, else "—".
pub fn score_text(r: &Results) -> String {
    match (r.score.money, r.score.percent) {
        (Some(m), _) => format!("{m}"),
        (None, Some(p)) => format!("{:.2}%", p * 100.0),
        _ => "—".into(),
    }
}

/// The lamp: a full combo's kind (LIFE4's single digit perfects and its
/// clear under DDR A's rules), and "ASSIST" when notes were removed or
/// simplified.
pub fn lamp(r: &Results) -> Option<String> {
    let fc = match Lamp::played(r) {
        Lamp::Failed | Lamp::Clear => "",
        Lamp::Life4Clear => "LIFE4 CLEAR",
        Lamp::FullCombo => "FULL COMBO",
        Lamp::GreatFullCombo => "GREAT FULL COMBO",
        Lamp::PerfectFullCombo => "PERFECT FULL COMBO",
        Lamp::SingleDigitPerfects => "SINGLE DIGIT PERFECTS",
        Lamp::MarvelousFullCombo => "MARVELOUS FULL COMBO",
    };
    match (fc.is_empty(), r.assist) {
        (true, false) => None,
        (true, true) => Some("ASSIST".into()),
        (false, true) => Some(format!("{fc} · ASSIST")),
        (false, false) => Some(fc.into()),
    }
}

/// The offset to offer from this play's mean error (seconds), when it is
/// worth offering: enough hits and a change of 2 ms or more from what was
/// played. Built on the offset played, so taking it twice does not add
/// the error twice. The screen offers it while it is not the offset set.
pub fn suggested_offset(r: &Results, played: f64) -> Option<f64> {
    let ms = |s: f64| (s * 1000.0).round() / 1000.0;
    let suggested = ms(played + r.mean_delta);
    (r.hits >= MIN_HITS_FOR_SONG_OFFSET && (suggested - played).abs() >= 0.002).then_some(suggested)
}

/// The results screen's data for a play.
pub fn results_data(p: &PlayInfo) -> ResultsData {
    let r = p.results;
    let names = &p.ruleset.names;
    // Tiers the ruleset has: named ("—" names none) and judged (a
    // disabled window counts nothing).
    let judgements = Judgement::ALL
        .iter()
        .filter(|j| {
            let name = names.name(**j).trim();
            !name.is_empty()
                && name != "—"
                && (j.is_miss()
                    || p.ruleset.judge.window(**j).is_some()
                    || r.tally.taps[j.index()] > 0)
        })
        .flat_map(|j| {
            let count = r.tally.taps[j.index()];
            match (&names.w0, j) {
                // FA+: the Fantastics inside the W0 window and the rest.
                (Some((blue, white)), Judgement::W1) => vec![
                    JudgementCount {
                        name: blue.clone(),
                        color: TIER_COLORS[0].to_string(),
                        count: r.tally.w0,
                    },
                    JudgementCount {
                        name: white.clone(),
                        color: W1_OUTSIDE_W0.to_string(),
                        count: count.saturating_sub(r.tally.w0),
                    },
                ],
                _ => vec![JudgementCount {
                    name: names.name(*j).to_string(),
                    color: TIER_COLORS[j.index()].to_string(),
                    count,
                }],
            }
        })
        .collect();
    let t = &r.tally;
    let mut stats = vec![(
        format!("{} / {}", names.held, names.let_go),
        format!("{} / {}", t.held, t.let_go),
    )];
    if t.checkpoint_hit + t.checkpoint_miss > 0 {
        stats.push((
            "Ticks hit / missed".into(),
            format!("{} / {}", t.checkpoint_hit, t.checkpoint_miss),
        ));
    }
    stats.push(("Mines hit".into(), t.mine_hit.to_string()));
    if t.boo > 0 {
        stats.push(("Stray presses".into(), t.boo.to_string()));
    }
    stats.push(("Max combo".into(), r.max_combo.to_string()));
    stats.extend(flare_stat(r));
    stats.push(("Fast / slow".into(), format!("{} / {}", r.fast, r.slow)));
    stats.push((
        "Mean offset".into(),
        format!(
            "{} ms (σ {:.1})",
            super::results::signed(r.mean_delta * 1000.0, 1),
            r.stddev_delta * 1000.0
        ),
    ));
    let hits = r
        .timeline
        .iter()
        .filter_map(|j| match j.kind {
            JudgeEventKind::Tap(t, delta) => Some(Hit {
                time: j.time,
                offset: (!t.is_miss()).then_some((delta * 1000.0) as f32),
                lane: j.lane,
            }),
            _ => None,
        })
        .collect();
    // FA+: the blue Fantastics' window first, the rest of the Fantastic
    // window white (the lanes panel counts what is short of the first).
    let judge = &p.ruleset.judge;
    let w0 = judge.w0.map(|w0| {
        let ms = (judge.scaled(w0) * 1000.0) as f32;
        Window {
            early: ms,
            late: ms,
            color: TIER_COLORS[0].to_string(),
        }
    });
    let windows = w0
        .into_iter()
        .chain(Judgement::ALL[..5].iter().filter_map(|j| {
            let w = judge.window(*j)?;
            Some(Window {
                early: (w.early * 1000.0) as f32,
                late: (w.late * 1000.0) as f32,
                color: if *j == Judgement::W1 && judge.w0.is_some() {
                    W1_OUTSIDE_W0
                } else {
                    TIER_COLORS[j.index()]
                }
                .to_string(),
            })
        }))
        .collect();
    let mut notes = Vec::new();
    if p.autoplay {
        notes.push("Autoplay: this play does not count toward your bests.".to_string());
    }
    if p.device_changed {
        notes.push("An audio device or the display changed during play.".to_string());
    }
    notes.extend(fail_note(r, p.ruleset));
    notes.extend(p.video_notes.iter().cloned());
    let mut mods = p.mods.clone();
    mods.extend(mods_summary(
        &r.transform,
        r.appearance,
        r.scroll_action,
        Some((r.lane_map.as_slice(), p.layout)),
    ));
    mods.extend(p.ruleset.stepmania.summary());
    let panel_sets = panel_sets();
    let panel_set = panel_sets
        .iter()
        .position(|s| s.name == p.panels)
        .unwrap_or(0);
    ResultsData {
        line: if p.autoplay {
            format!("{} · {} · AUTOPLAY", p.chart, p.ruleset.name)
        } else {
            format!("{} · {}", p.chart, p.ruleset.name)
        },
        mods: mods.join(" · "),
        grade: r.grade.clone(),
        score: score_text(r),
        ex: ex_text(&r.score),
        failed: r.failed,
        // The autoplayer's lamp would mean nothing: it says it played.
        lamp: if p.autoplay {
            Some(
                if r.assist {
                    "AUTOPLAY · ASSIST"
                } else {
                    "AUTOPLAY"
                }
                .into(),
            )
        } else {
            lamp(r)
        },
        judgements,
        stats,
        notes,
        hits,
        windows,
        life: r.life.iter().map(|l| (l[0], l[1])).collect(),
        danger: p.ruleset.gauge.danger_line(),
        lanes: lanes(p.layout),
        offset: p.offset.map(|(played, current)| SongOffset {
            played,
            current,
            suggested: suggested_offset(r, played),
        }),
        panel_sets,
        panel_set,
        note: String::new(),
        song: p.song.clone(),
        kind: Default::default(),
    }
}

/// A DDR A play's FLARE rank and FLARE SKILL points, as the results'
/// stats say them ("FLARE VII · 944 points"), where it kept a rank: none
/// failed, nor (points) assisted or of a chart level DDR has not.
pub fn flare_stat(r: &Results) -> Option<(String, String)> {
    let ddr = r.ddr.as_ref()?;
    let rank = ddr.flare?;
    let rank_text = (rank > 0).then(|| format!("FLARE {}", flare_name(rank)));
    let value = match (rank_text, ddr.points) {
        (Some(rank), Some(points)) => format!("{rank} · {points} points"),
        (Some(rank), None) => rank,
        (None, Some(points)) => format!("{points} points"),
        (None, None) => return None,
    };
    Some(("Flare skill".into(), value))
}

/// What the fail type made of an empty life bar, where it changed the
/// outcome: a play failed at once ends there, as it always did; one
/// played on, or not failed, says when the bar emptied.
pub fn fail_note(r: &Results, ruleset: &Ruleset) -> Option<String> {
    let at = clock(r.gauge_failed_at?);
    Some(match ruleset.stepmania.fail {
        FailType::Immediate => return None,
        FailType::ImmediateContinue => {
            format!("Failed at {at}: the life bar emptied, and the play went on (fail delayed).")
        }
        FailType::EndOfSong if r.failed => {
            format!("Failed at the end: the life bar emptied at {at} and was still empty.")
        }
        FailType::EndOfSong => {
            format!("The life bar emptied at {at} and came back: not failed (fail at end).")
        }
        FailType::Off => format!("The life bar emptied at {at}: not failed (fail off)."),
    })
}

/// A song second as `m:ss`.
fn clock(seconds: f32) -> String {
    let s = seconds.max(0.0).floor() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Results of a replay or a kept play: marked so, with when it was
/// played, and no song offset editor (they change nothing).
pub fn mark_replay(data: &mut ResultsData, kind: super::data::ResultsKind, when: &str) {
    data.kind = kind;
    data.offset = None;
    data.line = format!("{} · REPLAY", data.line);
    if !when.is_empty() {
        data.notes.insert(0, format!("{when}."));
    }
}

/// The notes under a play's results, in order: why its video was turned
/// off, a movie's trouble, why the play could not be saved, how a replay
/// judged differently (`check`) and that a kept play (`replay`) was
/// judged by an older version.
pub fn results_notes(
    video_off: Option<ddi_platform::video::StopReason>,
    video_trouble: Option<&str>,
    save_error: Option<&str>,
    check: Option<&ReplayCheck>,
    replay: Option<&Replay>,
) -> Vec<String> {
    let mut notes = Vec::new();
    if let Some(reason) = video_off {
        let why = match reason {
            ddi_platform::video::StopReason::DroppedFrames => "frames were being dropped",
            ddi_platform::video::StopReason::DecoderBehind => "the decoder could not keep up",
        };
        notes.push(format!(
            "Video was turned off for this song: {why} (Options)."
        ));
    }
    if let Some(why) = video_trouble {
        notes.push(format!("{why}; the song background showed."));
    }
    if let Some(e) = save_error {
        notes.push(format!("This play could not be saved: {e}."));
    }
    notes.extend(check.and_then(ReplayCheck::note));
    notes.extend(replay.and_then(Replay::older_judge_note));
    notes
}

/// The results' song offset editor's action: song `id`'s offset saved
/// in the settings.
pub fn song_offset_saver(id: String) -> super::results::OffsetFn {
    std::rc::Rc::new(move |seconds| {
        let mut settings = Settings::load();
        settings.set_song_offset(&id, seconds);
        settings.save();
    })
}

/// The layout's lanes, named.
pub fn lanes(layout: &Layout) -> Vec<LaneName> {
    layout
        .lanes
        .iter()
        .map(|l| LaneName {
            symbol: lane_symbol(&l.glyph).to_string(),
            label: if l.label.is_empty() {
                super::capitalized(&l.name)
            } else {
                l.label.clone()
            },
        })
        .collect()
}

/// One label per option that differs from the default, empty when none
/// does. With `lane_map` (`take_from[new_lane] = old_lane`) and the layout
/// played, a shuffle also shows the resulting order: lane `i` shows the
/// symbol of old lane `take_from[i]`.
pub fn mods_summary(
    transform: &TransformOptions,
    appearance: Appearance,
    scroll_action: ScrollAction,
    lane_map: Option<(&[u8], &Layout)>,
) -> Vec<String> {
    let mut out = Vec::new();
    match transform.turn {
        Turn::Off => {}
        Turn::Mirror => out.push("mirror".into()),
        Turn::Left => out.push("left".into()),
        Turn::Right => out.push("right".into()),
        Turn::Shuffle => out.push(match lane_map.and_then(shuffle_order) {
            Some(order) => format!("shuffle {order}"),
            None => "shuffle".into(),
        }),
    }
    match appearance {
        Appearance::Visible => {}
        Appearance::Hidden => out.push("hidden".into()),
        Appearance::Sudden => out.push("sudden".into()),
        Appearance::HiddenSudden => out.push("hidden + sudden".into()),
        Appearance::Stealth => out.push("stealth".into()),
    }
    match scroll_action {
        ScrollAction::Normal => {}
        ScrollAction::Boost => out.push("boost".into()),
        ScrollAction::Brake => out.push("brake".into()),
        ScrollAction::Wave => out.push("wave".into()),
    }
    match transform.cut {
        TimingCut::Off => {}
        TimingCut::Quarters => out.push("quarter notes".into()),
        TimingCut::Eighths => out.push("quarters and eighths".into()),
    }
    if transform.no_jumps {
        out.push("no jumps".into());
    }
    if transform.no_holds {
        out.push("no holds".into());
    }
    out
}

/// The lanes' symbols after the lane map, a space between pads, or `None`
/// when the map does not fit the layout.
fn shuffle_order((take_from, layout): (&[u8], &Layout)) -> Option<String> {
    if take_from.len() != layout.lane_count() {
        return None;
    }
    let mut out = String::new();
    for (new, &old) in take_from.iter().enumerate() {
        if new > 0 && layout.lanes[new].pad != layout.lanes[new - 1].pad {
            out.push(' ');
        }
        out.push_str(lane_symbol(&layout.lanes.get(usize::from(old))?.glyph));
    }
    Some(out)
}

/// A one-character symbol for a lane.
pub fn lane_symbol(glyph: &Glyph) -> &'static str {
    match glyph {
        Glyph::Arrow { rotation_deg } => {
            // 0 = left, 90 = up, clockwise in 45° steps.
            const ARROWS: [&str; 8] = ["←", "↖", "↑", "↗", "→", "↘", "↓", "↙"];
            let step = (rotation_deg / 45.0).round() as i64;
            ARROWS[step.rem_euclid(8) as usize]
        }
        Glyph::Center => "■",
        Glyph::Onigiri => "◉",
        _ => "●",
    }
}

#[cfg(test)]
mod tests {
    use ddi_chart::Layout;
    use ddi_engine::judge::JudgeEventKind;
    use ddi_engine::player::Judged;
    use ddi_engine::rules::{FullCombo, presets};
    use ddi_engine::{Judgement, Results, ScoreView, Tally};

    use super::*;

    fn results(taps: [u32; 6]) -> Results {
        Results {
            tally: Tally {
                taps,
                held: 3,
                let_go: 1,
                mine_hit: 2,
                ..Tally::default()
            },
            max_combo: 40,
            score: ScoreView {
                percent: Some(0.9677),
                ..ScoreView::default()
            },
            grade: "A".into(),
            full_combo: FullCombo::None,
            failed: false,
            fast: 4,
            slow: 5,
            hits: 30,
            late_edges: 0,
            mean_delta: -0.0021,
            stddev_delta: 0.0098,
            transform: Default::default(),
            lane_map: vec![0, 1, 2, 3],
            seed: 1,
            appearance: Default::default(),
            scroll_action: Default::default(),
            assist: false,
            timeline: Vec::new(),
            life: vec![[0.0, 0.5], [10.0, 0.6]],
            gauge_failed_at: None,
            ddr: None,
        }
    }

    fn info<'a>(r: &'a Results, rules: &'a Ruleset, layout: &'a Layout) -> PlayInfo<'a> {
        PlayInfo {
            results: r,
            ruleset: rules,
            layout,
            song: SongEntry::default(),
            chart: "Hard 13 · Single".into(),
            mods: vec!["×2.5".into()],
            device_changed: false,
            video_notes: Vec::new(),
            offset: Some((0.0, 0.0)),
            panels: "Timing".into(),
            autoplay: false,
        }
    }

    fn single() -> Layout {
        Layout::builtin("dance-single").unwrap()
    }

    fn names(d: &ResultsData) -> Vec<&str> {
        d.judgements.iter().map(|j| j.name.as_str()).collect()
    }

    #[test]
    fn each_ruleset_names_its_tiers_and_holds() {
        let r = results([10, 2, 1, 0, 0, 1]);
        let layout = single();
        let itg = presets::itg();
        let d = results_data(&info(&r, &itg, &layout));
        assert_eq!(
            names(&d),
            [
                "Fantastic",
                "Excellent",
                "Great",
                "Decent",
                "Way Off",
                "Miss"
            ]
        );
        assert_eq!(d.stats[0], ("Held / Let Go".into(), "3 / 1".into()));
        assert_eq!(d.line, format!("Hard 13 · Single · {}", itg.name));
        let sm5 = presets::sm5();
        let d = results_data(&info(&r, &sm5, &layout));
        assert_eq!(
            names(&d),
            ["Flawless", "Perfect", "Great", "Good", "Boo", "Miss"]
        );
        assert_eq!(d.stats[0].0, "OK / NG");
        // DDR A and Dancing☆Onigiri have no fifth tier.
        let ddr = presets::ddr_a();
        let d = results_data(&info(&r, &ddr, &layout));
        assert_eq!(names(&d), ["Marvelous", "Perfect", "Great", "Good", "Miss"]);
        assert_eq!(d.stats[0].0, "O.K. / N.G.");
        let danoni = presets::danoni();
        let d = results_data(&info(&r, &danoni, &layout));
        assert_eq!(names(&d), ["Perfect", "Great", "Good", "Bad", "Miss"]);
        // Colors follow the tier, not the row.
        assert_eq!(d.judgements[4].color, "Judgement.Miss");
        // Windows for the histogram, as the ruleset scales them.
        let d = results_data(&info(&r, &itg, &layout));
        assert_eq!(d.windows.len(), 5);
        // 21.5 ms and StepMania's TimingWindowAdd of 1.5.
        assert!(
            (d.windows[0].late - 23.0).abs() < 0.01,
            "{:?}",
            d.windows[0]
        );
        assert_eq!(d.panel_sets[d.panel_set].name, "Timing");
        assert_eq!(d.danger, itg.gauge.danger_line());
    }

    #[test]
    fn stats_say_only_what_happened() {
        let mut r = results([10, 0, 0, 0, 0, 0]);
        let layout = single();
        let itg = presets::itg();
        let labels = |d: &ResultsData| d.stats.iter().map(|s| s.0.clone()).collect::<Vec<_>>();
        let d = results_data(&info(&r, &itg, &layout));
        assert!(!labels(&d).iter().any(|l| l.starts_with("Hold ticks")));
        assert!(!labels(&d).iter().any(|l| l == "Stray presses"));
        assert_eq!(d.stats.last().unwrap().1, "−2.1 ms (σ 9.8)");
        r.tally.checkpoint_hit = 7;
        r.tally.checkpoint_miss = 1;
        r.tally.boo = 3;
        let d = results_data(&info(&r, &itg, &layout));
        assert!(
            d.stats
                .contains(&("Ticks hit / missed".into(), "7 / 1".into()))
        );
        assert!(d.stats.contains(&("Stray presses".into(), "3".into())));
        assert!(d.stats.contains(&("Fast / slow".into(), "4 / 5".into())));
    }

    #[test]
    fn scores_lamps_and_failing() {
        let mut r = results([10, 0, 0, 0, 0, 0]);
        let layout = single();
        let itg = presets::itg();
        let d = results_data(&info(&r, &itg, &layout));
        assert_eq!(d.score, "96.77%");
        assert_eq!(d.ex, None);
        assert_eq!(d.lamp, None);
        assert!(!d.failed);
        r.score = ScoreView {
            money: Some(987_650),
            ex: Some(1234),
            max_ex: Some(1400),
            ..ScoreView::default()
        };
        r.full_combo = FullCombo::PerfectFC;
        r.failed = true;
        let d = results_data(&info(&r, &itg, &layout));
        assert_eq!(d.score, "987650");
        assert_eq!(d.ex.as_deref(), Some("EX 1234 / 1400"));
        assert_eq!(d.lamp.as_deref(), Some("PERFECT FULL COMBO"));
        assert!(d.failed);
        r.assist = true;
        assert_eq!(lamp(&r).as_deref(), Some("PERFECT FULL COMBO · ASSIST"));
        r.full_combo = FullCombo::None;
        assert_eq!(lamp(&r).as_deref(), Some("ASSIST"));
        r.score = ScoreView::default();
        assert_eq!(score_text(&r), "—");
    }

    #[test]
    fn counts_from_the_tally_panels_from_the_timeline() {
        // DDR A counts a jump once; the timeline keeps both notes.
        let mut r = results([1, 0, 0, 0, 0, 1]);
        let tap = |time: f32, lane: u8, j: Judgement, delta: f64| Judged {
            time,
            lane,
            kind: JudgeEventKind::Tap(j, delta),
        };
        r.timeline = vec![
            tap(1.0, 0, Judgement::W1, -0.004),
            tap(1.0, 3, Judgement::W1, 0.006),
            tap(2.0, 1, Judgement::Miss, 0.2),
            Judged {
                time: 3.0,
                lane: 2,
                kind: JudgeEventKind::Held,
            },
        ];
        let layout = single();
        let ddr = presets::ddr_a();
        let d = results_data(&info(&r, &ddr, &layout));
        assert_eq!(d.judgements[0].count, 1);
        assert_eq!(d.hits.len(), 3);
        assert_eq!(d.hits[1].lane, 3);
        assert!((d.hits[0].offset.unwrap() + 4.0).abs() < 1e-3);
        assert_eq!(d.hits[2].offset, None);
        assert_eq!(d.life, [(0.0, 0.5), (10.0, 0.6)]);
    }

    #[test]
    fn lanes_of_every_layout() {
        let lanes = |id: &str| lanes(&Layout::builtin(id).unwrap());
        let single = lanes("dance-single");
        assert_eq!(single.len(), 4);
        assert_eq!(single[0].symbol, "←");
        assert_eq!(single[3].symbol, "→");
        assert_eq!(lanes("pump-single").len(), 5);
        assert_eq!(lanes("pump-single")[2].symbol, "■");
        assert_eq!(lanes("dance-double").len(), 8);
        assert_eq!(lanes("pump-double").len(), 10);
        assert_eq!(lanes("danoni-7").len(), 7);
        assert!(lanes("pump-single").iter().all(|l| !l.label.is_empty()));
    }

    #[test]
    fn the_offset_offered_builds_on_what_was_played() {
        let r = results([10, 0, 0, 0, 0, 0]);
        // −2.1 ms over a play at +5 ms: +2.9 ms (rounded to +3).
        assert_eq!(suggested_offset(&r, 0.005), Some(0.003));
        // Too few hits.
        let mut few = r.clone();
        few.hits = 5;
        assert_eq!(suggested_offset(&few, 0.0), None);
        // Less than 2 ms off: not worth it.
        let mut close = r.clone();
        close.mean_delta = 0.001;
        assert_eq!(suggested_offset(&close, 0.0), None);
    }

    #[test]
    fn an_autoplayed_play_says_so() {
        let rules = ddi_engine::rules::presets::itg();
        let layout = single();
        let r = results([10, 0, 0, 0, 0, 0]);
        let mut p = info(&r, &rules, &layout);
        p.autoplay = true;
        let d = results_data(&p);
        assert_eq!(d.lamp.as_deref(), Some("AUTOPLAY"));
        assert!(d.line.ends_with("· AUTOPLAY"), "{}", d.line);
        assert!(d.notes[0].starts_with("Autoplay"));
        let mut assisted = r.clone();
        assisted.assist = true;
        let mut p = info(&assisted, &rules, &layout);
        p.autoplay = true;
        assert_eq!(results_data(&p).lamp.as_deref(), Some("AUTOPLAY · ASSIST"));
    }

    #[test]
    fn fa_plus_shows_blue_and_white_fantastics_and_its_ex() {
        let rules = ddi_engine::rules::presets::itg_fa_plus();
        let layout = single();
        let mut r = results([10, 2, 0, 0, 0, 1]);
        r.tally.w0 = 7;
        r.score.ex_percent = Some(0.9107);
        let d = results_data(&info(&r, &rules, &layout));
        let rows: Vec<(&str, u32, &str)> = d
            .judgements
            .iter()
            .map(|j| (j.name.as_str(), j.count, j.color.as_str()))
            .collect();
        assert_eq!(
            rows[..3],
            [
                ("Blue Fantastic", 7, "Judgement.W1"),
                ("White Fantastic", 3, "Judgement.W1White"),
                ("Excellent", 2, "Judgement.W2"),
            ]
        );
        assert_eq!(d.ex.as_deref(), Some("EX 91.07%"));
        // The timing panel's bands: blue within 15 ms, white to 23.
        let bands: Vec<(i32, &str)> = d.windows[..2]
            .iter()
            .map(|w| ((w.late * 10.0).round() as i32, w.color.as_str()))
            .collect();
        assert_eq!(bands, [(150, "Judgement.W1"), (230, "Judgement.W1White")]);
        assert!(d.line.contains("ITG FA+ (Simply Love)"), "{}", d.line);
        // ITG has no split.
        let itg = ddi_engine::rules::presets::itg();
        let d = results_data(&info(&r, &itg, &layout));
        assert_eq!(d.judgements[0].name, "Fantastic");
        assert_eq!(d.judgements[0].count, 10);
    }

    #[test]
    fn the_fail_type_shows_where_it_changed_the_outcome() {
        use ddi_engine::rules::{FailType, StepManiaOptions};
        let layout = single();
        let rules = |fail| {
            ddi_engine::rules::presets::itg().with_stepmania(&StepManiaOptions {
                fail,
                life: 6,
                judge: 9,
                ..Default::default()
            })
        };
        let mut r = results([10, 0, 0, 0, 0, 0]);
        // The bar never emptied: nothing to say, the options in the mods.
        let off = rules(FailType::Off);
        let d = results_data(&info(&r, &off, &layout));
        assert!(d.notes.is_empty(), "{:?}", d.notes);
        assert!(
            d.mods.ends_with("· fail off · life 6 · judge Justice"),
            "{}",
            d.mods
        );
        r.gauge_failed_at = Some(83.4);
        let note = |rules: &Ruleset, r: &Results| results_data(&info(r, rules, &layout)).notes;
        assert_eq!(
            note(&off, &r),
            vec!["The life bar emptied at 1:23: not failed (fail off).".to_string()]
        );
        assert!(note(&rules(FailType::Immediate), &r).is_empty());
        assert!(note(&rules(FailType::EndOfSong), &r)[0].contains("came back"));
        r.failed = true;
        assert!(note(&rules(FailType::EndOfSong), &r)[0].starts_with("Failed at the end"));
        assert!(note(&rules(FailType::ImmediateContinue), &r)[0].starts_with("Failed at 1:23"));
    }

    #[test]
    fn ddr_a_shows_its_gauge_flare_and_life4_lamps() {
        use ddi_engine::rules::StepManiaOptions;
        use ddi_engine::rules::ddr::{DdrClear, DdrGauge};
        let layout = single();
        let flare = presets::ddr_a().with_stepmania(&StepManiaOptions {
            gauge: DdrGauge::Flare(7),
            ..StepManiaOptions::default()
        });
        let mut r = results([10, 2, 1, 0, 0, 0]);
        r.full_combo = FullCombo::GreatFC;
        r.ddr = Some(DdrClear {
            gauge: DdrGauge::Flare(7),
            flare: Some(7),
            points: Some(944),
        });
        let d = results_data(&info(&r, &flare, &layout));
        assert!(
            d.stats
                .contains(&("Flare skill".into(), "FLARE VII · 944 points".into())),
            "{:?}",
            d.stats
        );
        assert!(d.mods.ends_with("FLARE VII"), "{}", d.mods);
        assert_eq!(d.lamp.as_deref(), Some("GREAT FULL COMBO"));
        // Under NORMAL, the points alone; failed, nothing.
        r.ddr = Some(DdrClear {
            gauge: DdrGauge::Normal,
            flare: Some(0),
            points: Some(665),
        });
        assert_eq!(flare_stat(&r).unwrap().1, "665 points");
        r.ddr = Some(DdrClear {
            gauge: DdrGauge::Flare(9),
            flare: None,
            points: None,
        });
        assert_eq!(flare_stat(&r), None);
        // A LIFE4 clear, and a perfect full combo of 2 Perfects: single
        // digit perfects (DDR A's rules only).
        let mut r = results([10, 2, 1, 0, 0, 1]);
        r.ddr = Some(DdrClear {
            gauge: DdrGauge::Life4,
            flare: Some(0),
            points: None,
        });
        assert_eq!(Lamp::of(&r), Lamp::Life4Clear);
        assert_eq!(lamp(&r).as_deref(), Some("LIFE4 CLEAR"));
        let mut r = results([10, 2, 0, 0, 0, 0]);
        r.full_combo = FullCombo::PerfectFC;
        r.ddr = Some(DdrClear::default());
        assert_eq!(lamp(&r).as_deref(), Some("SINGLE DIGIT PERFECTS"));
        r.tally.taps[1] = 10;
        assert_eq!(lamp(&r).as_deref(), Some("PERFECT FULL COMBO"));
        r.tally.taps[1] = 2;
        r.ddr = None;
        assert_eq!(Lamp::of(&r), Lamp::PerfectFullCombo);
        // Failed: the lamp kept is FAILED, the full combo still shown.
        r.failed = true;
        assert_eq!(Lamp::of(&r), Lamp::Failed);
        assert_eq!(lamp(&r).as_deref(), Some("PERFECT FULL COMBO"));
    }
}
