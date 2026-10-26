"""Pack Blender's four blank caps into the theme atlas (requires Pillow)."""
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
atlas = Image.new("RGBA", (320, 512))
for row in range(4):
    cap = Image.open(ROOT / f"target/scratch/keycaps/cap-{row}.png")
    atlas.paste(cap, (0, row * 128))
atlas.save(ROOT / "assets/ui/keycaps.png", optimize=True)
