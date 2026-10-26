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
ddi(.exe)
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
Contents/MacOS/ddi
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

## The icon, the window and the console (Linux, Windows)

- **The window's icon** on Linux (X11) and Windows is the packages' icon
  (generated in Cargo’s `OUT_DIR` from `assets/icon/`) drawn by resvg when the window opens
  (`shell::window_icon`; resvg already draws the SVG skins): 64 pixels on
  X11; on Windows a 32-pixel small icon (title bar) and a 64-pixel big
  one (taskbar, Alt+Tab), which Windows scales to the display's. macOS
  shows the app's icon; Wayland has no window icons, its desktop shows
  the entry's.
- **The app id**: the window's Wayland app id and X11 class (both parts
  of `WM_CLASS`) are the bundle id, which names the desktop entry (and
  its `StartupWMClass`), so the desktop matches the window to it.
- **The Windows executable's icon and version** (`crates/app/build.rs`):
  a `.res` file written by the build script itself, in the format
  `rc.exe` writes, and passed to the linker (`rustc-link-arg-bins`; MSVC's
  `link.exe` and `lld-link` take `.res` files as they are, as
  `winresource` hands them `rc.exe`'s), so no resource compiler is
  needed, here or in CI. The icon group (id 1, the one Explorer shows)
  holds the SVG drawn at 16, 20, 24, 32, 40, 48 and 64 pixels as 32-bit
  bitmaps with their mask, and at 256 as a PNG; the version
  (`VS_VERSIONINFO`, US English, Unicode) has Cargo's version in its
  numbers and in `FileVersion` and `ProductVersion`, and the product name,
  description, file names and copyright line (`Info.plist`'s). resvg is
  a build dependency on Windows hosts only (Cargo matches a target table's
  build dependencies against the host), so other builds do not compile it
  for the build script; a cross build (the Mac's Windows clippy) writes
  the version alone. Only MSVC targets get the resource.
- **The console** (`ddi_desktop::attach_console`): a Windows build
  outside debug is a GUI program (`crates/app/src/main.rs`), so started
  from cmd or PowerShell it had no console, and what its flags print
  (`--seconds`, `--danoni-bundle-info`, `--video-check`, errors) reached
  only a file or a pipe it was redirected to. A run given any flag, whose
  stdout and stderr are both unset, now attaches to its parent's console
  (`AttachConsole(ATTACH_PARENT_PROCESS)`, first thing in `run`; from
  Explorer there is none and nothing changes). Attaching fills the unset
  standard handles with the console's (`GetStdHandle`'s documented
  attach behavior, unless the parent passed handles with
  `STARTF_USESTDHANDLES`, as a redirection does, which the check already
  leaves alone; "unset" is null, invalid or of no file type, and one
  still unset after attaching, say a stale value the attach left, is
  opened on the console's `CONOUT$` and set), and Rust's std looks the
  handle up on every write
  (`library/std/src/sys/stdio/windows.rs`: `get_handle` calls
  `GetStdHandle` each time, `WriteConsoleW` for a console), so every
  later `println!`/`eprintln!` reaches it; nothing in std caches a
  handle before. A run without flags (a player's) never attaches: an
  attached process ends with its console's Ctrl+C or its window's close.
  A debug build is a console program with its handles set, so it never
  attaches either.

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
  restores main's); `cargo build --locked -p ddi --profile
  desktop` (fat LTO); `package.py dist`; the archives uploaded.
- `release` (tags only, `contents: write`): the archives and a
  `SHA256SUMS.txt` attached to a draft release (`gh release create
  --draft --generate-notes`, then `gh release upload --clobber`, so a
  re-run replaces them), for the maintainer to publish.

## Decided here (the maintainer may change any)

- **Bundle id `dev.sugoijan.ditoditoinfinito`**, from the site's domain
  (sugoijan.dev); the same id names the Linux desktop entry and icon.
- **The executable is `ddi`** (`ddi.exe`), the app crate's package and
  binary named as the other `ddi-*` crates (`ditoditoinfinito` until
  2026-10-24). The bundle id, the desktop entry's id, the settings',
  data and cache folders and the export file names keep the long name,
  so a renamed build finds what an older one kept.
- **The icon is the Infinito down-right arrow** (approved 2026-10-26),
  with a frozen holographic color field, purple inner and outer outlines
  and transparent charge holes. Only `assets/icon/arrow.svg`, `color.png`
  and `profiles.json` are versioned: the approved geometry/appearance,
  frozen 512 px texture, and target framing/shadow settings. Later noteskin
  changes do not silently change the brand icon.
- **Generate only the target being built.** `ddi-icon` is the shared Rust
  compositor. Its optional `raster` feature uses resvg, including the PNG
  decoder, for all generated pixels. The web SVG worker's feature set is
  unchanged. `xtask icons --target web|windows|linux|macos` caches outputs
  under `target/icons/<target>/`; `--out DIR` stages only deliverables.
  The cache tracks the canonical inputs, compositor, xtask generator and
  renderer lockfile, and also checks that its outputs still exist.
  Trunk runs `icons --target web` after building, producing only
  `favicon.svg` in `TRUNK_STAGING_DIR` (256 px embedded color texture).
  Windows/Linux builds generate the window SVG in `OUT_DIR`, tracked by
  Cargo's rerun inputs. The Windows resource builder calls the same icon
  renderer directly for its native icon sizes. Linux packaging requests
  only its transparent SVG, and macOS packaging requests only `AppIcon.icns`.
  Standalone `--target windows` exports the same SVG as Linux's.
- **macOS uses the current legacy ICNS workflow.** The generated composition
  preserves the 824-unit rounded tile with a 100-unit outer margin; the
  chosen 75% symbol fit is inside that tile. resvg renders each iconset size
  directly; Apple's `iconutil` assembles the container during packaging.
  No generated PNG master, `sips` conversion, browser, shader or graphics
  editor is needed by this build path. `icons --target macos --layers`
  additionally exports unmasked 1024 px foreground/background SVGs for
  Icon Composer; compiling a Liquid Glass `.icon` document is still separate.
  See `assets/icon/README.md` for reproduction.
- **The `.res` file written by our build script**, rather than
  `winresource` or `embed-resource`, which need `rc.exe` or `llvm-rc`
  (neither on the Mac, where the Windows clippy runs) and an `.ico`
  file; the format is small and stable, and was checked here by linking
  it (below).
- **Version resource text**: no `CompanyName` (there is no company; a
  signature, not this field, names a publisher); the copyright line is
  `Info.plist`'s.
- **The console is attached only for runs given flags**, and only when
  neither stdout nor stderr is set: a player's start never ties the game
  to a console, and a redirection is never overridden.
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

The icon, the window and the console (later on 2026-10-21, the same Mac):

- The `.res` file: the build script run on the Mac with its icon code
  enabled (a scratch copy with resvg), then linked by `rust-lld -flavor
  link` into a small `no_std` x86_64 Windows executable, which
  `llvm-readobj --coff-resources` lists (8 icons, the group, the version)
  and `pefile` reads independently: the version's strings, `Translation`
  0409/04B0 and fixed info (signature `FEEF04BD`, 0.1.0, `VOS_NT_WINDOWS32`,
  `VFT_APP`); the icon group's entries match the images' sizes. The
  images, gathered back into an `.ico`, decoded by macOS (`sips`): the
  arrow upright, its colors right, the margin transparent, both a bitmap
  (16, 48, 64) and the PNG (256). The version-only `.res` a cross check
  writes links too.
- The Windows clippy (`ddi-desktop` with and without default features,
  `ditoditoinfinito`): clean. The Linux code (window icon, app id):
  `cargo clippy -p ddi-desktop --no-default-features --all-targets` and
  the icon's test in the `rust:1` image (Linux arm64) through
  `container`: clean, passing. `shell::tests::the_window_icon_draws`
  checks the drawing everywhere.
- `just package` (with `CARGO_PROFILE_DESKTOP_LTO=false`): it failed at
  `xattr -cr` on the importer's runtime executables, mode 0500 and
  carrying `com.apple.provenance` from this build, which a read-only file
  cannot have cleared; `make_app` now gives the owner write for the
  clearing and restores every mode after. Then `package.py dist`: the zip
  and the `.dmg`; unpacked from the zip, `codesign --verify --deep
  --strict` passes, `Signature=adhoc`, 1228 sealed files, the runtime
  executables 0500 again; `--danoni-importer-check` ready (4 bundled
  songs) and `--silent --seconds 4 --play beatucada --auto --start`
  playing, as before.
- Nothing was run on Windows or on a Linux desktop (the checks under
  "Left").

## Left

- Windows: the video helper built in the release job (MSYS2 and the MSVC
  tools, `DDI_VIDEO_NATIVE=1`) once it has run on Windows hardware;
  SmartScreen will warn about an unsigned download; the checks below.
- **The maintainer's checks on Windows** (none could run here; the
  release build's `ddi.exe`, or `cargo build --profile
  desktop`, whose build script draws the icon on a Windows host):
  1. Explorer shows the arrow icon on the `.exe` at every view size, and
     its Properties → Details the description, versions, product name,
     copyright and original file name.
  2. The running game's title bar, taskbar button and Alt+Tab show the
     icon (sharp enough at 100% and at 150% or 200% scale), and a
     taskbar pin keeps it.
  3. From cmd: `ddi.exe --danoni-bundle-info` prints
     `{"enabled":false}` in that window (after the prompt, which cmd
     shows again at once for a GUI program: Enter redraws it);
     `start /wait ddi.exe --silent --seconds 3` prints the
     run's report before the prompt returns. The same in PowerShell
     (`Start-Process -Wait -NoNewWindow .\ddi.exe -ArgumentList
     '--silent','--seconds','3'`), and in Windows Terminal.
  4. Redirected, unchanged: `ddi.exe --danoni-bundle-info >
     out.txt` writes the file and nothing to the console; `... | more`
     the same through the pipe; `2> err.txt` alone keeps stdout away
     from the console too (one redirected handle leaves both alone).
  5. Started with no flags from cmd, closing the cmd window or Ctrl+C in
     it leaves the game running; started from Explorer, no console window
     flashes.
- Notarization and a Developer ID signature (hardened runtime then
  required, and the importer's executables signed before its manifests
  are written), when the maintainer wants them.
- An Intel Mac package (an Intel runner, or a universal build once the
  importer and the helper can be built for both).
- Linux: the window's icon on X11 and its app id on Wayland checked on a
  desktop (GNOME, KDE) with the entry installed, and without it.
- An older glibc baseline for Linux; Flatpak or AppImage if players ask.
- The release workflow's first real run (it has only been read and its
  steps run locally on macOS).
