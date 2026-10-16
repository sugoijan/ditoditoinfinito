//! A play's results as the results screen shows them
//! (`docs/plans/ui-core.md`, step 6.1): the engine's [`Results`] and the
//! ruleset it was played by, made [`ResultsData`]. Counts come from the
//! ruleset's tally (a per-row ruleset, DDR A, counts a jump once); the
//! panels from the timeline (every note). Also the mods line the results
//! and the song list share.

use ddi_chart::{Glyph, Layout};
use ddi_engine::judge::JudgeEventKind;
use ddi_engine::rules::FullCombo;
use ddi_engine::{
    Appearance, Judgement, Results, Ruleset, ScrollAction, TimingCut, TransformOptions, Turn,
};

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

/// The colour names of the tiers, best first.
const TIER_COLORS: [&str; 6] = [
    "Judgement.W1",
    "Judgement.W2",
    "Judgement.W3",
    "Judgement.W4",
    "Judgement.W5",
    "Judgement.Miss",
];

/// The panel sets every ruleset offers, its default first.
pub fn panel_sets(_ruleset: &Ruleset) -> Vec<PanelSet> {
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

/// The score as the ruleset gives it: money, else percent, else "—".
pub fn score_text(r: &Results) -> String {
    match (r.score.money, r.score.percent) {
        (Some(m), _) => format!("{m}"),
        (None, Some(p)) => format!("{:.2}%", p * 100.0),
        _ => "—".into(),
    }
}

/// The lamp: a full combo's kind, and "ASSIST" when notes were removed
/// or simplified.
pub fn lamp(r: &Results) -> Option<String> {
    let fc = match r.full_combo {
        FullCombo::None => "",
        FullCombo::FC => "FULL COMBO",
        FullCombo::GreatFC => "GREAT FULL COMBO",
        FullCombo::PerfectFC => "PERFECT FULL COMBO",
        FullCombo::MarvelousFC => "MARVELOUS FULL COMBO",
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
        .map(|j| JudgementCount {
            name: names.name(*j).to_string(),
            color: TIER_COLORS[j.index()].to_string(),
            count: r.tally.taps[j.index()],
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
    stats.push(("Fast / slow".into(), format!("{} / {}", r.fast, r.slow)));
    stats.push((
        "Mean offset".into(),
        format!(
            "{} ms (σ {:.1})",
            signed(r.mean_delta * 1000.0),
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
    let windows = Judgement::ALL[..5]
        .iter()
        .filter_map(|j| {
            let w = p.ruleset.judge.window(*j)?;
            Some(Window {
                early: (w.early * 1000.0) as f32,
                late: (w.late * 1000.0) as f32,
                color: TIER_COLORS[j.index()].to_string(),
            })
        })
        .collect();
    let mut notes = Vec::new();
    if p.autoplay {
        notes.push("Autoplay: this play does not count toward your bests.".to_string());
    }
    if p.device_changed {
        notes.push("An audio device or the display changed during play.".to_string());
    }
    notes.extend(p.video_notes.iter().cloned());
    let mut mods = p.mods.clone();
    mods.extend(mods_summary(
        &r.transform,
        r.appearance,
        r.scroll_action,
        Some((r.lane_map.as_slice(), p.layout)),
    ));
    let panel_sets = panel_sets(p.ruleset);
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
        ex: match (r.score.ex, r.score.max_ex) {
            (Some(ex), Some(max)) => Some(format!("EX {ex} / {max}")),
            _ => None,
        },
        failed: r.failed,
        // The autoplayer's lamp would mean nothing: it says it played.
        lamp: if p.autoplay {
            Some("AUTOPLAY".into())
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
    }
}

/// "+1.2", "−3.4": a real minus sign.
fn signed(v: f64) -> String {
    let s = format!("{:.1}", v.abs());
    if s.chars().all(|c| c == '0' || c == '.') {
        s
    } else if v < 0.0 {
        format!("−{s}")
    } else {
        format!("+{s}")
    }
}

/// The layout's lanes, named.
pub fn lanes(layout: &Layout) -> Vec<LaneName> {
    layout
        .lanes
        .iter()
        .map(|l| LaneName {
            symbol: lane_symbol(&l.glyph).to_string(),
            label: if l.label.is_empty() {
                let mut c = l.name.chars();
                c.next()
                    .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                    .unwrap_or_default()
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
        // Colours follow the tier, not the row.
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
    }
}
