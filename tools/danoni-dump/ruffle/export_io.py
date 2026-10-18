"""Small shared helpers for derived local artifacts."""
import hashlib
import json
from pathlib import Path
import shutil

MODES = {"5", "7", "9A", "9B"}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path,value):
    path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')


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
