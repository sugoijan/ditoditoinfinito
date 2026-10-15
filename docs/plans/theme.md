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
- **The selected song's background, blurred, behind the menus.** How
  the background is chosen is to be settled later. One idea from the
  maintainer: when a chart starts on a background video, take the
  video's first frame as the song's first background, and use that frame
  in the menus too.
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
  its banner.
- **The subtitle** ("(OutFox Edit)") sits at the right of the title line
  in song select, smaller.
- **Compact:** the icon buttons show ‹, / and ••• (glyphs the face has);
  results and options keep the large layout scaled down for now.
- **Options' field preview** is a placeholder until step 6 draws the
  field there. Unlit tabs and the empty ladder have square dashes.
- **The compact threshold** is 480 CSS pixels of height
  (`Compact.Below`).

## Starting point

The maintainer (2026-10-15): close to today's pages, clean, on a black or
dark background, but laid out more as rhythm games lay out their screens.
"Today's pages" means:

- the dark scheme of `styles.css` (ground `#161318`, text `#f2eee9`,
  muted `#9a938c`, accent `#ff6ea0`);
- system fonts, with the HUD in Iosevka ExtraBold;
- one centred column per page, with the difficulty pills coloured as
  StepMania colours difficulties.

## What changes: packed like a game screen

A web page scrolls, and its column runs as long as its content. A game
screen is fixed instead: it holds everything a choice needs at once, in
regions that stay put while the selection moves through them. The
mockups keep the colours and the cleanliness, and change the packing:

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
  else is a quiet plate on the dark ground. Colour comes from the songs'
  art and the difficulty and judgement colours, never from the chrome.
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
  and 7:1 on the ground; every difficulty colour is above 4.5:1 on every
  plate).
- **Accent:** today's `#ff6ea0`. The mockups offer cyan, gold and
  violet as alternatives.
- **Difficulty colours:** today's hues, lifted for a dark ground so a
  meter in colour stays readable: Beginner `#4ea3f0`, Easy `#49b86a`,
  Medium `#d9a925`, Hard `#e8584a`, Challenge `#a979ea`, Edit `#8f8a94`.
- **Judgement colours:** the HUD's (`render/src/scene.rs`), so results
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
  meter big in its colour). Changing style keeps the chart's difficulty,
  or the closest. A song with no chart in the style says so in place of
  the ladder.
- **Right:** the wheel. The selected row sits on a fixed line, and the
  rows away from it drift right and fade (`Look::wheel` already does
  this).
- **Packs** are headers in the wheel that open and close.
- **Every song shows in every style;** a song with no chart in the
  selected style is greyed (dimmed, its banner grey, "no Double charts"
  in place of its meters) and can still be selected.
- **Keys:** Up and Down move through songs, Left and Right through charts.

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

## The theme in code

The theme becomes code in the shape P6 set (`ui/src/look.rs`):

- a `Look` with the components the screens of step 6 ask for (`SongList.Row`,
  `SongList.Header`, `ChartLadder`, `Panel`, `OptionRow`, `KeyHint`, …);
- a tunables file holding the colours, sizes and motion above;
- the chosen OFL face, subset to Latin, under `assets/fonts/` with its
  `PROVENANCE.toml`;
- art only if it is ours or freely licensed, each with its provenance.

Then step 6 replaces the Yew pages one by one, results first.
