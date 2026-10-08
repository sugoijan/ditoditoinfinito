"""Original synthetic state tests for the pinned collection adapter; no SWFs."""
import copy
import unittest

from kouhaku import project, config_for, sm_text, SELECTIONS, SWF_HASH, LANES
from test_projection import OriginalGraph, string


def synthetic_capture():
    captures = []
    for iteration, scene, timeline in [(90,4,None),(100,6,2),(101,6,3),(102,6,4)]:
        graph = OriginalGraph()
        root = {'artist':SELECTIONS[0][2],'difficulty':SELECTIONS[0][3],
                **{lane+'_data':'300,420' for lane in LANES}}
        ref = graph.obj(root)
        display = [{'object':ref,'parent':None,'movieFrame':scene}]
        if timeline is not None:
            controller = graph.obj({'Cnt':1,**{lane+'_timeline':['300','420'] for lane in LANES}})
            display += [{'object':None,'parent':0,'movieFrame':timeline},
                        {'object':controller,'parent':1,'movieFrame':1}]
        captures.append({'frame':iteration,'state':{'incomplete':False,'avm1Halted':False,
            'roots':[{'level':0,'movieFrame':scene,'value':ref}],'objects':graph.objects,'displayTree':display,
            'globalScopes':[{'caseSensitive':False,'properties':[
                {'name':string(k),'value':string(v) if isinstance(v,str) else v,'accessor':False}
                for k,v in {'kyoku':SELECTIONS[0][1],'SpeedY':188,'SpeedUp':2,'Reverse':False,'point':0}.items()]}]}})
    return {'source':{'movieSha256':SWF_HASH},'swfHeaderFps':60,'avm1Halted':False,'build':{},
            'audit':{'truncated':False,'runtimeErrors':0,'droppedEntries':0,'truncatedDetails':0,'entries':[]},
            'audio':{'streamStarts':[{'iteration':100,'outputSampleFrame':72765,'compression':'Mp3',
                    'sampleRate':22050,'stereo':False,'latencySeek':1633}]},'captures':captures}


class Projection(unittest.TestCase):
    def test_native_arrays_and_nominal_origin(self):
        ref = project(synthetic_capture(),SELECTIONS[0])
        self.assertEqual(ref['audioOrigin']['logicalFrame'],2)
        for lane in ref['lanes']:
            self.assertEqual(lane['judgementFrames'],[300,420])
            self.assertEqual(lane['spawnMovieFrames'],[150,270])
            self.assertEqual(lane['tapsSeconds'],[298/60,418/60])
        self.assertFalse(ref['metadata']['chartAuthor'])

    def test_failed_incomplete_mutated_or_unreviewed_state_is_rejected(self):
        for change in [lambda s:s['source'].update(movieSha256='0'*64),
                       lambda s:s.update(avm1Halted=True),
                       lambda s:s['audit'].update(runtimeErrors=1),
                       lambda s:s['audio']['streamStarts'][0].update(latencySeek=0),
                       lambda s:s['captures'].pop(),
                       lambda s:s['captures'][1]['state'].update(incomplete=True),
                       lambda s:s['captures'][2]['state']['displayTree'][1].update(movieFrame=5),
                       lambda s:s['captures'][1]['state']['objects'][0]['properties'][2].update(value=string('301,420'))]:
            data=copy.deepcopy(synthetic_capture());change(data)
            with self.assertRaises((ValueError,KeyError,StopIteration)):
                project(data,SELECTIONS[0])

    def test_simfile_grid_is_exact_and_five_key(self):
        ref=project(synthetic_capture(),SELECTIONS[0]);sm=sm_text(ref,'229.wav')
        self.assertIn('danoni-5:',sm);self.assertIn('#BPMS:0=75;',sm)
        body=sm.split('0,0,0,0,0:\n',1)[1].split(';',1)[0]
        notes=[]
        for measure,section in enumerate(body.split(',')):
            rows=section.strip().splitlines();self.assertEqual(len(rows),192)
            notes += [measure*192+i for i,row in enumerate(rows) if row != '00000']
            self.assertTrue(all(len(row)==5 for row in rows))
        self.assertEqual(notes,[298,418])

    def test_menu_and_hidden_chart_use_only_original_input(self):
        configs=[config_for(s) for s in SELECTIONS]
        self.assertTrue(all(len(c['captureFrames'])<=16 and max(c['captureFrames'])<=600 for c in configs))
        hard=configs[1]
        self.assertIn({'frame':120,'type':'click','x':165,'y':169},hard['events'])
        self.assertTrue(all(set(c)<= {'movie','sourceUrl','pageUrl','captureFrames','events','render','mixAudio','inspectGlobals','resources'} for c in configs))


if __name__=='__main__':unittest.main()
