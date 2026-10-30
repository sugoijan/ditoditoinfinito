# Design brief: our theme, first version (2026-10-15)

Step 5 of `docs/plans/ui-core.md`. The look is the maintainer's to
choose; this brief and the mockups are what they chose from. The mockups
are on a private design canvas shared with the maintainer. Banners and
backgrounds in them are the bundled songs' own (CC BY), and every other
song in them is a sample row.

## Play HUD study (2026-10-29)

Accepted and ported to the shared play renderer; see [Play HUD](play-hud.md).
`tools/play-hud-design/approved.json` records the maintainer's final controls.
Keep Iosevka for now: ExtraBold judgments, SemiBold score and combo. The
quiet jade/mint gauge shares turbulence with coral damage and blue recovery,
becomes Vivid at full life, and respects reduced motion. Combo hundreds
have a real transparent outline gap; at 1000 the white and color invert
and the text grows. The still wordmark keeps flat gray D/D/I outlines at
10% opacity. The browser study remains available for later font exploration.

## Decided (2026-10-15)

The maintainer chose:

- **Layout A, the wheel**, for song select. B and C stay on the canvas
  for reference only.
- **Iosevka Term for text wherever it has the glyphs.** Japanese and other
  scripts fall back to the system's fonts, the browser drawing the text
  (P4). Play numbers use Iosevka Term SemiBold since 2026-10-29; judgments keep the HUD's Iosevka ExtraBold. The maintainer is not
  yet sure about the face, so it stays one tunable.
- **Today's pink accent**, `#ff6ea0`.
- **The selected song's background, blurred, behind the menus.** Which
  picture that is was settled by the maintainer on 2026-10-16: when the
  first background change is a movie at or before the first note, the
  movie's frame at that point is the song's background everywhere (song
  select's backdrop, the start prompt, the field before the first
  change), kept as a still taken at import (and on the first play for
  songs imported before), so the menus never load the video decoder for
  it; otherwise the song's `#BACKGROUND`, as StepMania shows it. Done
  2026-10-16 (`docs/plans/video-backgrounds.md`, step 8). The lead's
  refinement, for the maintainer to overrule: a blank still (a black or
  white opening frame, as Bad Apple!!'s) shows nothing of the song, so
  song select's backdrop and the results show the `#BACKGROUND` then;
  the prompt and the field keep the still, so the cut stays seamless.
  With Video off (the movie never plays) the prompt and the field show
  the `#BACKGROUND`, and song select keeps the still.
- **The beat pulse** (removed 2026-10-30: the active outline, below,
  hid it; nothing of it showed).
- **The style selector moves next to the chart ladder.** The song list
  then shows every song whatever the style:
  - a song with no chart in the selected style is greyed out, not
    hidden;
  - each style is lit where the selected song has charts in it.

  This replaces today's filtering by style and its "also found in"
  line. It belongs to step 6.2.

The maintainer expects many more adjustments later; this is the first
version.

## Built (step 5, 2026-10-15)

The theme is `ddi_theme::Infinito` (`theme/`), its tunables in
`crates/theme/src/infinito.toml`; the screens are `ddi_ui::screens`, run on
sample data at `#/ui-dev?screen=select|results|options` and with the
desktop's `--screen`. Where the build differs from the mockups:

- **Corners, rings, the selected row's shadow, grey banners, spaced
  small capitals and "…"** are drawn as the mockups have them, through
  our extensions to actors (`docs/plans/ui-core.md`, step 5). Long
  titles are squeezed a little, then cut.
- **The backdrop** uses a cached Gaussian-blurred image (2026-10-18),
  decoded at most 256 physical pixels long, independent of display scale.
  Two GPU passes filter it once on upload (σ = 6 texels, radius 18,
  clamped edges, premultiplied alpha during filtering); it then draws once
  per layer, at 0.2 (results 0.14). Its cache key stays separate from sharp
  images, so a loaded full-size picture cannot replace the blur. The start
  prompt keeps that cover-fitted base; its framed content enters as a
  second copy, retaining a light blur until Start. New images keep their 0.35 s crossfade. A song with no background shows
  the play's plain field color, cross-faded like the rest (the
  maintainer, 2026-10-16: the blurred background is a step on the way
  into the song, and a banner blown up is not one).
- **The subtitle** ("(OutFox Edit)") sits at the right of the title line
  in song select, smaller.
- **Compact:** the icon buttons show ‹, / and ••• (glyphs the face has);
  options and results have phone layouts of their own (below).
- **Options' field preview** is a placeholder until step 6 draws the
  field there. Unlit tabs and the empty ladder have square dashes.
- **Pages** (calibration's menu, the credits; step 6.4): one column,
  at most 880 wide, under the header: titles at 20, entries at 16,
  text at 14, notes at 13 in the muted color, links in the accent,
  lit on the selected plate when focused, buttons as the theme's
  (`PlayButton` for the first test).
- **The device notices** (step 6.4): a band across the top in the
  second plate color with an accent line under it, the text at 15
  (13 on a phone), "calibrate" as a `PlayButton` and the rest as
  `Link`s, beside the text where they fit, else under it, "F2" faint at
  its top right; the screens lie below it; the short "devices: …" line
  a pill at the bottom.
- **The autoplay switch** (2026-10-16): a `Pill` at the header's right
  of song select ("Autoplay off", lit "AUTOPLAY ON"), the mods left of
  it; on a phone an `IconButton` "AUTO", left of "+", and the play
  button reads "AUTOPLAY HARD 11". The results show the lamp AUTOPLAY,
  "· AUTOPLAY" in the chart line and a note. **Removed 2026-10-26** (the
  maintainer: not useful): autoplay is the address's `auto=1` (and
  desktop's `--auto`) alone; the play button still says AUTOPLAY then.
- **The screens' header** (2026-10-26, the maintainer's request): one
  `Screen.Header` on song select, Options, the pages (the credits,
  calibration's menu), the results and the editor: a Back button at the
  left (`Back`: three bars on a plain plate, `BackButton`, the compact
  layouts' too, in place of "‹"), then one row: the Dosis wordmark
  filling the header's height (44 px, live as the start menu's title:
  `screens::Live` ticks its vivid clock on every screen that has one,
  still under reduced motion), a thin divider, the screen's name in
  small muted caps ("SELECT MUSIC", "OPTIONS", "CREDITS"), the
  maintainer's choice over a small wordmark above a bold name and over a
  larger one above the caps. `Header.TitleEnd` (460) says where the row
  ends; song select's search field shrinks to the room left past it and
  the mods. Song select's search
  field moved to the header's right, the mods left of it, and its
  "Esc / Backspace: start menu" hint became the footer's "ESC Start
  menu".
- **Song select upright on a phone** (2026-10-16): 480 wide zoomed to
  the width (`Compact.Upright.Margin` 12 at the sides): the compact bar
  on top; the song's banner the column's width, its title and line;
  the style button; the five chart buttons, 52 high; the wheel under
  them filling the height, its rows 52 high; the notices' line; Play,
  60 high, across the bottom (a thumb's reach, the opposite of the
  mockups' sideways column, where it closes the song's side).
- **Results on a phone** (2026-10-16): upright the layout is 480 wide
  zoomed to the width, sideways the screen as it is; a bar at the top
  (Back "‹", "Results", the panel sets' tabs), one scrolled column (the score
  column, the chosen set's panels stacked, the notes; sideways the
  panels beside the score), the offset and the buttons at the bottom
  (two rows upright).
- **Accent buttons' focus:** the ring inside the plate, in the label's
  color (an accent ring around an accent plate barely shows).
- **Shortcut caps** (2026-10-26): original OEM-inspired SVG paths and
  gradients in `assets/ui/keycaps-a1.svg` (charcoal satin) and
  `keycaps-a2.svg` (charcoal semi-gloss), and `keycaps-light.svg` (light gray
  satin with dark ink), plus `keycaps-gray.svg` (slightly darker neutral gray).
  Both grays use equal RGB channels and near-black `#111111` printing.
  All alternatives are retained;
  `Infinito::set_keycap_material` selects a finish before rebuilding screens,
  with deeper gray satin (`keycaps-deep-gray.svg`, `#7d7d7d` face) the selected
  app default. The previous `#858585` option remains available. The look follows the original Blender
  sculpts and selected B+D monitor lighting (60 cm behind, 27 cm high,
  15° back tilt, faint room light). `tools/keycap-design/charcoal_vector.py`
  generates the bodies parametrically, including standalone pressed references.
  The current sprite bridge retains four width presets plus convex Space;
  it still scales the nearest preset for arbitrary label widths. `KeyHint`
  supplies the SVG atlas through `Look::image`; the helper binds `CapN` parts.
  Song select shows separate up/down caps for Song and Chart; Chart's `2×`
  is ordinary text through `Spec::key_prefix`. All four navigation arrows
  retain their bindings. The selected size is 26 px with independent 10 px
  legends, no extra gap between caps and 18 px between shortcuts. The
  interactive study in `tools/keycap-design/playground.html` now compares
  the final deeper gray and previous gray with the selected Iosevka Term Regular Italic
  printing. Desktop and web load its real italic face. The light option's
  diffuse face tones are lifted from a separate B+D Blender reference for
  small dark legends' contrast; the geometric highlight placement is shared.
  UI image loaders use the existing checked SVG subset and bounded rasterizer
  on desktop, and the SVG worker with browser fallback on web. No Blender
  runtime or bitmap embedded in SVG is needed. Legends remain live text and
  vector marks, with a dark fallback plate during loading.
  Caps are 26 px high, with 10 px upper-left printing (lowercase names,
  uppercase letters). `ddi_actor::text::TextCurve` bends adjacent strips of
  the shaped text atlas cell without changing the font cache; the curve
  follows the Blender projection (0.568 px full dish depth on the original
  32 px artboard, scaled to the cap height). Enter and Tab
  carry keyboard marks. Space has a convex crown, centered label and 52 px
  minimum width. Typed `Keycap` requests separate keyboard identities from
  custom labels on `Normal`/`Space` shapes. Equal vector arrows include
  composite captions; individual arrows use Cherry-style mixed placement.
  The authoring README records references, preserved 3D/material/lighting
  studies and A1/A2 pressed renders (4 mm travel, about 2.27 px projected at
  the 32 px artboard). Pressed runtime behavior remains a later step.
- **The volume** is a slider row: a thin track, the accent fill, a
  light knob, the value and the dB note after it. A fresh profile
  starts at 69% (−10 dB, `settings::DEFAULT_VOLUME`); stored settings
  keep their volume.
- **The compact threshold** is 480 CSS pixels of height
  (`Compact.Below`) or 600 of width (`Compact.Narrower`: a portrait
  phone gets the compact layout, scaled to its width).

Song select on the real library (step 6.2, 2026-10-16), where it differs
from the mockups:

- **Bests:** on the ladder row the grade (numbers face), the score under
  it, the lamp as a short word (FAILED, CLEAR, FC, GFC, PFC, MFC) in its
  color (`Lamp.*`) and a bar of that color at the row's end; a line
  under each wheel meter and each compact chart button.
- **Notices** (storage, errors, controllers) are one line over the song
  column, cut to fit, their links after it.
- **Dialogs** for adding songs and for managing a pack or a song, on the
  `Dialog` panel with the theme's buttons and toggles.
- **The wheel's rows** reserve 8 px on each side for the active outline,
  keeping its 3 px line and 3 px gap inside the list's clip.
- **Compact** has a "+" icon for adding songs. Pack headers have no
  banners yet.

## Starting point

The maintainer (2026-10-15): close to today's pages, clean, on a black or
dark background, but laid out more as rhythm games lay out their screens.
"Today's pages" means:

- the dark scheme of `styles.css` (ground `#161318`, text `#f2eee9`,
  muted `#9a938c`, accent `#ff6ea0`);
- system fonts, with the HUD in Iosevka ExtraBold;
- one centred column per page, with the difficulty pills colored as
  StepMania colors difficulties.

## What changes: packed like a game screen

A web page scrolls, and its column runs as long as its content. A game
screen is fixed instead: it holds everything a choice needs at once, in
regions that stay put while the selection moves through them. The
mockups keep the colors and the cleanliness, and change the packing:

- **Fixed regions on a 1280×720 design.** This is the theme's design
  size, set as a StepMania theme sets it, for the virtual screen of
  `ddi_actor::screen` (the developer screen uses 960×540). Wider displays widen the screen and narrower
  ones heighten it, as StepMania's do. Header and footer stretch, and the
  content regions keep their size and anchor to their side.
- **Header:** the screen's name and context (style, mods, panel
  choice). **Footer:** what each key does right now, as key caps,
  changing with the screen's state (keyboard and pad players read it;
  touch hides it).
- **The selection is the brightest thing on screen.** It gets a 2 px
  accent ring, a raised plate, and a bigger row in the wheel. Everything
  else is a quiet plate on the dark ground. Color comes from the songs'
  art and the difficulty and judgement colors, never from the chrome.
- **The selected song's art behind the screen:** its background
  blurred and at about a fifth of its strength (a tweak in the
  mockups). The screen stays black at a glance, but each song tints it.
- **Readable from a pad's distance:** titles 18–34 px, meters and
  scores large, in the HUD's face.
- **Numbers in the HUD's face** (Iosevka ExtraBold, already bundled):
  meters, BPM, lengths, scores, counts and key caps. This ties the menus
  to play.
- **Touch targets of 44 px or more** wherever a finger goes.

## The system the mockups use

- **Ground** `#0a090d`, between today's page (`#161318`) and the play
  screen (`#050308`).
- **Plates** `#14121a` and `#1c1924`, raised `#241f2d`.
- **Lines** `#26222e` and `#3a3542`.
- **Text** `#f2eee9`, secondary `#c9c2bb`, muted `#a39c95` (17:1, 11:1
  and 7:1 on the ground; every difficulty color is above 4.5:1 on every
  plate).
- **Accent:** today's `#ff6ea0`. The mockups offer cyan, gold and
  violet as alternatives.
- **Difficulty colors:** today's hues, lifted for a dark ground so a
  meter in color stays readable: Beginner `#4ea3f0`, Easy `#49b86a`,
  Medium `#d9a925`, Hard `#e8584a`, Challenge `#a979ea`, Edit `#8f8a94`.
- **Judgement colors:** the HUD's (`crates/render/src/scene.rs`), so results
  read the same as play.
- **Radii:** 4–10 px. **Spacing:** multiples of 2 and 4. No gradients,
  no glows beyond the selection's shadow.
- **Text face: Iosevka Term** (chosen), Regular, SemiBold and Bold,
  subset to Latin-1, Latin Extended-A and the theme's symbols (27 KB a
  weight, `assets/fonts/PROVENANCE.toml`). The mockups also offer the
  candidates first shown (Saira, Chakra Petch, M PLUS 1, the system
  font) for comparison.

  Japanese titles fall back to M PLUS 1 in the mockups. In the game they
  fall back to the system's Japanese font, the browser drawing the text
  (P4). Bundling a Japanese face is a separate decision: a Japanese face
  subset to JIS levels 1 and 2 is still about a megabyte per weight.

## Song select: three layouts

All three show the same content, which is what the song list shows
today:

- packs and songs, with banners;
- the style selector and search;
- per chart, its difficulty, meter, notes and step artist;
- the song's BPM, its length (new), and the mods in effect;
- the preview playing;
- import, options, calibration and credits.

**A. Wheel.** The classic cabinet layout:

- **Left:** the selected song large: banner, title, BPM and length, the
  style tabs (lit where the song has charts, with how many; Tab moves to
  the next), then the chart ladder for that style (one row per chart, the
  meter big in its color). Changing style keeps the chart's difficulty,
  or the closest. A song with no chart in the style says so in place of
  the ladder.
- **Right:** the wheel. The selected row sits on a fixed line, and the
  rows away from it drift right and fade (`Look::wheel` already does
  this).
- **Packs** are headers in the wheel that open and close.
- **Every song shows in every style;** a song with no chart in the
  selected style is greyed (dimmed, its banner grey, "no Double charts"
  in place of its meters) and can still be selected.
- **Keys:** all four arrows browse songs (Up/Left previous, Down/Right
  next). First taps preview a move without changing song or preview audio;
  commit follows the same continuous curve. Isolated double taps change
  chart (Up/Left easier, Down/Right harder), including quick opposite
  pairs without transient song moves. Repeated tapping browses immediately;
  holds accelerate to a cap and stop on release. The tuned timings and
  input/animation contract are in [`song-navigation.md`](song-navigation.md).
  Enter/Tab resolves pending input; search, Back, pointers, focus loss
  and overlays cancel it. Header and text navigation keep their controls.

Built from widgets that exist (the wheel-style list, option rows). The
search mockup is in this layout:

- the field takes the header;
- suggestions drop under it, kana and romaji matching each other;
- the wheel shows the matches;
- while composing, the field shows the composition underlined and no
  popup (a tweak shows that state).

**B. Tiles.** Banners in a grid under pack tabs, with a detail panel on
the right holding the charts as buttons and a large Play button.

- Best for touch and mouse. Pads move in two dimensions, so they need one
  more key for charts.
- Needs a new widget: a virtual grid. The list's recycling carries over.

**C. Index.** Closest to today's pages:

- **Left:** a dense list with one line per song (title, artist, BPM, a
  meter in each difficulty's column) under pack headings.
- **Right:** the selected song with its charts as a table.
- **Look:** quiet and typographic, and fastest to scan on a big library.
  The least "game" of the three.

## The other screens (drawn in layout A)

- **Results.**
  - **Left:** grade, score and EX, the judgement counts with bars,
    held/let go, max combo, mines, fast/slow.
  - **Right:** panels the player picks. "Overview" shows four panels:
    timing histogram, life, offsets over the song, and misses by lane.
    "Timing" shows a large histogram and the offsets. "Customize…" is
    where a player would pick their own.
  - **Bottom:** the song offset editor, then Song list and Retry.

  The numbers are a sample ITG play. The panels need the per-hit record
  that step 6.1 adds to the engine.

  **Since 2026-10-25** (the maintainer's request): the panel sets were
  tabs over the panels (`Tab`: a muted label, bright over an accent
  underline when selected; a line under the strip), no "Panels" label;
  on a phone in the bar as before. **Since 2026-10-28** the tabs are
  gone: the main set (Overview) alone shows, the other sets waiting for
  a picker (`Settings::results_panels` stays); the bar is Retry then
  Song list, plain buttons both (Retry's accent look read as a focus
  beside the outline on Song list), Left and Right moving the focus
  along them; a quit play's grade is QUIT in the danger color, no lamp.
  The life panel's note is the largest
  drop: the largest fall within 4 s (`results::DROP_WINDOW`; a lowest
  never says more than where an ITG gauge starts, 50%), at when it
  began, the fall shaded on the graph ("no drop" when it never fell). A
  By beat panel takes By lane's place in Overview and joins Timing: a
  row per place in the beat of the notes (4th, 8th, 12th, 16th, … in
  that order), its notes, the mean as a tick and a standard deviation
  either way as a bar on the timing panels' scale over the best window,
  and the numbers; places with fewer than 5 notes (or 2% of them), and
  past the room the least played, go together into Other (a single one
  keeps its name). By lane stays in Song.
- **Options.**
  - **Left:** sections.
  - **Middle:** option rows as StepMania lays them out. The label is on
    the left and the choices are on the right, with the chosen one
    underlined; numbers use a ◀ value ▶ stepper.
  - **Right:** a live preview of the field with the current speed,
    reverse and arrows.
  - **Below:** what the focused option does, in a sentence.
  - **On a phone** (step 6.3): sideways, the sections as a strip of
    tabs (`StyleTab`) under a back icon, the rows down the whole
    height, and beside them a column (`Options.Compact.Preview` wide)
    with the preview and, under it, what the focused option does;
    upright, the layout 480 wide (`Options.Narrow.Width`) zoomed to the
    screen's width, the sentence and then the preview under the rows.
    The sentence wraps; its panel is as tall as the longest needs. Choices too many for a row are stepped one at a
    time (◀ value ▶); rows of buttons (bind, reset, export) use the
    small `Button`, the lit one in its focus look.
- **Phone, landscape (844×390 CSS px).** The 1280×720 design scaled to a
  phone's height would set titles at 10 px, so small screens get a
  compact layout of the same screen.
  - **Kept:** the wheel and the selected song.
  - **Changed:** charts as buttons, a large Play button, and icon buttons
    for back, search and options. There are no key hints.
  - **Margins:** 48 px kept clear for the notch.

## Motion (not shown in the mockups)

- **Selection moves:** 100–150 ms, decelerating.
- **Rows:** slide in and out along the wheel's curve.
- **Screens:** cross-fade, with a short slide (see "Transitions" below
  for what was built).
- **Off with reduced motion:** the slides, and the art held still. This
  follows the system's reduced-motion setting, and a setting of our own
  overrides it.

### Transitions (2026-10-26)

The maintainer's request: the menus' parts move in from the edges they
belong to and leave to them, rather than the whole screen fading and
sliding as one. `ddi_ui::screens::transitions` gives each named child of
a screen's root `On`, `Off` and `Cancel` commands for its edge: the
header's row (the header, the search, the mods, the compact layouts'
Back, title and icons) from above, as the start menu's title; the footer
(and the start menu's choices, the results' buttons, Options'
description) from below; a list (song select's wheel and its bar,
Options' rows and preview, a page's column, the results' panels) from the
right; a song's side (song select's song panel, Options' sections, the
results' score) from the left; the song's backdrop fading where it is.
A part slides `Motion.Slide.X` (sideways) or `Motion.Slide.Y` (up or
down) virtual pixels over `Motion.Screen` in, decelerating, and
`Motion.Screen.Out` out, accelerating, fading with it; the sides follow
the top and bottom by `Motion.Stagger`. Out, a part goes back to its
place unseen, so that coming in again slides it from the same edge.
Reduced motion keeps the fades alone. The chart editor keeps the whole
screen's fade and short slide (`transitions_whole`). A part built again
while it moves (song select's footer when its hints change, Options' rows,
a page's column) keeps its place and tweens: only its contents change
(`refill`); a screen laid out again while it comes in (the web's device
notices take their strip at the top right after a page is built, the
credits arrive later) goes on coming in from as far as it had come
(`relaid`).

Screens follow each other, never cross, on both shells (`ddi_ui::Ui`,
"Transitions"): a screen pushed over another waits for the one under it to
leave (its `Off`), which stays covered as that left it and comes in again
(`On`) once the screen over it has gone; a `Navigate` or `Back` request
from a screen (the web's way between its pages) plays the screen's `Off`
(or `Cancel`) first and goes out to the shell once it has left, the
layer kept as it ended until the page changes. So the web shows the
same sequence as desktop's stack, out then in, and the browser's own Back
(which changes the address before the page could leave) shows the
entrance alone. The shell's own `Ui::push` (the first screen, desktop's
`--screen`) is immediate. A screen takes input as soon as it starts
coming in; a press while the screen before it leaves is dropped
(2026-10-30, `ui-core-mechanics.md`). Checked natively (`crates/ui/src/ui.rs`'
transition tests, `crates/theme/src/tests.rs`) and in Chrome
(`target/scratch/pw/transitions.mjs`: the parts' places frame by frame
between the start menu and song select, Options and the credits, both
ways).

### Into the song (2026-10-20)

Song select, the shared start prompt and play use the same full-screen,
cover-fitted Gaussian base over `Play.Field` (#05020d), at opacity 0.2.
The prompt carries the menu's cached source while the play loads. Above it,
a copy contracts from full screen to the configured background frame in
450 ms, easing toward the background brightness setting and a softer blur
(σ = 6 at up to 1024 pixels, versus 256 for the base). It stays softly
blurred while waiting for Start. The card fades independently of the base.

Start finishes the unblur in 300 ms. An early Start continues from the
prompt's current position, without delaying audio or changing its clock.
Reduced motion shows each settled state immediately. Both shells use
`ddi_actor::background` for geometry and progress. Play keeps the blurred
base, even with a screen-sized content frame, and it follows the current
images, videos, crossfades and second background layer. Static sources reuse
their filtered copies; video uploads and changed layer weights refresh them.
Returning from the prompt uses the ordinary menu transition.

## The theme in code

The theme becomes code in the shape P6 set (`crates/ui/src/look.rs`):

- a `Look` with the components the screens of step 6 ask for (`SongList.Row`,
  `SongList.Header`, `ChartLadder`, `Panel`, `OptionRow`, `KeyHint`, …);
- a tunables file holding the colors, sizes and motion above;
- the chosen OFL face, subset to Latin, under `assets/fonts/` with its
  `PROVENANCE.toml`;
- art only if it is ours or freely licensed, each with its provenance.

Then step 6 replaces the Yew pages one by one, results first (done
2026-10-16; Yew is gone).

## Start menu (2026-10-21)

The shared `StartScreen` is our `ScreenTitleMenu` boundary: stable
Singleplayer, Multiplayer, Options, Credits and Quit actor IDs. The host
supplies enabled actions and desktop capability; disabled Multiplayer has
a focus target with a muted button plate but no action; Quit is last and
web omits it. `TitleMenu.Primary`,
`TitleMenu.Choice` and `TitleMenu.Disabled` components dress the choices;
`TitleMenu.Width`, `RowHeight`, `Gap`, `TitleWidth` and `TitleHeight` in `infinito.toml`
control their layout. Touch rows retain a 44 CSS pixel minimum. Start uses
the theme ground with no song backdrop or audio. Ordinary
On/Off and focus commands apply. Importing theme metrics/Lua screen flow
is still future work. See [start-menu.md](start-menu.md).

### Dosis title wordmark

The start menu's `TitleMenu.Title` is Dosis ExtraBold artwork, with white
lettering and Infinito Vivid outlines on D/D/I. Outline and protective gap
are both 12 units at a 96-unit cap height; the gap protects every letter.
The theme supplies a four-frame mask atlas and field coordinates, generated
by `tools/title-design/`. The screen binds the component's named image and
advances its Vivid clock. The shared 2D painter uses the noteskin's actual
material and noise prelude: palette phase cycles every four seconds while
the spatial field evolves on unwrapped time. Reduced motion freezes both
and allows the screen to idle; resizing preserves the clock. The look's
ordinary text title remains the fallback. The app's other text faces stay
as configured in the theme.


### Title-menu outline rain (2026-10-27)

`rain.rs` dresses `TitleMenu.Background` with 50 small outlines. The
`assets/ui/menu-rain.svg` atlas uses Infinito's outer arrow/onigiri contours,
four baked cardinal orientations and eight stroke weights, keeping the
outlines about 1.15 study pixels wide at their different depths. Onigiri
keeps its note alignment and upright orientation. A deterministic seed
chooses depth and position; the shape changes only at an offscreen respawn,
with a 1% onigiri chance. Portrait resizing preserves glyph proportions.

The existing Vivid painter samples a fixed snapshot (phase .363, seconds
9.04, a 96-unit field) with 35% saturation. Every sprite's field rectangle
comes from its viewport position, so colors belong to the screen while the
outlines move through them. This recreates the approved shared-texture
study with the same material rather than shipping another bitmap. A quiet
center preserves the title/menu's contrast. `Look::animate_component`
receives a screen-owned elapsed clock; reduced motion holds both title and
rain still, letting the screen idle. Relayout keeps the clock. Theme metrics
`TitleMenu.RainSpeed` (8 study px/s) and `TitleMenu.RainStrength` (22) keep
this independent of song tempo and selection.


### Shared active-element outline (2026-10-27)

The theme uses a 3 px Infinito outline with a 3 px clear gap, configured by
`Active.Width` and `Active.Gap`. Start-menu plates are neutral, including
Singleplayer, whose label stays larger and heavier. The outline reveals one
viewport-anchored field with the title's four-second palette cadence and
8× spatial evolution (`Active.EvolutionSpeed`). Reduced motion freezes it.

The shared UI grants the decoration to only one input layer. Each themed
screen exposes its presentation and primary target: ordinary screens use
focus; song select uses the selected wheel item, options the active option
row, and reading pages the active link/fold. Dialogs own the decoration while
open; covered screens have none. Changing focus replaces the outline without
cross-fading two active outlines. Secondary choices retain their selected
plates, text and underlines, rather than receiving another animated ring.

`active.rs` derives the outline from the actual plate's geometry, including
animated wheel rows, wrappers, press transforms and transitions. The old
`Ring` and `Pulse` parts and song select's beat pulse were removed
(2026-10-30): the outline had hidden them. The outline is visual-only (`Actor::decoration`), excluded from control
bounds and hit-testing. Frame controls own it without changing a list's row
slots; primitive page hit areas use their manually-built parent. The wheel reserves 8 px at both sides so the
6 px total extension is not clipped. `Screen::presentation` and
`Screen::active_element` keep the policy shared by web, desktop and dialogs;
`Look::active_element` owns the artwork. `Ui` supplies one animation clock
and idle/reduced-motion handling. Other looks retain their existing behavior.


The pre-start card's `Prompt.Start` uses a neutral plate with white lettering.
Its countdown is a muted pink background wash revealed left to right below
the label, clipped against the full rounded button shape. `CountdownFill`
is a visual-only frame; the prompt sets its elapsed fraction in place,
without a separate countdown bar/text or a card rebuild every second.
Reduced motion advances the fill in whole-second steps.


A fresh start menu has no active target. `Screen::defer_initial_focus` keeps
its first control unfocused until the first keyboard/menu-button press,
which reveals Singleplayer without activation. Start-menu hover follows the
pointed button, while explicit return focus and direct clicks/taps still
work. The web shell clears saved start-route focus at boot so reloads also
start unfocused. This opt-in policy leaves other screens' navigation unchanged.


### Infinito cursor

Options → Display offers **Infinito arrow** (default), **Pointer tip** and
**System**, plus a 16–64 logical-pixel size (28 by default). Approved vector
masks in `assets/ui/cursors.svg` use the existing Vivid painter. The pointer tip
uses the study's 58° angle, full-width tail and 11.5-unit extension; the arrow
retains the note's original proportions. Both use a 5-unit band, no hard outer
edge, and an alpha-mask shadow (2 px blur, 30% black, 0.75 px down at 28 px
artwork size). Its mask includes the visible charge, so the soft shadow also
reaches the transparent interior edges. The burst is original Infinito art
copied into `assets/ui/cursor-burst.svg`; neither asset needs the authoring
tools or the player's chosen noteskin.

The cursor is empty away from enabled targets. Hover charges at 120 BPM;
mouse-down uses the last painted charge: full arrow → W1, full tail → W2,
lower tail → W3, empty → W4. Every outcome shines, progressively smaller.
Feedback stays at the hotspot while the pointer moves. This is visual feedback
only: click/tap activation, disabled controls and drag cancellation keep their
normal semantics. System text/grab cursors remain native. Focus loss, touch,
keyboard/controller use, or no mouse position hides the overlay. Reduced
motion freezes color and charge (full while hovering), removes burst growth,
and permits only the brief press fade. System bypasses all custom art/effects.

`Ui` owns the clock, hit testing and overlay lifetime. `Look::pointer` builds
presentation from `cursor::Spec`; `Env::follow_cursor` follows stored and live
options in screens sharing it. The overlay is appended after all UI layers,
outside their hit trees and clips; its size is corrected for UI scale and
pixel ratio. Shells honor `Cursor::Hidden`; desktop balances AppKit hide/unhide
on macOS to avoid winit's invisible-GIF issue. Gameplay retains its existing
pointer hiding and quit-button behavior.
