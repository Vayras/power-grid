//! Balance harness. Includes the game modules straight from ../../src and plays them with a bot.
extern crate alloc;
#[path = "../../../src/data.rs"] #[allow(dead_code)] mod data;
#[path = "../../../src/game.rs"] #[allow(dead_code)] mod game;
#[path = "../../../src/math.rs"] #[allow(dead_code)] mod math;
#[path = "../../../src/rng.rs"] #[allow(dead_code)] mod rng;
#[path = "../../../src/sprites.rs"] #[allow(dead_code)] mod sprites;

use data::*;
use game::*;

/// Minutes of wall-clock play for `days` game days at a speed mix: early days at 1x (4 blocks/s),
/// the rest at 2x (30 blocks/s), plus 20 s of menus/building per game day in the first 30 days.
fn play_minutes(days: f64) -> f64 {
    let early = days.min(10.0); let rest = (days - early).max(0.0);
    (early * 144.0 / 4.0 + rest * 144.0 / 30.0 + days.min(30.0) * 20.0 + rest * 3.0) / 60.0
}

fn empty_neighbours(g: &Game, i: i32, j: i32) -> u32 {
    let mut n = 0;
    for dj in -1..=1 { for di in -1..=1 { if di == 0 && dj == 0 { continue; }
        if let Some(t) = g.tile(i + di, j + dj) { if t.occ >= 0 && is_miner(g.blds[t.occ as usize].kind) { n += 1; } } } }
    n
}

/// Spiral out from the plant and find a powered spot with few hot neighbours.
fn find_spot(g: &Game, kind: u8) -> Option<(i32, i32)> {
    let k = KINDS[kind as usize];
    for r in 1..RING {
        for j in 24 - r..=24 + r { for i in 24 - r..=24 + r {
            if (i - 24).abs() != r && (j - 24).abs() != r { continue; }
            if g.can_place(kind, i, j) != 0.0 { continue; }
            if is_miner(kind) && empty_neighbours(g, i, j) > 2 { continue; }
            if (i - 24).abs() >= RING - 0 || (j - 24).abs() >= RING - 0 { continue; }
            if !is_power(kind) && !g.would_connect(kind, i, j) { continue; }
            if is_power(kind) {
                // power: must touch the existing cluster so it joins the same net
                let mut touch = false;
                for b in -1..=k.d as i32 { for a in -1..=k.w as i32 { if let Some(t) = g.tile(i + a, j + b) { if t.occ >= 0 && KINDS[g.blds[t.occ as usize].kind as usize].electric { touch = true; } } } }
                if !touch { continue; }
            }
            return Some((i, j));
        } }
    }
    None
}

const RING: i32 = 5;
/// Lay a square wall ring RING tiles out from the plant centre, as far as cash goes.
fn fence(g: &mut Game) {
    for d in -RING..=RING { for &(i, j) in [(24 + d, 24 - RING), (24 + d, 24 + RING), (24 - RING, 24 + d), (24 + RING, 24 + d)].iter() {
        if let Some(t) = g.tile(i, j) { if t.occ >= 0 { continue; } }
        if g.cash < 400.0 { return; }
        if g.can_place(K_WALL, i, j) == 0.0 { g.place(K_WALL, i, j, 0); }
    } }
}

fn miner_profit_day(g: &Game, kind: u8, usd_mwh: f64) -> f64 {
    let k = KINDS[kind as usize];
    let rev = k.th / g.net_th * (g.reward + g.fees) * (1.0 - POOL_FEE) * 144.0 * g.price;
    rev - k.kw * 24.0 / 1000.0 * usd_mwh
}

struct Out { end_day: u64, gas_day: Option<u64>, diesel_day: Option<u64>, bankrupt: bool, worth: f64, miners: usize, curtail: u32, fee_spikes: u32 }

static mut LEDGER: [f64; 8] = [0.0; 8]; // 0 blocks (mining+power) 1 day tick/events 2 choices 3 sell btc 4 repairs/clean 5 miners+fans 6 power upgrades 7 walls
fn book(i: usize, d: f64) { unsafe { LEDGER[i] += d; } }
fn run(seed: u32, diff: u32, live: bool, max_days: u64, verbose: bool) -> Out {
    let mut m = Market::fallback(); m.live = live;
    let site = std::env::var("PG_SITE").ok().and_then(|v| v.parse::<u8>().ok()).unwrap_or(0);
    let mut g = Game::new_at(seed, diff, m, site);
    // PG_CONTRACT=1 spot (each box at its break-even, or PG_CAP=<$/MWh> for a flat cap), =2 fixed + demand response
    if let Some(c) = std::env::var("PG_CONTRACT").ok().and_then(|v| v.parse::<u8>().ok()) { g.set_contract(c); g.toasts.clear(); }
    // PG_POOL=0 big FPPS nearby, 1 budget FPPS overseas, 2 small PPLNS
    if let Some(p) = std::env::var("PG_POOL").ok().and_then(|v| v.parse::<u8>().ok()) { g.set_pool(p); g.toasts.clear(); }
    if let Some(cap) = std::env::var("PG_CAP").ok().and_then(|v| v.parse::<f64>().ok()) { g.set_spot_cap(cap); }
    let mut diesel_day = None; let mut gas_day = None;

    while g.day < max_days && g.over == 0 {
        // resolve pending choices like a cautious player
        if g.pending != EV_NONE {
            let c = match g.pending { EV_HEATWAVE => if g.cash > g.ev_arg * 3.0 { 0 } else { 1 }, EV_INSPECTOR => 0, _ => 0 };
            let c0 = g.cash; let p = g.pending; g.choose(c); book(2, g.cash - c0);
            if verbose && g.cash < c0 - 1_000.0 { println!("    choice {} on event {} cost {:.0}", c, p, c0 - g.cash); }
        }
        let d0 = g.day; let c0 = g.cash; let pend0 = g.pending;
        g.step_block();
        if g.day != d0 { book(1, g.cash - c0); } else { book(0, g.cash - c0); }
        if verbose && g.cash < c0 - 2_000.0 { println!("    big drop day {} block {}: {:.0} -> {:.0} pending {} -> {} toasts {:?}", g.day, g.block, c0, g.cash, pend0, g.pending, g.toasts); }
        g.toasts.clear();
        if g.day == d0 { continue; }
        // ---- once a day
        let c = g.cash; if g.btc > 0.0 { g.sell_btc(1.0); } book(3, g.cash - c); let c = g.cash;
        g.wake_all(); g.shed();
        // a sensible player switches off boxes that cost more in power than they mine (PG_NAIVE=1 keeps them on)
        if std::env::var("PG_NAIVE").is_err() {
            let gp = g.usd_mwh(K_GRID);
            for x in 0..g.blds.len() { let b = g.blds[x]; if is_miner(b.kind) && b.off == 0 && b.broken == 0 && miner_profit_day(&g, b.kind, gp) < 0.0 { g.toggle(x); } }
            // and doesn't run boxes on $240 diesel when the cheap supply (a declining well, a dry season, a brownout) falls short
            let cheap_now: f64 = g.blds.iter().filter(|b| b.kind == K_GRID || b.kind == K_GAS || (b.kind as usize) >= K_COAL as usize && is_power(b.kind)).map(|b| KINDS[b.kind as usize].cap_kw[(b.level - 1) as usize] * if b.kind == K_GRID { if g.brown_days > 0 { 0.0 } else { g.grid_f() } } else { 1.0 }).sum();
            loop {
                let on: f64 = g.blds.iter().filter(|b| is_consumer(b.kind) && b.off == 0 && b.broken == 0).map(|b| KINDS[b.kind as usize].kw * b.tune_kw()).sum();
                if on <= cheap_now * 0.98 { break; }
                let worst = (0..g.blds.len()).filter(|&x| is_miner(g.blds[x].kind) && g.blds[x].off == 0 && g.blds[x].broken == 0)
                    .max_by(|&a, &b| { let ka = KINDS[g.blds[a].kind as usize]; let kb = KINDS[g.blds[b].kind as usize]; (ka.kw / ka.th).partial_cmp(&(kb.kw / kb.th)).unwrap() });
                match worst { Some(x) => g.toggle(x), None => break }
            }
        }
        for x in 0..g.blds.len() { if is_miner(g.blds[x].kind) && (g.blds[x].broken != 0 || g.blds[x].health < 50.0) && g.cash > g.repair_cost(x) + 300.0 { g.repair(x); } }
        if g.blds.iter().any(|b| b.dust >= 25.0) && g.cash > 800.0 { g.clean_all(); }
        // parts: PG_PARTS=fix calls a contractor for boxes with 4+ failed parts; =crew hires technicians and keeps spares
        match std::env::var("PG_PARTS").as_deref() {
            Ok("fix") => { for x in 0..g.blds.len() { if is_miner(g.blds[x].kind) && g.blds[x].parts_out() >= 4 && g.cash > g.fix_cost(x) + 2_000.0 { g.fix_parts(x); } } }
            Ok("crew") => {
                let miners = g.blds.iter().filter(|b| is_miner(b.kind)).count() as u32;
                let techs = if miners >= 30 { 1 + miners / 150 } else { 0 };
                if techs != g.techs { let gd = g.guards; g.set_crew(techs, gd); }
                if techs > 0 { g.set_spare_target(P_BOARD, 2 + miners / 10); g.set_spare_target(P_PSU, 2 + miners / 10); g.set_spare_target(P_FAN, 4 + miners / 5); }
            }
            _ => {}
        }
        book(4, g.cash - c); let c = g.cash;
        if g.unlocked[1] && diesel_day.is_none() { diesel_day = Some(g.day); }
        if g.unlocked[2] && gas_day.is_none() { gas_day = Some(g.day); if verbose { println!("  gas on day {}", g.day); } break; }
        // 1) miners: fill cheap capacity with the best payback box
        let cheap: f64 = g.blds.iter().filter(|b| b.kind == K_GRID || b.kind == K_GAS).map(|b| KINDS[b.kind as usize].cap_kw[(b.level - 1) as usize] * if b.kind == K_GRID { g.grid_f() } else { 1.0 }).sum();
        let grid_price = g.usd_mwh(K_GRID);
        let plant: f64 = g.blds.iter().map(|b| b.invested).sum();
        let reserve = 2_000.0 + plant * 0.06;
        let mut bought = 0;
        loop {
            let installed: f64 = g.blds.iter().filter(|b| is_consumer(b.kind) && b.broken == 0).map(|b| KINDS[b.kind as usize].kw).sum();
            let spare = cheap * 0.92 - installed;
            let mut best = None; let mut bp = 0.0;
            for k in K_MINER1..=K_MINER4 {
                if !g.kind_unlocked(k) { continue; }
                let p = miner_profit_day(&g, k, grid_price);
                let kk = KINDS[k as usize];
                // power-limited: value profit per kW; capex-limited: profit per $
                let score = if spare < 200.0 { p / kk.kw } else { p / g.new_price(k) };
                if p > 0.0 && g.new_price(k) / p < 240.0 && kk.kw < spare && score > bp { bp = score; best = Some(k); }
            }
            let k = match best { Some(k) => k, None => break };
            // PG_USED=1: a bargain hunter who takes used machines whenever any are listed
            let used = std::env::var("PG_USED").is_ok() && g.stock[(k - K_MINER1) as usize] > 0;
            g.set_used(used);
            if g.cash < g.miner_price(k, used) + reserve || bought >= 6 { break; }
            match find_spot(&g, k) { Some((i, j)) => { g.place(k, i, j, 0); bought += 1; } None => break }
        }
        // PG_TUNE=1: put every box on the profile that nets the most at today's hashprice; no overclocking hot boxes
        if std::env::var("PG_TUNE").is_ok() {
            let hp = g.hashprice() * (1.0 - POOL_FEE);
            for x in 0..g.blds.len() {
                let b = g.blds[x]; if !is_miner(b.kind) { continue; }
                let k = KINDS[b.kind as usize]; let mut best = 0; let mut bv = f64::MIN;
                for t in -2i32..=2 {
                    let i = (t + 2) as usize; let fee = if t != 0 { 1.0 - FIRMWARE_FEE } else { 1.0 };
                    let v = k.th * TUNE_TH[i] * fee * hp - k.kw * TUNE_KW[i] * 24.0 / 1000.0 * grid_price;
                    if t > 0 && b.temp as f64 + TUNE_HEAT[i] - b.tune_heat() > 74.0 { continue; }
                    if v > bv { bv = v; best = t; }
                }
                g.set_tune(x, best);
            }
        }
        book(5, g.cash - c); let c = g.cash;
        // 2) power: upgrade the grid when it's nearly full
        let installed: f64 = g.blds.iter().filter(|b| is_consumer(b.kind) && b.broken == 0).map(|b| KINDS[b.kind as usize].kw).sum();
        let full = installed > cheap * 0.8;
        if let Some(x) = g.blds.iter().position(|b| b.kind == K_GRID) {
            let c = g.upgrade_cost(x);
            if c > 0.0 && (full || !g.unlocked[1]) && g.cash > c + reserve { g.upgrade(x); }
        }
        // 3) ladder: once the grid is maxed, push diesel to level 3 for gas
        if g.unlocked[1] && full {
            match g.blds.iter().position(|b| b.kind == K_DIESEL) {
                None => { if g.cash > KINDS[K_DIESEL as usize].cost + reserve { if let Some((i, j)) = find_spot(&g, K_DIESEL) { g.place(K_DIESEL, i, j, 0); } } }
                Some(x) => { let c = g.upgrade_cost(x); if c > 0.0 && g.cash > c + reserve { g.upgrade(x); } }
            }
        }
        book(6, g.cash - c); let c = g.cash;
        // 4) cooling: aim a fan at any box running hot without air
        for x in 0..g.blds.len() {
            if x >= g.blds.len() { break; }
            let b = g.blds[x];
            if !is_miner(b.kind) || b.temp < 76.0 || g.cooling_at(&b) > 4.0 || g.cash < 500.0 + reserve { continue; }
            for (d, (di, dj)) in DIRS.iter().enumerate() {
                let (fi, fj) = (b.i as i32 - di, b.j as i32 - dj);
                if g.can_place(K_FAN, fi, fj) == 0.0 && g.would_connect(K_FAN, fi, fj) { g.place(K_FAN, fi, fj, d as u8); break; }
            }
        }
        // PG_HEAT=1: once there are enough boxes, put a greenhouse where it reaches the most unpiped ones
        if std::env::var("PG_HEAT").is_ok() && g.cash > KINDS[K_GREENHOUSE as usize].cost + reserve {
            let unpiped: Vec<(i32, i32)> = (0..g.blds.len()).filter(|&x| is_miner(g.blds[x].kind) && g.piped.get(x).map_or(true, |p| *p < 0)).map(|x| (g.blds[x].i as i32, g.blds[x].j as i32)).collect();
            if unpiped.len() >= 5 {
                let mut best = None; let mut bn = 4;
                for j in 12..36 { for i in 12..36 {
                    if g.can_place(K_GREENHOUSE, i, j) != 0.0 { continue; }
                    let n = unpiped.iter().filter(|&&(mi, mj)| (mi - i).max(i + 1 - mi).max(0) <= HEAT_REACH + 0 && (mj - j).max(j + 1 - mj).max(0) <= HEAT_REACH && mi >= i - HEAT_REACH && mi <= i + 1 + HEAT_REACH && mj >= j - HEAT_REACH && mj <= j + 1 + HEAT_REACH).count();
                    if n > bn { bn = n; best = Some((i, j)); }
                } }
                if let Some((i, j)) = best { g.place(K_GREENHOUSE, i, j, 0); g.update_temps(); }
            }
        }
        let miners = g.blds.iter().filter(|b| is_miner(b.kind)).count();
        if miners >= 4 && g.exposed_miners > 0 { fence(&mut g); }
        book(7, g.cash - c);
        if verbose && g.day % 50 == 0 { unsafe { println!("  ledger day {}: blocks {:.0} daytick {:.0} choices {:.0} btc-sold {:.0} upkeep {:.0} miners {:.0} power {:.0} fans+walls {:.0}", g.day, LEDGER[0], LEDGER[1], LEDGER[2], LEDGER[3], LEDGER[4], LEDGER[5], LEDGER[6], LEDGER[7]); } }
        if verbose && (g.day % 10 == 0 || g.day < 8) {
            println!("  hp {:.4} eq {:.4} day {:>4} cash {:>10.0} th {:>8.0} cap {:>6.0} used {:>6.0} price {:>7.0} net {:>8.0}/day miners {} raids {} unpaid {} hp {:.0} hot {}", g.hashprice(), g.hp_eq, g.day, g.cash, g.my_th, g.cap_kw, g.used_kw, g.price, g.rev_day - g.cost_day, g.blds.iter().filter(|b| is_miner(b.kind)).count(), g.raids, g.unpaid_days, g.blds.iter().filter(|b| is_miner(b.kind)).map(|b| b.health as f64).sum::<f64>() / 1f64.max(g.blds.iter().filter(|b| is_miner(b.kind)).count() as f64), g.blds.iter().filter(|b| is_miner(b.kind) && b.temp > 80.0).count());
        }
        if verbose && false {
        }
    }
    if std::env::var("PG_PERF").is_ok() {
        let t = std::time::Instant::now(); let n = 3_000;
        for _ in 0..n { if g.pending != EV_NONE { g.choose(0); } g.step_block(); }
        println!("  perf: {} buildings, {:.1} µs per block (native)", g.blds.len(), t.elapsed().as_secs_f64() * 1e6 / n as f64);
    }
    let worth = g.cash + g.btc * g.price + g.blds.iter().map(|b| b.invested * 0.4).sum::<f64>();
    if verbose { println!("  parts failed {:?} swapped {:?} dead now {} lost {:.0} TH of {:.0} wages {:.0}", g.fails, g.swapped, g.blds.iter().map(|b| b.parts_out()).sum::<u32>(), g.parts_lost_th, g.my_th + g.parts_lost_th, g.wages_paid); }
    if verbose { println!("  pool {} stale {:.2}% rejected {:.2}% pplns luck {:.2} ({} pool blocks)", g.pool, g.stale_rate() * 100.0, if g.my_th > 0.0 { (1.0 - g.acc_th / g.my_th) * 100.0 } else { 0.0 }, if g.pplns_exp > 0.0 { g.pplns_paid / g.pplns_exp } else { 0.0 }, g.pool_blocks); }
    if verbose { println!("  greenhouses {} heat earned {:.0} heat now {:.0} kW", g.blds.iter().filter(|b| b.kind == K_GREENHOUSE).count(), g.heat_earned, g.heat_kw); }
    if verbose { println!("  contract {} avg paid {:.1}/MWh over {:.0} MWh, off-hours {:.0}, DR credits {:.0}, curtail credits {:.0}", g.contract, if g.c_mwh > 0.0 { g.c_cost / g.c_mwh } else { 0.0 }, g.c_mwh, g.cap_blocks as f64 / 6.0, g.dr_earned, g.curtail_earned); }
    Out { end_day: g.day, gas_day, diesel_day, bankrupt: g.over == 1, worth, miners: g.blds.iter().filter(|b| is_miner(b.kind)).count(), curtail: g.curtails, fee_spikes: g.fee_spikes }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let runs: u32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let diff: u32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let live = a.get(3).map_or(false, |s| s == "live");
    let verbose = a.get(4).map_or(false, |s| s == "v");
    let mut gas = Vec::new(); let mut dsl = Vec::new(); let mut bk = 0; let mut never = 0; let mut bkd: Vec<f64> = Vec::new();
    for s in 0..runs {
        let seed = 0xC0FFEE ^ (s * 2654435761u32);
        if verbose { println!("seed {s}"); }
        let o = run(seed, diff, live, 520, verbose);
        if o.bankrupt { bk += 1; bkd.push(o.end_day as f64); }
        match o.gas_day { Some(d) => gas.push(d as f64), None => never += 1 }
        if let Some(d) = o.diesel_day { dsl.push(d as f64); }
        if verbose { println!("  -> gas {:?} worth {:.0} miners {} curtails {} fee spikes {}", o.gas_day, o.worth, o.miners, o.curtail, o.fee_spikes); }
    }
    gas.sort_by(|a, b| a.partial_cmp(b).unwrap()); dsl.sort_by(|a, b| a.partial_cmp(b).unwrap());
    bkd.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("  bankrupt days: {:?}", bkd);
    let med = |v: &Vec<f64>| if v.is_empty() { f64::NAN } else { v[v.len() / 2] };
    println!("diff {diff} {} runs {runs}: diesel median day {:.0} | gas median day {:.0} (~{:.0} min play) range {:.0}-{:.0} | never {never} | bankrupt {bk}",
        if live { "live" } else { "sim" }, med(&dsl), med(&gas), play_minutes(med(&gas)), gas.first().copied().unwrap_or(f64::NAN), gas.last().copied().unwrap_or(f64::NAN));
    // CI guard: on Normal, most runs must reach gas before the first halving
    if diff == 0 && never * 2 > runs { eprintln!("BALANCE CHECK FAILED: {never}/{runs} Normal runs never reached gas"); std::process::exit(1); }
}
