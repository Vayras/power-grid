//! Static game data: building kinds, the power tree, miners and cooling.
//! Miner specs follow real Bitmain units (one container = 10 machines). Power-plant
//! capacities and costs are scaled to what fits on a 48x48 map.
use crate::sprites as S;

pub const K_GRID: u8 = 0;
pub const K_DIESEL: u8 = 1;
pub const K_GAS: u8 = 2;
pub const K_COAL: u8 = 3;
pub const K_SOLAR: u8 = 4;
pub const K_WIND: u8 = 5;
pub const K_HYDRO: u8 = 6;
pub const K_FISSION: u8 = 7;
pub const K_FUSION: u8 = 8;
pub const K_CORE: u8 = 9;
pub const K_MINER1: u8 = 10;
pub const K_MINER4: u8 = 13;
pub const K_FAN: u8 = 14;
pub const K_IMMERSION: u8 = 15;
pub const K_SHED: u8 = 16;
pub const K_POLE: u8 = 17;
pub const K_WALL: u8 = 18;
pub const K_SIGN: u8 = 19;
pub const K_GREENHOUSE: u8 = 20;
// The renewable branch's late nodes: real research today, built nowhere.
pub const K_DEEPGEO: u8 = 21;
pub const K_ORBITAL: u8 = 22;
pub const N_KINDS: usize = 23;
pub const N_TIERS: usize = 10;

#[derive(Clone, Copy)]
pub struct Kind {
    pub w: u8,
    pub d: u8,
    pub sprite: usize,
    pub cost: f64,
    pub up2: f64,
    pub up3: f64,
    pub cap_kw: [f64; 3],   // power sources: capacity per level
    pub usd_mwh: f64,       // power sources: fuel / running cost
    pub th: f64,            // miners: hashrate in TH/s
    pub kw: f64,            // consumers: draw in kW
    pub coal_pad: bool,
    pub pad: bool,          // lays a concrete pad
    pub reach: u8,          // electrical reach in tiles (0 = must touch)
    pub electric: bool,     // part of the power network at all
}

const fn pw(w: u8, d: u8, sprite: usize, cost: f64, up2: f64, up3: f64, cap: [f64; 3], usd: f64, coal: bool) -> Kind {
    Kind { w, d, sprite, cost, up2, up3, cap_kw: cap, usd_mwh: usd, th: 0.0, kw: 0.0, coal_pad: coal, pad: true, reach: 0, electric: true }
}
const fn mn(sprite: usize, cost: f64, th: f64, kw: f64) -> Kind {
    Kind { w: 1, d: 1, sprite, cost, up2: 0.0, up3: 0.0, cap_kw: [0.0; 3], usd_mwh: 0.0, th, kw, coal_pad: false, pad: true, reach: 0, electric: true }
}
const fn aux(sprite: usize, cost: f64, kw: f64) -> Kind {
    Kind { w: 1, d: 1, sprite, cost, up2: 0.0, up3: 0.0, cap_kw: [0.0; 3], usd_mwh: 0.0, th: 0.0, kw, coal_pad: false, pad: true, reach: 0, electric: true }
}

pub static KINDS: [Kind; N_KINDS] = [
    pw(1, 1, S::B_GRID, 12_000.0, 8_000.0, 12_000.0, [250.0, 600.0, 1_200.0], 45.0, false),
    pw(1, 1, S::B_DIESEL, 6_000.0, 6_000.0, 8_000.0, [500.0, 1_000.0, 2_000.0], 150.0, false),
    pw(2, 2, S::B_GAS, 10_000.0, 10_000.0, 15_000.0, [2_000.0, 5_000.0, 10_000.0], 32.0, false),
    pw(3, 2, S::B_COAL, 30_000.0, 35_000.0, 45_000.0, [10_000.0, 20_000.0, 40_000.0], 24.0, true),
    pw(2, 2, S::B_SOLAR, 12_000.0, 14_000.0, 18_000.0, [5_000.0, 12_000.0, 25_000.0], 0.0, false),
    pw(1, 1, S::B_WIND0, 35_000.0, 40_000.0, 50_000.0, [8_000.0, 16_000.0, 30_000.0], 0.0, false),
    pw(2, 2, S::B_HYDRO, 120_000.0, 160_000.0, 220_000.0, [20_000.0, 40_000.0, 80_000.0], 6.0, false),
    pw(2, 2, S::B_FISSION, 400_000.0, 550_000.0, 750_000.0, [50_000.0, 100_000.0, 150_000.0], 12.0, false),
    pw(2, 2, S::B_FUSION, 2_000_000.0, 2_600_000.0, 3_400_000.0, [200_000.0, 350_000.0, 500_000.0], 3.0, false),
    pw(2, 2, S::B_CORE, 10_000_000.0, 0.0, 0.0, [1.0e12, 1.0e12, 1.0e12], 0.0, false),
    // Game prices are about 1/6 of real (REAL_PRICE below) so a run fits in an evening.
    // Hashrate, power draw and everything Bitcoin pays are real.
    mn(S::B_MINER1, 600.0, 1_040.0, 30.7),        // 10x Antminer S19j Pro  (104 TH, 29.5 J/TH)
    mn(S::B_MINER1 + 1, 1_500.0, 1_410.0, 30.1),  // 10x Antminer S19 XP    (141 TH, 21.3 J/TH)
    mn(S::B_MINER1 + 2, 3_600.0, 2_000.0, 35.0),  // 10x Antminer S21       (200 TH, 17.5 J/TH)
    mn(S::B_MINER1 + 3, 10_000.0, 4_730.0, 56.8), // 10x Antminer S21 XP Hyd (473 TH, 12 J/TH)
    aux(S::B_FAN, 500.0, 4.0),
    aux(S::B_IMMERSION, 2_500.0, 6.0),
    Kind { w: 1, d: 1, sprite: S::B_SHED, cost: 0.0, up2: 0.0, up3: 0.0, cap_kw: [0.0; 3], usd_mwh: 0.0, th: 0.0, kw: 0.0, coal_pad: false, pad: true, reach: 0, electric: false },
    Kind { w: 1, d: 1, sprite: S::B_POLE, cost: 800.0, up2: 0.0, up3: 0.0, cap_kw: [0.0; 3], usd_mwh: 0.0, th: 0.0, kw: 0.0, coal_pad: false, pad: false, reach: 4, electric: true },
    // wall: Lv1 wooden palisade, Lv2 stone, Lv3 steel + razor wire. Raiders and air stop at it.
    Kind { w: 1, d: 1, sprite: S::B_WALL, cost: 300.0, up2: 900.0, up3: 2_500.0, cap_kw: [0.0; 3], usd_mwh: 0.0, th: 0.0, kw: 0.0, coal_pad: false, pad: false, reach: 0, electric: false },
    // "No trespassing" sign: each one (up to 4) makes raids less likely
    Kind { w: 1, d: 1, sprite: S::B_SIGN, cost: 250.0, up2: 0.0, up3: 0.0, cap_kw: [0.0; 3], usd_mwh: 0.0, th: 0.0, kw: 0.0, coal_pad: false, pad: false, reach: 0, electric: false },
    // greenhouse next door: buys the waste heat of miners within HEAT_REACH tiles
    Kind { w: 2, d: 2, sprite: S::B_GREENHOUSE, cost: 6_000.0, up2: 0.0, up3: 0.0, cap_kw: [0.0; 3], usd_mwh: 0.0, th: 0.0, kw: 0.0, coal_pad: false, pad: false, reach: 0, electric: false },
    // deep geothermal: wells 15 km down to superhot rock (game)
    pw(2, 2, S::B_DEEPGEO, 600_000.0, 800_000.0, 1_000_000.0, [80_000.0, 120_000.0, 160_000.0], 4.0, false),
    // orbital solar: sunlight collected in orbit and beamed to a ground station, day and night (game)
    pw(2, 2, S::B_ORBITAL, 2_200_000.0, 2_800_000.0, 3_600_000.0, [120_000.0, 200_000.0, 300_000.0], 0.0, false),
];

/// A utility only allots a site so much: this many grid hookups, and after that you make your own power.
/// It is what makes the ladder worth climbing before the halvings do.
pub const GRID_MAX: usize = 4;
/// The power tree. Diesel is open from the start (a backup genset, not a step). From the grid two branches
/// grow: fossil (gas, coal) and renewable (solar, wind, then hydro, deep geothermal, orbital). Nuclear opens
/// from the top of either early branch, and the core from the top of either late one.
/// Each entry: (node, parents). Maxing ANY parent (level 3) opens the node.
pub const TREE: [(u8, [u8; 2]); 12] = [
    (K_GRID, [NONE, NONE]), (K_DIESEL, [NONE, NONE]),
    (K_GAS, [K_GRID, NONE]), (K_COAL, [K_GAS, NONE]),
    (K_SOLAR, [K_GRID, NONE]), (K_WIND, [K_SOLAR, NONE]),
    (K_HYDRO, [K_WIND, NONE]), (K_DEEPGEO, [K_HYDRO, NONE]), (K_ORBITAL, [K_DEEPGEO, NONE]),
    (K_FISSION, [K_COAL, K_WIND]), (K_FUSION, [K_FISSION, NONE]),
    (K_CORE, [K_FUSION, K_ORBITAL]),
];
pub const NONE: u8 = 255;
/// The display order of the tree: branch by branch, the core last.
pub const LADDER: [u8; 12] = [K_GRID, K_DIESEL, K_GAS, K_COAL, K_SOLAR, K_WIND, K_HYDRO, K_DEEPGEO, K_ORBITAL, K_FISSION, K_FUSION, K_CORE];
pub fn parents(k: u8) -> [u8; 2] { TREE.iter().find(|t| t.0 == k).map_or([NONE, NONE], |t| t.1) }
/// The nodes that maxing `k` opens.
pub fn children(k: u8) -> impl Iterator<Item = u8> { TREE.iter().filter(move |t| t.1.contains(&k)).map(|t| t.0) }
/// The first node of each branch: maxing either is the end of the early game.
pub fn first_branch(k: u8) -> bool { k == K_GAS || k == K_SOLAR }

// Heat reuse. Almost every watt a miner draws comes out as heat. Air-cooled exhaust is only ~40 °C, so a
// grower can use about half of it; liquid-cooled machines give 50-60 °C water that's far easier to use.
pub const HEAT_REACH: i32 = 2;          // boxes within this many tiles of a greenhouse pipe their heat to it
pub const HEAT_CAP_KW: f64 = 200.0;     // heat one greenhouse can take
pub const HEAT_CAPTURE: [f64; 2] = [0.5, 0.85]; // share of a box's draw recovered: air-cooled, liquid (hydro or immersion)
pub const HEAT_USD: f64 = 20.0;         // $/MWh of heat when the grower needs it most (it saves them burning gas)
pub const HEAT_COOL: f64 = 6.0;         // °C cooler for a box whose exhaust is piped away

// ASIC market. Real machine prices track hashprice closely: when mining pays, rigs sell out and prices
// climb; when it doesn't, miners dump old machines and used prices crash (more than 80% in 2022).
/// The hashprice ($/TH/day) the list prices above were set at (late 2026).
pub const HP_REF: f64 = 0.035;
/// The power price ($/MWh) the buyers of used machines pay: miners with cheap power. A used machine is
/// worth what it earns above that.
pub const MKT_POWER: f64 = 35.0;
/// A broker keeps this share when you sell a machine.
pub const BROKER_CUT: f64 = 0.12;

// Firmware tuning. Custom firmware (Braiins OS, LuxOS, Vnish) lets miners pick a power target per machine.
// Indexed by tune + 2: low, eco, stock, turbo, max. Power climbs faster than hashrate, because chips need
// more voltage to run faster and power goes with frequency x voltage squared.
pub const TUNE_TH: [f64; 5] = [0.70, 0.85, 1.0, 1.15, 1.28];
pub const TUNE_KW: [f64; 5] = [0.55, 0.76, 1.0, 1.32, 1.62];
pub const TUNE_HEAT: [f64; 5] = [-10.0, -5.0, 0.0, 6.0, 13.0]; // °C on the box
pub const TUNE_WEAR: [f64; 5] = [0.7, 0.85, 1.0, 1.5, 2.2];
/// Custom firmware's dev fee: it mines this share of the time for its makers.
pub const FIRMWARE_FEE: f64 = 0.02;

/// What a 10-pack of each miner really costs (used S19j / XP, new S21s), for the dock and the ledger.
pub const REAL_PRICE: [f64; 4] = [3_500.0, 9_000.0, 22_000.0, 60_000.0];

/// Clearing land (tool id used by the UI, not a building)
pub const TOOL_CLEAR: i32 = 100;
pub const CLEAR_TREE: f64 = 150.0;
pub const CLEAR_RUBBLE: f64 = 250.0;

pub fn is_power(k: u8) -> bool { k <= K_CORE || k == K_DEEPGEO || k == K_ORBITAL }
pub fn is_miner(k: u8) -> bool { (K_MINER1..=K_MINER4).contains(&k) }
pub fn is_consumer(k: u8) -> bool { is_miner(k) || k == K_FAN || k == K_IMMERSION }

// Random events
pub const EV_NONE: i32 = -1;
pub const EV_HEATWAVE: i32 = 0;
pub const EV_GAS_SPIKE: i32 = 1;
pub const EV_BROWNOUT: i32 = 2;
pub const EV_PRICE_CRASH: i32 = 3;
pub const EV_PRICE_PUMP: i32 = 4;
pub const EV_LUCKY: i32 = 5;
pub const EV_INSPECTOR: i32 = 6;
pub const EV_DROUGHT: i32 = 7;
pub const EV_STORM: i32 = 8;
pub const EV_FIRE: i32 = 9;
pub const EV_DIFF_SWING: i32 = 10;
pub const EV_FUEL_DEAL: i32 = 11;
pub const EV_RAID: i32 = 12;
pub const EV_CURTAIL: i32 = 13;    // the grid asks you to cut load during a price spike (demand response)
pub const EV_FEE_SPIKE: i32 = 14;  // mempool congestion: transaction fees jump for a day or two
pub const EV_TARIFF: i32 = 15;     // the utility or the government raises what miners pay for power
pub const EV_HALVING: i32 = 20;
pub const EV_CORE_TRIAL: i32 = 21;

// Toast codes (JS turns them into text)
pub const T_RETARGET: f64 = 1.0;
pub const T_BAILOUT: f64 = 2.0;
pub const T_UNPAID: f64 = 3.0;
pub const T_UNLOCK: f64 = 4.0;
pub const T_BROKE: f64 = 5.0;
pub const T_EVENT: f64 = 6.0;
pub const T_NO_CASH: f64 = 7.0;
pub const T_BAD_SPOT: f64 = 8.0;
pub const T_LOCKED: f64 = 9.0;
pub const T_NEEDS_WATER: f64 = 10.0;
pub const T_CORE_REQ: f64 = 11.0;
pub const T_CORE_FAIL: f64 = 12.0;
pub const T_BUILT: f64 = 13.0;
pub const T_SHORTAGE: f64 = 14.0;
pub const T_REPAIRED: f64 = 15.0;
pub const T_SOLD_BTC: f64 = 16.0;
pub const T_BLACKMARKET: f64 = 17.0;
pub const T_BLOCK_FOUND: f64 = 18.0;
pub const T_UNPOWERED: f64 = 19.0;
pub const T_RAID: f64 = 20.0;          // arg = value stolen
pub const T_RAID_REPELLED: f64 = 21.0;
pub const T_SHED: f64 = 22.0;          // arg = miners switched off
pub const T_NOTHING_TO_CLEAR: f64 = 23.0;
pub const T_RAID_BREACH: f64 = 24.0;    // arg = value stolen
pub const T_WALLS_UP: f64 = 25.0;      // arg = walls upgraded
pub const T_LIGHTNING: f64 = 26.0;     // arg = 1 if it started a fire
pub const T_WEATHER: f64 = 27.0;       // arg = new weather
pub const T_CLEANED: f64 = 28.0;       // arg = boxes cleaned
pub const T_SOLO_BLOCK: f64 = 29.0;    // arg = BTC won solo
pub const T_MODE: f64 = 30.0;          // arg = 1 solo, 0 pool
pub const T_FEE_SPIKE: f64 = 31.0;     // arg = fee multiplier
pub const T_CURTAIL_PAID: f64 = 32.0;  // arg = credit paid
pub const T_CURTAIL_BILL: f64 = 33.0;  // arg = extra paid for running through the spike
pub const T_FEES_CALM: f64 = 34.0;     // fees back to normal
pub const T_LOAN: f64 = 35.0;          // arg = loan repayment taken from cash (history scenarios)
pub const T_NO_STOCK: f64 = 36.0;      // arg = miner kind with no used machines for sale
pub const T_MINERS_SELLING: f64 = 37.0; // capitulation: miners dump used machines. arg = machines listed
pub const T_SOLD_MINER: f64 = 38.0;    // arg = what the broker paid
pub const T_SPOT_SPIKE: f64 = 39.0;    // arg = spike price $/MWh
pub const T_DR_CALL: f64 = 40.0;       // arg = credit you'll be paid
pub const T_CONTRACT: f64 = 41.0;      // arg = new contract
pub const T_CONTRACT_LOCKED: f64 = 42.0; // arg = days left on the current one
pub const T_POOL: f64 = 43.0;          // arg = pool joined
pub const T_PARTS_FAILED: f64 = 44.0;  // arg = parts that failed today with nobody to swap them
pub const T_PARTS_FIXED: f64 = 45.0;   // arg = what the contractor charged
pub const T_TARIFF: f64 = 46.0;        // arg = new contract price $/MWh
pub const T_DRY_SEASON: f64 = 47.0;    // arg = 1 started, 0 ended
pub const T_NO_MARKET: f64 = 48.0;     // this site has no power market to buy spot or demand response from
pub const T_GRID_FULL: f64 = 49.0;     // the utility won't give this site another connection

pub const T_MILESTONE: f64 = 50.0;     // arg = index into MILESTONES
pub const T_AUTO_SOLD: f64 = 51.0;     // arg = BTC sold to cover bills

/// Backers' milestones. Miners raise money against hashrate they have actually switched on, so each target
/// reached releases a tranche of cash: something to aim for between rungs, and a cushion after a halving.
/// (what, target, cash): what 0 = hashrate in TH/s, 1 = halvings survived.
pub const MILESTONES: [(u8, f64, f64); 9] = [
    (0, 10_000.0, 3_000.0), (0, 50_000.0, 10_000.0), (0, 100_000.0, 20_000.0), (0, 250_000.0, 40_000.0), (1, 1.0, 25_000.0),
    (0, 500_000.0, 50_000.0), (0, 1_000_000.0, 100_000.0), (0, 2_000_000.0, 200_000.0), (1, 2.0, 250_000.0),
];

pub const W_CLEAR: u8 = 0;
pub const W_RAIN: u8 = 1;
pub const W_STORM: u8 = 2;
pub const W_DUST: u8 = 3;
pub const CLEAN_COST: f64 = 25.0;

// Demand response: the grid pays this much per MWh you don't use during a call (base + d20 roll x step),
// for a call lasting CURTAIL_HOURS. Decline and you pay CURTAIL_SPOT_X times the usual grid price instead.
pub const CURTAIL_HOURS: f64 = 6.0;
pub const CURTAIL_BASE: f64 = 150.0;
pub const CURTAIL_STEP: f64 = 30.0;
pub const CURTAIL_SPOT_X: f64 = 6.0;
// Power contracts for the grid hookup.
pub const C_FIXED: u8 = 0;  // one price every hour; the grid may offer to pay you to switch off
pub const C_SPOT: u8 = 1;   // the real-time price: cheaper on average, swings by hour and weather, spikes in emergencies
pub const C_DR: u8 = 2;     // fixed price plus a standby credit; grid calls switch you off automatically
/// Spot averages below a fixed contract: a fixed price includes the supplier's premium for carrying the risk.
pub const SPOT_MEAN: f64 = 0.85;
/// Spot price shape over the day (ERCOT-like: cheap overnight, dearest late afternoon).
pub const SPOT_HOUR: [f64; 24] = [0.62, 0.6, 0.58, 0.58, 0.6, 0.66, 0.78, 0.9, 0.95, 0.98, 1.0, 1.02,
                                  1.05, 1.1, 1.25, 1.4, 1.55, 1.6, 1.5, 1.3, 1.1, 0.95, 0.82, 0.7];
/// In a grid emergency the spot price runs this many times what a curtailment offer would pay.
pub const SPIKE_X: f64 = 4.0;
/// Demand response: credit per MWh drawn while enrolled ($/MWh), and how much more often calls come.
pub const DR_STANDBY: f64 = 3.0;
pub const DR_CALLS_X: f64 = 1.3;
/// Enrolled loads are paid less per call than an emergency offer: they're also paid to stand by.
pub const DR_CALL_PAY: f64 = 0.7;
/// Minimum term after a contract change, in days.
pub const CONTRACT_DAYS: u64 = 30;
// Mining pools. FPPS pays the expected value of every share at once and charges more for carrying the luck;
// PPLNS pays only when the pool finds a block. Latency to the pool's servers makes shares stale.
#[derive(Clone, Copy)]
pub struct Pool { pub fee: f64, pub ms: f64, pub pplns: bool, pub share: f64 }
pub const POOLS: [Pool; 3] = [
    Pool { fee: 0.02, ms: 25.0, pplns: false, share: 0.30 },  // big FPPS pool with servers near you
    Pool { fee: 0.01, ms: 240.0, pplns: false, share: 0.10 }, // budget FPPS pool, servers on another continent
    Pool { fee: 0.005, ms: 60.0, pplns: true, share: 0.02 },  // small PPLNS pool, 2% of the network
];
/// Stale shares: a share that reaches the pool after it has moved to new work earns nothing.
pub const STALE_BASE: f64 = 0.0005;
pub const STALE_PER_MS: f64 = 0.00008;
/// Rejected shares (hardware errors): bad results from chips pushed too hard.
pub const REJECT_BASE: f64 = 0.002;
pub const REJECT_OC: f64 = 0.012;   // per overclock step
pub const REJECT_HOT: f64 = 0.03;   // over 80 °C
pub const REJECT_WORN: f64 = 0.02;  // health under 30%
// Parts. One container = 10 machines; each machine has 3 hashboards, 1 power supply and 4 fans (hydro: no fans).
pub const P_BOARD: usize = 0;
pub const P_PSU: usize = 1;
pub const P_FAN: usize = 2;
pub const PARTS_PER_BOX: [u8; 3] = [30, 10, 40];
/// Daily failure odds per part for a healthy box running cool; wear, heat, overclocking, dust and brownouts raise them.
pub const PART_HAZARD: [f64; 3] = [0.00015, 0.0002, 0.0001];
/// Spare part prices (game dollars, about a sixth of real like everything else).
pub const PART_USD: [f64; 3] = [30.0, 12.0, 2.0];
/// A contractor's call-out fee, and the markup on parts they bring.
pub const CALLOUT_USD: f64 = 150.0;
pub const RUSH_X: f64 = 1.5;
pub const FAN_HEAT: f64 = 1.5;      // °C per dead fan in a box
pub const TECH_WAGE: f64 = 25.0;    // $/day
pub const TECH_SWAPS: u32 = 12;     // parts one technician swaps per day
pub const TECH_CLEANS: u32 = 10;    // dusty boxes one technician cleans per day, free
pub const GUARD_WAGE: f64 = 20.0;   // $/day
pub const GUARD_CUT: f64 = 0.25;    // each guard: raids 25% less likely, and a 30% chance to stop one
pub const MAX_CREW: u32 = 6;
// Sites. Each is a real place with its own power, climate, rules and risks. Texas is the game as it always was.
/// How wooded each site is, against Texas (map generation), and which tree sprites grow there (render).
pub const SITE_TREES: [f64; 6] = [1.0, 0.15, 1.25, 0.5, 1.35, 0.25];
pub const SITE_TREE_KINDS: [&[u8]; 6] = [&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10], &[0, 4, 7, 7], &[1, 3, 4, 8, 10, 1], &[2, 2, 1, 4, 7], &[5, 6, 9, 10, 5, 2], &[0, 3, 4, 7]];

#[derive(Clone, Copy)]
pub struct Site {
    pub grid: f64,      // contract $/MWh on Normal (+$10 per difficulty step)
    pub temp: f64,      // average air temperature, °C
    pub swing: f64,     // seasonal swing: + is hottest mid-year (north), - is hottest at the start (south)
    pub river: i8,      // -1 = map decides, 0 = never, 1 = always
    pub sun: f64, pub wind: f64,
    // event weights: heatwave, brownout, drought, storm, raid, curtailment call, inspector
    pub w: [f64; 7],
    pub tariff: f64,    // weight of a tariff hike
    pub duty: f64,      // machines cost this much more (import duty, shipping)
    pub callout: f64,   // contractor call-outs cost this much more (remote sites)
    pub market: bool,   // a real-time power market: spot and demand-response contracts
    pub dry: bool,      // hydro: the grid allocation halves in the dry season
    pub flare: bool,    // no grid: the hookup is a genset on a declining oil well's flare gas
}
pub const N_SITES: usize = 6;
pub const SITES: [Site; N_SITES] = [
    Site { grid: 35.0, temp: 24.0, swing: 6.0, river: -1, sun: 1.0, wind: 1.0, w: [1.0; 7], tariff: 0.0, duty: 1.0, callout: 1.0, market: true, dry: false, flare: false },   // Texas
    Site { grid: 35.0, temp: 4.0, swing: 6.0, river: 1, sun: 0.5, wind: 1.4, w: [0.0, 0.5, 0.6, 1.6, 0.2, 0.0, 1.0], tariff: 1.0, duty: 1.05, callout: 1.4, market: false, dry: false, flare: false }, // Iceland
    Site { grid: 26.0, temp: 24.0, swing: -5.0, river: 1, sun: 1.1, wind: 0.8, w: [1.3, 1.8, 1.0, 1.0, 1.8, 0.0, 1.4], tariff: 1.5, duty: 1.1, callout: 1.0, market: false, dry: false, flare: false }, // Paraguay
    Site { grid: 28.0, temp: 18.0, swing: 2.0, river: 1, sun: 1.2, wind: 0.9, w: [0.4, 2.5, 1.2, 1.0, 1.2, 0.0, 1.2], tariff: 1.5, duty: 1.25, callout: 1.6, market: false, dry: false, flare: false }, // Ethiopia
    Site { grid: 28.0, temp: 12.0, swing: 7.0, river: 1, sun: 0.9, wind: 0.8, w: [0.3, 0.8, 1.0, 1.0, 0.3, 0.0, 0.8], tariff: 0.5, duty: 1.15, callout: 1.8, market: false, dry: true, flare: false }, // Bhutan
    Site { grid: 20.0, temp: 6.0, swing: 15.0, river: 0, sun: 0.9, wind: 1.3, w: [0.6, 1.5, 0.0, 1.2, 0.4, 0.0, 1.0], tariff: 0.0, duty: 1.0, callout: 2.0, market: false, dry: false, flare: true }, // North Dakota
];
/// What opening another site costs: land, permits and a connection.
pub const OPEN_SITE_USD: f64 = 40_000.0;
/// Flare gas: a well's output falls this much a day.
pub const WELL_DECLINE: f64 = 0.997;
/// Arguing with the inspector and losing costs this many times the fine.
pub const INSPECTOR_LOSS_X: f64 = 2.5;
/// What harder difficulties add to a site's grid contract, $/MWh. Real miners pay roughly $25-60; it decides who survives a halving.
pub const GRID_EXTRA: [f64; 3] = [0.0, 8.0, 12.0];
/// Difficulty: the marginal miner's breakeven hashprice falls this much each retarget as hardware improves
/// (about -6% / -10% / -14% a year), and the random part of each retarget is drawn from this range.
pub const HP_DECAY: [f64; 3] = [0.9975, 0.995, 0.994];
pub const RETARGET_RANGE: [(f64, f64); 3] = [(-0.025, 0.025), (-0.025, 0.035), (-0.04, 0.05)];
/// At a halving the weakest machines unplug and the survivors' breakeven drops by this factor.
pub const HALVING_HP: f64 = 0.72;
/// Transaction fees grow this much a day (~20% a year): more of what miners earn as the subsidy shrinks.
pub const FEE_GROWTH: f64 = 1.0005;
/// The free energy core: a failed trial keeps the core and tries again after this many days, with this added to the roll.
pub const CORE_RETRY_DAYS: u64 = 10;
pub const CORE_TRY_BONUS: u32 = 4;
/// Starting cash by difficulty.
pub const START_CASH: [f64; 3] = [100_000.0, 75_000.0, 60_000.0];
