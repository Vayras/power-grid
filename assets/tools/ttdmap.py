"""Seeded old-school 2.5D map renderer using OpenGFX (GPL-2.0) sprites.
Native 640x360 canvas, 64x32 tiles; displayed at 2x with pixelated scaling."""
import random, glob, os
from PIL import Image, ImageDraw

CUT = '/home/claude/assets/cut'
HW, HH = 32, 16

def sp(path):
    return Image.open(f'{CUT}/{path}').convert('RGBA')

def first(dirn, idx):
    fs = sorted(glob.glob(f'{CUT}/{dirn}/*.png'))
    return Image.open(fs[idx]).convert('RGBA')

def by_label(dirn, label):
    return Image.open(glob.glob(f'{CUT}/{dirn}/{label:03d}_*.png')[0]).convert('RGBA')

GRASS = [first('grass-temperate.gimp', 0)]
ROUGH = first('rough-temperate', 0)
WATER = first('shoretiles', 0)
IM = lambda n: by_label('industries_misc', n)
def _leafy():
    out = []
    for d in sorted(glob.glob(f'{CUT}/trees/tree_wide_*')):
        best, bg = None, 0
        for f in sorted(glob.glob(d + '/*.png')):
            im = Image.open(f).convert('RGBA')
            px = [p for p in im.getdata() if p[3] > 0]
            if len(px) < 120: continue
            g = sum(1 for r, gg, b, a in px if gg > r + 6 and gg >= b) / len(px)
            if g > bg: best, bg = im, g
        if best is not None and bg > 0.3: out.append(best)
    return out
TREES = _leafy()

# palette for hand-drawn bits (TTD-ish)
DK = (52, 56, 60); MID = (88, 92, 98); LT = (128, 132, 138); TOP = (104, 108, 116)
CY = (84, 232, 228); CO = (255, 96, 72); YE = (252, 212, 56); SMOKE = [(214, 214, 206), (188, 188, 180), (160, 160, 154)]


class TTDMap:
    def __init__(self, seed, origin=(320, -40), w=640, h=360):
        self.rng = random.Random(seed)
        self.W, self.H = w, h
        self.ox, self.oy = origin
        self.img = Image.new('RGBA', (w, h), (64, 92, 40, 255))
        self.ground = {}
        self.objs = []
        self.post = []
        self.clear = set()

    def P(self, i, j, z=0):
        return (int((i - j) * HW + self.ox), int((i + j) * HH + self.oy - z))

    def terrain(self, lake=None, rough=0.08, trees=0.12, clusters=8):
        for i in range(-12, 30):
            for j in range(-12, 30):
                self.ground[(i, j)] = ROUGH if self.rng.random() < rough else GRASS[0]
        if lake:
            i0, j0, i1, j1 = lake
            for i in range(i0, i1 + 1):
                for j in range(j0, j1 + 1):
                    self.ground[(i, j)] = WATER; self.clear.add((i, j))
        self.tree_p = trees
        self.centers = [(self.rng.uniform(-8, 26), self.rng.uniform(-8, 26)) for _ in range(clusters)]

    def reserve(self, i, j, w, d):
        for a in range(int(i) - 0, int(i + w + 0.999)):
            for b in range(int(j) - 0, int(j + d + 0.999)):
                self.clear.add((a, b))

    def concrete(self, i, j, w, d):
        pad = IM(123)
        for a in range(i, i + w):
            for b in range(j, j + d):
                self.ground[(a, b)] = pad; self.clear.add((a, b))

    def coalground(self, i, j, w, d):
        for a in range(i, i + w):
            for b in range(j, j + d):
                self.ground[(a, b)] = IM(130 + (a + b) % 2); self.clear.add((a, b))

    def place_trees(self):
        for (i, j), g in self.ground.items():
            if (i, j) in self.clear or g is WATER:
                continue
            near = min((i - a) ** 2 + (j - b) ** 2 for a, b in self.centers)
            p = self.tree_p * (3.0 if near < 5 else 0.3)
            if self.rng.random() < p:
                for _ in range(self.rng.choice([1, 2, 2, 3])):
                    self.sprite(self.rng.choice(TREES), i + self.rng.uniform(0.2, 0.8), j + self.rng.uniform(0.2, 0.8), point=True)

    # anchor: sprite bottom-centre on the footprint's bottom corner (or a point)
    def sprite(self, im, i, j, w=1, d=1, point=False, dy=0):
        if point:
            x, y = self.P(i, j)
            depth = i + j
        else:
            x = (self.P(i + w, j)[0] + self.P(i, j + d)[0]) // 2
            y = self.P(i + w, j + d)[1]
            depth = i + j + w + d - 0.01
        self.objs.append((depth, 'img', (im, x - im.width // 2, y - im.height + dy)))

    def container(self, i, j, hot=False, w=1.0, d=0.5, h=12):
        self.objs.append((i + j + w + d, 'cont', (i, j, w, d, h, hot)))

    def worker(self, i, j):
        self.objs.append((i + j + 0.02, 'worker', (i, j)))

    def smoke(self, x, y, n=5, drift=1):
        self.post.append(('smoke', x, y, n, drift))

    def pylon_line(self, pts):
        self.post.append(('wire', pts))

    def ghost(self, i, j, w, d):
        self.post.append(('ghost', i, j, w, d))

    def fog(self, i0, j0, i1, j1):
        self.post.append(('fog', i0, j0, i1, j1))

    def glow(self, i, j):
        self.post.append(('glow', i, j))

    def render(self):
        img = self.img
        for (i, j) in sorted(self.ground, key=lambda k: (k[0] + k[1], k[0])):
            x, y = self.P(i, j)
            t = self.ground[(i, j)]
            if x + 40 < 0 or x - 40 > self.W or y > self.H or y + 40 < 0:
                continue
            img.alpha_composite(t, (x - t.width // 2, y + (31 - t.height)) if t.height <= 31 else (x - t.width // 2, y + 31 - t.height))
        d = ImageDraw.Draw(img)
        for _, kind, a in sorted(self.objs, key=lambda o: o[0]):
            if kind == 'img':
                im, x, y = a
                if -200 < x < self.W + 50 and -200 < y < self.H + 50:
                    img.alpha_composite(im, (x, y)) if x >= 0 and y >= 0 else self._paste_clip(im, x, y)
            elif kind == 'cont':
                self._cont(d, *a)
            elif kind == 'worker':
                x, y = self.P(*a)
                d.rectangle([x - 1, y - 6, x + 1, y - 2], fill=(56, 88, 150))
                d.rectangle([x - 1, y - 8, x + 1, y - 7], fill=(230, 186, 150))
                d.rectangle([x - 2, y - 9, x + 2, y - 9], fill=YE)
                d.point((x - 1, y - 1), fill=(40, 40, 40)); d.point((x + 1, y - 1), fill=(40, 40, 40))
        for p in self.post:
            getattr(self, '_' + p[0])(img, d, *p[1:])
        return img.convert('RGB')

    def _paste_clip(self, im, x, y):
        cx, cy = max(0, -x), max(0, -y)
        if cx >= im.width or cy >= im.height: return
        self.img.alpha_composite(im.crop((cx, cy, im.width, im.height)), (x + cx, y + cy))

    def _cont(self, d, i, j, w, dd, h, hot):
        N = self.P(i, j); E = self.P(i + w, j); S = self.P(i + w, j + dd); W = self.P(i, j + dd)
        up = lambda p, z=h: (p[0], p[1] - z)
        d.polygon([up(W), up(S), S, W], fill=MID)
        d.polygon([up(E), up(S), S, E], fill=DK)
        d.polygon([up(N), up(E), up(S), up(W)], fill=TOP)
        # corrugation ribs on the lit face
        n = int(abs(S[0] - W[0]) / 3)
        for k in range(1, n):
            t = k / n
            x = int(W[0] + (S[0] - W[0]) * t); y = int(W[1] + (S[1] - W[1]) * t)
            d.line([x, y - h + 1, x, y - 1], fill=LT if k % 2 else MID)
        # LEDs on the shaded face
        for k in range(4):
            t = (k + 0.5) / 4
            x = int(S[0] + (E[0] - S[0]) * t); y = int(S[1] + (E[1] - S[1]) * t)
            d.point((x, y - h // 2), fill=(CO if hot and k % 2 else CY))
            d.point((x, y - h // 2 - 3), fill=(CO if hot else CY))
        if hot:
            self.post.append(('glow_xy', (N[0] + S[0]) // 2, N[1] - h))

    def _smoke(self, img, d, x, y, n, drift):
        for k in range(n):
            r = 2 + k
            cx = x + k * 3 * drift + self.rng.randint(-1, 1)
            cy = y - 4 - k * 6
            col = SMOKE[min(k // 2, 2)]
            for dx in range(-r, r + 1):
                for dy in range(-r, r + 1):
                    if dx * dx + dy * dy <= r * r and (dx + dy + k) % (2 if k > 3 else 1) == 0:
                        d.point((cx + dx, cy + dy), fill=col)

    def _wire(self, img, d, pts):
        for (x, y, h) in pts:
            d.line([x, y, x, y - h], fill=(70, 70, 74)); d.line([x - 4, y - h + 2, x + 4, y - h + 2], fill=(70, 70, 74))
        for a, b in zip(pts, pts[1:]):
            for off in (-4, 4):
                x0, y0 = a[0] + off, a[1] - a[2] + 2; x1, y1 = b[0] + off, b[1] - b[2] + 2
                steps = max(abs(x1 - x0), 1)
                for s in range(steps + 1):
                    t = s / steps
                    d.point((round(x0 + (x1 - x0) * t), round(y0 + (y1 - y0) * t + 6 * 4 * t * (1 - t))), fill=(40, 40, 44))

    def _ghost(self, img, d, i, j, w, dd):
        lay = Image.new('RGBA', img.size, (0, 0, 0, 0)); ld = ImageDraw.Draw(lay)
        pts = [self.P(i, j), self.P(i + w, j), self.P(i + w, j + dd), self.P(i, j + dd)]
        ld.polygon(pts, fill=(252, 212, 56, 90))
        img.alpha_composite(lay)
        # dotted outline, old-school
        for a, b in zip(pts, pts[1:] + pts[:1]):
            steps = max(abs(b[0] - a[0]), 1)
            for s in range(0, steps + 1):
                if s % 4 < 2:
                    t = s / steps
                    d.point((round(a[0] + (b[0] - a[0]) * t), round(a[1] + (b[1] - a[1]) * t)), fill=YE)

    def _fog(self, img, d, i0, j0, i1, j1):
        poly = [self.P(i0, j0), self.P(i1, j0), self.P(i1, j1), self.P(i0, j1)]
        mask = Image.new('L', img.size, 255); ImageDraw.Draw(mask).polygon(poly, fill=0)
        dark = Image.new('RGBA', img.size, (10, 14, 18, 255))
        # 50% checker dither, like old fog-of-war
        dm = Image.new('L', img.size, 0); dmp = dm.load(); mp = mask.load()
        for y in range(img.size[1]):
            for x in range(img.size[0]):
                if mp[x, y] and ((x + y) % 2 == 0 or (x % 4 == 0 and y % 2 == 1)):
                    dmp[x, y] = 255
        img.paste(dark, (0, 0), dm)

    def _glow(self, img, d, i, j):
        x, y = self.P(i, j); self._glow_xy(img, d, x, y)

    def _glow_xy(self, img, d, x, y):
        for r, a in ((14, 50), (9, 80), (5, 120)):
            lay = Image.new('RGBA', img.size, (0, 0, 0, 0))
            ImageDraw.Draw(lay).ellipse([x - r * 2, y - r, x + r * 2, y + r], fill=(255, 96, 72, a))
            img.alpha_composite(lay)


# ---------------------------------------------------------------- custom tech sprites
STEEL = ((176, 182, 186), (140, 146, 152), (104, 110, 116))
CONC = ((196, 190, 176), (164, 158, 144), (126, 120, 108))
GEN = ((120, 140, 96), (92, 110, 72), (68, 82, 54))
BLUE = (40, 96, 176); BLUE2 = (88, 150, 220)

def _box(self, i, j, w, d, h, cols, extra=None, z0=0):
    self.objs.append((i + j + w + d, 'xbox', (i, j, w, d, h, cols, extra, z0)))
TTDMap.box = _box

def _xbox(self, dr, i, j, w, d, h, cols, extra, z0):
    N = self.P(i, j, z0); E = self.P(i + w, j, z0); S = self.P(i + w, j + d, z0); W = self.P(i, j + d, z0)
    up = lambda p, z=h: (p[0], p[1] - z)
    top, left, right = cols
    dr.polygon([up(W), up(S), S, W], fill=left)
    dr.polygon([up(E), up(S), S, E], fill=right)
    dr.polygon([up(N), up(E), up(S), up(W)], fill=top)
    if extra:
        extra(self, dr, N, E, S, W, h)
TTDMap._xbox = _xbox

_old_render = TTDMap.render
def _render(self):
    img = self.img
    for (i, j) in sorted(self.ground, key=lambda k: (k[0] + k[1], k[0])):
        x, y = self.P(i, j)
        t = self.ground[(i, j)]
        if x + 40 < 0 or x - 40 > self.W or y > self.H or y + 40 < 0:
            continue
        self._paste_clip(t, x - t.width // 2, y + 31 - t.height) if (x - t.width // 2 < 0 or y + 31 - t.height < 0) else img.alpha_composite(t, (x - t.width // 2, y + 31 - t.height))
    d = ImageDraw.Draw(img)
    for _, kind, a in sorted(self.objs, key=lambda o: o[0]):
        if kind == 'img':
            im, x, y = a
            if -300 < x < self.W + 50 and -300 < y < self.H + 50:
                self._paste_clip(im, x, y)
        elif kind == 'cont':
            self._cont(d, *a)
        elif kind == 'xbox':
            self._xbox(d, *a)
        elif kind == 'xfn':
            {'wind': _windraw, 'tokamak': _tokdraw, 'core': _coredraw}[a[0]](self, d, *a[1])
        elif kind == 'worker':
            x, y = self.P(*a)
            d.rectangle([x - 1, y - 6, x + 1, y - 2], fill=(56, 88, 150))
            d.rectangle([x - 1, y - 8, x + 1, y - 7], fill=(230, 186, 150))
            d.rectangle([x - 2, y - 9, x + 2, y - 9], fill=YE)
            d.point((x - 1, y - 1), fill=(40, 40, 40)); d.point((x + 1, y - 1), fill=(40, 40, 40))
    for p in self.post:
        getattr(self, '_' + p[0])(img, d, *p[1:])
    # film grain
    px = img.load(); rng = random.Random(7)
    for y in range(img.size[1]):
        for x in range(img.size[0]):
            n = rng.randint(-7, 7)
            r, g, b, a = px[x, y]
            px[x, y] = (max(0, min(255, r + n)), max(0, min(255, g + n)), max(0, min(255, b + n)), a)
    return img.convert('RGB')
TTDMap.render = _render

def _paste_clip(self, im, x, y):
    cx, cy = max(0, -x), max(0, -y)
    if cx >= im.width or cy >= im.height or x >= self.W or y >= self.H: return
    self.img.alpha_composite(im.crop((cx, cy, im.width, im.height)), (x + cx, y + cy))
TTDMap._paste_clip = _paste_clip

def _pix(d, x, y, c): d.point((int(x), int(y)), fill=c)
def _lerp(a, b, t): return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)

# --- mining tech
def immersion(self, i, j):
    def ex(m, d, N, E, S, W, h):
        up = lambda p: (p[0], p[1] - h)
        inset = [_lerp(up(N), up(S), 0.12), _lerp(up(E), up(W), 0.12), _lerp(up(S), up(N), 0.12), _lerp(up(W), up(E), 0.12)]
        d.polygon(inset, fill=BLUE)
        for k in range(6):
            p = _lerp(_lerp(inset[0], inset[2], m.rng.random()), _lerp(inset[1], inset[3], m.rng.random()), 0.5)
            _pix(d, p[0], p[1], BLUE2)
        for t in (0.25, 0.5, 0.75):
            p = _lerp(W, S, t); d.line([p[0], p[1] - 2, p[0], p[1] - h + 2], fill=STEEL[0])
    self.box(i, j, 1, 1, 9, STEEL, ex)
TTDMap.immersion = immersion

def transformer(self, i, j):
    def ex(m, d, N, E, S, W, h):
        for t in (0.2, 0.5, 0.8):
            p = _lerp(_lerp(N, S, 0.5), _lerp(E, W, 0.5), 0)  # centre
            q = _lerp(W, E, t); q = (q[0], q[1] - h)
            d.line([q[0], q[1], q[0], q[1] - 5], fill=(214, 200, 160)); _pix(d, q[0], q[1] - 6, (232, 228, 220))
        for t in (0.15, 0.4, 0.65, 0.9):
            a = _lerp(W, S, t); d.line([a[0], a[1] - 3, a[0] + 2, a[1] - 3], fill=YE)
    self.box(i + 0.15, j + 0.15, 0.7, 0.7, 10, ((150, 156, 160), (122, 128, 132), (90, 96, 100)), ex)
TTDMap.transformer = transformer

def fanwall(self, i, j, w=1, d=0.35):
    def ex(m, dr, N, E, S, W, h):
        n = 3
        for k in range(n):
            c = _lerp(W, S, (k + 0.5) / n); c = (c[0], c[1] - h / 2)
            dr.ellipse([c[0] - 3, c[1] - 3, c[0] + 3, c[1] + 3], fill=(44, 48, 52)); _pix(dr, c[0], c[1], STEEL[0])
            dr.line([c[0] - 2, c[1], c[0] + 2, c[1]], fill=(90, 94, 98))
    self.box(i, j, w, d, 10, STEEL, ex)
TTDMap.fanwall = fanwall

# --- power sources
def diesel(self, i, j):
    def ex(m, d, N, E, S, W, h):
        top = _lerp(N, S, 0.3); top = (top[0], top[1] - h)
        d.line([top[0], top[1], top[0], top[1] - 8], fill=(60, 60, 60)); d.line([top[0] + 1, top[1], top[0] + 1, top[1] - 8], fill=(90, 90, 90))
        m.smoke(top[0], top[1] - 9, 3)
        for t in (0.3, 0.55, 0.8):
            a = _lerp(W, S, t); d.line([a[0], a[1] - 3, a[0], a[1] - h + 3], fill=GEN[2])
    self.box(i + 0.1, j + 0.2, 0.8, 0.6, 9, GEN, ex)
TTDMap.diesel = diesel

def solar(self, i, j, w=2, d=1):
    for a in range(int(w * 2)):
        ii = i + a * 0.5
        def ex(m, dr, N, E, S, W, h):
            for t in (0.25, 0.5, 0.75):
                p = _lerp(N, W, t); q = _lerp(E, S, t)
                dr.line([p[0], p[1] - h, q[0], q[1] - h], fill=(70, 110, 170))
        self.box(ii + 0.05, j + 0.05, 0.4, d - 0.1, 3, ((36, 60, 120), (150, 156, 160), (110, 116, 120)), ex)
TTDMap.solar = solar

def wind(self, i, j, phase=0):
    x, y = self.P(i + 0.5, j + 0.5)
    self.objs.append((i + j + 1, 'wind', (x, y, phase)))
TTDMap.wind = wind

def _windraw(self, d, x, y, phase):
    d.line([x, y, x, y - 46], fill=(232, 232, 228)); d.line([x + 1, y, x + 1, y - 46], fill=(196, 196, 192))
    hub = (x, y - 46)
    import math
    for k in range(3):
        a = phase + k * 2 * math.pi / 3
        d.line([hub, (hub[0] + 18 * math.cos(a), hub[1] + 18 * math.sin(a))], fill=(240, 240, 236), width=1)
    d.rectangle([hub[0] - 2, hub[1] - 1, hub[0] + 2, hub[1] + 1], fill=(200, 200, 196))

def reactor(self, i, j):
    def ex(m, d, N, E, S, W, h):
        c = _lerp(N, S, 0.5); c = (c[0], c[1] - h)
        d.pieslice([c[0] - 18, c[1] - 16, c[0] + 18, c[1] + 10], 180, 360, fill=(214, 210, 200))
        d.pieslice([c[0] + 2, c[1] - 14, c[0] + 18, c[1] + 10], 270, 360, fill=(176, 172, 164))
        d.arc([c[0] - 18, c[1] - 16, c[0] + 18, c[1] + 10], 180, 360, fill=(150, 146, 138))
        _pix(d, c[0] - 6, c[1] - 9, (255, 255, 255))
    self.box(i, j, 1.4, 1.4, 8, CONC, ex)
TTDMap.reactor = reactor

def tokamak(self, i, j):
    x, y = self.P(i + 1, j + 1)
    self.objs.append((i + j + 2, 'tokamak', (x, y)))
TTDMap.tokamak = tokamak

def _tokdraw(self, d, x, y):
    d.ellipse([x - 30, y - 22, x + 30, y + 4], fill=(120, 124, 132)); d.ellipse([x - 30, y - 26, x + 30, y], fill=(176, 180, 188))
    d.ellipse([x - 14, y - 19, x + 14, y - 7], fill=(40, 30, 60))
    for r, col in ((24, (210, 120, 255)), (20, (120, 230, 255))):
        d.arc([x - r, y - 13 - r // 2.2, x + r, y - 13 + r // 2.2], 0, 360, fill=col)
    for k in range(10):
        _pix(d, x - 26 + k * 6, y - 13 + (k % 3) - 1, (255, 255, 255))

def core(self, i, j):
    x, y = self.P(i + 0.5, j + 0.5)
    self.objs.append((i + j + 1, 'core', (x, y)))
TTDMap.core = core

def _coredraw(self, d, x, y):
    d.polygon([(x - 10, y), (x, y + 5), (x + 10, y), (x + 6, y - 40), (x, y - 56), (x - 6, y - 40)], fill=(40, 44, 60))
    d.polygon([(x, y + 5), (x + 10, y), (x + 6, y - 40), (x, y - 56)], fill=(28, 30, 42))
    for k in range(0, 56, 2):
        _pix(d, x - 1, y - k, (200, 255, 250) if k % 4 else (120, 240, 255))
    d.ellipse([x - 7, y - 66, x + 7, y - 52], fill=(170, 250, 255)); d.ellipse([x - 4, y - 63, x + 4, y - 55], fill=(255, 255, 255))

_ext = TTDMap.render
def _render2(self):
    # inject custom draw kinds before base render handles them
    extra = [o for o in self.objs if o[1] in ('wind', 'tokamak', 'core')]
    self.objs = [o for o in self.objs if o[1] not in ('wind', 'tokamak', 'core')]
    for o in extra:
        self.objs.append((o[0], 'xfn', (o[1], o[2])))
    return _ext(self)
TTDMap.render = _render2

_old_xbox = TTDMap._xbox
def _xbox_or_fn(self, d, *a):
    return _old_xbox(self, d, *a)
TTDMap._xbox = _xbox_or_fn
