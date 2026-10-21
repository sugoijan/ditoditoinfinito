# Rules and options: fail type, difficulty, FA+, autoplay, event sounds

Status: in progress (2026-10-21). Closes the "Rules and options", "Play
screen" (event sounds) and "Menus" (autoplay) gaps of `docs/PLAN.md`
section 5 that this plan lists. Every feature lives in the shared crates
(`ddi-engine`, `ddi-session`, `ddi-ui`), so both shells get it with no
platform code beyond what they already have.

## Research

### Fail types (StepMania 5.1)

`PlayerOptions.cpp` has four `FailType`s, offered in the player options'
"Fail" row (`_fallback/metrics.ini` `Fail,1..4`; names from
`_fallback/Languages/en.ini`: Immediate, **Delayed** for
`ImmediateContinue`, **Fail at end**, Off). What each does is in
`ScreenGameplay.cpp` (`Update`, `STATE_DANCING`, and the stage's end):

- `Immediate`: once the life meter `IsFailing`, the player is marked
  failed and, all players failed, the song ends (`SM_NotesEnded`).
- `ImmediateContinue`: marked failed at once, "wait until the end of the
  song to fail": the song goes on. `LifeMeterBar::ChangeLife` returns
  early once `m_bFailed` ("If we've already failed, there's no point in
  letting them fill up the bar again"): the bar stays empty.
- `EndOfSong`: nothing during the song (`HealthState` still goes `Dead`
  and back); at the end, a player whose meter `IsFailing` then is marked
  failed. The bar can come back from empty.
- `Off`: never failed; `HealthState` never `Danger` or `Dead`.

`LifeMeterBar::IsFailing` is `life <= passmark` (0 unless a course sets
one), which is our gauges' `failed()`.

### Life and judge difficulty (StepMania 5.1)

`ScreenOptionsMasterPrefs.cpp` maps the machine options' rows to the
preferences, "StepMania 3.9 and 4.0 values":

- `LifeDifficulty` 1–7 → `LifeDifficultyScale` 1.60, 1.40, 1.20, **1.00**,
  0.80, 0.60, 0.40. `LifeMeterBar::ChangeLife` multiplies a gain by it and
  divides a loss by it (after the regen lockout); only the life bar reads
  it (`LifeMeterBattery` does not).
- `TimingWindowScale` "Judge Difficulty" 1–8 and Justice → 1.50, 1.33,
  1.16, **1.00**, 0.84, 0.66, 0.50, 0.33, 0.20. `Player::GetWindowSeconds`
  is `seconds × scale + TimingWindowAdd` for every window, holds', rolls',
  mines' and checkpoints' too.

Level 4 is 1.0 in both: each preset as it is (its own windows, its own
`TimingWindowAdd`, ITG's 1.5 ms).

### Dancing☆Onigiri

danoniplus has no fail type and no difficulty scales: its gauges (Original,
Light, NoRecovery, SuddenDeath, Practice, the chart's own) decide life and
failure, Practice never fails, and its "Judge range" option (already ours,
`DanoniOptions::range`) is its window choice. So none of the three applies
under Dancing☆Onigiri's rules; those keep the gauge and range chosen.

### ITG FA+ (Simply Love)

Simply Love (GPLv3: read for its rules, nothing transcribed; the numbers
are in `docs/research/rules-ddr-itg-dwi.md` 5.1–5.2) has two ways to the
15 ms "blue" Fantastic. Its FA+ *mode* shifts every window down a tier
(W1 13.5 ms, W2 21.5, …, Way Offs gone) with its own percentage. Its ITG
mode can instead show the FA+ window inside ITG's own judging: a
Fantastic within 13.5 ms (+ 1.5 ms `TimingWindowAdd`, so 15 ms) is a
blue one, the rest of the Fantastic window (to 23 ms) a white one
(`IsW0Judgment` tests a W1's offset against the FA+ W1 window), and the
EX score weighs them (`CalculateExScore`): blue 3.5, white 3, Excellent
2, Great 1, Decent, Way Off and Miss 0, held 1, let go 0, a mine hit −1;
EX% = floor(points / (steps × 3.5 + (holds + rolls) × 1) × 10000) / 100,
not below 0. That second way is what the community's events score by,
and what the engine's `W0` sub-window was made for ("re-classifies a W1
hit for display/EX only", section 3.3 of the research).

## Decided here (the maintainer may change any)

1. The fail type and both difficulty levels are the player's
   `StepManiaOptions` (`fail`, `life`, `judge`), stored in the settings
   (`stepmania`) and applied by `Ruleset::with_stepmania` to the StepMania
   presets only: not to Dancing☆Onigiri's rules (above), not to
   calibration (its own ruleset, fail off, one wide window).
2. The levels are StepMania's (1–7, 1–8 and Justice), not free scales, so
   a record names a level StepMania players know, and a key compares
   exactly.
3. DDR A's gauge (the arcade's, approximated) takes no life level, as
   StepMania scales only `LifeMeterBar`; the row says so, and the key
   keeps the default level there. Fail type and judge level apply to DDR A
   as to any StepMania preset.
4. A play's `RulesetKey` keeps the options (`stepmania`, written only when
   not the defaults, so older records read and write the same), so a
   replay and a re-judging build the same rules.
5. Bests are kept apart by judge level (it changes the windows) but not
   by fail type or life level (they change the bar and whether the play
   fails, not how notes are judged), as Dancing☆Onigiri's gauge choice
   already is. The lamp follows the fail type as StepMania records it: a
   play under fail off is not failed.
6. Results show the options in the mods line ("fail off · life 6 · judge
   5"), as song select's mods summary and the plays list do, and, where
   the fail type changed the outcome, a note: when the bar emptied, and
   that the play went on (delayed), came back (fail at end) or was not
   failed (off). `Results::gauge_failed_at` carries when.
7. A delayed fail shows no FAILED banner during the song (StepMania shows
   none either); the banner and the music's fade come when the play ends
   failed, as at the end of a fail-at-end play.
8. `itg-fa+` is ITG with the FA+ window shown, not Simply Love's FA+
   mode: judged, scored (the percentage), graded and gauged as `itg`, so
   a play's grade means the same under both; the FA+ window splits the
   Fantastics (`Tally::w0`, the HUD's Fantastic blue or white, the
   results' "Blue Fantastic" and "White Fantastic" rows, the timing
   panel's bands) and gives the EX score as Simply Love's percentage
   (`ScoreView::ex_percent`, "EX 98.76%"; weights kept in halves, 7, 6,
   4, 2, held 2, mine −2). The FA+ window scales with the judge level as
   every window does. The `itg` preset no longer carries the (unused)
   `w0`, so it shows no split. It is its own preset (a key of its own,
   bests apart from `itg`'s): its EX scores are not `itg`'s.
9. The ruleset list offers it after ITG.

## Steps

1. Fail type as an option. Done: engine (`FailType`, `FailPolicy::EndOfSong
   { min_life: None }`, the gauge stopped after a delayed fail,
   `gauge_failed_at`), settings and Options' Rules rows, results' note and
   mods, the session playing on.
2. Life and judge difficulty. Done with step 1 (the same options struct).
3. ITG FA+. Done: `presets::itg_fa_plus`, `JudgeTable::is_w0`,
   `ScoreCtx::w0`, `DancePoints::with_ex_w0`, `JudgementFlash::w0`, the
   results' rows, EX and bands, the theme's `Judgement.W1White`.

## Checks

- Engine: `crates/engine/tests/stepmania_options.rs` (each fail type, the
  life levels' misses to fail and gains, the judge levels' windows,
  Justice, the keys and bests, the other rules untouched), each play
  judged again from its key and log to the same results.
- UI: the options rows (`every_row_shows_its_setting_and_changes_it`
  covers the new rows), the results' note and mods
  (`the_fail_type_shows_where_it_changed_the_outcome`), the history
  (`stepmania_options_are_kept_with_the_play`), song select's best key.
- Session: `a_delayed_fail_or_fail_off_plays_to_the_end`.
- FA+: `fa_plus_splits_the_fantastics_and_scores_ex` (engine: counts, EX
  91.07% for 5 blue, 2 white and an Excellent, the same percentage and
  grade as ITG, the HUD flash, the judge level), the results' rows, EX and
  bands (`fa_plus_shows_blue_and_white_fantastics_and_its_ex`), the HUD's
  white Fantastic read back from the GPU
  (`crates/desktop/tests/hud_colors_offscreen.rs`).
- Browser (2026-10-21, `target/scratch/pw/rules-options.mjs`, headless
  Chrome, WebGPU and WebGL2, Beatucada Beginner autoplayed): judge
  Justice with presses 8 ms late gives 38 Excellents where judge 4 gives
  38 Fantastics, the mods "fail off · life 7 · judge Justice" and the
  history's key keep the options; presses 400 ms late under fail off play
  to the end, not failed, with "The life bar emptied at 0:19: not failed
  (fail off)."; ITG FA+ on time gives 38 blue Fantastics and EX 100.00%,
  18 ms late 38 white ones and EX 85.71%.
