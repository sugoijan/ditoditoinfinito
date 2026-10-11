"""Synthetic independent author clocks, weak charts and bounded shared exports."""
import json
from pathlib import Path
import random
import shutil
import tempfile
import unittest
from unittest.mock import patch

import numpy as np

from automatic_tempo import audio_pulse,select_grid
from export_io import digest
from musical_grid import quantize_lanes,validate_grid
from prepare_pack import prepare
from shared_tempo import select_shared_grids
from test_pack_policy import audio


class Shared(unittest.TestCase):
    def track(self,charts,bpm):
        estimates=[{'bpm':bpm,'beatTimesSeconds':[.1+i*60/bpm for i in range(200)]}]
        track={'distinctTimesSeconds':sorted(set(t for values in charts.values()for t in values)),
               'audioEstimates':estimates,'chartCandidates':[]}
        return track,[audio_pulse(estimates[0])],select_grid(track)

    def test_independent_phases_and_small_author_drift_recover_rhythmic_heads(self):
        bpms={'first':168.45,'second':168.6}
        phases={'first':.137,'second':.172}
        charts={ident:np.round((phases[ident]+np.arange(300)*60/bpm/4)*60)/60 for ident,bpm in bpms.items()}
        track,pulses,legacy=self.track(charts,168.5)
        grids,evidence=select_shared_grids(track,charts,pulses,legacy)
        self.assertEqual(evidence['status'],'independent-chart-consensus')
        self.assertEqual(set(evidence['supportingCharts']),set(charts))
        self.assertAlmostEqual(evidence['consensusBpm'],168.525,delta=.04)
        self.assertNotEqual(grids['first']['beatZeroSeconds'],grids['second']['beatZeroSeconds'])
        for ident,times in charts.items():
            grid=grids[ident];self.assertAlmostEqual(grid['bpm'],bpms[ident],delta=.04)
            self.assertEqual(grid['estimationMethod'],'independent-chart-phase')
            lane={'tapsSeconds':times.tolist(),'holdsSeconds':[]}
            _,endpoints=quantize_lanes({'lanes':[lane]},grid)
            self.assertGreater(sum(e['method']=='rhythmic-subdivision'for e in endpoints),.95*len(times))
            self.assertTrue(all(abs(e['shiftMs'])<=9 for e in endpoints))

    def test_a_noisy_chart_cannot_set_the_cohort_phase(self):
        charts={ident:np.round((phase+np.arange(220)*60/174/4)*60)/60 for ident,phase in [('a',.1),('b',.13)]}
        rng=random.Random(17);charts['noise']=sorted(rng.uniform(0,40)for _ in range(300))
        track,pulses,legacy=self.track(charts,174)
        grids,evidence=select_shared_grids(track,charts,pulses,legacy)
        self.assertEqual(set(evidence['supportingCharts']),{'a','b'})
        self.assertEqual(grids['noise']['estimationMethod'],'shared-tempo-with-fine-note-fallback')
        self.assertAlmostEqual(grids['noise']['bpm'],evidence['consensusBpm'])

    def test_one_supported_chart_is_not_a_majority(self):
        charts={'a':np.round((.1+np.arange(150)*60/174/4)*60)/60,
                'b':np.round((.17+np.arange(150)*60/220/4)*60)/60}
        track,pulses,legacy=self.track(charts,174)
        grids,evidence=select_shared_grids(track,charts,pulses,legacy)
        self.assertEqual(evidence['status'],'no-majority-chart-consensus')
        self.assertEqual(grids,{'a':legacy,'b':legacy})

    def test_consensus_bound_rechecks_the_supporting_chart_list(self):
        charts={ident:np.arange(300,dtype=float)+index for index,ident in enumerate(['edge','a','b'])}
        grid=validate_grid({'bpm':100,'beatZeroSeconds':0,'subdivisionsPerBeat':2})
        pulse={'bpm':100,'beatZeroSeconds':0}
        def fake_refine(times,bpm,sub):
            edge=times[0]==0
            clipped=edge and abs(bpm-100)>.01
            value=100 if clipped else 99.85 if edge else 100.15
            return {'bpm':value,'subdivisionsPerBeat':sub,'coherence':.2 if clipped else .99,
                    'p95GridResidualMs':50 if clipped else 1,'gridPhaseSeconds':0,
                    'gridPeriodSeconds':60/value/sub,'withinHalfSourceFrameFraction':.1 if clipped else .99}
        with patch('shared_tempo.cohort_candidates',return_value=[{'bpm':100,'subdivisionsPerBeat':2}]),patch('shared_tempo.refine',side_effect=fake_refine):
            grids,evidence=select_shared_grids({},charts,[pulse],grid)
        self.assertEqual(evidence['supportingCharts'],['a','b'])
        self.assertEqual(grids['edge']['estimationMethod'],'shared-tempo-with-fine-note-fallback')

    def test_sparse_and_nonfinite_charts_do_not_invent_consensus(self):
        legacy=validate_grid({'bpm':160,'beatZeroSeconds':0,'subdivisionsPerBeat':2})
        grids,evidence=select_shared_grids({}, {'a':[0,1],'b':[2,3]}, [],legacy)
        self.assertEqual(evidence['status'],'insufficient-independent-charts')
        self.assertEqual(grids['a'],legacy)
        with self.assertRaises(ValueError):select_shared_grids({}, {'a':[float('nan')]}, [],legacy)

    def test_shared_audio_serializes_chart_timing_and_matching_native_scroll(self):
        with tempfile.TemporaryDirectory(prefix='ddi-shared-grid-')as directory:
            root=Path(directory);source=root/'source';source.mkdir();audio(source/'sound.wav')
            lanes=[{'tapsSeconds':[],'holdsSeconds':[]}for _ in range(5)]
            lanes[0]['tapsSeconds']=[1,2,3];lanes[1]['holdsSeconds']=[[4,5]]
            charts=[{'id':ident,'mode':'5','difficultyStars':4,'lanes':lanes,'speed':[]}for ident in ['a','b']]
            manifest={'schema':'ddi-normalized-work/experimental-1','music':{'path':'sound.wav','sha256':digest((source/'sound.wav').read_bytes())},'charts':charts}
            raw=json.dumps(manifest).encode();(source/'manifest.json').write_bytes(raw)
            grids={'a':validate_grid({'bpm':160,'beatZeroSeconds':.1,'subdivisionsPerBeat':2}),
                   'b':validate_grid({'bpm':160.1,'beatZeroSeconds':.07,'subdivisionsPerBeat':4})}
            review={'schema':'ddi-reviewed-musical-grid/experimental-1','sourceManifestSha256':digest(raw),'charts':grids}
            (root/'grid.json').write_text(json.dumps(review))
            motion={'schema':'ddi-reviewed-native-scroll/experimental-1','sourceManifestSha256':digest(raw),
                    'charts':{ident:{'baseArrowHeightsPerSecond':3,'scope':'original synthetic movement'}for ident in grids}}
            (root/'motion.json').write_text(json.dumps(motion))
            pack=prepare(source,root/'pack','wav',Path(shutil.which('ffmpeg')),musical_grid=root/'grid.json',native_scroll=root/'motion.json')
            self.assertEqual(len(pack['songs']),1)
            ssc=(root/'pack'/pack['songs'][0]['simfile']).read_text()
            blocks=ssc.split('#NOTEDATA:;')[1:];self.assertEqual(len(blocks),2)
            for row,block in zip(pack['songs'][0]['charts'],blocks):
                bpm=grids[row['id']]['bpm'];self.assertIn(f'#BPMS:0={bpm:.15g};',block)
                self.assertAlmostEqual(row['nativeScroll']['sscBaseRatio']*bpm/60,3)
                self.assertLessEqual(row['musicalQuantization']['maximumNoteErrorMs'],9)
            self.assertEqual((source/'manifest.json').read_bytes(),raw)


if __name__=='__main__':unittest.main()
