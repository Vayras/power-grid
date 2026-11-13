"""Build game/web/history.js: daily real price, difficulty and fees for the history scenarios.
Sources: Coinbase Exchange daily BTC-USD closes (btc_daily_close.json, {date: close}); mempool.space
/v1/mining/blocks/fees/all (daily average fees) and /v1/mining/difficulty-adjustments.
(mempool.space's pre-2023 price history is weekly and dated 6 days early, so it isn't used for price.)"""
import json, sys, datetime as dt
src, out = sys.argv[1], sys.argv[2]
closes = json.load(open(f'{src}/btc_daily_close.json'))
fees = json.load(open(f'{src}/v1_mining_blocks_fees_all.json'))
adj = sorted(json.load(open(f'{src}/v1_mining_difficulty-adjustments.json')), key=lambda a: a[0])  # [time, height, difficulty, change]

def day_ts(s): return int(dt.datetime.strptime(s, '%Y-%m-%d').replace(tzinfo=dt.timezone.utc).timestamp())
def difficulty_at(t):
    d = adj[0][2]
    for a in adj:
        if a[0] <= t: d = a[2]
        else: break
    return d
def fee_row(t):  # the daily point nearest to noon of that day
    best = min(fees, key=lambda f: abs(f['timestamp'] - (t + 43200)))
    return best

S = {
  'uri': ('2021-02-01', 30, None), 'china': ('2021-05-01', 92, None),
  'bear': ('2022-04-01', 275, None), 'halving': ('2024-04-01', 91, 840000 - 144 * 19),
}
# scripted grid events for Uri (flags: 1 cold snap, 2 curtailment call, 4 rotating outage), $/MWh
URI = {'2021-02-13': (1, 0), '2021-02-14': (3, 1500), '2021-02-15': (7, 9000), '2021-02-16': (3, 9000),
       '2021-02-17': (3, 9000), '2021-02-18': (3, 9000), '2021-02-19': (2, 9000)}
data = {}
for key, (start, n, h0) in S.items():
    t0 = day_ts(start); rows = []
    for d in range(n):
        t = t0 + d * 86400; f = fee_row(t)
        date = dt.datetime.fromtimestamp(t, dt.timezone.utc).strftime('%Y-%m-%d')
        flags, rate = URI.get(date, (0, 0)) if key == 'uri' else (0, 0)
        if key == 'bear' and date.endswith('-01') and date != start: flags, rate = 8, 28000  # monthly machine-loan payment
        rows.append([round(closes[date]), round(difficulty_at(t + 43200) / 1e9) / 1e3, round(f['avgFees'] / 1e8, 5), flags, rate])
    first = fee_row(t0)
    data[key] = {'start': start, 'height': h0 or int(first['avgHeight']), 'rows': rows}
    print(key, start, 'height', data[key]['height'], 'price', rows[0][0], '->', rows[-1][0], 'diff T', rows[0][1], '->', rows[-1][1], 'fees', min(r[2] for r in rows), max(r[2] for r in rows))
with open(out, 'w') as fo:
    fo.write('// Real daily bitcoin price (USD), difficulty (trillions), average fees per block (BTC), scripted grid flags and $/MWh.\n')
    fo.write('// Generated from mempool.space data by tools/make_history.py. Do not edit by hand.\n')
    fo.write('window.PG_HISTORY = ' + json.dumps(data, separators=(',', ':')) + ';\n')
