"""Bounded musical-grid quantization with collision-safe fine-tick fallback."""
import math


def validate_grid(raw):
    bpm=raw['bpm'];phase=raw['beatZeroSeconds'];sub=raw['subdivisionsPerBeat']
    budget=raw.get('maximumErrorMs',9)
    if any(type(v) not in (int,float) or not math.isfinite(v) for v in (bpm,phase,budget)):
        raise ValueError('Musical grid values must be finite numbers')
    if not 60<=bpm<=360 or abs(phase)>600 or sub not in (1,2,3,4,6,8) or type(sub) is not int:
        raise ValueError('Unsupported musical tempo, phase or subdivision')
    if not 0<budget<10:raise ValueError('Musical timing budget must be positive and below 10 ms')
    return {**raw,'maximumErrorMs':budget}


def position(seconds,grid,rhythmic=True):
    quantum=60/grid['bpm']/48;phase=grid['beatZeroSeconds'];budget=grid['maximumErrorMs']/1000
    fine=round((seconds-phase)/quantum)
    subdivision_ticks=48//grid['subdivisionsPerBeat']
    coarse=round((seconds-phase)/(quantum*subdivision_ticks))*subdivision_ticks
    snapped=rhythmic and abs(phase+coarse*quantum-seconds)<=budget+1e-12
    tick=coarse if snapped else fine
    error=phase+tick*quantum-seconds
    if abs(error)>budget+1e-12:raise ValueError('Musical quantization exceeds its explicit timing budget')
    return tick,error,snapped


def quantize_lanes(chart,grid):
    lanes=[];endpoints=[]
    def valid(taps,holds):
        ordered=sorted(holds)
        return len(set(taps))==len(taps) and all(a<b for a,b in ordered) and not any(b[0]<=a[1]for a,b in zip(ordered,ordered[1:])) and not any(a<=t<=b for a,b in ordered for t in taps)
    for lane_index,lane in enumerate(chart['lanes']):
        if any(len(pair)!=2 for pair in lane['holdsSeconds']):raise ValueError('Unpaired hold')
        source_taps=lane['tapsSeconds'];source_holds=lane['holdsSeconds']
        if not valid(source_taps,source_holds):raise ValueError('Source lane has overlapping or invalid notes')
        times=[('tap',i,t)for i,t in enumerate(source_taps)]+[(kind,i,t)for i,pair in enumerate(source_holds)for kind,t in zip(('hold-head','hold-tail'),pair)]
        def mapped(rhythmic):
            values=[position(t,grid,rhythmic)for _,_,t in times]
            taps=[v[0]for v in values[:len(source_taps)]]
            hold_values=values[len(source_taps):];holds=[(a[0],b[0])for a,b in zip(hold_values[::2],hold_values[1::2])]
            return values,taps,holds
        values,taps,holds=mapped(True);safeguard=False
        if not valid(taps,holds):values,taps,holds=mapped(False);safeguard=True
        if not valid(taps,holds):raise ValueError('Fine musical grid cannot preserve lane relationships')
        lanes.append({'taps':taps,'holds':holds,'collisionSafeguard':safeguard})
        for (kind,index,seconds),(tick,error,snapped)in zip(times,values):
            endpoints.append({'lane':lane_index,'kind':kind,'index':index,'sourceSeconds':seconds,'exportSeconds':seconds+error,'shiftMs':error*1000,'tick':tick,
                'method':'lane-collision-fine-fallback'if safeguard else 'rhythmic-subdivision'if snapped else'fine-tick-fallback'})
    return lanes,endpoints


def summarize(chart,grid):
    _,endpoints=quantize_lanes(chart,grid);speed_events=[]
    for index,event in enumerate(chart.get('speed',[])):
        tick,error,_=position(event['seconds'],grid,False)
        speed_events.append({'index':index,'sourceSeconds':event['seconds'],'exportSeconds':event['seconds']+error,
            'shiftMs':error*1000,'tick':tick,'method':'fine-tick'})
    errors=[abs(e['shiftMs'])for e in endpoints];rhythmic=sum(e['method']=='rhythmic-subdivision'for e in endpoints)
    return {'grid':grid,'noteEndpoints':len(endpoints),'rhythmicEndpoints':rhythmic,
        'fineTickFallbackEndpoints':len(endpoints)-rhythmic,'maximumNoteErrorMs':max(errors,default=0),
        'meanNoteErrorMs':sum(errors)/len(errors)if errors else 0,'maximumScrollEventErrorMs':max((abs(e['shiftMs'])for e in speed_events),default=0),
        'endpoints':endpoints,'scrollEvents':speed_events,'scope':'Estimated constant musical grid; source references unchanged; collision-safe fine fallback preserves lane relationships'}
