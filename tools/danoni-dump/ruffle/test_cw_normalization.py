"""Synthetic captured-state checks for CW clock and statically proven movement."""
import json
from pathlib import Path
import tempfile
import unittest

from export_io import digest
from normalize_cw import relative_frames,movement_units


def reference():
    return {'fps':60,'timing':{'playbackRate':1,'userAdjustment':0,'decimalAdjustment':0,
        'headerAdjustment':2.7,'intAdjustment':7,'preblankFrame':5,'blankFrame':200},
        'lanes':[{'chara':'left','taps':[205,265],'holdEndpoints':[325,385],'holds':[[325,385]]}],
        'speed':[{'frame':265,'multiplier':2}],'boost':[]}


class Clock(unittest.TestCase):
    def test_startup_padding_is_removed_once_and_author_adjustment_is_retained(self):
        origin,lanes,events=relative_frames(reference())
        self.assertEqual(origin,205)
        self.assertEqual(lanes[0]['tapsSeconds'],[0,1])
        self.assertEqual(lanes[0]['holdsSeconds'],[[2,3]])
        self.assertEqual(events['speed'][0]['seconds'],1)
        self.assertEqual(lanes[0]['nativeFrames'],[205,265])

    def test_unproved_clock_and_corrupt_hold_projection_are_rejected(self):
        for field,value in [('playbackRate',2),('userAdjustment',1),('decimalAdjustment',.1),('intAdjustment',2)]:
            r=reference();r['timing'][field]=value
            with self.assertRaises(ValueError):relative_frames(r)
        for change in [{'holdEndpoints':[325]},{'holds':[[325,386]]},{'dummyTaps':[205]},{'taps':[205.5]}]:
            r=reference();r['lanes'][0].update(change)
            with self.assertRaises(ValueError):relative_frames(r)

    def test_engine_evidence_binds_raw_bytes_and_browser_text_with_bom_crlf(self):
        script=('const C_ARW_WIDTH = 50;\nconst setSpeedOnFrame = () => {\n'
                'currentSpeed = g_stateObj.speed * g_headerObj.baseSpeed * 2;\n'
                'Number(_speedData[s + 1]) * g_stateObj.speed * g_headerObj.baseSpeed * 2;\n};')
        with tempfile.TemporaryDirectory(prefix='ddi-cw-unit-') as directory:
            root=Path(directory)
            body=b'\xef\xbb\xbf'+script.replace('\n','\r\n').encode();(root/'engine.js').write_bytes(body)
            (root/'resources.json').write_text(json.dumps({'resources':[{'url':'https://example.invalid/engine.js',
                'sha256':digest(body),'status':200,'bodyPath':'engine.js'}]}))
            for observed in [script,script.replace('\n','\r\n')]:
                r={'engineScriptHashes':[{'url':'https://example.invalid/engine.js','sha256':digest(observed.encode())}]}
                proof=movement_units(root,r)
                self.assertEqual((proof['arrowPixels'],proof['pixelsPerFramePerSpeed']),(50,2))
            (root/'engine.js').write_bytes(body+b' ')
            with self.assertRaises(ValueError):movement_units(root,r)


if __name__=='__main__':unittest.main()
