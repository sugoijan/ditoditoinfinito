#!/usr/bin/env python3
"""Normalize verified saved CW browser references for the shared SSC exporter.

No browser, network, source JavaScript, or host ActionScript execution. Our
serializer decodes captured plain data; every source reference remains intact.
"""
import argparse
import json
import math
import re
from pathlib import Path
import shutil
import subprocess

from export_io import digest,write_json
from paths import external_directory
from rebuild_corpus import MODES,checked,copy_tree


def movement_units(root, reference):
    scripts={row['url']:row['sha256']for row in reference['engineScriptHashes']}
    evidence=[];width=factor=None
    for row in json.loads((root/'resources.json').read_text())['resources']:
        if row['url']not in scripts or row.get('status')!=200 or not row.get('bodyPath'):continue
        body=checked(root,row['bodyPath'],row['sha256']).read_bytes()
        decoded=body.decode('utf-8-sig')
        variants=[decoded,decoded.replace('\r\n','\n')]
        source=next((text for text in variants if digest(text.encode())==scripts[row['url']]),None)
        if source is None:raise ValueError('Cached engine text does not match observed script identity')
        match=re.search(r'const\s+C_ARW_WIDTH\s*=\s*(\d+)\b',source)
        start=source.find('const setSpeedOnFrame')
        if not match or start<0:continue
        section=source[start:source.index('\n};',start)]
        unit=re.search(r'currentSpeed\s*=\s*g_stateObj\.speed\s*\*\s*g_headerObj\.baseSpeed\s*\*\s*(\d+)\s*;',section)
        authored=re.search(r'(?:Number|getSpeedFactor)\(_speedData\[s\s*\+\s*1\]\)\s*\*\s*g_stateObj\.speed\s*\*\s*g_headerObj\.baseSpeed\s*\*\s*(\d+)',section)
        if not unit or not authored or unit[1]!=authored[1]:raise ValueError('Unrecognized CW speed-unit convention')
        width=int(match[1]);factor=int(unit[1]);evidence.append({'path':row['bodyPath'],'sha256':row['sha256'],'observedScriptSha256':scripts[row['url']]})
    if width!=50 or factor!=2:raise ValueError('This adapter requires observed standard CW movement units')
    return {'arrowPixels':width,'pixelsPerFramePerSpeed':factor,'engineSources':evidence}


def relative_frames(reference):
    t=reference['timing']
    if reference['fps']!=60 or t['playbackRate']!=1 or t['userAdjustment']!=0 or t['decimalAdjustment']!=0:raise ValueError('Unproved CW playback/adjustment convention')
    if t['intAdjustment']!=math.floor(t['headerAdjustment'])+t['preblankFrame']:raise ValueError('CW adjustment/padding identity failed')
    # scoreConvert adds preblank padding to notes; startup adds it to blankFrame.
    origin=t['blankFrame']+t['preblankFrame']
    def seconds(frame):
        if type(frame)not in(int,float)or not math.isfinite(frame)or int(frame)!=frame:raise ValueError('Need finite integer CW frames')
        return (frame-origin)/60
    lanes=[]
    for lane in reference['lanes']:
        if lane.get('dummyTaps')or lane.get('dummyHoldEndpoints'):raise ValueError('CW dummy notes need an explicit interchange policy')
        endpoints=lane['holdEndpoints']
        if len(endpoints)%2:raise ValueError('Unpaired CW holds')
        pairs=list(zip(endpoints[::2],endpoints[1::2]))
        if [list(p)for p in pairs]!=lane['holds']or any(b<=a for a,b in pairs):raise ValueError('CW hold projection mismatch')
        lanes.append({'name':lane['chara'],'nativeFrames':lane['taps'],'nativeHoldFrames':pairs,
                      'tapsSeconds':[seconds(f)for f in lane['taps']],'holdsSeconds':[[seconds(a),seconds(b)]for a,b in pairs]})
    events={kind:[{**e,'nativeFrame':e['frame'],'seconds':seconds(e['frame'])}for e in reference[kind]]for kind in ['speed','boost']}
    return origin,lanes,events


def normalize(source, output, ffmpeg):
    source=source.resolve(strict=True);root=source.parent;raw=source.read_bytes();m=json.loads(raw)
    verification_raw=(root/'verification.json').read_bytes();verification=json.loads(verification_raw)
    if m.get('schema')!='ddi-danoni-dump/1'or not verification.get('passed'):raise ValueError('Need a verified CW dump')
    compared={c['chartId']for c in verification['comparisons']if c['referenceMatched']}
    selected=[c for c in m['charts']if c['keyMode']in MODES]
    if not selected or any(c['chartId']not in compared or c['status']!='complete'for c in selected):raise ValueError('Incomplete CW selected chart')
    output=external_directory(output);output.mkdir(parents=True,exist_ok=False);(output/'references').mkdir();(output/'media').mkdir()
    # Preserve full dump including unsupported rows as evidence, outside playable ZIPs.
    copy_tree(root,output/'source')
    charts=[];media={};motions={}
    for c in selected:
        ref_path=checked(root,c['paths']['reference']);reference=json.loads(ref_path.read_text())
        if reference['chartId']!=c['chartId']or reference['difData']['keyLabel']!=c['keyMode']:raise ValueError('Wrong CW reference association')
        origin,lanes,events=relative_frames(reference)
        units=movement_units(root,reference)
        helper=Path(__file__).parent.parent/'cw-metadata.mjs'
        metadata=json.loads(subprocess.check_output(['node',str(helper),str(checked(root,c['paths']['snapshots'])),str(ref_path)],text=True))
        speed=metadata['initialSpeed'];base=metadata['baseSpeed']
        if metadata.get('velocity')in ('Extreme','Soft'):raise ValueError('Custom CW velocity mode is outside the standard motion mapping')
        if speed!=reference['difData']['initialSpeed']or speed!=metadata['selectedSpeed']or not 0<speed<=20 or not 0<base<=100:raise ValueError('Unproved CW default movement')
        candidates=[a for a in m['assets']if a['role']=='music'and c['chartId']in a.get('chartIds',[])]
        if len(candidates)!=1:raise ValueError('Need one CW chart/music association')
        a=candidates[0];path=a.get('decodedPath')or a['path'];sha=a.get('decodedSha256')if a.get('decodedPath')else a['sha256'];original=checked(root,path,sha)
        if sha not in media:
            directory=output/'media'/sha;directory.mkdir();copy=directory/('source'+original.suffix);shutil.copyfile(original,copy)
            command=[str(ffmpeg),'-nostdin','-v','error','-protocol_whitelist','file,pipe','-i',str(copy),'-map_metadata','-1','-c:a','pcm_s16le',str(directory/'music.wav')]
            subprocess.run(command,check=True,timeout=30)
            media[sha]={'path':str((directory/'music.wav').relative_to(output)),'sha256':digest((directory/'music.wav').read_bytes()),'mimeType':'audio/wav'}
            write_json(directory/'decode.json',{'sourceSha256':sha,'command':command,'originShiftSamples':0,'scope':'Ordinary CW file decode; source clock/padding handled in chart normalization'})
        ident=f"chart-{c['chartId']}";level=reference['difData']['rawRow'].split(',')[1].split('::')
        stars=reference['difData']['difficulty']if len(level)>=3 and level[2].strip()else None
        credits={'title':metadata['title'],'artist':metadata['artist'],'creator':metadata['creator'],'difficultyLabel':reference['difData']['label'],'sourceChartId':c['chartId'],'sourcePageUrl':m.get('pageUrl',m.get('page',{}).get('finalUrl')),'declaredDisplayBpm':metadata.get('referenceBpm')}
        colors=[{'kind':k,'data':v}for k,v in reference['colors'].items()if v]
        effects=[{'kind':'cw-presentation','snapshot':reference['effects']}]
        if reference['timing'].get('endFrame')is not None:effects.append({'kind':'endFrame','nativeFrame':reference['timing']['endFrame']})
        chart={'id':ident,'mode':c['keyMode'],'difficultyStars':stars,'metadata':credits,'music':media[sha],
               'lanes':lanes,**events,'colors':colors,'effects':effects,'nativeReference':f'references/{ident}.json',
               'cwReference':{'path':c['paths']['reference'],'sha256':digest(ref_path.read_bytes()),'chartId':c['chartId'],'effectiveDos':c['paths']['effectiveDos']},
               'audioOrigin':{'logicalFrame':origin,'fps':60,'blankFrame':reference['timing']['blankFrame'],'preblankFrame':reference['timing']['preblankFrame']}}
        write_json(output/chart['nativeReference'],{'schema':'ddi-cw-normalized-reference/1','metadata':credits,'sourceReference':reference,'audioOrigin':chart['audioOrigin'],'formula':'(parsed frame - captured blankFrame - observed preblankFrame) / 60'})
        # Standard CW source uses 2 * speed * baseSpeed px/frame on 50 px arrows.
        rate=speed*base*2*60/50
        measurement={'schema':'ddi-native-motion-measurement/experimental-1','baseArrowHeightsPerSecond':rate,'fps':60,'initialSpeed':speed,'baseSpeed':base,'pixelsPerSecond':speed*base*120,'referencePixels':50,'sourceReferenceSha256':digest(ref_path.read_bytes()),'unitsEvidence':units,'scope':'Captured CW default settings with standard setSpeedOnFrame/C_ARW_WIDTH units; engine custom presentation not reproduced'}
        path=output/'references'/f'{ident}-motion.json';write_json(path,measurement)
        motions[ident]={'baseArrowHeightsPerSecond':rate,'evidence':{'path':str(path),'sha256':digest(path.read_bytes())},'scope':measurement['scope']}
        charts.append(chart)
    manifest={'schema':'ddi-normalized-work/experimental-1','adapter':'cw-reference/1','adapterSourceSha256':digest(Path(__file__).read_bytes()),
              'sourceManifestSha256':digest(raw),'verificationSha256':digest(verification_raw),'sourcePageUrl':m.get('pageUrl',m.get('page',{}).get('finalUrl')),'metadata':{},'charts':charts,
              'assessment':{'notes':'Saved CW oracle; rate 1, zero user/fractional adjustment and padding independently identified','effects':'Source colours, boosts and presentation retained; omissions declared by SSC preparation'}}
    write_json(output/'manifest.json',manifest)
    write_json(output/'native-scroll-review.json',{'schema':'ddi-reviewed-native-scroll/experimental-1','sourceManifestSha256':digest((output/'manifest.json').read_bytes()),'charts':motions})
    return manifest


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('manifest',type=Path);p.add_argument('output',type=Path);p.add_argument('--ffmpeg',type=Path,default=shutil.which('ffmpeg'));a=p.parse_args()
    m=normalize(a.manifest,a.output,a.ffmpeg);print(json.dumps({'charts':len(m['charts'])}))
