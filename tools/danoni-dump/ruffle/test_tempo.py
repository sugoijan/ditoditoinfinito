"""Known synthetic clocks test tempo/phase fitting and its ambiguities."""
import unittest
import numpy as np
from analyze_tempo import chart_fit, chart_candidates, dominant_grid


class Grids(unittest.TestCase):
    def test_frame_rounded_tempo_and_phase_are_recovered(self):
        bpm=173.4;phase=.137;period=60/bpm/4
        exact=phase+np.arange(180)*period
        rounded=np.round(exact*60)/60
        candidates=chart_candidates(rounded)
        found=min((c for c in candidates if c['subdivisionsPerBeat']==4),key=lambda c:abs(c['bpm']-bpm))
        self.assertLess(abs(found['bpm']-bpm),.1)
        self.assertLess(found['p95GridResidualMs'],9)
        delta=(found['gridPhaseSeconds']-phase+period/2)%period-period/2
        self.assertLess(abs(delta),.002)
        self.assertLess(chart_fit(rounded,75,4)['coherence'],.2)

    def test_subdivision_aliases_are_indistinguishable(self):
        times=np.arange(100)*60/160/2+.1
        a=chart_fit(times,160,2);b=chart_fit(times,80,4);c=chart_fit(times,320/3,3)
        self.assertAlmostEqual(a['coherence'],b['coherence'])
        self.assertAlmostEqual(a['coherence'],c['coherence'])
        self.assertAlmostEqual(a['gridPhaseSeconds'],b['gridPhaseSeconds'])

    def test_tempo_changes_defeat_a_global_grid(self):
        first=np.arange(80)*60/160/2+.1
        second=20+np.arange(80)*60/168/2+.1
        self.assertGreater(chart_fit(first,160,2)['coherence'],.999)
        self.assertGreater(chart_fit(second,168,2)['coherence'],.999)
        self.assertLess(chart_fit(np.r_[first,second],160,2)['coherence'],.6)

    def test_dominant_fit_reports_excluded_faster_portions(self):
        times=[];windows=[]
        for index,bpm in enumerate([160,160,160,175]):
            period=60/bpm/2
            first=max(0,int(np.ceil((index*20-.1)/period)))
            subset=.1+(first+np.arange(40))*period
            times.extend(subset.tolist())
            windows.append({'startSeconds':index*20,'endSeconds':index*20+20,'notes':len(subset),**chart_fit(subset,bpm,2)})
        result=dominant_grid(times,windows,.2)
        self.assertEqual(result['inlierWindowIndices'],[0,1,2])
        self.assertEqual(result['excludedWindowIndices'],[3])
        self.assertEqual(result['retainedNotes'],120)
        self.assertEqual(result['totalNotes'],160)
        self.assertLess(abs(result['fit']['bpm']-160),.01)
        self.assertFalse(result['automaticAcceptance'])

    def test_equal_tempo_with_phase_jumps_is_not_one_verified_grid(self):
        times=[];windows=[]
        for index in range(3):
            subset=index*20+.1+np.arange(40)*60/160/2
            times.extend(subset.tolist())
            windows.append({'startSeconds':index*20,'endSeconds':index*20+20,'notes':40,**chart_fit(subset,160,2)})
        result=dominant_grid(times,windows,.2)
        self.assertEqual(result['status'],'tempo-consensus-phase-unresolved')
        self.assertEqual(result['tempoConsensusBpm'],160)
        self.assertFalse(result['phaseConsistent'])

    def test_insufficient_and_nonfinite_data_is_rejected(self):
        for times in [[0,1,2],np.r_[np.arange(8),np.nan]]:
            with self.assertRaises(ValueError):chart_candidates(times)


if __name__=='__main__':unittest.main()
