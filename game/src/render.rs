//! Software renderer: draws the isometric plant into an RGBA framebuffer that JS shows on a <canvas>.
//!
//! Sprites are pre-rendered voxel dioramas (tools/voxel). The atlas is baked at 2x the base tile
//! (128x64 px per tile) and a 1x copy is box-filtered at load. `s` picks the density per frame
//! (1 = base, 2 = hi-res), so the framebuffer never needs more pixels than the device shows.
//! Positions written as base-tile pixels (the old 64x32 art) go through `k()`; atlas units go through `a2()`.
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
    pub atlas: Vec<u32>,                     // 2x atlas, as loaded
    atlas1: Vec<u32>,                        // 1x atlas, box-filtered from it
    pub s: i32,                              // 1 or 2: framebuffer pixels per base-tile pixel
    list: Vec<(u32, u8, u16)>,
    puffs: Vec<(u8, i32, i32, u8, u8, u32)>, // kind, i, j, w, d, start frame
    trucks: Vec<(i32, i32, u32)>,            // delivery to tile (i, j), start frame
    raiders: Vec<(f32, f32, f32, f32, u32)>, // running from (i, j) towards (i, j), start frame
    pub gold: bool,                          // gold miner containers (a cosmetic)
    pub night: f32,                          // 0 = no day/night, 1 = full strength (JS lowers it at high speed)
}

const BG: u32 = 0xFF_E1_E7_E9; // soft daylight haze around the map (ABGR)
const TRUCK_FRAMES: u32 = 240;
const NET_COLS: [(u32, u32, u32); 6] = [(40, 190, 184), (240, 180, 30), (110, 190, 80), (240, 130, 40), (160, 110, 240), (80, 140, 240)];

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
                   atlas: Vec::new(), atlas1: Vec::new(), s: 2, list: Vec::with_capacity(4096), puffs: Vec::with_capacity(64) }
    }

    /// Build the 1x atlas from the 2x one (premultiplied 2x2 box filter). Call once the 2x atlas is loaded.
    pub fn atlas_ready(&mut self) {
        let (w, h) = (S::ATLAS_W, S::ATLAS_H);
        let (hw, hh) = (w / 2, h / 2);
        self.atlas1.clear(); self.atlas1.resize(hw * hh, 0);
        for y in 0..hh { for x in 0..hw {
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = self.atlas[(2 * y + dy) * w + 2 * x + dx];
                let pa = p >> 24;
                r += (p & 255) * pa; g += ((p >> 8) & 255) * pa; b += ((p >> 16) & 255) * pa; a += pa;
            }
            self.atlas1[y * hw + x] = if a == 0 { 0 } else { ((a / 4) << 24) | ((b / a) << 16) | ((g / a) << 8) | (r / a) };
        } }
    }

    /// Switch density, keeping the same world point at the centre of the view.
    pub fn set_scale(&mut self, s: i32) {
        let s = if s >= 2 { 2 } else { 1 };
        if s == self.s { return; }
        let cx = (self.cam_x + self.w / 2) * s / self.s; let cy = (self.cam_y + self.h / 2) * s / self.s;
        self.s = s; self.cam_x = cx - self.w / 2; self.cam_y = cy - self.h / 2;
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        let (w, h) = (imax(16, w), imax(16, h));
        self.cam_x += (self.w - w) / 2; self.cam_y += (self.h - h) / 2;
        self.w = w; self.h = h;
        self.fb.clear(); self.fb.resize((w * h) as usize, BG);
    }

    #[inline] fn k(&self, v: i32) -> i32 { v * self.s }
    #[inline] fn a2(&self, v: i16) -> i32 { v as i32 * self.s / 2 }

    fn wpos(&self, i: f32, j: f32) -> (i32, i32) { let s = self.s as f32; (floor32((i - j) * 32.0 * s), floor32((i + j) * 16.0 * s)) }

    pub fn center_on(&mut self, i: f32, j: f32) {
        let (x, y) = self.wpos(i, j);
        self.cam_x = x - self.w / 2; self.cam_y = y - self.h / 2;
    }

    pub fn clamp_cam(&mut self) {
        let t = 32 * self.s;
        let lo_x = -(MAP as i32) * t - self.w / 2; let hi_x = (MAP as i32) * t - self.w / 2;
        let lo_y = -self.h / 2; let hi_y = (MAP as i32) * t - self.h / 2;
        self.cam_x = self.cam_x.clamp(lo_x, hi_x); self.cam_y = self.cam_y.clamp(lo_y, hi_y);
    }

    pub fn pick(&self, sx: i32, sy: i32) -> (i32, i32) {
        let wx = (sx + self.cam_x) as f32; let wy = (sy + self.cam_y) as f32;
        let u = wx / (32.0 * self.s as f32); let v = wy / (16.0 * self.s as f32);
        (floor32((u + v) / 2.0), floor32((v - u) / 2.0))
    }

    pub fn to_screen(&self, i: f32, j: f32) -> (i32, i32) { let (x, y) = self.wpos(i, j); (x - self.cam_x, y - self.cam_y) }

    /// Blit a sprite with its anchor at screen point (ox, oy). `alpha` 0..=256 fades it; `tint` pulls every pixel toward a colour.
    fn blit_at(&mut self, spr: usize, ox: i32, oy: i32, alpha: u32, tint: (u32, u32)) {
        let s = S::SPRITES[spr];
        self.blit(spr, ox + self.a2(s.ax), oy + self.a2(s.ay), alpha, tint);
    }

    /// Blit a sprite with its top-left at (sx, sy).
    fn blit(&mut self, spr: usize, sx: i32, sy: i32, alpha: u32, tint: (u32, u32)) {
        let s = S::SPRITES[spr];
        let hi = self.s == 2;
        let (rx, ry, w, h, aw) = if hi { (s.x as i32, s.y as i32, s.w as i32, s.h as i32, S::ATLAS_W as i32) }
                                 else { (s.x as i32 / 2, s.y as i32 / 2, s.w as i32 / 2, s.h as i32 / 2, S::ATLAS_W as i32 / 2) };
        let x0 = imax(0, sx); let y0 = imax(0, sy);
        let x1 = if sx + w < self.w { sx + w } else { self.w };
        let y1 = if sy + h < self.h { sy + h } else { self.h };
        if x0 >= x1 || y0 >= y1 { return; }
        let atlas = if hi { &self.atlas } else { &self.atlas1 };
        if atlas.is_empty() { return; }
        for y in y0..y1 {
            let src_row = ((ry + y - sy) * aw + rx - sx) as usize;
            let dst_row = (y * self.w) as usize;
            for x in x0..x1 {
                let mut p = atlas[src_row + x as usize];
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

    /// One base-tile pixel: an s x s block.
    fn dot(&mut self, x: i32, y: i32, col: u32, a: u32) {
        for yy in 0..self.s { for xx in 0..self.s { self.px(x + xx, y + yy, col, a); } }
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, col: u32) { for yy in 0..h { for xx in 0..w { self.px(x + xx, y + yy, col, 256); } } }

    /// A rounded badge (base-tile size w x h centred on x, top at y), for map icons.
    fn badge(&mut self, cx: i32, y: i32, w: i32, h: i32, col: u32, a: u32) {
        let (w, h) = (self.k(w), self.k(h));
        let r = self.k(2);
        for yy in 0..h { for xx in 0..w {
            let dx = if xx < r { r - xx } else if xx >= w - r { xx - (w - r - 1) } else { 0 };
            let dy = if yy < r { r - yy } else if yy >= h - r { yy - (h - r - 1) } else { 0 };
            if dx * dx + dy * dy > r * r { continue; }
            self.px(cx - w / 2 + xx, y + yy, col, a);
        } }
    }

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
        let dl = 3 * self.s;
        for s in 0..=n {
            if dash && (s / dl) % 2 == 1 { continue; }
            self.px(a.0 + dx * s / n, a.1 + dy * s / n, col, 256);
        }
    }

    fn wire(&mut self, a: (i32, i32), b: (i32, i32), col: u32) {
        let n = imax(imax(iabs(b.0 - a.0), iabs(b.1 - a.1)), 1);
        let sag = imin(self.k(14), n / 6 + self.k(2));
        let mut prev = a;
        for s in 1..=n {
            let t = s as f32 / n as f32;
            let x = a.0 + ((b.0 - a.0) as f32 * t) as i32;
            let y = a.1 + ((b.1 - a.1) as f32 * t) as i32 + (4.0 * sag as f32 * t * (1.0 - t)) as i32;
            if (x, y) != prev { self.px(x, y, col, 230); if self.s == 2 { self.px(x, y + 1, col, 90); } prev = (x, y); }
        }
    }

    fn footprint(&self, i: i32, j: i32, w: i32, d: i32) -> [(i32, i32); 4] {
        [self.to_screen(i as f32, j as f32), self.to_screen((i + w) as f32, j as f32), self.to_screen((i + w) as f32, (j + d) as f32), self.to_screen(i as f32, (j + d) as f32)]
    }

    fn outline(&mut self, i: i32, j: i32, w: i32, d: i32, col: u32, dash: bool) {
        let p = self.footprint(i, j, w, d);
        for q in 0..4 {
            self.line(p[q], p[(q + 1) % 4], col, dash);
            if self.s == 2 { self.line((p[q].0, p[q].1 + 1), (p[(q + 1) % 4].0, p[(q + 1) % 4].1 + 1), col, dash); }
        }
    }

    /// A soft round puff, alpha falling off to the edge.
    fn puff(&mut self, cx: i32, cy: i32, r: i32, col: u32, a: u32) {
        if r <= 0 || a == 0 { return; }
        let r2 = r * r;
        for dy in -r..=r { for dx in -r..=r {
            let q = dx * dx + dy * dy; if q > r2 { continue; }
            let f = 256 - (q * 256 / (r2 + 1)) as u32;
            self.px(cx + dx, cy + dy, col, (a * f) >> 8);
        } }
    }

    fn smoke(&mut self, x: i32, y: i32, seed: u32, dark: bool) {
        for k in 0..6u32 {
            let ph = (self.frame / 2 + k * 11 + seed * 7) % 66;
            let r = self.k(2 + (ph / 10) as i32);
            let cx = x + self.k((ph / 6) as i32); let cy = y - self.k(ph as i32);
            let a = 170u32.saturating_sub(ph * 5 / 2);
            let col = if dark { rgba(58, 54, 52) } else if ph < 20 { rgba(246, 246, 242) } else { rgba(226, 228, 230) };
            self.puff(cx, cy, r, col, a);
        }
    }

    fn fire(&mut self, cx: i32, top: i32, seed: u32) {
        for k in 0..9i32 {
            let x = cx + self.k(-12 + k * 3);
            let h = self.k(5 + (((self.frame / 3) as i32 + k * 7 + seed as i32 * 5) % 9));
            for yy in 0..h {
                let col = if yy < h / 3 { rgba(255, 236, 140) } else if yy < 2 * h / 3 { rgba(255, 150, 40) } else { rgba(220, 60, 30) };
                let wob = if (yy / self.s + k + (self.frame / 4) as i32) % 4 == 0 { self.s } else { 0 };
                for w in 0..self.k(2) { self.px(x + wob + w, top - yy, col, if w < self.s { 256 } else { 170 }); }
            }
        }
        self.smoke(cx, top - self.k(14), seed * 3 + 1, true);
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
        let m = self.k(60);
        for b in g.blds.iter() {
            let kd = KINDS[b.kind as usize];
            let (cx, cy) = self.to_screen(b.i as f32 + kd.w as f32 * 0.5, b.j as f32 + kd.d as f32 * 0.5);
            if cx < -m || cx > self.w + m || cy < -m || cy > self.h + m { continue; }
            let running = b.off == 0 && b.broken == 0 && g.net_frac(b) > 0.0;
            if is_miner(b.kind) {
                if running {
                    self.light(cx + self.k(4), cy + self.k(2), self.k(10), rgba(90, 240, 230), la * 40 >> 8);
                    let blink = (self.frame / 8 + b.i as u32 * 3 + b.j as u32) % 5;
                    for s in 0..4 { if s != blink as i32 % 4 { self.dot(cx + self.k(-2 + s * 4), cy + self.k(1 + s), rgba(140, 255, 245), la); } }
                }
                if b.fire != 0 { self.light(cx, cy - self.k(10), self.k(16), rgba(255, 110, 40), la * 90 >> 8); }
            } else if b.kind == K_GREENHOUSE {
                self.light(cx, cy - self.k(6), self.k(34), rgba(255, 120, 200), la * 45 >> 8);
            } else if b.kind == K_SHED {
                self.light(cx + self.k(8), cy - self.k(2), self.k(9), rgba(255, 200, 110), la * 80 >> 8);
            } else if is_power(b.kind) {
                self.light(cx, cy - self.k(14), self.k(14), rgba(255, 214, 120), la * 50 >> 8);
                self.dot(cx, cy - self.k(18), rgba(255, 230, 160), la);
            } else if b.kind == K_POLE && (b.i + b.j) % 3 == 0 {
                self.light(cx, cy - self.k(26), self.k(9), rgba(255, 220, 150), la * 40 >> 8);
            }
        }
    }
    /// A soft elliptical pool of light (blended over whatever is there).
    fn light(&mut self, x: i32, y: i32, r: i32, col: u32, a: u32) {
        if a == 0 || r <= 0 { return; }
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
        let (sx, sy) = self.to_screen(ti, tj);
        self.blit_at(S::TRUCK, sx, sy, 256, (0, 0));
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
            let a = a * strength / 256; let r = self.k(r);
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
            let x = cx + self.k(-16 + ph);
            let y = cy + self.k(-6 + (k as i32) * 4 - (ph / 12));
            let fade = if ph < 8 { ph as u32 * 32 } else if ph > 38 { (48 - ph) as u32 * 25 } else { 256 };
            let al = a * fade / 256;
            for d in 0..6 { self.dot(x + self.k(d), y + if d > 3 { -self.s } else { 0 }, rgba(220, 252, 255), al); }
        }
        // snowflake badge
        let (sx, sy) = (cx + self.k(11), cy - self.k(12));
        let blink = 200 + ((self.frame / 6 + seed) % 8) * 7;
        let c = rgba(150, 240, 255);
        for d in -2..=2 { self.dot(sx + self.k(d), sy, c, blink); self.dot(sx, sy + self.k(d), c, blink); }
        for (dx, dy) in [(-1, -1), (1, 1), (-1, 1), (1, -1)] { self.dot(sx + self.k(dx), sy + self.k(dy), c, blink); }
    }

    /// Procedural weather over the whole view: rain, storms with lightning, dust storms.
    fn weather(&mut self, g: &Game) {
        let (w, h) = (self.w, self.h);
        let f = self.frame as i32;
        let s = self.s;
        match g.weather {
            W_RAIN | W_STORM => {
                let storm = g.weather == W_STORM;
                self.tint_rect(0, 0, w, h, rgba(40, 52, 72), if storm { 80 } else { 46 });
                // drifting cloud shadows
                for c in 0..5i32 {
                    let cx = (hash(c as u32 * 7 + 1) as i32 % (w + 300)) - 150 + f / 3 % (w + 300);
                    let cx = (cx).rem_euclid(w + 300) - 150; let cy = (hash(c as u32 * 13 + 5) as i32).rem_euclid(h.max(1));
                    self.puff(cx, cy, 90 * s, rgba(20, 26, 36), 40);
                }
                let n = (w * h / if storm { 900 } else { 1800 } / (s * s)) as u32;
                let slant = if storm { 3 } else { 1 };
                for k in 0..n {
                    let sp = (7 + (hash(k * 3 + 2) % 4) as i32) * s;
                    let x0 = (hash(k * 2 + 1) as i32).rem_euclid(w + 60) - 30;
                    let y0 = (hash(k * 5 + 7) as i32 + f * sp).rem_euclid(h + 20) - 10;
                    let x = (x0 - (f * sp * slant) / 7).rem_euclid(w + 60) - 30;
                    let len = if storm { 7 } else { 5 } * s;
                    for t in 0..len { self.px(x + t * slant / 3, y0 + t, rgba(214, 224, 240), 110); }
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
                        let mut x = tx + ((hash(st) % 60) as i32 - 30) * s; let mut y = -10;
                        let steps = imax(4, (ty - 20 * s - y) / (14 * s));
                        for st2 in 0..steps {
                            let nx = if st2 == steps - 1 { tx } else { x + ((hash(st + st2 as u32 * 31) % 25) as i32 - 12) * s };
                            let ny = if st2 == steps - 1 { ty - 20 * s } else { y + (ty - 20 * s - y) / (steps - st2) };
                            for o in -s..=s { self.line((x + o, y), (nx + o, ny), if o == 0 { rgba(255, 255, 255) } else { rgba(170, 190, 255) }, false); }
                            x = nx; y = ny;
                        }
                        self.puff(tx, ty - 18 * s, 10 * s, rgba(255, 240, 170), 200);
                    }
                }
            }
            W_DUST => {
                self.tint_rect(0, 0, w, h, rgba(210, 150, 80), 100);
                let n = (w * h / 400 / (s * s)) as u32;
                for k in 0..n {
                    let sp = (3 + (hash(k * 3 + 2) % 5) as i32) * s;
                    let x = (hash(k * 2 + 1) as i32 + f * sp).rem_euclid(w + 40) - 20;
                    let yb = (hash(k * 5 + 7) as i32).rem_euclid(h + 20) - 10;
                    let y = yb + ((f + k as i32 * 13) % 40 - 20) * s / 6;
                    let len = (2 + (k % 5) as i32) * s;
                    let col = if k % 3 == 0 { rgba(140, 100, 60) } else { rgba(224, 184, 124) };
                    for t in 0..len { self.px(x - t, y, col, 160); if s == 2 { self.px(x - t, y + 1, col, 100); } }
                }
            }
            _ => {}
        }
    }

    /// Iso box in tile space, lit like the voxel art (no outlines): the building block for walls.
    fn iso_box(&mut self, i0: f32, j0: f32, w: f32, d: f32, h: i32, cols: (u32, u32, u32), lift: i32) {
        let (h, lift) = (self.k(h), self.k(lift));
        let n = self.to_screen(i0, j0); let e = self.to_screen(i0 + w, j0);
        let s = self.to_screen(i0 + w, j0 + d); let ww = self.to_screen(i0, j0 + d);
        let up = |p: (i32, i32), z: i32| (p.0, p.1 - z - lift);
        self.quad([up(ww, 0), up(s, 0), up(s, h), up(ww, h)], cols.1, 256);
        self.quad([up(e, 0), up(s, 0), up(s, h), up(e, h)], cols.2, 256);
        self.quad([up(n, h), up(e, h), up(s, h), up(ww, h)], cols.0, 256);
        self.line(up(n, h), up(e, h), blend(cols.0, rgba(255, 255, 255), 70), false);
        self.line(up(n, h), up(ww, h), blend(cols.0, rgba(255, 255, 255), 70), false);
        self.line(up(ww, 0), up(s, 0), blend(cols.1, rgba(0, 0, 0), 40), false);
        self.line(up(s, 0), up(e, 0), blend(cols.2, rgba(0, 0, 0), 40), false);
    }

    /// Chunky connected wall. Each tile draws its pillar and the spans toward its +i and +j neighbours;
    /// the -i/-j spans belong to those neighbours.
    fn wall(&mut self, g: &Game, b: &Bld) {
        let (i, j) = (b.i as i32, b.j as i32);
        let is_wall = |a: i32, c: i32| g.tile(a, c).map_or(false, |t| t.occ >= 0 && g.blds[t.occ as usize].kind == K_WALL);
        let sec = self.view & VIEW_SECURITY != 0;
        let lvl = b.level.clamp(1, 3);
        let (pillar, span, ph, sh, pw, sw) = match lvl {
            1 => (((178, 130, 80), (146, 102, 60), (110, 76, 44)), ((164, 118, 70), (132, 92, 54), (100, 70, 40)), 17, 11, 0.30, 0.16),
            2 => (((206, 202, 192), (172, 168, 160), (130, 126, 120)), ((194, 190, 180), (160, 156, 148), (122, 118, 112)), 17, 13, 0.40, 0.34),
            _ => (((150, 158, 168), (116, 124, 134), (84, 90, 100)), ((188, 184, 174), (152, 148, 140), (114, 110, 104)), 21, 14, 0.42, 0.36),
        };
        let c = |t: (u32, u32, u32)| { let x = rgba(t.0, t.1, t.2); if sec { blend(x, rgba(90, 230, 110), 70) } else { x } };
        let pc = (c(pillar.0), c(pillar.1), c(pillar.2));
        let sc = (c(span.0), c(span.1), c(span.2));
        let o = (1.0 - pw) / 2.0;
        if lvl == 1 {
            // palisade: three sharpened logs
            for q in 0..3 {
                let a = o + q as f32 * pw / 3.0;
                self.iso_box(i as f32 + a, j as f32 + a, pw / 3.0, pw / 3.0, ph - (q % 2) * 2, pc, 0);
                let tip = self.to_screen(i as f32 + a + pw / 6.0, j as f32 + a + pw / 6.0);
                let (tx, ty) = (tip.0, tip.1 - self.k(ph - (q % 2) * 2));
                self.dot(tx, ty - self.k(1), pc.0, 256); self.dot(tx, ty - self.k(2), pc.0, 256); self.dot(tx, ty - self.k(3), rgba(80, 56, 32), 256);
            }
        } else {
            self.iso_box(i as f32 + o, j as f32 + o, pw, pw, ph, pc, 0);
            self.iso_box(i as f32 + o - 0.04, j as f32 + o - 0.04, pw + 0.08, pw + 0.08, 3, pc, ph);
            if lvl == 3 {
                // hazard stripes on the front-left face
                let wl = self.to_screen(i as f32 + o, j as f32 + 1.0 - o); let sl = self.to_screen(i as f32 + 1.0 - o, j as f32 + 1.0 - o);
                let n = 10 * self.s;
                for t in 0..=n {
                    let x = wl.0 + (sl.0 - wl.0) * t / n; let y = wl.1 + (sl.1 - wl.1) * t / n;
                    for hh in self.k(4)..self.k(9) { let col = if (t / self.s + hh / self.s / 2) % 3 == 0 { rgba(30, 30, 30) } else { rgba(250, 200, 40) }; self.px(x, y - hh, col, 256); }
                }
            }
        }
        for (di, dj) in [(1i32, 0i32), (0, 1)] {
            if !is_wall(i + di, j + dj) { continue; }
            let (i0, j0, w, d) = if di == 1 { (i as f32 + 1.0 - o, j as f32 + (1.0 - sw) / 2.0, 2.0 * o, sw) } else { (i as f32 + (1.0 - sw) / 2.0, j as f32 + 1.0 - o, sw, 2.0 * o) };
            if lvl == 1 {
                let (ri0, rj0, rw, rd) = if di == 1 { (i0, j0 + sw * 0.3, w, sw * 0.4) } else { (i0 + sw * 0.3, j0, sw * 0.4, d) };
                self.iso_box(ri0, rj0, rw, rd, 3, sc, 3);
                self.iso_box(ri0, rj0, rw, rd, 3, sc, 10);
            } else {
                self.iso_box(i0, j0, w, d, sh, sc, 0);
                let a = self.to_screen(i0, j0 + d);
                let bb = self.to_screen(i0 + w, j0 + d);
                let dark = blend(sc.1, rgba(0, 0, 0), 40);
                for hh in [sh / 3, 2 * sh / 3] { let hh = self.k(hh); self.line((a.0, a.1 - hh), (bb.0, bb.1 - hh), dark, false); }
                if lvl == 3 {
                    // razor wire coil on top
                    let p0 = self.to_screen(i0 + if di == 1 { 0.0 } else { sw / 2.0 }, j0 + if di == 1 { sw / 2.0 } else { 0.0 });
                    let p1 = self.to_screen(i0 + if di == 1 { w } else { sw / 2.0 }, j0 + if di == 1 { sw / 2.0 } else { d });
                    let n = imax(iabs(p1.0 - p0.0), 1);
                    for st in 0..=n {
                        let x = p0.0 + (p1.0 - p0.0) * st / n; let y = p0.1 + (p1.1 - p0.1) * st / n - self.k(sh);
                        let kk = (st / self.s) % 4;
                        self.px(x, y - self.k(2) - self.k(if kk < 2 { kk } else { 4 - kk }), rgba(214, 218, 224), 256);
                    }
                }
            }
        }
    }

    fn arrow(&mut self, cx: i32, cy: i32, dir: u8, col: u32) {
        let (dx, dy) = DIRS[(dir % 4) as usize];
        let (sx, sy) = ((dx - dy) * 2, (dx + dy));
        for t in 0..6 { self.dot(cx + self.k(sx * t / 2), cy + self.k(sy * t / 2), col, 256); }
        let tip = (cx + self.k(sx * 3), cy + self.k(sy * 3));
        let (px_, py_) = (-sy, sx);
        for t in 0..3 {
            self.dot(tip.0 + self.k(-sx * t / 2 + px_ * t / 2), tip.1 + self.k(-sy * t / 2 + py_ * t / 4), col, 256);
            self.dot(tip.0 + self.k(-sx * t / 2 - px_ * t / 2), tip.1 + self.k(-sy * t / 2 - py_ * t / 4), col, 256);
        }
    }

    fn draw_lane(&mut self, g: &Game, fi: i32, fj: i32, dir: u8, strength: f32) {
        let mut lane = [(0i32, 0i32, 0f32); 12];
        let n = g.fan_lane(fi, fj, dir, &mut lane);
        let (dx, dy) = DIRS[(dir % 4) as usize];
        for k in 0..n {
            let (ci, cj, s) = lane[k];
            let p = self.footprint(ci, cj, 1, 1);
            self.quad(p, rgba(60, 200, 210), (s * 5.0 * strength) as u32);
            for q in 0..2 {
                let ph = ((self.frame as f32 * 0.035 + q as f32 * 0.5 + k as f32 * 0.13) % 1.0) - 0.5;
                let side = if q == 0 { -0.18 } else { 0.18 };
                let (bx, by) = (ci as f32 + 0.5 + dx as f32 * ph - dy as f32 * side, cj as f32 + 0.5 + dy as f32 * ph + dx as f32 * side);
                let a = self.to_screen(bx, by);
                let b = self.to_screen(bx + dx as f32 * 0.22, by + dy as f32 * 0.22);
                if s >= 5.0 { self.line(a, b, rgba(235, 255, 252), false); if self.s == 2 { self.line((a.0, a.1 + 1), (b.0, b.1 + 1), rgba(235, 255, 252), false); } }
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
        // ground (voxel slabs: the edge tiles show their sides, the map reads as a diorama)
        let (mx, my, mb) = (self.k(40), self.k(48), self.k(8));
        for dsum in 0..(2 * MAP as i32 - 1) {
            for i in 0..MAP as i32 {
                let j = dsum - i;
                if j < 0 || j >= MAP as i32 { continue; }
                let (sx, sy) = self.to_screen(i as f32, j as f32);
                if sx < -mx || sx > self.w + mx || sy < -my || sy > self.h + mb { continue; }
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
                self.blit_at(spr, sx, sy, 256, (0, 0));
            }
        }
        // ground-level overlays (under buildings)
        if self.view & VIEW_GRID != 0 {
            for b in g.blds.iter() {
                if !KINDS[b.kind as usize].electric { continue; }
                let k = KINDS[b.kind as usize];
                let powered = g.nets.get(b.net as usize).map_or(false, |n| n.sources > 0);
                let c = if powered { NET_COLS[b.net as usize % 6] } else { (235, 70, 50) };
                let p = self.footprint(b.i as i32, b.j as i32, k.w as i32, k.d as i32);
                self.quad(p, rgba(c.0, c.1, c.2), 120);
            }
        }
        if self.view & VIEW_SECURITY != 0 {
            for b in g.blds.iter() {
                let k = KINDS[b.kind as usize];
                if b.kind == K_WALL { continue; }
                let p = self.footprint(b.i as i32, b.j as i32, k.w as i32, k.d as i32);
                self.quad(p, if b.exposed { rgba(235, 70, 50) } else { rgba(80, 200, 100) }, 120);
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
                    if sx < -self.k(60) || sx > self.w + self.k(60) || sy < -self.k(10) || sy > self.h + self.k(70) { continue; }
                    let t = g.tiles[idx as usize].tree as usize - 1;
                    let kinds = SITE_TREE_KINDS[g.site as usize % N_SITES];
                    let spr = S::TREE_0 + kinds[t % kinds.len()] as usize % S::N_TREES;
                    self.blit_at(spr, sx, sy, 256, (0, 0));
                }
                3 => self.draw_truck(idx as usize),
                4 => {
                    let (ri, rj, a) = self.raider_pos(idx as usize);
                    if a > 0 {
                        let (sx, sy) = self.to_screen(ri, rj);
                        let spr = S::WORKER0 + ((self.frame / 5) % 2) as usize;
                        self.blit_at(spr, sx, sy, a, (rgba(150, 30, 30), 170));
                        self.dot(sx + self.k(2), sy - self.k(9), rgba(60, 60, 60), a); // the loot
                    }
                }
                _ => {
                    let w = g.workers[idx as usize];
                    let (sx, sy) = self.to_screen(w.x, w.y);
                    let spr = S::WORKER0 + ((w.step / 10) % 2) as usize;
                    self.blit_at(spr, sx, sy, 256, (0, 0));
                }
            }
        }
        // build dust, removal poofs, upgrade sparkles
        for q in 0..self.puffs.len() {
            let (kind, i, j, w, d, st) = self.puffs[q];
            let age = self.frame.wrapping_sub(st) as i32;
            let (cx, cy) = self.to_screen(i as f32 + w as f32 * 0.5, j as f32 + d as f32 * 0.5);
            let r = self.k(6 + age * (w as i32 + d as i32) / 2);
            if kind == 3 { continue; }
            if kind == 4 {
                // compressed-air clean: white puffs blowing off the box
                for st2 in 0..10i32 {
                    let a = st2 as f32 * 0.628;
                    let rr = (4.0 + age as f32 * 0.9) * self.s as f32;
                    let (x, y) = (cx + (cos_approx(a) * rr * 1.5) as i32, cy - self.k(10 + age / 3) + (sin_approx(a) * rr * 0.6) as i32);
                    let al = 200u32.saturating_sub(age as u32 * 7);
                    self.puff(x, y, self.k(2), if st2 % 3 == 0 { rgba(190, 160, 120) } else { rgba(245, 250, 252) }, al);
                }
                continue;
            }
            if kind == 2 {
                for st2 in 0..8i32 {
                    let a = st2 as f32 * 0.785 + age as f32 * 0.08;
                    let rr = (6.0 + age as f32 * 0.6) * self.s as f32;
                    let (x, y) = (cx + (cos_approx(a) * rr * 1.6) as i32, cy - self.k(10 + age / 2) + (sin_approx(a) * rr * 0.8) as i32);
                    let al = 256u32.saturating_sub(age as u32 * 6);
                    let col = if st2 % 2 == 0 { rgba(255, 220, 90) } else { rgba(255, 255, 255) };
                    self.dot(x, y, col, al); self.dot(x + self.s, y, col, al / 2); self.dot(x, y - self.s, col, al / 2);
                }
            } else if age < 26 {
                let al = 170u32.saturating_sub(age as u32 * 6);
                for st2 in 0..14i32 {
                    let a = st2 as f32 * 0.449;
                    let (x, y) = (cx + (cos_approx(a) * r as f32) as i32, cy + (sin_approx(a) * r as f32 * 0.5) as i32 - self.k(age / 3));
                    let col = if kind == 0 { rgba(214, 200, 172) } else { rgba(200, 200, 194) };
                    self.puff(x, y, self.k(3), col, al);
                }
            }
        }
        // power lines between poles and what they feed
        for &(a, b) in g.links.iter() {
            let (pa, pb) = (self.anchor_top(g, a as usize), self.anchor_top(g, b as usize));
            let col = if self.view & VIEW_GRID != 0 { let c = NET_COLS[g.blds[a as usize].net as usize % 6]; rgba(c.0, c.1, c.2) } else { rgba(46, 46, 50) };
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
                let c = if b.temp < 70.0 { rgba(40, 190, 184) } else if b.temp < 80.0 { rgba(240, 180, 30) } else { rgba(235, 70, 50) };
                self.badge(cx, cy - self.k(30), 9, 9, rgba(255, 255, 255), 240);
                self.badge(cx, cy - self.k(28), 5, 5, c, 256);
            }
        }
        // orbital solar: the power beam coming down from orbit onto the receiver (game)
        for b in g.blds.iter() {
            if b.kind != K_ORBITAL || b.off != 0 { continue; }
            let (cx, cy) = self.to_screen(b.i as f32 + 1.0, b.j as f32 + 1.0);
            let top = cy - self.k(22); if cx < -self.k(20) || cx > self.w + self.k(20) || top < 0 { continue; }
            let fl = 40 + (hash(self.frame / 3 + b.i as u32) % 30);
            let hw = self.k(4);
            for y in 0..top { for dx in -hw..=hw {
                let ad = iabs(dx) / self.s;
                let a = if ad == 0 { fl + 80 } else if ad <= 1 { fl + 30 } else { fl / (ad as u32) };
                self.px(cx + dx, y, rgba(210, 240, 255), a);
            } }
            self.light(cx, top, self.k(10), rgba(230, 250, 255), 140);
        }
        self.weather(g);
        self.day_night(g);
        // hover outline (mouse)
        if self.tool < 0 {
            if let Some(t) = g.tile(self.hover.0, self.hover.1) {
                if t.occ >= 0 && t.occ as i32 != self.sel {
                    let b = g.blds[t.occ as usize]; let k = KINDS[b.kind as usize];
                    self.outline(b.i as i32, b.j as i32, k.w as i32, k.d as i32, rgba(255, 255, 255), false);
                }
            }
        }
        // selection
        let sel_col = rgba(17, 20, 24);
        if self.sel >= 0 && (self.sel as usize) < g.blds.len() {
            let b = g.blds[self.sel as usize]; let k = KINDS[b.kind as usize];
            let p = self.footprint(b.i as i32, b.j as i32, k.w as i32, k.d as i32);
            self.quad(p, rgba(255, 255, 255), 60);
            self.outline(b.i as i32, b.j as i32, k.w as i32, k.d as i32, sel_col, false);
            if b.kind == K_POLE { self.outline(b.i as i32 - 4, b.j as i32 - 4, 9, 9, sel_col, true); }
            if b.kind == K_GREENHOUSE { self.outline(b.i as i32 - HEAT_REACH, b.j as i32 - HEAT_REACH, 2 + 2 * HEAT_REACH, 2 + 2 * HEAT_REACH, rgba(240, 130, 50), true); }
        }
        let (ok_c, bad_c) = (rgba(40, 170, 90), rgba(230, 60, 50));
        // land clearing ghost
        if self.tool == TOOL_CLEAR && self.hover.0 >= 0 {
            let (i, j) = self.hover;
            let c = g.clear_cost(i, j);
            let ok = c > 0.0 && g.cash >= c;
            let p = self.footprint(i, j, 1, 1);
            self.quad(p, if ok { rgba(255, 255, 255) } else { bad_c }, 90);
            self.outline(i, j, 1, 1, if ok { sel_col } else { bad_c }, true);
        }
        // placement ghost
        if self.tool >= 0 && (self.tool as usize) < N_KINDS && self.hover.0 >= 0 {
            let kind = self.tool as u8; let k = KINDS[kind as usize];
            let (i, j) = self.hover;
            let ok = g.can_place(kind, i, j) == 0.0;
            let p = self.footprint(i, j, k.w as i32, k.d as i32);
            self.quad(p, if ok { ok_c } else { bad_c }, 90);
            let spr = self.sprite_for(kind, self.tool_dir);
            let (sx, sy) = self.to_screen(i as f32, j as f32);
            self.blit_at(spr, sx, sy, 160, (0, 0));
            self.outline(i, j, k.w as i32, k.d as i32, if ok { ok_c } else { bad_c }, true);
            if kind == K_FAN { self.draw_lane(g, i, j, self.tool_dir, 1.0); let (cx, cy) = self.to_screen(i as f32 + 0.5, j as f32 + 0.5); self.arrow(cx, cy - self.k(6), self.tool_dir, rgba(40, 190, 184)); }
            if kind == K_POLE { self.outline(i - 4, j - 4, 9, 9, sel_col, true); }
            if kind == K_GREENHOUSE { self.outline(i - HEAT_REACH, j - HEAT_REACH, 2 + 2 * HEAT_REACH, 2 + 2 * HEAT_REACH, rgba(240, 130, 50), true); }
        }
    }

    fn anchor_top(&self, g: &Game, idx: usize) -> (i32, i32) {
        let b = g.blds[idx]; let k = KINDS[b.kind as usize];
        let (x, y) = self.to_screen(b.i as f32 + k.w as f32 * 0.5, b.j as f32 + k.d as f32 * 0.5);
        if b.kind == K_POLE { (x, y - self.k(29)) } else { (x, y - self.k(12)) }
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
                if age < 10 { ay -= self.k((10 - age) * (10 - age) / 3); }
            }
        }
        let (sx, sy) = (ax + self.a2(s.ax), ay + self.a2(s.ay));
        let (sw, sh) = (s.w as i32 * self.s / 2, s.h as i32 * self.s / 2);
        if sx > self.w || sy > self.h || sx + sw <= 0 || sy + sh <= 0 { return; }
        let top = ay + self.a2(s.top);          // topmost solid pixel (the roof), not the shadow
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
            tint = if b.exposed { (rgba(235, 60, 50), 110) } else { (rgba(80, 210, 100), 80) };
        } else if self.view & VIEW_GRID != 0 && k.electric {
            let powered = g.nets.get(b.net as usize).map_or(false, |n| n.sources > 0);
            let c = if powered { NET_COLS[b.net as usize % 6] } else { (235, 60, 50) };
            tint = (rgba(c.0, c.1, c.2), 90);
        }
        self.blit(spr, sx, sy, 256, tint);
        if f > 0.0 {
            for e in 0..s.ne as usize {
                let (ex, ey) = S::EMIT[s.e0 as usize + e];
                self.smoke(ax + self.a2(ex), ay + self.a2(ey), (idx as u32) * 3 + e as u32, false);
            }
        }
        let (cx, cy) = self.to_screen(b.i as f32 + k.w as f32 * 0.5, b.j as f32 + k.d as f32 * 0.5);
        if is_miner(b.kind) {
            if b.broken != 0 {
                for d in -4..=4 { self.dot(cx + self.k(d), top - self.k(4 - d), rgba(235, 60, 50), 256); self.dot(cx + self.k(d), top - self.k(4 + d), rgba(235, 60, 50), 256); }
                if (self.frame / 8) % 2 == 0 { self.smoke(cx, top - self.k(4), idx as u32, true); }
            } else {
                if b.fire != 0 && b.off == 0 { self.fire(cx, top + self.k(4), idx as u32); }
                // fan air reaching this box: animated gusts + a snowflake
                let c = g.cooling_at(&b);
                if c > 0.5 && b.off == 0 && f > 0.0 { self.gusts(cx, cy - self.k(8), c as f32, idx as u32); }
                else if b.temp > 85.0 { let pulse = 160 + ((self.frame % 40) as i32 - 20).unsigned_abs() * 4; self.glow(cx, top + self.k(6), pulse); }
                // mined coins popping out now and then
                if f > 0.0 {
                    let ph = (self.frame + idx as u32 * 53) % 240;
                    if ph < 36 {
                        let (x, y) = (cx + self.k(4 + ph as i32 / 12), top - self.k(2 + ph as i32 / 2));
                        let al = if ph > 24 { 256 - (ph - 24) * 20 } else { 256 };
                        let w = if (ph / 4) % 4 == 1 || (ph / 4) % 4 == 3 { 1 } else { 2 }; // spin
                        for dy in -2..=2 { for dx in -w..=w { self.dot(x + self.k(dx), y + self.k(dy), if dx == -w || dy == -2 { rgba(255, 236, 140) } else { rgba(247, 168, 30) }, al); } }
                        self.dot(x, y, rgba(180, 110, 10), al);
                    }
                }
                if b.health < 90.0 {
                    let w = (b.health as i32 * 14) / 100;
                    let col = if b.health < 30.0 { rgba(235, 70, 50) } else if b.health < 60.0 { rgba(240, 180, 30) } else { rgba(60, 180, 90) };
                    let by = top - self.k(6);
                    self.badge(cx, by, 18, 5, rgba(255, 255, 255), 235);
                    self.rect(cx - self.k(7), by + self.k(2) - self.s / 2, imax(self.s, self.k(w)), self.s, col);
                }
            }
        } else if b.kind == K_FAN {
            self.arrow(cx, cy - self.k(8), b.dir, if f > 0.0 { rgba(40, 190, 184) } else { rgba(150, 150, 150) });
        } else if is_power(b.kind) && b.kind != K_CORE {
            let by = top - self.k(8);
            self.badge(cx, by, 19, 7, rgba(255, 255, 255), 235);
            for q in 0..3 {
                let col = if (q as u8) < b.level { rgba(17, 20, 24) } else { rgba(210, 212, 216) };
                self.badge(cx + self.k(-5 + q * 5), by + self.k(2), 3, 3, col, 256);
            }
            // affordable upgrade: bobbing green arrow badge
            let uc = g.upgrade_cost(idx);
            if uc > 0.0 && g.cash >= uc {
                let bob = if (self.frame / 20) % 2 == 0 { 0 } else { self.s };
                let (bx, by2) = (cx, by - self.k(11) - bob);
                self.badge(bx, by2, 11, 10, rgba(40, 170, 90), 256);
                for t in 0..4 { for w in -t..=t { self.dot(bx + self.k(w), by2 + self.k(2 + t), rgba(255, 255, 255), 256); } }
                self.rect(bx - self.k(1), by2 + self.k(6), self.k(3), self.k(2), rgba(255, 255, 255));
            }
        }
        // switched off: grey power symbol
        if b.off != 0 && b.broken == 0 {
            let (bx, by) = (cx, top - self.k(12));
            self.badge(bx, by - self.k(1), 11, 11, rgba(255, 255, 255), 240);
            for t in 0..7 { let a = t as f32 * 0.9 + 1.2; let (c, s_) = (cos_approx(a), sin_approx(a)); self.dot(bx + self.k((c * 3.0) as i32), by + self.k(4 + (s_ * 3.0) as i32), rgba(110, 114, 120), 256); }
            self.rect(bx, by, self.s, self.k(4), rgba(110, 114, 120));
        }
        // no power: blinking red bolt
        if is_consumer(b.kind) && b.broken == 0 && b.off == 0 && f <= 0.0 && (self.frame / 15) % 2 == 0 {
            let (bx, by) = (cx, top - self.k(14));
            self.badge(bx, by - self.k(1), 11, 12, rgba(255, 255, 255), 240);
            for (x, y) in [(1, 0), (0, 1), (-1, 2), (0, 3), (1, 3), (2, 3), (1, 4), (0, 5), (-1, 6)] { self.dot(bx + self.k(x), by + self.k(1 + y), rgba(230, 60, 50), 256); }
        }
        if self.view & VIEW_GRID != 0 && is_consumer(b.kind) && b.broken == 0 && f > 0.0 && f < 0.999 {
            self.outline(b.i as i32, b.j as i32, k.w as i32, k.d as i32, rgba(240, 180, 30), true);
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
