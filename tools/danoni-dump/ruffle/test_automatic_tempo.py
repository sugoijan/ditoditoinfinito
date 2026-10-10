"""Generic automatic tempo regressions; no acquired music or work identities."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from automatic_tempo import audio_pulse,automatic_review,select_grid
from musical_grid import validate_grid
from export_io import digest


class Automatic(unittest.TestCase):
    def test_missing_beats_do_not_change_refined_tempo(self):
        period=60/155
        times=[.17+i*period for i in range(200)if i%7!=3]
        p=audio_pulse({'bpm':154.7,'beatTimesSeconds':times})
        self.assertAlmostEqual(p['bpm'],155,places=8)
        self.assertAlmostEqual(p['beatZeroSeconds'],.17,places=8)
        self.assertLess(p['p95BeatResidualMs'],1e-8)
        self.assertIsNone(audio_pulse({'bpm':155,'beatTimesSeconds':times[:7]}))

    def test_authored_range_selects_a_supported_pulse_alias(self):
        times=[round((.1+i*.125)*60)/60 for i in range(300)]
        pulses=[{'bpm':b,'beatTimesSeconds':[.1+i*60/b for i in range(100)]}for b in [120,240]]
        track={'distinctTimesSeconds':times,'audioEstimates':pulses,'chartCandidates':[]}
        self.assertAlmostEqual(select_grid(track)['bpm'],120)
        track['authoredBpmRange']={'minimumBpm':168,'maximumBpm':325}
        self.assertAlmostEqual(select_grid(track)['bpm'],240)

    def test_unrepresentable_estimate_records_exact_fallback_in_pack_and_cache(self):
        grid=validate_grid({'bpm':60,'beatZeroSeconds':1/96,'subdivisionsPerBeat':1})
        entries=[{'id':'original','metadata':{},'chart':{'lanes':[{'tapsSeconds':[0,1,2],'holdsSeconds':[]}]}}]
        with tempfile.TemporaryDirectory(prefix='ddi-auto-fallback-') as directory:
            root=Path(directory);raw=b'original synthetic manifest'
            def analyze(source,path):
                report={'sourceManifestSha256':digest(raw),'tracks':[{'chartIds':['original']}]}
                path.write_text(json.dumps(report));return report
            with patch('analyze_tempo.analyze',side_effect=analyze),patch('automatic_tempo.select_grid',return_value=grid):
                for name in ['first','cached']:
                    (root/name).mkdir()
                    self.assertIsNone(automatic_review(root,raw,entries,root/name,root/'cache'))
                    report=json.loads((root/name/'automatic-tempo/insufficient.json').read_text())
                    self.assertEqual(report['sourceManifestSha256'],digest(raw))
                    self.assertEqual(report['fallback'],'exact source frame clock')

    def test_decision_cache_reuses_analysis_and_rechecks_current_notes(self):
        grid=validate_grid({'bpm':120,'beatZeroSeconds':0,'subdivisionsPerBeat':2})
        chart={'lanes':[{'tapsSeconds':[1,2,3],'holdsSeconds':[[4,5]]}]}
        entries=[{'id':'original','chart':chart,'metadata':{}}]
        with tempfile.TemporaryDirectory(prefix='ddi-auto-tempo-') as directory:
            root=Path(directory);raw=b'original synthetic manifest'
            report={'sourceManifestSha256':digest(raw),'tracks':[{'chartIds':['original']}]}
            def analyze(source,path):
                path.write_text(json.dumps(report));return report
            with patch('analyze_tempo.analyze',side_effect=analyze)as analyzer,patch('automatic_tempo.select_grid',return_value=grid)as selector:
                (root/'one').mkdir()
                result=automatic_review(root,raw,entries,root/'one',root/'cache')
                (root/'two').mkdir()
                cached=automatic_review(root,raw,entries,root/'two',root/'cache')
                self.assertEqual(result,cached)
                self.assertEqual(analyzer.call_count,1);self.assertEqual(selector.call_count,1)
                chart['lanes'][0]['tapsSeconds']=[1,1]
                (root/'three').mkdir()
                with self.assertRaises(ValueError):automatic_review(root,raw,entries,root/'three',root/'cache')


if __name__=='__main__':unittest.main()
