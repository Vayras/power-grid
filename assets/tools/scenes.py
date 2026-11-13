import sys, os
sys.path.insert(0, '/home/claude/pg/gen')
from ttdmap import TTDMap, IM, GRASS, WATER
from PIL import Image, ImageEnhance

OUT = '/home/claude/pg/render'
os.makedirs(OUT, exist_ok=True)

def save(img, name):
    img.save(f'{OUT}/{name}.png', optimize=True)
    print(name, img.size)

def pylons(m, pts):
    out = []
    for (i, j) in pts:
        x, y = m.P(i, j)
        out.append((x, y, 30))
        m.reserve(i, j, 0.2, 0.2)
    m.pylon_line(out)

def coal_plant(m, i, j):
    m.coalground(i, j, 3, 2)
    m.sprite(IM(137), i, j, 1, 1)
    m.sprite(IM(134), i + 1, j, 1, 1)
    m.sprite(IM(134), i + 2, j, 1, 1)
    m.sprite(IM(140), i, j + 1, 1, 1)
    x, y = m.P(i + 0.55, j + 0.2)
    m.smoke(x + 2, y - 52, 6)
    for k in (1, 2):
        x, y = m.P(i + k + 0.5, j + 0.5)
        m.smoke(x, y - 40, 4, drift=0)

def gas_plant(m, i, j):
    m.concrete(i, j, 2, 2)
    m.sprite(IM(150), i, j, 1, 1); m.sprite(IM(149), i + 1, j, 1, 1)
    m.sprite(IM(162), i, j + 1, 1, 1); m.sprite(IM(156), i + 1, j + 1, 1, 1)

def mining_yard(m, i, j, rows=3, cols=4, hot=None):
    m.concrete(i, j, cols, rows * 1 + 1)
    for r in range(rows):
        for c in range(cols):
            m.container(i + c + 0.06, j + r * 1.0 + 0.25, hot=(hot == (r, c)), w=0.88)
    for c in range(cols):
        m.fanwall(i + c, j + rows + 0.1)

def title_map(seed):
    m = TTDMap(seed, origin=(330, -150))
    m.terrain(lake=(15, 2, 19, 6), trees=0.14)
    coal_plant(m, 8, 7); gas_plant(m, 12, 9); mining_yard(m, 9, 11, rows=2, cols=4)
    m.immersion(13, 12); m.immersion(14, 12); m.transformer(12, 12)
    m.sprite(IM(165), 6, 11, 1, 1)
    pylons(m, [(11, 8.5), (11.5, 11), (8.5, 10.6)])
    for w in [(11.2, 10.2), (9.5, 13.6), (13.4, 11.4)]: m.worker(*w)
    m.place_trees()
    return m.render()

def garage_map(seed):
    m = TTDMap(seed, origin=(320, -150))
    m.terrain(lake=(3, 12, 6, 15), trees=0.16)
    m.concrete(10, 10, 2, 2)
    m.sprite(IM(124), 10, 10, 1, 1)           # the garage shed
    m.container(11, 10.3)
    m.transformer(11, 11)
    pylons(m, [(12.4, 11.4), (14.4, 11.4), (16.4, 11.4)])
    m.worker(10.6, 11.6)
    m.reserve(9, 9, 4, 4)
    m.place_trees()
    m.fog(9, 9, 13, 13)
    return m.render()

def plant_map(seed, heat=False):
    m = TTDMap(seed, origin=(300, -170))
    m.terrain(lake=(17, 4, 21, 8), trees=0.12)
    coal_plant(m, 7, 7)
    gas_plant(m, 11, 6)
    m.diesel(10, 9)
    m.solar(4, 12, 2, 1); m.solar(4, 13, 2, 1)
    mining_yard(m, 9, 11, rows=3, cols=4, hot=(1, 2))
    m.immersion(13, 11); m.immersion(13, 12); m.transformer(13, 13)
    m.sprite(IM(165), 14, 10, 1, 1)
    pylons(m, [(10.5, 8.6), (12.6, 10.4), (8.4, 10.6)])
    for w in [(10.8, 10.4), (12.6, 14.6), (8.6, 11.5), (13.8, 13.8)]: m.worker(*w)
    m.ghost(15, 12, 2, 2)
    m.place_trees()
    img = m.render()
    if heat:
        warm = Image.new('RGB', img.size, (255, 150, 60))
        img = Image.blend(img, warm, 0.18)
        img = ImageEnhance.Contrast(img).enhance(1.05)
    return img

def late_map(seed):
    m = TTDMap(seed, origin=(300, -170))
    m.terrain(lake=(4, 4, 7, 8), trees=0.1)
    m.concrete(9, 6, 4, 2)
    m.reactor(9, 6); m.sprite(IM(134), 11, 6, 1, 1); m.sprite(IM(134), 12, 6, 1, 1)
    x, y = m.P(11.5, 6.5); m.smoke(x, y - 40, 5, 0)
    x, y = m.P(12.5, 6.5); m.smoke(x, y - 40, 5, 0)
    for k in range(3): m.wind(15 + k * 1.3, 5 + k * 0.4, phase=k * 0.7)
    mining_yard(m, 8, 10, rows=3, cols=6)
    for k in range(4): m.immersion(14, 10 + k)
    m.transformer(15, 10); m.transformer(15, 11)
    m.sprite(IM(165), 6, 10, 1, 1); m.sprite(IM(165), 6, 11, 1, 1)
    pylons(m, [(13.4, 8.4), (14.6, 9.6), (7.6, 9.4)])
    for w in [(10, 13.8), (12, 13.6), (14.6, 12.4)]: m.worker(*w)
    m.place_trees()
    return m.render()

def vignette(kind, seed=3):
    m = TTDMap(seed, origin=(80, -2), w=160, h=120)
    m.terrain(trees=0.0)
    m.ground = {(i, j): GRASS[0] for i in range(-3, 6) for j in range(-3, 6)}
    if kind == 'grid':
        m.transformer(1, 1); pylons(m, [(0.4, 0.6), (2.4, 0.6)])
    elif kind == 'diesel':
        m.diesel(1, 1); m.diesel(1.9, 1)
    elif kind == 'gas':
        gas_plant(m, 1, 1)
    elif kind == 'coal':
        m.coalground(1, 1, 2, 2); m.sprite(IM(137), 1, 1, 1, 1); m.sprite(IM(134), 2, 1, 1, 1)
        x, y = m.P(1.55, 1.2); m.smoke(x + 2, y - 52, 5)
    elif kind == 'solar':
        m.solar(0.5, 1, 3, 1); m.solar(0.5, 2, 3, 1)
    elif kind == 'wind':
        m.wind(1.2, 1.4, 0.3); m.wind(2.4, 1.8, 1.2)
    elif kind == 'hydro':
        for i in range(0, 4):
            for j in range(-3, 6):
                if j <= 1: m.ground[(i, j)] = WATER
        m.box(0, 1.6, 4, 0.5, 14, ((196, 190, 176), (164, 158, 144), (126, 120, 108)))
        m.transformer(2, 2.6)
    elif kind == 'nuclear':
        m.concrete(1, 1, 2, 2); m.reactor(1, 1); m.sprite(IM(134), 2, 1, 1, 1)
        x, y = m.P(2.5, 1.5); m.smoke(x, y - 40, 4, 0)
    elif kind == 'fusion':
        m.concrete(1, 1, 2, 2); m.tokamak(1, 1)
    elif kind == 'core':
        m.concrete(1, 1, 2, 2); m.core(1.5, 1.5)
    return m.render()

if __name__ == '__main__':
    SEED = 0x7F3A
    save(title_map(SEED), 'map_title')
    save(garage_map(SEED), 'map_garage')
    save(plant_map(SEED), 'map_plant')
    save(plant_map(SEED, heat=True), 'map_heat')
    save(late_map(SEED), 'map_late')
    for k in ['grid', 'diesel', 'gas', 'coal', 'solar', 'wind', 'hydro', 'nuclear', 'fusion', 'core']:
        save(vignette(k), 'pw_' + k)
