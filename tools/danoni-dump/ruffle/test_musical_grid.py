"""Synthetic evidence for bounded, opt-in musical timing and fallback behavior."""
import json
import hashlib
from pathlib import Path
import shutil
import tempfile
import unittest

from musical_grid import validate_grid,position,summarize
from prepare_pack import prepare,ssc_chart
from test_pack_policy import audio


class Musical(unittest.TestCase):
    def test_frame_rounding_snaps_to_intended_subdivisions(self):
        grid=validate_grid({'bpm':173.4,'beatZeroSeconds':.137,'subdivisionsPerBeat':2})
        for n in range(100):
            intended=.137+n*60/173.4/2
            source=round(intended*60)/60
            tick,error,snapped=position(source,grid)
            self.assertTrue(snapped);self.assertEqual(tick,n*24)
            self.assertLessEqual(abs(error)*1000,9)
            self.assertAlmostEqual(source+error,intended)

    def test_outlier_uses_fine_tick_without_forcing_rhythmic_grid(self):
        grid=validate_grid({'bpm':110,'beatZeroSeconds':.1,'subdivisionsPerBeat':2})
        tick,error,snapped=position(.23,grid)
        self.assertFalse(snapped);self.assertLessEqual(abs(error)*1000,60/110/48/2*1000)
        narrow={**grid,'maximumErrorMs':.01}
        with self.assertRaises(ValueError):position(.23,narrow)

    def test_negative_pre_roll_padding_preserves_whole_beats(self):
        grid=validate_grid({'bpm':120,'beatZeroSeconds':0,'subdivisionsPerBeat':2})
        lanes=[{'tapsSeconds':[],'holdsSeconds':[]} for _ in range(5)]
        lanes[0]['tapsSeconds']=[-.25,.25]
        ssc=ssc_chart({'mode':'5','lanes':lanes},'Easy',3,grid)
        self.assertIn('#OFFSET:0.5;',ssc)
        self.assertIn('#BPMS:0=120;',ssc)
        rows=ssc.split('#NOTES:',1)[1].split(';')[0].strip().splitlines()
        self.assertEqual([i for i,row in enumerate(rows) if row=='10000'],[24,72])

    def test_invalid_grid_rejected_and_short_hold_uses_fine_fallback(self):
        grid={'bpm':180,'beatZeroSeconds':0,'subdivisionsPerBeat':2}
        for key,value in [('bpm',float('nan')),('beatZeroSeconds',float('inf')),('bpm',0),
                          ('subdivisionsPerBeat',5),('maximumErrorMs',10),('maximumErrorMs',0)]:
            with self.assertRaises(ValueError):validate_grid({**grid,key:value})
        grid=validate_grid({'bpm':180,'beatZeroSeconds':1/120,'subdivisionsPerBeat':2})
        lanes=[{'tapsSeconds':[],'holdsSeconds':[]} for _ in range(5)]
        lanes[0]['holdsSeconds']=[[0,1/60]]
        text=ssc_chart({'mode':'5','lanes':lanes},'Easy',3,grid)
        self.assertIn('2',text);self.assertIn('3',text)
        report=summarize({'lanes':lanes},grid)
        self.assertTrue(all(e['method']=='lane-collision-fine-fallback'for e in report['endpoints']))

    def test_source_bound_opt_in_pack_reports_every_shift(self):
        with tempfile.TemporaryDirectory(prefix='ddi-musical-grid-') as directory:
            root=Path(directory);source=root/'source';source.mkdir();audio(source/'sound.wav')
            lanes=[{'tapsSeconds':[],'holdsSeconds':[]} for _ in range(7)]
            lanes[0]['tapsSeconds']=[1,2,3];lanes[1]['holdsSeconds']=[[4,5]]
            chart={'id':'original','mode':'7','difficultyStars':4,'lanes':lanes,'speed':[{'seconds':2,'multiplier':1.5}]}
            manifest={'schema':'ddi-normalized-work/experimental-1','music':{'path':'sound.wav',
                'sha256':hashlib.sha256((source/'sound.wav').read_bytes()).hexdigest()},'charts':[chart]}
            (source/'manifest.json').write_text(json.dumps(manifest))
            grid=validate_grid({'bpm':160,'beatZeroSeconds':.1,'subdivisionsPerBeat':2})
            review={'schema':'ddi-reviewed-musical-grid/experimental-1',
                'sourceManifestSha256':hashlib.sha256((source/'manifest.json').read_bytes()).hexdigest(),'charts':{'original':grid}}
            (root/'review.json').write_text(json.dumps(review))
            result=prepare(source,root/'output','wav',Path(shutil.which('ffmpeg')),musical_grid=root/'review.json')
            self.assertTrue(result['noteTimingModified']);self.assertIsNone(result['timingBpm'])
            song=result['songs'][0];self.assertEqual(song['timingBpm'],160)
            report=song['charts'][0]['musicalQuantization']
            self.assertEqual(report,summarize(chart,grid));self.assertEqual(report['noteEndpoints'],5)
            self.assertEqual(len(report['endpoints']),5)
            self.assertEqual({e['kind'] for e in report['endpoints']},{'tap','hold-head','hold-tail'})
            self.assertLessEqual(report['maximumNoteErrorMs'],9)
            self.assertLessEqual(report['maximumScrollEventErrorMs'],9)
            ssc=(root/'output'/song['simfile']).read_text()
            self.assertIn('#BPMS:0=160;',ssc);self.assertNotIn('#BPMS:0=75;',ssc)
            self.assertEqual(json.loads((source/'manifest.json').read_text()),manifest)
            review['sourceManifestSha256']='0'*64;(root/'review.json').write_text(json.dumps(review))
            with self.assertRaises(ValueError):prepare(source,root/'bad','wav',Path(shutil.which('ffmpeg')),musical_grid=root/'review.json')


if __name__=='__main__':unittest.main()
