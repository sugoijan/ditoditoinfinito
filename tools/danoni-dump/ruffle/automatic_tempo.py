"""Default musical-grid estimation with cached evidence and bounded application.

Source frames stay immutable. Audio pulse aliases and chart lattices are ranked
without work-name rules; the shared quantizer checks every applied endpoint.
"""
import json
import math
from pathlib import Path
import shutil

from export_io import digest,write_json
from musical_grid import validate_grid,quantize_lanes
from pack_policy import authored_display_bpm
from paths import external_directory


def audio_pulse(estimate):
    times=estimate.get('beatTimesSeconds',[])
    if len(times)<8:return None
    seed=60/estimate['bpm']
    indices=[0]
    for a,b in zip(times,times[1:]):indices.append(indices[-1]+max(1,round((b-a)/seed)))
    indexed=list(zip(indices,times))
    def fit(points):
        n=len(points);mx=sum(x for x,y in points)/n;my=sum(y for x,y in points)/n
        period=sum((x-mx)*(y-my)for x,y in points)/sum((x-mx)**2 for x,y in points)
        return period,my-period*mx
    points=indexed
    for _ in range(4):
        period,phase=fit(points)
        residuals=sorted(abs(y-(period*x+phase))for x,y in points);median=residuals[len(residuals)//2]
        kept=[(x,y)for x,y in indexed if abs(y-(period*x+phase))<=max(.03,median*3)]
        if len(kept)<8 or kept==points:break
        points=kept
    period,phase=fit(points)
    if period<=0 or not 60<=60/period<=360:return None
    errors=sorted(abs(y-(period*x+phase))for x,y in points)
    p95=errors[min(len(errors)-1,int(.95*len(errors)))]*1000
    if p95>max(40,period*80):
        # Missing/irregular onsets cannot establish a global indexed beat phase.
        x=sum(math.cos(2*math.pi*t/seed)for t in times);y=sum(math.sin(2*math.pi*t/seed)for t in times)
        return {'bpm':estimate['bpm'],'beatZeroSeconds':math.atan2(y,x)/(2*math.pi)*seed%seed,'retainedBeats':len(points),'totalBeats':len(times),'p95BeatResidualMs':p95,'refinement':'coarse audio tempo; phase from circular beat evidence'}
    return {'bpm':60/period,'beatZeroSeconds':phase%period,'retainedBeats':len(points),'totalBeats':len(times),'p95BeatResidualMs':errors[min(len(errors)-1,int(.95*len(errors)))]*1000}


def select_grid(track):
    from analyze_tempo import chart_fit
    import numpy as np
    # Refined audio periods remove the estimator's coarse hop-size BPM rounding.
    pulses=[p for e in track['audioEstimates']if (p:=audio_pulse(e))]
    tempos=[p['bpm']for p in pulses]
    if not tempos:raise ValueError('No usable audio pulse to establish a musical tempo')
    times=np.asarray(track['distinctTimesSeconds'],dtype=float)
    candidates=[c for c in track['chartCandidates']if 60<=c['bpm']<=360]
    dominant=track.get('dominantGrid',{})
    if dominant.get('status')=='dominant-portions-only'and dominant.get('retainedFraction',0)>=.35:
        candidates.append({**dominant['fit'],'fitScope':'dominant stable portions','retainedFraction':dominant['retainedFraction']})
    for pulse in pulses:
        candidates += [chart_fit(times,pulse['bpm'],sub)for sub in(1,2,3,4,6,8)if 60/pulse['bpm']/sub*60>=3]
    authored=track.get('authoredBpmRange')
    if authored:
        inside=[c for c in candidates if authored['minimumBpm']*.99<=c['bpm']<=authored['maximumBpm']*1.01]
        if inside:candidates=inside
    def score(c):
        votes=sum(max(0,1-abs(math.log(c['bpm']/b))/.04)for b in tempos)/len(tempos)
        prior=.02*max(0,1-abs(math.log(c['bpm']/150))/.8)
        if c['subdivisionsPerBeat']in(3,6):prior-=.04
        return .75*c['coherence']+.25*votes+prior
    chosen=max(candidates,key=score)
    closest=min(pulses,key=lambda p:abs(math.log(p['bpm']/chosen['bpm'])))
    strong=chosen['p95GridResidualMs']<=12 and chosen['coherence']>=.85 and abs(math.log(chosen['bpm']/closest['bpm']))<=.02
    if strong:
        bpm=chosen['bpm'];phase=chosen['gridPhaseSeconds'];period=60/bpm
        # Choose the quarter-beat alias closest to the observed audio beat phase.
        aliases=[phase+i*chosen['gridPeriodSeconds']for i in range(chosen['subdivisionsPerBeat'])]
        phase=min(aliases,key=lambda p:abs((p-closest['beatZeroSeconds']+period/2)%period-period/2))%period
        method='joint-chart-audio'
    else:bpm=closest['bpm'];phase=closest['beatZeroSeconds'];method='audio-pulse-with-fine-note-fallback'
    return validate_grid({'bpm':float(bpm),'beatZeroSeconds':float(phase),'subdivisionsPerBeat':int(chosen['subdivisionsPerBeat']),'maximumErrorMs':9,
                         'phaseStatus':'Estimated quarter-beat phase from observed audio pulse; not author metadata','estimationMethod':method,
                         'chartFit':chosen,'audioPulse':closest,'alternativePulses':pulses,'authoredBpmRange':authored,'confidenceScope':'Fit statistics and pulse aliases are evidence, not a correctness probability'})


def automatic_review(source, manifest_bytes, entries, output, cache=None):
    import analyze_tempo
    source_hash=digest(manifest_bytes)
    key=digest((source_hash+digest(Path(__file__).with_name('musical_grid.py').read_bytes())+digest(Path(__file__).read_bytes())+digest(Path(analyze_tempo.__file__).read_bytes())+digest(Path(__file__).with_name('requirements-analysis.txt').read_bytes())+digest(Path(__file__).with_name('pack_policy.py').read_bytes())).encode())
    folder=external_directory(cache)/key if cache else output/'automatic-tempo'
    target=folder/'grid.json'
    def insufficient(reason):
        report={'sourceManifestSha256':source_hash,'reason':reason,'fallback':'exact source frame clock'}
        write_json(folder/'insufficient.json',report)
        if folder!=output/'automatic-tempo':
            destination=output/'automatic-tempo';destination.mkdir(exist_ok=True)
            write_json(destination/'insufficient.json',report)
        return None
    if (folder/'insufficient.json').exists():
        saved=json.loads((folder/'insufficient.json').read_text())
        if saved['sourceManifestSha256']!=source_hash:raise ValueError('Changed tempo cache binding')
        return insufficient(saved['reason'])
    if target.exists():
        review=json.loads(target.read_text())
        if review['sourceManifestSha256']!=source_hash:raise ValueError('Changed tempo cache binding')
        # The producer hash is in the key; cached decisions are still checked on current notes.
        for entry in entries:quantize_lanes(entry['chart'],review['charts'][entry['id']])
    else:
        folder.mkdir(parents=True,exist_ok=True)
        if cache and not (folder/'analysis.json').exists():
            # Reuse raw analysis when only the decision/quantizer policy changed.
            for saved in Path(cache).glob('*/analysis.json'):
                candidate=json.loads(saved.read_text())
                if candidate.get('sourceManifestSha256')==source_hash and candidate.get('analysisSourceSha256')==digest(Path(analyze_tempo.__file__).read_bytes()):
                    shutil.copyfile(saved,folder/'analysis.json');break
        try:report=json.loads((folder/'analysis.json').read_text())if(folder/'analysis.json').exists()else analyze_tempo.analyze(source,folder/'analysis.json')
        except ValueError as error:
            if str(error)!='Need at least eight finite distinct note times':raise
            return insufficient(str(error))
        charts={}
        for track in report['tracks']:
            selected=[e for e in entries if e['id']in track['chartIds']]
            track={**track,'distinctTimesSeconds':sorted(set(t for e in selected for l in e['chart']['lanes']for t in l['tapsSeconds']+[a for a,b in l['holdsSeconds']]))}
            authored=[authored_display_bpm(e['metadata'].get('declaredDisplayBpm'))for e in selected]
            if authored and authored[0] and all(v==authored[0]for v in authored):track['authoredBpmRange']=authored[0]
            try:grid=select_grid(track)
            except ValueError as error:
                if str(error)!='No usable audio pulse to establish a musical tempo':raise
                return insufficient(str(error))
            # Verify lane relationships before accepting an estimated grid.
            try:
                for entry in selected:quantize_lanes(entry['chart'],grid);charts[entry['id']]=grid
            except ValueError as error:
                if str(error)not in ('Musical quantization exceeds its explicit timing budget','Fine musical grid cannot preserve lane relationships'):raise
                return insufficient(str(error))
        review={'schema':'ddi-reviewed-musical-grid/experimental-1','sourceManifestSha256':source_hash,'charts':charts,
                'origin':'default estimated grid, independently bounded against every source endpoint','analysisSha256':digest((folder/'analysis.json').read_bytes()),'cacheKey':key}
        write_json(target,review)
    if folder!=output/'automatic-tempo':
        destination=output/'automatic-tempo';destination.mkdir()
        shutil.copyfile(target,destination/'grid.json')
        if (folder/'analysis.json').exists():shutil.copyfile(folder/'analysis.json',destination/'analysis.json')
    return review
