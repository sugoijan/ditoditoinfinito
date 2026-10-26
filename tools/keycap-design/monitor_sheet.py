"""Assemble the real Blender renders without changing their lighting/color."""
from pathlib import Path
from PIL import Image,ImageDraw,ImageFont
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'target/scratch/keycaps/monitor'
STUDIO=ROOT/'target/scratch/keycaps/materials'
FONT=ROOT/'assets/fonts/IosevkaTerm-SemiBold-latin.ttf'
REGULAR=ROOT/'assets/fonts/IosevkaTerm-Regular-latin.ttf'

def main():
    im=Image.new('RGB',(3300,1850),'#100e16'); draw=ImageDraw.Draw(im)
    def text(x,y,s,size=30,color='#ece7e5',bold=False):
        draw.text((x,y),s,font=ImageFont.truetype(str(FONT if bold else REGULAR),size),fill=color,anchor='lt')
    text(80,52,'Lighting / studio vs a monitor',54,bold=True)
    text(80,130,'Same deep-plum materials, key shapes and viewing angle in every column',28,'#a8a1b2')
    for col,(title,desc) in enumerate([
        ('Previous studio','Softbox + back strip + front fill'),
        ('Monitor / centered','One uniform white rectangular panel'),
        ('Monitor + faint room light','Same screen with soft ambient fill'),
    ]):
        text(80+col*1080,218,title,34,bold=True)
        text(80+col*1080,274,desc,25,'#a8a1b2')
    for row,(code,finish) in enumerate([('B1','Satin'),('B2','Semi-gloss'),('B3','Gloss')]):
        for col,path in enumerate([STUDIO/f'{code}.png',OUT/f'centered-{code}.png',OUT/f'room-{code}.png']):
            x,y=60+col*1080,340+row*410
            draw.rounded_rectangle((x,y,x+1050,y+385),radius=20,fill='#191620',outline='#302a3b',width=2)
            text(x+30,y+26,f'{code} / {finish}',30,bold=True)
            render=Image.open(path).convert('RGBA').resize((1020,408),Image.Resampling.LANCZOS)
            im.paste(render,(x+15,y+10),render)
    text(80,1620,'Panel: 60 × 34 cm · center 40 cm behind the keys and 27 cm above the desk',30,bold=True)
    text(80,1680,'Upright screen facing the player · middle: screen only · right: screen + faint neutral room light',27,'#a8a1b2')
    text(80,1740,'Illustrative screen brightness; no calibrated display luminance. App assets remain unchanged.',27,'#a8a1b2')
    im.save(OUT/'lighting-comparison-full.png')
    im.resize((2200,1233),Image.Resampling.LANCZOS).save(OUT/'lighting-comparison.png')

if __name__=='__main__': main()
