#!/usr/bin/env python3
"""Prepare a playable SSC pack from normalized, audio-relative chart exports.

Shared post-processing for any supported export; no SWF execution, fetching,
family-specific chart parsing occurs here. Default musical fitting is bounded;
explicit source-bound reviews override it, and exact frame timing remains available.
"""
import argparse
from collections import Counter
import json
import math
from pathlib import Path
import re
import shutil
import subprocess
import wave
import zipfile

from export_io import digest, write_json
from musical_grid import quantize_lanes, validate_grid,position,summarize as musical_summary
from paths import external_directory
from artwork_review import load_review
from scroll_policy import validate_motion,base_ratio,summarize as scroll_summary
from pack_policy import star_count,tier_for_stars,choose_preview,display_bpm,authored_display_bpm

SCHEMAS={'ddi-legacy-export/experimental-1','ddi-synthetic-export/experimental-1','ddi-normalized-work/experimental-1'}
MODES={'5':5,'7':7,'7i':7,'9A':9,'9B':9}


def load_export(source):
    source=source.resolve(strict=True);raw=(source/'manifest.json').read_bytes();manifest=json.loads(raw)
    if manifest.get('schema') not in SCHEMAS:raise ValueError('Unsupported normalized export schema')
    def checked(relative,expected=None):
        path=(source/relative).resolve(strict=True)
        if not path.is_relative_to(source):raise ValueError('Export path escapes its directory')
        if expected and digest(path.read_bytes())!=expected:raise ValueError('Changed source asset/evidence')
        return path
    entries=[];ids=set()
    for chart in manifest['charts']:
        if not isinstance(chart['id'],str) or not chart['id'] or chart['id'] in ids:raise ValueError('Need unique nonempty chart ids')
        ids.add(chart['id'])
        music=chart.get('music',manifest.get('music'))
        if not music or not music.get('sha256'):raise ValueError('Need hashed music for each chart')
        checked(music['path'],music['sha256'])
        refpath=chart.get('nativeReference',manifest.get('evidence',{}).get('reference'))
        reference=json.loads(checked(refpath).read_text()) if refpath else {}
        metadata={**manifest.get('metadata',{}),**reference.get('metadata',{}),**chart.get('metadata',{})}
        entries.append({'chart':chart,'music':music,'reference':reference,'metadata':metadata,
            'stars':star_count(metadata,chart),'id':str(chart['id'])})
    if not entries:raise ValueError('No charts')
    return source,raw,manifest,entries


def frame(seconds):
    if type(seconds) not in (float,int) or not math.isfinite(seconds):raise ValueError('Nonfinite time')
    tick=round(seconds*60)
    if abs(seconds-tick/60)>1e-9 or abs(tick)>432000:raise ValueError('Time is outside the supported exact 60 Hz frame grid')
    return tick


def ssc_value(value):
    value=str(value)
    if any(c in value for c in ';\r\n'):raise ValueError('Unrepresentable header')
    return ''.join('\\'+c if c in '\\:#/' else c for c in value)


def ssc_chart(chart,tier,stars,grid=None,native_motion=None,scroll_factor=1,preserve_label=False):
    mode=chart['mode'];lanes=chart['lanes']
    if mode not in MODES or len(lanes)!=MODES[mode]:raise ValueError('Unsupported/incorrect built-in DanOni layout')
    def event_tick(seconds,rhythmic=True):
        exact=frame(seconds)
        return position(seconds,grid,rhythmic)[0] if grid else exact
    bpm=grid['bpm'] if grid else 75;phase=grid['beatZeroSeconds'] if grid else 0;quantum=60/bpm/48
    notes={};events=[]
    mapped=quantize_lanes(chart,grid)[0]if grid else None
    for lane,data in enumerate(lanes):
        taps=mapped[lane]['taps']if mapped else[event_tick(seconds)for seconds in data['tapsSeconds']];holds=[]
        for seconds in taps:events.append((seconds,lane,'1'))
        for pair_index,pair in enumerate(data['holdsSeconds']):
            if len(pair)!=2:raise ValueError('Unpaired hold')
            head,tail=mapped[lane]['holds'][pair_index]if mapped else map(event_tick,pair)
            if head>=tail:raise ValueError('Invalid hold duration')
            holds.append((head,tail));events.extend([(head,lane,'2'),(tail,lane,'3')])
        holds.sort()
        if any(b[0]<=a[1] for a,b in zip(holds,holds[1:])) or any(h<=t<=end for h,end in holds for t in taps):
            raise ValueError('Overlapping holds/taps cannot be represented faithfully')
    if not events:raise ValueError('Empty chart')
    speed=chart.get('speed',[])
    padding=max(0,-min([t for t,_,_ in events]+[event_tick(e['seconds'],False) for e in speed]))
    if grid:padding=math.ceil(padding/48)*48  # Preserve musical quantization during pre-roll padding.
    if native_motion and phase/quantum>padding:
        padding+=math.ceil((phase/quantum-padding)/192)*192  # Start authored motion before audio zero; preserve the four-beat colour cycle.
    for tick,lane,kind in events:
        row=notes.setdefault(tick+padding,['0']*len(lanes))
        if row[lane]!='0':raise ValueError('Conflicting notes cannot be represented faithfully')
        row[lane]=kind
    rows=','.join('\n'+'\n'.join(''.join(notes.get(m*192+i,['0']*len(lanes))) for i in range(192))
                  for m in range(max(notes)//192+1))
    compensation=base_ratio(native_motion,bpm,scroll_factor)
    scroll=[f'0={compensation:.15g}'] if native_motion else []
    for e in speed:
        multiplier=e['multiplier']
        if type(multiplier) not in (float,int) or not math.isfinite(multiplier):raise ValueError('Invalid scroll multiplier')
        scroll.append(f"{(event_tick(e['seconds'],False)+padding)/48:.15g}={multiplier*compensation:.15g}")
    label=chart.get('metadata',{}).get('difficultyLabel','')
    if not isinstance(label,str) or any(c in label for c in ';\r\n'):raise ValueError('Unsafe source difficulty label')
    return ('#NOTEDATA:;\n'+(f'#CHARTNAME:{ssc_value(label)};\n' if label and(stars is None or preserve_label)else '')+f'#STEPSTYPE:danoni-{mode};\n#DESCRIPTION:;\n#DIFFICULTY:{tier};\n#METER:{stars if stars is not None else 0};\n'
        +f'#OFFSET:{padding*quantum-phase:.15g};\n#BPMS:0={bpm:.15g};\n'
        +(f"#SCROLLS:{','.join(scroll)};\n" if scroll else '')+f'#NOTES:{rows};\n')


def prepare(source,output,codec,ffmpeg,vorbis_encoder=None,bounds=(1,3,5,7),preview_seconds=12,tier_policy='stars',display_bpms=None,musical_grid=None,native_scroll=None,scroll_factor=1,preview_settings=None,artwork_review=None,tempo_policy="auto",tempo_cache=None,ratings_review=None):
    if tempo_policy not in ('auto','exact'):raise ValueError('Unknown tempo policy')
    if codec not in ('wav','ogg') or tier_policy not in ('stars','edit'):raise ValueError('Unknown audio/tier policy')
    tier_for_stars(None,bounds)
    source,raw,manifest,entries=load_export(source)
    ratings_bytes=None;ratings={}
    if ratings_review:
        from source_ratings import load_review as load_ratings
        ratings_bytes,ratings=load_ratings(ratings_review,digest(raw),entries)
        for entry in entries:
            if entry['id']in ratings:entry['stars']=ratings[entry['id']]['meter']
    reviews={ident:display_bpm(value) for ident,value in (display_bpms or {}).items()}
    if set(reviews)-{e['id'] for e in entries}:raise ValueError('Display BPM refers to an unknown chart')
    for entry in entries:
        declared=entry['metadata'].get('declaredDisplayBpm')
        if entry['id'] not in reviews and (authored:=authored_display_bpm(declared)):
            reviews[entry['id']]=authored
    grids={};grid_bytes=None
    if musical_grid:
        grid_bytes=musical_grid.read_bytes();review=json.loads(grid_bytes)
        if review.get('schema')!='ddi-reviewed-musical-grid/experimental-1' or review.get('sourceManifestSha256')!=digest(raw):
            raise ValueError('Musical grid review must match the normalized source manifest')
        if set(review['charts'])!={e['id'] for e in entries}:raise ValueError('Musical grid review must cover every chart id exactly')
        grids={ident:validate_grid(value) for ident,value in review['charts'].items()}
    motions={};motion_bytes=None;motion_evidence={}
    base_ratio(None,75,1 if native_scroll else scroll_factor)  # Reject ineffective/nonfinite factor options.
    if native_scroll:
        motion_bytes=native_scroll.read_bytes();review=json.loads(motion_bytes)
        if review.get('schema')!='ddi-reviewed-native-scroll/experimental-1' or review.get('sourceManifestSha256')!=digest(raw):
            raise ValueError('Native scroll review must match the normalized source manifest')
        if set(review['charts'])!={e['id'] for e in entries}:raise ValueError('Native scroll review must cover every chart id exactly')
        motions={ident:validate_motion(value) for ident,value in review['charts'].items()}
        for ident,motion in motions.items():
            base_ratio(motion,75,scroll_factor)
            evidence=motion.get('evidence')
            if isinstance(evidence,dict) and 'path' in evidence:
                path=(native_scroll.parent/evidence['path']).resolve(strict=True)
                if not path.is_relative_to(native_scroll.parent.resolve()):raise ValueError('Motion evidence escapes its review directory')
                body=path.read_bytes()
                if digest(body)!=evidence.get('sha256'):raise ValueError('Changed native motion evidence')
                measured=json.loads(body)
                if measured.get('schema')!='ddi-native-motion-measurement/experimental-1':raise ValueError('Unexpected native motion evidence schema')
                if measured.get('baseArrowHeightsPerSecond')!=motion['baseArrowHeightsPerSecond']:raise ValueError('Motion rate disagrees with its evidence')
                if manifest.get('sourceSwfSha256') and measured.get('sourceSwfSha256')!=manifest['sourceSwfSha256']:raise ValueError('Motion evidence comes from another SWF')
                motion_evidence[ident]=body
    artwork_bytes=None;artwork={}
    if artwork_review:
        artwork_bytes,artwork=load_review(artwork_review,digest(raw),[e['id']for e in entries],manifest.get('sourceSwfSha256'))
    output=external_directory(output);output.mkdir(parents=True,exist_ok=False)
    (output/'songs').mkdir();(output/'references').mkdir()
    if ratings_bytes:
        (output/'ratings').mkdir()
        (output/'ratings/review.json').write_bytes(ratings_bytes)
        for row in ratings.values():
            for value in row['ratings']:
                binding=value['source'];target=output/'ratings'/binding['path'];target.parent.mkdir(parents=True,exist_ok=True)
                body=(ratings_review.parent/binding['path']).read_bytes()
                if digest(body)!=binding['sha256']:raise ValueError('Rating evidence changed during copy')
                target.write_bytes(body)
    automatic=None
    if not grids and tempo_policy=='auto':
        from automatic_tempo import automatic_review
        automatic=automatic_review(source,raw,entries,output,tempo_cache)
        if automatic:
            grids={ident:validate_grid(value)for ident,value in automatic['charts'].items()}
            grid_bytes=json.dumps(automatic,ensure_ascii=False).encode()
            if not motions:
                motions={e['id']:{'baseArrowHeightsPerSecond':75/60,'scope':'Preserves the prior 75 BPM SSC baseline; native movement was not measured'}for e in entries}
    groups={}
    for e in entries:groups.setdefault(e['music']['sha256'],[]).append(e)
    for group in groups.values():
        if grids and any(grids[e['id']]!=grids[group[0]['id']] for e in group):raise ValueError('Charts sharing music need the same reviewed musical grid')
    if grid_bytes:write_json(output/'musical-grid-review.json',json.loads(grid_bytes))
    if motion_bytes:write_json(output/'native-scroll-review.json',json.loads(motion_bytes))
    evidence_paths={}
    for index,(ident,body) in enumerate(motion_evidence.items()):
        relative=f'references/native-motion-{index}.json';(output/relative).write_bytes(body);evidence_paths[ident]=relative
    songs=[];version=subprocess.check_output([str(ffmpeg),'-version'],text=True).splitlines()[0];used=set()
    for number,group in enumerate(groups.values(),1):
        first=group[0];ident=first['id'] if re.fullmatch(r'[A-Za-z0-9_-]+',first['id']) else f'song-{number}'
        if ident in used:ident=f'{ident}-{number}'
        used.add(ident);folder=output/'songs'/ident;folder.mkdir()
        music=first['music'];original=(source/music['path']).resolve();wav_path=output/'references'/f'{ident}.wav'
        decode=[str(ffmpeg),'-nostdin','-v','error','-i',str(original),'-map_metadata','-1','-c:a','pcm_s16le',str(wav_path)]
        subprocess.run(decode,check=True,timeout=30,stdin=subprocess.DEVNULL)
        with wave.open(str(wav_path),'rb') as wav:
            rate,channels,count=wav.getframerate(),wav.getnchannels(),wav.getnframes()
            if channels not in (1,2) or count>rate*600:raise ValueError('Unsupported/oversized audio')
            pcm=wav.readframes(count)
        preview=choose_preview(wav_path,preview_seconds,[e['chart'] for e in group],**(preview_settings or {}))
        name='music.'+codec;destination=folder/name;command=None;encoder_version=None
        if codec=='wav':shutil.copyfile(wav_path,destination)
        else:
            if vorbis_encoder is None:raise ValueError('Ogg needs --vorbis-encoder')
            command=[str(vorbis_encoder),str(rate),str(channels)]
            with destination.open('xb') as f:subprocess.run(command,input=pcm,stdout=f,check=True,timeout=30)
            encoder_version=subprocess.check_output([str(vorbis_encoder),'--version'],text=True).strip()
            decoded=output/'references'/f'{ident}-ogg-check.wav'
            subprocess.run([str(ffmpeg),'-nostdin','-v','error','-i',str(destination),str(decoded)],check=True,timeout=30)
            with wave.open(str(decoded),'rb') as wav:
                if (wav.getframerate(),wav.getnchannels(),wav.getnframes())!=(rate,channels,count):raise ValueError('Decoded Vorbis count/rate/channels differ')
            decoded.unlink()
        selected_art=[artwork.get(e['id'])for e in group]
        banner=next((v for v in selected_art if v is not None),None)
        if banner and any(v!=banner for v in selected_art):raise ValueError('Shared-music charts must have the same reviewed banner')
        if banner:(folder/'banner.png').write_bytes(banner['bytes'])
        meta=first['metadata'];title=meta.get('title',meta.get('musicTitle',first['id']));artist=meta.get('artist','')
        if any(c in str(title)+str(artist) for c in ';\r\n'):raise ValueError('Unrepresentable header')
        grid=grids.get(first['id']);timing_bpm=grid['bpm'] if grid else 75;phase=grid['beatZeroSeconds'] if grid else 0
        text=(f'#VERSION:0.83;\n#TITLE:{ssc_value(title)};\n#ARTIST:{ssc_value(artist)};\n#MUSIC:{name};\n#OFFSET:{-phase:.15g};\n#BPMS:0={timing_bpm:.15g};\n'
              +f"#SAMPLESTART:{preview['startSeconds']:.9g};\n#SAMPLELENGTH:{preview['lengthSeconds']:.9g};\n")
        if banner:text+='#BANNER:banner.png;\n'
        selected=[reviews[e['id']] for e in group if e['id'] in reviews]
        if selected and any(v!=selected[0] for v in selected):raise ValueError('Conflicting display BPMs for charts sharing music')
        reviewed=selected[0] if selected else None
        if reviewed:text+=f"#DISPLAYBPM:{reviewed['sscValue']};\n"
        charts=[]
        duplicates=Counter((e['chart']['mode'],e['stars'])for e in group if e['stars']is not None)
        for i,e in enumerate(group):
            tier=tier_for_stars(e['stars'],bounds) if tier_policy=='stars' else 'Edit'
            native_report=scroll_summary(motions.get(e['id']),timing_bpm,scroll_factor)
            if native_report and e['id'] in evidence_paths:native_report['evidenceStoredAt']=evidence_paths[e['id']]
            preserve_label=duplicates[(e['chart']['mode'],e['stars'])]>1
            text+=ssc_chart(e['chart'],tier,e['stars'],grids.get(e['id']),motions.get(e['id']),scroll_factor,preserve_label)
            omitted={key:len(e['chart'].get(key,[])) for key in ['colors','boost','stops','effects'] if e['chart'].get(key)}
            charts.append({'id':e['id'],'difficultyStars':e['stars'],'difficultyClassification':tier,
                'classificationSource':f'pack heuristic: {tier_policy}','sourceDifficultyClassification':e['metadata'].get('difficultyClassification'),
                'preservedDuplicateLabel':preserve_label,'sourceDifficultyLabel':e['metadata'].get('difficultyLabel',e['metadata'].get('label')),
                'sourceRating':ratings.get(e['id']),
                'unrepresentedEffects':omitted,'nativeScroll':native_report,'musicalQuantization':musical_summary(e['chart'],grids[e['id']]) if grids else None})
            write_json(output/'references'/f'{ident}-{i}.json',{'sourceChart':e['chart'],'sourceMetadata':e['metadata'],'sourceReference':e['reference'],'sourceRating':ratings.get(e['id'])})
        (folder/'song.ssc').write_text(text)
        songs.append({'id':ident,'title':title,'music':f'songs/{ident}/{name}','musicSha256':digest(destination.read_bytes()),
            'sourceAudioSha256':music['sha256'],'banner':f'songs/{ident}/banner.png' if banner else None,
            'artwork':{k:v for k,v in banner.items()if k!='bytes'} if banner else None,'pcmSampleRate':rate,'pcmChannels':channels,'pcmFrames':count,
            'decodeCommand':decode,'encodingCommand':command,'encoderVersion':encoder_version,
            'encoderSha256':digest(vorbis_encoder.read_bytes()) if codec=='ogg' else None,
            'timingBpm':timing_bpm,'timingOffsetSeconds':-phase,'preview':preview,'displayBpm':reviewed,'simfile':f'songs/{ident}/song.ssc','charts':charts})
    report={'schema':'ddi-playable-pack/experimental-1','sourceSchema':manifest['schema'],'sourceManifestSha256':digest(raw),
        'sourceExport':str(source),'ratingsReviewSha256':digest(ratings_bytes)if ratings_bytes else None,'ratingsReviewStoredAt':'ratings/review.json'if ratings_bytes else None,'ratingsPolicySourceSha256':digest(Path(__file__).with_name('source_ratings.py').read_bytes()),'artworkReviewSha256':digest(artwork_bytes) if artwork_bytes else None,'prepareSourceSha256':digest(Path(__file__).read_bytes()),
        'policySourceSha256':digest(Path(__file__).with_name('pack_policy.py').read_bytes()),
        'previewPolicySourceSha256':digest(Path(__file__).with_name('preview_policy.py').read_bytes()),
        'quantizerSourceSha256':digest(Path(__file__).with_name('musical_grid.py').read_bytes()),
        'scrollPolicySourceSha256':digest(Path(__file__).with_name('scroll_policy.py').read_bytes()),'codec':codec,'ffmpeg':version,
        'timingBpm':None if grids else 75,'noteTimingModified':bool(grids),'tempoPolicy':tempo_policy,'automaticTempoApplied':bool(automatic),
        'musicalGridReviewSha256':digest(grid_bytes) if grid_bytes else None,
        'nativeScrollReviewSha256':digest(motion_bytes) if motion_bytes else None,'scrollCorrectionFactor':scroll_factor,'tierPolicy':{'name':tier_policy,'upperBounds':list(bounds),'tiers':['Beginner','Easy','Medium','Hard','Challenge']},'songs':songs}
    if artwork_bytes:(output/'artwork-review.json').write_bytes(artwork_bytes)
    write_json(output/'manifest.json',report)
    with zipfile.ZipFile(output/'playable.zip','x',compression=zipfile.ZIP_DEFLATED) as archive:
        for f in sorted((output/'songs').rglob('*')):
            if f.is_file():archive.write(f,f.relative_to(output/'songs'))
    return report


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('source',type=Path);p.add_argument('output',type=Path)
    p.add_argument('--ratings-review',type=Path);p.add_argument('--tempo-policy',choices=['auto','exact'],default='auto');p.add_argument('--tempo-cache',type=Path);p.add_argument('--artwork-review',type=Path);p.add_argument('--audio',choices=['wav','ogg'],default='wav');p.add_argument('--ffmpeg',type=Path,default=shutil.which('ffmpeg'))
    p.add_argument('--vorbis-encoder',type=Path);p.add_argument('--tier-policy',choices=['stars','edit'],default='stars')
    p.add_argument('--star-tier-bounds',default='1,3,5,7');p.add_argument('--preview-seconds',type=float,default=12)
    p.add_argument('--musical-grid',type=Path,help='Opt-in source-bound reviewed musical grid JSON; changes timing within a <10 ms budget')
    p.add_argument('--native-scroll',type=Path,help='Source-bound native baseline motion review JSON')
    p.add_argument('--scroll-factor',type=float,default=1,help='Scale the native baseline; user X-mod still applies on top')
    p.add_argument('--preview-quiet-ratio',type=float,default=.35)
    p.add_argument('--preview-min-notes',type=int,default=4)
    p.add_argument('--preview-max-gap',type=float,default=1)
    p.add_argument('--preview-note-window',type=float,default=3)
    p.add_argument('--preview-lead-in',type=float,default=.08)
    p.add_argument('--display-bpm',action='append',default=[],metavar='CHART_ID=BPM|LOW:HIGH|*')
    a=p.parse_args();bounds=tuple(map(int,a.star_tier_bounds.split(',')));reviews={}
    for value in a.display_bpm:
        ident,bpm=value.rsplit('=',1)
        if ident in reviews:p.error('Duplicate display-BPM chart id')
        reviews[ident]=bpm
    if a.ffmpeg is None:p.error('Installed FFmpeg required')
    r=prepare(a.source,a.output,a.audio,a.ffmpeg,a.vorbis_encoder,bounds,a.preview_seconds,a.tier_policy,reviews,a.musical_grid,a.native_scroll,a.scroll_factor,
        {'quiet_ratio':a.preview_quiet_ratio,'min_notes':a.preview_min_notes,'max_gap':a.preview_max_gap,
         'note_window':a.preview_note_window,'lead_in':a.preview_lead_in},artwork_review=a.artwork_review,tempo_policy=a.tempo_policy,tempo_cache=a.tempo_cache,ratings_review=a.ratings_review)
    print(json.dumps({'songs':len(r['songs']),'charts':sum(len(s['charts']) for s in r['songs']),'noteTimingModified':r['noteTimingModified']}))
