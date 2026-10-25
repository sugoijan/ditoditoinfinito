# Implementation plan: skin parameters (2026-10-21)

Update 2026-10-25: Infinito is now the authored SVG skin formerly called Spiral
Base and then Dito. Its controls are Colors (Vivid default, Rainbow, Flat), Onigiri,
ShockArrows, Explosions, ReceptorFill and HitVariation. Colors now selects
the SVG material behavior within Infinito; the original color-variant folders
and ChartColors/Lighter controls are retired; procedural arrows are an internal fallback.
Settings migrate the old names and move Dito / Spiral Base's saved controls to
`bundled:dance/infinito`, each value checked against Infinito's declarations
(one at its default is not kept); the older `note_colors` and `chart_colors`
fields are no longer read. The dated design and validation below describe the
original rollout of the parameter mechanism.

`docs/PLAN.md`, "Options follow-up (2026-10-18)": note colors and songs'
own colors sat under Arrows, shown for Dito and Built-in only; "a future
noteskin parameter mechanism should let skins declare controls and
defaults, persist values per skin and share them between preview and
play. Dito's color variants would use it first; retiring the visible
Built-in choice can follow once Dito covers its needs, keeping
procedural arrows for fallbacks." This is that mechanism, Dito on it, and
the Built-in choice retired into it.

StepMania has nothing like it (5.1, ITGmania and OutFox skins choose
looks by being separate skins, as Dito's color schemes were), so it is
our extension, documented beside the others (SVG images,
`HoldUseDroppedState`, `HitX`/`HitY`, `TapJudgmentsPerNote`;
`docs/plans/bundled-skins.md`, `tools/noteskin-danoni/README.md`). A skin
without declarations loads, draws and is listed exactly as before.

## The declaration

In the skin's `metrics.ini`, so it is read without running anything
(Options lists a skin's controls before any Lua runs, and an imported
skin's are read from the texts its record already keeps), and so a skin's
fallbacks' declarations merge under its own like every metric (a color
scheme's skin falling back on Dito declares what Dito declares):

```ini
[Parameters]
List=Colors,Explosions,Size,Tint

[Parameter Colors]
Type=Choice
Label=Note colors
Description=How notes are colored.
Choices=Vivid,Flat,Quantized
ChoiceLabels=Vivid,Flat,By subdivision
Skins=Dito Vivid,Dito Flat,
Default=Vivid

[Parameter Explosions]
Type=Toggle
Default=1

[Parameter Size]
Type=Number
Min=0.5
Max=1.5
Step=0.25
Unit=×
Default=1

[Parameter Tint]
Type=Color
Default=#59D9FF
Choices=#59D9FF,#FF4D4D
ChoiceLabels=Cyan,Red
```

- `[Parameters] List` names them, in the order Options shows them. A name
  is up to 32 letters, digits, `_` or `-`; at most 16 parameters.
- `[Parameter <name>]`: `Type` (`Choice`, `Toggle`, `Number`, `Color`,
  any case), `Label` (Options' row; the name when missing; 48
  characters), `Description` (the row's sentence; 400 characters),
  `Default`.
- **Choice**: `Choices` (ids, as names), `ChoiceLabels` (what Options
  shows, the ids when missing), at most 32; the default is the first
  when not one of them.
- **Toggle**: `Default` read as `GetMetricB` reads (`1`, `true`).
- **Number**: `Min`, `Max`, `Step` (1 when missing), at most 1,001
  values; `Unit` after each (8 characters). The default (and any value)
  is clamped to the range and snapped to the step.
- **Color**: `Default` as `#RRGGBB` or `#RRGGBBAA`; `Choices` a palette
  of such colors with their `ChoiceLabels`, else a standard palette of 12
  (cyan to black), the default put first when not in it. A stored color
  off the palette (a hand edit, another version's) is kept and shown.
- **A choice may name the skin that draws it** (`Skins`, aligned with
  `Choices`, empty for the skin itself): a skin of the same game that
  falls back on this one, loaded in its place
  (`ddi_noteskin::params::head`), the way StepMania's own skins share a
  base. One that is missing or does not fall back on it is ignored. Only
  the first choice parameter naming skins picks one.

Anything past a limit or malformed is left out with a warning
(`Declared::warnings`); a skin's declarations never fail its load.

## What the Lua reads

`NOTESKIN:GetParameter(name)`: the value of a declared parameter, the
player's when its declaration takes it, else its default: a choice's id
(a string), a toggle's boolean, a number, a color as `color()` makes one.
`nil` for a name the skin does not declare. The values are resolved once
at load, in Rust (`Env::params`, resolved against the skin's merged
metrics in `LuaSkin::new_sliced`), and each call returns a copy (a color a
fresh table): nothing a script does changes what the next call reads,
and nothing else about the player leaks in. A call costs fuel as any host
call does (`charge` on the name and text), so a loop reading it ends on
its budget.

## Where the values live

`Settings::skin_params`: by skin key (`bundled:dance/infinito`,
`dance/<name>`), then by parameter name, as plain JSON (`"Vivid"`,
`true`, `0.5`, `"#FF8800"`). A value equal to its default is not kept,
so a skin's default can change and players who never chose follow it.
Bounded on load (`Settings::sanitize`): 64 skins, 16 values each, names
and texts 32 characters, numbers finite. What a skin's declarations take
of them is decided where they are read, not stored.

**Migration** (`Settings::migrate`, run by `sanitize` on every load, and
on a replay's settings): the retired Dito and Spiral Base keys become
Infinito's, their values merged under its key (its own winning), and
every value there goes through Infinito's declarations: one it does not
declare or take (a choice it does not offer) is let go, one at its
default is not kept, and an empty entry is dropped. `noteskin:
"builtin"` becomes no skin chosen. (Until 2026-10-25 this turned the old
`note_colors` and `chart_colors` fields into Dito's `Colors` and
`ChartColors`; neither is read any more.)

**Replays**: the play record's settings snapshot carries `skin_params`
like any setting, so a replay is drawn with the values played. A record
whose snapshot does not read falls back on `DrawSetup`, which now keeps
the drawn skin's values too (`skin_params`; Infinito's when none was
chosen) beside the note colors' name and the chart-colors toggle it
always kept, which a replay no longer reads.

## Preview and play pass the same values

`ddi_shell::play_skin` (a play's skin and its Lua's `Env`, both shells)
and `ddi_shell::field_preview_skin` (Options' preview, `SkinWant::params`)
take the values from `Settings::skin_params` for the same key, and
`SkinWant` compares them, so changing a parameter readies the preview
again. Each shell's `prepare` loads `params::head` of the chosen skin with
`env.params` (the web's `noteskins::prepare_until`, desktop's
`play::skins::read_inner`), so the color scheme's skin is the one built,
natively too when the Lua stops; a Lua stop still counts against the
chosen skin's key (Dito's for every scheme, as before).

## Options

Under Arrows, after the Arrows choice, the chosen skin's parameters as
rows (`skin.<name>`; a chosen pump skin's after the pump row,
`pump_skin.<name>`): a choice and a color as choices, a toggle as
Off/On, a number as a stepper. Dito's declarations are read from its
`metrics.ini` built into the game (`ddi_noteskin::bundled::dito_parameters`),
so its rows are there before any skin is read; the shells read the other
skins' with the list of skins (`params::catalog`: the web's
`noteskins::choices`, desktop's `library::noteskins::parameters`) and give
them to the model (`OptionsModel::set_skin_parameters`). Removing a skin
forgets its values; removing the imported songs and skins forgets theirs;
resetting the settings forgets all.

## Dito on it

Dito declares `Colors` (Vivid, Rainbow, By subdivision, Flat, Single
color; each but By subdivision its color scheme's skin, as before),
`ChartColors` (songs' own colors), `Explosions` (its `Down Explosion.lua`
returns an empty frame when off) and `Lighter`. The game reads three of
them itself, by name (`ddi_noteskin::bundled::dito`): the built-in arrows
draw in Dito's colors (`Settings::note_colors`, wherever they draw: the
lighter choice, a song's own colors, an element or a skin that falls
back), a chart with its own colors is drawn by the built-in arrows when
`ChartColors` is on (`Settings::chart_colors`, as before for Dito), and
`Lighter` draws the built-in arrows instead of the skin
(`Settings::lighter`).

## The Built-in choice retired

What "Built in" offered over Dito: the colors (Dito has them), a chart's
own colors (drawn by the built-in arrows either way), and being lighter:
no Lua, no explosions, and on WebGL2 no offscreen frame for the skin's
blending. Dito's `Lighter` toggle is exactly that drawing
(`Settings::builtin_arrows` tells the web's game loop not to ready the
offscreen path), so the Arrows list is now Dito, the bundled skins and the
player's, and the built-in arrows are Dito's lighter look. The procedural
arrows stay what every missing element, a stopped Lua and a chart's own
colors fall back on; `builtin` stays a key the skin preview page
(`#/skin-preview?skin=builtin`) and the play path understand.

## Decided here (the maintainer may change any)

- Declarations in `metrics.ini` (not Lua): readable without running a
  script, merged under fallbacks like metrics, and editable in SkinCraft
  as text.
- Variants by `Skins` on a choice rather than per-value metrics: Dito's
  color schemes differ in sheets as well as metrics, and the skins
  already existed and are tested; any skin can do the same.
- The game reads Dito's `Colors`, `ChartColors` and `Lighter` by name;
  other skins' parameters reach only their Lua (and their `Skins`). No
  generic "host role" key: the built-in arrows are Dito's look, nobody
  else's.
- Values at their default are not stored; a replay's snapshot keeps what
  was stored, so a replay of a play at the defaults follows the skin's
  defaults of the day (as it follows the skin's art).
- `Explosions` added to Dito, so a toggle the Lua reads is exercised by
  the default skin, and so Dito covers the "no explosions" look Built in
  had.
- The Built-in choice is retired into Dito's `Lighter` (above), not
  kept beside it.

## Steps

1. The declaration, its parsing and bounds, `head`, `catalog`
   (`crates/noteskin/src/params.rs`), the Lua accessor (`host.rs`,
   `prelude.lua`), `Env::params`.
2. Settings (`skin_params`, migration, Dito's three), Options' rows
   (`options_model.rs`), replays (`DrawSetup::skin_params`, the record),
   `ddi_shell::play_skin` and the preview's `SkinWant::params`, both
   shells' loads and lists.
3. Dito's declarations and `Explosions`; the Built-in choice retired.
4. Tests: below.
5. `docs/PLAN.md`, `AGENTS.md`, the noteskin README's extension list.

## Checks

- `crates/noteskin/src/params.rs`: every kind with its defaults, values
  only as declarations take them (case, clamping, snapping, wrong kinds,
  unknown names), malformed and oversized declarations left out with a
  warning, `head` (a variant that falls back on the base, one that does
  not, a missing one), `catalog`, JSON form.
- `crates/noteskin/src/lua/tests.rs`: the Lua reads each kind, defaults
  and set values, `nil` for undeclared names, a color is a copy, a loop
  of reads ends on its fuel.
- `crates/noteskin/src/bundled.rs`, `crates/noteskin/tests/bundled.rs`:
  Dito declares what the game reads; each color picks its skin; every
  scheme declares Dito's; `Explosions` off builds an empty explosion;
  every bundled skin still builds every element without a warning.
- `crates/ui`: the migration (old fields, `builtin`, defaults storing
  nothing), the bounds, Options' rows (Dito's built in, another skin's
  from the shell, steppers and palettes, a color off the palette, values
  per skin, removal), a replay's fallback restoring values.
- `crates/shell`: the preview and a play pass the same key and values,
  Dito's and a chosen skin's; `Lighter` gives the built-in arrows in
  both.
- Desktop, offscreen on the GPU (`field_skin_offscreen.rs`,
  `play_offscreen.rs`): the preview shows each color's skin and the
  built-in arrows for `Lighter`; the preview's skin and a play's are the
  same skin with the same values (`Explosions` off reaching the Lua), a
  Lua stop still counted against Dito; a play draws Dito Flat and the
  lighter built-in arrows.

**Done 2026-10-21.** Checked in headless Chrome on WebGPU and on WebGL2
(the Graphics setting, so Options' preview too) and in Playwright's
Firefox (WebGL2): settings stored before (`note_colors` Rainbow,
`chart_colors` on, `noteskin` `builtin`) load as Dito's parameters;
Options' Arrows lists `noteskin`, `skin.Colors`, `skin.ChartColors`,
`skin.Explosions`, `skin.Lighter`, and changing them by keys moves the
preview to Dito Rainbow, Dito (by subdivision) and Dito Flat, its Lua
reading `Explosions` off; an autoplay then draws Dito Flat with the same
values through its Lua (offscreen frames on WebGL2), its replay draws
the same after the settings changed to Single, and `Lighter` plays with
no skin and, on WebGL2, no offscreen frames. No console errors. Not
checked: Safari (the maintainer), the installed Firefox (a pending
update stalls its headless launch).

## Peters-DDR (2026-10-22)

Peters-DDR now uses the same declarations: Colors (Vivid / By subdivision)
and Explosions. The two old bundled choices migrate to the combined skin,
retaining their colors; compatibility names remain hidden variants. See
[bundled-skins.md](bundled-skins.md#peters-ddr-consolidation-2026-10-22).

## SkinCraft (2026-10-22)

The companion editor now reads these declarations for its preview controls
and MCP. Preview selections are saved per project and passed to the same
loader; variant assets remain available for editing and export. Runtime
defaults stay in metrics.ini. Dito's lighter drawing and color choices use
the shared settings bridge; the demo has no song-specific colors.

## Later

- A color parameter's swatches drawn in its row (the row shows the
  palette's names).
