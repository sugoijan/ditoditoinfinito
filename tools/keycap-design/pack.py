"""OEM comparison sheet and provisional home-row game atlas (Pillow)."""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "target/scratch/keycaps/oem"
FONT = ROOT / "assets/fonts/IosevkaTerm-SemiBold-latin.ttf"
REGULAR = ROOT / "assets/fonts/IosevkaTerm-Regular-latin.ttf"


def text(canvas, xy, value, size, color="#f2eee9", bold=False):
    ImageDraw.Draw(canvas).text(xy, value, font=ImageFont.truetype(str(FONT if bold else REGULAR), size), fill=color, anchor="lt")


def cap(row, wi, label, scale=1):
    label = label.upper() if len(label) == 1 else label.lower()
    space = label == "space"
    image = Image.open(OUT / ("space.png" if space else f"r{row+1}-{wi}.png")).convert("RGBA")
    ink = Image.new("RGBA", image.size)
    inset = (image.width-len(label)*44)/2 if space else 60
    text(ink, (inset, 78 if space else 48), label, 88, bold=True)
    d = ImageDraw.Draw(ink)
    sx = inset + len(label)*44 + 24
    paths = []
    if label == "enter":
        paths = [[(56,-24),(56,8),(0,8)],[(20,-12),(0,8),(20,28)]]
    elif label == "tab":
        paths = [[(0,0),(48,0)],[(28,-20),(48,0),(28,20)],[(64,-28),(64,28)]]
    for path in paths:
        d.line([(sx+x,92+y) for x,y in path], fill="#f2eee9",width=6,joint="curve")
    if not space:
        center = image.width/2-inset
        face_half = (6.8+wi*4.75)*256/23
        quadratic = -0.568*8/face_half**2
        # Inverse mapping, narrow vertical strips of the original live run.
        strips = []
        for x in range(image.width):
            offset = quadratic*(x-inset)*(x-inset-2*center)
            strips.append(((x,0,x+1,image.height),(x,-offset,x,image.height-offset,
                           x+1,image.height-offset,x+1,-offset)))
        ink = ink.transform(image.size, Image.Transform.MESH,strips,Image.Resampling.BICUBIC)
    image.alpha_composite(ink)
    if scale != 1:
        image = image.resize((round(image.width*scale), round(image.height*scale)), Image.Resampling.LANCZOS)
    return image


def main():
    sheet = Image.new("RGBA", (1800, 1020), "#0a090d")
    text(sheet, (56, 36), "OEM / four row shapes", 36, bold=True)
    text(sheet, (56, 90), "Original approximations · same camera and lighting · theme colors", 21, "#a39c95")
    for row, (name, legend) in enumerate([("Function / numbers", "1"), ("QWERTY", "Q"), ("Home", "A"), ("Lower / arrows", "Z")]):
        x = 56 + row*390
        text(sheet, (x, 150), f"R{row+1}  {name}", 23, "#ff6ea0")
        sheet.alpha_composite(cap(row, 0, legend), (x+25, 192))
        side = Image.open(OUT/f"r{row+1}-side.png").convert("RGBA").resize((215,215),Image.Resampling.LANCZOS)
        sheet.alpha_composite(side, (x+45, 460))
        text(sheet,(x+50,675),"← front · side view",18,"#a39c95")
    ImageDraw.Draw(sheet).line((56,732,1544,732),fill="#26222e",width=2)
    text(sheet, (56, 766), "Home row / curved legends and convex Space", 25, bold=True)
    x = 56
    for wi, label, action in [(0,"E","Edit"),(1,"Tab","Style"),(2,"Enter","Play"),(3,"Space","Listen")]:
        image = cap(2,wi,label,.42)
        sheet.alpha_composite(image,(x,820))
        x += image.width + 16
        text(sheet,(x,858),action,26,"#a39c95")
        x += len(action)*14+56
    sheet.convert("RGB").save(OUT/"row-study.png")
    atlas = Image.new("RGBA",(768,1280))
    for wi in range(4):
        image=Image.open(OUT/f"r3-{wi}.png")
        atlas.paste(image,(0,wi*256))
    atlas.paste(Image.open(OUT/"space.png"), (0,1024))
    atlas.save(ROOT/"assets/ui/keycaps.png",optimize=True)


if __name__ == "__main__":
    main()
