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

## Steps

1. Fail type as an option. Done: engine (`FailType`, `FailPolicy::EndOfSong
   { min_life: None }`, the gauge stopped after a delayed fail,
   `gauge_failed_at`), settings and Options' Rules rows, results' note and
   mods, the session playing on.
2. Life and judge difficulty. Done with step 1 (the same options struct).

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
- Browser: `target/scratch/pw/rules-options.mjs` (headless Chrome, WebGPU
  and WebGL2).
