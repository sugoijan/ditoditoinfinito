#!/usr/bin/env python3
# A synthetic dance noteskin for tests (docs/plans/noteskins.md, step 4):
# images drawn here, so it may be committed and imported anywhere.
#   python3 noteskin/tests/make_test_skin.py <dir>/TestSkin
# Taps: a vivid 4x4 sheet, one hue per frame (TapNoteAnimationLength 4).
# Holds: striped bodies (green held, grey not) and rounded bottom caps;
# a white outline receptor, a white glow explosion, a red mine.
import math, os, struct, sys, zlib

def png(path, w, h, pixel):
    raw = bytearray()
    for y in range(h):
        raw.append(0)
        for x in range(w):
            raw.extend(bytes(pixel(x, y)))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) \
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b"")
    open(path, "wb").write(data)

def hue(i, n):
    h = i / n * 6
    c = [0, 0, 0]
    k = int(h) % 6
    f = h - int(h)
    table = [(1, f, 0), (1 - f, 1, 0), (0, 1, f), (0, 1 - f, 1), (f, 0, 1), (1, 0, 1 - f)]
    return [int(255 * v) for v in table[k]]

def arrow(u, v):
    # A down-pointing arrow in the unit square (u, v in 0..1).
    head = v > 0.45 and abs(u - 0.5) < (1.0 - v) * 0.9
    shaft = v <= 0.5 and abs(u - 0.5) < 0.16 and v > 0.08
    return head or shaft

out = sys.argv[1]
os.makedirs(out, exist_ok=True)
F = 64
def tap(x, y):
    frame = (y // F) * 4 + x // F
    u, v = (x % F + 0.5) / F, (y % F + 0.5) / F
    return hue(frame, 16) + [255] if arrow(u, v) else [0, 0, 0, 0]
png(f"{out}/Down Tap Note 4x4.png", 4 * F, 4 * F, tap)
png(f"{out}/Down Hold Body Active.png", F, F, lambda x, y: [60, 230, 90, 255] if (y // 8) % 2 == 0 else [30, 150, 60, 255])
png(f"{out}/Down Hold Body Inactive.png", F, F, lambda x, y: [150, 150, 150, 255] if (y // 8) % 2 == 0 else [100, 100, 100, 255])
def cap(color):
    def p(x, y):
        u, v = (x + 0.5) / F, (y + 0.5) / 32
        inside = v < 0.5 or (u - 0.5) ** 2 + (v - 0.5) ** 2 * 4 < 0.25
        return color + [255] if inside else [0, 0, 0, 0]
    return p
png(f"{out}/Down Hold BottomCap Active.png", F, 32, cap([60, 230, 90]))
png(f"{out}/Down Hold BottomCap Inactive.png", F, 32, cap([150, 150, 150]))
def receptor(x, y):
    # A 3 px outline of the arrow: inside it, with a pixel outside nearby.
    def inside(px, py):
        return 0 <= px < F and 0 <= py < F and arrow((px + 0.5) / F, (py + 0.5) / F)
    if not inside(x, y):
        return [0, 0, 0, 0]
    edge = any(not inside(x + dx, y + dy) for dx in range(-3, 4) for dy in range(-3, 4))
    return [255, 255, 255, 255] if edge else [0, 0, 0, 0]
png(f"{out}/Down Receptor.png", F, F, receptor)
def glow(x, y):
    r = math.hypot((x + 0.5) / F - 0.5, (y + 0.5) / F - 0.5) * 2
    return [255, 255, 255, int(255 * max(0.0, 1 - r))]
png(f"{out}/Down Tap Explosion Bright.png", F, F, glow)
png(f"{out}/Down Tap Mine.png", F, F, lambda x, y: [230, 40, 40, 255] if math.hypot(x - 31.5, y - 31.5) < 22 else [0, 0, 0, 0])
open(f"{out}/metrics.ini", "w").write("""[Global]
FallbackNoteSkin=default

[NoteDisplay]
TapNoteAnimationLength=4
TapNoteAnimationIsVivid=1
HoldHeadAnimationLength=4
HoldHeadAnimationIsVivid=1
TapNoteNoteColorTextureCoordSpacingY=0
TapFakeNoteColorTextureCoordSpacingY=0
HoldHeadNoteColorTextureCoordSpacingY=0
""")
