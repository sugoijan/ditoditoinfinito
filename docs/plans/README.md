# Plans

Each plan was written before its work and kept as the record of what was
built, how it was checked and what was left; `docs/PLAN.md` holds the
architecture, the decisions and the roadmap they belong to. Status as of
2026-10-22. Dates are each plan's own (* its first commit).

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
  retired. Procedural arrows remain an internal fallback.
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

## Charts, layouts and play

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
  (2026-10-26).
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
- [OS and native video decoding on desktop](native-video.md) (2026-10-17):
  in progress. Done: VideoToolbox for checked H.264/HEVC/VP9, opt-in Metal
  planes, the sandboxed FFmpeg helper (decision 54). Left: Media Foundation
  and the helper on Windows hardware, Linux VA-API, wider stream classes.

## UI and desktop

- [Controller hints and reachability](controller-hints.md) (2026-10-27):
  implemented: automatic Xbox, PlayStation and Nintendo hints, generic fallback,
  per-controller artwork overrides and LB/LT/RT/RB primary Dance Single defaults;
  saved bindings remain intact. Physical hardware coverage is still unverified.
  Earlier reachability fixes include:
  More actions, shoulder navigation, results focus, binding capture and editor
  controls. Free-text typing is exempt; numeric editing is controller-operable.
  No virtual keyboard; dance-pad hint profiles deferred; browser/OS boundaries
  tracked separately. [Audit](../research/controller-reachability.md).
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
- [Desktop parity](desktop.md) (2026-10-16): in progress. Steps 7.0–7.9
  done; scripted backgrounds readied off the event loop (2026-10-22).
  Late pacing falls back to blocking where it keeps missing (fullscreen,
  measured, simulated and rerun 2026-10-22: late·1 falls back, late·2
  halves its misses without); a whole fullscreen song of late·2 fell back
  on a fullscreen transition's stall, which no longer counts; left: that
  run again, Windows (a hardware run, the helper's build).
- [Packaging and releases](packaging.md) (2026-10-21): done; the release
  workflow's first run waits for a tag. An ad hoc signed `.app` (zip,
  `.dmg`), a Linux tarball and a Windows zip, built for draft releases
  from tags; window icons, the Windows executable's icon and version, the
  console for flagged runs. Left: notarization, Intel Macs, Windows'
  video helper, the Windows parts checked on Windows.

## Library and import

- [Desktop DanOni URL importer](danoni-importer.md) (2026-10-19*): done; on
  by default for desktop since 2026-10-21 (never for the web or Windows);
  the build, package and integrity contract of the native helper. Left (decision 51): Windows confinement, managed-store
  durability, browser URL acquisition.
- [Quiet calibration](quiet-calibration.md) (2026-10-29): step 1 done
  (decision 76), the suggestion from the play history next. The
  device banners, the devices line, the prompt's new-device step and the
  calibration outcome card go; new devices get the defaults silently; one
  muted line on the prompt suggests calibrating, kept or dropped by what
  the play history shows of the timing on the current devices.
- [Desktop DanOni URL imports](danoni-import.md) (2026-10-19*): the
  app/helper protocol, storage and verification as first planned; where it
  differs (the offline switch, the origin prompt), `danoni-importer.md` is
  current.
