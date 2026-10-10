"""Original synthetic family contracts, corruptions and native audio alignment."""
import copy
from array import array
from pathlib import Path
import random
import struct
import tempfile
import unittest

from test_projection import OriginalGraph, string
from native_families import constant_projection,review_optional_colors,movie_observation,source_field, extract_movie_arrays, scheduled_projection, movie_audio_origin
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


def constant():
    indices=[0,2,4,6,3]
    bank=[[]for _ in range(7)];holds=[[]for _ in range(7)]
    for i in indices:bank[i]=[str(200+i*10)]
    holds[0]=['300','360']
    raw={'frame_num':0,'set_cnt':100,'set_spi':3,'dif_spi':1.5,'keyLabel':'5','diffi':'Normal','timeline':bank,'ftimeline':holds,
         'mkArrow':{},'mkFArrow':{},'speed_dac':['400','1.5'],'keyCtrl':[37,65,40,32,38,68,39]}
    compiled=copy.deepcopy(raw)
    for i in indices:compiled['timeline'][i]=[100+i*10]
    compiled['ftimeline'][0]=[200,360]
    compiled.update(mkArrow={str(100+i*10):[i]for i in indices},mkFArrow={'200':[0]},frzLength=[{'0':60}]+[{}for _ in range(6)])
    return capture([raw,compiled])


class Families(unittest.TestCase):
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
