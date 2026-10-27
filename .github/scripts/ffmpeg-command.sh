#!/usr/bin/env bash
# The ffmpeg command CI's Linux jobs test against (the decoders'
# references, the generated clips, the DanOni check's independent Opus
# decode): a static build from BtbN/FFmpeg-Builds, pinned by its release
# tag and the asset's SHA-256, its ffmpeg and ffprobe installed into
# /usr/local/bin. Ubuntu's `ffmpeg` package came through the Azure apt
# mirror, whose throttling took the install from 30 s to 9, 18 and 28
# minutes within a day (2026-10-27); GitHub serves its releases fast.
# The build tracks the major of the helper's pinned FFmpeg
# (video/FFMPEG.toml). Month-end autobuilds are kept for two years: to
# move, pick one, copy the assets' lines from its checksums.sha256 here
# and bump the tag. macOS jobs keep brew's ffmpeg.
#
# Usage: .github/scripts/ffmpeg-command.sh
set -euo pipefail

tag=autobuild-2026-09-30-13-08
case "$(uname -m)" in
  x86_64)
    asset=ffmpeg-n9.0.2-17-g2a571b6068-linux64-gpl-9.0.tar.xz
    sha256=68ee646831adaae2495618346f3bba94ff207ff83bbd34d643e7004730d66269
    ;;
  aarch64)
    asset=ffmpeg-n9.0.2-17-g2a571b6068-linuxarm64-gpl-9.0.tar.xz
    sha256=91afcd7695d4bbbb9e0057aa10baaf9875ec4e1a97303f9059ec971336a2ab46
    ;;
  *)
    echo "ffmpeg-command: no build for $(uname -m)" >&2
    exit 1
    ;;
esac

sudo=sudo
if [ "$(id -u)" = 0 ]; then sudo=; fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -sSfL --retry 3 -o "$work/$asset" \
  "https://github.com/BtbN/FFmpeg-Builds/releases/download/$tag/$asset"
(cd "$work" && echo "$sha256  $asset" | sha256sum -c -)
tar -xJf "$work/$asset" -C "$work" --strip-components=2 \
  "${asset%.tar.xz}/bin/ffmpeg" "${asset%.tar.xz}/bin/ffprobe"
$sudo install -m 755 "$work/ffmpeg" "$work/ffprobe" /usr/local/bin/
ffmpeg -version | head -n 1
