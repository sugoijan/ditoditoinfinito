#!/usr/bin/env python3
"""Audition announcer voices made with ElevenLabs (standard library only).

    announcer.py status                       credits and voice slots left
    announcer.py design [VOICE ...]           a round of three previews each
    announcer.py keep VOICE rR-N [--name N]   save a preview as a voice
    announcer.py speak [--voices ...] [--styles ...] [--lines ...]
    announcer.py page                         write the audition page again

`design` and `speak` take `--dry-run` (what they would make, and how many
characters it costs, with no request). What they make goes under
target/scratch/announcer-audition/ (`--out`), never into the repository:
the design previews, `Announcers/<voice>--<style>/<line folder>/` (a
StepMania announcer each, which the game reads), `state.json` (what was
asked for and what came back, for provenance) and `index.html`, the page
to compare them on. Made clips are kept: running `speak` again makes only
what is missing. The key is read from ELEVENLABS_API_KEY and never printed
or written; ELEVENLABS_API_BASE points the requests elsewhere (tests).
See README.md, and docs/plans/announcers.md for why the audition runs on
the free plan and a pack to bundle is made on a paid one.
"""
import argparse
import base64
import json
import os
import re
import sys
import time
import tomllib
from datetime import datetime, timezone
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlencode
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
API = os.environ.get("ELEVENLABS_API_BASE", "https://api.elevenlabs.io").rstrip("/")
FORMAT = "mp3_44100_128"
USER_AGENT = "ddi-announcer-design"
TAG = re.compile(r"\[[^\]]*\]")


class ApiError(Exception):
    def __init__(self, status, detail):
        super().__init__(f"HTTP {status}: {detail}")
        self.status = status


def api(method, path, body=None, query=None, binary=False):
    """One request, retried while the service is busy."""
    key = os.environ.get("ELEVENLABS_API_KEY")
    if not key:
        sys.exit("Set ELEVENLABS_API_KEY first (the key stays in the environment).")
    url = API + path + ("?" + urlencode(query) if query else "")
    data = None if body is None else json.dumps(body).encode()
    headers = {"xi-api-key": key, "User-Agent": USER_AGENT, "Accept": "*/*"}
    if data is not None:
        headers["Content-Type"] = "application/json"
    for attempt in range(6):
        try:
            with urlopen(Request(url, data=data, method=method, headers=headers), timeout=180) as r:
                payload = r.read()
                meta = {"request_id": r.headers.get("request-id")}
                return (payload if binary else json.loads(payload)), meta
        except HTTPError as e:
            detail = e.read().decode(errors="replace")[:600]
            # 409: a generation already in progress; 429: too many at once.
            if e.code in (409, 429) or e.code >= 500:
                time.sleep(min(30, 2 ** attempt))
                continue
            raise ApiError(e.code, detail) from None
        except URLError as e:
            if attempt >= 2:
                raise ApiError(0, str(e.reason)) from None
            time.sleep(2 ** attempt)
    raise ApiError(0, "still busy after six tries")


# --- configuration and state -------------------------------------------------


def load_config(path):
    with open(path, "rb") as f:
        cfg = tomllib.load(f)
    names = [v["name"] for v in cfg.get("voice", [])] + [s["name"] for s in cfg.get("style", [])]
    for name in names:
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", name) or "--" in name:
            sys.exit(f"{name!r}: names are lower case letters, digits and single hyphens")
    return cfg


class State:
    """What was asked for and what came back, in `state.json`."""

    def __init__(self, out):
        self.out = out
        self.path = out / "state.json"
        data = json.loads(self.path.read_text()) if self.path.exists() else {}
        self.designs = data.get("designs", {})
        self.kept = data.get("kept", {})
        self.clips = data.get("clips", {})

    def save(self):
        self.out.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(".tmp")
        data = {"designs": self.designs, "kept": self.kept, "clips": self.clips}
        tmp.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n")
        tmp.replace(self.path)


def shown(path):
    """A path as printed: from the repository's root where it is inside it."""
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return str(path)


def now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def spoken(text, tags):
    """The text as sent: audio tags kept, or taken out for a model that
    would read them."""
    return text if tags else re.sub(r"\s+", " ", TAG.sub("", text)).strip()


def voices_to_speak(cfg, state):
    """Kept voices, then the configuration's existing ones, by name."""
    voices = {name: k["voice_id"] for name, k in state.kept.items()}
    for v in cfg.get("voice", []):
        if "voice_id" in v:
            voices[v["name"]] = v["voice_id"]
    return voices


# --- commands ------------------------------------------------------------------


def cmd_status(cfg, state, args):
    sub, _ = api("GET", "/v1/user/subscription")
    left = sub.get("character_limit", 0) - sub.get("character_count", 0)
    print(f"tier: {sub.get('tier')} ({sub.get('status')})")
    print(f"characters: {sub.get('character_count')} of {sub.get('character_limit')} used, {left} left")
    reset = sub.get("next_character_count_reset_unix")
    if reset:
        print(f"resets: {datetime.fromtimestamp(reset, timezone.utc):%Y-%m-%d}")
    print(f"voice slots: {sub.get('voice_slots_used')} of {sub.get('voice_limit')} used")
    print(f"kept here: {', '.join(state.kept) or 'none'}")


def cmd_design(cfg, state, args):
    design = dict(cfg.get("design", {}))
    text = " ".join(design.pop("text", "").split())
    model = design.pop("model", "eleven_ttv_v3")
    if not 100 <= len(text) <= 1000:
        sys.exit(f"[design] text is {len(text)} characters; the service takes 100 to 1000")
    voices = [v for v in cfg.get("voice", []) if "description" in v]
    if args.voices:
        unknown = set(args.voices) - {v["name"] for v in voices}
        if unknown:
            sys.exit(f"no designed voice named {', '.join(sorted(unknown))}")
        voices = [v for v in voices if v["name"] in args.voices]
    print(f"{len(voices)} design round(s), {len(text)} characters each, "
          f"{len(voices) * len(text)} in all (three previews a round)")
    if args.dry_run:
        return
    for v in voices:
        rounds = state.designs.setdefault(v["name"], [])
        n = len(rounds) + 1
        body = {"voice_description": v["description"], "model_id": model, "text": text, **design}
        if "seed" in v:
            body["seed"] = v["seed"] + n - 1
        try:
            resp, meta = api("POST", "/v1/text-to-voice/design", body, {"output_format": FORMAT})
        except ApiError as e:
            print(f"{v['name']}: {e}")
            if e.status in (401, 402, 403):
                break
            continue
        folder = state.out / "design" / v["name"]
        folder.mkdir(parents=True, exist_ok=True)
        previews = []
        for i, p in enumerate(resp.get("previews", []), 1):
            file = folder / f"r{n}-{i}.mp3"
            file.write_bytes(base64.b64decode(p["audio_base_64"]))
            previews.append({
                "id": f"r{n}-{i}",
                "file": file.relative_to(state.out).as_posix(),
                "generated_voice_id": p["generated_voice_id"],
                "duration_secs": p.get("duration_secs"),
            })
        rounds.append({
            "round": n, "made": now(), "model": model, "description": v["description"],
            "text": resp.get("text", text), "params": body, "previews": previews,
            "request_id": meta["request_id"],
        })
        state.save()
        print(f"{v['name']}: round {n}, {len(previews)} previews in {shown(folder)}")
    write_page(cfg, state)


def cmd_keep(cfg, state, args):
    rounds = state.designs.get(args.voice, [])
    found = [(r, p) for r in rounds for p in r["previews"] if p["id"] == args.preview]
    if not found:
        have = [p["id"] for r in rounds for p in r["previews"]]
        sys.exit(f"{args.voice} has no preview {args.preview} (has: {', '.join(have) or 'none'})")
    rnd, preview = found[0]
    name = args.name or f"{args.voice}-{args.preview}"
    if name in state.kept:
        sys.exit(f"{name} is kept already ({state.kept[name]['voice_id']})")
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", name) or "--" in name:
        sys.exit(f"{name!r}: names are lower case letters, digits and single hyphens")
    body = {
        "voice_name": f"DDI {name}",
        "voice_description": rnd["description"],
        "generated_voice_id": preview["generated_voice_id"],
        "labels": {"use": "dito-dito-infinito announcer audition"},
    }
    try:
        resp, meta = api("POST", "/v1/text-to-voice", body)
    except ApiError as e:
        sys.exit(f"{e}\n(A preview's id lasts a while only; run `design {args.voice}` again "
                 "if it has gone. A voice slot is needed too: see `status`.)")
    state.kept[name] = {
        "voice_id": resp["voice_id"], "from": f"{args.voice} {args.preview}",
        "description": rnd["description"], "design_model": rnd["model"], "made": now(),
    }
    state.save()
    write_page(cfg, state)
    print(f"kept {name}: voice {resp['voice_id']} (one voice slot)")


def cmd_speak(cfg, state, args):
    voices = voices_to_speak(cfg, state)
    if args.voices:
        unknown = set(args.voices) - set(voices)
        if unknown:
            sys.exit(f"not kept: {', '.join(sorted(unknown))} (kept: {', '.join(voices) or 'none'})")
        voices = {k: v for k, v in voices.items() if k in args.voices}
    styles = cfg.get("style", [])
    if args.styles:
        styles = [s for s in styles if s["name"] in args.styles]
    lines = cfg.get("line", [])
    if args.lines:
        lines = [l for l in lines if any(w in l["folder"] for w in args.lines)]
    if not voices:
        sys.exit("no voice to speak with: keep a design (`keep`) or give one a voice_id")
    todo = []
    for voice, voice_id in voices.items():
        for s in styles:
            for line in lines:
                for t, text in enumerate(line["texts"], 1):
                    for take in range(1, s.get("takes", 1) + 1):
                        rel = f"Announcers/{voice}--{s['name']}/{line['folder']}/t{t}-{take}.mp3"
                        if rel not in state.clips or not (state.out / rel).exists():
                            todo.append((rel, voice, voice_id, s, line["folder"], t, take,
                                         spoken(text, s.get("tags", True))))
    chars = sum(len(c[-1]) for c in todo)
    print(f"{len(todo)} clip(s) to make, {chars} characters")
    if args.dry_run:
        for c in todo[:12]:
            print(f"  {c[0]}: {c[-1]}")
        if len(todo) > 12:
            print(f"  ... and {len(todo) - 12} more")
        return
    refused = set()
    for i, (rel, voice, voice_id, s, folder, t, take, text) in enumerate(todo, 1):
        if (voice, s["name"]) in refused:
            continue
        body = {"text": text, "model_id": s["model"], "seed": take}
        if s.get("settings"):
            body["voice_settings"] = s["settings"]
        try:
            audio, meta = api("POST", f"/v1/text-to-speech/{quote(voice_id)}", body,
                              {"output_format": FORMAT}, binary=True)
        except ApiError as e:
            print(f"{voice}--{s['name']}: {e}")
            if e.status in (401, 402, 403):
                break
            refused.add((voice, s["name"]))  # a model or setting refused: the rest would be too
            continue
        file = state.out / rel
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_bytes(audio)
        state.clips[rel] = {
            "voice": voice, "voice_id": voice_id, "style": s["name"], "folder": folder,
            "text_index": t, "take": take, "text": text, "model": s["model"],
            "settings": s.get("settings"), "seed": take, "made": now(),
            "request_id": meta["request_id"],
        }
        state.save()
        print(f"[{i}/{len(todo)}] {rel}")
    write_page(cfg, state)


def cmd_page(cfg, state, args):
    write_page(cfg, state)


# --- the page ------------------------------------------------------------------


def write_page(cfg, state):
    """index.html beside what was made: previews, then lines by voice and style."""
    style_order = [s["name"] for s in cfg.get("style", [])]
    voice_order = list(voices_to_speak(cfg, state))
    line_order = [l["folder"] for l in cfg.get("line", [])]

    def rank(seq, x):
        return seq.index(x) if x in seq else len(seq)

    columns = sorted({(c["voice"], c["style"]) for c in state.clips.values()},
                     key=lambda vs: (rank(voice_order, vs[0]), rank(style_order, vs[1]), vs))
    rows = sorted({(c["folder"], c["text_index"]) for c in state.clips.values()},
                  key=lambda ft: (rank(line_order, ft[0]), ft))
    texts = {}
    for c in state.clips.values():
        texts.setdefault((c["folder"], c["text_index"]), c["text"])
    grid = []
    for folder, t in rows:
        cells = []
        for voice, style in columns:
            takes = sorted(
                (c["take"], rel) for rel, c in state.clips.items()
                if (c["folder"], c["text_index"], c["voice"], c["style"]) == (folder, t, voice, style))
            cells.append([rel for _, rel in takes])
        grid.append({"folder": folder, "text": texts[(folder, t)], "cells": cells})
    data = {
        "designText": " ".join(cfg.get("design", {}).get("text", "").split()),
        "designs": [
            {"voice": voice, "rounds": rounds,
             "kept": {k["from"].split(" ")[1]: name for name, k in state.kept.items()
                      if k.get("from", "").split(" ")[0] == voice}}
            for voice, rounds in state.designs.items()
        ],
        "columns": [f"{v}--{s}" for v, s in columns],
        "grid": grid,
    }
    blob = json.dumps(data, ensure_ascii=False).replace("</", "<\\/")
    html = (HERE / "page.html").read_text().replace("/*DATA*/null", blob)
    state.out.mkdir(parents=True, exist_ok=True)
    (state.out / "index.html").write_text(html)
    print(f"page: {shown(state.out / 'index.html')}")


def main():
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--config", type=Path, default=HERE / "audition.toml")
    p.add_argument("--out", type=Path, default=ROOT / "target/scratch/announcer-audition")
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("status")
    d = sub.add_parser("design")
    d.add_argument("voices", nargs="*")
    d.add_argument("--dry-run", action="store_true")
    k = sub.add_parser("keep")
    k.add_argument("voice")
    k.add_argument("preview", help="as the page shows it, e.g. r1-2")
    k.add_argument("--name")
    s = sub.add_parser("speak")
    s.add_argument("--voices", nargs="+")
    s.add_argument("--styles", nargs="+")
    s.add_argument("--lines", nargs="+", help="words of the line folders to speak")
    s.add_argument("--dry-run", action="store_true")
    sub.add_parser("page")
    args = p.parse_args()
    cfg = load_config(args.config)
    state = State(args.out.resolve())
    {"status": cmd_status, "design": cmd_design, "keep": cmd_keep,
     "speak": cmd_speak, "page": cmd_page}[args.cmd](cfg, state, args)


if __name__ == "__main__":
    main()
