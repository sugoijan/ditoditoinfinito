"""Original synthetic policies and normalized works, unrelated to work adapters."""
import hashlib
import json
import math
from pathlib import Path
import struct
import shutil
import tempfile
import unittest
import wave

from pack_policy import star_count,tier_for_stars,choose_preview,display_bpm
from prepare_pack import prepare,ssc_chart


def audio(path,stereo=False):
    with wave.open(str(path),'wb') as wav:
        wav.setnchannels(2 if stereo else 1);wav.setsampwidth(2);wav.setframerate(8000)
        for second in range(20):
            values=[]
            for i in range(8000):
                value=0 if second<4 else round(1600*math.sin(i*2*math.pi*440/8000))
                values.append(struct.pack('<hh',value,-value) if stereo else struct.pack('<h',value))
            wav.writeframes(b''.join(values))


class Policies(unittest.TestCase):
    def test_source_stars_and_configurable_derived_tiers(self):
        self.assertEqual([tier_for_stars(n) for n in [1,3,4,5,6,7,8]],['Beginner','Easy','Medium','Medium','Hard','Hard','Challenge'])
        self.assertEqual(tier_for_stars(3,(2,5,9,13)),'Easy')
        self.assertEqual(tier_for_stars(None),'Edit')
        self.assertIsNone(star_count({'difficulty':'Hard 6'},{}))
        self.assertEqual(star_count({'difficulty':'★★★★'},{}),4)
        self.assertEqual(star_count({}, {'difficultyStars':5}),5)
        for value in [True,-1,1000]:
            with self.assertRaises(ValueError):tier_for_stars(value)

    def test_reviewed_display_bpm_is_separate_metadata(self):
        self.assertEqual(display_bpm('200')['sscValue'],'200')
        self.assertEqual(display_bpm('110:220')['kind'],'range')
        self.assertEqual(display_bpm('*')['kind'],'unknown')
        for value in ['0','nan','inf','-1','220:110','120:130:140','']:
            with self.assertRaises(ValueError):display_bpm(value)

    def test_preview_skips_silence_including_opposite_phase_stereo(self):
        with tempfile.TemporaryDirectory(prefix='ddi-preview-') as directory:
            for stereo in [False,True]:
                p=Path(directory)/f'{stereo}.wav';audio(p,stereo)
                before=hashlib.sha256(p.read_bytes()).hexdigest();preview=choose_preview(p,12)
                self.assertGreaterEqual(preview['startSeconds'],4)
                self.assertLessEqual(preview['startSeconds']+preview['lengthSeconds'],20)
                self.assertEqual(preview['activeFraction'],1)
                self.assertEqual(before,hashlib.sha256(p.read_bytes()).hexdigest())

    def test_other_modes_holds_pre_roll_and_loss_rejection(self):
        lanes=[{'tapsSeconds':[],'holdsSeconds':[]} for _ in range(9)]
        lanes[0]['tapsSeconds']=[-.5];lanes[8]['holdsSeconds']=[[1,2]]
        chart={'mode':'9A','lanes':lanes,'speed':[{'seconds':-.5,'multiplier':1.5}]}
        ssc=ssc_chart(chart,'Medium',4)
        self.assertIn('#STEPSTYPE:danoni-9A;',ssc);self.assertIn('#OFFSET:0.5;',ssc)
        self.assertIn('#SCROLLS:0=1.5;',ssc);self.assertIn('000000002',ssc);self.assertIn('000000003',ssc)
        lanes[8]['tapsSeconds']=[1.5]
        with self.assertRaises(ValueError):ssc_chart(chart,'Medium',4)
        lanes[8]['tapsSeconds']=[]
        lanes[0]['tapsSeconds']=[.123456]
        with self.assertRaises(ValueError):ssc_chart(chart,'Medium',4)

    def test_generic_pack_uses_no_family_identifier(self):
        with tempfile.TemporaryDirectory(prefix='ddi-generic-pack-') as directory:
            root=Path(directory);source=root/'input';source.mkdir();audio(source/'sound.wav',True)
            charts=[]
            for mode,count,stars in [('7',7,4),('9B',9,6)]:
                lanes=[{'name':str(i),'tapsSeconds':[1+i/60],'holdsSeconds':[]} for i in range(count)]
                charts.append({'id':mode,'mode':mode,'difficultyStars':stars,'lanes':lanes})
            (source/'manifest.json').write_text(json.dumps({'schema':'ddi-normalized-work/experimental-1',
                'metadata':{'title':'Original synthetic work','artist':'Original fixture'},
                'music':{'path':'sound.wav','sha256':hashlib.sha256((source/'sound.wav').read_bytes()).hexdigest()},'charts':charts}))
            result=prepare(source,root/'output','wav',Path(shutil.which('ffmpeg')),display_bpms={'7':'160'})
            self.assertEqual(len(result['songs']),1)
            self.assertEqual([c['difficultyClassification'] for c in result['songs'][0]['charts']],['Medium','Hard'])
            text=(root/'output'/result['songs'][0]['simfile']).read_text()
            self.assertIn('#DISPLAYBPM:160;',text)
            self.assertIn('#BPMS:0=75;',text)
            self.assertEqual(result['songs'][0]['displayBpm']['source'],'explicit review input')
            self.assertIn('#SAMPLESTART:',text);self.assertIn('#SAMPLELENGTH:12;',text)
            self.assertIn('#STEPSTYPE:danoni-7;',text);self.assertIn('#STEPSTYPE:danoni-9B;',text)
            self.assertEqual(result['songs'][0]['pcmChannels'],2)


if __name__=='__main__':unittest.main()
