#!/usr/bin/env python3
"""Measure translation of named clips in existing restricted native captures.

No SWF execution or work-specific formulas. The caller supplies a verified FPS,
clip selector and geometric reference; stable paths do not prove object lifetime.
"""
import argparse
import json
import math
from pathlib import Path
import re
import statistics
from export_io import digest,write_json
from higan import Graph,text
from paths import external_directory


def tree_paths(tree):
    nodes={d['id']:d for d in tree}
    if len(nodes)!=len(tree):raise ValueError('Duplicate native display id')
    def path(node,seen=()):
        if node['id'] in seen:raise ValueError('Cyclic display ancestry')
        name=text(node['name']) if node.get('name') else f"<{node['kind']}:{node['depth']}>"
        parent=node.get('parent')
        return (path(nodes[parent],seen+(node['id'],)) if parent is not None else ())+ (name,)
    result={}
    for node in tree:
        key=path(node)
        if key in result:raise ValueError('Ambiguous native display path')
        result[key]=node
    return result


def measure(capture,fps,moving_pattern,reference_name,dimension='width'):
    if not math.isfinite(fps) or not 0<fps<=240 or dimension not in ('width','height'):raise ValueError('Invalid measurement units')
    audit=capture['audit']
    if capture['avm1Halted'] or any(audit[k] for k in ('truncated','runtimeErrors','droppedEntries','truncatedDetails')):
        raise ValueError('Incomplete execution evidence')
    if any(e['kind']!='runtime-warning' for e in audit['entries']):raise ValueError('Unexpected runtime request/audit entry')
    regex=re.compile(moving_pattern);frames=[];references=[];excluded=[]
    for item in capture['captures']:
        Graph(item['state'])  # Reject halted/truncated graphs; never invokes ActionScript.
        tree=tree_paths(item['state']['displayTree'])
        refs=[d for p,d in tree.items() if p[-1]==reference_name and d['visible']]
        if not refs:continue
        if len(refs)!=1:raise ValueError('Ambiguous geometric reference')
        box=refs[0]['stageBoundsPixels'];size=box[2]-box[0] if dimension=='width' else box[3]-box[1]
        if not math.isfinite(size) or size<=0:raise ValueError('Invalid reference dimension')
        references.append({'iteration':item['frame'],'path':reference_name,'stageBoundsPixels':box,'pixels':size})
        clips={p:d for p,d in tree.items() if regex.fullmatch(p[-1]) and d['visible']}
        frames.append((item['frame'],clips))
    if len(frames)<2 or max(r['pixels'] for r in references)-min(r['pixels'] for r in references)>1e-6:
        raise ValueError('Need a stable reference in multiple captures')
    samples=[]
    for (first,a),(last,b) in zip(frames,frames[1:]):
        if last<=first:raise ValueError('Capture iterations must increase')
        for path in a.keys() & b.keys():
            x,y=a[path]['stageBoundsPixels'],b[path]['stageBoundsPixels']
            if len(x)!=4 or len(y)!=4 or not all(math.isfinite(v) for v in x+y):raise ValueError('Invalid native bounds')
            if max(abs((y[2]-y[0])-(x[2]-x[0])),abs((y[3]-y[1])-(x[3]-x[1])),abs(y[0]-x[0]))>1e-6:
                excluded.append({'path':list(path),'firstIteration':first,'lastIteration':last,'reason':'Not stable vertical translation'});continue
            velocity=(y[1]-x[1])/(last-first)
            if abs(velocity)<1e-9:continue
            samples.append({'path':list(path),'firstIteration':first,'lastIteration':last,'firstBounds':x,'lastBounds':y,
                'pixelsPerFrame':velocity,'pixelsPerSecond':velocity*fps})
    if len(samples)<2:raise ValueError('Need at least two tracked translations')
    velocity=statistics.median(s['pixelsPerFrame'] for s in samples)
    if max(abs(s['pixelsPerFrame']-velocity) for s in samples)>1e-6:raise ValueError('Motion varies; cannot claim one base velocity')
    reference=references[0]['pixels']
    return {'schema':'ddi-native-motion-measurement/experimental-1','sourceSwfSha256':capture['source']['movieSha256'],
        'fps':fps,'clockSource':'Explicit caller-supplied verified frame clock','movingNameRegex':moving_pattern,
        'referenceName':reference_name,'referenceDimension':dimension,'referencePixels':reference,
        'signedPixelsPerFrame':velocity,'pixelsPerSecond':abs(velocity)*fps,
        'baseArrowHeightsPerSecond':abs(velocity)*fps/reference,'samples':samples,'references':references,'excluded':excluded,
        'scope':'Observed stable vertical translation normalized by selected source geometry; screen sizes/approach distance may differ',
        'limits':['Matching paths can be reused by scripts; adapters must establish identity/lifetime',
                  'Only measured intervals and movement settings are covered; arbitrary native motion is not inferred']}


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('capture_directory',type=Path);p.add_argument('output',type=Path)
    p.add_argument('--fps',type=float,required=True);p.add_argument('--moving-name-regex',required=True)
    p.add_argument('--reference-name',required=True);p.add_argument('--reference-dimension',choices=['width','height'],default='width')
    a=p.parse_args();source=a.capture_directory.resolve(strict=True)
    parent_bytes=(source/'run.json').read_bytes();parent=json.loads(parent_bytes)
    if parent['exitCode'] or parent['termination'] or not parent['inputsUnchanged']:raise ValueError('Failed restricted capture')
    snapshot_bytes=(source/'snapshot.json').read_bytes();capture=json.loads(snapshot_bytes)
    report=measure(capture,a.fps,a.moving_name_regex,a.reference_name,a.reference_dimension)
    report.update(sourceCapture=str(source),snapshotSha256=digest(snapshot_bytes),parentReportSha256=digest(parent_bytes),
        workerSha256=parent['workerSha256'],configSha256=parent['configSha256'],sandboxSha256=parent['sandboxSha256'],
        retrievedAt=parent['startedAt'],measurementSourceSha256=digest(Path(__file__).read_bytes()))
    destination=external_directory(a.output);destination.parent.mkdir(parents=True,exist_ok=True)
    if destination.exists():raise ValueError('Measurement output must be fresh')
    write_json(destination,report)
    print(json.dumps({k:report[k] for k in ('pixelsPerSecond','referencePixels','baseArrowHeightsPerSecond')}))
