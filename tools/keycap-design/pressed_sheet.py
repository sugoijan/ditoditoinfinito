"""Assemble true-scale pressed stills and a loop from rendered travel steps.

No interpolated or warped cap pixels: every pose is an actual Blender render.
A fixed reference rule is outside the cap artwork to make the motion legible.
"""
from pathlib import Path
from PIL import Image,ImageDraw,ImageFont
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'target/scratch/keycaps/pressed'
FONT=ROOT/'assets/fonts/IosevkaTerm-SemiBold-latin.ttf'
REGULAR=ROOT/'assets/fonts/IosevkaTerm-Regular-latin.ttf'
CROP=(60,150,1480,475)


def text(im,xy,s,size=30,color='#ece7e5',bold=False):
    ImageDraw.Draw(im).text(xy,s,font=ImageFont.truetype(str(FONT if bold else REGULAR),size),fill=color,anchor='lt')


def artwork(code,filename):
    return Image.open(OUT/code/filename).convert('RGBA').crop(CROP)


def panel(im,box,code,title,picture):
    x,y,w,h=box
    ImageDraw.Draw(im).rounded_rectangle((x,y,x+w,y+h),radius=20,fill='#191620',outline='#302a3b',width=2)
    text(im,(x+30,y+22),title,32,bold=True)
    im.paste(picture,(x+10,y+62),picture)


def main():
    im=Image.new('RGB',(3000,1180),'#100e16')
    text(im,(60,40),'A1 / A2 · released and pressed',52,bold=True)
    text(im,(60,115),'4 mm vertical travel · B+D monitor + faint room light · fixed camera',28,'#a8a1b2')
    text(im,(60,180),'Released',32,bold=True)
    text(im,(1560,180),'Pressed / 4 mm down',32,bold=True)
    for row,(code,finish) in enumerate([('A1','Charcoal satin'),('A2','Charcoal semi-gloss')]):
        for col,depth in enumerate([0,4]):
            panel(im,(40+1500*col,235+440*row,1440,405),code,f'{code} / {finish}',artwork(code,f'depth-{depth}.png'))
    text(im,(60,1120),'Cap and printing travel together · shape, material, lighting and exposure stay fixed',27,'#a8a1b2')
    im.save(OUT/'pressed-comparison-full.png')
    im.resize((2100,826),Image.Resampling.LANCZOS).save(OUT/'pressed-comparison.png')
    # Hold endpoints, with actual 1 mm samples for the press and release.
    sequence=[(0,900),(1,70),(2,70),(3,70),(4,650),(3,70),(2,70),(1,70),(0,450)]
    frames=[]
    for depth,duration in sequence:
        canvas=Image.new('RGB',(1500,1020),'#100e16')
        text(canvas,(40,30),'Pressed reference / 4 mm travel',38,bold=True)
        text(canvas,(40,95),'B+D lighting · true-scale motion · same material throughout',22,'#a8a1b2')
        text(canvas,(1140,32),'released' if depth==0 else f'{depth} mm down',26)
        for row,(code,finish) in enumerate([('A1','Satin'),('A2','Semi-gloss')]):
            y=145+410*row
            panel(canvas,(20,y,1460,390),code,f'{code} / Charcoal {finish.lower()}',artwork(code,f'depth-{depth}.png'))
            line_y=y+362
            ImageDraw.Draw(canvas).line((65,line_y,1435,line_y),fill='#494052',width=1)
        text(canvas,(40,982),'The thin rules are fixed visual references; no keyboard deck is modeled.',20,'#a8a1b2')
        frames.append(canvas)
    # One shared palette avoids color flicker between frames.
    palette=frames[0].quantize(colors=256)
    indexed=[frame.quantize(palette=palette,dither=Image.Dither.NONE) for frame in frames]
    indexed[0].save(OUT/'press-release.gif',save_all=True,append_images=indexed[1:],
                    duration=[ms for _,ms in sequence],loop=0,optimize=False,disposal=2)
    # Keep a poster and lossless animation as well as the broadly supported GIF.
    frames[0].save(OUT/'animation-poster.png')
    frames[0].save(OUT/'press-release.png',save_all=True,append_images=frames[1:],
                   duration=[ms for _,ms in sequence],loop=0,disposal=0,blend=0)
    solo=Image.new('RGB',(4500,1160),'#100e16')
    text(solo,(60,40),'Individual presses / neighboring keys stay released',52,bold=True)
    text(solo,(60,115),'Each pressed key travels 4 mm · identical B+D lighting',28,'#a8a1b2')
    for row,code in enumerate(['A1','A2']):
        for col,key in enumerate(['e','enter','space']):
            panel(solo,(40+1500*col,220+440*row,1440,405),code,
                  f'{code} / {key.upper() if key=="e" else key} pressed',artwork(code,f'pressed-{key}.png'))
    solo.save(OUT/'individual-presses-full.png')

if __name__=='__main__': main()
