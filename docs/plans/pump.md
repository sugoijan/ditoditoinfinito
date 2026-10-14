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
judgement and a step of combo, instead of OK or N.G. at its end). For now
a pump hold is a dance hold, held to its end; ticked holds for pump charts
are the next step (below). Two-player styles (`pump-couple`,
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

## Later

- **Ticked holds on pump charts**, as StepMania 5.1's pump game has them:
  checkpoints at the chart's `#TICKCOUNTS` (4 a beat by default) judged
  while the hold is down, each counted in combo and score, the hold not
  dropped on release. It touches the engine's judge, the rulesets'
  scoring and the results, so it comes as a step of its own.
- Import `NoteSkins/pump/*` and draw pump layouts with them by default
  when the player picks one; their panel colours.
- Centre notes for the other bundled skins (the maintainer's wish).
