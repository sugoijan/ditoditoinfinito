#!/usr/bin/env bash
# Generates small synthetic clips of the codecs and pixel formats packs use
# (FFmpeg's test pattern: ours, never third-party video), decodes each with
# the ffmpeg command as the reference and checks the module against it with
# video/check.mjs: frames, timestamps, seeking, colour tags. Needs ffmpeg
# with libx264 and Node 22 or later; `just video-check` runs it.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
dir=${1:-$root/target/video/clips}
mkdir -p "$dir"
failed=()

# clip <name> <check.mjs options> -- <ffmpeg output options>
clip() {
  local name=$1 size=$2 checks=()
  shift 2
  while [ "$1" != "--" ]; do checks+=("$1"); shift; done
  shift
  ffmpeg -hide_banner -loglevel error -y -f lavfi -i "testsrc2=size=$size:rate=30" -t 2 \
    "$@" "$dir/$name"
  ffmpeg -hide_banner -loglevel error -y -i "$dir/$name" -fps_mode passthrough \
    -pix_fmt yuv420p -f rawvideo "$dir/$name.yuv"
  node --no-warnings "$root/video/check.mjs" "$dir/$name" --reference "$dir/$name.yuv" \
    --png "$dir/$name.png" ${checks[@]+"${checks[@]}"} || failed+=("$name")
}

# What packs ship (docs/research/video-backgrounds.md, section 1): Xvid and
# H.264 in AVI, raw MPEG-2 streams named .avi. B-frames on, as encoders do.
clip xvid.avi 320x240 --colorspace 601 --range limited -- \
  -c:v mpeg4 -vtag XVID -bf 2 -q:v 4
clip h264.avi 320x240 --min-psnr 99 --colorspace 601 --range limited -- \
  -c:v libx264 -profile:v main -pix_fmt yuv420p -vtag H264
clip mpeg2.avi 320x240 --colorspace 601 --range limited -- \
  -c:v mpeg2video -bf 2 -q:v 4 -f mpeg2video
# Converted by the shim itself, against FFmpeg's own (filtered, dithered)
# conversion: hence the lower bounds.
clip h264-444.avi 160x90 --min-psnr 35 -- \
  -c:v libx264 -pix_fmt yuv444p -vtag H264
clip mpeg2-422.avi 320x240 --min-psnr 40 -- \
  -c:v mpeg2video -pix_fmt yuv422p -bf 2 -q:v 4 -f mpeg2video
clip h264-10bit.avi 320x240 --min-psnr 50 -- \
  -c:v libx264 -pix_fmt yuv420p10le -vtag H264
# Colour tags: tagged BT.709 full range; untagged HD is taken as BT.709.
clip h264-709-full.avi 320x240 --min-psnr 99 --colorspace 709 --range full -- \
  -c:v libx264 -pix_fmt yuv420p -colorspace bt709 -color_primaries bt709 \
  -color_trc bt709 -color_range pc -vtag H264
clip h264-720.avi 1280x720 --min-psnr 99 --colorspace 709 --range limited -- \
  -c:v libx264 -preset ultrafast -pix_fmt yuv420p -vtag H264

if [ "${#failed[@]}" -gt 0 ]; then
  echo "video: failed: ${failed[*]}" >&2
  exit 1
fi
echo "video: all clips decoded as ffmpeg decodes them"
