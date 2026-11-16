"""Pack adapter for the studio's shared Rust stadium-soft processor.

FFmpeg is used only for final encoding/metering, outside this module.
"""
import hashlib
import json
from pathlib import Path
import subprocess

import numpy as np

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
RECIPE = HERE / "stadium.json"


def opening(folder):
    return folder in {"gameplay ready", "gameplay restart"} or folder.startswith("gameplay here we go")


class Stadium:
    def __init__(self, out, command=None):
        self.recipe = json.loads(RECIPE.read_text())
        self.version = self.recipe["version"]
        self.command = command or ["cargo", "run", "-q", "-p", "xtask", "--"]
        sources = [ROOT / "crates/library/src/announcer/stadium.rs",
                   ROOT / "crates/library/src/announcer/tidy.rs", ROOT / "crates/library/src/fft.rs"]
        responses = [HERE / self.recipe[k] for k in ("eq_ir", "space_ir")]
        self.fingerprint = {**self.recipe,
            "source_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},
            "response_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in responses},
            "renderer_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
        self.work = out / ".stadium"
        self.work.mkdir(parents=True, exist_ok=True)

    def render(self, path, body_seconds, folder):
        out = self.work / "processed.wav"
        cmd = [*self.command, "announcer", "stadium", "--input", str(path.resolve()), "--out", str(out.resolve())]
        timed = opening(folder)
        if timed:
            cmd.append("--opening")
        ran = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
        if ran.returncode:
            raise RuntimeError(f"The shared stadium processor failed:\n{ran.stderr}")
        report = json.loads(ran.stdout)
        if report["version"] != self.version or abs(report["body_seconds"]-body_seconds) > 1e-6:
            raise RuntimeError("The built xtask does not match the stadium recipe/input")
        raw = out.read_bytes()
        if raw[20:24] != b"\x03\x00\x01\x00" or raw[36:40] != b"data":
            raise RuntimeError("The stadium processor did not return a float mono WAV")
        audio = np.frombuffer(raw[44:], dtype="<f4").copy()
        render = {"body_seconds": body_seconds, "tail_seconds": 0 if timed else self.recipe["tail_seconds"],
                  "opening_wet_fade_seconds": self.recipe["opening_wet_fade_seconds"] if timed else 0}
        return audio, render, report
