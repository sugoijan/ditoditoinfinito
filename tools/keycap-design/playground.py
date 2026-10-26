"""Fill the literal playground fragment with the project's SVGs and fonts."""
import argparse
import base64
import json
from pathlib import Path

from fontTools.pens.basePen import BasePen
from fontTools.ttLib import TTFont

from charcoal_vector import ROOT, body


class Outline(BasePen):
    """Flatten small print contours before applying the surface projection."""

    def __init__(self, glyphs):
        super().__init__(glyphs)
        self.contours = []

    def _moveTo(self, point):
        self.contours.append([point])

    def _lineTo(self, point):
        self.contours[-1].append(point)

    def _qCurveToOne(self, control, end):
        start = self._getCurrentPoint()
        for i in range(1, 13):
            t = i / 12
            self._lineTo(tuple((1-t)**2*start[j] + 2*(1-t)*t*control[j] + t*t*end[j] for j in (0, 1)))

    def _curveToOne(self, first, second, end):
        start = self._getCurrentPoint()
        for i in range(1, 13):
            t = i / 12
            self._lineTo(tuple((1-t)**3*start[j] + 3*(1-t)**2*t*first[j] + 3*(1-t)*t*t*second[j] + t**3*end[j] for j in (0, 1)))

    def _closePath(self):
        pass


parser = argparse.ArgumentParser()
parser.add_argument('output', type=Path)
args = parser.parse_args()
fragment = Path(__file__).with_suffix('.html').read_text()
for weight in ('Regular', 'SemiBold'):
    font = (ROOT / f'assets/fonts/IosevkaTerm-{weight}-latin.ttf').read_bytes()
    fragment = fragment.replace(f'__{weight.upper()}_FONT__', base64.b64encode(font).decode())
pieces = []
for finish in ('a1', 'a2', 'light', 'gray', 'deep-gray'):
    for row, (width, space) in enumerate(((32, False), (44, False), (56, False), (68, False), (64, True))):
        name = f'{finish}-{"space" if space else row}'
        pieces.append(f'<g id="ddi-body-{name}">{body(width, finish, space, "ddi-study-" + name)}</g>')
fragment = fragment.replace('__CAP_BODIES__', ''.join(pieces))
outlines = {}
for style, face in [('regular', 'Regular'), ('regular-italic', 'Italic'),
                    ('medium', 'Medium'), ('medium-italic', 'MediumItalic'),
                    ('semibold', 'SemiBold'), ('semibold-italic', 'SemiBoldItalic')]:
    font = TTFont(ROOT / f'assets/fonts/IosevkaTerm-{face}-latin.ttf')
    glyphs, cmap = font.getGlyphSet(), font.getBestCmap()
    outlines[style] = {}
    for char in sorted(set('tabenter/escEspace')):
        pen = Outline(glyphs)
        glyphs[cmap[ord(char)]].draw(pen)
        outlines[style][char] = [[[round(x/font['head'].unitsPerEm, 5), round(y/font['head'].unitsPerEm, 5)] for x, y in contour] for contour in pen.contours]
fragment = fragment.replace('__PRINT_OUTLINES__', json.dumps(outlines, separators=(',', ':')))
args.output.write_text(fragment)
print(f'{args.output}: {len(fragment.encode())} bytes')
