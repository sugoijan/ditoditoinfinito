#!/usr/bin/env python3
"""Prove dos reconstruction for our synthetic AVM1 fixture (not a legacy adapter)."""
import argparse
import hashlib
import html
import json
import math
from pathlib import Path
import wave
from paths import external_directory

LANES = ["left","down","up","right","space"]


def properties(state, reference):
    obj = state["objects"][reference["ref"]]
    if obj["id"] != reference["ref"] or obj["truncated"]:
        raise ValueError("Invalid or truncated object")
    return {p["name"]["text"]:p["value"] for p in obj["properties"]}


def numbers(state, reference):
    props = properties(state,reference)
    length = props["length"]
    if not isinstance(length,(int,float)) or not 0 <= length <= 20000 or int(length) != length:
        raise ValueError("Invalid array length")
    result = [props[str(i)] for i in range(int(length))]
    if any(type(v) not in (float,int) or not math.isfinite(v) or v < 0 for v in result):
        raise ValueError("Only finite nonnegative numeric note frames may be exported")
    return result


def export(snapshot_path, directory):
    raw = snapshot_path.read_bytes()
    capture = json.loads(raw)
    if capture.get("avm1Halted") or capture.get("audit",{}).get("truncated"):
        raise ValueError("Halted execution or truncated audit cannot be exported")
    state = capture["captures"][-1]["state"]
    if state["incomplete"] or len(state["roots"]) != 1:
        raise ValueError("Incomplete or unexpected fixture snapshot")
    root = properties(state,state["roots"][0]["value"])
    chart = properties(state,root["chart"])
    if chart["title"]["text"] != "合成譜面" or chart["fps"] != 60:
        raise ValueError("This adapter only recognizes the original synthetic fixture")
    taps = {lane:numbers(state,chart[lane]) for lane in LANES}
    holds = numbers(state,chart["frzLeft"])
    if len(holds) % 2 or any(end < start for start,end in zip(holds[::2],holds[1::2])):
        raise ValueError("Invalid hold endpoints")
    headers = {"musicTitle":"Synthetic AVM1,DDI test,https://example.invalid/",
               "tuning":"DDI test,https://example.invalid/", "difData":"5,Fixture,3.5",
               "musicUrl":"../../media/synthetic.wav", "musicFolder":"./",
               "blankFrame":"0", "adjustment":"0", "playbackRate":"1", "startFrame":"0"}
    fields = {**headers, **{lane+"_data":",".join(format(v,".15g") for v in taps[lane]) for lane in LANES},
              "frzLeft_data":",".join(format(v,".15g") for v in holds)}
    text = "\n".join(f"|{name}={value}|" for name,value in fields.items())+"\n"
    directory = external_directory(directory)
    directory.mkdir(parents=True,exist_ok=False)
    chart_dir = directory/"charts/fixture"
    chart_dir.mkdir(parents=True)
    (chart_dir/"chart.dos.txt").write_text(text)
    (chart_dir/"chart.html").write_text('<!doctype html><meta charset="utf-8"><textarea hidden id="dos">'+html.escape(text)+'</textarea>\n')
    (chart_dir/"keys.json").write_text(json.dumps({"mode":"5","lanes":LANES},indent=2)+"\n")
    (directory/"media").mkdir()
    with wave.open(str(directory/"media/synthetic.wav"),"wb") as sound:
        sound.setnchannels(1); sound.setsampwidth(2); sound.setframerate(8000)
        sound.writeframes(b"\x00\x00"*(12*8000))
    expected = {"schema":"ddi-synthetic-export/experimental-1", "adapter":"original-avm1-fixture/1",
        "reconstructed":True,"sourceSnapshotSha256":hashlib.sha256(raw).hexdigest(),
        "captureFrame":capture["captures"][-1]["frame"],"sourceChartFps":60,"swfHeaderFps":capture["swfHeaderFps"],
        "audioOrigin":"fixture frame zero; generated silence is synthetic test media only",
        "charts":[{"id":"fixture","dos":"charts/fixture/chart.dos.txt","page":"charts/fixture/chart.html",
            "mode":"5","lanes":[{"name":lane,"tapsSeconds":[v/60 for v in taps[lane]],
                "holdsSeconds":[[a/60,b/60] for a,b in zip(holds[::2],holds[1::2])] if lane == "left" else []}
                for lane in LANES]}],
        "portability":"unverified-until-CW-roundtrip"}
    (directory/"manifest.json").write_text(json.dumps(expected,indent=2)+"\n")
    return expected


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("snapshot",type=Path)
    parser.add_argument("output",type=Path)
    args = parser.parse_args()
    export(args.snapshot,args.output)
