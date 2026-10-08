"""Projection/demux tests with original synthetic data, no acquired work assets."""
import copy
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zlib

from fixture import rect, tag
from higan import SWF_HASH, project
from swf_audio import demux, read_movie


def string(value):
    raw = value.encode("utf-16-le")
    return {"type":"string","text":value,"utf16":list(struct.unpack("<"+"H"*(len(raw)//2),raw)),"truncated":False}


class OriginalGraph:
    def __init__(self): self.objects = []
    def value(self,value):
        if isinstance(value,str): return string(value)
        if isinstance(value,list):
            return self.obj({"length":len(value),**{str(i):v for i,v in enumerate(value)}})
        if isinstance(value,dict) and value.get("type") == "undefined":return value
        if isinstance(value,dict):return self.obj(value)
        return value
    def obj(self,props):
        obj = {"id":len(self.objects),"kind":"object","properties":[],"truncated":False}
        self.objects.append(obj)
        obj["properties"] = [{"name":string(k),"value":self.value(v),"accessor":False,"enumerable":True} for k,v in props.items()]
        return {"ref":obj["id"]}


def original_capture():
    captures = []
    for frame,stage in [(52,"split"),(53,"prepared"),(55,"cleanup"),(201,"audio")]:
        g = OriginalGraph()
        root = {"keyLabel":"5","difficulty":1,"diffin":"","motion":"OFF","correction":0,
            "set_cnt":100,"startFrame":0,"frame_num":145 if stage == "audio" else 0,
            "keyCtrl":[37,300,40,32,38,301,39],"dif_spi":"1.5","set_spi":3,
            "musicTitle":"Original synthetic chart","tuning":"Synthetic author","diffi":"Normal",
            "mkArrow":{"100":[0],"160":[2],"220":[3],"280":[4],"340":[6]},
            "mkFArrow":{"400":[0]},"frzLength":[[60],[],[],[],[],[],[]],
            "speed_dac":[800,"1.25"],"iro_dac":[750,"0xff0000","al",750,"0x00ff00","oni"],"stop_dac":[""]}
        if stage in ("split","prepared"):
            root["timeline"] = [["200"],["$unused"],["260"],["320"],["380"],["$unused"],["440"]]
            root["ftimeline"] = [["500","560"],[],[],{"type":"undefined"},[],[],[]]
            if stage == "prepared":
                for index in [0,2,3,4,6]: root["timeline"][index][0] = int(root["timeline"][index][0])-100
                root["ftimeline"][0][0] = 400
        ref = g.obj(root)
        state = {"objects":g.objects,"roots":[{"level":0,"value":ref,"movieFrame":149 if stage == "audio" else 4}],
            "displayTree":[{"name":string("music_act"),"movieFrame":2}] if stage == "audio" else [],
            "incomplete":False,"avm1Halted":False}
        captures.append({"frame":frame,"state":state})
    return {"source":{"movieSha256":SWF_HASH},"avm1Halted":False,"audit":{"runtimeErrors":0,"truncated":False},
        "build":{"release":"synthetic-test"},"captures":captures,
        "audio":{"streamStarts":[{"iteration":201,"outputSampleFrame":147000,"compression":"Mp3","latencySeek":0}]}}


def mutate_root(capture,frame,name,value):
    state = next(c["state"] for c in capture["captures"] if c["frame"] == frame)
    props = state["objects"][state["roots"][0]["value"]["ref"]]["properties"]
    next(p for p in props if p["name"]["text"] == name)["value"] = value


class ProjectionTests(unittest.TestCase):
    def test_independent_stage_schedule_and_time_mapping(self):
        ref = project(original_capture())
        self.assertEqual([len(l["tapsSeconds"]) for l in ref["lanes"]],[1,1,1,1,1])
        self.assertEqual(ref["lanes"][0]["tapsSeconds"],[55/60])
        self.assertEqual(ref["lanes"][0]["holdsSeconds"],[[355/60,415/60]])
        self.assertEqual(ref["speed"],[{"nativeFrame":800,"seconds":655/60,"multiplier":1.25}])
        self.assertEqual(len(ref["colors"]),6)
        self.assertEqual(ref["colors"][-1],{"seconds":505/60,"target":4,"color":"#00ff00","all":True})
        self.assertEqual(ref["nativeKeyCodes"],[37,40,38,39,32])

    def test_invalid_evidence_never_becomes_export(self):
        for kind in ["hash","halt","audit","budget","settings","origin","missing-stage","schedule","hold"]:
            with self.subTest(kind=kind):
                c = original_capture()
                if kind == "hash": c["source"]["movieSha256"] = "0"*64
                elif kind == "halt": c["avm1Halted"] = True
                elif kind == "audit": c["audit"]["truncated"] = True
                elif kind == "budget": c["captures"][0]["state"]["incomplete"] = True
                elif kind == "settings": mutate_root(c,53,"correction",1)
                elif kind == "origin": mutate_root(c,201,"frame_num",144)
                elif kind == "missing-stage": c["captures"].pop(1)
                else:
                    s = c["captures"][1]["state"]
                    # Damage a stored numeric value in the compiled schedule/duration.
                    name = "mkArrow" if kind == "schedule" else "frzLength"
                    r = next(p["value"] for p in s["objects"][0]["properties"] if p["name"]["text"] == name)
                    obj = s["objects"][r["ref"]]
                    child = next(p["value"] for p in obj["properties"] if p["name"]["text"] == ("100" if kind == "schedule" else "0"))
                    next(p for p in s["objects"][child["ref"]]["properties"] if p["name"]["text"] == "0")["value"] = 6 if kind == "schedule" else 61
                with self.assertRaises((ValueError,KeyError,TypeError)): project(c)


class DemuxTests(unittest.TestCase):
    def swf(self,latency=0,bad_seek=False,bad_samples=False,compressed=False):
        # Original valid-sized Layer III silence frames, no third-party encoder output.
        packet = b"\xff\xfb\x90\x00"+bytes(413)
        head = tag(18,b"\x0f\x2f"+struct.pack("<Hh",735,latency))
        blocks = [(0,0,b""),(1152,734 if bad_seek else 735,packet),(0,0,b""),(1151 if bad_samples else 1152,1053,packet)]
        tags = head
        for samples,seek,payload in blocks:
            # All SoundStreamBlock records use long headers, including empty blocks.
            data = struct.pack("<Hh",samples,seek)+payload
            tags += struct.pack("<HI",19<<6|63,len(data))+data+tag(1)
        tags += tag(0)
        body = rect(320,240)+struct.pack("<HH",60*256,4)+tags
        return (b"CWS" if compressed else b"FWS")+b"\x08"+struct.pack("<I",8+len(body))+(zlib.compress(body) if compressed else body),packet

    def test_empty_long_blocks_and_continuous_seek_metadata(self):
        for compressed in [False,True]:
            with tempfile.TemporaryDirectory(prefix="ddi-swf-test-") as directory:
                data,packet = self.swf(compressed=compressed);path = Path(directory)/"fixture.swf";path.write_bytes(data)
                original,encoded,info = demux(path,"root")
                self.assertEqual(original,data);self.assertEqual(encoded,packet*2)
                self.assertEqual(info["declaredSampleFrames"],2304)
                self.assertTrue(info["continuousZeroLatencyEligible"])
                self.assertEqual(info["blocks"][0]["sampleCount"],0)
                self.assertEqual(info["blocks"][1]["seekSamples"],735)

    def test_unknown_timing_and_malformed_bytes(self):
        with tempfile.TemporaryDirectory(prefix="ddi-swf-test-") as directory:
            path = Path(directory)/"fixture.swf"
            for kwargs in [{"latency":1},{"bad_seek":True}]:
                path.write_bytes(self.swf(**kwargs)[0])
                self.assertFalse(demux(path,"root")[2]["continuousZeroLatencyEligible"])
            path.write_bytes(self.swf(bad_samples=True)[0])
            with self.assertRaises(ValueError): demux(path,"root")
            data,_ = self.swf()
            for invalid in [data[:-1],data+b"unexpected",b"CWS\x08"+struct.pack("<I",12)+zlib.compress(bytes(100000))]:
                path.write_bytes(invalid)
                with self.assertRaises(ValueError): read_movie(path)


if __name__ == "__main__": unittest.main()
