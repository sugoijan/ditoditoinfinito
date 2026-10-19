#!/usr/bin/env python3
"""Build-time notice collection for the helper's actual normal dependency graph.

No subprocess here is used by the shipped application. Fail closed if a normal
runtime package has an unapproved licence or no collected copyright/licence text.
"""
import hashlib
import json
from pathlib import Path
import re
import subprocess

ALLOWED = {'MIT', 'Apache-2.0', 'BSD-2-Clause', 'BSD-3-Clause', 'ISC', 'Zlib',
           '0BSD', 'Unicode-3.0', 'Unicode-DFS-2016', 'MPL-2.0', 'CC0-1.0',
           'Unlicense', 'BSL-1.0', 'Apache-2.0 WITH LLVM-exception'}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + '\n').encode()


def license_choice(expression, evidence=None):
    tokens = re.findall(r'\(|\)|AND|OR|WITH|[A-Za-z0-9.+-]+', (expression or '').replace('/', ' OR '))
    index = 0
    def primary():
        nonlocal index
        if index >= len(tokens):
            raise ValueError(f'Incomplete license: {expression}')
        token = tokens[index]
        index += 1
        if token == '(':
            result = union()
            if index >= len(tokens) or tokens[index] != ')':
                raise ValueError(f'Unbalanced license: {expression}')
            index += 1
            return result
        if index < len(tokens) and tokens[index] == 'WITH':
            index += 1
            token += ' WITH ' + tokens[index]
            index += 1
        return [[token]]
    def intersection():
        nonlocal index
        result = primary()
        while index < len(tokens) and tokens[index] == 'AND':
            index += 1
            right = primary()
            result = [a + b for a in result for b in right]
        return result
    def union():
        nonlocal index
        result = intersection()
        while index < len(tokens) and tokens[index] == 'OR':
            index += 1
            result += intersection()
        return result
    alternatives = union()
    if index != len(tokens):
        raise ValueError(f'Unknown license syntax: {expression}')
    return next((a for a in alternatives if all(x in ALLOWED for x in a)
                 and (evidence is None or all(has_grant(evidence, x) for x in a))), None)


def has_grant(text, license_id):
    text = ' '.join(text.lower().split())
    patterns = {
        'MIT': ('permission is hereby granted, free of charge', 'the software is provided'),
        'Apache-2.0': ('apache license', 'version 2.0', 'end of terms and conditions'),
        'BSD-2-Clause': ('redistribution and use', 'this software is provided'),
        'BSD-3-Clause': ('redistribution and use', 'this software is provided', 'endorse'),
        'ISC': ('permission to use, copy, modify', 'the software is provided'),
        'Zlib': ('altered source versions', 'origin of this software', 'this software is provided'),
        '0BSD': ('permission to use', 'any purpose', 'without fee', 'the software is provided'),
        'Unicode-3.0': ('unicode', 'permission is hereby granted', 'data files and software'),
        'Unicode-DFS-2016': ('unicode', 'permission is hereby granted', 'data files and software'),
        'MPL-2.0': ('mozilla public license', '2.0', 'exhibit a'),
        'CC0-1.0': ('creative commons', 'waiver', 'public domain'),
        'Unlicense': ('free and unencumbered software', 'public domain'),
        'BSL-1.0': ('boost software license', 'permission', 'distribute'),
        'Apache-2.0 WITH LLVM-exception': ('apache license', 'version 2.0', 'end of terms and conditions', 'llvm exceptions'),
    }
    return all(part in text for part in patterns[license_id])


# License names often use SPDX suffixes (LICENSE.MIT / LICENSE-APACHE-2.0), so
# exclude source/config formats rather than allowing only .txt documents.
SOURCE_SUFFIXES = {
    '.rs', '.c', '.cc', '.cpp', '.cxx', '.h', '.hh', '.hpp', '.hxx', '.m', '.mm',
    '.py', '.pyi', '.pyc', '.sh', '.bash', '.zsh', '.fish', '.ps1', '.bat', '.cmd',
    '.js', '.mjs', '.cjs', '.jsx', '.ts', '.tsx', '.go', '.java', '.kt', '.kts',
    '.swift', '.rb', '.pl', '.php', '.lua', '.ex', '.exs', '.erl', '.hs', '.cs',
    '.json', '.json5', '.toml', '.yaml', '.yml', '.xml', '.ini', '.cfg',
}


def notice_document(path):
    return (path.is_file() and not path.is_symlink()
            and not any(suffix.lower() in SOURCE_SUFFIXES for suffix in path.suffixes))


def notice_file(path):
    return (notice_document(path)
            and path.name.upper().startswith(('LICENSE', 'LICENCE', 'UNLICENSE', 'COPYING', 'NOTICE', 'COPYRIGHT')))


def metadata(manifest, target, env):
    manifest = Path(manifest).resolve()
    cmd = ['cargo', 'metadata', '--format-version', '1', '--locked',
           '--filter-platform', target, '--manifest-path', str(manifest)]
    data = json.loads(subprocess.check_output(cmd, env=env))
    package = next(p for p in data['packages'] if Path(p['manifest_path']).resolve() == manifest)
    # Metadata resolves all workspace members and can unify features from the
    # game/tests. Cargo tree selects the actual component's normal runtime graph.
    command = ['cargo', 'tree', '--locked', '--manifest-path', str(manifest),
               '--package', package['name'], '--target', target,
               '--edges', 'normal,no-proc-macro', '--prefix', 'none',
               '--format', '{p}', '--color', 'never']
    tree = subprocess.check_output(command, env=env, text=True)
    selected = {}
    for line in tree.splitlines():
        match = re.match(r'^(\S+) v(\S+)', line)
        if not match:
            raise ValueError('Unexpected Cargo dependency tree row: ' + line)
        name, version = match.groups()
        candidates = [p for p in data['packages'] if p['name'] == name and p['version'] == version]
        if len(candidates) > 1:
            # Ruffle contains both a local swf and a registry swf at the same
            # version. Cargo's display includes the source directory for paths;
            # crates.io packages have no source suffix. Refuse other ambiguities.
            display = line.removesuffix(' (*)')
            prefix = f'{name} v{version}'
            candidates = [p for p in candidates if
                          (p['source'] is None and display == f"{prefix} ({Path(p['manifest_path']).parent})")
                          or ((p['source'] or '').startswith('registry+') and display == prefix)]
        if len(candidates) != 1:
            raise ValueError(f'Ambiguous Cargo notice package: {name} {version}')
        selected[candidates[0]['id']] = candidates[0]
    if package['id'] not in selected:
        raise ValueError('Cargo normal runtime graph omits selected component')
    return data, [selected[key] for key in sorted(selected)]


def collect(root, sources, manifests, output, target, env):
    output.mkdir()
    artifacts = []
    packages = {}
    supplements = json.loads((root / 'tools/danoni-importer/licenses/supplemental.json').read_bytes())
    supplemental = {(s['name'], s['version']): s for s in supplements}
    unresolved = set()
    locks = []
    def save(name, data):
        path = output / name
        if path.exists():
            if path.read_bytes() != data:
                raise ValueError('Conflicting notice at ' + name)
            return
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        artifacts.append({'path': name, 'bytes': len(data), 'sha256': sha(data)})
    for component, manifest in manifests:
        data, selected = metadata(manifest, target, env)
        lock = (Path(data['workspace_root']) / 'Cargo.lock').read_bytes()
        save(f'locks/{component}-Cargo.lock', lock)
        locks.append({'component': component, 'sha256': sha(lock)})
        for package in selected:
            # Do not serialize Cargo's path package IDs or manifest paths.
            key = (package['name'], package['version'], package['source'] or 'workspace')
            if key in packages:
                packages[key]['components'].append(component)
                continue
            folder = Path(package['manifest_path']).parent.resolve()
            chosen = license_choice(package.get('license'))
            if not chosen:
                raise ValueError(f'Unapproved normal runtime license: {key[:2]} {package.get("license")}')
            texts = set()
            for candidate in folder.rglob('*'):
                if any(p in ('target', '.git') for p in candidate.relative_to(folder).parts):
                    continue
                if notice_file(candidate):
                    texts.add(candidate)
            if package.get('license_file'):
                declared = folder / package['license_file']
                if not notice_document(declared):
                    raise ValueError('Declared license_file is not a plain notice file')
                texts.add(declared)
            # Cargo workspace packages conventionally keep licences at their
            # repository/workspace root. Preserve that upstream notice verbatim.
            for parent in [folder, *folder.parents]:
                if (parent == root or parent == sources / 'upstream'
                        or ((parent / 'Cargo.toml').is_file() and '[workspace]' in (parent / 'Cargo.toml').read_text())):
                    inherited = {parent / name for name in ('LICENSE.md', 'LICENSE-MIT', 'LICENSE-APACHE', 'LICENSE', 'LICENSES/MIT.txt') if (parent / name).is_file()}
                    if inherited:
                        texts.update(inherited)
                        break
            proof = supplemental.get((package['name'], package['version']))
            if proof:
                vcs = json.loads((folder / '.cargo_vcs_info.json').read_bytes())
                if ((package.get('repository') or '').removesuffix('.git') != proof['repository']
                        or vcs['git']['sha1'] != proof['revision']
                        or vcs['path_in_vcs'] != proof['pathInVcs']):
                    raise ValueError('Supplemental notice does not match the exact published source revision')
                notice = root / 'tools/danoni-importer/licenses' / proof['path']
                if sha(notice.read_bytes()) != proof['sha256']:
                    raise ValueError('Supplemental upstream notice changed')
                if proof.get('manifestSha256'):
                    if (sha((folder / 'Cargo.toml').read_bytes()) != proof['manifestSha256']
                            or package.get('authors') != proof['authors']):
                        raise ValueError('Declared SPDX/author fallback evidence changed')
                if proof.get('declarationSha256') and sha((folder / 'README.md').read_bytes()) != proof['declarationSha256']:
                    raise ValueError('Published README license declaration changed')
                if proof.get('overviewPath'):
                    overview = root / 'tools/danoni-importer/licenses' / proof['overviewPath']
                    if sha(overview.read_bytes()) != proof['overviewSha256']:
                        raise ValueError('Pinned upstream licensing overview changed')
                    texts.add(overview)
                texts.add(notice)
            if package['name'] == 'ddi-ruffle-probe' and folder == sources / 'worker':
                texts.add(sources / 'licenses/DDI-MIT.txt')
            name = f'{package["name"]}-{package["version"]}-{sha(str(key).encode())[:12]}'
            notices = []
            for number, candidate in enumerate(sorted(texts)):
                content = candidate.read_bytes()
                if len(content) > 32 * 1024 * 1024:
                    raise ValueError(f'Oversized notice: {candidate.name}')
                path = f'packages/{name}/{number}-{candidate.name}'
                save(path, content)
                notices.append(path)
            evidence = '\n'.join(p.read_text(errors='replace') for p in texts)
            chosen = license_choice(package.get('license'), evidence)
            if chosen is None:
                # A NOTICE alone is not a grant. Only Apache's independently
                # offered, holder-independent standard grant may fill a missing
                # text; an AND branch still needs every other required grant.
                apache = (root / 'LICENSES/Apache-2.0.txt').read_bytes()
                with_apache = license_choice(package.get('license'), evidence + apache.decode())
                if with_apache == ['Apache-2.0']:
                    path = f'packages/{name}/Apache-2.0.txt'
                    save(path, apache)
                    notices.append(path)
                    chosen = with_apache
                else:
                    unresolved.add(f'{key[0]} {key[1]}: {package.get("license")}')
                    continue
            packages[key] = {'name': package['name'], 'version': package['version'],
                             'source': package['source'], 'license': package['license'],
                             'chosenLicense': chosen, 'components': [component], 'noticeFiles': notices,
                             'repository': package.get('repository'), 'supplementalNotice': proof,
                             'sourceArchive': f'https://crates.io/api/v1/crates/{package["name"]}/{package["version"]}/download' if (package['source'] or '').startswith('registry+') else package['source']}
    if unresolved:
        raise ValueError('Missing complete upstream license grants: ' + '; '.join(sorted(unresolved)))
    save('supplemental/manifest.json', encoded(supplements))
    save('SOURCE-CODE.txt', b'Unmodified third-party Rust source is available for every exact package version through each package row sourceArchive URL (crates.io download), or its pinned Git source/repository. In particular the Symphonia MPL-2.0 covered files can be obtained there under the same MPL license. No third-party MPL files were modified by this build. Workspace source and copied-code attribution are in first-party/; Ruffle source preparation and exact locks are in the packaged worker provenance.\n')
    # Shared first-party chart/library code includes permissively copied tables
    # with notices beyond the workspace's MIT grant. Keep their actual REUSE
    # copyright/annotations and every declared upstream notice with the bundle.
    for name in ('REUSE.toml', 'THIRD_PARTY.toml', 'CREDITS.md'):
        save('first-party/' + name, (root / name).read_bytes())
    for license_file in sorted((root / 'LICENSES').iterdir()):
        if license_file.is_file():
            data = license_file.read_bytes()
            if license_file.name == 'MIT.txt':
                data = data.replace(b'Copyright (c) <year> <copyright holders>',
                                    'Copyright (c) すごいジャン <me@sugoijan.dev>'.encode())
            save('first-party/LICENSES/' + license_file.name, data)
    import tomllib
    copied = tomllib.loads((root / 'THIRD_PARTY.toml').read_text())
    for row in copied.get('code', []):
        if row.get('notice'):
            save('first-party/' + row['notice'], (root / row['notice']).read_bytes())
    fonts = root / 'tools/danoni-importer/licenses'
    provenance = json.loads((fonts / 'PROVENANCE.json').read_bytes())
    subset = sources / 'upstream' / provenance['subset']
    if sha(subset.read_bytes()) != provenance['subsetSha256']:
        raise ValueError('Pinned embedded font subset changed')
    for name in ('PROVENANCE.json', 'NotoSans-OFL.txt', 'NotoSansMath-OFL.txt'):
        save(f'embedded-fonts/{name}', (fonts / name).read_bytes())
    save('embedded-fonts/update-font.sh', (sources / 'upstream/core/assets/update-font.sh').read_bytes())
    receipt = {'schema': 'ddi-danoni-runtime-notices/1', 'target': target, 'locks': locks,
               'packages': sorted(packages.values(), key=lambda p: (p['name'], p['version'])),
               'artifacts': sorted(artifacts, key=lambda a: a['path']),
               'packagesWithoutCollectedText': [], 'redistributionApproved': True,
               'scope': 'Resolved normal runtime dependencies of the exact SDK CLI, worker and helper lock graphs; build/dev/proc-macro code excluded. Preserves upstream copyright/license notices, workspace-root notices and pinned embedded Noto font notices. All normal runtime license choices satisfy the project allowlist. System libraries/drivers are not redistributed.'}
    (output / 'manifest.json').write_bytes(encoded(receipt))
    return receipt
