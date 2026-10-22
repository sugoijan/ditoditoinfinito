#!/usr/bin/env python3
"""Draw pump's center panel for bundled dance skins that lack one.

A dance skin draws a pump layout's center panel from its own `Center`
elements, else from Down's (docs/plans/pump.md). Skins whose upstream has
center art bundle it as it is; this script derives the center for the
others from each skin's own art, so that it keeps the skin's look:

- Delta: its tap note sheet, receptor and explosion reshaped from the arrow
  into a rounded square (`Reshape`): the square's rim takes the arrow's
  rim at the same depth where it faces the same way, its inside the
  arrow's own pixels, so the rim's shading, the glossy bands and their
  animation and the receptor's carbon weave carry over.

Standard library only. `--check` compares the files written with what this
script would write (decoded pixels for PNGs) and fails on a difference.

    python3 tools/noteskin-centers/generate.py [--check]
"""

import argparse
import bisect
import math
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SKINS = ROOT / "assets" / "noteskins" / "dance"


# --- PNG ------------------------------------------------------------------


def read_png(path):
    """An 8-bit RGB or RGBA PNG's (width, height, rows of RGBA tuples)."""
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path}: not a PNG")
    pos, idat, header = 8, b"", None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
    width, height, depth, color, _, _, interlace = header
    if depth != 8 or color not in (2, 6) or interlace:
        raise ValueError(f"{path}: only 8-bit, non-interlaced RGB(A) is read")
    channels = 4 if color == 6 else 3
    raw = zlib.decompress(idat)
    stride = width * channels
    rows, prev, i = [], bytearray(stride), 0
    for _ in range(height):
        kind, line = raw[i], bytearray(raw[i + 1 : i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - channels] if x >= channels else 0
            b = prev[x]
            c = prev[x - channels] if x >= channels else 0
            if kind == 1:
                line[x] = (line[x] + a) & 255
            elif kind == 2:
                line[x] = (line[x] + b) & 255
            elif kind == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif kind == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[x] = (line[x] + pred) & 255
        prev = line
        if channels == 4:
            rows.append([tuple(line[x : x + 4]) for x in range(0, stride, 4)])
        else:
            rows.append([tuple(line[x : x + 3]) + (255,) for x in range(0, stride, 3)])
    return width, height, rows


def png_bytes(rows):
    """RGBA rows as a PNG, unfiltered, compressed as zlib's level 9."""
    height, width = len(rows), len(rows[0])
    raw = bytearray()
    for row in rows:
        raw.append(0)
        for px in row:
            raw.extend(px)

    def chunk(kind, body):
        return (
            struct.pack(">I", len(body))
            + kind
            + body
            + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def crop(rows, x0, y0, w, h):
    return [row[x0 : x0 + w] for row in rows[y0 : y0 + h]]


def paste(sheet, tile, x0, y0):
    for y, row in enumerate(tile):
        sheet[y0 + y][x0 : x0 + len(row)] = row


# --- Reshaping an arrow's art into another outline ---------------------------

# How far outside an outline its soft edge reaches, in pixels.
EDGE = 2.5


def signed_distances(mask):
    """Each pixel's distance from the outline of `mask` (rows of bools),
    positive inside: the distance between pixel centers to the nearest
    pixel on the other side, less half a pixel."""
    h, w = len(mask), len(mask[0])
    inside, outside = [], []
    for y in range(h):
        for x in range(w):
            # Only pixels next to the other side can be the nearest one.
            edge = any(
                0 <= x + dx < w and 0 <= y + dy < h and mask[y + dy][x + dx] != mask[y][x]
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))
            )
            if edge:
                (inside if mask[y][x] else outside).append((x, y))
    out = []
    for y in range(h):
        row = []
        for x in range(w):
            others = outside if mask[y][x] else inside
            if not others:
                row.append(float(w + h) if mask[y][x] else -float(w + h))
                continue
            d = min((x - ox) ** 2 + (y - oy) ** 2 for ox, oy in others) ** 0.5 - 0.5
            row.append(d if mask[y][x] else -d)
        out.append(row)
    return out


def rounded_square(cx, cy, half, radius):
    """The signed distance (positive inside) from a square `half` wide each
    way around (cx, cy), its corners rounded by `radius`, at pixel centers."""

    def sdf(x, y):
        px, py = abs(x + 0.5 - cx) - (half - radius), abs(y + 0.5 - cy) - (half - radius)
        outer = math.hypot(max(px, 0.0), max(py, 0.0)) + min(max(px, py), 0.0)
        return radius - outer

    return sdf


def premultiplied(px):
    a = px[3] / 255.0
    return (px[0] * a, px[1] * a, px[2] * a, px[3])


def straight(p):
    a = p[3]
    if a <= 0.0:
        return (0, 0, 0, 0)
    k = 255.0 / a
    return tuple(min(255, max(0, round(v))) for v in (p[0] * k, p[1] * k, p[2] * k, a))


class Reshape:
    """Where each pixel of a new outline takes its color from, in art drawn
    for another outline (the source's signed `distances`), the same for
    every frame of a sheet of that art:

    - on the rim (less than `core` deep), the mean of the source's rim
      pixels in `rows` at about the same depth, weighted by how nearly
      their edge faces the same way, so a top edge takes the source's top
      edges' shading and a bottom edge its bottom edges' (`rows` leave out
      parts whose rim is drawn otherwise, as an arrow's shaft);
    - deeper, the source's own pixel there when it is at least `deep`
      inside (past the rim and its seams where the arrow's parts meet),
      else the nearest one that is, in the same row (or the nearest row
      that has one), a multiple of `period` pixels away (a pattern keeps
      its phase). From `blend` deep on, the rim fades into it."""

    def __init__(self, distances, sdf, size, rows, core, blend, deep, period):
        self.d = distances
        h, w = len(distances), len(distances[0])
        self.w, self.h = w, h
        rim = []
        for y in rows:
            for x in range(w):
                v = distances[y][x]
                if -EDGE <= v < core:
                    rim.append((x, y, v, self.normal(lambda xx, yy: self.at(xx, yy), x, y)))
        self.plan = []
        for y in range(size[1]):
            line = []
            for x in range(size[0]):
                depth = sdf(x, y)
                if depth >= core:
                    line.append(([(self.fill(x, y, deep, period), 1.0)], None))
                    continue
                if depth < -EDGE:
                    line.append(([], None))
                    continue
                normal = self.normal(sdf, x, y)
                weights = []
                for sx, sy, v, n in rim:
                    facing = n[0] * normal[0] + n[1] * normal[1]
                    if facing <= 0.0:
                        continue
                    wgt = facing**8 * math.exp(-(((v - depth) / 0.5) ** 2))
                    if wgt > 1e-4:
                        weights.append(((sx, sy), wgt))
                weights.sort(key=lambda e: -e[1])
                weights = weights[:32]
                total = sum(wgt for _, wgt in weights) or 1.0
                mix = [(at, wgt / total) for at, wgt in weights]
                fade = None
                if depth >= blend:
                    t = (depth - blend) / (core - blend)
                    fade = (self.fill(x, y, deep, period), t)
                line.append((mix, fade))
            self.plan.append(line)

    def at(self, x, y):
        return self.d[min(max(y, 0), self.h - 1)][min(max(x, 0), self.w - 1)]

    @staticmethod
    def normal(field, x, y):
        """The outward normal of a signed distance `field` (positive
        inside) at a pixel."""
        gx, gy = field(x - 1, y) - field(x + 1, y), field(x, y - 1) - field(x, y + 1)
        n = math.hypot(gx, gy) or 1.0
        return (gx / n, gy / n)

    def fill(self, x, y, deep, period):
        y = min(max(y, 0), self.h - 1)
        for sy in sorted(range(self.h), key=lambda r: (abs(r - y), r)):
            row = self.d[sy]
            for step in range(0, self.w + 1, period):
                for sx in (x - step, x + step) if step else (x,):
                    if 0 <= sx < self.w and row[sx] >= deep:
                        return (sx, sy)
        raise ValueError("the source has no pixel that deep")

    def apply(self, frame):
        out = []
        for line in self.plan:
            row = []
            for mix, fade in line:
                acc = [0.0, 0.0, 0.0, 0.0]
                for (sx, sy), wgt in mix:
                    p = premultiplied(frame[sy][sx])
                    for i in range(4):
                        acc[i] += p[i] * wgt
                if fade is not None:
                    (sx, sy), t = fade
                    p = premultiplied(frame[sy][sx])
                    acc = [a * (1.0 - t) + b * t for a, b in zip(acc, p)]
                row.append(straight(acc))
            out.append(row)
        return out


def profile_by_depth(rows, distances, step=0.25):
    """The source's mean premultiplied color at each depth (by `step`): its
    look as a function of the distance from its outline alone."""
    sums = {}
    for y, row in enumerate(rows):
        for x, px in enumerate(row):
            k = round(distances[y][x] / step)
            s = sums.setdefault(k, [0.0, 0.0, 0.0, 0.0, 0])
            p = premultiplied(px)
            for i in range(4):
                s[i] += p[i]
            s[4] += 1
    keys = sorted(sums)
    table = {k: tuple(v / sums[k][4] for v in sums[k][:4]) for k in keys}

    def at(depth):
        k = min(max(depth / step, keys[0]), keys[-1])
        i = bisect.bisect_right(keys, k) - 1
        lo, hi = keys[i], keys[min(i + 1, len(keys) - 1)]
        a, b = table[lo], table[hi]
        t = 0.0 if hi == lo else (k - lo) / (hi - lo)
        return straight(tuple(a[i] * (1 - t) + b[i] * t for i in range(4)))

    return at


# --- Delta ---------------------------------------------------------------

# Delta's arrow fills 64 px cells; its center's square, as wide as the
# arrow's head is tall, with corners as round as its shaft's end.
DELTA_HALF = 24.0
DELTA_RADIUS = 9.0
# Delta's rim is three pixels wide, fading into the note's fill from two
# pixels in; the fill is taken from where the arrow is deeper than the
# seams its rim leaves inside it, where its shaft meets its head. The rim
# is the head's (the shaft's is lighter), all round.
DELTA_CORE = 3.5
DELTA_BLEND = 2.0
DELTA_DEEP = 6.5
DELTA_RIM_ROWS = range(24, 64)
# The receptor's carbon weave repeats every three pixels in a row.
DELTA_PERIOD = 3


def delta():
    skin = SKINS / "Delta"
    files = {}
    sdf = rounded_square(32.0, 32.0, DELTA_HALF, DELTA_RADIUS)

    # The tap note sheet: 12 frames of animation by 8 note colors.
    w, h, sheet = read_png(skin / "_down tap note 12x8.png")
    cols, rows = 12, 8
    cw, ch = w // cols, h // rows
    first = crop(sheet, 0, 0, cw, ch)
    shape = Reshape(
        signed_distances([[px[3] >= 128 for px in row] for row in first]),
        sdf,
        (cw, ch),
        DELTA_RIM_ROWS,
        DELTA_CORE,
        DELTA_BLEND,
        DELTA_DEEP,
        1,
    )
    out = [[(0, 0, 0, 0)] * w for _ in range(h)]
    for r in range(rows):
        for c in range(cols):
            paste(out, shape.apply(crop(sheet, c * cw, r * ch, cw, ch)), c * cw, r * ch)
    files[skin / "_center tap note 12x8.png"] = out

    # The receptor: 4 frames of the beat's flash.
    w, h, receptor = read_png(skin / "Receptor 4x1.png")
    cw = w // 4
    first = crop(receptor, 0, 0, cw, h)
    shape = Reshape(
        signed_distances([[px[3] >= 128 for px in row] for row in first]),
        sdf,
        (cw, h),
        DELTA_RIM_ROWS,
        DELTA_CORE,
        DELTA_BLEND,
        DELTA_DEEP,
        DELTA_PERIOD,
    )
    out = [[(0, 0, 0, 0)] * w for _ in range(h)]
    for c in range(4):
        paste(out, shape.apply(crop(receptor, c * cw, 0, cw, h)), c * cw, 0)
    files[skin / "_center receptor 4x1.png"] = out

    # The explosion: a white glow around the arrow, hollow inside; the
    # square takes its look at each depth.
    w, h, glow = read_png(skin / "Down Tap Explosion Bright.png")
    distances = signed_distances([[px[3] >= 128 for px in row] for row in glow])
    at = profile_by_depth(glow, distances)
    square = rounded_square(w / 2.0, h / 2.0, DELTA_HALF, DELTA_RADIUS)
    files[skin / "Center Tap Explosion Bright.png"] = [
        [at(square(x, y)) for x in range(w)] for y in range(h)
    ]
    return files


# --- Writing ---------------------------------------------------------------


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--check", action="store_true", help="fail if a file differs")
    args = parser.parse_args()
    files = {}
    files.update(delta())
    stale = []
    for path, content in sorted(files.items()):
        rel = path.relative_to(ROOT)
        if isinstance(content, str):
            data = content.encode()
            same = path.exists() and path.read_bytes() == data
        else:
            data = png_bytes(content)
            same = path.exists() and read_png(path)[2] == content
        if args.check:
            if not same:
                stale.append(rel)
        elif not same:
            path.write_bytes(data)
            print(f"wrote {rel}")
    if stale:
        for rel in stale:
            print(f"stale: {rel}", file=sys.stderr)
        print("run python3 tools/noteskin-centers/generate.py", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
