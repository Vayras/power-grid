"""Build the Power Grid sprite atlas.
Composites buildings from OpenGFX cut sprites + hand-drawn tech sprites (tools/ttdmap.py),
packs them into web/atlas.png and writes src/sprites.rs (rects, anchors, smoke emitters).
Anchor convention: a sprite is drawn at P(origin tile) + (ax, ay), where P(i,j) is the
screen position of the top corner of tile (i,j)."""
import sys, os, math
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from PIL import Image, ImageDraw
import ttdmap as T

ROOT = os.path.dirname(HERE)
OX, OY = 160, 120          # P(0,0) on the scratch canvas


def scratch():
    m = T.TTDMap(1, origin=(OX, OY), w=360, h=260)
    m.img = Image.new('RGBA', (360, 260), (0, 0, 0, 0))
    m.ground = {}
    return m


def layer(m):
    """Render objects/post of a scratch map onto its transparent image (no grain)."""
    img = m.img
    for (i, j), t in sorted(m.ground.items(), key=lambda k: (k[0][0] + k[0][1], k[0][0])):
        x, y = m.P(i, j)
        img.alpha_composite(t, (x - t.width // 2, y + 31 - t.height))
    d = ImageDraw.Draw(img)
    for _, kind, a in sorted(m.objs, key=lambda o: o[0]):
        if kind == 'img':
            im, x, y = a
            img.alpha_composite(im, (x, y))
        elif kind == 'cont':
            m._cont(d, *a)
        elif kind == 'xbox':
            m._xbox(d, *a)
        elif kind in ('wind', 'tokamak', 'core'):
            {'wind': T._windraw, 'tokamak': T._tokdraw, 'core': T._coredraw}[kind](m, d, *a)
    emit = []
    for p in m.post:
        if p[0] == 'smoke':
            emit.append((p[1] - OX, p[2] - OY))
        elif p[0] == 'wire':
            m._wire(img, d, *p[1:])
        elif p[0] == 'glow_xy':
            pass
    return img, emit


def pack_sprite(img):
    bb = img.getbbox()
    if not bb:
        return None
    return img.crop(bb), bb[0] - OX, bb[1] - OY


SPRITES = []   # (name, image, ax, ay, emitters)

def add(name, m):
    img, emit = layer(m)
    sp, ax, ay = pack_sprite(img)
    # lightly grain the sprite like the maps
    import random
    rng = random.Random(hash(name) & 0xffff)
    px = sp.load()
    for y in range(sp.height):
        for x in range(sp.width):
            r, g, b, a = px[x, y]
            if a:
                n = rng.randint(-6, 6)
                px[x, y] = (max(0, min(255, r + n)), max(0, min(255, g + n)), max(0, min(255, b + n)), a)
    SPRITES.append((name, sp, ax, ay, emit))


def add_img(name, im, ax, ay):
    SPRITES.append((name, im, ax, ay, []))


# ---------------------------------------------------------------- ground tiles
def ground(name, im):
    add_img(name, im, -32, 31 - im.height)

ground('G_GRASS', T.GRASS[0])
ground('G_ROUGH', T.ROUGH)
ground('G_WATER', T.WATER)
ground('G_CONCRETE', T.IM(123))
ground('G_COAL', T.IM(130))

# per-site ground (sites 1..5; Texas keeps the OpenGFX grass): CC0 textures converted in site_ground.py
import site_ground as SG
for _site, _tiles in sorted(SG.site_tiles(T.GRASS[0].getchannel('A')).items()):
    for _k, _im in enumerate(_tiles):
        ground(f'G_S{_site}_{_k}', _im)

# ---------------------------------------------------------------- trees (point anchored at tile centre)
for k, tr in enumerate(T.TREES):
    add_img(f'TREE_{k}', tr, -tr.width // 2, 16 - tr.height)

# ---------------------------------------------------------------- buildings
def pads(m, w, d, kind='concrete'):
    for a in range(w):
        for b in range(d):
            m.ground[(a, b)] = T.IM(123) if kind == 'concrete' else T.IM(130 + (a + b) % 2)

# power sources
m = scratch(); pads(m, 1, 1); m.transformer(0, 0)
xs = []
for (i, j) in [(0.15, 0.9), (0.9, 0.15)]:
    x, y = m.P(i, j); xs.append((x, y, 30))
m.pylon_line(xs); add('B_GRID', m)

m = scratch(); pads(m, 1, 1); m.diesel(0, 0); add('B_DIESEL', m)

m = scratch(); pads(m, 2, 2)
m.sprite(T.IM(150), 0, 0, 1, 1); m.sprite(T.IM(149), 1, 0, 1, 1); m.sprite(T.IM(162), 0, 1, 1, 1); m.sprite(T.IM(156), 1, 1, 1, 1)
add('B_GAS', m)

m = scratch(); pads(m, 3, 2, 'coal')
m.sprite(T.IM(137), 0, 0, 1, 1); m.sprite(T.IM(134), 1, 0, 1, 1); m.sprite(T.IM(134), 2, 0, 1, 1); m.sprite(T.IM(140), 0, 1, 1, 1)
m.heap = None
x, y = m.P(0.55, 0.2); m.smoke(x + 2, y - 52, 1)
for k in (1, 2):
    x, y = m.P(k + 0.5, 0.5); m.smoke(x, y - 40, 1)
add('B_COAL', m)

m = scratch(); pads(m, 2, 2); m.solar(0, 0, 2, 1); m.solar(0, 1, 2, 1); add('B_SOLAR', m)

import math as _m
for f in range(3):
    m = scratch(); m.wind(0, 0, phase=f * (2 * _m.pi / 9)); add(f'B_WIND{f}', m)

m = scratch(); pads(m, 2, 2)
m.box(0, 0, 2, 0.9, 1, ((46, 92, 160), (40, 80, 140), (34, 70, 120)))
m.box(0, 0.9, 2, 0.5, 15, ((196, 190, 176), (164, 158, 144), (126, 120, 108)))
m.transformer(1, 1.4)
add('B_HYDRO', m)

m = scratch(); pads(m, 2, 2); m.reactor(0, 0.3); m.sprite(T.IM(134), 1, 1, 1, 1)
x, y = m.P(1.5, 1.5); m.smoke(x, y - 40, 1)
add('B_FISSION', m)

m = scratch(); pads(m, 2, 2); m.tokamak(0, 0); add('B_FUSION', m)
m = scratch(); pads(m, 2, 2); m.core(0.5, 0.5); add('B_CORE', m)

# miners (container colour per generation)
GENS = [((104, 108, 116), (88, 92, 98), (52, 56, 60)),
        ((150, 128, 96), (126, 106, 78), (92, 76, 54)),
        ((70, 86, 104), (56, 70, 86), (36, 46, 58)),
        ((44, 44, 50), (34, 34, 40), (22, 22, 26))]
for g, (top, mid, dk) in enumerate(GENS):
    m = scratch(); pads(m, 1, 1)
    T.TOP, T.MID, T.DK = top, mid, dk
    m.container(0.06, 0.2, w=0.88, d=0.6, h=13)
    add(f'B_MINER{g + 1}', m)
T.TOP, T.MID, T.DK = GENS[0]

m = scratch(); pads(m, 1, 1); m.fanwall(0, 0.3, 1, 0.4); add('B_FAN', m)
m = scratch(); pads(m, 1, 1); m.fanwall_i(0.3, 0, 0.4, 1); add('B_FAN2', m)
m = scratch(); m.pole(0, 0); add('B_POLE', m)
m = scratch()
STONE = ((186, 182, 172), (150, 146, 138), (112, 108, 102)); SPAN = ((172, 168, 158), (138, 134, 126), (104, 100, 94))
m.box(0.3, 0.3, 0.4, 0.4, 17, STONE); m.box(0.7, 0.33, 0.6, 0.34, 13, SPAN); m.box(1.3, 0.3, 0.4, 0.4, 17, STONE)
add('B_WALL', m)
m = scratch(); m.sign(0, 0); add('B_SIGN', m)
m = scratch(); pads(m, 1, 1); m.immersion(0, 0); add('B_IMMERSION', m)
m = scratch(); pads(m, 1, 1); m.sprite(T.IM(124), 0, 0, 1, 1); add('B_SHED', m)
m = scratch(); m.greenhouse(0, 0); add('B_GREENHOUSE', m)
m = scratch(); pads(m, 2, 2); m.deepgeo(0, 0); add('B_DEEPGEO', m)
m = scratch(); pads(m, 2, 2); m.orbital(0, 0); add('B_ORBITAL', m)

# workers (2 frames)
for f in range(2):
    im = Image.new('RGBA', (6, 11), (0, 0, 0, 0)); d = ImageDraw.Draw(im)
    d.rectangle([2, 0, 4, 0], fill=T.YE); d.rectangle([2, 1, 4, 2], fill=(230, 186, 150))
    d.rectangle([2, 3, 4, 7], fill=(56, 88, 150))
    if f == 0:
        d.point((2, 8), fill=(40, 40, 40)); d.point((4, 9), fill=(40, 40, 40))
    else:
        d.point((4, 8), fill=(40, 40, 40)); d.point((2, 9), fill=(40, 40, 40))
    add_img(f'WORKER{f}', im, -3, -10)

# ---------------------------------------------------------------- pack (shelf packer)
W = 1024
SPR = sorted(SPRITES, key=lambda s: -s[1].height)
x = y = rowh = 0
rects = {}
for name, im, ax, ay, em in SPR:
    if x + im.width + 1 > W:
        x = 0; y += rowh + 1; rowh = 0
    rects[name] = (x, y, im.width, im.height, ax, ay, em)
    x += im.width + 1; rowh = max(rowh, im.height)
H = y + rowh + 1
atlas = Image.new('RGBA', (W, H), (0, 0, 0, 0))
for name, im, ax, ay, em in SPRITES:
    r = rects[name]
    atlas.alpha_composite(im, (r[0], r[1]))
os.makedirs(f'{ROOT}/web', exist_ok=True)
atlas.save(f'{ROOT}/web/atlas.png', optimize=True)

names = [s[0] for s in SPRITES]
lines = ['// Generated by tools/build_atlas.py. Do not edit.', f'pub const ATLAS_W: usize = {W};', f'pub const ATLAS_H: usize = {H};',
         '#[derive(Clone, Copy)]', 'pub struct Spr { pub x: u16, pub y: u16, pub w: u16, pub h: u16, pub ax: i16, pub ay: i16, pub e0: u8, pub ne: u8 }']
emit_all = []
spr_lines = []
for k, n in enumerate(names):
    r = rects[n]
    e0 = len(emit_all); emit_all += r[6]
    spr_lines.append(f'    Spr {{ x: {r[0]}, y: {r[1]}, w: {r[2]}, h: {r[3]}, ax: {r[4]}, ay: {r[5]}, e0: {e0}, ne: {len(r[6])} }}, // {k} {n}')
    lines.append(f'pub const {n}: usize = {k};')
lines.append(f'pub const N_TREES: usize = {len(T.TREES)};')
lines.append(f'pub const G_SITE_VAR: usize = {SG.N_VAR}; pub const G_SITE_ACC: usize = {SG.N_ACC}; pub const G_SITE_ROUGH: usize = {SG.N_ROUGH};')
lines.append(f'pub const G_SITE_N: usize = {SG.N_VAR + SG.N_ACC + SG.N_ROUGH};')
lines.append('pub const G_SITE_ACC_SHARE: [f32; 6] = [0.0, ' + ', '.join(f'{SG.ACCENT_SHARE[s]:.2}' for s in range(1, 6)) + '];')
lines.append('pub static SPRITES: [Spr; %d] = [' % len(names))
lines += spr_lines
lines.append('];')
lines.append('pub static EMIT: [(i16, i16); %d] = [%s];' % (len(emit_all), ', '.join(f'({int(a)}, {int(b)})' for a, b in emit_all)))
open(f'{ROOT}/src/sprites.rs', 'w').write('\n'.join(lines) + '\n')
print('atlas', W, H, 'sprites', len(names), 'emitters', len(emit_all))

import json
json.dump({n: [rects[n][0], rects[n][1], rects[n][2], rects[n][3]] for n in names}, open(f'{ROOT}/web/sprites.json', 'w'))
