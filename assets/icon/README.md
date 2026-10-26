# Approved Infinito icon

These are authoring inputs, kept independent of later noteskin changes:

- `arrow.svg`: approved vector masks and rotated arrow. Its framing metrics
  record the band span and outer-border width in note coordinates.
  `icon-arrow` names the drawing, with roles for shadow, inner edge, outer
  border and charge. `currentColor` keeps the geometry shared while profiles
  control each target's appearance. The relative `color.png` reference is
  embedded in generated SVGs.
- `color.png`: the approved frozen 512 px holographic color field. Builds
  consume these pixels; they never execute the WGSL shader.
- `color-dev.png` and `dev.json`: the dev build's separate frozen field,
  phase/time record (0.881 / 1.47 s) and blue `#1b2e55` gap/outline color.
  Target framing, opacity and geometry stay shared with the release art.
- `profiles.json`: target framing, tile, outline color/width/opacity, charge
  color and shadow settings. The charge geometry and frozen phase/time are
  shared authoring settings; target appearances remain independently editable.

`ddi-icon` composes these inputs. Generate only the target you need:

```sh
cargo run -p xtask -- icons --target web
cargo run -p xtask -- icons --target windows
cargo run -p xtask -- icons --target linux
cargo run -p xtask -- icons --target macos
```

The source-keyed cache is `target/icons/<target>/<variant>/`. `--out DIR` copies just
that target's deliverables there. Web generation honors `TRUNK_STAGING_DIR`
when `--out` is omitted, as the Trunk post-build hook needs. Cache metadata
stays out of staged sites and packages. macOS generation uses resvg for each
size and Apple's `iconutil` for the ICNS container; it needs a macOS host.
`--target macos --layers` additionally exports the unmasked SVG layers for
Icon Composer. This does not compile a Liquid Glass `.icon` document.

Cargo debug builds and Trunk debug builds (`just dev`) use the dev icon.
Release and `desktop` profile builds keep the approved release icon.
Packaging reads the compiled variant from `--danoni-bundle-info`, so its
ICNS or Linux icon matches the executable. A plain macOS dev launch also
sets its Dock icon, without needing an app bundle.

Standalone generation defaults to release; add `--variant dev` to preview
the dev artwork. Trunk hooks infer the variant from `TRUNK_PROFILE`.
`DDI_ICON_VARIANT=dev|release` overrides automatic Cargo/Trunk selection,
including optimized local builds. An explicit xtask `--variant` wins over
that environment variable. Both variants use versioned frozen textures;
no shader, browser or authoring tool runs during the build.

Windows builds its executable resources directly through the same library;
Windows/Linux desktop builds embed their generated SVG from `OUT_DIR`.
Ordinary builds do not generate other platforms' icons. The canonical
sources and compositor changes invalidate Cargo's generated window icon;
xtask also keys its cache on the renderer lockfile and generator sources.

To approve a new design, update and review these three canonical inputs
before building. Keep the color texture frozen; target appearance, framing,
background and shadows may differ. Builds need no authoring tools.
No generated SVG variants, PNG icon masters, ICOs or ICNS files belong in Git.
