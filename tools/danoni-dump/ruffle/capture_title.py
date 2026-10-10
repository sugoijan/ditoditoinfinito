#!/usr/bin/env python3
"""Extract a named native title panel from a hash-checked restricted capture.

Ruffle renders the source typography. This tool maps native stage bounds into
its viewport, crops/encodes pixels with FFmpeg, and records every transformation.
It never runs acquired scripts, fetches assets, or substitutes a song title.
"""
import argparse
import json
import math
from pathlib import Path
import shutil
import subprocess

from export_io import digest, write_json
from higan import Graph, text
from native_families import audit_ok
from paths import external_directory


def panel_rectangle(state, name, width, height):
    Graph(state)  # Reject truncated or halted native evidence.
    displays={d['id']:d for d in state['displayTree']}
    def visible(d):
        seen=set()
        while d:
            if d['id'] in seen:raise ValueError('Cyclic native display ancestry')
            seen.add(d['id'])
            if not d['visible']:return False
            if d['parent'] is None:return True
            if d['parent'] not in displays:raise ValueError('Incomplete native display ancestry')
            d=displays[d['parent']]
        return False
    found=[d for d in displays.values()if d['kind']=='movie-clip' and d.get('name') and text(d['name'])==name and visible(d)]
    if len(found)!=1:raise ValueError('Need exactly one visible named title MovieClip')
    display=found[0];bounds=display['stageBoundsPixels'];matrix=state['stageToViewport']
    if len(matrix)!=6 or len(bounds)!=4 or not all(type(v)in(int,float) and math.isfinite(v)for v in matrix+bounds):raise ValueError('Invalid native geometry')
    a,b,c,d,tx,ty=matrix;x1,y1,x2,y2=bounds
    if x2<=x1 or y2<=y1:raise ValueError('Empty title bounds')
    points=[(a*x+c*y+tx,b*x+d*y+ty)for x in (x1,x2)for y in(y1,y2)]
    # One viewport pixel preserves an antialiased stroke at the bounds.
    left=math.floor(min(x for x,y in points))-1;top=math.floor(min(y for x,y in points))-1
    right=math.ceil(max(x for x,y in points))+1;bottom=math.ceil(max(y for x,y in points))+1
    if not 0<=left<right<=width or not 0<=top<bottom<=height:raise ValueError('Title panel extends outside the captured viewport')
    return display,[left,top,right-left,bottom-top]


def extract(snapshot_path, frame, name, output, ffmpeg, expected_clip_frame=None):
    snapshot_path=snapshot_path.resolve(strict=True);directory=snapshot_path.parent
    raw=snapshot_path.read_bytes();snapshot=json.loads(raw);run_raw=(directory/'run.json').read_bytes();parent=json.loads(run_raw)
    if parent['exitCode'] or parent['termination'] or not parent['inputsUnchanged']:raise ValueError('Need a successful restricted run')
    if snapshot.get('ruffleRevision')!='89a7049965f3090e706265670df309a5128233ac' or not snapshot.get('rendered'):raise ValueError('Need reviewed native rendering')
    audit_ok(snapshot)
    captures=[i for i in snapshot['captures']if i['frame']==frame]
    if len(captures)!=1:raise ValueError('Missing/ambiguous capture frame')
    pixels=directory/f'frame-{frame}.ppm';ppm=pixels.read_bytes();header,raw_rgb=ppm.split(b'\n255\n',1)
    parts=header.split()
    if len(parts)!=3 or parts[0]!=b'P6':raise ValueError('Unexpected native image format')
    width,height=map(int,parts[1:])
    if not 1<=width<=1280 or not 1<=height<=960 or len(raw_rgb)!=width*height*3:raise ValueError('Invalid/oversized native image')
    display,crop=panel_rectangle(captures[0]['state'],name,width,height)
    if expected_clip_frame is not None and display.get('movieFrame')!=expected_clip_frame:raise ValueError('Selected title frame differs from the recipe')
    output=external_directory(output);output.mkdir(parents=True,exist_ok=False)
    commands=[]
    filters=[('panel.png',f'crop={crop[2]}:{crop[3]}:{crop[0]}:{crop[1]}'),
             ('banner.png',f'crop={crop[2]}:{crop[3]}:{crop[0]}:{crop[1]},scale=640:200:force_original_aspect_ratio=decrease:flags=lanczos,pad=640:200:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1')]
    for filename,transform in filters:
        command=[str(ffmpeg),'-nostdin','-v','error','-i',str(pixels),'-vf',transform,'-frames:v','1',str(output/filename)]
        subprocess.run(command,check=True,timeout=30);commands.append(command)
    report={'schema':'ddi-native-title-panel/1','sourceSwfSha256':snapshot['source']['movieSha256'],
            'sourceUrl':snapshot['source']['config']['sourceUrl'],'pageUrl':snapshot['source']['config'].get('pageUrl'),
            'capturedAt':parent['startedAt'],'workerSha256':parent['workerSha256'],'ruffle':snapshot['build'],
            'snapshot':{'path':str(snapshot_path),'sha256':digest(raw)},'runSha256':digest(run_raw),
            'image':{'path':str(pixels),'sha256':digest(ppm),'dimensions':[width,height]},
            'captureFrame':frame,'clipName':name,'clipMovieFrame':display.get('movieFrame'),
            'stageBoundsPixels':display['stageBoundsPixels'],'stageToViewport':captures[0]['state']['stageToViewport'],
            'cropPixels':crop,'bannerDimensions':[640,200],'background':'black','titleReplaced':False,
            'commands':commands,'ffmpeg':subprocess.check_output([str(ffmpeg),'-version'],text=True).splitlines()[0],
            'files':{name:digest((output/name).read_bytes())for name,_ in filters},
            'scope':'Original native Ruffle title-panel pixels; source fonts/layout retained; aspect-preserving banner on black matte'}
    write_json(output/'panel.json',report)
    return report


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('snapshot',type=Path);parser.add_argument('frame',type=int)
    parser.add_argument('clip');parser.add_argument('output',type=Path);parser.add_argument('--expected-clip-frame',type=int)
    parser.add_argument('--ffmpeg',type=Path,default=shutil.which('ffmpeg'));args=parser.parse_args()
    result=extract(args.snapshot,args.frame,args.clip,args.output,args.ffmpeg,args.expected_clip_frame)
    print(json.dumps({'cropPixels':result['cropPixels'],'banner':str(args.output/'banner.png')}))
