#!/usr/bin/env python3
"""Estimate musical grids beside an export, without changing its note clock.

Run with the optional analysis requirements. Coherence is a fit statistic, not a
probability of musical correctness; subdivided/half/double grids are ambiguous.
"""
import argparse
import hashlib
from importlib.metadata import version
import json
from pathlib import Path

from paths import external_directory


def chart_fit(times, bpm, subdivisions):
    import numpy as np
    period = 60/bpm/subdivisions
    phase = np.exp(2j*np.pi*times/period).mean()
    origin = float(np.angle(phase)/(2*np.pi)*period % period)
    residual = (times-origin+period/2)%period-period/2
    return {"bpm":float(bpm),"subdivisionsPerBeat":subdivisions,"coherence":float(abs(phase)),
        "gridPhaseSeconds":origin,"gridPeriodSeconds":period,
        "medianGridResidualMs":float(np.median(abs(residual))*1000),
        "p95GridResidualMs":float(np.quantile(abs(residual),.95)*1000),
        "withinHalfSourceFrameFraction":float(np.mean(abs(residual)<=1/120))}


def chart_candidates(times):
    import numpy as np
    times = np.asarray(times,dtype=float)
    if len(times)<8 or not np.isfinite(times).all():
        raise ValueError("Need at least eight finite distinct note times")
    bpms=np.arange(60,240.001,.02); output=[]
    for subdivisions in [1,2,3,4,6,8]:
        periods=60/bpms/subdivisions;valid=periods*60>=3
        response=abs(np.exp(2j*np.pi*times[:,None]/periods[None,:]).mean(axis=0))
        # Keep distinct peaks, so nearby numerical trials do not hide aliases.
        peaks=np.flatnonzero(valid & (response>=np.r_[0,response[:-1]]) & (response>=np.r_[response[1:],0]))
        chosen=[]
        for index in sorted(peaks,key=lambda i:response[i],reverse=True):
            if any(abs(bpms[index]-b)<1 for b in chosen):continue
            chosen.append(bpms[index])
            trials=[chart_fit(times,b,subdivisions) for b in np.arange(bpms[index]-.025,bpms[index]+.0251,.0005)]
            output.append(max(trials,key=lambda c:c['coherence']))
            if len(chosen)==2:break
    return sorted(output,key=lambda c:c['coherence'],reverse=True)


def dominant_grid(times, windows, tolerance_bpm=.15):
    """Fit the best-supported stable windows and report every excluded portion."""
    import numpy as np
    eligible=[i for i,w in enumerate(windows) if w['p95GridResidualMs']<=12 and w['coherence']>=.94]
    if not eligible:return {'status':'insufficient-stable-windows','excludedWindowIndices':list(range(len(windows)))}
    cluster=max(([i for i in eligible if abs(windows[i]['bpm']-windows[j]['bpm'])<=tolerance_bpm] for j in eligible),
                key=lambda ids:(sum(windows[i]['notes'] for i in ids),len(ids)))
    if len(cluster)<2:return {'status':'insufficient-consensus','inlierWindowIndices':cluster,
        'excludedWindowIndices':[i for i in range(len(windows)) if i not in cluster]}
    retained=np.array([t for t in times if any(windows[i]['startSeconds']<=t<windows[i]['endSeconds'] for i in cluster)])
    centre=sum(windows[i]['bpm']*windows[i]['notes'] for i in cluster)/sum(windows[i]['notes'] for i in cluster)
    subdivisions=windows[cluster[0]]['subdivisionsPerBeat']
    fit=max((chart_fit(retained,b,subdivisions) for b in np.arange(centre-.15,centre+.1501,.0005)),key=lambda c:c['coherence'])
    phase_consistent=fit['coherence']>=.94 and fit['p95GridResidualMs']<=12
    return {'status':'dominant-portions-only' if phase_consistent else 'tempo-consensus-phase-unresolved',
        'tempoConsensusBpm':centre,'phaseConsistent':phase_consistent,'scope':'Excluded portions retain their original notes/timing; not a whole-song tempo claim',
        'inlierWindowIndices':cluster,'excludedWindowIndices':[i for i in range(len(windows)) if i not in cluster],
        'retainedNotes':len(retained),'totalNotes':len(times),'retainedFraction':len(retained)/len(times),
        'windowBpmTolerance':tolerance_bpm,'fit':fit,'automaticAcceptance':False}


def analyze(export, destination, preferred_bpms=None):
    import numpy as np
    import librosa
    import soundfile as sf
    from prepare_pack import load_export
    export,manifest_bytes,manifest,entries=load_export(export)
    preferred_bpms=preferred_bpms or {}
    if set(preferred_bpms)-{e['id'] for e in entries}:raise ValueError('Preferred BPM refers to an unknown chart')
    if any(not 60<=b<=240 for b in preferred_bpms.values()):raise ValueError('Preferred BPM must be finite and within 60–240')
    def file(relative):
        path=(export/relative).resolve(strict=True)
        if not path.is_relative_to(export):raise ValueError("Export path escapes its directory")
        return path
    groups={}
    for entry in entries:
        chart={**entry['chart'],'music':entry['music']}
        groups.setdefault(chart['music']['path'],[]).append(chart)
    results=[]
    for music,charts in groups.items():
        audio=file(music);body=audio.read_bytes()
        if hashlib.sha256(body).hexdigest()!=charts[0]['music']['sha256']:raise ValueError("Changed source audio")
        times=sorted(set(t for c in charts for l in c['lanes'] for t in l['tapsSeconds']+[pair[0] for pair in l['holdsSeconds']]))
        candidates=chart_candidates(times)
        y,sr=sf.read(audio,dtype='float32',always_2d=True)
        if len(y)>sr*600:raise ValueError("Audio analysis is limited to ten minutes per track")
        envelope=librosa.onset.onset_strength(y=y.T,sr=sr,hop_length=128).mean(axis=0)
        estimates=[]
        for prior in [60,120,180]:
            bpm,beats=librosa.beat.beat_track(onset_envelope=envelope,sr=sr,hop_length=128,start_bpm=prior)
            beat_times=librosa.frames_to_time(beats,sr=sr,hop_length=128)
            estimates.append({"tempoPriorBpm":prior,"bpm":float(np.asarray(bpm).item()),"beatTimesSeconds":beat_times.tolist()})
        local=[]
        # A changing best fit or phase between windows warns against one global grid.
        hints={preferred_bpms[c['id']] for c in charts if c['id'] in preferred_bpms}
        if len(hints)>1:raise ValueError('Conflicting preferred BPMs for charts sharing music')
        hint=next(iter(hints),None)
        seed=min(candidates,key=lambda c:abs(c['bpm']-hint)) if hint is not None else candidates[0]
        for start in np.arange(min(times),max(times),20):
            subset=np.array([t for t in times if start<=t<start+20])
            if len(subset)<8:continue
            best=max((chart_fit(subset,b,seed['subdivisionsPerBeat'])
                      for b in np.arange(seed['bpm']-2,seed['bpm']+2.001,.01)),key=lambda c:c['coherence'])
            local.append({"startSeconds":float(start),"endSeconds":float(start+20),"notes":len(subset),**best})
        result={"chartIds":[c['id'] for c in charts],"music":music,"audioSha256":hashlib.sha256(body).hexdigest(),
            "distinctNoteTimes":len(times),"chartCandidates":candidates,"audioEstimates":estimates,"chartWindows":local,
            "preferredBpmHint":hint,"preferredCandidate":seed if hint is not None else None,
            "hintSource":"explicit review input" if hint is not None else None,
            "dominantGrid":dominant_grid(times,local,tolerance_bpm=seed['bpm']*.0015),
            "automaticAcceptance":False,"beatPhaseStatus":"Unverified: subdivision phase is not necessarily quarter-beat phase",
            "timingModified":False}
        results.append(result)
        print(json.dumps({"charts":result['chartIds'],"bestChartFitBpm":candidates[0]['bpm'],
            "coherence":candidates[0]['coherence'],"audioBpm":[e['bpm'] for e in estimates]}),flush=True)
    report={"schema":"ddi-tempo-analysis/experimental-1","sourceManifestSha256":hashlib.sha256(manifest_bytes).hexdigest(),
        "analysisSourceSha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "versions":{p:version(p) for p in ['librosa','numpy','scipy','soundfile']},
        "settings":{"audioHopSamples":128,"tempoPriorsBpm":[60,120,180],"chartBpmRange":[60,240],
            "minimumGridSpacingSourceFrames":3,"chartWindowSeconds":20},"tracks":results,
        "limits":["Estimates do not identify original musical BPM or editor grid uniquely",
            "Half/double and triplet/binary aliases require musical review",
            "Variable tempo, swing, syncopation and author timing deviations can defeat a global fit",
            "Source frame timestamps remain authoritative; no estimated beat grid is applied"]}
    destination=external_directory(destination);destination.parent.mkdir(parents=True,exist_ok=True)
    with destination.open('x') as f:json.dump(report,f,ensure_ascii=False,indent=2);f.write('\n')
    return report


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('export',type=Path);p.add_argument('output',type=Path)
    p.add_argument('--preferred-bpm',action='append',default=[],metavar='CHART_ID=BPM')
    a=p.parse_args();hints={}
    for value in a.preferred_bpm:
        ident,bpm=value.rsplit('=',1);hints[ident]=float(bpm)
        if not 60<=hints[ident]<=240:p.error('Preferred BPM must be in the analyzed 60–240 range')
    analyze(a.export,a.output,hints)
