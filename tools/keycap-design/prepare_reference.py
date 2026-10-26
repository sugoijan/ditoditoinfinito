"""Identical theme-font decal masks for the physical/2D warp comparison."""
import json
from PIL import Image, ImageDraw, ImageFont
from pack import OUT,FONT

bounds = {}
for label,width,ui_width in [("e",256,32),("tab",352,48),("enter",544,64),("space",512,64)]:
    image=Image.new("RGBA",(ui_width*8,256))
    draw=ImageDraw.Draw(image)
    inset=(ui_width-27.5)/2 if label=="space" else 7.5
    draw.text((inset*8,78 if label=="space" else 48),label.upper() if len(label)==1 else label,font=ImageFont.truetype(str(FONT),88),fill="#f2eee9",anchor="lt")
    sx=inset*8+len(label)*44+24
    paths=[]
    if label=="enter": paths=[[(56,-24),(56,8),(0,8)],[(20,-12),(0,8),(20,28)]]
    if label=="tab": paths=[[(0,0),(48,0)],[(28,-20),(48,0),(28,20)],[(64,-28),(64,28)]]
    for path in paths: draw.line([(sx+x,80+y) for x,y in path],fill="#f2eee9",width=6,joint="curve")
    image=image.resize((width,256),Image.Resampling.LANCZOS)
    image.save(OUT/f"decal-{label}.png")
    box=image.getbbox()
    bounds[label]=[box[0]-1,box[1]-1,box[2]+1,box[3]+1]
(OUT/"decal-bounds.json").write_text(json.dumps(bounds))
