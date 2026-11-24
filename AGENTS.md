# Working on Dito Dito Infinito

Orientation for coding agents (and humans) on this repository. Keep it
accurate: when a convention here changes, change this file in the same commit.

## What this is

A dance rhythm game that plays StepMania simfiles; in genre terms a 4K VSRG
(four-key vertical scrolling rhythm game), which is the vocabulary to use when
researching comparable games or writing public copy. Written in Rust and shipped
as a static web app (wasm via Trunk, deployed to GitHub Pages) and as a
desktop app (`cargo run`), the same game: what a play does is shared code
both shells run. `docs/PLAN.md` holds the
architecture, decisions and roadmap; `docs/plans/` the plans for each piece
of work, indexed with their status in `docs/plans/README.md`; `docs/research/` holds the sourced notes
on formats, rules and the tech stack. Read those before changing the engine or
the importers.

Public copy (page title, descriptions, README lead, landing-page entry) says
"dance rhythm game"; the descriptions and the README's lead also say the
multiplayer battle mode in plain words (everyone plays the same song at
once and sends attacks until one player is left), naming no other game
for it and claiming nothing more. Other games may be named to explain supported file
formats, layouts, rulesets or accurate gameplay comparisons (StepMania
simfiles, Pump It Up-style five-panel charts, the DDR A ruleset); do not
imply official affiliation, bundled commercial song catalogs or exact
emulation. Keep the title focused on this game's own name and genre.
Rust and the browser are implementation details, not advertised features.

## Layout

- `crates/chart/` chart model and importers (`.sm`/`.ssc`; FFR's `beatBox`,
  `formats/ffr.rs`), 48 ticks per beat, exact
  StepMania timing semantics, and the `.ssc`/`.sm` writer the chart
  editor saves with (`formats/sm_write.rs`: what is read is written back
  equal), and a chart's Groove values as StepMania 5.1 computes its radar
  values (`groove.rs`, its `CalculateRadarValues` restated, credited in
  `credits/THIRD_PARTY.toml`). Pure, natively tested.
- `crates/engine/` deterministic gameplay: clock, judge, rulesets, scoring, gauges,
  scroll, calibration maths; a battle's shocks in a play (`battle.rs`:
  `HazardJudge` beside the chart's judge, `Player::add_hazards`, events
  and sprites from `HAZARD_INDEX_BASE`; under DDI a shock is a mine,
  `Player::shock_is_mine`: stepped on, it counts, scores and costs life
  as a chart's own and locks its lane, the chart and the shocks judged
  in step, a press stepping on the nearer of a note and a shock,
  `Judge::press_beside`; under a ruleset without the lock a hit is the
  ruleset's miss to the gauge alone, `INJECTED_MINE_V0`; `schedule` and
  `PLACEMENT`, where a chart has room for eight: `placement-v2`, which
  keeps `placement-v1`'s margins about each step of the lane exactly,
  `Clearance`, none within 0.25 s after one nor within W3
  before it, none from a hold's head to past its end nor from a second
  before a lift, and aims six of the eight at places next to a note of
  their lane, `Near`: up to half a second after one or up to twice W3
  before a step, spread over lanes and beats, one a note, a second
  between two shocks of a lane unless the section has no other room
  (`placement-v0` and the `placement-v1` constant retired 2026-11-14,
  v1's way kept as v2's fallback; for a match's Shocks per attack,
  `Placement::with_shocks`, `schedule_of`: the count its argument,
  three quarters of it aimed, eight `placement-v2` exactly); kept in
  `PlayLog` version 4; a
  battle's attack policy as the player changed it during the play in
  version 5,
  `PlayLog::policies`, `Player::set_policy_at`: a number of the
  battle's own, nothing judged by it;
  `reading_horizon`, how long a play's notes are shown before they are
  judged); a match's "Charts: Adaptive" (`ladder.rs`, `ADAPTIVE`, which
  is `ADAPTIVE_V5`: sixteen bars back up to a rung whose life ran out, a
  rung closed once life ran out on it twice, `Step::closes`, a step up
  early after four bars with 90% of their taps in the three best
  judgments, `Step::early`, `early_best`, or normally after six with
  80% in the three best, `normal_bars`; `adaptive-v0` to `adaptive-v4`
  retired; tests and studies
  set variants of it, `Player::set_ladder_rule`:
  a song's ladder of easier and harder charts, `ladder::Ladder`,
  `below`, `above`, its rungs numbered from 1, the easier then the
  harder, 0 the stage's; since room model revision 23 a play may start
  on any rung, `Player::start_on` (its own chart under Adaptive, the
  rungs keeping their numbers), a CPU on the chart its level is at home
  on, `ladder::cpu_start`, `cpu_start_in` (`cpu-start-v0`), its play
  planned from there (`Ladder::around`, the session's `cpus::around`,
  its steps put on the stage's numbers by chart, `rung_of_chart`); a
  play changes chart, `Player::add_rung`,
  `add_rung_up`, `Player::steps`: down at zero life, the old chart's
  notes not judged taken away, `NoteState::Removed`, `Judge::step_down`,
  the next rung's from a bar line past the reading horizon, half the
  life back, a breather with a floor; up after four bars of full life
  with no miss, from a bar line past the horizon and a second, the old
  chart played to the boundary, `Judge::step_up`, decided at bar lines
  judged to first, `Player::judge_to_bars`; the plan over every rung,
  `ladder::merged`; a CPU's rungs, `cpu::planned_ladder`,
  `Opponent::add_rungs`); a CPU opponent's ten levels (`cpu/levels.rs`,
  `CPU_LEVELS`, `docs/plans/cpu-player.md`: since round V (room model
  revision 14) `cpu-levels-v2`, `CPU_LEVELS_V2`, absolute,
  `cpu/absolute.rs`: each level a timing spread in milliseconds about a
  lean of its own (drawn from its seed, with a drift; poor calibration's
  look), the notes a second it keeps up with and a floor, the same on
  every chart; a row's demand read from the chart in real units,
  `RowDemand`, never normalized by the chart; under load its spread
  widens well before it leaves notes out, a play's form, bursts after a
  stumble and fatigue toward the end, `ABSOLUTE_V2`, `Personal`; the
  shape read from one player's local play history, not included, the
  levels fitted to targets stated first (`cpu_levels_study`, the
  ordering test `local_cpu_order`); `cpu-levels-v1`, `CPU_LEVELS_V1`,
  `ABSOLUTE_V1`, retired from play, kept pressing exactly as before for
  the studies' comparison (`DDI_CPU_SCALE=v1`);
  `Strength`, `Opponent::with_skills` and `on_scale`, a stage's CPUs
  on its chart planned once, `Opponent::on_rung`; what it plays, `cpu/full.rs`, `CPU_PLAY`: at an absolute
  level the chart a person is given, `cpu-play-v1`, `FullPlan`,
  `Plan::script_play`, `Opponent::planned_as`: holds kept down and let
  go early under load, `HOLD_MODEL_V1`, rolls re-tapped, lifts released,
  the chart's mines avoided as shocks are, what is held counted in a
  row's demand, `RowDemand::held`; the tap-only copy, `cpu-play-v0`,
  `cpu::project`, kept for prepared profiles, `cpu-levels-v0` and
  comparison, `DDI_CPU_PLAY=v0|v1`; the earlier `cpu-levels-v0`, `CPU_LEVELS_V0`, kept for the
  solo CPU and comparison: levels 6 to 10 its five frozen strengths,
  pressing exactly as before, 1 to 5 weaker by a lower strength and a
  lapse; `Plan::script_at`, `Opponent::planned`; measured
  over local packs, `crates/engine/tests/local_cpu.rs`,
  `DDI_CPU_CORPUS`, under the rulesets `DDI_CPU_RULES` names: a preset's
  id, or `ddi-easy`, `ddi-normal`, `ddi-hard`, the rooms' match rule) and its response to shocks put into its play
  (`cpu/mines.rs`: one avoidance value a CPU from its level,
  `MINE_RESPONSE_V0`, bent by its `Skills`' eighth axis, Evasion, where
  a shock is hard to get around, `MINE_RESPONSE_V1`, Evasion 1 the
  level's response exactly; `Opponent::receive_hazards`); DDI, our own
  ruleset (`rules/ddi.rs`, `docs/plans/ddi-ruleset.md`: `RulesetKey`'s
  `ddi` options, windows on a 1/120 s grid in real seconds (at a music
  rate `ddi::at_rate`), early and late words (`JudgeNames::word`), hold
  units in the chart's seconds, life, the point count's views
  (`ScoreView::ddi`), grades with floors, `ddi::Lamp`; the judge's lane
  lock, `JudgeTable::lock`/`Judge::lock_lane`, and Boos' exemptions,
  `EmptyPress::Guarded`; `presets::ddi`, the first of the presets, the
  player's options through `Ruleset::with_ddi`; a locked lane's mine
  effect set off again by the player, `LOCK_EFFECT_PERIOD`, and a W1
  outside W0 a dimmer explosion, `SkinEvent::dim`). No I/O.
  Replay fixtures in `crates/engine/tests/`.
- `crates/render/` wgpu scene (the built-in notes: Infinito's shapes as
  distance fields in `shader.wgsl`, made light, PLAN.md decision 80;
  glyphon text) and the 2D pass
  the menus move onto (`docs/plans/ui-core.md`): a draw list, its painter,
  the texture cache and the text atlas (runs measured and drawn by the
  platform: the browser on the web, cosmic-text natively; drawn over a
  play too, through an sRGB surface where there is no plain view, for
  Dancing☆Onigiri's lyrics, `crates/render/src/lyrics.rs`),
  a layer over the backdrop and under the field, composited by the
  backdrop's own shader with the filter's band (`BackgroundPipeline::
  set_overlay`, `Renderer::set_chat_layer`: the room host's chat; over
  a Dancing☆Onigiri work's back layers and the dancing hand where a play
  has them, a scene segment of its own, `scene::Segment::Overlay`, under
  the filter's band and the field),
  cached menu Gaussian images (`crates/render/src/gaussian.rs`, recorded once
  on upload before mipmaps, keyed separately from sharp images), Options'
  field preview (`crates/render/src/field_preview.rs`: the demo chart drawn as
  play draws it into a texture the 2D pass shows) and song select's
  preview movie's picture (`crates/render/src/movie_picture.rs`, the same
  way), a song's scripts' movies drawn into textures their quads sample
  (`crates/render/src/script_movies.rs`, a converter of its own) and
  the chart editor's field in the player's skin
  (`crates/render/src/editor_field.rs`: a play's frame paused at the
  cursor, the skin's clock at rest; `ddi_ui::screens::editor_field`
  places its notes and the overlay over it; its audio view's picture,
  `crates/render/src/editor_audio.rs`); StepMania's named
  background effects drawn from `ddi_library::backgrounds::BgDraw`
  (`crates/render/src/background.rs`) and a song's scripts' actors as quads
  in the background frame (`crates/render/src/scripted.rs`); generated
  backgrounds drawn into a texture of encoded values the backdrop shows
  as a picture, self-lit (`crates/render/src/generative.rs`,
  `generative.wgsl`: the look, then the lines' and outlines' strokes);
  skins'
  materials (`crates/render/src/material.rs`: a WGSL `paint` checked for
  WebGL2 with naga, drawn by the textured pipeline,
  `docs/plans/noteskin-scene.md`); each run of skins' quads put in an
  order drawing the same pixels in fewer batches
  (`crates/render/src/batching.rs`: a quad joins an earlier batch of its
  kind past batches that touch none of its pixels; `Renderer::set_batching`
  for the tests that compare, `opponent_fields_offscreen`), and what a
  frame's scene took (`Renderer::frame_stats`: passes, WebGL2's
  conversions, instances, batches) for the web's frame recorder. No
  platform code.
- `crates/dance-mat/` owns physical tile identities (single/left 1–9, right
  10–18), versioned millimeter geometry, built-in lane target sets and the
  accepted No Stretch regions. DanOni 9A's Oni has two alternative centers;
  callers must choose a physical tile before publishing a contact. Mapping is
  not a hand-support or reach certificate. Double geometry is available to
  version-2 pose plans and dance-double gameplay; the runtime's startup seed uses the assigned
  opening contacts without copying future tracks. Legacy single-mat startup is
  preserved. Explicit-geometry curves treat full-turn world yaw as periodic:
  internal phase stays continuous, model poses normalize to ±180°, and angular
  derivatives remain unwrapped. Finger joints and narrower root ranges keep
  their bounds. Explicit shared-tile spots follow the incoming finger order;
  overlapping contact chains and published endpoints keep their separation axis
  fixed through re-hits/pruning. Legacy slot placement stays unchanged. The live
  planner and gameplay support dance-double, Pump half-double, Pump double and
  DanOni 9A on the explicit 18-tile mat.
  Character presentation uses the same geometry for mesh centers, camera bounds,
  contact lights, presses and spill; render X reflects solver X. Visual slots
  retain the original row-major ordering inside each pad and map explicitly back
  to physical IDs. DanOni 9A's canonical `layout_panels` entry for Oni is 5;
  use `MatLayout::targets` for its physical 5/14 choice and `lane_for_tile` to
  aggregate both into lane 4. Choose within the received window and freeze with
  the physical prefix; holds/rolls retain that center. Oni hazards cover both
  centers, and a re-hit/lift needs an actual aggregate press/release. All eight
  unscored corners are supports; neither center is a safe escape. The 9A-only
  nominal candidate span is 4.7–68 mm; actual pose/contact bounds are unchanged.
- `crates/character/`, `crates/hand-plan/` and `crates/hand-pose/` supply the
  dancing hand (`docs/plans/dancing-hand.md`). Both default builds include it;
  settings retain saved Off choices. In a multiplayer stage it plays only
  where Options → Display's "Dancing hand in multiplayer" is On (Off by
  default, `Settings::dancing_hand_in`): Off, a stage prepares no hand at
  all (`dance::Preview::preload`). Shared At once Two trims excess simultaneous
  lifts after taps/new hold heads, preserving retained timing and metadata;
  One retains its legacy lift protection. The hand is an autoplay beside the play: it
  plays its own copy of the judged notes, at most two at once (`At once: up to
  two`, ongoing holds included), while the play judges every note with the
  player's options, never an assist because of the hand (`RuntimePreview::
  for_player`). `ddi_hand_plan::prepare_notes` applies At once Two and No stretch
  to that copy. General No stretch is a separate saved Options → Notes cut in
  `TransformOptions`, assisted when selected, checking final turned lanes and
  ongoing holds for dance-double, Pump half-double/double and DanOni 9A. Oni may
  use either center in the logical span policy. Dance-double maps
  L/D/U/R on the left and right pads to 4/2/8/6/13/11/17/15, using version-2
  contact plans with explicit geometry (24 mm pitch, zero seam gap). Its bounded
  search uses separate frozen geometry tables, checked No Stretch scored pairs
  and at most two nearby unscored support candidates; neither finger owns a pad.
  The three-second rolling planner preserves certified contacts, hold recovery
  and exact head/lift times. Only an unpublished double opening rest may choose
  the first received head's pad; a certified prefix is never reseated. Production
  hand layouts are dance-single, dance-double, dance-solo, danoni-5,
  danoni-7, danoni-9A, danoni-9B, pump-single, pump-halfdouble, pump-double and techno-single8.
  The shared `ddi_character::HAND_LAYOUTS` list controls saved settings and is
  checked against the planner. Resolve canonical lanes through
  `ddi_hand_plan::layout_panels` and physical alternatives through `ddi_dance_mat::MatLayout::targets`;
  Solo L/UL/D/U/UR/R map to mat 4/7/2/8/9/6, with center unscored;
  Pump DL/UL/C/UR/DR map to 1/7/5/9/3. Pump may use unscored side stances
  (4,6)/(6,4) and air time for clearance. Pump half-double maps its middle six
  lanes to 5/9/3/10/16/14; Pump double maps to 1/7/5/9/3/10/16/14/18/12.
  Both centers are scored. Their empty opening rest is unscored 6/13; only an
  unpublished opening may choose local unscored sides from received heads.
  Six side-escape recipes are ranked to at most two per parent, and lift returns
  choose a reachable local side while preserving the held finger. Pump's 90 mm
  nominal candidate envelope covers the surveyed 36/68 allowed ordered pairs;
  scored pairs still obey No Stretch and all actual pose/contact bounds remain
  unchanged. Legacy layouts retain their own envelopes and decisions. Seven-key L/DL/D/Oni/U/UR/R maps to
  4/1/2/5/8/9/6 from the displayed glyphs, leaving 7/3 as escape corners.
  Techno DL/L/UL/D/U/UR/R/DR maps to 1/4/7/2/8/9/6/3; center is unscored
  and can be used for escapes alongside air time. DanOni 9B maps displayed
  UL/L/DL/D/Oni/U/UR/R/DR to 7/4/1/2/5/8/9/6/3; all tiles are scored, so
  there is no designated escape tile. Hazard clearance uses air time; shared
  center repeats/rolls may use a temporary scored rest after checking known
  notes and hazards.
  Wide corner-layout transfers can move the free finger to an unscored tile;
  their balance guide eases toward the midpoint with existing pose/contact bounds.
  Keep native DanOni release budgets distinct from StepMania hold grace.
  Three-second planning and bounded f64 pose preparation
  run cooperatively on one thread; rendering samples prepared C2 motion. Preserve
  exact contact times, published prefixes and current contact/model bounds.
  Stream fallback ranks knuckle clearance alongside fingertip accuracy. Small
  floor corrections and near-edge tangent repairs run during checked curve
  preparation; keep sampling solve-free and retain the same published guards.
  Double-mat empty boundaries retain their chosen opening support. Difficult
  double-mat curves may use six additional refinement passes (12 total), with
  numerical knots down to 0.25 ms only in that second stage. Keep endpoints and
  all contact/phase timestamps fixed, the 32,768-knot cap, and at most eight fit
  iterations per checkpoint. Double preparation reserves 1.90 mm target error,
  0.225 mm planted height and 40.01 mm knuckle clearance inside the unchanged
  published 2 mm / 0.25 mm / 1 mm tile-margin / 40 mm limits. Single-mat curve refinement
  retains its existing policy.
  Mine clearing can reserve exact received tap/hold landings, with independent
  finger landing times. Grouped simultaneous hazard alternatives keep the beam
  bounded (at most five candidates per parent checkpoint). Guarded scored
  arrivals/departures replace only their own mine-window segment: midpoint
  +5 ms for an arrival, midpoint -5 ms for a departure. The 170 ms source-gap
  bound is where these guards meet the unchanged ±90 ms physical window.
  Require the exact received head, matching incoming swing and real outgoing
  flight for a departure; the current window end is never a release proof.
  The derived, non-serialized cache is capped at 32,768 combined entries and
  must retain only witnessed flights across visibility pruning.
  A fully rejected nearby-hazard row may retry once with timed clearance.
  Double-mat recovery may additionally try local unscored support for a single
  threatened finger (at most two targets; companion fixed). Retain geometry,
  contact and hold guards. Only recovery histories may consume a reserved
  intermediate tap before a later escape; do not rewind the pending track or
  extend this to long heads.
  Never exempt unrelated feet, unknown heads or unscored re-presses. Preserve
  declared-flight proofs across visible pruning without revealing future events.
  Fixed scored-tile hazard returns must not steal a head early; defer their
  commitment until the effective judges' early-hit neighborhood is received or
  EOF is known. Keep hold-release budgets distinct and prefixes immutable.
  Optional tight-burst reductions remain opt-in preparation proposals for
  uncommitted hand input; never change the play's judged chart or rewrite the
  hand's already-published contacts.
  Display-time revisions may only resample retained prepared output; never rewind
  the contact/planning cursor or change note times to follow a display prediction.
  The prompt gates audio on opening CPU/GPU readiness; quick restart warms a fresh
  buffer. Preload keys include source notes, geometry and effective play options;
  paused draws also invalidate when geometry or procedural source changes.
  Unsupported/failed hand preparation must never block ordinary play.
  Its preparation keeps to its share of a frame by the clock and, on a
  coarse one (Firefox's millisecond), by work counted (`dance_runtime::Costs`,
  per stage, learnt from whole frames); a hand that falls behind (its
  motion out, a planner or motion deadline missed) rests where it stood,
  never taking the frame. `ddi_session::hand_load`
  (`hand-load-v0`, `docs/plans/dancing-hand.md`, "Under load") watches late
  seconds while it plays and steps it one way per play: lighter (half its
  share, its picture every other frame), resting (nothing prepared, picture
  still), gone (a results note, as a movie's); never the field, judging,
  sound, input or record. `load=MS` (web) and `--load-ms N` (desktop) burn
  main thread a frame for tests.
  Only `assets/hand/` runtime data/notices are included; development archives,
  external modeling tools, full audits and developer overlays stay on the hand
  research branch. `assets/hand/NOTICE.txt` records what the hand is made from
  (MIT, CC0, CC BY 4.0) and what its studies only compared against; keep it
  current with the files.
- `crates/platform/` traits and plain data the shells implement (audio clock, input,
  device fingerprints, their ids written beside their reader,
  `devices.rs`), WAV files written and read (`wav.rs`: 16-bit PCM and
  32-bit float), plus the Ogg Vorbis and MP3 fallback decoder behind
  the `decode` feature (Symphonia, MPL-2.0; MP3 as StepMania decodes it,
  `crates/platform/src/mp3.rs`), Ogg Opus through `rusty-opus` (BSD-3-Clause)
  behind `opus` (`crates/platform/src/opus.rs`: the web's Opus worker and
  desktop's decoder, and the encoder behind the announcer studio's packs;
  the Ogg file around any encoder's packets, `ogg_opus.rs`, on every
  target) and, behind `decode-all` (the
  desktop's), every format a simfile's music comes in: MP3 as StepMania
  5.1 decodes it (`crates/platform/src/decode_all.rs`), resampling with rubato.
- `crates/opus-worker/` the web's Ogg Opus fallback (`ddi-opus-worker`, PLAN.md
  decision 9): a worker Trunk builds beside the app (`site/index.html`,
  `data-type="worker"`; `xtask stage-opus`, a step of `xtask
  post-build` (Trunk's one post-build hook: the favicon, `stage-video`,
  `stage-opus`, `stage-svg`, `stage-msd`, `check-boot`, in that order,
  since four of them rewrite index.html and Trunk runs a stage's hooks at
  once),
  names its files by their hash and tells the page in index.html's
  `ddi-opus-worker` meta), started by `crates/web/src/web/opus.rs` only when the
  browser cannot decode an Ogg Opus file (the module compiled once a
  session and handed to a worker a decode; one that has not started in
  25 s fails); it sends back a float WAV at 48 kHz that
  `decodeAudioData` resamples. It also encodes the announcer studio's
  takes where the browser has no `AudioEncoder` or it fails
  (`web::opus::encode`; `__DDI_FORCE_WASM_ENCODE` to test it). Empty
  natively.
- `crates/svg-worker/` skins' SVG images on the web (`ddi-svg-worker`,
  `docs/plans/noteskin-svg.md`): a worker Trunk builds beside the app
  (`xtask stage-svg` names its files by their hash, index.html's
  `ddi-svg-worker` meta) drawing them with desktop's code
  (`ddi_noteskin::raster`, resvg, the `raster` feature; the app's own wasm
  goes without it), its rasterizer's crates built at `opt-level = 3` in
  the release profile; started by `crates/web/src/web/svg.rs`, a few at
  once, ended once idle; the browser draws an SVG only where none starts.
  Empty natively.
- `crates/msd-worker/` chart ratings on the web (`ddi-msd-worker`,
  `docs/plans/msd-ratings.md`): a worker Trunk builds beside the app
  (`xtask stage-msd`, index.html's `ddi-msd-worker` meta) that parses an
  imported or shared song's simfile as the library does and rates its
  four-lane charts (`ddi_msd`) and estimates its original Single charts
  (`ddi_rating`, answering with the estimator's JSON, read by
  `ChartEstimate::from_rating_json`); started by `crates/web/src/ratings.rs`
  with the first song to rate while a ratings option is On, fed one song
  at a time, none during a play, ended after 10 s idle;
  `__DDI_DEBUG.ratings`. Empty natively.
- `crates/library/` song library logic shared by the app and xtask: pack scanning
  (which file is the chart, music, banner), a sans-IO zip reader, the manifest
  entry type, and the import's core (`scan.rs`: a song's entry, its files
  and the report's warnings from its folder; `works.rs`: a
  Dancing☆Onigiri work's songs, ids, music and layers; the web's import
  runs them on `Blob`s, desktop's library on its folders), FFR levels
  (`ffr.rs`: a song a level file, its folder its pack; the chart and
  music of a SWF, `swf.rs`), a stored song's entry after the chart
  editor saved it or made it new (`edited.rs`), a music file's title
  and artist from its tags (`tags.rs`), the lines announcers speak and
  the announcers found among an import's files (`announcer.rs`:
  StepMania's layout and aliases, Etterna's two names and ours; the
  multiplayer battle's own category, `Line::Battle`, `Spoken::Battle`,
  folders `battle …`, with their fallbacks, heard only in a stage;
  our bundled packs' keys, `bundled:<name>` (the built-in one's
  `bundled:clank`, `BUILT_IN`), and the web's index of them with what
  each sound says; an announcer as a pool of takes, its
  `announcer.ini`'s languages and names, resolved by the player's
  choices to the lines the session speaks, `announcer/pool.rs`; a
  pack's `announcer.lua` run once at load on the sandboxed Lua to build
  that pool (reuse, tags, weights; refused whole for its folders when it
  fails), `announcer/script.rs`;
  `docs/plans/announcers.md`; every line,
  when it speaks and what it falls back to, for pack authors:
  `docs/announcer-lines.md`; the announcer studio's drafts, takes
  matched to lines by name, their notes, the text lines planned and
  the export's plan (languages, tags, takes said for several lines:
  the plain layout where it says it all, else a folder a language, an
  `announcer.ini` and a script, read back through the pool before it
  is offered, `Draft::check`; a scripted pack read into a draft,
  `read_pack`), `announcer/draft.rs`, tidied by `announcer/tidy.rs`,
  the one leveler, which our packs' sounds are made through too (`xtask
  announcer tidy`): the ends cut and faded, then `tidy::LEVELING`'s
  chain, `announcer-level-v3` (a phase rotator, a compressor against
  the sound's own level, one gain to a voiced RMS of −12 dBFS, about
  −13.4 LUFS, and a limiter that rides the level up to 3 dB before it
  shapes a peak, true peaks under −1 dBFS; a sound at the level already
  gets the gain alone), exact arithmetic in a fixed order
  (`docs/plans/announcers.md`, "Louder packs" and "Smoother leveling";
  its study, `DDI_TIDY_STUDY=<dir> cargo test --release -p ddi-library
  --lib tidy::tests::study -- --ignored --nocapture`); `tidy::finished`
  (`announcer-finish-v1`, `xtask announcer finish`) reuses its gain solver
  and limiter after bundled voices' creative effects, without another
  compressor, phase rotation or trim, leveling only the original speech
  span and retaining the bounded effect tail. The optional studio Stadium
  effect and pack generation share `announcer/stadium.rs` and
  `draft::Processing`; its response data is in `assets/announcer-filters/`,
  fetched on demand by the web and embedded by desktop/xtask;
  `announcer/tail.rs` (`voice-tail-v0`, `docs/plans/announcers.md`, "A
  line ends where its voice ends"): where a sound's voice ends and only
  its tail (a reverb) is left, measured by both shells as they decode an
  announcer's sounds; the voice counts a line said until then
  (`ddi_session::announcer::voiced_length`) and the next line may start
  over the tail, both voice buses mixing it under. More
  examples of the lines,
  `announcer/examples.rs`, text only from `announcer/examples.toml`,
  which the library's `build.rs` makes a table, the game carrying no
  TOML parser; `docs/plans/announcer-studio.md`), zips
  of stored entries written (`zip::write_stored`) and 16-bit WAVs
  (`wav.rs`, the platform's writer), and what the shells and the
  private shared-songs Worker agree on without the web's record types
  (`share.rs`: `ddi_share_protocol` re-exported, the time left, the
  refusals worded, a grant's refusals kept, a share's upload sans-IO that
  both shells' transports drive, `share/upload.rs`;
  `docs/plans/shared-songs.md`), the music's analysis a generated
  background follows (`music.rs`, made at a play's load; its transform,
  `fft.rs`, the chart editor's too) and those backgrounds' four looks
  with the study's approved values (`generative.rs`: `stands_in`, never
  for a Dancing☆Onigiri work; `Series`, every smoothing, peak and clock
  made over the analysis, so a frame is a function of the song second;
  `docs/plans/phase-8-gaps.md`, step 5), and the schema of
  `credits/credits.json` that `xtask gen-credits` writes and the Credits
  screen reads (`credits.rs`). Pure, natively tested.
- `crates/actor/` StepMania's actor model, shared by noteskins and later our
  screens (`docs/plans/ui-core.md`): actors, the tween queue, effects,
  sprite states, eases, commands as metrics write them, a model's meshes,
  bones and animated textures, and what is drawn (with our own
  extensions for the theme: sprites' `cornerradius`, `softedge`,
  `outline`, `saturation`; text's `ellipsis` and letter spacing). Pure,
  natively tested;
  the files actors are built from (`metrics.ini`, MilkShape, legacy actor
  files, Lua) are read in `noteskin/`.
- `crates/ui/` the UI core our screens are built on (`docs/plans/ui-core.md`,
  step 3): input events in, requests to the shell out (handled
  synchronously, gesture-bound ones flagged), hit-testing through actors,
  touch and mouse gestures, menu navigation with key repeat and focus,
  screens with transitions, dialogs, messages to actors, the one-line text
  field (mirroring the web's hidden textarea, or editing itself on
  desktop), and a headless harness; the theme's screens
  (`crates/ui/src/screens/`: start menu, song select, results (a
  finished play's built alike by both shells, `results_facts::plan`),
  options, the pages for
  calibration's menu and the credits (`page.rs`: headings, text,
  links, buttons, rows of buttons, folds, fields, notes and zones a
  shell finds under a drop, `Zone.<id>`, lit by `PAGE_TARGET`), the
  announcer studio's page made from its draft (`studio.rs`; what each
  press does, the same for both shells over each one's `StudioHost`,
  `studio_shell.rs`) and the
  frame its scenes are shown in (`stage.rs`: a chrome drawing the
  screens under it smaller, `Screen::stage`; `Ui::mark` rings what a
  screen names, `Screen::spot`), the app's notices as the UI's
  chrome, the play's start prompt, `prompt.rs`: Start never waits for
  what loads behind it, a loading screen after Start until the play is
  ready, a countdown on song select's plays, and this play's options,
  `prompt_options.rs`; its controls diagram, `prompt_controls.rs`, shows
  the chart's receptors and primary bindings for the active keyboard/controller
  through the independent field preview, pressing each pair for a brief looping demonstration (140 ms
  per key, one-second pause; static under reduced motion); song select readies
  the diagram's skin for the chart it shows (`Controls::for_chart` from its
  place report, each shell's field preview's `warm`), so the prompt shows it
  from its first frame. That preview draws nothing until the skin wanted is
  in, readied for the layout wanted (the last frame holds meanwhile), never
  the built-in arrows on the way; on the web one preview outlives the pages.
  Modifiers sit below
  the diagram on the same card, without tabs or instructional copy;
  song select's quick volume sits in the card's top right corner (every
  card but a calibration's, `PromptState::without_volume`), "Music is
  muted" beside it while the music is muted or at zero, never the first
  focus nor heard by the countdown (round AB);
  a watched stage's prompt has neither (`PromptInfo::watched`, set by
  `set_watched_stage`: the card, the watching line and the buttons);
  auto-start always takes priority. Its countdown fills the Start button's
  background beneath the label (whole-second steps under reduced motion),
  without a separate bar or countdown text. A room's stage's prompt is
  a larger translucent card (`ready-card-v1`, the look's `Dialog.Glass`)
  with the players in columns, `room::players_part` drawing the players
  panel's line, `room::player_line`.
  `docs/plans/start-prompt.md`) with their sample
  data, options' rows made from the settings and the shell every
  platform shares (`options_model.rs`, `options_shell.rs`, the bind
  flow in `crates/ui/src/state/bind.rs`). Controller menus use shared
  `ControlBindings::menu_control`: West opens visible More actions, shoulders
  change charts/sections/panels (editor: lanes), and dedicated Start stays
  distinct until the screen resolves it. Standard-controller binding has
  explicit Capture and navigation phases; raw devices keep guided learning.
  `screens/numbers.rs` supplies controller-editable numeric/compound forms;
  `screens/code_plate.rs` desktop's sign-in code large on a plate that
  copies it when pressed (the sign-in dialog's, the Twitch access trip's
  and an invite page's, `page::Block::Code`);
  `crates/ui/src/radar.rs` a chart's skill radar (MSD's seven skillsets
  as shares of its overall, drawn from quads: the fill seven triangles
  cut by turned frame clips; the theme's `Radar.*` colors; each spoke's
  name and number by `radar::place`'s label rule, at the square's edges
  and corners where that gives the shape more room), always shown (no
  option since round SLB) as the second of the wide song side's two
  squares beside the cover (`docs/plans/song-navigation.md`, "The song's
  side"; a line in its place where the chart has none,
  `ratings::Standing`; phones show none): MSD's seven skillsets, the DDI
  estimate's eight skills' costs at the chart's matched level,
  `ddi_library::manifest::matched_profile`, or under From the song the
  Groove radar's five values (`ChartInfo::groove`, `ChartEntry::groove`,
  every chart at once); `crate::state::ratings` the
  chart ratings' state (`docs/plans/msd-ratings.md`: Chart ratings, the
  song's, MSD or the DDI estimate, `ratings::Source`, the DDI estimate by
  default since round SLB, `Settings::ratings_moved`, rated in the
  background for everyone; the bundled songs' MSD and
  estimates from their manifest, `ChartInfo::msd`, `ChartInfo::estimate`,
  the estimates kept under `ddi.estimate.v1`; the rest rated in the background by a
  shell, desktop's `crate::ratings` thread, the web's worker, one song at
  a time, never during a play, `set_playing`; kept by chart digest and
  calculator version under `ddi.msd.v1`; `ChartEntry::meter_text` and
  `chart_line_rated` show MSD in the number's place, the song's own
  fainter where a chart has none; everything that decides by a chart's
  meter keeps the song's);
  `crates/ui/src/stars.rs` a CPU's level as five stars (levels 1 to 5
  silver from the left over empty outlines, 6 to 10 gold over silver;
  vector shapes, never a glyph or a number: the look's `Stars` component
  through `stars::stars`/`stars_left`/`stars_right`, sized by a height,
  its colors `Star.Gold`/`Star.Silver`/`Star.Empty`; the play's
  miniatures draw the same geometry, `ddi_shell::opponents::draw_stars`);
  `crates/ui/src/heart.rs` a song's heart on song select, the same way
  (two stadiums, each cut at the middle by its own clip, so a faded heart
  never shows overlaps: the look's `Heart`, `Heart.Empty`); favorite
  songs (`state/favorites.rs`, `ddi.favorites.v1`, kept apart from the
  settings by the song's library id) and the Favorites pack at the top
  of the list (`screens/select_favorites.rs`; place key `favorites`;
  `docs/plans/song-navigation.md`, "Favorites");
  free-text entry alone may require keyboard/IME. `state/battle_plan.rs`:
  a stage's checkpoints, sections and reading horizon from its chart,
  practice's ledger and an online room's battle plan alike (the picker's
  device offers it, every device compares its digest before Loaded,
  `prepare_stage`, `RoomClient::finish_load_live`). Editor South places notes,
  Start listens, holds/rolls use head/tail confirmation; More exposes the same
  operations for minimal custom controllers. The UI core owns controller
  input (`docs/plans/controller-input-core.md`): the shells hand `Ui` the
  pads (`set_pads`), the bindings (`set_controls`; Options sets what it
  edits through `Cx::set_controls`) and each edge as read (`UiEvent::Pad`),
  which it resolves to a menu action or hands on raw to a binding screen,
  remembering what each held control pressed so a release or a pad's going
  releases exactly that. `controller_hints.rs`'s `Hints` (the `Ui`'s, read
  through `Cx::hints`) keeps the deliberate input source across screens (the
  web carries it between hosts, a play's own input sets it; a finger or a
  pen makes it `Source::Touch`, the web's first one where a finger is the
  primary pointer), classifies
  controller metadata and resolves hints through the effective
  dispatcher/play assignment. A play's touch controls
  (`docs/plans/touch-controls.md`): `crate::touch_controls` places them,
  pure (`Settings::touch_controls`: Buttons under the thumbs, by default,
  up to seven lanes and 44 px wide, else the dance mat's pad,
  `ddi_dance_mat::MatLayout`, two for the doubles, or the legacy Columns;
  `Settings::touch_hand`, the odd layouts' extra button's side;
  `Settings::touch_size`, Smaller, Standard or Larger from their
  corners, round DE) and finds
  a finger's lane (`Controls::lane_at`: a slide changes lane only
  `DEAD_BAND` inside the next target); their glyphs are the start
  menu's rain outlines, one quad of an atlas the game rasterizes a
  target, turned as its receptor (`touch_controls::glyphs`: the rain's
  arrow, onigiri, a round-cornered square for pump's center, a ring
  else; source `ddi:touch-glyphs`, answered by
  `ddi_shell::touch_controls::serve` in both shells' menu painters);
  `ddi_shell::touch_controls` draws
  them over the field while touch is the source (the field unchanged
  but under Reverse on an upright screen, where its receptors stand just
  above them, round DE, `ddi_shell::play_view::receptor_floor`,
  `FieldGeometry::lifted`; the look
  still open: `touchlook=` on the web, `DDI_TOUCH_LOOK` on desktop,
  TESTING ONLY); the shells press lanes through them (the web's
  `web::touch`, desktop's `Shell::touch`; which pointer holds a lane,
  the quit button, the policy label or the volume is one bookkeeping
  for both, `ddi_shell::play_pointers`, round PU; while a web play is on
  nothing of the page is selectable, `web::touch::NoSelection`); the
  prompt's diagram shows each receptor's control cut out under it, with
  its glyphs (`prompt_controls::touch_cut`). Overrides
  live separately in `Settings::controller_styles`; raw controls stay numbered
  even under a family override. New Dance Single defaults are LB/LT/RT/RB,
  then D-pad/face alternatives; existing saved/cleared tables are preserved
  (`docs/plans/controller-hints.md`). The state screens read (settings and
  their migrations, the song list's place, song select's data from a
  library (`select_view.rs`), the play
  history and its replays, `crates/ui/src/state/history/`, and when its
  stale plays are judged again, `reworker.rs`, kept plays judged again
  under other rules too, `Plays::rejudge`, both judgments kept,
  `PlaySummary::rejudged`; stored through
  `ddi_platform::storage`, which each shell installs), the chart
  editor's model (`crates/ui/src/state/editor/`: edits with undo; the
  music's waveform and spectrogram, `audio.rs`, which its audio view,
  `screens/editor_audio.rs`, draws on the field's rows) and
  screen (`crates/ui/src/screens/editor.rs`, `docs/plans/editor.md`;
  its note record, `state/editor/record.rs`, takes a press's time stamp
  as a play judges it, `ddi_session::listen::Listener::heard_at`) and
  the developer screen both shells run (`crates/ui/src/dev.rs`). Pure,
  natively tested.
- `crates/l10n/` the UI's languages (`ddi-l10n`,
  `docs/plans/localization.md`): Fluent messages in
  `locales/<tag>/<area>.ftl` (`locales/languages.toml` lists the
  languages, English first, the source), each English message a typed
  function made at build time (`ddi_l10n::msg::start::quit()`, its
  comment's declared variables its arguments) returning a `UiText` in
  the thread's language (`choose`: the settings' `language`, else the
  first of the system's languages a shipped language speaks, its region
  or script set aside (`ja-JP` is `ja`), else English,
  `set_system_languages`; English and Japanese shipped, Japanese since
  2026-11-17; `force` for `--lang` and the web's `?lang=`; the web
  page's loader chooses alike, both held to
  `locales/system-language-cases.txt`), missing messages from the fallback then English, numbers in
  the language's marks; `UiText::data` for what is not ours to translate;
  the pseudo-languages `qps-ploc` (accented, 40% longer, « », data ‹ ›)
  and `qps-wide`. A change of language rebuilds every screen in place
  (`Cx::rebuild_screens`, `Ui::rebuild_all`). A language's script beyond
  Iosevka Term's is drawn in its faces (`languages.toml`'s `faces` and
  `[[face]]`s; Japanese: Noto Sans CJK JP, cut to what the messages use
  into `assets/fonts/` by `xtask l10n fonts`), which `ddi_l10n::font_stack`
  puts after the theme's face in its stack; desktop embeds them (feature
  `embed-faces`) and resolves a stack per grapheme as a browser does
  (`ddi_render::cosmic`), the web fetches them once the language is
  chosen (`web::text::load_language_faces`); the painter tells the
  rasterizer the language when it changes (read from `ddi_l10n::current`
  as it lays out text: the system's faces for Han in its forms, the
  page's `lang`). Lines break by Unicode's
  rules (`ddi_actor::text::wrap`, UAX #14: Japanese between characters,
  never before 。、」ー). Beyond the canvas: the web page's loader says
  `boot.ftl`'s lines in the language the game will choose (English's
  and each language's first ones inlined by `xtask minify-boot` into
  `site/boot.js`, the rest fetched from `boot/<tag>-<hash>.json`), the
  shells' plain
  pages, desktop's app menu (`crates/desktop/src/app_menu.rs`, macOS)
  and file dialogs, and the packages' `Info.plist` and Linux entry say
  `shell.ftl`'s. `locales/README.md` says how to add a message or
  a language; `locales/<tag>/GLOSSARY.md` the terms and tone.
  Platform-free, natively tested.
- `crates/theme/` our theme (`ddi-theme`, `docs/plans/theme.md`): `Infinito`,
  a look for the UI core whose components are the maintainer's mockups,
  its colors, faces, sizes, timings and places in
  `crates/theme/src/infinito.toml` (turned into tables at build time). The
  play HUD is shared by both shells (`crates/render/src/hud.rs`, `hud.wgsl`;
  `docs/plans/play-hud.md`), using the accepted `tools/play-hud-design/approved.json`:
  a life gauge four lanes wide over the field, filling from the middle out
  and less than life's share below full (drawing only),
  with quiet jade/mint turbulence, coral damage and blue recovery trails,
  Vivid at full life, a battle's attack power directly above life in
  violet, half as thick, on the field and each miniature
  (`hud::BattleBar`, `Renderer::set_battle_bar`, `Miniature::battle`),
  Iosevka judgments and lighter SemiBold numbers,
  transparent combo outline gaps at each hundred and inverted color at 1000,
  and a still gray-outlined title watermark. Its host-time motion resets per
  play and follows reduced motion. The browser study remains independently
  editable; `generate.py` makes the digit-distance/title-mask R8 atlas from
  the bundled fonts and title art, without runtime image decoding.
  The start title is shaped Dosis artwork (`tools/title-design/`, `assets/title/`),
  with Infinito Vivid paint on D/D/I through the shared 2D painter; reduced
  motion freezes both its palette and evolving texture. The start menu's
  slow outline rain (`crates/theme/src/rain.rs`, `assets/ui/menu-rain.svg`)
  shares a stationary muted Vivid field; the screen supplies its elapsed
  clock through `Look::animate_component` and freezes it for reduced motion.
  Start-menu choices use neutral plates and open without focus; the first
  key/menu-button press only focuses Singleplayer. Hover moves button focus,
  while direct clicks/taps and explicit return-focus requests still work
  (`Screen::defer_initial_focus`). The web shell clears saved start-route
  focus at boot, so reloading also opens unfocused. A shared Vivid active outline
  (`crates/theme/src/active.rs`) has a 3 px line and 3 px gap, with the title's
  palette cadence and 8× spatial evolution (`Active.EvolutionSpeed`).
  `Screen::presentation`/`active_element` select one target on the input layer;
  song select uses the song, options the active row, pages the active item.
  Dialogs take exclusive ownership. `Look::place_measured` moves parts that
  follow measured text (song select's length and pack after a BPM range),
  each frame between measuring and drawing. `Look::active_element` paints the outline;
  `Ui` owns its clock and idle/reduced-motion handling. `Actor::decoration`
  excludes visual adornments from bounds and hit-testing. The field stays
  viewport-anchored, and `Vivid::clock` keeps palette/evolution independent. The
  menu pointer is a shared UI overlay (`ui::cursor`, `theme::cursor`,
  `assets/ui/cursors.svg`): Options → Display chooses Infinito cursor
  (the default since 2026-11-17, stored as `PointerTip`, once named
  Pointer tip), Infinito arrow or System and a logical-pixel size. Enabled targets charge at
  120 BPM; mouse-down shines at the hotspot without changing activation.
  Text/drag cursors stay native; touch, keyboard/controller input and focus loss
  hide the overlay. Reduced motion freezes paint/charge and burst growth. The
  shortcut indicators use original OEM-inspired SVG keycaps
  (`tools/keycap-design/`, `assets/ui/keycaps-a1.svg`, `keycaps-a2.svg`,
  `keycaps-light.svg`, `keycaps-gray.svg`, `keycaps-deep-gray.svg`), preserving charcoal satin/semi-gloss
  and neutral/darker gray satin alternatives with live legends. Gray caps use dark ink, including
  their arrow and keyboard marks, through the theme's Keycap.Light colors.
  UI images decode the bounded noteskin SVG subset on desktop and through
  the web SVG worker (browser fallback); deeper gray satin (#7d7d7d) is the selected default,
  with 26 px caps, 10 px legends, no extra inter-key gap and 18 px between hints.
  `ddi_ui::keycaps` distinguishes a specific keyboard key from custom printing
  on a normal or space body. The theme's `keycaps.rs` draws equal vector arrows
  with Cherry-style mixed placement; the shared `KeyHint` helper binds the
  look's atlas to its `CapN` parts. `Spec::key_prefix` supplies plain text
  outside the caps (song select's Chart repeat count). Arrow shortcuts use
  one cap per direction throughout the screens. `tools/keycap-design/playground.html` is the sizing
  study, with shared controls for cap height, legend size and spacing, now
  retaining the final deeper gray and previous gray comparison with the selected Iosevka Term Regular
  Italic legends. Both shells register the real bundled italic face.
  Controller hints use original shared-satin rendered art from
  `tools/controller-design/` in `assets/ui/controllers.png` (family-specific
  faces, shoulders/triggers, fixed-size D-pad direction masks and pressed states).
  The atlas is shipped directly; Blender/Pillow are authoring-only dependencies.
  The screens it dresses are `crates/ui/src/screens/` (start menu, song select, results,
  options; theme-agnostic, fed plain data). Pure, natively tested.
- `crates/noteskin/` StepMania noteskins: fallback chains, file lookup and
  redirects, `metrics.ini`, file name hints, what each element is (stand-ins
  for StepMania's `default` and `common`), its actors built (`actor/`) from
  images, MilkShape models and commands, and
  (`crates/noteskin/src/lua/`) a skin's Lua run at load over `lua/`; StepMania
  3.9's and OpenITG's format built natively (`crates/noteskin/src/legacy.rs`);
  SVG images in a checked subset (`crates/noteskin/src/svg.rs`, our extension);
  the buttons a skin draws without files of their names declared in its
  metrics (`[Global] Buttons=Onigiri,Center`, our extension);
  elements drawn by components of a skin's SVG, `Def.Component` with
  `Bind.*` bindings on the engine's clocks (`crates/noteskin/src/component.rs`,
  our extension, `docs/plans/noteskin-scene.md`; Infinito uses components for
  taps/fakes, receptors, holds/rolls (`Def.LongNote`) and lifts (`Def.Lift`), with
  masked WGSL holographic/wave paint; explosions use judgment components and
  original effect art, with lane/time-stable material variation. Explosion
  `rim-left`/`rim-right`/`link` roles join visible matching judgments on
  adjacent receptors (`crates/render/src/noteskin/judgment_links.rs`). Mines retain
  actors for their adjacency-linked electricity);
  an element of our own for a multiplayer battle's shock, `Tap Shock`
  (`[NoteDisplay] UseTapShock=1`, `docs/plans/noteskin-authoring.md`:
  loaded as any element for a play that may be given shocks,
  `lua::Env::shocks` and `NOTESKIN:PlayHasShocks()`; the renderer draws
  the notes numbered from `HAZARD_INDEX_BASE` with it, on the field and
  the miniatures, and a chart's own mines with `Tap Mine`, a row's links
  never joining the two; a skin without it draws shocks as its mines;
  Infinito's is its lane-shaped shock arrow, its mines the spiked band
  wherever the play may be given shocks; the built-in arrows tint a
  shock violet);
  opt-in mine row links from actual drawn positions (`MineConnectRows`,
  a `Tap Mine` frame’s `MineLink` child, with optional `MineRimLeft`/`MineRimRight`
  children hidden on connected sides; `docs/plans/noteskin-authoring.md`);
  opt-in roll body/cap animation from the active scaled roll timing window
  (`RollAnimationTimingWindowRatio`, `docs/plans/noteskin-authoring.md`);
  parameters a skin declares in its metrics, kept per skin in the
  settings, shown under Options → Arrows and read by its Lua
  (`NOTESKIN:GetParameter`; `crates/noteskin/src/params.rs`, our extension,
  `docs/plans/skin-parameters.md`; Infinito's Vivid (default), Rainbow and Flat colors, onigiri, mine shape,
  per-note charge timing (on by default), receptor fill and hit effects are its parameters; Peters-DDR combines
  the former Note/Vivid choices with Colors and Explosions, retaining
  their names as hidden compatibility variants);
  the skin keys settings store (`crates/noteskin/src/bundled.rs`); the same
  Lua host runs a song's scripted backgrounds over its folder
  (`Skin::song`, `LuaSkin::actor_file`, `lua/song.lua`;
  `docs/plans/scripted-backgrounds.md`), and builds a song's StepMania 3.9
  and ITG animations (`BGAnimation.ini`, `default.xml`, 3.0 layer
  folders) natively (`crates/noteskin/src/bganimation.rs`). Pure,
  natively tested; `docs/plans/noteskins.md`. `crates/render/src/noteskin.rs`
  loads a skin through its Lua for both shells (`NoteskinPlay::load_lua`),
  draws it in play and runs its Lua each frame. Our opt-in
  `HoldUseDroppedState` loads separate per-note dropped actors; hit messages
  carry `Autoplay` and local `HitX`/`HitY` from the hit note's last drawn
  position (`tools/noteskin-danoni/README.md`). These are presentation data,
  never changes to judging or replay inputs. `Def.Lift` draws a lift's
  leading, time-sized trail in the skin's material (`Frame.note_motion`
  supplies local scroll speed and beat duration for the lead).
  `TapJudgmentsPerNote` opts into
  immediate per-note flashes rather than StepMania's completed-row effects.
  Per-part `TapNoteAnimationOnChildren` (and `TapLift`, `TapFake`,
  `HoldHead`, etc.) opts layered notes into one beat/Vivid animation phase;
  descendants otherwise keep their own clocks, including mine electricity.
  `Def.LongNote` draws holds and rolls in the skin's material over band,
  envelope and shadow roles, with protected head holes and one fill from head
  to cap (Infinito's holds, rolls and lifts are drawn so, by its own wave
  material); see `docs/plans/noteskin-authoring.md`. Legacy holds and rolls
  keep their own path.

- `crates/lua/` StepMania's Lua for skins, sandboxed (`docs/plans/lua.md`):
  `lunita` (crates.io), our hardened fork of piccolo, with per-VM memory
  and string limits, fuel and a deadline per call, StepMania's `cmd`
  keyword, Lua 5.1's globals and nothing that reaches outside.
  Platform-free, natively tested.
- `crates/session/` one play session for both shells (`ddi-session`,
  `docs/plans/desktop.md` 7.1): the play (`play.rs`: the engine's
  `Player`, bindings, the quit gesture, autoplay and `auto=pad`, the
  fail's fade, a work's start and end fades, the facts a play is kept
  with, a kept play replayed from its log; what a play is for, decided
  once for both shells, `play::Purpose`: kept, given the ghost and the
  announcer's `Prior` or not), the event sounds (`sounds.rs`:
  the mine sound and the assist clap, made in code, the clap scheduled on
  the audio clock), calibration's modes, chart and click track, backgrounds' frame,
  the sides their images are kept at
  and the automatic video setting's step, a work's layers' timeline, the
  play's background movies (`movies.rs`: `MovieSlots`, which segments
  have a decoder, their tracks and what shows, each shell's decoders
  behind `MovieShell`; `BackdropTextures`, both shells' `movies.rs` run
  on it; song select's `#PREVIEWVID` movie, when it plays, `movies/preview.rs`), a song's scripted backgrounds and foregrounds (`scripted.rs`:
  their VM, the commands at each change, one budget a frame, their
  movies' clocks, one a file, and the danger told them), the
  announcer's lines in play and on the results (`announcer.rs`: 5.1's
  timings and precedence over what the play knows, the sounds a play
  decodes and their shuffles, spoken on the voice's bus; song select's
  intro and comments, which it asks the shells for with
  `UiRequest::Announce`, decoded from `menu`;
  `docs/plans/announcers.md`; the announcer studio's scenes,
  `announcer/scene.rs`: song select's data shaped so each comment has a
  chart that gets it, and for each results line a play staged on a
  chart made for it, judged by the engine under the player's ruleset
  where it gives the line, `docs/plans/announcer-studio.md`, step 3; and
  for each line said in play, `announcer/scene/in_play.rs`, a staged
  play: `play/staging.rs`'s `Staging`, from part-way with what came
  before judged at once, `Player::start_staged`, notes missed or pressed
  off, a combo given, step 4; `announcer/scene/director.rs`, what a
  scene does and when over a shell's `SceneHost`, step 5); generic over `ddi_platform::AudioBackend` and a shell's `Inputs`
  (`crates/web/src/play.rs` is the web's); a multiplayer practice
  stage's CPU opponents (`battle.rs`, `SessionConfig::opponents`, apart
  from the solo CPU demo's `cpu`: each a `ddi_engine::cpu::Opponent`
  judged beside the play, shown read-only, the stage's outcome as the
  room's ranking, `take_stage_outcome`; reported to its room for both
  shells, `battle::report_outcome`; a stage's prompt left, the shells'
  one way, `battle::leave_prompt` (withdrawn before its start, else a
  forfeit: an online room's as the player's own run, a practice's CPUs
  played out, `battle::forfeit`, or the match stopped); an announcer's
  sounds for a play, a stage or song select, `announcer::For`; the
  battle's moments it tells,
  `Battle::take_events`, spoken by a stage's voice,
  `announcer/battle.rs`: the provisional `BATTLE_VOICE_V3`
  (`battle-voice-v3`), its priorities, one battle line at a time, the
  combo lines below them all, `Line::is_combo`, said only in a quiet
  moment and at most a second late, and so the lines of the player's own
  play's end (`end_line`: the full combo's, cleared, failed; decided
  once the frame's battle moments are told, `Announcer::stage_play_end`,
  cleared and failed never after a line that said the stage's outcome
  nor where the voice has the lines its results will say of it,
  `Announcer::results_say`),
  a play's other routine lines left out; the player's steps under Adaptive charts said at their
  boundaries, `battle step up` and `battle step down`, never over a line
  being said, `Announcer::battle_patient`, `PlaySession::tell_steps`,
  `steps_told`; the results' line once the shell's `stage_reported` passes the
  room's `MatchEnd`; where no line said one, `PlaySession::battle_cue`,
  shown in the miniatures' status line, `ddi_shell::opponents`;
  the match's rules for a stage, everyone's and the CPUs', DDI with the
  match's difficulty and Boos and never Practice, `battle::stage_ruleset`
  from `ddi_ui::state::room::StageRules`; the local player's attack
  moments (`battle::AttackMoment`: an attack against them committed, one
  of their own), told at most once in six seconds for those against them:
  said by their battle lines (`battle attack incoming`, before its first
  shock comes into view or not at all, `Announcer::battle_by`; `battle
  attack launched`; `VoicePolicy::priority`, no fallback) and shown in
  the status line exactly where the line was not said
  (`PlaySession::stage_cue`, `AttackTold::said`), and their attacks
  summed up for the results
  (`Battle::attack_summary`, `StageOutcome::summary`);
  an opt-in study of attack rates between CPUs over the bundled charts,
  `battle/study.rs` (`cargo test --release -p ddi-session --lib
  attack_rates -- --ignored --nocapture`), and one of putting combos
  into DDI's score, `battle/combo_study.rs` (`combo_score_study`, the
  same way: candidate combo parts worked out beside the score without
  one over made-up plays, CPU plays, stages, an easier rung on purpose
  and, with `DDI_COMBO_PLAYS`, the maintainer's kept plays in
  aggregate, the engine's `ddi-combo-v0` asserted equal to its
  candidate; `docs/plans/ddi-ruleset.md`, "DDI's score and combos: a
  study" and "Combos count"), and its part 6, `combo_study/percent.rs`
  (`DDI_COMBO_ONLY=percent`: percents and grades counting the combo
  beside today's and beside other rulesets' readings of the same
  presses; "Should the combo score count toward the percent?"); since
  round U its "today" is the rule before it (its own `TAP_POINTS` and
  `MINE_POINTS`, every engine play read back on it, `before_u`), and
  part 8 (`DDI_COMBO_ONLY=built`) checks round U's rule as built over
  the kept plays;
  the local player's attack policy changed in play (D8,
  `Battle::policy`, `retarget`): either Shift and Tab, each unless the
  player's bindings give it to a lane (`play::POLICY_KEYS`, the shared
  `ddi_ui::controller_hints::POLICY_KEYS`; the stage's prompt shows the
  key under its policy row, the play a small cap after the label,
  `PlaySession::policy_key`), a tap or click on its label
  (`TOUCH_POLICY_CONTROL`) and a pad's Back pressed briefly (held it
  quits as ever; pressed twice it is two steps, never a quit, where the
  policy cycles: `play.rs`'s `policy_pad`), each kept in the play's log
  at its judged second and given to the ledger, whose launches follow
  it from the next checkpoint, the same at any frame rate and from the
  log; `StageOutcome::targeting`, what the play left it at;
  the local player's own attack relationships while they last
  (`Battle::links`: each attack they sent or were sent, its opponent's
  seat, when this play learned of it, whether it waits behind another,
  a chain) and the seats kept in view beyond the cap for them
  (`kept_seats`);
  the attack ledger run locally, `battle/attacks.rs`'s `AttackRunner`:
  a checkpoint once every competitor still in is judged past it, so the
  same at any frame rate and from a play's log; the chart's sections
  from the engine's placement, each competitor's lead from the player's
  reading horizon plus an online room's network lead
  (`battle::practice_lead_us`); commitments delivered by `Battle` (into the play,
  a CPU's and its field), the stage settled by `SETTLEMENT_V1` (the
  one left while attacks against them are still to come last standing,
  `Battle::last_standing`, `last_standing_me`, `WatchMoment::LastStanding`,
  before the win), the CPUs
  and their attacks played out once the player is gone,
  `Battle::play_out`; an extreme slow speed raised on such a stage,
  `ddi_ui::state::battle_plan::raised_speed`;
  each CPU's miniature field fed by its stream of judged moments,
  `battle/feed.rs`: `Moment` (when, which note or shock, a tap's judgment
  or a shock stepped on, life, combo, out), the shocks put into its play
  drawn from their commitment as the skin's mines (`FieldFeed::add_hazards`),
  a lane a shock locked for the lock's time (`FieldFeed::locks`),
  the chart as it plays it, `TapTrack::full` (`battle::cpu_track`:
  holds as held or let go, rolls, lifts and mines; `TapTrack::new` the
  tap-only copy `cpu-play-v0` played), and a presentation delay, 0
  for local CPUs, so a network stream can feed the same view later;
  an online room's CPUs played alike on every device, `battle/cpus.rs`
  (`Drive`: in fixed steps, the room's shocks put in at a song time
  everyone knows; its proof, `cpus/tests.rs` with `fixture.txt`,
  checked natively and built for wasm32, in CI too: under Node's WASI
  by `.cargo/config.toml`'s runner, `tools/wasi-run/wasi-run.mjs`, and
  on each desktop system);
  an online room's stage played live, `battle/live.rs`
  (`docs/plans/multiplayer-online-play.md`): `Battle::with_live` over a
  `LiveHandle` (the room client and the shell's steady clock,
  `ddi_platform::steady`, `SessionConfig::live`), the play started at
  the room's instant (`live_start`, a late join up to `late_join_ms`,
  past it a forfeit watched as a spectator watches, `Watching::Late`;
  one who missed the Ready window may enter
  within it, `Event::EnteredLate`, the others' plays drawing a field of
  them from then, `Live::admit_late`), a room's stage played on while its
  page or window is hidden (`PlaySession::plays_on_unseen`,
  `Inputs::hidden`; the shells' timers judge it on: the web's
  `GameLoop::tick_unseen`, desktop's `Host::step_unseen`; singleplayer
  still stops as interrupted, and CPU practice alone waits for the page,
  its Ready held back while unseen, `RoomClient::waits_unseen`) and
  started there when the room starts the stage its player readied for
  (`Screen::unseen`, `Ui::step_unseen`: the
  prompt's room run and Start pressed without a frame, on the web's
  prompt timer and its socket's wake), its moments and checkpoint reports streamed, the room's
  frames feeding people's fields (`TapTrack::full`, drawn wholly the
  device's view delay behind the song, `FieldFeed::set_delay`, a stall
  raising it, stale ones dimmed; `__DDI_DEBUG.live.delay`), its commitments into the play (refused when too
  late), its records driving the bars, warnings and links, its decision
  shown; `LivePlay` the same for the bots;
  a stage watched (`play/watch.rs`, `watch-v1`: on after its decision
  while anyone still plays it, `Battle::still_playing`,
  `docs/plans/multiplayer-prototype-progress.md`, "Watching a stage"):
  by a spectator, a member who chose Watch (`StageSetup::watching`: the
  session's own player never judged, its clock alone; the CPUs alone on
  the song clock, `Battle::advance_watched`, the watcher in no roster;
  every field, or one competitor followed, `PlaySession::watch`, whose
  `frame` is then the followed stream's, `FieldFeed::frame_large`: a
  moment's `offset`, `w0` and `score` say the judgment's word and the
  score, nothing is judged twice), and by a player out of a stage that
  goes on (`Battle::goes_on`): out by life, or by leaving its play,
  which is a forfeit (`Battle::forfeit_now`; `PlaySession::leave` from
  outside the play), watched on from their own field until it is
  decided (`WATCH_END_HOLD`), the song ends or they skip (Enter, Start,
  `TOUCH_SKIP_CONTROL`); who attacks whom for a watcher,
  `Battle::seen_links`, the same for every way of watching (a player
  out of it a field too, `Battle::own_seat`), and the stage's moments in the third person,
  `WatchMoment`; an online room's stage has nothing to watch yet and
  ends as before).
  A room host's Twitch chat beside a play (`chat.rs`,
  `docs/plans/multiplayer-twitch.md`, part C): its source advanced on
  the host's clock in the session's tick, never judged, logged or
  replayed, `PlaySession::chat`; its sources the scripted chat (TESTING
  ONLY, `chat::set_scripted`, its line
  `ddi_ui::state::twitch::scripted_ceiling`) and since round TW6 the live chat
  (`ChatSource::Live`: `ddi_chat::live`'s connection driven in the
  tick over a shell's `LiveIo`, installed by `chat::set_live`, the
  channel's emote sets, from the play's start to its end, but in an
  online room the room's: kept connected between its stages while the
  room shows chat and this device may, lent to each stage, round DE,
  `chat::keep_for_room` told by `RoomClient::chat_kept` as the room
  ticks, `__DDI_DEBUG.chatKept`; every
  player's since round TW7, a multiplayer feature alone: an online
  room's stage the room's channel through `POST <api>/rooms/<id>/chat/subscribe`
  (`RoomChat::room`), a singleplayer play and CPU practice none and no
  connection for it; only the development script,
  `ddi_ui::state::twitch::set_dev_live` (one's own channel, or with
  `live:<login>` a named one, `DevLive::Channel`, also in an online
  stage's room's channel's place on that device,
  `RoomChat::on_this_device`), gives solo plays and practice one,
  through `POST <api>/me/chat/subscribe` (a named channel's `channel`,
  administrators only); `ChatLayer::live_report`, the report's lines
  through `LiveIo::report`, and why it could not show,
  `ChatLayer::trouble`, `ddi_chat::live::Trouble`, one quiet line on
  the stage's results, `StageOutcome::chat`), where Options →
  Display's "Twitch chat in rooms" is on; `chat::room_ceiling` the one
  answer to the room's ceiling: in a room's stage the room's own
  (`RoomState::twitch`'s chat, carried by `StageSetup::chat`, a
  `ddi_ui::state::twitch::RoomChat`; none, no layer at all; and since
  round SD followed while the stage plays, the host's every change at
  once, `chat::Following` over `LiveRoom::room_chat`), outside a
  room the script's line, for tests (round TW3).
  Parts of a frame timed for a shell's frame recorder (`perf.rs`: a
  clock set only while the web's `perf=1` records; the battle's
  advance). Platform-free, natively tested on a fake clock.
- `crates/shell/` what both shells do between the settings (`ddi-ui`) and
  `render` (`ddi-shell`), so neither keeps a copy: a play's
  `RenderOptions` and Options' field preview's options and skin from the
  settings, the skin a play readies with its parameters' values (the
  preview's the same, `play_skins`), the note colors as the renderer has them, the ghost's lead as
  the HUD shows it, and a work's layers' textures
  (`crates/shell/src/stage.rs`, generic over each shell's decoded content:
  the web's `ImageBitmap`, desktop's RGBA rows) and a song's scripts'
  (`crates/shell/src/scripted.rs`), and a play's generated background:
  the look the settings give a song, its texture at the frame's size
  (within the device's largest texture), its place in the backdrop's
  list, the cover the kaleidoscope folds and the start prompt's still
  of it, drawn for the 2D pass when it asks (`Stills`)
  (`crates/shell/src/generative.rs`), and a play drawn: one frame body
  and its setters for both shells (`crates/shell/src/play_view.rs`,
  `PlayView` over each shell's `PlayHost`: its pictures, its movie
  decks, its log, its text; the web's `GameLoop` and desktop's `View`
  keep their surfaces; a practice stage's CPU opponents beside the field,
  the incoming attack's warning by the receptors (the nearer attack's,
  with a smaller glyph each for those placed behind it, up to three,
  then one and the count, `Incoming::behind`, `battle_cue::BEHIND_GLYPHS`;
  gone once its last shock has passed: no recovery since `attacks-v1`) the
  waiting line under the life gauge's right end and the player's
  attack policy, one small muted word under its left end, where a tap
  or a click cycles it (`battle_cue::draw_policy`, `policy_area`,
  `Drawn::policy_at`), and under Adaptive charts the chart played now
  on that line at the gauge's right end as song select names it,
  changing at a step's boundary, bright a moment with a chevron its way
  (`battle_cue::draw_chart`, `Battle::chart_line`, `chart_shown`; the
  waiting line then a line lower, or in its place where that would
  reach the receptors, `waiting_below_fits`),
  `crates/shell/src/battle_cue.rs`; the volume beside the quit
  button, pointer only, showing with it, but on a touch screen never in
  a room's stage (`play_volume::offered`) (round AB,
  `crates/shell/src/play_volume.rs`: its button and slider drawn by the
  2D pass, hit-tested by the shells from `Drawn::volume_at` as the quit
  button is, its edges handed to `PlayView::volume_edges`, never to the
  play's inputs; `play_volume::apply` moves the play's sound,
  `PlaySession::set_volume`, and keeps the setting; no keep-out of its
  own: the miniatures' slots never moved for it); who attacks
  whom for the player's own attacks, `crates/shell/src/attack_links.rs`
  (`attack-links-v1`): a line in the side space from the field's edge at
  the power bar to the recipient's miniature at its own bar, or from the
  sender's to the warning (a sender on the left: to the field's left
  edge on the receptors' row), a pulse along it for 0.6 s, the line gone
  after 1.5 s, the miniature outlined while the attack lasts, a chained
  attack's quieter; the line a curve (a cubic, two joined round what it
  keeps out of) that passes under the miniatures and is drawn behind
  them (`attack_links::draw_behind` into `Renderer::under_miniatures`,
  the sprites' `Shape::Link` and `Shape::Glow`; the outline and where it
  lands over them, `draw`), never over the band, the HUD, the judgment,
  the quit button, the status line or the ladder; no two lines sharing
  a stretch (`SPACING_V2`, `Routes`: a line laid once among those
  showing and kept while it shows, each on places of its own, never
  nearer than two strokes to another for longer than a crossing; with no
  free way, or past 64 lines, the outline alone until a line goes, then
  tried again, two a frame); no pulse and no fade
  under reduced motion; the theme's `Attack.Line` and `Attack.Glow`
  (`ddi_theme::attack_colors`); `PlayView::links_drawn`, and for tests'
  pictures `PlayView::stage_links`;
  the room host's chat behind the play, `crates/shell/src/chat.rs`
  (`ChatView`: the session's chat events into `ddi_chat::overlay` on the
  host's clock, over the whole screen behind the field, the miniatures
  and the HUD in the device's chosen style everywhere (round TW5: an
  ambient effect, nothing routed around, messages on a phone held
  upright too; the field filter's band dims it in the lanes),
  `chat::area`; every emote drawn by itself, never a count; drawn by the
  play's own 2D painter, which then draws twice a frame, into a layer
  the backdrop shows under the field, `Renderer::set_chat_layer`; its
  emotes from one atlas, `ddi_chat::cache`, fetched through
  `PlayHost::fetch_emote` (read before decoded, `ddi_chat::anim::probe`,
  then decoded off the frame: the web's `ImageDecoder` and
  `createImageBitmap`, desktop's `image` crate on the fetch's thread);
  an animated emote's frames each a cell, the frame drawn from the
  layer's own animation clock, one phase for every copy, held with new
  items and still under reduced motion; no new items from a late second
  to a good one, the hand's load rule's reading of a second);
  `crates/shell/src/opponents.rs`: miniature fields in stable slots of the
  side space on both sides, every opponent of a room its own at every
  size (eight at most; each a small copy of the play's field, in its
  proportion, `opponents::rows_for`: as many lanes tall, ten on a wide
  screen, eight to twelve where a window's shape gives another; as large
  as each side's room clear of the HUD, the quit button's place and the
  attack warning's allows, up to half the play's lane,
  `MINI_LANE_SHARE`, so a few opponents are no larger than many: lanes
  of 36, 36, 36, 33 and 28 px for 1, 2, 4, 6 and 8 at 1280×720 (without
  the attack warning's place; a stage with attacks, a practice room's
  default, keeps it clear beside the field: 25 px for eight); lanes of
  8 px at the least, in the margins a phone held upright leaves), with lyrics
  over the play too, a status line under the right side (beside the
  watermark where the sides are too narrow for words) for
  the battle's moment and "Last standing", then "Stage won" (the dancing hand goes behind the
  field during a stage, whole as a solo play shows it, its arm's end
  included, with no fade and no cut, in the band past the receptors'
  row and behind the judgment and the notes, `play_view::behind_field`,
  `dance::behind_field_distance`, ending above the touch controls on an
  upright screen; in a solo play on a screen taller than wide the hand
  takes the band clear of the receptors, the judgment, the combo and the
  touch controls, `play_view::upright_band`, `dance::UPRIGHT_ZOOM`),
  drawn by the play's own noteskin
  after its field on a band as its field's (`ddi_render::scene::Miniature`,
  `NoteLayer::miniature`: no Lua run, the play's field's state put back;
  a large field's miniatures under `opponents::CARRY_SKIN_NOTES` draw
  their holds, rolls and mines plainly, as the built-in skin does, in the
  sprites' one run, `Miniature::plain`, `plain-notes-v0`),
  each one's life gauge and combo by the play's HUD shader placed as the
  play's (`hud::Hud::prepare_minis`, each gauge's motion its seat's),
  names (cut between graphemes) and Out by the 2D pass, with
  each one's score small under its gauge (`opponents::ScoreShown`: DDI's
  score, the sum, or the percent, as the player's Score display has
  their own; never the proportional part alone; a CPU's its own, since
  it plays the whole chart, `cpu-play-v1`; the rescaling of a tap-only
  copy's sum, `cpu-score-v0`, retired) and a lane a shock
  locked darkened with its receptor's place tinted for the lock's time;
  a watched stage (`PlayView::watch_drawn`): a spectator's grid,
  everyone's fields over the whole screen in the play's proportion, up
  to a play's own lane (`opponents::arrange_grid`, `GRID_LANE_SHARE`:
  the half-lane cap is for miniatures beside a field of one's own; the
  renderer draws no field and no HUD of its own,
  `RenderOptions::no_field`), or the competitor followed in the play's
  field's place (`opponents::large_slot`: a miniature there, the one
  drawing of their notes, every field at the stage's song second less
  its own view delay) with the others beside it, their name plate under
  their life gauge (`battle_cue::draw_followed`: the name, a CPU's
  stars, their place of everyone or Out),
  who attacks whom between the fields (`attack_links::shown_between`)
  and each recipient's warning, the status line's "Watching [name]" and
  the stage's moments; a player's own field out of the stage, dimmed
  with Out or "Left the stage" and the skip's plate
  (`opponents::draw_own_out`); presses on the fields and the plate come
  back as `Drawn::watch_at` and `skip_at`; only
  beyond the cap (the development-only practice, at lanes of 16 px) do
  the opponents take turns in the slots, each in its own,
  the stage's kept seats (`Battle::kept_seats`) never rotating away
  (`opponents::rotate`): whoever
  attacks the player and whom the player attacked last, while those
  attacks last and a turn after; there is no list of opponents any
  more; past eight opponents, a large field of up to 72 (feedback round
  E1): every opponent a field of its own in seat order where all fit at
  `MINI_LANE_ALL` (72 at 1280×720 nine columns by four rows a side,
  lanes of 11; fields cut shorter, to `DENSE_ROWS_MIN`, only where that
  lets all fit), carrying what reads at their lane (`opponents::Carry`:
  the score, then the stars, then the name, then the combo go; a name
  beside a field attacking or attacked by the player), the miniatures'
  quads interleaved into one field's batches (`ddi_render::batching::
  interleave`); where they do not all fit, as many as do (never fewer
  than eight), those that matter to the player now in them, attackers
  first, then the nearest rivals by standing, changing hands rarely
  (`crate::rivals`); under the left side's the ladder: the player's
  place and, where not everyone is shown, a strip of everyone's life;
  Options → Display's "Opponents' fields: Up to eight"
  (`Settings::opponent_fields`, `RenderOptions::opponents_up_to_eight`)
  keeps the eight slots (a watcher's grid in pages of eight); a
  spectator's grid shows every field at the same least lane, in pages
  in the room's order only where they cannot fit, `grid-all-v1`,
  `PlaySession::set_watch_pages`; `__DDI_DEBUG.watch.trace` counts what
  changed from frame to frame).
  Platform-free (native and wasm), natively tested.
- `crates/app/` the game (`ddi`, the workspace's default member):
  a `main` that starts the web shell on wasm and the desktop shell
  everywhere else; Trunk builds it for the browser (`site/index.html`, with the page's
  styles, boot script and SEO data beside it in `site/`),
  `cargo run` for desktop; its `build.rs` writes the Windows (MSVC)
  executable's icon and version resource, with no resource compiler
  (`docs/plans/packaging.md`).
- `crates/web/` the web shell (`ddi-web`): the UI core's screens on the canvas
  (start menu at `#/`, `crates/web/src/components/page_routes.rs`; song
  select at `#/select`, `crates/web/src/components/select_page.rs`; the start
  prompt and the results on the play route,
  `crates/web/src/components/game_canvas.rs`; options at `#/options`,
  `crates/web/src/components/options_page.rs`; calibration's menu at
  `#/calibrate` and the credits at `#/credits`,
  `crates/web/src/components/page_routes.rs`; no device notices and no
  calibration card since 2026-10-29, `docs/plans/quiet-calibration.md`:
  a device first seen plays on the default offsets, kept as its profile
  from the play on, `Settings::register_devices`, and a calibration run
  settles by itself, `ddi_session::calibration::settle`, its line on the
  calibration menu, and the start prompt's one quiet line beside its
  buttons suggests calibrating or not by what the kept plays on the
  devices say, `ddi_ui::state::history::timing`,
  `screens::prompt::timing_hint`; Dancing☆Onigiri's lyrics drawn over the play
  by the 2D pass, `ddi_render::lyrics`), pages as components of our own
  (`crates/web/src/component.rs`: messages handled in a microtask, each page's
  few DOM elements its own; no Yew; `#/video-check`, the crash page and
  the no-GPU page are plain DOM; their host, `crate::ui_host`, the
  developer page `#/ui-dev` apart in `components/ui_dev.rs`), one wgpu canvas and
  device for the whole app (`crates/web/src/web/screen.rs`: made at boot, moved
  into the page that draws, its last frame held over the next page until
  that page draws, a second at most, made again after a lost device), Web Audio
  clock, settings in `localStorage` (`crates/web/src/web/kv.rs`), imported songs
  and the play history in IndexedDB (`ddi-library` version 4; the plays in
  `plays` and `play_summaries`, `crates/web/src/history.rs`,
  `docs/plans/play-history.md`; imported announcers in `announcers`, their
  sounds in `files`, `crates/web/src/announcers.rs`; the announcer studio
  at `#/announcer`, from Options → Sound, its draft and recordings kept
  in `files` under `studio/`, `components/studio_page.rs`, each line
  heard in its scene in a frame over the page, the real start menu,
  song select or results with the draft as the voice (our pack of the
  examples chosen where the line is not recorded yet), what it is said
  for ringed; a line said in play on the play route,
  `#/play?…&scene=<line>`, staged and drawn into a picture in the same
  frame, the voice handed over by `crate::studio_scene`,
  `docs/plans/announcer-studio.md`; ours listed by
  `announcers/index.json` and fetched from `announcers/`, a play's decoded
  behind its prompt there, spoken on a bus of their own beside the
  master's, `web/audio.rs`, song select's on its preview's context),
  replays on the play route (`#/play?replay=<play id>`,
  `crates/web/src/replay.rs`), the chart
  editor at `#/edit?song=<id>&chart=<n>` (`components/edit_page.rs`;
  its session, saving and playtests `#/play?…&edit=1`,
  `crates/web/src/editor.rs`; new songs from Add songs,
  `editor/new_song.rs`), songs shared by link at `#/s/<id>`
  (`crates/web/src/share.rs`: the share read from the private Worker,
  `api` beside the page, the private Worker's route on the app's origin
  (`DDI_API_URL` at build or `window.__DDI_API_URL` instead), held in memory
  for the session (several at once, by id: an online room's stages each
  have one), its songs `shared:<id>:<n>` read by URL through
  `songs::Origin::Shared`; `components/shared_page.rs` then song
  select in its shared mode, `select_page::Props::shared`,
  `SelectData::shared`: headed "SHARED SONGS", nothing added, managed
  or edited, or the page saying the link is gone; the uploader hidden
  behind `#/?share=1`, `Settings::sharing`: Options' Sharing section
  with the token, Share… in song select's More on imported songs and
  packs, the upload in `share.rs`; a shared song's movies streamed,
  `web/movie_source.rs` and the decoder worker's download of the whole
  file served to the decoder as it comes, small ranged requests for
  the header, the index and the probes; its download on the prompt's
  quiet line; the Worker itself in the private repository
  `../ddi-server`, routed at `sugoijan.dev/ditoditoinfinito/api/*` and
  proxied there by `just dev`; `docs/plans/shared-songs.md`). Compiles for `wasm32-unknown-unknown` only (empty when
  built natively, but for its routes' table, `crates/web/src/route.rs`:
  pure, so that its tests run natively, `cargo test -p ddi-web`).
- `crates/desktop/` the desktop shell (`ddi-desktop`, run by `app` natively;
  `docs/plans/desktop.md`): a winit window drawing the UI core with
  `render`'s painter (cosmic-text, system fonts); sound through cpal with
  our own mixer (the announcer on a bus of its own after the master,
  which the fail's and a work's fades leave alone), the clock from CoreAudio's host time and latencies
  (`audio/`, music decoded with `decode-all` on a thread); keys stamped
  with the OS's time where it gives one (`keyboard.rs`, macOS's
  `NSEvent`), controllers through gilrs on their own thread (`pads.rs`);
  the library read in place (`library/`: the song folders Options lists,
  StepMania roots, packs and zips (`.smzip` too), simfiles and
  Dancing☆Onigiri works, with a cache; announcers found in place,
  ours from the assets' `announcers/`, `announcers.rs`; a first start offers other games' song folders,
  `offer.rs`); plays through the shared session (`play/`: the UI core's
  start prompt, noteskins with their Lua read on a thread, backgrounds,
  a song's scripts read on a thread and their Lua run in slices
  (`play/scripts.rs`),
  movies (feature `video`, on by default: checked 8-bit H.264, HEVC and VP9 subsets through
  hardware VideoToolbox on macOS, experimental opt-in Media Foundation/D3D11
  on Windows (`mf.rs`, `mf_core.rs`; Windows Auto remains the software
  decoder), and for the rest and on failure the software decoder:
  FFmpeg in a sandboxed child process, `ddi-video-helper`
  (`video/helper`; `video.rs`, `video/helper.rs`, the arena in
  `video/arena.rs`; MP4/Matroska packets for the OS decoders demuxed by a
  helper of their own, `video/packets.rs`);
  movies in zips read in place or extracted once),
  a work's layers and lyrics, the theme's results, the history in the
  data folder (stale plays judged again on a thread, `history.rs`),
  replays, `play/replays.rs`; the announcer's sounds decoded on a
  thread, `play/voice.rs`, and the play's sound kept through its results
  for their line); a play's frames paced (`pacing/`: blocking in the
  acquire, or each started as late before its refresh as its
  cost allows (late with two frames in flight by default when a display
  link is available, block with one elsewhere; late falls back to block
  for a window state where it keeps missing with its margin at its cap,
  as modeled for fullscreen); pure, with a simulated display in `pacing/sim.rs`, built
  for tests only: feature `sim`, which the crate's dev-dependency on
  itself turns on; the refreshes from a display link on macOS,
  `display_link.rs`: the view's `CADisplayLink` from macOS 14, else a
  `CVDisplayLink`);
  songs shared by link (`share.rs`, `docs/plans/shared-songs.md`: a
  share read from the server (`DDI_API_URL`, else the production
  route) into the cache's `shares/<id>/<n>/song/`, each file at the path its
  entry names it by (a file above the song's folder, a pack's, beside
  it, kept by the share as `__up__/…`, `ddi_library::share::share_path`),
  the chart, music and pictures with the list and
  what the background changes play when the play loads, listed by song
  select's shared mode through `--shared <link or id>`; uploads through
  song select's Share… with the switch `--sharing on`, or `--share
  ID[,ID…]|pack:NAME [--expires W]` printing the link, the token from
  the settings or `DDI_SHARE_TOKEN`; HTTP through ureq on the calling
  thread, rustls on the system's trust store);
  calibration's menu and tests (`calibrate.rs`); the announcer studio
  (`studio.rs`, the web's page from Options or `--screen announcer`: its
  draft in the data folder's `studio/`, recordings from pickers, each
  line heard in its scene, a line said in play in the real play drawn
  into the frame's picture, `play/host.rs`; what Use in the game makes
  in the data folder's `announcers/`, `docs/plans/announcer-studio.md`,
  step 5); the chart editor
  (`editor.rs`: song select's E, saving into the song's folder, its
  playtests; new songs from a music file, `editor/new_song.rs`, made
  only once the music decodes, as on the web); the
  devices' fingerprints and notices (`devices.rs`). Settings in
  `storage.json` in the config folder; `DDI_HOME` moves the config, data
  and cache folders under it (`paths.rs`). One app a data folder
  (`instance.rs`; `docs/plans/desktop.md`, "One app a data folder"): a
  run that opens the window takes an OS file lock (the standard
  library's `File::try_lock`) on `instance.lock` in the config folder
  before it reads its settings and keeps it, the file open, until the
  process ends; started where another app has the folder it does not
  start: a line on stderr naming the folder and the other process, a
  system dialog for a player's own run (`shell-running-title`,
  `shell-running`; never with `DDI_HOME` set or without a display) and
  exit status 3. Only the lock decides, never the file's existence or
  what is written in it (the process and its start, for the message),
  and the file is never deleted; where no lock can be had the app starts
  as before and says so in a line. The runs that open no window
  (`--danoni-bundle-info`, `--danoni-importer-check`, `--mp-bots`,
  `--video-check`, `--share`, `--shares`, `--delete-share`:
  `instance::utility`, which a new one joins) take none, may run beside
  an open app, and read the settings without ever writing them
  (`store::install_reading`; a saved switch given beside one,
  `--sharing`, `--mp-rules-first`, `--mp-watch-link`, is not saved, and
  the run says so). A second app on one machine (a second player for a
  room) is `DDI_HOME=<another folder>`. Tested without a sound device
  on a fake clock and offscreen (`crates/desktop/tests/`; GPU tests skip without
  an adapter or with `DDI_NO_GPU=1`, as in CI's `desktop` job on Linux,
  macOS and Windows, with and without `video`; the room's screens'
  tests skip only their pictures there, `common::gpu::Pictures`, their
  frames laid out and measured undrawn, and a play runs undrawn there,
  `common::rig::Draws` over the play host's `Host::headless` and
  `Host::step_play`, TESTING ONLY: its prompt readies without a view and
  its frames are judged and not drawn, so a stage played through the
  host is checked but for what the view draws). The video decoder
  helper is found at `DDI_VIDEO_HELPER`, else as `video/ddi-video-helper`
  beside the executable, in a macOS app's `Contents/Helpers/` or beside a
  folder above it (`crates/desktop/build.rs`
  stages it in the build's `target/<profile>/video/`, which a test finds
  too, in `target/<profile>/build/<package>/<hash>/out/` since Cargo 1.100,
  `target/<profile>/deps/` before); `cargo test -p ddi-desktop --test
  video` checks it on generated clips; `--decoder both` compares the
  platform's strict OS subset against it, `DDI_VIDEO_DECODER=software`
  forces it for play (`module`, its old name, still works); Windows
  experiments use `mediafoundation-auto` for software recovery,
  `mediafoundation` for strict refusal; `video/check-windows.ps1 -Helper
  PATH` records the deferred-host fidelity run, without a window or
  sound). `crates/desktop/build.rs` builds the helper on every `video` build:
  FFmpeg natively first (`video/build-native.sh`, a few minutes once,
  cached in `target/video/native/`; nasm on x86_64 for its assembly), then
  the helper crate in `target/video/helper-target/` with the `helper`
  profile; `DDI_NO_VIDEO_HELPER=1` skips it (movies then show their song
  background), cross builds skip it, and Windows skips it unless
  `DDI_VIDEO_NATIVE=1` (MSYS2 bash with the MSVC tools, untested here).
  `target/debug/video/ddi-video-helper --sandbox-check` runs the sandbox
  self-test alone. On Linux it builds
  against ALSA and udev (`libasound2-dev`, `libudev-dev`; LGPL, linked
  from the system) and OpenSSL 3 (`libssl-dev`; Apache-2.0, linked from
  the system so the distribution patches it: the share client's TLS,
  `src/share.rs`; the package then needs `libssl.so.3`, which the
  package check allows, `tools/danoni-importer/build.py`); winit, rfd and
  arboard load their libraries at run time.
- SkinCraft is maintained in [sugoijan/skincraft](https://github.com/sugoijan/skincraft),
  a separate native application consuming our loader and field renderer as Git
  dependencies (`docs/plans/noteskin-authoring.md`). Its own repository contains
  the editor hosts, MCP bridge, local development patches and authoring tests.
- `crates/rating/` estimates teacher-v1 keyboard difficulty at 1× in one pass.
  `ddi_rating::estimate` takes an imported dance-single chart and returns its
  1–19 estimate (scalar qualification through 1,200 source notes and by
  duration and head density at 1,201–4,800, then only an exact 19 through 8,000),
  score curve and calibrated skill profiles (through 900). Song select’s matched-level
  radar carries per-spoke numeric, inactive and unavailable states. Full numeric
  qualification covers 1,200 mine-free source notes and 900 with mines. Longer
  scalar-qualified charts use the validated skill bands through 8,000. At
  1,201–3,200 notes Stream also checks chart patterns and the sensitivity of
  the shared numeric display in `ddi_rating::radar`. In that band Jumpstream
  requires a two-head row. It corrects the matched cost through 1 Accuracy pp
  and shows higher raw costs only at an exact scalar of 19 (`ddi_rating`’s `jumpstream` module). See the crate's
  README for the JSON command, model identity and qualification limits.
- `crates/msd/` MSD (`ddi-msd`, `docs/plans/msd-ratings.md`): Etterna's
  chart difficulty calculator, MinaCalc version 527 (etternagame/etterna
  `2d706147`, MIT, its notice in each translated file), translated file for
  file for four keys (`crates/msd/README.md`: what was left out and why);
  `ddi_msd::msd` the listing's eight numbers (rate 1.0, goal 93%), `ssr` a
  score's; a chart's rows as Etterna feeds them, `ddi_msd::chart::rows`, for
  one-player four-lane layouts, `four_keys`. Upstream's arithmetic, `libm`
  for its transcendentals and `fastsqrt` as its ARM build computes it, so
  native and wasm give the same bits. Its oracle, upstream's calculator
  built by itself, `tools/msd-oracle/build.sh` (the pinned commit fetched
  into `target/msd-oracle/`), compared by `crates/msd/tests/oracle.rs`
  (opt-in: `DDI_MSD_ORACLE`, `DDI_MSD_CORPUS`, `DDI_MSD_WRITE_FIXTURE`);
  the oracle's numbers in `tests/fixture.json`, checked natively and under
  WASI. Pure, natively tested.
- `crates/chat/` a room host's Twitch chat behind the play (`ddi-chat`,
  `docs/plans/multiplayer-twitch.md`, part C): Twitch's EventSub socket
  messages read (`eventsub.rs`; its fixtures in `tests/fixtures/`,
  written from the documentation, not recorded), the four providers'
  emotes and sets with each image's address and proxy path, an
  animated emote's animation (`emotes.rs`: the providers' answers read,
  the image picked and its proxy route named by `barbed-emotes`, the
  maintainer's own library, its model made ours at one seam,
  `emotes::from_barbed`, round BE), an image's container read
  before it is decoded, `anim-limits-v0`'s limits (the hostile and the
  huge refused, or their first frame still), the frames kept and which
  is drawn when (`anim.rs`, round TW4), a message made into an item
  after C11's filters
  (`item.rs`: every emote a piece of its own, 24 a message),
  `chat-overlay-v2`'s budgets (round DE: `chat-overlay-v1`'s of round
  TW5 tripled; no emote summarized, budgets in emotes), the moderation events and the two motions over the whole
  screen on the host's clock (`overlay.rs`: Across, Falling, reduced
  motion's fixed places; the looks shown to the maintainer, `LOOKS`,
  scattered by default), the emote images kept and their atlas cells,
  an animated emote's frames within their budget, `ANIMATED_UNITS`
  (`cache.rs`), the live connection to Twitch's EventSub socket as a
  state machine fed frames and time (`live.rs`, `live-chat-v0`, round
  TW6: the welcome subscribed through our server inside Twitch's ten
  seconds, the keep-alive's watchdog, Twitch's reconnect followed with
  the old socket kept until the new one welcomes, revocations, every
  close code, backoff and giving up quietly, our server's answer read,
  and the run's report, never a token or a person's chat: what was not
  understood kept as its shape, `redact`), the live chat's emote sets,
  the channel's and the globals (`channel.rs`), the scripted chat
  (`script.rs`, TESTING ONLY; its
  animated emotes drawn in code, `anim:`, and as GIF and WebP files made
  by code, `cargo run -p ddi-desktop --example chat_images -- <dir>`)
  and the global-emotes demo (`global.rs`, TESTING ONLY: the four
  providers' real global sets read by the real parsers, Twitch's
  through our server's `emotes/twitch-api/chat/emotes/global`; fixtures
  shaped as the providers' answers recorded 2026-11-20, names and ids
  invented; the recorded lists themselves stay local, read by
  `DDI_EMOTE_LISTS=<dir> cargo test -p ddi-chat --test emote_lists --
  --ignored`). Pure, natively tested.
- `crates/importer-protocol/` the platform-free framed app/helper IPC contract (serde
  and plain data only; no SDK/networking or filesystem).
- `crates/share-protocol/` songs shared by link's wire contract
  (`ddi-share-protocol`, serde only, as `crates/importer-protocol` is): ids,
  limits, expiry words, the share's JSON as made, answered and listed,
  path checks, file paths and routes, and an online room's upload grant
  (its claims, `Authorization: Grant`, its answer; one share a grant
  since 2026-11-15, by the claims' `grant_id`, which the shells pass on
  untouched in the signed grant: a second share is `grant_used`, a
  renewed grant keeps the id and goes on with its unfinished share).
  `ddi_library::share`
  re-exports it for the game; the private server (`../ddi-server`)
  depends on it by path and keeps no copy. Pure, natively tested.
- `crates/room/` the multiplayer room, match and stage model (`ddi-room`,
  `docs/plans/multiplayer-prototype-progress.md`): serde and JSON only, no
  chart/engine/library or I/O, so the private server can run it too;
  content is an opaque song key and chart digest. Membership and roles
  (`can`: capabilities from role, admin flag and phase, never from
  participation), stable ids, the match's locked rules beside the next
  match's draft, the competitor cap as one policy check
  (`RoomPolicy::check_competitors`: 73; `RoomPolicy::development`, a
  development-only practice's 129; the member bound 384,
  `DEFAULT_MAX_MEMBERS`, measured in `tests/scale.rs`), and stage awards
  as named rules (the eight-player table; for nine to 73 the same table
  stretched toward the top, `TopBiased`'s fixed tables, its bias the
  match rule `MatchRules::points`, Strong, Medium or Mild; under eight the
  table read up the field, `EightPlayerScaledV1`; `ExperimentalLinearV0`
  and `ExperimentalNineV0` retired, still read; development-only `DevelopmentLinearV0`; exact fractions,
  `Points`, shown as decimals); the battle a stage is
  played under, by name and version, and its ranking of judged runs
  (`battle.rs`: `BattlePreset::GaugeSurvivalV0`, provisional: each
  competitor's gauge as life, empty is out of the stage; `rank`,
  `decided`: those still in by life, then the score, DDI's sum,
  `Run::Live`'s `score`, a CPU's or a bot's its own (it plays the whole
  chart; a report without one, taken as given as any self-reported
  value is, ranks as 0), then the failed by score (`Run::Failed`: under
  the rule When life runs out, `MatchRules::life_out`, Play on, a new
  room's (revision 15 under Charts: Adaptive alone, revision 16 under
  both: As picked at zero life, Adaptive on the easiest chart), life out
  is failed, played on to the song's end and out for the battle; a live
  stage confirmed only once their streams ended; a forfeit's detail kept
  graded QUIT, one out or failed F, `RunDetail::graded_as`);
  `GaugeAttacksV0`, the same with attacks, chosen by the match's Attacks
  rule, `MatchRules::attacks`, On by default where attacks are offered
  (`MatchRules::practice`; a hosted room's, `MatchRules::hosted_for`,
  where it plays live, else none; a room that offers none plays none
  whatever its rules say,
  `ddi_ui`'s `RoomClient::attacks_played`); a member's own attack policy, `Member::targeting`
  (Spread until they choose; `Command::SetTargeting`, taken outside a
  stage's play, nobody's readiness cleared; a CPU's its
  `OpponentSpec::targeting`, which a roster entry may name; additive,
  left out of the JSON at its default), and a change made during a play
  carried by the ledger's reports (`attacks::Report::targeting`, applied
  from that checkpoint's launches, `AttackEvent::Retargeted`);
  `RunDetail`, how a starter performed for the stage's
  standings: optional on `Command::ReportOutcome` and
  `Event::OutcomeReported`, kept in `StagePlay::details`, self-reported,
  never ranked by, ADDITIVE to protocol 2 with serde defaults and no new
  version, so a peer that predates it ignores it); the match's ruleset
  everyone plays, DDI alone (`MatchRules::RULESETS`; the field
  `MatchRules::ruleset` keeps an older id as it was stored or received,
  `LEGACY_RULESETS`, so a stored room's rules read as they were written:
  read it through `MatchRules::ruleset_id`), with DDI's
  difficulty and Boos as match rules (`MatchRules::difficulty`, a
  `MatchDifficulty`, and `boos`: plain values, additive, left out of the
  JSON at their defaults; `docs/plans/ddi-ruleset.md`, "Round 3
  (built)"; decision D0 of `docs/plans/multiplayer-ui.md`'s 2026-11-11
  note); attack power's
  ledger and its attacks (`attacks.rs`: `StageBattle` credited at
  checkpoints every four beats from reports of judged tiers, life and
  score (`Report::score`, DDI's sum just before the checkpoint, from the
  play's record, `Player::score_at`; since revision 20),
  `ATTACKS`, which is `attacks-v4` (2026-11-20, round DC; `attacks-v3`'s
  numbers of 2026-11-13): cap and cost 100,
  `POWER_CLASSES_V0` by the match's ruleset; a full bar launched in a
  turning order at the target its `Targeting` picks (Spread, the
  fewest attacks still to come first; Retaliate; Leader, which people
  read as "Top score" since round DB: whoever reported the highest
  score at the checkpoint, ties by a pick drawn from the stage's seed,
  the checkpoint, the sender and the candidate, the same on every
  device and the room), the match's
  Shocks per attack (`MatchRules::shocks`, `ShocksPerAttack`: 2, 4, 6
  or 8, four for a new room, a match stored before the rule eight and
  its next match's draft four; per stage as Attacks at once; the
  plan's `shocks`; the cap and cost unchanged, `SHOCK_COST`,
  `shock-cost-v0`) in the first safe `Schedule` section past the target's lead
  (`LEAD_V0`) and at or after the moment the last attack placed against
  them clears, looked for to the song's end (none left: `Blocked`,
  `NoSafeSection`, the power kept); an attack over for the ledger once
  placed, so attacks against one recipient chain, never in the same
  region, but only in a section starting within 15 s of the launch's
  checkpoint (`AttackRules::window_us`, `ATTACK_WINDOW_V0_US`) and with
  at most the match's Attacks at once standing against them
  (`MatchRules::at_once`, `AttacksAtOnce`: 1, 2, 3 by default, No
  limit; `AttackRules::with_at_once`, `chain`; in the battle plan,
  `BattlePlan::at_once`, `BattlePlan::rules`); a recipient outside
  either is no valid target and the sender's policy goes on to its next
  choice (Leader the next in standing, Retaliate Spread's order), none
  valid `Blocked` (`NoRoomInWindow` where someone has room only later);
  named places for a limit, off (`chain_gap`, `cooldown_checkpoints`);
  a stage run under the rules its plan names (`attack_rules`), a plan
  naming any other refused as unknown (`attacks-v0` to `attacks-v2`
  retired 2026-11-14, `attacks-v3`, which ranked Top score by life,
  with revision 20; no recovery: an attack is over the checkpoint it
  clears, `AttackState::Cleared`, a stored `recovered` read so);
  expiry when the recipient goes out, every one placed; `SETTLEMENT_V1`
  (`settlement-v0` retired),
  decided only with no attack against the last one in outstanding, the
  one left before that last standing (`Settlement::last_standing`), a
  knockout by one of them a shared first; trace tests in
  `attacks/tests.rs`).
  A running match's per-stage rules (`rules::PlayRules`: Timing
  difficulty, Boos, Attacks, Charts, and Points, `PlayRules::points`,
  `None` for the match's own, `Match::play_rules_of`, each stage awarded
  by its own rules' bias; never the stages, seats, CPUs or ruleset) changed
  from a later stage on
  (`Command::ChangeStageRules { from, rules }`, the operator's who may
  edit rules, against `Match::changes_saved_at`; `Event::StageRulesChanged`;
  `Match::changes`, `rules_for(stage)`, `stage_rules_changed`): a change
  lands on `RoomState::rules_land_on`, the next stage nobody has readied,
  loaded or started (a pick opens its window, so a change saved once a
  stage is picked lands on the one after), and one made for a stage that
  has begun since is refused with `Rejection::RulesLandLater`; the
  battle plan names a changed stage's rules (`BattlePlan::stage_rules`,
  in the digest only then), the public view a changed stage's
  (`PublicStage::rules`).
  M7b's match flow in the model: `Command::Pick` (the operator's Start on
  a song: the next stage's song, the match started or gone on, its
  readiness window opened, all as one change), `Command::Spotlight` and
  `Event::SpotlightChanged` (the song the picker's song select rests on,
  `RoomState::spotlight`, with a `ContentCard` to show before it is
  loaded), `Command::SetSpotlightShare` (the authority's: its share, by
  chart, which a pick of that chart reuses, `RoomState::known_share`);
  one picker at a time (2026-11-12, `state::Picking` in
  `RoomState::picking`: who chooses the room's songs now, its owner at
  first; `picker()`, `chooses`, `may_take_pick`): `Spotlight` and `Pick`
  are the picker's alone (another operator refused
  with `Rejection::NotPicker`, naming who has it), an operator takes the
  pick at once in any phase (`Command::TakePick`, `Capability::TakePick`,
  `Event::PickerChanged`, which clears the spotlight and keeps its share
  known by chart), and it goes back to the owner, or to nobody with the
  owner gone, when its holder leaves, loses the role or is set
  `Disconnected` (`release_pick`); a room stored without `picking` reads
  as nobody's pick, its owner given it with the next change; the picker
  is "who chooses
  now", any participant in principle, only the taking an operator's;
  `hosted-deadline-v1` (a window counts from the song's availability and
  ends early once every connected player who could join is ready and
  loaded); `hosted-deadline-v2`, the default since follow-up round F (a
  loading grace of 45 s first, waiting for everyone connected to load,
  `ReadinessPolicy::loading_grace_ms`, `ReadinessWindow::loading_until`,
  then the window, ending early once everyone who loaded is ready;
  practice keeps v1's); `CpuMode::Roster` (named CPUs, each its own `OpponentSpec`);
  the host's succession (2026-11-12, `Hosting` in `RoomState::hosting`,
  the room's setting "If the host leaves", `HostLeaves`: Pass it on or
  Close, the owner's `Command::SetHostLeaves`): when the owner leaves,
  or is away past the authority's patience (`Connection::Disconnected`,
  the server's reconnect grace), the room passes by
  `RoomState::succession` (`succession-v1`: a connected co-host, else a
  co-host inside the grace, waited for, else the connected member who
  joined first; never a bot nor an observing administrator,
  `RoomState::observes`; the one before a member, only `MakeHost`
  leaving them a co-host) or ends (`Event::RoomClosed`,
  `RoomState::closed`, `Rejection::RoomClosed`: its authority closes
  every socket with 4410; the authority's own `Command::CloseRoom`
  ends one too, `Closure::ByAdministrator`, the server's operator's
  "Close this room": the screens say "The room was closed."); an owner
  away with nobody connected keeps
  the room; `Command::MakeHost` hands it over; a pick nobody holds is
  the owner's while the owner is there (`settle_host`, run after every
  command; a room stored without `hosting` reads as Pass it on);
  protocol 2, still taking 1; the room model's revision
  (`ROOM_MODEL_REVISION`, 24 since 2026-11-24: what the code review of
  2026-11-22 left for a revision, round R24: a battle plan's notes a
  checkpoint, `BattlePlan::notes`, in its digest, an offered plan
  without them refused, `PlanProblem::Notes`, bounding the judgments a
  report is credited, `StageBattle::with_notes` (`notes-v0`: the
  stage's chart's, under Adaptive every rung's together, up to
  `NOTES_AHEAD_US` past each checkpoint); the rows of people long gone
  forgotten, `Command::Forget`, `Event::MembersForgotten`,
  `RoomState::forgettable`, `to_forget` (`forget-v0`: plain members who
  left whom nothing kept names, `RoomState::referenced`, beyond
  `RoomPolicy::left_rows_kept`, 384, the earliest first; a newcomer's
  join forgets too; `member_rows` a far backstop); a kick remembered,
  `Member::kicked` (back as a newcomer comes in while admission is
  Closed, `AdmissionClosed`; the mark gone once let back in); a stage's
  plan offered by anyone present once the room waited for its picker,
  `live::plan::PLAN_OFFERS_V1` (`plan-offers-v1`, 8 s after its window
  opened, `RoomState::window_opened`, `plan_offers_open`;
  `LiveRefusal::OfferTooSoon`, offered again 2 s later); the pick
  released in one place, `settle_host`; `tests/revision_24.rs`,
  `tests/room-revision-23.json` and `tests/live-durable-revision-23.json`
  read with nobody kicked and plans unbounded; 23 since 2026-11-22: under Charts: Adaptive
  each player starts on their own chart of the song, round OC
  (`Command::SetStart`, `Event::StartChosen`, `Prep::start`,
  `RoomState::start_of`, `charts_for`, `Rejection::ChartsAsPicked`; a
  listed song's chart in the stage chart's style, taken while the stage
  gathers readiness and from a late entrant, locked from its start; the
  pick's or the drawn choice's chart the stage's own, the start of one
  who chose none; `RunDetail::start`, `PublicPlay::starts`, the live
  side's `StepStatus::after_from`; `tests/own_start.rs`,
  `tests/room-revision-22.json` read with no starts); 22 since
  2026-11-21: choosing by the group,
  `crate::choice` (`docs/plans/multiplayer-room-songs.md`, "Stage 4 as
  built"): the per-stage match rules Song choice, Host or Group
  (`MatchRules::choice`, `PlayRules::choice`, `SongChoice`; Group only
  with a song list, `RulesProblem::GroupNeedsList`), and Repeats under
  it (`Repeats`: Allowed, Not in a row, Not in a match, giving way where
  nothing would be left, `RoomState::excluded_songs`); each player's
  choice of a listed song and chart (`RoomState::choices`,
  `Command::Choose`, `Event::ChoiceChanged`; who may,
  `RoomState::choice_refusal`, `chooser_refusal`: those who will play
  the next stage, `choosing_for`), a host's or co-host's own standing
  choice made while the room could pick opening the draw's countdown
  where a pick could start (one made while a stage was readied or
  played waits, `Choice::waits`, until its maker makes it again once
  the room can pick, `choice_waits_for`; round FX, no new revision)
  (`Capability::StartDraw`, `draw_starter`, `Event::DrawOpened`,
  `RoomState::draw`; `draw-v0`, `DRAW_V0`: ten seconds, early once
  everyone who may has chosen, never before three, `draw_ends_at` in
  `next_deadline`), canceled when its choices all go
  (`Event::DrawCanceled`; the match stopped or ended taking the choices
  with it), a choice the room took away saying why to its maker
  (`ChoiceGone`), and the authority's draw at its end
  (`draw_due`, `Command::Draw { roll }`, the roll the server's random
  bytes, `LocalAuthority::set_draw_seed`, `FakeServer::set_draw_seed`
  for tests; `Event::Drawn`, `RoomState::drawn`, `DrawRecord`: the
  entries by id, the winner `roll % n`), then a pick of the drawn song
  as `PickListed`'s, its Ready countdown from the reveal's end
  (`ready_from`), the choices cleared; a picker's pick refused under
  Group (`Rejection::GroupChooses`); a chosen song in use
  (`song_in_use`); the audience's public view the choices, the countdown
  and the draw; round RL4; `tests/room-revision-21.json` reads as Host;
  21 since 2026-11-21: Who can add songs, the
  owner's room setting, Hosts or Everyone (`songs::WhoAdds`,
  `RoomSongs::who_adds`, `Command::SetWhoAdds`, `Event::WhoAddsChanged`),
  read by `RoomState::may_add`, the one check of who adds (every person
  in the room under Everyone, never one held to watching by the
  watch-only link), and `RoomState::add_refusal`, the reducer's decision
  a client asks before it sends; under Everyone a person adds up to half
  the list and removes their own; back to Hosts nothing listed goes;
  round RL3; `tests/room-revision-20.json` reads as Hosts; 20 since
  2026-11-20: Top score attacks the top score, `attacks-v4`, each checkpoint report carrying its score,
  binary `BATCH_V3` and `CPUS_V2`; round DC; `tests/room-revision-19.json`;
  19 since 2026-11-20: a bundled song added to
  the room's list uploaded as any other, the room's share the one
  source, an entry a room stored before listed by its content alone
  still read and played (`RoomSong::bundled`), and hosts and co-hosts
  exempt from a person's half of the list (`RoomState::song_quota`);
  round DB; `tests/room-revision-18.json`; 18 since 2026-11-20: the host's Twitch in a
  room, `crate::twitch` (`RoomState::twitch`, `TwitchSetting`: whose
  bans or bans and blocks keep people out, `TwitchMirror`, and the chat's
  ceiling, `TwitchChat`, the one who set it's while they are a member,
  `RoomState::may_set_twitch`; `Command::SetTwitch`, the authority's,
  `Event::TwitchChanged`, `Capability::Twitch`) and removal
  (`Command::Remove`: a host's or a co-host's Kick, `Capability::Kick`,
  `RoomState::may_kick`, `Event::MemberRemoved`, sockets closed 4403,
  `LinkState::Kicked`; or the authority's for the mirror, told to the
  others as a leave, 4406, `LinkState::NotAdmitted`,
  `Rejection::NotAdmitted`, `403 not_admitted`; `FakeServer::bar`,
  `sweep`, `set_twitch`; the Players pop-up's Moderate,
  `room_dialogs::MODERATE`; `docs/plans/multiplayer-twitch.md`;
  `tests/twitch.rs`, `tests/room-revision-17.json` read with none);
  17 since 2026-11-19: an online room's own song
  list, `crate::songs` (`RoomState::songs`, behind a shared pointer and
  stored by the server in a row of its own; `Command::AddSong`,
  `RemoveSong`, `SetSongList`, `SetSongContent`, `PickListed`,
  `Capability::AddSongs` through `RoomState::may_add`, `StageContent::song`;
  entries named by `ddi_share_protocol::song_key`, at most 100, a person
  half the list's size (hosts and co-hosts exempt since 19), the oldest not in use removed when full unless the
  host refuses new songs; `docs/plans/multiplayer-room-songs.md`; and
  its stage 2, in 17 too, nothing deployed between: who has which listed
  songs, `live/have.rs`, `ClientLive::Have` and `ServerLive::Have`, kept
  by the server beside each socket and never in the room's state);
  `tests/room-revision-16.json` reads with an empty list; 16 since
  2026-11-19: When life runs out under
  both chart rules, `MatchRules::life_out`, `life_out_any_chart`,
  `normalized`, a rule set stored before read as it was, and a
  retrying download, `Loading::Retrying`, holding the window's early end
  until its loading grace would have ended, `ReadinessWindow::hold_until`;
  `tests/room-revision-15.json`; 15 since 2026-11-19: joining during a match
  On for a new room, `MatchRules::mid_match_entry` (seated for the next
  stage not begun, up to the competitor cap, `RoomState::late_places`),
  open-ended matches, `MatchRules::open_ended` (`Stage::last`,
  `Command::SetLastStage`, `Command::EndMatch`), On the easiest chart's
  Play on (`MatchRules::easiest`, `Run::Failed`, `MomentKind::Failed`),
  a 4401 asking for a ticket first; `tests/room-revision-14.json`, read
  as it was: Stages a number, joining Off, Out at the easiest chart; 14
  since 2026-11-17: the CPUs' levels,
  `cpu-levels-v2`, which every device computes (a stored roster's level
  numbers mean the new levels; `tests/room-revision-13.json`); 13 since
  2026-11-17: DDI's points, percent and
  letters, `ddi-percent-v1`, the percent a starter reports
  (`RunDetail::percent`) the new one; 12 since 2026-11-16: Shocks per attack, a
  returning member's administrator flag, `Event::AdminChanged`, a member
  reconnecting for a moment keeping their place, the member's
  keep-alive, DDI's combo part (`ddi-combo-v0`) and `adaptive-v5`
  (in 12, deployed nowhere before them); and round P1b's: a late entry, `Event::EnteredLate`, a
  socket closed 4429 an end, `LinkState::OpenedElsewhere`, the quiet
  pings, `LivePolicy::quiet_ping_ms`, practice's Cancel ready stopping
  its countdown, `ReadinessPolicy::unready_cancels_countdown`; 11 `adaptive-v4`, a live snapshot's `step`,
  `Command::CloseRoom`; 10 `adaptive-v3`; 9 since
  2026-11-14; round B1, 2 since
  `attacks-v2`, 3 adaptive charts, 4 `attacks-v3`, 5 `cpu-levels-v1`,
  6 `adaptive-v1`), checked both ways: a client says its own in the
  join's, ticket's and audience join's `model`, an authority refusing
  an older client with `update_required` (`check_model`; a ticket not
  during a stage, `RoomState::admits_model`) and the shells saying so
  (the web with a Reload, `UiRequest::Reload`); the authority says its
  own in those answers, and a client refuses an older authority, or one
  that says none (`check_authority`, `SERVER_BEHIND`: "the server is
  being updated", no socket opened, a member's link waiting as
  `LinkState::ServerBehind`). Raise it whenever peers must agree on
  something new; server and game deploy together, the server first;
  build no per-feature soft failures for older peers. A room stored by
  revision 8 loads (`tests/room-revision-8.json`). The reducer (`reduce.rs`): commands from an
  `Actor` (a member, or the authority for joins, connections, content status
  and confirmed outcomes) decided into events, atomic, refusals typed
  (`Rejection`); `RoomState::commit` applies a revision's events, so a
  mirror holds the same state; `advance(now)` runs the readiness deadline
  and the countdown. The phase machine is multiplayer-ui.md's State model;
  participation is derived (`RoomState::participation`), never stored.
  The match's Charts rule (`MatchRules::charts`, `Charts`: As picked or
  Adaptive, a new room's default where it plays it since 2026-11-14,
  `MatchRules::practice`, `hosted_for`, the field still read as As picked
  where missing; read from the "At zero life" rule before it, `at_zero`;
  under Adaptive the pick or the draw names the song and each competitor
  starts on their own chart of it (revision 23, round OC:
  `RoomState::start_of`; this device's default the chart song select
  would give the player, `state/room/starts.rs`, `RoomClient::start_charts`,
  `start_shown`, `choose_start`, `remember_start` for the room's song
  list's song side, told the room as soon as the song is known here,
  `sync_start`; the stage's Ready card's chart row, `prompt_room.rs`'s
  `CHART`; a bot's `--start own|easiest|hardest`);
  a stage under Adaptive ranks those still in by
  the score as earned, life the tie-break, `RankBy::Score`,
  `Charts::rank_by`, `battle::rank`'s argument, never by steps,
  `Run::Live::steps` kept for the record; a stream's
  `live::MomentKind::Step`, a plan's `ladder` naming every rung).
  Conflicting edits carry the base revision they were made against.
  The wire (`protocol.rs`): a versioned envelope (`v`, client id, `seq`,
  `base`; the authority answers ack/reject/events/snapshot with the room's
  revision), id-keyed maps as `[key, value]` pairs, and `Mirror`, a
  client's copy that detects a missed revision. `authority.rs`: the
  `RoomAuthority` trait the shells hold, and `LocalAuthority`, CPU
  practice's authority on an injected `Clock` (idempotent retries by
  `seq`, a snapshot after a stale refusal). `online.rs`: the
  `OnlineAuthority`, the private server's room (`../ddi-server`'s
  `RoomDO`, this reducer) over a socket each shell supplies
  (`Transport`): the JSON codec, a mirror whose gaps send `Resync`
  (`{"v":1,"want":"snapshot"}`), commands kept until answered and sent
  again with their `(client, seq)` after a reconnect, duplicates and old
  revisions passed over, `LinkState` (joining, connected, reconnecting on
  a fresh ticket by `ReconnectPolicy::RECONNECT_BACKOFF_V0`, tried at
  once by `retry_now`, and the ends:
  4401 a ticket asked once at once (`SIGN_IN_RECHECK_MS`: granted, a
  reconnect; refused 401 or a second 4401 within a minute, sign in
  again; a link refused before it ever reached the room says so,
  `LinkState::SignInToEnter`, `NotAMember`), 4410 closed, 4000 left, 4429 the room opened on
  another device of the person's, a fifth socket, whose "Use this
  device" joins again, `OnlineAuthority::restart`); a socket's keep-alive
  (`KEEPALIVE_V0`, PROVISIONAL: "ping" after 20 s of silence, answered
  by the server's runtime without waking the room, the socket replaced
  when 20 s more of the client's own running time bring nothing; not
  sent where the transport keeps its own, `Transport::keeps_alive`,
  desktop's); tickets sans-IO
  (`wants_ticket`, `ticket`). `fake.rs`: `FakeServer`, that room in
  process behind JSON text both ways with faults (delay, duplicate, drop,
  reorder, stale, close codes, a socket gone silent, `Fault::Silent`),
  the keep-alive answered without the room (`auto_answered`), the server's reconnect grace where a
  test sets one (`set_grace`), its 4410 for a room that ended,
  `rewrite`, the room put as another authority would have it, and
  its three links answered as the server's bodies (`fake/invites.rs`:
  `invite_preview`, `kind: play|watch|audience`, `audience_count`, the
  audience offered at the member bound; `audience_join_answer`; a room's
  clients' requests as the server answers them, `answer_http`, which the
  fake rooms' HTTP of the web and the tests use). Tests:
  `crates/room/tests/walkthroughs.rs`, the handoff's walkthroughs;
  `tests/authority.rs`, the envelope and the authority; `tests/online.rs`,
  the adapter contract (one script through `LocalAuthority` and through
  `OnlineAuthority` + `FakeServer`, the same rooms) and the network's
  faults. Live play (`live.rs`, `docs/plans/multiplayer-online-play.md`):
  where `RoomPolicy::live` says so, the room's clock (`live/clock.rs`:
  pings, the estimator), a stage's song zero on it
  (`StartCommitted::song_zero_at`), the battle plan that comes with the
  pick (`live/plan.rs`: checkpoints and safe sections, `seed_for`, its
  digest on the content, `SetBattlePlan`), each competitor's moments and
  checkpoint reports (`attacks::Reporter`) in numbered batches gathered
  into one frame a tick (`live/stream.rs`; binary, `live/wire.rs`,
  `BATCH_V3` (a report's score since revision 20) and `FRAME_V2`, a
  step down a moment kind of its own; a
  stream's end says how, `StreamEnd`: out by life at a time, or left,
  `forfeit`, which a field marks "Left the stage" and the ladder ranks
  last, `ddi_session::battle::feed::MomentKind::Left`,
  `OpponentView::left`), the
  room's side sans-IO (`live/server.rs`, `LiveServer`: the ledger at
  deadlines, the decision as practice makes it, resume and snapshots,
  a snapshot's `CompetitorStatus::step` saying the chart of the ladder
  each competitor is on, `StepStatus`, so a field followed from part-way
  draws it at once, `RemoteSeat::step_status`;
  `LiveDurable` for the server's storage) and the client's in
  `OnlineAuthority` (`live/client.rs`, `LiveLink`); `FakeServer` runs it
  with latency and faults both ways (`Latency`, `OffsetClock`,
  `fault_uplink`); `tests/live.rs`. All live numbers in `LIVE_V1` and
  `LIVE_V2` (a frame and a batch every 100 ms: a room's
  `LivePolicy::timing`, additive; a room made before keeps `live-v1`'s);
  how far behind the song a device draws the others' fields, measured
  from its frames and a second at most (`live/delay.rs`: `ViewDelay`,
  `VIEW_DELAY_V0`; `ViewMeter` for the tests and the server's load
  driver; `view_delay_study`). CPUs in an online room
  (`live/cpus.rs`, `online-cpus-v0`, PROVISIONAL; where
  `RoomPolicy::seats_cpus` says so: a room that plays live): no connection plays them,
  every device computes them alike and sends their checkpoint reports
  and ends (`CpuNews`, binary `CPUS_V2`; `ClientLive::CpuRun`, the
  authority's `Command::ReportFor`), who sends `sends_for_cpus`
  (`cpu-reporters-v0`), the room taking the first report of each CPU
  and checkpoint and counting a different one
  (`LiveStage::cpu_mismatches`).
  The audience (`audience.rs`, `docs/plans/multiplayer-audience.md`,
  `audience-v0`, PROVISIONAL): people who watch beyond the members,
  counted and not listed, served by relays, admitted without a sign-in
  (since 2026-11-15: an anonymous seat for the server's daily mark of
  the connecting network, `is_anonymous`, eight a mark and its joins
  bounded a minute, `AudienceRules::seats_for`; a signed-in person's two
  their own); where `RoomPolicy::audience`
  says so, the public view derived from the state (`audience/public.rs`,
  `PublicView`: nothing a member alone knows), the members' frames
  coalesced four a second into one audience frame (`audience/frames.rs`),
  the room's side (`audience/hub.rs`, `AudienceHub`), a relay's
  (`audience/relay.rs`, `Relay`: admission, seats, fold, resume,
  `restore`) and a device's (`audience/client.rs`, `AudienceLink`,
  `Audience`), sans-IO; the count to members (`ServerLive::Audience`)
  and the owner's closing (`ClientLive::Audience`); `FakeServer` runs it
  (`fake/audience.rs`: `audience_join`, `FakeAudienceSocket`,
  `restart_relay`); `tests/audience.rs`.
  Pure (serde and JSON), natively tested.
- The multiplayer prototype's CPU practice (M2 and M3,
  `docs/plans/multiplayer-prototype-progress.md`), behind no switch since
  2026-11-14 (`Settings::multiplayer` gates nothing and is only read;
  the web's `mp=1`/`mp=0` and desktop's `--multiplayer on|off` are taken
  and do nothing): the start menu's Multiplayer opens the
  multiplayer home (`ddi_ui::screens::online_home::Home`: the start
  menu's column and rain under
  the bars; Find match and Browse rooms grayed "Coming soon", Private
  room, Join by link (a page with the invite field) and, last, CPU
  practice, the first key focusing Private room, `Screen::first_focus`;
  the account at the footer's right, Sign in with Twitch or the name
  with Sign out; `SignInDialog` asks for a sign-in where one is needed
  and follows it through). `crates/ui/src/state/room.rs` is the room's
  client: `RoomClient` holds `ddi_room`'s `LocalAuthority` and a `Mirror`
  kept from its envelopes, sends every change as a command (no room logic
  of its own), knows which library song and chart a stage's opaque content
  names (`Picked`; a shared song as a shell read it, `SharedFound`, made
  into one by the client, `finish_shared_found`), refuses charts the CPU cannot play (`eligible`: one
  player, four lanes), picks (`pick`: M7b's flow, `Command::Pick`),
  knows whether this player chooses the room's songs (`chooses`: the
  room's picker; `may_take_pick` and `take_pick` for an operator who
  follows), tells the room the picker's resting song after
  `SPOTLIGHT_REST_MS` (`hover`), hands the shell each chosen song to prepare (`LoadJob`,
  `RoomCall::Prepare`: the chart read and `check_chart`, the CPUs' plan
  built; `finish_load` reports it loaded or a `LoadProblem`), and the
  stage whose readiness gathers with this player in it, once, for its
  prompt (`StageOrder`, `RoomCall::PlayStage`; `view`, `stage_started`,
  `leave_prompt`), whose judged outcome the shell reports as the
  authority (`report_stage`); `SharedRoom` is what the shells keep across
  screens; a practice room as the shells make it, `practice_room`
  (the development switch's CPUs beyond the cap, `Development::cpus`).
  `state/cpu_roster.rs` (with `cpu_roster.json`): the named CPUs
  practice seats (invented names, data in every language; each at one of
  the ten levels, every level somebody's, shown as stars and never as a
  number), Add random's `RosterAim` hook; the roster a practice room was
  left with is the next one's (`Settings::practice_roster`, `cpu_roster::kept`,
  `RoomClient::practice`). A stage's prompt says so (`PromptInfo::set_stage`),
  its results go back to the room (`ResultsKind::Stage`: no Retry) and
  have two tabs where the graphs are (feedback round B,
  `ResultsScreen::with_room`, `screens/results_stage.rs`): Standings,
  shown first (every starter in the stage's order with the award, the
  match's total and place and how they performed: score or percent,
  grade, life or when out, combo, the judgments as one stacked bar in
  the ruleset's tiers' colors, `screens::play::tiers`; with attacks,
  three small columns in the combo's place, Sent, Taken and Shocks
  (`RunDetail::attacks`, `received`, `shocks`), and under the lines the
  player's own battle in short, `results_stage::summary_lines` from
  `RoomClient::set_stage_summary`; lines filling in
  as an online stage's reports come; the match's result on top after
  the last stage), and Graphs; Left and Right or the shoulders change
  the tab, remembered for the session; what it reads is
  `RoomClient::stage_table` (`StageTable`), a practice's competitors'
  details kept by the client (`set_stage_details`, from
  `ddi_session::battle::StageOutcome::details`, `run_detail`), an online
  stage's the room's (`report_played`); `ui.theme.stage` for tests. The
  room's song select pops the standings and the match's results up
  (`StandingsDialog`) for those who did not see them there; stage plays
  are never kept in the history.
  Watching (`watch-v0`, 2026-11-13): a member chooses Play or Watch for
  themselves (`ddi_room::Intent`; `RoomClient::set_intent`, `watching`,
  `may_choose_intent`; More's item and the Players pop-up's `Intent`
  button, `screens::room::toggle_intent`); a watcher is no competitor
  and a watching host still hosts; where every person watches, practice
  plays its CPUs alone (`ddi_room`'s `humans_needed`) and a hosted room
  is refused (`NoPlayingHumans`). In CPU practice a stage the player is
  not in is watched on this device (`RoomClient::watches`,
  `StageOrder::watching`): its prompt says Watching and readies
  nothing, its results name the stage's winner and have no Graphs tab
  (`ResultsScreen::watched`). CPU practice is never asked about before
  it is left (`RoomClient::leave_cost` none), and one left with a match
  running is forgotten (`screens::room::leave_now`; the web's
  `crate::room::forget_left_practice`); a stage's results ask where
  leaving the room costs (`ResultsScreen` guards its leaving), saying
  first that the stage was left where a close request left its play
  (`screens::room::ask_leave_after`; desktop's
  `Shell::close_requested`).
  Online rooms (M5c, M5d): `RoomClient::online` holds an online link
  instead (`crates/ui/src/state/online/link.rs`: `ddi_room`'s
  `OnlineAuthority` over the shell's socket, its tickets, the room's name
  and invite, a stage's upload grant); its roles come from the shared
  `ddi_room::can` (`RoomClient::may`). Since revision 17 a host adds
  songs of their library to the room's list, and since revision 21 anyone
  in the room where the owner's Who can add songs is Everyone
  (`RoomClient::may_add`, `who_adds`, `set_who_adds`, Host options' row;
  `add_blocked` and `my_song_count`, what My songs' status line says of
  a person's half and a full list before the press) (`RoomClient::add_song`, the
  library known to the client by each song's content,
  `RoomClient::set_library`, `room::song_key`), each uploaded when added
  under its entry's grant (`POST /rooms/<id>/songs/grant`,
  `UploadJob::entry`, `song_upload_progress`/`song_upload_finished`,
  `ShellNews::SongProgress`/`SongUploaded`), and the picker picks from the
  list (`pick_listed`), there at once; whoever has the song loads it from
  their library, the others from the entry's share. The picker's resting
  song is a mark on its entry (`hover_listed`, `spotlight_entry`): nothing
  is shared ahead or read for it any more. Since revision 22, under the
  match rule Song choice: Group, Enter on Room songs is this player's
  choice instead (`state/room/choosing.rs`: `RoomClient::choose`, again
  taking it back (a host's waiting choice made again instead, starting
  the draw: `choice_waits`, `starts_draw_now`), refused at the press as
  the room would,
  `choice_blocked`, `repeat_refused`, `chose_count`, `draw_countdown`,
  `draw_waits`), a host's or co-host's starting the draw
  (`starts_draw`, `someone_starts_draw`); the draw's reveal,
  `RoomClient::reveal` (`DRAW_V0.reveal_ms` from when this device saw
  it, the stage's prompt held back meanwhile), drawn over the room's
  song select by `screens/room_reveal.rs` (`RevealDialog`, layer
  `room-draw`: a reel of the entries slowing to the one drawn,
  `reel_at`, made from the record alone, `Reel`; still under reduced
  motion), and over the audience's page from the public view
  (`AudienceClient::reveal`, `open_audience_reveal`), the rows saying
  who chose them, Enter's hint on a song Repeats excludes saying why, the
  status line the choosing (`room::group_status`: a watching host told
  what starts the draw; a choice the room took away,
  `LinkNotice::ChoiceGone`), the players panel a tick (`Mark::Chose`),
  Rules' Song choice and Repeats rows.
  Stage 2 (still revision 17):
  each member's device downloads the listed songs it does not have ahead
  of the picks into a store of the room's own (`state/room/preload.rs`,
  `room-preload-v1`: listings and banners first, a stage's song, the
  picker's, the newest; never a movie; none from a stage's countdown to
  its end; `PreloadJob`, `RoomCall::Preload`, `RoomClient::store`,
  `ahead_song`, `have_count`; Options → Data's Room songs ahead,
  `Settings::room_ahead`, a phone's or a data-saving browser's default
  Likely next, the newest up to eight songs and 128 MB besides those), the web's in the library's IndexedDB under `room/<store>/`
  (`crates/web/src/room_store.rs`), desktop's in the cache folder's
  `rooms/<store>/` (`share::load_with`), each emptied when the room is
  left. A pick that names no entry
  (practice's, and revision 16's) is pending: the client asks the room for the
  stage's grant and hands the shell an `UploadJob` (`RoomCall::Upload`:
  the song uploaded as the room's share with `Authorization: Grant`,
  `upload_progress`/`upload_finished`, an expired grant renewed, a
  failure said with More's Retry sharing); once the room says the stage
  is available with its share, every member prepares it
  (`LoadJob::share`: read from the share, its chart found by the digest,
  `prepare_chart`, `finish_shared_load`; loaded only with the digest
  chosen). A stage is played on each member's device
  (`StageOrder::others`, nobody else shown during the play), its run
  reported as the member's own (`report_run`, `ddi_room`'s
  `ReportOutcome`; self-reported), the room ranking the reports
  (`gauge-survival-v0`). Desktop's threads answer through the client's
  inbox (`set_inbox`, `ShellNews`). `crates/ui/src/state/online/` is the sign-in and the
  rooms' HTTP, sans-IO (`Online` over the shell's `Http`; `api.rs` each
  request and answer, a room's socket's address, `api::socket_url`;
  desktop's attempt polled, the web's page left for the server's
  sign-in), and what both shells keep of it, `online::Kept` (the online
  state, the room by id, the audience's seat by link, each made by the
  shell's makers, let go when left), `screens/online.rs` Join by link
  and an invite's page as pages fed by it (`OnlineScreen`, a `FedPage`;
  `OnlineCall` to the shell). The audience (`docs/plans/multiplayer-audience.md`):
  `crates/ui/src/state/audience.rs`, `AudienceClient`, a seat over the
  shell's socket (`ddi_room::audience::Audience`), its tickets from
  `Online`'s audience join (`api::audience_join`, a new one per lost
  socket), the public view, the count, its stages' songs read from the
  room's share (`LoadJob`) and each stage handed over as a watched
  `StageOrder` (`AUDIENCE_ME`), fed through `room::LiveRoom` (the trait
  `ddi_session::battle::live` reads a member's `RoomClient` or a seat
  by), its music and fields `AUDIENCE_VIEW_V1`'s delay behind the
  room's song time (a member who watches a live stage the same way,
  `room::WATCH_MUSIC_V0`, `RoomClient::watch_music_ms`);
  `screens/audience.rs` its page between stages
  (`AudienceScreen`, `AudienceCall`; `ThemeState::audience`; built as
  the Ready card, `audience-page-v1`, over the menu's rain, then the
  last watched stage's picture, `AudienceClient::set_backdrop`; since
  round D3, `audience-page-v2`: once a stage settled, tabs Players and
  Standings over the players' part, the last settled stage's table the
  members' own, `AudienceClient::stage_table` over `room::TableSource`,
  the builder `RoomClient::stage_table` uses, drawn by
  `results::stage_standings` through `TableWho`; the picked song's
  banner beside its title once the seat read the room's share,
  `AudienceClient::banner`; and for a seat with no picture the last
  played stage's background read from its share, `sync_picture`,
  `SharedFound::background`); the invite
  page's Watch with the audience (`Online::watch_audience`, signed in
  or not, never asking for a sign-in; `OnlineCall::Watch`), members' count on the status line, Host options'
  Close/Open the audience and More's Copy audience link
  (`RoomClient::audience`, `set_audience_open`). M7b: the room's main view is song select
  (`SelectScreen::with_room`, `screens/select_room.rs`, a child module of
  `select`): practice's the player's library, Enter "Pick for stage N";
  an online room's (revision 17) two tabs, Room songs (the room's list,
  one flat list, each row's line its state for this player and who added
  it; the picker's Enter "Pick for stage N") and My songs (the player's
  library, Enter "Add to the room" for who may add), M, the tabs' buttons
  and More switching them, each keeping its place (`RoomMode::library`,
  `Place`, `room_place`); nobody follows the picker's song; Rules and
  Players in the
  header (More on a phone), the status line over the list. An operator
  who does not choose has Pick songs (`screens::room::TAKE_PICK`: a
  button at the status line's end, the room's More, T): the pick
  taken at once; the status line names who chooses and
  the players' rows mark them. The picker's list has a place of its own
  for the session, never singleplayer's (`state::place::ROOM_KEY`,
  `load_room`, `apply_room`, `room_picked`: the first room starts at
  singleplayer's, a pick stays selected after its stage; the shells build
  the room's song select from it; a follower tells none;
  `docs/plans/song-navigation.md`, "In a room"); its footer's Options and
  Calibrate open over it on both shells. The screen is built as one side at a time
  (`select_room::Side`: nothing of a view until the room says who the
  player is, an online room's first answer coming after the screen is
  built, only the bars and a quiet line; then the picker's or the
  follower's view, coming in once with the bars kept; a change of hands
  one swap, the old view's parts out and the new one's in, at once under
  reduced motion; `crates/desktop/tests/room_picker_offscreen.rs` draws
  the views over the fake room). The players
  are always there (feedback round B; round SLB): a room's wide layout
  is in three parts, the song's side where solo has it (narrower: its
  squares 208 at 1280×720, solo's 256), the list, and the players panel
  at the screen's right, 168 wide (round DC's balance, `select_side.rs`'s
  `ROOM_BALANCE`; the others studied selectable for tests,
  `study_room_balance`) (`screens/select_players.rs`,
  `RoomPlayers`: a panel of the frame foldable to a rail by its handle,
  L or More, `Settings::players_folded`; a row a monotone state icon,
  `room::StateIcon`, the marks, `room::Marks`: a golden or silver house,
  a wrench, the note, then the name; the place, a CPU's pips or a bot's
  head at the right; the words in a pop-over and the Players pop-up); a
  phone has them as one line over the list (`room::players_strip`); a
  tap on the strip, the panel's head or a row opens the Players pop-up.
  `screens/room.rs` what the
  room's screens share (`RoomCall`, the per-frame `Pump`, the status and
  rows, a person's state for a stage in the room's order, `person_state`
  (nothing before a pick, Waiting for song, Loading, Not ready, Ready,
  then Playing or sat out with the room's reason), the panel, More, `ThemeState::room`, a `RoomView`, for tests;
  `DueDialog`, the notice over a screen opened over the room (Options,
  Calibrate, the announcer studio) that the room's stage is due for the
  player, `RoomClient::stage_due`, with the room's status line and a
  button to its Ready screen, Stay here told once a stage, nothing
  closed for them: desktop's room screen under those screens opens it
  and asks them to leave by their own Back, `Cx::surface`,
  `Screen::yields`; the web's kept room's timer pushes it over the
  page, `crate::room::tell_due`, `Ui::push_dialog`, its button
  navigating to the room; never over a control being bound,
  `Screen::holds_notices`, `Cx::notices_wait`, `Ui::notices_wait`: it
  opens once the binding is saved or canceled; and over a stage's
  results, which tell it themselves, `results_stage.rs`, its button
  going back to the room as the results' own does, the web's kept room
  run under them too);
  `screens/room_dialogs.rs` Rules (from their button, Cancel and Apply;
  its rows scrolling between its title and buttons, focus scrolling a
  row into view, each note only where it applies;
  the roster's rows in practice and in an online room that plays live,
  `RoomClient::cpus_offered`, its seats kept for the people
  still to come; such a room's picks are charts its CPUs can play,
  `RoomClient::has_cpus`;
  during a running match, to who may edit rules
  (`RoomClient::may_change_stage_rules`) a row "Changes apply": From
  stage N, first, where only the per-stage rows change (Points too, a
  sentence under it where it differs from the stage before: totals
  shift)
  (`RoomClient::change_stage_rules`), or Next match; Apply and restart
  takes what the pop-up shows; the status line says a change until its
  stage begins, `room::pending_change`; the Standings mark such stages,
  `StageRules::changed`;
  a room opens straight on its song select with the default rules, and
  only the hidden option `Settings::multiplayer_rules_first`, off by
  default, opens them by themselves on entering in their first form,
  Reset and OK; its rows Stages, DDI's Difficulty and Boos, practice's
  roster, `AddCpuDialog`, and Attacks, offered where they work
  (`RoomClient::attacks_offered`: practice, On by default; an online room says
  they are not there yet); the rules in a few words where they are
  summarized, `room::rules_words`: what differs from the usual, so "No
  attacks" where a practice match has them turned off), `PlayersDialog`
  (the players, then the picks; what the room does if its host leaves,
  said to everyone; an online room's host has Host options there, a
  button or a tap on another person's row: `HostDialog`, the "If the
  host leaves" setting, Make co-host and Remove co-host, Make host
  asked once more, never for a bot; `RoomClient::set_host_leaves`,
  `set_co_host`, `make_host`, which send nothing a room could not
  read; a co-host's Host options, `RoomClient::may_host_options`, only
  Let play and the audience's row; More's Replace invite link asked
  once, `room::ask_replace_invite`; Copy watch-only link only under the
  hidden option `Settings::multiplayer_watch_link`), `StandingsDialog` and the refusal notice. Leaving asks first
  where it costs something (`RoomClient::leave_cost`, `LeaveCost`: an
  online room's host always; during a running match a player in it,
  the one choosing its songs, a co-host; practice with a match
  running): `room::ask_leave` ("Leave the room?", what the room will
  do, Stay chosen first; the layer `ddi_ui::LEAVE_QUESTION`), from Back
  on the room's song select, More's Leave room and a shell's close
  request (`ddi_ui::CLOSE_REQUESTED`, broadcast where
  `Ui::guards_leaving`, `Screen::guards_leaving`: desktop's
  `Shell::close_requested`, the window's close and Quit, which quit
  only on Leave or when asked again while the question is open); Back
  on an online room's song select leaves the room (told, forgotten),
  practice between matches is kept; `screens/prompt_room.rs` the
  stage's prompt (`PromptScreen::with_room`: Ready, its countdown, the
  players beside the diagram, where the match has attacks the player's
  attack policy as one quiet row with what it does
  (`Opt.room.targeting`: Left and Right, its arrows, a tap or Enter;
  `RoomClient::set_targeting`, the play given the room's policy as it
  starts and the room told what the play left it at,
  `played_targeting`; the players panel and the Players pop-up say
  everyone's policy quietly, `PlayerRow::policy`), `PromptChoice::Ready` for the shell's audio
  in the press, Start chosen when the room starts the stage). Text in
  `locales/*/room.ftl`. Desktop keeps the room for the run
  (`crates/desktop/src/room.rs`: the home and the room's song select on
  the UI's stack, a stage's prompt over it through the play host;
  `--screen multiplayer|room` and `--join` open on the song library as
  song select does, `shell::LIBRARY_SCREENS`; a
  stage played over the room through the play host, `play::Request::stage`
  with its room, `room::StageRoom` (a member's client or an audience
  seat's and the rooms' steady clock, `clock::now_ms`, a test's fake
  one), never a thread-local,
  `room_screen_with` for tests, `tests/room_stage_flow.rs`). Its online
  rooms (M5c, `crates/desktop/src/online.rs`): HTTP through ureq on a
  thread a request, the room's socket through tungstenite (native-tls,
  as ureq) on a thread of its own, signing in by the server's attempt in
  the system browser (`crate::os::open_url`) long-polled, the attempt's
  code shown in the sign-in dialog and typed by the person in the
  server's page before anything goes to Twitch (since 2026-11-15:
  `SignIn::Waiting::code`, `api::grouped_code`; five wrong codes end it,
  `api::TOO_MANY_CODES`, Try again a new one), the session kept
  in the config folder's `online-session` (never `share_token`, a
  refreshed one from `x-ddi-session`); `--join LINK|INVITE` opens an
  invite's page; a stage's song uploaded (`share::upload_room`) or read
  from the room's share into the cache (`share::load`, the songs in
  `library::register_room_songs`) on threads (`room::upload`,
  `room::prepare_online`); `tests/online.rs` runs against a local server
  named by `DDI_ONLINE_API` (sign-in, a room, the resting song uploaded
  under the spotlight's grant and read by the guest, the pick reusing it,
  both ready and both runs reported).
  Bots for testing rooms (M7d, a DEVELOPMENT TOOL,
  `crates/desktop/src/bots.rs`): `ddi --mp-bots`, headless, each bot a
  `RoomClient` over desktop's HTTP, socket and sharing (its own inbox,
  `room::news_inbox`), its play a room's CPU
  (`ddi_session::battle::cpus::opponent`, on the stage's ladder where
  its charts adapt) on a real-time song clock, reported as a player's
  run, each stage's song checked for it on the thread that read it and
  read once (`room::SongCheck`), a host bot's songs checked as it comes
  to each; signed in by the
  server's `POST /auth/bots` for an admin share token, identity
  `bot:<slug>` (`ddi_room::Identity::is_bot`), never an admin, labeled
  Bot wherever players are listed (`screens::room::person_name`, the
  invite's `host_bot`). A live stage's CPUs (`online-cpus-v0`) are
  computed by every bot as by a person's device and reported for
  (`battle::live::LivePlay`); `--cpus N[:LEVEL]` gives the host bot's
  Rules N CPUs of the roster (CPUs, never bots). `--audience N` with `--join LINK`: N seats in the
  room's audience that only watch (two a signed-in bot; with
  `--anonymous` no session at all, seated by network), each reporting
  what it received (`run_audience`). The audience's page on desktop
  (`room::audience_screen`): `--join` takes an audience link, its stages
  watched through the play host (`room::StageRoom::audience`, no results
  of their own); `tests/audience_offscreen.rs`.
  The web keeps it for the session (`crates/web/src/room.rs`, thread-local
  as `crate::share` is): `#/mp` is the
  home, `#/room` the practice room's song select
  (`select_page::Props::room`, `crate::room::RoomRoute`, its calls
  `crate::room::shell`; `#/room/pick` is `#/room` now), a stage's prompt
  and play at
  `#/play?song=…&chart=…&stage=<id>` (`game_canvas::Props::room_stage`, its
  outcome reported by the game loop, back to `#/room`; an address the room
  did not hand over shows the room). A reload starts a new practice. Online rooms (M5c,
  `crates/web/src/online.rs`): `#/mp` is the online home
  (`ddi_ui::screens::online_home::Home`), `#/mp/join/<invite>` an
  invite's page (links made before 2026-11-14 carry `mp=1`, still read),
  `#/mp/room/<id>` the online room's song select; signing in leaves the page for the
  server's `api/auth/twitch/login?return=<route>` and comes back to the
  route (`?signin=canceled` when canceled), what it was for kept in
  `sessionStorage` (`ddi.mp.after`); the session is the server's HttpOnly
  cookie; a room's socket is the browser's `WebSocket` on
  `api/sockets/<id>/ws`; a stage's song uploaded under the room's grant
  (`crate::room::upload`, `share::upload_room`: an imported or a bundled
  song), read from the room's share (`crate::room::prepare_for`: its
  music fetched to the end too, the room told how far in tenths,
  `RoomClient::load_progress`, so that a song is loaded once it can be
  played), its
  stage played at `#/play?…&stage=<id>&room=<id>` back to `#/mp/room/<id>`
  (`crate::room::play_in`, `back_route`). The kept online room is run
  from a timer and from its socket's events wherever no frame runs it
  (`crate::room::run_kept`, `wake_hidden`: under a page over it, on a
  stage's results, and on any of its pages while the tab is hidden, where
  a browser draws no frame: a pick is loaded, a dropped socket opened
  again at once, `RoomClient::hurry`, and the tab's title says the stage
  is starting while it waits for the player's Ready). A room watched with its
  audience: `#/mp/audience/<link>` its link's page, `#/mp/watch/<link>`
  the audience's page, `#/mp/watch/<link>/stage` a stage watched (the
  play route's spectator view with the seat in the room's place,
  `crate::online::watch_stage`, `game_canvas::Props::watch`: no prompt to
  press where the browser starts the sound, the Watch press's context
  kept for it; nothing reported, back to the audience's page; reloaded,
  or back on the page during a stage, the page offers Watch, focused
  first, whose press starts the sound and hands the stage over as it
  stands, `AudienceClient::watch_now`, `AudienceCall::WatchPressed`);
  TESTING
  ONLY, `mpfakeseat=1` (`mpfakeend=S`) in the address the page loads on
  makes the next seat one
  in a fake room in the page (`crate::fake_seat`;
  `mpfakematch=STAGES,CUT,WAIT` its match: so many stages, each cut to
  CUT seconds, the picks WAIT seconds apart, the members saying how
  they performed), and `mpfakeroom=S`
  makes online room `Room000001` a fake room in the page whose scripted
  owner picks a stage S seconds after the player joined (readying at
  once, or R seconds later with `mpfakeready=R`; playing live with
  `mpfakelive=1`, where she is a no-show), reports her
  run three seconds into its play and picks a second stage S seconds
  after the first's results, and whose bob readies at once (a start
  needs two: a Ready window runs out without the player; live, he
  streams the chart); `mpfakefail=N` fails the player's first N
  downloads of a stage's song, `mpfakeslow=S` makes each take S
  seconds, `mpfakeelsewhere=S` closes the player's socket 4429 S
  seconds after a pick (a fifth device; away after 5 s)
  (`crate::fake_room`: a member's view without a server). `Trunk.toml` passes `/api/` (redirects untouched) and
  `/api/sockets/` (WebSockets) to `just dev` of `../ddi-server`.
- `tools/danoni-importer/` the native helper, a root workspace member requiring
  Rust 1.99 (as does the SDK). Both share the root lock; the `importer` profile
  keeps native builds unwinding with bounded compiler memory.
  The desktop's default feature `danoni-importer` (`danoni` alias; the web
  never builds it) prepares the pinned worker/helper
  and complete notices through `crates/desktop/build.rs`, stages an immutable bundle
  beside the app, and compiles only its content ID/digest into desktop. The SDK
  is not linked into the app. `tools/package/package.py` copies a complete relocatable
  distribution; see `docs/plans/danoni-importer.md`. Python/Java are build-time
  prerequisites only. No helper preparation runs for wasm or Windows builds or
  with `--no-default-features`. `DDI_IMPORTER_TEST_ONLY=1` skips preparation
  only in debug unit/check builds (`just test`, `just check`, CI's
  `deploy.yml`, and agents' clippy and test runs set it), leaves import
  unavailable and cannot be packaged.
  Desktop's URL tab is a user-triggered single-work download: no site-approval
  prompt or robots.txt request, with the usual public-address checks, limits and
  pacing. The SDK's research/batch default still respects robots. Show real task
  counts where available and continuous activity otherwise; honor reduced motion.
- `tools/danoni-dump/` the Dancing☆Onigiri URL-import SDK
  (`rust/`, a root workspace member), its confined Ruffle worker build inputs
  (`ruffle/`), desktop handoff probe (`desktop-probe/`) and independent chart verifier
  (`pack-verify/`). The SDK, probe and verifier are root workspace members
  sharing `Cargo.lock` (the probe links the desktop, so a separate lock went
  stale with every game dependency change); only the generated upstream Ruffle
  worker keeps its own workspace and lock. Root Cargo checks cover them only
  when selected (the app remains the default member). CI's `danoni.yml` runs
  on changes to the crates, the DanOni tools, the manifests or the lock, not on
  documents, assets or the web shell's code alone. `just danoni-check`
  (Python 3, ffmpeg and native build prerequisites) runs their locked checks, synthetic tests and independent
  Opus decoder comparison; `--minimal` checks the SDK without native/codec
  features, including on Windows in CI. No live songs or runtime downloads
  are part of this check. The SDK runs inside the optional native helper rather
  than the game process; prepared SSC/Opus use its verified playback snapshot
  and the ordinary library. Never scan transaction directories. Sources and
  generated third-party packs stay outside Git. See
  `docs/plans/danoni-importer.md` for the current build/distribution contract;
  `docs/research/danoni-desktop-integration.md` records the research.
- `crates/icon/` the approved brand icon's build-time compositor: the vector
  artwork, frozen color texture and target profiles in `assets/icon/`. Cargo
  builds the Windows/Linux window SVG in `OUT_DIR`; Windows' executable
  resources use the same renderer. Optional `raster` uses resvg for xtask's
  target-specific output. Debug builds select the dev frozen texture and
  gap color; release/desktop profiles keep the approved release colors
  (`DDI_ICON_VARIANT` overrides). Plain macOS dev runs also set the Dock
  icon. Packaging uses the executable's compiled `iconVariant`, reported
  by `--danoni-bundle-info`. No shader or editor runs during builds.
- `xtask/` generators: target-specific icons (`icons --target
  web|windows|linux|macos`, cached under `target/icons/`), SEO fragments, the song manifest, `credits/CREDITS.md`
  and `credits/THIRD_PARTY_RUST.md`; and `announcer tidy --dir IN --out
  OUT [--level DB]` (`xtask/src/tidy.rs`): a folder of WAVs tidied by
  the game's own code, a line of JSON each, which
  `tools/announcer-design/kokoro.py` makes our packs' sounds through.
- `tools/package/` the desktop's packaging (`package.py`: the relocatable
  folder and each system's archives; the app's `Info.plist`, the Linux
  desktop entry, the icon), run by `just package` and
  `.github/workflows/release.yml` (`docs/plans/packaging.md`). Icon sources
  live only in `assets/icon/`; Trunk stages the generated favicon, and
  packaging asks xtask only for its platform's ICNS or SVG. Generated icon
  assets stay out of Git.
- `video/` the software video decoder: C shims over FFmpeg's video
  decoders (every format StepMania plays), built two ways from the one
  pinned release (`video/FFMPEG.toml`): for the web to
  `target/video/ddivideo.wasm` with wasi-sdk (`video/build.sh`; LGPL
  module, never committed; `docs/plans/video-backgrounds.md`), and for
  desktop as static libraries (`video/build-native.sh`, into
  `target/video/native/<target>/`) linked into `ddi-video-helper`
  (`video/helper`, the sandboxed decoder process: Seatbelt on macOS,
  seccomp and Landlock on Linux, a restricted token on Windows, each
  self-tested before it serves; `video/protocol`, `ddi-video-protocol`,
  the bounded pipe messages and the shared-memory arena both sides use;
  `docs/plans/native-video.md`, section 10). `video/ddi_io.c` is the
  helper's file layer (the game serves the bytes). Additive bounded demux
  exports in `video/packets.c` (`video/packets.md`), read on desktop by
  `crates/desktop/src/video/packets.rs` for native MP4/Matroska/WebM (`media.rs`,
  codec validation and output models in `codec.rs`, `h264_format.rs`,
  `hevc.rs` and `vp9.rs`). FFmpeg is never linked into the game.
- `assets/songs/<id>/` bundled songs with a `PROVENANCE.toml` each;
  `assets/noteskins/<game>/<name>/` bundled noteskins, the same way
  (`docs/plans/bundled-skins.md`; Infinito is the default SVG skin;
  the original Dito variants are retired
  and procedural arrows are an internal fallback without a picker entry);
  `assets/fonts/` the embedded HUD faces and the theme's text face
  (Iosevka Term, served as `fonts/`); `assets/announcers/Clank/` the
  built-in announcer, on by default (`bundled:clank`), its
  `announcer.ini` and a voice a language, `en/` (Mr. Clank) and `ja/`
  (ガキンさん), each made by `kokoro.py pack --bundle` with the
  `PROVENANCE.toml` it writes (`docs/plans/announcers.md`, "Packs of our
  own", "One built-in announcer, on by default").

## Commands

- `cargo build` includes the native URL-import helper/runtime on macOS and
  x86_64 Linux (the default `danoni-importer` feature; a JDK, CMake and
  Python 3.11+ at build time). The first build fetches pinned public
  build inputs; no runtime tools or maintainer configuration are needed by the
  result. `python3 tools/package/package.py folder --app
  target/debug/ddi --output /tmp/ddi-package` prepares a fresh
  relocatable folder (`build.py package` forwards to it). Keep its assets, notices, optional full video payload and
  `danoni-importer/` beside the executable. `--danoni-bundle-info` and
  `--danoni-importer-check` inspect it without a window or sound device. The app
  defaults to video and the importer; `--no-default-features --features
  danoni-importer` makes a deliberate no-video package, `--no-default-features
  --features video` a build without the importer.
- `just package` builds with `--profile desktop` and packages for this
  system into `target/dist/` (`package.py dist`: an ad hoc signed `.app`
  in a zip and a `.dmg` on macOS, its files in `Contents/Resources` and
  the helper in `Contents/Helpers/video`, found through
  `ddi_desktop::paths::package_dirs`; a tarball with a desktop entry on
  Linux; a zip on Windows, `--without-video-helper`), after the packaged
  game's headless checks. A pushed tag `v<Cargo's version>` runs the same
  in `.github/workflows/release.yml` on the three systems and attaches the
  archives to a draft release (`docs/plans/packaging.md`).
- `cargo run` opens the desktop app on our theme's silent start menu
  (`--profile desktop` for smooth frames: `release` is the wasm bundle's,
  tuned for size and aborting on panics). Flags (`ddi_desktop::run`,
  read through `ddi_desktop::args`; the saved switches written at once,
  `args::save_switch`):
  `--songs DIR` (a folder for this run), `--sample`, `--screen
  start|select|results|options|calibration|credits|announcer|multiplayer|room`, `--dev`, `--widgets`,
  `--play SONG [--chart N] [--auto | --auto=pad] [--start]`,
  `--edit SONG [--chart N]` (the chart editor on it),
  `--replay PLAY_ID [--results] [--start]` (a kept play's replay, or its
  results),
  `--calibrate visual|audio|combined [--auto] [--bias MS] [--start]`,
  `--shared LINK|ID`, `--sharing on|off`, `--multiplayer on|off` (once
  the multiplayer prototype's switch, taken and doing nothing now;
  `--screen multiplayer|room`; `--join
  LINK|INVITE` an online room's invite; `--mp-rules-first on|off`, a
  hidden option, off by default: a room's Rules open by themselves on
  entering, as the web's `mprules=1` (`mprules=0`),
  `Settings::multiplayer_rules_first`; `--mp-watch-link on|off`, another,
  More's Copy watch-only link, as the web's `mpwatchlink=1`,
  `Settings::multiplayer_watch_link`; `--mp-attacks` and its old name
  `--mp-dev-attacks`, still taken and redundant: a practice room's
  Attacks rule is On by default (anyone turns it Off in Rules), as the
  web's `mpattacks=1`; the web's `mpattacks=0` starts a practice room
  with the rule Off, for tests and screenshots; development only, `--mp-dev-cpus
  N`, practice against N CPUs beyond the cap, as the web's `mpcpus=N`;
  `--mp-dev-mines BEAT`, a section of battle shocks in each stage's play
  from that beat, as `mpdevmines=BEAT`; TESTING ONLY, `--mp-dev-chat
  FILE|NAME`, a scripted chat beside every play where Options →
  Display's Twitch chat in rooms is on, as the web's `mpfakechat`,
  `ddi_chat::script`; CPU practice's stages take its line as their
  room's ceiling (the chat's rows show to everyone since round TW7);
  `--mp-dev-chat live`, round TW6, the live chat's development script:
  the signed-in person's own channel's chat in solo plays and CPU
  practice's stages, through a room's code, its lines on stderr led by
  `[ddi-chat]` and, since round TW7, in the data folder's
  `chat-report.txt` (replaced each run; `chat::report_into`);
  `--mp-dev-chat live:LOGIN`, round TW7, that channel's chat there and
  in an online room's stage in the room's channel's place on this
  device, as the web's `mpfakechat=live:LOGIN` (our server allows it
  for administrators only); TESTING ONLY,
  `--mp-dev-chat-socket URL` a local fake EventSub socket in Twitch's
  place, `--mp-dev-chat-at BASE` the providers' addresses from a local
  base, `--mp-dev-chat-record DIR` every raw socket frame written
  there), `--share IDS|pack:NAME
  [--expires W]`, `--shares`, `--delete-share ID` (songs shared by link),
  `--mp-bots --api URL [--host] [--join LINK] [--guests N] [--site URL]
  [--stages N] [--matches N] [--rest S] [--wait-for N] [--songs DIR]
  [--pick ID[:N],…] [--add ID[:N],…] [--everyone] [--group]
  [--repeats allowed|row|match] [--choose random|first|none] [--names A,B]
  [--level N|A-B] [--personality P]
  [--cpus N[:LEVEL]] [--shocks N] [--ready-late S] [--audience N
  [--anonymous]]`
  (DEVELOPMENT TOOL: bots in a private online room, played by the CPU,
  no window or sound; the admin share token from `DDI_SHARE_TOKEN`;
  locally `DDI_SHARE_TOKEN=test-token … --api
  http://127.0.0.1:8787/ditoditoinfinito/api --site http://localhost:8080/`;
  `--level 5-10` spreads the bots over those levels in an order drawn
  from `--seed`; `--help` lists the flags; Ctrl-C leaves),
  `--fake-pad "1.0:13+,1.1:13-"`, `--silent` (no sound device: plays run
  on the host clock), `--lang TAG` (the UI's language for the run: `ja`,
  `qps-ploc`, `qps-wide`; TESTING, `DDI_SYSTEM_LANGUAGES=ja-JP,en-US`
  stands for the system's languages, `shell::system_languages`),
  `--volume V`, `--seconds N` (quits, printing the
  library, the clock's latency and jitter, the devices, the plays),
  `--lose-device-after S`, `--video-check FILE [--reference REF.yuv]
  [--frames] [--decoder auto|software|videotoolbox|mediafoundation|mediafoundation-auto|both]`, `--pacing block|late` (a play's frame pacing),
  `--in-flight 1|2` (defaults: late with two when a display link is
  available, block with one elsewhere), `--pacing-report` (block and late with one and two in
  flight, 4 s each in turn on Fifo, a table of each printed when the
  play ends; `docs/plans/desktop.md`, "Frame pacing"). **Agents' and tests' runs set
  `DDI_HOME=$PWD/target/scratch/<name>/home`** (never the maintainer's
  folders; the URL importer refuses a home inside a Git checkout, so
  `--danoni-importer-check` takes one outside it, as under `/tmp`), use
  `--volume 0.005` or `--silent`, and run under
  `target/scratch/wd.sh`; prefer offscreen tests: a window takes the
  Mac's focus. Two runs that open the window cannot share a home: the
  second ends with status 3 and a line on stderr (`instance.rs`), so
  runs at the same time take a home each. The same crate built for wasm
  is the web app, which Trunk builds (`just dev`, `just build`).
- The web build is compiled with wasm SIMD (`.cargo/config.toml`'s
  `+simd128` for `wasm32-unknown-unknown`, `--enable-simd` for wasm-opt in
  index.html; the page refuses a browser without it, `no-wasm-simd`).
- CPU player: desktop `--play SONG --chart N --cpu=1 --cpu-level 5
  --cpu-seed 7`; web `#/play?song=ID&chart=N&cpu=1&level=5&seed=7`.
  Automatic mode accepts web `skills=` / desktop `--cpu-skills`: a preset or seven
  0.5–2 multipliers (Stream, Jumpstream, Handstream, Stamina, JackSpeed, Chordjack,
  Technical). These are pattern proxies, not MSD measurements; balanced preserves
  the original seeded inputs. See `docs/plans/cpu-player.md`.
  Prepares an analytical plan locally for imported/bundled keyboard 4K charts.
  Tap-only, 1×, no fail; normal judge, no personal history or bests. It
  plays the multiplayer CPUs' absolute scale (`CPU_LEVELS`,
  `cpu-levels-v2`, `ddi_engine::cpu::solo_script`): `--cpu-level N` / `level=N`, 1 to 10,
  level 5 where none is named (`SOLO_LEVEL`); the five named strengths
  (`--cpu-strength`, `strength=`) are levels 6 to 10 (`strength_level`);
  a prepared profile plays its strength chart-relative (`cpu-levels-v0`)
  as before.
  Optional external prepared profiles use `--cpu FILE` or
  a site-relative `cpu=cpu/custom.json`; none are bundled. See
  `docs/plans/cpu-player.md` for limits, architecture and verification.
- `just dev` dev server (auto-reloads on every rebuild; unsuitable for long
  runtime tests). `just dev 0.0.0.0` serves the local network, but a LAN
  address over HTTP is not a secure context (no gamepads, no WebGPU); from
  another machine, tunnel instead (`ssh -N -L 8080:127.0.0.1:8080 <host>`,
  then open `http://localhost:8080`).
- `just build` release build with the production public URL.
- `just screenshot [scenario.json] [animation.avif] [still.png]`
  regenerates the README's animation and still from
  `docs/screenshots/readme.json` (Beatucada's Challenge chart,
  autoplayed). The desktop example `screenshot` drives the desktop shell
  offscreen on a fixed clock (`Shell::offscreen`, `offscreen_frame`), with
  no window or sound device: the start menu (its Quit hidden,
  `shell::hide_quit`), song select and the prompt pressed through, then
  the play's stretches the scenario names in beats, each crossfaded into
  the next on a downbeat of both (one unrecorded frame stretched so that
  they are exactly in step), and the results faded to black. Drawn at
  1280×720, it is encoded as a silent animated AVIF (SVT-AV1) at the
  scenario's `animation` size and rate; the README shows it with the
  PNG as the `<picture>` fallback. The still is the full-size frame just
  after the combo reaches 100. Its home is isolated under
  `target/scratch/screenshot/home`. Requires a GPU and FFmpeg with
  SVT-AV1 on `PATH`. Media provenance and reproduction are documented in
  `docs/screenshots/README.md`; the generated AVIF and PNG are committed
  and their third-party components are annotated in `REUSE.toml`.
- `just check` native checks, wasm check, SEO regen, REUSE lint.
- macOS video planes remain copied by default. A build with
  `--features ddi-desktop/video-metal` and `DDI_VIDEO_PLANES=metal` opts
  into IOSurface/Metal planes for checked VideoToolbox streams; unsupported
  imports copy the current image, and unconfirmed GPU teardown disables
  sharing. `docs/plans/native-video.md` section 4f records ownership bounds,
  measurements and ignored GPU tests. `video-bench` adds the offscreen
  comparison/benchmark; run each ignored test separately under the watchdog
  with a scratch home. Optimized benchmarks set
  `CARGO_PROFILE_DESKTOP_LTO=false` to keep compilation under 2 GiB.
- Windows from the Mac: `rustup target add x86_64-pc-windows-msvc`
  (and `aarch64-pc-windows-msvc`), then `cargo clippy -p ddi-desktop
  --target x86_64-pc-windows-msvc --all-targets` with and without
  `--no-default-features` (no `video`; checking needs no linker; running needs Windows).
  Linux: the CI job's commands in `catthehacker/ubuntu:act-latest`
  through `container run` (`act` with `socktainer` gets as far as
  `actions/checkout`, then cannot copy its step scripts into the
  container).
- `just l10n-check` (`cargo run -p xtask -- l10n check`): every language
  has every English message and no other, each well formed (variables,
  terms, plural categories; none in `boot.ftl`), no literal given to
  `UiText::data`, a page's DOM, a window's title or a file dialog's kind
  of file, no character its faces lack (and each face's file credited
  and copied by `site/index.html`); then each language's review states.
  `xtask l10n notes` is the same check printing the states as the
  release's notes (`release.yml` runs it before packaging: a language
  missing a message stops a release); `xtask l10n package [--previews]`
  the shipped languages' tags and Linux desktop entry lines as JSON
  (`tools/package/package.py`: `Info.plist`'s `CFBundleLocalizations`,
  the entry's `Key[ll]`). `xtask l10n
  fonts` cuts the languages' faces again after a message brings a
  character they lack (uv and curl; the upstream files cached in
  `target/fonts/`). `xtask l10n report <tag> [--unverified]
  [--area A]` lists messages for a review pass; `xtask l10n review <tag>
  --by agent|human <ids…>|--area A` records one (`locales/README.md`).
- `just test` native tests (chart, msd, rating, chat, engine, render, session, shell, room,
  share-protocol,
  platform with `decode` and, through desktop, `decode-all` (its references against
  the `ffmpeg` command where it is installed), library with `nfkd`,
  actor, ui, l10n, theme, noteskin, lua, desktop, the web's routes' table, xtask), after writing the
  bundled songs' and noteskins' indexes the desktop's tests read. The
  app crate has no native tests and the web shell only its routes'; verify the web in a browser. Desktop's
  local checks: `DDI_WEB_PLAYS=<file>` (`--test play -- --ignored`: the
  judgements against the web's plays of the same charts),
  `DDI_WORKS=<dir>` and `DDI_WEB_WORKS=<jsonl>` (`--test works --
  --ignored`: local works against the web's import, autoplayed),
  `DDI_WEB_EXPORT=<jsonl>` and `DDI_EXPORT_TO=<jsonl>` (`--test
  import_plays -- --ignored`: the web's play history export imported and
  judged again here; plays exported for the web to import),
  `DDI_FFR_FIXTURES=<dir>` (`--test ffr_levels -- --ignored`: our own
  FFR levels written for the web's import),
  `DDI_TIDY_OPUS=<dir>` and `DDI_TIDY_OPUS_OUT=<dir>` (`--test studio
  tidied_sounds -- --ignored --nocapture`: tidied 48 kHz WAVs through
  the studio's Opus export and back, for a true-peak meter),
  `DDI_MOVIE_SONGS`, `DDI_AUDIO_DEVICE_TESTS=1`, `DDI_OFFSCREEN_PPM=<dir>`
  (keeps the offscreen pictures). The VideoToolbox comparisons
  (`video_native*`) run on a Mac only: CI sets `DDI_SKIP_VT=1`, its
  virtual machines' VideoToolbox having stalled in them;
  `DDI_REQUIRE_VT=1` makes a refused hardware session fail them.
- `DDI_WRITE_CORPUS=<dir>[:<dir>…] cargo test -p ddi-chart --test write local_corpus -- --nocapture`
  writes every `.sm`/`.ssc` under local folders back as `.ssc` (and
  `.sm` when nothing is lost) and expects it read back equal.
- `DDI_DIFF_CORPUS=<dir> cargo test -p ddi-chart --test differential local_corpus -- --nocapture`
  compares every `.sm` under a local folder against the two oracle parsers
  (`DDI_EXTRA_SIMFILES` does the same for plain parsing in `simfiles.rs`;
  `--test digest local_corpus_digests` checks the chart digests over it).
  Third-party songs used this way stay local.
- `DDI_AUTOPLAY_CORPUS=<dir>[:<dir>…] cargo test --release -p ddi-engine --test local_autoplay -- --nocapture`
  autoplays every chart under local folders on ITG, SM5 and DDR A and
  expects a perfect play, judged again from its input log to the same
  results (`DDI_AUTOPLAY_FILTER=<text>`,
  `DDI_AUTOPLAY_VERBOSE=1` to see what fell short).
- `DDI_NOTESKINS=<dir>[:<dir>…] cargo test -p ddi-noteskin --test local_skins -- --nocapture`
  resolves every element of every skin under local `NoteSkins` folders
  (`DDI_NOTESKINS_STANDINS=1`: without their `default` and `common`;
  `DDI_NOTESKINS_LUA=1`, in release under the watchdog: with each skin's
  Lua; `DDI_NOTESKINS_LUA_PLAY=1` also plays 20 s at 60 and 144 Hz and
  reports the Lua's fuel per frame).
- `DDI_ANNOUNCERS=<dir>[:<dir>…] cargo test -p ddi-library --test local_announcers -- --ignored --nocapture`
  finds the announcers in every folder and zip under local folders and
  prints what each holds and passed over (third-party packs stay local).
- `DDI_LUA_CORPUS=<dir>[:<dir>…]` / `DDI_LUA_CASES=<cases.txt>` with
  `cargo test --release -p ddi-lua --test local -- --nocapture` compile
  local skins' Lua, or run lunita's exported limit cases with
  their time gates (under the memory watchdog).
- `just video` builds the web's decoder module (downloads the pinned
  toolchain and FFmpeg into `target/video/`); `just video-native` builds
  the desktop helper's FFmpeg the same way (`cargo build` of the desktop
  does it by itself); every Trunk build runs the former too when the
  module is missing (as after `cargo clean`, a few minutes) or older than
  its sources, and fails if it fails (`DDI_NO_VIDEO=1` builds the site
  without it); `just video-check [file]` checks it on a file
  or on generated clips against the `ffmpeg` command's decode.
- `cargo run -p xtask -- gen-songs` / `gen-noteskins` / `gen-announcers` /
  `gen-credits` regenerate the song manifest, the bundled noteskins' and
  announcers' indexes and the credits (Trunk runs them before every build; CI checks `credits/CREDITS.md` and
  `credits/THIRD_PARTY_RUST.md` are fresh). Infinito's SVG, WGSL and Lua
  sources live in `assets/noteskins/dance/Infinito/`; its experimental
  authoring playground is kept separately. The old `gen-dito` command is retired.
- `python3 tools/noteskin-danoni/generate.py [--check]` adapts the pinned
  danoniplus SVG alpha masks to the game's SVG subset for the `DanOni`
  noteskin proof. Its Lua behavior and host-interface limits are documented
  in `tools/noteskin-danoni/README.md`; upstream originals remain byte-identical.
- `python3 tools/noteskin-centers/generate.py [--check]` draws pump's
  center panel for Delta from its own art (`docs/plans/pump.md`, "Centers
  for the bundled skins").
- Peters-DDR's approved raster center, holds, rolls and layered mines are bundled
  from SkinCraft exports; editable sources are in `tools/noteskin-peters/design/`.
  `python3 tools/noteskin-peters/generate.py [--check]` reproduces only the earlier
  model/SVG study under `target/scratch/peters/model-svg-study`, not the bundled skin.
- `tools/background-design/` is the generative backgrounds study (serve
  the repository root and open it; `docs/plans/phase-8-gaps.md`, step 4):
  the candidates on a song's music, every parameter live, the choices
  exported as its `approved.json` (the accepted one, 2026-11-05, which
  `ddi_library::generative`'s values follow).
- `python3 tools/announcer-design/announcer.py` auditions announcer voices
  made with ElevenLabs (the key from `ELEVENLABS_API_KEY`, never written):
  free-plan audio, kept in `target/scratch/announcer-audition/` and never
  committed or bundled (`docs/plans/announcers.md`, "Packs of our own").
  `uv run tools/announcer-design/kokoro.py sample|speak|context` makes the same
  lines beside them with Kokoro-82M, locally (no key; the model from
  Hugging Face at a pinned revision; its output ours to release as CC0),
  and `kokoro.py pack` our announcers: every line the game speaks with
  its variants, in English (`tools/announcer-design/pack.toml`) and
  Japanese (`pack-ja.toml`), made there too, byte for byte the same each
  time on the same machine, each sound tidied by the game's own code
  (`cargo run -q -p xtask -- announcer tidy`, so the tool takes cargo
  beside uv and ffmpeg; `kokoro.py loudness --dest DIR` makes a few
  lines at several levels with each one's LUFS, true peak and what the
  compressor and the limiter took); `pack --bundle` puts them in `assets/announcers/`, which the game
  ships (run `gen-announcers` and `gen-credits` after).
  `python3 tools/announcer-design/effects.py --port PORT --engine-version VERSION`
  auditions the existing voices through a dedicated, disposable SoundCraft
  control session (its document is replaced): `effects.json`'s race, arena
  and stadium treatments beside the original, matched in loudness, alone
  and over music. Audio, measurements and command records stay under
  `target/scratch/announcer-effects/`; it never changes the bundled packs
  or the shared tidy. Use `--source DIR` for dry packs; already treated
  sources are refused. See the tool's README for isolated startup. The
  maintainer selected `stadium-soft` in `effects-stadium.json` (2026-11-16).
  Every English line now uses the approved "Here comes the next challenge.…"
  lead; Japanese retains its Japanese leads. Both packs use `stadium.json`
  through `stadium.py` → `xtask announcer stadium`, the studio's shared
  Rust `announcer::stadium` (`announcer-stadium-rust-v1`): partitioned FFT
  convolution with `assets/announcer-filters/`'s captured EQ/room responses,
  saturation and tone filtering, then the shared finish and one encoding.
  Studio Advanced → Stadium effect is optional, off by default, saved with
  the draft, applied to original recordings for previews/scenes/exports.
  Its bounded processed cache keys include settings, measurement and line.
  The browser fetches the responses on first use under their content revision;
  desktop/xtask embed them. Ready/restart/here-we-go retain dry duration;
  other tails are 0.6 s. Stadium's final leveling also runs when Tidy is off.
  Recipe, response and Rust source hashes invalidate generated caches. FFmpeg
  is used only for encoding/metering; `stadium_ffmpeg.py` is the approved
  comparison reference. `capture_stadium.py` and `stadium-soundcraft.json`
  preserve optional recapture with SoundCraft 0.20.1; no production path
  needs it. `pack-endings.toml` preserves the lead study.

Prefer the latest released versions of dependencies; check crates.io before
adding one and refresh the lockfile with `cargo update`.

## Verifying changes at runtime

The maintainer uses Firefox and tests it by hand; automated checks run in
headless Chrome. See `.claude/skills/verify/SKILL.md` for the full recipe. In
short: build a static bundle (`trunk build --release --public-url / --dist
<dir>`) and serve it with the skill's `serve.mjs` (Python's `http.server`
drops connections under the page's parallel requests), drive it with `playwright-core`
against the installed Chrome (`channel: 'chrome'`, flags
`--enable-unsafe-webgpu --use-angle=metal --autoplay-policy=no-user-gesture-required`),
and read `window.__DDI_DEBUG` for engine state. Headless Firefox runs too
(Playwright's build or the installed one, WebGL2 only), but only through the
skill's `firefox.mjs`: on macOS a bare launch cannot open Firefox's data folder,
and the helper keeps Firefox away from the maintainer's profiles.
Playwright's WebKit (not Safari) runs headless with WebGPU; it ignores the
gesture rule below, so it does not replace the maintainer's Safari check.

The UI (song select, options, the pages, the start prompt and the results)
is our theme's screens on the canvas, not DOM: drive them by keys, a fake
pad (`window.__DDI_FAKE_PAD`) or taps at `window.__DDI_DEBUG.ui.rects`, and
read their state from `__DDI_DEBUG.ui.theme` (published after every
frame and event where a test drives the page, `navigator.webdriver`;
otherwise at most a second late, and at once when a screen or dialog
comes or goes). The testing parameters (`gfx`, `auto`, `bias`,
`slow`, `perf`, `lang`, `msdslow` (chart ratings a song every MS,
`crate::ratings`), and the multiplayer ones, `mprules`,
`mpwatchlink`, `mpattacks`, `mpcpus`, `mpdevmines`, `mpfakeseat`,
`mpfakeend`, `mpfakematch`, `mpfakeroom`, `mpfakeready`, `mpfakelive`,
`mpfakefail`, `mpfakeslow`, `mpfakeelsewhere`, and the scripted chat's
`mpfakechat`, `mpfakechatimg`, and the live chat's `mpfakechat=live`
with `mpfakechaturl`, round TW6) take effect given in the address the page loads on, on any
route (`#/select?perf=1`, `#/mp?mpcpus=16`, `#/options?lang=ja`;
`crate::router::Route::current`'s first read), and later on the routes
each was always read on; `gfx`, `auto`, `bias` and `slow` stay for the
session, and `mpfakechat` (with `mpfakechatimg` and `mpfakechaturl`) for the tab's session,
through a reload and the trips through Twitch (`sessionStorage`'s
`ddi.mpfakechat`; `mpfakechat=0` ends it). The skill
lists the routes and their parameters (`#/play`, replays, calibration,
`#/video-check`, `#/skin-preview`, `#/ui-test`, `#/ui-dev`), what
`__DDI_DEBUG` carries, and a recipe for each screen and feature; song
select's arrow gestures are specified in `docs/plans/song-navigation.md`.

Import is tested by setting files on the import panel's inputs (folder or zip)
in a fresh browser profile; build test packs in a scratch directory, never in
the repo. Real third-party packs on the maintainer's machine may be used for
one-off local tests only: never copy them into the repository, fixtures or
published artifacts.

Safari is tested by the maintainer on the published URL after a push: local
origins are plain HTTP, which Safari treats as insecure, so local Safari
results are not representative. Firefox and Chrome must not break.

Start media from user gestures synchronously: create the `AudioContext` and
call `play()` inside the click or key handler, with no `await` (storage read,
fetch) in between. Safari refuses once the gesture is over, and the headless
runs (launched with an autoplay override) cannot catch it; for previews, test
with `--autoplay-policy=document-user-activation-required` and ask the
maintainer to confirm in Safari. A play opened from song select takes
song select's context, running from that press (or an earlier one), and
its start prompt decodes the music on it, so Start only starts the sound
(`select_preview::hand_over`, `web::audio::keep_for_play`); a play opened
any other way makes its context at Start, as before. That prompt counts
down to Start (Options → Display → Start countdown, 3 s by default): its
automatic Start is no gesture, so it runs only where song select handed
over a running context (never on a direct `#/play?…`, a replay, a
playtest or calibration), and where that context does not run any more
the prompt asks for a tap or key, as for a pad's Start
(`docs/plans/start-prompt.md`). The results' Retry counts down too: its
press hands a context over the same way (desktop sets the prompt's
countdown again). A quit (Escape held or twice, the quit button) ends
the play on the results (in a room's stage whose others play on here it
forfeits the stage and the rest of it is watched on from the player's
own dimmed field, the results following its decision, a skip or a
second quit: "Watching a stage" in
`docs/plans/multiplayer-prototype-progress.md`): judged to the quit with the notes left
unjudged (not misses, as after a fail: the counts and the timing are
of what was played),
graded QUIT in place of a letter (`Results::quit`, `Player::quit_at`),
kept in the history as a quit (`Ending::Quit`, the lamp QUIT) that never
counts for the bests or the ghost; its replay quits where it did
(`PlayLog::end`). R or `\` held or pressed twice (unless a lane is bound
to it: Dancing☆Onigiri 9A's R) starts the play again at once on the same
sound and music, without the prompt, keeping nothing (none in a replay or
a calibration; `SessionEvent::Restart`, PLAN.md decision 79). The results show the ruleset's main panel set alone
(the sets' tabs went; `Settings::results_panels` waits for a picker),
Retry then Song list as plain buttons, Song list focused first, Left and
Right moving the focus along them.

In calibration the autoplayer
follows the physical cue (heard audio, or drawn arrows in the muted test), not
the judged timeline; keep it that way or offset tests become meaningless. In
play (`&auto=1`) it presses on the judged timeline, so it stays exact under any
calibrated offset.

Menu Back is unmodified Escape or Backspace after text/binding/dialog handling.
The menus move with the arrow keys, W A S D or the number pad's 8 2 4 6 on
every screen (`ddi_ui::nav::key_action`, by physical code), Enter, Numpad
Enter or Space confirms; a text field being typed into takes the letters
and the pad's digits as typed. No screen takes W A S D as a shortcut (Watch
replay is V, the editor's audio view G).
Song browsing returns to Start in one press, keeping its filter and place;
Clear (the × inside Search) clears the filter explicitly; the quick volume
between Search and More is the game's one Volume, every sound's
(`ddi_ui::widgets::volume`, hover or tap for its slider, then press to mute;
`docs/plans/song-navigation.md`).
Options returns to its caller;
its P cycles the field preview's layout from anywhere on the screen (Shift+P
backwards), the preview following the edited settings on that layout on both
shells.
Every screen's header (`ddi_ui::screens::header`: song select, Options, the
pages, the results, the editor) has the visible Back, a three-bar button at
the left, then the Dosis wordmark filling the height (live, as the start
menu's), a divider and the screen's name in small caps; a play has
no link out of it: the scene's quit button (hold or double-tap it, as
Escape) shows throughout on a touch screen and for two seconds after the
mouse moves, on both shells, and the mouse over a play hovers and presses
nothing of the screens under it, with the default pointer (a play's
surface is not the UI's). Screens follow each other, never cross: a
screen's parts leave to their edges (the header up, the footer down, the
list right, the song's side left; `ddi_ui::screens::transitions`), then
the next screen's come in; on the web a `Navigate` or `Back` request waits
for that exit, so the address changes about 0.14 s after the press. The
header and footer, the bars (and what sits in the header's row: song
select's search, Options' Saved), are `Edge::HeaderBar`/`FooterBar`:
from a screen with bars to another (`Screen::shared_bars`, read from the
tree by `ddi_ui::ui::has_bars`) they stay put, only what is in them
changing at the swap, without animation; they slide only to or from a
screen without them (`BarsOn`/`BarsOff`/`BarsStay`, played by the `Ui`).
On the web each page has its own `Ui`: `Ui::set_bars_route` says which
routes have bars (`router::has_bars`, Back by `router::back_target`) and
`Ui::bars_kept`/`carry_bars` hand them to the next page's `Ui`
(`ui_host::theme_ui`). The chart editor fades as a whole and has none.
Web route ancestry and Options position live in history state; desktop uses
the UI stack. The web's history entries for dialogs take one operation at
a time (`ddi_ui::back_stack::Entries`): nothing is pushed or navigated
while a `history.go` of ours awaits its `popstate` (an entry pushed before
it makes Firefox drop the `go` and Chrome the entry), and a pop that
never comes is given up after a second. A screen that asks before it is
left (`Screen::guards_leaving`: a multiplayer room whose leaving costs
something) has one entry more over its route, so the browser's Back
reaches it as the menu's Back (it asks and stays) instead of navigating
away; `beforeunload` is registered only while that holds on a room's
page (`crate::room::sync_unload_guard`; `__DDI_DEBUG.ui.leaveGuard`,
`unloadGuard`), never in singleplayer. Every played layout's default keys end with the number pad as
a second set (4 2 8 6 the arrows, 5 the middle, the corners the diagonals;
`crates/chart/src/layout/mod.rs`, decision 70), the main keys first and
unchanged; `Numpad*` codes are the number pad's alone, never the number
row's, on both shells. Single and danoni 5 keys play their arrows on W A S D
too, between the arrow keys and the number pad (Single's D F J K went;
decision 77). Preview audio belongs to visible song select (or an explicit
Options sample); Start is silent. See `docs/plans/start-menu.md`.

## The DDI ruleset in the screens

`docs/plans/ddi-ruleset.md`, "Round 2 (built)". DDI is the default
ruleset (`Settings::ruleset`, `DEFAULT_RULESET`) and the first in
Options' list; its options are `Settings::ddi` (difficulty, Boos,
Practice) and `Settings::ddi_score` (the score or the percent in play
and in the lists), shown in the Rules section in the place of
StepMania's rows while DDI is chosen. Its difficulty reads "Timing
difficulty" wherever a person sees it (the option, the room's match
rule) and "Hard timing" or "Easy timing" in summaries, never
"Difficulty" alone, which is a chart's; identifiers keep `difficulty`.
The score shown is the
proportional part, the absolute score and the combo part together
(`DdiScoreView::score`, `ratio + absolute + combo`: a perfect
1,000-note chart about 7,060,000, a longer one more; the combo part,
`ddi-combo-v0`, `ddi::combo_points`, every note judged in combo adding
`6 × min(k, 100) + 28 × isqrt(k − 100)` of the combo `k` after it, in
whole numbers, nothing divided by the chart's size), on the points of
round U (2026-11-17, room model revision 13: a Miss −120, a mine or
shock −50, the rest as before). The percent is `ddi-percent-v1`
(`ddi::PERCENT_RULE`, `ddi::percent_of`, its shares
`ddi::PERCENT_SHARES`): nine tenths the accuracy (`points ÷ most`,
`DdiScoreView::accuracy`) and one tenth the combo part over a full
combo's of the notes shown (`DdiScoreView::max`), one floor (a rounding
down; the percent and the accuracy are shown never below 0.00 %,
`ddi::shown_hundredths`, round AC, the numbers themselves, which the
letters read, may be negative); the letters
read it (SSS 99, SS 95, S 89, A 75, B 60, C 42, the floors as before);
CPUs are tuned and measured by the accuracy (`ScoreView::accuracy`,
`battle::accuracy`), never the percent. The results show the score or
the percent, an Accuracy line and a Combo score line, and under the
grade one quiet line of what the next letter would have taken
(`ddi::next_grade`, `ResultsData::next_grade`); the absolute score has
no line; a person sees the score or the percent and nothing else of it,
in a room too (`ratio` stays for the sum's own arithmetic).
`JUDGE_VERSION` 5 judged every kept play again for the combo part and 6
for round U, from its log. Stored settings naming ITG moved
to DDI once (`Settings::rules_moved`; never move a ruleset again without
such a mark) and their ITG plays were asked to be judged again
(`Settings::rejudge_from`, `Plays::rejudge_asked`). Stored settings had
their language put back to the system's, their judgment words to
English and their cursor to the Infinito cursor once on 2026-11-17
(`Settings::defaults_reset_2026_11_17`, which `site/boot.js` reads too:
a stored language counts for the loader only once marked). A test about a
StepMania preset's own behavior (ticked pump holds, fail types, the
battery) names `itg` in its settings; `Settings::default()` plays DDI.
The multiplayer prototype's rooms play DDI alone (round 3,
`docs/plans/ddi-ruleset.md`): the match's Difficulty and Boos are match
rules, never the player's own `Settings::ddi`, and never Practice; the
player's Score display still chooses what their own HUD shows, and what
the miniatures and the stage's Standings show of the others: the score
(the sum) or the percent (round 3b). A CPU plays the chart a person is
given (`cpu-play-v1`), so its sum is its own and compares directly
(`cpu-score-v0`, which put a tap-only copy's sum on the chart's scale,
is retired); a stage ranks those still in by life, then that score.
Judgment and grade words are English data (`UiText::data`), the labels
around them messages, except DDI's judgment words, English by default
since 2026-11-17, else in Japanese (each language but English offered
by itself) or in the menus' language as the player chooses
(`Settings::judgment_words`, `ddi_l10n::judgments::Words`,
`JudgeNames::shown`; the HUD's text loads the language's bold face for
them, the web fetching it where the menus speak another,
`RunRasterizer::words_language`; round 4); under DDI the results say Early and Late, never
Fast and Slow. With Boos on, a stray press that counts shows "Boo"
where the tap judgments show, a judgment of its own
(`JudgementFlash::boo`, set where the event is scored so a replay and
the results' count agree with it; `JudgeNames::boo`, `scene::BOO_COLOR`,
the theme's `Judgement.Boo`), with no early or late word and nothing on
the receptor; an exempt press shows nothing; the results count them
in a row of their own among the judgments, in the Boo's color, where
Boos are on (the holds' Held and Off are judgment words too,
`Word::Held`, `Word::Dropped`). A grade may be two words ("Absolute Dito"): the results
set it on two lines, the lists write AD and AB
(`ddi_ui::state::history::list_grade`). A play judged from its log does
not know who played: grade an autoplayed record's with
`Worked::from_replay`, `Replay::grade_as_played` or `Player::set_bot`.
What a play came to as played is never replaced by a second judgment.
DDI's tiers have colors of their own, one table
(`ddi_engine::rules::colors::DDI`: white, white, yellow, green, blue,
orange, red; round 4), carried by `JudgeNames::palette` to the play,
the receptors' flashes, the results, the Standings and the miniatures,
to the theme as `Judgement.Ddi.*` and to skins' Lua as
`NOTESKIN:GetJudgmentColor`; every other ruleset keeps StepMania's set.

## Additional chart versions

- Options → Notes → Simplify → Without onigiri (`simplify_onigiri`)
  exposes additional versions of DanOni 5/7/9A/9B charts in Single,
  Solo, Double and Techno 8-panel (`techno-single8`); Without center
  (`simplify_center`: Off, Where missing, the default, listing one only
  where its style has no original of its difficulty (Edits always), or
  All) versions of pump single and half-double charts in Single and
  pump double in Double, the center removed and each pad turned an
  eighth clockwise, their holds still ticked as pump's
  (`Chart::pump_game`). Notes → Convert (`convert_pump_single`,
  `convert_five_keys`, on by default) converts pump single and 5 keys
  to each other, every lane kept (the center on the onigiri), not
  assisted (`Derivation::assisted`), each played by its original's
  rules. Song select marks every generated version
  (`ChartEntry::version`). See `docs/plans/chart-simplification.md`.
- Generated charts carry `Chart::derivation` and are appended after all
  originals (every onigiri-free version before every center-free one,
  then the conversions), independently of the display setting. Preserve
  original chart indexes, digests and files. Only a generated chart that
  leaves notes out gets the assisted mark from this feature; the
  existing transform options keep their behavior.
- Chart-scoped lyrics and layers resolve through `Song::effects_chart`.
  Keep chart provenance in manifests, bests and replay resolution. Old
  browser imports upgrade their metadata from stored chart data; desktop
  invalidates its library cache when the metadata changes.
- Simfiles used by the library, play and the bundled songs' index
  (`xtask gen-songs`) load through
  `ddi_library::scan::parse_simfile`, which appends generated versions too;
  the format importers preserve source charts, including SSC exports.

Song select → pre-start → play backgrounds share `crates/actor/src/background.rs`:
a cover-fitted Gaussian base at 0.2 over #05020d, with a framed content copy
entering over 450 ms and finishing its unblur over 300 ms after Start (on
the prompt's loading screen when the play is not ready yet, the play then
continuing from where it left the backdrop, `Entrance::resume`).
Reduced motion settles immediately; early Start samples the current visual
position without delaying audio. The play base follows current image/video
layers even for a screen-sized frame. Static filters are reused; direct RGB
movie uploads must call `Renderer::background_texture_changed`, while YUV
and NV12 upload methods invalidate their own filtered copies. The field
filter darkens only a drawn background (`Renderer::render`'s
`filter_path`, `Backdrop::shows`). Where the song background would show
and there is none (no `#BACKGROUND`, a picture that never loaded, the
time before a first change), a generated background drawn from the
music stands in, darkened as a picture (Options → Display → Generated
background, the field and lines by default, `ddi_shell::generative`;
over a song's own picture too with Over song backgrounds; the
kaleidoscope folds the song's jacket, else its banner, else glass), and
the prompt shows its first frame held still in the picture's place, the
play's entrance carrying on from it; with it Off,
for a Dancing☆Onigiri work, in calibration, or with brightness Off the
play is on the one #05020d ground with no band, as the prompt shows it
(`docs/plans/overlays.md`, "Field filter").

## Timing model (do not break)

- The audio clock is authoritative. Inputs carry browser timestamps and are
  judged against heard time: context time minus total output latency minus
  the user's audio offset. The visual offset moves only what is drawn.
- `getOutputTimestamp` already includes the latency; the web backend adds it
  back so the engine subtracts it exactly once.
- Offsets are stored per device profile (audio path fingerprint from sample
  rate and latencies; display fingerprint from screen size and scale, with the
  refresh rate deliberately excluded because variable-rate panels change it).
- Calibration converges on the real gameplay path with a non-periodic chart;
  the measurable range is ±250 ms by design. Changing the chart pattern or the
  window must keep every gap wider than twice the window.
- MP3s are heard where StepMania 5.1's reader has them (no delay trimmed, an
  `Info` tag frame played as silence; PLAN.md decision 46): the web moves
  each browser's decode to one of the lags the first frame allows, the one
  a wasm decode of the song's start agrees on, else the one measured for
  that browser (`ddi_platform::mp3::align`; `window.__DDI_MP3` says which
  and by how much), kept as the buffer's lead; desktop decodes that way.
  The shift is the song's audio start, never an offset. A
  Dancing☆Onigiri work's music goes where Chrome's decode has it
  instead (danoniplus plays the browser's own decode, and works are
  synced in Chrome): the lead less Chrome's expected lag
  (`mp3::Target::Chromium`, `ddi_session::music_target`).

## Licensing and attribution

- Code is MIT. Every file is covered by `REUSE.toml`; `just check` runs
  `reuse lint`. Full licence texts live in `LICENSES/`.
- Bundled songs: only works whose music, charts and graphics each carry an
  explicit free licence in the upstream files (CC BY, or CC BY-SA as a second
  tier). Never NC, ND, undeclared, or licensed only on a web page.
- Bundled noteskins: art under CC BY, CC BY-SA, Apache-2.0, the Unlicense
  or CC0, declared in the upstream files; code we may not take (Lua derived
  from unlicensed skins) is replaced by our own and listed as a
  modification.
- Bundled announcers: our own alone (no other pack, ever), made by
  `tools/announcer-design/kokoro.py` with Kokoro-82M and released as CC0;
  a voice that learned from CC BY recordings is credited from its
  `PROVENANCE.toml`'s `[[trained_on]]`, which kokoro.py writes.
- Bundled fonts: OFL-1.1 (as the HUD's and the theme's Iosevka subsets,
  `assets/fonts/`; menus load files, while play embeds ExtraBold judgments
  and SemiBold numbers) or
  the licences above, declared upstream; subsetting is a modification.
- Each song or skin directory keeps the upstream files byte-identical plus
  `PROVENANCE.toml` (source URL, pinned commit, retrieval date, licences per
  part, exclusions, modifications). In that file, `excluded` and
  `modifications` must sit above the `[source]`/`[[licenses]]` tables: TOML
  attaches keys after a table header to that table, and the generator rejects
  unknown keys there.
- Any modification of an asset (resizing, cropping, transcoding) is recorded
  as a modification with the exact command.
- Code or data copied from another project (StepMania's noteskin metrics,
  danoniplus's key tables) keeps its upstream form where possible, gets a
  `REUSE.toml` annotation and a `credits/THIRD_PARTY.toml` entry (the credits ship
  its notice). Copy only from permissive sources: never from ITGmania
  (GPLv3 since 2024-09-20; StepMania 5.1 has the same older files under its
  MIT-style licence). Reading GPL code to learn behaviour is fine;
  transcribing it is not.

Third-party Rust dependencies keep their own licences (e.g. Symphonia is
MPL-2.0, file-level copyleft, fine to link from MIT code). Only bundled
assets and copied code are tracked in `REUSE.toml`. Their notices ship
with the game: `gen-credits` lists every crate in the web build (the app
and its Opus and SVG workers) and in the desktop binary (`cargo tree`, normal
dependencies without proc macros, Linux, macOS and Windows together) with
its licence texts from the crate's files, deduplicated, into
`credits/THIRD_PARTY_RUST.md` and the Credits screen's folded "Rust libraries"
(`xtask/src/rust_credits.rs`); a licence outside its allowlist (MIT,
Apache-2.0, BSD, ISC, Zlib, 0BSD, Unicode, MPL-2.0, CC0, Unlicense,
BSL-1.0, the LLVM exception) fails the generator, so check a new
dependency's licence before adding it.

## Deployment

- SEO lives in `site/seo/metadata.toml`; `xtask regen-seo` generates the
  static head/body and image sitemap. Trunk publishes the existing
  `docs/screenshots/readme.png` unchanged as `images/readme.png` for share
  cards and structured data; keep its metadata dimensions in sync. The
  deployed audit and post-deploy checks are in `docs/research/seo.md`.
- `site/privacy.html` is the privacy note, a page of its own beside the
  game (English, then Japanese at `#ja`; no script, nothing fetched),
  copied by Trunk; the Credits' fold "Privacy" says the same in the UI's
  language (`ddi_ui::screens::credits::privacy`, `pages-privacy-*`), and
  the multiplayer home's footer and Options' Data section open the page
  (`credits::privacy_url`). Keep the three in step with what the game
  and the private server keep (its README's "Usage statistics").
- GitHub Pages from the `main` branch via the workflow in `.github/`. The
  public URL is lowercase; on a custom domain Pages paths are case-sensitive,
  which is why the repository name is lowercase.
- The page draws its loader from its own first bytes: its styles
  (`site/styles.css` inlined), boot script (`site/boot.js`) and art
  (`site/boot-art.js` with the data `tools/title-design/boot_title.py`
  writes to `site/boot-art-data.js` from the title study's geometry and
  the rain atlas: on one canvas, the start menu's title with its rings
  painted by Infinito's Vivid material ported to JavaScript, the menu's
  outline rain particle for particle and the progress bar painted the
  same way) are inlined into `index.html`, the two scripts from
  `target/boot/`, where `xtask minify-boot` (a pre-build hook) writes
  them minified with swc in release builds (Trunk's own minification
  never reaches an inlined script: its swc pass serves `data-trunk`
  script assets, which come out as files, and its HTML pass keeps
  JavaScript's comments and names) and copied as they are otherwise
  (with the loader's lines in each language put in, the rest of each
  language's beside them in `target/boot/lines/`, served as `boot/`;
  `docs/plans/localization.md`), so
  that the page fits the first round trip compressed (about 12.0 KB
  gzipped of the 12.5 the rule allows), and nothing the first paint
  waits for is fetched (no stylesheet links, classic scripts with a
  source, `@import` or `url()`). `xtask check-boot`, the last step of
  `xtask post-build`, fails a build that breaks it (a dev build, unminified
  with the dev server's reload script inside, is held to everything but
  the budget: `TRUNK_PROFILE`). The boot script lays the art out where
  the start menu's title, first choice and rain land (the theme's
  virtual screen and the title menu's layout, mirrored), and the art
  runs on the wall clock, seconds since the epoch, as the screens' live
  art, the focus outline and the menu's rain do (`Ui::set_clock`,
  `Cx::clock`, `screens::Live`, the start screen's background time: one
  across the screens, the web's `performance.timeOrigin` plus the
  frame's timestamp, desktop's wall origin run on by the host clock, so
  a start never begins the art over from the same state; reduced motion
  holds all of it at zero); the bar's done part is painted as the
  focused button's outline (`Active.EvolutionSpeed`, the field a square
  as wide as the viewport); the start
  menu built under the loader stands its title in place from its first
  frame (`StartScreen::in_place`, the entrance kept for its later
  visits) and the web shell fades the loader once that screen is at
  rest with everything the first screens need in: the web's `preload`
  (from the first UI host's first frame) loads the theme's essential
  images (`ddi_ui::screens::essential_images`: the title, the rain, the
  wordmark, the keycap and controller atlases, at the sides the screens
  draw them, into the painter's cache), the song manifest and the
  bundled skins' index into their caches, and readies the skin the
  settings name for their style on the app's field preview, so the
  first screens, the first prompt and the first play are instant; the
  bar's last stretch runs on them (`boot::settling`: each texture and
  the three units, then the screen's rest, each weighted by the
  milliseconds it should take, so the bar keeps one pace: the skin's
  files by their bytes, sized by the index, on the link as the boot
  script measured it downloading the game, and its SVG drawings at the
  count its last load took, remembered in `localStorage`'s
  `ddi.preload.v1`; its files are all asked for at once as soon as the
  index names them, texts first, `noteskins::prefetch`) so that it ends
  as the fade begins (`boot::ready_when_settled`, `Ui::settled`,
  `web::textures::counts`; the SVG worker is warmed at boot; the skin
  waited for 4 s at most, and not at all where it should take longer,
  loading on behind the menu, deferred to the fade so its files do not
  take the link from the textures; an 8 s cap should something never
  come), and the menu appears whole in place under the same title and
  rain. The download's share of the bar is the download's expected
  time against the rest's (the boot script measures the link over the
  download's first half; the rest's bytes and own time are what the
  app left in `ddi.boot.v1` at the last fade, a guess the first time;
  0.6 until measured), and the bar as drawn follows what is reported
  at a bounded pace, held under 0.95 of it, creeping a little while a
  report is late, filling the rest over 0.24 s once the app is ready
  and the shell fading after (`site/boot-art.js`; reduced motion takes
  each step whole and fades at once). The screens built under the shell
  keep their entrances for the fade (`Ui::hold_entrances` at each web
  UI's creation while the shell shows, `Ui::reveal` from the frame the
  fade begins, `boot::revealed`; each part's `On` kept as its
  `RevealMessage`, `screens::hold_entrances`, through a relayout,
  `relaid`; the start title stands in place meanwhile and the prompt's
  card's entrance is held as any part's), so the menu's choices and a
  direct play route's prompt come in as the shell goes rather than
  under it; the play route tells the shell a prompt is coming
  (`boot::expect_host`) so it waits for it as for a page's screen (its
  skin for 10 s rather than 4, the cap 12 s rather than 8, the diagram
  showing it from the first frame; the play's own skin load begins
  once the entrance is over, `boot::entrance_over`, and after the
  preview's load of the same skin, one load of a skin at a time,
  taking its drawings and component parts, `noteskin_cache`
  (desktop's reads the same way, `play::skins`), so one skin is not
  drawn twice and nothing is parsed under the entrance), and fades on
  a song that fails to load. Nothing heavy lands on
  the entrance: `noteskins::warm` no longer loads the skin again (it
  parsed the components a second time, 150 ms on the main thread, as
  the menu came in) but fetches the rest of the skin chain's files 1.5 s
  after the fade, and a skin put off on a slow link is readied 1 s
  after it. What is per song or per choice (banners, other skins,
  movies) loads when drawn, as before. A frame of the art costs a few
  milliseconds (`__DDI_ART.cost`).
  The page follows the game's theme alone, never the system's color
  scheme: its colors are the theme's (`xtask regen-seo` writes them from
  `crates/theme/src/infinito.toml` to `target/seo/theme.css`, inlined;
  the theme-color meta is its ground).
- `credits/CREDITS.md` and `credits/THIRD_PARTY_RUST.md` are generated and committed;
  `Trunk.toml` ignores them in the watcher so regenerating them does not
  loop the dev server.
- CI's storage (`.github/workflows/`): the Actions cache holds Rust build
  output only, saved on main only (`rust-cache`'s `save-if`) and pruned
  after each deploy and DanOni run (`.github/scripts/prune-caches.sh`:
  the newest of each key stays) so it fits the repository's 10 GB cap, under which
  GitHub evicts whatever was used least recently; the FFmpeg builds (the
  web's decoder module, the desktop helper's libraries) are assets of
  the rolling `build-inputs` pre-release named by their recipes' hashes
  (`.github/scripts/video-assets.sh`, `build-inputs.yml`; the build
  scripts' `--recipe`), fetched by the jobs and built only where the
  release lacks the recipe. CI builds without debug info
  (`CARGO_PROFILE_DEV_DEBUG=0`). The Linux jobs' `ffmpeg` command (the
  tests' reference decoder and clip encoder) is a static build from
  BtbN/FFmpeg-Builds' GitHub releases, pinned by tag and SHA-256
  (`.github/scripts/ffmpeg-command.sh`), not Ubuntu's package: the Azure
  apt mirror throttled that install to 28 minutes; macOS jobs use brew's.
  The Ubuntu packages the Linux jobs do install (the ALSA and udev
  headers, nasm, the importer's Vulkan and Mesa) go through
  `.github/actions/apt`: their `.deb` files kept in the Actions cache by
  package set and image version (an entry per image, which the prune
  script leaves alone: a run's jobs land on two image versions while
  GitHub rolls one out), installed with dpkg on a hit, and on a
  miss downloaded with Canonical's archive put before the Azure mirror,
  which apt never leaves while it trickles.

## Conventions

- Use American English in first-party identifiers, comments, UI text and
  documentation (`color`, for example). Preserve copied upstream text and
  names required by external APIs or file formats.
- Logical file paths use `ddi_library::path::{RelPath, RelPathBuf}`; convert to OS paths only with `to_path(base)`, never by joining their text onto a `PathBuf`.
- Commit messages: imperative summary line, body explaining why.
- Keep `docs/PLAN.md` status and decisions, and the plan's line in
  `docs/plans/README.md`, current when scope changes.
- UI text avoids jargon from specific games except where naming a ruleset
  or a layout by StepMania's own name for its game ("Pump single", from
  StepMania's `pump` game, `docs/plans/pump.md`).
- UI text is a message (`docs/plans/localization.md`, `locales/README.md`):
  write it in `locales/en/<area>.ftl` with a comment for the translator
  (where it shows, its room, each variable declared `$name (Number|String)`)
  and in every other language listed in `locales/languages.toml` in the
  same commit; use it as `ddi_l10n::msg::<area>::<name>(…)`. A new
  character (a kanji) the language's faces lack fails `just l10n-check`:
  run `cargo run -p xtask -- l10n fonts` and commit the cuts with the
  message. What is not ours to translate is `UiText::data`. Whole sentences only, data where
  nothing agrees with it, capitals written in the message. A translation
  an agent writes is a draft: it stays unverified until a review is
  recorded, and an agent never records a `human` review (an `agent` one
  only in a review pass asked for). Japanese: `locales/ja/GLOSSARY.md`.
  `crates/ui/src/screens/l10n_tests.rs` walks the screens in `qps-ploc`
  and fails on text written in code (a new screen or dialog joins its
  walk); `just l10n-check` on a language missing a message. The areas
  and the rules for a screen's text: `locales/README.md`, "Writing a
  screen's text". What the library, the importers, the session and the
  shells report reaches the screens as values worded there (an import's
  `ddi_library::report::Finding`, `screens::failures`, devices' facts),
  never as English sentences; a system's error goes through
  `screens::errors::error_text`. Logs and terminal output stay English.
