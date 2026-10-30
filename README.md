<h1><picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/logo.svg">
  <img alt="Dito Dito Infinito" src="docs/logo-light.svg" width="640">
</picture></h1>

A dance rhythm game (four arrows, hit them in time with the music) that plays
StepMania and DWI simfiles: a few bundled songs, plus any packs you import
(folder, zip or drag and drop; they stay in your browser). Live at
`https://sugoijan.dev/ditoditoinfinito/`.

![Beatucada's Hard single chart with Dito arrows over its beach background, autoplayed in a short loop.](docs/screenshots/readme.gif)

*Beatucada* by Kurio Prokos · chart by Chriszo · background by Jose Varela.
[Generated from a fixed gameplay scenario](docs/screenshots/README.md).
[Full-size still](docs/screenshots/readme.png).

Implementation notes: written in Rust; the same game runs in the browser (wasm
through Trunk, on GitHub Pages) and as a desktop app (`cargo run`), which reads
your song folders and zips where they are.

Status: early development. See [`docs/PLAN.md`](docs/PLAN.md) for the
architecture, MVP scope and roadmap, and [`docs/research/`](docs/research/)
for the format, rules and tech-stack research with sources.

## Develop and build

Prerequisites:

- Rust (stable) with the wasm target: `rustup target add wasm32-unknown-unknown`
- [Trunk](https://trunkrs.dev/): `cargo install trunk --locked`
- [just](https://github.com/casey/just) (task runner)
- [uv](https://docs.astral.sh/uv/) (runs the `reuse` linter via `uvx`)

Tasks (`just --list` shows them all):

- `just dev` — local dev server (`trunk serve`)
- `just build` — release build with the production public URL; output in `dist/`
- `just check` — native + wasm checks, SEO regen, REUSE lint
- `just test` — native unit tests
- `just animation` — regenerate the small gameplay GIF above (also requires
  FFmpeg on `PATH`)
- `just screenshot` — regenerate the full-size still, offscreen on a
  fixed clock (requires a GPU; no window or sound device)
- `cargo run` — the desktop app (`--profile desktop` for a fast build; add
  song folders under Options; on Linux it builds against ALSA and udev:
  `libasound2-dev`, `libudev-dev`)

Generated files: `cargo run -p xtask -- gen-songs` writes `target/songs/index.json`
from `assets/songs/*/PROVENANCE.toml`; `gen-noteskins` writes the bundled
noteskins' index; `gen-credits` writes `credits/CREDITS.md` (checked in, CI verifies it)
and `target/credits/credits.json`. Trunk runs them before every build, `just
test` the first two. Gameplay URLs: `#/play?song=<id>&chart=<n>` plus `&auto=1` for autoplay and
`&gfx=gl` to force WebGL2.

Workspace layout: the Rust crates live in `crates/`: `chart/` (chart model and
importers), `engine/` (deterministic gameplay core), `session/` (a play, for both
shells), `library/` (packs, zips, the import's core), `render/` (wgpu scene),
`actor/`, `ui/` and `theme/` (the screens), `noteskin/` and `lua/` (StepMania
noteskins), `platform/` (platform traits, decoders), `app/` (the game: the web
shell on wasm, the desktop shell natively), `web/` (web shell), `desktop/`
(desktop shell). Beside them: `site/` (the web page Trunk builds), `video/` (the
movie decoder), `tools/` (the DanOni importer), `xtask/` (SEO fragments, song
manifest, credits). `AGENTS.md` has more.

## Bundled songs

The demo songs under `assets/songs/` come from
[OutFox Serenity](https://github.com/TeamRizu/OutFox-Serenity) and are used
under CC BY and CC BY-SA licences; see [`credits/CREDITS.md`](credits/CREDITS.md) for the full
attribution and `assets/songs/*/PROVENANCE.toml` for the pinned source commits.
Only songs whose music, charts and graphics each carry an explicit free licence
in the upstream files are included.

## Licence

Code is MIT. Bundled demo songs are third-party Creative Commons works with
their own authors and licences, tracked in `REUSE.toml` and `credits/CREDITS.md`.
