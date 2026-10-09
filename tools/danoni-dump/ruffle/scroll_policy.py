"""Native base motion stays independent of musical BPM and player X-mod."""
import math


def validate_motion(raw):
    rate=raw['baseArrowHeightsPerSecond']
    if type(rate) not in (int,float) or not math.isfinite(rate) or not 0<rate<=1000:
        raise ValueError('Need a finite positive native base velocity at most 1000 arrow heights/second')
    return raw


def base_ratio(motion,bpm,factor=1):
    if type(factor) not in (int,float) or not math.isfinite(factor) or not 0<factor<=100:
        raise ValueError('Scroll factor must be finite, positive and at most 100')
    if motion is None:
        if factor!=1:raise ValueError('Scroll factor needs an explicit native motion review')
        return 1
    validate_motion(motion)
    return motion['baseArrowHeightsPerSecond']*factor*60/bpm


def summarize(motion,bpm,factor):
    ratio=base_ratio(motion,bpm,factor)
    if motion is None:return None
    return {'sourceMotion':motion,'correctionFactor':factor,'sscBaseRatio':ratio,
        'baseArrowHeightsPerSecond':motion['baseArrowHeightsPerSecond']*factor,
        'playerMultiplierModified':False,'mode':'X-mod; C-mod intentionally ignores this authored scroll map',
        'formula':'source native arrow heights/second * correction factor * 60 / timing BPM'}
