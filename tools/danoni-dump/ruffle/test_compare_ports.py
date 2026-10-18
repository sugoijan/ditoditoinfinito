"""Synthetic independent chart comparison cases, no acquired work data."""
import json
from pathlib import Path
import tempfile
import unittest
from compare_ports import compare


class Ports(unittest.TestCase):
    def test_adjustment_is_removed_but_note_edits_are_not_hidden(self):
        with tempfile.TemporaryDirectory()as tmp:
            root=Path(tmp);n=root/'native.json';c=root/'manifest.json';r=root/'reference.json'
            native={'schema':'ddi-normalized-work/experimental-1','charts':[{'id':'original','mode':'5','lanes':[{'name':'left','nativeFrames':[100,120],'nativeHoldFrames':[[140,160]]}]}]}
            cw={'schema':'ddi-danoni-dump/1','charts':[{'chartId':0,'status':'partial','paths':{'reference':'reference.json'}}]}
            ref={'schema':'ddi-danoni-reference/1','captureStage':'scoreConvert-return-full','fps':60,'chartId':0,'difData':{'keyLabel':'5'},'timing':{'playbackRate':1,'intAdjustment':15},'lanes':[{'chara':'left','taps':[115,135],'holds':[[155,175]]}]}
            for p,value in [(n,native),(c,cw),(r,ref)]:p.write_text(json.dumps(value))
            report=compare(n,c,'original',0);self.assertTrue(report['equalRawFrames']);self.assertFalse(report['cwOfflineWorkPassed'])
            ref['lanes'][0]['holds'][0][1]+=1;r.write_text(json.dumps(ref));report=compare(n,c,'original',0)
            self.assertFalse(report['equalRawFrames']);self.assertEqual(report['lanes'][0]['cwOnlyHoldFrames'],[(140,161)])
            ref['timing']['playbackRate']=2;r.write_text(json.dumps(ref))
            with self.assertRaises(ValueError):compare(n,c,'original',0)
