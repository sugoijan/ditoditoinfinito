"""Reusable presentation policies; extraction ratings/timestamps stay factual."""
import math
import re
from preview_policy import choose_preview

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


def authored_display_bpm(value):
    """Recognize numeric CW BPM metadata, preserving unknown placeholders as absent."""
    if not isinstance(value,str) or not re.fullmatch(r'[0-9]+(?:\.[0-9]+)?(?:-[0-9]+(?:\.[0-9]+)?)?',value):return None
    try:return {**display_bpm(value.replace('-',':')),'source':'authored CW header'}
    except ValueError:return None
