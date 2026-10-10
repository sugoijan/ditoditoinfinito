#!/usr/bin/env python3
"""Integration checks using only original synthetic SWFs and temporary files."""
import argparse
import json
from pathlib import Path
import socket
import struct
import tempfile
import time

from fixture import create
from run import run
from export_fixture import export, numbers
from paths import external_directory


def properties(state, reference):
    obj = state["objects"][reference["ref"]]
    assert obj["id"] == reference["ref"] and not obj["truncated"]
    return {p["name"]["text"]:p["value"] for p in obj["properties"]}


def assert_snapshot(snapshot):
    assert snapshot["swfHeaderFps"] == 30
    assert len(snapshot["captures"]) == 2
    for frame, capture in enumerate(snapshot["captures"],1):
        assert capture["frame"] == frame
        state = capture["state"]
        assert not state["incomplete"] and len(state["roots"]) == 1
        root = properties(state,state["roots"][0]["value"])
        assert root["getterCalls"] == 0, "inspection executed an ActionScript getter"
        assert root["difficulty"]["text"] == "Hard"
        assert root["query"]["text"] == "retained"
        assert root["cachedMarker"]["text"] == "from-cache"
        assert root["chart"] == root["alias"]
        displays = state["displayTree"]
        child = next(d for d in displays if d["name"] and d["name"]["text"] == "laneBank")
        assert child["parent"] == 0 and child["kind"] == "movie-clip"
        assert child["stageRegistrationPixels"] == [13.0,17.0]
        child_notes = properties(state,properties(state,child["object"])["notes"])
        assert child_notes["length"] == 2
        assert [child_notes["0"],child_notes["1"]] == [90,210]
        chart = properties(state,root["chart"])
        assert chart["self"] == root["chart"]
        assert chart["fps"] == 60  # Chart clock is deliberately different from SWF FPS.
        assert chart["title"]["text"] == "合成譜面"
        assert chart["trap"] == {"type":"uninspected","reason":"accessor"}
        for lane, expected in [("left",[120 if frame == 1 else 180,240.5]),
                               ("down",[]),("up",[180]),("right",[300]),("space",[360]),
                               ("frzLeft",[480,600])]:
            notes = properties(state,chart[lane])
            assert notes["length"] == len(expected)
            assert [notes[str(i)] for i in range(len(expected))] == expected
        sparse = properties(state,root["sparse"])
        assert sparse["length"] == 5 and sparse["0"] == {"type":"undefined"}
        assert sparse["2"] == 42 and not {"1","3","4"} & sparse.keys()
        nonfinite = properties(state,root["nonfinite"])
        for i, expected in enumerate(["NaN","Infinity","-Infinity","-0"]):
            assert nonfinite[str(i)] == {"type":"number","value":expected}
        assert nonfinite["4"] is None
    assert not snapshot["avm1Halted"] and not snapshot["audit"]["truncated"]
    kinds = {entry["kind"] for entry in snapshot["audit"]["entries"]}
    assert {"navigation-denied","fetch-denied","cache-hit"} <= kinds, snapshot["audit"]


def test(worker, directory):
    for value in [float("nan"),float("inf"),-float("inf")]:
        fake = {"objects":[{"id":0,"truncated":False,"properties":[
            {"name":{"text":"length"},"value":1},{"name":{"text":"0"},"value":value}]}]}
        try:
            numbers(fake,{"ref":0})
            raise AssertionError("Nonfinite note accepted")
        except ValueError:
            pass
    inputs = directory/"inputs"
    create(inputs)
    fake_checkout = directory/"fake-checkout"
    (fake_checkout/".git").mkdir(parents=True)
    try:
        external_directory(fake_checkout/"dump")
        raise AssertionError("Git output guard failed")
    except ValueError:
        pass
    secret = directory/"outside-secret"
    secret.write_text("synthetic denied data")
    (inputs/"escape-link").symlink_to(secret)
    with socket.socket() as listener:
        listener.bind(("127.0.0.1",0))
        listener.listen()
        address = f"127.0.0.1:{listener.getsockname()[1]}"
        for name, target, readable in [("outside",secret,False),("symlink",inputs/"escape-link",False),
                                       ("readonly-input",inputs/"fixture.swf",True)]:
            report = run(worker,inputs/"run.json",directory/name,probe=[str(target),address])
            assert report["exitCode"] == 0, (directory/name/"stderr.txt").read_text()
            actual = json.loads((directory/name/"snapshot.json").read_text())
            assert actual == {"readAllowed":readable,"writeAllowed":False,"networkAllowed":False}, actual
    captures = []
    for name in ["first","repeat"]:
        report = run(worker,inputs/"run.json",directory/name)
        assert report["exitCode"] == 0, (directory/name/"stderr.txt").read_text()
        snapshot = json.loads((directory/name/"snapshot.json").read_text())
        assert_snapshot(snapshot)
        captures.append(snapshot)
        for frame, rgb in [(1,bytes([200,80,40])),(2,bytes([40,160,80]))]:
            data = (directory/name/f"frame-{frame}.ppm").read_bytes()
            header = b"P6\n320 240\n255\n"
            assert data.startswith(header) and len(data) == len(header)+320*240*3
            pixels = data[len(header):]
            assert pixels[:3] == bytes([17,34,51]), "background pixels differ"
            offset = (50*320+50)*3
            assert pixels[offset:offset+3] == rgb, "ActionScript drawing pixels differ"
    assert captures[0] == captures[1], "fresh worker captures are not reproducible"
    scaled_config=json.loads((inputs/'run.json').read_text());scaled_config['renderScale']=4
    scaled_config['events']=[{'frame':1,'type':'click','x':50,'y':50}]
    (inputs/'scaled.json').write_text(json.dumps(scaled_config))
    assert run(worker,inputs/'scaled.json',directory/'scaled')['exitCode']==0
    scaled=json.loads((directory/'scaled/snapshot.json').read_text())
    assert scaled['renderScale']==4
    for captured in scaled['captures']:
        root=properties(captured['state'],captured['state']['roots'][0]['value'])
        assert abs(root['mouseX']-50)<.05 and abs(root['mouseY']-50)<.05, 'scaled press/release must keep logical coordinates'
    for frame,rgb in [(1,bytes([200,80,40])),(2,bytes([40,160,80]))]:
        image=(directory/'scaled'/f'frame-{frame}.ppm').read_bytes()
        header=b'P6\n1280 960\n255\n';assert image.startswith(header)
        assert len(image)==len(header)+1280*960*3
        offset=(200*1280+200)*3+len(header);assert image[offset:offset+3]==rgb
    for value in [0,5,1.5,'4']:
        (inputs/'invalid-scale.json').write_text(json.dumps({**scaled_config,'renderScale':value}))
        assert run(worker,inputs/'invalid-scale.json',directory/('invalid-scale-'+str(value)))['exitCode']!=0
    result = run(worker,inputs/"pcm.json",directory/"pcm")
    assert result["exitCode"] == 0, (directory/"pcm/stderr.txt").read_text()
    audio = json.loads((directory/"pcm/snapshot.json").read_text())["audio"]
    assert audio["sampleFrames"] == 4410 and len(audio["streamStarts"]) == 1
    assert audio["streamStarts"][0]["iteration"] == 2
    assert audio["streamStarts"][0]["outputSampleFrame"] == 1470
    data = (directory/"pcm/audio.pcm").read_bytes()
    assert len(data) == 4410*4
    impulses = {1470,1470+734,1470+1469}
    for i, sample in enumerate(struct.iter_unpack("<hh",data)):
        assert sample == ((16384,-16384) if i in impulses else (0,0)), (i,sample)
    for name in ["budget","audit"]:
        result = run(worker,inputs/f"{name}.json",directory/name)
        assert result["exitCode"] == 0
        data = json.loads((directory/name/"snapshot.json").read_text())
        assert data["captures"][0]["state"]["incomplete"] if name == "budget" else data["audit"]["truncated"]
        try:
            export(directory/name/"snapshot.json",directory/f"bad-{name}-export")
            raise AssertionError("Incomplete evidence accepted for export")
        except ValueError:
            pass
    result = run(worker,inputs/"input.json",directory/"polled-input")
    assert result["exitCode"] == 0
    data = json.loads((directory/"polled-input/snapshot.json").read_text())
    for capture, expected in zip(data["captures"],[True,False]):
        state = capture["state"]
        assert properties(state,state["roots"][0]["value"])["polledEnter"] is expected
    result = run(worker,inputs/"global-input.json",directory/"global-key-input")
    assert result["exitCode"] == 0
    data = json.loads((directory/"global-key-input/snapshot.json").read_text())
    for capture in data["captures"]:
        state = capture["state"]
        assert not state["incomplete"]
        root = properties(state,state["roots"][0]["value"])
        assert root["polledRight"] is True and root["polledEnter"] is False
        assert root["getterCalls"] == 0
        scope = next(s for s in state["globalScopes"] if s["caseSensitive"])
        stored = {p["name"]["text"]:p["value"] for p in scope["properties"]}
        assert stored["ddiFixtureGlobal"] == root["chart"]
        assert properties(state,stored["ddiFixtureGlobal"])["trap"] == {"type":"uninspected","reason":"accessor"}
    for label, event in [("unknown-key",{"frame":1,"type":"key","code":"F13"}),
                         ("long-key",{"frame":1,"type":"key","code":"ArrowRight","holdFrames":121})]:
        config = json.loads((inputs/"run.json").read_text());config["events"]=[event]
        (inputs/f"{label}.json").write_text(json.dumps(config))
        assert run(worker,inputs/f"{label}.json",directory/label)["exitCode"] != 0
    corrupt = json.loads((inputs/"run.json").read_text())
    corrupt["resources"][0]["sha256"] = "0"*64
    (inputs/"corrupt.json").write_text(json.dumps(corrupt))
    invalid = run(worker,inputs/"corrupt.json",directory/"corrupt-cache")
    assert invalid["exitCode"] != 0 and "hash mismatch" in (directory/"corrupt-cache/stderr.txt").read_text()
    loud=run(worker,inputs/'loud.json',directory/'loud')
    assert loud['exitCode']==0
    samples=struct.unpack('<'+'h'*((directory/'loud/audio.pcm').stat().st_size//2),(directory/'loud/audio.pcm').read_bytes())
    assert max(samples)==32767 and min(samples)==-32768, 'overlapping loud sounds must saturate PCM16'
    for label, extra in [('too-many-frames',{'captureFrames':[1201]}), ('bad-budget',{'avmExecutionMs':10001}),
                         ('empty-text',{'events':[{'frame':1,'type':'text','value':''}]}),
                         ('control-text',{'events':[{'frame':1,'type':'text','value':'\n'}]})]:
        cfg=json.loads((inputs/'run.json').read_text());cfg.update(extra)
        (inputs/(label+'.json')).write_text(json.dumps(cfg))
        assert run(worker,inputs/(label+'.json'),directory/label)['exitCode']!=0
    started = time.monotonic()
    loop = run(worker,inputs/"loop.json",directory/"loop",timeout=5)
    assert time.monotonic()-started < 7 and loop["termination"] in (None,"wall-time-limit")
    assert loop["exitCode"] != 0, "halted AVM must not report success"
    if loop["termination"] is None:
        snapshot = json.loads((directory/"loop/snapshot.json").read_text())
        assert snapshot["avm1Halted"] and snapshot["audit"]["runtimeErrors"] > 0
        try:
            export(directory/"loop/snapshot.json",directory/"bad-halted-export")
            raise AssertionError("Halted initialized chart accepted for export")
        except ValueError:
            pass
    # Exercise the independent outer timeout even when the interpreter cooperates.
    deadline = run(worker,inputs/"loop.json",directory/"deadline",timeout=0)
    assert deadline["termination"] == "wall-time-limit" and deadline["exitCode"] != 0
    memory = run(worker,inputs/"run.json",directory/"memory",memory_mb=0)
    assert memory["termination"] == "resident-memory-limit" and memory["exitCode"] != 0
    print(json.dumps({"passed":True,"directory":str(directory),
        "checks":["outside read/write denied","input write denied","symlink escape denied","network denied",
            "AVM resource requests denied","exact before/after graph","getter not invoked","sparse and special values",
            "FlashVars and URL parameters","child movie-clip storage","two offscreen pixel buffers","4x native rendering and logical press/release coordinates","fresh-process repeat",
            "delayed PCM stream and exact impulses","loud overlapping PCM saturation","native registration points","text/budget/iteration bounds","cache integrity",
            "budget and audit exhaustion rejected","frame-polled key input","bounded held arrow keys","stored global data without getter invocation","halted initialized chart rejected",
            "Git output guard","outer timeout","outer memory limit"]},indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("worker",type=Path)
    parser.add_argument("--directory",type=Path)
    args = parser.parse_args()
    directory = args.directory or Path(tempfile.mkdtemp(prefix="ddi-ruffle-test-"))
    directory.mkdir(parents=True,exist_ok=True)
    test(args.worker,directory)
