"""Package the original renders into a bounded, self-contained design preview."""
import argparse
import base64
import io
import json
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'target/scratch/controller-hints'
sys.path.insert(0,str(ROOT/'tools/keycap-design'))
from charcoal_vector import body


def data(mime, contents):
    return 'data:'+mime+';base64,'+base64.b64encode(contents).decode()


def main():
    parser=argparse.ArgumentParser();parser.add_argument('output',type=Path);args=parser.parse_args()
    proportions=json.loads((Path(__file__).parent/'proportions.json').read_text())
    def scale(family,control):
        face=control in ('south','east','west','north')
        kind=('direction' if control.startswith('dpad') or family=='generic' and face else
              'center_word' if family=='generic' and control in ('start','back') else
              'face' if face or control=='raw' else 'other')
        return proportions[kind]
    images={}
    for style,folder in [('family',OUT),('shared',OUT/'shared')]:
        for record in json.loads((folder/'study.json').read_text())['icons']:
            with Image.open(folder/record['file']) as original:
                resized=original.resize((round(144*record['aspect']),144),Image.Resampling.LANCZOS)
                b=io.BytesIO();resized.save(b,format='WEBP',quality=92,method=4)
                images[style+'-'+record['family']+'-'+record['control']]={'src':data('image/webp',b.getvalue()),'aspect':record['aspect']}
    key='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 44 32">'+body(44,'deep-gray',False,'ref-key')+'</svg>'
    fragment=(Path(__file__).parent/'study.html').read_text()
    fragment=fragment.replace('__IMAGES__',json.dumps(images,separators=(',',':'))).replace('__KEYCAP__',data('image/svg+xml',key.encode()))
    fragment=fragment.replace('__PROPORTIONS__',json.dumps(proportions,separators=(',',':')))
    for name,font in [('__FONT__','IosevkaTerm-Regular-latin.ttf'),('__ITALIC__','IosevkaTerm-Italic-latin.ttf')]:
        fragment=fragment.replace(name,base64.b64encode((ROOT/'assets/fonts'/font).read_bytes()).decode())
    assert len(fragment.encode())<1_000_000, len(fragment.encode())
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(fragment)
    print(args.output,len(fragment.encode()),'bytes')
    # Static proof includes both PlayStation treatments simultaneously.
    sheet=Image.new('RGB',(1600,1220),'#131019');draw=ImageDraw.Draw(sheet)
    regular=ROOT/'assets/fonts/IosevkaTerm-Regular-latin.ttf'
    title=ImageFont.truetype(str(regular),32);text=ImageFont.truetype(str(regular),22);small=ImageFont.truetype(str(regular),17)
    draw.text((48,34),'Controller buttons / material study',font=title,fill='#f3eef8')
    draw.text((48,80),'Original geometry · shared monitor lighting · 26 px base hint size',font=text,fill='#bdb4c9')
    families=[('xbox','Xbox / glossy charcoal'),('playstation','PlayStation / classic'),('dualsense','PlayStation / DualSense'),('nintendo','Nintendo / satin'),('generic','Generic / positional')]
    controls=['lb','lt','start','back','dpad_ud','dpad_all']
    labels=['L shoulder','L trigger','Start / menu','Select / view','Up / down','Four directions']
    for row,(family,label) in enumerate(families):
        y=145+row*207
        draw.line((48,y-17,1552,y-17),fill='#332d3d',width=1)
        draw.text((48,y),label,font=text,fill='#f3eef8')
        for j,(control,name) in enumerate(zip(controls,labels)):
            rec=images['family-'+family+'-'+control];x=[420,595,825,1085,1300,1470][j]
            with Image.open(OUT/f'{family}-{control}.png') as im:
                height=round(86*scale(family,control))
                im=im.resize((round(height*rec['aspect']),height),Image.Resampling.LANCZOS)
                sheet.paste(im,(round(x-im.width/2),round(y+43-im.height/2)),im)
            draw.text((x,y+104),name,font=small,fill='#bdb4c9',anchor='mt')
        x=48
        for control,action in [('south','Play'),('east','Back'),('west','More'),('lb','Previous chart')]:
            with Image.open(OUT/f'{family}-{control}.png') as im:
                height=round(26*scale(family,control))
                im=im.resize((round(height*images['family-'+family+'-'+control]['aspect']),height),Image.Resampling.LANCZOS)
                sheet.paste(im,(x,round(y+145-im.height/2)),im);x+=im.width+6
            draw.text((x,y+145),action,font=small,fill='#c6bdce',anchor='lm');x+=int(draw.textlength(action,font=small))+24
    sheet.save(OUT/'material-study.png')

if __name__=='__main__':main()
