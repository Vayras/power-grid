"""Per-site ground tiles, converted into the game's pixel style.

Source: rubberduck's isometric ground tiles and grassland tileset (CC0, OpenGameArt),
kept in assets/sprites/rubberduck/. They are smooth painted textures; here each tile is
cut to our 64x31 diamond, its local contrast is boosted, and its brightness is remapped
onto a short per-site colour ramp with ordered dithering, so the texture survives but it
reads as the same crisp pixel art as the OpenGFX tiles beside it.
"""
import os
from PIL import Image, ImageFilter

SRC = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'assets', 'sprites', 'rubberduck')

# Ramps run dark to light.
RAMPS = {
    'moss':     [(30, 36, 34), (42, 52, 46), (56, 68, 56), (72, 86, 66), (92, 106, 80), (118, 128, 100)],
    'lava':     [(18, 18, 21), (28, 28, 32), (40, 39, 44), (54, 52, 58), (70, 68, 74), (92, 90, 96)],
    'snow':     [(150, 162, 182), (178, 190, 206), (202, 212, 226), (222, 230, 240), (240, 244, 250)],
    'lush':     [(26, 50, 18), (36, 66, 22), (48, 82, 28), (62, 98, 34), (80, 116, 42), (100, 134, 54)],
    'laterite': [(84, 34, 22), (110, 46, 28), (136, 60, 34), (162, 78, 44), (186, 100, 60), (206, 126, 82)],
    'savanna':  [(58, 54, 30), (78, 72, 38), (98, 90, 48), (120, 108, 60), (142, 128, 76), (162, 148, 96)],
    'basalt':   [(46, 34, 28), (62, 46, 36), (80, 60, 46), (100, 76, 58), (122, 94, 72), (144, 114, 90)],
    'alpine':   [(20, 40, 28), (28, 54, 36), (38, 70, 44), (50, 88, 56), (66, 106, 68), (88, 126, 84)],
    'granite':  [(64, 66, 70), (84, 86, 90), (104, 106, 110), (126, 128, 132), (150, 152, 156), (176, 178, 182)],
    'prairie':  [(80, 64, 44), (102, 84, 58), (124, 104, 72), (148, 126, 88), (170, 148, 108), (192, 172, 132)],
    'frost':    [(140, 140, 136), (166, 168, 166), (192, 196, 198), (216, 220, 224), (236, 240, 244)],
}

# site -> (main ground source, ramp), (accent source, ramp, share), (rough source, ramp)
# Texas (0) keeps the original OpenGFX grass.
SITES = {
    1: (('stone_path', 'moss'), ('sand', 'snow', 0.16), ('dirt_dark', 'lava')),          # Iceland: moss on lava, snow patches
    2: (('grass_green', 'lush'), ('dirt', 'laterite', 0.12), ('dirt', 'laterite')),      # Paraguay: lush grass on red earth
    3: (('grass_dry', 'savanna'), ('dirt_dark', 'basalt', 0.14), ('dirt_dark', 'basalt')),  # Ethiopia: highland grass, dark soil
    4: (('grass_medium', 'alpine'), ('forest_ground', 'alpine', 0.3), ('stone_path', 'granite')),  # Bhutan: alpine meadow, granite
    5: (('grass_dry', 'prairie'), ('sand', 'frost', 0.16), ('sand', 'frost')),           # North Dakota: winter prairie, frost
}
N_VAR = 4    # plain variants of the main ground per site
N_ACC = 3    # main ground with a patch of the accent material
N_ROUGH = 2  # main ground with a patch of rough (rubble that costs to clear)

BAYER = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]


def _diamond(x, y):
    return abs(x - 31.5) / 32 + abs(y - 15.5) / 16


def _sheet_tiles(name):
    sheet = Image.open(os.path.join(SRC, name + '_64x32.png')).convert('RGBA')
    for r in range(sheet.height // 16 - 1):
        for c in range(sheet.width // 64):
            x = c * 64 + (r % 2) * 32
            if x + 64 <= sheet.width:
                yield sheet.crop((x, r * 16, x + 64, r * 16 + 32))


def _coverage(t, lim=0.85):
    a = t.getchannel('A').load()
    inside = solid = 0
    for y in range(32):
        for x in range(64):
            if _diamond(x, y) < lim:
                inside += 1
                solid += a[x, y] / 255
    return solid / inside


def plain_tiles(name):
    """The fully opaque tiles in a staggered 64x32 sheet (the variations, not the transitions)."""
    return [t for t in _sheet_tiles(name) if _coverage(t) > 0.995]


def patch_shapes(name):
    """Soft blobs from the transition tiles, as 0..1 masks pulled in towards the tile centre."""
    out = []
    for t in _sheet_tiles(name):
        c = _coverage(t)
        if 0.3 < c < 0.75:
            a = t.getchannel('A').load()
            # a round falloff in iso space keeps blobs off the tile's edges without making them diamond-shaped
            out.append([[a[x, y] / 255 * max(0.0, 1 - (((x - 31.5) / 27) ** 2 + ((y - 15.5) / 13.5) ** 2) ** 1.5) for x in range(64)] for y in range(31)])
    return out


def seamless(tile):
    """Lay the tile on its isometric lattice and cut one tile back out, so its edges blend into its neighbours."""
    big = Image.new('RGBA', (256, 128), (0, 0, 0, 0))
    for j in range(-3, 4):
        for i in range(-3, 4):
            big.alpha_composite(tile, (96 + (i - j) * 32, 48 + (i + j) * 16))
    return big.crop((96, 48, 160, 79)).convert('RGB')


def lum_detail(tile, contrast):
    """Brightness with its local contrast boosted, as a 64x31 grid."""
    t = seamless(tile)
    blur = t.filter(ImageFilter.GaussianBlur(3))
    p, b = t.load(), blur.load()
    L = lambda c: 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
    return [[L(b[x, y]) + (L(p[x, y]) - L(b[x, y])) * contrast for x in range(64)] for y in range(31)]


def stats(grids):
    vals = sorted(v for g in grids for row in g for v in row)
    return vals[len(vals) // 50], vals[-len(vals) // 50 - 1]


def paint(grid, ramp, lohi, seed=0):
    lo, hi = lohi
    n = len(ramp)
    out = [[None] * 64 for _ in range(31)]
    for y in range(31):
        for x in range(64):
            f = min(0.999, max(0.0, (grid[y][x] - lo) / max(1, hi - lo))) * (n - 1)
            k = int(f)
            if f - k > (BAYER[(y + seed) % 4][(x + seed) % 4] + 0.5) / 16:
                k += 1
            out[y][x] = ramp[min(n - 1, k)]
    return out


def to_image(px, mask):
    m = mask.load()
    img = Image.new('RGBA', (64, 31), (0, 0, 0, 0))
    o = img.load()
    for y in range(31):
        for x in range(64):
            if m[x, y]:
                o[x, y] = px[y][x] + (m[x, y],)
    return img


def blend(base, top, shape, seed=0):
    """Top over base where the blob says so, with a dithered edge."""
    out = [row[:] for row in base]
    for y in range(31):
        for x in range(64):
            if shape[y][x] > (BAYER[(y + seed) % 4][(x + seed) % 4] + 0.5) / 16 * 0.8 + 0.1:
                out[y][x] = top[y][x]
    return out


def material(name, ramp, contrast=1.9):
    tiles = plain_tiles(name)
    grids = [lum_detail(t, contrast) for t in tiles]
    lohi = stats(grids[:8])
    return [paint(g, RAMPS[ramp], lohi, k) for k, g in enumerate(grids)]


def site_tiles(mask):
    """{site: [N_VAR main, N_ACC accent patches, N_ROUGH rough patches]} as 64x31 RGBA tiles."""
    res = {}
    for site, (main, accent, rough) in SITES.items():
        mt = material(*main)
        at = material(accent[0], accent[1])
        rt = material(rough[0], rough[1], 2.3)
        shapes = patch_shapes(accent[0]) or patch_shapes(main[0])
        base = [mt[(k * 5 + site) % len(mt)] for k in range(N_VAR)]
        tiles = [to_image(b, mask) for b in base]
        for k in range(N_ACC):
            tiles.append(to_image(blend(base[k % N_VAR], at[(site + k * 3) % len(at)], shapes[(site * 5 + k * 7) % len(shapes)], k), mask))
        for k in range(N_ROUGH):
            tiles.append(to_image(blend(base[(k + 2) % N_VAR], rt[(site * 3 + k * 4) % len(rt)], shapes[(site * 5 + 11 + k * 13) % len(shapes)], k + 1), mask))
        res[site] = tiles
    return res


ACCENT_SHARE = {s: v[1][2] for s, v in SITES.items()}
