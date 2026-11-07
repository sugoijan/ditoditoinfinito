## The web page's loader (site/boot.js, site/index.html): what the page
## says before the game runs, when the browser cannot run it and when it
## crashes. Plain page text in the browser's own fonts, put into the page
## for each language by `xtask minify-boot`: no variables (nothing is
## there to pass them), terms are fine. Every language's lines travel in
## the page's first bytes, so keep them few and short.

# The loader's picture (the title, its rain and the progress bar), as
# screen readers say it.
boot-loading = { -game-name }, loading

# The heading of the box shown when the browser cannot run the game; a
# reason below follows it, then a hint.
boot-blocked = This browser cannot run { -game-name }.

# A reason under that heading: the browser lacks what the game is
# built on (WebAssembly).
boot-no-wasm = Your browser doesn't support WebAssembly.

# A reason: the browser lacks WebAssembly's vector instructions (SIMD).
boot-no-wasm-simd = Your browser doesn't support WebAssembly SIMD.

# A reason: the browser lacks fetch or promises.
boot-no-fetch = Required web APIs are missing.

# A reason: the browser lacks TextDecoder (an API's name).
boot-no-textdecoder = TextDecoder is required for the game.

# A reason: the browser lacks requestAnimationFrame (an API's name).
boot-no-raf = requestAnimationFrame is required for rendering.

# A reason: the browser lacks Pointer Events (an API's name).
boot-no-pointer = Pointer Events are required for input.

# A reason: the browser lacks Web Audio (an API's name).
boot-no-audio = Web Audio is required — this game is played by sound.

# The hint after a reason.
boot-update = Try an up-to-date browser.

# The hint after the SIMD reason: Safari is the browser that came to it last.
boot-update-simd = Try an up-to-date browser (Safari 16.4 or later).

# The game crashed and the page reloads itself; the error's own words
# (untranslated) follow on a line of their own.
boot-reloading = Something went wrong — reloading…

# The game crashed again soon after reloading: the page stops trying.
boot-reload = Something went wrong. Reload the page to continue.

# In place of the error's own words when it gave none.
boot-crashed = The game hit an unexpected error.

# The game's download or start failed; the browser's own words follow
# (untranslated).
boot-start-failed = Something went wrong while starting the game.

# In place of the browser's own words when it gave none.
boot-init-failed = Initialization failed.
