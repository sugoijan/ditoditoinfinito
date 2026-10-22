#!/usr/bin/env python3
"""Writes the zip reader's fixtures (`crates/library/src/zip.rs`): two
synthetic texts of our own (`small.txt`, 3 KB; `big.txt`, 335 KB, several
bzip2 blocks) compressed by real writers with each method the reader
supports beyond stored and deflate:

- 7z-deflate64.zip, 7z-bzip2.zip, 7z-lzma.zip: p7zip 17.05
  (`7z a -tzip -mm=Deflate64`, `-mm=BZip2 -md=100k`, `-mm=LZMA`)
- python-bzip2.zip, python-lzma.zip: Python's zipfile (libbz2, liblzma)
- zstd.zip: the zstd command's frames (1.5.7) in a zip written here,
  method 93, and `big.txt` again as method 20 in two frames with a
  skippable frame between them

Run from anywhere: `python3 crates/library/tests/zips/make.py` (needs `7z`
and `zstd` on PATH). The tests rebuild the texts the same way.
"""

import os
import struct
import subprocess
import sys
import tempfile
import zipfile
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
DATE = (2026, 1, 1, 0, 0, 0)


def small():
    return "".join(f"#NOTE {i}: {i * 7919 % 10007:05}\n" for i in range(200)).encode()


def big():
    arrows = ["LEFT", "DOWN", "UP", "RIGHT"]
    return "".join(
        f"{i // 4096:03} {arrows[i * i % 7 % 4]} {i % 97:02}\n" for i in range(30000)
    ).encode()


def run(*args, cwd=None):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True).stdout


def zstd(data, level):
    return subprocess.run(
        ["zstd", f"-{level}", "-c", "-q"], input=data, check=True, capture_output=True
    ).stdout


def write_raw_zip(path, entries):
    """A zip of already compressed entries: (name, method, data, raw)."""
    out = bytearray()
    central = bytearray()
    for name, method, data, raw in entries:
        offset = len(out)
        crc = zlib.crc32(data)
        name = name.encode()
        head = struct.pack(
            "<IHHHHHIIIHH", 0x04034B50, 63, 0x800, method, 0, 0x5C21,
            crc, len(raw), len(data), len(name), 0,
        )
        out += head + name + raw
        central += struct.pack(
            "<IHHHHHHIIIHHHHHII", 0x02014B50, (3 << 8) | 63, 63, 0x800, method,
            0, 0x5C21, crc, len(raw), len(data), len(name), 0, 0, 0, 0,
            0o100644 << 16, offset,
        ) + name
    end = struct.pack(
        "<IHHHHIIH", 0x06054B50, 0, 0, len(entries), len(entries),
        len(central), len(out), 0,
    )
    with open(path, "wb") as f:
        f.write(out + central + end)


def main():
    texts = {"small.txt": small(), "big.txt": big()}
    with tempfile.TemporaryDirectory() as tmp:
        for name, data in texts.items():
            p = os.path.join(tmp, name)
            with open(p, "wb") as f:
                f.write(data)
            os.utime(p, (1767225600, 1767225600))
        for out, args in [
            ("7z-deflate64.zip", ["-mm=Deflate64"]),
            ("7z-bzip2.zip", ["-mm=BZip2", "-md=100k"]),
            ("7z-lzma.zip", ["-mm=LZMA"]),
        ]:
            target = os.path.join(HERE, out)
            if os.path.exists(target):
                os.remove(target)
            run("7z", "a", "-tzip", *args, target, "small.txt", "big.txt", cwd=tmp)
    for out, method in [
        ("python-bzip2.zip", zipfile.ZIP_BZIP2),
        ("python-lzma.zip", zipfile.ZIP_LZMA),
    ]:
        with zipfile.ZipFile(os.path.join(HERE, out), "w") as z:
            for name, data in texts.items():
                info = zipfile.ZipInfo(name, DATE)
                info.compress_type = method
                z.writestr(info, data)
    b = texts["big.txt"]
    skippable = struct.pack("<II", 0x184D2A5E, 5) + b"skip!"
    write_raw_zip(
        os.path.join(HERE, "zstd.zip"),
        [
            ("small.txt", 93, texts["small.txt"], zstd(texts["small.txt"], 19)),
            ("big.txt", 93, b, zstd(b, 3)),
            ("big-frames.txt", 20, b, zstd(b[:100000], 19) + skippable + zstd(b[100000:], 1)),
        ],
    )


if __name__ == "__main__":
    sys.exit(main())
