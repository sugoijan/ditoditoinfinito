"""Original synthetic family contracts, corruptions and native audio alignment."""
import copy
from array import array
from pathlib import Path
import random
import struct
import tempfile
import unittest

from higan import Graph
from test_projection import OriginalGraph, string
from native_families import dac_five_projection,constant_projection,review_optional_colors,movie_observation,source_field, extract_movie_arrays, scheduled_projection, movie_audio_origin
from native_audio import origin_match
from extract_native import stream_path
from fixture import rect,tag
from swf_audio import read_movie
from prepare_pack import ssc_chart, ssc_value


def capture(roots):
    frames=[]
    for iteration,root in enumerate(roots,10):
        g=OriginalGraph();ref=g.obj(root)
        frames.append({'frame':iteration,'state':{'objects':g.objects,'roots':[{'level':0,'value':ref,'movieFrame':iteration}],
                       'incomplete':False,'avm1Halted':False,'displayTree':[]}})
    return {'source':{'movieSha256':'0'*64,'config':{'resources':[]}},'swfHeaderFps':60,'avm1Halted':False,
            'audit':{'entries':[],'truncated':False,'runtimeErrors':0,'droppedEntries':0,'truncatedDetails':0},'captures':frames}


def five(prefix=''):
    lanes=['left','down','up','right','space'];root={prefix+lane+'_data':'200,260'for lane in lanes}
    root['controller']={**{lane+'_timeline':['200','260']for lane in lanes},'Cnt':1,**{k:0 for k in ['v','w','x','y','z']}}
    return capture([root,copy.deepcopy(root)])


def seven():
    definitions=[('left','le'),('leftdia','led'),('down','do'),('space','oni'),('up','up'),('rightdia','rid'),('right','ri')]
    root={'diffin':'','speed_change':'400,1.5,','speed_dac':['400','1.5',''], 'stop_data':'500,560',
          **{name:0 for name in ['les','leds','dos','onis','ups','rids','ris']}}
    bank={clip+'_dac':['200','260','']for lane,clip in definitions};bank['stop_dac']=['500','560'];root['controller']=bank
    for lane,clip in definitions:root[lane+'_data']='200,260,'
    root['fle_dac']=['300','360',''];root['frzLeft_data']='300,360,'
    return capture([root,copy.deepcopy(root)])


def scheduled():
    lanes=['left','leftdia','down','space','up','rightdia','right']
    root={'frameNum':0,'keyLabel':'7','diffin':'','speed_change':'400,1.25',
          'mkArrow':{str(110+i*10):[i]for i in range(7)},'mkFArrow':{'210':[0]},
          'judgeY':{**{str(110+i*10):100 for i in range(7)},'210':100}}
    for i,lane in enumerate(lanes):root[lane+'_data']=str(210+i*10)
    root['frzLeft_data']='310,350'
    return capture([root])


def constant(*, delayed=False, suffix=''):
    indices=[0,2,4,6,3]
    bank=[[]for _ in range(7)];holds=[[]for _ in range(7)]
    for i in indices:bank[i]=[str(200+i*10)]
    holds[0]=['300','360']
    raw={'frame_num':0,'set_cnt':100,'set_spi':3,'dif_spi':1.5,'keyLabel':'5','diffi':'Normal','timeline':bank,'ftimeline':holds,
         'mkArrow':{},'mkFArrow':{},'speed_dac':['400','1.5'],'keyCtrl':[37,65,40,32,38,68,39]}
    raw['diffin']=suffix
    raw.update({name+'_data'+suffix:str(200+i*10) for name,i in zip(['left','down','up','right','space'],indices)})
    raw['frzLeft_data'+suffix]='300,360'
    raw['tuning']='Original creator'
    compiled=copy.deepcopy(raw)
    for i in indices:compiled['timeline'][i]=[100+i*10]
    compiled['ftimeline'][0]=[200,360]
    compiled.update(mkArrow={str(100+i*10):[i]for i in indices},mkFArrow={'200':[0]},frzLength=[{'0':60}]+[{}for _ in range(6)])
    if delayed:
        del raw['frame_num'];del compiled['frame_num']
        initialized=copy.deepcopy(compiled);initialized['frame_num']=0
        del initialized['timeline'];del initialized['ftimeline'];del initialized['tuning']
        return capture([raw,compiled,initialized])
    return capture([raw,compiled])


def dac_five(*, suffix='', fixed=False):
    definitions=[('left','le'),('down','do'),('up','up'),('right','ri'),('space','oni')]
    root={'diffin':suffix,'speed':'1.25','set_spi':2.5,'olds':2.5,'diffi':'Normal',
          'speed_dac':['400','1.5'],'speed'+suffix+'_data':'400,1.5',
          'keyCtrl':[37,300,40,32,38,301,39],
          **{clip+'s':0 for _,clip in definitions},**{'f'+clip+'s':0 for _,clip in definitions}}
    if fixed:
        root.update({'key_'+k:False for k in ('LE','DO','UP','RI','SH')});root['oni_spi']=2.5;del root['keyCtrl']
    else:root['keyLabel']='5'
    bank={'cunt':0,'sp':0,'stopcunt':0,'stop_dac':['500','560']}
    root['stop'+suffix+'_data']='500,560'
    for lane,clip in definitions:
        bank[clip+'_dac']=['200','260',''];root[lane+suffix+'_data']='200,260,'
        root['f'+clip+'_dac']=[]
    root['fle_dac']=['300','360'];root['frzLeft'+suffix+'_data']='300,360'
    root['controller']=bank
    return root


class Families(unittest.TestCase):
    def test_dac_five_selected_suffix_holds_stops_and_key_order(self):
        root=dac_five(suffix='2');root['controller']['oni_dac']={'type':'undefined'};del root['space2_data']
        chart,iteration=dac_five_projection(capture([root,copy.deepcopy(root)]))
        self.assertEqual(iteration,10)
        self.assertEqual(chart['lanes'][0]['nativeHoldFrames'],[(300,360)])
        self.assertEqual(chart['lanes'][4]['nativeFrames'],[])
        self.assertEqual(chart['nativeKeyCodes'],[37,40,38,39,32])
        self.assertEqual(chart['initialPixelSpeed'],2.5)
        self.assertEqual(chart['speed'],[{'nativeFrame':400,'multiplier':1.4}])
        self.assertEqual(chart['nativeSpeedChanges'],[{'nativeFrame':400,'multiplier':1.5,'applicationFrame':250,'pixelSpeed':3.5}])
        self.assertEqual(chart['stops'],[{'nativeFrame':500,'paused':True},{'nativeFrame':560,'paused':False}])

    def test_dac_speed_tail_retains_only_unreachable_terminal_tokens(self):
        for tokens in (['400','1.5','\n// next author //\n'],['','\n// next author //\n'],['','']):
            root=dac_five();root['speed_dac']=tokens;root['speed_data']=','.join(tokens)
            chart,_=dac_five_projection(capture([root,copy.deepcopy(root)]))
            self.assertEqual([v['text']for v in chart['inactiveSpeedTail']],tokens[2:]if tokens[0]=='400'else tokens)
        for tokens in (['400',''],['400','// broken multiplier'],['400','1.5','// interior','500','2'],['400','1.5','unknown_data=']):
            root=dac_five();root['speed_dac']=tokens;root['speed_data']=','.join(tokens)
            with self.assertRaises(ValueError):dac_five_projection(capture([root,copy.deepcopy(root)]))

    def test_dac_five_accepts_numeric_key_label_without_guessing_modes(self):
        root=dac_five();root['keyLabel']=5
        chart,_=dac_five_projection(capture([root,copy.deepcopy(root)]))
        self.assertEqual(chart['mode'],'5')
        for label in (5.5,7,-5,True):
            bad=copy.deepcopy(root);bad['keyLabel']=label
            with self.assertRaises(ValueError):dac_five_projection(capture([bad,copy.deepcopy(bad)]))

    def test_dac_five_rejects_wrong_selector_consumption_and_inactive_lanes(self):
        for corruption in ('source','hold','cursor','duplicate-bank','diagonal','mode','speed-units','speed-source','extra-effect','missing-control'):
            with self.subTest(corruption=corruption):
                root=dac_five()
                if corruption=='source':root['left_data']='201,260'
                elif corruption=='hold':root['fle_dac']=['300','361']
                elif corruption=='cursor':root['fles']=1
                elif corruption=='duplicate-bank':root['duplicate']=copy.deepcopy(root['controller'])
                elif corruption=='diagonal':root['controller']['led_dac']=['220']
                elif corruption=='mode':root['keyLabel']='7'
                elif corruption=='speed-units':root['set_spi']=3
                elif corruption=='speed-source':root['speed_data']='400,1.25'
                elif corruption=='extra-effect':root['boost_data']='400,2'
                else:del root['keyLabel']
                with self.assertRaises(ValueError):dac_five_projection(capture([root,copy.deepcopy(root)]))

    def test_dac_five_fixed_controls_and_bank_stability(self):
        root=dac_five(fixed=True);chart,_=dac_five_projection(capture([root,copy.deepcopy(root)]),fixed=True)
        self.assertIsNone(chart['nativeKeyCodes'])
        self.assertEqual(chart['speed'],[{'nativeFrame':400,'multiplier':1.5}])
        self.assertEqual(chart['nativeSpeedChanges'][0]['pixelSpeed'],3.75)
        self.assertEqual(chart['independentOnigiriPixelSpeed'],2.5)
        with self.assertRaises(ValueError):dac_five_projection(capture([root,copy.deepcopy(root)]))
        bad=copy.deepcopy(root);bad['onisp_change']='400,2'
        with self.assertRaises(ValueError):dac_five_projection(capture([bad,copy.deepcopy(bad)]),fixed=True)
        bad=copy.deepcopy(root);bad['oni_spi']=3
        with self.assertRaises(ValueError):dac_five_projection(capture([bad,copy.deepcopy(bad)]),fixed=True)
        later=copy.deepcopy(root);later['left_data']='200,261';later['controller']['le_dac']=['200','261']
        with self.assertRaises(ValueError):dac_five_projection(capture([root,later]),fixed=True)
        with self.assertRaises(ValueError):dac_five_projection(capture([root]),fixed=True)


    def test_dac_five_motion_uses_arrow_local_speed_at_birth(self):
        data=capture([{}, {}, {}])
        for item,clock in zip(data['captures'],(148,149,150)):
            graph=OriginalGraph();root=graph.obj({'unused':True});arrow=graph.obj({'spd':3})
            item['state'].update(objects=graph.objects,roots=[{'level':0,'value':root,'movieFrame':clock}])
            item['state']['displayTree']=[{'kind':'movie-clip','name':string('le_s1'),'object':arrow,'visible':True,'stageRegistrationPixels':[10,50+(160-clock)*3]},
                {'kind':'movie-clip','name':string('le_step'),'visible':True,'stageRegistrationPixels':[10,50],'stageBoundsPixels':[0,0,50,50]}]
        chart={'initialPixelSpeed':4,'motionSpeedPolicy':'per-arrow-at-spawn',
               'lanes':[{'nativeFrames':[160]}],'speed':[{'nativeFrame':160,'multiplier':.75},{'nativeFrame':170,'multiplier':.875}]}
        result=movie_observation(data,chart,'le_s1','le_step',0)
        self.assertEqual(result['observedAuthorMultiplier'],.75)
        self.assertEqual(result['arrivalDifferenceFrames'],0)
        self.assertEqual(result['baseArrowHeightsPerSecond'],4.8)
        next(p for p in data['captures'][1]['state']['objects'][1]['properties']if p['name']['text']=='spd')['value']=3.5
        with self.assertRaises(ValueError):movie_observation(data,chart,'le_s1','le_step',0)

    def test_selected_suffix_layout_is_unambiguous(self):
        self.assertEqual(source_field({'left_hard_data':1},'left','_hard'),'left_hard_data')
        self.assertEqual(source_field({'left_data_hard':1},'left','_hard'),'left_data_hard')
        with self.assertRaises(ValueError):source_field({'left_data_hard':1,'left_hard_data':2},'left','_hard')

    def test_initialized_bank_and_selected_source_prefix(self):
        chart,_=extract_movie_arrays(five('n'),'child-timeline-5','n')
        self.assertEqual([l['nativeFrames']for l in chart['lanes']],[[200,260]]*5)
        bad=five();bad['captures'][1]['state']['objects'][0]['properties'][0]['value']=string('201,260')
        with self.assertRaises(ValueError):extract_movie_arrays(bad,'child-timeline-5')
        with self.assertRaises(ValueError):extract_movie_arrays(five(),'unknown')
        bad=five();bad['audit']['entries']=[{'kind':'fetch-denied','detail':'https://example.invalid/'}]
        with self.assertRaises(ValueError):extract_movie_arrays(bad,'child-timeline-5')

    def test_seven_holds_and_stop_flags_are_preserved_without_timing_stops(self):
        chart,_=extract_movie_arrays(seven(),'dac-7')
        self.assertEqual(chart['lanes'][0]['nativeHoldFrames'],[(300,360)])
        self.assertEqual(chart['stops'],[{'nativeFrame':500,'paused':True},{'nativeFrame':560,'paused':False}])
        self.assertEqual(chart['speed'],[{'nativeFrame':400,'multiplier':1.5}])

    def test_compiled_spawn_plus_travel_is_independent_evidence(self):
        chart=scheduled_projection(scheduled(),'7');self.assertTrue(chart['scheduleAgreement'])
        self.assertEqual(chart['lanes'][0]['nativeFrames'],[210]);self.assertEqual(chart['lanes'][0]['nativeHoldFrames'],[(310,350)])
        bad=scheduled();state=bad['captures'][0]['state']
        obj=next(o for o in state['objects']if any(p['name']['text']=='110'for p in o['properties']) and any(p['value']==100 for p in o['properties']))
        next(p for p in obj['properties']if p['name']['text']=='110')['value']=99
        with self.assertRaises(ValueError):scheduled_projection(bad,'7')
        bad=scheduled();bad['audit']['runtimeErrors']=1;bad['audit']['entries']=[{'kind':'runtime-error','detail':'unknown','iteration':20}]
        with self.assertRaises(ValueError):scheduled_projection(bad,'7',allow_missing_decorative_sprites=True)

    def test_scheduled_initial_speed_uses_native_units_and_parsed_events(self):
        data=scheduled();graph=data['captures'][0]['state']
        root=OriginalGraph();value=root.obj({'frameNum':0,'keyLabel':'7','diffin':'',
            'set_spi':3,'dif_spi':1.5,'speed_change':'400,1.3','speed_dac':[400,1.25],
            'mkArrow':{str(110+i*10):[i]for i in range(7)},'mkFArrow':{'210':[0]},
            'judgeY':{**{str(110+i*10):100 for i in range(7)},'210':100},
            **{name+'_data':str(210+i*10)for i,name in enumerate(['left','leftdia','down','space','up','rightdia','right'])},
            'frzLeft_data':'310,350'})
        graph.update(objects=root.objects,roots=[{'level':0,'value':value,'movieFrame':10}])
        chart=scheduled_projection(data,'7')
        self.assertEqual(chart['initialPixelSpeed'],3)
        self.assertEqual(chart['speed'],[{'nativeFrame':400,'multiplier':1.25}])
        obj=root.objects[0]
        next(p for p in obj['properties']if p['name']['text']=='set_spi')['value']=4
        with self.assertRaises(ValueError):scheduled_projection(data,'7')

    def test_constant_travel_preserves_raw_frames_and_independent_duration_cache(self):
        data=constant();chart=constant_projection(data,'5')
        self.assertEqual(chart['lanes'][0]['nativeFrames'],[200])
        self.assertEqual(chart['lanes'][0]['nativeHoldFrames'],[(300,360)])
        self.assertEqual(chart['constantTravelFrames'],100)
        bad=constant();state=bad['captures'][1]['state']
        cache=next(o for o in state['objects']if any(p['name']['text']=='0'and p['value']==60 for p in o['properties']))
        next(p for p in cache['properties']if p['name']['text']=='0')['value']=61
        with self.assertRaises(ValueError):constant_projection(bad,'5')
        with self.assertRaises(ValueError):constant_projection(capture([]),'5')

    def test_base_velocity_does_not_double_apply_an_active_author_ratio(self):
        data=capture([{'frame_num':148},{'frame_num':149},{'frame_num':150}])
        for item,y in zip(data['captures'],[92,86,80]):
            item['state']['displayTree']=[{'kind':'movie-clip','name':string('le_s0'),'visible':True,'stageRegistrationPixels':[10,y]},
                {'kind':'movie-clip','name':string('le_step'),'visible':True,'stageRegistrationPixels':[10,20],'stageBoundsPixels':[0,0,50,50]}]
        chart={'initialPixelSpeed':3,'lanes':[{'nativeFrames':[160]}],'speed':[{'nativeFrame':0,'multiplier':2}]}
        measured=movie_observation(data,chart,'le_s0','le_step',0,clock_field='frame_num')
        self.assertEqual(measured['pixelsPerSecond'],360)
        self.assertEqual(measured['baseArrowHeightsPerSecond'],3.6)
        self.assertEqual(measured['observedAuthorMultiplier'],2)
        chart['initialPixelSpeed']=6
        with self.assertRaises(ValueError):movie_observation(data,chart,'le_s0','le_step',0,clock_field='frame_num')

    def test_twip_quantization_preserves_fractional_native_speed_proof(self):
        data=capture([{'frame_num':v}for v in range(830,836)])
        for item,clock in zip(data['captures'],range(830,836)):
            y=50+(836-clock)*4.4
            item['state']['displayTree']=[{'kind':'movie-clip','name':string('le_s0'),'visible':True,'stageRegistrationPixels':[85,y]},
                {'kind':'movie-clip','name':string('le_step'),'visible':True,'stageRegistrationPixels':[85,50],'stageBoundsPixels':[0,0,50,50]}]
        chart={'initialPixelSpeed':3.5,'lanes':[{'nativeFrames':[835]}],'speed':[{'nativeFrame':778,'multiplier':1.25}]}
        measured=movie_observation(data,chart,'le_s0','le_step',0,clock_field='frame_num')
        self.assertAlmostEqual(measured['pixelsPerSecond'],264)
        self.assertEqual(measured['baseArrowHeightsPerSecond'],4.2)
        self.assertAlmostEqual(measured['arrivalDifferenceFrames'],1,delta=.02)
        data['captures'][2]['state']['displayTree'][0]['stageRegistrationPixels'][1]+=.2
        with self.assertRaises(ValueError):movie_observation(data,chart,'le_s0','le_step',0,clock_field='frame_num')

    def test_optional_color_review_cannot_hide_other_failures(self):
        data=constant();data['source']['config']['sourceUrl']='https://example.invalid/song/game.swf'
        denied={'kind':'fetch-denied','detail':'iro.txt','iteration':2}
        error={'kind':'runtime-error','detail':'message=Asynchronous error occurred: Could not fetch: "Invalid URL iro.txt: no exact GET match in immutable resource cache" ','iteration':2}
        data['audit'].update(entries=[denied,error],runtimeErrors=1)
        review={'schema':'ddi-native-optional-color-review/1','sourceSwfSha256':'0'*64,
                'missingResources':[{'url':'https://example.invalid/song/iro.txt','status':404,'sha256':'1'*64,'retrievedAt':'2026-10-10'}]}
        cleaned,retained=review_optional_colors(data,review)
        self.assertEqual(retained,[denied,error]);self.assertEqual(data['audit']['runtimeErrors'],1)
        self.assertEqual(constant_projection(cleaned,'5')['lanes'][0]['nativeFrames'],[200])
        changed=copy.deepcopy(data);changed['audit']['entries'][0]['detail']='dos.txt'
        with self.assertRaises(ValueError):review_optional_colors(changed,review)
        changed=copy.deepcopy(review);changed['missingResources'][0]['status']=200
        with self.assertRaises(ValueError):review_optional_colors(data,changed)

    def test_constant_clock_can_initialize_after_array_cleanup(self):
        data=constant(delayed=True,suffix='2')
        chart=constant_projection(data,'5')
        self.assertEqual(chart['splitIteration'],10)
        self.assertEqual(chart['compiledIteration'],11)
        self.assertEqual(chart['preparedIteration'],12)
        self.assertEqual(chart['metadata']['tuning'],'Original creator')
        self.assertEqual(chart['lanes'][0]['nativeHoldFrames'],[(300,360)])
        with self.assertRaises(ValueError):constant_projection({**data,'captures':data['captures'][:2]},'5')
        bad=copy.deepcopy(data)
        root=Graph(bad['captures'][2]['state']).root()
        obj=next(o for o in bad['captures'][2]['state']['objects'] if o['id']==root['mkArrow']['ref'])
        next(p for p in obj['properties'] if p['name']['text']=='100')['name']=string('101')
        with self.assertRaises(ValueError):constant_projection(bad,'5')
        bad=copy.deepcopy(data)
        obj=bad['captures'][2]['state']['objects'][0]
        next(p for p in obj['properties'] if p['name']['text']=='frame_num')['value']=1
        with self.assertRaises(ValueError):constant_projection(bad,'5')

    def test_schedule_constructor_metadata_is_not_an_extra_event(self):
        data=constant(delayed=True)
        for item in data['captures']:
            graph=Graph(item['state']);root=graph.root()
            obj=next(o for o in item['state']['objects'] if o['id']==root['mkArrow']['ref'])
            obj['properties'].append({'name':string('constructor'),'value':{'type':'uninspected','reason':'native constructor'}})
        self.assertTrue(constant_projection(data,'5')['scheduleAgreement'])
        bad=copy.deepcopy(data)
        graph=Graph(bad['captures'][1]['state']);root=graph.root()
        obj=next(o for o in bad['captures'][1]['state']['objects'] if o['id']==root['mkArrow']['ref'])
        obj['properties'][-1]['name']=string('unknown_event')
        with self.assertRaises(ValueError):constant_projection(bad,'5')

    def test_constant_selected_source_and_duration_must_agree(self):
        data=constant(delayed=True,suffix='2')
        bad=copy.deepcopy(data)
        obj=bad['captures'][0]['state']['objects'][0]
        next(p for p in obj['properties'] if p['name']['text']=='left_data2')['value']=string('201')
        with self.assertRaises(ValueError):constant_projection(bad,'5')
        bad=copy.deepcopy(data)
        graph=Graph(bad['captures'][2]['state']);root=graph.root()
        ref=graph.array(root['frzLength'])[0]
        obj=next(o for o in bad['captures'][2]['state']['objects'] if o['id']==ref['ref'])
        next(p for p in obj['properties'] if p['name']['text']=='0')['value']=59
        with self.assertRaises(ValueError):constant_projection(bad,'5')

    def test_audio_start_selects_first_instance_and_limits_restarts(self):
        c=five();c['audio']={'streamStarts':[{'iteration':10,'compression':'Mp3','outputSampleFrame':6615},
                                           {'iteration':11,'compression':'Mp3','outputSampleFrame':7350}]}
        origin=movie_audio_origin(c);self.assertEqual(origin['logicalFrame'],10);self.assertEqual(origin['nextStreamOutputSampleFrame'],7350)

    def test_source_labels_are_distinct_from_tiers(self):
        lanes=[{'tapsSeconds':[1],'holdsSeconds':[]}]+[{'tapsSeconds':[],'holdsSeconds':[]}for _ in range(4)]
        text=ssc_chart({'mode':'5','lanes':lanes,'metadata':{'difficultyLabel':'Original publisher label'}},'Edit',None)
        self.assertEqual(ssc_value('RE:RAVE'),r'RE\:RAVE')
        self.assertEqual(ssc_value(r'a\b//c#d'),r'a\\b\/\/c\#d')
        self.assertIn('#CHARTNAME:Original publisher label;',text);self.assertIn('#DIFFICULTY:Edit;',text);self.assertIn('#METER:0;',text)
        with self.assertRaises(ValueError):ssc_chart({'mode':'5','lanes':lanes,'metadata':{'difficultyLabel':'bad;header'}},'Edit',None)

    def test_native_slice_offsets_exclude_the_swf_header_and_other_sprites(self):
        # Two original synthetic MPEG frames: metadata parsing only, not decoded.
        frame=bytes.fromhex('fffb90c4')+bytes(413)
        head=b'\x0e\x2e'+struct.pack('<Hh',735,0)
        root_tags=tag(45,head)+tag(19,struct.pack('<Hh',1152,0)+frame)+tag(1)
        nested=struct.pack('<HH',7,1)+tag(45,head)+tag(19,struct.pack('<Hh',1152,0)+frame)+tag(1)+tag(0)
        root_tags+=tag(39,nested)+tag(0)
        body=rect(320,240)+struct.pack('<HH',60*256,1)+root_tags
        movie=b'FWS\x08'+struct.pack('<I',len(body)+8)+body
        with tempfile.TemporaryDirectory()as d:
            p=Path(d)/'own.swf';p.write_bytes(movie);_,data,h=read_movie(p)
            route,encoded,report=stream_path(p,{'sliceStart':0,'sliceEnd':len(root_tags),'sampleRate':44100,'stereo':False,'latencySeek':0})
            self.assertEqual(route,'root');self.assertEqual(encoded,frame);self.assertEqual(report['nativeSlice']['tagDataOffsetBase'],h['tagStart'])


class Audio(unittest.TestCase):
    def test_measured_shift_and_silence_rejection(self):
        r=random.Random(123);source=array('h',[r.randrange(-12000,12000)for _ in range(200000)])
        native=source[137*2:];report=origin_match(native,source,0)
        self.assertEqual(report['lagSamplesAt44100'],137)
        self.assertGreaterEqual(len(report['windows']),2)
        with self.assertRaises(ValueError):origin_match(array('h',[100]*150000),array('h',[100]*200000),0)


if __name__=='__main__':unittest.main()

class ParaFla(unittest.TestCase):
    def test_numeric_suffix_is_bounded_and_not_a_rating(self):
        from native_families import source_suffix
        self.assertEqual(source_suffix(33.0),'33')
        self.assertEqual(source_suffix(string('')),'')
        for value in [True,-1,1.5,float('nan'),1000]:
            with self.assertRaises(ValueError):source_suffix(value)

    def fixture(self):
        root={'frameNum':0,'keyLabel':'5','diffin':33,'DOS_DIVIDE':True,'setSpd':5,'difSpd':2.5,
            'keyCtrl':[37,0,40,32,38,0,39],'arrowSpd':[0,5,400,7.5],'speed_change':'900,3',
            'speed_data':'401,1.5','boostDac':[],
            'mkArrow':{'110':[0],'120':[2],'130':[4],'140':[6],'150':[3],'160':[1]},
            'mkFArrow':{},'judgeY':{str(f):100 for f in [110,120,130,140,150,160]}}
        for name,frame in zip(['left','down','up','right','space'],[210,220,230,240,250]):root[name+'_data']=str(frame)
        return capture([root])

    def test_divided_fields_parsed_speeds_and_inactive_schedules(self):
        c=scheduled_projection(self.fixture(),'5',parafla=True)
        self.assertEqual(c['speed'],[{'nativeFrame':400,'multiplier':1.5}])
        self.assertEqual(c['initialPixelSpeed'],5)
        self.assertEqual(c['sourceSelection'],{'runtimeSuffix':'33','splitDos':True,'loadedFieldSuffix':''})
        self.assertEqual(c['inactiveSchedules']['taps'],[{'frame':160,'lane':1,'count':1}])
        self.assertEqual(sum(len(l['nativeFrames'])for l in c['lanes']),5)
        with self.assertRaises(ValueError):scheduled_projection(self.fixture(),'5')

    def test_active_extra_lane_and_wrong_speed_units_fail(self):
        for field in ['keyCtrl','setSpd']:
            c=self.fixture();g=c['captures'][0]['state'];root=g['objects'][g['roots'][0]['value']['ref']]['properties']
            if field=='setSpd':next(p for p in root if p['name']['text']==field)['value']=6
            else:
                ref=next(p for p in root if p['name']['text']==field)['value']['ref']
                next(p for p in g['objects'][ref]['properties']if p['name']['text']=='1')['value']=65
            with self.assertRaises(ValueError):scheduled_projection(c,'5',parafla=True)
