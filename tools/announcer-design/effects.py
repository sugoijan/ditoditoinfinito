#!/usr/bin/env python3
"""Audition the bundled voices through an isolated SoundCraft control session.

Python 3.11+, ffmpeg and a running SoundCraft --no-audio --control PORT.
Only scratch outputs are written; this never bundles or regenerates speech.
"""
import argparse
import hashlib
import html
import json
import math
from pathlib import Path
import re
import socket
import subprocess
import tomllib
import wave

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
RATE = 48000
LINES = ["gameplay ready", "gameplay 100 combo", "gameplay comment hot",
         "battle match begin", "battle final stage", "evaluation aa"]
PACKS = [("en", "Mr. Clank"), ("ja", "ガキンさん")]


def run(*args):
    return subprocess.run(args, check=True, capture_output=True).stdout


def ffmpeg(*args):
    return run("ffmpeg", "-nostdin", "-hide_banner", "-v", "error", "-y", *map(str, args))


class SoundCraft:
    def __init__(self, port):
        self.socket = socket.create_connection(("127.0.0.1", port), timeout=60)
        self.stream = self.socket.makefile("rw", encoding="utf-8")
        self.calls = []

    def call(self, method, params):
        request = {"id": len(self.calls), "method": method, "params": params}
        self.stream.write(json.dumps(request) + "\n")
        self.stream.flush()
        reply = json.loads(self.stream.readline())
        self.calls.append({"request": request, "reply": reply})
        if not reply.get("ok"):
            raise RuntimeError(reply)
        return reply["result"]

    def execute(self, command, **params):
        return self.call("engine.execute", {"command": command, "params": params})

    def close(self):
        self.stream.close()
        self.socket.close()


def measure(path):
    result = subprocess.run(
        ["ffmpeg", "-nostdin", "-hide_banner", "-i", str(path), "-af",
         "ebur128=peak=true", "-f", "null", "-"], capture_output=True, check=True)
    summary = result.stderr.decode().rsplit("Summary:", 1)[-1]
    lufs = float(re.search(r"I:\s+(-?[\d.]+) LUFS", summary)[1])
    peak = float(re.search(r"Peak:\s+(-?[\d.]+) dBFS", summary)[1])
    if not all(map(math.isfinite, (lufs, peak))):
        raise ValueError(f"No measurable audio in {path}")
    return {"lufs": lufs, "true_peak_dbfs": peak}


def reel(folder, dest):
    """The same decoded source and gaps for every treatment; no new TTS takes."""
    provenance = tomllib.loads((folder / "PROVENANCE.toml").read_text())
    pcm = bytearray(b"\0" * RATE)  # half a second, 16-bit mono
    cues = []
    for line in LINES:
        rel, text = next((r, t) for r, t in provenance["said"].items()
                         if r.startswith(line + "/"))
        source = folder / rel
        data = ffmpeg("-i", source, "-ar", RATE, "-ac", 1, "-f", "s16le", "-")
        cues.append({"text": text, "source": str(source.relative_to(ROOT) if source.is_relative_to(ROOT) else source),
                     "sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                     "start": len(pcm) / (2 * RATE), "seconds": len(data) / (2 * RATE)})
        pcm.extend(data)
        pcm.extend(b"\0" * (2 * RATE * 2))  # two seconds for the longest tail
    with wave.open(str(dest), "wb") as wav:
        wav.setparams((1, 2, RATE, 0, "NONE", "not compressed"))
        wav.writeframes(pcm)
    return cues, len(pcm) / (2 * RATE)


def page(dest, presets, records):
    sections = []
    for lang, name in PACKS:
        cards = []
        for preset in presets:
            key = f"{lang}-{preset['id']}"
            cards.append(f'''<article><h3>{html.escape(preset['name'])}</h3>
<p>{html.escape(preset['description'])}</p>
<label>Voice alone<audio controls preload="none" src="{key}.wav"></audio></label>
<label>Over music<audio controls preload="none" src="{key}-music.wav"></audio></label>
<small>{records[key]['final']['lufs']:.1f} LUFS ·
{records[key]['final']['true_peak_dbfs']:.1f} dBTP (voice alone)</small></article>''')
        cues = " · ".join(html.escape(c['text']) for c in records[f"{lang}-dry"]["cues"])
        sections.append(f"<section><h2>{name}</h2><p>{cues}</p><div class=grid>"
                        + "".join(cards) + "</div></section>")
    (dest / "index.html").write_text('''<!doctype html><html lang="en"><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>DDI announcer effects audition</title><style>
body{font:16px/1.5 system-ui;background:#151820;color:#eee;margin:0;padding:32px;max-width:1500px}
h1,h2,h3{line-height:1.2}p,small{color:#bdc5d8}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(290px,1fr));gap:16px}
article{background:#222733;padding:20px;border-radius:12px}article p{min-height:100px}
audio{display:block;width:100%;margin:8px 0 20px}section{margin:36px 0}a{color:#a4c6ff}
</style><h1>Announcer effects audition</h1>
<p>Same takes, same timing, matched loudness. Start with the voice alone, then compare over music.
The bundled packs are unchanged. No music ducking; the music balance is a listening example.</p>
''' + "".join(sections) + '''<p>Music: “Into My Dream” by Lagoona (Andreas Viklund),
<a href="https://github.com/TeamRizu/OutFox-Serenity">OutFox Serenity</a>,
<a href="https://creativecommons.org/licenses/by/4.0/">CC BY 4.0</a>.
Excerpt from 40 seconds, faded, attenuated and mixed with the voices. Voices: our CC0 Kokoro packs;
Japanese training-source attribution is retained in the accompanying PROVENANCE-ja.toml.</p>
<script>document.addEventListener('play', e => {if(e.target.tagName==='AUDIO')
document.querySelectorAll('audio').forEach(a=>{if(a!==e.target)a.pause()})}, true)</script></html>''')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, required=True,
                        help="a dedicated, disposable SoundCraft session; its document is replaced")
    parser.add_argument("--engine-version", required=True, help="SoundCraft version used for this study")
    parser.add_argument("--dest", type=Path, default=ROOT / "target/scratch/announcer-effects")
    parser.add_argument("--recipes", type=Path, default=HERE / "effects.json")
    parser.add_argument("--source", type=Path, default=ROOT / "assets/announcers",
                        help="folder containing dry packs with provenance, before creative effects")
    args = parser.parse_args()
    source_root = args.source.resolve()
    for _, name in PACKS:
        provenance = tomllib.loads((source_root / name / "PROVENANCE.toml").read_text())
        if "announcer-stadium-" in provenance.get("kokoro", {}).get("sound", ""):
            parser.error(f"{name} already has stadium effects; use --source DIR with dry packs")
    dest = args.dest.resolve()
    # Keep experiments away from shipping assets, source, and other projects.
    if not dest.is_relative_to(ROOT / "target/scratch"):
        parser.error("--dest must be inside this checkout's target/scratch/")
    dest.mkdir(parents=True, exist_ok=True)
    work = dest / "raw"
    work.mkdir(exist_ok=True)
    recipes = json.loads(args.recipes.read_text())
    presets = recipes["presets"]
    records = {}
    client = SoundCraft(args.port)
    try:
        for lang, name in PACKS:
            folder = source_root / name
            (dest / f"PROVENANCE-{lang}.toml").write_bytes((folder / "PROVENANCE.toml").read_bytes())
            source = work / f"{lang}-source.wav"
            cues, seconds = reel(folder, source)
            for preset in presets:
                key = f"{lang}-{preset['id']}"
                client.execute("session.new", name=key, sample_rate=RATE)
                client.execute("file.import_audio", path=str(source))
                imported = client.call("session.inspect", {"detail": "full"})
                track = imported["tracks"][0]["id"]
                for slot, insert in enumerate(preset["inserts"]):
                    client.execute("mix.insert", track=track, slot=slot, **insert)
                snapshot = client.call("session.inspect", {"detail": "full"})
                bounced = work / f"{key}-stereo.wav"
                report = client.execute("file.bounce_mix", path=str(bounced), bit_depth="32f",
                                        start=0, end={"seconds": seconds}, normalize=False, dither=False)
                # The shipping announcers are mono. Judge the effects in mono too.
                raw = work / f"{key}.wav"
                ffmpeg("-i", bounced, "-ac", 1, "-c:a", "pcm_f32le", raw)
                records[key] = {"cues": cues, "seconds": seconds, "raw": measure(raw),
                                "session": snapshot, "bounce": report}
                print(f"Rendered {key}", flush=True)
        # A single static gain per reel, no extra compression or limiting. Lower
        # every candidate equally if any needs more headroom at the target.
        target = min([-16.0] + [r["raw"]["lufs"] - 2 - r["raw"]["true_peak_dbfs"]
                               for r in records.values()])
        music = ROOT / "assets/songs/into-my-dream/Into My Dream.opus"
        for key, record in records.items():
            gain = target - record["raw"]["lufs"]
            out = dest / f"{key}.wav"
            ffmpeg("-i", work / f"{key}.wav", "-af", f"volume={gain}dB", "-c:a", "pcm_s16le", out)
            record.update(gain_db=gain, final=measure(out))
            if abs(record["final"]["lufs"] - target) > 0.2 or record["final"]["true_peak_dbfs"] > -1.8:
                raise RuntimeError(f"Audition level check failed: {key}: {record['final']}")
            with wave.open(str(out), "rb") as wav:
                if abs(wav.getnframes() / wav.getframerate() - record["seconds"]) > 1 / RATE:
                    raise RuntimeError(f"Unexpected timing change in {key}")
            # Same music and balance for all candidates. Fixed headroom for the
            # sum; never normalize or duck the mix depending on the treatment.
            seconds = record["seconds"]
            filters = ("[0:a]pan=stereo|c0=c0|c1=c0[v];"
                       f"[1:a]volume=-10dB,afade=t=in:d=0.3,"
                       f"afade=t=out:st={seconds-1}:d=1[m];"
                       "[v][m]amix=inputs=2:duration=first:normalize=0,volume=-6dB[a]")
            mixed = dest / f"{key}-music.wav"
            ffmpeg("-i", out, "-ss", 40, "-i", music, "-filter_complex", filters,
                   "-map", "[a]", "-c:a", "pcm_s16le", mixed)
            record["mix"] = measure(mixed)
            if record["mix"]["true_peak_dbfs"] > -1:
                raise RuntimeError(f"Music mix has insufficient headroom: {key}")
        page(dest, presets, records)
        metadata = {"recipes": recipes, "soundcraft": args.engine_version,
                    "ffmpeg": run("ffmpeg", "-version").decode().splitlines()[0],
                    "target_lufs": target, "records": records,
                    "music": {"source": str(music.relative_to(ROOT)), "start_seconds": 40,
                              "sha256": hashlib.sha256(music.read_bytes()).hexdigest(),
                              "credit": "Lagoona (Andreas Viklund), Into My Dream, CC BY 4.0"}}
        (dest / "study.json").write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n")
        (dest / "CREDITS.txt").write_text(
            "Voice sources: see PROVENANCE-en.toml and PROVENANCE-ja.toml.\n"
            "Effects: SoundCraft built-in plugins; recipes and gains in study.json.\n"
            "Music: Into My Dream by Lagoona (Andreas Viklund), CC BY 4.0.\n"
            "https://github.com/TeamRizu/OutFox-Serenity\n"
            "https://creativecommons.org/licenses/by/4.0/\n"
            "Music excerpt from 40 seconds, faded, attenuated and mixed with speech.\n")
        print(f"Matched to {target:.1f} LUFS; open {dest / 'index.html'}")
    finally:
        (dest / "commands.json").write_text(json.dumps(client.calls, ensure_ascii=False, indent=2) + "\n")
        client.close()


if __name__ == "__main__":
    main()
