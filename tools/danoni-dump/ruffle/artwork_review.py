"""Read source-bound artwork decisions without altering source chart evidence."""
import json
from pathlib import Path
import struct

from export_io import digest


def asset(root, row):
    relative=Path(row['path']);base=root.resolve(strict=True)
    if relative.is_absolute() or '..' in relative.parts:raise ValueError('Artwork must be a contained regular file')
    path=base
    for part in relative.parts:
        path=path/part
        if path.is_symlink():raise ValueError('Artwork symlinks are not allowed')
    path=path.resolve(strict=True)
    if not path.is_relative_to(base):raise ValueError('Artwork escapes review directory')
    if not path.is_file() or path.stat().st_size>8*1024*1024:raise ValueError('Invalid/oversized artwork asset')
    raw=path.read_bytes()
    if digest(raw)!=row['sha256']:raise ValueError('Artwork asset changed')
    return raw


def png_dimensions(raw):
    if len(raw)<33 or raw[:16]!=b'\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR':raise ValueError('Artwork must be PNG')
    dimensions=struct.unpack('>II',raw[16:24])
    if not all(1<=v<=2048 for v in dimensions):raise ValueError('Invalid artwork dimensions')
    return dimensions


def load_review(path, source_hash, chart_ids, source_swf=None):
    raw=path.read_bytes();review=json.loads(raw)
    if review.get('schema')!='ddi-reviewed-song-artwork/1' or review.get('sourceManifestSha256')!=source_hash:raise ValueError('Artwork review must bind the exact source manifest')
    if set(review['charts'])-set(chart_ids):raise ValueError('Artwork refers to an unknown chart')
    selected={}
    for ident,row in review['charts'].items():
        body=asset(path.parent,row['banner']);dimensions=png_dimensions(body)
        if dimensions!=(640,200):raise ValueError('Expected a 640x200 song banner')
        evidence=json.loads(asset(path.parent,row['evidence']))
        if row['origin']=='native-title-panel':
            if evidence.get('schema')!='ddi-native-title-panel/1' or evidence.get('sourceSwfSha256')!=source_swf or evidence.get('files',{}).get('banner.png')!=digest(body) or evidence.get('titleReplaced')is not False:raise ValueError('Native panel evidence does not match source/banner')
        elif row['origin']=='plain-title':
            records=evidence.get('records',[])
            if evidence.get('schema')!='ddi-artwork-study/experimental-1' or evidence.get('template')!='plain-title/1' or not any(r['role']=='banner' and r['pngSha256']==digest(body)for r in records):raise ValueError('Plain-title evidence does not match banner')
        else:raise ValueError('Unknown artwork origin')
        selected[ident]={'bytes':body,'sha256':digest(body),'dimensions':list(dimensions),'origin':row['origin'],'evidenceSha256':row['evidence']['sha256']}
    return raw,selected


def undecorated_simfile(raw):
    # Only our exact validated optional banner header may differ from a baseline.
    return raw.replace(b'#BANNER:banner.png;\n',b'',1)
