"""Encode original PCM with the optional native helper; never use work assets."""
import argparse
from pathlib import Path
import math
import json
import struct
import subprocess
import tempfile
import wave


def test(encoder, ffmpeg):
    with tempfile.TemporaryDirectory(prefix='ddi-vorbis-') as directory:
        directory=Path(directory)
        for rate,frames in [(22050,30003),(44100,50001)]:
            pcm=b''.join(struct.pack('<h',round(12000*math.sin(2*math.pi*440*i/rate))) for i in range(frames))
            compressed=directory/f'{rate}.ogg'
            with compressed.open('wb') as output:
                subprocess.run([str(encoder),str(rate)],input=pcm,stdout=output,check=True,timeout=10)
            decoded=directory/f'{rate}.wav'
            subprocess.run([str(ffmpeg),'-nostdin','-v','error','-i',str(compressed),str(decoded)],check=True,timeout=10)
            # Ogg granules carry the exact source length. FFmpeg can pad/trim
            # arbitrary short final packets, so do not assume its PCM length.
            probe=subprocess.check_output([str(ffmpeg.with_name('ffprobe')),'-v','error',
                '-show_entries','stream=duration_ts,sample_rate,channels','-of','json',str(compressed)],text=True)
            stream=json.loads(probe)['streams'][0]
            assert (stream['duration_ts'],int(stream['sample_rate']),stream['channels'])==(frames,rate,1)
            with wave.open(str(decoded),'rb') as wav:
                assert wav.getnframes()>0 and (wav.getframerate(),wav.getnchannels())==(rate,1)
        for rate,pcm in [('0',b''),('22050',b'\x00'),('abc',b'')]:
            assert subprocess.run([str(encoder),rate],input=pcm,stdout=subprocess.DEVNULL,timeout=10).returncode!=0
    print('Synthetic Vorbis granule counts, rates, mono channel and malformed-input rejection passed')


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('encoder',type=Path);p.add_argument('ffmpeg',type=Path)
    a=p.parse_args();test(a.encoder,a.ffmpeg)
