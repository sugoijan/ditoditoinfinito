"""Original audio/chart scenarios for earliest meaningful preview selection."""
import hashlib
import math
from pathlib import Path
import struct
import tempfile
import unittest
import wave
from preview_policy import choose_preview,chart_heads


def sound(path,amplitude,duration=20,stereo=False):
    with wave.open(str(path),'wb') as wav:
        wav.setnchannels(2 if stereo else 1);wav.setsampwidth(2);wav.setframerate(8000)
        for block in range(math.ceil(duration*10)):
            values=[]
            for i in range(min(800,round(duration*8000)-block*800)):
                frame=block*800+i;t=frame/8000
                v=round(amplitude(t)*math.sin(2*math.pi*440*t))
                values.append(struct.pack('<hh',v,-v) if stereo else struct.pack('<h',v))
            wav.writeframes(b''.join(values))


def chart(ident,times):
    return {'id':ident,'lanes':[{'tapsSeconds':times,'holdsSeconds':[]}]}


class Pickup(unittest.TestCase):
    def test_nonzero_quiet_intro_and_early_isolated_notes_are_skipped(self):
        with tempfile.TemporaryDirectory(prefix='ddi-preview-test-') as d:
            p=Path(d)/'audio.wav';sound(p,lambda t:100 if t<8 else 1600)
            before=hashlib.sha256(p.read_bytes()).hexdigest()
            r=choose_preview(p,charts=[chart('single',[2,5]+[8+i*.5 for i in range(20)])])
            self.assertEqual(r['anchorSeconds'],8);self.assertAlmostEqual(r['startSeconds'],7.92)
            self.assertEqual(r['status'],'selected');self.assertGreater(r['quietCandidates'],0)
            self.assertEqual(before,hashlib.sha256(p.read_bytes()).hexdigest())

    def test_loud_music_does_not_make_an_isolated_note_a_good_start(self):
        with tempfile.TemporaryDirectory(prefix='ddi-preview-test-') as d:
            p=Path(d)/'audio.wav';sound(p,lambda t:0 if t<2 else 1600)
            r=choose_preview(p,charts=[chart('single',[2]+[7+i*.5 for i in range(20)])])
            self.assertEqual(r['anchorSeconds'],7);self.assertGreater(r['sparseCandidates'],0)
            self.assertEqual(r['firstNoteSeconds'],2)

    def test_loud_transient_in_quiet_busy_intro_is_not_sustained_music(self):
        with tempfile.TemporaryDirectory(prefix='ddi-preview-test-') as d:
            p=Path(d)/'audio.wav';sound(p,lambda t:12000 if 2<=t<2.1 else (100 if t<8 else 1600))
            r=choose_preview(p,charts=[chart('single',[2+i*.5 for i in range(8)]+[8+i*.5 for i in range(20)])])
            self.assertEqual(r['anchorSeconds'],8)
            rejected=r['firstSkippedCandidates'][0]
            self.assertGreater(rejected['audio']['startRms'],r['thresholdRms'])
            self.assertFalse(rejected['audio']['supported'])

    def test_chart_consensus_does_not_let_a_dense_hard_intro_override_sparse_normal(self):
        with tempfile.TemporaryDirectory(prefix='ddi-preview-test-') as d:
            p=Path(d)/'audio.wav';sound(p,lambda t:0 if t<2 else 1600)
            charts=[chart('normal',[2]+[8+i*.5 for i in range(20)]),chart('hard',[2+i*.25 for i in range(60)])]
            r=choose_preview(p,charts=charts)
            self.assertEqual(r['requiredChartSupport'],2)
            self.assertEqual(r['anchorSeconds'],7)
            self.assertLessEqual(r['anchorSeconds'],8)
            self.assertGreater(r['sparseCandidates'],0)

    def test_chords_count_once_and_hold_tails_are_not_new_notes(self):
        lanes=[{'tapsSeconds':[2],'holdsSeconds':[[1,10]]} for _ in range(9)]
        self.assertEqual(chart_heads([{'id':'nine','lanes':lanes}],20),[('nine',[1,2])])
        with tempfile.TemporaryDirectory(prefix='ddi-preview-test-') as d:
            p=Path(d)/'audio.wav';sound(p,lambda t:0 if t<2 else 1600)
            r=choose_preview(p,charts=[{'id':'nine','lanes':lanes}])
            self.assertEqual(r['status'],'audio-note-fallback')
            self.assertEqual(r['informativeChartIds'],[])
            self.assertEqual(r['anchorSeconds'],2)

    def test_short_track_uses_late_pickup_and_shorter_available_preview(self):
        with tempfile.TemporaryDirectory(prefix='ddi-preview-test-') as d:
            p=Path(d)/'audio.wav';sound(p,lambda t:100 if t<12 else 1600,duration=15)
            r=choose_preview(p,charts=[chart('single',[12,12.5,13,13.5,14])])
            self.assertAlmostEqual(r['startSeconds'],11.92);self.assertAlmostEqual(r['lengthSeconds'],3.08)
            self.assertEqual(r['status'],'selected')

    def test_stereo_without_chart_and_silent_audio_have_explicit_fallbacks(self):
        with tempfile.TemporaryDirectory(prefix='ddi-preview-test-') as d:
            p=Path(d)/'audio.wav';sound(p,lambda t:100 if t<6 else 1600,stereo=True)
            r=choose_preview(p);self.assertEqual(r['status'],'audio-only-fallback');self.assertEqual(r['anchorSeconds'],6)
            p=Path(d)/'silent.wav';sound(p,lambda t:0)
            r=choose_preview(p,charts=[chart('single',[1,1.5,2,2.5])])
            self.assertEqual(r['status'],'no-activity');self.assertEqual(r['startSeconds'],0)
            for options in [{'quiet_ratio':float('nan')},{'min_notes':True},{'lead_in':2},{'max_gap':0}]:
                with self.assertRaises(ValueError):choose_preview(p,**options)


if __name__=='__main__':unittest.main()
