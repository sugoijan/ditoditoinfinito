"""Pick the earliest supported musical passage from audio and chart evidence.

This is a configurable onset heuristic, not semantic recognition of a chorus.
No source audio, notes or timing are changed.
"""
from array import array
from bisect import bisect_left,bisect_right
import math
import sys
import wave


def audio_envelope(path):
    energy=[]
    with wave.open(str(path),'rb') as wav:
        rate,channels,frames=wav.getframerate(),wav.getnchannels(),wav.getnframes()
        if wav.getsampwidth()!=2 or channels not in (1,2) or frames>rate*600:raise ValueError('Need bounded mono/stereo PCM16')
        if not frames:raise ValueError('Empty music')
        step=max(1,round(rate*.1))
        while raw:=wav.readframes(step):
            values=array('h');values.frombytes(raw)
            if sys.byteorder!='little':values.byteswap()
            energy.append(math.sqrt(sum(v*v for v in values)/len(values))/32768)
    return energy,step/rate,frames/rate


def percentile(values,fraction):
    ordered=sorted(values);index=(len(ordered)-1)*fraction;lower=math.floor(index)
    return ordered[lower]+(ordered[min(lower+1,len(ordered)-1)]-ordered[lower])*(index-lower)


def chart_heads(charts,duration):
    result=[]
    for index,chart in enumerate(charts or []):
        values=[t for lane in chart['lanes'] for t in lane['tapsSeconds']+[p[0] for p in lane['holdsSeconds']]]
        if any(type(t) not in (int,float) or not math.isfinite(t) for t in values):raise ValueError('Invalid preview note time')
        times=sorted(set(t for t in values if 0<=t<duration))
        result.append((str(chart.get('id',index)),times))
    return result


def choose_preview(path,seconds=12,charts=None,*,quiet_ratio=.35,min_notes=4,max_gap=1.0,note_window=3.0,lead_in=.08):
    values=(seconds,quiet_ratio,max_gap,note_window,lead_in)
    if any(type(v) not in (int,float) or not math.isfinite(v) for v in values):raise ValueError('Preview settings must be finite')
    if not 1<=seconds<=30 or not 0<quiet_ratio<=1 or not .1<=max_gap<=10 or not .5<=note_window<=20 or not 0<=lead_in<=1:
        raise ValueError('Invalid preview length, quiet ratio, gap, note window or lead-in')
    if type(min_notes) is not int or not 2<=min_notes<=64:raise ValueError('Preview minimum notes must be 2–64')
    energy,step,duration=audio_envelope(path)
    audible=[e for e in energy if e>=1/32768]
    typical=percentile(audible,.75) if audible else 0
    threshold=max(1/32768,typical*quiet_ratio)
    settings={'policy':'pickup','audioBinSeconds':step,'typicalRmsPercentile':.75,'quietRatio':quiet_ratio,
        'minimumNotes':min_notes,'maximumGapSeconds':max_gap,'noteWindowSeconds':note_window,'leadInSeconds':lead_in,
        'startAudioWindowSeconds':.15,'sustainAudioWindowSeconds':1.5,'requiredSustainHalfSeconds':2}
    if not audible:return {'startSeconds':0,'lengthSeconds':min(seconds,duration),'status':'no-activity',
        'thresholdRms':threshold,'typicalRms':typical,'settings':settings,'method':'No audio above the PCM activity floor'}
    def rms(start,end):
        start=max(0,start);end=min(duration,end)
        if end<=start:return 0
        weighted=0;weight=0
        for i in range(max(0,int(start/step)),min(len(energy),math.ceil(end/step))):
            length=max(0,min(end,(i+1)*step)-max(start,i*step))
            weighted+=energy[i]**2*length;weight+=length
        return math.sqrt(weighted/weight) if weight else 0
    def audio_support(t):
        start=rms(t,t+.15)
        chunks=[rms(t+i*.5,t+(i+1)*.5) for i in range(3) if t+i*.5<duration]
        sustained=sum(v>=threshold for v in chunks)
        return {'startRms':start,'sustainRms':rms(t,t+1.5),'strongHalfSecondWindows':sustained,
            'availableHalfSecondWindows':len(chunks),'supported':start>=threshold and sustained>=min(2,len(chunks))}
    tracks=chart_heads(charts,duration)
    def density(t,times):
        a=bisect_left(times,t-1e-9);b=bisect_right(times,t+note_window+1e-9)
        count=b-a
        first_gap=times[a]-t if count else None
        next_gap=times[a+1]-times[a] if count>=2 else None
        supported=count>=min_notes and first_gap<=max_gap+1e-9 and next_gap<=max_gap+1e-9
        return {'notes':count,'firstNoteGapSeconds':first_gap,'nextNoteGapSeconds':next_gap,'supported':supported}
    informative=[(ident,times) for ident,times in tracks if any(density(t,times)['supported'] for t in times)]
    informative_ids={ident for ident,_ in informative}
    required=len(informative)//2+1 if informative else 0
    candidates=sorted(set(t for _,times in tracks for t in times))
    trace=[];anchor=None;selected_audio=None;selected_chart=None;status=None;fallback_note=None
    rejection_prefix=[(0,0)]
    audio_rejected=density_rejected=0
    for t in candidates:
        sound=audio_support(t)
        support={ident:density(t,times) for ident,times in informative}
        votes=sum(d['supported'] for d in support.values())
        if sound['supported'] and required and votes>=required:
            anchor=t;selected_audio=sound;selected_chart=support;status='selected';break
        if sound['supported'] and fallback_note is None:
            fallback_note=(t,sound,support)
        reason='quiet-or-unsustained-audio' if not sound['supported'] else 'isolated-or-sparse-notes'
        audio_rejected+=int(not sound['supported']);density_rejected+=int(sound['supported'])
        rejection_prefix.append((audio_rejected,density_rejected))
        if len(trace)<16:trace.append({'noteSeconds':t,'reason':reason,'audio':sound,'chartSupport':support})
    scanned_rejections=audio_rejected+density_rejected
    if anchor is None and fallback_note:
        anchor,selected_audio,selected_chart=fallback_note;status='audio-note-fallback'
    if anchor is None:
        anchor=next((i*step for i in range(len(energy)) if energy[i]>=threshold and audio_support(i*step)['supported']),None)
        if anchor is not None:status='audio-only-fallback';selected_audio=audio_support(anchor)
        else:
            anchor=next(i*step for i,e in enumerate(energy) if e>=threshold)
            status='short-audio-fallback';selected_audio=audio_support(anchor)
    if status!='selected':
        audio_rejected,density_rejected=rejection_prefix[bisect_left(candidates,anchor)]
        trace=[v for v in trace if v['noteSeconds']<anchor]
    start=max(0,anchor-lead_in);length=min(seconds,duration-start)
    bins=[energy[i]>=threshold for i in range(int(start/step),min(len(energy),math.ceil((start+length)/step)))]
    first=next(i*step for i,e in enumerate(energy) if e>=threshold)
    return {'startSeconds':start,'lengthSeconds':length,'status':status,'anchorSeconds':anchor,
        'anchorType':'note' if status in ('selected','audio-note-fallback') else 'audio',
        'firstNoteSeconds':min(candidates) if candidates else None,'firstActiveSeconds':first,
        'thresholdRms':threshold,'typicalRms':typical,'activeFraction':sum(bins)/len(bins) if bins else 0,
        'settings':settings,'chartIds':[ident for ident,_ in tracks],'informativeChartIds':[ident for ident,_ in informative],
        'sparseChartIds':[ident for ident,_ in tracks if ident not in informative_ids],
        'requiredChartSupport':required,'selectedAudioSupport':selected_audio,'selectedChartSupport':selected_chart,
        'skippedCandidates':audio_rejected+density_rejected,'rejectedJointCandidates':scanned_rejections,'quietCandidates':audio_rejected,'sparseCandidates':density_rejected,
        'firstSkippedCandidates':trace,
        'method':'Earliest sustained music and nearby distinct chart heads; strict majority of informative charts; bounded lead-in; no audio cropping'}
