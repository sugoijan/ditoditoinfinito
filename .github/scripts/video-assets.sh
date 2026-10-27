#!/usr/bin/env bash
# The FFmpeg builds CI downloads instead of building: the web's decoder
# module (video/build.sh) and the desktop helper's static libraries
# (video/build-native.sh), kept as assets of the repository's rolling
# `build-inputs` pre-release, each named by the SHA-256 of its recipe
# (`--recipe` of its build script) and, for the libraries, its target.
# A pure function of pinned sources belongs in a permanent store: the
# Actions cache evicts under its cap and after a week unused, which had
# the module rebuilt on cold runs (docs/plans/video-backgrounds.md,
# "CI"). Built by .github/workflows/build-inputs.yml when the recipes
# change; deploy.yml, danoni.yml and release.yml fetch, and build when
# nothing is there yet.
#
# Usage: .github/scripts/video-assets.sh <command> module|native
#   name    the asset's file name for the current recipe (and host target)
#   exists  exit 0 when the release has it
#   fetch   download and unpack it into target/video/ (exit 2 when absent)
#   pack    make it from target/video/ as <name> in the current directory
#   upload  pack it and attach it to the release (made when missing)
# Needs gh (GH_TOKEN in CI) and, for `native`, rustc (the host's target).
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
toml="$root/video/FFMPEG.toml"
# shellcheck source=video/lib.sh
. "$root/video/lib.sh"

release=build-inputs
command=${1:-}
kind=${2:-}
case "$kind" in module | native) ;; *) die "usage: video-assets.sh <name|exists|fetch|pack|upload> module|native" ;; esac
built="$root/target/video"
ffmpeg_file=$(basename "$(value tarball_url)")
zlib_file=$(basename "$(value zlib_url)")

name() {
  case $kind in
    module) printf 'ddivideo-%s.tar.gz\n' "$(bash "$root/video/build.sh" --recipe)" ;;
    native)
      local target
      target=$(rustc -vV | sed -n 's/^host: //p')
      [ -n "$target" ] || die "rustc names no host target"
      printf 'ffmpeg-native-%s-%s.tar.gz\n' "$target" "$(bash "$root/video/build-native.sh" --recipe)" ;;
  esac
}

# What the asset holds, relative to target/video/: the built files and the
# source tarballs the site (xtask stage-video) and the package (package.py)
# serve beside them.
files() {
  case $kind in
    module) printf '%s\n' ddivideo.wasm ddivideo.json ddivideo.LICENSE.txt "downloads/$ffmpeg_file" ;;
    native)
      local target
      target=$(rustc -vV | sed -n 's/^host: //p')
      printf '%s\n' "native/$target/lib" "native/$target/include" "native/$target/ffmpeg.json" \
        "native/$target/ddivideo-native.LICENSE.txt" "downloads/$ffmpeg_file" "downloads/$zlib_file" ;;
  esac
}

exists() {
  local want=$1
  gh release view "$release" --json assets --jq '.assets[].name' 2>/dev/null | grep -qx "$want"
}

asset=$(name)
case "$command" in
  name)
    printf '%s\n' "$asset" ;;
  exists)
    if exists "$asset"; then say "$release has $asset"; else say "$release lacks $asset"; exit 2; fi ;;
  fetch)
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    if ! gh release download "$release" --pattern "$asset" --dir "$tmp" 2> "$tmp/err" || [ ! -f "$tmp/$asset" ]; then
      say "no $asset in the $release release ($(tr '\n' ' ' < "$tmp/err")): building"
      exit 2
    fi
    mkdir -p "$built"
    # `-m`: unpacked now, so xtask's stage-video (which compares the
    # module's time with its sources') sees it as current.
    tar -xzmf "$tmp/$asset" -C "$built"
    while IFS= read -r f; do [ -e "$built/$f" ] || die "$asset lacks $f"; done < <(files)
    say "$asset unpacked into target/video" ;;
  pack)
    members=()
    while IFS= read -r f; do
      [ -e "$built/$f" ] || die "target/video/$f is missing (build it first)"
      members+=("$f")
    done < <(files)
    tar -czf "$asset" -C "$built" "${members[@]}"
    say "$asset: $(du -h "$asset" | cut -f1)" ;;
  upload)
    "$0" pack "$kind"
    if ! gh release view "$release" >/dev/null 2>&1; then
      say "making the $release release"
      gh release create "$release" --prerelease --latest=false --title "Build inputs" \
        --notes "Not a release of the game. The FFmpeg builds the repository's CI downloads instead of building (.github/scripts/video-assets.sh): the web's decoder module and the desktop helper's static libraries, each named by the SHA-256 of its recipe. Assets of recipes no longer in use may be deleted."
    fi
    gh release upload "$release" "$asset" --clobber
    say "$asset uploaded" ;;
  *)
    die "unknown command ${command:-(none)}" ;;
esac
