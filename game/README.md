# Power Grid

A Bitcoin mining plant tycoon in the style of the old 2.5D tycoon games. Build power, mine blocks,
survive difficulty retargets and halvings, and grow a power tree to the free energy core.

- **Rust + WebAssembly.** The whole game (seeded map generation, simulation, random events and
  a software pixel renderer) is one dependency-free Rust crate in `src/`, compiled to `wasm32-unknown-unknown`.
- **Runs anywhere.** `web/` is a static site: phones, tablets and desktops in any modern browser,
  touch or mouse. It installs as an app (PWA) and plays offline after the first load.
- **History replays.** Winter Storm Uri (Feb 2021), the China ban (May–Jul 2021), the 2022 bear market
  and the 2024 halving, driven by real daily price, difficulty and fees in `web/history.js`.
  Rebuild that file with `python3 tools/make_history.py <data-dir> web/history.js`; it needs
  `btc_daily_close.json` (Coinbase Exchange daily closes) plus mempool.space's
  `/v1/mining/blocks/fees/all` and `/v1/mining/difficulty-adjustments` saved in `<data-dir>`.
  (mempool.space's pre-2023 price history is weekly and dated six days early, so prices come from Coinbase.)

## Play locally

```sh
cd web
python3 -m http.server 8000      # any static server works
# open http://localhost:8000
```

## Build

```sh
rustup target add wasm32-unknown-unknown
./build.sh                       # writes web/game.wasm
```

Sprites: `python3 tools/build_atlas.py` rebuilds `web/atlas.png`, `web/sprites.json` and `src/sprites.rs`
from the OpenGFX cut-outs in `../assets/sprites/opengfx` (override with `PG_SPRITES=...`) plus hand-drawn sprites in
`tools/ttdmap.py` (containers, greenhouse...). Needs Pillow.

## Balance simulator

`tools/sim` plays the real game core headless with a simple greedy bot and reports how fast runs max their first branch node (gas):

```sh
cd tools/sim
cargo run --release -- 60 0 sim     # 60 runs, Normal, simulated market
cargo run --release -- 60 1 live    # Hard, live flag (same rules as sim after day 0)
cargo run --release -- 1 0 sim v    # one run, verbose trace and cash ledger
PG_USED=1 cargo run --release -- 60 0 sim   # bot buys used machines whenever any are listed
PG_TUNE=1 cargo run --release -- 60 0 sim   # bot puts every box on its most profitable firmware profile
PG_SITE=5 cargo run --release -- 120 0 sim      # a site: 0 Texas .. 5 North Dakota
PG_PARTS=crew cargo run --release -- 200 0 sim  # parts: none (ignore), fix (contractor), crew (technicians + spares)
PG_POOL=2 cargo run --release -- 60 0 sim       # 0 big FPPS, 1 budget overseas, 2 PPLNS
PG_HEAT=1 cargo run --release -- 60 0 sim       # bot builds greenhouses next to its boxes
PG_CONTRACT=1 cargo run --release -- 60 0 sim   # 0 fixed, 1 spot (PG_CAP=<$/MWh> for a flat limit), 2 demand response
cargo run --release -- 15 0 full     # play the whole tree: the day each node opens (PG_DAYS=4000, PG_SITE; add v for a trace, live for the live flag)
PG_BRANCH=fossil cargo run --release -- 15 0 full   # keep the bot to one route: fossil or renewable
cargo run --release -- check         # rules that must hold whatever the balance: no-river maps, old saves, both roads to the core
```

CI runs these checks on every push: at least half of Normal runs max a branch node before the first halving (`sim`),
the tree rules hold (`check`), at least half of 15 Normal runs win (`full`), each branch wins on its own (`PG_BRANCH`),
and at least a third of 10 Hard runs win.

The `full` bot is a careful, unhurried player, not an expert: it grows while a box pays for itself, puts half its income
aside for the next node, trades old boxes in, keeps a guard and four signs but builds no walls. At build .32 (30 runs each):
Normal wins 30/30, median day ~860 (fossil only ~1,070, renewable only ~1,000); Hard wins 29/30, median ~1,435;
Brutal wins about 8/30, and most runs that don't win go bankrupt after the second halving. The first halving (day ~517)
is the earliest the core allows.

## Save format

Saves are versioned. v9+ writes the number of scalar fields after the version, so new builds can append
fields and still read old saves (v8 saves load too). v100 added three bytes per building (failed parts);
v9–99 saves still load with no failed parts. Versions 100–199 may only append scalars; changing the tile or
building records again needs v200+. A save that won't load is parked in the browser, never deleted.

## Put it on the internet

**GitHub Pages with automatic builds:** `.github/workflows/pages.yml` (at the repo root, one level above `game/`)
builds the WASM, runs the balance check and publishes `game/web` on every push to `main`. Set
Settings › Pages › Source to "GitHub Actions" once.


Upload `web/` to any static host. Make sure `.wasm` is served as `application/wasm`
(GitHub Pages, Netlify, Cloudflare Pages and Vercel all do).

- **GitHub Pages:** push this folder, then Settings › Pages › deploy from branch, folder `/game/web`
  (or copy `web/` to the repo root).
- **Netlify / Cloudflare Pages:** publish directory `game/web`, no build command.

## How to play

- Drag to pan, pinch or scroll to zoom, tap a building to inspect it.
- Pick something in the build bar, then tap the map (on touch screens, tap again on **Place** to confirm).
- Every power source upgrades to level 3. Maxing one opens what grows from it on the **power tree**:
  - the grid opens two branches: **fossil** (gas → coal) and **renewable** (solar → wind → hydro → deep geothermal → orbital solar);
  - coal *or* wind opens **nuclear** (fission → fusion);
  - fusion *or* orbital solar opens the **free energy core**;
  - diesel is a backup genset you can build from the start. On a map with no river, wind opens deep geothermal directly.

  Everything up to fission is real; deep geothermal, orbital solar and fusion are marked "game" in the UI, and the
  core's closing card explains why free energy wouldn't make mining free.
- The core needs a halving survived, fusion or orbital solar at level 3, ₿ 21 held, 2016 blocks with no power
  shortage, $10M, and a d20 roll of 12 or more. A failed roll keeps the core: it tries again 10 days later, +4 on the roll each time.
- **Grid limit:** a site gets four grid hookups (4.8 MW). Past that you make your own power, which is what the tree is for.
- **Bills:** they're paid in cash. If cash runs out and you hold bitcoin, just enough is sold at the market to cover them.
- **Live vs Simulated:** Live starts from today's real market (mempool.space) and plays it forward in the game; Simulated
  starts from a fixed snapshot, so a seed always plays the same. The rules are the same after day 0.
- **Milestones:** your backers pay out as hashrate comes online (10 PH/s up to 2 EH/s) and when you survive each halving:
  $3,000 up to $250,000. The next one shows under the alerts. Not in replays.
- **Skip ahead (the double arrow, or F):** top speed until an event, an alarm, a milestone, a new rung, or enough cash for the next step on the tree.
- **Building:** the tool stays in hand until Esc or Done. With a mouse, drag a miner to lay a row.
- Random events (heatwaves, gas spikes, brownouts, price swings, fires, inspectors, curtailment calls,
  fee spikes) hit most days on Hard. Unpaid bills for 3 days on Hard and the plant is repossessed.
- Your grid contract is $35 / $45 / $55 per MWh on Normal / Hard / Brutal.
- **Curtailment calls:** the grid pays you to switch off for 6 hours, or you keep mining at 6× the price.
- **Fee spikes:** fees jump for a day or two (and right after every halving).
- **The ledger:** 48 goals, each unlocking a card on how real mining works and where the game bends it.
- **Machine market (Miners tab):** ASIC prices follow hashprice. New machines move at about three-quarters
  strength (index^0.75); used ones are priced on what they earn for a buyer paying $35/MWh, so old models crash
  hardest in a downturn. Used listings refresh every 7 days, with more when margins are thin. Used boxes arrive
  at 45–80% health. A broker buys yours back at the used price less 12%. Replays price machines off that day's
  real hashprice and score cash + bitcoin + machines.
- **Game feel:** day and night (lights come on; fades out at the faster speeds, toggle in Settings), delivery
  trucks for new machines, technicians walking to boxes that need work, raiders you can see, a "First steps"
  checklist, and seed challenges: a code like `CA20A69A-0-0` (seed-site-difficulty) or a `#c=CODE` link starts
  the same run, with a result card after one year.
- **Sites:** each run starts somewhere real: Texas (the original game), Iceland, Paraguay, Ethiopia, Bhutan or a
  North Dakota oil well. Power price, climate, events, import duty, remoteness and contracts differ (only Texas has
  spot and demand response). The globe button opens more sites ($40,000 each): one wallet and one network, each site
  its own map. Saves with several sites start with the marker 1000.0, then each site's own save, length-prefixed.
- **Parts and crew (Site tab):** each container is 10 machines with 30 hashboards, 10 power supplies and 40 fans
  (hydro: none) that fail on their own; wear, heat, overclocking, dust and brownouts raise the odds. Technicians
  ($25/day) swap 12 parts a day from your spares and clean the 10 dustiest boxes; without one, a contractor charges
  $150 a visit plus 50% on parts you don't stock. Guards ($20/day) deter raids.
- **Pools (tap ₿):** a big FPPS pool nearby (2% fee, 25 ms), a budget FPPS pool overseas (1%, 240 ms) or a small
  PPLNS pool (0.5%, 60 ms, 2% of the network, pays only when it finds a block). Stale shares grow with latency;
  rejected shares with overclocking, heat over 80 °C and worn boxes.
- **Heat reuse (Cooling tab):** a greenhouse buys the heat of boxes within 2 tiles, up to 200 kW. Air-cooled boxes
  give 50% of their draw as useful heat, hydro or immersion-touching boxes 85%. Heat sells for ~$20/MWh scaled by
  cold and gas price; piped boxes run 6 °C cooler.
- **Power contracts (grid hookup, or Power tab):** Fixed (curtailment calls are offers), Spot (averages 85% of
  fixed, shaped by hour, gas, heat and brownouts, spikes in emergencies; boxes switch off at their own break-even
  or above a flat limit), or Fixed + demand response ($3/MWh standby credit, calls switch you off automatically).
  Any change runs at least 30 days.
- **Firmware (tap a miner):** five profiles, Low / Eco / Stock / Turbo / Max: 70–128% hashrate for 55–162%
  power. Overclocking runs hotter and wears faster; any custom profile costs a 2% fee. Saved in the high bits of
  each building's direction byte, so older saves load as Stock.
- **Game prices:** machines cost about 1/6 of real so a run fits in an evening. Hashrates, J/TH, difficulty,
  rewards, fees and the halving schedule are real. Difficulty follows profitability, as real hashrate does.

## Layout

```
src/lib.rs       exports to JS, allocator, UI mailbox
src/game.rs      map generation, simulation, events, save/load
src/render.rs    software renderer (isometric, pixel art)
src/data.rs      building kinds, power tree, events
src/sprites.rs   generated sprite table
web/             index.html, style.css, game.js, game.wasm, atlas.png, sw.js, manifest
tools/           atlas builder + sprite drawing code
```
