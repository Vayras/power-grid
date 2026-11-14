"""Voxel models for every Power Grid sprite (same names and order as the old OpenGFX atlas).

Buildings and trees use res=32 (32 voxels per tile edge, one voxel of height = 2.5 atlas px);
ground tiles use res=16 so the terrain reads as chunky voxels. Colours are sRGB 0-255.
"""
import math
import numpy as np
from vox import Model

T = 32  # voxels per tile edge for objects

# ---------------------------------------------------------------- palette
CONCRETE = (182, 178, 170)
CONCRETE_DK = (150, 146, 138)
ASPHALT = (78, 78, 80)
STEEL = (150, 156, 162)
STEEL_DK = (92, 98, 106)
WHITE = (232, 232, 226)
RED = (196, 58, 44)
YELLOW = (236, 190, 48)
GLASS = (150, 196, 210)
CYAN = (90, 240, 230)
COPPER = (196, 112, 58)
BRICK = (150, 82, 60)
WOOD = (122, 86, 54)
RUBBER = (36, 36, 38)


def pad(m, c=CONCRETE, h=1):
    m.box(0, 0, 0, m.W, m.D, h, c, jit=0.02, var=(CONCRETE_DK, 0.08))


def lattice(m, x0, y0, x1, y1, z0, z1, c=STEEL, step=6, r=0.7):
    """A lattice tower between two square footprints (legs + cross braces)."""
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    legs = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    top = [(cx + (x - cx) * 0.35, cy + (y - cy) * 0.35) for x, y in legs]
    for (a, b), (c2, d) in zip(legs, top):
        m.line((a, b, z0), (c2, d, z1), r, c, jit=0.02)
    n = int((z1 - z0) / step)
    for k in range(n):
        za, zb = z0 + k * step, z0 + (k + 1) * step
        fa = 1 - 0.65 * (za - z0) / (z1 - z0); fb = 1 - 0.65 * (zb - z0) / (z1 - z0)
        for q in range(4):
            (xa, ya), (xb, yb) = legs[q], legs[(q + 1) % 4]
            pa = (cx + (xa - cx) * fa, cy + (ya - cy) * fa, za)
            pb = (cx + (xb - cx) * fb, cy + (yb - cy) * fb, zb)
            m.line(pa, pb, r * 0.7, c, jit=0.02)


# ---------------------------------------------------------------- ground (res 16)
GROUND = {
    # site: (main, main_var, accent, accent_raise, rough)
    0: ((104, 128, 62), (92, 116, 54), (124, 112, 78), 0, (128, 112, 88)),     # Texas grass
    1: ((86, 102, 78), (70, 86, 66), (214, 222, 232), 0, (84, 80, 84)),        # Iceland: moss on lava, snow
    2: ((70, 112, 40), (60, 98, 34), (176, 86, 48), 0, (150, 70, 40)),         # Paraguay: lush on red earth
    3: ((146, 130, 78), (126, 112, 66), (92, 70, 54), 0, (84, 66, 52)),        # Ethiopia: highland grass, dark soil
    4: ((66, 104, 64), (54, 90, 54), (46, 78, 48), 0, (142, 144, 148)),        # Bhutan: alpine meadow, granite
    5: ((170, 150, 106), (150, 132, 92), (214, 218, 224), 0, (196, 192, 184)),  # North Dakota: winter prairie, frost
}


def ground(c, var=None, seed=1, tufts=None, stones=None, patch=None, raise_patch=0, water=False, seams=False, lumps=None):
    m = Model(1, 1, 6, res=16, zbase=3, seed=seed)
    m.catcher = False
    top = 2 if water else 3
    m.box(0, 0, 0, 16, 16, top, c, jit=0.03 if not water else 0.015, var=var)
    rng = np.random.default_rng(seed)
    if water:
        # gentle ripples: a few lighter voxels on the surface
        for _ in range(14):
            x, y = rng.integers(0, 16, 2)
            m.box(x, y, top - 1, x + 1 + rng.integers(0, 2), y + 1, top, (88, 150, 182), jit=0.01)
    if seams:
        for k in (0, 8):
            m.box(k, 0, 2, k + 1, 16, 3, (156, 152, 146), jit=0.01)
            m.box(0, k, 2, 16, k + 1, 3, (156, 152, 146), jit=0.01)
    if patch is not None:
        pc, shape = patch
        cx, cy, rx, ry = shape
        X, Y = np.mgrid[0:16, 0:16] + 0.5
        q = ((X - cx) / rx) ** 2 + ((Y - cy) / ry) ** 2 + rng.normal(0, 0.18, (16, 16))
        for x, y in zip(*np.nonzero(q < 1.0)):
            m.box(x, y, 2, x + 1, y + 1, 3 + (raise_patch if q[x, y] < 0.6 else 0), pc, jit=0.03)
    if tufts:
        tc, n = tufts
        for _ in range(n):
            x, y = rng.integers(0, 16, 2)
            m.box(x, y, 3, x + 1, y + 1, 4, tc, jit=0.05)
    if stones:
        sc, n = stones
        for _ in range(n):
            x, y = rng.integers(4, 12, 2)          # stones stay inside the patch
            w = 1 + rng.integers(0, 2)
            m.box(x, y, 3, x + w, y + 1 + rng.integers(0, 2), 4 + (rng.random() < 0.3), sc, jit=0.06)
    if lumps:
        lc, n = lumps
        for _ in range(n):
            x, y = rng.integers(0, 16, 2)
            m.box(x, y, 3, x + 1, y + 1, 4, lc, jit=0.08)
    return m


def site_tiles(site):
    main, mv, acc, ar, rough = GROUND[site]
    tiles = []
    tuft = tuple(int(v * 0.95) for v in mv)
    for k in range(4):
        tiles.append(ground(main, var=(mv, 0.32), seed=site * 31 + k))
    for k in range(3):
        shape = [(8, 8, 7, 6), (6, 10, 6.5, 7), (10, 7, 7.5, 5.5)][k]
        tiles.append(ground(main, var=(mv, 0.3), seed=site * 37 + k, patch=(acc, shape), raise_patch=ar))
    for k in range(2):
        shape = [(7, 9, 5, 5), (9, 7, 5.5, 4)][k]
        tiles.append(ground(main, var=(mv, 0.3), seed=site * 41 + k, patch=(tuple(int(v * 0.85) for v in rough), shape), stones=(rough, 3)))
    return tiles


# ---------------------------------------------------------------- trees (res 32, anchored at tile centre)
def tree(kind, seed=0):
    m = Model(1, 1, 60, res=T, seed=seed + kind * 13)
    cx = cy = 16
    bark = (96, 72, 50)
    if kind == 0:     # round oak
        m.cyl(cx, cy, 1.8, 0, 12, bark)
        m.ellipsoid(cx, cy, 20, 11, 11, 9, (72, 106, 46), noise=0.12, var=((92, 126, 58), 0.3))
    elif kind == 1:   # tall broadleaf
        m.cyl(cx, cy, 1.6, 0, 16, bark)
        m.ellipsoid(cx, cy, 26, 9, 9, 13, (54, 94, 42), noise=0.12, var=((70, 112, 52), 0.3))
    elif kind == 2:   # acacia: flat top on a leaning trunk
        m.line((cx, cy, 0), (cx - 3, cy + 2, 16), 1.5, bark)
        m.line((cx - 3, cy + 2, 14), (cx + 4, cy - 2, 19), 1.0, bark)
        m.ellipsoid(cx - 1, cy, 20, 13, 12, 3.6, (106, 118, 56), noise=0.1, var=((124, 132, 66), 0.3))
    elif kind == 3:   # bush
        m.ellipsoid(cx, cy, 6, 9, 9, 6.5, (78, 110, 52), noise=0.14, var=((96, 126, 62), 0.3))
    elif kind == 4:   # birch
        m.cyl(cx, cy, 1.4, 0, 20, (226, 222, 212), var=((54, 50, 48), 0.18))
        m.ellipsoid(cx, cy, 24, 7.5, 7.5, 10, (124, 152, 72), noise=0.14, var=((146, 168, 84), 0.3))
    elif kind == 5:   # pine
        m.cyl(cx, cy, 1.5, 0, 8, bark)
        m.lathe(cx, cy, 5, 42, lambda t: 10.5 * (1 - t) ** 0.9 * (0.82 + 0.18 * np.cos(t * 24)), (44, 82, 54), var=((56, 96, 62), 0.25))
    elif kind == 6:   # spruce
        m.cyl(cx, cy, 1.4, 0, 6, bark)
        m.lathe(cx, cy, 3, 48, lambda t: 8.5 * (1 - t) * (0.8 + 0.2 * np.cos(t * 30)), (32, 64, 48), var=((42, 76, 56), 0.25))
    elif kind == 7:   # low shrub with flowers
        m.ellipsoid(cx, cy, 4, 6.5, 6.5, 4.5, (104, 122, 64), noise=0.16, var=((214, 196, 88), 0.06))
    elif kind == 8:   # palm
        pts = [(cx + 0.18 * z, cy - 0.05 * z, z) for z in range(0, 31, 3)]
        for a, b in zip(pts, pts[1:]): m.line(a, b, 1.5, (150, 120, 84), var=((120, 94, 64), 0.3))
        top = pts[-1]
        for k in range(7):
            ang = k * 2 * math.pi / 7
            for s in range(1, 13):
                x = top[0] + math.cos(ang) * s; y = top[1] + math.sin(ang) * s; z = top[2] + 1.5 - 0.035 * s * s
                m.sphere(x, y, z, 1.4, (70, 122, 52), var=((88, 140, 62), 0.3))
    elif kind == 9:   # cypress
        m.cyl(cx, cy, 1.2, 0, 4, bark)
        m.ellipsoid(cx, cy, 22, 5, 5, 20, (44, 80, 48), noise=0.1, var=((54, 92, 56), 0.25))
    else:             # round dark
        m.cyl(cx, cy, 1.8, 0, 10, bark)
        m.ellipsoid(cx - 2, cy + 1, 17, 9, 9, 8, (46, 84, 44), noise=0.12, var=((58, 98, 52), 0.3))
        m.ellipsoid(cx + 4, cy - 3, 21, 7, 7, 6, (52, 92, 48), noise=0.12)
    return m


# ---------------------------------------------------------------- power
def b_grid():
    m = Model(1, 1, 56, res=T, seed=61); pad(m)
    # fenced transformer yard
    m.box(8, 9, 1, 22, 21, 13, (104, 114, 106), var=((92, 102, 94), 0.2))
    for x in range(9, 22, 2): m.box(x, 21, 2, x + 1, 23, 12, (86, 96, 90))     # radiator fins
    for x in (11, 15, 19):
        m.cyl(x, 13, 1.1, 13, 19, (214, 210, 200))
        m.cyl(x, 13, 1.6, 15, 16, (184, 180, 172))
    m.box(9, 3, 1, 12, 7, 6, (92, 98, 92))
    # lattice pylon at the back corner, wires attach above the box
    lattice(m, 22, 2, 30, 10, 1, 52, STEEL_DK, step=7, r=0.8)
    m.box(18, 4, 46, 32, 8, 47, STEEL_DK)
    for x in (19, 25, 31): m.cyl(x, 6, 0.8, 42, 46, (214, 210, 200))
    # chain-link posts
    for (x, y) in ((1, 1), (31, 1), (1, 31), (31, 31), (16, 31), (1, 16)):
        m.box(x - 0.5, y - 0.5, 1, x + 0.5, y + 0.5, 8, STEEL)
    return m


def b_diesel():
    m = Model(1, 1, 40, res=T, seed=62); pad(m)
    m.box(3, 8, 1, 29, 24, 13, (96, 112, 64), var=((86, 100, 58), 0.15))     # genset enclosure
    for x in range(5, 27, 3): m.box(x, 24, 4, x + 2, 25, 11, (70, 82, 48))    # louvres
    m.box(24, 9, 13, 28, 13, 15, (60, 64, 60))
    m.cyl(26, 11, 1.4, 13, 24, (84, 80, 76))                                    # exhaust stack
    m.hcyl('x', 4, 22, 4, 4, 3, (176, 172, 160))                                # fuel tank
    m.box(3, 8, 13, 29, 24, 14, (110, 126, 74))
    m.emit.append((26, 11, 25))
    return m


def b_gas():
    m = Model(2, 2, 92, res=T, seed=63); pad(m)
    m.box(6, 30, 1, 40, 58, 22, (196, 202, 208), var=((180, 186, 194), 0.1))  # turbine hall
    m.box(6, 30, 22, 40, 58, 24, (140, 150, 160))
    for x in range(8, 40, 4): m.box(x, 58, 6, x + 2, 58.6, 18, (150, 196, 214), glow=0.15)  # windows
    m.box(36, 10, 1, 56, 30, 34, (176, 172, 164), var=((160, 156, 150), 0.12))  # HRSG
    m.cyl(50, 18, 4.5, 1, 86, (226, 226, 222))                                  # stack
    m.cyl(50, 18, 4.7, 74, 80, RED)
    m.cyl(50, 18, 4.7, 60, 64, RED)
    m.hcyl('y', 30, 40, 45, 26, 2.2, STEEL)                                     # duct
    for (x, y) in ((12, 10), (24, 10)):
        m.cyl(x, y, 6, 1, 16, (222, 222, 216))                                  # tanks
        m.sphere(x, y, 16, 6, (222, 222, 216))
    m.line((18, 10, 6), (36, 18, 6), 1.2, STEEL_DK)
    m.box(46, 40, 1, 60, 56, 12, (104, 114, 106))                                # transformer
    m.emit.append((50, 18, 88))
    return m


def b_coal():
    m = Model(3, 2, 104, res=T, seed=64)
    m.box(0, 0, 0, m.W, m.D, 1, (66, 62, 58), jit=0.05)
    def tower(cx, cy):
        m.lathe(cx, cy, 1, 62, lambda t: 13 - 7.5 * np.sin(np.clip(t, 0, 1) * 2.4) ** 1.3 + 1.5 * t, (196, 192, 184), hollow=2.2, var=((176, 172, 166), 0.15))
        m.emit.append((cx, cy, 64))
    tower(48, 16); tower(76, 18)
    m.box(4, 34, 1, 34, 60, 46, (150, 86, 62), var=((136, 76, 56), 0.2))       # boiler house
    m.box(4, 34, 46, 34, 60, 48, (110, 70, 54))
    for z in range(8, 44, 7):
        for x in range(7, 33, 5): m.box(x, 60, z, x + 3, 60.6, z + 3, (160, 196, 210), glow=0.2)
    m.cyl(10, 26, 3.6, 1, 96, (162, 108, 82))                                   # chimney
    m.cyl(10, 26, 3.8, 88, 92, (230, 230, 226))
    m.emit.append((10, 26, 98))
    # coal pile + conveyor
    m.ellipsoid(68, 46, 1, 18, 12, 11, (40, 38, 38), noise=0.1, var=((56, 52, 50), 0.3))
    m.line((52, 46, 4), (34, 46, 30), 1.6, (120, 120, 116))
    return m


def b_solar():
    m = Model(2, 2, 20, res=T, seed=65)
    m.box(0, 0, 0, m.W, m.D, 1, (126, 132, 102), jit=0.04, var=((110, 118, 90), 0.3))
    for row in range(4):
        y0 = 3 + row * 15
        for x0 in range(2, 62, 15):
            for k in range(12):        # tilted toward the sun (+y): one voxel up every two rows
                z = 3 + (11 - k) // 2
                c = (120, 130, 140) if k in (0, 11) else (34, 54, 94)
                m.box(x0, y0 + k, z, x0 + 14, y0 + k + 1, z + 1, c, jit=0.02)
                m.box(x0 + 4.6, y0 + k, z, x0 + 5, y0 + k + 1, z + 1, (86, 104, 140), jit=0.01)
                m.box(x0 + 9.6, y0 + k, z, x0 + 10, y0 + k + 1, z + 1, (86, 104, 140), jit=0.01)
            for x in (x0 + 2, x0 + 11):
                m.box(x, y0 + 5, 1, x + 1, y0 + 6, 5, STEEL_DK)
    return m


def b_wind(phase):
    m = Model(1, 1, 150, res=T, seed=66)
    m.box(9, 9, 0, 23, 23, 2, CONCRETE)
    m.lathe(16, 16, 2, 118, lambda t: 2.6 - 1.2 * t, (236, 236, 232), jit=0.015)
    # nacelle facing the camera (rotor plane perpendicular to (1,1,0))
    m.box(13, 13, 116, 19, 19, 122, (232, 232, 228))
    hub = (19.5, 19.5, 119)
    m.sphere(*hub, 2.2, (220, 220, 214))
    u = np.array([1, -1, 0]) / math.sqrt(2); w = np.array([0, 0, 1.0])
    for k in range(3):
        a = phase + k * 2 * math.pi / 3
        d = u * math.cos(a) + w * math.sin(a)
        tip = np.array(hub) + d * 34
        mid = np.array(hub) + d * 12
        m.line(hub, mid, 1.5, (236, 236, 230))
        m.line(mid, tip, 0.9, (236, 236, 230))
        m.line(tip - d * 3, tip, 1.0, (210, 80, 60))
    return m


def b_hydro():
    m = Model(2, 2, 60, res=T, seed=67); pad(m)
    m.box(0, 0, 1, 64, 26, 14, (52, 104, 150), jit=0.02, var=((70, 126, 168), 0.15))  # reservoir
    m.box(0, 26, 1, 64, 34, 40, (190, 186, 176), var=((170, 166, 158), 0.15))        # dam wall
    m.box(0, 25, 38, 64, 35, 41, (160, 156, 148))
    for x in (12, 30, 48):
        m.hcyl('y', 34, 44, x, 10, 3, (120, 126, 134))                                 # penstocks
    m.box(6, 42, 1, 56, 60, 18, (176, 172, 162))                                       # powerhouse
    m.box(6, 42, 18, 56, 60, 20, (130, 138, 146))
    for x in range(9, 54, 5): m.box(x, 60, 6, x + 3, 60.6, 14, (160, 200, 214), glow=0.15)
    m.box(42, 50, 20, 52, 58, 28, (104, 114, 106))                                     # transformer
    # spillway foam
    m.box(28, 34, 1, 36, 42, 3, (214, 230, 236))
    return m


def b_fission():
    m = Model(2, 2, 110, res=T, seed=68); pad(m)
    m.cyl(18, 22, 13, 1, 34, (220, 218, 212), var=((206, 204, 198), 0.15))          # containment
    m.ellipsoid(18, 22, 34, 13, 13, 11, (220, 218, 212))
    m.box(4, 40, 1, 30, 60, 22, (196, 198, 202))                                     # turbine hall
    m.box(4, 40, 22, 30, 60, 24, (140, 150, 160))
    m.lathe(46, 46, 1, 70, lambda t: 15 - 8 * np.sin(np.clip(t, 0, 1) * 2.4) ** 1.3 + 1.5 * t, (200, 196, 188), hollow=2.4, var=((182, 178, 170), 0.15))
    m.emit.append((46, 46, 72))
    m.hcyl('x', 30, 34, 50, 12, 2, STEEL)
    return m


def b_fusion():
    m = Model(2, 2, 70, res=T, seed=69); pad(m)
    cx, cy, cz = 32, 32, 18
    R, r = 17, 8.5
    d = np.sqrt((m.X - cx) ** 2 + (m.Y - cy) ** 2)
    torus = (d - R) ** 2 + (m.Z - cz) ** 2 <= r * r
    m.fill(torus, (150, 158, 170), var=((128, 136, 148), 0.2))
    ang = np.arctan2(m.Y - cy, m.X - cx)
    coil = torus & (np.abs(((ang / (2 * np.pi) * 16) % 1) - 0.5) < 0.14) & ((d - R) ** 2 + (m.Z - cz) ** 2 >= (r - 1.5) ** 2)
    m.paint(coil, COPPER)
    # plasma glow in the bore
    m.fill(((d - R) ** 2 + (m.Z - cz) ** 2 <= 2.2 ** 2) & (m.Z > cz) & ((m.X + m.Y) > 2 * cx + 6), (255, 140, 230), glow=1.6)
    m.cyl(cx, cy, 4, 1, 34, STEEL_DK)
    for (x, y) in ((6, 56), (14, 58)):
        m.cyl(x, y, 3.6, 1, 22, (226, 230, 236))
        m.sphere(x, y, 22, 3.6, (226, 230, 236))
    m.box(48, 4, 1, 62, 18, 16, (196, 198, 202))
    return m


def b_core():
    m = Model(2, 2, 150, res=T, seed=70); pad(m, (70, 72, 80))
    cx = cy = 32
    m.cyl(cx, cy, 18, 1, 5, (92, 96, 108))
    m.cyl(cx, cy, 15, 5, 8, (60, 64, 74))
    m.lathe(cx, cy, 8, 120, lambda t: 6.5 * (1 - t) ** 0.7 + 1.2, (36, 40, 52), var=((54, 60, 76), 0.2))
    for z in (30, 60, 90):
        d = np.sqrt((m.X - cx) ** 2 + (m.Y - cy) ** 2)
        m.fill((np.abs(d - 11 + z * 0.04) < 1.0) & (np.abs(m.Z - z) < 1.0), (210, 220, 230))
    m.sphere(cx, cy, 126, 6, (80, 220, 214), glow=0.9)
    m.fill(m.cylm(cx, cy, 2.2, 8, 120) & ((m.X + m.Y) > 2 * cx + 1), (70, 210, 204), glow=0.9)
    for k in range(4):
        a = k * math.pi / 2 + math.pi / 4
        x, y = cx + math.cos(a) * 16, cy + math.sin(a) * 16
        m.line((x, y, 5), (cx + math.cos(a) * 5, cy + math.sin(a) * 5, 70), 1.2, (180, 186, 196))
    return m


# ---------------------------------------------------------------- miners: one shipping container per generation
GENS = [(110, 114, 120), (156, 132, 98), (68, 86, 108), (46, 46, 52)]
GOLDS = [(236, 196, 92), (230, 182, 104), (218, 188, 116), (206, 160, 66)]


def miner(c, seed):
    m = Model(1, 1, 24, res=T, seed=seed); pad(m)
    x0, x1, y0, y1 = 2, 30, 8, 25
    dk = tuple(int(v * 0.8) for v in c)
    m.box(x0, y0, 1, x1, y1, 13, c, jit=0.02)
    for x in range(x0 + 1, x1 - 1, 2):                 # corrugation
        m.box(x, y1 - 0.01, 2, x + 1, y1 + 0.6, 12, dk, jit=0.02)
        m.box(x, y0 - 0.6, 2, x + 1, y0, 12, dk, jit=0.02)
    for y in range(y0 + 1, y1 - 1, 2):
        m.box(x1, y, 2, x1 + 0.6, y + 1, 12, dk, jit=0.02)
    m.box(x0, y0, 13, x1, y1, 14, tuple(min(255, int(v * 1.08)) for v in c), jit=0.02)
    for y in (y0 + 3, y0 + 7, y0 + 11):               # door bars on the +x end
        m.box(x1 + 0.6, y, 2, x1 + 1.3, y + 1, 12, (170, 170, 168))
    for x in (8, 16, 24):                              # roof fans
        m.cyl(x, 16.5, 3, 14, 15, (40, 42, 46))
        m.cyl(x, 16.5, 1, 15, 16, (90, 94, 100))
    for x in range(5, 27, 4):                          # LED strip
        m.box(x, y1 + 0.6, 10, x + 2, y1 + 1.2, 11, CYAN, glow=1.2)
    return m


def fanwall(along_i):
    m = Model(1, 1, 30, res=T, seed=81); pad(m)
    frame = (120, 128, 136)
    if along_i:
        m.box(2, 12.5, 1, 30, 18.5, 13, frame)
        for x in (7, 16, 25):
            m.hcyl('y', 17.9, 19, x, 7, 3.6, (40, 44, 50))
            m.hcyl('y', 18.1, 19.3, x, 7, 1.0, (150, 156, 164))
            for k in range(3):
                a = k * 2.1
                m.line((x, 19, 7), (x + math.cos(a) * 3.1, 19, 7 + math.sin(a) * 3.1), 0.7, (170, 176, 184))
    else:
        m.box(12.5, 2, 1, 18.5, 30, 13, frame)
        for y in (7, 16, 25):
            m.hcyl('x', 17.9, 19, y, 7, 3.6, (40, 44, 50))
            m.hcyl('x', 18.1, 19.3, y, 7, 1.0, (150, 156, 164))
            for k in range(3):
                a = k * 2.1
                m.line((19, y, 7), (19, y + math.cos(a) * 3.1, 7 + math.sin(a) * 3.1), 0.7, (170, 176, 184))
    return m


def b_pole():
    m = Model(1, 1, 24, res=T, seed=83)
    m.cyl(16, 16, 1.0, 0, 19, WOOD, var=((106, 74, 46), 0.3))
    m.box(11, 15.2, 16, 21, 16.8, 17.2, WOOD)
    for x in (12, 16, 20): m.cyl(x, 16, 0.7, 17.2, 18.8, (200, 214, 220))
    m.cyl(18, 16, 1.2, 11, 14, (132, 138, 146))   # pole transformer
    return m


def b_wall_thumb():
    m = Model(2, 1, 40, res=T, seed=84)
    stone = (186, 182, 172)
    m.box(8, 8, 0, 20, 20, 30, stone, var=((170, 166, 158), 0.2))
    m.box(20, 10, 0, 44, 18, 22, (172, 168, 158), var=((150, 146, 138), 0.2))
    m.box(44, 8, 0, 56, 20, 30, stone, var=((170, 166, 158), 0.2))
    return m


def b_sign():
    m = Model(1, 1, 26, res=T, seed=85)
    for x in (12, 20): m.box(x, 15.4, 0, x + 1.2, 16.6, 16, STEEL_DK)
    m.box(10, 15.4, 10, 23, 16.6, 20, (240, 196, 52))
    m.box(11.5, 16.6, 12.5, 21.5, 17, 14, (40, 40, 40))
    m.box(11.5, 16.6, 16, 19, 17, 17.5, (40, 40, 40))
    return m


def b_immersion():
    m = Model(1, 1, 30, res=T, seed=86); pad(m)
    m.box(3, 4, 1, 23, 28, 13, (176, 182, 190))                       # tank
    m.box(5, 6, 12, 21, 26, 13, (40, 120, 190), glow=0.2, jit=0.01)   # dielectric fluid
    for y in range(7, 26, 4): m.box(6, y, 13, 20, y + 1, 14, (140, 146, 154))   # miner blades in the bath
    m.box(24, 6, 1, 31, 26, 18, (120, 128, 136))                      # dry cooler
    for y in (11, 21): m.cyl(27.5, y, 3, 18, 19, (40, 44, 50))
    m.hcyl('x', 22, 25, 10, 8, 1.2, STEEL_DK)
    return m


def b_shed():
    m = Model(1, 1, 36, res=T, seed=87); pad(m)
    wall = (196, 192, 180)
    m.box(4, 6, 1, 28, 26, 16, wall, var=((182, 178, 168), 0.2))
    for k in range(7):                                              # pitched roof
        m.box(3, 5 + k, 16 + k, 29, 6 + k, 17 + k, (150, 60, 50))
        m.box(3, 26 - k, 16 + k, 29, 27 - k, 17 + k, (150, 60, 50))
    m.box(3, 12, 22, 29, 20, 23, (130, 52, 44))
    m.box(12, 26, 1, 20, 26.6, 12, (110, 116, 124))                 # roller door
    m.box(28, 10, 7, 28.6, 16, 12, (255, 214, 140), glow=0.6)       # lit window
    return m


def b_greenhouse():
    m = Model(2, 2, 40, res=T, seed=88)
    m.box(0, 0, 0, 64, 64, 1, (140, 132, 104))
    for x0 in (2, 22, 42):
        m.box(x0, 2, 1, x0 + 18, 62, 12, GLASS, jit=0.02, var=((172, 214, 224), 0.3))
        for k in range(6):
            m.box(x0 + k * 1.5, 2, 12 + k, x0 + 18 - k * 1.5, 62, 13 + k, GLASS, jit=0.02, var=((190, 226, 230), 0.4))
        for y in range(2, 62, 6): m.box(x0, y, 1, x0 + 18, y + 0.8, 18, (226, 230, 232))
        # plants seen through the roof
        for y in range(5, 60, 6):
            m.box(x0 + 3, y, 12, x0 + 15, y + 3, 13, (86, 150, 62), jit=0.1)
    m.box(60, 20, 1, 64, 32, 10, (110, 114, 120))                  # heat exchanger
    return m


def b_deepgeo():
    m = Model(2, 2, 110, res=T, seed=89); pad(m)
    lattice(m, 6, 6, 24, 24, 1, 100, (196, 70, 52), step=8, r=0.9)  # derrick
    m.box(4, 4, 1, 26, 26, 6, STEEL_DK)
    m.box(30, 30, 1, 58, 58, 20, (190, 196, 204))                  # binary plant
    m.box(30, 30, 20, 58, 58, 22, (140, 150, 160))
    m.hcyl('x', 24, 44, 20, 8, 2.4, (200, 200, 196))
    m.hcyl('y', 20, 30, 44, 8, 2.4, (200, 200, 196))
    for (x, y) in ((40, 10), (52, 14)):
        m.cyl(x, y, 3, 1, 26, (220, 220, 214))
        m.emit.append((x, y, 28))
    return m


def b_orbital():
    m = Model(2, 2, 70, res=T, seed=90); pad(m, (126, 132, 102))
    for y in range(4, 62, 5):          # rectenna mesh
        for x in range(4, 62, 5):
            if (x - 32) ** 2 + (y - 32) ** 2 < 28 ** 2:
                m.box(x, y, 1, x + 4, y + 4, 3, (150, 160, 176), jit=0.02, var=((120, 132, 152), 0.3))
    lattice(m, 26, 26, 38, 38, 1, 36, STEEL, step=6, r=0.8)
    m.cyl(32, 32, 5, 36, 40, (220, 226, 236))
    m.cyl(32, 32, 2.5, 40, 42, (210, 245, 255), glow=1.2)
    return m


def worker(frame):
    m = Model(1, 1, 16, res=T, seed=91 + frame)
    cx = 16
    l1, l2 = (0, 1) if frame == 0 else (1, 0)
    m.box(cx - 1, cx - 1 + l1, 0, cx, cx + l1, 4, (50, 54, 60))          # legs
    m.box(cx, cx - 1 + l2, 0, cx + 1, cx + l2, 4, (50, 54, 60))
    m.box(cx - 1, cx - 1, 4, cx + 1, cx + 1, 8, (240, 130, 40))           # hi-vis vest
    m.box(cx - 1, cx - 1, 8, cx + 1, cx + 1, 10, (230, 186, 150))         # head
    m.box(cx - 1.2, cx - 1.2, 10, cx + 1.2, cx + 1.2, 11, (250, 210, 50))  # hard hat
    return m


def truck():
    m = Model(2, 1, 30, res=T, seed=93)
    m.box(1, 1, 3, 36, 18, 18, (236, 236, 230), var=((222, 222, 216), 0.1))   # box body
    m.box(36, 2, 3, 46, 17, 14, (206, 64, 50))                                 # cab
    m.box(41, 3, 10, 46, 16, 13, (150, 196, 214), glow=0.1)                    # windscreen
    m.box(1, 1, 2, 46, 18, 3, (60, 60, 62))
    for x in (6, 30, 41):
        m.hcyl('y', 0, 2, x, 2.6, 2.6, RUBBER)
        m.hcyl('y', 17, 19, x, 2.6, 2.6, RUBBER)
    m.box(4, 18, 9, 30, 18.6, 13, (247, 147, 26))                              # stripe
    return m


def battery():
    """Battery bank: two white utility-scale battery cabinets and an inverter on a pad."""
    m = Model(1, 1, 26, res=T, seed=94); pad(m)
    for y0 in (4, 17):
        m.box(3, y0, 1, 25, y0 + 11, 14, (236, 238, 238), jit=0.015, var=((226, 228, 230), 0.15))
        m.box(3, y0, 14, 25, y0 + 11, 15, (210, 214, 216))
        for x in range(5, 24, 3):                      # door seams + vents on the visible long side
            m.box(x, y0 + 11, 2, x + 0.6, y0 + 11.5, 13, (190, 194, 198))
        m.box(4, y0 + 11, 10.5, 24, y0 + 11.5, 11.5, (46, 160, 96))   # green stripe
        m.box(25, y0 + 2, 9, 25.6, y0 + 4, 11, (80, 240, 140), glow=1.0)  # status light
    m.box(26.5, 9, 1, 31, 22, 9, (150, 156, 162))      # inverter / transformer
    for y in (11, 15, 19): m.box(31, y, 3, 31.5, y + 2, 7, (110, 116, 122))
    return m


# ---------------------------------------------------------------- the full list, in atlas order
def all_sprites():
    """Yield (name, model, kind) with kind in {'ground', 'tree', 'point', 'building'}."""
    main, mv = GROUND[0][0], GROUND[0][1]
    yield 'G_GRASS', ground(main, var=(mv, 0.32), seed=1), 'ground'
    yield 'G_ROUGH', ground(main, var=(mv, 0.32), seed=2, patch=((128, 112, 84), (8, 8, 4.2, 3.6)), stones=((150, 146, 138), 3)), 'ground'
    yield 'G_WATER', ground((48, 100, 138), var=((56, 112, 150), 0.25), seed=3, water=True), 'ground'
    yield 'G_CONCRETE', ground((176, 172, 164), var=((166, 162, 154), 0.2), seed=4, seams=True), 'ground'
    yield 'G_COAL', ground((58, 54, 52), var=((46, 44, 42), 0.35), seed=5, lumps=((30, 30, 30), 14)), 'ground'
    for site in range(1, 6):
        for k, t in enumerate(site_tiles(site)):
            yield f'G_S{site}_{k}', t, 'ground'
    for k in range(11):
        yield f'TREE_{k}', tree(k), 'tree'
    yield 'B_GRID', b_grid(), 'building'
    yield 'B_DIESEL', b_diesel(), 'building'
    yield 'B_GAS', b_gas(), 'building'
    yield 'B_COAL', b_coal(), 'building'
    yield 'B_SOLAR', b_solar(), 'building'
    for f in range(3):
        yield f'B_WIND{f}', b_wind(f * 2 * math.pi / 9), 'building'
    yield 'B_HYDRO', b_hydro(), 'building'
    yield 'B_FISSION', b_fission(), 'building'
    yield 'B_FUSION', b_fusion(), 'building'
    yield 'B_CORE', b_core(), 'building'
    for g, c in enumerate(GENS): yield f'B_MINER{g + 1}', miner(c, 73 + g), 'building'
    for g, c in enumerate(GOLDS): yield f'B_MINERG{g + 1}', miner(c, 77 + g), 'building'
    yield 'B_FAN', fanwall(True), 'building'
    yield 'B_FAN2', fanwall(False), 'building'
    yield 'B_POLE', b_pole(), 'building'
    yield 'B_WALL', b_wall_thumb(), 'building'
    yield 'B_SIGN', b_sign(), 'building'
    yield 'B_IMMERSION', b_immersion(), 'building'
    yield 'B_SHED', b_shed(), 'building'
    yield 'B_GREENHOUSE', b_greenhouse(), 'building'
    yield 'B_DEEPGEO', b_deepgeo(), 'building'
    yield 'B_ORBITAL', b_orbital(), 'building'
    yield 'WORKER0', worker(0), 'point'
    yield 'WORKER1', worker(1), 'point'
    yield 'TRUCK', truck(), 'building'
    yield 'B_BATTERY', battery(), 'building'
