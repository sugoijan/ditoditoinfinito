# Portable native Ruffle rendering

For the subsequent x86-64 Linux implementation, confinement requirements and
validation scope, see [the Linux handover](danoni-rust-linux.md). The dated
macOS-only results below remain historical evidence.

The native extractor reads chart arrays and clocks from the ActionScript runtime.
Artwork capture additionally needs offscreen frames. A movie can also call
[`BitmapData.draw`][bitmap] during initialization or play: this asks the runtime to render
into a bitmap, even without a visible window. Calling it does not establish that
chart data depends on the pixels. Our strict audit rejects unsupported calls until
we establish a complete execution or a separately reviewed independence proof.
There is no new exception to that audit in this change.

Ruffle's existing `WgpuRenderBackend<TextureTarget>` already provides portable
frame readback through `capture_frame()`. The worker uses this renderer directly,
with a small local RGBA wrapper; it does not implement or forward the entire
`RenderBackend` trait. No Metal, Vulkan or Direct3D handles enter the extractor's
frame interface. The [pinned Ruffle implementation][backend] supplies rendering,
bitmap operations and the texture-to-buffer copy.

## Host policy and selection

The optional `renderer` object belongs to trusted capture/profile/URL job
configuration, never page-derived `NativeContext`, FlashVars or chart data:

```json
{
  "backend": "auto",
  "adapter": "auto",
  "adapterName": null
}
```

- `backend`: `auto`, `metal`, `dx12`, `vulkan` or `gl`.
- `adapter`: `auto`, `hardware` or `software`.
- `adapterName`: optional exact, case-sensitive device name (1–512 UTF-8 bytes,
  no control characters). It is not a substring, path or driver-loading option.
- Providing `renderer` on a profile or URL request explicitly enables offscreen
  rendering for its observations, including profiles that otherwise omit it.
  The saved replay plan retains that choice.
- Unknown keys and enum values reject. A low-level capture's options require
  `render: true`. Omission uses automatic selection when that capture renders.

Automatic selection considers only native backends compiled into the build,
then tries matching adapters, preferring integrated/discrete devices over virtual
or unclassified devices and exhausting non-CPU choices before CPU choices.
Initialization failures and initialization panics are recorded and another
matching candidate may be tried. No movie is executed before selection finishes;
movie execution failures are not retried by this mechanism. At most 32 enumerated
adapters and four backends are considered, under existing supervisor deadlines.
Explicit backend/name/type constraints never relax during fallback.

`software` accepts only devices wgpu identifies as `DeviceType::Cpu`;
`hardware` excludes those devices (it can include virtual/unclassified devices,
so this is not a physical-hardware attestation). These classifications come from
the [wgpu adapter API][device]. No suitable CPU adapter means a clear failure,
not a silent GPU substitution.

For `import-url-native`, put this object at
`request.preparation.source.renderer` in the job JSON; for `profile-native`, use
`request.renderer`. Low-level `capture-native` reads it from the bound capture
configuration beside `render`, not from the launcher command line.

The preparer adds two small, append-only entry points to the pinned Ruffle
backend module: synchronous adapter enumeration and device creation using
Ruffle's own feature/limit negotiation. It does not copy or reimplement that
negotiation. The source preparer, both new worker modules, manifest, lockfile
and hook bytes are covered by build/package verification. Old worker packages
must be rebuilt; their previous captures remain historical evidence.

## Pixels and receipts

The internal frame result contains `width`, `height` and tightly packed,
top-to-bottom RGBA8 `rgba` bytes. It comes from Ruffle's existing readback.
Current exported images remain `frame-<iteration>.ppm`, exact RGB with alpha
omitted, preserving the existing dump layout. No window or browser is needed.

Each rendered worker writes `renderer.json`:

```json
{
  "schema": "ddi-ruffle-renderer/1",
  "requested": {"backend":"auto","adapter":"auto","adapterName":null},
  "selected": {
    "backend":"vulkan",
    "name":"example CPU adapter",
    "deviceType":"Cpu",
    "software":true,
    "vendor":0,
    "device":0,
    "driver":"example",
    "driverInfo":"example"
  },
  "attempts": []
}
```

This is an illustrative schema example, not a successful device receipt.
`attempts` records preceding failed enumeration/initialization steps with bounded
error messages and, when available, adapter information. On initialization
failure `selected` is null and the report remains diagnostic; there is no Ready
capture. Successful snapshots repeat the same object as `snapshot.renderer`.
The supervisor checks that the report matches the snapshot, that the selected
backend/name/type satisfy the bound request, and that CPU/software fields agree.
The report is included in the normal artifact hash inventory. Renderer-free
snapshots have `renderer: null` and need no report file.

Failed initialization diagnostics are recorded before the movie's tracing audit
is installed. They cannot be mistaken for ActionScript execution errors, and no
movie runtime error is suppressed. Rendering choices are observation settings,
like capture frames and audio mixing; they do not change normalized script-input
identity. Every capture still binds its entire original configuration and bytes.

## Software availability and platform scope

This change selects an installed software adapter; it does not install or bundle
one. [SwiftShader][swiftshader] implements Vulkan on the CPU and is a candidate
for a future packaged fallback. Its compatibility, loader deployment, licensing
notices and confinement must be tested on each target. An `auto` request does not
promise software availability. A rendered frame also does not promise identical
antialiasing across APIs: exact chart/event comparisons remain separate from
artwork comparisons.

The renderer selection code contains no macOS-only graphics API. Linux/Windows
end-to-end imports still need their separately planned supervisor confinement
and worker packaging support. This milestone must not be described as tested
Linux/Windows extraction. The worker's network/filesystem/process restrictions
and script resource budgets are unchanged.

## Local validation and remaining work

The 2026-10-15 review is local-only under
`~/Music/DanOni/reviews/rust-rendering-20261015/`. It contains a verified worker
bundle, immutable source-preparation/package receipts, original blank-movie jobs,
their restricted worker outputs and an offline Crow URL-to-library run.
No accepted corpus exports or acquisition caches are promoted or replaced.

The original blank movie passes without a renderer. Automatic rendering finds
an integrated Metal adapter but catches and records the existing compiler-service
initialization failure. The software-only request finds no CPU adapter in this
build/environment; an explicit DX12 request reports an uncompiled backend, and a
missing exact adapter name cannot fall back to another device. All those failures
remain non-Ready. An unknown backend rejects before a worker/output is created.
The supervisor may create an empty stdout `snapshot.json` on a failed launch;
that is not a valid snapshot or an accepted capture.

The new worker's renderer-free Crow import passes, makes zero network requests,
and verifies after library reopen. Its normalized reference differs from the
previous run only in `sourceSnapshotSha256`, which necessarily changes with the
worker build and added `renderer: null` snapshot field. All chart, event, clock,
movement and audio-origin fields match exactly. The resulting SSC, Ogg and banner
files and the imported library entry are byte-identical to that frozen run.
`crow-parity.json` records that comparison explicitly rather than discarding
arbitrary provenance differences.

Checks pass: 273 native SDK tests (including the real installed-worker transport
check), 94 feature-disabled tests, six worker tests, strict Clippy for the SDK
and worker, formatting, and `just check` including REUSE lint.

Successful frame readback on another backend and actual GPU-to-CPU fallback are
not established by these negative probes. Next validate an installed software
adapter and the existing original two-frame colour fixture, then run the affected
bitmap-using works and their frozen reference comparisons. Packaging a software
driver and testing Linux/Windows confinement remain distinct steps. There is no
plan to make chart extraction depend on Metal compiler-service internals.

[backend]: https://github.com/ruffle-rs/ruffle/blob/89a7049965f3090e706265670df309a5128233ac/render/wgpu/src/backend.rs#L177-L217
[device]: https://docs.rs/wgpu/30.0.1/wgpu/enum.DeviceType.html
[swiftshader]: https://github.com/google/swiftshader

[bitmap]: https://github.com/ruffle-rs/ruffle/blob/89a7049965f3090e706265670df309a5128233ac/core/src/avm1/globals/bitmap_data.rs#L576-L600


The [Linux continuation](danoni-rust-linux.md) subsequently proves CPU Vulkan
frame readback and original ActionScript bitmap reads. Its checked CPU-only
fixed-block allocation source edit preserves Ruffle's negotiation and the existing kernel
file-size bound; hardware allocation hints stay unchanged. This is distinct from
real-chart parity or evidence that a real chart consumes the bitmap pixels.
