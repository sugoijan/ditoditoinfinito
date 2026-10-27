#!/usr/bin/env bash
# Builds the desktop decoder helper's FFmpeg natively (docs/plans/native-video.md,
# section 10): the release pinned in video/FFMPEG.toml, configured with its
# `native_configure` flags and the module's decoder list, as static libraries
# in target/video/native/<rust target>/ (lib/, include/), with zlib built the
# same way, a description (ffmpeg.json) and the notice the helper ships with
# (ddivideo-native.LICENSE.txt). `video/helper`'s build script compiles the
# shims against it and links them into `ddi-video-helper`; `crates/desktop/build.rs`
# runs this script when the output is missing or older than its recipe.
# Each stage is skipped when its output is current.
#
# Hosts: macOS (arm64, x86_64) and Linux (x86_64, arm64) with bash, curl, tar
# with xz, GNU make and a C compiler; nasm on x86_64 for FFmpeg's assembly
# (built without it otherwise, slower). Windows: MSYS2's bash with the MSVC
# tools on PATH (`--toolchain=msvc`), untested here.
#
# Usage: video/build-native.sh [--target <rust target>] [--check | --recipe]
# (--check: exit 0 when the output is current, 1 when it needs a build;
# --recipe: print the SHA-256 of the recipe, this script, lib.sh and
# FFMPEG.toml, which ffmpeg.json records and CI names the built libraries
# by, .github/scripts/video-assets.sh).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
toml="$root/video/FFMPEG.toml"
# shellcheck source=video/lib.sh
. "$root/video/lib.sh"

unset CPATH C_INCLUDE_PATH CPLUS_INCLUDE_PATH OBJC_INCLUDE_PATH LIBRARY_PATH

want_target=""
check=0
# What the output depends on: a change rebuilds (FFmpeg's make does the rest).
recipe=$(cat "$toml" "$root/video/build-native.sh" "$root/video/lib.sh" | { command -v sha256sum >/dev/null 2>&1 && sha256sum || shasum -a 256; } | cut -d' ' -f1)
while [ $# -gt 0 ]; do
  case $1 in
    --target) want_target=$2; shift 2 ;;
    --check) check=1; shift ;;
    --recipe) printf '%s\n' "$recipe"; exit 0 ;;
    *) die "unknown argument $1" ;;
  esac
done

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  MINGW*-x86_64 | MSYS*-x86_64) target=x86_64-pc-windows-msvc ;;
  MINGW*-aarch64 | MSYS*-aarch64 | MINGW*-arm64 | MSYS*-arm64) target=aarch64-pc-windows-msvc ;;
  *) die "unsupported host $(uname -s)-$(uname -m)" ;;
esac
if [ -n "$want_target" ] && [ "$want_target" != "$target" ]; then
  die "this host builds $target, not $want_target (the helper is never cross-built)"
fi
out="$root/target/video/native/$target"
downloads="$root/target/video/downloads"

ffmpeg_version=$(value version)
ffmpeg_tag=$(value tag)
ffmpeg_url=$(value tarball_url)
ffmpeg_sha=$(value tarball_sha256)
zlib_version=$(value zlib_version)
zlib_url=$(value zlib_url)
zlib_sha=$(value zlib_sha256)

configure_flags=()
while IFS= read -r flag; do
  configure_flags+=("$flag")
done < <(array native_configure)
[ "${#configure_flags[@]}" -gt 0 ] || die "no native_configure flags in $toml"
decoders=$(array video_decoders | paste -sd, -)
[ -n "$decoders" ] || die "no video_decoders in $toml"
configure_flags+=("--enable-decoder=$decoders")

case $target in
  x86_64-*)
    if ! command -v nasm >/dev/null 2>&1; then
      configure_flags+=("--disable-x86asm")
    fi ;;
esac
case $target in
  *-apple-darwin)
    # Rust's own minimum for these targets, so the helper links without
    # "built for newer macOS" warnings.
    case $target in
      aarch64-*) export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}" ;;
      *) export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-10.12}" ;;
    esac ;;
  *-pc-windows-msvc)
    configure_flags+=("--toolchain=msvc") ;;
esac

if [ -f "$out/ffmpeg.json" ] && grep -q "\"recipe_sha256\": \"$recipe\"" "$out/ffmpeg.json"; then
  [ "$check" = 1 ] && exit 0
  say "$out is current"
  exit 0
fi
[ "$check" = 1 ] && exit 1

mkdir -p "$downloads" "$out"
rm -f "$out/ffmpeg.json"

# 1. Sources.
ffmpeg_file=$(basename "$ffmpeg_url")
fetch "$ffmpeg_url" "$downloads/$ffmpeg_file" "$ffmpeg_sha"
src="$root/target/video/ffmpeg-$ffmpeg_version"
unpack "$downloads/$ffmpeg_file" "$src" "$ffmpeg_sha"
zlib_file=$(basename "$zlib_url")
fetch "$zlib_url" "$downloads/$zlib_file" "$zlib_sha"
zsrc="$root/target/video/zlib-$zlib_version"
unpack "$downloads/$zlib_file" "$zsrc" "$zlib_sha"

cc=${CC:-cc}
case $target in *-pc-windows-msvc) cc=${CC:-cl} ;; esac

# 2. zlib, compiled file by file (no gz*: FFmpeg does not use zlib's file API).
zlib="$out/zlib"
if [ ! -f "$zlib/lib/libz.a" ] || [ "$zsrc/.ddi-sha256" -nt "$zlib/lib/libz.a" ]; then
  say "building zlib $zlib_version for $target"
  rm -rf "$zlib"
  mkdir -p "$zlib/obj" "$zlib/include" "$zlib/lib"
  zobjs=()
  for c in adler32 compress crc32 deflate infback inffast inflate inftrees trees uncompr zutil; do
    case $target in
      *-pc-windows-msvc) "$cc" /nologo /O2 /c "/Fo$zlib/obj/$c.obj" "$zsrc/$c.c" > /dev/null; zobjs+=("$zlib/obj/$c.obj") ;;
      *) "$cc" -O2 -fPIC "-ffile-prefix-map=$zsrc=zlib" -c "$zsrc/$c.c" -o "$zlib/obj/$c.o"; zobjs+=("$zlib/obj/$c.o") ;;
    esac
  done
  case $target in
    *-pc-windows-msvc) lib /nologo "/OUT:$zlib/lib/libz.a" "${zobjs[@]}" ;;
    *) ar rcs "$zlib/lib/libz.a" "${zobjs[@]}" ;;
  esac
  cp "$zsrc/zlib.h" "$zsrc/zconf.h" "$zlib/include/"
fi

# 3. Configure, out of tree, installing into $out.
build="$out/ffmpeg-build"
mkdir -p "$build"
configure=(
  "$src/configure"
  "--prefix=$out" "--libdir=$out/lib" "--incdir=$out/include"
  "${configure_flags[@]}"
  "--extra-cflags=-ffile-prefix-map=$src=ffmpeg -ffile-prefix-map=$build=ffmpeg -I$zlib/include"
  "--extra-ldflags=-L$zlib/lib"
)
case $target in *-pc-windows-msvc) configure[5]="--extra-cflags=-I$zlib/include" configure[6]="--extra-ldflags=-LIBPATH:$zlib/lib" ;; esac
stamp="$build/.ddi-configure"
if [ ! -f "$stamp" ] || [ "$(cat "$stamp")" != "${configure[*]}" ]; then
  say "configuring FFmpeg $ffmpeg_version for $target"
  rm -f "$stamp"
  if ! (cd "$build" && "${configure[@]}" > configure.out 2>&1); then
    tail -n 30 "$build/ffbuild/config.log" >&2 || true
    die "configure failed (log: $build/ffbuild/config.log)"
  fi
  printf '%s' "${configure[*]}" > "$stamp"
fi

# 4. Build and install the libraries and headers.
jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)
say "building libraries for $target (make -j$jobs)"
make -C "$build" -j"$jobs" > "$build/make.out" 2>&1 ||
  { tail -n 30 "$build/make.out" >&2; die "make failed (log: $build/make.out)"; }
make -C "$build" install > "$build/install.out" 2>&1 ||
  { tail -n 30 "$build/install.out" >&2; die "make install failed (log: $build/install.out)"; }
rm -rf "$out/share"
mkdir -p "$out/lib"
cp "$zlib/lib/libz.a" "$out/lib/libz.a"

# 5. The notice the helper ships with.
notice() {
  printf '\n\n==== %s ====\n\n' "$1"
  cat "$2"
}
{
  cat <<NOTICE
ddi-video-helper, the video decoder of Dito Dito Infinito
($(value project_url))

This program uses code of FFmpeg $ffmpeg_version ($(value homepage)), licensed
under the GNU Lesser General Public License version 2.1 or later (below),
linked statically. Its source, unmodified, is distributed next to this file
as $ffmpeg_file, SHA-256 $ffmpeg_sha:
the release at $ffmpeg_url
(tag $ffmpeg_tag, $(value source_url)).
The program is built from it by video/build-native.sh in the repository above,
with the configure flags in video/FFMPEG.toml (also listed in ffmpeg.json
next to this file), and linked with video/ddivideo.c, video/packets.c and
the helper's own code in video/helper and video/protocol, the repository's
own code (MIT), whose source is distributed next to this file as
helper-source/. Building the repository's \`ddi-video-helper\` again
(\`cargo build -p ddi-video-helper\` after this script) relinks it, also
against a modified FFmpeg, which the LGPL allows you to do.

It also contains zlib $zlib_version (https://zlib.net/), under the zlib
licence (below).
NOTICE
  notice "FFmpeg: GNU Lesser General Public License version 2.1" "$src/COPYING.LGPLv2.1"
  notice "zlib $zlib_version: licence" "$zsrc/LICENSE"
} > "$out/ddivideo-native.LICENSE.txt"

# 6. The description, last, so its presence means the output is complete.
{
  printf '{\n'
  printf '  "name": "FFmpeg",\n'
  printf '  "version": %s,\n' "$(json_string "$ffmpeg_version")"
  printf '  "tag": %s,\n' "$(json_string "$ffmpeg_tag")"
  printf '  "license": %s,\n' "$(json_string "$(value license)")"
  printf '  "source_url": %s,\n' "$(json_string "$(value source_url)")"
  printf '  "target": %s,\n' "$(json_string "$target")"
  printf '  "zlib": %s,\n' "$(json_string "$zlib_version")"
  printf '  "configure": ['
  sep=""
  for flag in "${configure_flags[@]}"; do
    printf '%s\n    %s' "$sep" "$(json_string "$flag")"
    sep=","
  done
  printf '\n  ],\n'
  printf '  "libraries": ["avformat", "avcodec", "swscale", "avutil", "z"],\n'
  printf '  "recipe_sha256": %s\n' "$(json_string "$recipe")"
  printf '}\n'
} > "$out/ffmpeg.json.part"
mv "$out/ffmpeg.json.part" "$out/ffmpeg.json"
say "$out: $(du -sh "$out/lib" | cut -f1) of libraries"
