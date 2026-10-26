"""Compare B, D, their midpoint and their combined monitor position."""
from pathlib import Path
from PIL import Image,ImageDraw,ImageFont
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'target/scratch/keycaps/monitor-blends'
PREVIOUS=ROOT/'target/scratch/keycaps/monitor-positions'
FONT=ROOT/'assets/fonts/IosevkaTerm-SemiBold-latin.ttf'
REGULAR=ROOT/'assets/fonts/IosevkaTerm-Regular-latin.ttf'

def main():
    im=Image.new('RGB',(3240,1660),'#100e16'); draw=ImageDraw.Draw(im)
    def text(x,y,s,size=32,color='#ece7e5',bold=False):
        draw.text((x,y),s,font=ImageFont.truetype(str(FONT if bold else REGULAR),size),fill=color,anchor='lt')
    text(80,50,'Monitor positions / midpoint or B + D',54,bold=True)
    text(80,130,'Deep-plum semi-gloss · same screen brightness, exposure, camera and faint room light',28,'#a8a1b2')
    cases=[('B / tilt only','40 cm behind · 27 cm high · 15° back',PREVIOUS/'B.png'),
           ('D / distance only','60 cm behind · 27 cm high · upright',PREVIOUS/'D.png'),
           ('Midpoint','50 cm behind · 27 cm high · 7.5° back',OUT/'M.png'),
           ('B + D / combined','60 cm behind · 27 cm high · 15° back',OUT/'BD.png')]
    for i,(title,description,path) in enumerate(cases):
        x,y=60+(i%2)*1590,235+(i//2)*650
        draw.rounded_rectangle((x,y,x+1530,y+610),radius=22,fill='#191620',outline='#302a3b',width=2)
        text(x+40,y+28,title,42,bold=True)
        text(x+40,y+98,description,30,'#a8a1b2')
        render=Image.open(path).convert('RGBA')
        im.paste(render,(x+15,y+45),render)
    text(80,1550,'Uniform 60 × 34 cm monitor · height measured at screen center · no app changes',28,'#a8a1b2')
    im.save(OUT/'blend-comparison-full.png')
    im.resize((2160,1107),Image.Resampling.LANCZOS).save(OUT/'blend-comparison.png')

if __name__=='__main__': main()
