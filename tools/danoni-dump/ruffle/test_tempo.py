"""Known synthetic clocks test tempo/phase fitting and its ambiguities."""
import unittest
import numpy as np
from analyze_tempo import chart_fit, chart_candidates


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

    def test_insufficient_and_nonfinite_data_is_rejected(self):
        for times in [[0,1,2],np.r_[np.arange(8),np.nan]]:
            with self.assertRaises(ValueError):chart_candidates(times)


if __name__=='__main__':unittest.main()
