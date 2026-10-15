#!/usr/bin/env python3
"""Prepare a separate build from a verified Ruffle release; never execute a movie."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from paths import external_directory

ROOT = Path(__file__).resolve().parent
REVISION = "89a7049965f3090e706265670df309a5128233ac"
HOOKS = {
    "render/wgpu/src/backend.rs": '\npub fn ddi_adapters(instance: &wgpu::Instance, backends: wgpu::Backends) -> Vec<wgpu::Adapter> { futures::executor::block_on(instance.enumerate_adapters(backends)) }\npub fn ddi_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), wgpu::RequestDeviceError> { futures::executor::block_on(request_device(adapter)) }\n',
    "core/src/lib.rs": '\npub mod ddi_snapshot;\n',
    "core/src/avm1/object/script_object.rs": '\ninclude!("../../ddi_properties.rs");\n',
    "core/src/avm1/runtime.rs": "\nimpl Avm1<'_> { pub(crate) fn ddi_halted(&self) -> bool { self.halted } }\n",
}


MEMORY_HINT_FROM = 'memory_hints: Default::default(),'
MEMORY_HINT_TO = 'memory_hints: if adapter.get_info().device_type == wgpu::DeviceType::Cpu { wgpu::MemoryHints::Manual { suballocated_device_memory_block_size: 4 * 1024 * 1024..4 * 1024 * 1024 } } else { Default::default() },'

def patched(path, original):
    if path != "render/wgpu/src/backend.rs":
        return original
    if original.count(MEMORY_HINT_FROM.encode()) != 1:
        raise ValueError("Pinned memory hint location differs")
    return original.replace(MEMORY_HINT_FROM.encode(), MEMORY_HINT_TO.encode())

def write_changed(path, data):
    if not path.exists() or path.read_bytes() != data:
        path.write_bytes(data)


def prepare(upstream, build):
    upstream, build = upstream.resolve(), external_directory(build)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=upstream, text=True).strip()
    if revision != REVISION:
        raise ValueError(f"Expected Ruffle v0.7.1 ({REVISION}), got {revision}")
    if build == upstream or build.is_relative_to(upstream) or build.is_relative_to(ROOT.parents[2]):
        raise ValueError("Build directory must be outside the upstream and project checkouts")
    # Verify all tracked source changes, not just hook locations.
    changed = subprocess.check_output(["git", "diff", "HEAD", "--name-only"], cwd=upstream, text=True).splitlines()
    if set(changed) - HOOKS.keys():
        raise ValueError(f"Unexpected upstream modifications: {changed}")
    for path, hook in HOOKS.items():
        original = subprocess.check_output(["git", "show", f"{REVISION}:{path}"], cwd=upstream)
        current = (upstream / path).read_bytes()
        prepared = patched(path, original) + hook.encode()
        if current not in (original, original + hook.encode(), prepared):
            raise ValueError(f"Unexpected patch in {path}")
        write_changed(upstream / path, prepared)
    write_changed(upstream / "core/src/ddi_properties.rs", (ROOT / "core-properties.rs").read_bytes())
    write_changed(upstream / "core/src/ddi_snapshot.rs", (ROOT / "core-snapshot.rs").read_bytes())
    (build / "src").mkdir(parents=True, exist_ok=True)
    # JSON string escaping is also valid for these TOML basic string path contents.
    escaped = json.dumps(str(upstream))[1:-1]
    write_changed(build / "Cargo.toml", (ROOT / "Cargo.toml.in").read_text().replace("@UPSTREAM@", escaped).encode())
    for path in (ROOT / "src").glob("*.rs"):
        write_changed(build / "src" / path.name, path.read_bytes())
    if (ROOT / "Cargo.lock").exists():
        write_changed(build / "Cargo.lock", (ROOT / "Cargo.lock").read_bytes())
    identity = {"release":"v0.7.1", "revision":revision, "files":{}}
    for path in [ROOT / "prepare.py", ROOT / "Cargo.toml.in", ROOT / "Cargo.lock",
                 ROOT / "core-properties.rs", ROOT / "core-snapshot.rs", *sorted((ROOT / "src").glob("*.rs"))]:
        identity["files"][path.relative_to(ROOT).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    (build / "build-identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    print(build)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("build", type=Path)
    args = parser.parse_args()
    prepare(args.upstream, args.build)
