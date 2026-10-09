#!/usr/bin/env python3
"""Compatibility entry point; shared prepare_pack now handles normalized exports.

Extraction remains family-specific. Tier/preview/audio policies have no Kouhaku
song rules. Use prepare_pack.py for new integrations.
"""
import argparse
from pathlib import Path
import shutil
from prepare_pack import prepare

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('source',type=Path);p.add_argument('output',type=Path)
    p.add_argument('--audio',choices=['wav','ogg'],default='wav');p.add_argument('--ffmpeg',type=Path,default=shutil.which('ffmpeg'))
    p.add_argument('--vorbis-encoder',type=Path)
    a=p.parse_args()
    if a.ffmpeg is None:p.error('Installed FFmpeg required')
    prepare(a.source,a.output,a.audio,a.ffmpeg,a.vorbis_encoder)
