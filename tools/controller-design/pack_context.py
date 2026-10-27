"""Assemble the context study from local game captures and original button art."""
import argparse
import base64
import io
import json
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'target/scratch/controller-hints'

def webp(image, quality=92):
    out = io.BytesIO()
    image.save(out, format='WEBP', quality=quality, method=6)
    return 'data:image/webp;base64,' + base64.b64encode(out.getvalue()).decode()

def receptor_art(image):
    # Extract the bright, neutral receptor outline from the local game capture.
    # Transparent margins keep the press treatment off the opaque card background.
    image = image.convert('RGBA')
    image.putdata([(r,g,b,round(255*max(0,min(1,(max(r,g,b)-40)/40))))
                   for r,g,b,_ in image.getdata()])
    return webp(image)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    images = {}
    wanted = {'south','east','west','north','lb','rb','lt','rt','dpad','dpad_all','dpad_left','dpad_down','dpad_right'}
    for record in json.loads((OUT/'shared/study.json').read_text())['icons']:
        if record['control'] not in wanted:
            continue
        with Image.open(OUT/'shared'/record['file']) as im:
            images[record['family']+'-'+record['control']] = {
                'aspect':record['aspect'],
                'src':webp(im.resize((round(112*record['aspect']),112),Image.Resampling.LANCZOS))}
    with Image.open(OUT/'context-select.png') as im:
        select = webp(im.crop((0,0,1280,680)),88)
    with Image.open(OUT/'context-prompt.png') as im:
        receptors = [receptor_art(im.crop((496+i*72,234,568+i*72,306))) for i in range(4)]
    fragment = (Path(__file__).parent/'context.html').read_text()
    values = {'__SELECT__':select,'__IMAGES__':json.dumps(images,separators=(',',':')),
              '__RECEPTORS__':json.dumps(receptors,separators=(',',':')),
              '__PROPORTIONS__':(Path(__file__).parent/'proportions.json').read_text(),
              '__BOLD__':base64.b64encode((ROOT/'assets/fonts/IosevkaTerm-Bold-latin.ttf').read_bytes()).decode(),
              '__FONT__':base64.b64encode((ROOT/'assets/fonts/IosevkaTerm-Regular-latin.ttf').read_bytes()).decode()}
    for key,value in values.items():
        fragment = fragment.replace(key,value)
    assert len(fragment.encode()) < 1_000_000
    args.output.write_text(fragment)
    print(args.output,len(fragment.encode()),'bytes')

if __name__ == '__main__':
    main()
