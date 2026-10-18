#!/usr/bin/env python3
"""Maintainer-only license inventory; never used by installed imports.

Collect an intentionally overinclusive Cargo metadata graph for BOTH locked
manifests. This preserves upstream notice files, not a legal compatibility audit.
Missing texts and embedded assets remain explicit redistribution review items.
No acquired chart directory is read and no work code is executed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('sdk', type=Path, help='SDK Cargo.toml')
    parser.add_argument('worker', type=Path, help='prepared worker Cargo.toml')
    parser.add_argument('output', type=Path, help='fresh notice directory')
    parser.add_argument('--target', required=True, help='actual build target triple')
    args = parser.parse_args()
    args.output.mkdir(parents=False, exist_ok=False)
    packages = {}
    locks = []
    artifacts = []

    def save(name, data):
        target = args.output / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        artifacts.append({'path': name, 'bytes': len(data), 'sha256': digest(data)})

    for label, manifest in [('sdk', args.sdk), ('worker', args.worker)]:
        manifest = manifest.resolve(strict=True)
        command = ['cargo', 'metadata', '--format-version', '1', '--locked', '--offline',
                   '--filter-platform', args.target, '--manifest-path', str(manifest)]
        metadata = json.loads(subprocess.check_output(command))
        lock = Path(metadata['workspace_root']) / 'Cargo.lock'
        data = lock.read_bytes()
        save(f'locks/{label}-Cargo.lock', data)
        locks.append({'component': label, 'sha256': digest(data)})
        # Metadata includes development and target-inactive packages. Keeping
        # extra notices is deliberate; do not call this a linked-binary SBOM.
        for package in metadata['packages']:
            key = package['id']
            if key in packages:
                packages[key]['components'].append(label)
                continue
            root = Path(package['manifest_path']).parent
            name = f"{package['name']}-{package['version']}-{digest(key.encode())[:12]}"
            candidates = set()
            for candidate in root.rglob('*'):
                if any(part in ('target', '.git') for part in candidate.relative_to(root).parts):
                    continue
                if candidate.is_file() and not candidate.is_symlink() and candidate.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE', 'COPYRIGHT')) and candidate.suffix.lower() not in ('.rs', '.c', '.h', '.cpp', '.json'):
                    candidates.add(candidate)
            if package.get('license_file'):
                candidates.add(root / package['license_file'])
            texts = []
            for candidate in sorted(candidates):
                resolved = candidate.resolve(strict=True)
                relative = resolved.relative_to(root.resolve())
                data = resolved.read_bytes()
                if len(data) > 32 * 1024 * 1024:
                    raise ValueError(f'Oversized license file: {relative}')
                path = f'packages/{name}/{relative}'
                save(path, data)
                texts.append(path)
            packages[key] = {'name': package['name'], 'version': package['version'],
                             'source': package['source'], 'license': package['license'],
                             'components': [label], 'noticeFiles': texts}
    missing = [p for p in packages.values() if not p['noticeFiles']]
    receipt = {'schema': 'ddi-danoni-runtime-notices/1', 'target': args.target,
               'locks': locks, 'packages': sorted(packages.values(), key=lambda p: (p['name'], p['version'])),
               'artifacts': sorted(artifacts, key=lambda a: a['path']),
               'packagesWithoutCollectedText': missing,
               'redistributionApproved': False,
               'scope': 'Overinclusive locked Cargo metadata graph, not a linked-binary SBOM. Review missing notices, license compatibility, vendored C dependencies and Ruffle embedded NotoSans/NotoSansMath font notices before public distribution. System Vulkan/Mesa/glibc are not shipped by this package.'}
    save('manifest.json', json.dumps(receipt, ensure_ascii=False, indent=2).encode() + b'\n')
    print(json.dumps({'packages': len(packages), 'noticeFiles': len(artifacts) - 1,
                      'packagesWithoutCollectedText': len(missing), 'redistributionApproved': False}))


if __name__ == '__main__':
    main()
