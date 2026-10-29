#!/usr/bin/env python3
"""Generate the play HUD's R8 atlas (Pillow, NumPy and SciPy, authoring only).

Digits are the unmodified bundled Iosevka Term SemiBold at 64 px, sampled
at 4x before distance conversion. Ten 64x96 cells, advance 32, baseline 76,
16 px padding; signed distance encoded as 0.5 + distance/32. Two 640x128
rows below contain the existing title's lettering and protected outlines.
"""
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFont
from scipy.ndimage import distance_transform_edt

ROOT = Path(__file__).resolve().parents[2]
atlas = Image.new("L", (640, 352))
font = ImageFont.truetype(str(ROOT / "assets/fonts/IosevkaTerm-SemiBold-latin.ttf"), 256)
for digit in range(10):
    mask = Image.new("L", (256, 384))
    ImageDraw.Draw(mask).text((64, 304), str(digit), font=font, fill=255, anchor="ls")
    inside = np.asarray(mask) >= 128
    # Half a high-resolution pixel puts the boundary between samples.
    signed = np.where(inside, distance_transform_edt(inside) - .5,
                      .5 - distance_transform_edt(~inside)) / 4
    distance = Image.fromarray(np.uint8(np.clip(.5 + signed / 32, 0, 1) * 255))
    atlas.paste(distance.resize((64, 96), Image.Resampling.BOX), (digit * 64, 0))

title = Image.open(ROOT / "assets/title/dosis.png").getchannel("A")
w, total = title.size
h = total // 4
letters = title.crop((0, h * 3, w, h * 4))
rings = Image.new("L", (w, h))
for row in range(3):
    rings = ImageChops.lighter(rings, title.crop((0, row * h, w, (row + 1) * h)))
for image, top in [(letters, 96), (rings, 224)]:
    # The title atlas has 38 units of padding at 2x; the accepted still
    # mark uses 26. The draw rectangle restores its 974:176 aspect ratio.
    image = image.crop((24, 24, w - 24, h - 24))
    atlas.paste(image.resize((640, 128), Image.Resampling.LANCZOS), (0, top))
(ROOT / "assets/ui/play-hud.r8").write_bytes(atlas.tobytes())
