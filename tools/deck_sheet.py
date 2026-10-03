#!/usr/bin/env python3
"""Contact sheets from the PNGs `deck_review.sh` rasterises.

  deck_sheet.py slides  <svgdir> <theme> <out.png> [per_sheet=6]   -> out-1.png, out-2.png ...
  deck_sheet.py compare <outdir> <slide_no> <out.png>              -> one tile per theme, same slide
"""
import glob, os, sys
from PIL import Image

# Quick Look rasterises each SVG into a 1600x1600 canvas with the 16:9 slide
# centred (see scene_svg.rs); the slide is the middle 1600x900 band.
BAND = (0, 350, 1600, 1250)


def tile(path, size):
    return Image.open(path).convert("RGB").crop(BAND).resize(size)


def slides(svgdir, theme, out, per=6):
    files = sorted(glob.glob(f"{svgdir}/{theme}-*.svg.png"))
    tw, th = 800, 450
    for page in range(0, len(files), per):
        chunk = files[page:page + per]
        rows = (len(chunk) + 1) // 2
        sheet = Image.new("RGB", (2 * tw + 30, rows * th + (rows + 1) * 10), (40, 40, 40))
        for i, f in enumerate(chunk):
            sheet.paste(tile(f, (tw, th)), (10 + (i % 2) * (tw + 10), 10 + (i // 2) * (th + 10)))
        name = out.replace(".png", f"-{page // per + 1}.png")
        sheet.save(name)
        print(name)


def compare(outdir, slide_no, out):
    ids = sorted(os.path.basename(d)[:-4] for d in glob.glob(f"{outdir}/*-svg"))
    tw, th, cols = 640, 360, 3
    rows = (len(ids) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * tw + (cols + 1) * 8, rows * th + (rows + 1) * 8), (40, 40, 40))
    for i, tid in enumerate(ids):
        f = f"{outdir}/{tid}-svg/{tid}-{slide_no:02d}.svg.png"
        if os.path.exists(f):
            sheet.paste(tile(f, (tw, th)), (8 + (i % cols) * (tw + 8), 8 + (i // cols) * (th + 8)))
    sheet.save(out)
    print(out, ids)


if __name__ == "__main__":
    a = sys.argv[1:]
    if a and a[0] == "slides":
        slides(a[1], a[2], a[3], int(a[4]) if len(a) > 4 else 6)
    elif a and a[0] == "compare":
        compare(a[1], int(a[2]), a[3])
    else:
        sys.exit(__doc__)
