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


def source_suffix(value):
    if isinstance(value,dict):return text(value)
    if type(value)in(int,float)and math.isfinite(value)and 0<=value<=999 and int(value)==value:
        return str(int(value))
    raise ValueError('Source suffix must be a complete string or nonnegative integer')


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
            suffix=source_suffix(root['diffin']) if 'diffin'in root else ''
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
    a,b=frames[0],frames[-1];velocity=(b['y']-a['y'])/(b['movieFrame']-a['movieFrame'])
    # Flash stage positions are quantized to twentieths of a pixel (twips).
    tolerance=.05+1e-9
    if velocity>=0:raise ValueError('Expected upward linear movement')
    for f in frames:
        if f['movieFrame']-a['movieFrame']!=f['iteration']-a['iteration'] or abs(f['x']-a['x'])>1e-6 or abs(f['y']-(a['y']+velocity*(f['movieFrame']-a['movieFrame'])))>tolerance:
            raise ValueError('Nonlinear/reused arrow or non-movie clock')
    arrival=a['movieFrame']+(a['targetY']-a['y'])/velocity
    raw=chart['lanes'][lane]['nativeFrames'][0]
    if 'initialPixelSpeed'not in chart and abs(arrival-raw)>1.5:raise ValueError('Arrow arrival does not agree with source frame')
    base_velocity=abs(velocity);author_ratio=1.0
    if 'initialPixelSpeed'in chart:
        def active(frame):
            ratio=1.0
            for event in chart['speed']:
                if event['nativeFrame']<=frame:ratio=event['multiplier']
            return ratio
        ratios=[active(f['movieFrame'])for f in frames]
        if any(r!=ratios[0]for r in ratios)or ratios[0]<=0:raise ValueError('Need one positive author speed segment for base movement')
        author_ratio=ratios[0];base_velocity=chart['initialPixelSpeed']
        # Assigning _y can round each increment to one native twip. Corroborate
        # the stored pixel increment per frame, rather than accumulating an
        # impossible sub-twip position prediction across multiple assignments.
        if abs(abs(velocity)-base_velocity*author_ratio)>tolerance:
            raise ValueError('Observed movement disagrees with initial units and author ratio')
        for previous,current in zip(frames,frames[1:]):
            ticks=current['movieFrame']-previous['movieFrame']
            if abs((previous['y']-current['y'])-base_velocity*author_ratio*ticks)>tolerance*ticks:
                raise ValueError('Native increment does not explain quantized movement')
        if abs(arrival-raw)>1.5:raise ValueError('Arrow arrival does not agree with source frame')
    # Receptor art registration differs slightly from the judgment target in
    # early sources; target_y can be supplied only after independent native proof.
    return {'schema':'ddi-native-motion-measurement/experimental-1','sourceSwfSha256':capture['source']['movieSha256'],
            'fps':60,'pixelsPerSecond':abs(velocity)*60,'referencePixels':widths[0],
            'baseArrowHeightsPerSecond':base_velocity*60/widths[0],'observedAuthorMultiplier':author_ratio,'noteName':note_name,'receptorName':receptor_name,'geometryReferenceName':geometry_name or receptor_name,'clockField':clock_field,
            'stagePositionTolerancePixels':.05,'arrivalDifferenceFrames':arrival-raw,
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


def scheduled_projection(capture, mode, *, allow_missing_decorative_sprites=False, parafla=False):
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
            suffix=source_suffix(root['diffin'])
            # Divided ParaFla sidecars load the selected chart into unsuffixed
            # fields. The complete compiled schedule proves that association.
            if root.get('DOS_DIVIDE')is True:suffix=''
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
        inactive=[]
        if parafla:
            codes=numbers(graph,root['keyCtrl'])
            if 'frameNum'not in root or 'arrowSpd'not in root or len(codes)!=7 or numeric(root['setSpd'])!=2*numeric(root['difSpd'])or numeric(root['setSpd'])<=0:
                raise ValueError('Unproved ParaFla clock, keys or initial speed units')
            inactive=[index for index,code in enumerate(codes)if code==0 and index not in lane_indices]
        all_taps=schedule(graph,root['mkArrow'],lane_indices+inactive)
        all_freezes=schedule(graph,root['mkFArrow'],lane_indices+inactive)
        taps=Counter({k:v for k,v in all_taps.items()if k[1]in lane_indices})
        freezes=Counter({k:v for k,v in all_freezes.items()if k[1]in lane_indices})
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
        required=[clip+'_step'for index,(_,clip)in enumerate(SEVEN)if index in lane_indices]
        if not any(all(display_map(i).get(name) for name in required)for i in capture['captures']):raise ValueError('Required selected-mode sprites are missing')
    metadata={k:text(root[k])for k in ['musicTitle','GAME_TITLE','TUNING_NAME','TUNING_NAMES','tuning','diffi','VERSION','keyLabel','ARTIST_NAME','ARTIST_URL','CREDIT','CREATOR_NAME']if k in root and isinstance(root[k],dict)and root[k].get('type')=='string'}
    if metadata.get('keyLabel',mode)!=mode:raise ValueError('Selected key mode differs')
    speeds=numbers(graph,root['speed_dac']) if 'speed_dac'in root else split_numbers(root.get('speed_change',root.get('speed_data')))
    boosts=numbers(graph,root.get('boost_dac',root.get('boostDac',UNDEFINED)))
    parafla_data={}
    if not parafla and 'set_spi' in root and 'dif_spi' in root:
        initial=numeric(root['set_spi'])
        if initial<=0 or initial!=2*numeric(root['dif_spi']):
            raise ValueError('Unproved scheduled-family initial pixel speed')
        parafla_data['initialPixelSpeed']=initial
    if parafla:
        initial=numeric(root['setSpd']);pixels=numbers(graph,root['arrowSpd'])
        if len(pixels)%2 or not pixels or pixels[:2]!=[0,initial]or any(a>=b for a,b in zip(pixels[::2],pixels[2::2])):
            raise ValueError('Invalid parsed ParaFla pixel speed bank')
        # The first pair initializes base movement; it is not an author change.
        speeds=[value/initial if index%2 else value for index,value in enumerate(pixels[2:])]
        parafla_data={'initialPixelSpeed':initial,'parsedPixelSpeeds':pixels,
            'sourceSelection':{'runtimeSuffix':source_suffix(root['diffin']),'splitDos':root.get('DOS_DIVIDE')is True,'loadedFieldSuffix':suffix},
            'inactiveSchedules':{'nativeKeyCodes':numbers(graph,root['keyCtrl']),
                'taps':[{'frame':f,'lane':l,'count':n}for(f,l),n in sorted(all_taps.items())if l in inactive],
                'holds':[{'frame':f,'lane':l,'count':n}for(f,l),n in sorted(all_freezes.items())if l in inactive]}}
    def events(flat):
        if len(flat)%2:raise ValueError('Unpaired native events')
        return [{'nativeFrame':f,'multiplier':v}for f,v in zip(flat[::2],flat[1::2])]
    return {**parafla_data,'mode':mode,'lanes':lanes,'speed':events(speeds),'boost':events(boosts),'colors':[],
            'metadata':metadata,'nativeLaneIndices':lane_indices,'nativeKeyCodes':numbers(graph,root['keyCtrl']) if isinstance(root.get('keyCtrl'),dict) and 'ref' in root['keyCtrl'] else None,'clockField':clock,'scheduleAgreement':True,'preparedIteration':item['frame'],
            'nativeSchedules':{'taps':[{'frame':f,'lane':l,'count':n}for (f,l),n in sorted(taps.items())],
                               'holds':[{'frame':f,'lane':l,'count':n}for (f,l),n in sorted(freezes.items())]},
            'presentationDiagnostics':diagnostics}


def constant_projection(capture,mode):
    """Cross-check split judgement arrays, constant travel and compiled schedules."""
    audit_ok(capture)
    if mode not in ('5','7') or capture['swfHeaderFps']!=60:raise ValueError('Unsupported constant schedule mode/clock')
    definitions=FIVE if mode=='5'else SEVEN
    indices=[0,2,4,6,3]if mode=='5'else list(range(7))
    before=None
    for item in capture['captures']:
        graph=Graph(item['state']);root=graph.root()
        if any(k not in root for k in ['timeline','ftimeline','mkArrow','mkFArrow','frame_num','set_cnt']):continue
        if numeric(root['frame_num'])!=0:continue
        taps=graph.array(root['timeline']);holds=graph.array(root['ftimeline'])
        lanes=[{'name':name,'nativeFrames':numbers(graph,taps[i]),'nativeHoldFrames':numbers(graph,holds[i])}for (name,_),i in zip(definitions,indices)]
        if before is None:
            # Split source strings precede numeric spawn-time mutation.
            values=[v for i in indices for v in graph.array(taps[i])]
            if not values or not all(isinstance(v,dict)and v.get('type')=='string'for v in values):continue
            for lane in lanes:
                values=lane['nativeHoldFrames']
                if len(values)%2:raise ValueError('Unpaired constant-family holds')
                lane['nativeHoldFrames']=list(zip(values[::2],values[1::2]))
                if any(b<=a for a,b in lane['nativeHoldFrames']):raise ValueError('Invalid constant-family hold duration')
            before=(item,lanes,numeric(root['set_cnt']))
            continue
        original,raw,travel=before
        if not 0<travel<=12000 or numeric(root['set_cnt'])!=travel:raise ValueError('Unstable constant travel setting')
        compiled=schedule(graph,root['mkArrow'],indices);freezes=schedule(graph,root['mkFArrow'],indices)
        expected=Counter((f-travel,i)for i,lane in zip(indices,raw)for f in lane['nativeFrames'])
        expected_h=Counter((a-travel,i)for i,lane in zip(indices,raw)for a,b in lane['nativeHoldFrames'])
        if not compiled:continue
        if compiled!=expected or freezes!=expected_h:raise ValueError('Constant travel disagrees with compiled schedule')
        for source,prepared in zip(raw,lanes):
            if prepared['nativeFrames']!=[f-travel for f in source['nativeFrames']]:raise ValueError('Constant tap mutation mismatch')
            expected_holds=[v for a,b in source['nativeHoldFrames']for v in(a-travel,b)]
            if prepared['nativeHoldFrames']!=expected_holds:raise ValueError('Constant hold mutation mismatch')
        lengths=graph.array(root['frzLength'])
        for index,source in zip(indices,raw):
            if source['nativeHoldFrames']:
                duration=graph.props(lengths[index])
                if any(numeric(duration[str(i*2)])!=b-a for i,(a,b)in enumerate(source['nativeHoldFrames'])):raise ValueError('Constant hold duration cache mismatch')
        metadata={k:text(root[k])for k in ['diffi','keyLabel','musicTitle','tuning']if isinstance(root.get(k),dict)and root[k].get('type')=='string'}
        if metadata.get('keyLabel')!=mode:raise ValueError('Constant schedule key mode changed')
        speed=numbers(graph,root['speed_dac'])
        if len(speed)%2:raise ValueError('Unpaired constant-family speeds')
        if any(a>b for a,b in zip(speed[::2],speed[2::2])):raise ValueError('Unordered constant-family speeds')
        if numeric(root['set_spi'])!=numeric(root['dif_spi'])*2:raise ValueError('Unproved initial constant-family pixel speed')
        return {'mode':mode,'lanes':raw,'speed':[{'nativeFrame':f,'multiplier':v}for f,v in zip(speed[::2],speed[1::2])],
                'boost':[],'colors':[],'metadata':metadata,'nativeLaneIndices':indices,'nativeKeyCodes':numbers(graph,root['keyCtrl']),
                'initialPixelSpeed':numeric(root['set_spi']),'clockField':'frame_num','scheduleAgreement':True,'preparedIteration':item['frame'],'splitIteration':original['frame'],'constantTravelFrames':travel,
                'nativeSchedules':{'taps':[{'frame':f,'lane':l,'count':n}for(f,l),n in sorted(compiled.items())],
                                   'holds':[{'frame':f,'lane':l,'count':n}for(f,l),n in sorted(freezes.items())]},
                'nativePresentation':{'iro_dac':graph.array(root['iro_dac'])if root.get('iro_dac',UNDEFINED)!=UNDEFINED else[]}}
    raise ValueError('Need split and compiled constant-family snapshots before consumption')


def review_optional_colors(capture,review):
    """Accept only captured HTTP-404 color dependencies, retaining their diagnostics."""
    if review.get('schema')!='ddi-native-optional-color-review/1' or review.get('sourceSwfSha256')!=capture['source']['movieSha256']:raise ValueError('Optional color review must bind this movie')
    rows=review['missingResources']
    if not rows or any(r.get('status')!=404 or not r.get('url','').endswith('/iro.txt')or not r.get('sha256')or not r.get('retrievedAt')for r in rows):raise ValueError('Need observed 404 provenance for the color sidecar')
    from urllib.parse import urljoin
    if {r['url']for r in rows}!={urljoin(capture['source']['config']['sourceUrl'],'iro.txt')}:raise ValueError('Color review does not match the requested sidecar')
    denied=[e for e in capture['audit']['entries']if e['kind']=='fetch-denied']
    errors=[e for e in capture['audit']['entries']if e['kind']=='runtime-error']
    message='message=Asynchronous error occurred: Could not fetch: "Invalid URL iro.txt: no exact GET match in immutable resource cache" '
    if len(denied)!=1 or len(errors)!=1 or denied[0]['detail']!='iro.txt' or errors[0]['detail']!=message or denied[0]['iteration']!=errors[0]['iteration']:raise ValueError('Unknown missing-resource failure cannot be treated as optional color')
    retained=denied+errors
    clean={**capture,'audit':{**capture['audit'],'entries':[e for e in capture['audit']['entries']if e not in retained],
                             'runtimeErrors':capture['audit']['runtimeErrors']-1}}
    audit_ok(clean)
    return clean,retained
