# Plans

Each plan was written before its work and kept as the record of what was
built, how it was checked and what was left; `docs/PLAN.md` holds the
architecture, the decisions and the roadmap they belong to. Status as of
2026-10-22. Dates are each plan's own (* its first commit).

## CPU player

Seven pattern skill multipliers now work in automatic mode on desktop and web;
balanced preserves existing inputs. Presets/custom vectors require no profiles.

- [CPU player](cpu-player.md) (2026-11-10): implemented; automatic analytical
  preparation for imported/bundled keyboard 4K charts, seeded strength controls,
  shared input motor/judge, native and browser integration checks. An
  opponent's ten levels (2026-11-12): the five strengths and five weaker
  ones with lapses, measured over local arcade packs, provisional.

## Multiplayer design

- [Online play: the shared base](multiplayer-online-play.md) (2026-11-13):
  the contract for live online stages, built and tested in `crates/room`
  (`ddi_room::live`): one clock, one start, each competitor's moments and
  checkpoint reports gathered into one binary frame a tick, attacks decided
  by the room at deadlines from a battle plan that comes with the pick,
  settlement by the room; the fake room runs all of it. The server's and
  the game's fronts, the timing budget, the sizes and costs at 9, 33 and 73
  competitors, the views' delay (`live-v2`'s 100 ms ticks and a delay each
  device measures, half a second or less at typical latencies), and the
  open questions.
- [The audience](multiplayer-audience.md) (2026-11-13): watching a room
  beyond its members, counted and not listed: who is audience (a link of
  its own, offered at the member bound), the public view, relays against
  the room's own sockets and cached segments with the platform's numbers
  (relays recommended), four coalesced frames a second, the bounds and
  costs at 1,000 to 20,000 seats; the contract built and tested in
  `crates/room` (`ddi_room::audience`, `audience-v0`, the fake runs it);
  the server's and the game's work lists.
- [Multiplayer prototype progress](multiplayer-prototype-progress.md)
  (2026-11-10): what the prototype has actually built and tested, kept apart
  from the design studies below. M1: the shared room, match and stage model
  (`crates/room`). M2: the CPU-practice room on both shells behind a hidden
  switch (`#/?mp=1`, `--multiplayer on`), bound to the model's local
  authority, songs from the library through song select's pick mode. M3:
  stages played against engine-judged CPU opponents (a taps-only copy,
  provisional gauge survival), the intermission, the match's results and
  the next match. M4: the battle's announcer lines as their own category
  (`battle …`), spoken in a stage under a provisional voice policy, shown
  beside the field where no line said them; listed by the studio, no
  recordings. M5c: invite-only online rooms on both shells against the
  private server (Twitch sign-in, make, invite preview, join, the room
  bound to the server's authority through `ddi-room`'s online adapter,
  reconnects, roles). M5d: the host's song shared with the room through
  song sharing under the room's upload grant (`crates/share-protocol`, the
  contract the server shares), each member loading it from the share by
  its digest, Start as one command, each stage played on every member's
  device and ranked by the room from the players' own (self-reported)
  runs, to the match's results. M6 (first look): the CPUs' miniature
  fields in practice, drawn by the player's own skin from each CPU's
  stream of judged moments, with measured costs; M6b: their combo and
  life gauge as the play's HUD's, the field band's look, the hand behind
  the field, nine players, and a development-only practice far beyond the
  cap. M7a: the multiplayer home as the start menu's column with every
  planned entry (the unbuilt ones grayed), sign-in at the footer's right
  and asked for where needed, and the header and footer staying put from
  screen to screen everywhere. M7c: the hand larger behind the field, no
  heading beside the play (the battle's moment in a status line), the
  sides' whole height for the miniatures, and turns where not everyone
  fits. M7b: the maintainer's match flow: song select is the room (the
  host's pick starts or continues the match; the others follow the host's
  resting song, shared ahead), Rules first on entering, readiness on the
  stage's prompt (Ready, its countdown, the players; the stage starts once
  all are ready or at the deadline, the rest sit out), standings after
  each stage, a named CPU roster. M8a (the maintainer's attack decisions
  of 2026-11-11): the ruleset a match rule everyone plays (ITG FA+ by
  default), points shown as decimals, attack power earned by timing and
  shown above life (a checkpoint ledger in `ddi-room`; development only),
  and battle shocks judged beside the chart in the player's own play
  (`ddi_engine::battle`, safe sections per chart; a development trigger).
  M8b (development only): full bars launch eight-shock attacks at
  checkpoints by policy (Spread for everyone so far) from each target's
  lead, one at a time against anyone until they recover; CPUs receive
  shocks and answer them by level; miniatures show them; the incoming
  warning and the waiting line; settlement waits for the last attack.
  The DDI ruleset's round 3 (2026-11-12): rooms play DDI alone, with its
  Difficulty and Boos as match rules; attacks a match rule (Off by
  default, set in Rules, in CPU practice; not in online rooms yet); a
  shock a mine that locks its lane, placed by margins; ranking by life,
  then the proportional score; each opponent's score and locked lanes on
  its miniature; the ten CPU levels and the attack rates measured again
  under DDI; the reduced-motion stall of a stage's prompt found (the UI
  core's idle wake) and fixed.
  Round 3b (2026-11-12), after the maintainer's first test: the mostly
  black room after Stop match found (the web's history entries for
  dialogs) and fixed; the score a room shows and ranks by is the sum, a
  CPU's on the chart's scale (`CPU_SCORE_V0`); "Timing difficulty";
  shocks aimed beside the lane's notes (`placement-v2`); no recovery
  and chained attacks (`attacks-v1`); a battle's shocks drawn by a
  skin's `Tap Shock` (Infinito's shock arrows), a chart's mines as
  mines; the attack rates measured again.
  M8 round 3 (2026-11-12): attacks On by default where they are
  offered (CPU practice; the maintainer after testing them), still a
  rule of the match; each player's own attack policy (Spread,
  Retaliate, Leader) in the room's model, chosen on the stage's prompt,
  shown on the players' lines and changeable during the play (Tab, a
  controller's View, a tap on its label by the power bar), from the
  next checkpoint and kept in the play's log; who attacks whom for the
  player's own attacks (a line through the side space with a pulse, the
  opponent's miniature outlined while the attack lasts, a chained one
  quieter, the attacker and the target kept in view beyond the cap);
  the attack moments told in the status line (their two announcer lines
  not in the lines' table yet), and the stage's standings with attacks
  sent, taken and shocks stepped on a line and the player's own battle
  summed up.
  One picker at a time (2026-11-12): the room names who chooses its
  songs (its owner at first); only they see their own list, rest on
  songs for the room and pick, everyone else following; an operator
  takes the pick over at once with Pick songs and has their own list,
  place and search back; it goes to the owner (or nobody) when its
  holder leaves, loses the role or is gone too long; a room from before
  it chooses as it did. And the room's song select shows nothing of a
  view until the room says who the player is, then brings it in once,
  a change of hands one swap. A room's host (2026-11-12): what the
  room does when its host goes is a room setting ("If the host leaves":
  pass it on, to a co-host or whoever joined first, or close the room),
  applied when the host leaves and when they are away past the
  reconnect grace; the host's Host options in the Players pop-up (Make
  co-host, Remove co-host, Make host); and leaving asks first where it
  costs something (the host always; players, the picker and co-hosts
  during a match), from Back, More, the browser's Back and tab close,
  and the desktop window's close and Quit; CPU practice is never asked
  about, and a practice left with a match running is forgotten
  (2026-11-13). Watching a stage (2026-11-13, CPU practice first): a
  member chooses Play or Watch for themselves, a watching host still
  hosts, and where every person watches the CPUs play alone; a watched
  stage is a grid of everyone's fields over the whole screen, or one
  competitor followed (Tab, a press on a field) in the play's own
  field's place with the others beside it, drawn from the competitors'
  streams of judged moments, to results that name the stage's winner; a
  player out of a stage watches it on from their own dimmed field and
  can skip to the results, and leaving a stage's play forfeits it and
  comes to the same view.
  Provisional choices and open questions listed.
- [CPU practice and private-room prototype handoff](multiplayer-prototype-handoff.md)
  (2026-11-10): the maintainer authorized parallel implementation of these two
  flows while the original design lane continues. Separates selected constraints,
  working defaults and unresolved contracts; includes suggested slices, repository
  integration points and acceptance walkthroughs. The
  [copyable handover prompt](multiplayer-prototype-prompt.md) starts that work;
  [six archived mockup studies and seven logic checks](../../tools/multiplayer-design/README.md)
  preserve the reviewed examples. Documentation/snapshots only in this handoff,
  with no game/server implementation or claim of validated balance.
- [Multiplayer navigation and screens](multiplayer-ui.md) (2026-11-09): first
  menu tree and layout draft for discovery, hosted/automatic/local rooms,
  preparation, play/watch, intermission and match results. Separates room phase,
  participation, readiness and connection; includes compact/large-room behavior
  and announcer contexts. Twitch sign-in is selected for online identity, with
  return-to-room, cancellation and reauthentication states; no game-managed
  passwords/registration. Latest mockups fuse lobby, playlist and standings,
  reuse the song-select wheel for the host's entire local library, put Rules in
  a pop-up and model automatic sharing through the existing shared-song service.
  Includes the signed-out invite and return-to-room flow, personal stage
  evaluation, and a proposed host-triggered readiness deadline that makes
  unready participants sit out a stage instead of stalling it. Updated mockups
  include automatic advancement with a disconnected host, watching co-hosts,
  admin assistance without a playing slot, and owner-only controls. Automatic
  ownership transfer defaults Off and requires acceptance when enabled.
  Admins can watch or request a playing seat; holds and readiness remain separate
  from ownership. The play/watch study adds a single incoming warning, separate
  automatic power indicator, faithful opponent miniatures around normal gameplay,
  elimination-to-spectator transition, winner completion and a 128-player layout
  study. Miniatures preserve the followed player's display settings and enlarge
  into the spectator field; transport and large-party density remain proposals.
  The quieter HUD study puts power above life in every field, makes score optional,
  reduces incoming cues and compares the hand behind the note band. Electrical
  attack examples add committed-target curves, a traveling shock-arrow pulse,
  recipient highlights and a reduced-motion treatment.
  Ending examples distinguish pending hazards from confirmed victory, let the
  survivor finish, show stage-by-stage match points and shared ranks, and return
  to the same room with fresh readiness and waiting-player admission. Historical
  results remain available without blocking the next match's start.
  Reconnect examples add stale remote previews, conditional same-stage recovery,
  confirmed disconnect forfeits, watching and next-stage return, missed starts,
  same-account Twitch recovery and unavailable/interrupted room distinctions.
  Reconnect timing and attack-history validation remain provisional.
  Entry examples connect the multiplayer home to public-room discovery,
  automatic search/gathering with CPU fill, and local CPU practice. They include
  admission rechecks after sign-in, loading deadlines, no-human-ready behavior,
  offline song/CPU setup and a representative stage start; no services or bot
  simulation are implemented.
  The 2026-11-10 room-settings study adds creation, public/invite-only access,
  CPU seat/profile configuration, locked current-match rules and explicit
  next-match drafts. It validates incompatible rules and capacity, preserves
  readiness for access/future changes, and rechecks saved proposals at the match
  boundary. All changes remain design mockups.
  Proposed layouts and local mock interactions only;
  no multiplayer implementation.
- [Multiplayer battle](multiplayer.md) (2026-11-08): provisional experience
  design; proposed Room → Match → Stage vocabulary. Persistent rooms run finite
  matches of one or more songs (four proposed by default), with overall rankings.
  New players wait for the next match unless the host enables late entry, which
  earns nothing for missed stages and gets no compensation. Stage resets by
  default, optional persistent elimination, public/invite-only hosted rooms,
  automatic public rooms with CPU fill and local CPU practice. Eight participants initially, 100+ as a design
  target. Automatic mine attacks, non-spending accuracy-based weakening, recovery,
  winner song completion and distinct stage/match announcer events. A
  [placement-points study](../research/multiplayer-scoring.md) compares stage wins
  with consistent high placements; moderate podium bonuses are the preferred
  provisional default. Mixed-skill exploration now focuses on adaptation without
  prior skill knowledge: chart switching, a protected opening and bounded attack
  earning; manual handicaps and teams alone do not meet that launch constraint
  (2026-11-09). Design proceeds
  while the simulator develops independently; no validated battle default yet.
  CPU/private-room prototype implementation was authorized separately on
  2026-11-10 through the handoff above. Builds on the [exploration](../research/multiplayer.md), reference
  comparisons and the six [simulation scenarios](../research/multiplayer-simulations.md).

## Noteskins and Lua

- [Noteskins](noteskins.md) (2026-10-10): done; Lua and models in their own
  plans. Left: OutFox's `global/` skins, per-game extras. StepMania
  noteskins drawn as StepMania draws them: lookup, metrics, actors, events,
  import.
- [Running noteskins' Lua safely](lua.md) (2026-10-11): done 2026-10-12,
  with presentation extensions 2026-10-20 (`HoldUseDroppedState`,
  `HitX`/`HitY`, `TapJudgmentsPerNote`). Left: native ops for simple
  commands, scripted backgrounds, modfiles.
- [Hardening a fork of piccolo](piccolo-fork.md) (2026-10-11): done;
  published as `lunita` on crates.io. The brief and definition of done for
  the sandboxed VM.
- [Bundled noteskins](bundled-skins.md) (2026-10-12): done, with Peter
  Lawrence's skins (2026-10-12/13) and `DanOni` (2026-10-20) added since;
  Peters-DDR Note/Vivid combined with color and explosion options, and a
  raster center, derived holds/rolls and layered mines added (2026-10-22).
  The SVG skin developed as Spiral Base, now Infinito, replaces Dito as the default
  (2026-10-25); its original color variants and procedural controls are
  retired. Procedural arrows remain an internal fallback, drawn as
  Infinito's shapes made light (PLAN.md decision 80, 2026-11-03).
- [3D models](models.md) (2026-10-12): done 2026-10-13. Later: a depth
  buffer, lighting. MilkShape meshes, materials and bones.
- [StepMania 3.9 and OpenITG noteskins](legacy-skins.md) (2026-10-13):
  done 2026-10-13. Later: NotITG's Lua mods if a skin needs them.
- [Noteskin previews](skin-preview.md) (2026-10-13): done; Options'
  preview is now the UI core's field preview (`ui-core.md` 6.3). Later: a
  preview per skin, lifts and fakes in the demo. `#/skin-preview` and the
  demo chart.
- [Noteskins on Dancing☆Onigiri layouts](danoni-skins.md) (2026-10-13):
  done 2026-10-13. Skins draw every built-in layout, the onigiri lane
  included.
- [Noteskin authoring with SkinCraft](noteskin-authoring.md) (2026-10-20*):
  reference. SkinCraft lives in its own repository; this covers the shared
  crates it pins and how to test API changes against it.
- [Skin parameters](skin-parameters.md) (2026-10-21): done. Skins declare
  controls in their metrics (our extension), kept per skin, read by
  their Lua and shown under Arrows. Infinito declares Vivid (default),
  Rainbow and Flat colors, optional per-note charge timing, onigiri, mine shape, receptor fill and hit-effect
  controls (2026-10-25); its original asset variants and procedural controls
  are retired.
- [Noteskin SVG](noteskin-svg.md) (2026-10-24): in progress. One
  rasterizer on both shells (a worker on the web), vello_cpu measured,
  what images cost shown, bounded filters, raster quality apart from
  logical size; its material steps taken over by the components plan.
- [Noteskin components and materials](noteskin-scene.md) (2026-10-25):
  in progress; step 1 done (materials as shaders: long notes about 30
  times fewer quads), 2 done (components and bindings; SkinCraft's view of a part's bindings left), 3 done (a
  skin's own materials: Infinito's gradient as one), 4 done (long notes and
  lifts), 5 done (explosions as instances). Infinito's taps/fakes, receptors,
  holds, rolls, lifts and judgment effects are ported; its SVG, WGSL and Lua
  sources are bundled in `assets/noteskins/dance/Infinito/`.
  SVG components with named parts posed per note
  from the engine's clocks, materials as the skin's own bounded WGSL
  shaders; takes over the SVG plan's material and material-part
  metadata steps.
- [Infinito's wave as its own material](infinito-wave-material.md)
  (2026-10-30): done 2026-10-30. Infinito already draws its holds, rolls and
  lifts with its own WGSL; the unused `*Material*` metric family, the
  built-in wave shader and the stencil hold paths only they reach go.

## Charts, layouts and play

- [The DDI ruleset](ddi-ruleset.md) (2026-11-12): rounds 1 (the
  engine), 2 (the screens: Options' rows, the default with the move
  from ITG, the words and score in play, the results, lamps and bests,
  kept plays judged again), 3 (multiplayer: rooms play it alone, its
  Difficulty and Boos match rules, a battle's shock a mine that locks
  its lane) and 3b (the maintainer's first test: the summed score in
  rooms with a CPU's on the chart's scale, the label "Timing
  difficulty", `placement-v2`, `attacks-v1` without recovery and with
  chains, a battle's shocks drawn by a skin's `Tap Shock`) built
  2026-11-12, and 4 (2026-11-13: one set of tier colors, white to red,
  for words and hit effects; the judgment words in the player's
  language; shock avoidance a CPU's eighth skill axis); left: the
  targeting choice, online attacks. Our own
  ruleset, the only one in multiplayer and the default for normal play:
  W0–W5 on a 1/120 s grid (1/144 Hard, 1/90 Easy) with early and late
  words, FFR-style life and Boos (off by default), a 1,000,000 score or
  percent with an absolute score beside it, holds credited by whole
  seconds, mines that lock their lane, grades to Absolute Dito with
  floors, DDR-style full-combo lamps with three FFR-style flags.
- [Announcers](announcers.md) (2026-11-03): done 2026-11-03 (decision
  81). StepMania-family announcer packs imported on both shells, chosen
  in Options with a voice bus of their own, and spoken where StepMania
  5.1 speaks them (in play, on the results, on song select), Etterna's
  two names included; a few lines of our own in the same layout,
  documented for pack authors in `docs/announcer-lines.md`. Packs of our
  own made with Kokoro, `Mr. Clank` and `ガキンさん` (every line, from
  `tools/announcer-design/pack.toml` and `pack-ja.toml`), bundled
  2026-11-06; the announcer is Off by default. Louder since 2026-11-12:
  made through the announcer studio's tidy, the one leveler, whose
  chain (`announcer-level-v3`: a phase rotator, a compressor, a limiter
  that has little left to do) brings them to about 3 LU under the
  music's loudness, the loudest it reaches smoothly (provisional until
  listened to; an audition set of levels is made by `kokoro.py
  loudness`).
- [Announcer studio](announcer-studio.md) (2026-11-06): done (the draft;
  the web's page at `#/announcer`; the scenes on song select and the
  results, 2026-11-07; the scenes in play, desktop and the encoder,
  2026-11-08);
  web first, for authors who only play the game. They record with their own
  app; the recordings are picked or dropped onto their lines, with
  examples in English and Japanese, tidied, and each line heard in a
  staged scene of the game (song select, a play, the results). The pack
  is exported as a zip that Add songs imports as it is: tidied takes in
  Opus, the browser's encoder first and `rusty-opus` behind it;
  StepMania's formats only under an Advanced checkbox, off by default. Desktop follows with its
  pickers, no drop.
- [Play HUD](play-hud.md) (2026-10-29): done; accepted study ported
  to the shared renderer and checked offscreen. Life trails, Iosevka type,
  combo tiers and still title watermark. The life gauge four lanes wide
  over the field and filling from the middle out, on a curve, since
  2026-11-06.

- [Pump layouts](pump.md) (2026-10-13): done (ticked holds 2026-10-14,
  pump skins 2026-10-16, every bundled skin's own center 2026-10-22).
- [The play history](play-history.md) (2026-10-16): done, replays and the
  ghost included (decision 49); importing plays in `phase-10.md`.
- [Phase 10](phase-10.md) (2026-10-21): importing plays done; FFR
  levels (SWF and text `beatBox`) imported on both shells, played by
  FFR's rules (`ffr`, in `rulesets.md`) where each chart is left to its
  game, timed as R^3 times them (its half frame and music start,
  2026-10-22). Left: their music against a real level's.
- [The chart editor](editor.md) (2026-10-21): in progress. The
  `.ssc`/`.sm` writer, notes and timing with undo, the screen on both
  shells (song select's E, `#/edit`), the field in the player's skin
  (an edit and a play view), listening with the metronome, tap-to-tempo
  and the music's overview, playtests, saving and the autosave, new
  songs from a music file done.
- [Additional chart versions](chart-simplification.md)
  (2026-10-18): done. DanOni 5/7/9A/9B charts converted to Single, Solo,
  Double and Techno 8-panel, marked as assisted; pump charts without the
  center, turned onto Single and Double, on by default (2026-10-24);
  by default only where a style lacks the difficulty, marked in song
  select (2026-10-25); pump single and 5 keys converted to each other,
  on by default, marked but not assisted (2026-10-25); At once, a play
  option replacing No jumps, which can keep jumps and leave out hands
  (2026-10-26); No stretch, a saved play option for all four double-mat
  layouts with hold-aware span filtering (2026-11-07).
- [Song-select navigation](song-navigation.md) (2026-10-20): done; the
  reference for the tuned timings of taps, pairs and holds, and for the
  search field from a controller (2026-10-22).
- [Smaller chart and import format gaps](format-gaps.md) (2026-10-21):
  done: `.lrc` lyrics, jackets and CD titles, per-chart and preview
  music, background names with `,` or `=`, Korean and Chinese text.
  `#PREVIEWVID` shows in song select since 2026-10-22 (`lane-covers.md`).
  Zips in Deflate64, bzip2, LZMA and Zstandard since 2026-10-22.
- [Rules and options](rules-options.md) (2026-10-21): done (decision 55):
  StepMania's fail types, life and judge difficulty, ITG FA+, autoplay
  in the menus (found done), the event sounds, the battery.
- [More rulesets](rulesets.md) (2026-10-21): done (decision 57): DDR
  EXTREME's score, grades and windows, DDR A's LIFE4, RISKY, FLARE and
  FLOATING FLARE gauges with the FLARE rank and FLARE SKILL, LIFE4's
  lamps; `ffr` (2026-10-22, decision 58): R^3's windows, miss, Boos, raw
  and total score, life and flags; `ddr-x` (2026-10-22, decision 59):
  DDR X, SuperNOVA2's score, grades by the chart's difficulty.
- [Lane covers and the preview video](lane-covers.md) (2026-10-21):
  lane covers (Sudden+, Hidden+, both) done: drawn over the lanes,
  moved and hidden in play, kept in the play's log for replays.
  `#PREVIEWVID` imported, kept, and since 2026-10-22 played in song
  select's banner place on both shells.

## Presentation and video

- [Dancing hand in gameplay](dancing-hand.md): production extraction with three-second single-threaded planning for four-panel, dance-double eight-panel, Solo six-panel, DanOni five-key, seven-key, 9A and 9B, Pump single/half-double/double and Techno single8 charts (9B uses all nine tiles and air-time hazard clearance; the pre-rebase 2026-11-06 audit records 7,776 passes, 21 contact failures, three pose failures and 78 pose CPU timeouts; targeted Pump pose repairs clear those three original errors, with RUDE! still blocked by a later baseline contact rejection), accepted rendering, startup readiness, layout options and basic tests. Research/audit/developer tools remain on the hand branch.
- [Double mat for the remaining hand layouts](hand-double-mat.md) (2026-11-07): No stretch preparation and the general Options toggle are implemented for dance-double, Pump half-double/double and DanOni 9A (either center can stand for Oni). Shared physical geometry and versioned offline pose plans are implemented. The bounded reach survey fits every No Stretch pair at 24 mm, with checked slow crossing, held and wide-jump studies. Explicit-geometry startup fixes the original floor failure without changing single-mat motion; periodic world-yaw smoothing/interpolation now clears all 12 variants natively and in headless Chrome/WASM. Approach-aware shared spots now smooth the rapid entry while preserving published contacts (61.35° → 2.21° peak shared-entry step at 120 Hz); broader choreography still needs review. Dance-double now has automatic three-second rolling contact planning and actual-judge replay; 12 automatic/mirrored studies pass native and headless WASM pose checks, with existing-layout plans/publications bit-identical. The 18-tile renderer and dance-double gameplay are implemented (2026-11-08), including preload/restart, Options preview, contact lights/presses/spill and geometry-aware framing; native off-screen GPU and headless WebGPU/WebGL2 checks pass. Pump half-double and double now also use this gameplay path (2026-11-08), with scored centers, bounded side escapes, reachable lift returns and 32 native/WASM motion cases. Eight prior layouts' plans and 584 rolling publications remain bit-identical. DanOni 9A now has per-contact Oni center selection, logical contact aggregation,
  both-center hazards and unscored-corner escapes with unchanged timing/model
  bounds. The 2026-11-08 full audit is complete: 11,675 passes / 66 contact
  failures / 70 Double pose failures / 55 pose CPU timeouts / 234 unsupported
  unplayable layouts; zero contact/wall timeouts, fatal deadlines or settling
  warnings. All 7,776 former passes remain passes. Five passing records have
  nonfatal guide warnings; see the gameplay plan for source coverage and evidence.
  The initial targeted Double pose-repair rerun had 67 full passes and three
  later baseline contact rejections. Mine-to-head/staggered-clearance follow-up
  now passes all 70 records with no failures, timeouts or settling warnings;
  126 native tests, the 79-category gate and ten headless WASM motion cases pass.
  The original 66-contact rerun had 43 passes and 23 rejections. Lift/support
  repairs then passed the four RUDE! versions and Silent Hill; rerunning the 18
  unresolved records now gives six more passes and 12 contact rejections, with
  zero pose failures, timeouts or warnings. Timed mine arrival/departure repair
  then passes five affected records (1116, 人マニア and 混乱少女 Single/Double),
  preserving 14 control contact plans; 132 native tests and all 79 category
  expectations pass. The subsequent full rerun (2026-11-09) gives **11,791
  passes / 9 contact rejections / 0 pose failures / 66 pose CPU timeouts / 234
  unsupported records / 0 settling warnings**, with two nonfatal guide warnings.
  All 70 former pose failures and 61 former contacts pass. A subsequent generic
  guarded-departure validation fix clears the four new G-SYMPHONiCORE Single
  regressions: 35/35 targeted charts, 133 native tests and all 79 category
  expectations pass; 31 control contact plans and the affected checked prefixes
  remain identical. The full audit was not rerun; its twelve new CPU timeouts
  remain follow-ups. Published model/contact limits are unchanged. The gameplay
  plan records coverage, independent review and the four-row RUDE! inspector
  lift-count metadata erratum; private inputs and tools remain uncommitted.
  Staggered-landing follow-up then clears KEEP ON MOVIN' Double, Megalovania
  Single (SM/SSC) and converted UNIQUE (SM/SSC): **41/41 targeted charts**, 136
  native tests and all 79 category expectations pass. A final gated recovery tier
  reserves scored companion landings; a narrow, source-proven post-mine approach
  handles near-coincident timing. All 36 control contact plans and five affected
  checked prefixes remain identical. Contact/model bounds and the 450 ms outward
  limit are unchanged; no full audit was rerun. The synthetic empty-tail
  deadline was traced to the harness using note completion as motion completion;
  gameplay already handles that correctly. Corrected full-tail tests cover both
  mirrors at 4/60 Hz, with 137 native tests passing and no production changes.
  Prepared as one unsigned gameplay update atop local `main` `9b67a48d`, keeping
  its refactored APIs and shared supported-layout list. Consolidation passes
  1,208 native tests (two existing ignored), strict clippy, full-app WASM check
  and the off-screen consecutive-play desktop lifecycle test. No new full audit.


- [Video backgrounds](video-backgrounds.md) (2026-10-09): done on the web,
  steps 1–9. Movies through WebCodecs or the FFmpeg wasm module in a
  worker; desktop's in `native-video.md`.
- [Overlays](overlays.md) (2026-10-14): done: DanOni's back and mask
  layers, `#BGCHANGES2` (decision 48), StepMania's named background
  effects and transitions (2026-10-21); the field filter drawn only
  over a background (2026-10-24). Scripted backgrounds and
  `#FGCHANGES` in their own plan.
- [Scripted backgrounds and `#FGCHANGES`](scripted-backgrounds.md)
  (2026-10-21): first slice done (folders with a `default.lua`, behind
  the field and as foregrounds, on the skins' sandboxed Lua host);
  StepMania 3.9's and ITG's formats drawn natively, render targets
  (2026-10-22). Left: movies.
- [Phase 8's last gaps](phase-8-gaps.md) (2026-11-03): in progress, steps
  1 (random movies), 2 (a movie as an effect's second file) and 3 (a
  script's movie sprite) done 2026-11-03; step 4's study
  (`tools/background-design/`) chosen from and step 5 (generative
  backgrounds in play: four looks, the player's choice) done 2026-11-05;
  step 6 (documentation) left. Movies
  wherever StepMania 5.1 plays them (random movies for songs without
  changes and `-random-`, an effect's second file, a script's sprite),
  then generative backgrounds of our own, driven by the music alone, on
  by default in place of a missing song background (never for a
  Dancing☆Onigiri work) and optionally over a picture, the
  visualizations chosen from a round of experiments.
- [OS and native video decoding on desktop](native-video.md) (2026-10-17):
  in progress. Done: VideoToolbox for checked H.264/HEVC/VP9, opt-in Metal
  planes, the sandboxed FFmpeg helper (decision 54). Left: Media Foundation
  and the helper on Windows hardware, Linux VA-API, wider stream classes.

## UI and desktop

- [Localization](localization.md) (2026-11-07): steps 1 to 5 done (the
  groundwork; Noto Sans CJK JP's cuts on both shells, the font stack by
  language, Unicode line breaking; every screen's text a message, and
  what the rest of the game reports values the screens word; the edges:
  the web's loader and plain pages, desktop's app menu and dialogs, the
  packages' entries, the release's gate; a Japanese draft awaiting
  review). The UI in
  Japanese as well as English, more languages later: Fluent messages
  with typed functions, every text a message or marked data (a
  pseudo-language test over every screen), every maintained language
  written with each change and unverified until a review is recorded
  against its hashes, a coverage check before releases; Noto Sans CJK JP
  and Unicode line breaking for Japanese.
- [System errors as values](system-errors.md) (2026-11-08): planned. A
  system's failure carried with its kind (an `io::ErrorKind`, a
  DOMException's name, an HTTP status) instead of as text the screens
  guess the kind from again; the text guess kept for stored records.
- [Controller hints and reachability](controller-hints.md) (2026-10-27):
  implemented: automatic Xbox, PlayStation and Nintendo hints, generic fallback,
  per-controller artwork overrides and LB/LT/RT/RB primary Dance Single defaults;
  saved bindings remain intact. Physical hardware coverage is still unverified.
  Earlier reachability fixes include:
  More actions, shoulder navigation, results focus, binding capture and editor
  controls. Free-text typing is exempt; numeric editing is controller-operable.
  No virtual keyboard; dance-pad hint profiles deferred; browser/OS boundaries
  tracked separately. [Audit](../research/controller-reachability.md).
- [Controller input in the UI core](controller-input-core.md) (2026-10-30):
  done. The `Ui` owns the bindings, the connected pads and the hint
  source, takes pads' edges as they are and releases what each held
  control pressed; no thread-local, no revision polling, no settings side
  effect.
- [The start prompt, loading in the background](start-prompt.md)
  (2026-10-24): done (decision 61). Start never waits for the noteskin
  or the rest (a loading screen after an early Start), the countdown to
  Start on song select's plays, this play's options on the prompt (Speed,
  Reverse, Turn, Appearance, Lane cover, the noteskin; Save as default).
  Controls preview added 2026-10-27: the chart's receptors and primary
  bindings above the modifiers, a brief looping press demo, with auto-start
  taking priority.
- [Start menu and returning between screens](start-menu.md) (2026-10-20):
  implemented 2026-10-21. Singleplayer entry, a future Multiplayer hint, caller-relative
  Options, silent return from song select and desktop Quit. Animated Dosis
  wordmark added 2026-10-25 (Vivid outlines, reduced-motion still).
- [Our own UI core](ui-core.md) (2026-10-15): done 2026-10-16 (step 7
  through `desktop.md`). Later: StepMania themes, accessibility.
- [Our theme, first version](theme.md) (2026-10-15): done, refined since.
  The Infinito look; one header on every screen, a Back button and the
  live wordmark beside the screen's name; the menus' parts moving in from
  their edges, screens following each other on both shells (2026-10-26).
  Original OEM-inspired shortcut caps were added the same day, with retained
  A1/A2 charcoal SVG alternatives and pressed 3D references.
  The play HUD study was accepted 2026-10-29, retaining Iosevka; its
  shared renderer port is tracked in [Play HUD](play-hud.md).
- [Desktop parity](desktop.md) (2026-10-16): in progress. Steps 7.0–7.9
  done; scripted backgrounds readied off the event loop (2026-10-22).
  Late pacing falls back to blocking where it keeps missing (fullscreen,
  measured, simulated and rerun 2026-10-22: late·1 falls back, late·2
  halves its misses without); a whole fullscreen song of late·2 fell back
  on a fullscreen transition's stall, which no longer counts; left: that
  run again, Windows (a hardware run, the helper's build). One app a data
  folder (2026-11-12): the app takes an OS file lock on its configuration
  folder and a second one does not start; the command-line utilities
  take none and write no settings; left: the dialog seen by a person,
  Windows and Linux run.
- [Packaging and releases](packaging.md) (2026-10-21): done; the release
  workflow's first run waits for a tag. An ad hoc signed `.app` (zip,
  `.dmg`), a Linux tarball and a Windows zip, built for draft releases
  from tags; window icons, the Windows executable's icon and version, the
  console for flagged runs. Left: notarization, Intel Macs, Windows'
  video helper, the Windows parts checked on Windows.
- [UI core mechanics](ui-core-mechanics.md) (2026-10-30): done 2026-10-30.
  Screens told when another screen covers or uncovers them (dialogs do
  not), replacing desktop's layer-name checks; every screen taking input
  while it enters, the prompt's fade an ordinary entrance; hover picked
  again only when something changed; the web's history state written
  only when it changes.
- [Shell sharing](shell-sharing.md) (2026-10-30): done. One field
  preview skin state in `ddi_shell` for both shells; skin loads sharing
  their drawings and parsed components, one load of a skin at a time, so
  the web's play stops polling and sleeping before its skin; the prompt's
  info and filter band built once; one owner of the web canvas's pointer.

## Library and import

- [Desktop DanOni URL importer](danoni-importer.md) (2026-10-19*): done; on
  by default for desktop since 2026-10-21 (never for the web or Windows);
  the build, package and integrity contract of the native helper. Left (decision 51): Windows confinement, managed-store
  durability, browser URL acquisition.
- [Shared songs by link](shared-songs.md) (2026-10-29): planned. A
  short link loads a song or a pack from a private Cloudflare Worker to
  play without entering the library, with an expiry; a hidden,
  token-authenticated uploader in the app; the contract both repositories
  keep (one crate since 2026-11-10, `crates/share-protocol`), and online
  rooms' shares made under a room's upload grant (M5d).
- [Quiet calibration](quiet-calibration.md) (2026-10-29): done
  (decision 76), the thresholds to tune by playing. The
  device banners, the devices line, the prompt's new-device step and the
  calibration outcome card go; new devices get the defaults silently; one
  muted line on the prompt suggests calibrating, kept or dropped by what
  the play history shows of the timing on the current devices.
- [Desktop DanOni URL imports](danoni-import.md) (2026-10-19*): the
  app/helper protocol, storage and verification as first planned; where it
  differs (the offline switch, the origin prompt), `danoni-importer.md` is
  current.
