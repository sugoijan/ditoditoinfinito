"""Synthetic integrity/notice policy checks; never build or import a song."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
spec = importlib.util.spec_from_file_location("importer_build", ROOT / "build.py")
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)
from notices import encoded, has_grant, license_choice, metadata, notice_file, sha


class Packaging(unittest.TestCase):
    def test_workspace_membership_keeps_importer_out_of_app_dependency_graphs(self):
        root = ROOT.parents[1]
        data = json.loads(subprocess.check_output(
            ['cargo', 'metadata', '--locked', '--format-version', '1', '--no-deps'], cwd=root))
        members = {p['name'] for p in data['packages'] if p['id'] in data['workspace_members']}
        self.assertTrue({'ddi-danoni-export', 'ddi-danoni-importer'} <= members)
        self.assertFalse({'ddi-ruffle-probe', 'ddi-danoni-desktop-probe', 'ddi-danoni-pack-verify'} & members)
        defaults = {p['name'] for p in data['packages'] if p['id'] in data['workspace_default_members']}
        self.assertEqual(defaults, {'ditoditoinfinito'})
        for target in ('aarch64-apple-darwin', 'wasm32-unknown-unknown', 'x86_64-pc-windows-msvc'):
            for features in ([], ['--features', 'danoni-importer']):
                tree = subprocess.check_output(
                    ['cargo', 'tree', '--locked', '-p', 'ditoditoinfinito', '--target', target,
                     '--edges', 'normal', '--prefix', 'none', *features], cwd=root, text=True)
                self.assertNotIn('ddi-danoni-export ', tree)
                self.assertNotIn('ddi-danoni-importer ', tree)

    def test_nested_build_selects_importers_and_uses_an_independent_target(self):
        with patch.object(build, 'run') as run:
            build.cargo(Path('/source/Cargo.toml'), Path('/cache/importer-target'),
                        'aarch64-apple-darwin', {'CARGO_TARGET_DIR': '/app/target'},
                        profile='importer', packages=('ddi-danoni-export', 'ddi-danoni-importer'))
        command, env = run.call_args.args
        self.assertEqual(command, ['cargo', 'build', '--locked', '--profile', 'importer',
                                  '--target', 'aarch64-apple-darwin', '--manifest-path',
                                  Path('/source/Cargo.toml'), '--package', 'ddi-danoni-export',
                                  '--package', 'ddi-danoni-importer'])
        self.assertEqual(env['CARGO_TARGET_DIR'], '/cache/importer-target')

    def test_root_lock_invalidates_prepared_bundle_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'Cargo.toml').write_text('[workspace]\n')
            lock = root / 'Cargo.lock'
            lock.write_text('first dependency graph')
            with patch.object(build, 'ROOT', root), patch.object(build, 'compiler_identity', return_value=b'compiler'):
                first = build.fingerprint('target')
                lock.write_text('second dependency graph')
                self.assertNotEqual(first, build.fingerprint('target'))

    def test_notices_select_component_graph_instead_of_workspace_feature_union(self):
        manifest = (ROOT / 'Cargo.toml').resolve()
        packages = [
            {'id': 'helper', 'name': 'ddi-danoni-importer', 'version': '0.1.0', 'manifest_path': str(manifest)},
            {'id': 'dep', 'name': 'shared-dependency', 'version': '1.0.0'},
            {'id': 'unrelated', 'name': 'unrelated-workspace-feature', 'version': '2.0.0'},
        ]
        data = {'packages': packages, 'resolve': {'root': None}}
        tree = 'ddi-danoni-importer v0.1.0 (/source/helper)\nshared-dependency v1.0.0\nshared-dependency v1.0.0 (*)\n'
        with patch('notices.subprocess.check_output', side_effect=[json.dumps(data).encode(), tree]) as output:
            _, selected = metadata(manifest, 'target', {})
        self.assertEqual({p['id'] for p in selected}, {'helper', 'dep'})
        command = output.call_args.args[0]
        self.assertEqual(command[command.index('--package') + 1], 'ddi-danoni-importer')
        self.assertEqual(command[command.index('--edges') + 1], 'normal,no-proc-macro')

    def test_worker_notices_distinguish_local_and_registry_copies_at_same_version(self):
        manifest = (ROOT / 'Cargo.toml').resolve()
        packages = [
            {'id': 'worker', 'name': 'ddi-ruffle-probe', 'version': '0.1.0', 'manifest_path': str(manifest)},
            {'id': 'local-swf', 'name': 'swf', 'version': '0.3.0', 'source': None,
             'manifest_path': '/prepared upstream/swf/Cargo.toml'},
            {'id': 'registry-swf', 'name': 'swf', 'version': '0.3.0', 'source': 'registry+https://github.com/rust-lang/crates.io-index'},
        ]
        for row, selected_id in [('swf v0.3.0 (/prepared upstream/swf)', 'local-swf'),
                                 ('swf v0.3.0', 'registry-swf')]:
            tree = f'ddi-ruffle-probe v0.1.0 (/worker)\n{row}\n{row} (*)\n'
            with patch('notices.subprocess.check_output', side_effect=[json.dumps({'packages': packages}).encode(), tree]):
                _, selected = metadata(manifest, 'target', {})
            self.assertEqual({p['id'] for p in selected}, {'worker', selected_id})

    def test_normal_runtime_license_policy_obeys_and_or_and_exceptions(self):
        self.assertEqual(license_choice("MIT OR GPL-3.0-only"), ["MIT"])
        self.assertIsNone(license_choice("MIT AND GPL-3.0-only"))
        self.assertEqual(license_choice("(MIT OR Apache-2.0) AND BSD-3-Clause"), ["MIT", "BSD-3-Clause"])
        self.assertEqual(license_choice("Apache-2.0 WITH LLVM-exception"), ["Apache-2.0 WITH LLVM-exception"])
        self.assertIsNone(license_choice("Apache-2.0 WITH Unknown-exception"))

    def test_notices_are_not_complete_license_grants(self):
        self.assertFalse(has_grant("Copyright Example. Licensed under MIT.", "MIT"))
        apache = (ROOT.parents[1] / "LICENSES/Apache-2.0.txt").read_text()
        self.assertEqual(license_choice("MIT OR Apache-2.0", apache), ["Apache-2.0"])
        self.assertIsNone(license_choice("MIT AND Apache-2.0", apache))

    def test_source_code_cannot_supply_license_grant_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            grant = "Permission is hereby granted, free of charge. The software is provided."
            for name in ("notices.py", "LICENSE.sh", "COPYRIGHT.js", "NOTICE.RS", "LICENSE.json", "NOTICE.py.txt"):
                (folder / name).write_text(grant)
            evidence = "\n".join(p.read_text() for p in folder.iterdir() if notice_file(p))
            self.assertIsNone(license_choice("MIT", evidence))
            (folder / "LICENSE.MIT").write_text(grant)
            evidence = "\n".join(p.read_text() for p in folder.iterdir() if notice_file(p))
            self.assertEqual(license_choice("MIT", evidence), ["MIT"])

    def test_worker_cache_changes_with_deployment_and_target(self):
        with patch.dict(os.environ, {"MACOSX_DEPLOYMENT_TARGET": "14.0"}):
            first = build.compiler_identity("aarch64-apple-darwin")
            self.assertNotEqual(first, build.compiler_identity("x86_64-apple-darwin"))
        with patch.dict(os.environ, {"MACOSX_DEPLOYMENT_TARGET": "15.0"}):
            self.assertNotEqual(first, build.compiler_identity("aarch64-apple-darwin"))

    def test_inventory_binds_every_byte_and_rejects_extra_missing_and_symlink_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "bin").mkdir()
            (root / "bin/ddi-danoni-importer").write_bytes(b"synthetic inventory only")
            notices = root / "runtime/notices"
            notices.mkdir(parents=True)
            (notices / "manifest.json").write_bytes(encoded({"redistributionApproved": True, "packagesWithoutCollectedText": []}))
            manifest = {"schema": "ddi-danoni-app-bundle/1", "protocolVersion": 1, "target": "synthetic", "executable": "bin/ddi-danoni-importer", "artifacts": build.inventory(root)}
            manifest["id"] = sha(encoded(manifest))
            (root / "manifest.json").write_bytes(encoded(manifest))
            self.assertEqual(build.verify(root)[0], manifest)
            (root / "extra").write_bytes(b"untracked")
            with self.assertRaises(ValueError):
                build.verify(root)
            (root / "extra").unlink()
            helper = root / "bin/ddi-danoni-importer"
            helper.write_bytes(b"changed")
            with self.assertRaises(ValueError):
                build.verify(root)
            helper.unlink()
            with self.assertRaises(ValueError):
                build.verify(root)
            helper.symlink_to(notices / "manifest.json")
            with self.assertRaises(ValueError):
                build.verify(root)


if __name__ == "__main__":
    unittest.main()
