"""Pack approved shared-satin renders; no Blender or image tools at game build time."""
from pathlib import Path
from PIL import Image, ImageEnhance
ROOT=Path(__file__).resolve().parents[2]
SRC=ROOT/'target/scratch/controller-hints'
FAMILIES=['xbox','playstation','dualsense','nintendo','generic']
CONTROLS=['south','east','west','north','lb','rb','lt','rt','back','start']
CELL=64
atlas=Image.new('RGBA',(29*CELL,10*CELL))
def load(path):
    with Image.open(path) as im:return im.convert('RGBA').resize((CELL,CELL),Image.Resampling.LANCZOS)
raw=load(SRC/'runtime/generic-raw.png')
for row,family in enumerate(FAMILIES):
    neutral=load(SRC/f'runtime/{family}-dpad.png')
    for col in range(29):
        image=raw.copy()
        if col<10:image=load(SRC/f'shared/{family}-{CONTROLS[col]}.png')
        if 12<=col<=27:
            image=neutral.copy();mask=col-12
            arms=[(1,'dpad',(19,0,45,31)),(2,'dpad_down',(19,33,45,64)),(4,'dpad_left',(0,19,29,46)),(8,'dpad_right',(35,19,64,46))]
            for bit,name,rect in arms:
                if mask&bit:
                    part=load(SRC/f'shared/{family}-{name}.png').crop(rect)
                    image.paste(part,rect)
        atlas.paste(image,(col*CELL,row*2*CELL))
        if 12<=col<=27:
            pressed=image.copy()
            for bit,_,rect in arms:
                if mask&bit:
                    part=ImageEnhance.Brightness(image.crop(rect)).enhance(.77)
                    pressed.paste(part,(rect[0],rect[1]+2),part)
        else:
            pressed=Image.new('RGBA',(CELL,CELL))
            face=ImageEnhance.Brightness(image).enhance(.82)
            if col in (6,7):face=face.resize((CELL,CELL-2),Image.Resampling.LANCZOS)
            pressed.paste(face,(0,2))
        atlas.paste(pressed,(col*CELL,(row*2+1)*CELL))
atlas.save(ROOT/'assets/ui/controllers.png',optimize=True)
print('controllers.png',atlas.size)
