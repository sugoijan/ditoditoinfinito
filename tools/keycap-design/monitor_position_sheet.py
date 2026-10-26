"""Compare monitor-position renders on the same background and at equal scale."""
import json
from pathlib import Path
from PIL import Image,ImageDraw,ImageFont
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'target/scratch/keycaps/monitor-positions'
FONT=ROOT/'assets/fonts/IosevkaTerm-SemiBold-latin.ttf'
REGULAR=ROOT/'assets/fonts/IosevkaTerm-Regular-latin.ttf'

def main():
    im=Image.new('RGB',(3300,1390),'#100e16'); draw=ImageDraw.Draw(im)
    def text(x,y,s,size=30,color='#ece7e5',bold=False):
        draw.text((x,y),s,font=ImageFont.truetype(str(FONT if bold else REGULAR),size),fill=color,anchor='lt')
    text(80,52,'Monitor position / taming the Space reflection',52,bold=True)
    text(80,130,'Deep-plum semi-gloss + faint room light · same panel brightness, exposure and camera',28,'#a8a1b2')
    records=json.loads((OUT/'positions.json').read_text())
    for i,r in enumerate(records):
        x,y=60+(i%3)*1080,230+(i//3)*480
        draw.rounded_rectangle((x,y,x+1050,y+450),radius=20,fill='#191620',outline='#302a3b',width=2)
        tilt=r['tilt_back_degrees']
        orientation='upright' if tilt==0 else f'{abs(tilt)}° '+('back' if tilt>0 else 'forward')
        text(x+30,y+24,f"{r['code']} / {r['title']}",34,bold=True)
        text(x+30,y+80,f"{r['distance_m']*100:.0f} cm behind · {r['center_height_m']*100:.0f} cm high · {orientation}",25,'#a8a1b2')
        render=Image.open(OUT/f"{r['code']}.png").convert('RGBA').resize((1020,408),Image.Resampling.LANCZOS)
        im.paste(render,(x+15,y+65),render)
    text(80,1220,'Distances and heights refer to the screen center relative to the keys and desktop.',27,'#a8a1b2')
    text(80,1280,'60 × 34 cm uniform white panel · material and app assets unchanged',27,'#a8a1b2')
    im.save(OUT/'position-comparison-full.png')
    im.resize((2200,927),Image.Resampling.LANCZOS).save(OUT/'position-comparison.png')

if __name__=='__main__': main()
