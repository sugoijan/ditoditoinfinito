#!/usr/bin/env python3
"""Compare saved native raw frames with a CW converter return; never run work code.

A port may change notes or audio. This is an equality check, not an alignment
fit: differences remain differences, and a partial capture never becomes a
verified playable work because its converter reference matches.
"""
import argparse
from collections import Counter
import json
import math
from pathlib import Path
from export_io import digest,write_json
from paths import external_directory
from rebuild_corpus import checked


def frames(values):
    if not isinstance(values,list):raise ValueError('Need a frame array')
    if any(type(v)not in(int,float)or not math.isfinite(v)or int(v)!=v for v in values):
        raise ValueError('Raw comparison needs finite integer 60 Hz frames')
    return [int(v)for v in values]


def compare(native_manifest,cw_manifest,native_chart_id,cw_chart_id):
    nraw=native_manifest.read_bytes();craw=cw_manifest.read_bytes();native=json.loads(nraw);cw=json.loads(craw)
    if native.get('schema')!='ddi-normalized-work/experimental-1'or cw.get('schema')!='ddi-danoni-dump/1':
        raise ValueError('Need a normalized native manifest and a saved CW dump')
    n=next(c for c in native['charts']if c['id']==native_chart_id)
    c=next(c for c in cw['charts']if c['chartId']==cw_chart_id)
    refraw=checked(cw_manifest.parent,c['paths']['reference']).read_bytes();ref=json.loads(refraw)
    if ref.get('schema')!='ddi-danoni-reference/1'or ref.get('captureStage')!='scoreConvert-return-full'or ref.get('fps')!=60:
        raise ValueError('Need the full native CW converter return at 60 Hz')
    if ref['chartId']!=cw_chart_id or n['mode']!=ref['difData']['keyLabel']or len(n['lanes'])!=len(ref['lanes']):
        raise ValueError('Chart identity or lane count differs')
    if ref['timing']['playbackRate']!=1:raise ValueError('Rate-scaled conversion is not invertible here')
    shift=ref['timing']['intAdjustment'];frames([shift]);lanes=[]
    for a,b in zip(n['lanes'],ref['lanes']):
        if a['name']!=b['chara']:raise ValueError('Lane order differs')
        nt=Counter(frames(a['nativeFrames']));ct=Counter(f-shift for f in frames(b['taps']))
        nh=Counter(tuple(frames(pair))for pair in a['nativeHoldFrames'])
        ch=Counter(tuple(f-shift for f in frames(pair))for pair in b['holds'])
        if any(len(pair)!=2 for pair in [*nh,*ch]):raise ValueError('Unpaired hold')
        lanes.append({'name':a['name'],'nativeTaps':sum(nt.values()),'cwTaps':sum(ct.values()),
            'nativeHolds':sum(nh.values()),'cwHolds':sum(ch.values()),'equalRawFrames':nt==ct and nh==ch,
            'nativeOnlyTapFrames':sorted((nt-ct).elements()),'cwOnlyTapFrames':sorted((ct-nt).elements()),
            'nativeOnlyHoldFrames':sorted((nh-ch).elements()),'cwOnlyHoldFrames':sorted((ch-nh).elements())})
    verification_path=cw_manifest.parent/'verification.json';verification=json.loads(verification_path.read_text())if verification_path.exists()else{}
    matched=next((v['referenceMatched']for v in verification.get('comparisons',[])if v['chartId']==cw_chart_id),None)
    equal=all(l['equalRawFrames']for l in lanes)
    return {'schema':'ddi-danoni-port-comparison/1','nativeManifestSha256':digest(nraw),'cwManifestSha256':digest(craw),
        'cwReferenceSha256':digest(refraw),'nativeChartId':native_chart_id,'cwChartId':cw_chart_id,'mode':n['mode'],
        'units':'Integer frames at 60 Hz, before CW integer adjustment; no audio alignment or fitted shift',
        'cwRemovedIntegerAdjustment':shift,'cwCaptureStatus':c['status'],'cwOfflineReferenceMatched':matched,
        'cwOfflineWorkPassed':verification.get('passed',False),'nativeAudioOrigin':n.get('audioOrigin'),
        'cwTiming':ref['timing'],'equalRawFrames':equal,'lanes':lanes,
        'scope':'Exact note/hold multiset comparison. Equal frames do not prove equal audio or event semantics; changed charts cannot serve as equality oracles.'}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('native',type=Path);parser.add_argument('cw',type=Path)
    parser.add_argument('--native-chart',required=True);parser.add_argument('--cw-chart',type=int,required=True);parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();output=external_directory(args.out);result=compare(args.native,args.cw,args.native_chart,args.cw_chart);write_json(output,result)
    print(json.dumps({'equalRawFrames':result['equalRawFrames'],'cwOfflineReferenceMatched':result['cwOfflineReferenceMatched'],'output':str(output)}))
