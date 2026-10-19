#!/usr/bin/env python3
"""Maintainer checks for the workspace SDK and separate research probes; never fetches source songs/runtimes."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--minimal", action="store_true", help="SDK without native/codec features (also checked on Windows)")
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
env = {k: v for k, v in os.environ.items() if not k.startswith("DDI_DANONI_")}
env["DDI_HOME"] = str(root / "target/scratch/danoni-check/home")
env["DDI_NO_GPU"] = "1"
env.setdefault("CARGO_TARGET_DIR", str(root / "target"))
# The large SDK's debug-symbol generation is costly on small CI runners.
env.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
env.setdefault("CARGO_BUILD_JOBS", "2")
if not args.minimal:
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        parser.error("ffmpeg is required for the independent Opus decoder check")
    env["DDI_DANONI_FFMPEG"] = ffmpeg


def cargo(package, command, *options):
    # Avoid repeatedly optimizing the SDK's many integration-test binaries.
    # Dependencies keep their normal optimizations (notably the audio codecs).
    if command == "test" and package == "desktop-probe":
        options = ("--config=profile.test.package.ddi-danoni-export.opt-level=0", *options)
    cmd = ["cargo", command, "--manifest-path", f"tools/danoni-dump/{package}/Cargo.toml", *options]
    print("+ " + " ".join(cmd), flush=True)
    subprocess.run(cmd, cwd=root, env=env, check=True)


packages = ["rust"] if args.minimal else ["rust", "desktop-probe", "pack-verify"]
for package in packages:
    cargo(package, "fmt", "--check")

features = ["--no-default-features"] if args.minimal else []
cargo("rust", "clippy", "--locked", "--all-targets", *features, "--", "-D", "warnings")
cargo("rust", "test", "--locked", *features, "--", "--test-threads=1")
if not args.minimal:
    cargo("rust", "clippy", "--locked", "--all-targets", "--no-default-features", "--", "-D", "warnings")
    cargo("rust", "test", "--locked", "--no-default-features", "--", "--test-threads=1")
    cargo("rust", "clippy", "--locked", "--all-targets", "--all-features", "--", "-D", "warnings")
    cargo("rust", "test", "--locked", "--all-features", "--test", "audio", "--test", "opus", "--test", "preparation", "--", "--test-threads=1")
    for package in ["desktop-probe", "pack-verify"]:
        cargo(package, "clippy", "--locked", "--all-targets", "--", "-D", "warnings")
        cargo(package, "test", "--locked", "--", "--test-threads=1")
