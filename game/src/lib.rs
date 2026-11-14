#![allow(dead_code)]
//! Power Grid — a Bitcoin mining plant tycoon. Pure Rust, no dependencies,
//! compiled to wasm32-unknown-unknown. JS (web/game.js) drives input and UI.
#![no_std]
#![allow(static_mut_refs)]
extern crate alloc;

mod data;
mod game;
mod math;
mod render;
mod rng;
mod sprites;

use alloc::vec::Vec;
use data::*;
use game::*;
use render::Renderer;

// ---------------------------------------------------------------- allocator
mod heap {
    use core::alloc::{GlobalAlloc, Layout};
    const CLASSES: usize = 20; // 16 B .. 8 MiB
    static mut FREE: [usize; CLASSES] = [0; CLASSES];
    static mut TOP: usize = 0;
    static mut END: usize = 0;

    fn class(size: usize) -> usize { let mut c = 0; let mut s = 16; while s < size { s <<= 1; c += 1; } c }

    unsafe fn bump(size: usize, align: usize) -> *mut u8 {
        unsafe {
            if TOP == 0 { let pages = core::arch::wasm32::memory_size(0); TOP = pages * 65536; END = TOP; }
            let start = (TOP + align - 1) & !(align - 1);
            let end = start + size;
            if end > END {
                let need = (end - END + 65535) / 65536;
                if core::arch::wasm32::memory_grow(0, need) == usize::MAX { return core::ptr::null_mut(); }
                END += need * 65536;
            }
            TOP = end;
            start as *mut u8
        }
    }

    pub struct Alloc;
    unsafe impl GlobalAlloc for Alloc {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            unsafe {
                let size = if l.size() < l.align() { l.align() } else { l.size() };
                let c = class(size);
                if c >= CLASSES { return bump(size, l.align()); }
                let p = FREE[c];
                if p != 0 { FREE[c] = *(p as *const usize); return p as *mut u8; }
                let sz = 16usize << c;
                bump(sz, if sz < 4096 { sz } else { 4096 })
            }
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            unsafe {
                let size = if l.size() < l.align() { l.align() } else { l.size() };
                let c = class(size);
                if c >= CLASSES { return; }
                *(p as *mut usize) = FREE[c];
                FREE[c] = p as usize;
            }
        }
    }
}

#[global_allocator]
static ALLOC: heap::Alloc = heap::Alloc;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { core::arch::wasm32::unreachable() }

// ---------------------------------------------------------------- globals
/// Every site you run. WORLD[0] leads the network; the rest follow it. ACTIVE is the one on screen.
static mut WORLD: Vec<Game> = Vec::new();
static mut ACTIVE: usize = 0;
static mut R: Option<Renderer> = None;
static mut UI: [f64; 600] = [0.0; 600];
static mut BUF: Vec<u8> = Vec::new();
static mut ACC: f64 = 0.0;
static mut SPEED: u32 = 1;
/// Blocks simulated per second while playing (the speed slider). 144 blocks make a day.
static mut RATE: f64 = 4.0;
static mut MKT: Market = Market::fallback();
static mut SITE: u8 = 0;

fn g() -> &'static mut Game { unsafe { let a = ACTIVE.min(WORLD.len() - 1); &mut WORLD[a] } }
/// Anything spent or bought on the site on screen since the last step becomes the world's wallet.
unsafe fn sync_wallet() {
    unsafe {
        let n = WORLD.len(); if n < 2 { return; }
        let a = ACTIVE.min(n - 1);
        let (cash, btc, stock) = (WORLD[a].cash, WORLD[a].btc, WORLD[a].stock);
        let mut unl = [false; N_TIERS]; let mut unx = [false; 2];
        for gm in WORLD.iter() { for t in 0..N_TIERS { unl[t] |= gm.unlocked[t]; } for t in 0..2 { unx[t] |= gm.unlocked_x[t]; } }
        for gm in WORLD.iter_mut() { gm.cash = cash; gm.btc = btc; gm.stock = stock; gm.unlocked = unl; gm.unlocked_x = unx; }
    }
}
/// One block for every site. Sites not on screen have their events handled by the site manager (first choice).
unsafe fn step_world() {
    unsafe {
        let n = WORLD.len(); let a = ACTIVE.min(n - 1);
        if n == 1 { WORLD[0].step_block(); return; }
        sync_wallet();
        WORLD[0].step_block();
        if a != 0 && WORLD[0].pending != EV_NONE { WORLD[0].choose(0); }
        for k in 1..n {
            let (lead, rest) = WORLD.split_at_mut(1); let lead = &mut lead[0]; let f = &mut rest[k - 1];
            f.follow(lead); f.block = lead.block - 1;
            f.step_block();
            if k != a && f.pending != EV_NONE { f.choose(0); }
            lead.cash = f.cash; lead.btc = f.btc;
        }
        let (cash, btc) = (WORLD[0].cash, WORLD[0].btc);
        let over = WORLD.iter().map(|gm| gm.over).max().unwrap_or(0);
        for (k, gm) in WORLD.iter_mut().enumerate() { gm.cash = cash; gm.btc = btc; if k != a { gm.toasts.clear(); } if over != 0 { gm.over = over; } }
    }
}
fn r() -> &'static mut Renderer { unsafe { R.as_mut().unwrap() } }

// ---------------------------------------------------------------- exports
#[no_mangle]
pub extern "C" fn atlas_buf() -> *mut u8 {
    unsafe {
        if R.is_none() { R = Some(Renderer::new()); }
        let rr = r();
        rr.atlas.clear();
        rr.atlas.resize(sprites::ATLAS_W * sprites::ATLAS_H, 0);
        rr.atlas.as_mut_ptr() as *mut u8
    }
}
#[no_mangle] pub extern "C" fn atlas_ready() { r().atlas_ready(); }
/// Framebuffer density: 1 = base tile (64x32 px), 2 = hi-res (128x64). Keeps the view centred.
#[no_mangle] pub extern "C" fn set_scale(s: i32) { r().set_scale(s); }
#[no_mangle] pub extern "C" fn atlas_w() -> u32 { sprites::ATLAS_W as u32 }
#[no_mangle] pub extern "C" fn atlas_h() -> u32 { sprites::ATLAS_H as u32 }

#[no_mangle]
pub extern "C" fn new_game(seed: u32, diff: u32) {
    unsafe {
        if R.is_none() { R = Some(Renderer::new()); }
        WORLD.clear(); WORLD.push(Game::new_at(seed, diff.min(2), MKT, SITE)); ACTIVE = 0;
        ACC = 0.0; SPEED = 1; RATE = 4.0;
        let rr = r(); rr.sel = -1; rr.tool = -1; rr.center_on(24.5, 24.5);
        g().dispatch(false);
    }
}

/// Peek at a seed's map traits without starting a run: writes UI[30..34].
#[no_mangle]
pub extern "C" fn preview_seed(seed: u32) {
    let p = unsafe { Game::new_at(seed, 0, MKT, SITE) };
    unsafe { UI[30] = p.t_river as u8 as f64; UI[31] = p.t_sun; UI[32] = p.t_wind; UI[33] = p.t_coal as u8 as f64; }
}

#[no_mangle]
pub extern "C" fn resize(w: i32, h: i32) -> *const u8 { let rr = r(); rr.resize(w, h); rr.fb.as_ptr() as *const u8 }
#[no_mangle] pub extern "C" fn fb_ptr() -> *const u8 { r().fb.as_ptr() as *const u8 }
#[no_mangle] pub extern "C" fn ui_ptr() -> *const f64 { unsafe { UI.as_ptr() } }
#[no_mangle] pub extern "C" fn set_speed(s: u32) { unsafe { SPEED = s.min(3); RATE = [0.0, 4.0, 30.0, 90.0][SPEED as usize]; } }
/// Any speed from the slider, in blocks per second; 0 pauses.
#[no_mangle] pub extern "C" fn set_rate(bps: f64) { unsafe { RATE = if bps > 0.0 { bps.min(200.0) } else { 0.0 }; SPEED = if RATE <= 0.0 { 0 } else if RATE <= 8.0 { 1 } else if RATE <= 60.0 { 2 } else { 3 }; } }
#[no_mangle] pub extern "C" fn pan(dx: i32, dy: i32) { let rr = r(); rr.cam_x += dx; rr.cam_y += dy; rr.clamp_cam(); }
#[no_mangle] pub extern "C" fn center_home() { r().center_on(24.5, 24.5); }
#[no_mangle] pub extern "C" fn set_tool(k: i32) { let rr = r(); rr.tool = if (k >= 0 && (k as usize) < N_KINDS) || k == TOOL_CLEAR { k } else { -1 }; if rr.tool >= 0 { rr.sel = -1; } }

/// Market snapshot from mempool.space (or the cached / fallback one). Used by the next new_game.
#[no_mangle]
pub extern "C" fn set_market(price: f64, net_th: f64, height: f64, remaining: f64, est_change: f64, fees: f64, live: u32) {
    unsafe {
        let fb = Market::fallback();
        MKT = Market {
            price: if price > 1000.0 { price } else { fb.price },
            net_th: if net_th > 1.0e6 { net_th } else { fb.net_th },
            height: if height > 800_000.0 { height as u64 } else { fb.height },
            remaining: if remaining >= 1.0 && remaining <= 2016.0 { remaining as u64 } else { fb.remaining },
            est_change: if math::fabs(est_change) < 0.5 { est_change } else { 0.0 },
            fees: if fees >= 0.0 && fees < 5.0 { fees } else { fb.fees },
            live: live != 0,
        };
    }
}
/// Live price tick (only applies to live-mode runs).
#[no_mangle] pub extern "C" fn set_price(p: f64) { unsafe { if !WORLD.is_empty() { WORLD[0].set_price(p); } } }
/// Open a new site (SITES index). 1 = opened and now on screen, -1 = not enough cash, -2 = not allowed (owned, replay).
#[no_mangle]
pub extern "C" fn open_site(site: u32) -> i32 {
    unsafe {
        if WORLD.is_empty() { return -2; }
        sync_wallet();
        let site = (site as usize).min(N_SITES - 1) as u8;
        let lead = &WORLD[0];
        if lead.scen_id > 0 || WORLD.iter().any(|gm| gm.site == site) { return -2; }
        if lead.cash < OPEN_SITE_USD { WORLD[ACTIVE].toast(T_NO_CASH, 0.0); return -1; }
        let m = Market { price: lead.price, net_th: lead.net_th, height: lead.block, remaining: 1, est_change: 0.0, fees: lead.base_fees, live: lead.live };
        let mut gm = Game::new_at(lead.seed ^ (0x9E37_79B9u32.wrapping_mul(site as u32 + 1)), lead.diff, m, site);
        gm.follower = true; gm.day_off = lead.day + lead.day_off;
        gm.follow(lead); gm.cash = lead.cash - OPEN_SITE_USD; gm.unlocked = lead.unlocked; gm.unlocked_x = lead.unlocked_x;
        gm.dispatch(false);
        WORLD.push(gm); ACTIVE = WORLD.len() - 1;
        sync_wallet();
        let rr = r(); rr.sel = -1; rr.tool = -1; rr.center_on(24.5, 24.5);
        1
    }
}
/// Put another of your sites on screen.
#[no_mangle]
pub extern "C" fn switch_site(k: u32) {
    unsafe {
        if (k as usize) >= WORLD.len() { return; }
        sync_wallet(); ACTIVE = k as usize;
        let rr = r(); rr.sel = -1; rr.tool = -1; rr.center_on(24.5, 24.5);
        WORLD[ACTIVE].dispatch(false);
    }
}
#[no_mangle] pub extern "C" fn rotate_tool() { let rr = r(); rr.tool_dir = (rr.tool_dir + 1) % 4; }
#[no_mangle] pub extern "C" fn rotate_sel() { let s = r().sel; if s >= 0 { g().rotate(s as usize); } }
#[no_mangle] pub extern "C" fn toggle_sel() { let s = r().sel; if s >= 0 { g().toggle(s as usize); } }
#[no_mangle] pub extern "C" fn shed() -> u32 { g().shed() }
#[no_mangle] pub extern "C" fn upgrade_walls() -> u32 { g().upgrade_walls() }
#[no_mangle] pub extern "C" fn clean_sel() { let s = r().sel; if s >= 0 { g().clean(s as usize); } }
#[no_mangle] pub extern "C" fn clean_all() -> u32 { g().clean_all() }
/// Debug helpers (console only)
#[no_mangle] pub extern "C" fn dbg_weather(w: u32, days: u32) { g().set_weather((w % 4) as u8, days.max(1)); }
#[no_mangle] pub extern "C" fn dbg_strike() { g().lightning_pub(); }
#[no_mangle] pub extern "C" fn dbg_cash(c: f64) { g().cash += c; }
/// History scenarios: JS writes n rows of [price, difficulty, fees BTC, flags, curtail $/MWh] here...
#[no_mangle] pub extern "C" fn scen_buf(n: u32) -> *mut f64 { let gg = g(); gg.scen.clear(); gg.scen.resize(n as usize, [0.0; 5]); gg.scen.as_mut_ptr() as *mut f64 }
/// ...then starts the scenario on the current (fresh) game.
#[no_mangle] pub extern "C" fn scen_start(id: u32, height: f64, grid_usd: f64, cash: f64, max_miner: u32, ka: u32, na: u32, kb: u32, nb: u32) {
    g().scen_start(id, height as u64, grid_usd, cash, max_miner as u8, ka as u8, na, kb as u8, nb); unsafe { SPEED = 1; RATE = 4.0; }
}
#[no_mangle] pub extern "C" fn worth() -> f64 { g().worth() }
#[no_mangle] pub extern "C" fn dbg_event(ev: i32) { let gg = g(); if gg.pending == EV_NONE && (0..=14).contains(&ev) { gg.apply_event(ev); } }
#[no_mangle] pub extern "C" fn set_solo(s: u32) { g().set_solo(s != 0); }
/// The core held: keep mining with free power (every site).
#[no_mangle] pub extern "C" fn keep_mining() { unsafe { for gm in WORLD.iter_mut() { gm.keep_mining(); } } }
/// Day and night strength, 0..1 (JS lowers it at high speeds so nights don't strobe).
/// Gold miner containers on or off (cosmetic).
#[no_mangle] pub extern "C" fn set_skin(gold: u32) { r().gold = gold != 0; }
#[no_mangle] pub extern "C" fn set_night(s: f64) { r().night = if s.is_nan() { 0.0 } else { math::clamp(s, 0.0, 1.0) as f32 }; }
/// Where the next new_game happens (SITES index).
#[no_mangle] pub extern "C" fn set_site(s: u32) { unsafe { SITE = (s as usize).min(N_SITES - 1) as u8; } }
/// Crew and spares.
#[no_mangle] pub extern "C" fn set_crew(techs: u32, guards: u32) { g().set_crew(techs, guards); }
#[no_mangle] pub extern "C" fn set_staff(cleaners: u32, mechanics: u32) { g().set_staff(cleaners, mechanics); }
/// Sell surplus power to the city: on/off and the lowest $/MWh you'll take.
#[no_mangle] pub extern "C" fn set_sell(on: u32, floor: f64) { g().set_sell(on != 0, floor); }
#[no_mangle] pub extern "C" fn buy_upgrade(bit: u32) -> u32 { let gg = g(); let r = gg.buy_upgrade(bit); r }
#[no_mangle] pub extern "C" fn buy_spares(part: u32, n: u32) { g().buy_spares(part as usize, n); }
#[no_mangle] pub extern "C" fn set_spare_target(part: u32, n: u32) { g().set_spare_target(part as usize, n); }
#[no_mangle] pub extern "C" fn fix_sel() { let s = r().sel; if s >= 0 { g().fix_parts(s as usize); } }
/// Select the box with the most failed parts. 1 if found.
#[no_mangle] pub extern "C" fn select_worst_parts() -> i32 {
    let rr = r(); let gg = g();
    let best = (0..gg.blds.len()).filter(|&x| is_miner(gg.blds[x].kind) && gg.blds[x].parts_out() > 0).max_by_key(|&x| gg.blds[x].parts_out());
    match best { Some(x) => { let b = gg.blds[x]; rr.tool = -1; rr.sel = x as i32; rr.center_on(b.i as f32 + 0.5, b.j as f32 + 2.0); 1 } None => 0 }
}
/// Join pool 0..2 (leaves solo mining).
#[no_mangle] pub extern "C" fn set_pool(p: u32) { g().set_pool(p.min(255) as u8); }
/// Firmware: profile -2..2 on the selected miner, or on every miner of the selected one's kind. Returns changes.
#[no_mangle] pub extern "C" fn tune_sel(t: i32) -> u32 { let s = r().sel; if s >= 0 { g().set_tune(s as usize, t) } else { 0 } }
#[no_mangle] pub extern "C" fn tune_same(t: i32) -> u32 { let s = r().sel; let gg = g(); if s >= 0 && (s as usize) < gg.blds.len() { let k = gg.blds[s as usize].kind; gg.tune_kind(k, t) } else { 0 } }
/// Power contract: 0 fixed, 1 spot, 2 fixed + demand response. Returns 1 if it changed.
#[no_mangle] pub extern "C" fn set_contract(c: u32) -> u32 { g().set_contract(c.min(255) as u8) }
/// Spot only: 0 = each box switches off at its own break-even; > 0 = the grid feed goes dark above this $/MWh.
#[no_mangle] pub extern "C" fn set_spot_cap(usd: f64) { g().set_spot_cap(usd); }
/// ASIC market: 1 = the next miner placed is bought used (from the weekly listings), 0 = new.
#[no_mangle] pub extern "C" fn set_used(on: u32) { g().set_used(on != 0); }
#[no_mangle] pub extern "C" fn wake_all() { g().wake_all(); }
#[no_mangle] pub extern "C" fn set_view(mask: u32) { r().view = mask; }
/// Select (and centre on) the n-th building of a kind, cycling. Returns 1 if found.
#[no_mangle]
pub extern "C" fn select_kind(kind: u32, n: u32) -> i32 {
    let rr = r(); let gg = g();
    let idx: Vec<usize> = (0..gg.blds.len()).filter(|&x| gg.blds[x].kind as u32 == kind).collect();
    if idx.is_empty() { return 0; }
    let x = idx[n as usize % idx.len()];
    let b = gg.blds[x]; let k = KINDS[b.kind as usize];
    rr.tool = -1; rr.sel = x as i32; rr.center_on(b.i as f32 + k.w as f32 * 0.5, b.j as f32 + k.d as f32 * 0.5 + 1.5);
    1
}
/// Select the cheapest power source that can still be upgraded.
#[no_mangle]
pub extern "C" fn select_upgradable() -> i32 {
    let rr = r(); let gg = g();
    let mut best = usize::MAX; let mut bc = f64::MAX;
    for x in 0..gg.blds.len() { if !is_power(gg.blds[x].kind) { continue; } let c = gg.upgrade_cost(x); if c > 0.0 && c < bc { bc = c; best = x; } }
    if best == usize::MAX { return 0; }
    let b = gg.blds[best]; let k = KINDS[b.kind as usize];
    rr.tool = -1; rr.sel = best as i32; rr.center_on(b.i as f32 + k.w as f32 * 0.5, b.j as f32 + k.d as f32 * 0.5 + 1.5);
    1
}
/// Drag-painting for fences and land clearing. Quiet: no toasts for occupied tiles. 1 = did something.
#[no_mangle]
pub extern "C" fn paint_at(sx: i32, sy: i32) -> i32 { let (i, j) = r().pick(sx, sy); paint_tile(i, j) }
#[no_mangle]
pub extern "C" fn paint_tile(i: i32, j: i32) -> i32 {
    let rr = r(); let gg = g();
    rr.hover = (i, j);
    if rr.tool == TOOL_CLEAR { return gg.clear(i, j, true) as i32; }
    if rr.tool == K_WALL as i32 {
        let why = gg.can_place(K_WALL, i, j);
        if why == 0.0 { gg.place(K_WALL, i, j, 0); return 1; }
        if why == T_NO_CASH { gg.toast(T_NO_CASH, 0.0); return -1; }
    }
    // a row of boxes: dragging with a miner in hand places one on every free tile it crosses
    if rr.tool >= K_MINER1 as i32 && rr.tool <= K_MINER4 as i32 {
        let k = rr.tool as u8;
        let why = gg.can_place(k, i, j);
        if why == 0.0 { gg.place(k, i, j, 0); return 1; }
        if why == T_NO_CASH || why == T_NO_STOCK { gg.toast(why, k as f64); return -1; }
    }
    0
}
/// Backers' milestones: the next one (-1 = none), how far along it is, and its table row (what: 0 kind, 1 target, 2 cash).
#[no_mangle] pub extern "C" fn milestone_next() -> i32 { g().milestone_next() }
#[no_mangle] pub extern "C" fn milestone_progress() -> f64 { g().milestone_progress() }
#[no_mangle] pub extern "C" fn milestone_info(m: u32, what: u32) -> f64 {
    match MILESTONES.get(m as usize) { Some(x) => match what { 0 => x.0 as f64, 1 => x.1, _ => x.2 }, None => 0.0 }
}
/// The next step up the ladder here: its cost (0 = nothing left), its kind, and whether it is an upgrade.
#[no_mangle] pub extern "C" fn next_rung_cost() -> f64 { g().next_rung().map_or(0.0, |x| x.0) }
#[no_mangle] pub extern "C" fn next_rung_kind() -> i32 { g().next_rung().map_or(-1, |x| x.1 as i32) }
#[no_mangle] pub extern "C" fn next_rung_upgrade() -> i32 { g().next_rung().map_or(0, |x| x.2 as i32) }
#[no_mangle] pub extern "C" fn hover(sx: i32, sy: i32) { let rr = r(); let t = rr.pick(sx, sy); rr.hover = t; }
#[no_mangle] pub extern "C" fn hover_tile(i: i32, j: i32) { r().hover = (i, j); }

/// Tap/click at screen coords. Returns: 1 placed, 2 selected, 0 nothing, <0 failed (toast queued).
#[no_mangle]
pub extern "C" fn click(sx: i32, sy: i32) -> i32 {
    let rr = r(); let gg = g();
    let (i, j) = rr.pick(sx, sy);
    // Clicking an existing building while holding a build tool selects it instead of failing.
    if rr.tool >= 0 && rr.tool != K_WALL as i32 && rr.tool != TOOL_CLEAR {
        if let Some(t) = gg.tile(i, j) { if t.occ >= 0 { rr.tool = -1; rr.sel = t.occ as i32; return 3; } }
    }
    if rr.tool == TOOL_CLEAR { return if gg.clear(i, j, false) { 1 } else { -1 }; }
    if rr.tool >= 0 {
        let res = gg.place(rr.tool as u8, i, j, rr.tool_dir);
        return if res == 0.0 { 1 } else { -1 };
    }
    match gg.tile(i, j) {
        Some(t) if t.occ >= 0 => { rr.sel = t.occ as i32; 2 }
        _ => { rr.sel = -1; 0 }
    }
}
/// Refresh the UI mailbox right now (after a click, before the next frame).
/// Building index under a screen point, or -1.
#[no_mangle] pub extern "C" fn occ_at(sx: i32, sy: i32) -> i32 { let (i, j) = r().pick(sx, sy); match g().tile(i, j) { Some(t) => t.occ as i32, None => -1 } }
#[no_mangle] pub extern "C" fn sync_ui() { unsafe { sync_wallet(); fill_ui(g(), r()); } }
#[no_mangle] pub extern "C" fn select_none() { r().sel = -1; }
#[no_mangle] pub extern "C" fn upgrade_sel() { let s = r().sel; if s >= 0 { g().upgrade(s as usize); } }
#[no_mangle] pub extern "C" fn repair_sel() { let s = r().sel; if s >= 0 { g().repair(s as usize); } }
/// The one-stop service: health, dust and failed parts in a single visit.
#[no_mangle] pub extern "C" fn service_sel() { let s = r().sel; if s >= 0 { g().service(s as usize); } }
#[no_mangle] pub extern "C" fn sell_sel() { let rr = r(); if rr.sel >= 0 { g().sell(rr.sel as usize); rr.sel = -1; } }
#[no_mangle] pub extern "C" fn sell_btc(pct: u32) { g().sell_btc(pct.min(100) as f64 / 100.0); }
#[no_mangle] pub extern "C" fn choose(c: u32) { g().choose(c); }
#[no_mangle] pub extern "C" fn toasts_ack() { g().toasts.clear(); }

#[no_mangle]
pub extern "C" fn save() -> u32 {
    unsafe {
        sync_wallet();
        if WORLD.len() == 1 { WORLD[0].save(&mut BUF); return BUF.len() as u32; }
        // several sites: [1000.0, count, active] then each site's own save, length-prefixed
        let mut out: Vec<u8> = Vec::new(); let mut tmp: Vec<u8> = Vec::new();
        for v in [1000.0f64, WORLD.len() as f64, ACTIVE as f64] { out.extend_from_slice(&v.to_le_bytes()); }
        for gm in WORLD.iter() { gm.save(&mut tmp); out.extend_from_slice(&(tmp.len() as u32).to_le_bytes()); out.extend_from_slice(&tmp); }
        BUF = out; BUF.len() as u32
    }
}
#[no_mangle] pub extern "C" fn save_ptr() -> *const u8 { unsafe { BUF.as_ptr() } }
#[no_mangle]
pub extern "C" fn load_buf(n: u32) -> *mut u8 { unsafe { BUF.clear(); BUF.resize(n as usize, 0); BUF.as_mut_ptr() } }
#[no_mangle]
pub extern "C" fn load() -> i32 {
    unsafe {
        if R.is_none() { R = Some(Renderer::new()); }
        let head = if BUF.len() >= 24 { let mut a = [0u8; 8]; a.copy_from_slice(&BUF[0..8]); f64::from_le_bytes(a) } else { 0.0 };
        let mut sites: Vec<Game> = Vec::new(); let mut active = 0usize;
        if head == 1000.0 {
            let rd = |o: usize| { let mut a = [0u8; 8]; a.copy_from_slice(&BUF[o..o + 8]); f64::from_le_bytes(a) };
            let n = rd(8) as usize; active = rd(16) as usize; let mut p = 24usize;
            if n == 0 || n > N_SITES { return 0; }
            for _ in 0..n {
                if p + 4 > BUF.len() { return 0; }
                let len = u32::from_le_bytes([BUF[p], BUF[p + 1], BUF[p + 2], BUF[p + 3]]) as usize; p += 4;
                if p + len > BUF.len() { return 0; }
                match Game::load(&BUF[p..p + len]) { Some(gm) => sites.push(gm), None => return 0 }
                p += len;
            }
        } else {
            match Game::load(&BUF) { Some(gm) => sites.push(gm), None => return 0 }
        }
        WORLD = sites; ACTIVE = active.min(WORLD.len() - 1);
        let rr = r(); rr.sel = -1; rr.tool = -1; rr.center_on(24.5, 24.5); 1
    }
}

/// Advance the simulation by `dt_ms`, draw, and refresh the UI mailbox.
#[no_mangle]
pub extern "C" fn frame(dt_ms: f64) {
    unsafe {
        let bps = RATE;
        ACC += dt_ms.min(250.0) / 1000.0 * bps;
        let mut steps = 0;
        let cap = if WORLD.len() > 1 { 400 / WORLD.len() as u32 + 1 } else { 400 };
        while ACC >= 1.0 && steps < cap { step_world(); ACC -= 1.0; steps += 1; if g().pending != EV_NONE || g().over != 0 { break; } }
        let gg = g();
        if gg.pending != EV_NONE || gg.over != 0 { ACC = 0.0; }
        if SPEED > 0 && gg.pending == EV_NONE { gg.tick_workers(); }
        let rr = r();
        rr.draw(gg);
        fill_ui(gg, rr);
        fill_world();
    }
}

/// Per-site summaries for the sites sheet: 7 numbers per site at u[520..562], then count, active, open cost.
unsafe fn fill_world() {
    unsafe {
        let u = &mut UI;
        for k in 0..N_SITES {
            let o = 520 + k * 7;
            if let Some(gm) = WORLD.get(k) {
                let wages = gm.wages();
                let alerts = gm.blds.iter().filter(|b| is_miner(b.kind) && (b.broken != 0 || b.parts_out() > 0)).count() as u32 + gm.short_nets + gm.unpowered;
                u[o] = gm.site as f64; u[o + 1] = gm.my_th; u[o + 2] = gm.used_kw; u[o + 3] = gm.cap_kw;
                u[o + 4] = gm.rev_day - gm.cost_day + gm.heat_day() + gm.export_day - wages; u[o + 5] = alerts as f64;
                u[o + 6] = gm.blds.iter().filter(|b| is_miner(b.kind)).count() as f64;
            } else { for x in 0..7 { u[o + x] = -1.0; } }
        }
        u[562] = WORLD.len() as f64; u[563] = ACTIVE as f64; u[564] = OPEN_SITE_USD;
        u[565] = WORLD.iter().map(|gm| gm.fleet_value()).sum();
        { let gm = g(); for (x, k) in [K_DEEPGEO, K_ORBITAL].iter().enumerate() {
            u[566 + x * 2] = gm.blds.iter().filter(|b| b.kind == *k).count() as f64;
            u[567 + x * 2] = gm.blds.iter().filter(|b| b.kind == *k).map(|b| b.level).max().unwrap_or(0) as f64;
        } }
    }
}

unsafe fn fill_ui(gg: &Game, rr: &Renderer) {
    unsafe {
        let u = &mut UI;
        u[0] = gg.cash; u[1] = gg.btc; u[2] = gg.price; u[3] = gg.rev_day - gg.cost_day + gg.heat_day() + gg.export_day - gg.wages(); u[4] = gg.used_kw; u[5] = gg.cap_kw;
        u[6] = gg.block as f64; u[7] = gg.next_retarget.saturating_sub(gg.block) as f64; u[8] = (HALVING - gg.block % HALVING) as f64;
        u[9] = gg.reward; u[10] = gg.last_retarget; u[11] = gg.my_th; u[12] = gg.net_th; u[13] = gg.streak as f64;
        u[14] = gg.halvings as f64; u[15] = gg.day as f64; u[16] = gg.ambient(); u[17] = SPEED as f64;
        u[18] = gg.pending as f64; u[19] = gg.roll as f64; u[20] = gg.ev_arg; u[21] = gg.over as f64;
        u[22] = if gg.net_th > 0.0 { 144.0 * (gg.reward + gg.fees) * gg.price / gg.net_th } else { 0.0 };
        u[23] = gg.fees; u[24] = gg.cost_day; u[25] = gg.rev_day; u[26] = (gg.block % 144) as f64 / 6.0;
        u[27] = gg.gas_f; u[28] = gg.wind_now; u[30] = gg.t_river as u8 as f64; u[31] = gg.t_sun; u[32] = gg.t_wind; u[33] = gg.t_coal as u8 as f64;
        u[34] = gg.diff as f64; u[35] = gg.seed as f64; u[36] = rr.tool as f64; u[39] = gg.unpaid_days as f64;
        let mut hot = 0.0; let mut broken = 0.0; let mut miners = 0.0;
        for b in gg.blds.iter() { if is_miner(b.kind) { miners += 1.0; if b.broken != 0 { broken += 1.0; } else if b.temp > 80.0 { hot += 1.0; } } }
        u[40] = hot; u[41] = broken; u[42] = miners; u[43] = gg.frac;
        u[44] = gg.blds.iter().filter(|b| b.fire != 0 && b.off == 0 && b.broken == 0).count() as f64;
        u[46] = gg.blds.iter().filter(|b| is_miner(b.kind) && b.broken == 0 && b.off == 0 && gg.net_frac(b) > 0.0).count() as f64;
        u[47] = gg.blds.iter().filter(|b| b.kind == K_FAN && b.off == 0 && gg.net_frac(b) > 0.0).count() as f64;
        // hovered building (mouse tooltip)
        u[260] = -1.0;
        if rr.tool < 0 && rr.hover.0 >= 0 {
            if let Some(t) = gg.tile(rr.hover.0, rr.hover.1) {
                if t.occ >= 0 {
                    let x = t.occ as usize; let b = gg.blds[x]; let k = KINDS[b.kind as usize];
                    let f = if b.off != 0 { 0.0 } else { gg.net_frac(&b) };
                    u[260] = x as f64; u[261] = b.kind as f64; u[262] = b.temp as f64; u[263] = b.health as f64;
                    u[264] = gg.cooling_at(&b); u[265] = b.off as f64; u[266] = b.broken as f64; u[267] = b.fire as f64;
                    u[268] = f; u[269] = gg.nets.get(b.net as usize).map_or(0.0, |n| n.sources as f64);
                    let hot = if b.temp > 80.0 { 0.6 } else { 1.0 };
                    u[270] = if is_miner(b.kind) && b.broken == 0 { k.th * b.tune_th() * b.parts_f() * hot * f * (0.7 + 0.3 * b.health as f64 / 100.0) } else { 0.0 };
                    let (sx, sy) = rr.to_screen(b.i as f32 + k.w as f32 * 0.5, b.j as f32 + k.d as f32 * 0.5);
                    u[271] = sx as f64; u[272] = sy as f64; u[273] = b.exposed as u8 as f64; u[274] = b.level as f64;
                    u[275] = if is_power(b.kind) { k.cap_kw[(b.level - 1) as usize] } else { 0.0 };
                    u[276] = gg.packed_at(x) as f64; u[277] = b.dir as f64; u[290] = b.dust as f64;
                }
            }
        }
        for t in 0..N_TIERS {
            u[50 + t] = gg.unlocked[t] as u8 as f64;
            let mut ml = 0.0; let mut cnt = 0.0;
            for b in gg.blds.iter() { if b.kind as usize == t { cnt += 1.0; if b.level as f64 > ml { ml = b.level as f64; } } }
            u[60 + t] = ml; u[70 + t] = cnt;
        }
        // selection
        u[80] = rr.sel as f64;
        if rr.sel >= 0 && (rr.sel as usize) < gg.blds.len() {
            let b = gg.blds[rr.sel as usize]; let k = KINDS[b.kind as usize];
            u[81] = b.kind as f64; u[82] = b.level as f64; u[83] = b.temp as f64;
            let hotf = if b.temp > 80.0 { 0.6 } else { 1.0 };
            u[84] = if is_miner(b.kind) && b.broken == 0 { k.th * b.tune_th() * b.parts_f() * hotf } else { 0.0 };
            u[85] = k.kw * b.tune_kw() * b.parts_f();
            for p in 0..3 { u[506 + p] = b.dead[p] as f64; } u[509] = gg.fix_cost(rr.sel as usize); u[342] = b.tune as f64; u[343] = gg.blds.iter().filter(|o| o.kind == b.kind).count() as f64; u[87] = b.broken as f64; u[88] = gg.upgrade_cost(rr.sel as usize); u[89] = b.invested * 0.4;
            u[90] = if is_power(b.kind) { k.cap_kw[(b.level - 1) as usize] } else { 0.0 };
            u[93] = if k.th > 0.0 { k.kw * b.tune_kw() * 1000.0 / (k.th * b.tune_th()) } else { 0.0 }; u[94] = gg.usd_mwh(b.kind);
            u[86] = if is_miner(b.kind) && b.broken == 0 {
                let btc_day = u[84] * (1.0 - b.reject()) * (1.0 - gg.stale_rate()) / gg.net_th * (gg.reward + gg.fees) * (1.0 - gg.pool_fee()) * 144.0;
                let avg_usd = if gg.used_kw > 0.0 { gg.cost_day / 24.0 / gg.used_kw * 1000.0 } else { 0.0 }; // $/MWh blended
                btc_day * gg.price - u[85] * 24.0 / 1000.0 * avg_usd
            } else { 0.0 };
            let (sx, sy) = rr.to_screen(b.i as f32 + k.w as f32 * 0.5, b.j as f32 + k.d as f32 * 0.5);
            u[91] = sx as f64; u[92] = sy as f64; u[95] = k.w as f64; u[96] = k.d as f64; u[97] = k.th;
            u[99] = b.dir as f64; u[289] = b.dust as f64; u[596] = gg.is_liquid(rr.sel as usize) as u8 as f64; u[45] = gg.cooling_at(&b); u[48] = gg.packed_at(rr.sel as usize) as f64;
            let si = rr.sel as usize; u[466] = if is_miner(b.kind) { b.reject() } else { 0.0 };
            u[387] = gg.gh_kw.get(si).copied().unwrap_or(0.0); u[388] = gg.piped.iter().filter(|p| **p == si as i32).count() as f64;
            u[389] = gg.piped.get(si).map_or(0.0, |p| (*p >= 0) as u8 as f64);
            u[234] = b.health as f64; u[235] = gg.sell_value(rr.sel as usize); u[236] = gg.service_cost(rr.sel as usize); u[582] = gg.service_capped(rr.sel as usize) as u8 as f64;
            u[89] = u[235];
            u[237] = if b.net == u16::MAX { -1.0 } else { b.net as f64 };
            if let Some(nt) = gg.nets.get(b.net as usize) {
                u[238] = nt.cap * (1.0 - nt.loss); u[239] = nt.demand; u[244] = nt.sources as f64; u[250] = nt.frac; u[251] = nt.loss;
            } else { u[238] = 0.0; u[239] = 0.0; u[244] = 0.0; u[250] = 0.0; u[251] = 0.0; }
            u[242] = b.off as f64; u[249] = b.exposed as u8 as f64; u[247] = b.fire as f64;
            u[84] = if is_miner(b.kind) && b.broken == 0 && b.off == 0 { u[84] * u[250] * (0.7 + 0.3 * b.health as f64 / 100.0) } else { 0.0 };
            if b.off != 0 || b.broken != 0 { u[86] = 0.0; }
            else if is_miner(b.kind) {
                // what it really makes right now: after heat, health and any shortage on its network
                let btc_day = u[84] * (1.0 - b.reject()) * (1.0 - gg.stale_rate()) / gg.net_th * (gg.reward + gg.fees) * (1.0 - gg.pool_fee()) * 144.0;
                let avg_usd = if gg.used_kw > 0.0 { gg.cost_day / 24.0 / gg.used_kw * 1000.0 } else { gg.grid_usd };
                u[86] = btc_day * gg.price - u[85] * hotf * u[250] * 24.0 / 1000.0 * avg_usd;
            }
        }
        // toasts
        let n = gg.toasts.len().min(15);
        u[100] = n as f64;
        for (x, t) in gg.toasts.iter().take(15).enumerate() { u[101 + x * 2] = t.0; u[102 + x * 2] = t.1; }
        // core requirements
        let req = gg.core_reqs();
        for x in 0..4 { u[140 + x] = req[x] as u8 as f64; }
        u[144] = gg.core_cooldown as f64; u[145] = gg.core_bonus() as f64;
        // costs + unlocks per kind
        for k in 0..N_KINDS {
            let cost = if is_miner(k as u8) { gg.new_price(k as u8) } else { KINDS[k].cost };
            let unl = gg.kind_unlocked(k as u8) as u8 as f64;
            if k < 20 { u[150 + k] = cost; u[170 + k] = KINDS[k].up2; u[190 + k] = KINDS[k].up3; u[210 + k] = unl; }
            u[400 + k] = cost; u[430 + k] = unl; // build .15+: room for 30 kinds
        }
        // heat reuse
        u[390] = gg.heat_price(); u[391] = gg.heat_kw; u[392] = gg.heat_day(); u[393] = gg.heat_earned;
        u[394] = gg.blds.iter().filter(|b| b.kind == K_GREENHOUSE).count() as f64; u[395] = HEAT_CAP_KW;
        u[396] = gg.piped.iter().filter(|p| **p >= 0).count() as f64;
        // pools (build .16)
        u[397] = gg.pool as f64; u[398] = gg.stale_rate(); u[399] = if gg.my_th > 0.0 { 1.0 - gg.acc_th / gg.my_th } else { 0.0 };
        u[460] = if gg.pplns_exp > 0.0 { gg.pplns_paid / gg.pplns_exp } else { 0.0 }; u[461] = gg.pplns_paid; u[462] = gg.pplns_exp;
        u[463] = gg.pool_fee(); { let p = POOLS[gg.pool as usize % POOLS.len()]; u[464] = p.ms; u[465] = p.share; u[469] = p.pplns as u8 as f64; }
        u[467] = gg.pool_switches as f64; u[468] = gg.pool_blocks as f64;
        // crew and spares (build .17)
        u[480] = gg.techs as f64; u[481] = gg.guards as f64;
        let mut dead_now = [0u32; 3]; let mut boxes = 0;
        for b in gg.blds.iter() { if is_miner(b.kind) { for p in 0..3 { dead_now[p] += b.dead[p] as u32; } if b.parts_out() > 0 { boxes += 1; } } }
        for p in 0..3 {
            u[482 + p] = gg.spares[p] as f64; u[485 + p] = gg.spare_target[p] as f64; u[488 + p] = dead_now[p] as f64;
            u[491 + p] = gg.fails[p] as f64; u[494 + p] = gg.swapped[p] as f64; u[499 + p] = PART_USD[p];
        }
        u[497] = gg.wages(); u[498] = gg.wages_paid;
        // maintenance staff and site upgrades (build .38, u[570..582])
        u[570] = gg.cleaners as f64; u[571] = gg.mechanics as f64; u[572] = CLEANER_WAGE; u[573] = MECH_WAGE;
        u[574] = gg.upgrades as f64; u[575] = FILTER_USD; u[576] = MONITOR_USD;
        { let (mut n, mut hs, mut low) = (0.0, 0.0, 0.0); for b in gg.blds.iter() { if is_miner(b.kind) && b.broken == 0 { n += 1.0; hs += b.health as f64; if b.health < 70.0 { low += 1.0; } } }
          u[577] = if n > 0.0 { hs / n } else { 100.0 }; u[578] = low; }
        u[579] = CLEANER_CLEANS as f64; u[580] = MECH_SERVICES as f64; u[581] = MECH_HEAL as f64;
        // batteries and selling power (build .41, u[583..593])
        u[583] = gg.batt_mwh; u[584] = gg.batt_cap; u[585] = gg.batt_flow; u[586] = gg.feed_now;
        u[587] = gg.sell_on as u8 as f64; u[588] = gg.sell_floor; u[589] = gg.export_day; u[590] = gg.export_usd; u[591] = gg.export_mwh;
        u[592] = (!gg.st().flare) as u8 as f64; u[593] = gg.export_kw;
        { let (hot, _) = gg.hot_boxes(); u[594] = hot as f64; u[595] = gg.blds.iter().filter(|b| is_miner(b.kind) && b.broken == 0).count() as f64; }
        u[502] = TECH_WAGE; u[503] = GUARD_WAGE; u[504] = TECH_SWAPS as f64; u[505] = CALLOUT_USD;
        u[510] = boxes as f64; u[511] = gg.parts_lost_th;
        u[512] = gg.site as f64; u[513] = gg.well_f; u[514] = (gg.st().dry && gg.dry_season()) as u8 as f64; u[515] = gg.grid_f();
        u[516] = gg.st().market as u8 as f64; u[517] = gg.tariffs as f64; u[518] = gg.st().duty; u[519] = gg.st().callout;
        u[98] = rr.tool_dir as f64;
        u[240] = gg.unpowered as f64; u[241] = gg.live as u8 as f64; u[243] = gg.est_change;
        u[245] = gg.short_kw; u[246] = gg.short_nets as f64; u[248] = gg.exposed_miners as f64; u[252] = rr.view as f64;
        u[253] = gg.off_count as f64; u[278] = gg.fence_level as f64; u[284] = gg.weather as f64; u[291] = gg.strikes as f64; u[292] = gg.retargets as f64; u[293] = gg.solo as u8 as f64; u[294] = if gg.my_th > 0.0 { gg.net_th / gg.my_th / 144.0 } else { 0.0 }; u[295] = gg.blocks_found as f64; u[296] = gg.last_found as f64; u[297] = gg.reward; u[285] = gg.weather_days as f64;
        { let mut dusty = 0.0; let mut sum = 0.0; let mut n = 0.0; for b in gg.blds.iter() { if is_miner(b.kind) || b.kind == K_FAN || b.kind == K_IMMERSION { n += 1.0; sum += b.dust as f64; if b.dust >= 30.0 { dusty += 1.0; } } } u[286] = if n > 0.0 { sum / n } else { 0.0 }; u[287] = dusty; u[288] = dusty * CLEAN_COST; } u[279] = gg.blds.iter().filter(|b| b.kind == K_WALL).count() as f64; u[280] = gg.blds.iter().filter(|b| b.kind == K_SIGN).count() as f64;
        { let lo = gg.blds.iter().filter(|b| b.kind == K_WALL && b.level < 3).map(|b| b.level).min(); u[281] = lo.map_or(0.0, |l| l as f64); u[282] = gg.blds.iter().filter(|b| b.kind == K_WALL && Some(b.level) == lo).count() as f64; u[283] = match lo { Some(1) => KINDS[K_WALL as usize].up2, Some(2) => KINDS[K_WALL as usize].up3, _ => 0.0 }; } u[254] = gg.raids as f64; u[255] = gg.blocks_found as f64;
        // fee market, demand response, contract, miner fleet (build .9)
        u[298] = gg.fee_mult; u[299] = gg.fee_days as f64; u[300] = gg.curtail_blocks as f64; u[301] = gg.curtail_mode as f64;
        u[302] = gg.curtails as f64; u[303] = gg.curtail_earned; u[304] = gg.fee_spikes as f64; u[305] = gg.usd_mwh(K_GRID);
        u[306] = gg.hp_eq; u[307] = gg.grid_kw; u[316] = gg.curtail_pay; u[317] = gg.curtail_extra;
        for m in 0..4 { u[308 + m] = REAL_PRICE[m] * gg.new_price(K_MINER1 + m as u8) / KINDS[K_MINER1 as usize + m].cost; u[312 + m] = gg.blds.iter().filter(|b| b.kind == K_MINER1 + m as u8).count() as f64; }
        u[318] = gg.scen_id as f64; u[319] = gg.scen_worth0;
        u[49] = gg.scen_hold_btc; u[256] = gg.spot_x;
        // ASIC market (build .12)
        u[320] = gg.fleet_value(); u[321] = gg.mkt_idx; u[341] = gg.mkt_raw();
        for m in 0..4 { let kind = K_MINER1 + m as u8;
            u[322 + m] = gg.new_price(kind); u[326 + m] = gg.used_price(kind); u[330 + m] = gg.stock[m] as f64; u[334 + m] = gg.margin(kind); }
        u[344] = gg.blds.iter().filter(|b| is_miner(b.kind) && b.tune < 0).count() as f64;
        u[345] = gg.blds.iter().filter(|b| is_miner(b.kind) && b.tune > 0).count() as f64;
        u[338] = gg.buy_used as u8 as f64;
        // power contract (build .14)
        u[346] = gg.contract as f64; u[347] = gg.spot_now(); u[348] = gg.spot_cap; u[349] = gg.spike_blocks as f64; u[350] = gg.spike_usd;
        u[351] = gg.contract_days_left() as f64; u[352] = gg.econ_off as f64; u[353] = gg.capped as u8 as f64;
        u[354] = if gg.c_mwh > 0.01 { gg.c_cost / gg.c_mwh } else { 0.0 }; u[355] = gg.dr_earned; u[356] = gg.cap_blocks as f64 / 6.0;
        u[357] = (0..24).map(|h| gg.spot_at(h)).sum::<f64>() / 24.0; u[358] = DR_STANDBY; u[359] = gg.contract_from as f64;
        for h in 0..24 { u[360 + h] = gg.spot_at(h); } u[384] = gg.hour() as f64; u[385] = gg.switches as f64; u[386] = gg.c_mwh; u[339] = gg.used_bought as f64; u[340] = gg.miners_sold as f64;
        u[29] = { let mut c = f64::MAX; for x in 0..gg.blds.len() { if !is_power(gg.blds[x].kind) { continue; } let uc = gg.upgrade_cost(x); if uc > 0.0 && uc < c { c = uc; } } if c == f64::MAX { 0.0 } else { c } };
        // ghost
        if rr.tool == TOOL_CLEAR && rr.hover.0 >= 0 {
            let c = gg.clear_cost(rr.hover.0, rr.hover.1);
            u[37] = (c > 0.0 && gg.cash >= c) as u8 as f64; u[38] = c;
            let (sx, sy) = rr.to_screen(rr.hover.0 as f32 + 0.5, rr.hover.1 as f32 + 0.5);
            u[230] = sx as f64; u[231] = sy as f64;
        } else if rr.tool >= 0 && rr.hover.0 >= 0 {
            u[37] = (gg.can_place(rr.tool as u8, rr.hover.0, rr.hover.1) == 0.0) as u8 as f64;
            u[38] = gg.place_cost(rr.tool as u8, rr.hover.0, rr.hover.1);
            let k = KINDS[rr.tool as usize];
            let (sx, sy) = rr.to_screen(rr.hover.0 as f32 + k.w as f32 * 0.5, rr.hover.1 as f32 + k.d as f32 * 0.5);
            u[230] = sx as f64; u[231] = sy as f64;
        }
        u[232] = rr.w as f64; u[233] = rr.h as f64;
    }
}

/// Place the active tool at the hovered tile (touch "confirm"). 1 placed, -1 failed.
#[no_mangle]
pub extern "C" fn place_hover() -> i32 {
    let rr = r(); let gg = g();
    if rr.tool < 0 || rr.hover.0 < 0 { return -1; }
    if rr.tool == TOOL_CLEAR { return if gg.clear(rr.hover.0, rr.hover.1, false) { 1 } else { -1 }; }
    if gg.place(rr.tool as u8, rr.hover.0, rr.hover.1, rr.tool_dir) == 0.0 { 1 } else { -1 }
}
/// Hovered tile, packed: i * 1000 + j (or -1).
#[no_mangle] pub extern "C" fn hover_packed() -> i32 { let h = r().hover; if h.0 < 0 { -1 } else { h.0 * 1000 + h.1 } }
