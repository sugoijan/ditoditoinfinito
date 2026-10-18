# DanOni exports in the rebased game

Target update (2026-10-15): 7i is retired. The dated playback counts and
measurements below describe the preserved earlier generation; new mode galleries
and verification requests target 5, 7, 9A and 9B.

Verified 2026-10-13 against a fresh release bundle from `danoni-research`
`7094061`, including game `main` revision `1ab3a9b`. No game crate or accepted
export was changed. The corpus remains **93 songs / 144 charts**. Original works,
music, corpus-specific specs, screenshots and receipts remain local.

## Observed results

Installed Chrome imports both accepted ZIPs: **93 songs, 144 charts and 100,939
judged heads**, without errors or import warnings. All 93 banners decode; the
103 published numerical ratings in Single Songs retain the expected tiers.
The mode galleries contain 84 five-key, ten seven-key, two 7i, two 9A and one
9B songs. A song can appear in more than one gallery.

Twenty-second autoplay checks cover six chart selections on **both WebGPU and
WebGL2**, using the default x2.5 setting:

| Selection | Mode | W1, WebGPU / WebGL2 | Completed holds |
| --- | --- | --- | --- |
| Air | 5 | 54 / 54 | 0 |
| NEEDS, second five-key selector | 5 | 89 / 89 | 4 |
| Decipher | 7 | 40 / 40 | 3 |
| カナリア | 7i | 90 / 89 | 3 |
| ふしぎなひかり, Easy | 9A | 18 / 18 | 4 |
| Same work, Normal (meter 6) | 9B | 46 / 46 | 0 |

All twelve plays have only top-tier judgments, no failure and no let-go.
The 7i count differs because the debug snapshot is published every 20 rendered
frames, so the wall-clock endpoint can include a later head. These are playback
smoke checks, not full-chart or all-144-chart autoplay. The separate game-parser
verifier covers every accepted chart and its timing/scroll metadata.

### The default skin and DanOni symbols

DanOni modes correctly use the **procedural renderer**, with
`__DDI_DEBUG.noteskin == null`: their star, square and circle lane names are
outside dance noteskins' button vocabulary. Screenshots retain the correct
symbols and diagonal arrows. Dito is the default for dance layouts, not a
replacement for these non-dance glyphs.

An eight-second bundled Sweeteners dance-chart initialization control on each
backend loads **Dito Vivid**, produces textured quads, runs Lua and reports no
skin problems or Lua stop. Its early interval contains no judged heads; this
control establishes loading/rendering, not full dance-chart playback. The
underlying checks read `noteskin.name`, `quads`, `problems`, `lua.runs` and
`lua.stopped` from the public game debug state.

### Native base scroll and player multipliers

A read-only WebGPU observer captures the procedural instance buffer submitted
by the actual renderer. The current `render/src/sprite.rs::Instance` layout is
56 bytes; its centre, size and shape distinguish the nearest unjudged arrow
from the leftmost directional receptor. A linear fit against host timestamps
measures physical velocity. Replaced heads, unstable geometry, insufficient
samples and nonlinear paths fail instead of yielding a plausible number.

Air's independently measured source baseline is **2.790697674 arrow heights/s**.
Final-run observations are:

| Player setting | Expected arrow heights/s | Measured | Relative difference |
| --- | --- | --- | --- |
| x1 | 2.790698 | 2.790625 | −0.0026% |
| x2 | 5.581395 | 5.579660 | −0.0311% |
| x3 | 8.372093 | 8.383783 | +0.1396% |

Each proof uses multiple submitted frames and stable receptor geometry, with a
5% acceptance bound for real rendering/clock sampling. This confirms the user's
multiplier applies over the compensated source base. It does not establish
perfect reproduction of every source's custom or per-arrow speed effects.
`#SCROLLS` semantics before the first authored segment remain distinct from its
active segment, as in the parser verifier. The selected time window belongs to
the local proof spec; it is not an extraction or musical-fitting parameter.

### Preview gestures and the Ogg fallback

Real clicks play Air, カナリア and Kouhaku's emy from their imported
`preview_start` values, with nonzero analyser output. A fourth Air case forces
`window.__DDI_FORCE_WASM_DECODE`; it remains audible and the console confirms
that the built-in decoder was used. Every preview context is created while
user activation is active and subsequently runs.

The negative control needs care: Playwright's Chromium function evaluation
sets **`userGesture: true`**. A script click performed through ordinary
`page.evaluate` therefore grants activation and is not a valid gesture-free
control. The runner uses a fresh browser context, avoids locator/evaluate calls
there, and executes its wait, observations and script click through
`Runtime.evaluate` with **`userGesture: false`**. Beforehand both active/sticky
activation flags are false; afterward all audio contexts remain suspended,
current time and output peak stay zero, and no buffer source starts.
[Playwright v1.64.0 implementation](https://github.com/microsoft/playwright/blob/v1.64.0/packages/playwright-core/src/server/chromium/crExecutionContext.ts),
[Chrome autoplay policy](https://developer.chrome.com/blog/autoplay).

This verifies installed Chrome under the strict policy and exercises the Vorbis
fallback. It does not replace the maintainer's Safari gesture check after
publication, nor claim a Firefox/WebKit runtime run in this batch.

## Reusable runner

Build and serve a static bundle from this worktree using the project's
[verify skill](../../.claude/skills/verify/SKILL.md):

```sh
env -u NO_COLOR trunk build --release --public-url / \
  --dist /tmp/ddi-game-check
node .claude/skills/verify/serve.mjs /tmp/ddi-game-check 8097
```

Removing the inherited `NO_COLOR` is needed when its value is `1`, which this
Trunk CLI rejects as a boolean. Use the Node server for the bundle's parallel
requests. In another shell, with existing `npm ci` dependencies:

```sh
node tools/danoni-dump/verify-game.mjs http://127.0.0.1:8097/ \
  "$HOME/Music/DanOni/reviews/runtime-20261013/spec.json" \
  "$HOME/Music/DanOni/reviews/runtime-20261013/new-run"
```

The output directory must be new, external and have an existing parent. The spec
and ZIPs must also be outside Git. Only a loopback game URL is accepted. No chart,
page or work script is executed on the host; the browser loads the trusted game
and imports the selected local chart/media data. Native extraction does not use
this runner or acquire new dependencies through it.

Spec schema **`ddi-danoni-game-check/1`** has these fields:

| Field | Meaning |
| --- | --- |
| `packs` | Array of `{title,path,songs}`; absolute local ZIP path and expected imported count |
| `songs`, `charts`, `heads` | Expected corpus counts; heads include holds |
| `ratedPack`, `ratedCharts` | Collection and expected count of positive published meters |
| `modes` | `{button,layout,songs}` per supported style; canonical `danoni-5/7/9A/9B` |
| `playSpeed` | Positive user x-mod multiplier for playback smoke checks |
| `play` | `{pack,title,chartIndex,layout,seconds,minimumHolds?}`; exact imported chart index |
| `danceControl` | `{songId,chartIndex}` for a bundled eight-second Dito initialization control |
| `scroll` | A play target plus `baseArrowHeightsPerSecond`, `captureUntilSeconds`, `publishedTimeWindow: [low,high]` |
| `previews` | `{pack,title,modeButton,wasm?}`; preview offset comes from the imported record |
| `phases` | Optional subset of `play`, `scroll`, `previews`; defaults to all three. Import/gallery checks always run |

`publishedTimeWindow` uses seconds from the periodically published debug clock;
GPU timestamps are monotonic host milliseconds. Choose a stable segment with
one observable head that stays before its receptor. The runner records raw
positions, fit residuals, measured units and artifact hashes. Source-specific
song choices and their recipes remain in the local spec, not in tracked code.

Output **`ddi-danoni-game-verification/1`** records `phases`, browser version,
spec/runner/probe hashes, exact ZIP hashes, import/menu outcomes, per-play debug
states, physical scroll samples, preview audio observations, the negative
control, errors and `passed`. A phase-subset pass claims only those requested
checks. The runner also writes imported song records and screenshots locally.
It changes only the fresh test profile's settings/storage; it never promotes,
rewrites or republishes exports.

## Local receipts and verification scope

`~/Music/DanOni/reviews/runtime-20261013/` retains the original full-run spec and
all diagnostic attempts. `run-v1` completed the twelve DanOni plays and two Dito
controls, then failed on the preview test's ambiguous substring selector.
`run-v2-preview` exposed the invalid activation-granting negative control.
`run-v4-final` passes the final scroll and preview phases, including the corrected
negative control. Passing playback results are retained separately, not
relabelled as a complete pass of that earlier runner. Both accepted phase groups
bind identical ZIP hashes and the same static game bundle.

`validation-summary.json` combines those explicitly scoped receipts and binds
bundle files, corpus generation, repository revision and tooling hashes. The
55 Node tests include five new probe regressions: velocity/epoch/multiplier,
replaced-head/geometry rejection, unchanged native call/data forwarding, and
activation-free CDP control, and nonfinite-data rejection. `just check`, including REUSE, passes. The corpus
remains unchanged; the next milestone is the tracked native batch workflow and
seven pending embedded-data single-song sources in the
[progress plan](danoni-progress.md#recommended-sequence).
