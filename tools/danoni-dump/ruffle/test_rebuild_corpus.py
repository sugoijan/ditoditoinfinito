"""Synthetic transaction, eligibility and export-integrity checks; no work assets."""
import json
import random
import wave
from array import array
import sys
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import rebuild_corpus as rebuild
from export_io import digest, write_json


class Rebuild(unittest.TestCase):
    def test_audio_loss_is_distinct_from_origin_and_sample_count_errors(self):
        values=array('h',[random.Random(i).randrange(-12000,12000)for i in range(20000)])
        def wav(path,data):
            with wave.open(str(path),'wb') as output:
                output.setnchannels(1);output.setframerate(4000);output.setsampwidth(2)
                copy=array('h',data)
                if sys.byteorder!='little':copy.byteswap()
                output.writeframes(copy.tobytes())
        with tempfile.TemporaryDirectory() as d:
            a,b=Path(d)/'a.wav',Path(d)/'b.wav';wav(a,values);wav(b,values)
            self.assertTrue(rebuild.audio_comparison(a,b)['windows'])
            wav(b,[v+random.Random(i+30000).randrange(-6000,6000)for i,v in enumerate(values)])
            with self.assertRaises(rebuild.AudioQualityError):rebuild.audio_comparison(a,b)
            wav(b,[0]*7+list(values[:-7]))
            with self.assertRaises(ValueError) as error:rebuild.audio_comparison(a,b)
            self.assertNotIsInstance(error.exception,rebuild.AudioQualityError)
            wav(b,values[:-1])
            with self.assertRaises(ValueError) as error:rebuild.audio_comparison(a,b)
            self.assertNotIsInstance(error.exception,rebuild.AudioQualityError)

    def test_only_9a_and_9b_are_packaged_from_a_mixed_nine_key_work(self):
        with tempfile.TemporaryDirectory()as d:
            root=Path(d);charts=[]
            for index,mode in enumerate(['9i','9A','9B','9t']):
                paths={key:f'{index}-{key}.json'for key in ['effectiveDos','keys','reference','snapshots']}
                for key,path in paths.items():write_json(root/path,{'chartId':index}if key=='reference'else{})
                charts.append({'chartId':index,'keyMode':mode,'status':'complete'if mode in ('9A','9B')else'partial','paths':paths})
            (root/'music.ogg').write_bytes(b'original synthetic media')
            m={'schema':'ddi-danoni-dump/1','status':'partial','charts':charts,'assets':[{'role':'music','path':'music.ogg','sha256':digest((root/'music.ogg').read_bytes()),'chartIds':[1,2]}]}
            write_json(root/'manifest.json',m)
            report=rebuild.cw_dump(root/'manifest.json',root/'package',{'passed':True,'comparisons':[{'chartId':i,'referenceMatched':True}for i in [1,2]]})
            packaged=json.loads((root/'package/manifest.json').read_text())
            self.assertEqual([c['keyMode']for c in packaged['charts']],['9A','9B'])
            self.assertEqual([c['mode']for c in report['charts']],['9A','9B'])
            self.assertFalse((root/'package/0-reference.json').exists());self.assertFalse((root/'package/3-reference.json').exists())

    def generation(self, folder):
        folder.mkdir()
        (folder / 'playable.zip').write_bytes(b'original synthetic archive')
        (folder / 'validation.jsonl').write_text(json.dumps({'work': 'one', 'validation': {'passed': True}}) + '\n')
        report = {'schema': 'ddi-danoni-export-generation/1', 'passed': True,
                  'failedRequiredWorks': [], 'works': [{'id': 'one', 'status': 'built', 'required': True}],
                  'artifacts': [{'path': 'playable.zip', 'sha256': digest((folder / 'playable.zip').read_bytes())}],
                  'files': [rebuild.binding(folder, p) for p in folder.iterdir()]}
        write_json(folder / 'index.json', report)
        return report

    def test_changed_or_unlisted_files_cannot_replace_accepted_outputs(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d); stage = root / 'stage'; self.generation(stage)
            destination = root / 'exports'; destination.mkdir(); (destination / 'keep').write_text('accepted')
            (stage / 'extra').write_text('unexpected')
            with self.assertRaises(ValueError): rebuild.promote(stage, destination, root / 'history')
            (stage / 'extra').unlink(); (stage / 'playable.zip').write_bytes(b'changed')
            with self.assertRaises(ValueError): rebuild.promote(stage, destination, root / 'history')
            self.assertEqual((destination / 'keep').read_text(), 'accepted')

    def test_promotion_archives_old_and_recovery_rolls_back_interruption(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d); stage = root / 'stage'; self.generation(stage)
            destination = root / 'exports'; destination.mkdir(); (destination / 'keep').write_text('accepted')
            state = rebuild.promote(stage, destination, root / 'history')
            self.assertEqual(state['phase'], 'promoted')
            self.assertEqual((Path(state['archive']) / 'keep').read_text(), 'accepted')
            journal = next((root / 'history').glob('*.json'))
            destination.rename(stage); state['phase'] = 'old-archived'; write_json(journal, state)
            self.assertEqual(rebuild.recover(journal)['phase'], 'rolled-back')
            self.assertEqual((destination / 'keep').read_text(), 'accepted')
            rebuild.validate_generation(stage)

    def test_copy_path_hash_and_symlink_guards(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d); source = root / 'source'; source.mkdir(); (source / 'file').write_text('own data')
            with self.assertRaises(ValueError): rebuild.checked(source, '../source/file', '0' * 64)
            (root / 'outside').write_text('outside data')
            with self.assertRaises(ValueError): rebuild.checked(source, '../outside')
            (source / 'link').symlink_to(source / 'file')
            with self.assertRaises(ValueError): rebuild.copy_tree(source, root / 'copy')
            with self.assertRaises(ValueError): rebuild.disjoint(source, root)

    def test_staging_cannot_overwrite_native_evidence(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);registry=root/'registry.json'
            write_json(registry,{'schema':rebuild.SCHEMA,'root':str(root),'collections':[],'works':[]})
            for stage in [root/name/part for name in ['native-adapters-v5','native-adapters-v6','native-adapters-v7'] for part in ['','new-stage']]:
                with self.assertRaisesRegex(ValueError,'overlap evidence'):
                    rebuild.build(registry,stage,None,None,None)
                self.assertFalse(stage.exists())

    def test_required_pending_and_optional_failed_work_are_explicit(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d); (root / 'source.json').write_text('{}')
            entry = {'id': 'one', 'kind': 'pending', 'required': True, 'collection': 'songs',
                     'source': rebuild.binding(root, root / 'source.json'), 'evidence': [], 'reason': 'Needs an adapter'}
            registry = root / 'registry.json'
            config = {'schema': rebuild.SCHEMA, 'root': str(root), 'collections': [{'id': 'songs', 'title': 'Songs'}], 'works': [entry]}
            write_json(registry, config)
            with self.assertRaises(ValueError): rebuild.build(registry, root / 'stage', None, None, None)
            report = json.loads((root / 'stage/index.json').read_text())
            self.assertFalse(report['passed']); self.assertEqual(report['failedRequiredWorks'], ['one', 'no-built-works'])
            # One failed optional work writes partial data; another work succeeds.
            (root / 'verification.json').write_text('{}'); (root / 'verifier').write_bytes(b'own verifier')
            optional = {**entry, 'id': 'bad', 'kind': 'cw-dump', 'required': False,
                        'verification': rebuild.binding(root, root / 'verification.json')}
            config['works'] = [optional, {**entry, 'kind': 'normalized'}]; write_json(registry, config)
            def bad(source, destination, verification):
                destination.mkdir(parents=True); (destination / 'partial').write_text('incomplete'); raise ValueError('missing chart')
            def prepared(source, output, *args, **kwargs):
                output.mkdir(parents=True); folder = output / 'songs/synthetic'; folder.mkdir(parents=True)
                (folder / 'song.ssc').write_text('own synthetic chart'); (output / 'playable.zip').write_bytes(b'old flat zip')
                return {'songs': [{'id': 'synthetic', 'charts': [{'id': 'chart', 'unrepresentedEffects': []}]}]}
            verification = subprocess.CompletedProcess([], 0, json.dumps({'work': 'one', 'validation': {'passed': True}}) + '\n', '')
            with patch.object(rebuild, 'cw_dump', bad), patch.object(rebuild, 'prepare', prepared), patch.object(rebuild, 'audio_check'), patch.object(rebuild.subprocess, 'run', return_value=verification):
                report = rebuild.build(registry, root / 'stage2', None, None, root / 'verifier')
            self.assertTrue(report['passed']); self.assertEqual(report['works'][0]['status'], 'failed')
            self.assertTrue((root / 'stage2/failed/bad/bad/partial').is_file())
            import zipfile
            with zipfile.ZipFile(root / 'stage2/songs/Songs.zip') as archive:
                self.assertEqual(archive.namelist(), ['Songs/one--synthetic/song.ssc'])
            rebuild.validate_generation(root / 'stage2')

    def test_retirement_requires_independent_evidence_and_preserves_old_bytes(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d).resolve(); generation = root / 'exports'; report = self.generation(generation)
            old = root / 'old'; old.mkdir(); (old / 'evidence').write_bytes(b'original evidence')
            registry = root / 'registry.json'
            config = {'root': str(root), 'works': [{'source': rebuild.binding(root, old / 'evidence')}]}
            write_json(registry, config)
            report.update(registry=str(registry), registrySha256=digest(registry.read_bytes()))
            write_json(generation / 'index.json', report)
            with self.assertRaises(ValueError): rebuild.retire(old, generation, root / 'history')
            self.assertTrue(old.exists())
            copied = root / 'canonical'; copied.write_bytes((old / 'evidence').read_bytes())
            config['works'][0]['source'] = rebuild.binding(root, copied); write_json(registry, config)
            with self.assertRaises(ValueError): rebuild.retire(old, generation, root / 'history')
            report['registrySha256'] = digest(registry.read_bytes()); write_json(generation / 'index.json', report)
            state = rebuild.retire(old, generation, root / 'history')
            self.assertEqual(state['phase'], 'archived'); self.assertFalse(old.exists())
            self.assertEqual((Path(state['archive']) / 'evidence').read_bytes(), b'original evidence')
            rebuild.validate_generation(generation)

    def test_archive_contents_are_reproducible(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d); folder = root / 'files'; folder.mkdir(); (folder / 'file').write_bytes(b'own data')
            rebuild.write_zip(folder, root / 'a.zip'); rebuild.write_zip(folder, root / 'b.zip')
            self.assertEqual((root / 'a.zip').read_bytes(), (root / 'b.zip').read_bytes())


if __name__ == '__main__': unittest.main()
