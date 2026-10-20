#!/usr/bin/env python3
"""Build and package the native importer; never installed or called at runtime."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import shlex
import re
import subprocess
import sys
sys.dont_write_bytecode = True
import tempfile
import time

from notices import collect, encoded, sha

ROOT = Path(__file__).resolve().parents[2]
REVISION = '89a7049965f3090e706265670df309a5128233ac'
REPOSITORY = 'https://github.com/ruffle-rs/ruffle.git'
MAX_FILE = 256 * 1024 * 1024
MAX_TOTAL = 512 * 1024 * 1024


def run(command, env, capture=False, cwd=ROOT):
    print('+ ' + ' '.join(str(x) for x in command), file=sys.stderr, flush=True)
    process = subprocess.Popen([str(x) for x in command], cwd=cwd, env=env,
                               stdout=subprocess.PIPE if capture else sys.stderr,
                               stderr=sys.stderr, start_new_session=True)
    start = time.monotonic()
    try:
        while process.poll() is None:
            lines = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,rss=,comm='], text=True)
            rows = {}
            for line in lines.splitlines():
                fields = line.split(None, 3)
                if len(fields) == 4:
                    rows[int(fields[0])] = (int(fields[1]), int(fields[2]), fields[3])
            children = {process.pid}
            for _ in range(32):
                grown = children | {pid for pid, (parent, _, _) in rows.items() if parent in children}
                if grown == children:
                    break
                children = grown
            excessive = [rows[pid] for pid in children if pid in rows and rows[pid][1] > 2097152]
            if excessive:
                raise RuntimeError(f'Build process exceeded 2 GiB RSS: {excessive}')
            if time.monotonic() - start > 2400:
                raise RuntimeError('Build command exceeded 40 minutes')
            time.sleep(0.5)
        if process.returncode:
            raise subprocess.CalledProcessError(process.returncode, command)
        return process.stdout.read() if capture else b''
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()


def inventory(root):
    result = []
    total = 0
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            raise ValueError(f'Symlink in bundle: {path.name}')
        if path.is_dir():
            continue
        size = path.stat().st_size
        total += size
        if size > MAX_FILE or total > MAX_TOTAL or len(result) >= 9999:
            raise ValueError('Bundle exceeds runtime inventory limits')
        result.append({'path': path.relative_to(root).as_posix(), 'bytes': size, 'sha256': sha(path.read_bytes())})
    return result


def verify(bundle):
    raw = (bundle / 'manifest.json').read_bytes()
    if len(raw) > 4 * 1024 * 1024:
        raise ValueError('Oversized bundle manifest')
    manifest = json.loads(raw)
    actual = [row for row in inventory(bundle) if row['path'] != 'manifest.json']
    if (manifest['schema'] != 'ddi-danoni-app-bundle/1'
            or manifest['executable'] != 'bin/ddi-danoni-importer'
            or manifest['protocolVersion'] != 1
            or manifest['artifacts'] != actual):
        raise ValueError('Incomplete or changed native importer bundle')
    identity = dict(manifest)
    identity.pop('id')
    if manifest['id'] != sha(encoded(identity)):
        raise ValueError('Bundle content identity changed')
    if len(manifest['id']) != 64:
        raise ValueError('Invalid bundle identity')
    notices = json.loads((bundle / 'runtime/notices/manifest.json').read_bytes())
    if not notices.get('redistributionApproved') or notices.get('packagesWithoutCollectedText'):
        raise ValueError('Runtime distribution notices are incomplete')
    return manifest, sha(raw)


def external(path):
    path = path.resolve()
    if any((parent / '.git').exists() for parent in (path, *path.parents)):
        raise ValueError('Build cache/upstream must be outside Git checkouts')
    return path


def cargo_env(cache):
    env = dict(os.environ)
    for key in ('CARGO_TARGET_DIR', 'CARGO_MAKEFLAGS', 'MAKEFLAGS', 'MFLAGS', 'CARGO_ENCODED_RUSTFLAGS'):
        env.pop(key, None)
    for key in list(env):
        if key.startswith('CARGO_PROFILE_'):
            env.pop(key)
    env.update(CARGO_BUILD_JOBS='2', CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
               CARGO_PROFILE_RELEASE_DEBUG='0', CARGO_PROFILE_RELEASE_LTO='false',
               CARGO_PROFILE_RELEASE_OPT_LEVEL='1', CARGO_PROFILE_RELEASE_CODEGEN_UNITS='256',
               CARGO_PROFILE_RELEASE_STRIP='debuginfo')
    caller_flags = os.environ.get('CARGO_ENCODED_RUSTFLAGS')
    flags = caller_flags.split('\x1f') if caller_flags else shlex.split(os.environ.get('RUSTFLAGS', ''))
    flags.extend([f'--remap-path-prefix={ROOT}=/ddi/source', f'--remap-path-prefix={cache}=/ddi/build'])
    env.pop('RUSTFLAGS', None)
    env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(flags)
    return env


def cargo(manifest, target_dir, target, env, profile='release', packages=()):
    build_env = dict(env, CARGO_TARGET_DIR=str(target_dir))
    command = ['cargo', 'build', '--locked', '--profile', profile, '--target', target,
               '--manifest-path', manifest]
    for package in packages:
        command.extend(['--package', package])
    run(command, build_env)


def compiler_identity(target):
    digest = hashlib.sha256(target.encode())
    digest.update(subprocess.check_output(['rustc', '-vV']))
    keys = {'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTC_WRAPPER',
            'RUSTC_WORKSPACE_WRAPPER', 'CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS',
            'MACOSX_DEPLOYMENT_TARGET', 'SDKROOT'}
    keys.update(f'CARGO_TARGET_{target.upper().replace("-", "_")}_{suffix}' for suffix in ('LINKER', 'RUSTFLAGS'))
    keys.update(f'{prefix}_{suffix}' for prefix in ('CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS')
                for suffix in (target, target.replace('-', '_')))
    keys.update(f'{prefix}_{suffix}' for prefix in ('HOST', 'TARGET')
                for suffix in ('CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS'))
    digest.update(encoded({key: os.environ[key] for key in keys if key in os.environ}))
    return digest.digest()


def fingerprint(target):
    digest = hashlib.sha256(compiler_identity(target))
    paths = ['Cargo.toml', 'Cargo.lock', 'chart', 'engine', 'library', 'platform', 'importer-protocol',
             'tools/danoni-importer', 'tools/danoni-dump/rust', 'tools/danoni-dump/ruffle', 'LICENSES', 'REUSE.toml', 'THIRD_PARTY.toml', 'CREDITS.md']
    for relative in paths:
        path = ROOT / relative
        files = [path] if path.is_file() else sorted(path.rglob('*'))
        for file in files:
            if (not file.is_file() or any(part in ('target', '.git', '__pycache__') for part in file.relative_to(ROOT).parts)):
                continue
            digest.update(file.relative_to(ROOT).as_posix().encode() + b'\0')
            digest.update(file.read_bytes())
    for key in ('DDI_IMPORTER_WORKER_BUNDLE', 'DDI_IMPORTER_WORKER_SOURCES'):
        if os.environ.get(key):
            digest.update((Path(os.environ[key]) / 'manifest.json').read_bytes())
    return digest.hexdigest()


def native_dependencies(binary, target):
    if 'apple-darwin' in target:
        out = subprocess.check_output(['otool', '-L', str(binary)], text=True)
        for line in out.splitlines()[1:]:
            library = line.strip().split(' ', 1)[0]
            if not library.startswith(('/System/Library/', '/usr/lib/')):
                raise ValueError(f'Unpackaged dynamic dependency in {binary.name}: {library}')
        return out.replace(str(binary), binary.name)
    out = subprocess.check_output(['ldd', str(binary)], text=True)
    if 'not found' in out:
        raise ValueError('Missing native dependency: ' + out)
    allowed = ('linux-vdso', 'libc.so', 'libm.so', 'libdl.so', 'libpthread.so',
               'librt.so', 'libgcc_s.so', 'ld-linux', 'libstdc++.so', 'libasound.so', 'libudev.so')
    for line in out.splitlines():
        name = line.strip().split(' ', 1)[0].rsplit('/', 1)[-1]
        if name and not name.startswith(allowed):
            raise ValueError(f'Unpackaged non-system dynamic dependency in {binary.name}: {line.strip()}')
        if '=>' in line:
            location = line.split('=>', 1)[1].strip().split(' ', 1)[0]
            if not location.startswith(('/lib/', '/lib64/', '/usr/lib/', '/usr/lib64/')):
                raise ValueError(f'Custom system-library location cannot be redistributed implicitly: {location}')
    return re.sub(r'\s+\(0x[0-9a-f]+\)', '', out.replace(str(binary), binary.name))


def upstream(cache, env):
    configured = os.environ.get('DDI_IMPORTER_UPSTREAM')
    if configured:
        return Path(configured).resolve(strict=True)
    path = cache / 'ruffle'
    if not (path / '.git').exists():
        if env.get('CARGO_NET_OFFLINE') == 'true':
            raise ValueError('Offline build needs the pinned upstream already cached; DDI_IMPORTER_UPSTREAM is a build-only override')
        run(['git', 'clone', '--filter=blob:none', '--no-checkout', '--depth', '1', '--branch', 'v0.7.1', REPOSITORY, path], env)
        run(['git', '-C', path, 'sparse-checkout', 'init', '--cone'], env)
        run(['git', '-C', path, 'sparse-checkout', 'set', 'core', 'render', 'swf', 'flv', 'wstr', 'video', 'tools/asc'], env)
        run(['git', '-C', path, 'checkout', '--detach', REVISION], env)
    actual = subprocess.check_output(['git', '-C', path, 'rev-parse', 'HEAD'], text=True).strip()
    if actual != REVISION:
        raise ValueError('Pinned Ruffle checkout has a different commit')
    return path


def prepare(target, stage):
    host = subprocess.check_output(['rustc', '-vV'], text=True).split('host: ')[1].splitlines()[0]
    if target != host or not ('apple-darwin' in target or target == 'x86_64-unknown-linux-gnu'):
        raise ValueError('Prepare on a supported native macOS or x86_64 Linux host')
    cache = external(Path(os.environ.get('DDI_IMPORTER_BUILD_CACHE', Path.home() / '.cache/ditoditoinfinito/danoni-importer')))
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / 'build.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        key = fingerprint(target)
        key_dir = cache / 'builds' / key
        receipt = key_dir / 'ready.json'
        if receipt.exists():
            content = json.loads(receipt.read_bytes())
            bundle = cache / 'bundles' / content['id']
            manifest, digest = verify(bundle)
        else:
            env = cargo_env(cache)
            cli_manifest = ROOT / 'tools/danoni-dump/rust/Cargo.toml'
            helper_manifest = ROOT / 'tools/danoni-importer/Cargo.toml'
            # Select only these two packages: never recurse into desktop/build.rs.
            # Both executables share one lock, feature graph and compilation cache.
            cargo(ROOT / 'Cargo.toml', cache / 'importer-target', target, env,
                  profile='importer', packages=('ddi-danoni-export', 'ddi-danoni-importer'))
            binaries = cache / 'importer-target' / target / 'importer'
            cli = binaries / 'ddi-danoni-export'
            helper = binaries / 'ddi-danoni-importer'
            worker_key = sha(compiler_identity(target) + b''.join(p.read_bytes() for p in sorted((ROOT / 'tools/danoni-dump/ruffle').rglob('*')) if p.is_file() and '__pycache__' not in p.parts))
            worker_root = cache / 'workers' / worker_key
            worker_root.mkdir(parents=True, exist_ok=True)
            source_override = os.environ.get('DDI_IMPORTER_WORKER_SOURCES')
            sources = Path(source_override).resolve(strict=True) if source_override else worker_root / 'sources'
            if not sources.exists():
                run([cli, 'prepare-worker', upstream(cache, env), sources], env)
            bundle_override = os.environ.get('DDI_IMPORTER_WORKER_BUNDLE')
            worker = Path(bundle_override).resolve(strict=True) if bundle_override else worker_root / 'bundle'
            if not worker.exists():
                # Cargo's jobserver also bounds rustc's parallel LLVM work.
                # Ruffle exceeds the per-process memory limit with two slots;
                # serialize this build while retaining its optimization level.
                worker_env = dict(env, CARGO_BUILD_JOBS='1')
                cargo(sources / 'worker/Cargo.toml', cache / 'worker-target', target, worker_env)
                executable = cache / 'worker-target' / target / 'release/ddi-ruffle-probe'
                job = worker_root / 'package.json'
                job.write_bytes(encoded({'schema': 'ddi-danoni-worker-package-job/1', 'sources': str(sources),
                                         'executable': str(executable), 'executableSha256': sha(executable.read_bytes())}))
                run([cli, 'package-worker', job, worker], env)
            run([cli, 'verify-worker', worker], env)
            worker_manifest = json.loads((worker / 'manifest.json').read_bytes())
            if worker_manifest['sourceManifestSha256'] != sha((sources / 'manifest.json').read_bytes()):
                raise ValueError('Worker and notice source preparation differ')
            key_dir.parent.mkdir(parents=True, exist_ok=True)
            work = Path(tempfile.mkdtemp(prefix='prepare-', dir=cache))
            try:
                notices = work / 'notices'
                collect(ROOT, sources, [('sdk', cli_manifest), ('worker', sources / 'worker/Cargo.toml'), ('helper', helper_manifest)], notices, target, env)
                package = work / 'bundle'
                package.mkdir()
                run([cli, 'install-runtime', worker, notices, package / 'runtime'], env)
                (package / 'bin').mkdir()
                shutil.copy2(helper, package / 'bin/ddi-danoni-importer')
                native = {name: native_dependencies(binary, target) for name, binary in [('helper', helper), ('sdk', cli), ('worker', worker / 'ddi-ruffle-probe')]}
                manifest = {'schema': 'ddi-danoni-app-bundle/1', 'protocolVersion': 1, 'target': target,
                            'executable': 'bin/ddi-danoni-importer', 'artifacts': inventory(package),
                            'systemPrerequisites': 'macOS system frameworks; on Linux system glibc/libgcc plus the Vulkan loader and a Mesa software ICD. No Java, Python, Cargo, FFmpeg command, source checkout, host.json or home-path binding is used at runtime.',
                            'nativeDependencies': native}
                manifest['id'] = sha(encoded(manifest))
                (package / 'manifest.json').write_bytes(encoded(manifest))
                verify(package)
                run([package / 'bin/ddi-danoni-importer', '--check-bundle'], env)
                bundle = cache / 'bundles' / manifest['id']
                bundle.parent.mkdir(exist_ok=True)
                if bundle.exists():
                    verify(bundle)
                else:
                    package.rename(bundle)
                key_dir.mkdir(exist_ok=True)
                receipt.write_bytes(encoded({'id': manifest['id']}))
                digest = sha((bundle / 'manifest.json').read_bytes())
            finally:
                shutil.rmtree(work)
        destination = stage.resolve() / 'danoni-importer' / manifest['id']
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.exists():
            temporary = Path(tempfile.mkdtemp(prefix='.stage-', dir=destination.parent))
            try:
                shutil.copytree(bundle, temporary / 'bundle')
                (temporary / 'bundle').rename(destination)
            finally:
                shutil.rmtree(temporary)
        staged, staged_hash = verify(destination)
        if staged != manifest or staged_hash != digest:
            raise ValueError('Staged importer differs from the compiled binding')
        print(manifest['id'])
        print(digest)


def package(app, output):
    app = app.resolve(strict=True)
    env = dict(os.environ)
    info = json.loads(run([app, '--danoni-bundle-info'], env, capture=True))
    if not info.get('enabled') or not re_hash(info.get('id')) or not re_hash(info.get('manifestSha256')):
        raise ValueError('Application has no prepared production importer binding')
    source = app.parent / 'danoni-importer' / info['id']
    manifest, digest = verify(source)
    if digest != info['manifestSha256']:
        raise ValueError('Application and staged importer differ')
    app_dependencies = native_dependencies(app, manifest['target'])
    if output.exists():
        raise ValueError('Package output must be a fresh directory')
    output.mkdir(parents=True)
    try:
        shutil.copy2(app, output / app.name)
        shutil.copytree(source, output / 'danoni-importer' / manifest['id'])
        shutil.copytree(ROOT / 'assets', output / 'assets')
        for name in ['songs', 'noteskins']:
            index = ROOT / 'target' / name / 'index.json'
            if not index.is_file():
                raise ValueError(f'Generate {name} index with cargo run -p xtask -- gen-{name} before packaging')
            shutil.copy2(index, output / 'assets' / name / 'index.json')
        # The sandboxed decoder helper desktop/build.rs staged beside the app
        # (video/ddi-video-helper), with its notice and FFmpeg's description.
        exe = 'ddi-video-helper.exe' if manifest['target'].endswith('windows-msvc') else 'ddi-video-helper'
        helper = app.parent / 'video' / exe
        if info.get('video', True) and helper.is_file():
            import tomllib
            video_config = tomllib.loads((ROOT / 'video/FFMPEG.toml').read_text())
            notice = helper.parent / 'ddi-video-helper.LICENSE.txt'
            metadata_file = helper.parent / 'ffmpeg.json'
            if not notice.is_file() or not metadata_file.is_file():
                raise ValueError('Video helper lacks its build metadata/notices; rebuild the app (desktop/build.rs stages them)')
            metadata = json.loads(metadata_file.read_bytes())
            expected_configure = video_config['native_configure'] + ['--enable-decoder=' + ','.join(video_config['video_decoders'])]
            # The script adds host-dependent flags (--disable-x86asm, --toolchain=msvc) after the pinned ones.
            if metadata['version'] != video_config['version'] or metadata['configure'][:len(expected_configure)] != expected_configure or metadata['target'] != manifest['target']:
                raise ValueError('Video helper metadata differs from current pinned build recipe or target')
            check = run([helper, '--sandbox-check'], env, capture=True)
            if 'holds' not in check:
                raise ValueError('Video helper sandbox check failed: ' + check)
            (output / 'video').mkdir()
            for file in (helper, notice, metadata_file):
                shutil.copy2(file, output / 'video' / file.name)
            downloads = ROOT / 'target/video/downloads'
            # These are the exact unmodified upstream archives named by the
            # helper's shipped notice. Their hashes are pinned in FFMPEG.toml.
            for source_url, source_sha in [(video_config['tarball_url'], video_config['tarball_sha256']), (video_config['zlib_url'], video_config['zlib_sha256'])]:
                name = source_url.rsplit('/', 1)[-1]
                archive = downloads / name
                if not archive.is_file() or sha(archive.read_bytes()) != source_sha:
                    raise ValueError('Video source archive missing or changed: ' + name)
                shutil.copy2(archive, output / 'video' / archive.name)
            # The LGPL's relinking route: the shims, the build scripts, the
            # helper and protocol crates (no build output).
            shutil.copytree(ROOT / 'video', output / 'video' / 'helper-source', ignore=shutil.ignore_patterns('target', '__pycache__'))
        elif info.get('video', True):
            raise ValueError('Video-enabled app needs its built decoder helper beside it (video/ddi-video-helper); rebuild the app, or build it without video')
        shutil.copytree(ROOT / 'LICENSES', output / 'LICENSES')
        for name in ['CREDITS.md', 'THIRD_PARTY_RUST.md']:
            shutil.copy2(ROOT / name, output / name)
        run([output / 'danoni-importer' / manifest['id'] / manifest['executable'], '--check-bundle'], env, cwd=output)
        (output / 'PACKAGE.json').write_bytes(encoded({'schema': 'ddi-desktop-distribution/1', 'appSha256': sha(app.read_bytes()), 'importerId': manifest['id'], 'manifestSha256': digest, 'target': manifest['target'], 'appNativeDependencies': app_dependencies}))
        (output / 'README.txt').write_text('Run ./' + app.name + '\nKeep assets/, video/ (when present), LICENSES/ and danoni-importer/ beside the executable.\nThe importer is self-contained native code; Java/Python/Cargo are build tools only.\nSee danoni-importer/' + manifest['id'] + '/runtime/notices/manifest.json for exact helper/SDK/worker dependency and embedded-font notices.\n')
    except BaseException:
        shutil.rmtree(output)
        raise
    print(output)


def re_hash(value):
    return isinstance(value, str) and len(value) == 64 and all(c in '0123456789abcdef' for c in value)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    p = sub.add_parser('prepare')
    p.add_argument('--target', required=True)
    p.add_argument('--stage', required=True, type=Path)
    p = sub.add_parser('package')
    p.add_argument('--app', required=True, type=Path)
    p.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    if args.command == 'prepare':
        prepare(args.target, args.stage)
    else:
        package(args.app, args.output.resolve())


if __name__ == '__main__':
    try:
        main()
    except (ValueError, RuntimeError, subprocess.CalledProcessError) as failure:
        print(f'Importer build/package failed: {failure}', file=sys.stderr)
        sys.exit(1)
