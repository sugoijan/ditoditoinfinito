#!/usr/bin/env python3
"""Optional development tool: recapture the stadium responses with SoundCraft 0.20.1.

Use a dedicated --no-audio --control instance; its document is replaced.
This is not used by pack generation. Writes candidates under target/scratch.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

import numpy as np

from effects import SoundCraft
from kokoro import write_wav

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--dest", type=Path, default=ROOT / "target/scratch/stadium-responses")
    args = parser.parse_args()
    dest = args.dest.resolve()
    if not dest.is_relative_to(ROOT / "target/scratch"):
        parser.error("--dest must be under this checkout's target/scratch")
    dest.mkdir(parents=True, exist_ok=True)
    recipe = json.loads((HERE / "stadium-soundcraft.json").read_text())
    capture = json.loads((HERE / "stadium.json").read_text())["reference"]
    rate = recipe["sample_rate"]
    settle = round(capture["settle_seconds"] * rate)
    impulse = np.zeros(settle + 4 * rate, dtype=np.float32)
    impulse[settle] = capture["impulse_amplitude"]
    source = dest / "impulse.wav"
    write_wav(source, impulse, rate)
    client = SoundCraft(args.port)
    records = {}
    try:
        for name, inserts in [("dry", []), ("eq", recipe["inserts"][:1]),
                              ("space", recipe["inserts"][2:])]:
            client.execute("session.new", name=f"stadium response {name}", sample_rate=rate)
            imported = client.execute("file.import_audio", path=str(source))
            track = imported["imported"][0]["track"]
            for slot, insert in enumerate(inserts):
                client.execute("mix.insert", track=track, slot=slot, **insert)
            snapshot = client.call("session.inspect", {"detail": "full"})
            installed = snapshot["tracks"][0]["inserts"]
            if len(installed) != len(inserts):
                raise RuntimeError("SoundCraft did not install every reference insert")
            for actual, wanted in zip(installed, inserts):
                if (actual["plugin"] != wanted["plugin"] or actual["bypass"] or
                        any(abs(actual["params"][k] - v) > 1e-4 for k, v in wanted["params"].items())):
                    raise RuntimeError("SoundCraft did not apply the reference recipe")
            bounced = dest / f"{name}-stereo.wav"
            client.execute("file.bounce_mix", path=str(bounced), bit_depth="32f", start=0,
                           end=len(impulse), normalize=False, dither=False)
            raw = subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-i", str(bounced),
                                  "-ac", "1", "-f", "f32le", "-"], check=True, capture_output=True).stdout
            response = np.frombuffer(raw, dtype="<f4")[settle:].copy()
            if name == "dry":
                gain = float(response[0]) / capture["impulse_amplitude"]
                if np.count_nonzero(response[1:]):
                    raise RuntimeError("The dry impulse has a tail")
            else:
                response *= 1 / capture["impulse_amplitude"]
                response /= gain
                length = round(capture[f"{name}_seconds"] * rate)
                omitted = float(abs(response[length:]).max())
                if omitted > 1e-7:
                    raise RuntimeError(f"{name}: capture window is too short")
                path = dest / f"{name}.wav"
                write_wav(path, response[:length], rate)
                records[name] = {"sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                                 "omitted_peak": omitted, "session": snapshot}
        (dest / "capture.json").write_text(json.dumps(
            {"reference": capture, "bounce_gain": gain, "responses": records}, indent=2) + "\n")
        print(f"Captured responses in {dest}; compare hashes before replacing the checked-in files.")
    finally:
        client.close()


if __name__ == "__main__":
    main()
