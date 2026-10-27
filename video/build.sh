#!/usr/bin/env bash
# Builds the software video decoder (docs/plans/video-backgrounds.md, step 1):
# downloads the wasi-sdk and FFmpeg release pinned in video/FFMPEG.toml into
# target/video/, checks their SHA-256, configures and builds FFmpeg's
# libraries and links the decode/packet shims into target/video/ddivideo.wasm, with
# target/video/ddivideo.json describing it. Each stage is skipped when its
# output is current, so running it again is cheap.
#
# Hosts: macOS and Linux, arm64 or x86_64. Needs bash, curl, tar with xz,
# GNU make and a host C compiler (configure's checks). No containers.
#
# Usage: video/build.sh [--recipe]
# (--recipe: print the SHA-256 of what the module is built from, this
# script, lib.sh, FFMPEG.toml and the shims, and exit; CI names the built
# module by it, .github/scripts/video-assets.sh.)
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
toml="$root/video/FFMPEG.toml"
shim="$root/video/ddivideo.c"
packets="$root/video/packets.c"
out="$root/target/video"

# shellcheck source=video/lib.sh
. "$root/video/lib.sh"

recipe=$(cat "$toml" "$root/video/build.sh" "$root/video/lib.sh" "$shim" "$packets" | { command -v sha256sum >/dev/null 2>&1 && sha256sum || shasum -a 256; } | cut -d' ' -f1)
case "${1:-}" in
  --recipe) printf '%s\n' "$recipe"; exit 0 ;;
  "") ;;
  *) die "unknown argument $1" ;;
esac

# Include and library paths meant for the host (Homebrew sets these) would
# reach the wasm compiler too.
unset CPATH C_INCLUDE_PATH CPLUS_INCLUDE_PATH OBJC_INCLUDE_PATH LIBRARY_PATH

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) host=arm64-macos ;;
  Darwin-x86_64) host=x86_64-macos ;;
  Linux-x86_64) host=x86_64-linux ;;
  Linux-aarch64 | Linux-arm64) host=arm64-linux ;;
  *) die "unsupported host $(uname -s)-$(uname -m)" ;;
esac
host_key=$(printf '%s' "$host" | tr '-' '_')
# Binaryen names Linux on ARM `aarch64`.
binaryen_host=${host/arm64-linux/aarch64-linux}

ffmpeg_version=$(value version)
ffmpeg_tag=$(value tag)
ffmpeg_url=$(value tarball_url)
ffmpeg_sha=$(value tarball_sha256)
wasi_version=$(value wasi_sdk_version)
wasi_url=$(value wasi_sdk_url)
wasi_sha=$(value "wasi_sdk_sha256_$host_key")
zlib_version=$(value zlib_version)
zlib_url=$(value zlib_url)
zlib_sha=$(value zlib_sha256)
binaryen_version=$(value binaryen_version)
binaryen_url=$(value binaryen_url)
binaryen_sha=$(value "binaryen_sha256_$host_key")

configure_flags=()
while IFS= read -r flag; do
  configure_flags+=("$flag")
done < <(array configure)
[ "${#configure_flags[@]}" -gt 0 ] || die "no configure flags in $toml"
decoders=$(array video_decoders | paste -sd, -)
[ -n "$decoders" ] || die "no video_decoders in $toml"
configure_flags+=("--enable-decoder=$decoders")

mkdir -p "$out/downloads"

# 1. Toolchain.
wasi_name="wasi-sdk-$wasi_version-$host"
fetch "$wasi_url$wasi_name.tar.gz" "$out/downloads/$wasi_name.tar.gz" "$wasi_sha"
wasi="$out/$wasi_name"
unpack "$out/downloads/$wasi_name.tar.gz" "$wasi" "$wasi_sha"

binaryen_name="binaryen-$binaryen_version-$binaryen_host"
fetch "$binaryen_url$binaryen_name.tar.gz" "$out/downloads/$binaryen_name.tar.gz" "$binaryen_sha"
binaryen="$out/binaryen-$binaryen_version"
unpack "$out/downloads/$binaryen_name.tar.gz" "$binaryen" "$binaryen_sha"

# 2. FFmpeg source.
ffmpeg_file=$(basename "$ffmpeg_url")
fetch "$ffmpeg_url" "$out/downloads/$ffmpeg_file" "$ffmpeg_sha"
src="$out/ffmpeg-$ffmpeg_version"
unpack "$out/downloads/$ffmpeg_file" "$src" "$ffmpeg_sha"

# 3. zlib, compiled file by file (its configure picks the host's archiver
# on macOS); FFmpeg finds it through the paths below.
zlib_file=$(basename "$zlib_url")
fetch "$zlib_url" "$out/downloads/$zlib_file" "$zlib_sha"
zsrc="$out/zlib-$zlib_version"
unpack "$out/downloads/$zlib_file" "$zsrc" "$zlib_sha"
zlib="$out/zlib-build"
if [ ! -f "$zlib/lib/libz.a" ] || [ "$zsrc/.ddi-sha256" -nt "$zlib/lib/libz.a" ]; then
  say "building zlib $zlib_version"
  mkdir -p "$zlib/obj" "$zlib/include" "$zlib/lib"
  zobjs=()
  # Without gz*, zlib's file API, which FFmpeg does not use.
  for c in adler32 compress crc32 deflate infback inffast \
    inflate inftrees trees uncompr zutil; do
    "$wasi/bin/clang" -Oz -msimd128 "-ffile-prefix-map=$zsrc=zlib" -c "$zsrc/$c.c" -o "$zlib/obj/$c.o"
    zobjs+=("$zlib/obj/$c.o")
  done
  rm -f "$zlib/lib/libz.a"
  "$wasi/bin/ar" rcs "$zlib/lib/libz.a" "${zobjs[@]}"
  cp "$zsrc/zlib.h" "$zsrc/zconf.h" "$zlib/include/"
fi

# wasi-libc's licence files, for the notices (step 7).
libc_commit=$(value wasi_libc_commit)
libc_url="$(value wasi_libc_url)$libc_commit"
libc="$out/downloads/wasi-libc-$libc_commit"
mkdir -p "$libc"
libc_files=(LICENSE LICENSE-MIT libc-top-half/musl/COPYRIGHT libc-bottom-half/cloudlibc/LICENSE)
libc_keys=(LICENSE LICENSE_MIT musl_COPYRIGHT cloudlibc_LICENSE)
for i in "${!libc_files[@]}"; do
  fetch "$libc_url/${libc_files[$i]}" "$libc/${libc_keys[$i]}" "$(value "wasi_libc_sha256_${libc_keys[$i]}")"
done

# 4. Configure, out of tree. Paths are mapped away so the module does not
# depend on where it was built.
build="$out/ffmpeg-build"
mkdir -p "$build"
configure=(
  "$src/configure"
  "--cc=$wasi/bin/clang" "--ar=$wasi/bin/ar" "--ranlib=$wasi/bin/ranlib"
  "--nm=$wasi/bin/nm" "--strip=$wasi/bin/strip"
  "${configure_flags[@]}"
  "--extra-cflags=-ffile-prefix-map=$src=ffmpeg -ffile-prefix-map=$build=ffmpeg -I$zlib/include"
  "--extra-ldflags=-L$zlib/lib"
)
stamp="$build/.ddi-configure"
if [ ! -f "$stamp" ] || [ "$(cat "$stamp")" != "${configure[*]}" ]; then
  say "configuring FFmpeg $ffmpeg_version"
  rm -f "$stamp"
  # configure probes stdbit.h and fails that one check; harmless.
  if ! (cd "$build" && "${configure[@]}" > configure.out 2>&1); then
    tail -n 30 "$build/ffbuild/config.log" >&2 || true
    die "configure failed (log: $build/ffbuild/config.log)"
  fi
  printf '%s' "${configure[*]}" > "$stamp"
fi

# 5. The libraries (make itself skips what is current).
jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)
say "building libraries (make -j$jobs)"
make -C "$build" -j"$jobs" > "$build/make.out" 2>&1 ||
  { tail -n 30 "$build/make.out" >&2; die "make failed (log: $build/make.out)"; }

# 6. Link the shim. A reactor (`_initialize`, no `_start`); the stack sits
# below the data so an overflow traps instead of corrupting memory, and the
# memory is capped so a hostile file cannot take more than 1 GiB.
libs=("$build/libavformat/libavformat.a" "$build/libavcodec/libavcodec.a"
  "$build/libswscale/libswscale.a" "$build/libavutil/libavutil.a" "$zlib/lib/libz.a")
wasm="$out/ddivideo.wasm"
exports=()
while IFS= read -r name; do
  exports+=("$name")
done < <(sed -n 's/^EXPORT [^(]*[ *]\(ddi_[a-z_]*\)(.*/\1/p' "$shim" "$packets")
relink=0
[ -f "$wasm" ] || relink=1
for input in "$shim" "$packets" "$root/video/build.sh" "$toml" "${libs[@]}"; do
  if [ "$input" -nt "$wasm" ]; then relink=1; fi
done
if [ "$relink" = 1 ]; then
  say "linking ddivideo.wasm"
  export_flags=()
  for name in "${exports[@]}"; do export_flags+=("-Wl,--export=$name"); done
  # `-v` records the exact linker command and versions (link.out), to
  # compare builds on different hosts.
  "$wasi/bin/clang" -v -Oz -msimd128 -mexec-model=reactor --no-wasm-opt \
    "-ffile-prefix-map=$root=ddi" -I"$build" -I"$src" \
    -o "$wasm.linked" "$shim" "$packets" "${libs[@]}" \
    "${export_flags[@]}" -Wl,--stack-first -Wl,-z,stack-size=1048576 \
    -Wl,--max-memory=1073741824 -Wl,--strip-all -Wl,--keep-section=target_features \
    2> "$build/link.out" || { cat "$build/link.out" >&2; die "linking failed"; }
  say "optimising with wasm-opt ($binaryen_version)"
  "$binaryen/bin/wasm-opt" -Oz "$wasm.linked" -o "$wasm.part" 2>> "$build/link.out" ||
    { cat "$build/link.out" >&2; die "wasm-opt failed"; }
  rm -f "$wasm.linked"
  built="$wasm.part"
else
  built="$wasm"
fi

# 7. The notices served next to the module: what is in it, where its
# source is, and the licence texts (FFmpeg's LGPL from its source, zlib's,
# wasi-libc's and those of the parts of it the module links).
notice() {
  printf '\n\n==== %s ====\n\n' "$1"
  cat "$2"
}
{
  cat <<NOTICE
ddivideo.wasm, the video decoder of Dito Dito Infinito
($(value project_url))

This module uses code of FFmpeg $ffmpeg_version ($(value homepage)), licensed
under the GNU Lesser General Public License version 2.1 or later (below).
Its source, unmodified, is served next to this file as $ffmpeg_file,
SHA-256 $ffmpeg_sha:
the release at $ffmpeg_url
(tag $ffmpeg_tag, $(value source_url)).
The module is built from it by video/build.sh in the repository above,
with the configure flags in video/FFMPEG.toml (also listed in
ddivideo.json next to this file), and linked with video/ddivideo.c and video/packets.c, the
repository's own code (MIT). Running the build script rebuilds the module,
also against a modified FFmpeg.

It also contains:
- zlib $zlib_version (https://zlib.net/), under the zlib licence (below);
- wasi-libc from wasi-sdk $wasi_version, under the MIT licence (below),
  with parts from musl (MIT), cloudlibc (BSD-2-Clause) and dlmalloc
  (public domain, CC0): https://github.com/WebAssembly/wasi-libc
  at commit $libc_commit;
- LLVM's compiler-rt builtins from wasi-sdk $wasi_version, under the Apache
  License 2.0 with LLVM Exceptions (no notice needed in compiled form).
NOTICE
  notice "FFmpeg: GNU Lesser General Public License version 2.1" "$src/COPYING.LGPLv2.1"
  notice "zlib $zlib_version: licence" "$zsrc/LICENSE"
  notice "wasi-libc: licensing" "$libc/LICENSE"
  notice "wasi-libc: MIT licence" "$libc/LICENSE_MIT"
  notice "musl (in wasi-libc): COPYRIGHT" "$libc/musl_COPYRIGHT"
  notice "cloudlibc (in wasi-libc): licence" "$libc/cloudlibc_LICENSE"
} > "$out/ddivideo.LICENSE.txt"

# 8. Description for the site and the credits, written before the module
# is moved into place so the two always match.
size=$(wc -c < "$built" | tr -d ' ')
gz_size=$(gzip -9 -c "$built" | wc -c | tr -d ' ')
{
  printf '{\n'
  printf '  "name": "FFmpeg",\n'
  printf '  "version": %s,\n' "$(json_string "$ffmpeg_version")"
  printf '  "tag": %s,\n' "$(json_string "$ffmpeg_tag")"
  printf '  "license": %s,\n' "$(json_string "$(value license)")"
  printf '  "source_url": %s,\n' "$(json_string "$(value source_url)")"
  printf '  "wasi_sdk": %s,\n' "$(json_string "$wasi_version")"
  printf '  "zlib": %s,\n' "$(json_string "$zlib_version")"
  printf '  "binaryen": %s,\n' "$(json_string "$binaryen_version")"
  printf '  "configure": ['
  sep=""
  for flag in "${configure_flags[@]}"; do
    printf '%s\n    %s' "$sep" "$(json_string "$flag")"
    sep=","
  done
  printf '\n  ],\n'
  printf '  "exports": ['
  sep=""
  for name in "${exports[@]}"; do
    printf '%s%s' "$sep" "$(json_string "$name")"
    sep=", "
  done
  printf '],\n'
  printf '  "size": %s,\n' "$size"
  printf '  "gzip_size": %s,\n' "$gz_size"
  printf '  "sha256": %s\n' "$(json_string "$(sha256 "$built")")"
  printf '}\n'
} > "$out/ddivideo.json.part"
if [ "$built" != "$wasm" ]; then mv "$built" "$wasm"; fi
mv "$out/ddivideo.json.part" "$out/ddivideo.json"
say "$wasm: $size bytes, $gz_size gzipped"
