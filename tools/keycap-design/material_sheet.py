"""Prepare local print masks, or assemble the controlled Blender comparison."""
import json
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'target/scratch/keycaps/materials'
FONT=ROOT/'assets/fonts/IosevkaTerm-SemiBold-latin.ttf'
REGULAR=ROOT/'assets/fonts/IosevkaTerm-Regular-latin.ttf'

def prepare():
    OUT.mkdir(parents=True,exist_ok=True)
    bounds={}
    for label,width,ui_width in [('e',256,32),('enter',544,64),('space',512,64)]:
        im=Image.new('RGBA',(ui_width*8,256)); draw=ImageDraw.Draw(im)
        inset=(ui_width-27.5)/2 if label=='space' else 7.5
        draw.text((inset*8,78 if label=='space' else 48),label.upper() if len(label)==1 else label,
            font=ImageFont.truetype(str(FONT),88),fill='#f2eee9',anchor='lt')
        if label=='enter':
            sx=inset*8+len(label)*44+24
            for path in [[(56,-24),(56,8),(0,8)],[(20,-12),(0,8),(20,28)]]:
                draw.line([(sx+x,80+y) for x,y in path],fill='#f2eee9',width=6,joint='curve')
        im=im.resize((width,256),Image.Resampling.LANCZOS); im.save(OUT/f'decal-{label}.png')
        l,t,r,b=im.getbbox(); bounds[label]=[l-1,t-1,r+1,b+1]
    (OUT/'decal-bounds.json').write_text(json.dumps(bounds))

def sheet(out=OUT, title='Darker keycaps / material study',
          subtitle='Original OEM model · identical neutral studio lighting · warm-white surface printing'):
    im=Image.new('RGB',(3300,2050),'#100e16'); draw=ImageDraw.Draw(im)
    def text(x,y,s,size=30,color='#ece7e5',bold=False):
        draw.text((x,y),s,font=ImageFont.truetype(str(FONT if bold else REGULAR),size),fill=color,anchor='lt')
    text(80,52,title,54,bold=True)
    text(80,130,subtitle,27,'#a8a1b2')
    for col,(title,desc) in enumerate([('1 / Satin','Broad, soft highlights'),('2 / Semi-gloss','Defined sheen, restrained reflections'),('3 / Gloss','Sharper reflections and a clear coat')]):
        text(80+col*1080,218,title,36,bold=True)
        text(80+col*1080,274,desc,25,'#a8a1b2')
    for row,(letter,name) in enumerate([('A','Charcoal'),('B','Deep plum'),('C','Ink blue')]):
        for col in range(3):
            x,y=60+col*1080,340+row*410
            draw.rounded_rectangle((x,y,x+1050,y+385),radius=20,fill='#191620',outline='#302a3b',width=2)
            text(x+30,y+26,f'{letter}{col+1} / {name}',30,bold=True)
            render=Image.open(out/f'{letter}{col+1}.png').convert('RGBA').resize((1020,408),Image.Resampling.LANCZOS)
            im.paste(render,(x+15,y+10),render)
    text(80,1610,'Reference / previous plum satin',32,bold=True)
    reference=Image.open(out/'reference.png').convert('RGBA').resize((1020,408),Image.Resampling.LANCZOS)
    im.paste(reference,(75,1630),reference)
    text(1210,1725,'Previous base color, rendered under the same lights.',30)
    text(1210,1790,'Compare against A/B/C to judge the darker palette.',27,'#a8a1b2')
    text(1210,1880,'Study only · no app colors or materials changed.',27,'#a8a1b2')
    im.save(out/'material-grid-full.png')
    im.resize((2200,1367),Image.Resampling.LANCZOS).save(out/'material-grid.png')

if __name__=='__main__':
    prepare() if len(sys.argv)>1 and sys.argv[1]=='prepare' else sheet()
