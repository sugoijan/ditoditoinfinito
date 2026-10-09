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


def analyze(export, destination):
    import numpy as np
    import librosa
    import soundfile as sf
    export=export.resolve(strict=True)
    manifest_bytes=(export/'manifest.json').read_bytes();manifest=json.loads(manifest_bytes)
    def file(relative):
        path=(export/relative).resolve(strict=True)
        if not path.is_relative_to(export):raise ValueError("Export path escapes its directory")
        return path
    groups={}
    for chart in manifest['charts']:
        groups.setdefault(chart['music']['path'],[]).append(chart)
    results=[]
    for music,charts in groups.items():
        audio=file(music);body=audio.read_bytes()
        if hashlib.sha256(body).hexdigest()!=charts[0]['music']['sha256']:raise ValueError("Changed source audio")
        times=sorted(set(t for c in charts for l in c['lanes'] for t in l['tapsSeconds']))
        candidates=chart_candidates(times)
        y,sr=sf.read(audio,dtype='float32',always_2d=True);y=y.mean(axis=1)
        if len(y)>sr*600:raise ValueError("Audio analysis is limited to ten minutes per track")
        envelope=librosa.onset.onset_strength(y=y,sr=sr,hop_length=128)
        estimates=[]
        for prior in [60,120,180]:
            bpm,beats=librosa.beat.beat_track(onset_envelope=envelope,sr=sr,hop_length=128,start_bpm=prior)
            beat_times=librosa.frames_to_time(beats,sr=sr,hop_length=128)
            estimates.append({"tempoPriorBpm":prior,"bpm":float(np.asarray(bpm).item()),"beatTimesSeconds":beat_times.tolist()})
        local=[]
        # A changing best fit or phase between windows warns against one global grid.
        seed=candidates[0]
        for start in np.arange(min(times),max(times),20):
            subset=np.array([t for t in times if start<=t<start+20])
            if len(subset)<8:continue
            best=max((chart_fit(subset,b,seed['subdivisionsPerBeat'])
                      for b in np.arange(seed['bpm']-2,seed['bpm']+2.001,.01)),key=lambda c:c['coherence'])
            local.append({"startSeconds":float(start),"endSeconds":float(start+20),"notes":len(subset),**best})
        result={"chartIds":[c['id'] for c in charts],"music":music,"audioSha256":hashlib.sha256(body).hexdigest(),
            "distinctNoteTimes":len(times),"chartCandidates":candidates,"audioEstimates":estimates,"chartWindows":local,
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
    a=p.parse_args();analyze(a.export,a.output)
