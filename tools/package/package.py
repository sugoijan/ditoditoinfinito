#!/usr/bin/env python3
"""Package the desktop game (docs/plans/packaging.md).

`folder` makes the relocatable folder: the executable with its assets, the
generated credits, the video decoder helper with its LGPL payload, the
licenses and, where the build has one, the URL importer's bundle.
`dist` makes it, then the system's archive in an output directory: a
signed (ad hoc) `.app` in a zip and a `.dmg` on macOS, a tarball with a
desktop entry and its icon on Linux, a zip on Windows. CI's release
workflow and local runs use the same commands."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import stat
import subprocess
import sys
sys.dont_write_bytecode = True
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]
# The importer's build driver (Unix only: imported where it is used) and
# its portable helpers.
sys.path.insert(0, str(ROOT / 'tools/danoni-importer'))
from notices import encoded, sha  # noqa: E402

HERE = Path(__file__).resolve().parent
NAME = 'Dito Dito Infinito'
ARCHIVE = 'DitoDitoInfinito'
BUNDLE_ID = 'dev.sugoijan.ditoditoinfinito'
EXE = 'ditoditoinfinito'
HELPER = 'ddi-video-helper'


def run(command, env, capture=False, cwd=ROOT):
    print('+ ' + ' '.join(str(x) for x in command), file=sys.stderr, flush=True)
    result = subprocess.run([str(x) for x in command], cwd=cwd, env=env, check=True,
                            stdout=subprocess.PIPE if capture else sys.stderr)
    return result.stdout if capture else b''


def host_target():
    return subprocess.check_output(['rustc', '-vV'], text=True).split('host: ')[1].splitlines()[0]


def version():
    return tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']


def is_hash(value):
    return isinstance(value, str) and len(value) == 64 and all(c in '0123456789abcdef' for c in value)


def folder(app, output, without_video_helper=False):
    """The relocatable folder at `output` (a fresh directory) for the
    executable `app`, as built by Cargo (assets and the helper staged
    beside it by crates/desktop/build.rs)."""
    app = app.resolve(strict=True)
    output = output.resolve()
    env = dict(os.environ)
    info = json.loads(run([app, '--danoni-bundle-info'], env, capture=True))
    importer = None
    if os.name != 'nt':
        from build import native_dependencies, verify
    if info.get('enabled'):
        if not is_hash(info.get('id')) or not is_hash(info.get('manifestSha256')):
            raise ValueError('Application has no prepared production importer binding')
        source = app.parent / 'danoni-importer' / info['id']
        manifest, digest = verify(source)
        if digest != info['manifestSha256']:
            raise ValueError('Application and staged importer differ')
        importer = (source, manifest, digest)
        target = manifest['target']
    elif 'windows' in host_target():
        # Never built for Windows (its confinement is still to come).
        target = host_target()
    else:
        raise ValueError('Application has no prepared importer: build with the default '
                         'features and without DDI_IMPORTER_TEST_ONLY')
    windows = target.endswith('windows-msvc')
    app_dependencies = None if windows else native_dependencies(app, target)
    if output.exists():
        raise ValueError('Package output must be a fresh directory')
    output.mkdir(parents=True)
    try:
        shutil.copy2(app, output / app.name)
        if importer:
            source, manifest, _ = importer
            shutil.copytree(source, output / 'danoni-importer' / manifest['id'])
        shutil.copytree(ROOT / 'assets', output / 'assets')
        for name in ['songs', 'noteskins']:
            index = ROOT / 'target' / name / 'index.json'
            if not index.is_file():
                raise ValueError(f'Generate {name} index with cargo run -p xtask -- gen-{name} before packaging')
            shutil.copy2(index, output / 'assets' / name / 'index.json')
        # The Credits screen's generated list (the desktop's, with its Rust
        # libraries), read beside the assets as the shell.rs credits_screen
        # looks for it; without it the screen would show sample credits.
        credits = ROOT / 'target' / 'credits' / 'credits-desktop.json'
        if not credits.is_file():
            raise ValueError('Generate the credits with cargo run -p xtask -- gen-credits before packaging')
        (output / 'assets' / 'credits').mkdir()
        shutil.copy2(credits, output / 'assets' / 'credits' / credits.name)
        # The sandboxed decoder helper crates/desktop/build.rs staged beside the app
        # (video/ddi-video-helper), with its notice and FFmpeg's description.
        helper = app.parent / 'video' / (HELPER + ('.exe' if windows else ''))
        if info.get('video', True) and helper.is_file():
            video_config = tomllib.loads((ROOT / 'video/FFMPEG.toml').read_text())
            notice = helper.parent / 'ddi-video-helper.LICENSE.txt'
            metadata_file = helper.parent / 'ffmpeg.json'
            if not notice.is_file() or not metadata_file.is_file():
                raise ValueError('Video helper lacks its build metadata/notices; rebuild the app (crates/desktop/build.rs stages them)')
            metadata = json.loads(metadata_file.read_bytes())
            expected_configure = video_config['native_configure'] + ['--enable-decoder=' + ','.join(video_config['video_decoders'])]
            # The script adds host-dependent flags (--disable-x86asm, --toolchain=msvc) after the pinned ones.
            if metadata['version'] != video_config['version'] or metadata['configure'][:len(expected_configure)] != expected_configure or metadata['target'] != target:
                raise ValueError('Video helper metadata differs from current pinned build recipe or target')
            check = run([helper, '--sandbox-check'], env, capture=True).decode()
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
        elif info.get('video', True) and not without_video_helper:
            raise ValueError('Video-enabled app needs its built decoder helper beside it (video/ddi-video-helper); rebuild the app, '
                             'build it without video, or pass --without-video-helper (movies then show their song background)')
        shutil.copytree(ROOT / 'LICENSES', output / 'LICENSES')
        for name in ['CREDITS.md', 'THIRD_PARTY_RUST.md']:
            shutil.copy2(ROOT / 'credits' / name, output / name)
        readme = [f'Run ./{app.name}' if not windows else f'Run {app.name}',
                  'Keep assets/, LICENSES/' + (', video/' if (output / 'video').exists() else '')
                  + (' and danoni-importer/' if importer else '') + ' beside the executable.']
        package_info = {'schema': 'ddi-desktop-distribution/1', 'version': version(), 'target': target,
                        'appSha256': sha(app.read_bytes()), 'appNativeDependencies': app_dependencies,
                        'videoHelper': (output / 'video').exists()}
        if importer:
            source, manifest, digest = importer
            run([output / 'danoni-importer' / manifest['id'] / manifest['executable'], '--check-bundle'], env, cwd=output)
            package_info.update(importerId=manifest['id'], manifestSha256=digest)
            readme += ['The importer is self-contained native code; Java/Python/Cargo are build tools only.',
                       f'See danoni-importer/{manifest["id"]}/runtime/notices/manifest.json for exact helper/SDK/worker dependency and embedded-font notices.']
        (output / 'PACKAGE.json').write_bytes(encoded(package_info))
        (output / 'README.txt').write_text('\n'.join(readme) + '\n')
    except BaseException:
        shutil.rmtree(output)
        raise
    return output


def archive_name(target, extension):
    system = 'macos' if 'apple-darwin' in target else 'windows' if 'windows' in target else 'linux'
    arch = {'aarch64': 'arm64'}.get(target.split('-')[0], target.split('-')[0])
    return f'{ARCHIVE}-{version()}-{system}-{arch}.{extension}'


def minimum_macos(executables):
    """The newest macOS any of the executables was built for (their
    LC_BUILD_VERSION's minos), so Info.plist does not claim older."""
    newest = (11, 0)
    for executable in executables:
        out = subprocess.check_output(['otool', '-l', str(executable)], text=True)
        for command in out.split('Load command')[1:]:
            kind = re.search(r'^\s*cmd (\S+)', command, re.M)
            key = {'LC_BUILD_VERSION': 'minos', 'LC_VERSION_MIN_MACOSX': 'version'}.get(kind and kind.group(1))
            found = key and re.search(rf'^\s*{key} (\d+)\.(\d+)', command, re.M)
            if found:
                newest = max(newest, (int(found.group(1)), int(found.group(2))))
    return '.'.join(str(x) for x in newest)


def macos_icon(destination):
    """AppIcon.icns from tools/package/icon.svg (sips renders SVG)."""
    with tempfile.TemporaryDirectory() as work:
        iconset = Path(work) / 'AppIcon.iconset'
        iconset.mkdir()
        for size in [16, 32, 128, 256, 512]:
            for scale in [1, 2]:
                name = f'icon_{size}x{size}{"@2x" if scale == 2 else ""}.png'
                pixels = str(size * scale)
                subprocess.run(['sips', '-s', 'format', 'png', '-z', pixels, pixels, str(HERE / 'icon.svg'),
                                '--out', str(iconset / name)], check=True, stdout=subprocess.DEVNULL)
        subprocess.run(['iconutil', '-c', 'icns', '-o', str(destination), str(iconset)], check=True)


def is_macho(path):
    with path.open('rb') as file:
        return file.read(4) in (b'\xcf\xfa\xed\xfe', b'\xca\xfe\xba\xbe')


def macos_app(source, app):
    """The `.app` from the folder at `source`: the game in Contents/MacOS,
    the video helper in Contents/Helpers/video, everything else (assets,
    the importer's bundle, the helper's notices and sources, licenses) in
    Contents/Resources, where crates/desktop/src/paths.rs looks; then
    signed ad hoc, inside out."""
    contents = app / 'Contents'
    (contents / 'MacOS').mkdir(parents=True)
    resources = contents / 'Resources'
    resources.mkdir()
    shutil.copy2(source / EXE, contents / 'MacOS' / EXE)
    for item in sorted(source.iterdir()):
        if item.name in (EXE, 'README.txt'):
            continue
        if item.is_dir():
            shutil.copytree(item, resources / item.name)
        else:
            shutil.copy2(item, resources / item.name)
    helper = resources / 'video' / HELPER
    if helper.exists():
        (contents / 'Helpers' / 'video').mkdir(parents=True)
        helper.rename(contents / 'Helpers' / 'video' / HELPER)
        helper = contents / 'Helpers' / 'video' / HELPER
    macos_icon(resources / 'AppIcon.icns')
    # The importer's executables keep the ad hoc signature the linker gave
    # them: the app and the importer check their bytes (manifest hashes), so
    # signing them again would make the bundle "damaged". The app's seal
    # covers them as resources.
    importer = [p for p in sorted((resources / 'danoni-importer').rglob('*')) if p.is_file() and is_macho(p)] \
        if (resources / 'danoni-importer').exists() else []
    executables = [contents / 'MacOS' / EXE] + ([helper] if helper.exists() else []) + importer
    with (HERE / 'macos/Info.plist').open('rb') as file:
        plist = plistlib.load(file)
    short = re.match(r'\d+(\.\d+){0,2}', version()).group(0)
    plist.update(CFBundleShortVersionString=short, CFBundleVersion=short,
                 LSMinimumSystemVersion=minimum_macos(executables))
    with (contents / 'Info.plist').open('wb') as file:
        plistlib.dump(plist, file)
    (contents / 'PkgInfo').write_text('APPL????')
    # Finder's and the quarantine's attributes would break the seal.
    subprocess.run(['/usr/bin/xattr', '-cr', str(app)], check=True)
    for executable in importer:
        subprocess.run(['codesign', '--verify', '--strict', str(executable)], check=True)
    if helper.exists():
        subprocess.run(['codesign', '--force', '--sign', '-', '--identifier', BUNDLE_ID + '.video-helper',
                        str(helper)], check=True)
    subprocess.run(['codesign', '--force', '--sign', '-', str(app)], check=True)
    subprocess.run(['codesign', '--verify', '--deep', '--strict', '--verbose=2', str(app)], check=True)
    return app


def check_app(executable, helper):
    """The packaged game's headless checks: the helper's sandbox holds and
    the importer answers through the game's own client (no window, no
    sound device, no network), from a scratch home."""
    with tempfile.TemporaryDirectory() as home:
        env = dict(os.environ, DDI_HOME=home)
        if helper and helper.exists():
            check = run([helper, '--sandbox-check'], env, capture=True).decode()
            if 'holds' not in check:
                raise ValueError('Packaged video helper sandbox check failed: ' + check)
        info = json.loads(run([executable, '--danoni-bundle-info'], env, capture=True))
        if info.get('enabled'):
            result = json.loads(run([executable, '--danoni-importer-check'], env, capture=True))
            if not result.get('ready') or not result.get('bundledSongs'):
                raise ValueError(f'Packaged importer check failed: {result}')


def write_zip(source, archive, root_name):
    """A zip of `source` under `root_name/`, keeping Unix modes."""
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as out:
        for path in sorted(source.rglob('*')):
            name = (Path(root_name) / path.relative_to(source)).as_posix()
            if path.is_dir():
                continue
            entry = zipfile.ZipInfo.from_file(path, name)
            entry.compress_type = zipfile.ZIP_DEFLATED
            with path.open('rb') as file:
                out.writestr(entry, file.read(), compresslevel=9)


def dist(app, out_dir, without_video_helper=False, check=True):
    out_dir = out_dir.resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='ddi-dist-') as work:
        work = Path(work)
        plain = folder(app, work / 'folder', without_video_helper)
        target = json.loads((plain / 'PACKAGE.json').read_bytes())['target']
        made = []
        if 'apple-darwin' in target:
            staging = work / 'dmg'
            staging.mkdir()
            bundle = macos_app(plain, staging / f'{NAME}.app')
            if check:
                check_app(bundle / 'Contents/MacOS' / EXE, bundle / 'Contents/Helpers/video' / HELPER)
            zipped = out_dir / archive_name(target, 'zip')
            # ditto keeps the modes and the bundle's structure (Apple's way to zip an app).
            subprocess.run(['ditto', '-c', '-k', '--keepParent', str(bundle), str(zipped)], check=True)
            made.append(zipped)
            (staging / 'Applications').symlink_to('/Applications')
            image = out_dir / archive_name(target, 'dmg')
            image.unlink(missing_ok=True)
            # hdiutil now and then finds its device busy on CI; try again.
            for attempt in range(3):
                result = subprocess.run(['hdiutil', 'create', '-volname', NAME, '-srcfolder', str(staging),
                                         '-fs', 'HFS+', '-format', 'UDZO', '-imagekey', 'zlib-level=9', '-ov', str(image)])
                if result.returncode == 0:
                    break
            else:
                raise RuntimeError('hdiutil could not create the disk image')
            made.append(image)
        elif 'linux' in target:
            share = plain / 'share'
            (share / 'applications').mkdir(parents=True)
            (share / 'icons/hicolor/scalable/apps').mkdir(parents=True)
            shutil.copy2(HERE / f'linux/{BUNDLE_ID}.desktop', share / 'applications')
            shutil.copy2(HERE / 'icon.svg', share / f'icons/hicolor/scalable/apps/{BUNDLE_ID}.svg')
            script = plain / 'install-desktop-entry.sh'
            shutil.copy2(HERE / 'linux/install-desktop-entry.sh', script)
            script.chmod(script.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
            with (plain / 'README.txt').open('a') as readme:
                readme.write('./install-desktop-entry.sh adds the game to your applications menu (--remove takes it out).\n')
            if check:
                check_app(plain / EXE, plain / 'video' / HELPER)
            name = archive_name(target, 'tar.gz')
            tarball = out_dir / name
            def anonymous(entry):
                # Not the builder's user: whoever unpacks it owns the files.
                entry.uid = entry.gid = 0
                entry.uname = entry.gname = ''
                return entry
            with tarfile.open(tarball, 'w:gz', compresslevel=9) as out:
                out.add(plain, arcname=name.removesuffix('.tar.gz'), filter=anonymous)
            made.append(tarball)
        else:
            if check:
                check_app(plain / f'{EXE}.exe', None)
            name = archive_name(target, 'zip')
            write_zip(plain, out_dir / name, name.removesuffix('.zip'))
            made.append(out_dir / name)
    for path in made:
        print(path)
    return made


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    for command in ['folder', 'dist']:
        p = sub.add_parser(command)
        p.add_argument('--app', required=True, type=Path, help='the built executable (target/desktop/ditoditoinfinito)')
        if command == 'folder':
            p.add_argument('--output', required=True, type=Path, help='a fresh directory')
        else:
            p.add_argument('--output-dir', required=True, type=Path, help='where the archives go')
            p.add_argument('--no-check', action='store_true', help="skip the packaged game's headless checks")
        p.add_argument('--without-video-helper', action='store_true',
                       help='package a video-enabled game without its helper (movies show their song background)')
    p = sub.add_parser('version', help="print the game's version (Cargo's)")
    args = parser.parse_args()
    if args.command == 'folder':
        print(folder(args.app, args.output, args.without_video_helper))
    elif args.command == 'dist':
        dist(args.app, args.output_dir, args.without_video_helper, not args.no_check)
    else:
        print(version())


if __name__ == '__main__':
    try:
        main()
    except (ValueError, RuntimeError, subprocess.CalledProcessError) as failure:
        print(f'Packaging failed: {failure}', file=sys.stderr)
        sys.exit(1)
