#!/usr/bin/env python3
"""Prepare a numeric-rated WAV/Ogg pack from existing validated native captures.

This performs no SWF execution or fetching. Original extraction evidence remains
untouched. The required simfile category is Edit; classification is a consumer's
choice. Musical tempo estimates are never applied to the authoritative grid.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import wave
import zipfile

from kouhaku import project, sm_text, SELECTIONS, SWF_HASH, write_json, digest
from paths import external_directory


def repack(source, output, codec, ffmpeg, vorbis_encoder=None):
    source=source.resolve(strict=True);original_manifest=(source/'manifest.json').read_bytes();manifest=json.loads(original_manifest)
    if manifest['sourceSwfSha256']!=SWF_HASH:raise ValueError('Unreviewed collection')
    def checked(relative, expected=None):
        path=(source/relative).resolve(strict=True)
        if not path.is_relative_to(source) or path.is_symlink():raise ValueError('Invalid source path')
        if expected and digest(path.read_bytes())!=expected:raise ValueError('Changed source evidence')
        return path
    refs={};charts={c['id']:c for c in manifest['charts']}
    for selection in SELECTIONS:
        ident=selection[0];chart=charts[ident]
        snapshot=checked(f'native/{ident}/snapshot.json',chart['sourceSnapshotSha256'])
        parent=json.loads(checked(f'native/{ident}/run.json').read_text())
        if parent['exitCode'] or parent['termination'] or not parent['inputsUnchanged']:raise ValueError('Failed capture')
        refs[ident]=project(json.loads(snapshot.read_text()),selection)
        if refs[ident]['lanes']!=chart['lanes']:raise ValueError('Note data changed on re-projection')
        verification=json.loads(checked(chart['validation']).read_text())
        if not verification['passed'] or verification['sourceSnapshotSha256']!=chart['sourceSnapshotSha256']:raise ValueError('Missing passing CW evidence')
    output=external_directory(output);output.mkdir(parents=True,exist_ok=False)
    (output/'songs').mkdir();(output/'references').mkdir()
    songs=[];version=subprocess.check_output([str(ffmpeg),'-version'],text=True).splitlines()[0]
    for selection in SELECTIONS:
        ident=selection[0]
        if ident=='tougenkyo-hard':continue
        ids=['tougenkyo','tougenkyo-hard'] if ident=='tougenkyo' else [ident]
        music=charts[ident]['music'];wav_path=checked(music['path'],music['sha256'])
        folder=output/'songs'/ident;folder.mkdir();name='music.'+codec;destination=folder/name
        if codec=='wav':shutil.copyfile(wav_path,destination);command=None
        else:
            if vorbis_encoder is None:raise ValueError('Ogg output needs the separately built --vorbis-encoder')
            with wave.open(str(wav_path),'rb') as wav:
                if wav.getnchannels()!=1 or wav.getsampwidth()!=2:raise ValueError('Encoder accepts mono PCM16 only')
                sample_rate=wav.getframerate();samples=wav.readframes(wav.getnframes())
            command=[str(vorbis_encoder),str(sample_rate)]
            with destination.open('xb') as encoded:
                subprocess.run(command,input=samples,stdout=encoded,check=True,timeout=30)
            encoder_version=subprocess.check_output([str(vorbis_encoder),'--version'],text=True).strip()
            # Independent decode verifies sample count. No delay adjustment or trim.
            decoded=output/'references'/f'{ident}-ogg-decoded.wav'
            decode=[str(ffmpeg),'-nostdin','-v','error','-i',str(destination),'-c:a','pcm_s16le',str(decoded)]
            subprocess.run(decode,check=True,timeout=30,stdin=subprocess.DEVNULL)
            with wave.open(str(wav_path),'rb') as a,wave.open(str(decoded),'rb') as b:
                original=(a.getnchannels(),a.getframerate(),a.getnframes())
                actual=(b.getnchannels(),b.getframerate(),b.getnframes())
                if actual!=original:raise ValueError(f'Vorbis decoded sample origin/length needs review: {original} vs {actual}')
            decoded.unlink()
        content=[]
        for index,selected in enumerate(ids):
            ref=refs[selected];sm=sm_text(ref,name).replace('../../media/'+name,name)
            if index:sm='#NOTES:'+sm.split('#NOTES:',1)[1]
            content.append(sm)
            write_json(output/'references'/f'{selected}.json',ref)
        (folder/'song.sm').write_text('\n'.join(content))
        songs.append({'id':ident,'title':refs[ident]['metadata']['title'],'music':f'songs/{ident}/{name}',
            'musicSha256':digest(destination.read_bytes()),'sourceWavSha256':music['sha256'],
            'sourceWavPath':str(wav_path),'encodingCommand':command,
            'encoderVersion':encoder_version if codec=='ogg' else None,
            'encoderSha256':digest(vorbis_encoder.read_bytes()) if codec=='ogg' else None,'sampleOriginChange':0,'sampleCountVerified':True,
            'simfile':f'songs/{ident}/song.sm','charts':[{'id':i,'difficultyStars':refs[i]['metadata']['difficultyStars'],
                'difficultyClassification':None,'rawDifficulty':refs[i]['metadata']['rawDifficulty']} for i in ids]})
    report={'schema':'ddi-kouhaku-playable-pack/experimental-1','sourceManifestSha256':digest(original_manifest),
        'sourceSwfSha256':SWF_HASH,'sourceExport':str(source),'repackSourceSha256':digest(Path(__file__).read_bytes()),
        'projectionSourceSha256':digest((Path(__file__).parent/'kouhaku.py').read_bytes()),'codec':codec,'ffmpeg':version,
        'timingBpm':75,'timingModified':False,'musicalBpm':None,'musicalBeatGrid':None,'songs':songs,
        'classificationPolicy':'Numeric source stars only. Edit is the mandatory simfile placeholder, not an inferred difficulty category.'}
    write_json(output/'manifest.json',report)
    with zipfile.ZipFile(output/'playable.zip','x',compression=zipfile.ZIP_DEFLATED) as archive:
        for file in sorted((output/'songs').rglob('*')):
            if file.is_file():archive.write(file,file.relative_to(output/'songs'))
    return report


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('source',type=Path);p.add_argument('output',type=Path)
    p.add_argument('--vorbis-encoder',type=Path)
    p.add_argument('--audio',choices=['wav','ogg'],default='wav');p.add_argument('--ffmpeg',type=Path,default=shutil.which('ffmpeg'))
    a=p.parse_args()
    if a.ffmpeg is None:p.error('Installed FFmpeg required')
    r=repack(a.source,a.output,a.audio,a.ffmpeg,a.vorbis_encoder);print(json.dumps({'songs':len(r['songs']),'charts':sum(len(s['charts']) for s in r['songs']),'codec':r['codec'],'timingModified':False}))
