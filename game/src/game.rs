//! Game state, seeded map generation, power networks and the simulation.
use alloc::vec::Vec;
use crate::data::*;
use crate::rng::Rng;
use crate::sprites as S;
use crate::math::*;

pub const MAP: usize = 48;
pub const BLOCKS_PER_DAY: u64 = 144;
pub const RETARGET: u64 = 2016;
pub const HALVING: u64 = 210_000;
pub const POOL_FEE: f64 = 0.02;
pub const G_GRASS: u8 = 0;
pub const G_ROUGH: u8 = 1;
pub const G_WATER: u8 = 2;
pub const G_CONCRETE: u8 = 3;
pub const G_COAL: u8 = 4;

/// Fan directions in tile space: +i, +j, -i, -j.
pub const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
pub const LANE: [f32; 4] = [11.0, 9.0, 7.0, 5.0];

#[derive(Clone, Copy)]
pub struct Tile { pub ground: u8, pub tree: u8, pub occ: i16 }

#[derive(Clone, Copy)]
pub struct Bld {
    pub kind: u8, pub i: u8, pub j: u8, pub level: u8, pub broken: u8, pub temp: f32, pub invested: f64,
    pub dir: u8, pub health: f32, pub net: u16, pub fire: u8,
    pub off: u8,        // switched off by the player
    pub dust: f32,      // 0..100: clogged filters run hotter, dusty fans move less air
    pub exposed: bool,  // reachable from outside the perimeter
    pub tune: i8,       // firmware profile, -2 (low power) .. 0 (stock) .. +2 (max overclock)
    pub dead: [u8; 3],  // failed hashboards, power supplies, fans (see PARTS_PER_BOX)
}
impl Bld {
    /// Share of the box still working: a dead board costs a third of a machine, a dead PSU a whole machine.
    pub fn parts_f(&self) -> f64 {
        (1.0 - self.dead[P_BOARD] as f64 / PARTS_PER_BOX[P_BOARD] as f64) * (1.0 - self.dead[P_PSU] as f64 / PARTS_PER_BOX[P_PSU] as f64)
    }
    pub fn parts_out(&self) -> u32 { self.dead.iter().map(|d| *d as u32).sum() }
    /// Share of this box's work the pool rejects: overclocking, heat and wear all cause hardware errors.
    pub fn reject(&self) -> f64 {
        REJECT_BASE + REJECT_OC * (self.tune.max(0) as f64) + if self.temp > 80.0 { REJECT_HOT } else { 0.0 } + if self.health < 30.0 { REJECT_WORN } else { 0.0 }
    }
    fn t(&self) -> usize { (self.tune.clamp(-2, 2) + 2) as usize }
    /// Hashrate multiplier from firmware, after the custom firmware's fee.
    pub fn tune_th(&self) -> f64 { TUNE_TH[self.t()] * if self.tune != 0 { 1.0 - FIRMWARE_FEE } else { 1.0 } }
    pub fn tune_kw(&self) -> f64 { TUNE_KW[self.t()] }
    pub fn tune_heat(&self) -> f64 { TUNE_HEAT[self.t()] }
    pub fn tune_wear(&self) -> f64 { TUNE_WEAR[self.t()] }
}

#[derive(Clone, Copy, Default)]
pub struct Net { pub cap: f64, pub demand: f64, pub frac: f64, pub loss: f64, pub sources: u16, pub poles: u16, pub consumers: u16 }

#[derive(Clone, Copy)]
pub struct Worker { pub x: f32, pub y: f32, pub tx: f32, pub ty: f32, pub step: u32 }

/// Live (or cached) market data used to start a run.
#[derive(Clone, Copy)]
pub struct Market { pub price: f64, pub net_th: f64, pub height: u64, pub remaining: u64, pub est_change: f64, pub fees: f64, pub live: bool }
impl Market {
    pub const fn fallback() -> Market {
        // mempool.space, 13 Nov 2026
        Market { price: 73_324.0, net_th: 9.5555e8, height: 975_493, remaining: 215, est_change: -0.0739, fees: 0.03, live: false }
    }
}

pub fn subsidy(height: u64) -> f64 { let h = height / HALVING; if h >= 64 { 0.0 } else { 50.0 / (1u64 << h) as f64 } }

pub struct Game {
    pub rng: Rng,
    pub seed: u32,
    pub diff: u32, // 0 normal, 1 hard, 2 brutal
    pub tiles: Vec<Tile>,
    pub blds: Vec<Bld>,
    pub workers: Vec<Worker>,
    pub nets: Vec<Net>,
    pub links: Vec<(u16, u16)>,
    pub cool: Vec<f32>,
    // traits
    pub t_river: bool, pub t_sun: f64, pub t_wind: f64, pub t_coal: bool,
    // economy
    pub live: bool,
    pub cash: f64, pub btc: f64, pub price: f64, pub net_th: f64, pub reward: f64, pub fees: f64, pub base_fees: f64,
    pub block: u64, pub start_block: u64, pub day: u64, pub halvings: u32, pub last_retarget: f64, pub diff_swing: f64,
    pub next_retarget: u64, pub est_change: f64, pub retargets: u32,
    pub gas_f: f64, pub diesel_f: f64, pub wind_now: f64,
    pub streak: u64, pub unpaid_days: u32, pub bailouts_used: u32, pub last_bailout_day: u64, pub blocks_found: u32,
    // event timers
    pub heat_days: u32, pub heat_add: f64, pub gas_days: u32, pub brown_days: u32, pub drought_days: u32,
    pub storm_days: u32, pub throttle_days: u32, pub fuel_days: u32, pub core_cooldown: u64,
    pub pending: i32, pub roll: i32, pub ev_arg: f64,
    pub over: u8, // 0 playing, 1 bankrupt, 2 won
    pub unlocked: [bool; N_TIERS],
    // derived each block
    pub used_kw: f64, pub cap_kw: f64, pub my_th: f64, pub rev_day: f64, pub cost_day: f64, pub frac: f64, pub unpowered: u32,
    pub fence_level: u8, pub fx: Vec<(u8, i32, i32, u8, u8)>,
    pub weather: u8, pub weather_days: u32, pub strikes: u32,
    pub solo: bool, pub last_found: u64,
    pub short_kw: f64, pub short_nets: u32, pub off_count: u32, pub exposed_miners: u32, pub raids: u32,
    pub toasts: Vec<(f64, f64)>,
    // fee market: spikes multiply the day's fees
    pub fee_mult: f64, pub fee_days: u32, pub fee_spikes: u32,
    // demand response: 1 = curtailed for credits, 2 = declined and paying spot price
    pub curtail_blocks: u32, pub curtail_mode: u8, pub curtail_pay: f64, pub curtail_extra: f64,
    pub curtails: u32, pub curtail_earned: f64, pub grid_kw: f64,
    // simulated market: the price wanders but is pulled back towards a slowly rising trend
    pub anchor_px: f64,
    // network hashrate drifts towards the hashprice where the marginal miner breaks even ($/TH/day)
    pub hp_eq: f64,
    // grid contract ($/MWh); difficulty sets the default, a history scenario sets its own
    pub grid_usd: f64,
    // history scenario: one row per day [price, difficulty, fees per block, flags, curtail $/MWh]
    pub spot_x: f64, // what declining a curtailment call costs, as a multiple of the contract price
    pub scen: Vec<[f64; 5]>, pub scen_id: u32, pub max_miner: u8, pub scen_worth0: f64, pub scen_hold_btc: f64,
    // ASIC market: mkt_idx is market hashprice / HP_REF, smoothed (machine prices lag the network by days).
    // stock = used machines listed per miner kind; buy_used = the next miner you place comes from that list.
    pub mkt_idx: f64, pub stock: [u32; 4], pub buy_used: bool, pub last_flood: u64, pub used_bought: u32, pub miners_sold: u32,
    // power contract: spot_walk is the day's random part of the spot price; spot_cap 0 = each box switches off at
    // its own break-even, > 0 = the whole grid feed goes dark above that price
    pub contract: u8, pub contract_from: u64, pub switches: u32, pub spot_walk: f64, pub spot_cap: f64,
    pub spike_blocks: u32, pub spike_usd: f64, pub dr_mwh: f64, pub dr_earned: f64,
    pub c_mwh: f64, pub c_cost: f64, pub cap_blocks: u32, pub econ_off: u32, pub capped: bool,
    // heat reuse: piped[x] = greenhouse a miner heats (-1 none), gh_kw[x] = heat a greenhouse takes; both rebuilt in update_temps
    pub piped: Vec<i32>, pub gh_kw: Vec<f64>, pub heat_kw: f64, pub heat_earned: f64,
    // pool: which one, accepted share of raw hashrate after rejects, PPLNS paid vs expected since joining
    pub pool: u8, pub pool_switches: u32, pub acc_th: f64, pub pplns_paid: f64, pub pplns_exp: f64, pub pool_blocks: u32,
    // crew and spares: spares on the shelf, the level to keep them topped up to, failures and swaps so far
    pub techs: u32, pub guards: u32, pub spares: [u32; 3], pub spare_target: [u32; 3], pub fails: [u32; 3], pub swapped: [u32; 3],
    pub wages_paid: f64, pub parts_lost_th: f64,
    // where in the world: SITES index; well_f = a flare-gas well's output left (North Dakota)
    pub site: u8, pub well_f: f64, pub tariffs: u32,
    // several sites: a follower copies the network and wallet from the lead site; day_off keeps the calendar shared
    pub follower: bool, pub day_off: u64,
    // unlock flags for the ladder rungs past index 9 (deep geothermal, orbital solar)
    pub unlocked_x: [bool; 2],
}

impl Game {
    pub fn new(seed: u32, diff: u32, m: Market) -> Game { Game::new_at(seed, diff, m, 0) }
    pub fn st(&self) -> Site { SITES[self.site as usize % N_SITES] }
    pub fn new_at(seed: u32, diff: u32, m: Market, site: u8) -> Game {
        let site = (site as usize).min(N_SITES - 1) as u8;
        let mut g = Game {
            rng: Rng::new(seed as u64 ^ 0x9E37_79B9_7F4A_7C15), seed, diff,
            tiles: Vec::with_capacity(MAP * MAP), blds: Vec::with_capacity(256), workers: Vec::with_capacity(16),
            nets: Vec::new(), links: Vec::new(), cool: Vec::new(),
            t_river: false, t_sun: 1.0, t_wind: 1.0, t_coal: false,
            live: m.live, cash: START_CASH[diff as usize % 3], btc: 0.0, price: m.price, net_th: m.net_th, reward: subsidy(m.height), fees: m.fees, base_fees: m.fees,
            block: m.height, start_block: m.height, day: 0, halvings: 0, last_retarget: 0.0, diff_swing: 0.0,
            next_retarget: m.height + m.remaining.max(1), est_change: m.est_change, retargets: 0,
            gas_f: 1.0, diesel_f: 1.0, wind_now: 0.7,
            streak: 0, unpaid_days: 0, bailouts_used: 0, last_bailout_day: 0, blocks_found: 0,
            heat_days: 0, heat_add: 0.0, gas_days: 0, brown_days: 0, drought_days: 0, storm_days: 0,
            throttle_days: 0, fuel_days: 0, core_cooldown: 0,
            pending: EV_NONE, roll: 0, ev_arg: 0.0, over: 0,
            unlocked: [false; N_TIERS],
            used_kw: 0.0, cap_kw: 0.0, my_th: 0.0, rev_day: 0.0, cost_day: 0.0, frac: 1.0, unpowered: 0,
            fence_level: 0, fx: Vec::with_capacity(16), weather: W_CLEAR, weather_days: 2, strikes: 0, solo: false, last_found: 0,
            short_kw: 0.0, short_nets: 0, off_count: 0, exposed_miners: 0, raids: 0,
            toasts: Vec::with_capacity(32),
            fee_mult: 1.0, fee_days: 0, fee_spikes: 0,
            curtail_blocks: 0, curtail_mode: 0, curtail_pay: 0.0, curtail_extra: 0.0, curtails: 0, curtail_earned: 0.0, grid_kw: 0.0,
            anchor_px: m.price, hp_eq: 0.0,
            grid_usd: GRID_USD[diff as usize % 3], spot_x: CURTAIL_SPOT_X,
            scen: Vec::new(), scen_id: 0, max_miner: K_MINER4, scen_worth0: 0.0, scen_hold_btc: 0.0,
            mkt_idx: 1.0, stock: [0; 4], buy_used: false, last_flood: 0, used_bought: 0, miners_sold: 0,
            contract: C_FIXED, contract_from: 0, switches: 0, spot_walk: 1.0, spot_cap: 0.0,
            spike_blocks: 0, spike_usd: 0.0, dr_mwh: 0.0, dr_earned: 0.0, c_mwh: 0.0, c_cost: 0.0, cap_blocks: 0, econ_off: 0, capped: false,
            piped: Vec::new(), gh_kw: Vec::new(), heat_kw: 0.0, heat_earned: 0.0,
            pool: 0, pool_switches: 0, acc_th: 0.0, pplns_paid: 0.0, pplns_exp: 0.0, pool_blocks: 0,
            techs: 0, guards: 0, spares: [0; 3], spare_target: [0; 3], fails: [0; 3], swapped: [0; 3], wages_paid: 0.0, parts_lost_th: 0.0,
            site, well_f: 1.0, tariffs: 0, follower: false, day_off: 0, unlocked_x: [false; 2],
        };
        g.grid_usd = SITES[site as usize].grid + 10.0 * (diff.min(2) as f64);
        g.hp_eq = g.hashprice();
        g.unlocked[0] = true;
        g.gen_map();
        g.rebuild_nets();
        g.mkt_idx = g.mkt_raw(); g.restock(true);
        g
    }

    pub fn tile(&self, i: i32, j: i32) -> Option<&Tile> {
        if i < 0 || j < 0 || i >= MAP as i32 || j >= MAP as i32 { None } else { Some(&self.tiles[j as usize * MAP + i as usize]) }
    }
    fn tile_mut(&mut self, i: i32, j: i32) -> &mut Tile { &mut self.tiles[j as usize * MAP + i as usize] }

    // ------------------------------------------------------------ map generation
    fn gen_map(&mut self) {
        let r = &mut self.rng;
        self.t_river = r.f() < 0.65;
        self.t_sun = 0.5 + r.f() * 0.7;
        self.t_wind = 0.4 + r.f() * 1.0;
        self.t_coal = r.f() < 0.5;
        let st = SITES[self.site as usize % N_SITES];
        if st.river >= 0 { self.t_river = st.river == 1; }
        self.t_sun = clamp(self.t_sun * st.sun, 0.3, 1.3); self.t_wind = clamp(self.t_wind * st.wind, 0.3, 1.6);
        if st.flare { self.t_coal = false; }
        for _ in 0..MAP * MAP {
            let ground = if self.rng.f() < 0.09 { G_ROUGH } else { G_GRASS };
            self.tiles.push(Tile { ground, tree: 0, occ: -1 });
        }
        if self.t_river {
            let (ci, cj) = loop {
                let ci = 6 + self.rng.below(36) as i32; let cj = 6 + self.rng.below(36) as i32;
                if iabs(ci - 24) + iabs(cj - 24) > 12 { break (ci, cj); }
            };
            let ri = 3 + self.rng.below(4) as i32; let rj = 3 + self.rng.below(4) as i32;
            for j in cj - rj..=cj + rj {
                for i in ci - ri..=ci + ri {
                    let di = (i - ci) as f64 / ri as f64; let dj = (j - cj) as f64 / rj as f64;
                    if di * di + dj * dj <= 1.0 + self.rng.f() * 0.25 && i >= 0 && j >= 0 && (i as usize) < MAP && (j as usize) < MAP {
                        self.tile_mut(i, j).ground = G_WATER;
                    }
                }
            }
        }
        let mut centers = [(0i32, 0i32); 10];
        for c in centers.iter_mut() { *c = (self.rng.below(MAP as u32) as i32, self.rng.below(MAP as u32) as i32); }
        for j in 0..MAP as i32 {
            for i in 0..MAP as i32 {
                if self.tile(i, j).unwrap().ground == G_WATER { continue; }
                let mut near = 9999;
                for c in centers.iter() { let d = (i - c.0) * (i - c.0) + (j - c.1) * (j - c.1); if d < near { near = d; } }
                let p = if near < 20 { 0.45 } else { 0.035 } * SITE_TREES[self.site as usize % N_SITES];
                if self.rng.f() < p { let t = 1 + self.rng.below(S::N_TREES as u32) as u8; self.tile_mut(i, j).tree = t; }
            }
        }
        for j in 20..29 { for i in 20..29 { let t = self.tile_mut(i, j); t.tree = 0; if t.ground == G_WATER { t.ground = G_GRASS; } } }
        self.force_place(K_SHED, 23, 23, 0);
        self.force_place(K_GRID, 25, 24, 0);
        self.force_place(K_MINER1, 24, 25, 0);
        for _ in 0..6 {
            let x = 22.0 + self.rng.f() as f32 * 5.0; let y = 22.0 + self.rng.f() as f32 * 5.0;
            self.workers.push(Worker { x, y, tx: x, ty: y, step: 0 });
        }
    }

    fn force_place(&mut self, kind: u8, i: i32, j: i32, dir: u8) {
        let k = KINDS[kind as usize];
        let idx = self.blds.len() as i16;
        for b in 0..k.d as i32 { for a in 0..k.w as i32 {
            let t = self.tile_mut(i + a, j + b); t.occ = idx; t.tree = 0;
            if k.pad { t.ground = if k.coal_pad { G_COAL } else { G_CONCRETE }; }
        } }
        self.blds.push(Bld { kind, i: i as u8, j: j as u8, level: 1, broken: 0, temp: 40.0, invested: k.cost, dir: dir % 4, health: 100.0, net: 0, fire: 0, off: 0, exposed: true, dust: 0.0, tune: 0, dead: [0; 3] });
    }

    // ------------------------------------------------------------ power networks
    /// Two buildings are wired together if they touch (shared busbar), or one is a pole within reach.
    fn connected(a: &Bld, b: &Bld) -> bool {
        let ka = KINDS[a.kind as usize]; let kb = KINDS[b.kind as usize];
        if !ka.electric || !kb.electric { return false; }
        let (ai0, aj0, ai1, aj1) = (a.i as i32, a.j as i32, a.i as i32 + ka.w as i32 - 1, a.j as i32 + ka.d as i32 - 1);
        let (bi0, bj0, bi1, bj1) = (b.i as i32, b.j as i32, b.i as i32 + kb.w as i32 - 1, b.j as i32 + kb.d as i32 - 1);
        let gi = imax(0, imax(bi0 - ai1, ai0 - bi1) - 1);
        let gj = imax(0, imax(bj0 - aj1, aj0 - bj1) - 1);
        let gap = imax(gi, gj);
        let reach = imax(ka.reach as i32, kb.reach as i32);
        gap <= reach
    }

    pub fn rebuild_nets(&mut self) {
        let n = self.blds.len();
        for b in self.blds.iter_mut() { b.net = u16::MAX; }
        self.links.clear();
        let mut nets = 0u16;
        let mut stack: Vec<usize> = Vec::with_capacity(n);
        for s in 0..n {
            if self.blds[s].net != u16::MAX || !KINDS[self.blds[s].kind as usize].electric { continue; }
            self.blds[s].net = nets; stack.push(s);
            while let Some(x) = stack.pop() {
                for y in 0..n {
                    if y == x || !Self::connected(&self.blds[x], &self.blds[y]) { continue; }
                    let pole_link = self.blds[x].kind == K_POLE || self.blds[y].kind == K_POLE;
                    if pole_link && x < y { self.links.push((x as u16, y as u16)); }
                    if self.blds[y].net == u16::MAX { self.blds[y].net = nets; stack.push(y); }
                }
            }
            nets += 1;
        }
        self.nets.clear();
        self.nets.resize(nets as usize, Net::default());
        for b in self.blds.iter() {
            if b.net == u16::MAX { continue; }
            let nt = &mut self.nets[b.net as usize];
            if is_power(b.kind) { nt.sources += 1; }
            if b.kind == K_POLE { nt.poles += 1; }
            if is_consumer(b.kind) { nt.consumers += 1; }
        }
        for nt in self.nets.iter_mut() { nt.loss = fmin(0.12, 0.02 + 0.004 * nt.poles as f64); }
        self.rebuild_security();
        self.dispatch(false);
        self.update_temps();
    }

    /// Flood-fill from the map edge: anything the outside can walk to is exposed to raiders.
    /// Fences and water block; buildings, trees and rubble don't.
    pub fn rebuild_security(&mut self) {
        let mut seen = alloc::vec![false; MAP * MAP];
        let mut stack: Vec<usize> = Vec::with_capacity(MAP * 4);
        let blocks = |g: &Game, x: usize| { let t = g.tiles[x]; t.ground == G_WATER || (t.occ >= 0 && g.blds[t.occ as usize].kind == K_WALL) };
        for e in 0..MAP {
            for x in [e, (MAP - 1) * MAP + e, e * MAP, e * MAP + MAP - 1] {
                if !seen[x] && !blocks(self, x) { seen[x] = true; stack.push(x); }
            }
        }
        while let Some(x) = stack.pop() {
            let (i, j) = ((x % MAP) as i32, (x / MAP) as i32);
            for (di, dj) in DIRS.iter() {
                let (ni, nj) = (i + di, j + dj);
                if ni < 0 || nj < 0 || ni >= MAP as i32 || nj >= MAP as i32 { continue; }
                let y = nj as usize * MAP + ni as usize;
                if !seen[y] && !blocks(self, y) { seen[y] = true; stack.push(y); }
            }
        }
        let mut em = 0; let mut fl = 9u8;
        for b in self.blds.iter_mut() {
            let k = KINDS[b.kind as usize];
            let mut ex = false;
            if b.kind == K_WALL {
                // a wall is on the perimeter if the outside touches it
                for (di, dj) in DIRS.iter() {
                    let (ni, nj) = (b.i as i32 + di, b.j as i32 + dj);
                    if ni < 0 || nj < 0 || ni >= MAP as i32 || nj >= MAP as i32 { ex = true; continue; }
                    if seen[nj as usize * MAP + ni as usize] { ex = true; }
                }
                if ex && b.level < fl { fl = b.level; }
            } else {
                for bb in 0..k.d as usize { for a in 0..k.w as usize { if seen[(b.j as usize + bb) * MAP + b.i as usize + a] { ex = true; } } }
            }
            b.exposed = ex;
            if b.exposed && is_miner(b.kind) { em += 1; }
        }
        self.exposed_miners = em;
        self.fence_level = if fl == 9 { 0 } else { fl };
    }

    /// Would a building of `kind` at (i,j) join a network with a power source?
    pub fn would_connect(&self, kind: u8, i: i32, j: i32) -> bool {
        let probe = Bld { kind, i: i as u8, j: j as u8, level: 1, broken: 0, temp: 0.0, invested: 0.0, dir: 0, health: 100.0, net: 0, fire: 0, off: 0, exposed: false, dust: 0.0, tune: 0, dead: [0; 3] };
        if is_power(kind) { return true; }
        self.blds.iter().any(|b| b.net != u16::MAX && Self::connected(&probe, b) && self.nets.get(b.net as usize).map_or(false, |n| n.sources > 0))
    }

    // ------------------------------------------------------------ building rules
    pub fn kind_unlocked(&self, kind: u8) -> bool {
        if self.scen_id > 0 && is_miner(kind) && kind > self.max_miner { return false; } // that machine didn't exist yet
        match kind {
            0..=9 => self.unlocked[kind as usize],
            10 | 11 => true,
            12 => self.unlocked[1],
            13 => self.unlocked[2],
            14 | 17 | 18 | 19 | 20 => true,
            21 | 22 => self.unlocked_x[(kind - 21) as usize],
            15 => self.unlocked[3],
            _ => false,
        }
    }

    pub fn tier_unlocked(&self, k: u8) -> bool { if k >= 21 { self.unlocked_x[(k - 21) as usize] } else { self.unlocked[k as usize] } }
    pub fn unlock_tier(&mut self, k: u8) { if k >= 21 { self.unlocked_x[(k - 21) as usize] = true; } else { self.unlocked[k as usize] = true; } }

    pub fn core_reqs(&self) -> [bool; 4] {
        let fusion3 = self.blds.iter().any(|b| b.kind == K_FUSION && b.level >= 3);
        [self.halvings >= 2, fusion3, self.btc >= 21.0, self.streak >= RETARGET]
    }

    /// 0 ok, else a toast code explaining why not.
    pub fn can_place(&self, kind: u8, i: i32, j: i32) -> f64 {
        if !self.kind_unlocked(kind) { return T_LOCKED; }
        let k = KINDS[kind as usize];
        for b in 0..k.d as i32 { for a in 0..k.w as i32 {
            match self.tile(i + a, j + b) {
                None => return T_BAD_SPOT,
                Some(t) => { if t.occ >= 0 || t.ground == G_WATER || t.ground == G_CONCRETE || t.ground == G_COAL { return T_BAD_SPOT; } }
            }
        } }
        if kind == K_HYDRO {
            let mut near_water = false;
            for b in -1..=k.d as i32 { for a in -1..=k.w as i32 {
                if let Some(t) = self.tile(i + a, j + b) { if t.ground == G_WATER { near_water = true; } }
            } }
            if !near_water { return T_NEEDS_WATER; }
        }
        if kind == K_CORE && (self.core_reqs().iter().any(|x| !*x) || self.block < self.core_cooldown) { return T_CORE_REQ; }
        if is_miner(kind) && self.buy_used && self.stock[(kind - K_MINER1) as usize] == 0 { return T_NO_STOCK; }
        if self.cash < self.place_cost(kind, i, j) { return T_NO_CASH; }
        0.0
    }

    pub fn place_cost(&self, kind: u8, i: i32, j: i32) -> f64 {
        let k = KINDS[kind as usize];
        let mut extra = 0.0;
        for b in 0..k.d as i32 { for a in 0..k.w as i32 { if let Some(t) = self.tile(i + a, j + b) {
            if t.tree > 0 { extra += CLEAR_TREE; }
            if t.ground == G_ROUGH { extra += CLEAR_RUBBLE; }
        } } }
        (if is_miner(kind) { self.miner_price(kind, self.buy_used) } else { k.cost }) + extra
    }

    pub fn place(&mut self, kind: u8, i: i32, j: i32, dir: u8) -> f64 {
        let why = self.can_place(kind, i, j);
        if why != 0.0 { self.toast(why, kind as f64); return why; }
        let cost = self.place_cost(kind, i, j);
        self.cash -= cost;
        self.force_place(kind, i, j, dir);
        { let k = KINDS[kind as usize]; self.fx.push((0, i, j, k.w, k.d)); if is_miner(kind) { self.fx.push((5, i, j, 1, 1)); } }
        self.blds.last_mut().unwrap().invested = cost;
        if is_miner(kind) && self.buy_used {
            // second-hand: somebody else's dusty, tired machine
            self.stock[(kind - K_MINER1) as usize] -= 1; self.used_bought += 1;
            let (h, d) = (45.0 + self.rng.f() * 35.0, 15.0 + self.rng.f() * 30.0);
            let (nb, np, nf) = (self.rng.below(4) as u8, self.rng.below(2) as u8, if kind == K_MINER4 { 0 } else { self.rng.below(5) as u8 });
            let b = self.blds.last_mut().unwrap(); b.health = h as f32; b.dust = d as f32; b.dead = [nb, np, nf];
        }
        self.rebuild_nets();
        let b = *self.blds.last().unwrap();
        if is_consumer(kind) && self.nets.get(b.net as usize).map_or(true, |n| n.sources == 0) { self.toast(T_UNPOWERED, 0.0); }
        if kind == K_CORE { self.pending = EV_CORE_TRIAL; self.roll = 0; }
        if kind != K_WALL { self.blds.last_mut().unwrap().off = 0; }
        0.0
    }

    /// What clearing (i,j) would cost; 0 = nothing there.
    pub fn clear_cost(&self, i: i32, j: i32) -> f64 {
        match self.tile(i, j) {
            Some(t) if t.occ < 0 => (if t.tree > 0 { CLEAR_TREE } else { 0.0 }) + (if t.ground == G_ROUGH { CLEAR_RUBBLE } else { 0.0 }),
            _ => 0.0,
        }
    }

    /// Bulldoze trees and rubble off a tile.
    pub fn clear(&mut self, i: i32, j: i32, quiet: bool) -> bool {
        let c = self.clear_cost(i, j);
        if c <= 0.0 { if !quiet { self.toast(T_NOTHING_TO_CLEAR, 0.0); } return false; }
        if self.cash < c { if !quiet { self.toast(T_NO_CASH, 0.0); } return false; }
        self.cash -= c;
        let t = self.tile_mut(i, j); t.tree = 0; if t.ground == G_ROUGH { t.ground = G_GRASS; }
        true
    }

    /// Switch a consumer on or off.
    pub fn toggle(&mut self, idx: usize) {
        if idx >= self.blds.len() || !is_consumer(self.blds[idx].kind) { return; }
        self.blds[idx].off ^= 1;
        self.dispatch(false); self.update_temps(); self.dispatch(false);
    }

    /// Switch off the least efficient running miners on every short network until supply covers demand.
    pub fn shed(&mut self) -> u32 {
        self.dispatch(false);
        let mut n = 0u32;
        for _ in 0..self.blds.len() {
            let short = self.nets.iter().position(|nt| nt.sources > 0 && nt.cap > 0.0 && nt.demand > 0.0 && nt.frac < 0.999);
            let ni = match short { Some(x) => x, None => break };
            let mut worst = usize::MAX; let mut wv = -1.0;
            for (x, b) in self.blds.iter().enumerate() {
                if b.net as usize != ni || !is_miner(b.kind) || b.broken != 0 || b.off != 0 { continue; }
                let k = KINDS[b.kind as usize];
                let v = k.kw * b.tune_kw() / (k.th * b.tune_th() * (0.7 + 0.3 * b.health as f64 / 100.0)) + if b.temp > 80.0 { 1.0 } else { 0.0 };
                if v > wv { wv = v; worst = x; }
            }
            if worst == usize::MAX { break; }
            self.blds[worst].off = 1; n += 1;
            self.dispatch(false); self.update_temps(); self.dispatch(false);
        }
        self.update_temps(); self.dispatch(false);
        self.toast(T_SHED, n as f64);
        n
    }

    // ------------------------------------------------------------ weather and dust
    pub fn set_weather(&mut self, w: u8, days: u32) {
        let changed = w != self.weather;
        self.weather = w; self.weather_days = days;
        if w == W_STORM { self.storm_days = days; }
        if changed { self.toast(T_WEATHER, w as f64); }
        self.update_temps();
    }

    fn roll_weather(&mut self) {
        // dust settles every day; rain rinses some off; dust storms coat everything
        let add = match self.weather { W_DUST => 22.0, W_RAIN => -12.0, W_STORM => -18.0, _ => 1.6 };
        for b in self.blds.iter_mut() {
            if is_miner(b.kind) || b.kind == K_FAN || b.kind == K_IMMERSION {
                let run = if b.off == 0 && b.broken == 0 { 1.0 } else { 0.3 };
                b.dust = (b.dust + add * if add > 0.0 { run } else { 1.0 }).clamp(0.0, 100.0);
            }
        }
        if self.weather_days > 0 { self.weather_days -= 1; }
        if self.weather_days > 0 { return; }
        let doy = ((self.day + self.day_off) % 365) as f64; let summer = 1.0 - fabs(doy - 182.0) / 182.0;
        let w = [6.0, 2.2 + (1.0 - summer) * 1.2, 0.7 + summer * 0.6, (0.4 + summer * 1.2) * self.t_sun];
        let total: f64 = w.iter().sum();
        let mut r = self.rng.f() * total; let mut pick = 0u8;
        for (k, x) in w.iter().enumerate() { if r < *x { pick = k as u8; break; } r -= *x; }
        let days = 1 + self.rng.below(if pick == W_CLEAR { 4 } else { 2 });
        self.set_weather(pick, days);
    }

    pub fn lightning_pub(&mut self) { self.lightning(); }
    fn lightning(&mut self) {
        // strikes favour tall things: poles, turbines, then anything
        let n = self.blds.len() as u32; if n == 0 { return; }
        let mut x = self.rng.below(n) as usize;
        for _ in 0..3 { let y = self.rng.below(n) as usize; let k = self.blds[y].kind; if k == K_POLE || k == K_WIND || k == K_GRID { x = y; break; } }
        let b = self.blds[x]; let k = KINDS[b.kind as usize];
        self.fx.push((3, b.i as i32, b.j as i32, k.w, k.d)); self.strikes += 1;
        if is_miner(b.kind) && b.broken == 0 {
            let fire = self.rng.f() < 0.35;
            self.blds[x].health = fmax(1.0, self.blds[x].health as f64 - 30.0) as f32;
            if fire && b.off == 0 { self.blds[x].fire = 1; }
            self.toast(T_LIGHTNING, fire as u8 as f64);
        }
    }

    pub fn clean_cost(&self, idx: usize) -> f64 {
        let b = &self.blds[idx];
        if (is_miner(b.kind) || b.kind == K_FAN || b.kind == K_IMMERSION) && b.dust >= 5.0 { CLEAN_COST } else { 0.0 }
    }
    pub fn clean(&mut self, idx: usize) {
        if idx >= self.blds.len() { return; }
        let c = self.clean_cost(idx); if c <= 0.0 { return; }
        if self.cash < c { self.toast(T_NO_CASH, 0.0); return; }
        self.cash -= c; self.blds[idx].dust = 0.0;
        let b = self.blds[idx]; self.fx.push((4, b.i as i32, b.j as i32, 1, 1));
        self.update_temps(); self.dispatch(false);
    }
    /// Blow out every box and fan with more than 30% dust (dirtiest first, as far as cash goes).
    pub fn clean_all(&mut self) -> u32 {
        let mut idx: Vec<usize> = (0..self.blds.len()).filter(|&x| self.clean_cost(x) > 0.0 && self.blds[x].dust >= 30.0).collect();
        idx.sort_unstable_by(|a, b| (self.blds[*b].dust as i32).cmp(&(self.blds[*a].dust as i32)));
        let mut n = 0;
        for x in idx { if self.cash < CLEAN_COST { break; } self.cash -= CLEAN_COST; self.blds[x].dust = 0.0; n += 1;
            let b = self.blds[x]; if self.fx.len() < 200 { self.fx.push((4, b.i as i32, b.j as i32, 1, 1)); } }
        if n == 0 { self.toast(T_NO_CASH, 0.0); } else { self.toast(T_CLEANED, n as f64); }
        self.update_temps(); self.dispatch(false);
        n
    }

    /// Upgrade every wall at the lowest level, as far as the cash goes. Returns how many.
    pub fn upgrade_walls(&mut self) -> u32 {
        let lo = self.blds.iter().filter(|b| b.kind == K_WALL && b.level < 3).map(|b| b.level).min();
        let lo = match lo { Some(l) => l, None => return 0 };
        let mut n = 0;
        for x in 0..self.blds.len() {
            if self.blds[x].kind != K_WALL || self.blds[x].level != lo { continue; }
            let c = self.upgrade_cost(x);
            if self.cash < c { if n == 0 { self.toast(T_NO_CASH, 0.0); } break; }
            self.cash -= c; self.blds[x].level += 1; self.blds[x].invested += c; n += 1;
            let (bi, bj) = (self.blds[x].i as i32, self.blds[x].j as i32); if self.fx.len() < 200 { self.fx.push((2, bi, bj, 1, 1)); }
        }
        self.rebuild_security();
        if n > 0 { self.toast(T_WALLS_UP, n as f64); }
        n
    }

    pub fn wake_all(&mut self) {
        for b in self.blds.iter_mut() { b.off = 0; }
        self.dispatch(false); self.update_temps(); self.dispatch(false);
    }

    pub fn upgrade_cost(&self, idx: usize) -> f64 {
        let b = &self.blds[idx];
        let k = KINDS[b.kind as usize];
        if !(is_power(b.kind) || b.kind == K_WALL) || b.kind == K_CORE { return 0.0; }
        match b.level { 1 => k.up2, 2 => k.up3, _ => 0.0 }
    }

    pub fn upgrade(&mut self, idx: usize) {
        if idx >= self.blds.len() { return; }
        let c = self.upgrade_cost(idx);
        if c <= 0.0 { return; }
        if self.cash < c { self.toast(T_NO_CASH, 0.0); return; }
        self.cash -= c;
        let b = &mut self.blds[idx];
        b.level += 1; b.invested += c;
        let (kind, lvl) = (b.kind, b.level);
        { let k = KINDS[kind as usize]; let (bi, bj) = (b.i as i32, b.j as i32); self.fx.push((2, bi, bj, k.w, k.d)); }
        if kind == K_WALL { self.rebuild_security(); return; }
        if lvl >= 3 { if let Some(nx) = next_tier(kind) { if !self.tier_unlocked(nx) { self.unlock_tier(nx); self.toast(T_UNLOCK, nx as f64); } } }
        self.dispatch(false);
    }

    pub fn repair_cost(&self, idx: usize) -> f64 {
        let b = &self.blds[idx];
        if !is_miner(b.kind) { return 0.0; }
        let price = self.new_price(b.kind); // parts cost what the market says
        if b.broken != 0 { price * 0.3 } else { price * 0.25 * (1.0 - b.health as f64 / 100.0) }
    }

    /// Miners go to a broker at today's used price, less their cut, scaled by condition (a dead one is sold
    /// for parts at 35%). Other buildings: 40% of what you spent.
    pub fn sell_value(&self, idx: usize) -> f64 {
        let b = &self.blds[idx];
        if is_miner(b.kind) { self.used_price(b.kind) * (1.0 - BROKER_CUT) * (0.35 + 0.65 * b.health as f64 / 100.0) } else { b.invested * 0.4 }
    }
    /// What your miners would fetch from a broker today.
    pub fn fleet_value(&self) -> f64 { (0..self.blds.len()).filter(|&x| is_miner(self.blds[x].kind)).map(|x| self.sell_value(x)).sum() }

    // ------------------------------------------------------------ ASIC market
    /// Today's hashprice against the one the list prices were set at.
    pub fn mkt_raw(&self) -> f64 { clamp(self.hashprice() / HP_REF, 0.25, 10.0) }
    /// Share of a machine's earnings left after paying MKT_POWER for its power, at market hashprice.
    /// Below zero it's a space heater for anyone paying the going rate.
    pub fn margin(&self, kind: u8) -> f64 {
        let k = KINDS[kind as usize]; let rev = k.th * self.mkt_idx * HP_REF;
        if rev <= 0.0 { -1.0 } else { 1.0 - k.kw * 24.0 / 1000.0 * MKT_POWER / rev }
    }
    /// New from the maker: follows hashprice, less than one for one (index ^ 0.75).
    pub fn new_price(&self, kind: u8) -> f64 { let r = sqrt(self.mkt_idx); KINDS[kind as usize].cost * r * sqrt(r) * self.st().duty }
    /// Used, in perfect condition: priced on its margin, so old power-hungry machines crash hardest.
    pub fn used_price(&self, kind: u8) -> f64 { self.new_price(kind) * clamp(0.1 + 0.8 * self.margin(kind), 0.08, 0.9) }
    pub fn miner_price(&self, kind: u8, used: bool) -> f64 { if used { self.used_price(kind) } else { self.new_price(kind) } }
    /// Weekly listings. Thin margins mean capitulation: miners sell the machines that no longer pay.
    fn restock(&mut self, quiet: bool) {
        let mut listed = 0;
        for m in 0..4 {
            let squeeze = clamp(1.0 - self.margin(K_MINER1 + m as u8) * 2.0, 0.0, 1.0); // 0 above 50% margin, 1 at breakeven
            let n = ([3.0, 2.0, 1.0, 0.4][m] * (0.5 + self.rng.f()) + squeeze * [5.0, 4.0, 2.0, 1.0][m]) as u32;
            self.stock[m] = n.min(12); listed += self.stock[m];
        }
        if !quiet && listed >= 10 && self.day >= self.last_flood + 28 { self.last_flood = self.day; self.toast(T_MINERS_SELLING, listed as f64); }
    }
    fn market_day(&mut self) {
        self.mkt_idx += (self.mkt_raw() - self.mkt_idx) * 0.2;
        if self.day % 7 == 0 { self.restock(false); }
    }
    pub fn set_used(&mut self, on: bool) { self.buy_used = on; }

    // ------------------------------------------------------------ firmware
    /// Set one miner's firmware profile (-2..2). Returns 1 if it changed.
    pub fn set_tune(&mut self, idx: usize, t: i32) -> u32 {
        if idx >= self.blds.len() || !is_miner(self.blds[idx].kind) { return 0; }
        let t = t.clamp(-2, 2) as i8;
        if self.blds[idx].tune == t { return 0; }
        self.blds[idx].tune = t; self.update_temps(); self.dispatch(false); 1
    }
    /// The same profile on every miner of a kind. Returns how many changed.
    pub fn tune_kind(&mut self, kind: u8, t: i32) -> u32 {
        let t = t.clamp(-2, 2) as i8; let mut n = 0;
        for b in self.blds.iter_mut() { if b.kind == kind && b.tune != t { b.tune = t; n += 1; } }
        if n > 0 { self.update_temps(); self.dispatch(false); }
        n
    }

    pub fn repair(&mut self, idx: usize) {
        if idx >= self.blds.len() || !is_miner(self.blds[idx].kind) { return; }
        if self.blds[idx].broken == 0 && self.blds[idx].health >= 99.5 { return; }
        let c = self.repair_cost(idx);
        if self.cash < c { self.toast(T_NO_CASH, 0.0); return; }
        self.cash -= c;
        let b = &mut self.blds[idx]; if b.broken != 0 { b.dead = [0; 3]; } // a dead box comes back rebuilt; a service doesn't replace parts
        b.broken = 0; b.health = 100.0; b.fire = 0;
        self.toast(T_REPAIRED, 0.0);
        self.dispatch(false);
    }

    pub fn rotate(&mut self, idx: usize) {
        if idx < self.blds.len() && self.blds[idx].kind == K_FAN { self.blds[idx].dir = (self.blds[idx].dir + 1) % 4; self.update_temps(); }
    }

    pub fn sell(&mut self, idx: usize) {
        if idx >= self.blds.len() { return; }
        let b = self.blds[idx];
        if b.kind == K_SHED { return; }
        let v = self.sell_value(idx);
        self.cash += v;
        if is_miner(b.kind) { self.miners_sold += 1; self.toast(T_SOLD_MINER, v); }
        self.remove(idx);
    }

    fn remove(&mut self, idx: usize) {
        let k = KINDS[self.blds[idx].kind as usize];
        let b = self.blds[idx];
        self.fx.push((1, b.i as i32, b.j as i32, k.w, k.d));
        for bb in 0..k.d as i32 { for a in 0..k.w as i32 {
            let t = self.tile_mut(b.i as i32 + a, b.j as i32 + bb); t.occ = -1; if k.pad { t.ground = G_ROUGH; }
        } }
        self.blds.swap_remove(idx);
        if idx < self.blds.len() {
            let m = self.blds[idx]; let mk = KINDS[m.kind as usize];
            for bb in 0..mk.d as i32 { for a in 0..mk.w as i32 { self.tile_mut(m.i as i32 + a, m.j as i32 + bb).occ = idx as i16; } }
        }
        self.rebuild_nets();
    }

    pub fn sell_btc(&mut self, frac: f64) {
        let amt = self.btc * frac;
        if amt <= 0.0 { return; }
        self.btc -= amt; self.cash += amt * self.price * 0.995;
        self.toast(T_SOLD_BTC, amt * self.price);
    }

    // ------------------------------------------------------------ parts, spares, crew
    /// Once a day: parts fail, technicians swap what they can from the shelf, the shelf is topped up, wages are paid.
    fn parts_day(&mut self) {
        let mut failed = 0u32;
        for x in 0..self.blds.len() {
            let b = self.blds[x];
            if !is_miner(b.kind) || b.broken != 0 || b.off != 0 { continue; }
            let wear = (1.0 + (100.0 - b.health as f64) / 25.0) * b.tune_wear() * if b.temp > 75.0 { 1.0 + (b.temp as f64 - 75.0) / 5.0 } else { 1.0 };
            let sag = { let f = self.net_frac(&b); f > 0.0 && f < 0.95 } || self.brown_days > 0;
            for p in 0..3 {
                let total = if p == P_FAN && b.kind == K_MINER4 { 0 } else { PARTS_PER_BOX[p] };
                let alive = total.saturating_sub(self.blds[x].dead[p]) as f64;
                let extra = match p { P_PSU => if sag { 3.0 } else { 1.0 }, P_FAN => 1.0 + b.dust as f64 / 50.0, _ => 1.0 };
                let mut e = alive * PART_HAZARD[p] * wear * extra;
                while e > 0.0 && self.blds[x].dead[p] < total {
                    if self.rng.f() < fmin(e, 1.0) { self.blds[x].dead[p] += 1; self.fails[p] += 1; failed += 1; }
                    e -= 1.0;
                }
            }
        }
        // technicians: power supplies first (a whole machine each), then boards, then fans
        let mut cap = self.techs * TECH_SWAPS; let mut swapped = 0;
        for p in [P_PSU, P_BOARD, P_FAN] {
            for x in 0..self.blds.len() {
                while cap > 0 && self.spares[p] > 0 && self.blds[x].dead[p] > 0 && self.blds[x].broken == 0 {
                    self.blds[x].dead[p] -= 1; self.spares[p] -= 1; self.swapped[p] += 1; cap -= 1; swapped += 1;
                }
            }
        }
        // preventive maintenance: each technician also blows out the dustiest boxes and fan walls
        if self.techs > 0 {
            let mut dusty: Vec<usize> = (0..self.blds.len()).filter(|&x| (is_miner(self.blds[x].kind) || self.blds[x].kind == K_FAN) && self.blds[x].dust >= 15.0).collect();
            dusty.sort_unstable_by(|a, b| self.blds[*b].dust.partial_cmp(&self.blds[*a].dust).unwrap_or(core::cmp::Ordering::Equal));
            for x in dusty.into_iter().take((self.techs * TECH_CLEANS) as usize) { self.blds[x].dust = 0.0; }
        }
        for p in 0..3 { if self.spares[p] < self.spare_target[p] { let n = self.spare_target[p] - self.spares[p]; let c = n as f64 * PART_USD[p]; if self.cash >= c { self.cash -= c; self.spares[p] += n; } } }
        let w = self.techs as f64 * TECH_WAGE + self.guards as f64 * GUARD_WAGE;
        self.cash -= w; self.wages_paid += w;
        let left: u32 = self.blds.iter().filter(|b| is_miner(b.kind)).map(|b| b.parts_out()).sum();
        if failed > 0 && left > 0 && (self.techs == 0 || swapped == 0) && left >= 3 { self.toast(T_PARTS_FAILED, failed as f64); }
        if failed > 0 || swapped > 0 { self.update_temps(); self.dispatch(false); }
    }
    /// What fixing one box's failed parts costs now: a contractor's call-out plus parts (from the shelf if you have them).
    pub fn fix_cost(&self, idx: usize) -> f64 {
        let b = match self.blds.get(idx) { Some(b) if is_miner(b.kind) && b.parts_out() > 0 => b, _ => return 0.0 };
        let mut c = CALLOUT_USD * self.st().callout;
        for p in 0..3 { let from_shelf = (b.dead[p] as u32).min(self.spares[p]); c += (b.dead[p] as u32 - from_shelf) as f64 * PART_USD[p] * RUSH_X; }
        c
    }
    pub fn fix_parts(&mut self, idx: usize) {
        let c = self.fix_cost(idx); if c <= 0.0 { return; }
        if self.cash < c { self.toast(T_NO_CASH, 0.0); return; }
        self.cash -= c;
        for p in 0..3 { let from_shelf = (self.blds[idx].dead[p] as u32).min(self.spares[p]); self.spares[p] -= from_shelf; self.blds[idx].dead[p] = 0; }
        self.toast(T_PARTS_FIXED, c); self.update_temps(); self.dispatch(false);
    }
    pub fn buy_spares(&mut self, p: usize, n: u32) {
        if p > 2 || n == 0 { return; }
        let c = n as f64 * PART_USD[p]; if self.cash < c { self.toast(T_NO_CASH, 0.0); return; }
        self.cash -= c; self.spares[p] += n;
    }
    pub fn set_spare_target(&mut self, p: usize, n: u32) { if p <= 2 { self.spare_target[p] = n.min(500); } }
    pub fn set_crew(&mut self, techs: u32, guards: u32) {
        self.techs = techs.min(MAX_CREW); self.guards = guards.min(MAX_CREW); self.sync_workers();
    }
    /// One walking figure per crew member on top of the six regulars.
    pub fn sync_workers(&mut self) {
        let want = (6 + self.techs + self.guards) as usize;
        while self.workers.len() < want { let x = 22.0 + self.rng.f() as f32 * 5.0; let y = 22.0 + self.rng.f() as f32 * 5.0; self.workers.push(Worker { x, y, tx: x, ty: y, step: 0 }); }
        self.workers.truncate(want);
    }

    pub fn pool_fee(&self) -> f64 { if self.solo { 0.0 } else { POOLS[self.pool as usize % POOLS.len()].fee } }
    /// Share of accepted work that arrives too late to count. Solo miners build their own work, so ~none.
    pub fn stale_rate(&self) -> f64 { if self.solo { 0.0 } else { STALE_BASE + POOLS[self.pool as usize % POOLS.len()].ms * STALE_PER_MS } }
    pub fn set_pool(&mut self, p: u8) {
        let p = p.min(POOLS.len() as u8 - 1);
        if p == self.pool && !self.solo { return; }
        self.pool = p; self.solo = false; self.pool_switches += 1;
        if POOLS[p as usize].pplns { self.pplns_paid = 0.0; self.pplns_exp = 0.0; self.pool_blocks = 0; }
        self.toast(T_POOL, p as f64); self.dispatch(false);
    }
    pub fn set_solo(&mut self, solo: bool) { if self.solo != solo { self.solo = solo; self.toast(T_MODE, solo as u8 as f64); self.dispatch(false); } }

    pub fn set_price(&mut self, p: f64) { if self.live && p > 0.0 { self.price = p; self.dispatch(false); } }

    pub fn toast(&mut self, code: f64, arg: f64) { if self.toasts.len() < 15 { self.toasts.push((code, arg)); } }

    // ------------------------------------------------------------ heat
    pub fn ambient(&self) -> f64 {
        let doy = ((self.day + self.day_off) % 365) as f64;
        let season = 1.0 - fabs(doy - 182.0) / 91.0;
        let st = self.st();
        st.temp + season * st.swing + if self.heat_days > 0 { self.heat_add } else { 0.0 }
            + match self.weather { W_RAIN => -4.0, W_STORM => -6.0, W_DUST => 2.0, _ => 0.0 }
    }

    /// Cells a fan at (fi,fj) blowing `dir` cools, with strength in °C. Lanes stop at plant buildings.
    pub fn fan_lane(&self, fi: i32, fj: i32, dir: u8, out: &mut [(i32, i32, f32); 12]) -> usize {
        let (dx, dy) = DIRS[(dir % 4) as usize];
        let (px, py) = (-dy, dx);
        let mut n = 0;
        for d in 1..=4i32 {
            let (ci, cj) = (fi + dx * d, fj + dy * d);
            let blocked = match self.tile(ci, cj) {
                None => true,
                Some(t) => t.occ >= 0 && { let k = self.blds[t.occ as usize].kind; !is_consumer(k) && k != K_POLE && k != K_SIGN },
            };
            if blocked { break; }
            let s = LANE[(d - 1) as usize];
            out[n] = (ci, cj, s); n += 1;
            for side in [-1i32, 1] {
                let (si, sj) = (ci + px * side, cj + py * side);
                if self.tile(si, sj).is_some() { out[n] = (si, sj, s * 0.45); n += 1; }
            }
        }
        n
    }

    /// °C of fan cooling reaching this building's tile.
    pub fn cooling_at(&self, b: &Bld) -> f64 {
        if self.cool.len() != MAP * MAP { return 0.0; }
        fmin(self.cool[b.j as usize * MAP + b.i as usize] as f64, 30.0)
    }
    /// Running miners packed right next to this one (each adds hot exhaust).
    /// Heat reuse: each running miner within reach of a greenhouse pipes its exhaust to the nearest one with room.
    fn link_heat(&mut self) {
        let n = self.blds.len();
        self.piped.clear(); self.piped.resize(n, -1); self.gh_kw.clear(); self.gh_kw.resize(n, 0.0); self.heat_kw = 0.0;
        let ghs: Vec<usize> = (0..n).filter(|&x| self.blds[x].kind == K_GREENHOUSE).collect();
        if ghs.is_empty() { return; }
        for x in 0..n {
            let b = self.blds[x];
            if !is_miner(b.kind) || b.broken != 0 || b.off != 0 { continue; }
            let f = fmin(self.net_frac(&b), 1.0); if f <= 0.0 { continue; }
            let k = KINDS[b.kind as usize];
            // liquid-cooled: a hydro container, or a box touching an immersion tank
            let liquid = b.kind == K_MINER4 || self.blds.iter().any(|o| o.kind == K_IMMERSION && o.off == 0 && imax(iabs(o.i as i32 - b.i as i32), iabs(o.j as i32 - b.j as i32)) <= 1);
            let heat = k.kw * b.tune_kw() * f * HEAT_CAPTURE[liquid as usize];
            let mut best = usize::MAX; let mut bd = i32::MAX;
            for &gx in ghs.iter() {
                let g = self.blds[gx]; let gk = KINDS[g.kind as usize];
                // distance from the box to the greenhouse's footprint
                let di = imax(imax(g.i as i32 - b.i as i32, b.i as i32 - (g.i as i32 + gk.w as i32 - 1)), 0);
                let dj = imax(imax(g.j as i32 - b.j as i32, b.j as i32 - (g.j as i32 + gk.d as i32 - 1)), 0);
                let d = imax(di, dj);
                if d <= HEAT_REACH && d < bd && self.gh_kw[gx] + heat <= HEAT_CAP_KW { bd = d; best = gx; }
            }
            if best != usize::MAX { self.piped[x] = best as i32; self.gh_kw[best] += heat; self.heat_kw += heat; }
        }
    }
    /// What growers pay for a MWh of heat right now: more when it's cold and when gas (their alternative) is dear.
    pub fn heat_price(&self) -> f64 { HEAT_USD * clamp((26.0 - self.ambient()) / 10.0, 0.15, 1.5) * (0.5 + 0.5 * self.gas_f) }
    pub fn heat_day(&self) -> f64 { self.heat_kw * 24.0 / 1000.0 * self.heat_price() }

    pub fn packed_at(&self, x: usize) -> u32 {
        let b = self.blds[x]; let mut n = 0;
        for (y, o) in self.blds.iter().enumerate() {
            if y == x || !is_miner(o.kind) || o.broken != 0 || o.off != 0 || self.net_frac(o) <= 0.0 { continue; }
            if imax(iabs(o.i as i32 - b.i as i32), iabs(o.j as i32 - b.j as i32)) <= 1 { n += 1; }
        }
        n
    }

    pub fn net_frac(&self, b: &Bld) -> f64 { self.nets.get(b.net as usize).map_or(0.0, |n| n.frac) }

    pub fn update_temps(&mut self) {
        let amb = self.ambient();
        if self.cool.len() != MAP * MAP { self.cool.clear(); self.cool.resize(MAP * MAP, 0.0); }
        for c in self.cool.iter_mut() { *c = 0.0; }
        let mut lane = [(0i32, 0i32, 0f32); 12];
        for x in 0..self.blds.len() {
            let b = self.blds[x];
            if b.kind == K_FAN {
                let run = if b.off != 0 { 0.0 } else { self.net_frac(&b) as f32 * (1.0 - b.dust / 200.0) };
                let n = self.fan_lane(b.i as i32, b.j as i32, b.dir, &mut lane);
                for &(ci, cj, s) in lane.iter().take(n) { self.cool[cj as usize * MAP + ci as usize] += s * run; }
            }
        }
        let n = self.blds.len();
        self.link_heat();
        for x in 0..n {
            if !is_miner(self.blds[x].kind) { continue; }
            let (bi, bj) = (self.blds[x].i as i32, self.blds[x].j as i32);
            let fans = fmin(self.cool[bj as usize * MAP + bi as usize] as f64, 30.0) + if self.piped[x] >= 0 { HEAT_COOL } else { 0.0 };
            let mut imm = 0; let mut packed = 0;
            for y in 0..n {
                if y == x { continue; }
                let o = &self.blds[y];
                let d = imax(iabs(o.i as i32 - bi), iabs(o.j as i32 - bj));
                if o.kind == K_IMMERSION && o.off == 0 && d <= 1 && imm < 2 && self.net_frac(o) > 0.5 { imm += 1; }
                if is_miner(o.kind) && o.broken == 0 && o.off == 0 && d <= 1 && self.net_frac(o) > 0.0 { packed += 1; } // hot exhaust from neighbours
            }
            let running = self.blds[x].broken == 0 && self.blds[x].off == 0 && self.net_frac(&self.blds[x]) > 0.0;
            let gen_heat = if running { 40.0 + (self.blds[x].kind - K_MINER1) as f64 * 3.0 + self.blds[x].tune_heat() + self.blds[x].dead[P_FAN] as f64 * FAN_HEAT } else { 0.0 };
            let soak = packed as f64 * 4.5 + if running { self.blds[x].dust as f64 * 0.15 } else { 0.0 };
            let cool = fans + imm as f64 * 22.0;
            self.blds[x].temp = (amb + gen_heat + soak - fmin(cool, 60.0)) as f32;
        }
    }

    // ------------------------------------------------------------ simulation
    pub fn step_block(&mut self) {
        if self.over != 0 || self.pending != EV_NONE { return; }
        self.block += 1;
        if self.block % 6 == 0 {
            let target = (0.15 + self.rng.f() * 1.1) * self.t_wind;
            self.wind_now = self.wind_now * 0.7 + target * 0.3;
            self.update_temps();
        }
        if self.weather == W_STORM && self.rng.f() < 1.0 / 40.0 { self.lightning(); }
        self.dispatch(true);
        if self.spike_blocks > 0 { self.spike_blocks -= 1; }
        if self.curtail_blocks > 0 {
            self.curtail_blocks -= 1;
            if self.curtail_blocks == 0 {
                if self.curtail_mode == 1 {
                    self.cash += self.curtail_pay; self.curtail_earned += self.curtail_pay; self.curtails += 1;
                    self.toast(T_CURTAIL_PAID, self.curtail_pay);
                } else if self.curtail_mode == 2 { self.toast(T_CURTAIL_BILL, self.curtail_extra); }
                self.curtail_mode = 0; self.curtail_pay = 0.0; self.curtail_extra = 0.0;
            }
        }
        if (self.block - self.start_block) % BLOCKS_PER_DAY == 0 { self.new_day(); }
        if !self.follower {
            if self.block >= self.next_retarget { self.retarget(); }
            if self.block % HALVING == 0 { self.halving(); }
        }
    }

    /// Per network: match supply to demand, cheapest source first, with line losses.
    pub fn dispatch(&mut self, apply: bool) {
        let hour = ((self.block - self.start_block) % BLOCKS_PER_DAY) as f64 / 6.0;
        let daylight = fmax(0.0, 1.0 - fabs(hour - 12.0) / 6.0);
        let throttle_ev = self.throttle_days > 0;
        let nn = self.nets.len();
        for nt in self.nets.iter_mut() { nt.cap = 0.0; nt.demand = 0.0; }
        // spot contract: the price right now, and which boxes it switches off
        let spot_now = if self.contract == C_SPOT { self.spot_now() } else { 0.0 };
        let capped = self.contract == C_SPOT && self.spot_cap > 0.0 && spot_now > self.spot_cap;
        let mut econ: Vec<bool> = alloc::vec![false; self.blds.len()];
        if self.contract == C_SPOT && self.spot_cap <= 0.0 && self.net_th > 0.0 {
            let mut has_grid: Vec<bool> = alloc::vec![false; nn];
            for b in self.blds.iter() { if b.kind == K_GRID && (b.net as usize) < nn { has_grid[b.net as usize] = true; } }
            let rev_th = 144.0 * (self.reward + self.fees) * self.price / self.net_th * (1.0 - self.pool_fee()) * (1.0 - self.stale_rate());
            for (x, b) in self.blds.iter().enumerate() {
                if !is_miner(b.kind) || b.off != 0 || b.broken != 0 || (b.net as usize) >= nn || !has_grid[b.net as usize] { continue; }
                let k = KINDS[b.kind as usize];
                let rev = k.th * b.tune_th() * (0.7 + 0.3 * b.health as f64 / 100.0) * (1.0 - b.reject()) * rev_th;
                if rev < k.kw * b.tune_kw() * 24.0 / 1000.0 * spot_now { econ[x] = true; } // costs more in power than it earns
            }
        }
        self.econ_off = econ.iter().filter(|e| **e).count() as u32; self.capped = capped;
        for (x, b) in self.blds.iter().enumerate() {
            if b.net as usize >= nn { continue; }
            let k = KINDS[b.kind as usize];
            if b.off != 0 || econ[x] { continue; }
            if is_miner(b.kind) {
                if b.broken != 0 { continue; }
                let f = if throttle_ev || b.temp > 80.0 { 0.7 } else { 1.0 };
                self.nets[b.net as usize].demand += k.kw * b.tune_kw() * b.parts_f() * f;
            } else if b.kind == K_FAN || b.kind == K_IMMERSION { self.nets[b.net as usize].demand += k.kw; }
        }
        // sources sorted by cost (insertion sort, few sources)
        let mut src: Vec<(f64, f64, u16, u8)> = Vec::with_capacity(32);
        // networks switched off on purpose for a curtailment call: dark, but not a shortage
        let mut grid_net: Vec<bool> = alloc::vec![false; nn];
        let curtailed = self.curtail_blocks > 0 && self.curtail_mode == 1;
        let spot = if self.curtail_blocks > 0 && self.curtail_mode == 2 { self.spot_x } else { 1.0 };
        for b in self.blds.iter() {
            if !is_power(b.kind) || b.net as usize >= nn { continue; }
            let k = KINDS[b.kind as usize];
            let base = k.cap_kw[(b.level - 1) as usize];
            let (avail, usd) = match b.kind {
                K_GRID => (if self.brown_days > 0 || curtailed || capped { 0.0 } else { base * self.grid_f() }, if self.contract == C_SPOT { spot_now } else { self.grid_usd * spot }),
                K_DIESEL => (base, k.usd_mwh * self.diesel_f * if self.fuel_days > 0 { 0.6 } else { 1.0 }),
                K_GAS => (base, k.usd_mwh * self.gas_f * if self.gas_days > 0 { 2.1 } else { 1.0 }),
                K_COAL => (base, k.usd_mwh + if self.t_coal { 0.0 } else { 15.0 }),
                K_SOLAR => (base * daylight * self.t_sun * match self.weather { W_RAIN => 0.4, W_STORM => 0.15, W_DUST => 0.5, _ => 1.0 }, 0.0),
                K_WIND => (base * if self.storm_days > 0 || self.weather == W_STORM { 0.0 } else { fmin(self.wind_now * if self.weather == W_DUST { 1.4 } else { 1.0 }, 1.3) }, 0.0),
                K_HYDRO => (base * if self.drought_days > 0 { 0.4 } else { 1.0 }, k.usd_mwh),
                _ => (base, k.usd_mwh),
            };
            src.push((usd, avail, b.net, b.kind));
            if b.kind == K_GRID && (curtailed || capped) { grid_net[b.net as usize] = true; }
            self.nets[b.net as usize].cap += avail;
        }
        for a in 1..src.len() { let mut x = a; while x > 0 && src[x - 1].0 > src[x].0 { src.swap(x - 1, x); x -= 1; } }
        let mut left: Vec<f64> = self.nets.iter().map(|n| n.demand * (1.0 + n.loss)).collect();
        let mut usd_block = 0.0; let mut grid_kw = 0.0; let mut spot_extra = 0.0; let mut grid_usd_block = 0.0;
        for s in src.iter() {
            let l = &mut left[s.2 as usize];
            if *l <= 0.0 { continue; }
            let take = fmin(*l, s.1);
            usd_block += take * (10.0 / 60.0) / 1000.0 * s.0;
            if s.3 == K_GRID { grid_kw += take; grid_usd_block += take * (10.0 / 60.0) / 1000.0 * s.0; spot_extra += take * (10.0 / 60.0) / 1000.0 * s.0 * (1.0 - 1.0 / spot); }
            *l -= take;
        }
        if !curtailed && !capped { self.grid_kw = grid_kw; }
        let mut short = false; let mut used = 0.0; let mut cap = 0.0; let mut dem = 0.0;
        let mut short_kw = 0.0; let mut short_nets = 0;
        for (x, nt) in self.nets.iter_mut().enumerate() {
            let deliver = nt.cap * (1.0 - nt.loss);
            nt.frac = if nt.demand > 0.0 { fmin(1.0, deliver / nt.demand) } else { 1.0 };
            if nt.demand > 0.0 && nt.frac < 0.999 && nt.sources > 0 && !grid_net[x] { short = true; short_kw += nt.demand - deliver; short_nets += 1; }
            used += fmin(nt.demand, deliver); cap += nt.cap; dem += nt.demand;
        }
        // hashrate
        self.short_kw = short_kw; self.short_nets = short_nets;
        let mut th = 0.0; let mut acc = 0.0; let mut lost = 0.0; let mut unpowered = 0u32; let mut off = 0u32;
        for (x, b) in self.blds.iter().enumerate() {
            if !is_consumer(b.kind) { continue; }
            if b.off != 0 { if is_miner(b.kind) { off += 1; } continue; }
            if econ[x] { continue; }
            let f = self.nets.get(b.net as usize).map_or(0.0, |n| n.frac);
            if f <= 0.0 && b.broken == 0 && !grid_net.get(b.net as usize).copied().unwrap_or(false) { unpowered += 1; }
            if !is_miner(b.kind) || b.broken != 0 { continue; }
            let k = KINDS[b.kind as usize];
            let thr = if throttle_ev || b.temp > 80.0 { 0.6 } else { 1.0 };
            let h0 = k.th * b.tune_th() * thr * f * (0.7 + 0.3 * b.health as f64 / 100.0);
            let h = h0 * b.parts_f(); lost += h0 - h;
            th += h; acc += h * (1.0 - b.reject());
        }
        self.unpowered = unpowered; self.off_count = off;
        self.frac = if dem > 0.0 { fmin(1.0, used / dem) } else { 1.0 };
        self.used_kw = used; self.cap_kw = cap; self.my_th = th; self.acc_th = acc; self.parts_lost_th = lost;
        // FPPS: paid every block for your accepted share of the work, minus the pool's fee.
        // PPLNS: paid your cut only when the pool finds a block. Solo: you win the whole block, or nothing.
        let share = if self.net_th > 0.0 { acc * (1.0 - self.stale_rate()) / self.net_th } else { 0.0 };
        let btc_block = share * (self.reward + self.fees) * (1.0 - self.pool_fee());
        let pool = POOLS[self.pool as usize % POOLS.len()];
        self.rev_day = btc_block * self.price * 144.0;
        self.cost_day = usd_block * 144.0;
        if apply {
            if self.solo {
                if share > 0.0 && self.rng.f() < share {
                    let won = self.reward + self.fees;
                    self.btc += won; self.blocks_found += 1; self.last_found = self.block;
                    self.toast(T_SOLO_BLOCK, won);
                }
            } else if pool.pplns {
                self.pplns_exp += btc_block;
                if share > 0.0 && self.rng.f() < pool.share {
                    let pay = share / pool.share * (self.reward + self.fees) * (1.0 - pool.fee);
                    self.btc += pay; self.pplns_paid += pay; self.pool_blocks += 1;
                }
            } else { self.btc += btc_block; }
            self.cash -= usd_block;
            self.curtail_extra += spot_extra;
            if self.heat_kw > 0.0 { let v = self.heat_kw * (10.0 / 60.0) / 1000.0 * self.heat_price(); self.cash += v; self.heat_earned += v; }
            let mwh = grid_kw * (10.0 / 60.0) / 1000.0;
            self.c_mwh += mwh; self.c_cost += grid_usd_block;
            if self.contract == C_DR && !curtailed { self.dr_mwh += mwh; }
            if capped || self.econ_off > 0 { self.cap_blocks += 1; }
            if short { if self.streak > 0 { self.toast(T_SHORTAGE, dem - used); } self.streak = 0; } else { self.streak += 1; }
        }
    }

    fn new_day(&mut self) {
        self.day += 1;
        if self.scen_id > 0 {
            self.scen_day();
            if self.over != 0 { return; }
        } else if !self.live && !self.follower {
            let vol = [0.025, 0.035, 0.055][self.diff as usize];
            // random walk around a trend that rises ~15% a year; the pull keeps runs from drifting to $5k or $1M
            self.anchor_px *= 1.0004;
            self.price *= 1.0 + (self.rng.f() - 0.5) * 2.0 * vol;
            self.price += (self.anchor_px - self.price) * 0.015;
            self.price = fmax(self.price, 5_000.0);
        }
        self.gas_f = clamp(self.gas_f * (1.0 + (self.rng.f() - 0.5) * 0.12), 0.5, 3.0);
        self.diesel_f = clamp(self.diesel_f * (1.0 + (self.rng.f() - 0.5) * 0.08), 0.7, 2.0);
        if self.st().flare { self.well_f = clamp(self.well_f * WELL_DECLINE * (1.0 + (self.rng.f() - 0.5) * 0.04), 0.25, 1.0); }
        if self.st().dry { let d = self.dry_season(); let was = { let doy = ((self.day + self.day_off + 364) % 365) as f64; 1.0 - fabs(doy - 182.0) / 91.0 < -0.3 }; if d != was { self.toast(T_DRY_SEASON, d as u8 as f64); } }
        self.spot_walk = clamp(self.spot_walk * (1.0 + (self.rng.f() - 0.5) * 0.3) + (1.0 - self.spot_walk) * 0.25, 0.5, 2.0);
        if self.dr_mwh > 0.0 { let c = self.dr_mwh * DR_STANDBY; self.cash += c; self.dr_earned += c; self.dr_mwh = 0.0; }
        if self.fee_days > 0 && !self.follower { self.fee_days -= 1; if self.fee_days == 0 { self.fee_mult = 1.0; self.toast(T_FEES_CALM, 0.0); } }
        if self.scen_id == 0 && !self.follower { self.fees = self.base_fees * (0.6 + self.rng.f() * 0.8) * self.fee_mult; }
        if !self.follower { self.market_day(); }
        for t in [&mut self.heat_days, &mut self.gas_days, &mut self.brown_days, &mut self.drought_days,
                  &mut self.storm_days, &mut self.throttle_days, &mut self.fuel_days] { if *t > 0 { *t -= 1; } }
        self.roll_weather();
        self.update_temps();
        // heat wear and fires
        let n = self.blds.len();
        for x in 0..n {
            if !is_miner(self.blds[x].kind) || self.blds[x].broken != 0 { continue; }
            let sag = { let f = self.net_frac(&self.blds[x]); f > 0.0 && f < 0.95 };
            if self.blds[x].off != 0 { self.blds[x].fire = 0; continue; }
            let t = self.blds[x].temp;
            if t > 95.0 && self.blds[x].fire == 0 && self.rng.f() < 0.35 { self.blds[x].fire = 1; }
            if t < 88.0 { self.blds[x].fire = 0; }
            let wear = (0.04 + if t > 75.0 { (t - 75.0) * 0.35 } else { 0.0 }) * self.blds[x].tune_wear() as f32 + if self.blds[x].fire != 0 { 18.0 } else { 0.0 } + if sag { 1.5 } else { 0.0 }; // brownouts stress PSUs
            self.blds[x].health -= wear;
            if self.blds[x].health <= 0.0 {
                self.blds[x].health = 0.0; self.blds[x].broken = 1; self.blds[x].fire = 0; self.toast(T_BROKE, x as f64);
            }
        }
        self.parts_day();
        // money trouble
        if self.cash < 0.0 {
            self.unpaid_days += 1;
            let early = !self.unlocked[2];
            if self.diff == 0 && early && !self.follower && self.day >= self.last_bailout_day + 30 {
                self.cash += 15_000.0; self.bailouts_used += 1; self.last_bailout_day = self.day; self.unpaid_days = 0;
                self.toast(T_BAILOUT, 15_000.0);
            } else {
                let limit = [7, 3, 1][self.diff as usize];
                self.toast(T_UNPAID, (limit - self.unpaid_days as i32) as f64);
                if self.unpaid_days as i32 >= limit { self.over = 1; }
            }
        } else { self.unpaid_days = 0; }
        let p = [0.22, 0.38, 0.45][self.diff as usize];
        if self.scen_id > 0 { self.scen_events(); }
        else if self.day > 3 && self.rng.f() < p { self.random_event(); }
    }

    fn has(&self, kind: u8) -> bool { self.blds.iter().any(|b| b.kind == kind) }

    fn random_event(&mut self) {
        let summer = { let doy = ((self.day + self.day_off) % 365) as f64; fabs(doy - 182.0) < 60.0 };
        let hot_miner = self.blds.iter().any(|b| is_miner(b.kind) && b.temp > 80.0 && b.broken == 0);
        let doy = (self.day + self.day_off) % 365; let peak_season = summer || doy < 45 || doy > 330; // heat and cold both stress a grid
        let sw = self.st().w; let tariff_w = self.st().tariff;
        let w: [f64; 16] = [
            sw[0] * if summer { 22.0 } else { 10.0 },
            if self.has(K_GAS) { 12.0 } else { 0.0 },
            sw[1] * if self.has(K_GRID) { 12.0 } else { 0.0 },
            if self.live || self.follower { 0.0 } else { 10.0 },   // live mode: the price is the real price
            if self.live || self.follower { 0.0 } else { 9.0 },
            0.0, // (was "lucky block": pools pay per share, so this is now the solo-mining choice instead)
            if self.has(K_COAL) { 8.0 } else { 0.0 },
            sw[2] * if self.has(K_HYDRO) { 8.0 } else { 0.0 },
            sw[3] * if self.has(K_WIND) { 8.0 } else { 0.0 },
            if hot_miner { 12.0 } else { 3.0 },
            if self.follower { 0.0 } else { 7.0 },
            if self.has(K_DIESEL) { 6.0 } else { 0.0 },
            sw[4] * (1.0 - GUARD_CUT * self.guards.min(3) as f64) * (if self.day < 10 { 0.0 } else if self.exposed_miners > 0 { (4.0 + 2.0 * fmin(self.exposed_miners as f64, 8.0)) * [1.0, 1.4, 1.8][self.diff as usize] } else { 5.0 })
                * (1.0 - 0.12 * fmin(self.blds.iter().filter(|b| b.kind == K_SIGN).count() as f64, 4.0)),
            if (self.grid_kw > 1.0 || self.contract == C_SPOT) && self.curtail_blocks == 0 && self.spike_blocks == 0 && self.st().market { (if peak_season { 14.0 } else { 6.0 }) * if self.contract == C_DR { DR_CALLS_X } else { 1.0 } } else { 0.0 },
            if self.fee_days == 0 && !self.follower { 6.0 } else { 0.0 },
            if self.day > 30 && self.has(K_GRID) { tariff_w } else { 0.0 },
        ];
        let total: f64 = w.iter().sum();
        let mut r = self.rng.f() * total;
        let mut ev = 0;
        for (k, x) in w.iter().enumerate() { if r < *x { ev = k as i32; break; } r -= *x; }
        self.apply_event(ev);
    }

    /// Fire one random event (also used by the console debug helper).
    pub fn apply_event(&mut self, ev: i32) {
        self.roll = 1 + self.rng.below(20) as i32;
        match ev {
            EV_HEATWAVE => { self.heat_days = 3; self.heat_add = 9.0; self.update_temps(); self.pending = ev; self.ev_arg = self.chiller_cost(); }
            EV_GAS_SPIKE => { self.gas_days = 3; self.toast(T_EVENT, ev as f64); }
            EV_BROWNOUT => { self.brown_days = 1; self.toast(T_EVENT, ev as f64); }
            EV_PRICE_CRASH => { self.price *= 0.75; self.toast(T_EVENT, ev as f64); }
            EV_PRICE_PUMP => { self.price *= 1.2; self.toast(T_EVENT, ev as f64); }
            EV_LUCKY => {
                // Your pool found a block and you hit the jackpot share: a full block reward + fees.
                let b = self.reward + self.fees;
                self.btc += b; self.blocks_found += 1;
                self.toast(T_BLOCK_FOUND, b);
            }
            EV_INSPECTOR => { self.pending = ev; self.ev_arg = clamp(self.cash * 0.08, 1_500.0, 250_000.0); }
            EV_DROUGHT => { self.drought_days = 5; self.toast(T_EVENT, ev as f64); }
            EV_STORM => { self.set_weather(W_STORM, 1); }
            EV_FIRE => {
                let mut best = usize::MAX; let mut bt = -1.0f32;
                for (x, b) in self.blds.iter().enumerate() { if is_miner(b.kind) && b.broken == 0 && b.temp > bt { bt = b.temp; best = x; } }
                if best != usize::MAX { self.blds[best].broken = 1; self.blds[best].health = 0.0; self.blds[best].fire = 0; self.pending = ev; self.ev_arg = best as f64; }
            }
            EV_RAID => {
                self.raids += 1;
                if self.guards > 0 && self.rng.f() < fmin(0.3 * self.guards as f64, 0.9) { self.toast(T_RAID_REPELLED, 9.0); return; } // the guards saw them off
                let mut exp: Vec<usize> = (0..self.blds.len()).filter(|&x| is_miner(self.blds[x].kind) && self.blds[x].exposed).collect();
                if exp.is_empty() {
                    // fenced in: the weakest stretch of perimeter decides whether they get through
                    let chance = [0.0, 0.40, 0.15, 0.03][self.fence_level.min(3) as usize];
                    let miners: Vec<usize> = (0..self.blds.len()).filter(|&x| is_miner(self.blds[x].kind)).collect();
                    if miners.is_empty() || self.rng.f() >= chance { self.toast(T_RAID_REPELLED, self.fence_level as f64); return; }
                    let lvl = self.fence_level;
                    let walls: Vec<usize> = (0..self.blds.len()).filter(|&x| self.blds[x].kind == K_WALL && self.blds[x].exposed && self.blds[x].level == lvl).collect();
                    let victim = miners[self.rng.below(miners.len() as u32) as usize];
                    let v = self.sell_value(victim);
                    let wall = if walls.is_empty() { usize::MAX } else { walls[self.rng.below(walls.len() as u32) as usize] };
                    // remove the higher index first so the other stays valid
                    { let vb = self.blds[victim]; self.fx.push((6, vb.i as i32, vb.j as i32, 1, 1)); }
                    if wall != usize::MAX && wall > victim { self.remove(wall); self.remove(victim); }
                    else { self.remove(victim); if wall != usize::MAX { self.remove(wall); } }
                    self.toast(T_RAID_BREACH, v);
                    return;
                }
                let max = [1u32, 2, 3][self.diff as usize];
                let n = (1 + self.rng.below(max)) as usize;
                let mut take: Vec<usize> = Vec::new();
                for _ in 0..n.min(exp.len()) { let r = self.rng.below(exp.len() as u32) as usize; take.push(exp.swap_remove(r)); }
                take.sort_unstable_by(|a, b| b.cmp(a));
                let mut v = 0.0;
                for x in take { v += self.sell_value(x); let vb = self.blds[x]; self.fx.push((6, vb.i as i32, vb.j as i32, 1, 1)); self.remove(x); }
                self.toast(T_RAID, v);
            }
            EV_CURTAIL => {
                // Real demand response (ERCOT, for one): when the grid is short, big loads that switch off get paid.
                let rate = CURTAIL_BASE + self.roll as f64 * CURTAIL_STEP; // $/MWh offered
                self.grid_call(rate, rate * SPIKE_X, CURTAIL_SPOT_X);
            }
            EV_FEE_SPIKE => { self.fee_spike(4.0 + self.roll as f64 * 0.8, 1 + (self.roll / 8) as u32); }
            EV_TARIFF => {
                self.grid_usd = (fmin(self.grid_usd * (1.05 + self.rng.f() * 0.1), self.st().grid * 1.6 + 10.0 * self.diff as f64) * 10.0) as i64 as f64 / 10.0; self.tariffs += 1;
                self.toast(T_TARIFF, self.grid_usd); self.dispatch(false);
            }
            EV_DIFF_SWING => { self.diff_swing = if self.rng.f() < 0.5 { -0.05 } else { 0.05 }; self.ev_arg = self.diff_swing; self.toast(T_EVENT, ev as f64); }
            _ => { self.fuel_days = 5; self.toast(T_EVENT, EV_FUEL_DEAL as f64); }
        }
    }

    /// Base running cost of a power source in $/MWh (the grid depends on your contract).
    pub fn usd_mwh(&self, kind: u8) -> f64 { if kind == K_GRID { self.grid_usd } else { KINDS[kind as usize].usd_mwh } }

    pub fn chiller_cost(&self) -> f64 {
        // three days of rented chillers: roughly what throttling would cost you, so it's a real choice
        fmax(400.0, self.rev_day * 0.9)
    }

    pub fn choose(&mut self, c: u32) {
        match self.pending {
            EV_HEATWAVE => match c {
                0 => { let cost = self.ev_arg; self.cash -= cost; self.heat_add = 0.0; self.update_temps(); }
                1 => { self.throttle_days = 3; }
                _ => {
                    let r = 1 + self.rng.below(20) as i32; self.roll = r;
                    if r < 11 {
                        let mut hit = 0;
                        for x in 0..self.blds.len() { if hit < 2 && is_miner(self.blds[x].kind) && self.blds[x].broken == 0 { self.blds[x].fire = 1; self.blds[x].health = fmin(self.blds[x].health as f64, 25.0) as f32; hit += 1; } }
                    } else { self.heat_add = 0.0; self.update_temps(); }
                }
            },
            EV_INSPECTOR => match c {
                0 => { self.cash -= self.ev_arg; }
                _ => { let r = 1 + self.rng.below(20) as i32; self.roll = r; if r < 14 { self.cash -= self.ev_arg * INSPECTOR_LOSS_X; } }
            },
            EV_FIRE => { if c == 0 { let idx = self.ev_arg as usize; self.repair(idx); } }
            EV_CURTAIL => {
                self.curtail_blocks = (CURTAIL_HOURS * 6.0) as u32;
                if c == 0 { self.curtail_mode = 1; self.curtail_pay = self.ev_arg; } else { self.curtail_mode = 2; self.curtail_extra = 0.0; }
                self.update_temps();
            }
            EV_CORE_TRIAL => {
                let r = 1 + self.rng.below(20) as i32; self.roll = r;
                if r >= 12 { self.over = 2; }
                else {
                    if let Some(x) = self.blds.iter().position(|b| b.kind == K_CORE) { self.remove(x); }
                    self.core_cooldown = self.block + 30 * BLOCKS_PER_DAY;
                    self.toast(T_CORE_FAIL, r as f64);
                }
            }
            _ => {}
        }
        self.pending = EV_NONE;
        self.dispatch(false);
    }

    /// What one TH/s earns per day in BTC terms x price ($/TH/day), before pool fees.
    pub fn hashprice(&self) -> f64 { if self.net_th > 0.0 { 144.0 * (self.reward + self.fees / self.fee_mult) * self.price / self.net_th } else { 0.0 } }

    fn retarget(&mut self) {
        // First retarget follows mempool.space's live estimate; later ones are projections.
        // Hashrate chases profit: when hashprice is above the marginal miner's breakeven, machines get
        // plugged in and difficulty rises; below it, old machines switch off. Better hardware keeps
        // pushing that breakeven down (~15% a year).
        self.hp_eq *= [0.996, 0.993, 0.989][self.diff as usize];
        let ch = if self.retargets == 0 && self.est_change != 0.0 { self.est_change } else {
            let (lo, hi) = [(-0.025, 0.025), (-0.025, 0.04), (-0.05, 0.08)][self.diff as usize];
            let pull = if self.hp_eq > 0.0 { clamp(0.12 * (self.hashprice() / self.hp_eq - 1.0), -0.08, 0.10) } else { 0.0 };
            clamp(lo + self.rng.f() * (hi - lo) + self.diff_swing + pull, -0.15, 0.15)
        };
        self.diff_swing = 0.0;
        self.net_th *= 1.0 + ch;
        self.last_retarget = ch;
        self.retargets += 1;
        self.next_retarget = self.block + RETARGET;
        self.toast(T_RETARGET, ch);
    }

    fn halving(&mut self) {
        self.reward = subsidy(self.block);
        self.halvings += 1;
        self.hp_eq *= 0.62; // the weakest machines unplug; the survivors settle for less
        let r = 1 + self.rng.below(20) as i32;
        self.roll = r;
        let m = if self.live || self.scen_id > 0 { 1.0 } else { match r { 1..=4 => 0.8, 5..=10 => 0.95, 11..=16 => 1.1, 17..=19 => 1.22, _ => 1.45 } };
        self.price *= m;
        self.ev_arg = m;
        self.pending = EV_HALVING;
        // everyone wants a transaction in the first post-halving blocks (block 840,000 paid 37.6 BTC in fees)
        if self.scen_id == 0 { self.fee_spike(12.0, 1); } // a history replay already has the real fees
    }

    // ------------------------------------------------------------ history scenarios
    /// Network hashrate (TH/s) implied by a difficulty: difficulty x 2^32 hashes every 600 s.
    pub fn th_from_difficulty(d: f64) -> f64 { d * 4_294_967_296.0 / 600.0 / 1.0e12 }

    /// Start a scenario on a fresh game: real start height, contract price, era machines and a prebuilt plant.
    pub fn scen_start(&mut self, id: u32, height: u64, grid_usd: f64, cash: f64, max_miner: u8, kind_a: u8, n_a: u32, kind_b: u8, n_b: u32) {
        let d0 = if self.scen.is_empty() { return } else { self.scen[0] };
        self.scen_id = id; self.grid_usd = grid_usd; self.max_miner = max_miner; self.live = false;
        self.block = height; self.start_block = height; self.next_retarget = u64::MAX; self.retargets = 0;
        self.reward = subsidy(height); self.price = d0[0]; self.anchor_px = d0[0]; self.net_th = Self::th_from_difficulty(d0[1]);
        self.fees = d0[2]; self.base_fees = d0[2]; self.cash = cash; self.day = 0; self.halvings = 0;
        self.unlocked = [false; N_TIERS]; self.unlocked[0] = true; self.unlocked[1] = true;
        // a working plant: the grid hookup at level 3, rows of boxes fed by a line of poles
        if let Some(x) = self.blds.iter().position(|b| is_miner(b.kind)) { self.remove(x); }
        if let Some(x) = self.blds.iter().position(|b| b.kind == K_GRID) { self.blds[x].level = 3; self.blds[x].invested = KINDS[K_GRID as usize].cost + KINDS[K_GRID as usize].up2 + KINDS[K_GRID as usize].up3; }
        let mut left = [(kind_a, n_a), (kind_b, n_b)];
        let mut j = 27i32;
        while (left[0].1 > 0 || left[1].1 > 0) && j < 44 {
            self.scen_tile(20, j); self.force_place(K_POLE, 20, j, 0);
            for i in 21..36i32 {
                let slot = if left[0].1 > 0 { 0 } else if left[1].1 > 0 { 1 } else { break };
                self.scen_tile(i, j); self.force_place(left[slot].0, i, j, 0); left[slot].1 -= 1;
            }
            j += 2;
        }
        self.rebuild_nets(); self.dispatch(false);
        // machines are priced off that day's real hashprice
        self.mkt_idx = self.mkt_raw(); self.restock(true);
        // a 36-container site has a technician and a small shelf of spares
        self.techs = 1; self.spare_target = [4, 4, 10]; self.spares = [4, 4, 10]; self.sync_workers();
        for b in self.blds.iter_mut() { if is_miner(b.kind) { b.invested = 0.0; } }
        // Scored on cash + bitcoin + what the machines would fetch. Holding = selling it all on day one for bitcoin.
        self.scen_worth0 = self.cash + self.fleet_value(); self.scen_hold_btc = self.scen_worth0 / self.price;
    }
    fn scen_tile(&mut self, i: i32, j: i32) { let t = self.tile_mut(i, j); t.tree = 0; if t.ground == G_WATER || t.ground == G_ROUGH { t.ground = G_GRASS; } }

    /// Cash + bitcoin + what the equipment would fetch (miners at black-market prices, the rest at 40%).
    pub fn worth(&self) -> f64 { self.cash + self.btc * self.price + (0..self.blds.len()).map(|x| self.sell_value(x)).sum::<f64>() }

    /// A new day of a scenario: today's real price, difficulty and fees.
    fn scen_day(&mut self) {
        let d = self.day as usize;
        if d >= self.scen.len() { self.over = 3; return; }
        let row = self.scen[d];
        self.price = row[0];
        let th = Self::th_from_difficulty(row[1]);
        if fabs(th / self.net_th - 1.0) > 1e-6 {
            let ch = th / self.net_th - 1.0;
            self.net_th = th; self.last_retarget = ch; self.retargets += 1;
            self.toast(T_RETARGET, ch);
        }
        self.fees = row[2]; self.base_fees = row[2];
    }

    /// Scripted events for the day (flags: 1 cold snap, 2 curtailment call at `rate` $/MWh,
    /// 4 rotating outage, 8 loan repayment of `rate` dollars).
    fn scen_events(&mut self) {
        let d = self.day as usize; if d >= self.scen.len() { return; }
        let (flags, rate) = (self.scen[d][3] as u32, self.scen[d][4]);
        if flags & 8 != 0 { self.cash -= rate; self.toast(T_LOAN, rate); }
        if flags & 1 != 0 { self.heat_days = 2; self.heat_add = -26.0; self.update_temps(); } // an ice storm: bitter cold, no lightning
        if flags & 4 != 0 { self.brown_days = 1; self.toast(T_EVENT, EV_BROWNOUT as f64); }
        if flags & 2 != 0 && (self.grid_kw > 1.0 || self.contract == C_SPOT) && self.curtail_blocks == 0 {
            self.roll = clamp((rate - CURTAIL_BASE) / CURTAIL_STEP + 0.5, 1.0, 20.0) as i32;
            self.grid_call(rate, rate, fmax(1.0, rate / self.grid_usd)); // history: the spike is the real spot price
        }
    }

    /// A grid emergency, played out by contract: an offer on fixed, a price spike on spot, an automatic call on DR.
    /// `rate` = what switching off is paid ($/MWh), `spike` = the spot price meanwhile, `spot_x` = decline multiplier.
    fn grid_call(&mut self, rate: f64, spike: f64, spot_x: f64) {
        match self.contract {
            C_SPOT => { self.spike_blocks = (CURTAIL_HOURS * 6.0) as u32; self.spike_usd = spike; self.toast(T_SPOT_SPIKE, spike); self.dispatch(false); }
            C_DR => {
                self.curtail_blocks = (CURTAIL_HOURS * 6.0) as u32; self.curtail_mode = 1;
                self.curtail_pay = self.grid_kw / 1000.0 * CURTAIL_HOURS * rate * DR_CALL_PAY;
                self.toast(T_DR_CALL, self.curtail_pay); self.update_temps(); self.dispatch(false);
            }
            _ => { self.spot_x = spot_x; self.ev_arg = self.grid_kw / 1000.0 * CURTAIL_HOURS * rate; self.pending = EV_CURTAIL; }
        }
    }

    // ------------------------------------------------------------ power contracts
    /// How much of the hookup's rating you actually get: a declining well, or a hydro grid in the dry season.
    pub fn grid_f(&self) -> f64 {
        let st = self.st();
        if st.flare { self.well_f } else if st.dry && self.dry_season() { 0.5 } else { 1.0 }
    }
    pub fn dry_season(&self) -> bool { let doy = ((self.day + self.day_off) % 365) as f64; 1.0 - fabs(doy - 182.0) / 91.0 < -0.3 }
    pub fn hour(&self) -> usize { (((self.block - self.start_block) % BLOCKS_PER_DAY) / 6) as usize }
    /// Today's spot price for an hour, before any emergency spike: contract price x 0.78 x the day's
    /// gas-driven level x the hour, with heat, cold, gas spikes and brownouts pushing it up.
    pub fn spot_at(&self, hour: usize) -> f64 {
        let h = hour % 24;
        let mut x = self.grid_usd * SPOT_MEAN * self.spot_walk * (0.6 + 0.4 * self.gas_f) * SPOT_HOUR[h];
        if self.heat_days > 0 { if self.heat_add < 0.0 { x *= 3.0 } else if (13..20).contains(&h) { x *= 2.5 } else { x *= 1.3 } }
        if self.gas_days > 0 { x *= 1.8; }
        if self.brown_days > 0 { x *= 4.0; }
        x
    }
    pub fn spot_now(&self) -> f64 { let x = self.spot_at(self.hour()); if self.spike_blocks > 0 { fmax(x, self.spike_usd) } else { x } }
    pub fn contract_days_left(&self) -> u64 { if self.switches == 0 { 0 } else { (self.contract_from + CONTRACT_DAYS).saturating_sub(self.day) } }
    /// Change contract. 30-day minimum term once you've switched. Returns 1 if it changed.
    pub fn set_contract(&mut self, c: u8) -> u32 {
        if c > C_DR || c == self.contract { return 0; }
        if c != C_FIXED && !self.st().market { self.toast(T_NO_MARKET, 0.0); return 0; }
        let left = self.contract_days_left();
        if left > 0 { self.toast(T_CONTRACT_LOCKED, left as f64); return 0; }
        if self.curtail_blocks > 0 && self.curtail_mode == 1 { self.toast(T_CONTRACT_LOCKED, 0.0); return 0; } // not mid-call
        self.contract = c; self.contract_from = self.day; self.switches += 1; self.c_mwh = 0.0; self.c_cost = 0.0;
        if c != C_SPOT { self.spike_blocks = 0; }
        if self.curtail_mode == 2 { self.curtail_blocks = 0; self.curtail_mode = 0; } // a declined call ends with the old contract
        self.toast(T_CONTRACT, c as f64); self.dispatch(false); 1
    }
    pub fn set_spot_cap(&mut self, usd: f64) { self.spot_cap = if usd <= 0.0 { 0.0 } else { clamp(usd, 10.0, 1.0e6) }; self.dispatch(false); }

    /// A follower site takes the network (price, difficulty, rewards, fees, machine market) and the wallet from the lead.
    pub fn follow(&mut self, lead: &Game) {
        self.price = lead.price; self.net_th = lead.net_th; self.reward = lead.reward; self.fees = lead.fees; self.base_fees = lead.base_fees;
        self.fee_mult = lead.fee_mult; self.fee_days = lead.fee_days; self.mkt_idx = lead.mkt_idx; self.stock = lead.stock;
        self.halvings = lead.halvings; self.last_retarget = lead.last_retarget; self.retargets = lead.retargets; self.est_change = lead.est_change;
        self.live = lead.live; self.hp_eq = lead.hp_eq; self.next_retarget = lead.next_retarget; self.anchor_px = lead.anchor_px;
        self.cash = lead.cash; self.btc = lead.btc;
        for t in 0..N_TIERS { self.unlocked[t] = self.unlocked[t] || lead.unlocked[t]; }
        for t in 0..2 { self.unlocked_x[t] = self.unlocked_x[t] || lead.unlocked_x[t]; }
    }

    /// After the core: carry on mining with free power.
    pub fn keep_mining(&mut self) { if self.over == 2 { self.over = 0; self.pending = EV_NONE; } }

    pub fn fee_spike(&mut self, mult: f64, days: u32) {
        self.fee_mult = mult; self.fee_days = days; self.fee_spikes += 1;
        self.fees = self.base_fees * mult;
        self.toast(T_FEE_SPIKE, mult);
        self.dispatch(false);
    }

    // ------------------------------------------------------------ workers
    pub fn tick_workers(&mut self) {
        let nb = self.blds.len() as u32;
        for x in 0..self.workers.len() {
            let mut w = self.workers[x];
            let dx = w.tx - w.x; let dy = w.ty - w.y;
            if fabs32(dx) + fabs32(dy) < 0.05 {
                if nb > 0 {
                    let mut pick = self.rng.below(nb) as usize;
                    if (x as u32) < self.techs {
                        let need: Vec<usize> = (0..self.blds.len()).filter(|&y| is_miner(self.blds[y].kind) && (self.blds[y].broken != 0 || self.blds[y].parts_out() > 0 || self.blds[y].dust >= 30.0)).collect();
                        if !need.is_empty() { pick = need[(x + (w.step / 400) as usize) % need.len()]; }
                    }
                    let b = self.blds[pick];
                    let k = KINDS[b.kind as usize];
                    w.tx = b.i as f32 + k.w as f32 + 0.2 * self.rng.f() as f32;
                    w.ty = b.j as f32 + k.d as f32 * self.rng.f() as f32;
                }
            } else {
                let sp = 0.012;
                if fabs32(dx) > 0.01 { w.x += if dx > 0.0 { sp } else { -sp }; }
                else if fabs32(dy) > 0.01 { w.y += if dy > 0.0 { sp } else { -sp }; }
                else { w.x = w.tx; w.y = w.ty; }
            }
            w.step = w.step.wrapping_add(1);
            self.workers[x] = w;
        }
    }

    // ------------------------------------------------------------ save / load
    pub fn save(&self, out: &mut Vec<u8>) {
        out.clear();
        let mut f = |v: f64| out.extend_from_slice(&v.to_le_bytes());
        let scal = [100.0, self.seed as f64, self.diff as f64, self.t_river as u8 as f64, self.t_sun, self.t_wind, self.t_coal as u8 as f64,
            self.cash, self.btc, self.price, self.net_th, self.reward, self.fees, self.block as f64, self.day as f64, self.halvings as f64,
            self.last_retarget, self.diff_swing, self.gas_f, self.diesel_f, self.wind_now, self.streak as f64, self.unpaid_days as f64,
            self.bailouts_used as f64, self.last_bailout_day as f64, self.heat_days as f64, self.heat_add, self.gas_days as f64,
            self.brown_days as f64, self.drought_days as f64, self.storm_days as f64, self.throttle_days as f64, self.fuel_days as f64,
            self.core_cooldown as f64, self.over as f64,
            self.live as u8 as f64, self.start_block as f64, self.next_retarget as f64, self.est_change, self.retargets as f64,
            self.base_fees, self.blocks_found as f64, self.raids as f64, self.weather as f64, self.weather_days as f64, self.solo as u8 as f64, self.last_found as f64,
            // v9
            self.fee_mult, self.fee_days as f64, self.fee_spikes as f64, self.curtail_blocks as f64, self.curtail_mode as f64,
            self.curtail_pay, self.curtail_extra, self.curtails as f64, self.curtail_earned, self.grid_kw, self.anchor_px, self.hp_eq,
            self.grid_usd, self.scen_id as f64, self.max_miner as f64, self.scen_worth0, self.scen_hold_btc, self.spot_x,
            // build .12: ASIC market
            self.mkt_idx, self.stock[0] as f64, self.stock[1] as f64, self.stock[2] as f64, self.stock[3] as f64,
            self.last_flood as f64, self.used_bought as f64, self.miners_sold as f64,
            // build .14: power contracts
            self.contract as f64, self.contract_from as f64, self.switches as f64, self.spot_walk, self.spot_cap,
            self.spike_blocks as f64, self.spike_usd, self.dr_mwh, self.dr_earned, self.c_mwh, self.c_cost, self.cap_blocks as f64,
            // build .15: heat reuse
            self.heat_earned,
            // build .16: pools
            self.pool as f64, self.pool_switches as f64, self.pplns_paid, self.pplns_exp, self.pool_blocks as f64,
            // v100: crew and spares
            self.techs as f64, self.guards as f64, self.spares[0] as f64, self.spares[1] as f64, self.spares[2] as f64,
            self.spare_target[0] as f64, self.spare_target[1] as f64, self.spare_target[2] as f64,
            self.fails[0] as f64, self.fails[1] as f64, self.fails[2] as f64, self.swapped[0] as f64, self.swapped[1] as f64, self.swapped[2] as f64,
            self.wages_paid,
            // v100+: site
            self.site as f64, self.well_f, self.tariffs as f64, self.follower as u8 as f64, self.day_off as f64,
            self.unlocked_x[0] as u8 as f64, self.unlocked_x[1] as u8 as f64];
        // v9+: version, then how many scalars follow, so later builds can append fields without breaking old saves
        f(scal[0]); f((scal.len() - 1) as f64);
        for v in scal.iter().skip(1) { f(*v); }
        for u in self.unlocked.iter() { f(*u as u8 as f64); }
        for t in self.tiles.iter() { out.push(t.ground); out.push(t.tree); }
        out.extend_from_slice(&(self.blds.len() as u32).to_le_bytes());
        for b in self.blds.iter() {
            out.push(b.kind); out.push(b.i); out.push(b.j); out.push(b.level); out.push(b.broken);
            out.push(b.dir % 4 | ((b.tune as u8) & 7) << 2); out.push(b.health as u8); out.push(b.fire); out.push(b.off); out.push(b.dust as u8);
            out.extend_from_slice(&b.invested.to_le_bytes());
            out.extend_from_slice(&b.dead); // v100+
        }
    }

    pub fn load(data: &[u8]) -> Option<Game> {
        let mut p = 0usize;
        let mut rd = |n: usize| -> Option<&[u8]> { if p + n > data.len() { None } else { let s = &data[p..p + n]; p += n; Some(s) } };
        let rdf = |b: &[u8]| { let mut a = [0u8; 8]; a.copy_from_slice(b); f64::from_le_bytes(a) };
        let ver = rdf(rd(8)?);
        // v8 had 46 scalars after the version; v9+ stores the count. Missing fields keep their defaults.
        // Rule: versions 9-99 may only APPEND scalars. Changing the tile or building records needs v100+.
        // v100 added three bytes per building (failed parts). Versions 100-199 may only append scalars again.
        let n = if ver == 8.0 { 46 } else if ver >= 9.0 && ver < 200.0 { let c = rdf(rd(8)?); if !(1.0..=4096.0).contains(&c) { return None; } c as usize } else { return None };
        let rec = if ver >= 100.0 { 21 } else { 18 };
        if n < 46 { return None; }
        let mut fs = [f64::NAN; 128];
        fs[0] = ver;
        for x in 1..=n { let v = rdf(rd(8)?); if x < fs.len() { fs[x] = v; } }
        let has = |x: usize| x < fs.len() && !fs[x].is_nan();
        let m = Market { price: fs[9], net_th: fs[10], height: fs[36] as u64, remaining: 1, est_change: 0.0, fees: fs[40], live: fs[35] != 0.0 };
        let mut g = Game::new(fs[1] as u32, fs[2] as u32, m);
        g.t_river = fs[3] != 0.0; g.t_sun = fs[4]; g.t_wind = fs[5]; g.t_coal = fs[6] != 0.0;
        g.cash = fs[7]; g.btc = fs[8]; g.price = fs[9]; g.net_th = fs[10]; g.reward = fs[11]; g.fees = fs[12];
        g.block = fs[13] as u64; g.day = fs[14] as u64; g.halvings = fs[15] as u32; g.last_retarget = fs[16]; g.diff_swing = fs[17];
        g.gas_f = fs[18]; g.diesel_f = fs[19]; g.wind_now = fs[20]; g.streak = fs[21] as u64; g.unpaid_days = fs[22] as u32;
        g.bailouts_used = fs[23] as u32; g.last_bailout_day = fs[24] as u64; g.heat_days = fs[25] as u32; g.heat_add = fs[26];
        g.gas_days = fs[27] as u32; g.brown_days = fs[28] as u32; g.drought_days = fs[29] as u32; g.storm_days = fs[30] as u32;
        g.throttle_days = fs[31] as u32; g.fuel_days = fs[32] as u32; g.core_cooldown = fs[33] as u64; g.over = fs[34] as u8;
        g.live = fs[35] != 0.0; g.start_block = fs[36] as u64; g.next_retarget = fs[37] as u64; g.est_change = fs[38];
        g.retargets = fs[39] as u32; g.base_fees = fs[40]; g.blocks_found = fs[41] as u32; g.raids = fs[42] as u32; g.weather = fs[43] as u8; g.weather_days = fs[44] as u32; g.solo = fs[45] != 0.0; g.last_found = fs[46] as u64;
        if has(57) {
            g.fee_mult = fs[47]; g.fee_days = fs[48] as u32; g.fee_spikes = fs[49] as u32; g.curtail_blocks = fs[50] as u32; g.curtail_mode = fs[51] as u8;
            g.curtail_pay = fs[52]; g.curtail_extra = fs[53]; g.curtails = fs[54] as u32; g.curtail_earned = fs[55]; g.grid_kw = fs[56]; g.anchor_px = fs[57];
        } else { g.anchor_px = g.price; }
        g.hp_eq = if has(58) && fs[58] > 0.0 { fs[58] } else { g.hashprice() };
        if has(63) { g.grid_usd = fs[59]; g.scen_id = fs[60] as u32; g.max_miner = fs[61] as u8; g.scen_worth0 = fs[62]; g.scen_hold_btc = fs[63]; }
        if has(64) && fs[64] >= 1.0 { g.spot_x = fs[64]; }
        g.mkt_idx = if has(65) && fs[65] > 0.0 { fs[65] } else { g.mkt_raw() };
        if has(84) {
            g.contract = (fs[73] as u8).min(C_DR); g.contract_from = fs[74] as u64; g.switches = fs[75] as u32; g.spot_walk = clamp(fs[76], 0.5, 2.0);
            g.spot_cap = fs[77]; g.spike_blocks = fs[78] as u32; g.spike_usd = fs[79]; g.dr_mwh = fs[80]; g.dr_earned = fs[81];
            g.c_mwh = fs[82]; g.c_cost = fs[83]; g.cap_blocks = fs[84] as u32;
        }
        if has(85) { g.heat_earned = fs[85]; }
        if has(112) { g.unlocked_x = [fs[111] != 0.0, fs[112] != 0.0]; }
        if has(110) { g.follower = fs[109] != 0.0; g.day_off = fs[110] as u64; }
        if has(108) { g.site = (fs[106] as usize).min(N_SITES - 1) as u8; g.well_f = clamp(fs[107], 0.25, 1.0); g.tariffs = fs[108] as u32; }
        if has(105) {
            g.techs = (fs[91] as u32).min(MAX_CREW); g.guards = (fs[92] as u32).min(MAX_CREW);
            for p in 0..3 { g.spares[p] = fs[93 + p] as u32; g.spare_target[p] = fs[96 + p] as u32; g.fails[p] = fs[99 + p] as u32; g.swapped[p] = fs[102 + p] as u32; }
            g.wages_paid = fs[105];
        }
        if has(90) { g.pool = (fs[86] as u8).min(POOLS.len() as u8 - 1); g.pool_switches = fs[87] as u32; g.pplns_paid = fs[88]; g.pplns_exp = fs[89]; g.pool_blocks = fs[90] as u32; }
        if has(72) { for m in 0..4 { g.stock[m] = fs[66 + m] as u32; } g.last_flood = fs[70] as u64; g.used_bought = fs[71] as u32; g.miners_sold = fs[72] as u32; }
        if g.scen_id > 0 { g.next_retarget = u64::MAX; } // the scenario's rows come back from JS (scen_buf + scen_resume)
        for x in 0..N_TIERS { g.unlocked[x] = rdf(rd(8)?) != 0.0; }
        for x in 0..MAP * MAP { let b = rd(2)?; g.tiles[x] = Tile { ground: b[0], tree: b[1], occ: -1 }; }
        let nb = { let b = rd(4)?; u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize };
        g.blds.clear();
        for _ in 0..nb {
            let b = rd(rec)?;
            if b[0] as usize >= N_KINDS { return None; }
            g.blds.push(Bld { kind: b[0], i: b[1], j: b[2], level: b[3], broken: b[4], dir: b[5] % 4, tune: { let v = (b[5] >> 2 & 7) as i8; if v >= 4 { v - 8 } else { v } }, health: b[6] as f32, fire: b[7], off: b[8],
                              temp: 40.0, invested: rdf(&b[10..18]), net: 0, exposed: false, dust: b[9] as f32,
                              dead: if rec >= 21 { [b[18].min(30), b[19].min(10), b[20].min(40)] } else { [0; 3] } });
        }
        for x in 0..g.blds.len() {
            let b = g.blds[x]; let k = KINDS[b.kind as usize];
            for bb in 0..k.d as i32 { for a in 0..k.w as i32 { g.tile_mut(b.i as i32 + a, b.j as i32 + bb).occ = x as i16; } }
        }
        g.rebuild_nets(); g.sync_workers();
        g.dispatch(false); g.update_temps(); g.dispatch(false); // temperatures and heat links, not just the defaults
        Some(g)
    }
}
