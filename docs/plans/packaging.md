# Packaging the desktop game and releasing it (2026-10-21)

Status: done 2026-10-21 for macOS (Apple silicon), Linux (x86_64) and
Windows (x86_64), checked locally on macOS; the release workflow has not
run yet (it needs a pushed tag). Builds on [desktop parity](desktop.md)
("Packaging and signing"), [the URL importer's package
contract](danoni-importer.md) and [native video](native-video.md)
(the helper's LGPL payload). PLAN.md phase 9.

## Goals

- One command, run the same way by CI and on a maintainer's machine, that
  turns a built game into what a player downloads: a `.app` (zip and
  `.dmg`) on macOS, a tarball on Linux, a zip on Windows.
- The game runs from wherever the archive is unpacked, its own files found
  relative to its executable, with nothing from the build machine
  remembered and nothing written beside it.
- Pushed tags `v*` build and package on all three systems in CI and attach
  the archives to a draft GitHub release.
- No notarization or paid signing for now (the maintainer's call):
  macOS gets an ad hoc signature, Windows none.

## How the game finds its files (research)

Everything the game reads at run time that is not the player's lives in
the package; everything it writes goes to the platform's folders
(`crates/desktop/src/paths.rs`: settings in the config folder, the play
history in the data folder, the library's cache in the cache folder;
`DDI_HOME` moves all three). So an app bundle can stay read-only and
sealed, and macOS's App Translocation (a quarantined app run from a
random read-only mount) changes nothing.

| What | Where it is looked for | Code |
|---|---|---|
| Bundled songs, noteskins, fonts, the songs' and skins' indexes | `DDI_ASSETS`; else `assets/` in a package folder; else a build's `target/<profile>/assets` (tests and examples one folder below); else the workspace's `assets/` above `target/` | `images::assets` |
| The Credits screen's list | `credits/credits-desktop.json` in the assets (a package), or `../target/credits/` beside them (the workspace) | `shell::credits_screen` |
| The video decoder helper | `DDI_VIDEO_HELPER`; else `video/ddi-video-helper` in a package folder, or beside any folder above the executable up to `target/` | `video::helper_path` |
| The URL importer's bundle | `danoni-importer/<id>/` in a package folder, its id and manifest digest compiled in, every file checked by size and hash | `danoni::bundle::locate` |

"A package folder" is `paths::package_dirs`: the executable's own folder
and, for an executable in an app's `Contents/MacOS/`, the app's
`Contents/Helpers/` and `Contents/Resources/` (added here; the plain
folder and `cargo run` work as before).

The helper's Seatbelt profile (`video/helper/src/sandbox/macos.rs`)
names no path, so it holds wherever the helper is; the package checks it
from the signed app. The importer's helper finds its runtime at
`../runtime` from its own executable and checks it the same way from any
folder.

## Layouts

The relocatable folder (`package.py folder`, formerly `build.py
package`, which still forwards here):

```
ditoditoinfinito(.exe)
assets/                 bundled songs, skins, fonts, indexes, credits/credits-desktop.json
video/                  ddi-video-helper, its notice, ffmpeg.json, FFmpeg's and zlib's
                        source archives, helper-source/ (the LGPL's relinking route)
danoni-importer/<id>/   the importer's bundle (macOS, x86_64 Linux)
LICENSES/  CREDITS.md  THIRD_PARTY_RUST.md  PACKAGE.json  README.txt
```

macOS, `Dito Dito Infinito.app`:

```
Contents/Info.plist             tools/package/macos/Info.plist, versions filled in
Contents/PkgInfo
Contents/MacOS/ditoditoinfinito
Contents/Helpers/video/ddi-video-helper
Contents/Resources/AppIcon.icns
Contents/Resources/assets/  video/ (notice, sources)  danoni-importer/<id>/
Contents/Resources/LICENSES/  CREDITS.md  THIRD_PARTY_RUST.md  PACKAGE.json
```

Shipped as `DitoDitoInfinito-<version>-macos-arm64.zip` (`ditto`, which
keeps the signature) and `.dmg` (`hdiutil`, UDZO, the app beside an
`/Applications` link).

Linux, `DitoDitoInfinito-<version>-linux-x86_64.tar.gz`: the folder plus
`share/applications/dev.sugoijan.ditoditoinfinito.desktop`,
`share/icons/hicolor/scalable/apps/dev.sugoijan.ditoditoinfinito.svg` and
`install-desktop-entry.sh`, which writes the entry (with this folder's
absolute path, quoted as the Desktop Entry specification asks) and the
icon into `$XDG_DATA_HOME` (`--remove` takes them out).

Windows, `DitoDitoInfinito-<version>-windows-x86_64.zip`: the folder
without `danoni-importer/` (never built for Windows: its confinement is
still to come, `danoni-importer.md`) and without `video/` (below).

## Signing and Gatekeeper (macOS)

Inside out, ad hoc (`codesign --sign -`): the video helper (identifier
`dev.sugoijan.ditoditoinfinito.video-helper`), then the app, which signs
the main executable and seals every resource; then `codesign --verify
--deep --strict`. Extended attributes are cleared first (`xattr -cr`).

The importer's three executables (`bin/ddi-danoni-importer`, the SDK's
`runtime/bin/ddi-danoni-export`, the worker's
`runtime/worker/ddi-ruffle-probe`) are not signed again: the linker
already signs them ad hoc (`flags=adhoc,linker-signed`), and the app, the
importer and the SDK all check their bytes against manifests, so a new
signature would make the bundle "damaged". The package checks their
signatures (`codesign --verify --strict`) and the app's seal covers them.

No hardened runtime, no notarization. A downloaded copy is quarantined,
so the first open is refused; the player opens it once through System
Settings → Privacy & Security → Open Anyway (macOS 15 and later; on
earlier systems Control-click → Open), or clears the quarantine
(`xattr -dr com.apple.quarantine "Dito Dito Infinito.app"`). The README
and the release notes should say so.

`LSMinimumSystemVersion` is the newest `minos` among the executables in
the app (their `LC_BUILD_VERSION`), so the app never claims a system its
executables refuse; with today's toolchain that is 11.0 (Rust's
aarch64-apple-darwin default). The display link falls back from
`CADisplayLink` (macOS 14) to `CVDisplayLink` below it.

## Entry point

`tools/package/package.py` (Python 3.11+, already a build prerequisite
for the importer; the standard library only):

- `folder --app EXE --output DIR [--without-video-helper]` the relocatable
  folder, after checking the importer bundle against the app's compiled
  identity, the helper's FFmpeg recipe and sandbox, and the native
  dependencies (the importer's audit; Windows skips it).
- `dist --app EXE --output-dir DIR [--without-video-helper] [--no-check]`
  the folder, then this system's archives in `DIR`, after the packaged
  game's headless checks from its final layout with a scratch `DDI_HOME`:
  the helper's `--sandbox-check`, `--danoni-bundle-info` and
  `--danoni-importer-check` (no window, no sound device, no network).
- `version` Cargo's version (the workspace's), which names the archives
  and fills `Info.plist`.

`just package` generates the indexes and credits, builds with `--profile
desktop` and runs `dist` into `target/dist/`.

## Release workflow

`.github/workflows/release.yml`, on pushed tags `v*` (and by hand, which
keeps the archives as the run's artifacts only):

- `package` on `macos-latest`, `ubuntu-latest` and `windows-latest`: the
  tag must be `v` + Cargo's version; the toolchain, Python 3.12, JDK 17,
  CMake, pkg-config and the ALSA/udev/Vulkan packages as `danoni.yml`'s
  native lane installs them; the importer's and the helper's FFmpeg caches
  under the same keys as `danoni.yml` and `deploy.yml` (a tag's run
  restores main's); `cargo build --locked -p ditoditoinfinito --profile
  desktop` (fat LTO); `package.py dist`; the archives uploaded.
- `release` (tags only, `contents: write`): the archives and a
  `SHA256SUMS.txt` attached to a draft release (`gh release create
  --draft --generate-notes`, then `gh release upload --clobber`, so a
  re-run replaces them), for the maintainer to publish.

## Decided here (the maintainer may change any)

- **Bundle id `dev.sugoijan.ditoditoinfinito`**, from the site's domain
  (sugoijan.dev); the same id names the Linux desktop entry and icon.
- **The icon is the site's favicon** (`site/seo/favicon.svg`, first-party)
  redrawn on the macOS icon grid (`tools/package/icon.svg`: an 824-unit
  tile with a 100-unit margin), rendered by `sips` and `iconutil` at
  package time; Linux ships the SVG. No icon is committed as pixels.
- **The helper in `Contents/Helpers/video/`, everything else in
  `Contents/Resources/`**, the importer's bundle included (Apple's places
  for helper tools and resources; its executables are sealed there as
  resources, since they cannot be re-signed).
- **The importer's executables keep the linker's ad hoc signature**
  (above), rather than changing the importer's build to sign them before
  its manifests are written.
- **A packaging script of its own** (`tools/package/`), taking the
  relocatable folder from `build.py`, which now prepares the importer
  only; `build.py package` forwards so existing instructions still work.
- **Apple silicon only on macOS.** The importer and the helper are built
  on their own host (no cross builds), so an Intel package needs an Intel
  runner; Intel Macs build from source for now.
- **Linux is built on `ubuntu-latest`** (as CI tests it), so the package
  needs that release's glibc (2.39 on 24.04) or newer; an older baseline
  (a container or `ubuntu-22.04`) is a later change.
- **Windows ships without the video helper** for now: its FFmpeg build
  (`DDI_VIDEO_NATIVE=1`, MSYS2 with the MSVC tools) has never run, here or
  in CI, and its restricted-token sandbox has not been checked on
  Windows hardware (`native-video.md`). Movies show their song's
  background (the game's behavior without a helper);
  `--without-video-helper` says so explicitly.
- **The tag must match Cargo's version**, so an archive's name, its
  `Info.plist` and the release agree.
- **Draft releases**: the maintainer publishes after looking.

## Verification (2026-10-21, an Apple silicon Mac)

- `cargo build --locked -p ditoditoinfinito --profile desktop` with the
  default features (the real importer bundle and video helper;
  `CARGO_PROFILE_DESKTOP_LTO=false` locally, to stay under the 2 GiB
  watchdog, as AGENTS.md says for optimized builds), then `package.py
  dist`: a 173 MB zip and a 173 MB `.dmg`; `codesign --verify --deep
  --strict` passes on the app as made, as copied out of the mounted
  `.dmg` and as unpacked from the zip; the main executable and the helper
  show `Signature=adhoc`, the app 1228 sealed files; `Info.plist` with
  0.1.0 and `LSMinimumSystemVersion` 11.0 (every executable's `minos`).
- From the copy out of the `.dmg`, in a scratch folder outside the
  repository, with a scratch `DDI_HOME` (outside any Git checkout: the
  importer refuses storage inside one): `--danoni-bundle-info`
  (enabled, the compiled id), `--danoni-importer-check` (`ready`, 4
  bundled songs: the assets found in `Contents/Resources`), the helper's
  `--sandbox-check` from `Contents/Helpers/video` ("seatbelt holds"),
  and `--silent --seconds 5 --play beatucada --auto --start` under the
  watchdog: the song and its Hard chart read, the preview decoded, the
  play started. It presented no frames, nor did the same run of the
  unpackaged build, with the Mac unattended (its display asleep).
- The relocatable folder (through `build.py package`, the forwarder)
  moved outside the repository: the same three checks with an empty
  `PATH`.
- Unit tests: `paths::package_dirs` (an app bundle, a plain folder, a
  folder named `MacOS` outside one) and the helper found in an app's
  `Contents/Helpers/video`.
- The Linux and Windows archive steps run on a stand-in folder (modes
  kept: the executable and the installer 0755, owners anonymous);
  `install-desktop-entry.sh` installed and removed into a scratch
  `XDG_DATA_HOME` from a folder named with `$`, `"`, `` ` ``, `\`, `&`
  and `|`, its `Exec=` quoted as the specification asks; shellcheck
  clean.
- `release.yml`: actionlint (with shellcheck) clean; its script steps run
  here as above. Not run on GitHub yet.

## Left

- Windows: the video helper built in the release job (MSYS2 and the MSVC
  tools, `DDI_VIDEO_NATIVE=1`) once it has run on Windows hardware; the
  flags' printed output in a console the game was started from (the game
  is a GUI program outside debug builds since 2026-10-21, so no console
  window opens beside it, and its output reaches only a file or a pipe it
  is redirected to; `AttachConsole` once tried on Windows); an icon and
  version resource in the executable; SmartScreen will warn about an unsigned download.
- Notarization and a Developer ID signature (hardened runtime then
  required, and the importer's executables signed before its manifests
  are written), when the maintainer wants them.
- An Intel Mac package (an Intel runner, or a universal build once the
  importer and the helper can be built for both).
- A window icon on Linux and Windows (winit's `with_window_icon`), and an
  app id on Wayland matching the desktop entry.
- An older glibc baseline for Linux; Flatpak or AppImage if players ask.
- The release workflow's first real run (it has only been read and its
  steps run locally on macOS).
