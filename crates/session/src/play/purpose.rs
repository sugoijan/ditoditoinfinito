//! What a play is for, decided once from the facts both shells have (a
//! replay, calibration, the chart editor's playtest, an announcer studio
//! scene, the solo CPU, a room's stage), and what follows from it: whether
//! the play is kept in the history, gets its best play's ghost, gives
//! the announcer what the history says before it (`Prior`), and has the
//! start prompt's timing hint.

/// What a play is for. Each flag a fact a shell knows before the play
/// starts; none set is a song played for itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Purpose {
    /// A kept play replayed, or its results shown.
    pub replay: bool,
    /// A calibration test instead of a song.
    pub calibration: bool,
    /// The chart editor's playtest (`docs/plans/editor.md`).
    pub playtest: bool,
    /// The announcer studio's scene in play
    /// (`docs/plans/announcer-studio.md`).
    pub scene: bool,
    /// The solo CPU demonstration (`docs/plans/cpu-player.md`).
    pub cpu: bool,
    /// A multiplayer room's stage, played or watched.
    pub stage: bool,
}

impl Purpose {
    /// A song played for itself: none of the others.
    pub fn ordinary(self) -> bool {
        self == Purpose::default()
    }

    /// Kept in the history (`docs/plans/play-history.md`): a play for
    /// itself only; never a test, a replay, a scene, a CPU's or a stage.
    pub fn keeps(self) -> bool {
        self.ordinary()
    }

    /// Given the chart's best play's ghost where the settings ask
    /// (`Settings::ghost`): a play for itself only (a playtest "has no
    /// ghost", `docs/plans/editor.md`).
    pub fn has_ghost(self) -> bool {
        self.ordinary()
    }

    /// Given what the history says of the chart before it, for the
    /// announcer's first clear and new record: a play that is kept.
    pub fn has_prior(self) -> bool {
        self.keeps()
    }

    /// Whether the start prompt's quiet line suggests calibrating or not
    /// (`screens::prompt::timing_hint`, `docs/plans/quiet-calibration.md`),
    /// the play autoplayed where `auto`: never on a calibration test, a
    /// replay, an announcer studio scene or a room's stage, nor under
    /// autoplay (nobody's timing is played there).
    pub fn shows_timing_hint(self, auto: bool) -> bool {
        !(self.calibration || self.replay || self.scene || self.stage || auto)
    }
}

#[cfg(test)]
mod tests {
    use super::Purpose;

    /// Every purpose against every predicate, as desktop decided them
    /// (`play::host::begin`, `Play::record`), but for the playtest's
    /// ghost, which the web never gave and `docs/plans/editor.md` says it
    /// has not.
    #[test]
    fn each_purpose_keeps_ghosts_and_tells_as_the_shells_decided() {
        let p = Purpose::default();
        // (name, purpose, keeps, ghost, prior, timing hint)
        let table = [
            ("a song", p, true, true, true, true),
            ("a replay", Purpose { replay: true, ..p }, false, false, false, false),
            ("calibration", Purpose { calibration: true, ..p }, false, false, false, false),
            ("a playtest", Purpose { playtest: true, ..p }, false, false, false, true),
            ("a scene", Purpose { scene: true, ..p }, false, false, false, false),
            ("the solo CPU", Purpose { cpu: true, ..p }, false, false, false, true),
            ("a stage", Purpose { stage: true, ..p }, false, false, false, false),
            // A watched stage is a stage (its order says it is watched).
            ("a watched stage", Purpose { stage: true, ..p }, false, false, false, false),
        ];
        for (name, purpose, keeps, ghost, prior, hint) in table {
            assert_eq!(purpose.keeps(), keeps, "{name} kept");
            assert_eq!(purpose.has_ghost(), ghost, "{name} ghost");
            assert_eq!(purpose.has_prior(), prior, "{name} prior");
            assert_eq!(purpose.shows_timing_hint(false), hint, "{name} timing hint");
            // Autoplayed, none says anything of the player's timing.
            assert!(!purpose.shows_timing_hint(true), "{name} autoplayed");
        }
    }
}
