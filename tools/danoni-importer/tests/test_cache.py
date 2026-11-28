"""The build cache keeps itself small (build.prune): synthetic folders only."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
spec = importlib.util.spec_from_file_location("importer_build_cache", ROOT / "build.py")
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)


def name(n):
    return f'{n:064x}'


class Cache(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.cache = Path(self.folder.name) / 'cache'
        (self.cache / 'bundles').mkdir(parents=True)
        (self.cache / 'builds').mkdir()

    def tearDown(self):
        self.folder.cleanup()

    def bundle(self, n, used, receipts=1, size=10):
        """Bundle `n`, named by `receipts` receipts last used at `used`."""
        folder = self.cache / 'bundles' / name(n)
        folder.mkdir()
        (folder / 'manifest.json').write_bytes(b'x' * size)
        for k in range(receipts):
            key = self.cache / 'builds' / f'key-{n}-{k}'
            key.mkdir()
            receipt = key / 'ready.json'
            receipt.write_text(json.dumps({'id': name(n)}))
            os.utime(receipt, (used + k, used + k))

    def bundles(self):
        return sorted(p.name for p in (self.cache / 'bundles').iterdir())

    def test_the_bundles_used_most_recently_are_kept_with_their_receipts(self):
        for n in range(10):
            self.bundle(n, 1000 + 10 * n, receipts=2)
        kept = build.prune(self.cache)
        self.assertEqual(self.bundles(), [name(n) for n in range(10 - build.KEEP_BUNDLES, 10)])
        self.assertEqual(sorted(kept), self.bundles())
        receipts = sorted(json.loads(p.read_text())['id'] for p in (self.cache / 'builds').glob('*/ready.json'))
        self.assertEqual(sorted(set(receipts)), self.bundles(), 'no receipt names a bundle that went')
        self.assertEqual(len(receipts), 2 * build.KEEP_BUNDLES)

    def test_the_bundle_just_used_stays_whatever_its_receipts_time(self):
        for n in range(10):
            self.bundle(n, 1000 + 10 * n)
        build.prune(self.cache, keep=name(0))
        self.assertIn(name(0), self.bundles())
        self.assertEqual(len(self.bundles()), build.KEEP_BUNDLES)
        self.assertIn(name(9), self.bundles())

    def test_what_nothing_reads_goes(self):
        self.bundle(1, 1000)
        (self.cache / 'bundles' / name(2)).mkdir()  # no receipt
        (self.cache / 'prepare-killed').mkdir()
        (self.cache / 'prepare-killed' / 'half').write_bytes(b'x')
        for legacy in build.LEGACY:
            (self.cache / legacy).mkdir()
        broken = self.cache / 'builds' / 'broken'
        broken.mkdir()
        (broken / 'ready.json').write_text('not json')
        gone = self.cache / 'builds' / 'gone'
        gone.mkdir()
        (gone / 'ready.json').write_text(json.dumps({'id': name(7)}))  # its bundle is not there
        build.prune(self.cache)
        self.assertEqual(self.bundles(), [name(1)])
        self.assertEqual(sorted(p.name for p in self.cache.iterdir()), ['builds', 'bundles'])
        self.assertEqual(sorted(p.name for p in (self.cache / 'builds').iterdir()), ['key-1-0'])

    def test_a_receipt_never_names_a_folder_outside_the_bundles(self):
        outside = Path(self.folder.name) / 'outside'
        outside.mkdir()
        (outside / 'kept').write_bytes(b'x')
        key = self.cache / 'builds' / 'hostile'
        key.mkdir()
        (key / 'ready.json').write_text(json.dumps({'id': '../../outside'}))
        build.prune(self.cache)
        self.assertTrue((outside / 'kept').exists())
        self.assertFalse(key.exists())

    def test_above_the_cap_cargos_outputs_go_in_order(self):
        self.bundle(1, 1000, size=100)
        for folder, size in (('importer-target', 3000), ('worker-target', 2000), ('workers', 500)):
            (self.cache / folder).mkdir()
            (self.cache / folder / 'file').write_bytes(b'x' * size)
        with patch.dict(os.environ, {'DDI_IMPORTER_CACHE_CAP_GB': str(4000 / 1024 ** 3)}):
            build.prune(self.cache)
        self.assertFalse((self.cache / 'importer-target').exists(), 'the first to go')
        self.assertTrue((self.cache / 'worker-target').exists(), 'under the cap by then')
        with patch.dict(os.environ, {'DDI_IMPORTER_CACHE_CAP_GB': str(1000 / 1024 ** 3)}):
            build.prune(self.cache)
        self.assertFalse((self.cache / 'worker-target').exists())
        self.assertTrue((self.cache / 'workers' / 'file').exists(), 'the packaged worker is never the cap\'s')
        self.assertEqual(self.bundles(), [name(1)])

    def test_under_the_cap_nothing_of_cargos_goes(self):
        self.bundle(1, 1000)
        (self.cache / 'importer-target').mkdir()
        (self.cache / 'importer-target' / 'file').write_bytes(b'x' * 100)
        build.prune(self.cache)
        self.assertTrue((self.cache / 'importer-target' / 'file').exists())

    def test_staged_copies_keep_the_one_staged_and_the_one_before(self):
        parent = Path(self.folder.name) / 'stage' / 'danoni-importer'
        parent.mkdir(parents=True)
        for n in range(6):
            folder = parent / name(n)
            folder.mkdir()
            (folder / 'manifest.json').write_bytes(b'x')
            os.utime(folder, (1000 + n, 1000 + n))
        (parent / '.stage-killed').mkdir()
        (parent / 'notes.txt').write_bytes(b'x')  # not a bundle: left alone
        build.prune_staged(parent, name(2))
        self.assertEqual(sorted(p.name for p in parent.iterdir()), sorted([name(2), name(5), 'notes.txt']))


if __name__ == '__main__':
    unittest.main()
