# Converts captures made on Pure Black into 320x180 RGBA thumbnails whose alpha
# is the pixel's peak channel, so the settings UI can lay them over any
# background gradient (result = capture + background * (1 - alpha)).
import sys
from pathlib import Path
from PIL import Image, ImageChops
src, dst = Path(sys.argv[1]), Path(sys.argv[2])
dst.mkdir(parents=True, exist_ok=True)
for path in sorted(src.glob("*.png")):
    im = Image.open(path).convert("RGB").resize((320, 180), Image.LANCZOS)
    r, g, b = im.split()
    alpha = ImageChops.lighter(ImageChops.lighter(r, g), b)
    out = Image.new("RGBA", im.size)
    px, ap, op = im.load(), alpha.load(), out.load()
    for y in range(im.height):
        for x in range(im.width):
            a = ap[x, y]
            if a:
                R, G, B = px[x, y]
                op[x, y] = (min(255, R * 255 // a), min(255, G * 255 // a), min(255, B * 255 // a), a)
    out.save(dst / path.name, optimize=True)
    print(path.name)
