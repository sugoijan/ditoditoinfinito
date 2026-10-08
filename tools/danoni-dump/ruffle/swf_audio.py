"""Bounded, non-executing SWF MP3 stream demux and timing metadata.

SWF19 pp.182-186: continuous playback concatenates MP3 frames; block SeekSamples
is used on timeline seeking. Keep all headers; never discard unknown timing.
"""
import hashlib
from pathlib import Path
import struct
import zlib

MAX_SWF = 16*1024*1024
MAX_DECODED = 64*1024*1024


def read_movie(path):
    with Path(path).open("rb") as file:
        original = file.read(MAX_SWF+1)
    if len(original) > MAX_SWF or len(original) < 12 or original[:3] not in (b"FWS",b"CWS"):
        raise ValueError("Only bounded FWS/CWS input is supported")
    declared = struct.unpack_from("<I",original,4)[0]
    if not 12 <= declared <= MAX_DECODED:
        raise ValueError("Invalid declared SWF length")
    if original[:3] == b"CWS":
        decoder = zlib.decompressobj()
        body = decoder.decompress(original[8:],declared-8+1)
        if not decoder.eof or decoder.unconsumed_tail or decoder.unused_data:
            raise ValueError("Invalid or oversized compressed SWF")
        data = original[:8]+body
    else:
        data = original
    if len(data) != declared:
        raise ValueError("SWF size does not match header")
    bits = data[8] >> 3
    offset = 8+(5+4*bits+7)//8
    if bits == 0 or offset+4 > len(data):
        raise ValueError("Invalid stage RECT")
    fps, frames = struct.unpack_from("<HH",data,offset)
    if not 1 <= fps/256 <= 240 or frames == 0:
        raise ValueError("Unsupported SWF frame rate/count")
    return original,data,{"fps":fps/256,"frames":frames,"tagStart":offset+4,
        "swfVersion":data[3],"sha256":hashlib.sha256(original).hexdigest()}


def iter_tags(data, start, end, path="root", depth=0, budget=None):
    if depth > 32:
        raise ValueError("SWF sprite nesting limit")
    budget = [200000] if budget is None else budget
    pos, frame = start, 1
    while pos+2 <= end:
        budget[0] -= 1
        if budget[0] < 0:
            raise ValueError("SWF tag limit")
        offset = pos
        header = struct.unpack_from("<H",data,pos)[0]; pos += 2
        code, size = header >> 6, header & 63
        if size == 63:
            if pos+4 > end:
                raise ValueError("Truncated long tag header")
            size = struct.unpack_from("<I",data,pos)[0]; pos += 4
        if pos+size > end:
            raise ValueError("Truncated tag payload")
        payload = memoryview(data)[pos:pos+size]
        yield {"path":path,"frame":frame,"code":code,"offset":offset,"payload":payload}
        if code == 39:
            if size < 4:
                raise ValueError("Truncated sprite")
            sprite_id = struct.unpack_from("<H",payload)[0]
            yield from iter_tags(data,pos+4,pos+size,f"{path}/sprite{sprite_id}",depth+1,budget)
        pos += size
        if code == 0:
            if pos != end:
                raise ValueError("Unexpected bytes after End tag")
            return
        if code == 1:
            frame += 1
    raise ValueError("Missing End tag")


def mp3_info(data):
    """Read whole MPEG Layer III frame headers; never decode or execute payloads."""
    pos, samples, frames, formats = 0, 0, 0, set()
    while pos < len(data):
        if len(data)-pos < 4:
            raise ValueError("Truncated MPEG header")
        header = int.from_bytes(data[pos:pos+4],"big")
        version, layer = (header>>19)&3, (header>>17)&3
        bitrate, rate_index, pad = (header>>12)&15, (header>>10)&3, (header>>9)&1
        if header & 0xffe00000 != 0xffe00000 or version == 1 or layer != 1 or bitrate in (0,15) or rate_index == 3:
            raise ValueError("Unsupported or malformed MPEG Layer III frame")
        rate = [44100,48000,32000][rate_index]//{3:1,2:2,0:4}[version]
        rates = [0,32,40,48,56,64,80,96,112,128,160,192,224,256,320] if version == 3 else [0,8,16,24,32,40,48,56,64,80,96,112,128,144,160]
        size = (144000 if version == 3 else 72000)*rates[bitrate]//rate+pad
        if pos+size > len(data):
            raise ValueError("MP3 frame crosses block boundary")
        formats.add((rate,1 if (header>>6)&3 == 3 else 2))
        samples += 1152 if version == 3 else 576
        frames += 1; pos += size
    return samples,frames,formats


def demux(path, selected_path):
    original,data,movie = read_movie(path)
    streams, active = [], {}
    for tag in iter_tags(data,movie["tagStart"],len(data)):
        route, code, payload = tag["path"],tag["code"],tag["payload"]
        if code in (18,45):
            if len(payload) < 4:
                raise ValueError("Truncated stream head")
            packed = payload[1]; compression = packed >> 4
            head = {"schema":"ddi-swf-audio/experimental-1","path":route,
                "headTagOffset":tag["offset"],"headMovieFrame":tag["frame"],
                "compression":compression,"sampleRate":[5512,11025,22050,44100][(packed>>2)&3],
                "channels":1+(packed&1),"samplesPerMovieFrame":struct.unpack_from("<H",payload,2)[0],
                "latencySeek":struct.unpack_from("<h",payload,4)[0] if compression == 2 and len(payload) >= 6 else None,
                "blocks":[],"bytes":bytearray(),"movie":movie}
            streams.append(head); active[route] = head
        elif code == 19:
            if route not in active:
                raise ValueError("SoundStreamBlock without head")
            head = active[route]
            if head["compression"] != 2:
                continue
            if len(payload) < 4:
                raise ValueError("Truncated MP3 stream block")
            samples, seek = struct.unpack_from("<Hh",payload)
            chunk = bytes(payload[4:]); actual,frames,formats = mp3_info(chunk)
            if actual != samples or formats - {(head["sampleRate"],head["channels"])}:
                raise ValueError("MP3 block sample count/format mismatch")
            head["blocks"].append({"movieFrame":tag["frame"],"tagOffset":tag["offset"],
                "sampleCount":samples,"seekSamples":seek,"mp3Frames":frames,
                "mp3Offset":len(head["bytes"]),"mp3Bytes":len(chunk),"sha256":hashlib.sha256(chunk).hexdigest()})
            head["bytes"].extend(chunk)
    selected = [s for s in streams if s["path"] == selected_path]
    if len(selected) != 1 or selected[0]["compression"] != 2 or not selected[0]["blocks"]:
        raise ValueError("Select exactly one MP3 stream with blocks")
    head = selected[0]; encoded = bytes(head.pop("bytes")); blocks = head["blocks"]
    origin = blocks[0]["movieFrame"]; total = 0; regular = True
    for block in blocks:
        block["sourceSampleOffset"] = total
        ideal = (block["movieFrame"]-origin)*head["sampleRate"]/movie["fps"]
        block["seekResidualSamples"] = total+block["seekSamples"]-ideal-(head["latencySeek"] or 0)
        if block["mp3Bytes"] and abs(block["seekResidualSamples"]) > 1e-9:
            regular = False
        total += block["sampleCount"]
    contiguous = [b["movieFrame"] for b in blocks] == list(range(origin,origin+len(blocks)))
    head.update(mp3Sha256=hashlib.sha256(encoded).hexdigest(),mp3Bytes=len(encoded),
        declaredSampleFrames=total,firstBlockMovieFrame=origin,
        continuousZeroLatencyEligible=regular and contiguous and head["latencySeek"] == 0 and blocks[0]["seekSamples"] == 0,
        transformation="Concatenate whole MP3 payloads after each four-byte samples/seek header; no decoding, trimming or transcoding")
    return original,encoded,head
