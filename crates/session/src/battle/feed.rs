//! An opponent's miniature field as the play shows it
//! (`docs/plans/multiplayer-ui.md`, "Faithful miniature fields";
//! `docs/plans/multiplayer-prototype-progress.md`, M6): the notes they
//! play scrolling to their receptors on this play's song clock, and what
//! a stream of their judged moments says of them, drawn a presentation
//! delay after each moment.
//!
//! The view is fed by moments ([`Moment`]: when, which note, the
//! judgment, the life and combo after it and whether it put them out), never by
//! the player behind them, so that a remote player's stream can feed the
//! same view later. A local CPU's moments come from its own judged play
//! as it is advanced ([`crate::battle::Battle`]), with no delay
//! ([`LOCAL_DELAY`]); a network's would come late, and its delay hides
//! that: a moment shows once the song is `delay` seconds past it.
//!
//! The notes are those of the chart's tap-only copy the CPUs play
//! (`ddi_engine::cpu::project`: holds and rolls as taps, mines, lifts
//! and fakes left out), never the chart itself ([`TapTrack`]). A note
//! leaves the field when a moment says it was hit; a missed one scrolls
//! on past the receptors, as a play shows it. Nothing is inferred from
//! a note reaching the receptors.
//!
//! A battle's shocks put into their play (M8b) come with the attack's
//! commitment ([`FieldFeed::add_hazards`]) and are drawn as the skin's
//! mines (`SpriteKind::Mine`, numbered from
//! `ddi_engine::battle::HAZARD_INDEX_BASE`); a moment saying one was
//! stepped on ([`MomentKind::ShockHit`]) takes it off the field and
//! flashes its receptor ([`ReceptorState::mine`]); one avoided scrolls on
//! past the receptors. Where the stage's rules lock a lane a shock is
//! stepped on in (DDI's, `JudgeTable::lock`), the same moment says the
//! lane is locked for the lock's time ([`FieldFeed::locks`]), which the
//! miniature shows.

use ddi_chart::{Quantization, Song, Tick, TimingMap};
use ddi_engine::battle::{HAZARD_INDEX_BASE, Hazard};
use ddi_engine::frame::{Frame, HoldJudgements, NoteSprite, ReceptorState, SpriteKind};
use ddi_engine::rules::Judgement;
use ddi_engine::scroll::{ScrollOptions, ScrollState};

/// A local opponent's presentation delay: none (its moments are judged
/// on this device, in step with the song).
pub const LOCAL_DELAY: f64 = 0.0;

/// How long a receptor shows a judgment after it (seconds): the hit
/// effect's time in a skin that draws its own, the flash's otherwise.
pub const FLASH_SECONDS: f32 = 0.4;

/// What a moment judged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MomentKind {
    /// A note of the copy, and its judgment.
    Tap(Judgement),
    /// A battle shock stepped on (M8b).
    ShockHit,
}

/// One judged moment of a player's run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moment {
    /// When it was judged: judged song seconds.
    pub at: f64,
    /// Which note of the copy (or which shock) it judged: its tick and
    /// its lane.
    pub tick: Tick,
    pub lane: u8,
    pub kind: MomentKind,
    /// The player's life after it, `0..=1`, their combo after it, and
    /// whether it put them out of the stage.
    pub life: f32,
    pub combo: u32,
    pub out: bool,
}

#[derive(Clone, Copy, Debug)]
struct TrackNote {
    seconds: f64,
    displayed: f64,
    tick: Tick,
    lane: u8,
    quantization: Quantization,
}

/// The chart's tap-only copy, as the miniatures draw it: its notes in
/// time order and its timing. One for all the opponents of a stage.
#[derive(Clone, Debug)]
pub struct TapTrack {
    timing: TimingMap,
    notes: Vec<TrackNote>,
    lanes: u8,
}

impl TapTrack {
    /// The copy of chart `chart` of `song` that the CPUs play; refused
    /// where they cannot play it.
    pub fn new(song: &Song, chart: usize) -> Result<TapTrack, String> {
        let copy = ddi_engine::cpu::project(song, chart)?;
        let c = &copy.charts[chart];
        let timing = c.timing(&copy).clone();
        let lanes = copy.layout_of(c).map_or(4, |l| l.lane_count()) as u8;
        let mut notes: Vec<TrackNote> = c
            .notes
            .iter()
            .filter(|n| timing.judgeable(n.tick))
            .map(|n| TrackNote {
                seconds: timing.seconds_at(n.tick),
                displayed: timing.displayed_beat(n.tick.beat()),
                tick: n.tick,
                lane: n.lane,
                quantization: n.tick.quantization(),
            })
            .collect();
        notes.sort_by(|a, b| a.seconds.total_cmp(&b.seconds).then(a.lane.cmp(&b.lane)));
        Ok(TapTrack {
            timing,
            notes,
            lanes,
        })
    }

    pub fn lanes(&self) -> u8 {
        self.lanes
    }

    /// The notes, in time order.
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// The note a moment judged.
    fn find(&self, tick: Tick, lane: u8) -> Option<usize> {
        let seconds = self.timing.seconds_at(tick);
        let from = self.notes.partition_point(|n| n.seconds < seconds - 1e-6);
        self.notes[from..]
            .iter()
            .take_while(|n| n.seconds <= seconds + 1e-6)
            .position(|n| n.tick == tick && n.lane == lane)
            .map(|i| from + i)
    }
}

/// What a field shows of its player at a moment: their life, their
/// combo and whether they are out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub life: f32,
    pub combo: u32,
    pub out: bool,
}

/// A battle shock in a player's field ([`FieldFeed::add_hazards`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FeedShock {
    pub tick: Tick,
    pub lane: u8,
    pub seconds: f64,
    displayed: f64,
    /// Drawn from this song second on (its attack's commitment plus the
    /// delay).
    pub shown_from: f64,
    /// When it leaves the field: stepped on (its moment plus the delay).
    pub gone: f64,
}

/// One player's stream of moments and what it says of their field.
#[derive(Clone, Debug)]
pub struct FieldFeed {
    moments: Vec<Moment>,
    /// For each note of the track: when it leaves the field (its hit's
    /// moment plus the delay), if it was hit.
    gone: Vec<f64>,
    /// The battle's shocks put into their play, in time order.
    shocks: Vec<FeedShock>,
    delay: f64,
    /// Life before any moment.
    life: f32,
    /// How long a lane a shock was stepped on in stays locked, seconds,
    /// where the stage's rules lock it.
    lock: Option<f64>,
}

impl FieldFeed {
    /// A field of `track` starting at `life`, its moments drawn `delay`
    /// seconds after they happened.
    pub fn new(track: &TapTrack, life: f32, delay: f64) -> FieldFeed {
        FieldFeed {
            moments: Vec::new(),
            gone: vec![f64::INFINITY; track.len()],
            shocks: Vec::new(),
            delay: delay.max(0.0),
            life,
            lock: None,
        }
    }

    /// The stage's rules lock a lane a shock is stepped on in for
    /// `seconds` (DDI's, `JudgeTable::lock`); `None`, they lock nothing.
    pub fn set_lock(&mut self, seconds: Option<f64>) {
        self.lock = seconds.filter(|s| *s > 0.0);
    }

    /// Each of `lanes` lanes' lock as shown at song second `t`: how far
    /// into it, `0..1`, where a shock stepped on there
    /// ([`MomentKind::ShockHit`]) locks it still; none where the rules
    /// lock nothing. (A locked lane steps on nothing, so a lane's lock is
    /// its last such moment's.)
    pub fn locks(&self, lanes: u8, t: f64) -> Vec<Option<f32>> {
        let mut out = vec![None; usize::from(lanes)];
        let Some(lock) = self.lock else {
            return out;
        };
        for m in self.shown_moments(t).iter().rev() {
            let since = t - (m.at + self.delay);
            if since >= lock {
                break;
            }
            if m.kind == MomentKind::ShockHit
                && let Some(l) = out.get_mut(usize::from(m.lane))
                && l.is_none()
            {
                *l = Some((since / lock).clamp(0.0, 1.0) as f32);
            }
        }
        out
    }

    pub fn delay(&self) -> f64 {
        self.delay
    }

    /// The moments received, in the order they happened.
    pub fn moments(&self) -> &[Moment] {
        &self.moments
    }

    /// The battle's shocks put into their play, in time order.
    pub fn shocks(&self) -> &[FeedShock] {
        &self.shocks
    }

    /// An attack's shocks `cells` put into their play at song second `at`
    /// (its commitment): drawn from the delay after it (on `track`'s
    /// timing), as they scroll in.
    pub fn add_hazards(&mut self, track: &TapTrack, cells: &[Hazard], at: f64) {
        for c in cells {
            let seconds = track.timing.seconds_at(c.tick);
            let shock = FeedShock {
                tick: c.tick,
                lane: c.lane,
                seconds,
                displayed: track.timing.displayed_beat(c.tick.beat()),
                shown_from: at + self.delay,
                gone: f64::INFINITY,
            };
            let i = self.shocks.partition_point(|s| s.seconds <= seconds);
            self.shocks.insert(i, shock);
        }
    }

    /// Takes in a moment of the stream (they come in the order they
    /// happened; one out of order is put in its place).
    pub fn push(&mut self, track: &TapTrack, m: Moment) {
        match m.kind {
            MomentKind::Tap(j) if !j.is_miss() => {
                if let Some(i) = track.find(m.tick, m.lane) {
                    self.gone[i] = self.gone[i].min(m.at + self.delay);
                }
            }
            MomentKind::Tap(_) => {}
            MomentKind::ShockHit => {
                if let Some(s) = self
                    .shocks
                    .iter_mut()
                    .find(|s| s.tick == m.tick && s.lane == m.lane)
                {
                    s.gone = s.gone.min(m.at + self.delay);
                }
            }
        }
        let at = self.moments.partition_point(|x| x.at <= m.at);
        self.moments.insert(at, m);
    }

    /// The moments shown at song second `t`.
    fn shown_moments(&self, t: f64) -> &[Moment] {
        let n = self.moments.partition_point(|m| m.at + self.delay <= t);
        &self.moments[..n]
    }

    /// Life and out as shown at song second `t`.
    pub fn shown(&self, t: f64) -> Shown {
        let shown = self.shown_moments(t);
        Shown {
            life: shown.last().map_or(self.life, |m| m.life),
            combo: shown.last().map_or(0, |m| m.combo),
            out: shown.iter().any(|m| m.out),
        }
    }

    /// The field at song second `t` for a frame drawn with `scroll` (the
    /// local player's own speed and direction), `range` arrow heights
    /// of it visible past the receptors: the copy's notes not yet gone,
    /// faded in at the far end and out a little past the receptors (a
    /// miniature has no edge to hide them behind), each lane's last
    /// judgment shown within [`FLASH_SECONDS`]. Out of the stage, no
    /// notes, as a play's field goes blank.
    pub fn frame(&self, track: &TapTrack, scroll: ScrollOptions, t: f64, range: f32) -> Frame {
        let state = ScrollState::at(scroll, &track.timing, t);
        let shown = self.shown_moments(t);
        let out = shown.iter().any(|m| m.out);
        let mut receptors = vec![
            ReceptorState {
                pressed: false,
                mine: None,
                flash: None,
                holding: None,
            };
            usize::from(track.lanes)
        ];
        for m in shown.iter().rev() {
            let age = (t - (m.at + self.delay)) as f32;
            if age > FLASH_SECONDS {
                break;
            }
            let Some(r) = receptors.get_mut(usize::from(m.lane)) else {
                continue;
            };
            match m.kind {
                MomentKind::Tap(j) if r.flash.is_none() => r.flash = Some((j, age.max(0.0))),
                MomentKind::ShockHit if r.mine.is_none() => r.mine = Some(age.max(0.0)),
                _ => {}
            }
        }
        let mut notes = Vec::new();
        if !out {
            // From a second back (a missed note still in sight) to the
            // first one past the far end.
            let from = track.notes.partition_point(|n| n.seconds < t - 1.0);
            for (i, n) in track.notes.iter().enumerate().skip(from) {
                let y = state.y_displayed(n.displayed, n.seconds);
                if y > range && n.seconds > t {
                    break;
                }
                if self.gone[i] <= t || y > range || y < -0.6 {
                    continue;
                }
                let alpha = ((range - y) / 0.8).min((y + 0.6) / 0.6).clamp(0.0, 1.0);
                let beat = n.tick.beat();
                notes.push(NoteSprite {
                    note_index: i,
                    lane: n.lane,
                    y,
                    kind: SpriteKind::Tap,
                    quantization: n.quantization,
                    beat_frac: (beat - beat.floor()) as f32,
                    color: None,
                    alpha,
                    glow: 0.0,
                    beat: beat as f32,
                    hold_life: 1.0,
                });
            }
            // The battle's shocks: from their commitment, until stepped
            // on; avoided ones scroll on past the receptors.
            for (i, s) in self.shocks.iter().enumerate() {
                if s.shown_from > t || s.gone <= t {
                    continue;
                }
                let y = state.y_displayed(s.displayed, s.seconds);
                if y > range || y < -0.6 {
                    continue;
                }
                let alpha = ((range - y) / 0.8).min((y + 0.6) / 0.6).clamp(0.0, 1.0);
                let beat = s.tick.beat();
                notes.push(NoteSprite {
                    note_index: HAZARD_INDEX_BASE + i,
                    lane: s.lane,
                    y,
                    kind: SpriteKind::Mine,
                    quantization: s.tick.quantization(),
                    beat_frac: (beat - beat.floor()) as f32,
                    color: None,
                    alpha,
                    glow: 0.0,
                    beat: beat as f32,
                    hold_life: 1.0,
                });
            }
        }
        let life = shown.last().map_or(self.life, |m| m.life);
        let combo = shown.last().map_or(0, |m| m.combo);
        Frame {
            song_time: t,
            beat: state.beat(),
            note_motion: state.note_motion(&track.timing),
            roll_window_seconds: 0.0,
            lanes: track.lanes,
            receptors,
            notes,
            judgement: None,
            holds: HoldJudgements::default(),
            combo,
            score: Default::default(),
            life,
            danger: false,
            failed: out,
            finished: false,
            visible_range: range,
            appearance: Default::default(),
            cover: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song() -> Song {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs/beatucada");
        let path = std::fs::read_dir(path)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "ssc"))
            .unwrap();
        ddi_library::scan::parse_simfile(&std::fs::read_to_string(path).unwrap(), "ssc").unwrap()
    }

    fn moment(track: &TapTrack, i: usize, j: Judgement, late: f64) -> Moment {
        let n = track.notes[i];
        Moment {
            at: n.seconds + late,
            tick: n.tick,
            lane: n.lane,
            kind: MomentKind::Tap(j),
            life: 0.5,
            combo: 3,
            out: false,
        }
    }

    #[test]
    fn the_copy_is_taps_only_and_every_note_is_found_again() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        assert!(!track.is_empty());
        assert_eq!(track.lanes(), 4);
        let taps = song.charts[1]
            .notes
            .iter()
            .filter(|n| {
                matches!(
                    n.kind,
                    ddi_chart::NoteKind::Tap
                        | ddi_chart::NoteKind::HoldHead { .. }
                        | ddi_chart::NoteKind::RollHead { .. }
                )
            })
            .count();
        assert_eq!(track.len(), taps);
        for (i, n) in track.notes.iter().enumerate() {
            assert_eq!(track.find(n.tick, n.lane), Some(i));
        }
        // A chart the CPUs cannot play has no copy.
        assert!(TapTrack::new(&song, 99).is_err());
    }

    #[test]
    fn a_hit_leaves_the_field_and_a_miss_scrolls_on() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        let mut feed = FieldFeed::new(&track, 1.0, LOCAL_DELAY);
        let scroll = ScrollOptions::default();
        let (a, b) = (0, 1);
        let t = track.notes[a].seconds;
        let before = feed.frame(&track, scroll, t, 6.0);
        assert!(before.notes.iter().any(|n| n.note_index == a));
        feed.push(&track, moment(&track, a, Judgement::W1, 0.0));
        feed.push(&track, moment(&track, b, Judgement::Miss, 0.0));
        let after = feed.frame(&track, scroll, track.notes[b].seconds + 0.01, 6.0);
        assert!(!after.notes.iter().any(|n| n.note_index == a), "hit: gone");
        let missed = after.notes.iter().find(|n| n.note_index == b);
        assert!(
            missed.is_some_and(|n| n.y <= 0.0),
            "missed: past the receptor"
        );
        // The judgments flash on their lanes, then stop.
        let lane = usize::from(track.notes[b].lane);
        assert!(after.receptors[lane].flash.is_some());
        let later = feed.frame(&track, scroll, track.notes[b].seconds + 1.0, 6.0);
        assert!(later.receptors.iter().all(|r| r.flash.is_none()));
        // Nothing beyond the visible range, none far behind.
        assert!(
            later
                .notes
                .iter()
                .all(|n| n.y <= 6.0 && n.y >= -0.6 && (0.0..=1.0).contains(&n.alpha))
        );
        assert_eq!(feed.shown(t + 0.5).life, 0.5);
    }

    #[test]
    fn a_delay_holds_each_moment_back() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        let mut feed = FieldFeed::new(&track, 1.0, 0.25);
        let scroll = ScrollOptions::default();
        let mut m = moment(&track, 0, Judgement::W2, 0.01);
        m.life = 0.0;
        m.out = true;
        feed.push(&track, m);
        let t = m.at;
        // At its time: not shown yet; the note still there, life as it was.
        let f = feed.frame(&track, scroll, t + 0.1, 6.0);
        assert!(f.notes.iter().any(|n| n.note_index == 0));
        assert!(f.receptors.iter().all(|r| r.flash.is_none()));
        assert_eq!(
            feed.shown(t + 0.1),
            Shown {
                life: 1.0,
                combo: 0,
                out: false
            }
        );
        // The delay later: shown, out, the field blank.
        let f = feed.frame(&track, scroll, t + 0.26, 6.0);
        assert!(f.failed && f.notes.is_empty());
        assert!(f.receptors.iter().any(|r| r.flash.is_some()));
        assert_eq!(
            feed.shown(t + 0.26),
            Shown {
                life: 0.0,
                combo: 3,
                out: true
            }
        );
        assert_eq!(f.combo, 3);
    }

    #[test]
    fn shocks_show_from_their_commitment_and_a_hit_takes_one_off() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        let mut feed = FieldFeed::new(&track, 1.0, LOCAL_DELAY);
        let scroll = ScrollOptions::default();
        let section =
            ddi_engine::battle::schedule(&song, 1, &ddi_engine::rules::presets::itg().judge, 3)
                .into_iter()
                .flatten()
                .nth(2)
                .unwrap();
        let first = track.timing.seconds_at(section.cells[0].tick);
        let committed = first - 3.0;
        feed.add_hazards(&track, &section.cells, committed);
        assert_eq!(feed.shocks().len(), 8);
        let mines = |f: &Frame| {
            f.notes
                .iter()
                .filter(|n| n.kind == SpriteKind::Mine)
                .map(|n| n.note_index - HAZARD_INDEX_BASE)
                .collect::<Vec<_>>()
        };
        // Not before the commitment, in view after it.
        assert!(mines(&feed.frame(&track, scroll, committed - 0.1, 12.0)).is_empty());
        assert!(mines(&feed.frame(&track, scroll, first - 0.5, 12.0)).contains(&0));
        // The first stepped on: gone, its receptor flashing; the second
        // avoided: on past the receptor.
        let c = section.cells[0];
        feed.push(
            &track,
            Moment {
                at: first + 0.01,
                tick: c.tick,
                lane: c.lane,
                kind: MomentKind::ShockHit,
                life: 0.4,
                combo: 0,
                out: false,
            },
        );
        let second = track.timing.seconds_at(section.cells[1].tick);
        let f = feed.frame(&track, scroll, first + 0.05, 12.0);
        assert!(!mines(&f).contains(&0));
        assert!(f.receptors[usize::from(c.lane)].mine.is_some());
        assert_eq!(feed.shown(first + 0.05).life, 0.4);
        // The lane's lock, where the rules lock it: for the lock's second
        // from the moment, that lane alone; none where they do not.
        let lanes = track.lanes();
        assert!(feed.locks(lanes, first + 0.05).iter().all(Option::is_none));
        feed.set_lock(Some(1.0));
        let locks = feed.locks(lanes, first + 0.26);
        for (lane, l) in locks.iter().enumerate() {
            if lane == usize::from(c.lane) {
                assert!(l.is_some_and(|k| (k - 0.25).abs() < 1e-3), "{locks:?}");
            } else {
                assert_eq!(*l, None);
            }
        }
        assert!(feed.locks(lanes, first).iter().all(Option::is_none), "not before");
        assert!(feed.locks(lanes, first + 1.02).iter().all(Option::is_none));
        let after = feed.frame(&track, scroll, second + 0.05, 12.0);
        let passed = after
            .notes
            .iter()
            .find(|n| n.note_index == HAZARD_INDEX_BASE + 1);
        assert!(passed.is_some_and(|n| n.y < 0.0), "avoided: scrolls past");
    }

    #[test]
    fn moments_out_of_order_take_their_place() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        let mut feed = FieldFeed::new(&track, 1.0, LOCAL_DELAY);
        let second = (2..track.len())
            .find(|&i| track.notes[i].seconds > track.notes[1].seconds)
            .unwrap();
        let mut late = moment(&track, second, Judgement::W3, 0.0);
        late.life = 0.3;
        let mut early = moment(&track, 1, Judgement::W1, 0.0);
        early.life = 0.9;
        feed.push(&track, late);
        feed.push(&track, early);
        assert!(feed.moments().windows(2).all(|w| w[0].at <= w[1].at));
        assert_eq!(feed.shown(late.at).life, 0.3);
        assert_eq!(feed.shown(early.at).life, 0.9);
    }
}
