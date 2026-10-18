# Installed runtime capability fixture

`runtime-bitmap.swf` is the original synthetic SWF8 from the 2026-10-15
Linux probe, not a third-party work. SHA-256:
`5ac2a0b1c0b5cd9a460c88dd5376ad4c7a3aa055ff8ecc1071f2bd10c9f0cabd`.
It draws the animated synthetic rectangle into ActionScript BitmapData, reads
its pixel back into root `bitmapColor`, and draws the same rectangle onscreen.
The independent expected colours at frames 1/2 are RGB 200,80,40 / 40,160,80;
background RGB 17,34,51. Stage size is 320×240. Runtime diagnostics execute it
only through the production supervisor, native scale and unchanged limits.
