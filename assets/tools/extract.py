"""Cut sprites out of OpenGFX sheets: each sprite sits on a pure-blue (0,0,255) box on a
white page. Usage: python3 -I extract.py <sheet.png> <outdir> [minsize]"""
import sys, os
from PIL import Image
from collections import deque

sheet, outdir = sys.argv[1], sys.argv[2]
minsize = int(sys.argv[3]) if len(sys.argv) > 3 else 12
os.makedirs(outdir, exist_ok=True)
im = Image.open(sheet).convert('RGBA')
W, H = im.size
px = im.load()

def is_page(p):
    r, g, b, a = p
    return a == 0 or (r > 235 and g > 235 and b > 235)

seen = bytearray(W * H)
boxes = []
for y in range(H):
    for x in range(W):
        if seen[y * W + x] or is_page(px[x, y]):
            continue
        q = deque([(x, y)]); seen[y * W + x] = 1
        x0 = x1 = x; y0 = y1 = y
        while q:
            cx, cy = q.popleft()
            x0 = min(x0, cx); x1 = max(x1, cx); y0 = min(y0, cy); y1 = max(y1, cy)
            for nx, ny in ((cx + 1, cy), (cx - 1, cy), (cx, cy + 1), (cx, cy - 1)):
                if 0 <= nx < W and 0 <= ny < H and not seen[ny * W + nx] and not is_page(px[nx, ny]):
                    seen[ny * W + nx] = 1
                    q.append((nx, ny))
        if x1 - x0 + 1 >= minsize and y1 - y0 + 1 >= minsize:
            boxes.append((x0, y0, x1 + 1, y1 + 1))

boxes.sort(key=lambda b: (b[1] // 20, b[0]))
for k, b in enumerate(boxes):
    sp = im.crop(b)
    d = sp.load()
    for yy in range(sp.height):
        for xx in range(sp.width):
            r, g, bb, a = d[xx, yy]
            if r < 20 and g < 20 and bb > 235:
                d[xx, yy] = (0, 0, 0, 0)
    bbox = sp.getbbox()
    if not bbox:
        continue
    sp = sp.crop(bbox)
    sp.save(os.path.join(outdir, f'{k:03d}_{b[0]}_{b[1]}.png'))
print(len(boxes), 'sprites')
