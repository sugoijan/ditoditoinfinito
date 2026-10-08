#!/usr/bin/env python3
"""Extract the pinned 2004 Kouhaku collection locally through restricted Ruffle.

All chart values come from initialized native storage. No acquired ActionScript
is executed outside run.py. The output follows the observed Ruffle audio origin;
Flash's nonzero MP3 latency/seek behavior remains unverified.
"""
import argparse
import hashlib
import html
import json
from pathlib import Path
import re
import shutil
import subprocess
import wave
from datetime import datetime, timezone

from higan import Graph, text, numeric
from paths import external_directory
from run import run
from swf_audio import demux, read_movie

SWF_HASH = "e14e472911b6603ddcc58bd40049d0b05cfa986a2240209dd848a516cf40fed2"
PAGE = "http://eucaly.net/~iso50/kouhaku.html"
SOURCE = "http://eucaly.net/~iso50/2004kouhaku.swf"
LANES = ["left", "down", "up", "right", "space"]
# Names, routes and controls reviewed against this exact movie and original page.
SELECTIONS = [
    ("tougenkyo", "桃源郷", "公", "★★★", 0, 229, 6),
    ("tougenkyo-hard", "桃源郷 hard", "公", "★★★★★★★★", 0, 229, 6),
    ("run", "RUN", "tio", "★★★★★★", 1, 232, 7),
    ("kitakaze", "北風のメロディー", "八乙女葦菜", "★★★★★", 2, 234, 8),
    ("korokke", "ころっけわっしょい", "ゆうかり", "★★★★", 3, 236, 9),
    ("emy", "emy", "onoken", "★★★★★", 4, 238, 10),
]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2)+"\n")


def config_for(selection):
    ident, _, _, _, steps, _, _ = selection
    events = [{"frame":70,"type":"click","x":160,"y":157}]
    start = 100 if steps == 0 else 250
    if ident == "tougenkyo-hard":
        # Use the original FMV control in Options; do not change VM variables.
        events += [{"frame":100,"type":"key","code":"Space"},
                   {"frame":120,"type":"click","x":165,"y":169},
                   {"frame":140,"type":"key","code":"Space"}]
        start = 180
    else:
        events += [{"frame":80+40*i,"type":"key","code":"ArrowRight"} for i in range(steps)]
    events += [{"frame":start,"type":"enter"}]
    return {"movie":"blobs/"+SWF_HASH,"sourceUrl":SOURCE,"pageUrl":PAGE,
            "captureFrames":[start-10,start,start+1,start+2,start+60,start+150],
            "events":events,"render":False,"mixAudio":True,"inspectGlobals":True,"resources":[]}


def globals_data(state):
    scope = [s for s in state["globalScopes"] if s["caseSensitive"] is False]
    if len(scope) != 1:
        raise ValueError("Need the SWF6 global data environment")
    result = {}
    for prop in scope[0]["properties"]:
        name = text(prop["name"])
        if name in result:
            raise ValueError("Duplicate global property")
        result[name] = prop["value"]
    return result


def project(capture, selection):
    ident, title, artist, difficulty, _, sprite, root_frame = selection
    if capture["source"]["movieSha256"] != SWF_HASH or capture["swfHeaderFps"] != 60 or capture["avm1Halted"]:
        raise ValueError("Unreviewed or halted movie")
    audit = capture["audit"]
    if audit["truncated"] or audit["runtimeErrors"] or audit["droppedEntries"] or audit["truncatedDetails"]:
        raise ValueError("Incomplete execution audit")
    if any(e["kind"] not in ["runtime-warning"] for e in audit["entries"]):
        raise ValueError("Unexpected execution or resource request")
    for item in capture["captures"]:
        Graph(item["state"])
    starts = capture["audio"]["streamStarts"]
    if len(starts) != 1 or starts[0]["compression"] != "Mp3" or starts[0]["sampleRate"] != 22050 or starts[0]["stereo"] or starts[0]["latencySeek"] != 1633:
        raise ValueError("Unreviewed music stream start")
    origin = next(c for c in capture["captures"] if c["frame"] == starts[0]["iteration"])
    menu = next(c for c in capture["captures"] if c["frame"] < origin["frame"])
    graph, before = Graph(origin["state"]), Graph(menu["state"])
    root, menu_root = graph.root(), before.root()
    gl = globals_data(origin["state"])
    if origin["state"]["roots"][0]["movieFrame"] != root_frame or menu["state"]["roots"][0]["movieFrame"] != 4:
        raise ValueError("Unexpected menu/play stage")
    if (text(gl["kyoku"]), text(root["artist"]), text(root["difficulty"])) != (title,artist,difficulty):
        raise ValueError("Selection metadata disagrees with reviewed collection")
    if gl["Reverse"] is not False or numeric(gl["SpeedUp"]) != 2 or numeric(gl["SpeedY"]) != 188:
        raise ValueError("Unreviewed movement settings")
    if numeric(gl["point"]) != (5 if ident == "tougenkyo-hard" else 0):
        raise ValueError("Unexpected hidden-chart selection state")
    controllers = []
    for display in origin["state"]["displayTree"]:
        if display["object"] and "left_timeline" in graph.props(display["object"]):
            controllers.append(display)
    if len(controllers) != 1:
        raise ValueError("Need exactly one initialized chart controller")
    display = controllers[0]; controller = graph.props(display["object"])
    parent = origin["state"]["displayTree"][display["parent"]]
    if parent["movieFrame"] != 2 or numeric(controller["Cnt"]) != 1:
        raise ValueError("Capture must precede note consumption at the observed audio origin")
    for elapsed in (1,2):
        later = next((c for c in capture["captures"] if c["frame"] == origin["frame"]+elapsed),None)
        if later is None:
            raise ValueError("Missing immediate timeline progression evidence")
        Graph(later["state"])
        following = later["state"]["displayTree"][display["parent"]]
        if following["movieFrame"] != 2+elapsed:
            raise ValueError("Observed chart timeline does not advance at 60 Hz")
    lanes = []
    for lane in LANES:
        raw = text(root[lane+"_data"])
        if text(menu_root[lane+"_data"]) != raw or not re.fullmatch(r"[0-9]+(?:,[0-9]+)*", raw):
            raise ValueError("Unexpected source frame syntax or selection mutation")
        source = list(map(int,raw.split(",")))
        native = [numeric(v) for v in graph.array(controller[lane+"_timeline"])]
        if native != source or source != sorted(source) or len(set(source)) != len(source) or not all(150 < n < 100000 for n in source):
            raise ValueError("Native compiled timeline disagrees with source frames")
        lanes.append({"name":lane,"judgementFrames":source,"spawnMovieFrames":[n-150 for n in source],
            "holdFrames":[],"tapsSeconds":[(n-2)/60 for n in source],"holdsSeconds":[]})
    return {"schema":"ddi-danoni-avm1-reference/experimental-1","adapter":"iso50-kouhaku-2004/1",
        "id":ident,"sourceSwfSha256":SWF_HASH,"ruffle":capture["build"],"fps":60,
        "metadata":{"title":title,"artist":artist,"difficulty":difficulty,"keyMode":"5","chartAuthor":None},
        "audioOrigin":{"logicalFrame":2,"iteration":origin["frame"],"outputSampleFrame":starts[0]["outputSampleFrame"],
            "stream":starts[0],"sprite":sprite,"scope":"Nominal native frame grid anchored to the Ruffle stream start; not pixel-perfect receptor-centre timing or original Flash seek compensation"},
        "stages":{"menu":menu["frame"],"initialized":origin["frame"]},"lanes":lanes,"speed":[],"boost":[],"colors":[],
        "sourceCompiledAgreement":True,"limits":["Pinned six-chart collection and default movement only",
            "Original Flash MP3 latency and timeline seek behavior unverified",
            "Ruffle stream decoder ignores declared 1633-sample delay",
            "Nominal chart frames preserved; hit-box geometry and judge/gauge behavior not reproduced",
            "Chart author not identified; credits retained from original page and runtime"]}


def sm_text(reference, music_name):
    # At synthetic 75 BPM / 48 rows per beat, each row is exactly 1/60 second.
    rows = {}
    for index, lane in enumerate(reference["lanes"]):
        for frame in lane["judgementFrames"]:
            tick = frame-2
            rows.setdefault(tick,["0"]*5)[index] = "1"
    last = max(rows)
    measures = ["\n".join("".join(rows.get(m*192+i,["0"]*5)) for i in range(192)) for m in range(last//192+1)]
    metadata = reference["metadata"]
    return (f"#TITLE:{metadata['title']};\n#ARTIST:{metadata['artist']};\n#MUSIC:../../media/{music_name};\n"
            "// Synthetic BPM encodes the native 60 Hz grid; not a recovered musical BPM.\n"
            "#OFFSET:0;\n#BPMS:0=75;\n#NOTES:\n     danoni-5:\n     Reconstructed native frame grid:\n"
            "     Edit:\n     1:\n     0,0,0,0,0:\n"+",\n".join(measures)+"\n;\n")


def extract(movie, output, worker, ffmpeg):
    movie = movie.resolve(strict=True)
    original, _, header = read_movie(movie)
    if header["sha256"] != SWF_HASH:
        raise ValueError("This adapter supports only the reviewed 2004kouhaku.swf hash")
    output = external_directory(output); output.mkdir(parents=True,exist_ok=False)
    inputs = output/"source/inputs"; (inputs/"blobs").mkdir(parents=True)
    (inputs/"blobs"/SWF_HASH).write_bytes(original)
    write_json(inputs/"provenance.json",{"localSource":str(movie),"sourceSwfSha256":SWF_HASH,
        "pageUrl":PAGE,"sourceUrl":SOURCE,"sourceUrlEvidence":"Maintainer supplied page; page object/embed and DL link identify this filename; original SWF not re-downloaded",
        "originalAcquisitionDate":None,"inspectedAt":datetime.now(timezone.utc).isoformat(),
        "originalLocalModifiedAt":datetime.fromtimestamp(movie.stat().st_mtime,timezone.utc).isoformat()})
    (output/"native").mkdir(); (output/"media").mkdir()
    charts, music_files, references = [], {}, {}
    for selection in SELECTIONS:
        ident, _, _, _, _, sprite, _ = selection
        config = config_for(selection); cfg = inputs/f"{ident}.json"; write_json(cfg,config)
        capture_dir = output/"native"/ident
        result = run(worker,cfg,capture_dir)
        if result["exitCode"] or result["termination"] or not result["inputsUnchanged"] or result["configSha256"] != digest(cfg.read_bytes()):
            raise ValueError(f"Restricted capture failed: {ident}")
        snapshot = (capture_dir/"snapshot.json").read_bytes(); capture = json.loads(snapshot)
        if capture["source"]["config"] != config:
            raise ValueError("Recorded configuration differs")
        ref = project(capture,selection); references[ident] = ref; write_json(capture_dir/"reference.json",ref)
        if sprite not in music_files:
            _, mp3, audio = demux(inputs/"blobs"/SWF_HASH,f"root/sprite{sprite}")
            if ref["audioOrigin"]["stream"]["sliceStart"]+header["tagStart"] != audio["headTagOffset"]:
                raise ValueError("Observed stream disagrees with selected sprite")
            mp3_path = output/"media"/f"{sprite}.mp3"; mp3_path.write_bytes(mp3)
            wav_path = mp3_path.with_suffix('.wav')
            command = [str(ffmpeg),"-nostdin","-v","error","-i",str(mp3_path),"-map_metadata","-1","-c:a","pcm_s16le",str(wav_path)]
            subprocess.run(command,check=True,timeout=30,stdin=subprocess.DEVNULL)
            with wave.open(str(wav_path),'rb') as wav:
                if (wav.getnchannels(),wav.getframerate(),wav.getsampwidth(),wav.getnframes()) != (1,22050,2,audio["declaredSampleFrames"]):
                    raise ValueError("Independent decode changed expected sample count or format")
            audio["decodedWav"] = {"path":f"media/{sprite}.wav","sha256":digest(wav_path.read_bytes()),
                "command":command,"trimmedSampleFrames":0,"resampled":False,
                "decoderVersion":subprocess.check_output([str(ffmpeg),"-version"],text=True).splitlines()[0]}
            write_json(output/"native"/f"audio-{sprite}.json",audio)
            music_files[sprite] = {"path":f"media/{sprite}.wav","mimeType":"audio/wav","sha256":audio["decodedWav"]["sha256"],
                "originalMp3":f"media/{sprite}.mp3","latencySeekSamples":1633,"sampleRate":22050,"durationSeconds":audio["declaredSampleFrames"]/22050}
        chart_dir = output/"charts"/ident; chart_dir.mkdir(parents=True)
        metadata = ref["metadata"]
        fields = {"musicTitle":f"{metadata['title']},{metadata['artist']},{PAGE}","tuning":f"Chart author not identified,{PAGE}",
            "difData":f"5,{metadata['difficulty']},1","musicUrl":f"../../media/{sprite}.wav","musicFolder":"./",
            "blankFrame":"0","adjustment":"0","startFrame":"0","playbackRate":"1"}
        for lane in ref["lanes"]:fields[lane["name"]+"_data"] = ','.join(str(n-2) for n in lane["judgementFrames"])
        if any(any(c in v for c in '|\r\n') for v in fields.values()):raise ValueError("Unsafe dos header")
        dos='\n'.join(f"|{k}={v}|" for k,v in fields.items())+'\n'
        (chart_dir/"chart.dos.txt").write_text(dos)
        (chart_dir/"chart.html").write_text('<!doctype html><meta charset="utf-8"><textarea hidden id="dos">'+html.escape(dos)+'</textarea>\n')
        (chart_dir/"chart.sm").write_text(sm_text(ref,f"{sprite}.wav"))
        write_json(chart_dir/"keys.json",{"mode":"5","lanes":LANES,"nativeKeyCodes":[37,40,38,39,32],"source":"Reviewed pinned movie key handlers"})
        charts.append({"id":ident,"mode":"5","dos":f"charts/{ident}/chart.dos.txt","page":f"charts/{ident}/chart.html",
            "simfile":f"charts/{ident}/chart.sm","keys":f"charts/{ident}/keys.json","music":music_files[sprite],
            "sourceSnapshotSha256":digest(snapshot),"nativeReference":f"native/{ident}/reference.json",
            "lanes":ref["lanes"],"speed":[],"colors":[],"validation":f"validation/{ident}/conversion.json"})
        print(json.dumps({"chart":ident,"taps":sum(len(l['tapsSeconds']) for l in ref['lanes']),"nativeReference":"source-and-compiled-arrays-agree"}),flush=True)
    # A small standalone pack is convenient to import: five song folders,
    # with the normal/hidden charts sharing one music file in the first folder.
    for selection in SELECTIONS:
        ident, _, _, _, _, sprite, _ = selection
        if ident == "tougenkyo-hard":
            continue
        folder = output/"playable"/ident; folder.mkdir(parents=True)
        shutil.copyfile(output/"media"/f"{sprite}.wav",folder/"music.wav")
        ids = ["tougenkyo","tougenkyo-hard"] if ident == "tougenkyo" else [ident]
        combined = []
        for number, selected in enumerate(ids):
            content = sm_text(references[selected],f"{sprite}.wav")
            content = content.replace(f"../../media/{sprite}.wav","music.wav")
            label = references[selected]["metadata"]["difficulty"]
            content = content.replace("Reconstructed native frame grid:",label+" (native frame grid):")
            if number:
                content = "#NOTES:"+content.split("#NOTES:",1)[1]
            combined.append(content)
        (folder/"song.sm").write_text("\n".join(combined))
    manifest={"schema":"ddi-legacy-export/experimental-1","adapter":"iso50-kouhaku-2004/1","reconstructed":True,
        "adapterSourceSha256":digest(Path(__file__).read_bytes()),"graphReaderSourceSha256":digest((Path(__file__).parent/"higan.py").read_bytes()),"demuxSourceSha256":digest((Path(__file__).parent/'swf_audio.py').read_bytes()),
        "sourceSwfSha256":SWF_HASH,"sourcePageUrl":PAGE,"sourceUrl":SOURCE,"charts":charts,"playablePack":"playable",
        "evidence":{"inputs":"source/inputs"},"assessment":{"nativeReference":"source-and-compiled-arrays-cross-checked",
            "conversion":"unverified-until-per-chart-CW-roundtrip","audio":"untrimmed-PCM-decode-of-preserved-MP3",
            "originalFlashLatency":"unverified","nominalTimebase":"observed-Ruffle-60-Hz-timeline-origin-2"},"limitations":ref['limits']}
    write_json(output/'manifest.json',manifest)
    return manifest


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('movie',type=Path);parser.add_argument('output',type=Path)
    parser.add_argument('--worker',required=True,type=Path);parser.add_argument('--ffmpeg',default=shutil.which('ffmpeg'),type=Path)
    args=parser.parse_args()
    if args.ffmpeg is None:parser.error('An installed ffmpeg is required for explicit untrimmed PCM decode')
    result=extract(args.movie,args.output,args.worker,args.ffmpeg)
    print(json.dumps({'charts':len(result['charts']),'songs':len({c['music']['path'] for c in result['charts']}),'assessment':result['assessment']},indent=2))
