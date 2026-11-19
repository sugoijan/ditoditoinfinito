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
  `choose_language`, at boot, and when Options' rows change); the web's
  rating loop is looked for again on every route (`ratings::kick` from
  `App::show`, round Z1).
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
  (`crate::ratings`). A room's shared song is rated too. Dancing☆Onigiri
  works and FFR levels (round Y3, 2026-11-18; before, the web's worker read
  simfiles alone and they kept their songs' numbers): desktop's thread
  reads them as a play does and rates their four-lane charts (generated
  versions among them); the web reads the song as a play does on the page
  and hands the worker those charts written as an `.ssc`
  (`ratings::rating_simfile`: generated versions as charts of their own,
  their notes as generated), the numbers put back in their places by each
  chart's own digest (`ratings::placed`; `a_works_generated_four_lane_version_rates_as_its_own_chart`:
  the same numbers as rating the song itself).
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
  song select's list rows (to a tenth since round AE, the one chart
  Play would play: `song-navigation.md`, "A song's row"; whole on
  the level chips before), its chart buttons (whole; a phone's,
  a room's short side), its ladder rows (to a tenth, the song's own number
  said first in the row's line, "Song 9 · 300 notes"), the Play button and
  a room's chart line (`chart_shown`, whole), the plays list's title, the
  start prompt's chart line (`PromptChart::msd`, `chart_line_rated`, to a
  tenth), the results' header (the prompt's line on desktop; the web's own
  and desktop's kept results, `ratings::shown_msd`), a room's players panel
  and status lines (`meter_text`). The number keeps its difficulty's
  color. A chart without MSD (not four lanes, or not rated yet) shows the
  song's number at 55% opacity where it is a part of its own (a list
  row's level, buttons, ladder rows): quiet, and the same color. Sorting, the ladder's order,
  the Adaptive ladder, the CPUs' eligibility, the announcer's hard-chart
  comment, the editor and the room's content card (agreed between peers)
  keep reading the song's number.
- **The radar** (`ddi_ui::radar`): the seven skillsets, Stream at the top
  then clockwise, each as a share of the chart's overall (the outer ring
  is the overall, rings a quarter apart), so shapes compare as profiles
  while the numbers say how hard; drawn from quads (rings and spokes as
  thin bars, the fill as seven triangles cut by two turned frame clips
  each, the outline over it), in the theme's `Radar.*` colors. Where it
  goes since round Z1: below (until then it overlaid the banner's right
  end, for a rated chart alone, with short labels and its numbers only
  under the pointer).
- **In a room**: ratings are each device's own (nothing is sent; the room
  model's revision is untouched). The picked chart's song is rated first
  where it has none (`want_first`), so a follower sees its MSD once their
  device has it.
- **Debug**: `__DDI_DEBUG.ratings` on the web (wanted, paused, pending,
  rating, rated, known, the worker's songs and time).

Checked: `ddi_ui` tests (`screens/ratings_tests.rs`: Off reads, writes
and offers nothing; MSD in the numbers' place, the song's in the row's
line, a new rating shown unasked; the radar for a rated chart only),
`state/ratings.rs`'s (the queue paused during a play, kept and read
back), the theme's walk, desktop's offscreen pictures
(`tests/ratings_offscreen.rs`, wide, phone sideways and upright, en and
ja), and in headless Chrome and Firefox on a release build: both options
On, the bundled songs' MSD and radar, the pointer's list, a scratch pack
of synthetic charts imported and rated by the worker (`msdslow=400`), a
play started while rating was under way pausing it (`paused` true, nothing
rated through the play) and its results letting it go on to every chart,
the phone upright without a radar, and both Off: nothing wanted, stored
or fetched. The rating worker is 296 kB of wasm; the page's first bytes
are unchanged but for its meta tag (25 bytes gzipped).

Not done: MSD at other rates (the listing's 1.0 only).

## Round Z1 (2026-11-18): the radar in the chart ladder's place

The maintainer, after turning the option On in the released build: "I've
turned the skill radar on but I can't see it. I think it should replace
the difficulty selector on the left side and simplify it to something
similar to the difficulty selector of the multiplayer host."

**Why it was not seen** (reproduced on the build before, headless Chrome
and Firefox, and desktop's library through `tests/ratings_library.rs`):

- **The web never rated an imported song when the option was turned On
  in Options** (a defect, fixed): the rating worker's loop was started
  only at the page's load (`ratings::kick`) and when `lang=` was cleared,
  never on a route, though this plan said "on every web route". A fresh
  profile, a scratch pack of ten songs imported, Options → Display → Skill
  radar On, back to song select: `wanted` true, `pending` 10, `rated` 0
  and no worker for as long as the page stayed (12 s watched), so no
  imported song ever had a radar until a reload (after which all 30
  charts were rated in 26 ms of the worker's time and the radar showed).
  The bundled songs' charts, rated at build time, showed theirs. Fixed by
  `kick` on every route (`App::show`); the walk now sees the radar come
  on an imported song without a reload.
- **The size never hid it on a wide screen**: the radar's test was the
  banner's height, which the wide layout keeps at 175 at every size, so
  it showed at every size tried (CSS pixels: 1280×720, 1366×768,
  1440×900, 1536×864, 1920×1080, 1680×1050, 1024×768, a 1440×900 Mac's
  browser window with its toolbars at 100, 110 and 125%, a 1280×800
  one's at 125%, a 1280×720 one's at 125% (1024×488), a 1470×956 one at
  100%), 162×108 CSS pixels at its smallest over the banner. Only the
  compact layout (under 480 CSS pixels high, a 1280×720 window at 133%:
  962×459) had none, saying nothing. Desktop's default window is
  1280×720 logical: wide.
- **A chart without a rating had none, saying nothing**: one not rated
  yet (a library song on desktop is rated within 0.1 s of song select
  showing it, the shown song first; an imported one on the web as fast,
  once the worker runs), one that cannot be (not four lanes: Double,
  Pump, Dancing☆Onigiri's other layouts), one the calculator skipped.
- Chart ratings "From the song" with the radar On: the bundled songs'
  numbers are read and the background rating runs (the radar alone
  wants ratings). Turned On under song select (desktop's Options over
  it): the radar came within 0.12 s.

Most likely what the maintainer met: in Firefox, the option turned On in
Options and song select's imported songs looked at, which were never
rated in that session; on a bundled song the radar was there, small, over
the banner, and easy to take for part of its art; on desktop it showed.

**The layout with "Skill radar: On"** (the wide layout on both shells;
`Layout::radar`, `fill_radar_side`):

- The ladder goes. Where its rows were: **one row of chart buttons**, the
  multiplayer host's (`chart_buttons`, the look's `ChartButton`: the
  difficulty's color, its number, the song's or the MSD as Chart ratings
  says, the song's fainter where MSD shows and the chart has none, the
  lamp under it, a version's dot) across the song's side, a place for
  each of the style's charts (five places at least, eight at most, as the
  ladder showed eight rows; past eight a window round the chosen one, as
  the ladder scrolled), the chosen one marked; a pointer or a tap picks (a
  tap on the chosen one plays, as a row's did; a room's five-place window
  scrolling with the chosen chart would have put it under the next press,
  so the radar's side gives every chart its place); the keyboard's and a controller's chart gestures are the
  ladder's (twice Up or Left an easier chart, twice Down or Right a
  harder one); the buttons and the style tabs are the side's focusables.
- Under it, left: **the chosen chart's lines**, what its row said: its
  difficulty (or an Edit's name), its number as the row had it (MSD to a
  tenth), "Song N" where MSD takes the number's place, a generated
  version's kind, its notes, "Steps by …" (a line of its own, so less of a
  long credit is cut than the row cut), its best's grade, score (with "…"
  while it waits to be judged again) and lamp. The other charts' lines
  are a press away.
- Right: **the radar**, the rest of the row's width (338 at the design's
  1280) and the rest of the height down to the footer (191 at 16:9, 271 at
  16:10, 320 at most on 4:3), its overall in its upper left corner ("MSD
  24.3") and each spoke's name over its number, always drawn. **The label
  rule** (`radar::place`): text a sixteenth of the radar's height, 9 to
  13 px (the numbers 2 px larger); each label beyond its spoke's end, away
  from the middle (right of a spoke going right, left of one going left,
  above one going up, below one going down, centered otherwise); the full
  names (Jumpstream, Jack speed…) where the shape keeps at least
  four-fifths of the radius the short names (Jumps, Jacks…) leave it, else
  the short names; where neither fits inside the radar with no two labels
  (or a label and the caption, or a label and the shape) meeting, the
  text a pixel smaller, to 9 px; the radius is what the labels leave.
  Nothing is cut and nothing overlaps
  (`the_radar_s_labels_never_overlap` over English, Japanese, `qps-ploc`
  and `qps-wide`; desktop's `ratings_offscreen` measures the drawn
  labels with the platform's text at three window sizes).
- **No rating**: one quiet line in the radar's place, the chart's lines
  beside it: "Rating this chart…" while its song waits or is being rated
  (it is rated first, and the radar comes by itself), "No rating for this
  layout" (not four lanes), "No rating for this chart" (the calculator
  skipped it, or nothing rates it: a share's song).
  `ddi_ui::state::ratings::standing`.
- **Motion**: another chart's shape morphs from the last one's in 0.22 s,
  eased out; one coming where there was none grows from the middle; at
  once under reduced motion. The labels' numbers change at once.
- **Where not**: a room's host and followers keep their short side (a row
  of chart buttons beside a small banner, the players under it): at
  1280×720 it has 244×140 beside the banner, taken by the style button,
  the buttons and the chart's two lines, and the players panel below, so
  no radar there. A phone sideways or upright keeps its short side (the
  style button, the buttons and Play fill its column; upright, a radar
  would push the list), so none; the option's description says "On wide
  screens".
- With "Skill radar: Off", whatever Chart ratings says, song select draws
  exactly as before (`song_select_with_the_radar_off_draws_as_pinned`
  and, in the theme, `…_in_the_theme_as_pinned`, recorded on the code
  before).
- The option's description now says what it does to the song side.
- `__DDI_DEBUG.ui.theme.radar` (and `ThemeState::radar`): `shown`, or why
  not (`rating`, `unrated`, `no-layout`, `no-chart`; `off`, `phone`,
  `room`).

Checked: `ddi_ui`'s `ratings_tests` (the radar at every size of the list
above, the buttons by a tap, the keys and a pad, the morph and its
absence under reduced motion, the lines, no rating for a layout, the Off
pins), `state::ratings`' `a_chart_stands_rated_rating_or_unrated`, the
theme's walk and Off pins, the `qps-ploc` walk over the radar's side,
desktop's `ratings_offscreen` (pictures at 1280×720, 1280×800 and
1024×768 in English, Japanese and `qps-ploc`, a phone's two) and
`ratings_library` (a library read in place, rated on the thread, the
radar coming on a fresh profile and when turned On under song select),
and in headless Chrome and Firefox on a release build (part 1's sizes,
the option turned On through Options, the scratch pack rated slowly, a
practice room's host, Off).

## Round Z2 (2026-11-18): the DDI estimate as a source

The maintainer, after cherry-picking the single-pass estimator
(`crates/rating`, `ddi_rating::estimate`): "Please take a look and let's
try to use that for the ratings and skill radar." A try: MSD stays as it
is and nothing is removed.

- **Options → Display → Chart ratings** has a third choice, "DDI
  estimate" (`ChartRatings::Estimate`, stored `"Estimate"`; a build
  before it reads that as an unknown value, `ddi_engine::lenient`'s
  default, the song's numbers, and keeps that once it saves the
  settings). Its description: our own estimate, experimental, for Single
  charts. `ddi_ui::state::ratings::Source` carries the choice.
- **The number**: an original `dance-single` chart's `Estimate::level`
  (1 to 19) where the estimate qualifies it (`within_scalar_domain`: 48+
  playable heads, at most 1,200 source notes, 15 to 180 s, no lifts),
  wherever MSD's number shows. Since round AD (2026-11-19, the
  maintainer: "A tenth everywhere") its decimal to a tenth everywhere it
  shows (the list's rows, the chart buttons, a ladder row, the prompt,
  the Play button, a room's lines, the radar's side's detail; "11.3"), its
  rounded level nowhere (`Shown::whole` and `fine` alike for it; whole
  before). The list's chips squeezed a number wider than their cell to
  it (`maxwidth,24`) until round AE gave each row one level with room for
  four characters, unsqueezed; the chart buttons fit it as they are
  (`the_estimates_tenths_fit_the_rows_and_the_chart_buttons`, at the
  design's size, 600×480 and a phone's, the radar's places and the
  ladder's side). Elsewhere (not Single, a generated version,
  outside the domain, an error) the song's number, fainter, as MSD's
  rule. Everything that decides by a chart's meter keeps the song's.
  `ratings::shown`, `Shown`.
- **Making them**: the same machinery as MSD's, only while the choice is
  the estimate: song select offers its songs' original Single charts
  without one (`Job::estimates`), one song at a time, never during a
  play; desktop's thread (`ratings::rate_song`), the web's existing
  worker (`ddi-msd-worker`, no new worker and no new meta), which now
  carries the estimator (and the engine's code it runs) and answers each
  chart with the estimator's own JSON; the bundled songs' at build time
  (`xtask gen-songs`, `ChartInfo::estimate`, additive). Read everywhere
  by one function, `ddi_library::manifest::ChartEstimate::from_rating_json`
  (the build, desktop's thread, and the page reading the worker's
  answer). Kept by chart digest under `ddi.estimate.v1`, beside
  `ddi.msd.v1`, with `ddi_rating::ESTIMATOR`, `SCALE` and the profile's
  rule's name (`PROFILE_RULE`): a new one forgets them. A chart 39
  characters (digest, decimal and level, flags), 71 with a profile (its
  eight costs); a chart estimated without result kept as such, so it is
  not estimated again.
- **The radar** follows Chart ratings: under the DDI estimate the
  estimate's eight skills (`ddi_engine::cpu::SKILL_NAMES`, Evasion the
  eighth: the radar takes as many spokes as it is given); under MSD and
  under the song's numbers, MSD's seven, as before.
- **The profile drawn** (the maintainer chose it, "Let's try your idea
  for fixing the radar", over the estimator's calibrated
  `expected.profile`, which anchors every chart at CPU level 10 where a
  chart costs a CPU almost nothing: present for about a quarter of
  charts, half of those flat): `matched-level-profile-v0`
  (`ddi_library::manifest::matched_profile`), from `Estimate::decimal`
  and `Estimate::means` alone, where `within_studied_domain` (to 900
  source notes): the CPU level matched to the chart's own estimate,
  `clamp(decimal / 19 × 10, 1, 10)`, each skill's cost the two rows of
  `means` about it interpolated there, a negative cost as zero. Each
  spoke's number is its cost in points of Accuracy to a tenth; the
  caption is "DDI" and the decimal; a line beside it says what the
  numbers are. How the spokes are drawn: round Z3 below. Outside the
  studied domain: "Not available" (the maintainer's wording, 2026-11-19; "Outside what the estimate was studied on" before); a chart that is not an original Single chart: "The DDI
  estimate is for Single charts"; none made: "No estimate for this
  chart"; on its way: "Rating this chart…". Round Z1's layout, label rule,
  motion and tests apply to both sources.
- **Limits of the try**, from the coordinator's study of synthetic pattern
  charts and the tests here (`the_matched_level_profile_reads_patterns`):
  a stream's largest spokes are Stream and Stamina, jacks' JackSpeed,
  jumpstream's and handstream's own skills stand out against a stream's
  (by share: on handstream Stamina is still the largest cost);
  chordjacks read as Jumpstream with Stamina, the Chordjack spoke barely
  moving; Technical never moves.
- **Round Z3 (2026-11-19): a floor for every skill, the whole shape
  growing with the difficulty.** The maintainer, the shape having read as
  a thin spike (most costs are zero): "What if we set a minimum value of
  1 for each skill and grow everything proportionally to the difficulty?
  That way if no skill stands out it will just be an octagon with size
  that varies with difficulty." Drawing only (nothing estimated, kept or
  cached changes; MSD's radar draws as before, `msd_s_shape_draws_as_pinned`):
  each skill's value is `1 + e`, `e = cost / (cost + 2)` (`COST_HALF`, 2
  points: a typical chart's Stream cost), absolute rather than a share of
  the chart's largest; a spoke is `base(decimal) × (1 + e × 0.6)` of the
  radius (`GROWTH` 0.6: a standout reaches at most six tenths past the
  base, a bulge rather than a spike, and leaves the base room to grow
  2.8 times), `base` rising linearly from 0.22 at 1 (`BASE_AT_1`: about
  14 px at 1280 × 720, still a clear octagon inside its labels) to
  `1 / 1.6` = 0.625 at 19 (`BASE_AT_19`, so the longest spoke there can
  be ends at the outer ring, where the labels begin);
  `crate::radar::estimate_spokes`. A chart with no costs is a regular
  octagon whose size says its difficulty; a standout bulges past it. The
  rings behind it mark difficulties: the base octagon at 5, 10 and 15,
  then the outer ring (`estimate_rings`). The "No skill stands out"
  line and its state are gone.
- **Round Z4 (2026-11-19): each skill against charts this hard.** The
  maintainer: "For the skill radar, I feel like the shapes don't change
  too much. What if analyze the distribution of each skill to learn how
  they vary in practice (using my local library as reference) and then do
  some normalization so that the apparent skill level is not absolute but
  relative to the distribution?" As round Z3 drew it, Stream was the
  largest spoke on 69% of charts (91 to 96% from 9 up). The rule now,
  `relative-profile-v0` (`crate::radar::skill_standing`; drawing only:
  the kept costs and the cache are unchanged, MSD's radar too): a skill's
  `e` is 0 at a cost of 0.1 point or less, else its percentile among
  charts of similar difficulty where it costs anything, read off its
  band's costs at the percentiles 0, 10, 25, 50, 75, 90, 97 and 100
  (linear between them, 1 past the last), the two bands about the
  chart's decimal blended (centers 3, 7, 11, 14.5 and 17.5; the first
  band's below 3, the last's past 17.5), so nothing jumps at a band's
  edge; a band with fewer than 40 charts for the skill reads the
  table over every chart; Handstream, which has no table (6 charts in
  the library), keeps round Z3's `cost / (cost + 2)`. The spoke stays
  `base(decimal) × (1 + e × 0.6)`. Each spoke's number is now the
  skill's level on the difficulty scale, `decimal × (1 + e × 0.6) / 1.3`,
  consistent with its length (a skill as demanding as usual for charts
  this hard reads the chart's own estimate, a standout up to about a
  quarter above it, an absent one about a quarter below);
  `crate::radar::spoke_number` is the one place it is made, so showing
  the costs again is one line. The line beside the radar: "Skills,
  compared with charts this hard". The table,
  `crates/ui/src/radar_table.rs`, is aggregates generated by the
  coordinating session's study from the maintainer's local library
  (5,170 original Single charts with a profile): no chart's own data is
  in it. Its limits: it is one library's taste; Handstream has no table;
  Evasion's bands are thin (a band's 40 charts or every chart's 180).
  The study: no skill is the largest on more than a quarter of charts
  from 9 up, and two charts of similar difficulty differ by about 0.29 a
  skill, three times round Z3's 0.10.
- **Measured** (release build, 2026-11-18): the worker's wasm went from
  295,735 to 502,669 bytes (117,106 to 227,893 gzipped; the app's own
  wasm 14,388,263 to 14,418,184, the estimator not linked into it); the
  worker estimated a scratch pack's 21 charts (the bundled songs copied
  with their offsets moved, and synthetic patterns) in 58 ms in Chrome
  and 55 ms in Firefox, about 2.7 ms a chart; every one equal to the
  native estimator's to the hundredth (decimal, level, both domains and
  the eight costs). Of the bundled songs' 19 original Single charts, 18
  have a number (one Beginner chart is short of 48 heads) and 18 a
  profile (12 with a skill standing out, 6 easy ones flat: since round Z3
  each drawn, the flat ones as small regular octagons).

Checked: `state::ratings`' tests (the record kept and read back, a new
estimator's or rule's names forgetting them, waiting for a play, the
matched level on its scale, the pattern charts), `ratings_tests`
(`the_three_choices_number_the_bundled_charts` against the calculators'
own values, `the_estimate_s_radar_draws_each_state`, the label rule over
eight spokes in four languages, the Off pins unmoved), desktop's
`ratings_offscreen` (`the_ddi_estimate_s_radar_draws_in_the_theme`:
pictures, eight labels apart as the platform measures them, English and
Japanese) and in headless Chrome and Firefox on a release build (each
choice on Into My Dream, the scratch pack estimated by the worker and
compared with the native estimator, the pattern songs' radars).

The song-select qualification in `matched-level-profile-v1` extends the
displayed profile to 1,200 source notes on mine-free charts. On 58/60 fresh
mine-free families all eight displayed labels were within 1.5 levels of
teacher-v1. Evasion on mine-bearing charts exceeded the registered error
limits. Their radar and the all-level API retain the 900-note limit.
The manifest reader uses `within_matched_profile_domain`, with the earlier
all-level flag as the fallback for older estimator JSON. The cache rule
version makes previously stored estimates eligible for recomputation.
