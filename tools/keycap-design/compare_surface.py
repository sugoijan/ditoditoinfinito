"""Controlled comparison: previous bend, Blender decal, measured 2D bend."""
import json
from PIL import Image, ImageDraw
from pack import OUT,text

measurements=json.loads((OUT/'surface-measurements.json').read_text())
sheet=Image.new('RGBA',(1800,1280),'#0a090d')
text(sheet,(56,32),'Legend curvature / checked against Blender',34,bold=True)
text(sheet,(56,86),'Same lowercase decal and camera. Blender also applies its view transform.',21,'#a39c95')
for x,title in [(200,'Previous bend'),(735,'Printed on the 3D surface'),(1300,'Matched 2D bend')]:
    text(sheet,(x,146),title,24,'#ff6ea0')
for row,(label,ui_width) in enumerate([('e',32),('tab',48),('enter',64)]):
    size=(ui_width*8,256)
    blank=Image.open(OUT/f'reference-{label}-blank.png').convert('RGBA').resize(size,Image.Resampling.LANCZOS)
    ink=Image.open(OUT/f'decal-{label}.png').convert('RGBA').resize(size,Image.Resampling.LANCZOS)
    ref=Image.open(OUT/f'reference-{label}.png').convert('RGBA').resize(size,Image.Resampling.LANCZOS)
    center=(ui_width/2-7.5)*8
    a=measurements[label]['half_face_ui_px']
    coeffs=[-1.4/(ui_width/2-6)**2/8, None, -0.568/a**2/8]
    for col,q in enumerate(coeffs):
        if q is None: image=ref
        else:
            strips=[]
            for x in range(size[0]):
                dx=x-60
                offset=q*dx*(dx-2*center)
                strips.append(((x,0,x+1,256),(x,-offset,x,256-offset,x+1,256-offset,x+1,-offset)))
            image=blank.copy()
            image.alpha_composite(ink.transform(size,Image.Transform.MESH,strips,Image.Resampling.BICUBIC))
        image=image.resize((round(image.width*.78),200),Image.Resampling.LANCZOS)
        sheet.alpha_composite(image,(round(350+550*col-image.width/2),200+row*228))
ImageDraw.Draw(sheet).line((56,912,1744,912),fill='#26222e',width=2)
text(sheet,(56,950),'space / centered, shorter convex cap',26,bold=True)
space=Image.open(OUT/'reference-space.png').convert('RGBA')
sheet.alpha_composite(space,(58,1000))
text(sheet,(650,1050),'64 px wide (previously 96 px)',24,'#a39c95')
text(sheet,(650,1094),'About 0.45–0.50 px of visible bend on the other labels.',21,'#a39c95')
sheet.convert('RGB').save(OUT/'surface-comparison.png')
