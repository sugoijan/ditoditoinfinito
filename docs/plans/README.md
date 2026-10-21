# Plans

Each plan was written before its work and kept as the record of what was
built, how it was checked and what was left; `docs/PLAN.md` holds the
architecture, the decisions and the roadmap they belong to. Status as of
2026-10-21. Dates are each plan's own (* its first commit).

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
  Lawrence's skins (2026-10-12/13) and `DanOni` (2026-10-20) added since.
  Freely licensed skins shipped with the game; Dito, the built-in arrows
  as a Lua skin, is the default.
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
  their Lua and shown under Arrows; Dito's colors, songs' own colors,
  explosions and the lighter drawing (the Built-in choice retired into
  it) are its parameters.

## Charts, layouts and play

- [Pump layouts](pump.md) (2026-10-13): done (ticked holds 2026-10-14,
  pump skins 2026-10-16). Later: center notes for the other bundled skins.
- [The play history](play-history.md) (2026-10-16): done, replays and the
  ghost included (decision 49); importing plays in `phase-10.md`.
- [Phase 10](phase-10.md) (2026-10-21): importing plays done; FFR
  levels (SWF and text `beatBox`) imported on both shells, their rules
  waiting for the rulesets. Rulesets later.
- [The chart editor](editor.md) (2026-10-21): planned, not started. A
  first version for `.ssc`/`.sm` timing and notes, playtests, saving on
  both shells.
- [Additional charts without onigiri](chart-simplification.md)
  (2026-10-18): done. DanOni 5/7/9A/9B charts converted to Single, Solo,
  Double and Techno 8-panel, marked as assisted.
- [Song-select navigation](song-navigation.md) (2026-10-20): done; the
  reference for the tuned timings of taps, pairs and holds.
- [Smaller chart and import format gaps](format-gaps.md) (2026-10-21):
  done: `.lrc` lyrics, jackets and CD titles, per-chart and preview
  music, background names with `,` or `=`, Korean and Chinese text.
  Left: `#PREVIEWVID`, zips with other compression methods.
- [Rules and options](rules-options.md) (2026-10-21): done (decision 55):
  StepMania's fail types, life and judge difficulty, ITG FA+, autoplay
  in the menus (found done), the event sounds, the battery.
- [More rulesets](rulesets.md) (2026-10-21): done (decision 57): DDR
  EXTREME's score, grades and windows, DDR A's LIFE4, RISKY, FLARE and
  FLOATING FLARE gauges with the FLARE rank and FLARE SKILL, LIFE4's
  lamps. Left: SuperNOVA2/X, `ffr` (with the FFR import).
- [Lane covers and the preview video](lane-covers.md) (2026-10-21):
  lane covers (Sudden+, Hidden+, both) done: drawn over the lanes,
  moved and hidden in play, kept in the play's log for replays.

## Presentation and video

- [Video backgrounds](video-backgrounds.md) (2026-10-09): done on the web,
  steps 1–9. Movies through WebCodecs or the FFmpeg wasm module in a
  worker; desktop's in `native-video.md`.
- [Overlays](overlays.md) (2026-10-14): done: DanOni's back and mask
  layers, `#BGCHANGES2` (decision 48), StepMania's named background
  effects and transitions (2026-10-21). Scripted backgrounds and
  `#FGCHANGES` in their own plan.
- [Scripted backgrounds and `#FGCHANGES`](scripted-backgrounds.md)
  (2026-10-21): first slice done (folders with a `default.lua`, behind
  the field and as foregrounds, on the skins' sandboxed Lua host). Left:
  movies, render targets, StepMania 3.9's and ITG's formats.
- [OS and native video decoding on desktop](native-video.md) (2026-10-17):
  in progress. Done: VideoToolbox for checked H.264/HEVC/VP9, opt-in Metal
  planes, the sandboxed FFmpeg helper (decision 54). Left: Media Foundation
  and the helper on Windows hardware, Linux VA-API, wider stream classes.

## UI and desktop

- [Start menu and returning between screens](start-menu.md) (2026-10-20):
  implemented 2026-10-21. Singleplayer entry, a future Multiplayer hint, caller-relative
  Options, silent return from song select and desktop Quit.
- [Our own UI core](ui-core.md) (2026-10-15): done 2026-10-16 (step 7
  through `desktop.md`). Later: StepMania themes, accessibility.
- [Our theme, first version](theme.md) (2026-10-15): done, refined since.
  The Infinito look.
- [Desktop parity](desktop.md) (2026-10-16): in progress. Steps 7.0–7.9
  done. Left: the maintainer's hardware checks, fullscreen frame pacing,
  Windows (a hardware run, the helper's build).
- [Packaging and releases](packaging.md) (2026-10-21): done; the release
  workflow's first run waits for a tag. An ad hoc signed `.app` (zip,
  `.dmg`), a Linux tarball and a Windows zip, built for draft releases
  from tags. Left: notarization, Intel Macs, Windows' video helper and icon.

## Library and import

- [Desktop DanOni URL importer](danoni-importer.md) (2026-10-19*): done; on
  by default for desktop since 2026-10-21 (never for the web or Windows);
  the build, package and integrity contract of the native helper. Left (decision 51): Windows confinement, managed-store
  durability, browser URL acquisition.
- [Desktop DanOni URL imports](danoni-import.md) (2026-10-19*): the
  app/helper protocol, storage and verification as first planned; where it
  differs (the offline switch, the origin prompt), `danoni-importer.md` is
  current.
