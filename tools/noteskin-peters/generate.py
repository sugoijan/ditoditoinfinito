#!/usr/bin/env python3
"""Fill Peters-DDR's center gap using its original model materials.

The 64 px cell matches the original arrow's logical size. A chamfered,
hollow square has a black silhouette, colored bevel and fine inner lip.
Tap/lift models reuse upstream texture coordinates, materials and animation.
Simple supporting shapes remain SVG; there is no new color system.
"""
import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2] / "assets/noteskins/dance"
BASE = ROOT / "Peters-DDR"
VIVID = ROOT / "Peters-DDR-Vivid"
# Outer and inner octagons, opposite windings: transparent center.
RING = "M14 7H50L57 14V50L50 57H14L7 50V14Z M19 18L18 19V45L19 46H45L46 45V19L45 18Z"
LIP = "M19 16H45L48 19V45L45 48H19L16 45V19Z"
OUTLINE = "M14 7H50L57 14V50L50 57H14L7 50V14Z"


def octagon(radius, cut):
    r, c = radius, radius - cut
    return [(-c, r), (c, r), (r, c), (r, -c), (c, -r), (-c, -r), (-r, -c), (-r, c)]


def ring(name, outer, inner, material, uv, flags=0):
    points = octagon(*outer) + octagon(*inner)
    lines = [f'"{name}" 0 {material}', str(len(points))]
    for x, y in points:
        u, v = uv(x, y)
        lines.append(f'{flags} {x:.6f} {y:.6f} 0.000000 {u:.6f} {v:.6f} -1')
    lines += ['1', '0.000000 0.000000 1.000000', '16']
    for i in range(8):
        j = (i + 1) % 8
        lines += [f'0 {i} {j+8} {j} 0 0 0 1', f'0 {i} {i+8} {j+8} 0 0 0 1']
    return '\n'.join(lines)


def model(source, vivid=False):
    # These projections and vertex flags come from the source arrow's
    # outer edge, colored band and inner facets. Keep its material block
    # verbatim so the same INI textures scroll at exactly the same rates.
    materials = source.read_text().split('Materials: 2', 1)[1]
    if vivid:
        outer_uv = lambda x, y: (.750977 + x * .0073242, .46875 - y / 128)
        color_uv = lambda x, y: (0, (28-y) / 118.4)
        lip_uv = lambda x, y: (.198399, .001033)
        outer_material, color_material, flags = 1, 0, 0
    else:
        outer_uv = lambda x, y: (.125 - x / 480, .498619 - y / 120.6667)
        color_uv = lambda x, y: (.5475, .309693 - y / 120.6667)
        lip_uv = lambda x, y: (.509639, .312455)
        outer_material, color_material, flags = 0, 0, 3
    meshes = [ring('Square outer frame', (26, 5), (24, 4), outer_material, outer_uv, flags),
              ring('Square color', (24, 4), (18, 3), color_material, color_uv),
              ring('Square inner frame', (18, 3), (16, 2), outer_material, lip_uv)]
    return '// MilkShape 3D ASCII\n\nFrames: 30\nFrame: 1\n\nMeshes: 3\n' + '\n'.join(meshes) + '\n\nMaterials: 2' + materials


def sheet(columns, rows, colors, title):
    defs, cells = [], []
    for row in range(rows):
        for col in range(columns):
            key = f"c{row}_{col}"
            top, bottom = colors(col, row)
            defs.append(f'<linearGradient id="{key}" x1="0" y1="0" x2="0" y2="1">'
                        f'<stop offset="0" stop-color="{top}"/>'
                        '<stop offset="0.46" stop-color="#ffffff"/>'
                        f'<stop offset="0.58" stop-color="{bottom}"/>'
                        f'<stop offset="1" stop-color="{bottom}"/></linearGradient>')
            cells.append(f'<g transform="translate({col * 64} {row * 64})">'
                         f'<use href="#ring" fill="url(#{key})"/>'
                         '<use href="#outline"/><use href="#lip"/></g>')
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{columns * 64}" height="{rows * 64}" '
            f'viewBox="0 0 {columns * 64} {rows * 64}">\n<title>{title}</title>\n<defs>\n'
            f'<path id="ring" d="{RING}" fill-rule="evenodd"/>\n'
            f'<path id="outline" d="{OUTLINE}" fill="none" stroke="#080c12" stroke-width="2"/>\n'
            f'<path id="lip" d="{LIP}" fill="none" stroke="#edf7ff" stroke-width="1"/>\n'
            + "\n".join(defs) + '\n</defs>\n' + "\n".join(cells) + '\n</svg>\n')


def outputs():
    yield BASE / "_center tap note model.txt", model(BASE / "_down tap note model.txt")
    yield BASE / "_center tap lift model.txt", model(BASE / "_down tap lift model.txt")
    yield VIVID / "_center tap note model.txt", model(VIVID / "_down tap note model.txt", vivid=True)
    for name, top, bottom in [("hold active", "#44ec32", "#e6ff50"),
                              ("hold inactive", "#40bd30", "#a4cb3f"),
                              ("roll active", "#44ec32", "#e6ff50"),
                              ("roll inactive", "#40bd30", "#a4cb3f")]:
        yield BASE / f"_center {name}.svg", sheet(1, 1, lambda c, r: (top, bottom), f"Peters-DDR square {name}")
    # Two beat-clock states, with the same press timing as the arrows.
    yield BASE / "_center receptor 2x1.svg", sheet(2, 1, lambda c, r: ("#909ba2", "#303c45") if c == 0 else ("#616a72", "#222e38"), "Peters-DDR square receptor")
    for name, width in [("dim", 6), ("bright", 10)]:
        yield BASE / f"_center flash {name}.svg", (f'<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">\n'
            f'<path d="{LIP}" fill="none" stroke="#ffffff" stroke-width="{width}"/>\n</svg>\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--output-root", type=Path, default=ROOT.parents[2] / "target/scratch/peters/model-svg-study", help="Historical study output; does not regenerate the bundled raster skin")
    args = parser.parse_args()
    stale = []
    for source_path, text in outputs():
        path = args.output_root / source_path.relative_to(ROOT)
        if args.check:
            if not path.exists() or path.read_text() != text:
                stale.append(str(path))
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
    if stale:
        raise SystemExit("Regenerate with python3 tools/noteskin-peters/generate.py: " + ", ".join(stale))


if __name__ == "__main__":
    main()
