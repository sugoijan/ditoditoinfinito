"""Fit shared music without requiring different authors to share one note phase.

Audio votes and independent chart lattices establish a cohort tempo. A small
per-chart refinement accounts for authored rounding/drift; notes remain governed
by the existing quantizer budget. Scores are evidence, not correctness odds.
"""
import math

from analyze_tempo import chart_fit
from musical_grid import validate_grid

SUBDIVISIONS = (1, 2, 3, 4, 6, 8)
REFINEMENT_FRACTION = .0015  # Same relative tolerance as existing stable-window consensus.


def refine(times, bpm, subdivisions):
    import numpy as np
    radius=bpm*REFINEMENT_FRACTION
    trials=np.arange(max(60,bpm-radius),min(360,bpm+radius)+.00025,.0005)
    best_bpm=bpm;best_response=-1
    for start in range(0,len(trials),256):
        values=trials[start:start+256]
        response=abs(np.exp(2j*np.pi*times[:,None]*values[None,:]*subdivisions/60).mean(axis=0))
        index=int(np.argmax(response))
        if response[index]>best_response:best_response=float(response[index]);best_bpm=float(values[index])
    return chart_fit(times,best_bpm,subdivisions)


def cohort_candidates(charts):
    import numpy as np
    bpms=np.arange(60,360.001,.02);output=[]
    for sub in SUBDIVISIONS:
        periods=60/bpms/sub;response=np.zeros(len(bpms))
        # Bound temporary memory and give each chart one vote, irrespective of density.
        for times in charts.values():
            for start in range(0,len(bpms),256):
                response[start:start+256]+=abs(np.exp(2j*np.pi*times[:,None]/periods[None,start:start+256]).mean(axis=0))/len(charts)
        valid=periods*60>=3
        peaks=np.flatnonzero(valid&(response>=np.r_[0,response[:-1]])&(response>=np.r_[response[1:],0]))
        chosen=[]
        for index in sorted(peaks,key=lambda i:response[i],reverse=True):
            if any(abs(bpms[index]-previous)<1 for previous in chosen):continue
            chosen.append(float(bpms[index]));output.append({'bpm':float(bpms[index]),'subdivisionsPerBeat':sub})
            if len(chosen)==2:break
    return output


def phase_evidence(fit, count):
    # Compare the measured phase support with a uniform-phase null. This is a
    # safeguard against inventing a phase from noise, not musical confidence.
    chance=min(1.,(1/60)/fit['gridPeriodSeconds'])
    margin=3*math.sqrt(chance*(1-chance)/count)
    required=max(.35,chance+margin)
    return {'supported':fit['withinHalfSourceFrameFraction']>=required,'distinctHeads':count,
        'matchedFraction':fit['withinHalfSourceFrameFraction'],'uniformPhaseFraction':chance,
        'requiredFraction':required,'scope':'Uniform-phase safeguard; not a probability of correct musical phase'}


def select_shared_grids(track, charts, pulses, legacy_grid):
    import numpy as np
    arrays={ident:np.asarray(sorted(set(times)),dtype=float)for ident,times in charts.items()}
    if any(not np.isfinite(times).all()for times in arrays.values()):raise ValueError('Nonfinite shared chart times')
    eligible={ident:times for ident,times in arrays.items()if len(times)>=8}
    if len(eligible)<2:return {ident:legacy_grid for ident in arrays},{'status':'insufficient-independent-charts'}
    candidates=cohort_candidates(eligible)
    candidates.extend({'bpm':p['bpm'],'subdivisionsPerBeat':sub}for p in pulses for sub in SUBDIVISIONS if 60/p['bpm']/sub*60>=3)
    authored=track.get('authoredBpmRange')
    if authored:
        inside=[c for c in candidates if authored['minimumBpm']*.99<=c['bpm']<=authored['maximumBpm']*1.01]
        if inside:candidates=inside
    refined={};ranked=[]
    for candidate in candidates:
        bpm,sub=candidate['bpm'],candidate['subdivisionsPerBeat'];frequency=bpm*sub
        key=(round(frequency,5),round(max(60,bpm-bpm*REFINEMENT_FRACTION)*sub,5),round(min(360,bpm+bpm*REFINEMENT_FRACTION)*sub,5))
        if key not in refined:
            # Alias candidates share the same subdivision frequency. Retain the
            # original BPM bounds when refining, so unsupported aliases stay out.
            refined[key]={ident:refine(times,bpm,sub)for ident,times in eligible.items()}
        fits={ident:{**fit,'bpm':fit['bpm']*fit['subdivisionsPerBeat']/sub,'subdivisionsPerBeat':sub}for ident,fit in refined[key].items()}
        coherence=sum(f['coherence']for f in fits.values())/len(fits)
        votes=sum(max(0,1-abs(math.log(bpm/p['bpm']))/.04)for p in pulses)/len(pulses)
        prior=.02*max(0,1-abs(math.log(bpm/150))/.8)-(.04 if sub in (3,6)else 0)
        ranked.append({**candidate,'coherence':coherence,'score':.75*coherence+.25*votes+prior,'chartFits':fits})
    chosen=max(ranked,key=lambda c:c['score']);fits=chosen['chartFits'];sub=chosen['subdivisionsPerBeat']
    supported=[ident for ident,f in fits.items()if f['coherence']>=.85 and f['p95GridResidualMs']<=12]
    bpms=sorted(fits[ident]['bpm']for ident in supported)
    consensus=float(np.median(bpms))if bpms else chosen['bpm']
    # Refine around the accepted median if a candidate-edge fit would exceed
    # the same tolerance relative to the final shared tempo.
    for ident,fit in fits.items():
        if abs(fit['bpm']/consensus-1)>REFINEMENT_FRACTION:fits[ident]=refine(eligible[ident],consensus,sub)
    supported=[ident for ident,f in fits.items()if f['coherence']>=.85 and f['p95GridResidualMs']<=12]
    pulse=min(pulses,key=lambda p:abs(math.log(p['bpm']/consensus)))
    strong=len(supported)>len(eligible)//2 and abs(math.log(consensus/pulse['bpm']))<=.02
    evidence={'status':'independent-chart-consensus'if strong else'no-majority-chart-consensus',
        'consensusBpm':consensus,'eligibleChartCount':len(eligible),'supportingCharts':supported,
        'refinementFraction':REFINEMENT_FRACTION,'chosenCandidate':chosen,'audioPulse':pulse,
        'candidates':ranked,'scope':'Independent chart phases and tightly bounded tempo refinements; no source notes removed'}
    if not strong:return {ident:legacy_grid for ident in arrays},evidence
    summary={k:evidence[k]for k in ['status','consensusBpm','eligibleChartCount','supportingCharts','refinementFraction']}
    grids={}
    for ident,times in arrays.items():
        fit=fits.get(ident)
        support=phase_evidence(fit,len(times))if fit is not None else None
        local=support is not None and support['supported']
        bpm=fit['bpm']if local else consensus;period=60/bpm
        if local:
            phase=fit['gridPhaseSeconds'];aliases=[phase+i*fit['gridPeriodSeconds']for i in range(sub)]
            phase=min(aliases,key=lambda p:abs((p-pulse['beatZeroSeconds']+period/2)%period-period/2))%period
        else:phase=pulse['beatZeroSeconds']%period
        grids[ident]=validate_grid({'bpm':bpm,'beatZeroSeconds':phase,'subdivisionsPerBeat':sub,'maximumErrorMs':9,
            'phaseStatus':'Estimated chart-specific subdivision phase; quarter-beat alias guided by audio, not author metadata',
            'estimationMethod':'independent-chart-phase'if local else'shared-tempo-with-fine-note-fallback',
            'chartFit':fit,'phaseEvidence':support,'sharedTempo':summary,'audioPulse':pulse,'authoredBpmRange':authored,
            'confidenceScope':'Fit and uniform-phase-null statistics do not prove musical correctness'})
    return grids,evidence
