#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11,<3.13"
# dependencies = [
#   "misaki==0.9.4",
#   "num2words",
#   "spacy>=3.8,<3.9",
#   "en-core-web-sm @ https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl",
#   "fugashi",
#   "jaconv",
#   "mojimoji",
#   "pyopenjtalk",
#   "unidic-lite",
#   "onnx",
#   "onnxruntime",
#   "numpy",
#   "huggingface-hub",
# ]
# [tool.uv]
# exclude-newer = "2026-11-01T00:00:00Z"
# ///
"""Audition announcer voices made here with Kokoro-82M, an open-weights model,
and make our announcer with the one chosen.

    uv run tools/announcer-design/kokoro.py sample [VOICE ...]
    uv run tools/announcer-design/kokoro.py speak [--voices ...] [--styles plain lead] [--lines ...]
    uv run tools/announcer-design/kokoro.py context
    uv run tools/announcer-design/kokoro.py pack [PACK.toml ...]

`sample` has every voice of `[kokoro] sample` (Kokoro's own) and every
blend of `[[kokoro.voice]]` read `[design] text`; `speak` has the
`[[kokoro.voice]]` voices say the lines, as they are (`plain`) and said
after `[kokoro] lead`, which is cut off (`lead`: Kokoro is weakest on
short utterances, VOICES.md says). They go beside announcer.py's, under
target/scratch/announcer-audition/ (`--out`), on the same page and as
the same StepMania announcers (`Announcers/<voice>--kokoro[-lead]/`).
`context` has `[[kokoro.context]]`'s lines said plainly, after each
mood's lead, before its closer and between both, all cut off: how what
is said around a line changes how it is said.
`pack` makes the whole announcer a pack file writes (pack.toml, the
English one, and pack-ja.toml, the Japanese, by default), every line the
game speaks with its variants, as `Announcers/<name>/` there, on the page
too. Kokoro speaks English (its `a` and `b` voices) and Japanese (`j`)
here; a voice speaks its own language unless `lang` gives another.

uv makes the environment from the header above; the model comes from
Hugging Face at the pinned revision (its cache, when it holds it). No
account or key. Kokoro is Apache-2.0, misaki its phonemizer Apache-2.0;
what they make is ours to release as CC0. Made again only when what it
was made from changed. See README.md.
"""
import argparse
import json
import re
import subprocess
import sys
import tomllib
from importlib.metadata import version
from pathlib import Path

import numpy as np

sys.dont_write_bytecode = True  # no __pycache__ in the tools' folder
import announcer as A  # noqa: E402 (beside this file: the state, the page and the helpers)

RATE = 24_000
# The export's tensor of each phoneme's predicted length (in the model's
# frames), made an output too: where the lead ends in what is said.
DURATIONS = "/encoder/Gather_output_0"
STYLES = {"plain": "kokoro", "lead": "kokoro-lead"}
# Said before a line in the `lead` style, then cut off.
LEAD = "Alright, everybody.…"
# Where the game speaks a line, by its folder's first word: a pack may
# give a lead for each.
PLACES = {"select": "song select", "gameplay": "play", "evaluation": "the results"}
# How the sound is made from the model's: raise it when that changes, and
# everything is made again.
PROCESS = 3
GROUPS = {"af": "US female", "am": "US male", "bf": "UK female", "bm": "UK male",
          "jf": "Japanese female", "jm": "Japanese male"}
# Kokoro's languages this script speaks, by its codes: a voice's first letter.
LANGS = {"a": "American English", "b": "British English", "j": "Japanese"}
# The phonemizer's packages, recorded with what each made.
PACKAGES = {"a": ("misaki", "spacy", "en_core_web_sm"), "b": ("misaki", "spacy", "en_core_web_sm"),
            "j": ("misaki", "fugashi", "unidic-lite", "pyopenjtalk")}


class Kokoro:
    def __init__(self, k):
        from huggingface_hub import hf_hub_download
        import onnx
        import onnxruntime as ort

        self.repo = k.get("repo", "onnx-community/Kokoro-82M-v1.0-ONNX")
        self.revision = k["revision"]
        self.model = k.get("model", "onnx/model.onnx")
        self.fetch = lambda f: Path(hf_hub_download(self.repo, f, revision=self.revision))
        tokenizer = json.loads(self.fetch("tokenizer.json").read_text())
        self.vocab = tokenizer["model"]["vocab"]
        graph = onnx.load(self.fetch(self.model))
        if not any(DURATIONS in n.output for n in graph.graph.node):
            sys.exit(f"{self.model} has no {DURATIONS}: another export than this script knows")
        graph.graph.output.append(onnx.helper.make_tensor_value_info(DURATIONS, onnx.TensorProto.INT64, None))
        opts = ort.SessionOptions()
        opts.log_severity_level = 3
        self.session = ort.InferenceSession(graph.SerializeToString(), opts,
                                            providers=["CPUExecutionProvider"])
        words = k.get("words", {})
        self.words = words
        self.marks = (re.compile("|".join(
            rf"(?<!\w){re.escape(w)}(?!\w)" for w in sorted(words, key=len, reverse=True)))
            if words else None)
        self.g2ps = {}
        self.packs = {}

    def versions(self, lang):
        return {p: version(p) for p in (*PACKAGES[lang], "onnxruntime")}

    def ids(self, ps):
        return [self.vocab[c] for c in ps if c in self.vocab]

    def phonemes(self, text, lang):
        """misaki's phonemes for the text in the language: in English, the
        configured words as given; Japanese as its dictionary reads it
        (Kokoro's own way: cutlet over UniDic, its lite edition)."""
        g2p = self.g2ps.get(lang)
        if g2p is None:
            if lang in ("a", "b"):
                from misaki import en
                g2p = en.G2P(trf=False, british=lang == "b", fallback=None)
            elif lang == "j":
                from misaki import ja
                g2p = ja.JAG2P()
            else:
                sys.exit(f"lang {lang!r}: this script speaks {', '.join(f'{c} ({n})' for c, n in LANGS.items())}")
            self.g2ps[lang] = g2p
        english = lang in ("a", "b")
        marked = self.marks.sub(lambda m: f"[{m[0]}](/{self.words[m[0]]}/)", text) \
            if self.marks and english else text
        ps, tokens = g2p(marked)
        unknown = [t.text for t in tokens or [] if t.phonemes in (None, "❓")]
        if unknown or "❓" in ps:
            sys.exit(f"{text!r}: Kokoro's dictionary lacks {', '.join(unknown) or 'a word'}; "
                     "give it in [kokoro.words] as misaki's phonemes")
        # What Kokoro has no phoneme for (Japanese passes words it cannot
        # read through as they are written: Latin letters, a rare kanji).
        unsaid = sorted({c for c in ps if c not in self.vocab or (not english and c.isascii() and c.isupper())})
        if unsaid:
            sys.exit(f"{text!r} gives {ps!r}: Kokoro cannot say {' '.join(unsaid)}; "
                     + ("write them in kana" if lang == "j" else "give the word in [kokoro.words]"))
        return ps

    def pack(self, name):
        if name not in self.packs:
            raw = np.fromfile(self.fetch(f"voices/{name}.bin"), dtype=np.float32)
            self.packs[name] = raw.reshape(-1, 1, 256)
        return self.packs[name]

    def synth(self, ps, voice, speed):
        """The sound, and each token's samples (the pads' at either end too)."""
        ids = self.ids(ps)
        if len(ids) > 510:
            sys.exit(f"{ps!r}: {len(ids)} phonemes, Kokoro takes 510")
        mix = mixture(voice)
        style = sum(self.pack(v)[min(len(ids), 509)] * w for v, w in mix.items())
        audio, frames = self.session.run(None, {
            "input_ids": np.array([[0, *ids, 0]], dtype=np.int64),
            "style": style.astype(np.float32),
            "speed": np.array([speed], dtype=np.float32),
        })
        audio, frames = audio[0], frames.reshape(-1)
        hop = len(audio) // int(frames.sum())
        return audio, frames * hop

    def around(self, lead_ps, ps, closer_ps, voice, speed):
        """The line said after a lead and before a closer (either None), each
        cut off in the quietest 10 ms of the quarter second before what
        follows it (where the model's durations put that: its first sound
        starts a little earlier). Where each was cut, in seconds, or None."""
        audio, samples = self.synth(" ".join(p for p in (lead_ps, ps, closer_ps) if p), voice, speed)

        def cut_before(at):
            lo, hi = max(0, at - RATE // 4), min(len(audio) - FRAME, at + RATE // 40)
            quietest = min(range(lo, hi, FRAME // 2), key=lambda i: float(np.sum(audio[i:i + FRAME] ** 2)))
            return quietest + FRAME // 2

        before = 1 + (len(self.ids(lead_ps)) + 1 if lead_ps else 0)  # the pad, the lead and its space
        start = cut_before(int(samples[:before].sum())) if lead_ps else 0
        end = cut_before(int(samples[:before + len(self.ids(ps)) + 1].sum())) if closer_ps else len(audio)
        return audio[start:end], start / RATE if lead_ps else None, end / RATE if closer_ps else None


def mixture(voice):
    """A voice's style vectors and their weights: one of Kokoro's, or a blend."""
    mix = {voice: 1.0} if isinstance(voice, str) else dict(voice)
    total = sum(mix.values())
    return {v: w / total for v, w in mix.items()}


def own_language(voice):
    """The language of the voice that weighs most in it (its first letter)."""
    return max(mixture(voice).items(), key=lambda vw: vw[1])[0][0]


def described(voice, speed, lang=None):
    mix = mixture(voice)
    s = " + ".join(f"{v} {w:.0%}" if len(mix) > 1 else v for v, w in mix.items())
    if lang and lang != own_language(voice):
        s += f" speaking {LANGS.get(lang, lang)}"
    return s + (f", speed {speed}" if speed != 1 else "")


def with_lang(params, voice, lang):
    """The parameters, with the language where it is not the voice's own
    (so what was made before languages were keeps its parameters)."""
    return {**params, "lang": lang} if lang != own_language(voice) else params


# --- the sound -------------------------------------------------------------------

FRAME = RATE // 100  # 10 ms


def levels(audio):
    """Each 10 ms frame's level, in dB under the loudest frame."""
    n = len(audio) // FRAME
    rms = np.sqrt(np.mean(audio[: n * FRAME].reshape(n, FRAME) ** 2, axis=1)) + 1e-9
    return 20 * np.log10(rms / rms.max())


def trim(audio, head=0.02, tail=0.08, floor=-45):
    loud = np.nonzero(levels(audio) > floor)[0]
    if not len(loud):
        return audio
    a = max(0, loud[0] * FRAME - int(head * RATE))
    b = min(len(audio), (loud[-1] + 1) * FRAME + int(tail * RATE))
    return audio[a:b]


def finish(audio):
    """Trimmed, faded at the ends and leveled: voiced RMS at -18 dBFS, peaks under -1."""
    audio = trim(audio).astype(np.float64)
    fade_in, fade_out = int(0.005 * RATE), int(0.015 * RATE)
    audio[:fade_in] *= np.linspace(0, 1, fade_in)
    audio[-fade_out:] *= np.linspace(1, 0, fade_out)
    n = len(audio) // FRAME
    frames = audio[: n * FRAME].reshape(n, FRAME)
    voiced = frames[levels(audio) > -30]
    rms = np.sqrt(np.mean(voiced ** 2)) if len(voiced) else 1e-9
    gain = min(10 ** (-18 / 20) / rms, 10 ** (-1 / 20) / (np.abs(audio).max() + 1e-9))
    return (audio * gain).astype(np.float32)


def write_mp3(path, audio):
    path.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-f", "f32le", "-ar", str(RATE), "-ac", "1", "-i", "-",
         "-ar", "44100", "-c:a", "libmp3lame", "-b:a", "128k", str(path)],
        input=audio.astype("<f4").tobytes(), check=True)


# --- commands ----------------------------------------------------------------------


def fresh(record, params, file):
    return record is not None and record.get("params") == params and file.exists()


def render(engine, said, voice, speed, lead, lang):
    """A line said, finished: its sound, its phonemes and where the lead
    was cut off (None without one)."""
    ps = engine.phonemes(said, lang)
    if lead:
        audio, cut, _ = engine.around(engine.phonemes(lead, lang), ps, None, voice, speed)
    else:
        audio, cut = engine.synth(ps, voice, speed)[0], None
    return finish(audio), ps, cut


# Kokoro's Japanese phonemes in plain letters, for file names.
ROMAJI = str.maketrans({"ɯ": "u", "ɨ": "u", "ɾ": "r", "ɸ": "f", "β": "w", "ç": "h", "ɕ": "sh",
                        "ʥ": "j", "ʨ": "ch", "ʦ": "ts", "ʣ": "z", "ɲ": "n", "ɴ": "n", "ɡ": "g",
                        "ʲ": "y", "ː": "", "ʔ": "", "ŋ": "n", "ɰ": "w"})


def slug(text):
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")[:40].strip("-") or "line"


def english(lang):
    return lang in ("a", "b")


def cmd_sample(cfg, k, engine, state, args):
    """Kokoro's voices, and ours, each reading the text of its language."""
    texts = {lang: " ".join(t.split()) for lang, t in k.get("texts", {}).items()}
    design = " ".join(cfg.get("design", {}).get("text", "").split())
    entries = [{"name": v, "voice": v, "group": GROUPS.get(v[:2], "Kokoro")} for v in k.get("sample", [])]
    for v in k.get("voice", []):
        # Kokoro's own voices as they are are sampled already.
        if not isinstance(v["voice"], str) or v.get("speed", 1) != 1 or "lang" in v:
            entries.append({**v, "group": "Ours"})
    if args.voices:
        entries = [e for e in entries if e["name"] in args.voices]
    made = 0
    for e in entries:
        speed, lang = e.get("speed", 1), e.get("lang") or own_language(e["voice"])
        text = texts.get(lang) or (design if english(lang) else None)
        if not text:
            sys.exit(f"{e['name']}: no [kokoro.texts] {lang} to read ({LANGS.get(lang, lang)})")
        params = with_lang({"repo": engine.repo, "revision": engine.revision, "model": engine.model,
                            "process": PROCESS, "voice": e["voice"], "speed": speed, "text": text},
                           e["voice"], lang)
        rel = f"kokoro/sample/{e['name']}.mp3"
        if fresh(state.samples.get(rel), params, state.out / rel):
            state.samples[rel]["group"] = e["group"]
            continue
        ps = engine.phonemes(text, lang)
        audio = finish(engine.synth(ps, e["voice"], speed)[0])
        write_mp3(state.out / rel, audio)
        state.samples[rel] = {
            "name": e["name"], "group": e["group"], "detail": described(e["voice"], speed, lang),
            "params": params, "phonemes": ps, "duration_secs": round(len(audio) / RATE, 2),
            "versions": engine.versions(lang), "made": A.now(),
        }
        state.save()
        made += 1
        print(f"{rel}")
    state.save()
    print(f"{made} sample(s) made, {len(entries) - made} kept")
    A.write_page(cfg, state)


def cmd_speak(cfg, k, engine, state, args):
    voices = k.get("voice", [])
    if args.voices:
        unknown = set(args.voices) - {v["name"] for v in voices}
        if unknown:
            sys.exit(f"no [[kokoro.voice]] named {', '.join(sorted(unknown))}")
        voices = [v for v in voices if v["name"] in args.voices]
    lines = cfg.get("line", [])
    if args.lines:
        lines = [l for l in lines if any(w in l["folder"] for w in args.lines)]
    lead = k.get("lead", LEAD)
    made = kept = 0
    for v in voices:
        speed, lang = v.get("speed", 1), v.get("lang") or own_language(v["voice"])
        if not english(lang):
            print(f"{v['name']}: the lines are English; {LANGS.get(lang, lang)} is a pack's (pack-ja.toml)")
            continue
        for st in args.styles:
            style = STYLES[st]
            for line in lines:
                for t, text in enumerate(line["texts"], 1):
                    said = A.spoken(text, False)
                    params = with_lang({"repo": engine.repo, "revision": engine.revision, "model": engine.model,
                                        "process": PROCESS, "voice": v["voice"], "speed": speed, "text": said,
                                        "lead": lead if st == "lead" else None}, v["voice"], lang)
                    rel = f"Announcers/{v['name']}--{style}/{line['folder']}/t{t}-1.mp3"
                    if fresh(state.clips.get(rel), params, state.out / rel):
                        kept += 1
                        continue
                    audio, ps, cut = render(engine, said, v["voice"], speed, params["lead"], lang)
                    write_mp3(state.out / rel, audio)
                    state.clips[rel] = {
                        "voice": v["name"], "style": style, "folder": line["folder"],
                        "text_index": t, "take": 1, "text": said, "engine": "kokoro",
                        "detail": described(v["voice"], speed, lang), "params": params,
                        "phonemes": ps, "cut_secs": cut, "duration_secs": round(len(audio) / RATE, 2),
                        "versions": engine.versions(lang), "made": A.now(),
                    }
                    state.save()
                    made += 1
                    print(f"{rel}")
    print(f"{made} clip(s) made, {kept} kept")
    A.write_page(cfg, state)


def cmd_context(cfg, k, engine, state, args):
    """How what is said around a line changes how it is said: each study's
    lines said plainly, then after each mood's lead, before its closer and
    between both, cut off each time."""
    wanted = set()
    made = kept = 0
    for study in k.get("context", []):
        sname, voice, speed = study["name"], study["voice"], study.get("speed", 1)
        lang = study.get("lang") or own_language(voice)
        ways = [("plain", None, None)]
        for m in study.get("mood", []):
            ways += [(f"{m['name']}-lead", m["lead"], None), (f"{m['name']}-closer", None, m["closer"]),
                     (f"{m['name']}-both", m["lead"], m["closer"])]
        for name in [sname] + [m["name"] for m in study.get("mood", [])]:
            if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", name) or "--" in name:
                sys.exit(f"{name!r}: names are lower case letters, digits and single hyphens")
        for li, line in enumerate(study["lines"]):
            said = A.spoken(line, False)
            for way, lead, closer in ways:
                rel = f"kokoro/context/{sname}/{slug(said)}/{way}.mp3"
                if rel in wanted:
                    sys.exit(f"context {sname}: {line!r} twice")
                wanted.add(rel)
                params = with_lang({"repo": engine.repo, "revision": engine.revision, "model": engine.model,
                                    "process": PROCESS, "voice": voice, "speed": speed, "text": said,
                                    "lead": lead, "closer": closer}, voice, lang)
                place = {"study": sname, "line_index": li, "way": way}
                if fresh(state.contexts.get(rel), params, state.out / rel):
                    state.contexts[rel].update(place)
                    kept += 1
                    continue
                ps = engine.phonemes(said, lang)
                if lead or closer:
                    raw, cut, cut_end = engine.around(lead and engine.phonemes(lead, lang), ps,
                                                      closer and engine.phonemes(closer, lang), voice, speed)
                else:
                    raw, cut, cut_end = engine.synth(ps, voice, speed)[0], None, None
                # How loud the sound is where it was cut, under its loudest:
                # above -35 dB the cut is in a sound, not the pause.
                db = levels(raw)
                audio = finish(raw)
                write_mp3(state.out / rel, audio)
                state.contexts[rel] = {
                    **place, "text": said, "detail": described(voice, speed, lang), "params": params,
                    "phonemes": ps, "cut_secs": cut, "cut_end_secs": cut_end,
                    "cut_db": [round(float(db[0]), 1) if lead else None, round(float(db[-1]), 1) if closer else None],
                    "duration_secs": round(len(audio) / RATE, 2), "versions": engine.versions(lang), "made": A.now(),
                }
                state.save()
                made += 1
                print(rel)
    removed = 0
    for rel in [r for r in state.contexts if r not in wanted]:
        file = state.out / rel
        file.unlink(missing_ok=True)
        del state.contexts[rel]
        removed += 1
        for d in file.parents[:2]:  # its line's folder, and its study's
            if d.is_dir() and not any(d.iterdir()):
                d.rmdir()
    state.save()
    print(f"{len(wanted)} clip(s): {made} made, {kept} kept, {removed} removed")
    A.write_page(cfg, state)


def cmd_pack(cfg, source, pack, engine, state):
    """Every line of the pack, each text a sound in its folder; made again
    where what it is made from changed, and what the pack no longer has
    removed. `source` is the pack file's name: what it made is its own."""
    name, voice, speed = pack["name"], pack["voice"], pack.get("speed", 1)
    lang = pack.get("lang") or own_language(voice)
    # Any name a folder can have on every system (Japanese too), and not
    # an audition's (`<voice>--<style>`).
    if (not name.strip() or name != name.strip(" .") or re.search(r'[\x00-\x1f/\\:*?"<>|]', name)
            or "--" in name):
        sys.exit(f"{name!r}: a pack's name is a folder's: none of / \\ : * ? \" < > |, no space or "
                 "dot at either end, no --")
    others = {c.get("source", "pack.toml") for c in state.pack.values() if c["pack"] == name} - {source}
    if others:
        sys.exit(f"{source}: {others.pop()} makes a pack named {name!r} already")
    # One lead for every line, or a [lead] table of one for each place.
    leads = pack.get("lead")
    if not isinstance(leads, dict):
        leads = dict.fromkeys(PLACES, leads)
    elif set(leads) - set(PLACES):
        sys.exit(f"{source}: [lead] {', '.join(sorted(set(leads) - set(PLACES)))}: "
                 f"its leads are for {', '.join(PLACES)}")
    # Each line's style and lead, all of them before anything is made.
    lines = []
    for line in pack.get("line", []):
        # A line with a lead of its own is in the lead style.
        style = line.get("style") or ("lead" if "lead" in line else pack.get("style", "lead"))
        if style not in STYLES:
            sys.exit(f"{line['folder']}: style {style!r}, not one of {', '.join(STYLES)}")
        if style != "lead" and "lead" in line:
            sys.exit(f"{source}: {line['folder']}: a `lead` in the {style} style, which has none")
        lead = None
        if style == "lead":
            where = line["folder"].split(" ")[0]
            lead = line.get("lead") or leads.get(where) or (LEAD if english(lang) else None)
            if not lead:
                sys.exit(f"{source}: {line['folder']}: the lead style needs a `lead` "
                         f"in {LANGS.get(lang, lang)}" + (f" (or [lead] {where})" if where in PLACES else ""))
        lines.append((line, style, lead))
    # A folder may be given in several [[line]]s, each with its own style
    # and lead (the same text after two leads, say): its sounds are theirs
    # together. The same sound twice is a slip.
    sounds = set()
    for line, style, lead in lines:
        for text in line["texts"]:
            said = (line["folder"], A.spoken(text, False), lead)
            if said in sounds:
                sys.exit(f"{source}: {line['folder']}: {text!r} twice, "
                         + (f"both after {lead!r}" if lead else "both plain"))
            sounds.add(said)
    folders = {line["folder"] for line, _, _ in lines}
    names = {}  # each folder's files' names, across its lines
    wanted = set()
    made = kept = 0
    for li, (line, style, lead) in enumerate(lines):
        files = names.setdefault(line["folder"], set())
        for ti, text in enumerate(line["texts"], 1):
            said = A.spoken(text, False)
            # Named by the text, or by how it sounds where it is in other letters.
            base = slug(said if english(lang) else engine.phonemes(said, lang).translate(ROMAJI))
            file, n = base, 2
            while file in files:
                file, n = f"{base}-{n}", n + 1
            files.add(file)
            rel = f"Announcers/{name}/{line['folder']}/{file}.mp3"
            wanted.add(rel)
            params = with_lang({"repo": engine.repo, "revision": engine.revision, "model": engine.model,
                                "process": PROCESS, "voice": voice, "speed": speed, "text": said,
                                "lead": lead}, voice, lang)
            place = {"pack": name, "source": source, "folder": line["folder"], "line_index": li,
                     "text_index": ti, "style": style}
            if fresh(state.pack.get(rel), params, state.out / rel):
                state.pack[rel].update(place)
                kept += 1
                continue
            audio, ps, cut = render(engine, said, voice, speed, lead, lang)
            write_mp3(state.out / rel, audio)
            state.pack[rel] = {
                **place, "text": said, "detail": described(voice, speed, lang), "params": params,
                "phonemes": ps, "cut_secs": cut, "duration_secs": round(len(audio) / RATE, 2),
                "versions": engine.versions(lang), "made": A.now(),
            }
            state.save()
            made += 1
            print(rel)
    removed = 0
    mine = [r for r, c in state.pack.items() if c.get("source", "pack.toml") == source]
    for rel in [r for r in mine if r not in wanted]:
        file = state.out / rel
        file.unlink(missing_ok=True)
        del state.pack[rel]
        removed += 1
        for d in file.parents[:2]:  # its line's folder, and its pack's (a pack renamed)
            if d.is_dir() and not any(d.iterdir()):
                d.rmdir()
    root = state.out / "Announcers" / name
    for f in sorted(root.rglob("*"), reverse=True):
        if f.is_file() and f.relative_to(state.out).as_posix() not in wanted:
            f.unlink()
            removed += 1
        elif f.is_dir() and not any(f.iterdir()):
            f.rmdir()
    state.save()
    print(f"{name}: {len(folders)} lines, {len(wanted)} sounds ({made} made, {kept} kept, {removed} removed)")


def main():
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--config", type=Path, default=A.HERE / "audition.toml")
    p.add_argument("--out", type=Path, default=A.ROOT / "target/scratch/announcer-audition")
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("sample")
    s.add_argument("voices", nargs="*")
    sp = sub.add_parser("speak")
    sp.add_argument("--voices", nargs="+")
    sp.add_argument("--styles", nargs="+", choices=list(STYLES), default=list(STYLES))
    sp.add_argument("--lines", nargs="+", help="words of the line folders to speak")
    sub.add_parser("context")
    pk = sub.add_parser("pack")
    pk.add_argument("packs", nargs="*", type=Path,
                    default=[A.HERE / "pack.toml", A.HERE / "pack-ja.toml"])
    args = p.parse_args()
    cfg = A.load_config(args.config)
    k = cfg.get("kokoro", {})
    for v in k.get("voice", []):
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", v["name"]) or "--" in v["name"]:
            sys.exit(f"{v['name']!r}: names are lower case letters, digits and single hyphens")
    state = A.State(args.out.resolve())
    if args.cmd == "pack":
        for path in args.packs:
            with open(path, "rb") as f:
                pack = tomllib.load(f)
            cmd_pack(cfg, path.name, pack, Kokoro(pack["kokoro"]), state)
        A.write_page(cfg, state)
        return
    engine = Kokoro(k)
    {"sample": cmd_sample, "speak": cmd_speak, "context": cmd_context}[args.cmd](cfg, k, engine, state, args)


if __name__ == "__main__":
    main()
