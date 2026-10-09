#!/usr/bin/env python3
"""Rebuild reviewed local exports offline; never executes acquired work scripts."""
import argparse
from array import array
import math
import os
import wave
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import shutil
import subprocess
import uuid
import zipfile

from export_io import digest, write_json
from paths import external_directory
from prepare_pack import prepare

MODES = {'5', '7', '7i', '9A', '9B'}
SCHEMA = 'ddi-danoni-export-registry/1'


def journal_write(path, value):
    temporary = path.with_name(path.name + '.tmp')
    write_json(temporary, value)
    with temporary.open('rb') as file: os.fsync(file.fileno())
    os.replace(temporary, path)
    descriptor = os.open(path.parent, os.O_RDONLY)
    try: os.fsync(descriptor)
    finally: os.close(descriptor)


def slug(value):
    if not isinstance(value, str) or not re.fullmatch(r'[a-z0-9][a-z0-9_-]{0,79}', value):
        raise ValueError('Expected a safe work/collection id')
    return value


def checked(root, relative, sha=None):
    root = root.resolve(strict=True)
    if not isinstance(relative, str) or Path(relative).is_absolute():
        raise ValueError('Use a relative corpus path')
    path = (root / relative).resolve(strict=True)
    if not path.is_relative_to(root) or not path.is_file():
        raise ValueError('Input escapes corpus or is not a file')
    if sha is not None and digest(path.read_bytes()) != sha:
        raise ValueError(f'Changed input: {relative}')
    return path


def copy_tree(source, target):
    if target.exists():
        raise ValueError(f'Destination already exists: {target}')
    target.mkdir(parents=True)
    for path in sorted(source.rglob('*')):
        if path.is_symlink():
            raise ValueError('Symlinks are not preserved in evidence trees')
        if path.is_file():
            dest = target / path.relative_to(source)
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, dest)
            if digest(path.read_bytes()) != digest(dest.read_bytes()):
                raise ValueError('Copy hash mismatch')


def binding(root, path):
    return {'path': str(path.relative_to(root)), 'sha256': digest(path.read_bytes())}


def source_evidence(root, manifest_path):
    manifest = json.loads(manifest_path.read_text())
    if manifest.get('schema') == 'ddi-danoni-dump/1':
        files = {checked(manifest_path.parent, relative) for c in manifest['charts'] if c['keyMode'] in MODES
                 for relative in c['paths'].values() if isinstance(relative, str)}
    else:
        files = {p for p in manifest_path.parent.rglob('*') if p.is_file() and p != manifest_path}
    return [binding(root, p) for p in sorted(files)]


def preserve_motion_review(original, destination):
    """Retain original bytes and create a hash-bound, locally relocatable review."""
    raw = original.read_bytes()
    (destination.parent / 'native-scroll-review.original.json').write_bytes(raw)
    review = json.loads(raw)
    for ident, chart in review['charts'].items():
        slug(ident)
        evidence = chart['evidence']
        source = (original.parent / evidence['path']).resolve(strict=True)
        body = source.read_bytes()
        if digest(body) != evidence['sha256']: raise ValueError('Changed motion evidence')
        relative = 'motion/' + ident + '.json'
        target = destination.parent / relative; target.parent.mkdir(exist_ok=True)
        target.write_bytes(body)
        evidence['path'] = relative
    review['relocatedFromSha256'] = digest(raw)
    write_json(destination, review)


def bootstrap(root, registry):
    """Preserve existing evidence; this explicit migration never removes a file."""
    root = external_directory(root).resolve(strict=True)
    if registry.exists():
        raise ValueError('Registry must be new')
    old = root / 'legacy-pilot/exports'
    works = []
    for ident, folder, collection in [('kouhaku', '2004-kouhaku', 'kouhaku'), ('higan', 'autumn-higan-normal', 'single-songs')]:
        target = root / 'normalized' / ident
        copy_tree(old / folder, target)
        entry = {'id': ident, 'kind': 'normalized', 'collection': collection, 'required': True,
                 'source': binding(root, target / 'manifest.json'), 'evidence': source_evidence(root, target / 'manifest.json'), 'policy': {'audio': 'ogg'}}
        if ident == 'kouhaku':
            reviews = root / 'reviews/kouhaku'; reviews.mkdir(parents=True, exist_ok=False)
            for name, original in [('musical-grid-review.json', old / '2004-kouhaku-musical-grid-review.json'),
                                   ('native-scroll-review.json', root / 'legacy-pilot/motion/2004-kouhaku/native-scroll-review.json')]:
                if name == 'native-scroll-review.json': preserve_motion_review(original, reviews / name)
                else: shutil.copyfile(original, reviews / name)
            entry['policy'].update(musicalGrid=binding(root, reviews / 'musical-grid-review.json'),
                                   nativeScroll=binding(root, reviews / 'native-scroll-review.json'),
                                   displayBpms={'tougenkyo': '160', 'tougenkyo-hard': '160', 'run': '175',
                                                'kitakaze': '110', 'korokke': '150', 'emy': '200'})
            golden = reviews / 'accepted-pack'; golden.mkdir()
            accepted = old / '2004-kouhaku-pickup-previews-v2'
            shutil.copyfile(accepted / 'manifest.json', golden / 'manifest.json')
            copy_tree(accepted / 'songs', golden / 'songs')
            for path in accepted.glob('*.json'):
                if path.name != 'manifest.json': shutil.copyfile(path, golden / path.name)
            entry['baseline'] = binding(root, golden / 'manifest.json')
        works.append(entry)
    selected = {'jam-with-pancake', 'straight-soul-html5', 'vh-dancing-at-dusk', 'vh-canaria-7i', 'vh-hikari-9ab'}
    for path in sorted((root / 'corpus').glob('*/manifest.json')):
        manifest = json.loads(path.read_text())
        if manifest.get('schema') != 'ddi-danoni-dump/1': continue
        ident = slug(path.parent.name)
        entry = {'id': ident, 'source': binding(root, path), 'collection': 'single-songs', 'required': ident in selected}
        if ident in selected:
            entry.update(kind='cw-dump', verification=binding(root, path.parent / 'verification.json'), evidence=source_evidence(root, path))
        else:
            entry.update(kind='pending', reason='Incomplete capture or unsupported key definitions/modes; needs separate review')
        works.append(entry)
    inventory = root / 'single-song-corpus-v1/manifest.json'
    native = json.loads(inventory.read_text())
    for work in native['works']:
        source = root / 'single-song-corpus-v1' / work['manifest']
        works.append({'id': slug(work['id']), 'kind': 'pending', 'source': binding(root, source),
                      'required': False, 'collection': 'single-songs', 'reason': 'Title captured; gameplay projection/audio assembly not validated'})
    for ident in ['autumn-destined-marionette', 'cw-straight-soul']:
        source = root / 'legacy-pilot/inputs' / ident / 'provenance.json'
        works.append({'id': ident, 'kind': 'pending', 'source': binding(root, source), 'required': False,
                      'collection': 'single-songs', 'reason': 'Legacy original acquired; native gameplay projection not validated'})
    document = {'schema': SCHEMA, 'createdAt': datetime.now(timezone.utc).isoformat(), 'root': str(root),
                'collections': [{'id': 'kouhaku', 'title': '紅白FLASH合戦 2004'},
                                {'id': 'single-songs', 'title': 'DanOni Single Songs'}], 'works': works}
    registry.parent.mkdir(parents=True, exist_ok=True); write_json(registry, document)
    return document


def cw_dump(source, destination, verification):
    raw = source.read_bytes(); manifest = json.loads(raw); root = source.parent
    if manifest.get('schema') != 'ddi-danoni-dump/1' or not verification.get('passed'):
        raise ValueError('Need a successfully verified CW dump')
    charts = [c for c in manifest['charts'] if c['keyMode'] in MODES]
    if not charts or any(c['status'] != 'complete' for c in charts):
        raise ValueError('Supported captured charts must be complete')
    compared = {c['chartId'] for c in verification['comparisons'] if c['referenceMatched']}
    if any(c['chartId'] not in compared for c in charts): raise ValueError('Missing oracle verification')
    destination.mkdir(parents=True)
    paths = set(); mappings = []; references = []
    for chart in charts:
        for field in ['effectiveDos', 'keys', 'reference', 'snapshots']:
            relative = chart['paths'][field]; checked(root, relative); paths.add(relative)
        reference = json.loads(checked(root, chart['paths']['reference']).read_text())
        if reference.get('chartId') != chart['chartId']: raise ValueError('Wrong reference chart id')
        references.append({'id': chart['chartId'], 'mode': chart['keyMode'], 'reference': chart['paths']['reference']})
        candidates = [a for a in manifest['assets'] if a['role'] == 'music' and chart['chartId'] in a.get('chartIds', [])]
        if len(candidates) != 1: raise ValueError('Need exactly one captured music binding per chart')
        asset = candidates[0]; relative = asset.get('decodedPath') or asset['path']
        sha = asset.get('decodedSha256') if asset.get('decodedPath') else asset['sha256']
        if not sha: raise ValueError('Captured media needs a hash')
        checked(root, relative, sha); paths.add(relative); mappings.append({'chartId': chart['chartId'], 'path': relative, 'sha256': sha})
    for relative in sorted(paths):
        target = destination / relative; target.parent.mkdir(parents=True, exist_ok=True)
        original = checked(root, relative)
        shutil.copyfile(original, target)
        if digest(original.read_bytes()) != digest(target.read_bytes()): raise ValueError('Copy hash mismatch')
    # Keep the untouched source manifest separately; the import manifest describes
    # this data package rather than claiming a complete browser replay archive.
    (destination / 'source-manifest.json').write_bytes(raw)
    packaged = {**manifest, 'charts': charts, 'assets': [a for a in manifest['assets'] if a['role'] == 'music' and any(c['chartId'] in a.get('chartIds', []) for c in charts)],
                'status': 'partial', 'export': {'schema': 'ddi-cw-import-package/1', 'sourceManifestSha256': digest(raw),
                'scope': 'Complete selected importer data/music; browser scripts/fonts/presentation remain in source corpus'}}
    write_json(destination / 'manifest.json', packaged)
    return {'charts': references, 'music': mappings, 'workStatus': manifest['status'], 'format': 'native-cw-dump',
            'scope': 'Captured effective dos and music; original source effects remain data, browser custom presentation is not replayed'}


def audio_check(prepared, ffmpeg):
    """Independent decode and sampled correlation; no browser is needed."""
    report = json.loads((prepared / 'manifest.json').read_text()); results = []
    for song in report['songs']:
        reference = prepared / 'references' / (song['id'] + '.wav')
        decoded = prepared / 'references' / (song['id'] + '-verify.wav')
        subprocess.run([str(ffmpeg), '-nostdin', '-v', 'error', '-i', str(prepared / song['music']),
                        '-c:a', 'pcm_s16le', str(decoded)], check=True, timeout=30)
        def pcm(path):
            with wave.open(str(path), 'rb') as f:
                spec = (f.getframerate(), f.getnchannels(), f.getnframes(), f.getsampwidth())
                if spec[3] != 2: raise ValueError('Expected PCM16')
                values = array('h', f.readframes(spec[2]))
                if __import__('sys').byteorder != 'little': values.byteswap()
                return spec, values
        a, x = pcm(reference); b, y = pcm(decoded)
        if a != b: raise ValueError('Encoded audio changed decoded sample count/format')
        rate, channels, count, _ = a; windows = []
        length = min(rate, max(1, (count - 64) // 3))
        for channel in range(channels):
            for part in range(3):
                low = max(32, part * count // 3); high = min(count - length - 32, (part + 1) * count // 3 - length)
                candidates = range(low, high + 1, max(1, length // 2))
                def energy(at): return sum(x[(at + i) * channels + channel] ** 2 for i in range(0, length, 16))
                at = max(candidates, key=energy, default=None)
                if at is None or energy(at) == 0: continue
                best = (-1., None)
                for lag in range(-32, 33):
                    xy = xx = yy = 0
                    for i in range(0, length, 16):
                        u = x[(at + i) * channels + channel]; v = y[(at + i + lag) * channels + channel]
                        xy += u * v; xx += u * u; yy += v * v
                    corr = xy / math.sqrt(xx * yy) if xx and yy else -1
                    if corr > best[0]: best = (corr, lag)
                if best[0] < .97 or abs(best[1]) > 1: raise ValueError('Audio correlation/origin verification failed')
                windows.append({'channel': channel, 'part': part, 'startSeconds': at / rate,
                                'correlation': best[0], 'lagSamples': best[1]})
        if not windows: raise ValueError('No energetic audio window to verify')
        decoded.unlink()
        results.append({'song': song['id'], 'sampleRate': rate, 'channels': channels, 'frames': count, 'windows': windows})
    result = {'schema': 'ddi-audio-origin-comparison/native-1', 'passed': True, 'comparisons': results,
              'scope': 'Independent FFmpeg decode, exact sample counts and sampled energetic-window correlation; original Flash/device latency remains unmeasured'}
    write_json(prepared / 'audio-verification.json', result)
    return result


def write_zip(folder, target):
    with zipfile.ZipFile(target, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(folder.rglob('*')):
            if not path.is_file(): continue
            info = zipfile.ZipInfo(path.relative_to(folder).as_posix(), (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED; info.external_attr = 0o100644 << 16
            archive.writestr(info, path.read_bytes())


def build(registry, stage, ffmpeg, encoder, verifier):
    registry = registry.resolve(strict=True); raw = registry.read_bytes(); config = json.loads(raw)
    if config.get('schema') != SCHEMA: raise ValueError('Unsupported registry schema')
    root = external_directory(config['root']).resolve(strict=True)
    stage = external_directory(stage)
    protected = [root / name for name in ('normalized', 'reviews', 'corpus', 'single-song-corpus-v1', 'legacy-pilot', 'exports', 'exports-history')]
    if stage == root or any(stage.is_relative_to(p) or p.is_relative_to(stage) for p in protected):
        raise ValueError('Staging directory must not overlap evidence/reviews')
    stage.mkdir(parents=True, exist_ok=False)
    collections = {}; results = []; ids = set(); failures = []
    for collection in config['collections']:
        ident = slug(collection['id']); title = collection['title']
        if ident in collections or not isinstance(title, str) or title in ('', '.', '..') or any(c in title for c in '/\\\x00\r\n'):
            raise ValueError('Unsafe/duplicate collection')
        collections[ident] = title
        (stage / ident / 'files' / title).mkdir(parents=True)
    report = {'schema': 'ddi-danoni-export-generation/1', 'generatedAt': datetime.now(timezone.utc).isoformat(),
              'registrySha256': digest(raw), 'registry': str(registry), 'builderSha256': digest(Path(__file__).read_bytes()),
              'sourceRoot': str(root), 'networkRequests': 0, 'browserLaunches': 0, 'works': results, 'passed': False}
    for entry in config['works']:
        ident = slug(entry['id']); collection = entry['collection']
        if ident in ids or collection not in collections: raise ValueError('Duplicate work or unknown collection')
        ids.add(ident)
        if type(entry.get('required', False)) is not bool: raise ValueError('required must be boolean')
        result = {'id': ident, 'kind': entry['kind'], 'required': entry.get('required', False), 'collection': collection}
        results.append(result)
        try:
            source = checked(root, entry['source']['path'], entry['source']['sha256'])
            result['source'] = str(source); result['sourceSha256'] = entry['source']['sha256']
            if entry['kind'] == 'pending':
                if result['required']: raise ValueError('Required work cannot be pending')
                result.update(status='pending', reason=entry['reason']); continue
            if not isinstance(entry.get('evidence'), list): raise ValueError('Recipe must bind its source evidence files')
            for evidence in entry['evidence']: checked(root, evidence['path'], evidence['sha256'])
            result['evidence'] = entry['evidence']
            destination = stage / collection / 'files' / collections[collection] / ident
            if entry['kind'] == 'normalized':
                policy = entry.get('policy', {})
                def review(name):
                    b = policy.get(name)
                    return checked(root, b['path'], b['sha256']) if b else None
                prepared = stage / 'reports' / ident
                native = prepare(source.parent, prepared, policy.get('audio', 'ogg'), ffmpeg, encoder,
                                 display_bpms=policy.get('displayBpms'), musical_grid=review('musicalGrid'),
                                 native_scroll=review('nativeScroll'), scroll_factor=policy.get('scrollFactor', 1))
                audio_check(prepared, ffmpeg)
                (prepared / 'playable.zip').unlink()
                if entry.get('baseline'):
                    b = entry['baseline']; baseline = checked(root, b['path'], b['sha256'])
                    golden = json.loads(baseline.read_text())
                    for song in native['songs']:
                        old = next(row for row in golden['songs'] if row['id'] == song['id'])
                        for field in ('simfile', 'music'):
                            if (prepared / song[field]).read_bytes() != (baseline.parent / old[field]).read_bytes():
                                raise ValueError('Accepted Kouhaku chart/audio bytes changed')
                for song in native['songs']:
                    folder = stage / collection / 'files' / collections[collection] / (ident + '--' + song['id'])
                    copy_tree(prepared / 'songs' / song['id'], folder)
                result.update(format='ssc', prepared=str(prepared.relative_to(stage)),
                              source= str(source), charts=sum(len(s['charts']) for s in native['songs']), songs=len(native['songs']),
                              omittedEffects=[{'chart': c['id'], 'effects': c['unrepresentedEffects']} for s in native['songs'] for c in s['charts'] if c['unrepresentedEffects']])
                if entry.get('baseline'):
                    b = entry['baseline']; baseline = checked(root, b['path'], b['sha256'])
                    result['baseline'] = str(baseline)
            elif entry['kind'] == 'cw-dump':
                b = entry['verification']; v = json.loads(checked(root, b['path'], b['sha256']).read_text())
                result.update(cw_dump(source, destination, v), dump=str((destination / 'manifest.json').relative_to(stage)))
            else: raise ValueError('Unknown recipe kind')
            result['status'] = 'built'
        except Exception as error:
            result.update(status='failed', error=f'{type(error).__name__}: {error}')
            # Isolate any partial generated files from importable collections.
            folder = stage / collection / 'files' / collections[collection]
            for partial in list(folder.iterdir()):
                if partial.name == ident or partial.name.startswith(ident + '--'):
                    failed = stage / 'failed' / ident / partial.name; failed.parent.mkdir(parents=True, exist_ok=True); partial.rename(failed)
            if result['required']: failures.append(ident)
    report['failedRequiredWorks'] = failures
    if not any(w.get('status') == 'built' for w in results): failures.append('no-built-works')
    write_json(stage / 'index.json', report)
    if failures: raise ValueError(f'Required works failed: {failures}; staging retained')
    verification = subprocess.run([str(verifier), str(stage)], capture_output=True, text=True, timeout=60)
    (stage / 'validation.jsonl').write_text(verification.stdout)
    (stage / 'validation-stderr.txt').write_text(verification.stderr)
    if verification.returncode: raise ValueError('Game parser verification failed; staging retained')
    report['verifierSha256'] = digest(verifier.read_bytes())
    report['artifacts'] = []
    for ident in collections:
        archive = stage / ident / (collections[ident] + '.zip'); write_zip(stage / ident / 'files', archive)
        report['artifacts'].append({'collection': ident, 'path': str(archive.relative_to(stage)), 'sha256': digest(archive.read_bytes())})
    report['files'] = [{'path': str(f.relative_to(stage)), 'sha256': digest(f.read_bytes())} for f in sorted(stage.rglob('*')) if f.is_file() and f != stage / 'index.json']
    report['passed'] = True; write_json(stage / 'index.json', report)
    return report


def validate_generation(stage):
    report = json.loads((stage / 'index.json').read_text())
    if report.get('schema') != 'ddi-danoni-export-generation/1' or report.get('passed') is not True:
        raise ValueError('Only a validated generation can be promoted')
    if report.get('failedRequiredWorks') or any(w.get('required') and w.get('status') != 'built' for w in report['works']):
        raise ValueError('Required work is not validated')
    actual = set()
    for path in stage.rglob('*'):
        if path.is_symlink(): raise ValueError('Generation contains symlinks')
        if path.is_file() and path != stage / 'index.json': actual.add(str(path.relative_to(stage)))
    listed = [f['path'] for f in report['files']]
    if len(set(listed)) != len(listed) or actual != set(listed): raise ValueError('Generation inventory changed')
    for file in report['files']: checked(stage, file['path'], file['sha256'])
    for artifact in report['artifacts']: checked(stage, artifact['path'], artifact['sha256'])
    validations = [json.loads(line) for line in (stage / 'validation.jsonl').read_text().splitlines()]
    if {v['work'] for v in validations if v['validation']['passed']} != {w['id'] for w in report['works'] if w['status'] == 'built'}:
        raise ValueError('Missing game-parser validation')
    return report


def disjoint(*paths):
    for i, a in enumerate(paths):
        for b in paths[i + 1:]:
            if a.is_relative_to(b) or b.is_relative_to(a): raise ValueError('Paths must be disjoint')


def recover(journal):
    """Finish a completed rename or roll back an interrupted pre-promotion swap."""
    journal = external_directory(journal).resolve(strict=True)
    state = json.loads(journal.read_text())
    if state.get('schema') != 'ddi-danoni-promotion/1': raise ValueError('Unknown journal')
    stage, destination, archive = [external_directory(state[k]) for k in ('stage', 'destination', 'archive')]
    disjoint(stage, destination, archive)
    if (destination / 'index.json').is_file() and digest((destination / 'index.json').read_bytes()) == state['generationSha256']:
        validate_generation(destination); state['phase'] = 'promoted'
    elif stage.exists():
        if digest((stage / 'index.json').read_bytes()) != state['generationSha256']: raise ValueError('Staging generation changed')
        validate_generation(stage)
        if archive.exists():
            if destination.exists(): raise ValueError('Both old locations exist; inspect journal manually')
            archive.rename(destination)
        state['phase'] = 'rolled-back'
    else: raise ValueError('Cannot establish a safe recovery; inspect journal manually')
    journal_write(journal, state)
    return state


def retire(old, generation, history):
    """Move the old mixed exports after successful promotion, preserving every byte."""
    old = external_directory(old).resolve(strict=True)
    generation = external_directory(generation).resolve(strict=True)
    history = external_directory(history)
    disjoint(old, generation, history)
    report = validate_generation(generation)
    raw_registry = Path(report['registry']).read_bytes()
    if digest(raw_registry) != report['registrySha256']: raise ValueError('Generation registry changed before retirement')
    registry = json.loads(raw_registry)
    root = Path(registry['root']).resolve(strict=True)
    for work in registry['works']:
        for b in [work['source'], work.get('baseline'), work.get('verification'), *work.get('evidence', []), *[work.get('policy', {}).get(k) for k in ('musicalGrid', 'nativeScroll')]]:
            if b and checked(root, b['path'], b['sha256']).is_relative_to(old): raise ValueError('Registry still depends on old exports')
    history.mkdir(parents=True, exist_ok=True)
    ident = 'legacy-mixed-' + datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8]
    archive = history / ident; journal = history / (ident + '.json')
    if any(p.is_symlink() for p in old.rglob('*')): raise ValueError('Old exports contain symlinks; inspect before retirement')
    files = [{'path': str(p.relative_to(old)), 'sha256': digest(p.read_bytes())} for p in sorted(old.rglob('*')) if p.is_file()]
    state = {'schema': 'ddi-danoni-retirement/1', 'source': str(old), 'archive': str(archive), 'phase': 'prepared', 'files': files}
    journal_write(journal, state)
    old.rename(archive)  # Single atomic directory rename; journal always records both paths.
    for item in files: checked(archive, item['path'], item['sha256'])
    state['phase'] = 'archived'; journal_write(journal, state)
    return state


def promote(stage, destination, history):
    """Archive before replacement; a durable journal supports interrupted swaps."""
    stage = external_directory(stage).resolve(strict=True); destination = external_directory(destination)
    history = external_directory(history)
    disjoint(stage, destination, history)
    validate_generation(stage)
    destination.parent.mkdir(parents=True, exist_ok=True); history.mkdir(parents=True, exist_ok=True)
    ident = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8]
    archive = history / ident; journal = history / (ident + '.json')
    state = {'schema': 'ddi-danoni-promotion/1', 'stage': str(stage), 'destination': str(destination),
             'archive': str(archive), 'phase': 'prepared', 'generationSha256': digest((stage / 'index.json').read_bytes())}
    journal_write(journal, state)
    moved = False
    try:
        if destination.exists(): destination.rename(archive); moved = True
        state['phase'] = 'old-archived'; journal_write(journal, state)
        stage.rename(destination)
        state['phase'] = 'promoted'; journal_write(journal, state)
    except Exception:
        if moved and not destination.exists(): archive.rename(destination)
        state['phase'] = 'rolled-back' if destination.exists() else 'interrupted'; journal_write(journal, state)
        raise
    return state


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__); sub = parser.add_subparsers(dest='command', required=True)
    init = sub.add_parser('bootstrap'); init.add_argument('root', type=Path); init.add_argument('registry', type=Path)
    run = sub.add_parser('build'); run.add_argument('registry', type=Path); run.add_argument('stage', type=Path)
    run.add_argument('--ffmpeg', type=Path, default=shutil.which('ffmpeg')); run.add_argument('--vorbis-encoder', type=Path, required=True); run.add_argument('--verifier', type=Path, required=True)
    swap = sub.add_parser('promote'); swap.add_argument('stage', type=Path); swap.add_argument('destination', type=Path); swap.add_argument('history', type=Path)
    undo = sub.add_parser('recover'); undo.add_argument('journal', type=Path)
    archive = sub.add_parser('retire'); archive.add_argument('old', type=Path); archive.add_argument('generation', type=Path); archive.add_argument('history', type=Path)
    args = parser.parse_args()
    if args.command == 'bootstrap': result = bootstrap(args.root, external_directory(args.registry))
    elif args.command == 'build': result = build(args.registry, args.stage, args.ffmpeg, args.vorbis_encoder, args.verifier)
    elif args.command == 'promote': result = promote(args.stage, args.destination, args.history)
    elif args.command == 'recover': result = recover(args.journal)
    else: result = retire(args.old, args.generation, args.history)
    print(json.dumps({'command': args.command, 'passed': result.get('passed'), 'works': len(result.get('works', [])), 'phase': result.get('phase')}))
