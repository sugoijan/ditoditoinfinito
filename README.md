# Dito Dito Infinito (Demo)

A dance rhythm game (four arrows, hit them in time with the music) that plays
StepMania simfiles. Live at
`https://sugoijan.dev/ditoditoinfinito/`.

Implementation notes: written in Rust; the web build is wasm (Trunk, GitHub
Pages) and the gameplay core is platform-free so a desktop build can follow.

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

Generated files: `cargo run -p xtask -- gen-songs` writes `target/songs/index.json`
from `assets/songs/*/PROVENANCE.toml`; `gen-credits` writes `CREDITS.md` (checked
in, CI verifies it) and `target/credits/credits.json`. Trunk runs both before every
build. Gameplay URLs: `#/play?song=<id>&chart=<n>` plus `&auto=1` for autoplay and
`&gfx=gl` to force WebGL2.

Workspace layout: `chart/` (chart model and importers), `engine/` (deterministic
gameplay core), `render/` (wgpu scene), `platform/` (platform traits), `app/`
(Yew web shell), `xtask/` (SEO fragments, song manifest, credits).

## Bundled songs

The demo songs under `assets/songs/` come from
[OutFox Serenity](https://github.com/TeamRizu/OutFox-Serenity) and are used
under CC BY licences; see [`CREDITS.md`](CREDITS.md) for the full attribution
and `assets/songs/*/PROVENANCE.toml` for the pinned source commits. Only songs
whose music, charts and graphics are each explicitly CC BY are included.

## Licence

Code is MIT. Bundled demo songs are third-party Creative Commons works with
their own authors and licences, tracked in `REUSE.toml` and `CREDITS.md`.
