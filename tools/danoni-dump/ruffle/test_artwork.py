"""Original synthetic crop geometry, provenance guards and art-only baselines."""
import copy
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zlib

from artwork_review import load_review,undecorated_simfile
from capture_title import panel_rectangle
from export_io import digest,write_json
from test_projection import string


def png():
    def chunk(name,payload):return struct.pack('>I',len(payload))+name+payload+struct.pack('>I',zlib.crc32(name+payload))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',640,200,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes(640*3))*200))+chunk(b'IEND',b'')


def state():
    return {'incomplete':False,'avm1Halted':False,'stageToViewport':[2,0,0,2,20,40],
            'displayTree':[{'id':0,'parent':None,'kind':'movie-clip','name':string('root'),'visible':True},
                           {'id':1,'parent':0,'kind':'movie-clip','name':string('title'),'visible':True,'movieFrame':31,'stageBoundsPixels':[10,20,110,50]}]}


class Artwork(unittest.TestCase):
    def test_native_crop_uses_viewport_transform_and_visible_ancestry(self):
        display,crop=panel_rectangle(state(),'title',1280,960)
        self.assertEqual(crop,[39,79,202,62]);self.assertEqual(display['movieFrame'],31)
        for change in ['hidden','duplicate','outside','truncated','cycle']:
            s=state()
            if change=='hidden':s['displayTree'][0]['visible']=False
            elif change=='duplicate':s['displayTree'].append({**s['displayTree'][1],'id':2})
            elif change=='outside':s['displayTree'][1]['stageBoundsPixels']=[-100,20,110,50]
            elif change=='cycle':s['displayTree'][0]['parent']=1
            else:s['incomplete']=True
            with self.assertRaises(ValueError):panel_rectangle(s,'title',1280,960)

    def test_review_binds_source_asset_and_native_panel_evidence(self):
        with tempfile.TemporaryDirectory()as d:
            root=Path(d);body=png();(root/'banner.png').write_bytes(body)
            proof={'schema':'ddi-native-title-panel/1','sourceSwfSha256':'a'*64,'files':{'banner.png':digest(body)},'titleReplaced':False}
            write_json(root/'panel.json',proof)
            review={'schema':'ddi-reviewed-song-artwork/1','sourceManifestSha256':'b'*64,'charts':{'one':{'origin':'native-title-panel','banner':{'path':'banner.png','sha256':digest(body)},'evidence':{'path':'panel.json','sha256':digest((root/'panel.json').read_bytes())}}}}
            write_json(root/'review.json',review)
            _,selected=load_review(root/'review.json','b'*64,['one'],'a'*64)
            self.assertEqual(selected['one']['bytes'],body)
            with self.assertRaises(ValueError):load_review(root/'review.json','c'*64,['one'],'a'*64)
            with self.assertRaises(ValueError):load_review(root/'review.json','b'*64,['two'],'a'*64)
            with self.assertRaises(ValueError):load_review(root/'review.json','b'*64,['one'],'d'*64)
            bad=copy.deepcopy(review);bad['charts']['one']['banner']['path']='../outside.png';write_json(root/'bad.json',bad)
            with self.assertRaises((ValueError,FileNotFoundError)):load_review(root/'bad.json','b'*64,['one'],'a'*64)
            (root/'banner.png').write_bytes(body+b'changed')
            with self.assertRaises(ValueError):load_review(root/'review.json','b'*64,['one'],'a'*64)

    def test_art_only_baseline_cannot_hide_note_or_other_header_changes(self):
        baseline=b'#TITLE:Synthetic;\n#BPMS:0=75;\n#NOTES:10000;\n'
        decorated=b'#BANNER:banner.png;\n'+baseline
        self.assertEqual(undecorated_simfile(decorated),baseline)
        self.assertNotEqual(undecorated_simfile(decorated.replace(b'10000',b'01000')),baseline)
        self.assertNotEqual(undecorated_simfile(decorated.replace(b'0=75',b'0=80')),baseline)


if __name__=='__main__':unittest.main()
