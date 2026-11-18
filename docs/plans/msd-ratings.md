# MSD chart ratings and a skill radar

The maintainer (2026-11-18): chart ratings of the game's own in place of the
simfiles' difficulty numbers, and a skill radar, both behind options Off by
default. Their own simulation-based rating (`teacher-v1`) is not ready
("too slow and too flat a scale"), so until it is: "Maybe we can use MSD
for the feature and replace it with a new system when it's ready."

MSD is Etterna's chart difficulty (its calculator, MinaCalc, MIT): an
overall rating and seven skillsets for keyboard play, at a music rate and
a score goal; the listing is rate 1.0, goal 93%.

## Stage A: the calculator (built, 2026-11-18)

- `crates/msd/` (`ddi-msd`): MinaCalc version 527 at etternagame/etterna
  `2d706147`, translated file for file for four keys; its README says what
  was translated, what was left out (five to seven keys, debug output, the
  XML parameters) and where it differs on purpose (a new state every
  rating). Pure, natively tested, built for wasm; `libm` (already in the
  tree) for `erf`, `erfc` and `pow`, so native and wasm agree to the bit;
  `fastsqrt` as upstream's ARM build computes it, in software.
- A chart's rows (`ddi_msd::chart::rows`): taps and hold and roll heads in
  judged time as bits; mines, lifts, fakes and keysounds a row without
  bits (the first row of any kind is the time origin, as in Etterna). Rated
  layouts: four lanes for one player, `ddi_msd::chart::four_keys` (of the
  built-in layouts dance-single, generated versions on it included; the
  charts the CPU plays). Every other chart keeps the simfile's number.
- The oracle: `tools/msd-oracle/build.sh` builds upstream's calculator by
  itself from the pinned commit (fetched into `target/msd-oracle/`) as a
  small program; `crates/msd/tests/oracle.rs` (opt-in) compares the port
  against it. Synthetic and bundled charts: 113, 109 identical to the bit,
  the largest difference 4e-6; a local corpus of 5,656 charts: 5,436
  identical, the rest within 2e-6. Tolerance 0.005; the oracle's numbers
  for the synthetic and bundled charts are committed
  (`crates/msd/tests/fixture.json`) and checked natively and under WASI.
- Cost: about 0.2 ms a chart natively (release, median; worst 4 ms), 0.3 ms
  debug, 0.4 ms in wasm.

## Stage B: the feature (built, 2026-11-18)

- **Options** (Options → Display, under Menus: they change what song
  select, the prompt and the results show, not the notes played, which is
  Notes'): "Chart ratings: From the song / MSD"
  (`Settings::chart_ratings`, `ChartRatings`) and "Skill radar: Off / On"
  (`Settings::skill_radar`), both Off by default and left out of the
  stored settings at their defaults, so settings read and write as before.
  The descriptions say what MSD is: a rating computed from the chart's
  patterns for keyboard play at normal speed, after Etterna's calculator
  (naming the rating's own source, as a ruleset's is named; AGENTS.md's
  rule keeps other games out of public copy, not out of an attribution).
  `Settings::show_ratings` tells `ddi_ui::state::ratings` (at each
  `choose_language`, at boot and on every web route, and when the rows
  change).
- **What is rated, where, and when** (`ddi_ui::state::ratings`): the
  bundled songs' four-lane charts at build time (`xtask gen-songs` writes
  `ChartInfo::msd`, an additive manifest field, to the hundredth; read by
  both shells and remembered by digest for the session). Imported (web)
  and library (desktop) charts in the background, only while an option is
  On: song select offers its songs whose four-lane charts have none, the
  focused song and six either side first (`offer`, `first`); a shell takes
  one song at a time (`next`), rates it off its main thread and hands the
  numbers back (`done`). Desktop: a thread of its own (`crate::ratings`),
  reading the chart as a play does, driven from the loop beside the kept
  plays' reworker, waking it when a song is done. Web: a worker of its own
  (`crates/msd-worker`, built by Trunk and named by its hash as the Opus
  and SVG workers are, `xtask stage-msd`; a new worker rather than the
  SVG one, whose job and lifetime are a skin's), started with the first
  song and ended after 10 s idle; the page reads the stored simfile and
  posts its text, the worker parses it as the library does and rates it
  (`crate::ratings`). Dancing☆Onigiri works and FFR levels are not rated
  in the background (their generated four-lane versions keep their songs'
  numbers); a room's shared song is.
- **Paused during play**: nothing is taken from the prompt's Start to the
  results (`ratings::set_playing`: desktop's play stage, the web's
  `history::set_playing`, the same points the reworker yields at), as the
  maintainer asked; a song already handed over finishes off the main
  thread.
- **Cache**: by chart digest and calculator version, under
  `ddi.msd.v1` in the device's local storage (the web's `localStorage`,
  desktop's `storage.json`), apart from the settings: a chart 64
  characters (its digest, then the eight numbers in hundredths, hex), about
  640 kB at 10,000 charts, written every 50 charts and when the queue
  empties. A new calculator version forgets them and rates again. With both
  options Off nothing is read, written, offered or started.
- **The number** where Chart ratings is MSD and the chart has one
  (`ChartEntry::meter_text`, `meter_text_fine`, `msd`, `meter_is_songs`):
  song select's list chips (whole), its chart buttons (whole; a phone's,
  a room's short side), its ladder rows (to a tenth, the song's own number
  said first in the row's line, "Song 9 · 300 notes"), the Play button and
  a room's chart line (`chart_shown`, whole), the plays list's title, the
  start prompt's chart line (`PromptChart::msd`, `chart_line_rated`, to a
  tenth), the results' header (the prompt's line on desktop; the web's own
  and desktop's kept results, `ratings::shown_msd`), a room's players panel
  and status lines (`meter_text`). The number keeps its difficulty's
  color. A chart without MSD (not four lanes, or not rated yet) shows the
  song's number at 55% opacity where it is a part of its own (chips,
  buttons, rows): quiet, and the same color. Sorting, the ladder's order,
  the Adaptive ladder, the CPUs' eligibility, the announcer's hard-chart
  comment, the editor and the room's content card (agreed between peers)
  keep reading the song's number.
- **The radar** (`ddi_ui::radar`, song select's `fill_radar`): the seven
  skillsets, Stream at the top then clockwise, each as a share of the
  chart's overall (the outer ring is the overall, rings a quarter apart),
  so shapes compare as profiles while the number says how hard; drawn from
  quads (rings and spokes as thin bars, the fill as seven triangles cut by
  two turned frame clips each, the outline over it), in the theme's
  `Radar.*` colors, with short labels. It overlays the banner's right end
  (1.5 times as wide as tall, 8 px in) in the wide layout, for a rated chart
  only, so nothing moves when it comes or goes; a phone's and a room's
  short side have no room and show none. With the pointer on it the eight
  numbers are listed in its place.
- **In a room**: ratings are each device's own (nothing is sent; the room
  model's revision is untouched). The picked chart's song is rated first
  where it has none (`want_first`), so a follower sees its MSD once their
  device has it.
- **Debug**: `__DDI_DEBUG.ratings` on the web (wanted, paused, pending,
  rating, rated, known, the worker's songs and time).

Not done: the radar's numbers on keyboard focus (hover only); MSD at other
rates (the listing's 1.0 only); works and FFR levels rated in the
background.
