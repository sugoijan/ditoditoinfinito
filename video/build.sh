#!/usr/bin/env bash
# Builds the software video decoder (docs/plans/video-backgrounds.md, step 1):
# downloads the wasi-sdk and FFmpeg release pinned in video/FFMPEG.toml into
# target/video/, checks their SHA-256, configures and builds FFmpeg's
# libraries and links video/ddivideo.c into target/video/ddivideo.wasm, with
# target/video/ddivideo.json describing it. Each stage is skipped when its
# output is current, so running it again is cheap.
#
# Hosts: macOS and Linux, arm64 or x86_64. Needs bash, curl, tar with xz,
# GNU make and a host C compiler (configure's checks). No containers.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
toml="$root/video/FFMPEG.toml"
shim="$root/video/ddivideo.c"
out="$root/target/video"

say() { printf 'video: %s\n' "$*"; }
die() { printf 'video: %s\n' "$*" >&2; exit 1; }

# `key = "value"` from FFMPEG.toml.
value() {
  local v
  v=$(sed -n "s/^$1 = \"\(.*\)\"\$/\1/p" "$toml")
  [ -n "$v" ] || die "$1 missing from $toml"
  printf '%s\n' "$v"
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# Downloads `url` to `dest` unless it is there with the right hash.
fetch() {
  local url=$1 dest=$2 want=$3 got
  if [ -f "$dest" ] && [ "$(sha256 "$dest")" = "$want" ]; then return; fi
  say "downloading $url"
  curl -fL --retry 3 --progress-bar -o "$dest.part" "$url"
  got=$(sha256 "$dest.part")
  if [ "$got" != "$want" ]; then
    rm -f "$dest.part"
    die "SHA-256 mismatch for $url: got $got, FFMPEG.toml pins $want"
  fi
  mv "$dest.part" "$dest"
}

# Unpacks `tarball` into `dir` (its single top-level directory) once per hash.
unpack() {
  local tarball=$1 dir=$2 want=$3
  if [ -f "$dir/.ddi-sha256" ] && [ "$(cat "$dir/.ddi-sha256")" = "$want" ]; then return; fi
  say "unpacking $(basename "$tarball")"
  rm -rf "$dir"
  tar -xf "$tarball" -C "$(dirname "$dir")"
  [ -d "$dir" ] || die "$(basename "$tarball") did not unpack to $dir"
  printf '%s\n' "$want" > "$dir/.ddi-sha256"
}

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) host=arm64-macos ;;
  Darwin-x86_64) host=x86_64-macos ;;
  Linux-x86_64) host=x86_64-linux ;;
  Linux-aarch64 | Linux-arm64) host=arm64-linux ;;
  *) die "unsupported host $(uname -s)-$(uname -m)" ;;
esac
host_key=$(printf '%s' "$host" | tr '-' '_')

ffmpeg_version=$(value version)
ffmpeg_tag=$(value tag)
ffmpeg_url=$(value tarball_url)
ffmpeg_sha=$(value tarball_sha256)
wasi_version=$(value wasi_sdk_version)
wasi_url=$(value wasi_sdk_url)
wasi_sha=$(value "wasi_sdk_sha256_$host_key")

configure_flags=()
while IFS= read -r flag; do
  configure_flags+=("$flag")
done < <(sed -n '/^configure = \[/,/^\]/s/^ *"\(.*\)",\{0,1\}$/\1/p' "$toml")
[ "${#configure_flags[@]}" -gt 0 ] || die "no configure flags in $toml"

mkdir -p "$out/downloads"

# 1. Toolchain.
wasi_name="wasi-sdk-$wasi_version-$host"
fetch "$wasi_url$wasi_name.tar.gz" "$out/downloads/$wasi_name.tar.gz" "$wasi_sha"
wasi="$out/$wasi_name"
unpack "$out/downloads/$wasi_name.tar.gz" "$wasi" "$wasi_sha"

# 2. FFmpeg source.
ffmpeg_file=$(basename "$ffmpeg_url")
fetch "$ffmpeg_url" "$out/downloads/$ffmpeg_file" "$ffmpeg_sha"
src="$out/ffmpeg-$ffmpeg_version"
unpack "$out/downloads/$ffmpeg_file" "$src" "$ffmpeg_sha"

# 3. Configure, out of tree. Paths are mapped away so the module does not
# depend on where it was built.
build="$out/ffmpeg-build"
mkdir -p "$build"
configure=(
  "$src/configure"
  "--cc=$wasi/bin/clang" "--ar=$wasi/bin/ar" "--ranlib=$wasi/bin/ranlib"
  "--nm=$wasi/bin/nm" "--strip=$wasi/bin/strip"
  "${configure_flags[@]}"
  "--extra-cflags=-ffile-prefix-map=$src=ffmpeg -ffile-prefix-map=$build=ffmpeg"
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

# 4. The four libraries (make itself skips what is current).
jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)
say "building libraries (make -j$jobs)"
make -C "$build" -j"$jobs" > "$build/make.out" 2>&1 ||
  { tail -n 30 "$build/make.out" >&2; die "make failed (log: $build/make.out)"; }

# 5. Link the shim. A reactor (`_initialize`, no `_start`); the stack sits
# below the data so an overflow traps instead of corrupting memory, and the
# memory is capped so a hostile file cannot take more than 1 GiB.
libs=("$build/libavformat/libavformat.a" "$build/libavcodec/libavcodec.a"
  "$build/libavutil/libavutil.a")
wasm="$out/ddivideo.wasm"
exports=()
while IFS= read -r name; do
  exports+=("$name")
done < <(sed -n 's/^EXPORT [^(]*[ *]\(ddi_[a-z_]*\)(.*/\1/p' "$shim")
relink=0
[ -f "$wasm" ] || relink=1
for input in "$shim" "$root/video/build.sh" "$toml" "${libs[@]}"; do
  if [ "$input" -nt "$wasm" ]; then relink=1; fi
done
if [ "$relink" = 1 ]; then
  say "linking ddivideo.wasm"
  export_flags=()
  for name in "${exports[@]}"; do export_flags+=("-Wl,--export=$name"); done
  "$wasi/bin/clang" -Oz -msimd128 -mexec-model=reactor \
    "-ffile-prefix-map=$root=ddi" -I"$build" -I"$src" \
    -o "$wasm.part" "$shim" "${libs[@]}" \
    "${export_flags[@]}" -Wl,--stack-first -Wl,-z,stack-size=1048576 \
    -Wl,--max-memory=1073741824 -Wl,--strip-all
  mv "$wasm.part" "$wasm"
fi

# 6. The module is LGPL-2.1-or-later: its licence text, from the FFmpeg
# source, is served next to it.
cp "$src/COPYING.LGPLv2.1" "$out/ddivideo.LICENSE.txt"

# 7. Description for the site and the credits.
json_string() { printf '"%s"' "$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g')"; }
size=$(wc -c < "$wasm" | tr -d ' ')
gz_size=$(gzip -9 -c "$wasm" | wc -c | tr -d ' ')
{
  printf '{\n'
  printf '  "name": "FFmpeg",\n'
  printf '  "version": %s,\n' "$(json_string "$ffmpeg_version")"
  printf '  "tag": %s,\n' "$(json_string "$ffmpeg_tag")"
  printf '  "license": %s,\n' "$(json_string "$(value license)")"
  printf '  "source_url": %s,\n' "$(json_string "$(value source_url)")"
  printf '  "wasi_sdk": %s,\n' "$(json_string "$wasi_version")"
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
  printf '  "sha256": %s\n' "$(json_string "$(sha256 "$wasm")")"
  printf '}\n'
} > "$out/ddivideo.json"
say "$wasm: $size bytes, $gz_size gzipped"
