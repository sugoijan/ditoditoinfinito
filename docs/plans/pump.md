# Implementation plan: pump layouts (2026-10-13)

StepMania plays a second pad game besides dance: `pump`, a pad of five
panels (down-left, up-left, centre, up-right, down-right) instead of four
arrows. Simfiles carry its charts next to the dance ones (`pump-single`,
`pump-halfdouble`, `pump-double`), and three bundled songs have fifteen of
them (Beatucada, Into My Dream, Some Things Must). Until now they were
kept in the song, unplayable. The maintainer's decisions (2026-10-13):

- **Named "Pump"** in the UI: "Pump single", "Pump half-double", "Pump
  double". It is StepMania's own name for the game (its `pump` game and
  `pump-*` steps types), so it names a layout the way "Single" does, not
  another game as a selling point.
- **The chosen skin draws them.** A dance skin draws a pump layout as it
  draws Dancing☆Onigiri's (`docs/plans/danoni-skins.md`), from its own
  folders and the ones it falls back on only: a lane takes the skin's own
  `DownLeft`, `DownRight` or `Center` elements when it has them (as they
  are, unturned), else Down's, turned to the lane's angle (the
  diagonals) or as they are (the centre). Dito gets a centre of its own.
- **Pump skins later.** Importing StepMania's `NoteSkins/pump/*` (their own
  art per panel, coloured by panel rather than by beat) is a later step;
  import keeps leaving them out.

## Rules

Pump charts play under the chosen ruleset like any StepMania chart
(`LayoutFamily::StepMania`): taps, mines and lifts are judged as dance
judges them. Holds differ: StepMania 5.1's pump game ticks them
(`m_bTickHolds` is set for `pump` alone, `src/GameManager.cpp`; `Player.cpp`
judges a held hold at checkpoints, `#TICKCOUNTS` a beat, each one a
judgement and a step of combo, instead of OK or N.G. at its end): see
"Ticked holds" below. Two-player styles (`pump-couple`,
`pump-routine`) stay unplayable, as dance's do.

## Status (2026-10-13)

Done:

- `chart/src/layout/mod.rs`: `Layout::pump_single`, `pump_halfdouble`,
  `pump_double` in StepMania's track order (`src/GameManager.cpp`: single
  DL, UL, C, UR, DR; half-double the middle six panels of two pads, the
  left pad's C, UR, DR and the right pad's DL, UL, C; double both pads),
  a new `Glyph::Center`, Left and Right from StepMania's
  `NoteDataUtil::Turn` (Mirror is the full reversal it also is there).
  Keys: a 3×3 block per hand (Q E / S / Z C on the left, U O / K / M .
  on the right) and the number pad as StepMania's second player has it;
  each panel keeps its keys on every pump layout.
- Bindings (`engine/src/bindings.rs`): standard controllers get each
  panel on the d-pad (left pad) or the face buttons (right pad), the pad
  turned an eighth (up-left on Up, up-right on Right…), the centres on
  the shoulders. Two controllers on a two-pad layout each play one pad
  with their bindings of the layout's one-pad layout
  (`Layout::one_pad_layout`: `pump-single` for the pump ones,
  `dance-single` for dance double), each pad's lanes taken by name, so
  half-double's left pad plays the centre, up-right and down-right rows.
- The built-in arrows draw the centre as a square (the symbols' style,
  decision 27).
- Skins (`noteskin/src/load.rs`): `Button::DownLeft`, `DownRight`; the
  lane choices above, natively tested and through a skin's Lua (a skin's
  Lua without a redirect for a button falls back to the next choice).
  A dance skin's Lua is told the dance game on every layout, and the
  dance style of the layout's pads (`single`, or `double` on two pads;
  `ddi_noteskin::lua::dance_style`): StepMania never loads a dance skin
  for pump, and a skin that reads the game switches to tables for pump's
  buttons, which the play does not pass (Dake drew placeholders on three
  lanes when it was told `pump`). Pump skins will be told `pump`
  (`Env::game`, the steps types `StepsType_Pump_*`).
- Dito's centre: `xtask gen-dito` draws the shader's rounded square
  (`sd_square`) as tap, lift and fake sheets for every colour scheme and a
  receptor; `Center Tap Note.lua` and `Center Receptor.lua` load the
  symbols' shared files (`_symbol tap note.lua`, `_symbol receptor.lua`,
  the onigiri's too), and the explosion rings the centre in its square.
  CF_CHROME, CF_VIBRANT and CF_ALLOY draw their own centre; the other
  bundled skins their Down.
- The preview's layout picker and the bindings page list the three; the
  song list offers them as styles where the library has their charts, and
  warns about a connected controller with no bindings for the layout (a
  dance pad on a pump layout; before, any controller with saved bindings
  counted as a spare). Packs imported before keep the songs and charts
  they were imported with: a song with only pump charts was left out then,
  and pump charts were not listed. Importing the pack again, or "re-read
  folder" for a linked one, brings them in.

Checked: native tests; the bundled songs' pump charts in headless Chrome
(WebGPU and WebGL2), Firefox and WebKit with Dito, the built-in arrows,
CF_CHROME and Delta on all three layouts; autoplays of Beatucada's
half-double (311 notes), double and single charts all top tier through
their first 101 s with the skins' Lua running. Every bundled skin builds
every built-in layout's lanes through its Lua with the style it is told,
without warnings (`noteskin/tests/bundled.rs`). After an independent
review: dance skins told the dance game (above), the controller warning,
the docs on holds.

## Ticked holds (2026-10-14)

The maintainer's decision: a rule option, **Holds**, with three values:
"by the chart's game" (the default: pump charts ticked, every other
chart dance-style), "dance-style" and "ticked". It applies to the
StepMania presets (ITG, StepMania 5, DDR A); Dancing☆Onigiri rules keep
their own holds. A chart without `#TICKCOUNTS` ticks at 4 a beat, as in
StepMania (`TickcountSegment::DEFAULT_TICK_COUNT`); a section set to 0
has no checkpoints. Editors write `#TICKCOUNTS:0=4` into every `.ssc`
(every bundled song has it, dance-only ones too), so neither the tag nor
its absence tells what an author meant, and there is no fallback to
dance-style holds by the tag.

StepMania 5.1's pump game, which ticked holds reproduce (`Player.cpp`'s
`UpdateHoldNotes`, its checkpoint loop and `HandleHoldCheckpoint`;
`ScoreKeeperNormal.cpp`; the `_fallback`
metrics its theme sets for `pump`):

- **Checkpoints** lie on the tick grid of the tick count in force at
  each row (`row % (48 / ticks) == 0`, 48 rows a beat as our ticks),
  strictly after a hold's head and up to and including its tail; rolls
  have them too.
- **A hold is judged at each checkpoint row**: it is hit if the hold's
  life is above zero there. Released, a hold's life runs out over the
  checkpoint window (0.1664 s, scaled like the others) instead of the
  hold window; pressed again, it is full again. A hold is never dropped
  before its end (`ImmediateHoldLetGo` is false for pump), and it starts
  at its head whether or not the head was stepped on
  (`RequireStepOnHoldHeads` is false). Its life then starts full if its
  panel is down as the head passes, else at `InitialHoldLife` (0.05 for
  pump, so it runs out 8 ms later unless the panel is pressed).
- **The head**: any press that takes it (late, or early within the
  windows) is a W1, and a panel down as the head passes steps on it, a
  W1 too (`Player::Step` without `RequireStepOnHoldHeads`;
  `CrossedRows`). Its real offset still goes to the fast/slow counts. A
  head never pressed is a Miss; its hold still runs.
- **One judgement per checkpoint row**, whatever the number of holds
  crossing it: a hit when none of them is released, else a miss. A row
  that also has a step (a tap or a hold head) is not judged separately
  (`CheckpointsTapsSeparateJudgment` is false for pump), but its
  checkpoints still count for the holds' outcomes. The row of a hold's
  tail is judged but does not count for that hold: StepMania settles
  the hold (`UpdateHoldNotes`) before it crosses the row.
- **Combo**: a hit adds the number of holds held (one under per-row
  combo, DDR), a miss breaks it. A missed checkpoint loses the full
  combo.
- **Score and life**: a checkpoint hit weighs as a W1, a miss as a Miss
  (StepMania's pump weights are exactly its W1 and Miss weights: percent
  3 and 0, grade 2 and −8, life +0.008 and −0.080), and the hold's own
  O.K. and N.G. weigh nothing (pump sets `Held` and `LetGo` to 0). The
  same rule carries to ITG's weights and to DDR A's money score, where
  checkpoints count as steps and holds do not.
- **The hold's outcome** at its tail: O.K. when no checkpoint was missed
  and at least one was hit (a hold without checkpoints: when its head
  was stepped on); a roll is O.K. when its life is above zero at its
  tail. StepMania's battery gauge (no preset uses one yet) loses no life
  on a missed checkpoint and one on a dropped hold, ticked or not.
- **Display**: a checkpoint shows no judgement word (the `_fallback`
  theme draws none for it), only the combo; held lanes flash as a
  completed hold does (`CheckpointsFlashOnHold`). StepMania's results
  count checkpoint hits with the W1 row and misses with Miss
  (`CheckpointsWithJudgments`); ours give them a row of their own, "hold
  ticks hit / missed", so the steps' counts stay the steps'.

Deviations: StepMania's best possible score counts every checkpoint row
(`NoteDataUtil::GetTotalHoldTicks`), including the rows with steps it
does not judge, so a perfect pump play scores below 100 % when holds
cross steps; ours counts the rows it judges. A checkpoint row inside a
warp or a fake segment counts for no hold here, where StepMania still
counts it for the holds' outcomes without judging it.

Status (2026-10-14): done in the engine (`HoldRules::Ticked`,
`JudgeEventKind::Checkpoint`, `presets::HoldStyle`, `TimingMap::ticks_per_beat_at`)
and the app (the option under Rules, the results row). Natively tested:
the judge's rules one by one (`engine/src/judge.rs`) and whole plays
under StepMania 5, ITG and DDR A (`engine/tests/ticked.rs`: 100 % and
1,000,000 for a perfect play, combo by holds kept, a missed checkpoint's
cost). After an independent review: heads always W1 (pressed late or
held through), `InitialHoldLife`, the tail row left out of the outcome,
the events of a press in time order, the battery.

## Later

- Import `NoteSkins/pump/*` and draw pump layouts with them by default
  when the player picks one; their panel colours.
- Centre notes for the other bundled skins (the maintainer's wish).
