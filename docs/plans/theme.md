# Design brief: our theme, first version (2026-10-15)

Step 5 of `docs/plans/ui-core.md`. The look is the maintainer's to
choose; this brief and the mockups are what they chose from. The mockups
are on a private design canvas shared with the maintainer. Banners and
backgrounds in them are the bundled songs' own (CC BY), and every other
song in them is a sample row.

## Decided (2026-10-15)

The maintainer chose:

- **Layout A, the wheel**, for song select. B and C stay on the canvas
  for reference only.
- **Iosevka Term for text wherever it has the glyphs.** Japanese and other
  scripts fall back to the system's fonts, the browser drawing the text
  (P4). Numbers keep the HUD's Iosevka ExtraBold. The maintainer is not
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
- **The beat pulse.**
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
`theme/src/infinito.toml`; the screens are `ddi_ui::screens`, run on
sample data at `#/ui-dev?screen=select|results|options` and with the
desktop's `--screen`. Where the build differs from the mockups:

- **Corners, rings, the selected row's shadow, grey banners, spaced
  small capitals and "…"** are drawn as the mockups have them, through
  our extensions to actors (`docs/plans/ui-core.md`, step 5). Long
  titles are squeezed a little, then cut.
- **The backdrop** is the background asked for tiny and drawn stretched,
  five times a little apart, at 0.2 (results 0.14): close to the
  mockups' 30 px blur at no extra pass. A song with no background shows
  the theme's plain backdrop, cross-faded like the rest (the
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
  "· AUTOPLAY" in the chart line and a note.
- **Song select upright on a phone** (2026-10-16): 480 wide zoomed to
  the width (`Compact.Upright.Margin` 12 at the sides): the compact bar
  on top; the song's banner the column's width, its title and line;
  the style button; the five chart buttons, 52 high; the wheel under
  them filling the height, its rows 52 high; the notices' line; Play,
  60 high, across the bottom (a thumb's reach, the opposite of the
  mockups' sideways column, where it closes the song's side).
- **Results on a phone** (2026-10-16): upright the layout is 480 wide
  zoomed to the width, sideways the screen as it is; a bar at the top
  (Back "‹", "Results", the panel pills), one scrolled column (the score
  column, the chosen set's panels stacked, the notes; sideways the
  panels beside the score), the offset and the buttons at the bottom
  (two rows upright).
- **Accent buttons' focus:** the ring inside the plate, in the label's
  color (an accent ring around an accent plate barely shows).
- **The volume** is a slider row: a thin track, the accent fill, a
  light knob, the value and the dB note after it.
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
- **The wheel's rows** are drawn 5 px in, so the selected row's ring is
  never cut by the list's edge; the list starts 5 px further out, so
  nothing moved.
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
- **Judgement colors:** the HUD's (`render/src/scene.rs`), so results
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
- **Keys:** all four arrows move through songs (Up/Left previous,
  Down/Right next), after a 250 ms window for a possible second tap.
  Double-tapping the same arrow keeps the song and changes chart:
  Up/Left easier, Down/Right harder. Held arrows repeat through songs.
  Enter or Tab settles a queued song move first; search, Back, a pointer
  press or focus change cancels it. Header navigation and typing keep
  their usual focus and editing behavior.

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
- **Beat pulse:** the selected row and the accent pulse once per beat of
  the previewed song, at its BPM.
- **Rows:** slide in and out along the wheel's curve.
- **Screens:** cross-fade, with a short slide.
- **Off with reduced motion:** the beat pulse and the slides. This
  follows the system's reduced-motion setting, and a setting of our own
  overrides it.

### Proposal: into the song (with 6.4 part two's shared prompt)

The maintainer's idea (2026-10-16): song select's blurred backdrop is
where the song begins, so starting a chart should carry it on rather
than cut to another page: the blur sharpens into the start prompt's
picture, then the play's background. Not built; it belongs with the
shared start prompt of 6.4 part two. A sketch:

1. **Start on song select** (Enter, Play, a pad's Start): the screen
   does not leave at once. Its panels, wheel and footer slide and fade
   out over about 250 ms (`Off`, as screens do now) while the backdrop
   stays, and the play's loading starts in the same gesture (audio
   context, `play()` as today).
2. **The backdrop sharpens** over about 400 ms, decelerating: the five
   spread copies close in on the middle one (their offsets to 0), the
   strength goes from 0.2 to the prompt's dim (the background
   brightness setting), and the tiny texture is swapped for the full
   one when it is ready (the image cache has it by then: the prompt
   asks for it anyway). A song with no background goes from the plain
   backdrop to the prompt's, nothing to sharpen.
3. **The prompt fades in over it** (title, chart, "press Enter or tap
   to start"); its picture is the same backdrop actor, handed over, not
   drawn again, so nothing jumps. Desktop and the web do the same: the
   backdrop is the UI core's, the prompt is the shared one.
4. **Into play:** at the start, the play's background layer (its first
   frame, or the movie's still) cross-fades over the prompt's picture
   in the time the prompt takes to go (about 200 ms), so the last image
   of song select is the first of the song.
5. **Back** (Escape at the prompt) runs it backwards: blur, panels in.
6. **Reduced motion:** cross-fades only, no sliding or closing in;
   frames stop once each step settles.

What it needs: the backdrop as an actor the prompt can take over (one
texture, the spread and strength as tweened properties, not five fixed
copies), the play's loading overlapped with the transition, and the
prompt drawn by the UI core on both shells, which 6.4 part two brings.

## The theme in code

The theme becomes code in the shape P6 set (`ui/src/look.rs`):

- a `Look` with the components the screens of step 6 ask for (`SongList.Row`,
  `SongList.Header`, `ChartLadder`, `Panel`, `OptionRow`, `KeyHint`, …);
- a tunables file holding the colors, sizes and motion above;
- the chosen OFL face, subset to Latin, under `assets/fonts/` with its
  `PROVENANCE.toml`;
- art only if it is ours or freely licensed, each with its provenance.

Then step 6 replaces the Yew pages one by one, results first.
