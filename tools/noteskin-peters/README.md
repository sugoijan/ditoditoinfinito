# Peters-DDR raster extension

The approved SkinCraft revision is bundled in `assets/noteskins/dance/Peters-DDR`
and `Peters-DDR-Vivid`. Select **Peters-DDR** in the game; **Note colors** switches
between Vivid and By subdivision. The old Note/Vivid names remain compatibility aliases.

The square center uses the selected outer-width-2 geometry, matching receptor
regions and inner-to-outer charge. Center and diagonal holds repeat the note's
silhouette, with continuous green/yellow material through heads, bodies and caps.
Rolls use blue-purple gradients, repeating flashing bars and charged heads;
bar cadence follows the active ruleset's scaled roll window in seconds.
[Layered mines](design/mines-layered/README.md) retain the original metallic and
electricity style, with complete isolated effects and connections across adjacent lanes.

The native `.pcraft` masters and intermediate review boards are under `design/`;
its README files describe each historical iteration. Final runtime images are
copied from the approved SkinCraft export, without re-rendering or resampling.
The original arrow taps, cardinal holds, model inputs and source textures remain
unchanged. `PROVENANCE.toml` and `REUSE.toml` distinguish upstream Unlicense art,
new MIT code and the combined derivative raster artwork.

`generate.py` is the **earlier model/SVG study**, superseded by the raster design.
It writes to `target/scratch/peters/model-svg-study` by default and is not a
bundled-skin generator. Use `--output-root DIR` to inspect it elsewhere, or
`--check` to compare an existing study output.
