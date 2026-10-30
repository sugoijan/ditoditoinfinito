#!/usr/bin/env python3
"""The README's animated title (`docs/logo.svg`, `docs/logo-light.svg`):
the start menu's title as SVG, its rings painted by Infinito's Vivid
material rebuilt from SVG filter primitives and animated with SMIL, so
that it moves wherever an SVG image does (GitHub's README among them)
with no script.

The lettering, rings and clearance are the loader's (`site/boot-art-data.js`,
boot_title.py; drawn as `site/boot-art.js` draws them). The paint follows
`site/boot-art.js`'s port of the material, step for step:

- the value noise is feTurbulence's (Perlin, one octave), its lattice
  NOISE_SPAN times the value noise's and its contrast NOISE_GAIN times
  feTurbulence's, so that the two match in spread and in how fast they
  decorrelate (measured; they differ in character, not in scale);
- the domain warp `w = p + 1.35 (noise - 0.5)` is feDisplacementMap,
  whose formula it is (over the whole source: WebKit clips a primitive's
  input to the primitive's own subregion);
- `q = (0.18 wx + 0.12 wy + 1.25 folds) 2 - 97/360` is a linear gradient
  (the source graphic's red and green) plus the folds' noise, displaced
  together;
- the noises' drift: each initial's paint is drawn DISTANCE away from
  its user space's origin, where a small change of a noise's frequency
  moves it nearly as a translation (its scale changes by a few percent);
  the frequency swings and comes back, so the drift does too. Stitched
  tiles moved by feOffset would loop a constant drift, but Chrome's
  stitching leaves seams, which the warp tears open;
- `fract(q)`, the ramp and the seam's blend are tables (two palette
  offsets and the blend's weight, continuous wherever their weight is
  not zero), then the palette looked up twice and mixed: the shader's
  `mix(ramp(phase + .43 q), ramp(phase + .43 (q - 1)), u)`;
- the palette window and its four-second cycle are a color matrix whose
  offset is animated, then the closed eight-stop palette as tables, each
  stop on a table node; each initial's quarter-turn offset rides in the
  source's blue.

One filter serves the three initials (each a <use> of one rect, its fill
the initial's gradient), computed only over its ring's box. The field is
the shader's for each initial, centered on it, as in the game.

Run by generate.py (after boot_title.py); alone:
python3 tools/title-design/readme_logo.py
"""
import json
import math
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]

GAP, OUTLINE = 12, 12
REACH = GAP + OUTLINE + 2
TEXT = {"dark": "#f2eee9", "light": "#14111a"}  # the theme's Text; its dark for light pages

# Field units: a 96-unit capital is 64 field units; the shader's p is
# field / 1.089 * 0.021, so one p unit is U title units.
U = 1.089 / 0.021 * 1.5
NOISE_SPAN = 1.756  # Perlin lattice / value lattice at equal decorrelation
NOISE_GAIN = 2.09  # value noise's spread / feTurbulence's
DISPLACE = 1.35 * U * NOISE_GAIN
MARGIN = 50  # the warp's reach, title units: 1.35 U * 0.45 rounded up
# The paint's distance from its origin, and the direction the noises
# drift in (toward the origin as their frequencies rise).
DISTANCE = 30000
TOWARD = (-0.35, -1)
SPAN = 0.43  # the palette window
# Smooths the warped source (browsers dither its gradient and sample
# it at the nearest pixel) before the folds magnify it to grain (title
# units).
BLUR = 1.2


def offsets(q):
    """The two palette offsets and the weight of the second at q, each
    continuous wherever its weight is not zero: before 0.2 both the ramp
    (the weight falling from one to zero meanwhile), then the second
    going where the blend wants it from 0.5."""
    a1 = SPAN * q
    if q <= 0.2:
        a2 = a1
    elif q <= 0.5:
        a2 = SPAN * 0.2 + (-SPAN / 2 - SPAN * 0.2) * (q - 0.2) / 0.3
    else:
        a2 = SPAN * (q - 1)
    if q <= 0.1:
        u = 1
    elif q <= 0.2:
        u = (0.2 - q) / 0.1
    elif q <= 0.5:
        u = 0
    else:
        s = (q - 0.5) / 0.5
        u = s * s * (3 - 2 * s)
    return a1, a2, u


def n(v):
    """A number at three decimals, as short as it goes."""
    s = f"{v:.3f}".rstrip("0").rstrip(".")
    s = s.replace("0.", ".", 1) if s.startswith(("0.", "-0.")) else s
    return "0" if s in ("-0", "") else s


def rect(x, y, w, h):
    return f'x="{n(x)}" y="{n(y)}" width="{n(w)}" height="{n(h)}"'


def grow(box, by):
    x, y, w, h = box
    return (x - by, y - by, w + 2 * by, h + 2 * by)


def noise(seed, lattice, speed, period, box, matrix, name):
    """A noise over `box` drifting at about `speed` title units a second
    for `period` seconds, then back."""
    f = 1 / lattice
    swing = speed * period / DISTANCE
    return (f'<feTurbulence type="fractalNoise" baseFrequency="{f:.6g}" seed="{seed}" {rect(*box)}>'
            f'<animate attributeName="baseFrequency" values="{f:.6g};{f * (1 + swing):.6g};{f:.6g}" '
            f'dur="{2 * period}s" calcMode="spline" keySplines=".45 0 .55 1;.45 0 .55 1" repeatCount="indefinite"/>'
            f'</feTurbulence>'
            f'<feColorMatrix values="{matrix}" {rect(*box)} result="{name}"/>')


def build(data, variant):
    palette = [[int(h[i:i + 2], 16) for i in (1, 3, 5)] for h in data["palette"]]
    bounds = data["bounds"]
    initials = data["initials"]
    shapes = dict(data["glyphs"])

    # The paint's center, far from its origin.
    length = math.hypot(*TOWARD)
    ox, oy = -TOWARD[0] / length * DISTANCE, -TOWARD[1] / length * DISTANCE
    # The rect each initial's paint is computed on, centered on it: the
    # widest initial's ring box (the output), and the warp's reach
    # around it (the displaced source).
    hw = max((bounds[i][2] - bounds[i][0]) / 2 for i in initials) + REACH
    hh = max((bounds[i][3] - bounds[i][1]) / 2 for i in initials) + REACH
    ring = (ox - hw, oy - hh, 2 * hw, 2 * hh)
    source = grow(ring, MARGIN)

    # q's encoding: A (the argument before fract) as k A + o over the
    # source's box; its range is the gradient's over the box plus the
    # folds' noise.
    grad = (0.36 * source[2] / 2 + 0.24 * source[3] / 2) / U
    a_mid = -8.712 / 1.089 * 0.021 * 0.24 - 97 / 360 + 2.5 * 0.5
    k = 0.96 / (2 * (grad + 2.5 * 0.45))
    o = 0.5 - k * a_mid
    gain = k * 2.5 * NOISE_GAIN  # the folds' noise, centered on .5
    row = f"{n(gain)} 0 0 0 {n(0.5 - gain / 2)}"

    # The shader's drift speeds (title units a second; T = 0.3 s):
    # dx = noise(0.6 p + (T, 0.23 T)), dy = noise(0.6 p + (8.7 - 0.31 T, 2.3 + T)),
    # folds = noise(w + (-0.15 T, 0.37 T)).
    warp_lattice = U / 0.6 * NOISE_SPAN
    speed = lambda x, y, unit: math.hypot(x, y) * 0.3 * unit
    noises = (
        noise(3, U * NOISE_SPAN, speed(0.15, 0.37, U), 37, source,
              f"{row} {row} 0 0 0 0 .5 0 0 0 0 1", "f")
        # Both warps from one turbulence's red and green (independent
        # noises), drifting together.
        + noise(5, warp_lattice, speed(0.65, 0.62, U / 0.6), 23, ring,
                "1 0 0 0 0 0 1 0 0 0 0 0 0 0 0 0 0 0 0 1", "w"))

    # fract(A) to the two offsets and the weight, as tables over the red
    # (and green, the same) of the displaced source.
    nodes = 64
    folded = [offsets(((i / (nodes - 1) - o) / k) % 1) for i in range(nodes)]
    first = " ".join(n(a1 / SPAN) for a1, _, _ in folded)
    second = " ".join(n((a2 + SPAN / 2) / SPAN) for _, a2, _ in folded)
    weight = " ".join(n(u) for _, _, u in folded)
    # x = (phase + offset + a - A0) / WIDTH, the palette as tables over x
    # with a stop on every node; the initial's offset is -n/4 from blue n/2.
    a0, stops = -6 / 7, 17
    width = stops / 7
    tables = "".join(
        f'<feFunc{c} type="table" tableValues="'
        + " ".join(n(palette[(i - 6) % 7][j] / 255) for i in range(stops + 1)) + '"/>'
        for j, c in enumerate("RGB"))

    def lookup(at, lo, name):
        """The palette at offset SPAN * red + lo, phase animated."""
        m = lambda phase: " ".join(
            f"{n(SPAN / width)} 0 {n(-0.5 / width)} 0 {n((phase + lo - a0) / width)}" for _ in range(3)) + " 0 0 0 0 1"
        return (f'<feColorMatrix in="{at}" values="{m(1)}" {rect(*ring)}>'
                f'<animate attributeName="values" values="{m(1)};{m(0)}" dur="4s" repeatCount="indefinite"/>'
                f'</feColorMatrix><feComponentTransfer {rect(*ring)} result="{name}">{tables}</feComponentTransfer>')

    filt = (
        f'<filter id="v" filterUnits="userSpaceOnUse" {rect(*source)} color-interpolation-filters="sRGB">'
        + noises
        + f'<feComposite in="SourceGraphic" in2="f" operator="arithmetic" k2="1" k3="1" k4="-.5" {rect(*source)} result="q"/>'
        + f'<feDisplacementMap in="q" in2="w" scale="{n(DISPLACE)}" xChannelSelector="R" yChannelSelector="G" {rect(*source)}/>'
        + f'<feGaussianBlur stdDeviation="{BLUR}" {rect(*ring)} result="d"/>'
        + f'<feComponentTransfer {rect(*ring)} result="a"><feFuncR type="table" tableValues="{first}"/>'
        + f'<feFuncG type="table" tableValues="{weight}"/></feComponentTransfer>'
        + f'<feComponentTransfer in="d" {rect(*ring)} result="b"><feFuncR type="table" tableValues="{second}"/></feComponentTransfer>'
        + f'<feColorMatrix in="a" values="0 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 0 0 0 1" {rect(*ring)} result="u"/>'
        + f'<feColorMatrix in="a" values="0 -1 0 0 1 0 -1 0 0 1 0 -1 0 0 1 0 0 0 0 1" {rect(*ring)} result="v"/>'
        + lookup("a", 0, "c")
        + f'<feComposite in="c" in2="v" operator="arithmetic" k1="1" {rect(*ring)} result="p"/>'
        + lookup("b", -SPAN / 2, "r")
        + f'<feComposite in="r" in2="u" operator="arithmetic" k1="1" {rect(*ring)}/>'
        + f'<feComposite in2="p" operator="arithmetic" k2="1" k3="1" {rect(*ring)}/>'
        + '</filter>')

    # Each initial's source: red and green the gradient along (0.36, 0.24)
    # in p, blue its palette offset.
    d = (0.36, 0.24)
    dl = math.hypot(*d)
    reach = (d[0] * source[2] / 2 + d[1] * source[3] / 2) / dl
    ends = [(ox - d[0] / dl * reach, oy - d[1] / dl * reach), (ox + d[0] / dl * reach, oy + d[1] / dl * reach)]
    red = [0.5 - k * reach * dl / U, 0.5 + k * reach * dl / U]
    gradients = "".join(
        f'<linearGradient id="g{j}" gradientUnits="userSpaceOnUse" '
        f'x1="{n(ends[0][0])}" y1="{n(ends[0][1])}" x2="{n(ends[1][0])}" y2="{n(ends[1][1])}">'
        + "".join(f'<stop offset="{e}" stop-color="rgb({n(r * 100)}%,{n(r * 100)}%,{n(j * 50)}%)"/>'
                  for e, r in zip((0, 1), red))
        + '</linearGradient>'
        for j in range(len(initials)))

    # The view: the rings' reach around the initials, the lettering's
    # edge elsewhere.
    edge = lambda side, pick, out: pick(pick(bounds[i][side] + REACH * out for i in initials),
                                        pick(b[side] + 2 * out for b in bounds))
    x0, y0, x1, y1 = edge(0, min, -1), edge(1, min, -1), edge(2, max, 1), edge(3, max, 1)
    view = (math.floor(x0), math.floor(y0), math.ceil(x1 - x0), math.ceil(y1 - y0))

    glyph = lambda g: f'<use href="#{g[0]}" x="{n(g[1])}" y="{n(g[2])}"/>'
    centers = [((bounds[i][0] + bounds[i][2]) / 2, (bounds[i][1] + bounds[i][3]) / 2) for i in initials]
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{" ".join(map(str, view))}" '
        f'width="{view[2]}" height="{view[3]}" role="img" aria-label="Dito Dito Infinito">'
        f'<defs>'
        + "".join(f'<path id="{k}" d="{d}"/>' for k, d in shapes.items())
        + filt + gradients
        + f'<rect id="r" {rect(*source)} filter="url(#v)"/>'
        + f'<mask id="m" maskUnits="userSpaceOnUse" x="{view[0]}" y="{view[1]}" width="{view[2]}" height="{view[3]}">'
        + f'<g fill="#fff" stroke="#fff" stroke-width="{2 * (GAP + OUTLINE)}" stroke-linejoin="round">'
        + "".join(glyph(data["uses"][i]) for i in initials) + '</g>'
        + f'<g fill="#000" stroke="#000" stroke-width="{2 * GAP}" stroke-linejoin="round">'
        + "".join(glyph(u) for u in data["uses"]) + '</g></mask></defs>'
        + '<g mask="url(#m)">'
        + "".join(f'<use href="#r" x="{n(cx - ox)}" y="{n(cy - oy)}" fill="url(#g{j})"/>'
                  for j, (cx, cy) in enumerate(centers))
        + f'</g><g fill="{TEXT[variant]}">'
        + "".join(glyph(u) for u in data["uses"]) + '</g></svg>\n')


def main():
    js = (ROOT / "site/boot-art-data.js").read_text()
    data = json.loads(js[js.index("{"):js.rindex("}") + 1])
    (ROOT / "docs/logo.svg").write_text(build(data, "dark"))
    (ROOT / "docs/logo-light.svg").write_text(build(data, "light"))


if __name__ == "__main__":
    main()
