"""Strict, data-only projections of observed native DanOni engine families.

Family recognition is structural, not a SWF hash allowlist. Every export still
needs unconsumed data, matching source fields, a verified frame clock, native
arrival/movement observations, and a captured audio start. Work scripts only run
in the restricted native worker; this module never evaluates ActionScript.
"""
import math
from collections import Counter
from higan import Graph, numeric, text

FIVE = [('left','left'),('down','down'),('up','up'),('right','right'),('space','space')]
SEVEN = [('left','le'),('leftdia','led'),('down','do'),('space','oni'),('up','up'),('rightdia','rid'),('right','ri')]
UNDEFINED = {'type':'undefined'}


def numbers(graph, ref):
    if ref == UNDEFINED: return []
    values = graph.array(ref)
    if values and isinstance(values[-1],dict) and values[-1].get('type')=='string' and not text(values[-1]).strip():
        values = values[:-1]  # A terminal empty split sentinel is not a frame 0 note.
    return [numeric(v) for v in values]


def split_numbers(value):
    parts = text(value).split(',')
    while parts and not parts[-1].strip(): parts.pop()
    if any(not p.strip() for p in parts): raise ValueError('Empty interior source token')
    result = [float(p) for p in parts]
    if any(not math.isfinite(v) for v in result): raise ValueError('Nonfinite source token')
    return result


def source_field(root, stem, suffix='', prefix=''):
    # Historical forks place the difficulty suffix on either side of _data.
    names={prefix+stem+suffix+'_data',prefix+stem+'_data'+suffix}
    present=[name for name in names if name in root]
    if len(present)>1:raise ValueError('Ambiguous selected source fields')
    return present[0] if present else None


def stored_bank(graph, required):
    matches=[]
    for obj in graph.state['objects']:
        props=graph.props({'ref':obj['id']})
        if all(k in props for k in required): matches.append((obj['id'],props))
    if len(matches)!=1: raise ValueError('Need exactly one recognized native data bank')
    return matches[0]


def audit_ok(capture, reviewed_diagnostics=()):
    audit={**capture['audit']}
    audit['entries']=[e for e in audit['entries'] if e not in reviewed_diagnostics]
    audit['runtimeErrors']-=sum(e['kind']=='runtime-error' for e in capture['audit']['entries'] if e in reviewed_diagnostics)
    if capture['avm1Halted'] or any(audit[k] for k in ['truncated','runtimeErrors','droppedEntries','truncatedDetails']):
        raise ValueError('Incomplete or erroneous native execution')
    urls={r['url'] for r in capture['source']['config'].get('resources',[])}
    for entry in audit['entries']:
        if entry['kind']=='runtime-warning': continue
        if entry['kind']=='cache-hit' and entry['detail'] in urls: continue
        raise ValueError('Unexpected native request/audit entry')


def extract_movie_arrays(capture, family, source_prefix=""):
    if source_prefix not in ("", "n"): raise ValueError("Unknown source field prefix")
    audit_ok(capture)
    if capture['swfHeaderFps']!=60: raise ValueError('These families require a verified 60 Hz movie clock')
    if family not in ('child-timeline-5','dac-7'): raise ValueError('Unknown family')
    mode, definitions = ('5',FIVE) if family=='child-timeline-5' else ('7',SEVEN)
    initialized=[]
    for item in capture['captures']:
        graph=Graph(item['state']);root=graph.root()
        required=[lane+'_timeline' for lane,_ in definitions] if mode=='5' else [clip+'_dac' for _,clip in definitions]
        try: bank_id,bank=stored_bank(graph,required)
        except ValueError: continue
        indices=['v','y','x','w','z'] if mode=='5' else ['les','leds','dos','onis','ups','rids','ris']
        owner=bank if mode=='5' else root
        if any(numeric(owner[k])!=0 for k in indices):continue
        lanes=[]
        for lane,clip in definitions:
            taps=numbers(graph,bank[lane+'_timeline' if mode=='5' else clip+'_dac'])
            holds=numbers(graph,root.get('f'+clip+'_dac',UNDEFINED)) if mode=='7' else []
            if len(holds)%2 or any(b<a for a,b in zip(holds[::2],holds[1::2])):raise ValueError('Malformed holds')
            suffix=text(root['diffin']) if 'diffin'in root else ''
            field=source_field(root,lane,suffix,source_prefix)
            if field in root and split_numbers(root[field])!=taps:raise ValueError('Native taps disagree with loaded source')
            if mode=='7':
                hold_field=source_field(root,{'leftdia':'frzLdia','rightdia':'frzRdia','space':'frzSpace'}.get(lane,'frz'+lane.capitalize()),suffix)
                if hold_field in root and split_numbers(root[hold_field])!=holds:raise ValueError('Native holds disagree with loaded source')
            if any(int(v)!=v or v<0 or v>720000 for v in taps+holds):raise ValueError('Invalid movie-frame note')
            lanes.append({'name':lane,'nativeFrames':taps,'nativeHoldFrames':list(zip(holds[::2],holds[1::2])),
                          'sourceOrderIssues':[{'index':j,'frame':b,'previousFrame':a} for j,(a,b) in enumerate(zip(taps,taps[1:]),1) if b<=a]})
        speed=[]
        if mode=='7':
            flat=numbers(graph,root['speed_dac'])
            if split_numbers(root['speed_change'])!=flat:raise ValueError('Native speed conversion disagrees with source')
        elif 'speed_timeline' in bank:
            flat=numbers(graph,bank['speed_timeline'])
            if 'speed_data'in root and split_numbers(root['speed_data'])!=flat:raise ValueError('Native speed conversion disagrees with source')
        else:flat=[]
        if len(flat)%2:raise ValueError('Unpaired speed events')
        speed=[{'nativeFrame':f,'multiplier':m} for f,m in zip(flat[::2],flat[1::2])]
        for field in ('boost_data','color_data','acolor_data'):
            if field in root and text(root[field]).strip(',\r\n '):raise ValueError('This family has an unimplemented authored effect')
        stops=numbers(graph,bank.get('stop_dac',UNDEFINED)) if mode=='7' else []
        if 'stop_data' in root and split_numbers(root['stop_data'])!=stops:raise ValueError('Native stop flags disagree with source')
        known={lane+'_timeline'for lane,_ in FIVE}|{'speed_timeline'} if mode=='5' else {clip+'_dac'for _,clip in SEVEN}|{'stop_dac','speed_dac'}
        if any(k.endswith('_timeline' if mode=='5'else'_dac') and k not in known for k in bank):raise ValueError('Unrecognized extra native lanes/effects')
        initialized.append((item,{'stops':[{'nativeFrame':frame,'paused':index%2==0}for index,frame in enumerate(stops)],'mode':mode,'lanes':lanes,'speed':speed,'boost':[],'colors':[],
                                 'metadata':{k:text(root[k]) for k in ['diffi','musicTitle','tuning','gatit'] if k in root and isinstance(root[k],dict) and root[k].get('type')=='string'},
                                 'bankObject':bank_id,'nativeKeyCodes':numbers(graph,root['keyCtrl']) if isinstance(root.get('keyCtrl'),dict) and 'ref' in root['keyCtrl'] else None}))
    if len(initialized)<2:raise ValueError('Need repeated unconsumed native arrays')
    baseline=initialized[0][1]
    for _,later in initialized[1:]:
        if any(later[k]!=baseline[k] for k in ['lanes','speed','stops','mode']):raise ValueError('Initialized data changed before note consumption')
    return baseline,initialized[0][0]['frame']


def display_map(item):
    result={}
    for display in item['state']['displayTree']:
        if display.get('name') and display['kind']=='movie-clip' and display['visible']:
            name=text(display['name'])
            if name in result: result[name]=None  # Ambiguity must fail, not pick an arbitrary child.
            else:result[name]=display
    return result


def movie_observation(capture, chart, note_name, receptor_name, lane, target_y=None, geometry_name=None, clock_field=None):
    """Prove a linear arrow's arrival and native speed against a raw lane frame."""
    audit_ok(capture,chart.get('presentationDiagnostics',[]))
    frames=[];widths=[]
    for item in capture['captures']:
        Graph(item['state']);objects=display_map(item)
        note,ref=objects.get(note_name),objects.get(receptor_name)
        if not note or not ref:continue
        point=note['stageRegistrationPixels'];rp=ref['stageRegistrationPixels']
        geometry=objects.get(geometry_name) if geometry_name else ref
        if not geometry:raise ValueError('Missing geometry reference')
        bounds=geometry['stageBoundsPixels']
        clock=numeric(Graph(item['state']).root()[clock_field]) if clock_field else item['state']['roots'][0]['movieFrame']
        frames.append({'iteration':item['frame'],'movieFrame':clock,'x':point[0],'y':point[1],
                       'targetY':rp[1] if target_y is None else target_y})
        widths.append(bounds[2]-bounds[0])
    if len(frames)<3 or min(widths)<=0 or max(widths)-min(widths)>1e-6:raise ValueError('Need multiple stable arrow/reference observations')
    a,b=frames[0],frames[1];velocity=(b['y']-a['y'])/(b['movieFrame']-a['movieFrame'])
    if velocity>=0:raise ValueError('Expected upward linear movement')
    for f in frames:
        if f['movieFrame']-a['movieFrame']!=f['iteration']-a['iteration'] or abs(f['x']-a['x'])>1e-6 or abs(f['y']-(a['y']+velocity*(f['movieFrame']-a['movieFrame'])))>1e-6:
            raise ValueError('Nonlinear/reused arrow or non-movie clock')
    arrival=a['movieFrame']+(a['targetY']-a['y'])/velocity
    raw=chart['lanes'][lane]['nativeFrames'][0]
    if abs(arrival-raw)>1.5:raise ValueError('Arrow arrival does not agree with source frame')
    # Receptor art registration differs slightly from the judgment target in
    # early sources; target_y can be supplied only after independent native proof.
    return {'schema':'ddi-native-motion-measurement/experimental-1','sourceSwfSha256':capture['source']['movieSha256'],
            'fps':60,'pixelsPerSecond':abs(velocity)*60,'referencePixels':widths[0],
            'baseArrowHeightsPerSecond':abs(velocity)*60/widths[0],'noteName':note_name,'receptorName':receptor_name,'geometryReferenceName':geometry_name or receptor_name,'clockField':clock_field,
            'arrivalDifferenceFrames':arrival-raw,
            'rawFirstFrame':raw,'observedArrivalMovieFrame':arrival,'samples':frames,
            'scope':'Observed native default movement, measured source geometry and verified raw movie-frame arrival'}


def movie_audio_origin(capture, clock_field=None, stream_index=0):
    starts=(capture.get('audio') or {}).get('streamStarts',[])
    if type(stream_index)is not int or not 0<=stream_index<len(starts) or starts[stream_index]['compression']!='Mp3':raise ValueError('Need an observed MP3 song stream start')
    start={**starts[stream_index],'streamStartIndex':stream_index}
    if stream_index+1<len(starts):start['nextStreamOutputSampleFrame']=starts[stream_index+1]['outputSampleFrame']
    matching=[i for i in capture['captures'] if i['frame']==start['iteration']]
    if len(matching)!=1:raise ValueError('Need a snapshot at the exact audio start iteration')
    item=matching[0];movie_frame=numeric(Graph(item['state']).root()[clock_field]) if clock_field else item['state']['roots'][0]['movieFrame']
    return {**start,'logicalFrame':movie_frame,'clock':clock_field or 'root movie frame','scope':'Observed restricted Ruffle stream start; original Flash/device latency not measured'}


def relative_chart(chart, origin):
    result={**chart}
    result['lanes']=[{**l,'tapsSeconds':[(f-origin)/60 for f in l['nativeFrames']],
                     'holdsSeconds':[[(a-origin)/60,(b-origin)/60] for a,b in l['nativeHoldFrames']]}for l in chart['lanes']]
    for field in ('speed','boost','stops'):
        result[field]=[{**e,'seconds':(e['nativeFrame']-origin)/60}for e in chart.get(field,[])]
    return result


def schedule(graph, reference, lane_indices):
    result=Counter()
    for key,value in graph.props(reference).items():
        if not key.lstrip('-').isdecimal():
            if key not in ('length','__proto__','__constructor__'):raise ValueError('Unknown schedule property')
            continue
        for lane in graph.array(value):
            lane=numeric(lane)
            if lane not in lane_indices:raise ValueError('Schedule contains an unsupported lane')
            result[(int(key),int(lane))]+=1
    return result


def scheduled_projection(capture, mode, *, allow_missing_decorative_sprites=False):
    """Cross-check independent raw lane banks and the native spawn/travel table."""
    # AVM errors from missing optional attachments can only be reviewed after
    # converter completion. They are retained; a halted VM is always rejected.
    diagnostics=[]
    clean={**capture,'audit':{**capture['audit'],'entries':list(capture['audit']['entries'])}}
    if allow_missing_decorative_sprites:
        diagnostics=[e for e in clean['audit']['entries'] if e['kind']=='runtime-error']
        if not diagnostics or any(e['detail']!='message=Tried to instantiate a non-registered character undefined ' or not isinstance(e.get('iteration'),int) for e in diagnostics):
            raise ValueError('Unknown presentation failure; cannot review it as cosmetic')
        clean['audit']['entries']=[e for e in clean['audit']['entries'] if e not in diagnostics]
        clean['audit']['runtimeErrors']-=len(diagnostics)
    audit_ok(clean)
    if mode not in ('5','7') or capture['swfHeaderFps']!=60:raise ValueError('Unsupported scheduled family mode/clock')
    definitions=FIVE if mode=='5' else SEVEN
    lane_indices=[0,2,4,6,3] if mode=='5' else list(range(7))
    raw=prepared=None
    for item in capture['captures']:
        graph=Graph(item['state']);root=graph.root()
        if 'mkArrow'not in root or 'judgeY'not in root:continue
        clock='frameNum' if 'frameNum'in root else 'frame_num'
        if numeric(root[clock])!=0:continue
        if 'timeline'in root:
            bank=graph.array(root['timeline']);holds=graph.array(root['ftimeline'])
            lanes=[]
            for (lane,_),index in zip(definitions,lane_indices):
                values=numbers(graph,holds[index])
                if len(values)%2:raise ValueError('Unpaired native hold endpoints')
                lanes.append({'name':lane,'nativeFrames':numbers(graph,bank[index]),'nativeHoldFrames':list(zip(values[::2],values[1::2]))})
        else:
            # Later ParaFla families retain raw loaded fields through converter cleanup.
            suffix=text(root['diffin'])
            lanes=[]
            for lane,_ in definitions:
                field=source_field(root,lane,suffix)
                if not field:raise ValueError('Missing selected source lane')
                taps=split_numbers(root[field])
                field=source_field(root,{'leftdia':'frzLdia','rightdia':'frzRdia','space':'frzSpace'}.get(lane,'frz'+lane.capitalize()),suffix)
                values=split_numbers(root[field]) if field in root else []
                if len(values)%2:raise ValueError('Unpaired source holds')
                lanes.append({'name':lane,'nativeFrames':taps,'nativeHoldFrames':list(zip(values[::2],values[1::2]))})
        if any(b<a for l in lanes for a,b in l['nativeHoldFrames']):raise ValueError('Backwards holds')
        if not any(l['nativeFrames'] for l in lanes):continue
        source_clock='frameNum' if 'frameNum'in root else 'frame_num'
        raw=(item,graph,root,lanes,source_clock)
        taps=schedule(graph,root['mkArrow'],lane_indices)
        freezes=schedule(graph,root['mkFArrow'],lane_indices)
        if not taps:continue
        travel=graph.props(root['judgeY'])
        recovered=Counter((frame+numeric(travel[str(frame)]),lane)for frame,lane in taps.elements())
        expected=Counter((f,index)for index,l in zip(lane_indices,lanes)for f in l['nativeFrames'])
        expected_h=Counter((a,index)for index,l in zip(lane_indices,lanes)for a,_ in l['nativeHoldFrames'])
        recovered_h=Counter((frame+numeric(travel[str(frame)]),lane)for frame,lane in freezes.elements())
        if recovered!=expected or recovered_h!=expected_h:raise ValueError('Native spawn + travel disagrees with raw heads')
        prepared=(item,graph,root,lanes,source_clock,taps,freezes);break
    if not raw or not prepared:raise ValueError('Need complete native lane data and a compiled schedule before consumption')
    item,graph,root,lanes,clock,taps,freezes=prepared
    if diagnostics:
        if any(e['iteration']<=item['frame']for e in diagnostics):raise ValueError('Failure preceded converter completion')
        required=[clip+'_step'for _,clip in SEVEN]
        if not any(all(display_map(i).get(name) for name in required)for i in capture['captures']):raise ValueError('Required selected-mode sprites are missing')
    metadata={k:text(root[k])for k in ['musicTitle','GAME_TITLE','TUNING_NAME','TUNING_NAMES','tuning','diffi','VERSION','keyLabel','ARTIST_NAME','ARTIST_URL','CREDIT','CREATOR_NAME']if k in root and isinstance(root[k],dict)and root[k].get('type')=='string'}
    if metadata.get('keyLabel',mode)!=mode:raise ValueError('Selected key mode differs')
    speeds=numbers(graph,root['speed_dac']) if 'speed_dac'in root else split_numbers(root.get('speed_change',root.get('speed_data')))
    boosts=numbers(graph,root.get('boost_dac',root.get('boostDac',UNDEFINED)))
    def events(flat):
        if len(flat)%2:raise ValueError('Unpaired native events')
        return [{'nativeFrame':f,'multiplier':v}for f,v in zip(flat[::2],flat[1::2])]
    return {'mode':mode,'lanes':lanes,'speed':events(speeds),'boost':events(boosts),'colors':[],
            'metadata':metadata,'nativeLaneIndices':lane_indices,'nativeKeyCodes':numbers(graph,root['keyCtrl']) if isinstance(root.get('keyCtrl'),dict) and 'ref' in root['keyCtrl'] else None,'clockField':clock,'scheduleAgreement':True,'preparedIteration':item['frame'],
            'nativeSchedules':{'taps':[{'frame':f,'lane':l,'count':n}for (f,l),n in sorted(taps.items())],
                               'holds':[{'frame':f,'lane':l,'count':n}for (f,l),n in sorted(freezes.items())]},
            'presentationDiagnostics':diagnostics}
