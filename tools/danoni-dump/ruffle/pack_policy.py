"""Reusable presentation policies; extraction ratings/timestamps stay factual."""
from array import array
import math
import re
import sys
import wave

TIERS = ['Beginner','Easy','Medium','Hard','Challenge']


def star_count(metadata, chart):
    value=chart.get('difficultyStars',metadata.get('difficultyStars'))
    if value is not None:
        if type(value) is not int or not 0<=value<=999:raise ValueError('Invalid numeric star rating')
        return value
    raw=metadata.get('rawDifficulty',metadata.get('difficulty'))
    return len(raw) if isinstance(raw,str) and re.fullmatch('★+',raw) else None


def tier_for_stars(stars, bounds=(1,3,5,7)):
    if len(bounds)!=4 or tuple(sorted(set(bounds)))!=tuple(bounds) or any(type(n) is not int or n<0 for n in bounds):
        raise ValueError('Tier bounds must be four increasing nonnegative integers')
    if stars is None:return 'Edit'
    if type(stars) is not int or not 0<=stars<=999:raise ValueError('Invalid star rating')
    return next((tier for tier,maximum in zip(TIERS,bounds) if stars<=maximum),'Challenge')


def choose_preview(wav_path, seconds=12):
    if not 1<=seconds<=30:raise ValueError('Preview length must be 1–30 seconds')
    energy=[]
    with wave.open(str(wav_path),'rb') as wav:
        rate,channels,frames=wav.getframerate(),wav.getnchannels(),wav.getnframes()
        if wav.getsampwidth()!=2 or channels not in (1,2) or frames>rate*600:raise ValueError('Need bounded mono/stereo PCM16')
        step=max(1,rate//2)
        while raw:=wav.readframes(step):
            values=array('h');values.frombytes(raw)
            if sys.byteorder!='little':values.byteswap()
            energy.append(math.sqrt(sum((v/32768)**2 for v in values)/len(values)))
    duration=frames/rate;length=min(seconds,duration)
    if not duration:raise ValueError('Empty music')
    threshold=max(1/32768,max(energy)*.15)
    active=[e>=threshold for e in energy]
    first=next((i for i in range(len(active)-2) if all(active[i:i+3])),None)
    if first is None:
        first=next((i for i,a in enumerate(active) if a),None)
    if first is None:return {'startSeconds':0,'lengthSeconds':length,'status':'no-activity','thresholdRms':threshold}
    first_seconds=first*step/rate
    # Choose a substantial excerpt after the introduction, rather than seek/trim audio.
    minimum=min(max(first_seconds+1,duration*.2),max(0,duration-length))
    maximum=max(minimum,min(duration*.8,duration-length))
    width=max(1,math.ceil(length*rate/step));best=None
    for i in range(math.ceil(minimum*rate/step),min(len(energy),math.floor(maximum*rate/step)+1)):
        chunk=energy[i:i+width]
        if not chunk:continue
        fraction=sum(active[i:i+width])/len(chunk)
        score=(fraction>=.8,fraction,sum(chunk)/len(chunk),-i)
        if best is None or score>best[0]:best=(score,i,fraction)
    start=(best[1]*step/rate) if best else minimum
    return {'startSeconds':start,'lengthSeconds':min(length,duration-start),'status':'selected',
        'firstActiveSeconds':first_seconds,'activeFraction':best[2] if best else None,
        'thresholdRms':threshold,'method':'sustained RMS activity and energetic excerpt after intro; no audio cropping'}


def display_bpm(value):
    """Explicit reviewed display metadata, independent of the playback grid."""
    if value=='*':return {'kind':'unknown','sscValue':'*','source':'explicit review input'}
    try:
        parts=[float(p) for p in value.split(':')]
    except (AttributeError,ValueError):raise ValueError('Display BPM must be BPM, LOW:HIGH or *') from None
    if len(parts) not in (1,2) or any(not math.isfinite(p) or not 0<p<=1e6 for p in parts):
        raise ValueError('Display BPM must contain one or two finite positive values at most 1e6')
    if len(parts)==2 and parts[0]>parts[1]:raise ValueError('Display BPM range must be increasing')
    return {'kind':'single' if len(parts)==1 else 'range','minimumBpm':parts[0],'maximumBpm':parts[-1],
        'sscValue':':'.join(f'{p:.12g}' for p in parts),'source':'explicit review input'}
