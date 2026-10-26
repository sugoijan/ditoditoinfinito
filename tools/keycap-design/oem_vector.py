"""Parametric, original vector approximation of the OEM study.

Geometry is redrawn for each width; corner radii and wall widths are fixed.
This is a design proof, not yet the game's renderer. No bitmap is embedded.
"""
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.pens.basePen import BasePen
from fontTools.pens.svgPathPen import SVGPathPen

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "target/scratch/keycaps/oem"


def legend_path(label, offset):
    """The bundled face's real outlines, curved in vector coordinates."""
    font = TTFont(ROOT / "assets/fonts/IosevkaTerm-SemiBold-latin.ttf")
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    scale = 11 / font["head"].unitsPerEm
    out = SVGPathPen(glyphs, ntos=lambda n: f"{n:.4f}".rstrip("0").rstrip("."))
    class CurvedPen(BasePen):
        def point(self, p):
            x = advance + p[0]*scale
            return x, 11-p[1]*scale+offset(x)
        def _moveTo(self, p): out.moveTo(self.point(p))
        def _lineTo(self, p):
            start = self._getCurrentPoint()
            steps = max(1, int(abs(p[0]-start[0])*scale/.5))
            for i in range(1, steps+1):
                t = i/steps
                out.lineTo(self.point((start[0]+(p[0]-start[0])*t,
                                       start[1]+(p[1]-start[1])*t)))
        def _curveToOne(self, a, b, c): out.curveTo(self.point(a),self.point(b),self.point(c))
        def _qCurveToOne(self, a, b): out.qCurveTo(self.point(a),self.point(b))
        def _closePath(self): out.closePath()
        def _endPath(self): out.endPath()
    advance = 0
    for char in label:
        name = cmap[ord(char)]
        glyphs[name].draw(CurvedPen(glyphs))
        advance += font["hmtx"][name][0]*scale
    return f'<path d="{out.getCommands()}" fill="#f2eee9"/>'


def key(width, row=2, label="", ident="cap"):
    w = width
    label = label.upper() if len(label) == 1 else label.lower()
    space = label == "space"
    inset = (w-len(label)*5.5)/2 if space else 7.5
    # The upper face shifts and tilts with the physical keyboard row.
    top, front = [(2.6, 20.1), (3.4, 21.2), (3.2, 22.2), (2.4, 22.6)][row]
    r = w - 6.1
    top_path = (f'M7.2,{top} Q{w/2},{top+1.2} {w-7.2},{top} '
                f'Q{r},{top} {r},{top+1.4} L{r},{front-1.2} '
                f'Q{r},{front} {r-1.3},{front} Q{w/2},{front+1.1} 7.4,{front} '
                f'Q6.1,{front} 6.1,{front-1.2} L6.1,{top+1.4} Q6.1,{top} 7.2,{top} Z')
    if space:
        top_path = (f'M7.2,3.2 Q{w/2},2.4 {w-7.2},3.2 '
                    f'Q{r},3.2 {r},4.6 V22 Q{r},23.2 {r-1.3},23.2 '
                    f'H7.4 Q6.1,23.2 6.1,22 V4.6 Q6.1,3.2 7.2,3.2 Z')
    center = w/2-inset
    atlas_row = min(3, max(0, int((w/32-1)/.375+.5)))
    face_half = (6.8+atlas_row*4.75)*32/23*w/(32+atlas_row*12)
    quadratic = 0 if space else -0.568/face_half**2
    def offset(x):
        return quadratic*x*(x-2*center)
    ink = legend_path(label, offset)
    sx = len(label)*5.5+3
    if label == "enter":
        ink += f'<path d="M{sx+7},6 V10 H{sx} M{sx+2.5},7.5 L{sx},10 L{sx+2.5},12.5" fill="none" stroke="#f2eee9" stroke-width=".8"/>'
    if label == "tab":
        ink += f'<path d="M{sx},9 H{sx+6} M{sx+3.5},6.5 L{sx+6},9 L{sx+3.5},11.5 M{sx+8},5.5 V12.5" fill="none" stroke="#f2eee9" stroke-width=".8"/>'
    legend = f'<g transform="translate({inset},{top+(3.5 if space else 0)})">{ink}</g>'
    face = (f'<linearGradient id="{ident}-face" x2="0" y2="1"><stop stop-color="#474251"/><stop offset=".45" stop-color="#595463"/><stop offset="1" stop-color="#3c3747"/></linearGradient>' if space else
            f'<linearGradient id="{ident}-face"><stop stop-color="#575160"/><stop offset=".16" stop-color="#443e50"/><stop offset=".52" stop-color="#3b3548"/><stop offset=".88" stop-color="#454050"/><stop offset="1" stop-color="#504a5b"/></linearGradient>')
    return f'''<g>
      <defs>
        <linearGradient id="{ident}-wall" x2="0" y2="1"><stop stop-color="#3f394c"/><stop offset="1" stop-color="#272330"/></linearGradient>
        {face}
        <linearGradient id="{ident}-front" x2="0" y2="1"><stop stop-color="#3d3749"/><stop offset="1" stop-color="#2b2735"/></linearGradient>
        <linearGradient id="{ident}-light" x2="1" y2="1"><stop stop-color="#8c829a" stop-opacity=".45"/><stop offset=".5" stop-color="#655b74" stop-opacity=".12"/><stop offset="1" stop-color="#a59aaf" stop-opacity=".20"/></linearGradient>
      </defs>
      <path d="M6.5,{top+.3} L{w-6.5},{top+.3} L{w-2.4},8.2 Q{w-2},9 {w-2},10 L{w-2},29 Q{w-2},30.4 {w-3.5},30.4 H3.5 Q2,30.4 2,29 V10 Q2,9 2.4,8.2 Z" fill="url(#{ident}-wall)" stroke="#19161f" stroke-width=".35"/>
      <path d="M6.1,{front-1} Q{w/2},{front+2} {r},{front-1} L{w-2.5},29.4 Q{w-2.5},30.1 {w-4},30.1 H4 Q2.5,30.1 2.5,29.4 Z" fill="url(#{ident}-front)"/>
      <path d="{top_path}" fill="url(#{ident}-face)" stroke="url(#{ident}-light)" stroke-width=".55"/>
      {legend}
    </g>'''


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    pieces = ['<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="740" viewBox="0 0 1600 740">',
              '<rect width="1600" height="740" fill="#0a090d"/>',
              '<g fill="#f2eee9" font-family="Iosevka Term Semibold,Iosevka Term,monospace"><text x="56" y="66" font-size="32">OEM / vector width study</text>',
              '<text x="56" y="104" font-size="18" fill="#a39c95">Procedural paths and gradients. Corners and walls stay fixed as the label changes.</text></g>']
    x = 56
    for i, (label, width) in enumerate([("E", 32), ("Tab", 48), ("Enter", 64), ("Space", 64)]):
        pieces.append(f'<g transform="translate({x},155) scale(4)">{key(width, label=label, ident=str(i))}</g>')
        x += width*4 + 65
    for row, name in enumerate(["Function / numbers", "QWERTY", "Home", "Lower"]):
        y = 390 + row*75
        pieces.append(f'<text x="56" y="{y+22}" fill="#a39c95" font-family="Iosevka Term Semibold,Iosevka Term,monospace" font-size="18">{name}</text>')
        x = 315
        for i, (label, width, action) in enumerate([("Tab", 48, "Style"), ("Enter", 64, "Play"), ("/", 32, "Search"), ("Esc", 32, "Back")]):
            pieces.append(f'<g transform="translate({x},{y}) scale(1.25)">{key(width,row,label,f"r{row}k{i}")}</g>')
            x += width*1.25 + 12
            pieces.append(f'<text x="{x}" y="{y+25}" fill="#a39c95" font-family="Iosevka Term Semibold,Iosevka Term,monospace" font-size="16">{action}</text>')
            x += len(action)*8+36
    pieces.append('</svg>')
    (OUT / "vector-study.svg").write_text("\n".join(pieces))
    for row in range(4):
        (OUT / f"vector-r{row+1}.svg").write_text('<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 32 32">'+key(32,row)+"</svg>")


if __name__ == "__main__":
    main()
