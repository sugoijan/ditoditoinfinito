"""Original synthetic tests for numeric publisher ratings and derived tiers."""
import copy
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from export_io import digest
from source_ratings import rating,visible_text,load_review
from prepare_pack import prepare
from test_pack_policy import audio


class Ratings(unittest.TestCase):
    def test_notations_count_only_filled_stars_and_preserve_ranges(self):
        self.assertEqual(rating('filled-stars','★★★★☆')['maximum'],4)
        self.assertEqual(rating('star-number','☆08')['maximum'],8)
        self.assertEqual(rating('star-number','★×１８')['maximum'],18)
        self.assertEqual(rating('star-range','★→★★★★★ ★★')['minimum'],1)
        self.assertEqual(rating('star-range','★→★★★★★ ★★')['maximum'],7)
        self.assertTrue(rating('star-number','★×28?')['uncertain'])
        for notation,token in [('filled-stars','Hard ★'),('filled-stars','☆☆'),('integer','3.5'),('integer','1000'),('star-range','★★→★')]:
            with self.assertRaises(ValueError):rating(notation,token)

    def test_html_is_data_and_decorative_stars_are_not_selected(self):
        body=b'<title>Title star</title><script>danger()</script><input value="Hard 99"><p>Difficulty <b>13</b></p>'
        self.assertEqual(visible_text(body,'utf-8'),'Difficulty 13')
        body='<p>難易度 <font color="red">★</font>☆☆</p>'.encode()
        self.assertEqual(visible_text(body,'utf-8'),'難易度 ★☆☆')

    def fixture(self,root):
        body='<p>5key LVUP ★→★★★★★ ★★</p><p>Other author ★×28?</p>'.encode()
        (root/'page.html').write_bytes(body)
        evidence={'notation':'star-range','token':'★→★★★★★ ★★','excerpt':'5key LVUP ★→★★★★★ ★★',
                  'encoding':'utf-8','scale':'Original publisher scale','authority':'Original maker',
                  'evidence':{'path':'page.html','sha256':digest(body),'url':'https://example.invalid/work/','retrievedAt':'2026-10-10'}}
        entries=[{'id':'original','chart':{'mode':'5'},'metadata':{'difficultyLabel':'LVUP'},'stars':None}]
        review={'schema':'ddi-reviewed-source-ratings/1','sourceManifestSha256':'synthetic source',
                'charts':{'original':{'chartIdentity':{'mode':'5','label':'LVUP'},'ratings':[evidence],
                                     'selectedRating':0,'selectionReason':'Use published progression peak'}}}
        return review,entries

    def load(self,root,review,entries):
        path=root/'review.json';path.write_text(json.dumps(review));return load_review(path,'synthetic source',entries)[1]

    def test_review_binds_chart_source_excerpt_and_body(self):
        with tempfile.TemporaryDirectory(prefix='ddi-source-rating-') as directory:
            root=Path(directory);review,entries=self.fixture(root)
            result=self.load(root,review,entries)['original']
            self.assertEqual(result['meter'],7);self.assertEqual(result['ratings'][0]['minimum'],1)
            for field,value in [('sourceManifestSha256','changed')]:
                changed=copy.deepcopy(review);changed[field]=value
                with self.assertRaises(ValueError):self.load(root,changed,entries)
            changed=copy.deepcopy(review);changed['charts']['original']['chartIdentity']['label']='Hard'
            with self.assertRaises(ValueError):self.load(root,changed,entries)
            changed=copy.deepcopy(review);changed['charts']['original']['ratings'][0]['excerpt']='absent'
            with self.assertRaises(ValueError):self.load(root,changed,entries)
            entries[0]['stars']=4
            with self.assertRaises(ValueError):self.load(root,review,entries)
            entries[0]['stars']=None;(root/'page.html').write_text('modified')
            with self.assertRaises(ValueError):self.load(root,review,entries)

    def test_uncertain_alternative_is_preserved_but_cannot_set_meter(self):
        with tempfile.TemporaryDirectory(prefix='ddi-rating-alternatives-') as directory:
            root=Path(directory);review,entries=self.fixture(root);row=review['charts']['original']
            alt=copy.deepcopy(row['ratings'][0]);alt.update(notation='star-number',token='★×28?',excerpt='Other author ★×28?')
            row['ratings'].append(alt)
            self.assertTrue(self.load(root,review,entries)['original']['ratings'][1]['uncertain'])
            row['selectedRating']=1
            with self.assertRaises(ValueError):self.load(root,review,entries)

    def test_integer_token_uses_digit_boundaries(self):
        with tempfile.TemporaryDirectory(prefix='ddi-rating-integer-') as directory:
            root=Path(directory);review,entries=self.fixture(root)
            body=b'<p>Chart 90 LVUP level 9 (2009)</p>';(root/'page.html').write_bytes(body)
            row=review['charts']['original']['ratings'][0]
            row.update(notation='integer',token='9',excerpt='Chart 90 LVUP level 9 (2009)');row['evidence']['sha256']=digest(body)
            self.assertEqual(self.load(root,review,entries)['original']['meter'],9)

    def test_prepared_ssc_uses_shared_tier_and_meter_retaining_source_label(self):
        with tempfile.TemporaryDirectory(prefix='ddi-rated-pack-') as directory:
            root=Path(directory);review,entries=self.fixture(root);source=root/'source';source.mkdir();audio(source/'music.wav')
            chart={'id':'original','mode':'5','metadata':{'difficultyLabel':'LVUP'},
                   'lanes':[{'tapsSeconds':[1,2]if i==0 else[],'holdsSeconds':[]}for i in range(5)]}
            manifest={'schema':'ddi-normalized-work/experimental-1','charts':[chart],
                      'music':{'path':'music.wav','sha256':digest((source/'music.wav').read_bytes())}}
            raw=json.dumps(manifest).encode();(source/'manifest.json').write_bytes(raw)
            review['sourceManifestSha256']=digest(raw);(root/'review.json').write_text(json.dumps(review))
            result=prepare(source,root/'pack','wav',Path(shutil.which('ffmpeg')),tempo_policy='exact',ratings_review=root/'review.json')
            c=result['songs'][0]['charts'][0];text=(root/'pack'/result['songs'][0]['simfile']).read_text()
            self.assertEqual(c['sourceDifficultyLabel'],'LVUP');self.assertEqual(c['difficultyStars'],7)
            self.assertIn('#DIFFICULTY:Hard;',text);self.assertIn('#METER:7;',text);self.assertNotIn('#CHARTNAME:',text)
            self.assertEqual((source/'manifest.json').read_bytes(),raw)
            self.assertEqual((root/'pack/ratings/page.html').read_bytes(),(root/'page.html').read_bytes())

    def test_equal_ratings_preserve_names_for_chart_identity_without_changing_tiers(self):
        with tempfile.TemporaryDirectory(prefix='ddi-equal-ratings-') as directory:
            root=Path(directory);source=root/'source';source.mkdir();audio(source/'music.wav')
            charts=[{'id':name.lower(),'mode':'5','difficultyStars':13,'metadata':{'difficultyLabel':name},
                     'lanes':[{'tapsSeconds':[1,2]if i==0 else[],'holdsSeconds':[]}for i in range(5)]}for name in ['Remake','Hard']]
            manifest={'schema':'ddi-normalized-work/experimental-1','charts':charts,'music':{'path':'music.wav','sha256':digest((source/'music.wav').read_bytes())}}
            (source/'manifest.json').write_text(json.dumps(manifest))
            result=prepare(source,root/'pack','wav',Path(shutil.which('ffmpeg')),tempo_policy='exact')
            text=(root/'pack'/result['songs'][0]['simfile']).read_text()
            self.assertIn('#CHARTNAME:Remake;',text);self.assertIn('#CHARTNAME:Hard;',text)
            self.assertEqual(text.count('#DIFFICULTY:Challenge;'),2)
            self.assertTrue(all(c['preservedDuplicateLabel']for c in result['songs'][0]['charts']))


if __name__=='__main__':unittest.main()
