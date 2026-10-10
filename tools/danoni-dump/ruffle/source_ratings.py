"""Extract reviewed numeric ratings from immutable, cached work-page evidence.

HTML is parsed as data. Reviews select exact excerpts and chart identities;
there is no inference from chart names, key counts, speed or tool difficulty.
"""
import json
from html.parser import HTMLParser
from pathlib import Path
import re
import unicodedata

from export_io import digest


class VisibleText(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True);self.ignored=[];self.parts=[]
    def handle_starttag(self,tag,attrs):
        if tag in ('script','style','textarea','title'):self.ignored.append(tag)
        elif not self.ignored and tag not in ('span','font','b','strong','em','i','a'):self.parts.append(' ')
    def handle_endtag(self,tag):
        if self.ignored:
            if tag==self.ignored[-1]:self.ignored.pop()
        elif tag not in ('span','font','b','strong','em','i','a'):self.parts.append(' ')
    def handle_data(self,data):
        if not self.ignored:self.parts.append(data)


def visible_text(body,encoding):
    parser=VisibleText();parser.feed(body.decode(encoding));parser.close()
    return ' '.join(unicodedata.normalize('NFKC',''.join(parser.parts)).split())


def rating(notation,token):
    token=unicodedata.normalize('NFKC',token)
    def filled(s):
        if not re.fullmatch(r'[★☆\s]+',s) or '★'not in s:raise ValueError('Need an explicit filled-star rating')
        return s.count('★')
    if notation=='filled-stars':low=high=filled(token)
    elif notation=='star-range':
        parts=re.split(r'\s*(?:→|〜|～|~|-)\s*',token)
        if len(parts)!=2:raise ValueError('Need exactly two star range endpoints')
        low,high=map(filled,parts)
    elif notation=='star-number':
        match=re.fullmatch(r'[★☆]\s*(?:[×x*]\s*)?(\d+)(\?)?',token)
        if not match:raise ValueError('Need an explicit numbered star rating')
        low=high=int(match[1])
    elif notation=='integer':
        if not re.fullmatch(r'\d+',token):raise ValueError('Need an explicit integer level')
        low=high=int(token)
    else:raise ValueError('Unknown source rating notation')
    if not 0<=low<=high<=999:raise ValueError('Invalid source rating range')
    return {'kind':'single'if low==high else'range','minimum':low,'maximum':high,'rawNotation':token,'notation':notation,'uncertain':token.endswith('?')}


def load_review(path,source_hash,entries):
    raw=path.read_bytes();review=json.loads(raw)
    if review.get('schema')!='ddi-reviewed-source-ratings/1' or review.get('sourceManifestSha256')!=source_hash:raise ValueError('Rating review must match the exact normalized source manifest')
    indexed={e['id']:e for e in entries}
    if set(review['charts'])-set(indexed):raise ValueError('Rating review refers to an unknown chart')
    result={};bodies={}
    for ident,row in review['charts'].items():
        entry=indexed[ident];metadata=entry['metadata']
        if row['chartIdentity']!={'mode':entry['chart']['mode'],'label':metadata.get('difficultyLabel',metadata.get('label',''))}:raise ValueError('Rating chart identity mismatch')
        extracted=[]
        for evidence in row['ratings']:
            binding=evidence['evidence'];relative=Path(binding['path'])
            if relative.is_absolute()or '..'in relative.parts:raise ValueError('Unsafe rating evidence path')
            file=path.parent
            for part in relative.parts:
                file=file/part
                if file.is_symlink():raise ValueError('Rating evidence symlinks are not allowed')
            file=file.resolve(strict=True)
            if not file.is_relative_to(path.parent.resolve())or not file.is_file()or file.stat().st_size>8*1024*1024:raise ValueError('Invalid rating evidence file')
            if file not in bodies:
                body=file.read_bytes();bodies[file]=(body,digest(body))
            body,sha=bodies[file]
            if sha!=binding['sha256']:raise ValueError('Rating evidence changed')
            if not isinstance(binding['url'],str)or not binding['url'].startswith(('http://','https://'))or not binding.get('retrievedAt'):raise ValueError('Rating evidence needs retrieval provenance')
            text=visible_text(body,evidence['encoding']);excerpt=evidence['excerpt'];token=evidence['token']
            matches=len(re.findall(r'(?<!\d)'+re.escape(token)+r'(?!\d)',excerpt))if evidence['notation']=='integer'else excerpt.count(token)
            if not excerpt or text.count(excerpt)!=1 or not token or matches!=1:raise ValueError('Rating excerpt/token must be unambiguous in the saved page')
            value=rating(evidence['notation'],token)
            if not isinstance(evidence['scale'],str)or not evidence['scale']:raise ValueError('Rating needs its publisher scale')
            extracted.append({**value,'scale':evidence['scale'],'authority':evidence['authority'],'excerpt':excerpt,'source':binding,'encoding':evidence['encoding'],'method':'reviewed exact visible-HTML excerpt; numeric notation parsed by tool'})
        selected=row['selectedRating']
        if type(selected)is not int or not 0<=selected<len(extracted):raise ValueError('Invalid selected source rating')
        value=extracted[selected]
        if value['uncertain']:raise ValueError('An uncertain publisher rating cannot set the meter')
        stars=value['maximum']
        if entry['stars']is not None and entry['stars']!=stars:raise ValueError('Page review conflicts with an extracted runtime rating')
        result[ident]={'ratings':extracted,'selectedRating':selected,'meter':stars,'selectionReason':row['selectionReason'],
            'meterPolicy':'source count'if value['kind']=='single'else'maximum of published source range'}
    return raw,result
