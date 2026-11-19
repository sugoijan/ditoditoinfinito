# ddi-rating

Estimate the whole-chart difficulty of an original dance-single chart under DDI Normal Practice at 1×. The shared Rust entry point is `ddi_rating::estimate(&song, chart_index)`. It embeds the model and works on native and WebAssembly targets. No simulations or external model files are required.

Print an estimate as JSON using a zero-based chart index:

```sh
cargo run --release -p ddi-rating --example estimate -- assets/songs/beatucada/Beatucada.ssc 5
```

- `decimal` and `level` give the nonlinear 1–19 estimate and its rounded display value. The mapping uses balanced Accuracy at ten CPU levels, normalized hold/roll losses against a fixed easy reference, and heads per second.
- `means` has one row per CPU level, from 1 to 10. Each row contains balanced raw Accuracy followed by eight estimated changes in percentage points from reducing a single skill to 0.75. The axes follow `ddi_engine::cpu::SKILL_NAMES`.
- `expected` gives calibrated crossings for 90%, 95% and 98% expected Accuracy. Its `profile` gives the radar at a uniquely resolved 95% level. `conservative` repeats the readout with wider bounds that account for the simulation teacher's batch uncertainty. Keep missing profiles, signed costs, per-spoke intervals and statuses visible to callers. Handstream bounds are unavailable for charts containing a three- or four-key head.
- `within_scalar_domain` qualifies the 1–19 estimate for 48+ playable heads, at most 1,200 source notes, 15–180 seconds and zero lifts. The extension was checked against teacher-v1 on 113 fresh families with teacher ratings spanning 14.53–19.00.
- `within_matched_profile_domain` qualifies the eight song-select spokes at the chart’s decimal-matched CPU level. The limit is 900 source notes with playable mines and 1,200 without them, under the same head-count, duration and lift conditions. On 58/60 fresh mine-free families all eight displayed spokes were within 1.5 levels of teacher-v1. Those teacher ratings spanned 16.13–19.00.
- `within_studied_domain` qualifies the score curve and skill readouts under the same conditions through 900 source notes. Both calibrated readouts are absent outside that range. The extractor accepts at most 8,192 source notes. The scale estimates keyboard-simulated performance on DDR-style singles.

The primary `single-pass-rating-v1` weights are seed 20261102, fit against teacher-v1 revision `49a79082`. The embedded file contains the normalization and affine parameters as 13,826 little-endian float32 values from the frozen model (source JSON SHA-256 `ee1178a81ea2df953a5a3a91cb9daa6915298e55a9acfe78e53d37693b6ffacf`). The `ddi-ddr19-prototype-v1` scale uses the frozen primary knots and pooled reference (source JSON SHA-256 `17cc130894670dd5ac0ac92904e402d96cf377060f3c9cc3decc6be1e95095de`).
