//! The long game. A bot that plays from the first box towards the free energy core and reports the day
//! each node of the power tree opened, so the whole tree can be balanced and not only its first nodes.
//! PG_BRANCH=fossil or =renewable keeps the bot to one route (grid, fission, fusion and the core are shared).
//!
//! The bot is a sensible, unhurried player: it grows while a new box pays for itself, keeps a cash reserve,
//! switches off boxes that cost more in power than they mine, adds the cheapest power it can use, and buys
//! the next rung when that costs under about six weeks of income or when there is nothing better to buy.
use crate::data::*;
use crate::game::*;

const C: i32 = 24;
/// Boxes stay inside this ring; the outer tiles are kept for power plants.
const BOX_RING: i32 = 19;
const PLANT_RING: i32 = 23;
/// A new box must earn back its price, with a margin, inside this many days.
const HORIZON: f64 = 300.0;

pub struct Full { pub rung: [Option<u64>; 12], pub won: Option<u64>, pub bankrupt: Option<u64>, pub end_day: u64, pub worth: f64, pub miners: usize, pub blocked: Option<u8> }

fn hot_neighbours(g: &Game, i: i32, j: i32) -> u32 {
    let mut n = 0;
    for dj in -1..=1 { for di in -1..=1 { if di == 0 && dj == 0 { continue; }
        if let Some(t) = g.tile(i + di, j + dj) { if t.occ >= 0 && is_miner(g.blds[t.occ as usize].kind) { n += 1; } } } }
    n
}

fn find_spot(g: &Game, kind: u8) -> Option<(i32, i32)> {
    let k = KINDS[kind as usize];
    let ring = if is_power(kind) { PLANT_RING } else { BOX_RING };
    for r in 1..=ring {
        for j in C - r..=C + r { for i in C - r..=C + r {
            if (i - C).abs() != r && (j - C).abs() != r { continue; }
            if g.can_place(kind, i, j) != 0.0 { continue; }
            if is_miner(kind) && hot_neighbours(g, i, j) > 2 { continue; }
            if !is_power(kind) && !g.would_connect(kind, i, j) { continue; }
            if is_power(kind) {
                let mut touch = false;
                for b in -1..=k.d as i32 { for a in -1..=k.w as i32 { if let Some(t) = g.tile(i + a, j + b) { if t.occ >= 0 && KINDS[g.blds[t.occ as usize].kind as usize].electric { touch = true; } } } }
                if !touch { continue; }
            }
            return Some((i, j));
        } }
    }
    // a dam goes where the water is, even away from the plant (a player would run poles to it)
    if kind == K_HYDRO { for j in 0..MAP as i32 { for i in 0..MAP as i32 { if g.can_place(kind, i, j) == 0.0 { return Some((i, j)); } } } }
    None
}

/// Power the bot plans around: sources that run all day. Diesel is a backup, not a plan.
fn firm(kind: u8) -> bool { is_power(kind) && kind != K_DIESEL && kind != K_SOLAR && kind != K_WIND && kind != K_CORE }
fn usd(g: &Game, kind: u8) -> f64 {
    match kind { K_GRID => g.usd_mwh(K_GRID), K_GAS => KINDS[kind as usize].usd_mwh * g.gas_f, K_COAL => KINDS[kind as usize].usd_mwh + if g.t_coal { 0.0 } else { 8.0 }, _ => KINDS[kind as usize].usd_mwh }
}
/// (price, kW available today) per firm source, cheapest first.
fn supply(g: &Game) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = g.blds.iter().filter(|b| firm(b.kind)).map(|b| {
        let cap = KINDS[b.kind as usize].cap_kw[(b.level - 1) as usize] * match b.kind {
            K_GRID => if g.brown_days > 0 { 0.0 } else { g.grid_f() }, K_HYDRO => if g.drought_days > 0 { 0.4 } else { 1.0 }, _ => 1.0 };
        (usd(g, b.kind), cap * 0.95)
    }).collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    v
}
fn running_kw(g: &Game) -> f64 { g.blds.iter().filter(|b| is_consumer(b.kind) && b.broken == 0 && b.off == 0).map(|b| KINDS[b.kind as usize].kw).sum() }

fn profit(g: &Game, kind: u8, usd_mwh: f64, reward_f: f64) -> f64 {
    let k = KINDS[kind as usize];
    k.th / g.net_th * (g.reward * reward_f + g.fees) * (1.0 - POOL_FEE) * 144.0 * g.price - k.kw * 24.0 / 1000.0 * usd_mwh
}
/// What a box earns over the horizon at this power price, allowing for a halving on the way.
fn value(g: &Game, kind: u8, usd_mwh: f64) -> f64 {
    let to_halving = ((g.block / HALVING + 1) * HALVING - g.block) as f64 / 144.0;
    let before = to_halving.min(HORIZON);
    profit(g, kind, usd_mwh, 1.0) * before + profit(g, kind, usd_mwh, 0.6).max(0.0) * (HORIZON - before)
}

fn fossil(k: u8) -> bool { k == K_GAS || k == K_COAL }
fn renewable(k: u8) -> bool { k == K_SOLAR || k == K_WIND || k == K_HYDRO || k == K_DEEPGEO || k == K_ORBITAL }
/// Nodes the bot may climb through: everything, or one route when PG_BRANCH says so.
fn allowed(k: u8) -> bool {
    match std::env::var("PG_BRANCH").as_deref() { Ok("fossil") => !renewable(k), Ok("renewable") => !fossil(k) && k != K_FISSION && k != K_FUSION, _ => true }
}
/// The next step up the tree: (cost, kind, building to upgrade or None to build one). Without PG_BRANCH it is the
/// game's own pick (the cheapest step that opens something new).
fn next_step(g: &Game, no_room: &[u8]) -> Option<(f64, u8, Option<usize>)> {
    let top = |k: u8| (0..g.blds.len()).filter(|&x| g.blds[x].kind == k).max_by_key(|&x| g.blds[x].level);
    if std::env::var("PG_BRANCH").is_err() && no_room.is_empty() {
        let (c, k, up) = g.next_rung()?;
        return Some((c, k, if up { top(k) } else { None }));
    }
    let mut best: Option<(f64, u8, Option<usize>)> = None;
    for &(node, _) in TREE.iter() {
        if node == K_CORE || node == K_DIESEL || !g.tier_unlocked(node) || !allowed(node) || (no_room.contains(&node) && top(node).is_none()) { continue; }
        if node == K_HYDRO && !g.t_river { continue; }
        if !children(node).any(|c| !g.tier_unlocked(c) && allowed(c)) { continue; }
        let st = match top(node) { None => Some((KINDS[node as usize].cost, node, None)), Some(x) => { let c = g.upgrade_cost(x); if c > 0.0 { Some((c, node, Some(x))) } else { None } } };
        if let Some(st) = st { if best.map_or(true, |b| st.0 < b.0) { best = Some(st); } }
    }
    if best.is_none() && g.tier_unlocked(K_CORE) && !g.blds.iter().any(|b| b.kind == K_CORE) { return Some((KINDS[K_CORE as usize].cost, K_CORE, None)); }
    best
}

/// The cheapest way to add firm power, counting a year or so of running it: (capex, kind, upgrade or build, $/MWh, kW added).
fn cheapest_power(g: &Game) -> Option<(f64, u8, Option<usize>, f64, f64)> {
    let mut best: Option<(f64, u8, Option<usize>, f64, f64)> = None; let mut bs = f64::MAX;
    let mut consider = |capex: f64, kw: f64, kind: u8, at: Option<usize>, best: &mut Option<(f64, u8, Option<usize>, f64, f64)>| {
        let p = usd(g, kind);
        let s = capex / kw + 400.0 * 0.024 * p;
        if s < bs { bs = s; *best = Some((capex, kind, at, p, kw)); }
    };
    for kind in 0..N_KINDS as u8 {
        if !firm(kind) || !g.tier_unlocked(kind) || !allowed(kind) { continue; }
        if kind == K_HYDRO && !g.t_river { continue; }
        let k = KINDS[kind as usize];
        if !(kind == K_GRID && g.blds.iter().filter(|b| b.kind == K_GRID).count() >= GRID_MAX) { consider(k.cost, k.cap_kw[0], kind, None, &mut best); }
        for (x, b) in g.blds.iter().enumerate() {
            if b.kind != kind || b.level >= 3 { continue; }
            let c = g.upgrade_cost(x);
            if c > 0.0 { consider(c, k.cap_kw[b.level as usize] - k.cap_kw[(b.level - 1) as usize], kind, Some(x), &mut best); }
        }
    }
    best
}

/// What the bot would buy next to grow: a box kind, how many times over it earns its price back inside the
/// horizon, and the power it has to add first (None when there is spare power to run it on).
struct Grow { kind: u8, ratio: f64, add: Option<(f64, u8, Option<usize>, f64, f64)> }
fn best_growth(g: &Game, spare_usd: Option<f64>) -> Option<Grow> {
    let more = cheapest_power(g);
    let add = if spare_usd.is_none() { more } else { None };
    let p_usd = match (spare_usd, add) { (Some(p), _) => p, (None, Some(a)) => a.3, _ => return None };
    let kw_usd = more.map_or(0.0, |a| a.0 / a.4); // a kW a box sits on is a kW that has to be bought again
    let mut best: Option<Grow> = None;
    for k in K_MINER1..=K_MINER4 {
        if !g.kind_unlocked(k) { continue; }
        let ratio = value(g, k, p_usd) / (g.new_price(k) + KINDS[k as usize].kw * 1.08 * kw_usd);
        if profit(g, k, p_usd * 1.3, 1.0) <= 0.0 { continue; } // too thin: one bad week and it runs at a loss
        if ratio > 1.15 && best.as_ref().map_or(true, |b| ratio > b.ratio) { best = Some(Grow { kind: k, ratio, add }); }
    }
    best
}
/// Price of the power the next box would run on, if any is spare.
fn spare_power(g: &Game) -> Option<f64> {
    let mut sup = supply(g); let mut need = running_kw(g) * 1.08;
    for s in sup.iter_mut() { let t = s.1.min(need); s.1 -= t; need -= t; }
    sup.iter().find(|x| x.1 >= 62.0).map(|x| x.0)
}

pub fn run(seed: u32, diff: u32, site: u8, max_days: u64, verbose: bool, live: bool) -> Full {
    let mut m = Market::fallback(); m.live = live;
    let mut g = Game::new_at(seed, diff, m, site);
    let mut out = Full { rung: [None; 12], won: None, bankrupt: None, end_day: 0, worth: 0.0, miners: 0, blocked: None };
    out.rung[0] = Some(0);
    let mut boxes_full = false;
    // nodes the bot found no room for: it climbs another branch instead (a player would too)
    let mut no_room: Vec<u8> = Vec::new();
    // money put aside for the next rung: half of what comes in while the plant is still growing well, all of it otherwise
    let mut fund = 0.0f64; let mut cash_last = g.cash;
    // what a day really made (bitcoin mined less every bill), and the safety margin a box must clear to stay on:
    // it widens after losing days, whatever caused them (still air, drought, a fuel spike), and relaxes slowly
    let mut op = 0.0f64; let mut margin = 1.1f64;
    let dbg: Option<u64> = std::env::var("PG_DBG").ok().and_then(|v| v.parse().ok());
    while g.day < max_days && g.over == 0 {
        if g.pending != EV_NONE {
            let c = match g.pending { EV_HEATWAVE => if g.cash > g.ev_arg * 3.0 && g.my_th > 0.0 { 0 } else { 1 }, _ => 0 };
            g.choose(c);
            if g.over == 2 { out.won = Some(g.day); break; }
        }
        let d0 = g.day; let (c0, b0) = (g.cash, g.btc);
        g.step_block(); g.toasts.clear(); g.fx.clear();
        op += g.cash - c0 + (g.btc - b0) * g.price;
        if g.day == d0 { continue; }
        margin = if op < 0.0 { (margin + 0.15).min(3.0) } else { (margin - 0.01).max(1.1) }; op = 0.0;

        // ---- once a day
        for (w, k) in LADDER.iter().enumerate() { if out.rung[w].is_none() && g.tier_unlocked(*k) { out.rung[w] = Some(g.day);
            if verbose { println!("  day {:>5}: rung {:>2} open | cash {:>13.0} net/day {:>10.0} boxes {:>4} {:>8.1} PH", g.day, w + 1, g.cash, g.rev_day - g.cost_day, g.blds.iter().filter(|b| is_miner(b.kind)).count(), g.my_th / 1000.0); } } }
        // bitcoin pays the bills; near the end keep the 21 the core asks for
        let keep = if g.tier_unlocked(K_CORE) { 21.0 } else { 0.0 };
        if g.btc > keep { let f = (g.btc - keep) / g.btc; g.sell_btc(f); }
        else if g.btc > 0.0 && g.cash < 20_000.0 + 3.0 * g.cost_day { g.sell_btc(((25_000.0 + 3.0 * g.cost_day) / (g.btc * g.price)).min(1.0)); } // the bills come first

        // each box on the cheapest power left, most efficient first; off if that power costs more than it mines
        let mut sup = supply(&g); let mut s = 0usize;
        let mut order: Vec<usize> = (0..g.blds.len()).filter(|&x| is_miner(g.blds[x].kind) && g.blds[x].broken == 0).collect();
        order.sort_by(|&a, &b| { let (ka, kb) = (KINDS[g.blds[a].kind as usize], KINDS[g.blds[b].kind as usize]); (ka.kw / ka.th).partial_cmp(&(kb.kw / kb.th)).unwrap() });
        for x in order {
            let kind = g.blds[x].kind; let kw = KINDS[kind as usize].kw * 1.08; // fans, tanks and line losses ride along
            while s < sup.len() && sup[s].1 < kw { s += 1; }
            let on = s < sup.len() && profit(&g, kind, sup[s].0.max(8.0) * margin, 1.0) > 0.0; // a margin for wear, parts and wages
            if on { sup[s].1 -= kw; }
            if on != (g.blds[x].off == 0) { g.toggle(x); }
        }
        // a fan wall with nothing running in front of it is switched off
        let mut lane = [(0i32, 0i32, 0f32); 12];
        for x in 0..g.blds.len() { let f = g.blds[x]; if f.kind != K_FAN { continue; }
            let n = g.fan_lane(f.i as i32, f.j as i32, f.dir, &mut lane);
            let need = lane.iter().take(n).any(|&(i, j, _)| g.tile(i, j).map_or(false, |t| t.occ >= 0 && { let b = g.blds[t.occ as usize]; is_miner(b.kind) && b.off == 0 && b.broken == 0 }));
            if need != (f.off == 0) { g.toggle(x); } }

        // crew for the boxes that are running: nobody is paid to watch dark machines
        let miners = g.blds.iter().filter(|b| is_miner(b.kind) && b.off == 0 && b.broken == 0).count() as u32;
        let techs = if miners >= 40 { (1 + miners / 120).min(MAX_CREW) } else { 0 };
        let guards = if miners >= 50 { 3 } else if miners >= 25 { 2 } else if miners >= 6 { 1 } else { 0 };
        let (techs, guards) = if g.cash < 2_000.0 { (0, 0) } else { (techs, guards) };
        if techs != g.techs || guards != g.guards { g.set_crew(techs, guards); }
        if techs > 0 { g.set_spare_target(P_BOARD, 2 + miners / 10); g.set_spare_target(P_PSU, 2 + miners / 10); g.set_spare_target(P_FAN, 4 + miners / 5); }
        let to_halving = ((g.block / HALVING + 1) * HALVING - g.block) / 144;
        let plant: f64 = g.blds.iter().map(|b| b.invested).sum();
        let reserve = (3_000.0 + plant * if to_halving < 60 { 0.12 } else { 0.04 }).max(5.0 * g.cost_day) + 30.0 * (techs as f64 * TECH_WAGE + guards as f64 * GUARD_WAGE);
        // four "no trespassing" signs are the cheapest security there is
        if g.cash > 2_000.0 { let signs = g.blds.iter().filter(|b| b.kind == K_SIGN).count() as i32;
            for n in signs..4 { let (i, j) = [(1, 1), (46, 1), (1, 46), (46, 46)][n as usize]; for d in 0..6 { let jj = j + d * if j > 24 { -1 } else { 1 }; if g.can_place(K_SIGN, i, jj) == 0.0 { g.place(K_SIGN, i, jj, 0); break; } } } }
        // upkeep comes before everything else: air for hot boxes, repairs, parts, dust
        for x in 0..g.blds.len() {
            let b = g.blds[x];
            if !is_miner(b.kind) || b.off != 0 || b.temp < 73.0 || g.cooling_at(&b) > 4.0 || g.cash < 1_500.0 { continue; }
            for (d, (di, dj)) in DIRS.iter().enumerate() {
                let (fi, fj) = (b.i as i32 - di, b.j as i32 - dj);
                if g.can_place(K_FAN, fi, fj) == 0.0 && g.would_connect(K_FAN, fi, fj) { g.place(K_FAN, fi, fj, d as u8); break; }
            }
        }
        for x in 0..g.blds.len() { let b = g.blds[x]; if is_miner(b.kind) && b.off == 0 && (b.broken != 0 || b.health < 50.0) && g.cash > g.repair_cost(x) + 1_000.0 { g.repair(x); } }
        if techs == 0 {
            for x in 0..g.blds.len() { if is_miner(g.blds[x].kind) && g.blds[x].off == 0 && g.blds[x].parts_out() >= 4 && g.cash > g.fix_cost(x) + 1_000.0 { g.fix_parts(x); } }
            if g.blds.iter().any(|b| b.dust >= 25.0 && b.off == 0) && g.cash > 800.0 { g.clean_all(); }
        }
        g.toasts.clear();

        // the core itself
        if g.tier_unlocked(K_CORE) && !g.blds.iter().any(|b| b.kind == K_CORE) && g.core_reqs().iter().all(|x| *x) && g.cash >= KINDS[K_CORE as usize].cost + reserve * 0.25 && g.block >= g.core_cooldown {
            if let Some((i, j)) = find_spot(&g, K_CORE) { g.place(K_CORE, i, j, 0); cash_last = g.cash; continue; }
        }

        let net = g.rev_day - g.cost_day;
        let spare = spare_power(&g);
        let grow = if boxes_full { None } else { best_growth(&g, spare) };
        // a box that pays for itself twice over in the horizon is worth splitting the money with; otherwise the rung gets it all
        let mut strong = grow.as_ref().map_or(false, |x| x.ratio >= 2.0);
        // on a full map, trading old boxes for the best one is the growth that competes with the ladder
        if boxes_full { if let Some(p_usd) = spare.or_else(|| supply(&g).first().map(|x| x.0)) {
            let top = (K_MINER1..=K_MINER4).filter(|k| g.kind_unlocked(*k)).max_by(|a, b| value(&g, *a, p_usd).partial_cmp(&value(&g, *b, p_usd)).unwrap()).unwrap_or(K_MINER1);
            if let Some(x) = g.blds.iter().position(|b| is_miner(b.kind) && b.kind < top) {
                let gain = value(&g, top, p_usd) - value(&g, g.blds[x].kind, p_usd).max(0.0);
                if gain > 2.0 * (g.new_price(top) - g.sell_value(x)).max(1.0) { strong = true; }
            }
        } }
        let step = next_step(&g, &no_room);
        let surplus = (g.cash - reserve).max(0.0);
        fund += if strong { 0.5 } else { 1.0 } * (g.cash - cash_last).max(0.0);
        if !strong { fund = surplus; }
        fund = fund.min(surplus).min(step.map_or(0.0, |x| x.0));
        // a rung far out of reach doesn't freeze the plant: carry on growing meanwhile
        let hopeless = step.map_or(true, |x| x.0 > surplus + 400.0 * net.max(0.0));
        if hopeless { fund = 0.0; }
        if let Some((c, k, at)) = step { if fund >= c {
            match at { Some(x) => g.upgrade(x), None => match find_spot(&g, k) { Some((i, j)) => { g.place(k, i, j, 0); } None => { out.blocked = Some(k); if !no_room.contains(&k) { no_room.push(k); } } } }
            fund = 0.0;
        } }
        if let Some(f) = dbg { if verbose && g.day >= f && g.day < f + 40 {
            println!("    d{:>4} cash {:>8.0} reserve {:>7.0} fund {:>7.0} net {:>6.0} step {:?} grow {:?} spare {:?} boxes {} off {} th {:.1} hp {:.4} raids {}", g.day, g.cash, reserve, fund, net, step.map(|x| (x.0 as i64, x.1)),
                grow.as_ref().map(|x| (x.kind, (x.ratio * 100.0) as i64, x.add.is_some())), spare.map(|x| x as i64), g.blds.iter().filter(|b| is_miner(b.kind)).count(), g.off_count, g.my_th / 1000.0, g.hashprice(), g.raids);
        } }

        // growth with what is left: boxes, and power for them when there is none spare
        let floor = reserve + fund; let mut bought = 0;
        while !boxes_full && bought < 60 {
            let gr = match best_growth(&g, spare_power(&g)) { Some(x) => x, None => break };
            if !strong && !hopeless && gr.ratio < 2.0 { break; }
            if let Some((capex, kind, at, _, _)) = gr.add {
                if g.cash < capex + floor { break; }
                match at { Some(x) => g.upgrade(x), None => match find_spot(&g, kind) { Some((i, j)) => { g.place(kind, i, j, 0); } None => break } }
                continue;
            }
            if g.cash < g.new_price(gr.kind) + floor { break; }
            match find_spot(&g, gr.kind) { Some((i, j)) => { g.place(gr.kind, i, j, 0); bought += 1; } None => { boxes_full = true; break; } }
        }
        // an old box is traded in when the better one earns back the difference with room to spare; dark boxes go first
        {
            let p_usd = spare_power(&g).or_else(|| supply(&g).first().map(|x| x.0)).unwrap_or_else(|| g.usd_mwh(K_GRID));
            let mut best = None; let mut bs = 1.15;
            // with room to spare the best box is the one that earns most per dollar; on a full map, most per tile
            for k in K_MINER1..=K_MINER4 { if !g.kind_unlocked(k) { continue; } let v = value(&g, k, p_usd); if v < g.new_price(k) * 1.15 { continue; }
                let sc = if boxes_full { v } else { v / g.new_price(k) }; if sc > bs { bs = sc; best = Some(k); } }
            let mut n = 0;
            while let (Some(k), true) = (best, n < 30) {
                let mut cand: Option<usize> = None;
                for (x, b) in g.blds.iter().enumerate() {
                    if !is_miner(b.kind) || b.kind >= k { continue; }
                    let gain = value(&g, k, p_usd) - if b.off != 0 { 0.0 } else { value(&g, b.kind, p_usd).max(0.0) };
                    if gain > (g.new_price(k) - g.sell_value(x)) * 1.5 { cand = Some(x); if b.off != 0 { break; } }
                }
                let x = match cand { Some(x) => x, None => break };
                if g.cash < g.new_price(k) + floor { break; }
                let (i, j) = (g.blds[x].i as i32, g.blds[x].j as i32);
                g.sell(x); g.clear(i, j, true);
                if g.place(k, i, j, 0) != 0.0 { break; }
                n += 1;
            }
        }
        g.toasts.clear(); cash_last = g.cash;
        if verbose && g.day % 250 == 0 && g.tier_unlocked(K_CORE) { println!("      core: reqs {:?} btc {:.1} streak {} cooldown {} cash {:.0} reserve {:.0}", g.core_reqs(), g.btc, g.streak, g.core_cooldown > g.block, g.cash, reserve); }
        if verbose && g.day % 250 == 0 {
            println!("  day {:>5}: cash {:>13.0} net/day {:>10.0} boxes {:>4} (off {:>4}, hot {:>3}) {:>8.1} PH cap {:>8.0} kW btc ${:>7.0} hashprice {:.4} halvings {} raids {}", g.day, g.cash, g.rev_day - g.cost_day, g.blds.iter().filter(|b| is_miner(b.kind)).count(), g.off_count,
                g.blds.iter().filter(|b| is_miner(b.kind) && b.temp > 80.0).count(), g.my_th / 1000.0, g.cap_kw.min(9.9e7), g.price, g.hashprice(), g.halvings, g.raids);
        }
    }
    if g.over == 1 { out.bankrupt = Some(g.day); }
    if g.over == 2 && out.won.is_none() { out.won = Some(g.day); }
    for (w, k) in LADDER.iter().enumerate() { if out.rung[w].is_none() && g.tier_unlocked(*k) { out.rung[w] = Some(g.day); } }
    out.end_day = g.day; out.worth = g.worth(); out.miners = g.blds.iter().filter(|b| is_miner(b.kind)).count();
    out
}

const NAMES: [&str; 12] = ["grid", "diesel", "gas", "coal", "solar", "wind", "hydro", "deep geo", "orbital", "fission", "fusion", "core"];

/// Play `runs` seeds to `max_days` and print when each rung opened. Returns how many runs reached `rung`.
pub fn report(runs: u32, diff: u32, site: u8, max_days: u64, verbose: bool, live: bool) -> Vec<Full> {
    let first: u32 = std::env::var("PG_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let seeds: Vec<u32> = (first..first + runs).map(|s| 0xC0FFEE ^ s.wrapping_mul(2654435761u32)).collect();
    let res: Vec<Full> = if verbose { seeds.iter().map(|s| { println!("seed {s:08X}"); run(*s, diff, site, max_days, true, live) }).collect() } else {
        std::thread::scope(|sc| { let hs: Vec<_> = seeds.iter().map(|s| { let s = *s; sc.spawn(move || run(s, diff, site, max_days, false, live)) }).collect(); hs.into_iter().map(|h| h.join().unwrap()).collect() })
    };
    if std::env::var("PG_EACH").is_ok() { for (n, r) in res.iter().enumerate() { println!("  seed {:>2}: rungs {} won {:?} bankrupt {:?} boxes {} worth {:.0}", first as usize + n, r.rung.iter().filter(|x| x.is_some()).count(), r.won, r.bankrupt, r.miners, r.worth); } }
    println!("full tree, diff {diff}, site {site}, {} market, branch {}, {runs} runs of up to {max_days} days", if live { "live" } else { "sim" }, std::env::var("PG_BRANCH").unwrap_or("any".into()));
    println!("  {:<10} {:>8} {:>10} {:>14}", "node", "reached", "median day", "range");
    for w in 1..12 {
        let mut d: Vec<u64> = res.iter().filter_map(|r| r.rung[w]).collect(); d.sort();
        if d.is_empty() { println!("  {:<10} {:>5}/{:<2} {:>10} {:>14}", NAMES[w], 0, runs, "-", "-"); continue; }
        // the median over all runs: "never" counts as later than any day
        let med = if d.len() * 2 > runs as usize { format!("{}", d[runs as usize / 2]) } else { "never".into() };
        println!("  {:<10} {:>5}/{:<2} {:>10} {:>14}", NAMES[w], d.len(), runs, med, format!("{}-{}", d[0], d[d.len() - 1]));
    }
    let mut won: Vec<u64> = res.iter().filter_map(|r| r.won).collect(); won.sort();
    let bk = res.iter().filter(|r| r.bankrupt.is_some()).count();
    let blocked: Vec<u8> = res.iter().filter_map(|r| r.blocked).collect();
    { let mut b: Vec<u64> = res.iter().filter_map(|r| r.bankrupt).collect(); b.sort(); if !b.is_empty() { println!("  bankrupt on days {:?}", b); } }
    println!("  won {}/{}{} | bankrupt {} | median boxes at the end {}{}", won.len(), runs, if won.is_empty() { String::new() } else { format!(" (median day {}, range {}-{})", won[won.len() / 2], won[0], won[won.len() - 1]) }, bk,
        { let mut m: Vec<usize> = res.iter().map(|r| r.miners).collect(); m.sort(); m[m.len() / 2] },
        if blocked.is_empty() { String::new() } else { format!(" | no room to build kind {:?}", blocked) });
    res
}

/// A map with no river must still be able to climb past hydro: maxing wind opens deep geothermal and fission,
/// and an old save that already has wind at level 3 is mended when it loads.
pub fn check_no_river() -> bool {
    let max = |g: &mut Game, k: u8| { g.unlock_tier(k); let (i, j) = find_spot(g, k).expect("room"); g.place(k, i, j, 0); let x = g.blds.len() - 1; g.upgrade(x); g.upgrade(x); };
    let mut seed = 1u32;
    let mut g = loop { let g = Game::new_at(seed, 0, Market::fallback(), 0); if !g.t_river { break g; } seed += 1; };
    g.cash = 1.0e9; max(&mut g, K_WIND);
    let open = g.tier_unlocked(K_FISSION) && g.tier_unlocked(K_DEEPGEO) && !g.tier_unlocked(K_ORBITAL);
    // the same plant as an older build would have saved it: wind maxed, the nodes after it still locked
    g.unlocked[K_FISSION as usize] = false; g.unlocked_x = [false; 2];
    let mut buf = Vec::new(); g.save(&mut buf);
    let mended = Game::load(&buf).map_or(false, |l| l.tier_unlocked(K_FISSION) && l.tier_unlocked(K_DEEPGEO) && l.tier_unlocked(K_DIESEL));
    // a river map: wind opens hydro and fission, not deep geothermal
    let mut seed = 1u32;
    let mut r = loop { let g = Game::new_at(seed, 0, Market::fallback(), 0); if g.t_river { break g; } seed += 1; };
    r.cash = 1.0e9; max(&mut r, K_WIND);
    let river_ok = r.tier_unlocked(K_HYDRO) && r.tier_unlocked(K_FISSION) && !r.tier_unlocked(K_DEEPGEO);
    // either top node can carry the core; coal alone opens fission but not wind's children
    let mut f = Game::new_at(1, 0, Market::fallback(), 0); f.cash = 1.0e9; max(&mut f, K_COAL);
    let fossil_ok = f.tier_unlocked(K_FISSION) && !f.tier_unlocked(K_HYDRO) && !f.tier_unlocked(K_DEEPGEO);
    let mut o = Game::new_at(1, 0, Market::fallback(), 0); o.cash = 1.0e9; max(&mut o, K_ORBITAL);
    let orbital_ok = o.tier_unlocked(K_CORE) && o.core_reqs()[1];
    // bills: a wallet with bitcoin in it is never repossessed for want of cash
    let mut b = Game::new_at(3, 2, Market::fallback(), 0); b.cash = -500.0; b.btc = 1.0;
    let d = b.day; while b.day == d { if b.pending != EV_NONE { b.choose(0); } b.step_block(); }
    let sold_ok = b.cash >= 0.0 && b.unpaid_days == 0 && b.btc < 1.0 && b.btc > 0.9;
    // the core: a failed roll keeps it standing, and it tries again with a bonus
    let mut core_ok = false;
    for seed in 1..60u32 {
        let mut c = Game::new_at(seed, 0, Market::fallback(), 0); c.cash = 1.0e9; max(&mut c, K_ORBITAL); c.unlock_tier(K_CORE);
        c.halvings = 1; c.btc = 30.0; c.streak = 5_000;
        let Some((i, j)) = find_spot(&c, K_CORE) else { continue };
        c.place(K_CORE, i, j, 0); c.choose(0);
        if c.over == 0 {
            let kept = c.blds.iter().any(|x| x.kind == K_CORE) && c.core_bonus() == CORE_TRY_BONUS as i32;
            let until = c.core_cooldown; while c.block < until + 2 * BLOCKS_PER_DAY && c.pending != EV_CORE_TRIAL { if c.pending != EV_NONE { c.choose(0); } c.streak = 5_000; c.step_block(); }
            core_ok = kept && c.pending == EV_CORE_TRIAL; break;
        }
    }
    println!("bills paid from bitcoin {sold_ok} | failed core stays and retries {core_ok}");
    println!("tree: no-river wind opens deep geo + fission {open} | old save mended {mended} | river map {river_ok} | coal opens fission {fossil_ok} | orbital opens core {orbital_ok}");
    open && mended && river_ok && fossil_ok && orbital_ok && sold_ok && core_ok
}
