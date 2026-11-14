"""Tiny voxel modeller + offline renderer for Power Grid's sprites.

Models are boolean occupancy grids with a colour per voxel. `render()` splats every visible voxel face
into an isometric image (the game's 2:1 projection) with a z-buffer and lights it like a small
path-traced diorama: warm sun with soft shadows, cool sky light with ambient occlusion, a contact
shadow on the ground around the model, 2x supersampling. No outlines, no dithering.

Scale: the atlas is drawn at 2x the game's base tile, so one tile is 128x64 px. A model's `res` is the
number of voxels per tile edge (16 = chunky, 32 = detailed). One voxel of height is 80/res px.
Anchor convention (same as tools/build_atlas.py): a sprite is drawn at P(origin tile) + (ax, ay), where P is
the screen position of the top corner of the tile at ground level.
"""
import numpy as np
from PIL import Image

SSAA = 2
TILE_W, TILE_H, TILE_Z = 128, 64, 80          # atlas px per tile along the screen diagonals, and per tile of height
VIEW = np.array([1.0, 1.0, 0.8])              # toward the camera (null space of the projection)
SUN = np.array([0.18, 0.72, 0.90]); SUN /= np.linalg.norm(SUN)
SUN_COL = np.array([1.00, 0.91, 0.78]) * 1.02
SKY_COL = np.array([0.66, 0.78, 1.00]) * 0.36
GROUND_BOUNCE = np.array([0.55, 0.50, 0.40]) * 0.08


def srgb(c):
    c = np.asarray(c, np.float32)
    if c.max() > 1.0: c = c / 255.0
    return c


def to_lin(c): return np.power(np.clip(c, 0, 1), 2.2)


def to_srgb(c): return np.power(np.clip(c, 0, 1), 1 / 2.2)


def aces(x):
    """Linear below 0.7, then a soft shoulder (keeps base colours honest, rolls off highlights)."""
    k = 0.7
    return np.clip(np.where(x < k, x, k + (1 - k) * (1 - np.exp(-(x - k) / (1 - k)))), 0, 1)


class Model:
    def __init__(self, w=1, d=1, h=64, res=32, zbase=0, seed=1):
        self.res = res
        self.W, self.D, self.H = int(round(w * res)), int(round(d * res)), int(h)
        self.occ = np.zeros((self.W, self.D, self.H), bool)
        self.col = np.zeros((self.W, self.D, self.H, 3), np.float32)
        self.glow = np.zeros((self.W, self.D, self.H), np.float32)
        self.zbase = zbase           # voxel z of the ground surface (ground slabs sit below it)
        self.emit = []               # smoke emitters, voxel coords
        self.catcher = True          # cast a shadow on the ground around the model
        self.rng = np.random.default_rng(seed)
        g = np.mgrid[0:self.W, 0:self.D, 0:self.H].astype(np.float32) + 0.5
        self.X, self.Y, self.Z = g[0], g[1], g[2]

    # ------------------------------------------------------------ painting
    def fill(self, mask, c, jit=0.035, glow=0.0, var=None):
        """Set voxels in `mask` to colour c (sRGB 0-255 or 0-1) with a little per-voxel variation."""
        if not mask.any(): return
        c = srgb(c)
        n = mask.sum()
        k = 1.0 + self.rng.normal(0, jit, n).astype(np.float32)
        cols = np.clip(c[None, :] * k[:, None], 0, 1)
        if var is not None:          # a second colour mixed in at random (weathering, mottling)
            c2, p = srgb(var[0]), var[1]
            pick = self.rng.random(n) < p
            cols[pick] = np.clip(c2[None, :] * k[pick, None], 0, 1)
        self.occ[mask] = True
        self.col[mask] = cols
        self.glow[mask] = glow
        return mask

    def clear(self, mask): self.occ[mask] = False

    def paint(self, mask, c, jit=0.03, glow=0.0):
        """Recolour voxels that already exist."""
        return self.fill(mask & self.occ, c, jit, glow)

    def box(self, x0, y0, z0, x1, y1, z1, c, **kw):
        m = (self.X >= x0) & (self.X < x1) & (self.Y >= y0) & (self.Y < y1) & (self.Z >= z0) & (self.Z < z1)
        return self.fill(m, c, **kw)

    def boxm(self, x0, y0, z0, x1, y1, z1):
        return (self.X >= x0) & (self.X < x1) & (self.Y >= y0) & (self.Y < y1) & (self.Z >= z0) & (self.Z < z1)

    def cyl(self, cx, cy, r, z0, z1, c, hollow=0, **kw):
        d2 = (self.X - cx) ** 2 + (self.Y - cy) ** 2
        m = (d2 <= r * r) & (self.Z >= z0) & (self.Z < z1)
        if hollow: m &= d2 > (r - hollow) ** 2
        return self.fill(m, c, **kw)

    def cylm(self, cx, cy, r, z0, z1):
        return (((self.X - cx) ** 2 + (self.Y - cy) ** 2) <= r * r) & (self.Z >= z0) & (self.Z < z1)

    def lathe(self, cx, cy, z0, z1, rfun, c, hollow=0, **kw):
        """Solid of revolution: radius rfun(t) for t in 0..1 from z0 to z1."""
        t = np.clip((self.Z - z0) / max(1e-6, z1 - z0), 0, 1)
        r = rfun(t)
        d2 = (self.X - cx) ** 2 + (self.Y - cy) ** 2
        m = (d2 <= r * r) & (self.Z >= z0) & (self.Z < z1)
        if hollow: m &= d2 > (r - hollow) ** 2
        return self.fill(m, c, **kw)

    def hcyl(self, axis, a0, a1, cu, cz, r, c, **kw):
        """Horizontal cylinder along x (axis='x') or y, centred at (cu, cz) in the other two axes."""
        if axis == 'x':
            m = (self.X >= a0) & (self.X < a1) & (((self.Y - cu) ** 2 + (self.Z - cz) ** 2) <= r * r)
        else:
            m = (self.Y >= a0) & (self.Y < a1) & (((self.X - cu) ** 2 + (self.Z - cz) ** 2) <= r * r)
        return self.fill(m, c, **kw)

    def ellipsoid(self, cx, cy, cz, rx, ry, rz, c, noise=0.0, **kw):
        q = ((self.X - cx) / rx) ** 2 + ((self.Y - cy) / ry) ** 2 + ((self.Z - cz) / rz) ** 2
        if noise:
            q = q + self.rng.normal(0, noise, q.shape).astype(np.float32)
        return self.fill(q <= 1.0, c, **kw)

    def sphere(self, cx, cy, cz, r, c, **kw): return self.ellipsoid(cx, cy, cz, r, r, r, c, **kw)

    def line(self, p0, p1, r, c, **kw):
        """A rod of radius r between two points (voxel coords)."""
        p0, p1 = np.array(p0, np.float32), np.array(p1, np.float32)
        d = p1 - p0; L2 = max(1e-6, float(d @ d))
        t = np.clip(((self.X - p0[0]) * d[0] + (self.Y - p0[1]) * d[1] + (self.Z - p0[2]) * d[2]) / L2, 0, 1)
        dx = self.X - (p0[0] + t * d[0]); dy = self.Y - (p0[1] + t * d[1]); dz = self.Z - (p0[2] + t * d[2])
        return self.fill(dx * dx + dy * dy + dz * dz <= r * r, c, **kw)

    def pad(self, c=(150, 148, 142), h=1, inset=0, **kw):
        """A flat slab (concrete pad) over the whole footprint."""
        return self.box(inset, inset, 0, self.W - inset, self.D - inset, h, c, **kw)


# ---------------------------------------------------------------- face stamps
FACES = {   # normal, base offset, U, V (unit voxel vectors)
    'top': (np.array([0, 0, 1.]), np.array([0, 0, 1.]), np.array([1, 0, 0.]), np.array([0, 1, 0.])),
    'px':  (np.array([1, 0, 0.]), np.array([1, 0, 0.]), np.array([0, 1, 0.]), np.array([0, 0, 1.])),
    'py':  (np.array([0, 1, 0.]), np.array([0, 1, 0.]), np.array([1, 0, 0.]), np.array([0, 0, 1.])),
}


def steps(res):
    ex = TILE_W / 2 / res * SSAA; ey = TILE_H / 2 / res * SSAA; ez = TILE_Z / res * SSAA
    assert ex == int(ex) and ey == int(ey) and ez == int(ez), (res, ex, ey, ez)
    return int(ex), int(ey), int(ez)


def pvec(v, st):
    ex, ey, ez = st
    return np.array([ex * (v[0] - v[1]), ey * (v[0] + v[1]) - ez * v[2]], np.float64)


_stamp_cache = {}


def stamp(face, st):
    key = (face, st)
    if key in _stamp_cache: return _stamp_cache[key]
    _, _, U, V = FACES[face]
    su, sv = pvec(U, st), pvec(V, st)
    pts = np.array([[0, 0], su, sv, su + sv])
    x0, y0 = np.floor(pts.min(0)).astype(int) - 1
    x1, y1 = np.ceil(pts.max(0)).astype(int) + 1
    gx, gy = np.meshgrid(np.arange(x0, x1), np.arange(y0, y1), indexing='ij')
    cx, cy = gx + 0.5, gy + 0.5
    M = np.array([su, sv]).T
    inv = np.linalg.inv(M)
    a = inv[0, 0] * cx + inv[0, 1] * cy
    b = inv[1, 0] * cx + inv[1, 1] * cy
    e = 0.06
    keep = (a >= -e) & (a <= 1 + e) & (b >= -e) & (b <= 1 + e)
    out = (gx[keep], gy[keep], np.clip(a[keep], 0, 1), np.clip(b[keep], 0, 1))
    _stamp_cache[key] = out
    return out


# ---------------------------------------------------------------- lighting helpers
def _occ_lookup(P, pts):
    """Occupancy at float points (N,3) in a padded grid P (pad 1 voxel); outside = empty."""
    idx = np.floor(pts).astype(np.int64) + 1
    W, D, H = P.shape
    ok = (idx[:, 0] >= 0) & (idx[:, 0] < W) & (idx[:, 1] >= 0) & (idx[:, 1] < D) & (idx[:, 2] >= 0) & (idx[:, 2] < H)
    out = np.zeros(len(pts), bool)
    i = idx[ok]
    out[ok] = P[i[:, 0], i[:, 1], i[:, 2]]
    return out


def _march(P, pts, d, n, step):
    """Fraction of rays from pts along direction d that escape (1 = clear) within n steps."""
    alive = np.ones(len(pts), bool)
    for s in range(1, n + 1):
        q = pts + d[None, :] * (s * step)
        hit = _occ_lookup(P, q)
        alive &= ~hit
        if not alive.any(): break
    return alive


def _hemi_dirs(n, count=14, seed=3):
    rng = np.random.default_rng(seed)
    out = []
    while len(out) < count:
        v = rng.normal(size=3); v /= np.linalg.norm(v)
        if v @ n < 0.15: continue
        out.append(v)
    return out


SUN_DIRS = []
_r = np.random.default_rng(7)
for _k in range(6):
    v = SUN + _r.normal(0, 0.045, 3); SUN_DIRS.append(v / np.linalg.norm(v))
SKY_DIRS = {f: _hemi_dirs(FACES[f][0], 14, seed=k) for k, f in enumerate(FACES)}


def _light_points(P, pts, face, max_len):
    """Sun visibility (0..1) and sky visibility (0..1) for points on a face of the given orientation."""
    n = FACES[face][0]
    p = pts + n[None, :] * 0.06
    sun = np.zeros(len(p))
    if n @ SUN > 0:
        for d in SUN_DIRS:
            sun += _march(P, p, d, int(max_len / 0.7) + 2, 0.7)
        sun /= len(SUN_DIRS)
    sky = np.zeros(len(p))
    for d in SKY_DIRS[face]:
        sky += _march(P, p, d, 10, 0.8) * max(0.2, d @ n)
    sky /= sum(max(0.2, d @ n) for d in SKY_DIRS[face])
    return sun, sky


def _vertex_ao(P, fx, fy, fz, face):
    """Classic voxel vertex AO for the 4 corners (a,b) in {00,10,01,11}; 1 = open, ~0.45 = deep corner."""
    n, base, U, V = FACES[face]
    n = n.astype(int); U = U.astype(int); V = V.astype(int)
    out = []
    W, D, H = P.shape
    def occ(ix, iy, iz):
        ix = ix + 1; iy = iy + 1; iz = iz + 1
        ok = (ix >= 0) & (ix < W) & (iy >= 0) & (iy < D) & (iz >= 0) & (iz < H)
        r = np.zeros(len(ix), bool); r[ok] = P[ix[ok], iy[ok], iz[ok]]
        return r
    for (a, b) in ((0, 0), (1, 0), (0, 1), (1, 1)):
        sa = 1 if a else -1; sb = 1 if b else -1
        o = np.array([fx, fy, fz]) + n[:, None]
        s1 = occ(*(o + sa * U[:, None]))
        s2 = occ(*(o + sb * V[:, None]))
        cc = occ(*(o + sa * U[:, None] + sb * V[:, None]))
        v = np.where(s1 & s2, 0, 3 - (s1.astype(int) + s2 + cc))
        out.append(v)
    ao = np.stack(out, 1).astype(np.float32)   # N x 4, 0..3
    return 0.5 + 0.5 * (ao / 3.0) ** 0.8


# ---------------------------------------------------------------- render
def render(m, catcher_margin=None, shadow_alpha=0.42, solid=False):
    """Render a model. Returns (RGBA image at atlas scale, ax, ay, emitters[(x, y)]) with ax/ay/size even."""
    st = steps(m.res)
    ex, ey, ez = st
    P = np.pad(m.occ, 1)
    occ = m.occ
    frags = []      # (px, py, depth, rgb_lin(3), alpha)
    max_len = (m.W + m.D + m.H) * 1.2
    zb = m.zbase
    lin = to_lin(m.col)

    def emit_face(face, mask):
        fx, fy, fz = np.nonzero(mask)
        if len(fx) == 0: return
        n, base, U, V = FACES[face]
        bx, by, bz = fx + base[0], fy + base[1], fz + base[2]
        # corners (a,b): 00, 10, 01, 11
        cs = []
        for (a, b) in ((0, 0), (1, 0), (0, 1), (1, 1)):
            cs.append(np.stack([bx + a * U[0] + b * V[0], by + a * U[1] + b * V[1], bz + a * U[2] + b * V[2]], 1))
        allc = np.concatenate(cs, 0)
        uniq, inv = np.unique(allc, axis=0, return_inverse=True)
        sun_u, sky_u = _light_points(P, uniq.astype(np.float64), face, max_len)
        N = len(fx)
        inv = inv.reshape(-1)
        sun_c = sun_u[inv].reshape(4, N).T
        sky_c = sky_u[inv].reshape(4, N).T
        ao_c = _vertex_ao(P, fx, fy, fz, face)
        ndl = max(0.0, float(n @ SUN))
        amb = 1.0 if face == 'top' else 0.78
        base_lin = lin[fx, fy, fz]                        # N x 3
        glow = m.glow[fx, fy, fz]
        sx, sy, sa, sb = stamp(face, st)
        # base pixel of each face
        px0 = ex * (bx - by); py0 = ey * (bx + by) - ez * (bz - zb)
        PX = px0[:, None] + sx[None, :]; PY = py0[:, None] + sy[None, :]
        w00 = (1 - sa) * (1 - sb); w10 = sa * (1 - sb); w01 = (1 - sa) * sb; w11 = sa * sb
        Wt = np.stack([w00, w10, w01, w11], 0)                       # 4 x Ps
        sun_f = sun_c @ Wt; sky_f = sky_c @ Wt; ao_f = ao_c @ Wt      # N x Ps
        light = (SKY_COL[None, None, :] * (sky_f * ao_f * amb)[..., None]
                 + SUN_COL[None, None, :] * (ndl * sun_f * (0.55 + 0.45 * ao_f))[..., None]
                 + (GROUND_BOUNCE[None, None, :] * ao_f[..., None] if face != 'top' else 0))
        rgb = base_lin[:, None, :] * light + (base_lin * glow[:, None] * 3.0)[:, None, :]
        depth = (bx[:, None] + sa[None, :] * U[0] + sb[None, :] * V[0]) * VIEW[0] + \
                (by[:, None] + sa[None, :] * U[1] + sb[None, :] * V[1]) * VIEW[1] + \
                (bz[:, None] + sa[None, :] * U[2] + sb[None, :] * V[2]) * VIEW[2]
        frags.append((PX.ravel(), PY.ravel(), depth.ravel(), rgb.reshape(-1, 3), np.ones(PX.size)))

    top = occ & ~P[1:-1, 1:-1, 2:]
    pxm = occ & ~P[2:, 1:-1, 1:-1]
    pym = occ & ~P[1:-1, 2:, 1:-1]
    emit_face('top', top); emit_face('px', pxm); emit_face('py', pym)

    # ground shadow catcher: semi-transparent darkening on the ground plane around (and under) the model
    if m.catcher:
        M = catcher_margin if catcher_margin is not None else int(min(m.H * 1.3, m.res * 2))
        gx, gy = np.meshgrid(np.arange(-M, m.W + M), np.arange(-M, m.D + M), indexing='ij')
        gx, gy = gx.ravel(), gy.ravel()
        inside = (gx >= 0) & (gx < m.W) & (gy >= 0) & (gy < m.D)
        covered = np.zeros(len(gx), bool)
        covered[inside] = occ[gx[inside], gy[inside], zb] if zb < m.H else False
        gx, gy = gx[~covered], gy[~covered]
        if len(gx):
            cs = [np.stack([gx + a, gy + b, np.full(len(gx), float(zb))], 1) for (a, b) in ((0, 0), (1, 0), (0, 1), (1, 1))]
            allc = np.concatenate(cs, 0)
            uniq, inv = np.unique(allc, axis=0, return_inverse=True)
            sun_u, sky_u = _light_points(P, uniq.astype(np.float64), 'top', max_len)
            inv = inv.reshape(-1)
            N = len(gx)
            shade = (1 - sun_u[inv].reshape(4, N).T) * 0.85 + (1 - sky_u[inv].reshape(4, N).T) * 0.5
            keep = shade.max(1) > 0.03
            if keep.any():
                gx, gy, shade = gx[keep], gy[keep], shade[keep]
                sx, sy, sa, sb = stamp('top', st)
                bz = zb
                px0 = ex * (gx - gy); py0 = ey * (gx + gy)
                PX = px0[:, None] + sx[None, :]; PY = py0[:, None] + sy[None, :]
                Wt = np.stack([(1 - sa) * (1 - sb), sa * (1 - sb), (1 - sa) * sb, sa * sb], 0)
                a = np.clip(shade @ Wt, 0, 1) * shadow_alpha
                frags.append((PX.ravel(), PY.ravel(), np.full(PX.size, -1e9), np.zeros((PX.size, 3)), a.ravel()))

    # resolve: z-buffer by sorting (nearest last wins)
    PX = np.concatenate([f[0] for f in frags]).astype(np.int64)
    PY = np.concatenate([f[1] for f in frags]).astype(np.int64)
    DP = np.concatenate([f[2] for f in frags])
    RGB = np.concatenate([f[3] for f in frags])
    AL = np.concatenate([f[4] for f in frags])
    # origin (P of the model's origin tile at ground level) must land on a multiple of 2*SSAA so that the atlas
    # anchor is even (the half-scale atlas is a plain 2x2 box filter of the full one)
    q = 2 * SSAA
    x0 = (PX.min() // q) * q - q; y0 = (PY.min() // q) * q - q
    w = ((PX.max() - x0) // q + 2) * q; h = ((PY.max() - y0) // q + 2) * q
    order = np.argsort(DP, kind='stable')
    img = np.zeros((h, w, 4), np.float32)
    ix = PX[order] - x0; iy = PY[order] - y0
    col = aces(RGB[order] * 1.0)
    img[iy, ix, :3] = to_srgb(col)
    img[iy, ix, 3] = AL[order]
    # downsample (premultiplied)
    pm = img.copy(); pm[..., :3] *= pm[..., 3:4]
    pm = pm.reshape(h // SSAA, SSAA, w // SSAA, SSAA, 4).mean((1, 3))
    a = pm[..., 3:4]
    rgb = np.where(a > 1e-4, pm[..., :3] / np.maximum(a, 1e-4), 0)
    if solid:   # ground tiles: no soft edges, or the darker slab sides of the tiles behind bleed through the seams
        a = np.where(a > 0.02, 1.0, 0.0)
    out = np.concatenate([rgb, a], -1)
    out8 = (np.clip(out, 0, 1) * 255 + 0.5).astype(np.uint8)
    im = Image.fromarray(out8, 'RGBA')
    ox, oy = -x0 // SSAA, -y0 // SSAA          # origin pixel in the downsampled image
    bb = im.getbbox()
    if bb is None: return None
    bx0 = bb[0] - (bb[0] - ox) % 2; by0 = bb[1] - (bb[1] - oy) % 2
    bx1 = bb[2] + (bb[2] - bx0) % 2; by1 = bb[3] + (bb[3] - by0) % 2
    im = im.crop((bx0, by0, bx1, by1))
    ax, ay = bx0 - ox, by0 - oy
    em = []
    for (x, y, z) in m.emit:
        sx_ = (ex * (x - y)) / SSAA; sy_ = (ey * (x + y) - ez * (z - zb)) / SSAA
        em.append((int(round(sx_)), int(round(sy_))))
    return im, int(ax), int(ay), em
