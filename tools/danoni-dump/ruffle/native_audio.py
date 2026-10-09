"""Match demuxed music to observed restricted Ruffle PCM without a browser.

The correlation estimates a decoder-origin difference, never a musical offset.
Only one continuous observed song stream is accepted. Source MP3 and sample/
seek headers remain preserved; a canonical PCM WAV applies the measured shift.
"""
from array import array
import math
from pathlib import Path
import subprocess
import sys
import wave
from export_io import digest,write_json


def pcm(path):
    with wave.open(str(path),'rb') as f:
        spec=(f.getframerate(),f.getnchannels(),f.getnframes(),f.getsampwidth())
        if spec[3]!=2:raise ValueError('Need PCM16 audio')
        samples=array('h',f.readframes(spec[2]))
    if sys.byteorder!='little':samples.byteswap()
    return spec,samples


def origin_match(native, decoded, start_frame, rate=44100):
    # Both buffers are interleaved stereo at the native mixer's rate.
    import numpy as np
    from scipy.signal import correlate
    x=native[start_frame*2:];count=min(len(x),len(decoded))//2
    if count<rate//2:raise ValueError('Need at least half a second of observed song PCM')
    length=min(rate//3,count//3);margin=rate//4;windows=[]
    def energy(at):return sum(x[(at+i)*2]**2+x[(at+i)*2+1]**2 for i in range(0,length,32))
    candidates=list(range(margin,count-length+1,max(1,length//2)))
    selected=[]
    for at in sorted(candidates,key=energy,reverse=True):
        if all(abs(at-old)>=length//2 for old in selected):selected.append(at)
        if len(selected)==8:break
    rejected=[]
    for part,at in enumerate(sorted(selected)):
        ref=np.asarray(decoded[(at-margin)*2:(at+length+margin)*2],dtype=np.float64).reshape(-1,2)
        window=np.asarray(x[at*2:(at+length)*2],dtype=np.float64).reshape(-1,2)
        window=window-np.mean(window,axis=0)
        xx=np.sum(window*window)
        if math.sqrt(xx/(length*2))<4:continue
        xy=sum(correlate(ref[:,ch],window[:,ch],mode='valid',method='fft')for ch in range(2))
        yy=np.zeros(len(ref)-length+1)
        for ch in range(2):
            cumulative=np.concatenate(([0.],np.cumsum(ref[:,ch]**2)))
            sums=np.concatenate(([0.],np.cumsum(ref[:,ch])))
            yy+=cumulative[length:]-cumulative[:-length]-(sums[length:]-sums[:-length])**2/length
        denom=np.sqrt(np.maximum(0.,yy)*xx)
        scores=np.divide(xy,denom,out=np.full_like(xy,-1.),where=denom>0)
        lag=int(np.argmax(scores))-margin;quality=float(scores[lag+margin])
        row={'part':part,'startSeconds':at/rate,'lagSamples':lag,'correlation':quality}
        (windows if quality>=.97 else rejected).append(row)
    if len(windows)<2 or any(w['correlation']<.97 for w in windows) or max(w['lagSamples'] for w in windows)-min(w['lagSamples'] for w in windows)>2:
        raise ValueError(f'No consistent decoder-origin match: {windows}')
    lag=round(sum(w['lagSamples'] for w in windows)/len(windows))
    return {'lagSamplesAt44100':lag,'maximumSpreadSamples':max(w['lagSamples']for w in windows)-min(w['lagSamples']for w in windows),
            'windows':windows,'rejectedWindows':rejected,'scope':'Measured decoder-origin difference against captured native PCM; original Flash/device latency unmeasured'}


def canonical_audio(mp3_path, capture_directory, stream_start, output, ffmpeg):
    original=output/'source-decoded.wav';comparison=output/'source-comparison.wav'
    commands=[]
    for path,extra in [(original,[]),(comparison,['-ar','44100','-ac','2'])]:
        command=[str(ffmpeg),'-nostdin','-v','error','-i',str(mp3_path),'-map_metadata','-1',*extra,'-c:a','pcm_s16le',str(path)]
        subprocess.run(command,check=True,timeout=30);commands.append(command)
    spec,decoded=pcm(comparison)
    if spec[0:2]!=(44100,2):raise ValueError('Unexpected comparison audio units')
    native_bytes=(capture_directory/'audio.pcm').read_bytes();native=array('h',native_bytes)
    if sys.byteorder!='little':native.byteswap()
    if 'nextStreamOutputSampleFrame' in stream_start:native=native[:stream_start['nextStreamOutputSampleFrame']*2]
    match=origin_match(native,decoded,stream_start['outputSampleFrame'])
    original_spec,values=pcm(original);rate,channels,frames,_=original_spec
    shift=round(match['lagSamplesAt44100']*rate/44100)
    if shift>=0:values=values[shift*channels:]
    else:values=array('h',[0]*(-shift*channels))+values
    target=output/'music.wav'
    with wave.open(str(target),'wb') as f:
        f.setnchannels(channels);f.setframerate(rate);f.setsampwidth(2)
        if sys.byteorder!='little':values.byteswap()
        f.writeframes(values.tobytes())
    match.update(schema='ddi-native-audio-origin/experimental-1',sourceMp3Sha256=digest(mp3_path.read_bytes()),
                 nativePcmSha256=digest(native_bytes),originalFrames=frames,sampleRate=rate,channels=channels,
                 appliedShiftSamples=shift,canonicalSha256=digest(target.read_bytes()),commands=commands,
                 transformation='Trim measured decoder lead-in when shift is positive; prepend zero samples when negative; no note-grid or musical offset fitting')
    write_json(output/'audio-origin.json',match)
    comparison.unlink()  # A reproducible resampling intermediate; source PCM/MP3 and evidence remain.
    return match
