"""Approved FFmpeg comparison reference; normal generation uses shared Rust.

The reference was captured once from SoundCraft. Pack generation uses only
FFmpeg; its fixed room response approximates the original's modulation.
The game's shared finish pass still owns final leveling and peak limiting.
"""
import hashlib
import json
import math
from pathlib import Path
import subprocess

import numpy as np

HERE = Path(__file__).resolve().parent
RECIPE = HERE / "stadium.json"


def opening(folder):
    return folder in {"gameplay ready", "gameplay restart"} or folder.startswith("gameplay here we go")


def close_opening(full, color, direct_gain, fade_frames):
    """Fade only the spatial return; preserve the colored direct speech."""
    direct = color * direct_gain
    out = full[:len(color)].copy()
    n = min(fade_frames, len(out))
    if n:
        wet = out[-n:] - direct[-n:]
        out[-n:] = direct[-n:] + wet * np.linspace(1, 0, n, dtype=np.float32)
    return out


class StadiumReference:
    def __init__(self, out):
        self.recipe = json.loads(RECIPE.read_text())
        self.recipe["eq_ir"] = "../../assets/announcer-filters/eq.wav"
        self.recipe["space_ir"] = "../../assets/announcer-filters/space.wav"
        version = subprocess.run(["ffmpeg", "-version"], capture_output=True,
                                 check=True, text=True).stdout.splitlines()[0]
        self.version = version.split()[2]
        self.responses = [HERE / self.recipe[k] for k in ("eq_ir", "space_ir")]
        response_hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in self.responses}
        self.fingerprint = {**self.recipe, "ffmpeg_version": version,
                            "response_sha256": response_hashes,
                            "renderer_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
        self.work = out / ".stadium"
        self.work.mkdir(parents=True, exist_ok=True)

    def graph(self, frames):
        """No automatic IR normalization or mix normalization: keep the measured gains."""
        r = self.recipe
        sat = r["saturation"]
        rate = r["sample_rate"]
        a = math.exp(-2 * math.pi * sat["tone_hz"] / rate)
        drive = 10 ** (sat["drive_db"] / 20)
        output = 10 ** (sat["output_db"] / 20)
        mix = sat["mix"]
        fir = "afir=dry=1:wet=1:irnorm=-1:irgain=1:minp=256:maxp=8192:precision=float"
        return (
            f"[0:a]aresample={rate},aformat=sample_fmts=fltp:channel_layouts=mono,"
            f"apad=whole_len={frames},atrim=end_sample={frames}[in];"
            f"[in][1:a]{fir},asplit=2[dry][wet];"
            f"[wet]aeval='tanh({drive}*val(0))/tanh({drive})',"
            f"biquad=b0={1-a}:b1=0:b2=0:a0=1:a1={-a}:a2=0:precision=f32[soft];"
            f"[dry][soft]amix=inputs=2:weights='{1-mix} {mix}':normalize=0,"
            f"volume={output}:precision=float,asplit=2[color][direct];"
            f"[color][2:a]{fir},volume={r['bounce_gain']}:precision=float[full];"
            f"[direct]volume={r['bounce_gain']}:precision=float[voice];"
            "[full][voice]amerge=inputs=2[out]"
        )

    def render(self, path, body_seconds, folder):
        r = self.recipe
        rate = r["sample_rate"]
        timed = opening(folder)
        tail = 0 if timed else r["tail_seconds"]
        frames = round((body_seconds + tail) * rate)
        graph = self.graph(frames)
        (self.work / "filter.txt").write_text(graph + "\n")
        args = ["ffmpeg", "-nostdin", "-v", "error", "-i", str(path)]
        for response in self.responses:
            args += ["-i", str(response)]
        args += ["-filter_complex_threads", "1", "-filter_complex", graph,
                 "-map", "[out]", "-f", "f32le", "-"]
        ran = subprocess.run(args, capture_output=True)
        if ran.returncode:
            raise RuntimeError(f"FFmpeg stadium render failed (needs afir with irnorm):\n{ran.stderr.decode()}")
        samples = np.frombuffer(ran.stdout, dtype="<f4")
        if len(samples) != frames * 2 or not np.isfinite(samples).all():
            raise RuntimeError("FFmpeg stadium render returned invalid samples or duration")
        full, color = samples.reshape(-1, 2).T
        audio = full.copy()
        if timed:
            audio = close_opening(full, color, r["direct_gain"],
                                  round(r["opening_wet_fade_seconds"] * rate))
        return audio, {"body_seconds": body_seconds, "tail_seconds": tail,
                       "opening_wet_fade_seconds": r["opening_wet_fade_seconds"] if timed else 0}
