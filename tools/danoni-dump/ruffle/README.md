# Confined native Ruffle worker

This directory contains the pinned Ruffle worker's Rust sources, hooks, manifest
template and lockfile. The [Rust SDK](../rust/README.md) owns source preparation,
confinement, capture, normalization and export. The earlier Python SWF pipeline
has been retired; see the [consolidation review](../../../docs/research/danoni-consolidation.md).

## Build and run

Use [build-native-runtime.sh](../build-native-runtime.sh) to build and install a
runtime, following the [runtime guide](../../../docs/research/danoni-rust-runtime.md).
It calls the Rust `prepare-worker` / `package-worker` APIs. The pinned upstream
release is Ruffle v0.7.1, commit `89a7049965f3090e706265670df309a5128233ac`.

For individual research stages, the SDK CLI provides `capture-native`,
`project-native`, `demux-native` and `normalize-native`, with job contracts in the
[supervisor](../../../docs/research/danoni-rust-supervisor.md),
[extraction](../../../docs/research/danoni-rust-extraction.md) and
[normalization](../../../docs/research/danoni-rust-normalization.md) documents.
Run movies only through the supervisor. It enforces the reviewed platform sandbox
and resource limits; unavailable confinement is an error, with no unrestricted
fallback. Inputs and outputs remain outside Git checkouts.

`legacy-build-identity.txt` retains the old builder's bytes verbatim. Existing
worker identities bind its digest under the key `prepare.py`; it is inert
compatibility data, not a build script. The SDK never executes it. Keep it until
the worker identity contract is deliberately versioned and old bundles migrated.

## Remaining HTML5 research helpers

These have no equivalent in the native SWF importer and are not runtime SDK
dependencies:

- `normalize_cw.py`: verified saved CW browser references to normalized works,
  retaining source clock/movement evidence. Needs Node for `../cw-metadata.mjs`
  and FFmpeg for ordinary file decoding.
- `compare_ports.py`: exact raw note/hold frame comparison between a saved native
  reference and its CW port. Needs Python's standard library only.
- `cw-roundtrip.mjs`: independent danoniplus converter check for exported dos.
- `verify_audio.mjs`: independent installed-Chrome decoder comparison.

`export_io.py` and `paths.py` provide filesystem helpers for the remaining Python
commands. No NumPy/SciPy analysis environment or custom C encoder is required.
Run their synthetic checks from this directory:

```sh
python3 -B -m unittest test_cw_normalization test_compare_ports
```

The browser tools use the parent directory's `npm ci` dependencies. Their original
research contracts and historical results remain in `docs/research/danoni-*.md`.
New native tests live in `../rust/tests/` and `../desktop-probe/tests/`; local corpus
gates read preserved references without invoking the retired Python pipeline.
