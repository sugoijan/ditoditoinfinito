//! A finished play's results as both shells show them: the play's facts
//! as plain data in, the results' data, whether Retry shows, the song
//! offset's editor and the notes in their order out ([`plan`]); the room's
//! standings put beside a stage's ([`in_room`]). What a shell adds of its
//! own is its actions and its own notes ([`Finished::own_notes`]).

use ddi_chart::Layout;
use ddi_engine::{Results, Ruleset};

use crate::state::history::{Replay, ReplayCheck};
use crate::state::room::{SharedRoom, StageMark, StageOrder};

use super::data::{ResultsData, ResultsKind, SongEntry};
use super::play::{PlayInfo, hand_off_note, mark_replay, results_data, results_notes};
use super::results::{OffsetFn, ResultsScreen};

/// The solo CPU a play was (`docs/plans/cpu-player.md`), as its results
/// describe it ([`super::prompt::cpu_description_at`]).
#[derive(Clone, Copy, Debug)]
pub struct CpuShown {
    pub level: usize,
    pub absolute: Option<usize>,
    pub seed: u64,
    pub skills: ddi_engine::cpu::Skills,
}

/// A multiplayer stage's play: how it went for the player as the room
/// confirmed it, and whether a final attack's knockout drew it
/// (`SETTLEMENT_V1`).
#[derive(Clone, Debug, Default)]
pub struct StageShown {
    pub mark: Option<StageMark>,
    pub drawn_by_final_attack: bool,
}

/// A replayed play, or a kept play's results (`kind`, [`ResultsKind::Replay`]
/// or [`ResultsKind::Kept`]), with when it was played as its results say.
pub struct ReplayShown<'a> {
    pub replay: &'a Replay,
    pub kind: ResultsKind,
    pub when: String,
    /// The replay's results against the play's kept ones (none for a kept
    /// play's results, or a replay its viewer quit).
    pub check: Option<&'a ReplayCheck>,
}

/// What a finished play's results are made of, beside the engine's.
pub struct Finished<'a> {
    pub results: &'a Results,
    /// The ruleset played by.
    pub ruleset: &'a Ruleset,
    pub layout: &'a Layout,
    pub song: SongEntry,
    /// The chart ("Hard 13 · Single").
    pub chart: String,
    /// The song's id, for its offset's editor.
    pub song_id: String,
    /// The song's offset: as played, as set now (seconds).
    pub song_offset: (f64, f64),
    pub device_changed: bool,
    /// Autoplayed (as asked: a replay says whether its play was).
    pub auto: bool,
    /// The shell's own notes, first (desktop: the sound's dropouts).
    pub own_notes: Vec<String>,
    /// Why the song's scripts were stopped, as their note says it
    /// (`ddi_session::scripted::stop_note`).
    pub scripts_stopped: Option<String>,
    pub video_off: Option<ddi_platform::video::StopReason>,
    pub video_trouble: Option<&'a ddi_platform::video::VideoTrouble>,
    /// What the storage said when the play could not be kept.
    pub save_error: Option<&'a str>,
    /// The dancing hand was turned off for the rest of the play.
    pub hand_gone: bool,
    pub replay: Option<ReplayShown<'a>>,
    pub cpu: Option<CpuShown>,
    pub stage: Option<StageShown>,
}

/// A finished play's results as the screen is given them.
pub struct ResultsPlan {
    pub data: ResultsData,
    /// Retry shows: never on a stage's (a stage is played once) nor a kept
    /// play's.
    pub retry: bool,
    /// The song offset's editor's action: a song played for itself, not
    /// the CPU's, a stage's or a replay's.
    pub offset: Option<OffsetFn>,
}

/// The results of the play `f` ([`ResultsPlan`]). Its notes, in order:
/// the shell's own, the scripts stopped, the video's and the storage's
/// and a replay's ([`results_notes`]), the hand gone, the CPU described;
/// a stage's mark before them all, and a replay's when and a final
/// attack's draw before that.
pub fn plan(f: Finished) -> ResultsPlan {
    let mut notes = f.own_notes;
    notes.extend(f.scripts_stopped);
    notes.extend(results_notes(
        f.video_off,
        f.video_trouble,
        f.save_error,
        f.replay.as_ref().and_then(|r| r.check),
        f.replay.as_ref().map(|r| r.replay),
    ));
    if f.hand_gone {
        notes.push(hand_off_note());
    }
    if let Some(cpu) = &f.cpu {
        notes.push(super::prompt::cpu_description_at(
            cpu.level,
            cpu.absolute,
            cpu.seed,
            cpu.skills,
        ));
    }
    if let Some(mark) = f.stage.as_ref().and_then(|s| s.mark.as_ref()) {
        notes.insert(0, super::room::mark_text(mark));
    }
    // The offset only for a song played for itself (a replay's results
    // have none: `mark_replay`).
    let offset = f.cpu.is_none() && f.stage.is_none();
    let mut data = results_data(&PlayInfo {
        results: f.results,
        ruleset: f.ruleset,
        layout: f.layout,
        song: f.song,
        chart: f.chart,
        mods: Vec::new(),
        device_changed: f.device_changed,
        video_notes: notes,
        offset: offset.then_some(f.song_offset),
        autoplay: match &f.replay {
            Some(r) => r.replay.record.autoplay.is_some(),
            None => f.auto,
        },
    });
    if let Some(r) = &f.replay {
        mark_replay(&mut data, r.kind, &r.when);
    }
    if let Some(stage) = &f.stage {
        data.kind = ResultsKind::Stage;
        if stage.drawn_by_final_attack {
            data.notes
                .insert(0, ddi_l10n::msg::room::results_drawn_final_attack().into());
        }
    }
    ResultsPlan {
        retry: !matches!(data.kind, ResultsKind::Stage | ResultsKind::Kept),
        offset: (offset && f.replay.is_none())
            .then(|| super::play::song_offset_saver(f.song_id)),
        data,
    }
}

/// `results` of stage `order` (if it was one) with its room's standings
/// where the graphs are; a stage the player watched has no play of their
/// own.
pub fn in_room(results: ResultsScreen, stage: Option<(&StageOrder, SharedRoom)>) -> ResultsScreen {
    match stage {
        Some((order, room)) if order.watching => results.with_room(room, order.stage).watched(),
        Some((order, room)) => results.with_room(room, order.stage),
        None => results,
    }
}

#[cfg(test)]
mod tests {
    use ddi_engine::rules::{FullCombo, presets};
    use ddi_engine::{ScoreView, Tally};
    use ddi_l10n::msg::{results as rm, room as om};
    use ddi_platform::video::{StopReason, VideoError, VideoTrouble};
    use ddi_room::{Finish, ParticipantId, Placement, Points};

    use super::*;

    fn results() -> Results {
        Results {
            tally: Tally {
                taps: [30, 0, 0, 0, 0, 0],
                ..Tally::default()
            },
            max_combo: 30,
            score: ScoreView::default(),
            grade: "A".into(),
            full_combo: FullCombo::None,
            failed: false,
            quit: false,
            fast: 0,
            slow: 0,
            hits: 30,
            late_edges: 0,
            mean_delta: 0.0,
            stddev_delta: 0.0,
            transform: Default::default(),
            lane_map: vec![0, 1, 2, 3],
            seed: 1,
            appearance: Default::default(),
            cover: Default::default(),
            scroll_action: Default::default(),
            assist: false,
            timeline: Vec::new(),
            life: Vec::new(),
            gauge_failed_at: None,
            ddr: None,
            ffr: None,
            battle_mines: None,
            ddi: None,
            steps: Vec::new(),
        }
    }

    fn finished<'a>(r: &'a Results, rules: &'a Ruleset, layout: &'a Layout) -> Finished<'a> {
        Finished {
            results: r,
            ruleset: rules,
            layout,
            song: SongEntry::default(),
            chart: "Hard 13 · Single".into(),
            song_id: "song".into(),
            song_offset: (0.01, 0.02),
            device_changed: false,
            auto: false,
            own_notes: Vec::new(),
            scripts_stopped: None,
            video_off: None,
            video_trouble: None,
            save_error: None,
            hand_gone: false,
            replay: None,
            cpu: None,
            stage: None,
        }
    }

    fn mark() -> StageMark {
        StageMark {
            number: 2,
            placement: Placement {
                participant: ParticipantId(1),
                place: 1,
                finish: Finish::Survived,
                points: Points::whole(12),
            },
            field: 4,
        }
    }

    /// A song played for itself: Retry and the offset's editor, its
    /// notes in desktop's order (its own first, the scripts, the video's
    /// and the storage's, the hand).
    #[test]
    fn a_play_has_retry_the_offset_and_its_notes_in_order() {
        ddi_platform::storage::install(std::rc::Rc::new(
            ddi_platform::storage::MemoryStore::default(),
        ));
        let (r, rules, layout) = (results(), presets::itg(), Layout::builtin("dance-single").unwrap());
        let trouble = VideoTrouble {
            name: "bg.mp4".into(),
            why: VideoError::NotIncluded,
            count: 1,
            same: true,
        };
        let plan = plan(Finished {
            own_notes: vec!["own".into()],
            scripts_stopped: Some("scripts".into()),
            video_off: Some(StopReason::DroppedFrames),
            video_trouble: Some(&trouble),
            save_error: Some("the store is full"),
            hand_gone: true,
            ..finished(&r, &rules, &layout)
        });
        assert!(plan.retry && plan.offset.is_some());
        assert_eq!(plan.data.kind, ResultsKind::Play);
        assert!(plan.data.offset.is_some());
        let want: Vec<String> = vec![
            "own".into(),
            "scripts".into(),
            rm::note_video_dropped_frames().into(),
            crate::screens::failures::video_trouble_text(&trouble).into(),
            rm::note_not_saved(crate::screens::errors::error_text("the store is full")).into(),
            rm::note_hand_dropped_frames().into(),
        ];
        assert_eq!(plan.data.notes, want);
    }

    /// The solo CPU: described last, Retry, no offset.
    #[test]
    fn a_cpu_play_is_described_with_no_offset() {
        let (r, rules, layout) = (results(), presets::itg(), Layout::builtin("dance-single").unwrap());
        let plan = plan(Finished {
            hand_gone: true,
            cpu: Some(CpuShown {
                level: 2,
                absolute: Some(4),
                seed: 7,
                skills: Default::default(),
            }),
            ..finished(&r, &rules, &layout)
        });
        assert!(plan.retry && plan.offset.is_none() && plan.data.offset.is_none());
        let notes = &plan.data.notes;
        assert_eq!(notes[0], String::from(rm::note_hand_dropped_frames()));
        assert_eq!(
            notes[1],
            crate::screens::prompt::cpu_description_at(2, Some(4), 7, Default::default())
        );
        assert_eq!(notes.len(), 2);
    }

    /// A stage's: its kind, no Retry, no offset; a final attack's draw,
    /// then the room's mark, before the rest.
    #[test]
    fn a_stage_is_played_once_its_draw_and_mark_first() {
        let (r, rules, layout) = (results(), presets::itg(), Layout::builtin("dance-single").unwrap());
        let plan = plan(Finished {
            scripts_stopped: Some("scripts".into()),
            stage: Some(StageShown {
                mark: Some(mark()),
                drawn_by_final_attack: true,
            }),
            ..finished(&r, &rules, &layout)
        });
        assert_eq!(plan.data.kind, ResultsKind::Stage);
        assert!(!plan.retry && plan.offset.is_none() && plan.data.offset.is_none());
        let want: Vec<String> = vec![
            om::results_drawn_final_attack().into(),
            crate::screens::room::mark_text(&mark()),
            "scripts".into(),
        ];
        assert_eq!(plan.data.notes, want);
        // Without a mark yet, nor a draw: the rest alone.
        let plan = super::plan(Finished {
            stage: Some(StageShown::default()),
            ..finished(&r, &rules, &layout)
        });
        assert!(plan.data.notes.is_empty() && !plan.retry);
    }
}
