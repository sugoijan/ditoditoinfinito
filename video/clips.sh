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

encoders=$(ffmpeg -hide_banner -encoders 2>/dev/null | awk '{print $2}')
skipped=()

# clip <name> <size> <check.mjs options> -- <encoder options> [-- <reference options>]
# The encoder is the value after `-c:v`; a clip whose encoder this ffmpeg
# lacks is skipped (and listed at the end).
clip() {
  local name=$1 size=$2 checks=() encode=() reference=() codec=""
  shift 2
  while [ "$1" != "--" ]; do checks+=("$1"); shift; done
  shift
  while [ $# -gt 0 ] && [ "$1" != "--" ]; do
    if [ "$1" = "-c:v" ]; then codec=$2; fi
    encode+=("$1")
    shift
  done
  if [ $# -gt 0 ]; then shift; reference=("$@"); fi
  if [ -n "$codec" ] && ! printf '%s\n' "$encoders" | grep -qx -- "$codec"; then
    skipped+=("$name ($codec)")
    return
  fi
  ffmpeg -hide_banner -loglevel error -y -f lavfi -i "testsrc2=size=$size:rate=30" -t 2 \
    "${encode[@]}" "$dir/$name"
  # Box-filtered chroma and rounding without dither: the conversion the
  # shim does, so 4:4:4, 4:2:2 and 10-bit compare exactly (with FFmpeg 8;
  # older versions round 10-bit slightly differently).
  ffmpeg -hide_banner -loglevel error -y -i "$dir/$name" -fps_mode passthrough \
    -sws_flags area+accurate_rnd+bitexact -sws_dither none \
    ${reference[@]+"${reference[@]}"} -pix_fmt yuv420p -f rawvideo "$dir/$name.yuv"
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
# Dropped frames (Xvid writes them as empty chunks): a seek into the gap
# must land on the frame before it, which is still showing.
clip xvid-gap.avi 320x240 -- \
  -vf "select='not(between(n,10,19))'" -fps_mode passthrough \
  -c:v mpeg4 -vtag XVID -bf 2 -q:v 4
# Converted to 8-bit 4:2:0 by the shim itself (exact here; the bound
# leaves room for other FFmpeg versions' conversion).
clip h264-444.avi 160x90 --min-psnr 60 -- \
  -c:v libx264 -pix_fmt yuv444p -vtag H264
clip mpeg2-422.avi 320x240 -- \
  -c:v mpeg2video -pix_fmt yuv422p -bf 2 -q:v 4 -f mpeg2video
clip h264-10bit.avi 320x240 --min-psnr 50 -- \
  -c:v libx264 -pix_fmt yuv420p10le -vtag H264
# Colour tags: tagged BT.709 full range; untagged HD is taken as BT.709.
clip h264-709-full.avi 320x240 --min-psnr 99 --colorspace 709 --range full -- \
  -c:v libx264 -pix_fmt yuv420p -colorspace bt709 -color_primaries bt709 \
  -color_trc bt709 -color_range pc -vtag H264 -- \
  -vf scale=in_range=full:out_range=full
clip h264-720.avi 1280x720 --min-psnr 99 --colorspace 709 --range limited -- \
  -c:v libx264 -preset ultrafast -pix_fmt yuv420p -vtag H264

# Other containers and codecs: everything StepMania's FFmpeg plays, so the
# common ones are checked (MP4/MOV, Matroska/WebM, Ogg, ASF, FLV, MPEG
# program and transport streams) and so is each way the shim hands frames
# over (RGB and palette images go through swscale: compared with a bound).
clip h264.mp4 320x240 --min-psnr 99 -- -c:v libx264 -pix_fmt yuv420p
clip hevc.mp4 320x240 --min-psnr 99 -- -c:v libx265 -pix_fmt yuv420p -x265-params log-level=error
clip mpeg4.mov 320x240 -- -c:v mpeg4 -bf 2 -q:v 4
clip h264.mkv 320x240 --min-psnr 99 -- -c:v libx264 -pix_fmt yuv420p
clip vp8.webm 320x240 --min-psnr 99 -- -c:v libvpx -b:v 500k
clip vp9.webm 320x240 --min-psnr 99 -- -c:v libvpx-vp9 -b:v 500k -row-mt 0
clip theora.ogv 320x240 -- -c:v libtheora -q:v 6
clip wmv2.wmv 320x240 -- -c:v wmv2 -q:v 4
clip flv1.flv 320x240 -- -c:v flv -q:v 4
clip divx3.avi 320x240 -- -c:v msmpeg4 -q:v 4
clip mpeg1.mpg 320x240 -- -c:v mpeg1video -bf 2 -q:v 4 -f mpeg
clip mpeg2.mpg 320x240 -- -c:v mpeg2video -bf 2 -q:v 4 -f vob
clip h264.ts 320x240 --min-psnr 99 -- -c:v libx264 -pix_fmt yuv420p -f mpegts
clip mjpeg.avi 320x240 -- -c:v mjpeg -q:v 4 -- -vf scale=in_range=full:out_range=full
clip huffyuv.avi 320x240 --min-psnr 99 -- -c:v huffyuv -pix_fmt yuv422p
clip ffv1.mkv 320x240 --min-psnr 99 -- -c:v ffv1
clip cinepak.avi 320x240 --min-psnr 30 -- -c:v cinepak
clip msvideo1.avi 320x240 --min-psnr 30 -- -c:v msvideo1
clip zmbv.avi 320x240 --min-psnr 30 -- -c:v zmbv -pix_fmt bgr0
clip png.mov 320x240 --min-psnr 30 -- -c:v png -pix_fmt rgb24
clip qtrle.mov 320x240 --min-psnr 30 -- -c:v qtrle

if [ "${#skipped[@]}" -gt 0 ]; then
  echo "video: skipped, this ffmpeg has no encoder for: ${skipped[*]}"
fi
if [ "${#failed[@]}" -gt 0 ]; then
  echo "video: failed: ${failed[*]}" >&2
  exit 1
fi
echo "video: all clips decoded as ffmpeg decodes them"
