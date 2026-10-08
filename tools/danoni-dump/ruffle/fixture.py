#!/usr/bin/env python3
"""Generate an original two-frame AVM1 fixture; no Flash authoring tool required.

Opcode reference: Ruffle v0.7.1 swf/src/avm1/opcode.rs and write.rs.
Expected values in test_native.py are specified independently of the snapshot.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct


def action(opcode, payload=b""):
    return bytes([opcode]) + (struct.pack("<H", len(payload)) + payload if opcode >= 0x80 else b"")


UNDEFINED = object()


def push(value):
    if value is UNDEFINED:
        data = b"\x03"
    elif value is None:
        data = b"\x02"
    elif isinstance(value, str):
        data = b"\x00" + value.encode() + b"\x00"
    elif isinstance(value, int):
        data = b"\x07" + struct.pack("<i", value)
    else:
        data = b"\x01" + struct.pack("<f", value)
    return action(0x96, data)


def get(name):
    return push(name) + action(0x1c)


def assign(name, expression):
    return push(name) + expression + action(0x1d)


def array(values):
    return b"".join(push(v) for v in reversed(values)) + push(len(values)) + action(0x42)


def member(obj, name, expression):
    return obj + push(name) + expression + action(0x4f)


def draw(colour):
    code = b""
    for method, args in [("clear",[]),("beginFill",[colour,100]),("moveTo",[20,20]),
                         ("lineTo",[100,20]),("lineTo",[100,100]),("lineTo",[20,100]),
                         ("lineTo",[20,20]),("endFill",[])]:
        code += b"".join(push(arg) for arg in reversed(args)) + push(len(args))
        code += get("_root") + push(method) + action(0x52) + action(0x17)
    return code


def tag(code, data=b""):
    size = len(data)
    return struct.pack("<H", code << 6 | min(size, 63)) + (struct.pack("<I", size) if size >= 63 else b"") + data


def rect(width, height):
    values = [0, width * 20, 0, height * 20]
    nbits = max(values).bit_length() + 1
    bits = f"{nbits:05b}" + "".join(f"{v:0{nbits}b}" for v in values)
    bits += "0" * (-len(bits) % 8)
    return int(bits, 2).to_bytes(len(bits)//8, "big")


def movie(actions, second=b"", frames=2):
    tags = tag(9, bytes([17,34,51])) + tag(12, actions + b"\x00") + tag(1)
    if frames == 2:
        tags += tag(12, second + action(7) + b"\x00") + tag(1)
    body = rect(320,240) + struct.pack("<HH", 30*256, frames) + tags + tag(0)
    return b"FWS\x07" + struct.pack("<I",8+len(body)) + body


def create(directory, fractional=True):
    directory.mkdir(parents=True, exist_ok=False)
    first = assign("getterCalls",push(0))
    first += assign("chart", push(0) + action(0x43))
    for lane, notes in [("left",[120,240.5 if fractional else 240]),("down",[]),("up",[180]),("right",[300]),("space",[360])]:
        first += member(get("chart"),lane,array(notes))
    first += member(get("chart"),"frzLeft",array([480,600]))
    first += member(get("chart"),"fps",push(60))
    first += member(get("chart"),"title",push("合成譜面"))
    first += member(get("chart"),"self",get("chart"))
    first += assign("alias",get("chart"))
    first += assign("child",push(37)+push("laneBank")+push(2)+get("_root")+push("createEmptyMovieClip")+action(0x52))
    first += member(get("child"),"notes",array([90,210]))
    first += assign("sparse",array([]))
    first += member(get("sparse"),0,push(UNDEFINED))
    first += member(get("sparse"),2,push(42))
    first += member(get("sparse"),"length",push(5))
    first += assign("nonfinite",array([float("nan"),float("inf"),-float("inf"),-0.0,None]))
    getter = assign("getterCalls",get("getterCalls") + push(1) + action(0x0a)) + push(999) + action(0x3e)
    # DefineFunction's length excludes the function body (SWF encoding rule).
    first += action(0x9b,b"trapGetter\x00" + struct.pack("<HH",0,len(getter))) + getter
    first += push(None) + get("trapGetter") + push("trap") + push(3) + get("chart") + push("addProperty") + action(0x52) + action(0x17)
    # Browser navigation and a URL load must be recorded as denied, not performed.
    first += action(0x83,b"https://example.invalid/should-not-open\x00_blank\x00")
    first += push("https://example.invalid/should-not-load") + push("_root") + action(0x9a,b"\xc0")
    first += push("fixture-data.txt") + push("_root") + action(0x9a,b"\xc0")
    first += draw(0xc85028)
    poll = assign("polledEnter",push(13)+push(1)+get("Key")+push("isDown")+action(0x52))
    first += poll
    second = member(get("chart") + push("left") + action(0x4e),0,push(180))
    second += draw(0x28a050) + poll
    (directory/"fixture.swf").write_bytes(movie(first,second))
    resource = b"cachedMarker=from-cache&number=17"
    (directory/"fixture-data.txt").write_bytes(resource)
    config = {"movie":"fixture.swf", "sourceUrl":"https://example.invalid/fixture.swf?query=retained",
              "pageUrl":"https://example.invalid/work.html", "flashVars":{"difficulty":"Hard"},
              "captureFrames":[1,2], "render":True,
              "resources":[{"url":"https://example.invalid/fixture-data.txt","path":"fixture-data.txt",
                            "sha256":hashlib.sha256(resource).hexdigest()}]}
    (directory/"run.json").write_text(json.dumps(config,indent=2)+"\n")
    # Infinite AVM1 loop must be bounded by the interpreter and outer process.
    (directory/"loop.swf").write_bytes(movie(first + action(0x99,struct.pack("<h",-5)),frames=1))
    (directory/"loop.json").write_text(json.dumps({**config,"movie":"loop.swf","captureFrames":[1],"render":False})+"\n")
    for name, extra in [("budget",assign("large",array([1]*20010))),
                        ("audit",(push("synthetic trace")+action(0x26))*300)]:
        (directory/f"{name}.swf").write_bytes(movie(first+extra,frames=1))
        (directory/f"{name}.json").write_text(json.dumps({**config,"movie":f"{name}.swf","captureFrames":[1],"render":False})+"\n")
    (directory/"input.json").write_text(json.dumps({**config,"events":[{"frame":1,"type":"enter"}]})+"\n")
    # Original stereo PCM impulses; audio starts one movie frame after startup.
    samples = [(16384,-16384) if i in (0,734,1469) else (0,0) for i in range(1470)]
    pcm = b"".join(struct.pack("<hh",*sample) for sample in samples)
    stream_tags = tag(45,b"\x0f\x3f"+struct.pack("<H",1470))+tag(1)
    stream_tags += tag(19,pcm)+tag(1)+tag(12,action(7)+b"\x00")+tag(1)+tag(0)
    body = rect(320,240)+struct.pack("<HH",30*256,3)+stream_tags
    (directory/"pcm.swf").write_bytes(b"FWS\x08"+struct.pack("<I",8+len(body))+body)
    (directory/"pcm.json").write_text(json.dumps({**config,"movie":"pcm.swf",
        "captureFrames":[1,2,3],"render":False,"mixAudio":True,"resources":[]})+"\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory",type=Path)
    parser.add_argument("--integer",action="store_true")
    args = parser.parse_args()
    create(args.directory, fractional=not args.integer)
