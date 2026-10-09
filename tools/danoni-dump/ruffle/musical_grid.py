"""Optional bounded musical-grid quantization; exact exports remain the default."""
import math


def validate_grid(raw):
    bpm=raw['bpm'];phase=raw['beatZeroSeconds'];sub=raw['subdivisionsPerBeat']
    budget=raw.get('maximumErrorMs',9)
    if any(type(v) not in (int,float) or not math.isfinite(v) for v in (bpm,phase,budget)):
        raise ValueError('Musical grid values must be finite numbers')
    if not 60<=bpm<=240 or abs(phase)>600 or sub not in (1,2,3,4,6,8) or type(sub) is not int:
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


def summarize(chart,grid):
    endpoints=[];speed_events=[]
    for lane_index,lane in enumerate(chart['lanes']):
        times=[('tap',i,t) for i,t in enumerate(lane['tapsSeconds'])]
        times += [(kind,i,t) for i,pair in enumerate(lane['holdsSeconds'])
                  for kind,t in zip(('hold-head','hold-tail'),pair)]
        for kind,index,seconds in times:
            tick,error,snapped=position(seconds,grid)
            endpoints.append({'lane':lane_index,'kind':kind,'index':index,'sourceSeconds':seconds,
                'exportSeconds':seconds+error,'shiftMs':error*1000,'tick':tick,
                'method':'rhythmic-subdivision' if snapped else 'fine-tick-fallback'})
    for index,event in enumerate(chart.get('speed',[])):
        tick,error,_=position(event['seconds'],grid,False)
        speed_events.append({'index':index,'sourceSeconds':event['seconds'],'exportSeconds':event['seconds']+error,
            'shiftMs':error*1000,'tick':tick,'method':'fine-tick'})
    errors=[abs(e['shiftMs']) for e in endpoints]
    rhythmic=sum(e['method']=='rhythmic-subdivision' for e in endpoints)
    return {'grid':grid,'noteEndpoints':len(endpoints),'rhythmicEndpoints':rhythmic,
        'fineTickFallbackEndpoints':len(endpoints)-rhythmic,
        'maximumNoteErrorMs':max(errors,default=0),'meanNoteErrorMs':sum(errors)/len(errors) if errors else 0,
        'maximumScrollEventErrorMs':max((abs(e['shiftMs']) for e in speed_events),default=0),
        'endpoints':endpoints,'scrollEvents':speed_events,
        'scope':'Experimental constant musical grid; source references unchanged; fallback colours may remain irregular'}
