"""Project the pinned Higan pilot's observed AVM1 state into portable static data.

No ActionScript execution or source-text chart parsing occurs here. Three native
stages are cross-checked before output: split input, compiled arrays, final schedule.
"""
import argparse
from collections import Counter
import hashlib
import html
import json
import math
from pathlib import Path
import shutil

from paths import external_directory
from swf_audio import demux

SWF_HASH = "6ce53b126e90b25a9f307595bf226ed4e6ce8f70c74d816a1d372c7f20de6e85"
LANES = [(0,"left"),(2,"down"),(4,"up"),(6,"right"),(3,"space")]


def text(value):
    if not isinstance(value,dict) or value.get("type") != "string" or value.get("truncated"):
        raise ValueError("Expected complete native UTF-16 string")
    return b"".join(int(u).to_bytes(2,"little") for u in value["utf16"]).decode("utf-16-le","surrogatepass")


def numeric(value):
    if isinstance(value,dict):
        value = float(text(value))
    if type(value) not in (int,float) or not math.isfinite(value):
        raise ValueError("Expected finite numeric value")
    return value


class Graph:
    def __init__(self,state):
        if state["incomplete"] or state["avm1Halted"]:
            raise ValueError("Incomplete or halted native state")
        self.state = state

    def props(self,ref):
        obj = self.state["objects"][ref["ref"]]
        if obj["id"] != ref["ref"] or obj["truncated"]:
            raise ValueError("Invalid native object")
        result = {}
        for property in obj["properties"]:
            name = text(property["name"])
            if name in result:
                raise ValueError("Duplicate property")
            result[name] = property["value"]
        return result

    def array(self,ref):
        props = self.props(ref); length = numeric(props["length"])
        if not 0 <= length <= 20000 or int(length) != length:
            raise ValueError("Invalid native array length")
        return [props[str(i)] for i in range(int(length))]

    def root(self):
        roots = self.state["roots"]
        if len(roots) != 1 or roots[0]["level"] != 0:
            raise ValueError("Unexpected root levels")
        return self.props(roots[0]["value"])

    def schedule(self,ref):
        result = Counter()
        for key,value in self.props(ref).items():
            if not key.isdecimal():
                if key not in ("length","__proto__","__constructor__"):
                    raise ValueError("Unexpected schedule property")
                continue
            for lane in self.array(value):
                lane = numeric(lane)
                if int(lane) != lane or lane not in [n for n,_ in LANES]:
                    raise ValueError("Unsupported compiled lane")
                result[(int(key),int(lane))] += 1
        return result


def project(capture):
    if capture["source"]["movieSha256"] != SWF_HASH or capture["avm1Halted"] or capture["audit"]["truncated"] or capture["audit"]["runtimeErrors"]:
        raise ValueError("Unreviewed SWF or invalid execution evidence")
    split = prepared = final = None
    for item in capture["captures"]:
        graph = Graph(item["state"]); root = graph.root()
        if "timeline" in root:
            head = graph.array(graph.array(root["timeline"])[0])[0]
            if isinstance(head,dict) and head.get("type") == "string":
                split = (item,graph,root)
            elif type(head) in (float,int):
                prepared = (item,graph,root)
        elif "mkArrow" in root and item["state"]["roots"][0]["movieFrame"] == 4:
            final = (item,graph,root)
    if not split or not prepared or not final:
        raise ValueError("Need split, prepared and cleanup snapshots before consumption")
    _,before,b = split; _,after,p = prepared; _,clean,f = final
    if not split[0]["frame"] < prepared[0]["frame"] < final[0]["frame"]:
        raise ValueError("Unexpected initialization stage order")
    for root in (b,p,f):
        if text(root["keyLabel"]) != "5" or numeric(root["difficulty"]) != 1 or text(root["diffin"]) != "" or text(root["motion"]) != "OFF":
            raise ValueError("Only the normal five-key pilot with Motion OFF is reviewed")
        if any(numeric(root[k]) != v for k,v in {"correction":0,"set_cnt":100,"startFrame":0,"frame_num":0}.items()):
            raise ValueError("Unreviewed timing settings")
    starts = (capture.get("audio") or {}).get("streamStarts",[])
    if len(starts) != 1 or starts[0]["compression"] != "Mp3" or starts[0]["latencySeek"] != 0:
        raise ValueError("Need one observed zero-latency MP3 stream start")
    audio_item = next((c for c in capture["captures"] if c["frame"] == starts[0]["iteration"]),None)
    if audio_item is None:
        raise ValueError("Capture the stream-start iteration")
    audio_graph = Graph(audio_item["state"])
    origin = numeric(audio_graph.root()["frame_num"])
    music = [d for d in audio_item["state"]["displayTree"] if d["name"] and text(d["name"]) == "music_act"]
    if len(music) != 1 or music[0]["movieFrame"] != 2 or origin != 145:
        raise ValueError("Unreviewed music origin")
    codes = after.array(p["keyCtrl"])
    key_codes = [numeric(codes[i]) for i,_ in LANES]
    if any(int(code) != code or not 0 <= code <= 65535 for code in key_codes):
        raise ValueError("Invalid native key codes")
    key_codes = list(map(int,key_codes))
    lanes, tap_schedule, hold_schedule = [], Counter(), Counter()
    bt,pt = before.array(b["timeline"]),after.array(p["timeline"])
    bh,ph = before.array(b["ftimeline"]),after.array(p["ftimeline"])
    lengths = after.array(p["frzLength"])
    for native_index,name in LANES:
        taps = [numeric(v) for v in before.array(bt[native_index])]
        spawns = [numeric(v) for v in after.array(pt[native_index])]
        if spawns != [v-100 for v in taps]:
            raise ValueError("Unexpected note transformation")
        for frame in spawns: tap_schedule[(frame,native_index)] += 1
        holds = []
        if bh[native_index] == {"type":"undefined"}:
            if native_index != 3 or ph[native_index] != bh[native_index]:
                raise ValueError("Unexpected missing hold lane")
        else:
            raw = [numeric(v) for v in before.array(bh[native_index])]
            prepared_holds = [numeric(v) for v in after.array(ph[native_index])]
            duration = after.props(lengths[native_index])
            if len(raw)%2 or len(prepared_holds) != len(raw):
                raise ValueError("Unpaired native holds")
            for i in range(0,len(raw),2):
                start,end = raw[i:i+2]
                if end < start or prepared_holds[i:i+2] != [start-100,end] or numeric(duration[str(i)]) != end-start:
                    raise ValueError("Unexpected hold transformation")
                holds.append([start,end]);hold_schedule[(start-100,native_index)] += 1
        lanes.append({"name":name,"nativeLaneIndex":native_index,"judgementFrames":taps,
            "spawnFrames":spawns,"holdFrames":holds,"tapsSeconds":[(v-origin)/60 for v in taps],
            "holdsSeconds":[[(a-origin)/60,(z-origin)/60] for a,z in holds]})
    for graph,root in [(after,p),(clean,f)]:
        if graph.schedule(root["mkArrow"]) != tap_schedule or graph.schedule(root["mkFArrow"]) != hold_schedule:
            raise ValueError("Compiled schedule disagrees with note arrays")
    speed_values = clean.array(f["speed_dac"])
    if len(speed_values)%2:
        raise ValueError("Unpaired speed events")
    speed = [{"nativeFrame":numeric(speed_values[i]),"seconds":(numeric(speed_values[i])-origin)/60,
        "multiplier":numeric(speed_values[i+1])} for i in range(0,len(speed_values),2)]
    colors, native_colors = [], []
    color_values = clean.array(f["iro_dac"])
    if len(color_values)%3:
        raise ValueError("Unpaired colour events")
    for i in range(0,len(color_values),3):
        frame,code,target = numeric(color_values[i]),text(color_values[i+1]),text(color_values[i+2])
        if target not in ("al","oni") or len(code) != 8 or not code.startswith("0x"):
            raise ValueError("Unreviewed colour target/value")
        int(code[2:],16)
        seconds = (frame-100-origin)/60
        native_colors.append({"frame":frame,"target":target,"color":code,"seconds":seconds})
        for lane in range(5) if target == "al" else [4]:
            colors.append({"seconds":seconds,"target":lane,"color":"#"+code[2:],"all":True})
    if clean.array(f["stop_dac"]) != [{"type":"string","text":"","utf16":[],"truncated":False}]:
        raise ValueError("Stop events are outside this pilot")
    return {"schema":"ddi-danoni-avm1-reference/experimental-1","adapter":"reku-2.16-higan/1",
        "sourceSwfSha256":SWF_HASH,"ruffle":capture["build"],"fps":60,
        "stages":{"split":split[0]["frame"],"prepared":prepared[0]["frame"],"cleanup":final[0]["frame"]},
        "audioOrigin":{"logicalFrame":origin,"iteration":audio_item["frame"],"streamMovieFrame":2,
            "outputSampleFrame":starts[0]["outputSampleFrame"],"scope":"Observed Ruffle post-frame counter and continuous zero-latency stream; original Flash latency unmeasured"},
        "metadata":{"keyMode":"5","label":text(f["diffi"]),"authorSpeed":numeric(f["dif_spi"]),
            "initialPixelSpeed":numeric(p["set_spi"]),"musicTitle":text(b["musicTitle"]),"tuning":text(b["tuning"])},
        "lanes":lanes,"nativeKeyCodes":key_codes,"speed":speed,"colors":colors,"nativeColors":native_colors,
        "boost":[],"stops":[],"scheduleAgreement":True,
        "limits":["Pinned work and default options only","Original Flash playback latency unmeasured",
            "Cosmetics and judgement/gauges are not reproduced","Diagonal placeholder lanes excluded after compiled-schedule check"]}


def export(snapshot_path,input_dir,output):
    snapshot_bytes = snapshot_path.read_bytes(); capture = json.loads(snapshot_bytes)
    run_path = snapshot_path.parent/"run.json"
    run_bytes = run_path.read_bytes(); run = json.loads(run_bytes)
    if run["exitCode"] != 0 or run["termination"] is not None or not run["inputsUnchanged"]:
        raise ValueError("Parent run report does not establish successful execution")
    reference = project(capture); config = capture["source"]["config"]
    input_dir = input_dir.resolve(strict=True)
    movie_path = (input_dir/config["movie"]).resolve(strict=True)
    if not movie_path.is_relative_to(input_dir) or any(p.is_symlink() for p in input_dir.rglob("*")):
        raise ValueError("Input directory must contain only its own regular resources")
    matching_configs = [path for path in input_dir.glob("*.json")
        if hashlib.sha256(path.read_bytes()).hexdigest() == run["configSha256"]]
    if not matching_configs or json.loads(matching_configs[0].read_bytes()) != config:
        raise ValueError("Recorded source configuration is missing or changed")
    for blob in (input_dir/"blobs").iterdir():
        if not blob.is_file() or hashlib.sha256(blob.read_bytes()).hexdigest() != blob.name:
            raise ValueError("Original input blob does not match its hash")
    original,mp3,audio = demux(movie_path,"root/sprite1602")
    if audio["movie"]["sha256"] != SWF_HASH or not audio["continuousZeroLatencyEligible"]:
        raise ValueError("Unverified source/audio continuity")
    provenance = json.loads((input_dir/"provenance.json").read_text())
    output = external_directory(output); output.mkdir(parents=True,exist_ok=False)
    chart = output/"charts/normal";chart.mkdir(parents=True)
    (output/"media").mkdir();(output/"native").mkdir();(output/"source").mkdir()
    (output/"media/music.mp3").write_bytes(mp3)
    (output/"native/snapshot.json").write_bytes(snapshot_bytes)
    (output/"native/run.json").write_bytes(run_bytes)
    for name,value in [("reference.json",reference),("audio.json",audio)]:
        (output/"native"/name).write_text(json.dumps(value,ensure_ascii=False,indent=2)+"\n")
    shutil.copytree(input_dir,output/"source/inputs")
    fields = {"musicTitle":f"{reference['metadata']['musicTitle']},{provenance['artist']},{provenance['page']['url']}",
        "tuning":f"{provenance['chartAuthor']},{provenance['page']['url']}",
        "difData":f"5,{reference['metadata']['label']},{reference['metadata']['authorSpeed']}",
        "musicUrl":"../../media/music.mp3","musicFolder":"./","blankFrame":"0","adjustment":"0",
        "playbackRate":"1","startFrame":"0"}
    frames = lambda values: ",".join(format(v,".15g") for v in values)
    origin = reference["audioOrigin"]["logicalFrame"]
    for lane in reference["lanes"]:
        fields[lane["name"]+"_data"] = frames([v-origin for v in lane["judgementFrames"]])
        fields["frz"+lane["name"].capitalize()+"_data"] = frames([v-origin for pair in lane["holdFrames"] for v in pair])
    fields["speed_data"] = frames([v for e in reference["speed"] for v in [e["nativeFrame"]-origin,e["multiplier"]]])
    fields["acolor_data"] = ",".join(str(v) for e in reference["colors"] for v in [round(e["seconds"]*60),e["target"],e["color"]])
    if any(any(c in value for c in "|\r\n") for value in fields.values()):
        raise ValueError("Header cannot be represented safely in dos")
    dos = "\n".join(f"|{key}={value}|" for key,value in fields.items())+"\n"
    (chart/"chart.dos.txt").write_text(dos)
    (chart/"chart.html").write_text('<!doctype html><meta charset="utf-8"><textarea hidden id="dos">'+html.escape(dos)+'</textarea>\n')
    (chart/"keys.json").write_text(json.dumps({"mode":"5","lanes":[{"name":n,"nativeLaneIndex":i} for i,n in LANES],
        "nativeKeyCodes":reference["nativeKeyCodes"],"source":"Observed keyCtrl; unused diagonal indices 1/5 are retained in raw snapshots"},indent=2)+"\n")
    manifest = {"schema":"ddi-legacy-export/experimental-1","reconstructed":True,"adapter":reference["adapter"],
        "adapterSourceSha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "demuxSourceSha256":hashlib.sha256((Path(__file__).parent/"swf_audio.py").read_bytes()).hexdigest(),
        "sourceSnapshotSha256":hashlib.sha256(snapshot_bytes).hexdigest(),"sourceSwfSha256":SWF_HASH,
        "sourcePageUrl":provenance["page"]["url"],"capturedAt":run["startedAt"],
        "evidence":{"inputs":"source/inputs","snapshot":"native/snapshot.json","run":"native/run.json",
            "reference":"native/reference.json","audio":"native/audio.json"},
        "music":{"path":"media/music.mp3","mimeType":"audio/mpeg","sha256":audio["mp3Sha256"]},
        "charts":[{"id":"normal","mode":"5","dos":"charts/normal/chart.dos.txt","page":"charts/normal/chart.html",
            "keys":"charts/normal/keys.json",
            "lanes":reference["lanes"],"speed":reference["speed"],"colors":reference["colors"]}],
        "assessment":{"acquisition":"preserved","nativeReference":"schedule-cross-checked",
            "conversion":"unverified-until-CW-roundtrip","audio":"continuous-zero-latency-MP3-demux",
            "originalFlashLatency":"unmeasured","visualFidelity":"partial"},
        "validation":{"cw":"validation/cw/conversion.json"},"limitations":reference["limits"]}
    (output/"manifest.json").write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+"\n")
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("snapshot",type=Path);parser.add_argument("inputs",type=Path);parser.add_argument("output",type=Path)
    args = parser.parse_args()
    result = export(args.snapshot,args.inputs,args.output)
    print(json.dumps({"charts":len(result["charts"]),"taps":sum(len(l["tapsSeconds"]) for l in result["charts"][0]["lanes"]),
        "holds":sum(len(l["holdsSeconds"]) for l in result["charts"][0]["lanes"]),"assessment":result["assessment"]},indent=2))
