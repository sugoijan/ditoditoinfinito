"""Original synthetic motion and authored-base scroll compensation checks."""
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from measure_motion import measure
from scroll_policy import base_ratio
from prepare_pack import prepare,ssc_chart
from test_pack_policy import audio


def name(value):return {'type':'string','utf16':[ord(c) for c in value],'truncated':False}


def capture():
    frames=[]
    for iteration in [10,20,30]:
        # Display indices change; only scoped clip paths match between frames.
        ref_id,note_id=(1,2) if iteration!=20 else (2,1)
        tree=[{'id':0,'parent':None,'name':name('root'),'depth':0,'kind':'movie-clip','visible':True,'stageBoundsPixels':[0,0,400,400]},
              {'id':ref_id,'parent':0,'name':name('receptor'),'depth':1,'kind':'movie-clip','visible':True,'stageBoundsPixels':[0,0,40,40]},
              {'id':note_id,'parent':0,'name':name('note1'),'depth':2,'kind':'movie-clip','visible':True,
               'stageBoundsPixels':[0,400-2*iteration,40,440-2*iteration]}]
        frames.append({'frame':iteration,'state':{'incomplete':False,'avm1Halted':False,'displayTree':tree}})
    return {'avm1Halted':False,'audit':{'truncated':False,'runtimeErrors':0,'droppedEntries':0,'truncatedDetails':0,'entries':[]},
        'source':{'movieSha256':'0'*64},'captures':frames}


class Scroll(unittest.TestCase):
    def test_measurement_uses_scoped_paths_and_explicit_units(self):
        report=measure(capture(),60,r'note[0-9]+','receptor')
        self.assertEqual(report['signedPixelsPerFrame'],-2)
        self.assertEqual(report['pixelsPerSecond'],120)
        self.assertEqual(report['referencePixels'],40)
        self.assertEqual(report['baseArrowHeightsPerSecond'],3)
        self.assertEqual(len(report['samples']),2)
        c=capture();c['captures'][1]['state']['incomplete']=True
        with self.assertRaises(ValueError):measure(c,60,r'note[0-9]+','receptor')
        c=capture();c['captures'][1]['state']['displayTree'][-1]['stageBoundsPixels'][3]+=1
        with self.assertRaises(ValueError):measure(c,60,r'note[0-9]+','receptor')

    def test_only_declared_cached_requests_are_accepted(self):
        c=capture();url='https://example.invalid/chart.txt'
        c['source']['config']={'resources':[{'url':url}]}
        c['audit']['entries']=[{'kind':'cache-hit','detail':url}]
        self.assertEqual(measure(c,60,r'note[0-9]+','receptor')['pixelsPerSecond'],120)
        for entry in [{'kind':'cache-hit','detail':url+'?other'}, {'kind':'network-denied','detail':url}]:
            c['audit']['entries']=[entry]
            with self.assertRaises(ValueError):measure(c,60,r'note[0-9]+','receptor')

    def test_authored_baseline_is_independent_of_bpm_and_user_multiplier(self):
        motion={'baseArrowHeightsPerSecond':3}
        for bpm in [75,110,160,200]:
            ratio=base_ratio(motion,bpm)
            for user_multiplier in [1,2,3]:
                self.assertAlmostEqual(bpm/60*ratio*user_multiplier,3*user_multiplier)
            self.assertAlmostEqual(bpm/60*base_ratio(motion,bpm,1.25),3.75)
        for rate in [0,-1,float('nan'),True]:
            with self.assertRaises(ValueError):base_ratio({'baseArrowHeightsPerSecond':rate},200)
        with self.assertRaises(ValueError):base_ratio(None,200,1.2)

    def test_compensation_multiplies_author_changes_without_changing_notes(self):
        lanes=[{'tapsSeconds':[1,3],'holdsSeconds':[]}]+[{'tapsSeconds':[],'holdsSeconds':[]} for _ in range(4)]
        chart={'mode':'5','lanes':lanes,'speed':[{'seconds':2,'multiplier':2},{'seconds':3,'multiplier':.5}]}
        grid={'bpm':200,'beatZeroSeconds':0,'subdivisionsPerBeat':2,'maximumErrorMs':9}
        old=ssc_chart(chart,'Easy',3,grid);new=ssc_chart(chart,'Easy',3,grid,{'baseArrowHeightsPerSecond':3})
        self.assertEqual(old.split('#NOTES:',1)[1],new.split('#NOTES:',1)[1])
        points=new.split('#SCROLLS:',1)[1].split(';')[0].split(',')
        self.assertEqual([float(p.split('=')[1]) for p in points],[.9,1.8,.45])
        self.assertNotIn('#SPEEDS:',new)

    def test_positive_beat_phase_pads_whole_measures_without_moving_notes(self):
        grid={'bpm':120,'beatZeroSeconds':.1,'subdivisionsPerBeat':2,'maximumErrorMs':9}
        lanes=[{'tapsSeconds':[1],'holdsSeconds':[]}]+[{'tapsSeconds':[],'holdsSeconds':[]} for _ in range(4)]
        chart={'mode':'5','lanes':lanes}
        old=ssc_chart(chart,'Easy',3,grid);new=ssc_chart(chart,'Easy',3,grid,{'baseArrowHeightsPerSecond':3})
        def info(ssc):
            offset=float(ssc.split('#OFFSET:',1)[1].split(';')[0])
            rows=ssc.split('#NOTES:',1)[1].split(';')[0].replace(',','').splitlines()
            rows=[r for r in rows if r]
            tick=next(i for i,r in enumerate(rows) if r=='10000')
            return offset,tick
        a,t=info(old);b,u=info(new)
        self.assertEqual(u-t,192);self.assertAlmostEqual(t/96-a,u/96-b)
        self.assertLessEqual(-b,0);self.assertEqual(u%48,t%48)

    def test_source_bound_generic_review_and_adjustable_factor(self):
        with tempfile.TemporaryDirectory(prefix='ddi-scroll-policy-') as directory:
            root=Path(directory);source=root/'source';source.mkdir();audio(source/'sound.wav')
            lanes=[{'tapsSeconds':[1,2],'holdsSeconds':[]}]+[{'tapsSeconds':[],'holdsSeconds':[]} for _ in range(8)]
            manifest={'schema':'ddi-normalized-work/experimental-1','music':{'path':'sound.wav',
                'sha256':hashlib.sha256((source/'sound.wav').read_bytes()).hexdigest()},'charts':[{'id':'nine','mode':'9B','lanes':lanes}]}
            (source/'manifest.json').write_text(json.dumps(manifest))
            review={'schema':'ddi-reviewed-native-scroll/experimental-1',
                'sourceManifestSha256':hashlib.sha256((source/'manifest.json').read_bytes()).hexdigest(),
                'charts':{'nine':{'baseArrowHeightsPerSecond':3,'evidence':'Original synthetic measurement'}}}
            measured=measure(capture(),60,r'note[0-9]+','receptor')
            (root/'motion.json').write_text(json.dumps(measured))
            review['charts']['nine']['evidence']={'path':'motion.json','sha256':hashlib.sha256((root/'motion.json').read_bytes()).hexdigest()}
            (root/'review.json').write_text(json.dumps(review))
            result=prepare(source,root/'pack','wav',Path(shutil.which('ffmpeg')),native_scroll=root/'review.json',scroll_factor=1.25)
            report=result['songs'][0]['charts'][0]['nativeScroll']
            self.assertEqual(report['baseArrowHeightsPerSecond'],3.75)
            self.assertEqual(report['sscBaseRatio'],3)
            self.assertFalse(report['playerMultiplierModified'])
            self.assertFalse(result['noteTimingModified'])
            self.assertIn('#SCROLLS:0=3;',(root/'pack'/result['songs'][0]['simfile']).read_text())
            (root/'motion.json').write_text('{}')
            with self.assertRaises(ValueError):prepare(source,root/'changed','wav',Path(shutil.which('ffmpeg')),native_scroll=root/'review.json')
            review['sourceManifestSha256']='0'*64;(root/'review.json').write_text(json.dumps(review))
            with self.assertRaises(ValueError):prepare(source,root/'bad','wav',Path(shutil.which('ffmpeg')),native_scroll=root/'review.json')


if __name__=='__main__':unittest.main()
