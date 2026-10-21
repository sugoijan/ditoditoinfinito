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

## Charts, layouts and play

- [Pump layouts](pump.md) (2026-10-13): done (ticked holds 2026-10-14,
  pump skins 2026-10-16). Later: center notes for the other bundled skins.
- [The play history](play-history.md) (2026-10-16): done, replays and the
  ghost included (decision 49). Not done: importing plays.
- [Additional charts without onigiri](chart-simplification.md)
  (2026-10-18): done. DanOni 5/7/9A/9B charts converted to Single, Solo,
  Double and Techno 8-panel, marked as assisted.
- [Song-select navigation](song-navigation.md) (2026-10-20): done; the
  reference for the tuned timings of taps, pairs and holds.

## Presentation and video

- [Video backgrounds](video-backgrounds.md) (2026-10-09): done on the web,
  steps 1–9. Movies through WebCodecs or the FFmpeg wasm module in a
  worker; desktop's in `native-video.md`.
- [Overlays](overlays.md) (2026-10-14): mostly done: DanOni's back and
  mask layers, `#BGCHANGES2` (decision 48). Not done: StepMania's
  script-free background effects, scripted backgrounds, `#FGCHANGES`.
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
  Windows (a hardware run, the helper's build), packaging and signing.

## Library and import

- [Desktop DanOni URL importer](danoni-importer.md) (2026-10-19*): done; on
  by default for desktop since 2026-10-21 (never for the web or Windows);
  the build, package and integrity contract of the native helper. Left (decision 51): Windows confinement, managed-store
  durability, browser URL acquisition.
- [Desktop DanOni URL imports](danoni-import.md) (2026-10-19*): the
  app/helper protocol, storage and verification as first planned; where it
  differs (the offline switch, the origin prompt), `danoni-importer.md` is
  current.
