//! Software renderer: draws the isometric plant into an RGBA framebuffer that
//! JS shows on a <canvas> with nearest-neighbour scaling.
use alloc::vec::Vec;
use crate::data::*;
use crate::game::*;
use crate::math::*;
use crate::sprites as S;

pub const VIEW_AIR: u32 = 1;
pub const VIEW_GRID: u32 = 2;
pub const VIEW_SECURITY: u32 = 4;

pub struct Renderer {
    pub fb: Vec<u32>,
    pub w: i32,
    pub h: i32,
    pub cam_x: i32,
    pub cam_y: i32,
    pub frame: u32,
    pub tool: i32,
    pub tool_dir: u8,
    pub hover: (i32, i32),
    pub sel: i32,
    pub view: u32,
    pub atlas: Vec<u32>,
    list: Vec<(u32, u8, u16)>,
    puffs: Vec<(u8, i32, i32, u8, u8, u32)>, // kind, i, j, w, d, start frame
    trucks: Vec<(i32, i32, u32)>,            // delivery to tile (i, j), start frame
    raiders: Vec<(f32, f32, f32, f32, u32)>, // running from (i, j) towards (i, j), start frame
    pub gold: bool,                          // gold miner containers (a cosmetic)
    pub night: f32,                          // 0 = no day/night, 1 = full strength (JS lowers it at high speed)
}

const BG: u32 = 0xFF18_2414;
const TRUCK_FRAMES: u32 = 240;
const NET_COLS: [(u32, u32, u32); 6] = [(95, 230, 224), (255, 210, 63), (183, 230, 138), (255, 159, 67), (190, 140, 255), (120, 170, 255)];

#[inline] fn rgba(r: u32, g: u32, b: u32) -> u32 { 0xFF00_0000 | (b << 16) | (g << 8) | r }

#[inline]
fn blend(dst: u32, src: u32, a: u32) -> u32 {
    let ia = 256 - a;
    let r = ((src & 0xFF) * a + (dst & 0xFF) * ia) >> 8;
    let g = (((src >> 8) & 0xFF) * a + ((dst >> 8) & 0xFF) * ia) >> 8;
    let b = (((src >> 16) & 0xFF) * a + ((dst >> 16) & 0xFF) * ia) >> 8;
    0xFF00_0000 | (b << 16) | (g << 8) | r
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer { trucks: Vec::new(), raiders: Vec::new(), gold: false, night: 1.0, fb: Vec::new(), w: 0, h: 0, cam_x: 0, cam_y: 0, frame: 0, tool: -1, tool_dir: 0, hover: (-1, -1), sel: -1, view: 0,
                   atlas: Vec::new(), list: Vec::with_capacity(4096), puffs: Vec::with_capacity(64) }
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        let (w, h) = (imax(16, w), imax(16, h));
        self.cam_x += (self.w - w) / 2; self.cam_y += (self.h - h) / 2;
        self.w = w; self.h = h;
        self.fb.clear(); self.fb.resize((w * h) as usize, BG);
    }

    pub fn center_on(&mut self, i: f32, j: f32) {
        let (x, y) = world(i, j);
        self.cam_x = x - self.w / 2; self.cam_y = y - self.h / 2;
    }

    pub fn clamp_cam(&mut self) {
        let lo_x = -(MAP as i32) * 32 - self.w / 2; let hi_x = (MAP as i32) * 32 - self.w / 2;
        let lo_y = -self.h / 2; let hi_y = (MAP as i32) * 32 - self.h / 2;
        self.cam_x = self.cam_x.clamp(lo_x, hi_x); self.cam_y = self.cam_y.clamp(lo_y, hi_y);
    }

    pub fn pick(&self, sx: i32, sy: i32) -> (i32, i32) {
        let wx = (sx + self.cam_x) as f32; let wy = (sy + self.cam_y) as f32;
        let u = wx / 32.0; let v = wy / 16.0;
        (floor32((u + v) / 2.0), floor32((v - u) / 2.0))
    }

    pub fn to_screen(&self, i: f32, j: f32) -> (i32, i32) { let (x, y) = world(i, j); (x - self.cam_x, y - self.cam_y) }

    /// Blit a sprite. `alpha` 0..=256 fades it; `tint` (colour, amount 0..=256) pulls every pixel toward a colour.
    fn blit(&mut self, spr: usize, sx: i32, sy: i32, alpha: u32, tint: (u32, u32)) {
        let s = S::SPRITES[spr];
        let (w, h) = (s.w as i32, s.h as i32);
        let x0 = imax(0, sx); let y0 = imax(0, sy);
        let x1 = if sx + w < self.w { sx + w } else { self.w };
        let y1 = if sy + h < self.h { sy + h } else { self.h };
        if x0 >= x1 || y0 >= y1 { return; }
        let aw = S::ATLAS_W as i32;
        for y in y0..y1 {
            let src_row = ((s.y as i32 + y - sy) * aw + s.x as i32 - sx) as usize;
            let dst_row = (y * self.w) as usize;
            for x in x0..x1 {
                let mut p = self.atlas[src_row + x as usize];
                let a = p >> 24;
                if a == 0 { continue; }
                if tint.1 > 0 { p = blend(p, tint.0, tint.1); }
                let d = &mut self.fb[dst_row + x as usize];
                if alpha >= 256 && a >= 250 { *d = p | 0xFF00_0000; continue; }
                let aa = if alpha >= 256 { a + 1 } else { ((a + 1) * alpha) >> 8 };
                *d = blend(*d, p, aa);
            }
        }
    }

    fn tint_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, col: u32, a: u32) {
        let x0 = imax(0, x0); let y0 = imax(0, y0);
        let x1 = if x1 < self.w { x1 } else { self.w }; let y1 = if y1 < self.h { y1 } else { self.h };
        for y in y0..y1 { for x in x0..x1 { let d = &mut self.fb[(y * self.w + x) as usize]; *d = blend(*d, col, a); } }
    }

    fn px(&mut self, x: i32, y: i32, col: u32, a: u32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h { return; }
        let d = &mut self.fb[(y * self.w + x) as usize];
        *d = if a >= 256 { col } else { blend(*d, col, a) };
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, col: u32) { for yy in 0..h { for xx in 0..w { self.px(x + xx, y + yy, col, 256); } } }

    fn quad(&mut self, p: [(i32, i32); 4], col: u32, a: u32) {
        let mut ymin = p[0].1; let mut ymax = p[0].1;
        for q in p.iter() { if q.1 < ymin { ymin = q.1; } if q.1 > ymax { ymax = q.1; } }
        for y in imax(0, ymin)..(if ymax < self.h { ymax } else { self.h }) {
            let yf = y as f32 + 0.5;
            let mut xl = 1.0e9f32; let mut xr = -1.0e9f32;
            for k in 0..4 {
                let (a0, b0) = p[k]; let (a1, b1) = p[(k + 1) % 4];
                let (ya, yb) = (b0 as f32, b1 as f32);
                if (yf >= ya && yf < yb) || (yf >= yb && yf < ya) {
                    let t = (yf - ya) / (yb - ya);
                    let x = a0 as f32 + (a1 - a0) as f32 * t;
                    if x < xl { xl = x; } if x > xr { xr = x; }
                }
            }
            if xl > xr { continue; }
            for x in imax(0, xl as i32)..(if (xr as i32) < self.w { xr as i32 } else { self.w }) {
                let d = &mut self.fb[(y * self.w + x) as usize]; *d = blend(*d, col, a);
            }
        }
    }

    fn line(&mut self, a: (i32, i32), b: (i32, i32), col: u32, dash: bool) {
        let dx = b.0 - a.0; let dy = b.1 - a.1;
        let n = imax(iabs(dx), iabs(dy));
        if n == 0 { return; }
        for s in 0..=n {
            if dash && (s / 3) % 2 == 1 { continue; }
            self.px(a.0 + dx * s / n, a.1 + dy * s / n, col, 256);
        }
    }

    fn wire(&mut self, a: (i32, i32), b: (i32, i32), col: u32) {
        let n = imax(imax(iabs(b.0 - a.0), iabs(b.1 - a.1)), 1);
        let sag = imin(14, n / 6 + 2);
        let mut prev = a;
        for s in 1..=n {
            let t = s as f32 / n as f32;
            let x = a.0 + ((b.0 - a.0) as f32 * t) as i32;
            let y = a.1 + ((b.1 - a.1) as f32 * t) as i32 + (4.0 * sag as f32 * t * (1.0 - t)) as i32;
            if (x, y) != prev { self.px(x, y, col, 256); prev = (x, y); }
        }
    }

    fn footprint(&self, i: i32, j: i32, w: i32, d: i32) -> [(i32, i32); 4] {
        let f = |a: i32, b: i32| { let (x, y) = world(a as f32, b as f32); (x - self.cam_x, y - self.cam_y) };
        [f(i, j), f(i + w, j), f(i + w, j + d), f(i, j + d)]
    }

    fn outline(&mut self, i: i32, j: i32, w: i32, d: i32, col: u32, dash: bool) {
        let p = self.footprint(i, j, w, d);
        for q in 0..4 { self.line(p[q], p[(q + 1) % 4], col, dash); }
    }

    fn smoke(&mut self, x: i32, y: i32, seed: u32, dark: bool) {
        for k in 0..6u32 {
            let ph = (self.frame / 2 + k * 11 + seed * 7) % 66;
            let r = 1 + (ph / 14) as i32;
            let cx = x + (ph / 7) as i32; let cy = y - ph as i32;
            let a = 230u32.saturating_sub(ph * 3);
            let col = if dark { rgba(52, 48, 46) } else if ph < 20 { rgba(220, 220, 212) } else if ph < 44 { rgba(196, 196, 188) } else { rgba(168, 168, 160) };
            for dy in -r..=r { for dx in -r..=r {
                if dx * dx + dy * dy <= r * r && ((dx + dy + ph as i32) & 1 == 0 || ph < 30) { self.px(cx + dx, cy + dy, col, a); }
            } }
        }
    }

    fn fire(&mut self, cx: i32, top: i32, seed: u32) {
        // flickering pixel flames along the roof
        for k in 0..9i32 {
            let x = cx - 12 + k * 3;
            let h = 5 + (((self.frame / 3) as i32 + k * 7 + seed as i32 * 5) % 9);
            for yy in 0..h {
                let col = if yy < h / 3 { rgba(255, 236, 120) } else if yy < 2 * h / 3 { rgba(255, 150, 40) } else { rgba(220, 60, 30) };
                let wob = if (yy + k + (self.frame / 4) as i32) % 4 == 0 { 1 } else { 0 };
                self.px(x + wob, top - yy, col, 256);
                self.px(x + 1 + wob, top - yy + 1, col, 200);
            }
        }
        self.smoke(cx, top - 14, seed * 3 + 1, true);
    }

    // ------------------------------------------------------------ day and night
    /// 0 by day, up to ~0.5 at night, ramping at dawn (5-7) and dusk (17-20).
    fn darkness(&self, g: &Game) -> f32 {
        let h = ((g.block - g.start_block) % BLOCKS_PER_DAY) as f32 / 6.0;
        let d = if h < 5.0 || h >= 20.0 { 1.0 } else if h < 7.0 { (7.0 - h) / 2.0 } else if h < 17.0 { 0.0 } else { (h - 17.0) / 3.0 };
        d * 0.5 * self.night
    }
    fn day_night(&mut self, g: &Game) {
        let d = self.darkness(g);
        if d < 0.01 { return; }
        let k = (d * 256.0) as u32;
        let (kr, kg, kb) = (256 - k, 256 - k * 9 / 10, 256 - k * 6 / 10);
        for p in self.fb.iter_mut() {
            let c = *p;
            *p = 0xFF00_0000 | ((((c >> 16) & 255) * kb >> 8) << 16) | ((((c >> 8) & 255) * kg >> 8) << 8) | ((c & 255) * kr >> 8);
        }
        // lights come on
        let la = (d * 2.0 * 256.0) as u32;
        for b in g.blds.iter() {
            let k = KINDS[b.kind as usize];
            let (cx, cy) = self.to_screen(b.i as f32 + k.w as f32 * 0.5, b.j as f32 + k.d as f32 * 0.5);
            if cx < -60 || cx > self.w + 60 || cy < -60 || cy > self.h + 60 { continue; }
            let running = b.off == 0 && b.broken == 0 && g.net_frac(b) > 0.0;
            if is_miner(b.kind) {
                if running {
                    self.light(cx + 6, cy - 8, 7, rgba(90, 240, 230), la * 35 >> 8);
                    let blink = (self.frame / 8 + b.i as u32 * 3 + b.j as u32) % 5;
                    for s in 0..3 { if s != blink as i32 % 3 { self.px(cx + 3 + s * 3, cy - 7 + s, rgba(120, 255, 240), la); } }
                }
                if b.fire != 0 { self.light(cx, cy - 10, 16, rgba(255, 110, 40), la * 90 >> 8); }
            } else if b.kind == K_GREENHOUSE {
                self.light(cx, cy - 8, 34, rgba(255, 110, 200), la * 45 >> 8);
            } else if b.kind == K_SHED {
                self.light(cx - 4, cy - 10, 8, rgba(255, 200, 110), la * 70 >> 8);
            } else if is_power(b.kind) {
                self.light(cx, cy - 20, 12, rgba(255, 214, 120), la * 50 >> 8);
                self.px(cx, cy - 22, rgba(255, 230, 160), la);
            } else if b.kind == K_POLE && (b.i + b.j) % 3 == 0 {
                self.light(cx, cy - 24, 9, rgba(255, 220, 150), la * 40 >> 8);
            }
        }
    }
    /// A soft elliptical pool of light (blended over whatever is there).
    fn light(&mut self, x: i32, y: i32, r: i32, col: u32, a: u32) {
        if a == 0 { return; }
        for dy in -r / 2..=r / 2 { for dx in -r..=r {
            let q = dx * dx + 4 * dy * dy; if q > r * r { continue; }
            let f = 256 - (q * 256 / (r * r + 1)) as u32;
            self.px(x + dx, y + dy, col, (a * f) >> 8);
        } }
    }
    // ------------------------------------------------------------ deliveries and raiders
    fn truck_pos(&self, x: usize) -> (f32, f32) {
        let (i, j, st) = self.trucks[x]; let t = self.frame.wrapping_sub(st) as f32;
        let p = if t < 90.0 { t / 90.0 } else if t < 150.0 { 1.0 } else { 1.0 - (t - 150.0) / 90.0 };
        let p = p * p * (3.0 - 2.0 * p); // ease in and out
        (i as f32 - 11.0 + p * 9.0, j as f32 + 0.22) // parks with its tail at the box
    }
    fn draw_truck(&mut self, x: usize) {
        let (ti, tj) = self.truck_pos(x);
        let white = (rgba(232, 232, 226), rgba(196, 196, 190), rgba(150, 150, 146));
        let red = (rgba(214, 70, 52), rgba(176, 52, 40), rgba(130, 38, 30));
        self.iso_box(ti, tj, 1.1, 0.56, 10, white, 2);
        self.iso_box(ti + 1.14, tj + 0.04, 0.34, 0.48, 8, red, 2);
        for w in [0.15f32, 0.85, 1.3] { let (wx, wy) = self.to_screen(ti + w, tj + 0.56); self.rect(wx - 1, wy - 2, 3, 2, rgba(24, 24, 24)); }
    }
    fn raider_pos(&self, x: usize) -> (f32, f32, u32) {
        let (fi, fj, ti, tj, st) = self.raiders[x];
        if self.frame < st { return (fi, fj, 0); }
        let t = (self.frame - st) as f32 / 170.0;
        let a = if t > 0.75 { ((1.0 - t) * 4.0 * 256.0) as u32 } else { 256 };
        (fi + (ti - fi) * t, fj + (tj - fj) * t, a.min(256))
    }

    fn glow(&mut self, x: i32, y: i32, strength: u32) {
        for (r, a) in [(18i32, 40u32), (12, 60), (7, 90)] {
            let a = a * strength / 256;
            for dy in -r / 2..=r / 2 { for dx in -r..=r {
                if dx * dx + 4 * dy * dy <= r * r { self.px(x + dx, y + dy, rgba(255, 80, 50), a); }
            } }
        }
    }

    fn gusts(&mut self, cx: i32, cy: i32, strength: f32, seed: u32) {
        let a = (90.0 + strength * 6.0).min(230.0) as u32;
        let n = if strength > 15.0 { 4 } else if strength > 7.0 { 3 } else { 2 };
        for k in 0..n {
            let ph = ((self.frame + seed * 13 + k * 17) % 48) as i32;
            let x = cx - 16 + ph;
            let y = cy - 6 + (k as i32) * 4 - (ph / 12);
            let fade = if ph < 8 { ph as u32 * 32 } else if ph > 38 { (48 - ph) as u32 * 25 } else { 256 };
            let al = a * fade / 256;
            for d in 0..6 { self.px(x + d, y + if d > 3 { -1 } else { 0 }, rgba(200, 250, 255), al); }
            self.px(x + 6, y - 2, rgba(200, 250, 255), al / 2);
        }
        // snowflake badge
        let (sx, sy) = (cx + 11, cy - 12);
        let blink = 200 + ((self.frame / 6 + seed) % 8) * 7;
        for d in -2..=2 { self.px(sx + d, sy, rgba(150, 240, 255), blink); self.px(sx, sy + d, rgba(150, 240, 255), blink); }
        self.px(sx - 1, sy - 1, rgba(150, 240, 255), blink); self.px(sx + 1, sy + 1, rgba(150, 240, 255), blink);
        self.px(sx - 1, sy + 1, rgba(150, 240, 255), blink); self.px(sx + 1, sy - 1, rgba(150, 240, 255), blink);
    }

    /// Procedural weather over the whole view: rain, storms with lightning, dust storms.
    fn weather(&mut self, g: &Game) {
        let (w, h) = (self.w, self.h);
        let f = self.frame as i32;
        match g.weather {
            W_RAIN | W_STORM => {
                let storm = g.weather == W_STORM;
                self.tint_rect(0, 0, w, h, rgba(20, 28, 44), if storm { 80 } else { 46 });
                // drifting cloud shadows
                for c in 0..5i32 {
                    let cx = (hash(c as u32 * 7 + 1) as i32 % (w + 300)) - 150 + f / 3 % (w + 300);
                    let cx = (cx).rem_euclid(w + 300) - 150; let cy = (hash(c as u32 * 13 + 5) as i32).rem_euclid(h.max(1));
                    for dy in -24..24 { for dx in -110..110 {
                        if dx * dx / 4 + dy * dy * 6 < 3000 && (dx + dy) & 1 == 0 { self.px(cx + dx, cy + dy, rgba(10, 14, 22), 30); }
                    } }
                }
                let n = (w * h / if storm { 900 } else { 1800 }) as u32;
                let slant = if storm { 3 } else { 1 };
                for k in 0..n {
                    let sp = 7 + (hash(k * 3 + 2) % 4) as i32;
                    let x0 = (hash(k * 2 + 1) as i32).rem_euclid(w + 60) - 30;
                    let y0 = (hash(k * 5 + 7) as i32 + f * sp).rem_euclid(h + 20) - 10;
                    let x = (x0 - (f * sp * slant) / 7).rem_euclid(w + 60) - 30;
                    let len = if storm { 7 } else { 5 };
                    for t in 0..len { self.px(x + t * slant / 3, y0 + t, rgba(196, 210, 230), 120); }
                    if (y0 + 10) % 97 < 3 { self.px(x - 1, y0 + len + 1, rgba(230, 240, 255), 160); self.px(x + 2, y0 + len + 1, rgba(230, 240, 255), 160); }
                }
                // lightning: flash + bolt
                for q in 0..self.puffs.len() {
                    let (kind, i, j, pw, pd, st) = self.puffs[q];
                    if kind != 3 { continue; }
                    let age = self.frame.wrapping_sub(st) as i32;
                    if age > 18 { continue; }
                    let fl = if age < 3 || (age > 5 && age < 8) { 120 } else { 0 };
                    if fl > 0 { self.tint_rect(0, 0, w, h, rgba(235, 240, 255), fl); }
                    if age < 10 {
                        let (tx, ty) = self.to_screen(i as f32 + pw as f32 * 0.5, j as f32 + pd as f32 * 0.5);
                        let mut x = tx + ((hash(st) % 60) as i32 - 30); let mut y = -10;
                        let steps = imax(4, (ty - 20 - y) / 14);
                        for s in 0..steps {
                            let nx = if s == steps - 1 { tx } else { x + ((hash(st + s as u32 * 31) % 25) as i32 - 12) };
                            let ny = if s == steps - 1 { ty - 20 } else { y + (ty - 20 - y) / (steps - s) };
                            for o in -1..=1 { self.line((x + o, y), (nx + o, ny), if o == 0 { rgba(255, 255, 255) } else { rgba(170, 190, 255) }, false); }
                            x = nx; y = ny;
                        }
                        for r in 0..6 { self.px(tx - r * 2, ty - 18 + r % 2, rgba(255, 240, 160), 220); self.px(tx + r * 2, ty - 18 + r % 2, rgba(255, 240, 160), 220); }
                    }
                }
            }
            W_DUST => {
                self.tint_rect(0, 0, w, h, rgba(200, 130, 60), 100);
                let n = (w * h / 400) as u32;
                for k in 0..n {
                    let sp = 3 + (hash(k * 3 + 2) % 5) as i32;
                    let x = (hash(k * 2 + 1) as i32 + f * sp).rem_euclid(w + 40) - 20;
                    let yb = (hash(k * 5 + 7) as i32).rem_euclid(h + 20) - 10;
                    let y = yb + ((f + k as i32 * 13) % 40 - 20) / 6;
                    let len = 2 + (k % 5) as i32;
                    let col = if k % 3 == 0 { rgba(120, 82, 46) } else { rgba(214, 170, 110) };
                    for t in 0..len { self.px(x - t, y, col, 190); if k % 4 == 0 { self.px(x - t, y + 1, col, 120); } }
                }
                // rolling haze bands
                for band in 0..3i32 {
                    let y0 = ((f / 2 + band * h / 3) % (h + 60)) - 30;
                    for y in y0..y0 + 18 { for x in (band % 2..w).step_by(2) { if (x + y) % 3 == 0 { self.px(x, y, rgba(170, 120, 70), 28); } } }
                }
            }
            _ => {}
        }
    }

    /// Iso box in tile space with a dark outline: the building block for walls.
    fn iso_box(&mut self, i0: f32, j0: f32, w: f32, d: f32, h: i32, cols: (u32, u32, u32), lift: i32) {
        let n = self.to_screen(i0, j0); let e = self.to_screen(i0 + w, j0);
        let s = self.to_screen(i0 + w, j0 + d); let ww = self.to_screen(i0, j0 + d);
        let up = |p: (i32, i32), z: i32| (p.0, p.1 - z - lift);
        self.quad([up(ww, 0), up(s, 0), up(s, h), up(ww, h)], cols.1, 256);
        self.quad([up(e, 0), up(s, 0), up(s, h), up(e, h)], cols.2, 256);
        self.quad([up(n, h), up(e, h), up(s, h), up(ww, h)], cols.0, 256);
        let ol = rgba(34, 30, 28);
        self.line(up(ww, h), up(s, h), ol, false); self.line(up(s, h), up(e, h), ol, false);
        self.line(up(ww, 0), up(s, 0), ol, false); self.line(up(s, 0), up(e, 0), ol, false);
        self.line(up(s, 0), up(s, h), ol, false); self.line(up(ww, 0), up(ww, h), ol, false); self.line(up(e, 0), up(e, h), ol, false);
        self.line(up(n, h), up(e, h), blend(cols.0, rgba(255, 255, 255), 90), false);
        self.line(up(n, h), up(ww, h), blend(cols.0, rgba(255, 255, 255), 90), false);
    }

    /// Chunky connected wall (tycoon / base-builder style). Each tile draws its pillar and the
    /// spans toward its +i and +j neighbours; the -i/-j spans belong to those neighbours.
    fn wall(&mut self, g: &Game, b: &Bld) {
        let (i, j) = (b.i as i32, b.j as i32);
        let is_wall = |a: i32, c: i32| g.tile(a, c).map_or(false, |t| t.occ >= 0 && g.blds[t.occ as usize].kind == K_WALL);
        let sec = self.view & VIEW_SECURITY != 0;
        let lvl = b.level.clamp(1, 3);
        let (pillar, span, ph, sh, pw, sw) = match lvl {
            1 => (((164, 116, 66), (130, 88, 46), (100, 66, 34)), ((150, 104, 58), (118, 80, 42), (90, 60, 30)), 17, 11, 0.30, 0.16),
            2 => (((186, 182, 172), (150, 146, 138), (112, 108, 102)), ((172, 168, 158), (138, 134, 126), (104, 100, 94)), 17, 13, 0.40, 0.34),
            _ => (((132, 140, 150), (98, 106, 116), (70, 76, 86)), ((168, 164, 154), (132, 128, 120), (98, 94, 88)), 21, 14, 0.42, 0.36),
        };
        let c = |t: (u32, u32, u32)| { let x = rgba(t.0, t.1, t.2); if sec { blend(x, rgba(90, 230, 110), 70) } else { x } };
        let pc = (c(pillar.0), c(pillar.1), c(pillar.2));
        let sc = (c(span.0), c(span.1), c(span.2));
        let o = (1.0 - pw) / 2.0;
        // pillar
        if lvl == 1 {
            // palisade: three sharpened logs
            for q in 0..3 {
                let a = o + q as f32 * pw / 3.0;
                self.iso_box(i as f32 + a, j as f32 + a, pw / 3.0, pw / 3.0, ph - (q % 2) * 2, pc, 0);
                let tip = self.to_screen(i as f32 + a + pw / 6.0, j as f32 + a + pw / 6.0);
                let (tx, ty) = (tip.0, tip.1 - ph + (q % 2) * 2);
                self.px(tx, ty - 1, pc.0, 256); self.px(tx, ty - 2, pc.0, 256); self.px(tx, ty - 3, rgba(60, 40, 22), 256);
            }
        } else {
            self.iso_box(i as f32 + o, j as f32 + o, pw, pw, ph, pc, 0);
            // cap stone
            self.iso_box(i as f32 + o - 0.04, j as f32 + o - 0.04, pw + 0.08, pw + 0.08, 3, pc, ph);
            let mid = self.to_screen(i as f32 + 0.5, j as f32 + 1.0 - o);
            if lvl == 3 {
                // hazard stripes on the front-left face
                let wl = self.to_screen(i as f32 + o, j as f32 + 1.0 - o); let sl = self.to_screen(i as f32 + 1.0 - o, j as f32 + 1.0 - o);
                for t in 0..=10 {
                    let x = wl.0 + (sl.0 - wl.0) * t / 10; let y = wl.1 + (sl.1 - wl.1) * t / 10;
                    for h in 4..9 { let col = if (t + h / 2) % 3 == 0 { rgba(30, 30, 30) } else { rgba(250, 200, 40) }; self.px(x, y - h, col, 256); }
                }
            } else {
                self.line((mid.0 - 6, mid.1 - 8), (mid.0 + 2, mid.1 - 4), blend(pc.1, rgba(0, 0, 0), 60), false);
            }
        }
        // spans to +i / +j neighbours
        for (di, dj) in [(1i32, 0i32), (0, 1)] {
            if !is_wall(i + di, j + dj) { continue; }
            let (i0, j0, w, d) = if di == 1 { (i as f32 + 1.0 - o, j as f32 + (1.0 - sw) / 2.0, 2.0 * o, sw) } else { (i as f32 + (1.0 - sw) / 2.0, j as f32 + 1.0 - o, sw, 2.0 * o) };
            if lvl == 1 {
                // two rails of planks
                let (ri0, rj0, rw, rd) = if di == 1 { (i0, j0 + sw * 0.3, w, sw * 0.4) } else { (i0 + sw * 0.3, j0, sw * 0.4, d) };
                self.iso_box(ri0, rj0, rw, rd, 3, sc, 3);
                self.iso_box(ri0, rj0, rw, rd, 3, sc, 10);
            } else {
                self.iso_box(i0, j0, w, d, sh, sc, 0);
                // brick courses
                let a = self.to_screen(if di == 1 { i0 } else { i0 }, if di == 1 { j0 + d } else { j0 + d });
                let bb = self.to_screen(if di == 1 { i0 + w } else { i0 + w }, j0 + d);
                let dark = blend(sc.1, rgba(0, 0, 0), 50);
                for h in [sh / 3, 2 * sh / 3] { self.line((a.0, a.1 - h), (bb.0, bb.1 - h), dark, false); }
                if lvl == 3 {
                    // razor wire coil on top
                    let p0 = self.to_screen(i0 + w * 0.0 + if di == 1 { 0.0 } else { sw / 2.0 }, j0 + if di == 1 { sw / 2.0 } else { 0.0 });
                    let p1 = self.to_screen(i0 + if di == 1 { w } else { sw / 2.0 }, j0 + if di == 1 { sw / 2.0 } else { d });
                    let n = imax(iabs(p1.0 - p0.0), 1);
                    for s in 0..=n {
                        let x = p0.0 + (p1.0 - p0.0) * s / n; let y = p0.1 + (p1.1 - p0.1) * s / n - sh;
                        let k = s % 4;
                        self.px(x, y - 2 - if k < 2 { k } else { 4 - k }, rgba(214, 218, 224), 256);
                        if k == 0 { self.px(x, y - 4, rgba(120, 124, 130), 256); }
                    }
                }
            }
        }
    }

    /// Chain-link fence: a post per tile, mesh + razor wire to each fenced neighbour.
    fn fence(&mut self, g: &Game, b: &Bld) {
        let (i, j) = (b.i as i32, b.j as i32);
        let is_wall = |a: i32, c: i32| g.tile(a, c).map_or(false, |t| t.occ >= 0 && g.blds[t.occ as usize].kind == K_WALL);
        let tint = if self.view & VIEW_SECURITY != 0 { rgba(120, 240, 130) } else { rgba(150, 156, 160) };
        let c = self.to_screen(i as f32 + 0.5, j as f32 + 0.5);
        let mut any = false;
        for (di, dj) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            if !is_wall(i + di, j + dj) { continue; }
            any = true;
            // draw half the span (to the shared edge) so each tile owns its own half
            let e = self.to_screen(i as f32 + 0.5 + di as f32 * 0.5, j as f32 + 0.5 + dj as f32 * 0.5);
            let n = imax(iabs(e.0 - c.0), 1);
            for s in 0..=n {
                let x = c.0 + (e.0 - c.0) * s / n; let y = c.1 + (e.1 - c.1) * s / n;
                let k = x.rem_euclid(64);
                for h in 1..12 { if (k + h) % 4 == 0 || (k - h).rem_euclid(4) == 0 { self.px(x, y - h, tint, 150); } }
                self.px(x, y - 12, rgba(70, 72, 76), 256);
                self.px(x, y - 1, rgba(70, 72, 76), 200);
                if k % 3 == 0 { self.px(x, y - 14, rgba(210, 214, 220), 256); } else if k % 3 == 1 { self.px(x, y - 13, rgba(120, 124, 130), 256); }
            }
        }
        // post
        for h in 0..15 { self.px(c.0, c.1 - h, rgba(58, 60, 64), 256); self.px(c.0 + 1, c.1 - h, rgba(96, 98, 104), 256); }
        if !any { for h in 2..12 { if h % 3 == 0 { self.px(c.0 - 2, c.1 - h, tint, 200); self.px(c.0 + 3, c.1 - h, tint, 200); } } }
    }

    fn arrow(&mut self, cx: i32, cy: i32, dir: u8, col: u32) {
        let (dx, dy) = DIRS[(dir % 4) as usize];
        // screen direction of a tile step
        let (sx, sy) = ((dx - dy) * 2, (dx + dy));
        for t in 0..6 { self.px(cx + sx * t / 2, cy + sy * t / 2, col, 256); }
        let tip = (cx + sx * 3, cy + sy * 3);
        let (px_, py_) = (-sy, sx);
        for t in 0..3 {
            self.px(tip.0 - sx * t / 2 + px_ * t / 2, tip.1 - sy * t / 2 + py_ * t / 4, col, 256);
            self.px(tip.0 - sx * t / 2 - px_ * t / 2, tip.1 - sy * t / 2 - py_ * t / 4, col, 256);
        }
    }

    fn draw_lane(&mut self, g: &Game, fi: i32, fj: i32, dir: u8, strength: f32) {
        let mut lane = [(0i32, 0i32, 0f32); 12];
        let n = g.fan_lane(fi, fj, dir, &mut lane);
        let (dx, dy) = DIRS[(dir % 4) as usize];
        for k in 0..n {
            let (ci, cj, s) = lane[k];
            let p = self.footprint(ci, cj, 1, 1);
            self.quad(p, rgba(95, 230, 224), (s * 5.0 * strength) as u32);
            // moving streaks
            for q in 0..2 {
                let ph = ((self.frame as f32 * 0.035 + q as f32 * 0.5 + k as f32 * 0.13) % 1.0) - 0.5;
                let side = if q == 0 { -0.18 } else { 0.18 };
                let (bx, by) = (ci as f32 + 0.5 + dx as f32 * ph - dy as f32 * side, cj as f32 + 0.5 + dy as f32 * ph + dx as f32 * side);
                let a = self.to_screen(bx, by);
                let b = self.to_screen(bx + dx as f32 * 0.22, by + dy as f32 * 0.22);
                if s >= 5.0 { self.line(a, b, rgba(220, 255, 252), false); }
            }
        }
    }

    pub fn draw(&mut self, gm: &mut Game) {
        self.frame = self.frame.wrapping_add(1);
        for e in gm.fx.drain(..) {
            match e.0 {
                5 => if self.trucks.len() < 6 { self.trucks.push((e.1, e.2, self.frame)); },
                6 => if self.raiders.len() < 12 {
                    for k in 0..2 { let off = k as f32 * 0.6; self.raiders.push((e.1 as f32 + 0.5 + off, e.2 as f32 + 0.5, e.1 as f32 - 14.0 + off * 3.0, e.2 as f32 + 10.0, self.frame + k * 6)); }
                },
                _ => if self.puffs.len() < 300 { self.puffs.push((e.0, e.1, e.2, e.3, e.4, self.frame)); },
            }
        }
        let fr = self.frame;
        self.puffs.retain(|p| fr.wrapping_sub(p.5) < 40);
        self.trucks.retain(|t| fr.wrapping_sub(t.2) < TRUCK_FRAMES);
        self.raiders.retain(|r| fr.wrapping_sub(r.4) < 170 || fr < r.4);
        let g: &Game = gm;
        for p in self.fb.iter_mut() { *p = BG; }
        // ground
        for dsum in 0..(2 * MAP as i32 - 1) {
            for i in 0..MAP as i32 {
                let j = dsum - i;
                if j < 0 || j >= MAP as i32 { continue; }
                let (sx, sy) = self.to_screen(i as f32, j as f32);
                if sx < -40 || sx > self.w + 40 || sy < -40 || sy > self.h + 8 { continue; }
                let t = g.tiles[j as usize * MAP + i as usize];
                let site = g.site as usize % N_SITES;
                let spr = match t.ground {
                    G_WATER => S::G_WATER, G_CONCRETE => S::G_CONCRETE, G_COAL => S::G_COAL,
                    G_ROUGH if site == 0 => S::G_ROUGH,
                    _ if site == 0 => S::G_GRASS,
                    // other sites: a few plain variants, some patches of the accent material, rubble as patches
                    gr => {
                        let base = S::G_S1_0 + (site - 1) * S::G_SITE_N;
                        let h = tile_hash(i, j);
                        if gr == G_ROUGH { base + S::G_SITE_VAR + S::G_SITE_ACC + (h >> 8) as usize % S::G_SITE_ROUGH }
                        // accents gather in clumps: a 4x4-tile patch is either a drift (about 40% of its tiles) or bare
                        else if ((tile_hash(i >> 2, (j >> 2) + 977) & 0xff) as f32) < S::G_SITE_ACC_SHARE[site] * 2.5 * 256.0 && (h & 0xff) < 102 { base + S::G_SITE_VAR + (h >> 8) as usize % S::G_SITE_ACC }
                        else { base + (h >> 16) as usize % S::G_SITE_VAR }
                    }
                };
                let s = S::SPRITES[spr];
                self.blit(spr, sx + s.ax as i32, sy + s.ay as i32, 256, (0, 0));
            }
        }
        // ground-level overlays (under buildings)
        if self.view & VIEW_GRID != 0 {
            for b in g.blds.iter() {
                if !KINDS[b.kind as usize].electric { continue; }
                let k = KINDS[b.kind as usize];
                let powered = g.nets.get(b.net as usize).map_or(false, |n| n.sources > 0);
                let c = if powered { NET_COLS[b.net as usize % 6] } else { (255, 80, 60) };
                let p = self.footprint(b.i as i32, b.j as i32, k.w as i32, k.d as i32);
                self.quad(p, rgba(c.0, c.1, c.2), 110);
            }
        }
        if self.view & VIEW_SECURITY != 0 {
            for b in g.blds.iter() {
                let k = KINDS[b.kind as usize];
                if b.kind == K_WALL { continue; }
                let p = self.footprint(b.i as i32, b.j as i32, k.w as i32, k.d as i32);
                self.quad(p, if b.exposed { rgba(255, 80, 60) } else { rgba(120, 230, 120) }, 110);
            }
        }
        // objects
        self.list.clear();
        for (x, b) in g.blds.iter().enumerate() {
            let k = KINDS[b.kind as usize];
            let depth = ((b.i as u32 + b.j as u32 + k.w as u32 + k.d as u32) * 16).saturating_sub(1);
            self.list.push((depth, 0, x as u16));
        }
        for j in 0..MAP { for i in 0..MAP {
            let t = g.tiles[j * MAP + i];
            if t.tree > 0 && t.occ < 0 { self.list.push((((i + j + 1) * 16) as u32, 1, (j * MAP + i) as u16)); }
        } }
        for (x, w) in g.workers.iter().enumerate() { self.list.push((((w.x + w.y) * 16.0) as u32 + 2, 2, x as u16)); }
        for x in 0..self.trucks.len() { let (ti, tj) = self.truck_pos(x); self.list.push((((ti + tj + 1.6) * 16.0) as u32, 3, x as u16)); }
        for x in 0..self.raiders.len() { let (ri, rj, _) = self.raider_pos(x); self.list.push((((ri + rj) * 16.0) as u32 + 2, 4, x as u16)); }
        self.list.sort_unstable_by_key(|e| e.0);
        let n = self.list.len();
        for e in 0..n {
            let (_, typ, idx) = self.list[e];
            match typ {
                0 => self.draw_building(g, idx as usize),
                1 => {
                    let (i, j) = ((idx as usize % MAP) as f32, (idx as usize / MAP) as f32);
                    let (sx, sy) = self.to_screen(i + 0.5, j + 0.5);
                    let t = g.tiles[idx as usize].tree as usize - 1;
                    let kinds = SITE_TREE_KINDS[g.site as usize % N_SITES];
                    let spr = S::TREE_0 + kinds[t % kinds.len()] as usize % S::N_TREES;
                    let s = S::SPRITES[spr];
                    self.blit(spr, sx + s.ax as i32, sy + s.ay as i32 - 8, 256, (0, 0));
                }
                3 => self.draw_truck(idx as usize),
                4 => {
                    let (ri, rj, a) = self.raider_pos(idx as usize);
                    if a > 0 {
                        let (sx, sy) = self.to_screen(ri, rj);
                        let spr = S::WORKER0 + ((self.frame / 5) % 2) as usize; let s = S::SPRITES[spr];
                        self.blit(spr, sx + s.ax as i32, sy + s.ay as i32, a, (rgba(150, 30, 30), 170));
                        self.px(sx + 2, sy - 7, rgba(60, 60, 60), a); self.px(sx + 3, sy - 7, rgba(60, 60, 60), a); // the loot
                    }
                }
                _ => {
                    let w = g.workers[idx as usize];
                    let (sx, sy) = self.to_screen(w.x, w.y);
                    let spr = S::WORKER0 + ((w.step / 10) % 2) as usize;
                    let s = S::SPRITES[spr];
                    self.blit(spr, sx + s.ax as i32, sy + s.ay as i32, 256, (0, 0));
                }
            }
        }
        // build dust, removal poofs, upgrade sparkles
        for q in 0..self.puffs.len() {
            let (kind, i, j, w, d, st) = self.puffs[q];
            let age = self.frame.wrapping_sub(st) as i32;
            let (cx, cy) = self.to_screen(i as f32 + w as f32 * 0.5, j as f32 + d as f32 * 0.5);
            let r = 6 + age * (w as i32 + d as i32) / 2;
            if kind == 3 { continue; }
            if kind == 4 {
                // compressed-air clean: white puffs blowing off the box
                for s in 0..10i32 {
                    let a = s as f32 * 0.628;
                    let rr = 4.0 + age as f32 * 0.9;
                    let (x, y) = (cx + (cos_approx(a) * rr * 1.5) as i32, cy - 10 + (sin_approx(a) * rr * 0.6) as i32 - age / 3);
                    let al = 220u32.saturating_sub(age as u32 * 7);
                    for dy in 0..2 { for dx in 0..2 { self.px(x + dx, y + dy, if s % 3 == 0 { rgba(180, 150, 110) } else { rgba(235, 245, 250) }, al); } }
                }
                continue;
            }
            if kind == 2 {
                for s in 0..8i32 {
                    let a = s as f32 * 0.785 + age as f32 * 0.08;
                    let rr = 6.0 + age as f32 * 0.6;
                    let (x, y) = (cx + (cos_approx(a) * rr * 1.6) as i32, cy - 10 - age / 2 + (sin_approx(a) * rr * 0.8) as i32);
                    let al = 256u32.saturating_sub(age as u32 * 6);
                    let col = if s % 2 == 0 { rgba(255, 236, 120) } else { rgba(255, 255, 255) };
                    self.px(x, y, col, al); self.px(x + 1, y, col, al / 2); self.px(x, y - 1, col, al / 2);
                }
            } else if age < 26 {
                let al = 200u32.saturating_sub(age as u32 * 8);
                for s in 0..14i32 {
                    let a = s as f32 * 0.449;
                    let (x, y) = (cx + (cos_approx(a) * r as f32) as i32, cy + (sin_approx(a) * r as f32 * 0.5) as i32 - age / 3);
                    let col = if kind == 0 { rgba(196, 180, 150) } else { rgba(170, 170, 160) };
                    for dy in -1..=1 { for dx in -2..=2 { self.px(x + dx, y + dy, col, al); } }
                }
            }
        }
        // power lines between poles and what they feed
        for &(a, b) in g.links.iter() {
            let (pa, pb) = (self.anchor_top(g, a as usize), self.anchor_top(g, b as usize));
            let col = if self.view & VIEW_GRID != 0 { let c = NET_COLS[g.blds[a as usize].net as usize % 6]; rgba(c.0, c.1, c.2) } else { rgba(40, 40, 44) };
            self.wire(pa, pb, col);
        }
        // airflow
        let sel_fan = self.sel >= 0 && (self.sel as usize) < g.blds.len() && g.blds[self.sel as usize].kind == K_FAN;
        if self.view & VIEW_AIR != 0 || sel_fan {
            for x in 0..g.blds.len() {
                let b = g.blds[x];
                if b.kind != K_FAN { continue; }
                if self.view & VIEW_AIR == 0 && x as i32 != self.sel { continue; }
                let f = if b.off != 0 { 0.0 } else { g.net_frac(&b) as f32 };
                self.draw_lane(g, b.i as i32, b.j as i32, b.dir, if f > 0.0 { 1.0 } else { 0.3 });
            }
        }
        if self.view & VIEW_AIR != 0 {
            for b in g.blds.iter() {
                if !is_miner(b.kind) || b.broken != 0 || b.off != 0 { continue; }
                let (cx, cy) = self.to_screen(b.i as f32 + 0.5, b.j as f32 + 0.5);
                let c = if b.temp < 70.0 { rgba(95, 230, 224) } else if b.temp < 80.0 { rgba(255, 210, 63) } else { rgba(255, 80, 60) };
                self.rect(cx - 2, cy - 30, 5, 5, rgba(10, 10, 10)); self.rect(cx - 1, cy - 29, 3, 3, c);
            }
        }
        // orbital solar: the power beam coming down from orbit onto the receiver (game)
        for b in g.blds.iter() {
            if b.kind != K_ORBITAL || b.off != 0 { continue; }
            let (cx, cy) = self.to_screen(b.i as f32 + 1.0, b.j as f32 + 1.0);
            let top = cy - 36; if cx < -20 || cx > self.w + 20 || top < 0 { continue; }
            let fl = 40 + (hash(self.frame / 3 + b.i as u32) % 30);
            for y in 0..top { for dx in -4..=4i32 {
                let a = if dx == 0 { fl + 80 } else if dx.abs() <= 1 { fl + 30 } else { fl / (dx.abs() as u32) };
                self.px(cx + dx, y, rgba(210, 240, 255), a);
            } }
            self.light(cx, top, 10, rgba(230, 250, 255), 140);
        }
        self.weather(g);
        self.day_night(g);
        // hover outline (mouse)
        if self.tool < 0 {
            if let Some(t) = g.tile(self.hover.0, self.hover.1) {
                if t.occ >= 0 && t.occ as i32 != self.sel {
                    let b = g.blds[t.occ as usize]; let k = KINDS[b.kind as usize];
                    self.outline(b.i as i32, b.j as i32, k.w as i32, k.d as i32, rgba(230, 230, 220), false);
                }
            }
        }
        // selection
        if self.sel >= 0 && (self.sel as usize) < g.blds.len() {
            let b = g.blds[self.sel as usize]; let k = KINDS[b.kind as usize];
            self.outline(b.i as i32, b.j as i32, k.w as i32, k.d as i32, rgba(255, 210, 63), true);
            if b.kind == K_POLE { self.outline(b.i as i32 - 4, b.j as i32 - 4, 9, 9, rgba(255, 210, 63), true); }
            if b.kind == K_GREENHOUSE { self.outline(b.i as i32 - HEAT_REACH, b.j as i32 - HEAT_REACH, 2 + 2 * HEAT_REACH, 2 + 2 * HEAT_REACH, rgba(255, 150, 70), true); }
        }
        // land clearing ghost
        if self.tool == TOOL_CLEAR && self.hover.0 >= 0 {
            let (i, j) = self.hover;
            let c = g.clear_cost(i, j);
            let ok = c > 0.0 && g.cash >= c;
            let p = self.footprint(i, j, 1, 1);
            self.quad(p, if ok { rgba(255, 210, 63) } else { rgba(255, 90, 70) }, 80);
            for q in 0..4 { self.line(p[q], p[(q + 1) % 4], if ok { rgba(255, 210, 63) } else { rgba(255, 90, 70) }, true); }
        }
        // placement ghost
        if self.tool >= 0 && (self.tool as usize) < N_KINDS && self.hover.0 >= 0 {
            let kind = self.tool as u8; let k = KINDS[kind as usize];
            let (i, j) = self.hover;
            let ok = g.can_place(kind, i, j) == 0.0;
            let p = self.footprint(i, j, k.w as i32, k.d as i32);
            self.quad(p, if ok { rgba(120, 230, 120) } else { rgba(255, 90, 70) }, 90);
            let spr = self.sprite_for(kind, self.tool_dir);
            let (sx, sy) = self.to_screen(i as f32, j as f32);
            let s = S::SPRITES[spr];
            self.blit(spr, sx + s.ax as i32, sy + s.ay as i32, 150, (0, 0));
            for q in 0..4 { self.line(p[q], p[(q + 1) % 4], if ok { rgba(255, 210, 63) } else { rgba(255, 90, 70) }, true); }
            if kind == K_FAN { self.draw_lane(g, i, j, self.tool_dir, 1.0); let (cx, cy) = self.to_screen(i as f32 + 0.5, j as f32 + 0.5); self.arrow(cx, cy - 6, self.tool_dir, rgba(95, 230, 224)); }
            if kind == K_POLE { self.outline(i - 4, j - 4, 9, 9, rgba(255, 210, 63), true); }
            if kind == K_GREENHOUSE { self.outline(i - HEAT_REACH, j - HEAT_REACH, 2 + 2 * HEAT_REACH, 2 + 2 * HEAT_REACH, rgba(255, 150, 70), true); }
        }
    }

    fn anchor_top(&self, g: &Game, idx: usize) -> (i32, i32) {
        let b = g.blds[idx]; let k = KINDS[b.kind as usize];
        let (x, y) = self.to_screen(b.i as f32 + k.w as f32 * 0.5, b.j as f32 + k.d as f32 * 0.5);
        if b.kind == K_POLE { (x, y - 26) } else { (x, y - 12) }
    }

    fn sprite_for(&self, kind: u8, dir: u8) -> usize {
        match kind {
            K_FAN => if dir % 2 == 0 { S::B_FAN2 } else { S::B_FAN },
            K_WIND => S::B_WIND0 + ((self.frame / 6) % 3) as usize,
            _ => KINDS[kind as usize].sprite,
        }
    }

    fn draw_building(&mut self, g: &Game, idx: usize) {
        let b = g.blds[idx];
        let k = KINDS[b.kind as usize];
        if b.kind == K_WALL { self.wall(g, &b); return; }
        let mut spr = self.sprite_for(b.kind, b.dir);
        if self.gold && is_miner(b.kind) { spr = spr - S::B_MINER1 + S::B_MINERG1; }
        if b.kind == K_WIND { spr = S::B_WIND0 + ((self.frame / 6 + idx as u32) % 3) as usize; }
        let s = S::SPRITES[spr];
        let (ax, mut ay) = self.to_screen(b.i as f32, b.j as f32);
        // drop-in when just built
        for p in self.puffs.iter() {
            if p.0 == 0 && p.1 == b.i as i32 && p.2 == b.j as i32 {
                let age = self.frame.wrapping_sub(p.5) as i32;
                if age < 10 { ay -= (10 - age) * (10 - age) / 3; }
            }
        }
        let (sx, sy) = (ax + s.ax as i32, ay + s.ay as i32);
        if sx > self.w || sy > self.h || sx + s.w as i32 <= 0 || sy + s.h as i32 <= 0 { return; }
        let f = if b.off != 0 { 0.0 } else { g.net_frac(&b) };
        // heat tint: 70 °C starts warming the box orange, 95 °C is fully red
        let mut tint = (0u32, 0u32);
        if is_miner(b.kind) {
            if b.broken != 0 { tint = (rgba(20, 18, 16), 170); }
            else if b.off != 0 { tint = (rgba(30, 34, 40), 90); }
            else if b.temp <= 70.0 && b.dust > 10.0 { tint = (rgba(150, 118, 78), (b.dust * 1.4) as u32); }
            else if b.temp > 70.0 {
                let t = fmin(1.0, (b.temp as f64 - 70.0) / 25.0);
                let pulse = if b.temp > 85.0 { (((self.frame % 30) as i32 - 15).unsigned_abs() * 4) as u32 } else { 0 };
                tint = (rgba(255, (140.0 - t * 100.0) as u32, 40), (t * 120.0) as u32 + pulse / 2);
            }
        }
        if self.view & VIEW_SECURITY != 0 && b.kind != K_WALL {
            tint = if b.exposed { (rgba(255, 60, 50), 120) } else { (rgba(90, 230, 110), 90) };
        } else if self.view & VIEW_GRID != 0 && k.electric {
            let powered = g.nets.get(b.net as usize).map_or(false, |n| n.sources > 0);
            let c = if powered { NET_COLS[b.net as usize % 6] } else { (255, 60, 50) };
            tint = (rgba(c.0, c.1, c.2), 100);
        }
        self.blit(spr, sx, sy, 256, tint);
        if f > 0.0 {
            for e in 0..s.ne as usize {
                let (ex, ey) = S::EMIT[s.e0 as usize + e];
                self.smoke(ax + ex as i32, ay + ey as i32, (idx as u32) * 3 + e as u32, false);
            }
        }
        let (cx, cy) = self.to_screen(b.i as f32 + k.w as f32 * 0.5, b.j as f32 + k.d as f32 * 0.5);
        if is_miner(b.kind) {
            if b.broken != 0 {
                for d in -4..=4 { self.px(cx + d, cy - 14 + d, rgba(255, 80, 60), 256); self.px(cx + d, cy - 14 - d, rgba(255, 80, 60), 256); }
                if (self.frame / 8) % 2 == 0 { self.smoke(cx, cy - 18, idx as u32, true); }
            } else {
                if b.fire != 0 && b.off == 0 { self.fire(cx, sy + 6, idx as u32); }
                // fan air reaching this box: animated cyan gusts + a snowflake
                let c = g.cooling_at(&b);
                if c > 0.5 && b.off == 0 && f > 0.0 { self.gusts(cx, cy - 10, c as f32, idx as u32); }
                else if b.temp > 85.0 { let pulse = 160 + ((self.frame % 40) as i32 - 20).unsigned_abs() * 4; self.glow(cx, sy + 8, pulse); }
                // mined coins popping out now and then
                if f > 0.0 {
                    let ph = (self.frame + idx as u32 * 53) % 240;
                    if ph < 36 {
                        let (x, y) = (cx + 4 + (ph as i32 / 12), sy - 2 - ph as i32 / 2);
                        let al = if ph > 24 { 256 - (ph - 24) * 20 } else { 256 };
                        let w = if (ph / 4) % 4 == 1 || (ph / 4) % 4 == 3 { 1 } else { 2 }; // spin
                        for dy in -2..=2 { for dx in -w..=w { self.px(x + dx, y + dy, if dx == -w || dy == -2 { rgba(255, 236, 140) } else { rgba(247, 168, 30) }, al); } }
                        self.px(x, y, rgba(180, 110, 10), al);
                    }
                }
                if f > 0.0 && (self.frame / 8 + idx as u32) % 3 != 0 {
                    for q in 0..3 { self.px(ax + 6 + q * 5, ay + 20 - q * 2, rgba(84, 232, 228), 256); }
                }
                if b.health < 90.0 {
                    let w = (b.health as i32 * 14) / 100;
                    let col = if b.health < 30.0 { rgba(255, 80, 60) } else if b.health < 60.0 { rgba(255, 210, 63) } else { rgba(183, 230, 138) };
                    self.rect(cx - 8, sy - 4, 16, 3, rgba(10, 10, 10)); self.rect(cx - 7, sy - 3, imax(1, w), 1, col);
                }
            }
        } else if b.kind == K_FAN {
            self.arrow(cx, cy - 8, b.dir, if f > 0.0 { rgba(95, 230, 224) } else { rgba(120, 120, 120) });
        } else if is_power(b.kind) && b.kind != K_CORE {
            for q in 0..3 {
                let col = if (q as u8) < b.level { rgba(255, 210, 63) } else { rgba(40, 40, 40) };
                self.rect(cx - 7 + q * 5, sy - 5, 3, 3, col);
            }
            // affordable upgrade: bobbing green arrow badge
            let uc = g.upgrade_cost(idx);
            if uc > 0.0 && g.cash >= uc {
                let bob = if (self.frame / 20) % 2 == 0 { 0 } else { 1 };
                let (bx, by) = (cx, sy - 15 - bob);
                self.rect(bx - 5, by - 1, 11, 9, rgba(20, 30, 20));
                for t in 0..4 { for w in -t..=t { self.px(bx + w, by + 1 + t, rgba(120, 230, 120), 256); } }
                self.rect(bx - 1, by + 5, 3, 2, rgba(120, 230, 120));
            }
        }
        // switched off: grey power symbol
        if b.off != 0 && b.broken == 0 {
            let (bx, by) = (cx, sy - 8);
            self.rect(bx - 4, by - 1, 9, 9, rgba(20, 20, 20));
            for t in 0..7 { let a = t as f32 * 0.9 + 1.2; let (c, s_) = (cos_approx(a), sin_approx(a)); self.px(bx + (c * 3.0) as i32, by + 4 + (s_ * 3.0) as i32, rgba(170, 170, 170), 256); }
            self.rect(bx, by, 1, 4, rgba(170, 170, 170));
        }
        // no power: blinking red bolt
        if is_consumer(b.kind) && b.broken == 0 && b.off == 0 && f <= 0.0 && (self.frame / 15) % 2 == 0 {
            let (bx, by) = (cx, sy - 10);
            self.rect(bx - 4, by - 1, 9, 10, rgba(20, 20, 20));
            for (x, y) in [(1, 0), (0, 1), (-1, 2), (0, 3), (1, 3), (2, 3), (1, 4), (0, 5), (-1, 6)] { self.px(bx + x, by + 1 + y, rgba(255, 80, 60), 256); }
        }
        if self.view & VIEW_GRID != 0 && is_consumer(b.kind) && b.broken == 0 && f > 0.0 && f < 0.999 {
            self.outline(b.i as i32, b.j as i32, k.w as i32, k.d as i32, rgba(255, 210, 63), true);
        }
    }
}

pub fn world(i: f32, j: f32) -> (i32, i32) { (((i - j) * 32.0) as i32, ((i + j) * 16.0) as i32) }
fn imin(a: i32, b: i32) -> i32 { if a < b { a } else { b } }
fn hash(n: u32) -> u32 { let mut x = n.wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA6B; x ^= x >> 15; x = x.wrapping_mul(0x2C1B_3C6D); x ^= x >> 12; x = x.wrapping_mul(0x297A_2D39); x ^ (x >> 15) }

fn sin_approx(x: f32) -> f32 {
    // Bhaskara-style approximation, good enough for 7 dots on a power icon
    let mut x = x % 6.2831855; if x < 0.0 { x += 6.2831855; }
    let (sgn, x) = if x > 3.1415927 { (-1.0, x - 3.1415927) } else { (1.0, x) };
    sgn * 16.0 * x * (3.1415927 - x) / (49.348022 - 4.0 * x * (3.1415927 - x))
}
fn cos_approx(x: f32) -> f32 { sin_approx(x + 1.5707964) }

/// A stable per-tile hash for picking ground variants.
fn tile_hash(i: i32, j: i32) -> u32 {
    let mut x = (i as u32).wrapping_mul(374_761_393) ^ (j as u32).wrapping_mul(668_265_263);
    x = (x ^ (x >> 13)).wrapping_mul(1_274_126_177);
    x ^ (x >> 16)
}
