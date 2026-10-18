# Installed native import runtime

This tooling milestone follows `0edc1be`; it does not rebase onto main or change
the desktop, application or library crates. The existing single-work native
profile/admission boundary remains in force. Installation is host configuration;
a page supplies only its URL. HTML5, unknown Flash engines, automatic Kouhaku URL
controls, Windows and non-x86-64 Linux remain outside this runtime's tested scope.
The pinned Kouhaku capture/normalized route remains available separately.

## What is installed and what the system supplies

`runtime::install` copies a verified worker bundle, the operator-selected SDK
executable, the exact packaged fonts, font provenance/licences, DDI licence and
collected dependency notices into a **fresh** external prefix. It neither builds
nor downloads anything. `install-runtime BUNDLE NOTICES NEW_PREFIX` uses the
running SDK executable. The layout is:

```text
prefix/
  bin/ddi-danoni-export
  worker/                       complete existing verified worker bundle
  fonts/                        exact packaged fonts and PROVENANCE.toml
  licenses/                     DDI MIT and both font licences
  notices/                      collected texts, both lockfiles and review gaps
  reviews/optional-colors.json   five existing exact missing-colour reviews
  manifest.json                 version/platform/identity and complete inventory
```

Fresh installed runtimes require the `opus` feature. Opus is the URL-import
default; Vorbis is an optional compatibility encoder. SDK lock/source identity
changes require rebuilding/reinstalling the host prefix; the worker identity
and existing captured sources are unchanged. See [the Opus contract](danoni-rust-opus.md).

`Installation::load` checks the contract, SDK version, enabled Opus/Vorbis features,
SDK lock/source identities, platform, every file's size/hash, exact font bytes,
notice inventories and both lock bindings, and the worker's independent bundle,
ELF/Mach-O, source identity and confinement-policy checks. Missing, extra,
changed or symlinked files reject. There are no executable download or upgrade
fallbacks. This is integrity against trusted host installation metadata, **not**
a signature or authentication of arbitrary binaries. Installers/distributors
must supply their own trusted delivery/attestation.

`Installation::locate` uses an explicit host path, then `DDI_DANONI_RUNTIME`, then
the parent of the executable's `bin/` directory. A moved complete prefix needs no
rewrite of its inventory. Work URLs cannot select a prefix, fonts, driver paths,
confinement rules or filesystem roots.

The prefix ships **no Linux graphics libraries or drivers**. Linux prerequisites
are x86-64, the glibc ELF loader at `/lib64/ld-linux-x86-64.so.2`, compatible libc,
libm and libgcc_s, and a kernel permitting Landlock ABI >= 6, seccomp and
close_range. Rendering additionally needs the distribution's Vulkan loader
(`libvulkan.so.1`) and Mesa Lavapipe CPU ICD, its manifest and transitive shared
libraries in distribution paths. No GPU, display server or hardware-device grant
is required. Mesa's LLVM and other dependencies belong to the system package
manager. On Debian-family hosts the usual packages are `libvulkan1` and
`mesa-vulkan-drivers`; Gentoo needs Vulkan Loader and Mesa built with its Lavapipe
Vulkan driver. Distribution package/version details vary; use the host's package
manager and the diagnostic below, not a copied Debian setup script.

The previously reviewed local ELF worker and this host-built SDK require
**GLIBC_2.43**. A prefix built here is not thereby portable to older-glibc
machines. Build/distribute against a chosen compatible minimum baseline and test
it there before claiming a cross-distribution binary. The workflow is portable
source/installation configuration; self-contained or universal Linux binary
distribution is not established. macOS retains its existing confinement and
Mach-O bundle contracts; this new installed-prefix workflow is validated on
Linux only.

End-user imports invoke native Rust APIs and the confined installed worker only:
no Python, Node, Java, shell wrapper, compiler, Git, FFmpeg command, browser or
research directory is needed. Rust audio/Opus and text rasterization code is
compiled into the SDK. Java builds Ruffle's playerglobal data; it is a maintainer
requirement, not a media decoder or rendering dependency.

## Maintainer build workflow

Use Rust **1.99**, Git, a JDK with `java`, a C/C++ toolchain, CMake, pkg-config and
Python 3 for the **build-time notice/job helpers**. Cargo obtains public packages
from the committed independent SDK/worker lockfiles. The script does not install
system packages, modify users, run as root or execute acquired work code.

From this branch's checkout, with fresh external locations and already configured
system Vulkan/Mesa:

```sh
git clone --depth 1 --branch v0.7.1 --filter=blob:none --sparse \
  https://github.com/ruffle-rs/ruffle.git "$HOME/.cache/ddi-ruffle-upstream"
git -C "$HOME/.cache/ddi-ruffle-upstream" sparse-checkout set \
  core render swf flv wstr video tools/asc
sh tools/danoni-dump/build-native-runtime.sh \
  "$HOME/.cache/ddi-ruffle-upstream" \
  "$HOME/.cache/ddi-native-build-001" "$HOME/.local/ddi-native-001"
```

The Rust preparer verifies pinned revision
`89a7049965f3090e706265670df309a5128233ac`, exact hooks/templates and the CPU
allocation hint. The script builds both locked release executables, packages the
worker, collects notices, installs a new prefix and runs capability diagnosis.
Git/Cargo may fetch public build sources; installed imports never call them.
Set `CARGO_NET_OFFLINE=true` only after all locked target packages/Git blobs
are cached; the overinclusive notice inventory also reads inactive-target sources. Keep build/cache directories outside Git; do not place corpus files
in them. A failed build/install is not overwritten; inspect it and choose a new
build/prefix directory. Review source/script changes before rebuilding a trusted
runtime. Runtime install failures publish no Ready manifest.

The notice collector preserves an intentionally overinclusive Cargo metadata
graph, including development/inactive packages and vendored licence files it
finds. Its receipt binds **both** exact lockfiles, records package licence
expressions, copied text hashes and missing text. It is not a linked-binary SBOM
or a completed redistribution audit. In the recorded local study it collects
453 packages and 748 files, with 54 packages lacking a discovered text (including
workspace members whose licences live at repository roots). Public distribution
still requires resolving those notices, reviewing licence compatibility,
vendored C dependencies and the worker's embedded NotoSans/NotoSansMath font
notices. The receipt explicitly says `redistributionApproved:false`. The local
prefix is useful for repeatable host installation without claiming that this
remaining distribution review is complete.

An already built reviewed worker can be reused without Java or a compiler:

```sh
# Build-time collector; use the manifests that produced these exact binaries.
python3 tools/danoni-dump/collect-runtime-notices.py \
  tools/danoni-dump/rust/Cargo.toml /external/prepared/worker/Cargo.toml \
  /external/new-notices --target x86_64-unknown-linux-gnu
/external/sdk/ddi-danoni-export install-runtime \
  /external/verified-worker /external/new-notices /external/new-prefix
/external/new-prefix/bin/ddi-danoni-export verify-runtime /external/new-prefix
/external/new-prefix/bin/ddi-danoni-export diagnose-runtime \
  /external/new-prefix /external/new-capability-output
```

`diagnose-runtime` runs the packaged **original synthetic** bitmap SWF through
the production supervisor, unchanged default limits and native scale. It checks
adapter-policy provenance, PPM readback pixels, ActionScript `BitmapData.draw` /
`getPixel` values at two independent frames, strict runtime errors and a mixed
Japanese/Latin title. It retains `diagnostic.json` and capture evidence on either
success or failure. Errors identify the report/stderr and the kernel/driver
prerequisites; there is no retry without confinement. A synthetic success proves
capability, never real-chart equivalence.

## Host configuration and URL-only imports

Initialize one managed library and configure writable locations once. These
examples require no per-song font path, renderer flags, selector recipe or grid:

```sh
mkdir -p "$HOME/.cache/ddi-imports" "$HOME/.local/share/ddi-staging"
"$HOME/.local/ddi-native-001/bin/ddi-danoni-export" create-store \
  "$HOME/.local/share/ddi-library"
```

Example `host.json` (relative paths resolve against this file, not the song):

```json
{
  "runtime": "/absolute/installed/prefix",
  "cache": "/absolute/writable/cache",
  "staging": "/absolute/writable/staging",
  "library": "/absolute/initialized/library",
  "offline": false,
  "origins": {
    "https://reviewed.example": "host's recorded public-access terms decision"
  }
}
```

Set `runtime:null` to use the lookup rule. Site decisions are explicit host
configuration, never a robots exemption. Review each real origin's public-access
rules; acquisition retains existing robots, caching, request and byte/rate limits.
`offline:true` requires complete cache evidence and makes no source-site requests.

```sh
/absolute/installed/prefix/bin/ddi-danoni-export import-url host.json \
  https://reviewed.example/supported-work.html
/absolute/installed/prefix/bin/ddi-danoni-export verify-store /absolute/library
/absolute/installed/prefix/bin/ddi-danoni-export resume-import \
  /absolute/staging/import-UNIQUE/import-candidate.json /absolute/library
```

Candidates use fresh timestamp/PID directories and the existing restart token;
existing directories, accepted exports and historical evidence are not rewritten.
Resume revalidates the retained candidate without fonts/cache/worker access, and
same-content imports remain no-ops. Duplicate policy, cancellation, uncertain
commit/recovery and all audited admission rules remain the existing coordinator's.

Embedding hosts call `Installation::load`/`locate`, then
`installation.request(url, offline)` and `import_job::execute` with their cache,
worker, store, site callback, progress and cancellation. The CLI uses those same
APIs through `runtime::import_url`; it contains no special extraction recipes.
The host still owns blocking-job scheduling. No desktop integration is made here.

The shared request selects packaged title artwork and
`RendererPolicy::ReviewedNativeScale`. Exact control fingerprints are checked
before execution. Later constant-menu profiles retain renderer-free operation;
reviewed early and scheduled families receive an explicit native-scale renderer
from initialization. Linux uses **Vulkan/software**, while macOS retains the
portable automatic backend policy. The policy is bounded to these reviewed
families, not a whole-movie script whitelist; all source/runtime/normalization
proofs are still mandatory. No arbitrary failure is retried with rendering or
relaxed restrictions. Legacy explicit-only APIs remain available. Dependency
recovery and every renderer capture share existing whole-job and per-process
budgets. Linux scale > 1 remains rejected.

The host factory also supplies the five **existing** exact optional-colour reviews
for March, Riguru, Saigo, Regret Enemy and Danmaku. The packaged JSON contains only
review facts (movie hash, logical `iro.txt` URL and response hash), not third-party
bodies or scripts. URL preparation obtains the actual response through normal
policy-aware acquisition, binds HTTP 404/status/URL/body hash/retrieval provenance,
and supplies the existing `OptionalColorEvidence` contract. The unchanged audit
matcher must see exactly its known denial/error pair at one iteration; all raw
diagnostics remain archived and normalization retains the missing-colour effect.
No unknown source, changed 404 body, different resource or additional runtime
error gets this exception. A 200 response follows ordinary dependency recovery.
The table is bounded to 16 host entries and duplicates/invalid bindings reject
before worker/network access. Existing explicit-only URL APIs default to no such
reviews. This makes the already reviewed seven-key path usable from a URL without
requiring a per-song review path; it does not introduce a generic 404 waiver.

## Portable title fonts

Read-only `origin/main` inspection used commit
`448c0a5555dd5ae31f3ac73fe812b1a9828debbe` (`git ls-tree`, `git show`);
there was no merge, rebase or main-asset edit. All three Term subsets contain 349
Unicode scalars and lack the tested kana/kanji. Main's provenance explicitly
expects system fallback; that is unsuitable for an isolated import renderer.

Tooling therefore reuses the **exact Regular subset** (26,940 bytes, OFL-1.1),
SHA-256 `4aa11ea11626ffe18bb66800f1ee5225fbcbcd49c05f6bb9833fea8a9a941c5d`,
with complete, unchanged Noto Sans CJK JP Regular **2.004**, OFL-1.1, SHA-256
`68a3fc98800b2a27b371f2fb79991daf3633bd89309d4ffaa6946fd587f375b5`.
Its pinned upstream is `notofonts/noto-cjk` commit
`523d033d6cb47f4a80c58a35753646f5c3608a78` (`Sans2.004`). Copyright is
2015–2026 Renzhi Li for Iosevka and 2014–2021 Adobe for the Noto face. Full source,
prior main subset command and absence of new transformations are recorded in
`tools/danoni-dump/rust/fonts/PROVENANCE.toml` and tooling `THIRD_PARTY.toml`.
No host font or absolute font path is used.

The existing usvg/resvg text path uses Rustybuzz shaping and explicit fontdb
fallback. Only the packaged two-face database is loaded. Latin-only text selects
Iosevka; usvg may select Noto for a **whole shaped mixed-script run** when all its
characters match. Proofs record actual glyph font families/counts, both font
hashes and the no-system-font policy. Missing glyph IDs and clipped/truncated
text remain errors, including a deliberately unsupported Unicode scalar.
This is not a promise of every language, emoji or arbitrary title layout.

Actual cmap and raster/shaping tests cover `灼冬赫夜`, `ふしぎなひかり`,
`上海紅茶館 ～ Chinese Tea`, `メイドと血の懐中時計`, `Crow / 桃源郷`,
`北風のメロディー`, `ころっけわっしょい`,
`風神少女 ～ The Crow's Newspaper`, and decomposed kana. Noto covers all tested
scalars (44,810 encoded scalars in this face). Portable banner tests also check
credits, dimensions, glyph IDs and family selection. Unknown glyphs reject;
they are not silently rendered as boxes. SVG proof files describe text; their
independent display requires the named fonts. The PNG is the portable installed
artifact and needs no font renderer at playback.


## Validation checkpoint and remaining work

See the [Linux continuation](danoni-rust-linux.md#installed-runtime-and-normal-export-validation-2026-10-16)
for exact tested works, musical differences and reproducible gates. The new
source-build script passes syntax checks; its constituent source/bundle operations
are covered by SDK tests and the earlier pinned Linux build. This checkpoint
**reuses that verified worker** and builds the current SDK, then tests two fresh
installation prefixes directly against system Mesa. It does not claim a new
end-to-end release-build rehearsal or a binary reproducibility guarantee.

Remaining work includes that clean release-build rehearsal/minimum-glibc matrix,
completion of redistribution notices, musical phase/metre review, broader URL
profiles/collections, Linux hardware rendering and enlarged rendering, other
Linux architectures and Windows. Desktop integration and rebasing are deferred.
