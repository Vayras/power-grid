// Power Grid web shell: loads the Rust/WASM core, draws its framebuffer, and runs the UI.
'use strict';
const BUILD = 'build 2026-11-13.31';
(async function () {
const $ = (s) => document.querySelector(s);
const el = (tag, attrs = {}, html = '') => { const e = document.createElement(tag); for (const k in attrs) e.setAttribute(k, attrs[k]); e.innerHTML = html; return e; };

// ------------------------------------------------------------------ data (mirrors src/data.rs)
const KINDS = [
  { name: 'Grid hookup', tab: 'power', spr: 'B_GRID', desc: 'Utility power on a fixed contract. Up to 4 per site. Brownouts and curtailment calls happen.' },
  { name: 'Diesel genset', tab: 'power', spr: 'B_DIESEL', desc: '$240/MWh, fuel price swings' },
  { name: 'Gas peaker', tab: 'power', spr: 'B_GAS', desc: '$60/MWh × gas price' },
  { name: 'Coal station', tab: 'power', spr: 'B_COAL', desc: '$45/MWh, inspectors' },
  { name: 'Solar field', tab: 'power', spr: 'B_SOLAR', desc: 'Free, daytime only' },
  { name: 'Wind turbine', tab: 'power', spr: 'B_WIND0', desc: 'Free, random gusts' },
  { name: 'Hydro dam', tab: 'power', spr: 'B_HYDRO', desc: 'Must touch water, $10/MWh' },
  { name: 'Fission reactor', tab: 'power', spr: 'B_FISSION', desc: '$28/MWh, huge output' },
  { name: 'Fusion tokamak', tab: 'power', spr: 'B_FUSION', desc: 'Game: no fusion plant sells power yet. $8/MWh, gigawatts', game: true },
  { name: 'Free energy core', tab: 'power', spr: 'B_CORE', desc: 'Game: impossible. ∞ MW at $0. Ends the game', game: true },
  { name: '10× Antminer S19j Pro', tab: 'miners', spr: 'B_MINER1', desc: '1.04 PH/s · 30.7 kW · 29.5 J/TH' },
  { name: '10× Antminer S19 XP', tab: 'miners', spr: 'B_MINER2', desc: '1.41 PH/s · 30.1 kW · 21.3 J/TH' },
  { name: '10× Antminer S21', tab: 'miners', spr: 'B_MINER3', desc: '2.0 PH/s · 35 kW · 17.5 J/TH' },
  { name: '10× S21 XP Hydro', tab: 'miners', spr: 'B_MINER4', desc: '4.73 PH/s · 56.8 kW · 12 J/TH' },
  { name: 'Fan wall', tab: 'cooling', spr: 'B_FAN', desc: 'Blows 4 tiles in one direction. Press R to turn.' },
  { name: 'Immersion tank', tab: 'cooling', spr: 'B_IMMERSION', desc: '−22 °C to boxes touching it' },
  { name: 'Garage shed', tab: 'none', spr: 'B_SHED', desc: 'Where it all started' },
  { name: 'Power pole', tab: 'site', spr: 'B_POLE', desc: 'Carries power 4 tiles. +0.4% line loss' },
  { name: 'Wall', tab: 'site', spr: 'B_WALL', desc: 'Drag to lay. Upgrade: wood → stone → steel' },
  { name: 'No Trespassing sign', tab: 'site', spr: 'B_SIGN', desc: 'Each sign (up to 4) makes raids 12% rarer' },
  { name: 'Greenhouse', tab: 'cooling', spr: 'B_GREENHOUSE', desc: 'Buys the heat of boxes within 2 tiles, up to 200 kW. They run 6 °C cooler.' },
];
const CLEAR = 100;
KINDS[21] = { name: 'Deep geothermal', tab: 'power', spr: 'B_DEEPGEO', desc: 'Game: wells 15 km down to superhot rock. $6/MWh, steady', game: true };
KINDS[22] = { name: 'Orbital solar', tab: 'power', spr: 'B_ORBITAL', desc: 'Game: sunlight beamed down from orbit, day and night. Free', game: true };
// The ladder's order: maxing one unlocks the next (same as LADDER in data.rs).
const LADDER = [0, 1, 2, 3, 4, 5, 6, 7, 21, 22, 8, 9];
// A map with no river can't build a dam, so the ladder steps over hydro there (UI()[30] = this site has a river).
const noRiver = () => !UI()[30];
const prevTier = (k) => { const p = LADDER[LADDER.indexOf(k) - 1]; return p === 6 && noRiver() ? 5 : p; };
const nextTier = (k) => { const n = LADDER[LADDER.indexOf(k) + 1]; return n === 6 && noRiver() ? 7 : n; };
const isPow = (k) => k <= 9 || k === 21 || k === 22;
const CLEAR_ITEM = { name: 'Clear land', tab: 'site', spr: 'TREE_0', desc: 'Drag to remove trees ($150) and rubble ($250)' };
const kindOf = (k) => (k === CLEAR ? CLEAR_ITEM : KINDS[k]);
const isMiner = (k) => k >= 10 && k <= 13;
const PAINT = (k) => k === 18 || k === CLEAR;
const CAP = [[250, 600, 1200], [500, 1000, 2000], [2000, 5000, 10000], [10000, 20000, 40000], [5000, 12000, 25000], [6000, 12000, 20000], [20000, 40000, 80000], [50000, 100000, 150000], [200000, 350000, 500000], [Infinity, Infinity, Infinity]];
const UNLOCK_HINT = { 12: 'Unlocks with diesel', 13: 'Unlocks with gas', 15: 'Unlocks with coal' };
CAP[21] = [80000, 120000, 160000]; CAP[22] = [120000, 200000, 300000];
const DIRN = ['→ south-east', '↓ south-west', '← north-west', '↑ north-east'];
const EVENTS = {
  0: { t: 'Heatwave', c: 'var(--alarm)', d: '+9 °C for 3 days. Air-cooled miners throttle above 80 °C and can break above 95 °C.' },
  1: { t: 'Gas price spike', c: 'var(--btc)', d: 'Gas costs ×2.1 for 3 days.' },
  2: { t: 'Grid brownout', c: 'var(--alarm)', d: 'The grid hookup is down for a day.' },
  3: { t: 'BTC price crash', c: 'var(--alarm)', d: 'Bitcoin drops 25%.' },
  4: { t: 'BTC pumps', c: 'var(--cash)', d: 'Bitcoin jumps 20%.' },
  5: { t: 'Lucky block', c: 'var(--btc)', d: 'Your pool credited you a whole block.' },
  6: { t: 'Inspector visit', c: 'var(--btc)', d: 'An inspector found coal dust everywhere.' },
  7: { t: 'Drought', c: 'var(--alarm)', d: 'Hydro output −60% for 5 days.' },
  8: { t: 'Storm', c: 'var(--hash)', d: 'Turbines locked for a day.' },
  9: { t: 'Miner fire', c: 'var(--alarm)', d: 'Your hottest miner box caught fire.' },
  10: { t: 'Difficulty swing', c: 'var(--volt)', d: 'Hashrate is moving: the next projected retarget swings an extra 5%.' },
  11: { t: 'Diesel deal', c: 'var(--cash)', d: 'Diesel −40% for 5 days.' },
  12: { t: 'Raid', c: 'var(--alarm)', d: 'Thieves came over the fields.' },
  13: { t: 'Curtailment call', c: 'var(--volt)', d: 'The grid is short and power prices are spiking.' },
  14: { t: 'Fee spike', c: 'var(--btc)', d: 'The mempool is jammed and transaction fees jumped.' },
};
const GRID_USD = [35, 45, 55], START_CASH = [80000, 60000, 50000];
const DIFFS = [
  ['Normal', '$80k to start, the site\'s usual power price. Early bailouts from the Foreman. 7 unpaid days before bankruptcy.'],
  ['Hard', '$60k, power $10/MWh dearer than the site\'s usual price. No bailouts, a random event most shifts. 3 unpaid days.'],
  ['Brutal', '$50k, power $20/MWh dearer. Wild prices, events nearly every other day, 1 unpaid day and you are out.'],
];

// ------------------------------------------------------------------ formatting
const money = (v) => { const s = v < 0 ? '−' : ''; v = Math.abs(v); if (v >= 1e9) return s + '$' + (v / 1e9).toFixed(2) + 'B'; if (v >= 1e6) return s + '$' + (v / 1e6).toFixed(2) + 'M'; if (v >= 1e4) return s + '$' + (v / 1e3).toFixed(1) + 'k'; return s + '$' + Math.round(v).toLocaleString('en-US'); };
const btcs = (v) => v > 0 && v < 0.001 ? Math.round(v * 1e8).toLocaleString() + ' sats' : btcf(v);
const btcf = (v) => '₿ ' + (v >= 100 ? v.toFixed(1) : v >= 1 ? v.toFixed(3) : v.toFixed(5));
const power = (kw) => kw === Infinity || kw > 1e11 ? '∞' : kw >= 1e6 ? (kw / 1e6).toFixed(1) + ' GW' : kw >= 1000 ? (kw / 1000).toFixed(1) + ' MW' : Math.round(kw) + ' kW';
const hash = (th) => th >= 1e6 ? (th / 1e6).toFixed(2) + ' EH/s' : th >= 1000 ? (th / 1000).toFixed(2) + ' PH/s' : Math.round(th) + ' TH/s';


// ------------------------------------------------------------------ pixel glyphs (16x16, 2px strokes, square caps) — no emoji anywhere
const ICONS = {
  bolt: 'M9 1L3 9h5l-1 6 6-8H8z', chip: 'M4 4h8v8H4zM6 1v3M10 1v3M6 12v3M10 12v3M1 6h3M1 10h3M12 6h3M12 10h3',
  flame: 'M8 1l4 5v5l-2 3H6l-2-3V7l2 2z', lock: 'M3 7h10v7H3zM5 7V3h6v4', gauge: 'M2 13V9l2-4 4-2 4 2 2 4v4M8 12l3-5',
  drop: 'M8 1l4 7v3l-2 3H6l-2-3V8z', sun: 'M5 5h6v6H5zM8 1v2M8 13v2M1 8h2M13 8h2', warn: 'M8 2l7 12H1zM8 6v4M8 12v1',
  shield: 'M3 2h10v6l-5 6-5-6z', wind: 'M1 5h9V3M1 9h12v2M1 13h6', plug: 'M5 1v4M11 1v4M3 5h10v4l-3 3H6L3 9zM8 12v3',
  power: 'M8 1v6M4 4L2 7v4l3 3h6l3-3V7l-2-3', dust: 'M3 10h1M7 7h1M12 11h1M6 13h1M10 4h1M4 4h1', book: 'M2 2h5l1 1 1-1h5v12H9l-1 1-1-1H2zM8 3v11',
  cube: 'M8 1l6 3v8l-6 3-6-3V4zM2 4l6 3 6-3M8 7v8', chart: 'M1 15h14M2 12l4-5 3 3 5-7', broom: 'M11 1L7 8M3 9h8l2 6H2z',
  x: 'M3 3l10 10M13 3L3 13', rain: 'M3 8h10V5h-2V3H5v2H3zM5 11v2M8 11v3M11 11v2', storm: 'M3 8h10V5h-2V3H5v2H3zM9 9l-2 3h3l-2 3',
  check: 'M2 8l4 4 8-8', pause: 'M5 3v10M11 3v10', play: 'M5 3l7 5-7 5z', up: 'M8 14V2M3 7l5-5 5 5',
  stack: 'M2 3h12M2 7h12M2 11h9M2 15h5', help: 'M5 5V3h6v4L8 9v2M8 13v2', heart: 'M8 14L2 8V4l2-2h2l2 2 2-2h2l2 2v4z',
  snow: 'M8 1v14M2 4l12 8M14 4L2 12', therm: 'M7 2h2v8M5 10h6v5H5z', pick: 'M2 6l3-3h6l3 3M8 3L5 15', speaker: 'M2 6h3l4-3v10l-4-3H2zM12 5v6',
  music: 'M6 12V3l8-2v9M3 12h3v3H3zM11 10h3v3h-3z', menu: 'M2 4h12M2 8h12M2 12h12', skip: 'M3 3l6 5-6 5zM12 3v10', turn: 'M13 8a5 5 0 1 1-2-4M11 1v3h3',
  wrench: 'M10 2l4 4-3 1-6 6-2-2 6-6z',
};
const ic = (n, cls = '') => `<svg class="ic ${cls}" viewBox="0 0 16 16" aria-hidden="true"><path d="${ICONS[n] || ICONS.help}"/></svg>`;

// ------------------------------------------------------------------ boot
const bootBar = $('#boot-bar'), bootMsg = $('#boot-msg');
const step = (p, m) => { bootBar.style.width = p + '%'; if (m) bootMsg.textContent = m; };
if ('serviceWorker' in navigator && location.protocol.startsWith('http')) navigator.serviceWorker.register('sw.js', { updateViaCache: 'none' }).then((r) => r.update()).catch(() => {});

let E, mem;
try {
  step(20, 'Loading the WASM core…');
  const bytes = await (await fetch('game.wasm?' + BUILD.slice(6))).arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  E = instance.exports;
  step(55, 'Unpacking sprites…');
  const [img, sprites] = await Promise.all([
    new Promise((ok, bad) => { const i = new Image(); i.onload = () => ok(i); i.onerror = bad; i.src = 'atlas.png?' + BUILD.slice(6); }),
    fetch('sprites.json?' + BUILD.slice(6)).then((r) => r.json()),
  ]);
  window.SPR = sprites;
  const aw = E.atlas_w(), ah = E.atlas_h();
  const c = document.createElement('canvas'); c.width = aw; c.height = ah;
  const cx = c.getContext('2d'); cx.drawImage(img, 0, 0);
  const px = cx.getImageData(0, 0, aw, ah).data;
  const ptr = E.atlas_buf();
  new Uint8Array(E.memory.buffer, ptr, aw * ah * 4).set(px);
  step(80, 'Waking the Foreman…');
  if (document.fonts && document.fonts.ready) await Promise.race([document.fonts.ready, new Promise((r) => setTimeout(r, 1500))]);
} catch (err) {
  bootMsg.textContent = 'This browser could not start the game: ' + err;
  return;
}
mem = () => E.memory.buffer;
const UI = () => new Float64Array(mem(), E.ui_ptr(), 570);
window.PG = { get E() { return E; }, UI, finale: () => showFinale(UI()) };

// ------------------------------------------------------------------ canvas + loop
const canvas = $('#view');
const ctx = canvas.getContext('2d', { alpha: false });
let zoom = innerWidth >= 900 ? 2 : 1, fbW = 0, fbH = 0, imgData = null;
function fit() {
  const dpr = Math.max(1, window.devicePixelRatio || 1);
  const dprR = Math.max(1, Math.round(dpr));
  const S = zoom * dprR;
  const w = Math.ceil(innerWidth * dpr / S), h = Math.ceil(innerHeight * dpr / S);
  if (w === fbW && h === fbH) return;
  fbW = w; fbH = h; canvas.width = w; canvas.height = h;
  E.resize(w, h); imgData = ctx.createImageData(w, h);
  redraw = true; // resizing a canvas wipes it: paint it again even when no run is going
}
addEventListener('resize', fit);

let running = false, last = performance.now(), uiTick = 0, redraw = true;
function loop(now) {
  const dt = now - last; last = now;
  if (running || redraw) {
    // Behind the start screen the map only needs a fresh frame after a resize or a new preview (no time passes).
    E.frame(running ? dt : 0); redraw = false;
    imgData.data.set(new Uint8ClampedArray(mem(), E.fb_ptr(), fbW * fbH * 4));
    ctx.putImageData(imgData, 0, 0);
  }
  if (running) {
    uiTick += dt;
    if (uiTick > 90) { uiTick = 0; updateUI(); }
  }
  requestAnimationFrame(loop);
}

// ------------------------------------------------------------------ input
const toFb = (e) => { const r = canvas.getBoundingClientRect(); return [Math.floor((e.clientX - r.left) * fbW / r.width), Math.floor((e.clientY - r.top) * fbH / r.height)]; };
const pointers = new Map();
let drag = null, pinch0 = 0, tool = -1, paint = null;
canvas.addEventListener('pointerdown', (e) => {
  canvas.setPointerCapture(e.pointerId);
  pointers.set(e.pointerId, { x: e.clientX, y: e.clientY, type: e.pointerType });
  if (pointers.size === 1 && (PAINT(tool) || (isMiner(tool) && e.pointerType === 'mouse' && E.occ_at(...toFb(e)) < 0))) { const [x, y] = toFb(e); E.hover(x, y); const c = E.hover_packed(); paint = { i: Math.floor(c / 1000), j: c % 1000, n: 0 }; paintTile(paint.i, paint.j); drag = null; return; }
  if (pointers.size === 1) drag = { x: e.clientX, y: e.clientY, moved: 0 };
  if (pointers.size === 2) { const [a, b] = [...pointers.values()]; pinch0 = Math.hypot(a.x - b.x, a.y - b.y); drag = null; paint = null; }
});
function paintTile(i, j) {
  const r = E.paint_tile(i, j);
  if (r === 1) { paint && paint.n++; if (!paint || paint.n % 2 === 1) sfx(tool === CLEAR ? 'clear' : isMiner(tool) ? 'build' : 'fence'); }
  if (r === -1) { sfx('bad'); paint = null; }
}
function paintTo(i1, j1) {
  // 4-connected line so fences never leave diagonal gaps
  let { i, j } = paint;
  while ((i !== i1 || j !== j1) && paint) {
    if (Math.abs(i1 - i) >= Math.abs(j1 - j)) i += Math.sign(i1 - i); else j += Math.sign(j1 - j);
    paint.i = i; paint.j = j; paintTile(i, j);
  }
}
canvas.addEventListener('pointermove', (e) => {
  const p = pointers.get(e.pointerId);
  if (e.pointerType === 'mouse') { const [x, y] = toFb(e); E.hover(x, y); mouseAt = [e.clientX, e.clientY]; }
  if (!p) return;
  if (paint && pointers.size === 1) { const [x, y] = toFb(e); E.hover(x, y); const c = E.hover_packed(); if (c >= 0) paintTo(Math.floor(c / 1000), c % 1000); return; }
  p.x = e.clientX; p.y = e.clientY;
  if (pointers.size === 2) {
    const [a, b] = [...pointers.values()]; const d = Math.hypot(a.x - b.x, a.y - b.y);
    if (pinch0 && d / pinch0 > 1.35) { setZoom(zoom + 1); pinch0 = d; } else if (pinch0 && d / pinch0 < 0.74) { setZoom(zoom - 1); pinch0 = d; }
    return;
  }
  if (drag) {
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    drag.moved += Math.abs(dx) + Math.abs(dy);
    if (drag.moved > 14) {
      const r = canvas.getBoundingClientRect();
      E.pan(-Math.round(dx * fbW / r.width), -Math.round(dy * fbH / r.height));
      drag.x = e.clientX; drag.y = e.clientY;
    }
  }
});
const endPtr = (e) => {
  const p = pointers.get(e.pointerId); pointers.delete(e.pointerId);
  if (paint || (PAINT(tool) && p)) { paint = null; if (!pointers.size) drag = null; if (isMiner(tool)) { E.sync_ui(); showPlacebar(); } return; }
  if (!p || !drag || drag.moved > 14 || pointers.size) { if (!pointers.size) drag = null; return; }
  drag = null;
  const [x, y] = toFb(e);
  if (tool >= 0 && p.type !== 'mouse' && E.occ_at(x, y) < 0) { E.hover(x, y); showPlacebar(); return; }
  const r = E.click(x, y);
  E.sync_ui();
  if (r === 1 && (tool === 14 || tool === 15)) unlockGoal('cooling');
  if (r === 1) { sfx(tool === 14 ? 'whoosh' : 'build'); if (tool === 9) setTool(-1); else showPlacebar(); } // the tool stays in hand until Esc, Done or a tap on a building
  if (r === -1) sfx('bad');
  if (r === 3) dropTool();
  if (r === 2 || r === 3) { sfx('tap'); openInspector(); } else if (r === 0) closeInspector();
};
canvas.addEventListener('pointerup', endPtr);
canvas.addEventListener('pointerleave', (e) => { if (e.pointerType === 'mouse' && tool < 0) { E.hover_tile(-1, -1); mouseAt = null; } });
let mouseAt = null;
canvas.addEventListener('pointercancel', (e) => { pointers.delete(e.pointerId); drag = null; });
canvas.addEventListener('wheel', (e) => { e.preventDefault(); setZoom(zoom + (e.deltaY < 0 ? 1 : -1)); }, { passive: false });
function setZoom(z) { z = Math.max(1, Math.min(4, z)); if (z === zoom) return; zoom = z; fit(); }
addEventListener('keydown', (e) => {
  if (e.target.tagName === 'INPUT') return;
  if (e.key === 'r' || e.key === 'R') { if (tool === 14) { E.rotate_tool(); E.sync_ui(); showPlacebar(); } else if (inspOpen && UI()[81] === 14) { E.rotate_sel(); E.sync_ui(); renderInspector(true); } }
  if (e.key === 'Escape') { setTool(-1); closeInspector(); if (!$('#modal-wrap').hidden && closableModal) closeModal(); }
  if (e.key === ' ') { e.preventDefault(); if (skipping) stopSkip(false); setSpeed(speed === 0 ? 1 : 0); }
  if (e.key === 'f' || e.key === 'F') startSkip();
  if (['1', '2', '3'].includes(e.key)) setRate([RATE_DEF, 30, RATE_MAX][+e.key - 1]);
  const pan = { ArrowLeft: [-24, 0], ArrowRight: [24, 0], ArrowUp: [0, -24], ArrowDown: [0, 24] }[e.key];
  if (pan) E.pan(...pan);
  if (e.key === '+' || e.key === '=') setZoom(zoom + 1);
  if (e.key === '-') setZoom(zoom - 1);
});


// ------------------------------------------------------------------ settings, music and voice
const FM = window.ForemanFM;
let settings = { music: true, vol: 0.55, voice: true, voiceName: '', sfx: true, rate: 0.97, daynight: true, tutorial: true };
try { Object.assign(settings, JSON.parse(localStorage.getItem('pg-settings') || '{}')); } catch (_) {}
const saveSettings = () => { try { localStorage.setItem('pg-settings', JSON.stringify(settings)); } catch (_) {} };
FM.setVolume(settings.vol);
FM.onSong((title) => { $('#np-t').textContent = title; if (settings.music && running) $('#np').hidden = false; });
function startAudio() { if (settings.music) { FM.start(); $('#np').hidden = false; } }
$('#np').onclick = () => FM.skip();

const synth = window.speechSynthesis;
let voices = [];
const PREFER = ['Daniel', 'Google UK English Male', 'Arthur', 'Rishi', 'Fred', 'Alex', 'Aaron', 'Microsoft Guy', 'Microsoft Ryan', 'Microsoft David', 'Google US English'];
function loadVoices() { voices = synth ? synth.getVoices().filter((v) => /^en/i.test(v.lang)) : []; }
if (synth) { loadVoices(); synth.onvoiceschanged = loadVoices; }
function pickVoice() {
  if (settings.voiceName) { const v = voices.find((x) => x.name === settings.voiceName); if (v) return v; }
  for (const n of PREFER) { const v = voices.find((x) => x.name.includes(n)); if (v) return v; }
  return voices[0] || null;
}
function speak(text) {
  if (!synth || !settings.voice) return;
  try {
    synth.cancel();
    const u = new SpeechSynthesisUtterance(text.replace(/₿/g, 'bitcoin ').replace(/°C/g, ' degrees').replace(/kW/g, ' kilowatts').replace(/J\/TH/g, 'joules per terahash'));
    const v = pickVoice(); if (v) { u.voice = v; u.lang = v.lang; }
    u.rate = settings.rate; u.pitch = 0.88;
    u.onstart = () => FM.duck(true); u.onend = u.onerror = () => FM.duck(false);
    synth.speak(u);
  } catch (_) {}
}
function stopSpeaking() { try { synth && synth.cancel(); FM.duck(false); } catch (_) {} }

function openSettings() {
  const vopts = voices.map((v) => `<option ${v.name === (pickVoice() || {}).name ? 'selected' : ''}>${v.name}</option>`).join('');
  openModal(`<div class="mh"><h2>Settings</h2><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <div class="set">
      <div class="setrow"><div><b>Tutorial</b><small>The foreman explains things as they come up, and the game waits while he talks.</small></div><button class="sw" role="switch" aria-checked="${settings.tutorial !== false}" data-k="tutorial" aria-label="Tutorial"></button></div>
      <div class="setrow"><div><b>Foreman FM</b><small>Endless lo-fi made live in your browser. Every song is new.</small></div><button class="sw" role="switch" aria-checked="${settings.music}" data-k="music" aria-label="Music"></button></div>
      <div class="setrow"><div><b>Music volume</b></div><input type="range" min="0" max="1" step="0.05" value="${settings.vol}" data-k="vol" aria-label="Music volume"></div>
      <div class="setrow"><div><b>Now playing</b><small>${FM.title || '—'}</small></div><button class="btn" data-a="skip">Next song</button></div>
      <div class="setrow"><div><b>Foreman's voice</b><small>Reads the tutorial tips out loud${synth ? '' : ' (not supported in this browser)'}.</small></div><button class="sw" role="switch" aria-checked="${settings.voice && !!synth}" data-k="voice" aria-label="Voice" ${synth ? '' : 'disabled'}></button></div>
      ${voices.length ? `<div class="setrow"><div><b>Voice</b><small>Voices come from your device.</small></div><select data-k="voiceName" aria-label="Voice">${vopts}</select><button class="btn" data-a="test">Test</button></div>` : ''}
      <div class="setrow"><div><b>Speaking speed</b></div><input type="range" min="0.7" max="1.3" step="0.05" value="${settings.rate}" data-k="rate" aria-label="Speaking speed"></div>
      <div class="setrow"><div><b>Sound effects</b><small>Build sounds, alarms, sirens and the plant's hum.</small></div><button class="sw" role="switch" aria-checked="${settings.sfx}" data-k="sfx" aria-label="Sound effects"></button></div>
      <div class="setrow"><div><b>Day and night</b><small>Nights get dark and the lights come on. Fades out at high speeds.</small></div><button class="sw" role="switch" aria-checked="${settings.daynight !== false}" data-k="daynight" aria-label="Day and night"></button></div>
    </div>`, true);
  const m = $('#modal');
  m.querySelector('[data-x]').onclick = closeModal;
  m.querySelectorAll('.sw').forEach((b) => b.onclick = () => {
    const k = b.dataset.k; settings[k] = !settings[k]; b.setAttribute('aria-checked', settings[k]); saveSettings();
    if (k === 'music') { if (settings.music) startAudio(); else { FM.stop(); $('#np').hidden = true; } }
    if (k === 'voice' && !settings.voice) stopSpeaking();
    if (k === 'daynight') applyNight();
    if (k === 'tutorial' && !settings.tutorial) { $('#memo').hidden = true; stopSpeaking(); }
  });
  m.querySelector('[data-k=vol]').oninput = (e) => { settings.vol = +e.target.value; FM.setVolume(settings.vol); saveSettings(); };
  m.querySelector('[data-k=rate]').oninput = (e) => { settings.rate = +e.target.value; saveSettings(); };
  const sel = m.querySelector('select'); if (sel) sel.onchange = (e) => { settings.voiceName = e.target.value; saveSettings(); speak("Right, kid. This is how I sound now."); };
  const t = m.querySelector('[data-a=test]'); if (t) t.onclick = () => speak("Kid, a box that costs more in power than it mines is a space heater.");
  m.querySelector('[data-a=skip]').onclick = () => { FM.skip(); setTimeout(() => { const sm = m.querySelector('[data-a=skip]'); if (sm) sm.previousElementSibling.querySelector('small').textContent = FM.title; }, 600); };
}
$('#b-music').onclick = openSettings;

// ------------------------------------------------------------------ sound effects + plant ambience (all synthesized, no files)
let actx = null, noiseBuf = null, amb = null;
function ac() {
  actx = actx || new (window.AudioContext || window.webkitAudioContext)();
  if (actx.state === 'suspended') actx.resume();
  if (!noiseBuf) { noiseBuf = actx.createBuffer(1, actx.sampleRate * 2, actx.sampleRate); const d = noiseBuf.getChannelData(0); for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1; }
  return actx;
}
function tone(type, f0, f1, t0, dur, vol, out) {
  const a = actx, o = a.createOscillator(), g = a.createGain();
  o.type = type; o.frequency.setValueAtTime(f0, t0); if (f1 !== f0) o.frequency.exponentialRampToValueAtTime(f1, t0 + dur);
  g.gain.setValueAtTime(0.0001, t0); g.gain.exponentialRampToValueAtTime(vol, t0 + 0.008); g.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
  o.connect(g).connect(out || a.destination); o.start(t0); o.stop(t0 + dur + 0.02);
}
function noise(t0, dur, vol, ftype, f, q = 1, f1) {
  const a = actx, src = a.createBufferSource(), flt = a.createBiquadFilter(), g = a.createGain();
  src.buffer = noiseBuf; src.loop = true; flt.type = ftype; flt.frequency.setValueAtTime(f, t0); flt.Q.value = q;
  if (f1) flt.frequency.exponentialRampToValueAtTime(f1, t0 + dur);
  g.gain.setValueAtTime(0.0001, t0); g.gain.exponentialRampToValueAtTime(vol, t0 + 0.01); g.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
  src.connect(flt).connect(g).connect(a.destination); src.start(t0, Math.random()); src.stop(t0 + dur + 0.02);
}
const SFX = {
  tap: (t) => tone('square', 660, 660, t, 0.05, 0.03),
  build: (t) => { noise(t, 0.18, 0.12, 'lowpass', 900); tone('square', 110, 70, t, 0.16, 0.05); tone('triangle', 220, 330, t + 0.05, 0.08, 0.03); },
  bad: (t) => { tone('square', 180, 120, t, 0.09, 0.04); tone('square', 120, 90, t + 0.1, 0.12, 0.04); },
  coin: (t) => { [988, 1319, 1976].forEach((f, i) => tone('square', f, f, t + i * 0.06, 0.12, 0.03)); },
  cash: (t) => { noise(t, 0.05, 0.08, 'bandpass', 3000, 2); [1568, 2093].forEach((f, i) => tone('triangle', f, f, t + 0.05 + i * 0.07, 0.25, 0.05)); },
  event: (t) => { tone('triangle', 330, 330, t, 0.15, 0.05); tone('triangle', 220, 220, t + 0.16, 0.25, 0.05); },
  upgrade: (t) => { tone('sawtooth', 110, 880, t, 0.45, 0.035); [523, 659, 784, 1047].forEach((f, i) => tone('square', f, f, t + 0.3 + i * 0.07, 0.1, 0.03)); },
  fence: (t) => { tone('square', 1800, 1700, t, 0.04, 0.03); tone('triangle', 420, 380, t, 0.12, 0.05); noise(t, 0.06, 0.05, 'highpass', 4000); },
  clear: (t) => { noise(t, 0.22, 0.12, 'bandpass', 1400, 1.5, 500); tone('sawtooth', 90, 60, t, 0.2, 0.025); },
  block: (t) => { [784, 988, 1175, 1568, 1175, 1568].forEach((f, i) => tone('square', f, f, t + i * 0.09, 0.16, 0.035)); tone('triangle', 196, 196, t, 0.6, 0.05); },
  siren: (t) => { for (let i = 0; i < 3; i++) { tone('sawtooth', 600, 1200, t + i * 0.5, 0.25, 0.035); tone('sawtooth', 1200, 600, t + i * 0.5 + 0.25, 0.25, 0.035); } },
  buzz: (t) => { tone('sawtooth', 60, 55, t, 0.35, 0.05); tone('square', 120, 110, t, 0.35, 0.02); },
  fire: (t) => { noise(t, 0.6, 0.12, 'bandpass', 900, 0.8, 300); for (let i = 0; i < 6; i++) noise(t + Math.random() * 0.5, 0.03, 0.15, 'highpass', 2500); },
  whoosh: (t) => noise(t, 0.4, 0.08, 'bandpass', 400, 1, 1600),
  power_off: (t) => tone('sine', 300, 60, t, 0.35, 0.06),
  power_on: (t) => tone('sine', 80, 400, t, 0.3, 0.05),
  clean: (t) => { noise(t, 0.5, 0.1, 'highpass', 2500); noise(t + 0.1, 0.4, 0.06, 'bandpass', 5000, 2); },
  thunder: (t) => { noise(t, 0.12, 0.25, 'highpass', 1500); noise(t + 0.05, 2.6, 0.32, 'lowpass', 400, 0.7, 60); noise(t + 0.4, 1.8, 0.2, 'lowpass', 160, 0.7, 50); },
  repair: (t) => { for (let i = 0; i < 3; i++) { tone('square', 900, 800, t + i * 0.09, 0.04, 0.03); noise(t + i * 0.09, 0.04, 0.06, 'highpass', 3000); } },
};
let lastSfx = {};
function sfx(kind) {
  if (!settings.sfx) return;
  try {
    const a = ac(); const now = a.currentTime;
    if (lastSfx[kind] && now - lastSfx[kind] < 0.05) return; lastSfx[kind] = now;
    (SFX[kind] || SFX.tap)(now + 0.005);
  } catch (_) {}
}
// Plant ambience: server-fan hiss + 50 Hz transformer hum that swell with the number of running boxes, and a fire crackle bed.
function ambience(u) {
  if (!settings.sfx || !running || !actx) { if (amb) { ['hum', 'hiss', 'crk', 'rain', 'wind'].forEach((k) => amb[k].gain.setTargetAtTime(0, actx.currentTime, 0.3)); } return; }
  try {
    const a = actx;
    if (!amb) {
      const mk = (ftype, f, q) => { const s = a.createBufferSource(); s.buffer = noiseBuf; s.loop = true; const fl = a.createBiquadFilter(); fl.type = ftype; fl.frequency.value = f; fl.Q.value = q; const g = a.createGain(); g.gain.value = 0; s.connect(fl).connect(g).connect(a.destination); s.start(); return g; };
      const hum = a.createGain(); hum.gain.value = 0; hum.connect(a.destination);
      [50, 100, 150].forEach((f, i) => { const o = a.createOscillator(); o.type = 'sine'; o.frequency.value = f; const g = a.createGain(); g.gain.value = [1, 0.5, 0.25][i]; o.connect(g).connect(hum); o.start(); });
      amb = { hum, hiss: mk('bandpass', 2200, 0.6), crk: mk('bandpass', 700, 0.7), rain: mk('highpass', 3500, 0.5), wind: mk('bandpass', 420, 1.6), strikes: u[291] };
    }
    const run = speed > 0 && $('#modal-wrap').hidden ? 1 : 0.35;
    const boxes = u[46], fans = u[47], fires = u[44];
    const level = Math.min(1, Math.sqrt(boxes) / 6) * run;
    amb.hum.gain.setTargetAtTime(0.012 * level, a.currentTime, 0.4);
    amb.hiss.gain.setTargetAtTime((0.01 * level + Math.min(0.02, fans * 0.004)) * run, a.currentTime, 0.4);
    amb.crk.gain.setTargetAtTime(fires > 0 ? Math.min(0.09, 0.03 + fires * 0.02) * (0.6 + Math.random() * 0.8) : 0, a.currentTime, 0.08);
    if (fires > 0 && Math.random() < 0.25) sfx('fire');
    const wx = u[284];
    amb.rain.gain.setTargetAtTime(wx === 1 ? 0.035 : wx === 2 ? 0.06 : 0, a.currentTime, 0.8);
    amb.wind.gain.setTargetAtTime(wx === 3 ? 0.07 + Math.random() * 0.04 : wx === 2 ? 0.03 : 0, a.currentTime, 0.5);
    if (u[291] > amb.strikes) { setTimeout(() => sfx('thunder'), 150 + Math.random() * 500); } amb.strikes = u[291];
  } catch (_) {}
}
addEventListener('pointerdown', () => { try { ac(); } catch (_) {} }, { once: true });

// ------------------------------------------------------------------ speed
// speed is 0 (paused) or 1 (playing); rate is how fast playing runs, in blocks a second (144 blocks make a day).
let speed = 1;
const RATE_MIN = 1, RATE_MAX = 90, RATE_DEF = 4;
let rate = RATE_DEF;
try { rate = Math.min(RATE_MAX, Math.max(RATE_MIN, +(localStorage.getItem('pg-rate') || RATE_DEF) || RATE_DEF)); } catch (_) {}
const posToRate = (v) => Math.round(RATE_MIN * (RATE_MAX / RATE_MIN) ** (v / 100) * 10) / 10;
const rateToPos = (r) => Math.round(Math.log(r / RATE_MIN) / Math.log(RATE_MAX / RATE_MIN) * 100);
const perDay = (r) => { const s = 144 / r; return s >= 60 ? (s / 60).toFixed(1) + ' min a day' : (s < 10 ? s.toFixed(1) : Math.round(s)) + ' s a day'; };
const calm = matchMedia('(prefers-reduced-motion: reduce)').matches;
// Nights strobe when a day lasts a second, so they fade out as the speed goes up.
function applyNight() { if (E.set_night) E.set_night(settings.daynight === false ? 0 : rate <= 8 ? 1 : calm ? 0 : Math.max(0, 1 - (rate - 8) / 40)); }
// Skip ahead: top speed until something needs you (an event, an alarm, a milestone, a new rung, or the cash for the next one).
let skipping = null;
function startSkip() {
  if (skipping) return stopSkip(false);
  const c = E.next_rung_cost(), u = UI();
  skipping = { prev: rate, afford: c > 0 && u[0] < c ? c : 0 };
  rate = RATE_MAX; setSpeed(1);
}
function stopSkip(pause) {
  if (!skipping) return;
  rate = skipping.prev; skipping = null;
  if (pause) setSpeed(0); else if (speed) setSpeed(1); else paintSpeed();
}
function checkSkip(u) {
  if (!skipping) return;
  if (skipping.afford && u[0] >= skipping.afford) {
    const k = E.next_rung_kind(), up = E.next_rung_upgrade();
    stopSkip(true);
    if (k >= 0) toast('You can afford the next rung', `${up ? 'Upgrade your' : 'Build a'} ${KINDS[k].name.toLowerCase()}: ${money(E.next_rung_cost())}.`, 'var(--hash)', 'unlock', 'up');
  }
}
function paintSpeed() {
  const b = $('#b-play'); b.innerHTML = ic(speed ? 'pause' : 'play'); b.setAttribute('aria-label', speed ? 'Pause' : 'Play');
  $('#spd').value = rateToPos(rate); $('#spd-t').textContent = !speed ? 'Paused' : skipping ? 'Skipping ahead' : perDay(rate);
  { const sk = $('#b-skip'); if (sk) sk.setAttribute('aria-pressed', !!skipping); }
  $('#spd').setAttribute('aria-valuetext', perDay(rate)); $('#speed').classList.toggle('paused', !speed);
}
function setSpeed(s) { speed = s ? 1 : 0; E.set_rate(speed ? rate : 0); applyNight(); paintSpeed(); }
function setRate(r) { skipping = null; rate = Math.min(RATE_MAX, Math.max(RATE_MIN, r)); try { localStorage.setItem('pg-rate', rate); } catch (_) {} setSpeed(1); }
$('#b-play').onclick = () => { if (skipping) stopSkip(false); setSpeed(speed ? 0 : 1); };
$('#b-skip').onclick = () => { sfx('tap'); startSkip(); };
$('#spd').oninput = (e) => setRate(posToRate(+e.target.value));

// ------------------------------------------------------------------ dock
let tab = 'power';
document.querySelectorAll('#tabs [data-tab]').forEach((b) => b.onclick = () => { tab = b.dataset.tab; document.querySelectorAll('#tabs [data-tab]').forEach((x) => x.setAttribute('aria-selected', x === b)); renderDock(); });
$('#b-ladder').onclick = () => openLadder();
// map overlays
let viewMask = 0, viewAuto = 0;
const applyView = () => { E.set_view(viewMask | viewAuto); document.querySelectorAll('#views [data-view]').forEach((b) => b.setAttribute('aria-pressed', !!(viewMask & +b.dataset.view))); };
document.querySelectorAll('#views [data-view]').forEach((b) => b.onclick = () => { viewMask ^= +b.dataset.view; applyView(); sfx('tap'); });
function showView(bit) { viewMask |= bit; applyView(); }
function thumb(sprName, boxH) {
  const r = SPR[sprName]; const sc = Math.min(1, (boxH - 4) / r[3], 110 / r[2]);
  return `<i style="width:${r[2]}px;height:${r[3]}px;background-position:-${r[0]}px -${r[1]}px;transform:scale(${sc});transform-origin:bottom center;margin-bottom:2px"></i>`;
}
const syncDockH = () => { const d = $('#dock'); if (d && !d.hidden) document.documentElement.style.setProperty('--dock-h', d.offsetHeight + 'px'); };
addEventListener('resize', () => setTimeout(syncDockH, 50));
function renderDock() {
  const u = UI(); const box = $('#items'); box.innerHTML = '';
  if (tab === 'power') {
    // what you own, with upgrade buttons
    const own = [];
    for (let t = 0; t < 9; t++) if (u[70 + t] > 0) own.push(t);
    if (u[566] > 0) own.push(21); if (u[568] > 0) own.push(22);
    if (own.length) {
      const c = el('div', { class: 'item owned' });
      c.innerHTML = `<span class="n">Your power</span><span class="s">${power(u[5])} capacity · ${power(u[4])} used</span>` +
        own.map((t) => `<span class="s">${KINDS[t].name} ×${t === 21 ? u[566] : t === 22 ? u[568] : u[70 + t]} · best Lv ${t === 21 ? u[567] : t === 22 ? u[569] : u[60 + t]}/3</span>`).join('') +
        (u[70] > 0 ? `<span class="s">${u[512] === 5 && !u[318] ? 'Genset: flare gas' : 'Grid: ' + CONTRACTS[u[346]].t.toLowerCase()}${u[346] === 1 ? ` · $${Math.round(u[347])}/MWh now` : ` · $${u[305]}/MWh`}</span>` : '') +
        `<button class="btn btn-p mini" data-up>${u[29] > 0 ? 'Upgrade ' + money(u[29]) : 'All maxed'}</button>` +
        (u[70] > 0 ? `<button class="btn mini" data-ct>Power contract</button>` : '');
      c.querySelector('[data-up]').onclick = (e) => { e.stopPropagation(); if (E.select_upgradable()) { E.sync_ui(); openInspector(); tip('upgrade'); } };
      const ct = c.querySelector('[data-ct]'); if (ct) ct.onclick = (e) => { e.stopPropagation(); openContract(); };
      box.appendChild(c);
    }
  }
  if (tab === 'site') {
    const c = el('div', { class: 'item owned' }); const dead = u[488] + u[489] + u[490];
    c.innerHTML = `<span class="n">Crew & spares</span><span class="s">${u[480]} technician${u[480] === 1 ? '' : 's'} · ${u[481]} guard${u[481] === 1 ? '' : 's'}${u[497] ? ` · ${money(u[497])}/day` : ''}</span>` +
      `<span class="s">Shelf: ${u[482]} boards, ${u[483]} PSUs, ${u[484]} fans</span>` + (dead ? `<span class="s" style="color:var(--btc)">${dead} failed parts in your boxes</span>` : '') +
      `<button class="btn btn-p mini" data-crew>Crew & spares</button>`;
    c.querySelector('[data-crew]').onclick = (e) => { e.stopPropagation(); openCrew(); };
    box.appendChild(c);
  }
  if (tab === 'miners') {
    const listed = [0, 1, 2, 3].filter((m) => u[430 + 10 + m] > 0).reduce((a, m) => a + u[330 + m], 0);
    const c = el('div', { class: 'item owned' });
    c.innerHTML = `<span class="n">Machine market</span><span class="s">New machines at <b>${Math.round(listPct(u))}%</b> of list</span><span class="s">${listed ? listed + ' used for sale this week' : 'No used machines this week'}</span>` +
      (u[42] > 0 ? `<span class="s">Yours would fetch ${money(u[320])}</span>` : '') + `<button class="btn btn-p mini" data-mkt>Open market</button>`;
    c.querySelector('[data-mkt]').onclick = (e) => { e.stopPropagation(); openMarket(); };
    box.appendChild(c);
  }
  const list = tab === 'power' ? LADDER.map((idx) => [KINDS[idx], idx]) : KINDS.map((k, idx) => [k, idx]); if (tab === 'site') list.push([CLEAR_ITEM, CLEAR]);
  list.forEach(([k, idx]) => {
    if (k.tab !== tab) return;
    const unlocked = idx === CLEAR || u[430 + idx] > 0;
    const b = el('button', { class: 'item' + (unlocked ? '' : ' locked') + (tool === idx ? ' on' : ''), 'aria-pressed': tool === idx });
    const cost = idx === CLEAR ? 150 : u[400 + idx];
    let sub = isPow(idx) ? power(CAP[idx][0]) + ' · ' + (idx === 0 ? `$${u[305]}/MWh ${u[512] === 5 && !u[318] ? 'flare gas' : 'contract'}. ${u[512] === 0 || u[318] ? 'Brownouts and curtailment calls.' : k.desc}` : k.desc) : k.desc;
    if (idx === 20) sub = k.desc + `<br><span class="dim">Heat sells for $${u[390].toFixed(1)}/MWh today${u[394] ? ` · yours: ${money(u[392])}/day` : ''}</span>`;
    if (isMiner(idx)) { const ec = econ(idx, u); sub = k.desc + `<br><b style="color:${ec.profit > 0 ? 'var(--cash)' : 'var(--alarm)'}">${ec.profit > 0 ? '+' : '−'}${money(Math.abs(ec.profit))}/day</b> · ${ec.profit > 0 ? 'pays back in ' + days(ec.payback) : 'loses money at $' + Math.round(ec.pp) + '/MWh'}<br><span class="dim">${u[330 + idx - 10] > 0 ? `Used ${money(u[326 + idx - 10])} · ${u[330 + idx - 10]} for sale` : 'No used ones for sale'} · real ≈ ${money(u[308 + idx - 10])}</span>`; }
    b.innerHTML = `<span class="thumb">${thumb(k.spr, innerWidth < 720 ? 44 : 52)}</span><span class="n">${k.name}</span><span class="s">${sub}</span>` +
      (unlocked ? `<span class="c">${idx === CLEAR ? 'from $150' : money(cost)}</span>` : `<span class="lockt">${ic('lock')} ${isPow(idx) ? 'Max out ' + KINDS[prevTier(idx)].name.toLowerCase() : UNLOCK_HINT[idx] || ''}</span>`);
    b.onclick = () => { if (!unlocked) { sfx('bad'); toast('Locked', isPow(idx) ? `Upgrade a ${KINDS[prevTier(idx)].name.toLowerCase()} to level 3 first.` : UNLOCK_HINT[idx] + '.', 'var(--muted)'); return; } setTool(tool === idx ? -1 : idx); };
    box.appendChild(b);
  });
  syncDockH();
}
// Put the build tool down without touching the current selection.
function dropTool() { tool = -1; E.set_tool(-1); viewAuto = 0; applyView(); $('#placebar').hidden = true; renderDock(); }
function setTool(k) {
  tool = k; E.set_tool(k); renderDock(); closeInspector();
  viewAuto = k === 14 ? 1 : k === 17 ? 2 : k === 18 ? 4 : 0; applyView();
  if (k < 0) { $('#placebar').hidden = true; return; }
  if (PAINT(k)) { showPlacebar(); tip(k === 18 ? 'fence' : 'clear'); return; }
  if (k === 14) tip('fan');
  if (k === 20) tip('greenhouse');
  if (k === 17) tip('pole');
  if (matchMedia('(pointer: coarse)').matches) { const c = E.hover_packed(); if (c < 0) E.hover(fbW >> 1, fbH >> 1); }
  showPlacebar();
}
function showPlacebar() {
  if (tool < 0) return;
  E.sync_ui(); const u = UI(); $('#placebar').hidden = false;
  const touch = matchMedia('(pointer: coarse)').matches;
  let t = `${kindOf(tool).name}`;
  if (PAINT(tool)) t += ` · <span class="dim">${tool === 18 ? 'drag to lay walls, $300 a tile' : 'drag over trees and rubble'}</span>`;
  else t += ` · <b>${money(u[38] || (isMiner(tool) ? u[(u[338] > 0 ? 326 : 322) + tool - 10] : u[400 + tool]))}</b>`;
  if (isMiner(tool) && !touch) t += ` <span class="dim">· click, or drag out a row</span>`;
  { const list = isMiner(tool) ? u[(u[338] > 0 ? 326 : 322) + tool - 10] : u[400 + tool]; if (!PAINT(tool) && u[38] > list + 1) t += ` <span class="dim">incl. ${money(u[38] - list)} to clear the land</span>`; }
  if (tool === 14) t += ` · blows ${DIRN[u[98]]}`;
  $('#pb-text').innerHTML = t;
  const pm = $('#pb-mode'); pm.hidden = !isMiner(tool);
  if (isMiner(tool)) {
    const m = tool - 10, used = u[338] > 0, n = u[330 + m];
    if (used && !n) { E.set_used(0); return showPlacebar(); }
    pm.innerHTML = `<button data-u="0" aria-selected="${!used}">New ${money(u[322 + m])}</button><button data-u="1" aria-selected="${used}" ${n ? '' : 'disabled'}>Used ${n ? money(u[326 + m]) + ` · ${n} left` : '· none'}</button>`;
    pm.querySelectorAll('[data-u]').forEach((b) => b.onclick = () => { E.set_used(+b.dataset.u); sfx('tap'); showPlacebar(); if (+b.dataset.u) tip('used'); });
  }
  $('#pb-rot').hidden = tool !== 14;
  $('#pb-ok').hidden = PAINT(tool) || !touch;
  $('#pb-cancel').textContent = 'Done';
}
$('#pb-rot').onclick = () => { E.rotate_tool(); showPlacebar(); sfx('whoosh'); };
$('#pb-ok').onclick = () => { if (E.place_hover() === 1) { if (tool === 14 || tool === 15) unlockGoal('cooling'); sfx('build'); if (tool === 9) setTool(-1); else showPlacebar(); } else sfx('bad'); };
$('#pb-mode').hidden = true;
$('#pb-cancel').onclick = () => setTool(-1);

// ------------------------------------------------------------------ inspector
let inspOpen = false, inspHtml = '';
function openInspector() { inspOpen = true; inspHtml = ''; $('#inspector').hidden = false; renderInspector(true); }
function closeInspector() { inspOpen = false; $('#inspector').hidden = true; E.select_none(); }
const bar = (pct, col) => `<span class="hbar"><i style="width:${Math.max(0, Math.min(100, pct))}%;background:${col}"></i></span>`;
function netRows(u) {
  if (u[237] < 0) return '';
  const src = u[244], cap = u[238], dem = u[239], frac = u[250];
  if (!src) return `<div class="row warn"><span>${ic('plug')} Not connected to any power. Touch a power source or run a pole (4 tiles reach).</span></div>`;
  const ok = frac >= 0.999;
  return `<div class="row"><span>Network ${u[237] + 1}</span><b style="color:${ok ? 'var(--volt)' : 'var(--alarm)'}">${power(dem)} / ${power(cap)}</b></div>` +
    (ok ? '' : `<div class="row warn"><span>Short ${power(dem - cap)}: everything on this network runs at ${Math.round(frac * 100)}%.</span><button class="btn mini" data-a="shed">Switch off worst</button></div>`);
}
function renderInspector(force) {
  const u = UI(); if (u[80] < 0) { closeInspector(); return; }
  const kind = u[81], k = KINDS[kind], lvl = u[82], off = u[242] > 0;
  let h = `<div class="ihead"><span class="px" style="font-size:12px">${k.name}</span>`;
  const sell = (label) => `<button class="btn btn-d" data-a="sell">${label} ${money(u[235])}</button>`;
  const sw = `<button class="btn" data-a="toggle">${ic('power')}${off ? 'Switch on' : 'Switch off'}</button>`;
  if (isMiner(kind)) {
    const t = u[83], broken = u[87] > 0, hp = u[234], fire = u[247] > 0, nopow = u[244] === 0 || u[250] <= 0;
    const status = broken ? ['Broken', 'var(--alarm)'] : off ? ['Switched off', 'var(--muted)'] : fire ? ['On fire', 'var(--alarm)'] : nopow ? ['No power', 'var(--alarm)'] : t > 80 ? ['Throttled (hot)', 'var(--btc)'] : u[250] < 0.999 ? ['Underpowered', 'var(--btc)'] : ['Mining', 'var(--hash)'];
    h += `<span class="tag" style="color:${status[1]}">${status[0]}</span></div>
      <div class="row"><span>Temperature</span><b style="color:${t > 85 ? 'var(--alarm)' : t > 70 ? 'var(--btc)' : 'var(--ink)'}">${broken || off ? '—' : t.toFixed(0) + ' °C'}</b></div>
      ${!broken && !off ? `<div class="row" style="font-size:12px"><span>Heat: ${u[16].toFixed(0)} °C air + box${u[48] > 0 ? ` + ${u[48]} packed neighbour${u[48] > 1 ? 's' : ''} (+${(u[48] * 4.5).toFixed(0)} °C)` : ''}</span><b style="color:${u[45] > 0.5 ? 'var(--hash)' : 'var(--muted)'}">${u[45] > 0.5 ? ic('snow') + ' −' + u[45].toFixed(0) + ' °C fans' : 'no fan air'}</b></div>` : ''}
      ${u[389] > 0 && !broken && !off ? `<div class="row" style="font-size:12px"><span>${ic('flame')} Exhaust piped to a greenhouse</span><b style="color:var(--hash)">−6 °C · sold</b></div>` : ''}
      <div class="row"><span>Health</span>${bar(hp, hp < 30 ? 'var(--alarm)' : hp < 60 ? 'var(--btc)' : 'var(--hash)')}<b>${hp.toFixed(0)}%${hp < 99 && !broken ? ` · ${Math.round(70 + 0.3 * hp)}% speed` : ''}</b></div>
      <div class="row"><span>Dust</span>${bar(u[289], u[289] > 60 ? 'var(--alarm)' : u[289] > 30 ? 'var(--btc)' : 'var(--muted)')}<b>${u[289].toFixed(0)}%${u[289] > 5 ? ` · +${(u[289] * 0.15).toFixed(0)} °C` : ''}</b></div>
      <div class="row"><span>Hashrate</span><b style="color:var(--hash)">${hash(u[84])}</b></div>
      ${broken ? '' : partsRow(kind, u)}
      ${!broken && !off && u[466] > 0.005 ? `<div class="row" style="font-size:12px"><span>Rejected shares ${whyBtn('rejects')}</span><b style="color:${u[466] > 0.02 ? 'var(--alarm)' : 'var(--ink)'}">${(u[466] * 100).toFixed(1)}%</b></div>` : ''}
      <div class="row"><span>Draw</span><b style="color:var(--volt)">${off || broken ? '0' : (+u[85].toFixed(1))} kW · ${u[93].toFixed(1)} J/TH</b></div>
      ${broken ? '' : firmwareRows(kind, u)}
      <div class="row"><span>Net / day</span><b style="color:${u[86] < 0 ? 'var(--alarm)' : 'var(--ink)'}">${u[86] < 0 ? '▼ ' : '▲ '}${money(u[86])}</b></div>
      ${(() => { const ec = econ(kind, u); return `<div class="row" style="font-size:12px"><span>Break-even power price</span><b style="color:${ec.breakeven > ec.pp ? 'var(--cash)' : 'var(--alarm)'}">$${Math.round(ec.breakeven)}/MWh <span class="dim">(you pay $${Math.round(ec.pp)})</span></b></div>
        <div class="row" style="font-size:12px"><span>New one pays back in</span><b>${days(ec.payback)}</b></div>`; })()}
      ${netRows(u)}
      ${u[249] > 0 ? `<div class="row warn"><span>${ic('shield')} Outside your fence. Raiders can steal it.</span></div>` : ''}
      <div class="acts">${broken ? `<button class="btn btn-p" data-a="repair">Repair ${money(u[236])}</button>` : sw + (hp < 99 ? `<button class="btn" data-a="repair">Service ${money(u[236])}</button>` : '') + (u[289] >= 5 ? `<button class="btn" data-a="clean">${ic('broom')}Clean $120</button>` : '') + (u[509] > 0 ? `<button class="btn btn-p" data-a="fix">${ic('wrench')}Fix parts ${money(u[509])}</button>` : '')}${sell('Sell to a broker')}</div>`;
  } else if (isPow(kind)) {
    const next = lvl < 3 && kind !== 9 ? CAP[kind][lvl] : 0;
    h += `<span class="tag">Level ${lvl}/3</span></div>
      <div class="row"><span>Capacity</span><b style="color:var(--volt)">${power(u[90])}${next ? ` → <span style="color:var(--hash)">${power(next)}</span>` : ''}</b></div>
      <div class="row"><span>Running cost</span><b>${kind === 0 && u[346] === 1 ? `$${Math.round(u[347])}/MWh spot` : u[94] ? '$' + u[94] + '/MWh' : 'free'}</b></div>
      ${kind === 0 ? `<div class="row"><span>Contract</span><button class="btn mini" data-a="contract">${CONTRACTS[u[346]].t}</button></div>` : ''}
      ${netRows(u)}
      ${lvl === 2 && kind !== 8 && kind !== 9 ? `<div class="row" style="font-size:12px;color:var(--hash)"><span>Level 3 unlocks ${KINDS[nextTier(kind)].name.toLowerCase()}.</span></div>` : ''}
      ${k.game ? `<div class="row" style="font-size:12px"><span>Not real: this rung is the game's. ${whyBtn(kind === 21 ? 'deepgeo' : kind === 22 ? 'orbital' : kind === 8 ? 'fusion' : 'site')}</span></div>` : ''}
      <div class="acts">${u[88] > 0 ? `<button class="btn btn-p big" data-a="up">${ic('up')}Upgrade to level ${lvl + 1} · ${money(u[88])}</button>` : kind !== 9 ? '<span class="tag" style="color:var(--hash)">Max level</span>' : ''}${kind !== 9 ? sell('Sell') : ''}</div>`;
  } else if (kind === 14) {
    h += `<span class="tag">${off ? 'Off' : 'Blowing ' + DIRN[u[99]].split(' ')[0]}</span></div>
      <div class="row" style="font-size:12px"><span>Cools boxes in a 4-tile lane: −11, −9, −7, −5 °C, and half that to each side. Fences and power plants block the air.</span></div>
      ${netRows(u)}
      <div class="row"><span>Dust</span>${bar(u[289], u[289] > 60 ? 'var(--alarm)' : 'var(--muted)')}<b>${u[289].toFixed(0)}% · airflow −${(u[289] / 2).toFixed(0)}%</b></div>
      <div class="acts"><button class="btn btn-p" data-a="rot">${ic('turn')}Turn (R)</button>${sw}${u[289] >= 5 ? `<button class="btn" data-a="clean">${ic('broom')}Clean $25</button>` : ''}${sell('Sell')}</div>`;
  } else if (kind === 15) {
    h += `<span class="tag">${off ? 'Off' : 'Running'}</span></div><div class="row" style="font-size:12px"><span>${k.desc}. Draws 6 kW.</span></div>${netRows(u)}<div class="acts">${sw}${sell('Sell')}</div>`;
  } else if (kind === 20) {
    const take = u[387], n = u[388];
    h += `<span class="tag" style="color:${take > 0 ? 'var(--btc)' : 'var(--muted)'}">${take > 0 ? 'Heating' : 'No heat yet'}</span></div>
      <div class="row"><span>Heat taken</span>${bar(take / u[395] * 100, 'var(--btc)')}<b>${Math.round(take)} / ${u[395]} kW</b></div>
      <div class="row"><span>Boxes piped in</span><b>${n}</b></div>
      <div class="row"><span>Heat price now ${whyBtn('heat')}</span><b>$${u[390].toFixed(1)}/MWh</b></div>
      <div class="row"><span>Pays per day</span><b style="color:var(--cash)">${money(take * 24 / 1000 * u[390])}</b></div>
      <div class="row" style="font-size:12px"><span>Boxes within 2 tiles send it their exhaust. Air-cooled boxes give half their draw as useful heat; hydro boxes, and boxes touching an immersion tank, give 85%. The grower pays most when it's cold, or when gas is dear.</span></div>
      ${u[393] > 0 ? `<div class="row"><span>Heat sold so far</span><b>${money(u[393])}</b></div>` : ''}
      <div class="acts">${sell('Sell')}</div>`;
  } else if (kind === 17) {
    h += `</div><div class="row" style="font-size:12px"><span>Links anything electric within 4 tiles. Each pole on a network adds 0.4% line loss (max 12%).</span></div>${netRows(u)}<div class="acts">${sell('Sell')}</div>`;
  } else if (kind === 18) {
    const WL = ['', 'Wooden palisade', 'Stone wall', 'Steel wall'], BR = ['', '40%', '15%', '3%'];
    h += `<span class="tag">Lv ${lvl}/3 · ${WL[lvl]}</span></div>
      <div class="row" style="font-size:12px"><span>Raiders can't walk through walls. If they can't reach a box, they try to break in: the weakest wall on your perimeter decides it.</span></div>
      <div class="row"><span>Your weakest perimeter</span><b>${u[278] ? WL[u[278]] + ' · ' + BR[u[278]] + ' breach' : 'no closed perimeter yet'}</b></div>
      <div class="row"><span>Walls</span><b>${u[279]}</b></div>
      <div class="acts">${u[88] > 0 ? `<button class="btn btn-p" data-a="up">${ic('up')}This one · ${money(u[88])}</button>` : ''}${u[281] > 0 ? `<button class="btn btn-p" data-a="walls">${ic('up')}All Lv ${u[281]} walls (${u[282]}) · ${money(u[282] * u[283])}</button>` : '<span class="tag" style="color:var(--hash)">All walls maxed</span>'}${sell('Remove')}</div>`;
  } else if (kind === 19) {
    h += `</div><div class="row" style="font-size:12px"><span>NO TRESPASSING. Private property, monitored. Each sign (up to 4) makes raids 12% less likely. You have ${u[280]}.</span></div><div class="acts">${sell('Remove')}</div>`;
  } else {
    h += `</div><span style="font-size:13px;color:var(--muted)">Your first shed. It stays.</span>`;
  }
  if (!force && h === inspHtml) { positionInspector(); return; }
  inspHtml = h;
  const box = $('#inspector'); box.innerHTML = h;
  box.querySelectorAll('[data-a]').forEach((b) => b.onclick = () => {
    const a = b.dataset.a;
    if (a === 'up') { E.upgrade_sel(); sfx('upgrade'); }
    if (a === 'repair') { E.repair_sel(); sfx('repair'); }
    if (a === 'toggle') { sfx(u[242] > 0 ? 'power_on' : 'power_off'); E.toggle_sel(); }
    if (a === 'rot') { E.rotate_sel(); sfx('whoosh'); }
    if (a === 'shed') { E.shed(); }
    if (a === 'walls') { E.upgrade_walls(); sfx('upgrade'); }
    if (a === 'clean') { E.clean_sel(); sfx('clean'); }
    if (a === 'sell') { E.sell_sel(); sfx('cash'); closeInspector(); return; }
    if (a === 'contract') { openContract(); return; }
    if (a === 'fix') { E.fix_sel(); sfx('repair'); }
    if (a === 'crew') { openCrew(); return; }
    if (a === 'tune') { if (E.tune_sel(+b.dataset.t)) { sfx(+b.dataset.t < u[342] ? 'power_off' : 'power_on'); tip('firmware'); } }
    if (a === 'tunesame') { const n = E.tune_same(u[342]); if (n) { sfx('upgrade'); toast('Firmware', `${n} more ${SHORT[u[81]] || 'boxes'} set to ${TUNE.name[u[342] + 2]}.`, 'var(--hash)', 'firmware', 'chip'); } }
    E.sync_ui(); renderInspector(true);
  });
  positionInspector();
}
function positionInspector() {
  const u = UI(); const box = $('#inspector'); const r = canvas.getBoundingClientRect();
  if (innerWidth < 720) { box.style.left = '10px'; box.style.right = '10px'; box.style.width = 'auto'; box.style.top = 'auto'; box.style.bottom = 'calc(var(--dock-h,170px) + 64px + env(safe-area-inset-bottom,0px))'; return; }
  const x = u[91] * r.width / fbW, y = u[92] * r.height / fbH;
  box.style.left = Math.min(innerWidth - 272, Math.max(10, x + 30)) + 'px';
  box.style.top = Math.max(64, Math.min(innerHeight - box.offsetHeight - $('#dock').offsetHeight - 24, Math.max(80, y - 70))) + 'px';
}

// ------------------------------------------------------------------ toasts
const TOAST_IC = { 'var(--alarm)': 'warn', 'var(--cash)': 'check', 'var(--btc)': 'cube', 'var(--hash)': 'bolt', 'var(--muted)': 'help', 'var(--volt)': 'bolt' };
function toast(title, body, color = 'var(--volt)', whyKey = '', icon = '') {
  if (skipping && (color === 'var(--alarm)' || title === 'New power source')) stopSkip(color === 'var(--alarm)');
  const t = el('div', { class: 'toast', style: `--tc:${color}` }, `<span class="ti">${ic(icon || TOAST_IC[color] || 'bolt')}</span><div><b>${title}</b><span>${body}</span></div>${whyKey ? whyBtn(whyKey) : ''}`);
  $('#toasts').prepend(t); while ($('#toasts').children.length > (innerWidth < 720 ? 2 : 3)) $('#toasts').lastChild.remove();
  setTimeout(() => t.remove(), 6500);
}
function drainToasts(u) {
  const n = u[100]; if (!n) return;
  for (let x = 0; x < n; x++) {
    const code = u[101 + x * 2], arg = u[102 + x * 2];
    switch (code) {
      case 1: toast('Difficulty retarget', `Network difficulty ${arg >= 0 ? '+' : ''}${(arg * 100).toFixed(1)}%. ${arg > 0 ? 'Each miner now earns less.' : 'Your miners earn a bit more.'}`, arg > 0 ? 'var(--alarm)' : 'var(--cash)', 'retarget', 'gauge'); tip('retarget'); break;
      case 2: toast('Foreman bailout', `The Foreman covered you with ${money(arg)}. Don't make it a habit.`, 'var(--cash)', 'bailout', ''); break;
      case 3: toast('Unpaid bills', arg > 0 ? `${arg} more day${arg === 1 ? '' : 's'} in the red and the plant is repossessed.` : 'Out of time.', 'var(--alarm)', 'unpaid', ''); sfx('bad'); break;
      case 4: sfx('upgrade'); toast('New power source', `${KINDS[arg].name} unlocked on the ladder.`, 'var(--hash)', 'unlock', 'up'); sfx('coin'); renderDock(); tip('ladder'); break;
      case 5: sfx('bad'); toast('Miner died', 'A box ran out of health. Repair it, or sell it to a broker for parts.', 'var(--alarm)', 'dead', 'x'); tip('heat'); break;
      case 6: { const ev = EVENTS[arg]; toast('Random event · ' + ev.t, ev.d, ev.c, 'event', ''); sfx('event'); break; }
      case 7: toast('Not enough cash', 'Sell some ₿ or wait for income.', 'var(--alarm)'); sfx('bad'); break;
      case 8: toast("Can't build there", 'Needs open land: no water, buildings or pads.', 'var(--alarm)'); sfx('bad'); break;
      case 9: toast('Locked', 'Max out the previous power source first.', 'var(--muted)'); break;
      case 10: toast('Needs water', 'A dam has to touch a lake tile.', 'var(--alarm)'); sfx('bad'); break;
      case 11: toast('Not yet', 'The free energy core needs every requirement on the ladder.', 'var(--alarm)'); break;
      case 12: toast('Core collapsed', `Rolled ${arg}. The field destabilised. Try again in 30 days.`, 'var(--alarm)'); break;
      case 14: sfx('buzz'); toast('Power shortage', `Short ${power(arg)}. Everything on that network slows down and brownouts wear PSUs.`, 'var(--alarm)', 'shortage', 'bolt'); tip('power'); break;
      case 15: toast('Repaired', 'Back online.', 'var(--cash)'); break;
      case 16: toast('Sold bitcoin', `+${money(arg)} cash.`, 'var(--cash)'); sfx('cash'); break;
      case 17: toast('Black market', `A buyer took it for ${money(arg)}, no questions asked.`, 'var(--btc)'); break;
      case 18: toast('Block found', `Your pool hit a block and paid you the lot: ${btcf(arg)}.`, 'var(--btc)', 'block', 'cube'); sfx('block'); break;
      case 19: toast('No power here', 'It isn\'t wired to a power source. Place it touching one, or run a power pole.', 'var(--alarm)', 'unpowered', 'plug'); sfx('bad'); tip('pole'); break;
      case 20: toast('Raid', `Thieves cut through and drove off with ${money(arg)} of miners. Fence your plant in.`, 'var(--alarm)', 'raid', 'shield'); sfx('siren'); tip('raid'); break;
      case 21: toast('Raid repelled', arg === 9 ? 'Your guards saw them coming. They left with nothing.' : arg ? `Thieves tried your ${['', 'wooden', 'stone', 'steel'][arg]} walls and gave up.` : 'Thieves looked around and found nothing worth taking.', 'var(--cash)'); break;
      case 22: if (arg) sfx('power_off'); toast(arg ? 'Load shed' : 'Nothing to shed', arg ? `Switched off ${arg} of your least efficient boxes. The rest run at full speed.` : 'No network can be fixed by switching boxes off.', 'var(--volt)'); break;
      case 24: toast('Wall breached', `Raiders smashed through your weakest wall and took ${money(arg)} of miners. Upgrade your walls.`, 'var(--alarm)', 'raid', 'shield'); sfx('siren'); break;
      case 25: toast('Walls upgraded', `${arg} wall${arg === 1 ? '' : 's'} reinforced.`, 'var(--cash)'); break;
      case 26: toast('Lightning strike', arg ? 'A bolt hit a miner box and set it on fire.' : 'A bolt hit a miner box. Its health took a beating.', 'var(--alarm)', 'lightning', 'storm'); break;
      case 27: { const W = [['Skies cleared', 'Back to normal.', 'var(--cash)'], ['Rain', 'Cooler air (−4 °C), weak solar. Rain rinses some dust off.', 'var(--hash)'], ['Thunderstorm', 'Turbines lock, solar is near zero, and lightning can hit your boxes.', 'var(--alarm)'], ['Dust storm', 'Grit clogs filters fast. Dusty boxes run hot. Clean them after.', 'var(--btc)']][arg]; toast(W[0], W[1], W[2]); if (arg === 3) tip('dust'); if (arg === 2) tip('storm'); break; }
      case 28: toast('Cleaned', `${arg} ${arg === 1 ? 'box' : 'boxes'} blown out with compressed air.`, 'var(--cash)'); break;
      case 29: toast('You found a block, solo', `A whole block: ${btcf(arg)}. No pool, no fee.`, 'var(--btc)', 'block', 'cube'); sfx('block'); rememberBlock(u, arg); pendingLab = arg; setTimeout(() => tipNudge('A solo block', 'Real solo miners wait years for one. If you enjoyed finding it, the tip jar is open.'), 4000); break;
      case 30: toast(arg ? 'Mining solo' : 'Back in the pool', arg ? 'No more steady payouts. You get paid only when you find a block yourself.' : 'Steady payouts every block for your share, minus 2%.', 'var(--btc)'); if (arg) tip('solo'); break;
      case 23: toast('Nothing to clear', 'Only trees and rubble can be cleared.', 'var(--muted)'); break;
      case 31: toast('Fee spike', `The mempool is jammed: fees ×${arg.toFixed(arg < 10 ? 1 : 0)}. Every block now pays ${btcf(u[297] + u[23])}. Fees are paid on top of the block reward.`, 'var(--btc)', 'fees', 'stack'); sfx('coin'); tip('fees'); break;
      case 32: toast('Curtailment credit', `The grid paid you ${money(arg)} for switching off. Grid power is back.`, 'var(--cash)', 'curtail', 'power'); sfx('cash'); break;
      case 33: toast('Spot price bill', arg > 1 ? `Running through the spike cost ${money(arg)} extra in power.` : 'The spike passed. You barely drew from the grid.', arg > 1 ? 'var(--alarm)' : 'var(--muted)', 'spot', 'bolt'); break;
      case 35: toast('Loan payment', `${money(arg)} went to the lender, whatever bitcoin is worth today.`, 'var(--alarm)', 'unpaid', 'warn'); sfx('cash'); break;
      case 36: toast('None for sale', `No used ${SHORT[arg] || 'machines'} listed this week. New listings come every 7 days.`, 'var(--muted)', 'market', 'chip'); sfx('bad'); break;
      case 37: toast('Miners are selling', `${arg} used machines listed this week. Margins are thin, so miners are selling machines that no longer pay.`, 'var(--btc)', 'capitulation', 'chip'); break;
      case 38: toast('Sold to a broker', `${money(arg)} after their 12% cut.`, 'var(--cash)', 'market', 'chip'); sfx('cash'); unlockGoal('flip'); break;
      case 39: toast('Price spike', `Spot power hit ${money(arg)}/MWh for the next 6 hours. ${u[348] <= 0 ? 'Boxes that can\'t cover it switched themselves off.' : u[348] < arg ? 'Your price limit switched the grid feed off.' : 'You set no limit that low, so everything keeps running at that price.'}`, u[348] > 0 && u[348] >= arg ? 'var(--alarm)' : 'var(--btc)', 'spot', 'bolt'); sfx(u[348] > 0 && u[348] >= arg ? 'bad' : 'power_off'); break;
      case 40: toast('Grid call', `You're enrolled in demand response, so the grid switched you off for 6 hours. You'll be paid ${money(arg)}.`, 'var(--hash)', 'contract', 'power'); sfx('power_off'); break;
      case 41: toast('New power contract', `${CONTRACTS[arg].t} from today. It runs at least 30 days.`, 'var(--volt)', 'contract', 'bolt'); break;
      case 42: toast('Contract locked', arg > 0 ? `Your contract runs ${arg} more day${arg === 1 ? '' : 's'}.` : 'Not in the middle of a grid call.', 'var(--muted)', 'contract'); sfx('bad'); break;
      case 43: toast('New pool', `You're mining with the ${PAYPOOLS[arg].t.charAt(0).toLowerCase() + PAYPOOLS[arg].t.slice(1)}: ${PAYPOOLS[arg].pplns ? 'paid when it finds a block' : 'paid every block'}, ${(PAYPOOLS[arg].fee * 100).toFixed(1)}% fee.`, 'var(--hash)', PAYPOOLS[arg].pplns ? 'pplns' : 'stale', 'stack'); break;
      case 44: toast('Parts failing', `${arg} ${arg === 1 ? 'part' : 'parts'} failed today${u[480] > 0 ? ' and the shelf is empty' : ', and nobody is on site to swap them'}. Hashrate drops until they're replaced.`, 'var(--btc)', 'parts', 'wrench'); tip('parts'); break;
      case 45: toast('Contractor came out', `${money(arg)} for the call-out and parts. Spares on your shelf were used first.`, 'var(--muted)', 'parts', 'wrench'); break;
      case 46: toast('Tariff hike', `${u[512] === 0 ? 'Your supplier' : 'The utility'} raised what miners pay: now $${arg}/MWh.`, 'var(--alarm)', 'tariff', 'bolt'); sfx('bad'); break;
      case 47: toast(arg ? 'Dry season' : 'The rains are back', arg ? 'Rivers are low, so the state halves the power it sells to miners until the rains come.' : 'Rivers are full again. Your whole allocation is back.', arg ? 'var(--btc)' : 'var(--hash)', 'dry', 'drop'); break;
      case 48: toast('No power market here', 'Spot and demand-response contracts need a real-time power market. Here you buy from one supplier at a fixed price.', 'var(--muted)', 'site'); sfx('bad'); break;
      case 49: toast('No more grid here', 'The utility allots a site four connections. For more power, climb the ladder and make your own.', 'var(--muted)', 'unlock'); sfx('bad'); break;
      case 50: { const what = E.milestone_info(arg, 0), target = E.milestone_info(arg, 1); toast('Milestone reached', `${what ? (target > 1 ? 'Second' : 'First') + ' halving survived' : hash(target) + ' switched on'}. Your backers release <b>${money(E.milestone_info(arg, 2))}</b>.`, 'var(--cash)', '', 'check'); sfx('coin'); stopSkip(false); break; }
      case 34: toast('Fees back to normal', 'The mempool cleared. Blocks pay the usual fees again.', 'var(--muted)'); break;
    }
  }
  E.toasts_ack();
}

// ------------------------------------------------------------------ sites (same order as SITES in data.rs)
const SITES_UI = [
  { key: 'texas', t: 'Texas', short: 'Texas', power: '$35/MWh, or spot', climate: '18–30 °C', risks: 'Heatwaves, grid calls, price spikes',
    real: 'Texas became the biggest mining state after 2021: a deregulated grid (ERCOT) with plenty of cheap wind and gas, few rules for big loads, and a market that pays flexible users to switch off. Its weak spot is the weather. Summer heat and winter storms push power prices to the cap.',
    game: 'The game as it has always been: fixed, spot and demand-response contracts, curtailment calls and hot summers.' },
  { key: 'iceland', t: 'Iceland', short: 'Iceland', power: '$35/MWh geothermal', climate: '−2–10 °C', risks: 'Storms, remote, tariff rises',
    real: 'Geothermal and hydro power and cold air made Iceland an early home for mining: machines barely need cooling, and their heat is welcome. But the island\'s power is finite. The national power company has turned away new mining contracts and cut supply when reservoirs ran low.',
    game: 'Cold air keeps boxes cool without fans, and a greenhouse pays well. No spot market or curtailment income. Storms are common and the site is remote (contractors +40%).' },
  { key: 'paraguay', t: 'Paraguay', short: 'Paraguay', power: '$26/MWh hydro', climate: '19–29 °C', risks: 'Brownouts, theft, tariff hikes',
    real: 'The Itaipú dam gives Paraguay far more electricity than it uses, and miners followed. So did problems: illegal connections and power theft, a strained grid in summer, and repeated proposals in Congress to raise miners\' tariffs or ban mining outright.',
    game: 'Cheap power, but brownouts and raids are about 80% more common, tariff hikes come now and then, and machines cost 10% more to import. Seasons are reversed: the run starts in summer.' },
  { key: 'ethiopia', t: 'Ethiopia', short: 'Ethiopia', power: '$28/MWh state hydro', climate: '16–20 °C', risks: 'Outages, import duty, remote',
    real: 'As the Grand Ethiopian Renaissance Dam came online, the state utility sold power to miners at a few cents per kilowatt-hour, and hashrate there grew fast. The grid is less reliable, imported machines face duties and long shipping, and the government has signalled limits on new mining power contracts.',
    game: 'Cheap power and mild highland weather, but brownouts are 2.5 times as common, machines cost 25% more, contractors 60% more, and the tariff can rise.' },
  { key: 'bhutan', t: 'Bhutan', short: 'Bhutan', power: '$28/MWh, half in winter', climate: '5–19 °C', risks: 'Dry-season cuts, very remote',
    real: 'Bhutan\'s state investment arm mines bitcoin with surplus hydro power, partnering with a large mining company. Hydro output falls in the dry winter, when the country needs its power at home, so mining runs hardest in the wet summer.',
    game: 'In the dry season (roughly November to March) your grid allocation is halved. Little crime and few events, but very remote: contractors cost 80% more.' },
  { key: 'dakota', t: 'North Dakota', short: 'N. Dakota', power: '$20/MWh flare gas', climate: '−9–21 °C', risks: 'The well declines, no market',
    real: 'Oil wells in North Dakota\'s Bakken produce gas that often can\'t reach a pipeline, so it\'s burned in a flare. Companies put generators and mining containers at the well and turn that gas into power instead, cutting waste and emissions. The catch: a shale well\'s output falls fast, so the mining moves on.',
    game: 'The cheapest power in the game, from a genset on the well. Its output falls about 0.3% a day, so the site shrinks unless you climb the ladder. No market and no curtailment calls; contractors cost double.' },
];
const GRID_KIND = { ...KINDS[0] };
// The grid hookup is a flare-gas genset at the well, and a state utility line elsewhere.
// ---- several sites at once
function openSites() {
  E.sync_ui(); const u = UI(); const n = u[562], act = u[563];
  const owned = []; for (let k = 0; k < n; k++) { const o = 520 + k * 7; owned.push({ k, site: u[o], th: u[o + 1], used: u[o + 2], cap: u[o + 3], net: u[o + 4], alerts: u[o + 5], miners: u[o + 6] }); }
  const total = owned.reduce((a, s) => a + s.net, 0);
  const have = new Set(owned.map((s) => s.site));
  openModal(`<div class="mh"><div><h2>Your sites</h2><span class="dim">${fmtDate(runDate(u))} · one wallet, one bitcoin network</span></div><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <p class="lead">Every site runs at the same time. The one on screen is the one you're managing; at the others, a site manager handles events for you. ${whyBtn('sites')}</p>
    <table class="mtable"><thead><tr><th scope="col">Site</th><th scope="col">Boxes</th><th scope="col">Hashrate</th><th scope="col">Power</th><th scope="col">Net/day</th><th scope="col"></th></tr></thead><tbody>
      ${owned.map((s) => `<tr><th scope="row"><b>${SITES_UI[s.site].t}</b><small>${s.alerts ? `<span style="color:var(--btc)">${s.alerts} need${s.alerts === 1 ? 's' : ''} attention</span>` : SITES_UI[s.site].power}</small></th><td class="num">${s.miners}</td><td class="num">${hash(s.th)}</td><td class="num">${power(s.used)} / ${power(s.cap)}</td><td class="num ${s.net < 0 ? 'neg' : ''}">${s.net < 0 ? '−' : '+'}${money(Math.abs(s.net))}</td><td>${s.k === act ? '<span class="dim">On screen</span>' : `<button class="btn mini" data-go="${s.k}">Go</button>`}</td></tr>`).join('')}
    </tbody></table>
    ${n > 1 ? `<div class="rows"><div class="row tot"><span>All sites, per day</span><b class="num">${total < 0 ? '−' : '+'}${money(Math.abs(total))}</b></div></div>` : ''}
    <h3>Open a new site · ${money(u[564])}</h3>
    <p class="note">Land, permits and a connection. It starts like your first: a hookup, a shed and one box, on its own map, with its own power, weather and risks. Your ladder progress comes with you.</p>
    <div class="diffs sites">${SITES_UI.map((st, i) => have.has(i) ? '' : `<button class="btn" data-open="${i}" ${u[0] < u[564] ? 'disabled' : ''}><span class="px" style="font-size:13px">${st.t}</span><small>${st.power} · ${st.climate}</small><small class="risk">${st.risks}</small></button>`).join('')}</div>`, true);
  const m = $('#modal'); m.classList.add('wide');
  m.querySelector('[data-x]').onclick = closeModal;
  m.querySelectorAll('[data-go]').forEach((b) => b.onclick = () => { E.switch_site(+b.dataset.go); closeModal(); closeInspector(); E.sync_ui(); renderDock(); statusHtml = ''; sfx('whoosh'); toast(SITES_UI[UI()[512]].t, 'You\'re managing this site now.', 'var(--volt)', 'sites'); });
  m.querySelectorAll('[data-open]').forEach((b) => b.onclick = () => {
    const r = E.open_site(+b.dataset.open); E.sync_ui(); drainToasts(UI());
    if (r === 1) { closeModal(); closeInspector(); renderDock(); statusHtml = ''; sfx('build'); unlockGoal('world2'); toast('New site: ' + SITES_UI[+b.dataset.open].t, 'Your second map. Same wallet, same network, different everything else.', 'var(--hash)', 'sites'); tip('sites'); }
  });
}
function applySite(u) {
  const site = u[318] ? 0 : u[512]; if (applySite.last === site) return; applySite.last = site;
  KINDS[0].name = site === 5 ? 'Flare-gas genset' : GRID_KIND.name;
  KINDS[0].desc = site === 5 ? 'Burns the well\'s flare gas. Output falls as the well declines.' : site === 0 ? GRID_KIND.desc : 'Power from the state utility on a fixed contract.';
  renderDock();
}

// ------------------------------------------------------------------ first steps: the first five minutes, as a checklist
const STEPS = [
  ['look', 'Tap your box to see what it earns', (u) => u[80] >= 0 && u[81] >= 10 && u[81] <= 13],
  ['second', 'Build a second box (Miners tab)', (u) => u[42] >= 2],
  ['upgrade', 'Upgrade the grid hookup (Power tab)', (u) => u[60] >= 2],
  ['sell', 'Sell some bitcoin to pay for power (tap ₿)', () => !!ledger.sell],
  ['ledger', 'Open the ledger to read what you\'ve learned', () => !!steps.ledger],
];
let steps = {};
try { steps = JSON.parse(localStorage.getItem('pg-steps') || '{}') || {}; } catch (_) { steps = {}; }
const saveSteps = () => { try { localStorage.setItem('pg-steps', JSON.stringify(steps)); } catch (_) {} };
function stepsHtml(u) {
  if (settings.tutorial === false || steps.off || u[318] || !running || !seenTips.grow || !$('#memo').hidden) return '';
  let changed = false; for (const [id, , ok] of STEPS) if (!steps[id] && ok(u)) { steps[id] = 1; changed = true; }
  if (changed) saveSteps();
  const done = STEPS.filter(([id]) => steps[id]).length;
  if (done === STEPS.length) { if (!steps.cheered) { steps.cheered = 1; steps.off = 1; saveSteps(); setTimeout(() => toast('First steps done', 'That\'s the loop: mine, pay for power, grow, climb the ladder. The rest is up to you.', 'var(--cash)', '', 'check'), 50); } return ''; }
  return `<div class="steps"><div class="sh"><b>First steps</b><span class="dim">${done} of ${STEPS.length}</span><button class="ib mini" data-s="steps-x" aria-label="Hide first steps">${ic('x')}</button></div>` +
    STEPS.map(([id, t]) => `<div class="stp ${steps[id] ? 'done' : ''}">${ic(steps[id] ? 'check' : 'pause')}<span>${t}</span></div>`).join('') + '</div>';
}

// Backers' milestones: the next target, what it pays and how close you are.
function milestoneHtml() {
  if (!running || !E.milestone_next) return '';
  const m = E.milestone_next(); if (m < 0 || hushed['ms' + m]) return '';
  const what = E.milestone_info(m, 0), target = E.milestone_info(m, 1), pct = Math.floor(E.milestone_progress() * 100);
  const label = what ? `Survive the ${target > 1 ? 'second' : 'first'} halving` : `Switch on ${hash(target)}`;
  return `<div class="steps ms"><div class="sh"><b>Next milestone</b><span class="dim">pays <b style="color:var(--cash)">${money(E.milestone_info(m, 2))}</b></span><button class="ib mini" data-dx="ms${m}" aria-label="Hide this milestone">${ic('x')}</button></div><div class="stp"><span>${label}</span>${what ? '' : bar(pct, 'var(--hash)') + `<span class="dim num">${pct}%</span>`}</div></div>`;
}

// ------------------------------------------------------------------ Foreman tips (the learning curve)
const TIPS = {
  sites: "Two sites, one wallet. Whatever you spend here comes out of the same pot. The globe up top switches between them, and shows which one needs you.",
  parts: "Boxes don't just die, they bleed: a fan here, a board there. Keep a few spares on the shelf and a technician on the payroll, or call a contractor for each box. A dead power supply takes a whole machine down, so fix those first.",
  greenhouse: "Put it right up against your boxes. Anything within 2 tiles pipes its heat over and runs cooler. It pays best on cold days. In a Texas August, nobody wants your heat.",
  spot: "Spot power's cheap at night and dear on hot afternoons. Leave it on break-even and each box decides for itself: if an hour of power costs more than it mines, it sits out.",
  firmware: "Underclock when a box barely covers its power: it loses some hashrate but saves more power. Overclock when hashprice is high and power's cheap, and watch the heat. Past 80 °C a box throttles itself anyway.",
  used: "Used boxes are cheap for a reason: somebody ran them hard. They come at half to four-fifths health, slower and dusty. Service them and they're as good as new.",
  start: "Kid, that grey box is your first miner. It turns electricity into guesses at the next block. Drag to look around, tap things to inspect them.",
  hashrate: "Top left: cash, your bitcoin, and net per day. If net goes red, you're paying more for power than you mine. Tap ₿ to sell some.",
  grow: "Your grid hookup gives 250 kilowatts. One container of S19s pulls about 31. To get more, tap the hookup and press Upgrade. Level 3 unlocks diesel. That's the ladder.",
  power: "Power shortage. The bar up top has two fixes: switch off your worst boxes, or upgrade a power source. Don't leave it short: brownouts wear out power supplies.",
  upgrade: "That's the upgrade card. Each level adds capacity. Level 3 unlocks the next rung of the ladder.",
  pole: "Power only flows through things that touch, or through poles. A pole reaches 4 tiles. Turn on the Grid view to see which boxes are on which network.",
  fan: "Fans blow one way, 4 tiles deep. Press R or the turn button to aim them. Turn on Airflow to see the wind.",
  fence: "Drag to lay walls. Raiders walk in from the edge of the map. If they can reach a box, it's at risk. Close the loop, then upgrade: wood, stone, steel. Security view shows red for exposed.",
  clear: "Drag over trees and rubble to clear them. Building on them costs the same anyway.",
  dust: "Dust storm. Grit clogs the filters and dusty boxes run hotter. When it passes, clean them: the alert bar has a Clean all button.",
  storm: "Thunderstorm. Lightning goes for tall things first: poles, turbines, the grid hookup. Boxes can get hit too. Keep some cash for repairs.",
  solo: "Solo mining. Same expected income as the pool, minus nothing, but it all arrives at once, or never. Real home miners do this as a lottery ticket. Most businesses can't wait years for a payday.",
  raid: "We got raided. Draw a fence all the way round. One gap and they walk in.",
  retarget: "That was a retarget. Every 2016 blocks the network adjusts difficulty to keep blocks 10 minutes apart. More miners out there means less ₿ for you.",
  heat: "Packed boxes heat each other. They glow orange from 70, red past 85, and can catch fire past 95. Space them out, aim fans at them, or switch some off.",
  ladder: "New rung on the ladder. Cheaper power means older miners stay profitable longer. Climb to fusion, then the core.",
  fees: "Fee spike. People are bidding to get their transactions into the next block, and the winner of each block keeps the fees. After every halving, fees are a bigger part of what we earn.",
  curtail: "Curtailment. When the grid's short, it pays big users to switch off. Sometimes that's worth more than mining the power. Real Texas miners live on deals like this.",
  ledger: "New card in your ledger. Each one is a real piece of how mining works. Tap the book up top to read it.",
  halving: "Halving. The block reward just got cut in half and your bills didn't. Efficient miners and cheap power are all that matter now.",
};
let seenTips = {}, memoResume = false;
const CHAIN = ['start', 'hashrate', 'grow'];
// One thing at a time: while the foreman talks, the game waits.
function tip(key) {
  if (settings.tutorial === false || seenTips[key] || !$('#modal-wrap').hidden) return;
  if (speed) { memoResume = true; setSpeed(0); }
  seenTips[key] = 1; try { localStorage.setItem('pg-tips', JSON.stringify(seenTips)); } catch (_) {}
  const m = $('#memo'); m.hidden = false;
  const ci = CHAIN.indexOf(key); const next = ci >= 0 && ci < CHAIN.length - 1 ? CHAIN[ci + 1] : null;
  m.innerHTML = `<b>Foreman:</b> ${TIPS[key]}<div class="foot">${ci >= 0 ? `<span class="step">${ci + 1} of ${CHAIN.length}</span>` : ''}<button class="btn say" aria-label="Read it again">${ic('speaker')}</button><button class="btn" data-ok>${next ? 'Next' : 'Got it'}</button></div>`;
  m.querySelector('.say').onclick = () => speak(TIPS[key]);
  m.querySelector('[data-ok]').onclick = () => { m.hidden = true; stopSpeaking(); if (next) setTimeout(() => tip(next), 250); else if (memoResume) { memoResume = false; setSpeed(1); } };
  speak(TIPS[key]);
}


// ------------------------------------------------------------------ the ledger: 13 goals, each unlocks a Real / Game card
// Cards stay unlocked across runs: it's a record of what you've learned, not of one plant.
const GOALS = [
  { id: 'payout', ch: 'Basics', t: 'First payout', goal: 'Get paid by your pool.', ok: (u) => u[1] > 0,
    real: 'Almost every miner joins a pool. Under FPPS (full pay per share), the pool pays you for every valid share you submit, at the expected value of the block reward plus fees, whether or not the pool finds a block. Pools charge roughly 1–4% and usually pay out once a day.',
    game: 'You are paid every block for your share of the network\'s hashrate, minus a 2% pool fee.' },
  { id: 'lab', ch: 'Basics', t: 'Proof of work', goal: 'Mine a block by hand in the hash lab.', flag: true,
    real: 'A block header is 80 bytes. Miners run it through SHA-256 twice and check whether the result is below the target, changing the nonce (and the extranonce in the coinbase transaction) billions of times a second. At today\'s difficulty a block takes about 6 × 10²³ hashes on average.',
    game: 'The lab runs the real algorithm on a real-format header, slowed to browser speed with a much easier target.' },
  { id: 'petahash', ch: 'Basics', t: 'Ten petahash', goal: 'Reach 10 PH/s of hashrate.', ok: (u) => u[11] >= 10000,
    real: '10 PH/s is 10¹⁶ hashes a second, about fifty Antminer S21s. Against a network of roughly 950 EH/s that is about one hundred-thousandth of the work: mining solo you would expect one block every year and a half or so, and a pool pays you a sliver of every block instead.',
    game: 'Same maths. The Treasury shows your real share and your solo odds.' },
  { id: 'retarget', ch: 'Network', t: 'Difficulty retarget', goal: 'Live through a difficulty adjustment.', ok: (u) => u[292] >= 1,
    real: 'Every 2016 blocks (about two weeks) the network resets difficulty so blocks keep averaging 10 minutes. Hashrate follows profit: when mining pays well, more machines get plugged in and difficulty climbs; when it doesn\'t, old machines switch off.',
    game: 'Retargets keep the real schedule. In live runs the first one follows mempool.space\'s estimate. Later ones are simulated from profitability, with hardware getting a little better every fortnight.' },
  { id: 'efficient', ch: 'Economics', t: 'Joules per terahash', goal: 'Run an Antminer S21 or better.', ok: (u) => u[314] + u[315] > 0,
    real: 'Efficiency decides who survives. An S19j Pro burns about 29.5 J/TH, an S21 17.5, an S21 XP Hydro about 12. At $35/MWh, power for one TH/s costs about 2.5¢ a day on an S19j Pro and 1.5¢ on an S21. When hashprice falls below that, the old box is a space heater.',
    game: 'Hashrate and power draw are real Bitmain specs. Prices are about a sixth of real: a 10-pack of S21s really costs around $22,000.' },
  { id: 'used', ch: 'Economics', t: 'Second-hand', goal: 'Buy a used machine.', ok: (u) => u[339] > 0,
    real: 'ASIC prices follow hashprice. When mining pays, machines sell out and buyers wait months for delivery; when it doesn\'t, miners sell what no longer earns its power, and used prices crash. An S19j Pro sold for around $10,000 in late 2021 and well under $2,000 a year later. Old, power-hungry models fall hardest, because their margin disappears first. Miners with cheap power buy them up.',
    game: 'Prices follow your run\'s hashprice, smoothed over a few days. New machines move about three-quarters as much as hashprice. Used ones are priced on what they earn for a buyer paying $35/MWh, since cheap-power miners are who buys old machines. Listings refresh every 7 days, with more when margins are thin.' },
  { id: 'flip', ch: 'Economics', t: 'Selling into the market', goal: 'Sell a machine to a broker.', flag: true,
    real: 'A miner\'s machines are often worth more than the business that runs them, and their value can halve in a month. In 2022 miners who had borrowed against their machines found the collateral worth a fraction of the loan. Some sold machines to pay power bills; buyers with cheap power picked them up.',
    game: 'The broker pays today\'s used price, less a 12% cut, scaled by the machine\'s health. A dead one goes for parts at 35%.' },
  { id: 'firmware', ch: 'Operations', t: 'Custom firmware', goal: 'Run a machine on a custom firmware profile.', ok: (u) => u[344] + u[345] > 0,
    real: 'Stock firmware runs every chip at the factory settings. Custom firmware such as Braiins OS, LuxOS and Vnish tunes each chip and lets the miner pick a power target. Underclocking cuts power faster than hashrate, because slower chips need less voltage. After the 2024 halving many miners underclocked older S19s instead of switching them off. Overclocking does the reverse: more hashrate, worse J/TH and more heat, worth it only when hashprice is high and power is cheap. The firmware makers take a fee of around 2–3% of hashrate.',
    game: 'Five profiles, from 70% hashrate at 55% power to 128% at 162%. Overclocked boxes run up to 13 °C hotter and wear up to 2.2 times faster; underclocked ones run cooler and last longer. Any custom profile costs a 2% fee.' },
  { id: 'spot', ch: 'Operations', t: 'Riding the spot price', goal: 'Let the spot price switch your boxes off.', ok: (u) => u[356] > 0,
    real: 'Many Texas miners buy power at the ERCOT real-time price instead of a fixed rate. It averages less, but it moves every five minutes: cheap at night or when the wind blows, dear on summer afternoons, and up to the $5,000/MWh cap in an emergency. Their machines switch off automatically whenever an hour of power would cost more than the bitcoin it mines, and back on when it falls. That flexibility is what makes spot power a good deal for them.',
    game: 'Spot averages 85% of the fixed contract, shaped by the hour, the gas price, heatwaves and brownouts, with spikes during grid emergencies. Break-even mode switches each box off when power costs more than it earns at today\'s hashprice; a flat limit cuts the whole grid feed instead.' },
  { id: 'dr', ch: 'Operations', t: 'Paid to stand by', goal: 'Earn a demand-response credit.', ok: (u) => u[355] > 0,
    real: 'Grids pay big loads to be ready to disappear. In ERCOT, miners sell that readiness as ancillary services and emergency response: they are paid whether or not they are called, and must switch off within minutes when they are. In August 2023, a heatwave month, Riot Platforms reported $31.7 million in power and demand-response credits, more than the bitcoin it mined that month was worth.',
    game: 'Demand response pays $3 back on every MWh you draw. Calls come about 30% more often, switch you off automatically, and pay 70% of an emergency offer.' },
  { id: 'heat', ch: 'Operations', t: 'Selling the heat', goal: 'Sell waste heat to a greenhouse.', ok: (u) => u[393] > 0,
    real: 'A miner turns almost every watt it draws into heat. Most sites blow it away; some sell it. Dutch tomato growers have heated greenhouses with bitcoin miners, and MintGreen heats buildings in North Vancouver through a district heating network. Air-cooled exhaust is only about 40 °C and hard to use. Liquid-cooled machines give 50–60 °C water that plugs into heating systems. Heat is only worth money where and when someone needs it, so it pays far better somewhere cold than in Texas.',
    game: 'A greenhouse takes heat from boxes within 2 tiles, up to 200 kW. Air-cooled boxes give 50% of their draw as useful heat; hydro boxes, or boxes touching an immersion tank, give 85%. Heat sells for around $20/MWh on a cool day, scaled by how cold it is and the gas price, and piped boxes run 6 °C cooler.' },
  { id: 'pools', ch: 'Network', t: 'Picking a pool', goal: 'Switch to a different pool.', ok: (u) => u[467] > 0,
    real: 'Pools compete on fees, payout method and how much of your work they lose. A share only counts if it reaches the pool before the pool moves on to new work. Shares that arrive late are stale, and the main cause is distance to the pool\'s servers, which is why pools run servers on several continents. Shares with bad results, from chips running hot or overclocked, are rejected. Miners watch both numbers. Stratum V2, a newer protocol, sends work in smaller, encrypted messages and can let miners pick their own block transactions.',
    game: 'Three made-up pools: 2% fee 25 ms away, 1% fee 240 ms away, and a 0.5% PPLNS pool 60 ms away. Stale shares are 0.05% plus 0.008% per millisecond. Rejects start at 0.2% and rise with each overclock step (+1.2%), heat over 80 °C (+3%) and health under 30% (+2%).' },
  { id: 'pplns', ch: 'Network', t: 'Pool luck', goal: 'Get paid by a PPLNS pool.', ok: (u) => u[461] > 0,
    real: 'FPPS (Full Pay Per Share) pays the expected value of each share at once, transaction fees included; the pool carries the risk of bad luck and charges more for it. PPLNS (Pay Per Last N Shares) pays out only when the pool finds a block, split among recent shares. Over a long time PPLNS averages about the same before fees, but a small pool can go a day without a block. Most large miners use FPPS because their lenders and power bills want steady income.',
    game: 'The PPLNS pool has 2% of the network, so it finds about 3 blocks a day. Each time, you get your accepted share of that block. Pool luck shows what it paid against what FPPS would have.' },
  { id: 'crew', ch: 'Operations', t: 'Spares on the shelf', goal: 'Hire a technician.', flag: true,
    real: 'An ASIC is not one thing. An Antminer S19 has three hashboards, a power supply and four fans, and they fail separately: fans clog and seize, power supplies die in heat and brownouts, and boards crack solder joints after thousands of heat cycles. Large sites keep shelves of spares and technicians on shift who swap a part in minutes; a machine with one dead board still mines at two-thirds speed. Small miners often ship broken units to repair shops and wait weeks.',
    game: 'Each container is 10 machines: 30 boards, 10 power supplies and 40 fans (hydro boxes have none). A technician costs $25 a day, swaps 12 parts a day from your shelf and cleans the 10 dustiest boxes. Without one, a contractor charges $150 a visit plus 50% on parts you don\'t stock. Fixing a dead box rebuilds it; a service only restores health.' },
  { id: 'guards', ch: 'Operations', t: 'Security', goal: 'Hire a guard.', flag: true,
    real: 'Mining sites hold millions of dollars of small, valuable hardware, often far from town. Theft of machines happens, so serious sites run fences, cameras and guards around the clock, and insurers ask about all three.',
    game: 'Each guard costs $20 a day, makes raids 25% less likely (up to three guards) and has a 30% chance of stopping a raid at the gate.' },
  ...SITES_UI.map((st, i) => ({ id: 'site_' + st.key, ch: 'World', t: st.t, goal: `Mine for 30 days in ${st.t}.`, ok: (u) => !u[318] && u[512] === i && u[15] >= 30, real: st.real, game: st.game })),
  { id: 'world2', ch: 'World', t: 'Going global', goal: 'Run two sites at once.', flag: true,
    real: 'After China\'s 2021 ban, miners learned what a single country can do to a fleet. Large miners now spread their machines across states and countries, so one heatwave, tariff hike or law can\'t take everything offline, and each site\'s cheap hours and seasons are different.',
    game: 'Opening a site costs $40,000 and starts with a hookup, a shed and one box. All sites share one wallet and one network (price, difficulty, the machine market); each has its own map, weather, power and events. Sites you aren\'t looking at have their events handled with the first choice.' },
  { id: 'sell', ch: 'Economics', t: 'Paying the bills', goal: 'Sell some bitcoin.', flag: true,
    real: 'Power bills are paid in dollars, so miners sell bitcoin. Many public miners sell most of what they mine each month; others hold and borrow against it, which pays off in a bull market and hurts in a bear market.',
    game: 'Selling costs a 0.5% spread. Holding ₿ 21 is one of the core requirements.' },
  { id: 'fence', ch: 'Operations', t: 'Perimeter', goal: 'Fence every miner in.', ok: (u) => u[42] > 0 && u[279] > 0 && u[248] === 0,
    real: 'Physical security is a real line item. Sites use fences, cameras, access control and guards, because ASICs are valuable, portable and easy to resell.',
    game: 'Raids are dramatised: thieves walk in from the map edge, and only a closed wall keeps them out.' },
  { id: 'curtail', ch: 'Operations', t: 'Paid to switch off', goal: 'Accept a curtailment call.', ok: (u) => u[302] >= 1,
    real: 'Miners are one of the few loads that can switch off in seconds. In Texas (ERCOT) they sign demand-response deals and get paid to power down when the grid is stressed. In August 2023, Riot Platforms reported $31.7 million in power and demand-response credits, more than the value of the bitcoin it mined that month.',
    game: 'Calls come with a d20-rolled price. Accept and your grid-fed boxes go dark for 6 hours. Decline and you pay 6× your contract price.' },
  { id: 'fees', ch: 'Network', t: 'Fee market', goal: 'Mine through a fee spike.', ok: (u) => u[304] >= 1,
    real: 'Users bid fees (in sats per vbyte) to get into the next block, and the miner keeps them on top of the subsidy. When the mempool jams, fees spike. Block 840,000, the 2024 halving block, paid about 37.6 BTC in fees: twelve times its 3.125 BTC subsidy.',
    game: 'Normal fees follow mempool.space\'s daily average. Spikes are random events, plus one after every halving.' },
  { id: 'ladder', ch: 'Operations', t: 'Cheaper power', goal: 'Max out a power source and unlock the next.', ok: (u) => u[51] > 0,
    real: 'Power is most of a miner\'s running cost. Real miners chase power under about $40/MWh: long-term contracts, stranded hydro, flared gas at oil wells, and sites that sell power back to the grid.',
    game: 'The power ladder is game-sized: plant prices, outputs and the upgrade path are made up so they fit on one map.' },
  { id: 'solo', ch: 'Economics', t: 'Lottery ticket', goal: 'Find a block mining solo.', ok: (u) => u[295] >= 1,
    real: 'Solo mining has the same expected income as a pool, without the fee, but it arrives all at once or never. Every so often a hobbyist with a few TH/s wins a whole block; most never do.',
    game: 'Your odds each block are your real share of the network\'s hashrate.' },
  { id: 'gas', ch: 'Endgame', t: 'Gas turbines', goal: 'Unlock the gas peaker.', ok: (u) => u[52] > 0,
    real: 'Natural gas powers a lot of real mining. Some sites sit at oil wells and burn gas that would otherwise be flared, turning waste into hashrate. Gas prices swing, so margins do too.',
    game: 'The gas peaker costs $60/MWh times a gas price that drifts day to day, with occasional spikes.' },
  { id: 'halving', ch: 'Endgame', t: 'The halving', goal: 'Survive a halving.', ok: (u) => u[14] >= 1,
    real: 'Every 210,000 blocks the subsidy halves: 50, 25, 12.5, 6.25, then 3.125 BTC in April 2024. The next cut, to 1.5625 BTC, is at block 1,050,000, around 2028. Hashprice roughly halves overnight and the least efficient machines switch off.',
    game: 'Same schedule. In simulated markets the price reaction is a d20 roll; in live runs the price stays the real one.' },
  // ---- Phase 2 cards
  { id: 'block', ch: 'Basics', t: 'Inside a block', goal: 'Open a block in the explorer.', flag: true,
    real: 'A block is an 80-byte header plus a list of transactions, capped at 4 million weight units. That is about 1 million vbytes, or up to roughly 4 MB on disk when transactions carry a lot of witness data. A typical block today holds a few thousand transactions.',
    game: 'Blocks around your run are simulated with realistic sizes, transaction counts and pool shares. Their fees follow your run\'s fee level.' },
  { id: 'coinbase', ch: 'Basics', t: 'The coinbase transaction', goal: 'Open a block\'s coinbase transaction.', flag: true,
    real: 'The first transaction in every block has no real input. It creates the new coins and collects every fee in the block. It must start with the block height (BIP34), and pools write a tag into it, such as "/Foundry USA Pool/". A second output commits to the block\'s witness data.',
    game: 'Your solo blocks carry the tag "/Power Grid/". The extranonce is random; in real mining it gives the hardware more values to try once all 4 billion nonces are used up.' },
  { id: 'mempool', ch: 'Network', t: 'The waiting room', goal: 'Look at the mempool.', flag: true,
    real: 'Every node keeps its own mempool of valid, unconfirmed transactions, so there is no single mempool. Bitcoin Core caps it at 300 MB by default and drops the cheapest transactions first when it fills.',
    game: 'In a calm live run you see mempool.space\'s real projected blocks. During the game\'s fee spikes, and in simulated runs, the queue is modelled from your run\'s fees.' },
  { id: 'confirm', ch: 'Network', t: 'Six confirmations', goal: 'Look at a block, then come back after 6 more blocks.', flag: true,
    real: 'Each block built on top of a transaction\'s block is one more confirmation. Undoing a confirmed transaction means redoing all that work faster than the rest of the network. Six confirmations, about an hour, is the traditional bar for large payments.',
    game: 'Confirmations count up as your run\'s blocks are mined. The explorer remembers the blocks you have looked at.' },
  { id: 'charts', ch: 'Network', t: 'A year of hashrate', goal: 'Open the network charts.', flag: true,
    real: 'Nobody can measure the network\'s hashrate directly. It is estimated from how quickly blocks arrive at a known difficulty, so short-term numbers are noisy. The biggest drop on record came after China\'s 2021 mining ban: the July 2021 retarget cut difficulty by about 28%.',
    game: 'The charts are real mempool.space data. Your run\'s own network is simulated from today\'s numbers, so it drifts away from the real one as you play.' },
  { id: 'feerate', ch: 'Economics', t: 'Fee rate, not fee', goal: 'Build a block by hand and compare both sorting rules.', flag: true,
    real: 'Block space is the scarce thing, so miners rank transactions by fee per vbyte, not by total fee. Bitcoin Core\'s block template goes further and scores whole packages of parent and child transactions together, which is how "child pays for parent" works.',
    game: 'The pack-a-block puzzle has no packages, and its block is tiny so you can do it by hand.' },
  { id: 'hashprice', ch: 'Economics', t: 'Hashprice', goal: 'Compare your run\'s hashprice with the real one.', ok: () => !!(ledger.charts && chartData),
    real: 'Hashprice is what a unit of hashrate earns per day, usually quoted in dollars per PH/s. It folds the price, the difficulty, the subsidy and fees into one number, and it is the first thing miners compare with their power cost.',
    game: 'The HUD\'s net per day and the dock\'s payback numbers use the same formula, from your run\'s own price and difficulty.' },
  { id: 'shed', ch: 'Operations', t: 'Switching off', goal: 'Switch off a box to save power.', ok: (u) => u[253] > 0,
    real: 'Many miners run automatic economic curtailment: when the power price climbs past a model\'s breakeven, those machines shut down within seconds and come back when it falls. A machine that is off costs nothing but its rent and depreciation.',
    game: 'You switch boxes off by hand, or use "Switch off worst" during a shortage. There is no automatic breakeven switch yet.' },
  { id: 'cooling', ch: 'Operations', t: 'Moving heat', goal: 'Build a fan wall or an immersion tank.', flag: true,
    real: 'Practically every watt a miner draws comes out as heat. Air-cooled sites move huge volumes of air; immersion cooling sinks the boards in dielectric fluid, which lets them run harder and quieter. Chips throttle or shut down when they get too hot.',
    game: 'Heat is simplified: each box adds heat to its neighbours, fans blow a 4-tile lane and tanks cool what they touch.' },
  { id: 'halvingdate', ch: 'Endgame', t: 'When is the halving?', goal: 'Look up the next real halving date.', flag: true,
    real: 'Halvings happen at block heights, not dates. The date is only an estimate from recent block times. When hashrate is growing, blocks arrive a little faster than every ten minutes between retargets, so the halving tends to come early.',
    game: 'Your run halves at the same heights. Its blocks just arrive much faster than ten minutes.' },
  { id: 'maturity', ch: 'Endgame', t: '100 blocks to wait', goal: 'Find a block solo and watch it reach 100 confirmations.', flag: true,
    real: 'New coins from a coinbase transaction can\'t be spent until 100 more blocks are built on top, about 17 hours. If the block is orphaned in a reorganisation, its reward simply never existed, so the network makes miners wait.',
    game: 'Your bitcoin is credited straight away. The explorer shows the real countdown for your solo blocks.' },
  { id: 'deepgeo', ch: 'Endgame', t: 'Superhot rock', goal: 'Build deep geothermal.', ok: (u) => u[566] > 0,
    real: 'Geothermal plants already run on hot water a few kilometres down, in Iceland, Kenya, California and elsewhere. Several projects are now trying to drill far deeper, where rock is hot enough that one well could make many times more power. It is promising and unproven at scale.',
    game: 'Deep geothermal is the game\'s: steady, $6/MWh, up to 160 MW. It sits between fission and orbital solar on the ladder.' },
  { id: 'orbital', ch: 'Endgame', t: 'Power from orbit', goal: 'Build an orbital solar station.', ok: (u) => u[568] > 0,
    real: 'In orbit the sun never sets and no clouds get in the way. Engineers have studied beaming that power down as microwaves for decades, and small tests have now done it, at the scale of milliwatts. A power station in orbit would need enormous structures launched cheaply.',
    game: 'Orbital solar is the game\'s: free power, day and night, up to 300 MW, received by a field of antennas on the ground.' },
  { id: 'core', ch: 'Endgame', t: 'Free energy', goal: 'Ignite the free energy core.', ok: (u) => u[21] === 2,
    real: 'Even free power wouldn\'t make mining free. If everyone had it, more machines would plug in and difficulty would rise until the margin was gone again. The cost of mining is set by competition: miners spend roughly what the block reward is worth.',
    game: 'The core is the game\'s one fantasy. It ends the run because nobody else gets one.' },
  // ---- History: finish a replay to unlock its card
  { id: 'h_uri', ch: 'History', t: 'Winter Storm Uri, 2021', goal: 'Finish the Winter Storm Uri replay.', flag: true,
    real: 'When the Texas grid nearly collapsed in February 2021, power was worth $9,000/MWh and miners switched off. A load that can disappear in seconds is worth a lot to a grid, which is why Texas now pays miners to be that load.',
    game: 'The replay\'s prices, difficulty and fees are real; the timing of the calls within each day is the game\'s.' },
  { id: 'h_china', ch: 'History', t: 'The China ban, 2021', goal: 'Finish the great migration replay.', flag: true,
    real: 'Half the network vanished in weeks, difficulty fell a record 27.9%, and miners elsewhere earned more for every terahash. Within a year the machines had moved and hashrate was back. Difficulty adjustment is what made the network shrug it off.',
    game: 'Real daily price, difficulty and fees. Your plant is the game\'s.' },
  { id: 'h_bear', ch: 'History', t: 'Crypto winter, 2022', goal: 'Finish the crypto winter replay.', flag: true,
    real: 'Price fell by two thirds while difficulty kept rising and power got dearer. Leverage turned a bad year into bankruptcies. The survivors had cheap power, efficient machines and little debt.',
    game: 'Machine resale prices don\'t fall with the market in the game; in 2022 they fell more than 80%.' },
  { id: 'lightning', ch: 'Lightning', t: 'First Lightning payment', goal: 'Send a tip of any size from the tip jar.', flag: true,
    real: 'Lightning is a network of payment channels on top of bitcoin. To get paid, a wallet writes an invoice that carries the hash of a secret, the preimage. The payment hops across channels, and each hop can only claim its money by revealing that secret, so when the preimage comes back to the payer it is a receipt nobody could fake. Payments settle in seconds for a few sats in fees, which is why paying 21 sats makes sense at all. Underneath, every channel is an ordinary bitcoin transaction that miners confirm.',
    game: 'The tip jar asks a real wallet for a real invoice, and the sats go to the developer. Paste the preimage your wallet shows and the game hashes it with SHA-256 and compares it with the invoice\'s payment hash, the same check Lightning nodes make.' },
  { id: 'h_halving', ch: 'History', t: 'The 2024 halving', goal: 'Finish the fourth halving replay.', flag: true,
    real: 'The subsidy halved, a fee frenzy paid one block more than any before it, and then hashprice fell to record lows. Every halving forces the same question: is this machine still worth the power it burns?',
    game: 'The halving lands on the real day and the fee spike is in the real data.' },
];
const CHAPTERS = ['Basics', 'Network', 'Economics', 'Operations', 'World', 'Endgame', 'Lightning', 'History'];
GOALS.sort((a, b) => CHAPTERS.indexOf(a.ch) - CHAPTERS.indexOf(b.ch));
let ledger = {};
try { ledger = JSON.parse(localStorage.getItem('pg-ledger') || '{}') || {}; } catch (_) { ledger = {}; }
const saveLedger = () => { try { localStorage.setItem('pg-ledger', JSON.stringify(ledger)); } catch (_) {} };
function unlockGoal(id) {
  if (ledger[id]) return;
  const g = GOALS.find((x) => x.id === id); if (!g) return;
  ledger[id] = { at: Date.now() }; saveLedger();
  toast('New ledger card: ' + g.t, `${g.goal} Open the ledger to read it.`, 'var(--btc)', '', 'book'); sfx('coin');
  updateLedgerBadge(); if ($('#memo').hidden) tip('ledger'); // never cut into the onboarding chain
}
function checkGoals(u) { for (const g of GOALS) if (!g.flag && !ledger[g.id] && g.ok(u)) unlockGoal(g.id); }
function updateLedgerBadge() { const b = $('#b-ledger'); if (b) b.dataset.n = GOALS.filter((g) => ledger[g.id]).length; }
function openLedger(focus) {
  if (!steps.ledger) { steps.ledger = 1; saveSteps(); statusHtml = ''; }
  const done = GOALS.filter((g) => ledger[g.id]).length;
  const chapters = CHAPTERS;
  openModal(`<div class="mh"><div><h2>Ledger</h2><span class="dim">${done} of ${GOALS.length} cards</span></div><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <p>Each goal unlocks a card: what happens in real Bitcoin mining, and where this game bends it. Cards stay unlocked across runs.</p>
    ${chapters.map((ch) => { const gs = GOALS.filter((g) => g.ch === ch); const n = gs.filter((g) => ledger[g.id]).length;
      return `<h3 class="chap">${ch} <span class="dim num">${n}/${gs.length}</span></h3><div class="ledger">${gs.map((g) => ledger[g.id]
      ? `<details class="lcard" ${focus === g.id ? 'open' : ''}><summary><b>${g.t}</b><span class="dim">${g.goal}</span></summary><div class="rg"><div><h4>In real Bitcoin</h4><p>${g.real}</p></div><div><h4>In this game</h4><p>${g.game}</p></div></div></details>`
      : `<div class="lcard locked"><b>${ic('lock')}${g.t}</b><span class="dim">${g.goal}</span></div>`).join('')}</div>`; }).join('')}`, true);
  $('#modal [data-x]').onclick = closeModal;
}

// ------------------------------------------------------------------ "Why?" — every alert and number taps through to a short card
const WHY = {
  deepgeo: ['Deep geothermal (game)', 'Real today: geothermal plants tap hot water a few kilometres down, and projects are testing drilling much deeper, to rock hot enough to make far more power per well. Nobody runs a 15 km superhot well yet. The game builds one anyway.', 'fusion'],
  orbital: ['Orbital solar (game)', 'Real today: small experiments have beamed power from orbit, including a 2023 test that sent a measurable trickle to the ground. A station that delivers megawatts, day and night, doesn\'t exist. The game\'s does.', 'fusion'],
  fusion: ['The game\'s late ladder', 'Fusion experiments have produced more energy from the reaction than the lasers delivered to it, but no fusion plant sells power yet. Past fission, the ladder is the game\'s: deep geothermal, orbital solar and fusion are real research, and the free energy core is pure fantasy.'],
  sites: ['Several sites', 'Big miners spread out on purpose. One heatwave, one tariff hike or one new law can\'t hit every site at once, and each place has a different cheap hour, season and risk. In the game your sites share one wallet and one view of the network, and each runs its own plant, weather and power.', 'site'],
  site: ['Where you mine', 'Miners go where power is cheap and stays cheap: stranded hydro, flared gas, cold places with surplus geothermal, grids that pay big loads to be flexible. Every place trades something for that price: weather, reliability, distance, or how welcome miners are.'],
  dry: ['Dry season', 'Hydro power depends on rain. In the dry months rivers run low, the dams make less power, and the country needs what\'s left for homes and businesses, so miners get less.', 'site'],
  flare: ['Flare gas', 'Oil wells bring up gas that often has no pipeline to go to, so it\'s burned off. A generator at the well turns it into cheap power instead. Shale wells decline fast, though: the gas, and your power, falls month by month.', 'site'],
  tariff: ['Tariff hikes', 'When miners use a lot of a country\'s power, governments and utilities often raise what they pay, or stop signing new contracts. Cheap power is only cheap until someone decides it should cost more.', 'site'],
  parts: ['Failed parts', 'Each machine has three hashboards, a power supply and four fans, and they fail on their own. A dead board costs a third of the machine\'s hashrate, a dead power supply takes the whole machine offline, and dead fans make it run hotter, which makes everything else fail faster. Wear, heat, overclocking, dust and brownouts all raise the odds.', 'crew'],
  crew: ['Technicians and guards', 'Big sites keep technicians on shift and shelves of spare parts, so a failed board is swapped in minutes instead of waiting for a contractor. Guards and fences protect hardware worth millions in remote places. Both cost wages every day, whether anything breaks or not.'],
  stale: ['Stale shares', 'A pool hands out new work every time the network moves on. A share that arrives after that is stale and earns nothing. The further you are from the pool\'s servers, the more of your work arrives late, so a cheap pool on another continent can cost more than its fee saves.', 'pplns'],
  rejects: ['Rejected shares', 'Chips pushed too hard make mistakes. A share with a bad result is rejected by the pool and earns nothing. Heat, wear and overclocking all raise the error rate, so the extra hashrate of an overclock is worth a little less than it looks.', 'firmware'],
  pplns: ['FPPS and PPLNS', 'An FPPS pool pays the expected value of every share straight away, including fees, and charges a higher fee for carrying the luck. A PPLNS pool pays your share of a block only when the pool actually finds one. Over months it averages out a little ahead of FPPS; day to day it swings with the pool\'s luck.'],
  heat: ['Heat reuse', 'Almost every watt a miner draws comes out as heat. Sold to a greenhouse it becomes a second product: the grower burns less gas, and you get paid for something you would otherwise blow into the sky. It is worth most when it\'s cold, and liquid-cooled machines give hotter, more useful heat than air-cooled ones.'],
  firmware: ['Firmware tuning', 'Custom firmware sets how hard each chip runs. A slower chip needs less voltage, and power rises with voltage squared, so underclocking cuts power faster than hashrate: fewer terahashes, each one cheaper. Overclocking is the reverse. Which profile wins depends on hashprice and what you pay for power.', 'hashprice'],
  market: ['Machine market', 'Mining machines are priced like the business they do. When hashprice is high, every box pays and buyers queue; when it falls, the oldest boxes stop paying and their owners sell. New machines follow hashprice at about three-quarters strength. Used ones follow their margin, so old ones swing hardest.', 'hashprice'],
  capitulation: ['Miner capitulation', 'When hashprice falls below what older machines cost to run, their owners switch them off and sell them. More used machines come up for sale, at lower prices. It\'s the best time to buy if your power is cheap.', 'market'],
  cash: ['Cash', 'Dollars in the bank. Power bills, repairs and new machines come out of it every block. Bitcoin only turns into cash when you sell it.'],
  hashrate: ['Your hashrate', 'How many guesses your running boxes make every second. Your share of the network\'s total hashrate is your share of every block\'s reward, so this number against the network\'s is what you earn. It drops when boxes are off, unpowered, too hot, worn or missing parts.', 'hashprice'],
  net: ['Net per day', 'What your bitcoin earnings are worth per day at today\'s price, minus what your power costs per day. It moves with the price, the difficulty and your power mix.', 'hashprice'],
  price: ['Bitcoin price', 'In a live run this is today\'s real price from mempool.space, refreshed every few minutes. In a simulated run it wanders around a slowly rising trend, with crashes and pumps.'],
  power: ['Power used and capacity', 'Used is what your machines draw right now. Capacity is what your sources can supply. If a network asks for more than it gets, every box on it slows down.'],
  hashprice: ['Hashprice', 'What one unit of hashrate earns per day: (block subsidy + fees) × blocks per day × price ÷ network hashrate. It is the number miners watch most. It falls when difficulty rises or the price drops, and it halves at every halving.', 'charts'],
  shortage: ['Power shortage', 'This network asks for more power than its sources can deliver after line losses, so every box on it runs slower. Brownouts also stress power supplies, so health drops faster. Switch off the least efficient boxes or add capacity.'],
  unpowered: ['No power', 'Electricity only flows between things that touch, or along power poles (4-tile reach). A box that isn\'t connected to a source sits idle.'],
  off: ['Switched off', 'Boxes you switched off draw nothing and earn nothing. Real miners do this when a machine costs more in power than it earns.'],
  heat: ['Running hot', 'Above 80 °C a box throttles to 60% hashrate, and above 95 °C it can catch fire. Neighbouring boxes heat each other. Fans blow a 4-tile lane; immersion tanks cool whatever touches them.'],
  exposed: ['Outside the fence', 'Raiders walk in from the edge of the map. Anything they can reach without crossing a wall or water can be stolen.'],
  dust: ['Dust', 'Dust clogs filters and fans. Every 10% adds about 1.5 °C to a box. Rain rinses some off; dust storms coat everything.'],
  dead: ['Dead boxes', 'Health reached zero, usually from heat. A dead box earns nothing until you repair it, or you can sell it on the black market.'],
  curtail: ['Curtailment', 'When the grid is short of power, the price of electricity can spike to hundreds or thousands of dollars per MWh. A miner that can switch off in seconds is valuable to the grid, so utilities pay for it.'],
  spot: ['Spot price', 'The real-time price of power on the grid. It is lowest overnight and highest on hot afternoons, it follows the price of gas, and in an emergency it can jump to thousands of dollars per MWh. On a fixed contract, declining a curtailment call means paying a spike price until it passes.', 'contract'],
  contract: ['Power contract', 'How you buy power matters as much as what it costs. A fixed price is predictable, and the supplier charges a premium for carrying the risk. The spot price is cheaper on average but follows demand, weather and gas, and can spike a hundredfold. Miners can switch off in seconds, so cheap-but-wild spot power, or being paid to stand by, is worth more to them than to almost anyone.'],
  econoff: ['Economic curtailment', 'At today\'s hashprice each box earns a set amount per hour. When an hour of power costs more than that, running it loses money, so on the spot contract it switches itself off and comes back when the price falls. Real miners automate exactly this.', 'hashprice'],
  fees: ['Fee spike', 'Users bid for space in the next block in sats per vbyte. When many people want in at once, the bids climb. The miner who wins a block keeps every fee in it, on top of the subsidy.', 'mempool'],
  retarget: ['Difficulty retarget', 'Every 2016 blocks the network adjusts difficulty so blocks keep averaging ten minutes. If blocks came faster than that, difficulty rises and each machine earns less.', 'charts'],
  halving: ['Halving', 'Every 210,000 blocks the new-coin subsidy in each block is cut in half. Power bills don\'t change, so the least efficient machines stop paying for themselves.', 'charts'],
  unpaid: ['Unpaid bills', 'Your cash is below zero. Sell bitcoin, switch off losing boxes or sell equipment before the bank repossesses the plant.'],
  bailout: ['Bailout', 'On Normal, the Foreman covers you once a month in the early game. It stops once you reach gas.'],
  raid: ['Raids', 'ASICs are valuable and easy to resell, so sites get robbed. Close a wall loop around every box and upgrade the weakest stretch.'],
  lightning: ['Lightning', 'Strikes hit tall things first: poles, turbines and the grid hookup. A hit on a box costs health and can start a fire.'],
  weather: ['Weather', 'Rain cools the plant and weakens solar. Storms lock wind turbines and bring lightning. Dust storms clog filters.'],
  block: ['Block found', 'Your hash beat the target, so your block joins the chain and its coinbase transaction pays you the subsidy plus every fee in the block.', 'blocks'],
  solo: ['Solo vs pool', 'Pool and solo have the same expected income. The pool pays you a little every block; solo pays nothing until you find a whole block yourself.'],
  unlock: ['Unlocks', 'Upgrading a power source to level 3 unlocks the next rung of the ladder: cheaper or bigger power.'],
  event: ['Random events', 'Each day there is a chance of an event. Hard and Brutal roll more often. Some are real mining risks, some are the game\'s drama.'],
  mempool: ['The mempool', 'Unconfirmed transactions wait in each node\'s mempool. Miners build the next block from it, taking the highest fee rates first, until the block is full.', 'mempool'],
};
function why(key) {
  const w = WHY[key]; if (!w) return;
  const box = $('#why');
  box.innerHTML = `<div class="sheet" role="dialog" aria-label="${w[0]}"><div class="sh-head"><b>${w[0]}</b><button class="ib" data-x aria-label="Close">${ic('x')}</button></div><p>${w[1]}</p>${w[2] ? `<button class="btn mini" data-go="${w[2]}">${{ hashprice: 'See hashprice', charts: 'Open the charts', mempool: 'Open the mempool', blocks: 'Open the explorer' }[w[2]]}</button>` : ''}</div>`;
  box.hidden = false; sfx('tap');
  box.querySelector('[data-x]').onclick = () => { box.hidden = true; };
  const go = box.querySelector('[data-go]'); if (go) go.onclick = () => { box.hidden = true; openNetwork(w[2] === 'hashprice' ? 'charts' : w[2]); };
}
document.addEventListener('pointerdown', (e) => { const b = $('#why'); if (!b.hidden && !b.contains(e.target) && !e.target.closest('[data-why]')) b.hidden = true; });
document.addEventListener('click', (e) => { const t = e.target.closest('[data-why]'); if (t) { e.stopPropagation(); why(t.dataset.why); } }, true);
const whyBtn = (k) => `<button class="whyb" data-why="${k}" aria-label="Why?">Why?</button>`;

// ------------------------------------------------------------------ network hub: mempool, blocks, charts
const SAT = 1e8;
const mulberry = (a) => () => { a |= 0; a = (a + 0x6D2B79F5) | 0; let t = Math.imul(a ^ (a >>> 15), 1 | a); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
// fee-rate classes in sat/vB, and a one-hue ramp from the panel up to bright orange (validated as ordinal)
const FEE_BINS = [0, 1, 2, 4, 10, 30, 100];
const FEE_RAMP = ['#5E4128', '#7A4E26', '#985E25', '#B86E22', '#D9801F', '#F29B3A', '#FFC27A'];
const binOf = (r) => { let b = 0; for (let i = 0; i < FEE_BINS.length; i++) if (r >= FEE_BINS[i]) b = i; return b; };
const binLabel = (i) => i === FEE_BINS.length - 1 ? `${FEE_BINS[i]}+` : `${FEE_BINS[i]}–${FEE_BINS[i + 1]}`;
const satvb = (r) => r < 1 ? r.toFixed(2) : r < 10 ? r.toFixed(1) : Math.round(r).toString();

let netTab = 'mempool', liveMempool = null, liveMempoolAt = 0;
async function fetchMempool() {
  if (Date.now() - liveMempoolAt < 60000 && liveMempool) return liveMempool;
  try { const b = await getJ('/v1/fees/mempool-blocks'); if (Array.isArray(b) && b.length) { liveMempool = b; liveMempoolAt = Date.now(); } } catch (_) {}
  return liveMempool;
}
// Projected blocks: real ones from mempool.space in a calm live run, otherwise modelled from the game's own fees.
function mempoolBlocks(u) {
  const spike = u[299] > 0;
  if (u[241] && !spike && liveMempool) return { src: 'live', blocks: liveMempool.slice(0, 6) };
  const avg = Math.max(0.15, u[23] * SAT / 1e6);             // sat/vB that a full block at today's fees averages
  const n = spike ? Math.min(12, 5 + Math.round(u[298] * 0.5)) : 2 + ((u[6] % 3));
  const blocks = [];
  for (let k = 0; k < n; k++) {
    const med = Math.max(0.1, avg * 0.8 / (1 + (spike ? 0.35 : 0.9) * k)); // fee rates are skewed: the median sits below the average
    const range = [med * 0.45, med * 0.7, med * 0.9, med, med * 1.25, med * 1.9, med * (k ? 3 : 25)];
    const vs = k === n - 1 ? 300000 + ((u[6] * 7919) % 600000) : 999000;
    blocks.push({ blockVSize: vs, nTx: Math.round(vs / 260), totalFees: k ? Math.round(med * 1.12 * vs) : Math.round(u[23] * SAT), medianFee: med, feeRange: range });
  }
  return { src: spike ? 'spike' : 'model', blocks };
}

function openNetwork(tab) {
  if (tab) netTab = tab;
  const u = UI();
  openModal(`<div class="mh"><h2>Network</h2><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <div class="seg" role="tablist">${[['mempool', 'Mempool'], ['blocks', 'Blocks'], ['charts', 'Charts']].map(([k, t]) => `<button role="tab" data-t="${k}" aria-selected="${netTab === k}">${t}</button>`).join('')}</div>
    <div id="net-body"></div>`, true);
  const m = $('#modal'); m.classList.add('wide');
  m.querySelector('[data-x]').onclick = closeModal;
  m.querySelectorAll('[data-t]').forEach((b) => b.onclick = () => { netTab = b.dataset.t; m.querySelectorAll('[data-t]').forEach((x) => x.setAttribute('aria-selected', x === b)); renderNet(); sfx('tap'); });
  renderNet();
}
function renderNet() {
  const body = $('#net-body'); if (!body) return;
  if (netTab === 'mempool') renderMempool(body);
  else if (netTab === 'blocks') renderBlocks(body, null);
  else renderCharts(body);
}

// ---- mempool
function renderMempool(body) {
  const u = UI(); const mp = mempoolBlocks(u); const b0 = mp.blocks[0];
  const minIn = b0.feeRange[0], demand = minIn < 1 ? 'low' : minIn < 5 ? 'normal' : minIn < 30 ? 'busy' : 'high';
  unlockGoal('mempool');
  const yourCut = u[293] ? null : (u[11] / u[12]) * (b0.totalFees / SAT) * payFactor(u);
  body.innerHTML = `
    <p class="lead">Demand is ${demand}. A transaction paying <b>${satvb(minIn)} sat/vB</b> or more gets into the next block${mp.blocks.length > 5 ? `, with ${mp.blocks.length > 7 ? 'a long' : 'a'} queue behind it` : ''}. ${mp.src === 'live' ? 'These are the real projected blocks from mempool.space.' : mp.src === 'spike' ? 'This is the game\'s fee spike, modelled from your run.' : 'Modelled from your run\'s fees.'}</p>
    <canvas id="mp-live" class="mplive" aria-label="The mempool: bundles of transactions arrive, sort by fee rate, and the top 1 MvB is mined into each block"></canvas>
    <div class="legend" aria-label="Fee rate, sat/vB">${FEE_RAMP.map((c, i) => `<span><i style="background:${c}"></i>${binLabel(i)}</span>`).join('')}<span class="dim">sat/vB · one square is about 30 transactions · a block holds 120 squares · blocks come every few seconds here, every 10 minutes for real</span></div>
    <div class="mp">
      <div class="rows">
        <div class="row"><span>Next block · ${(b0.blockVSize / 1e6).toFixed(2)} MvB</span><b class="num">${b0.nTx.toLocaleString()} txs</b></div>
        <div class="row"><span>Median fee rate</span><b class="num">${satvb(b0.medianFee)} sat/vB</b></div>
        <div class="row"><span>Range</span><b class="num">${satvb(b0.feeRange[0])} – ${satvb(b0.feeRange[b0.feeRange.length - 1])} sat/vB</b></div>
        <div class="row"><span>Fees in this block</span><b class="num">${btcf(b0.totalFees / SAT)}</b></div>
        <div class="row"><span>Subsidy</span><b class="num">${btcf(u[297])}</b></div>
        <div class="row tot"><span>Whoever mines it gets</span><b class="num">${btcf(u[297] + b0.totalFees / SAT)}</b></div>
        ${yourCut != null ? `<div class="row dim"><span>Your pool share of its fees</span><span class="num">${btcs(yourCut)}</span></div>` : ''}
      </div>
    </div>
    <h3>Waiting behind it</h3>
    <div class="qlist">${mp.blocks.slice(1).map((b, i) => `<div class="row"><span>In ~${(i + 2) * 10} min</span><span class="num">${satvb(b.feeRange[0])}–${satvb(b.feeRange[b.feeRange.length - 1])} sat/vB</span><span class="num dim">${(b.blockVSize / 1e6).toFixed(2)} MvB</span></div>`).join('') || '<div class="row dim"><span>Nothing else is waiting. The next block clears the queue.</span></div>'}</div>
    <h3>You pick the transactions</h3>
    <p>Your pool builds the block template, but the rule is the same for everyone: space is limited, so take the best fee <i>per vbyte</i>, not the biggest fee.</p>
    <div id="pack"></div>`;
  animateMempool($('#mp-live'), mp);
  renderPack($('#pack'), (u[6] | 0) ^ 0x5bd1e995);
  if (u[241] && !liveMempool) fetchMempool().then((b) => { if (b && netTab === 'mempool' && $('#net-body')) renderMempool($('#net-body')); });
}
// Toy block template: 4,000 vB of space, a dozen transactions. Knapsack by hand.
function renderPack(box, seed) {
  if (!box) return;
  const rnd = mulberry(seed); const CAP = 4000;
  const txs = []; for (let i = 0; i < 11; i++) { const vs = [140, 220, 380, 560, 900, 1400][(rnd() * 6) | 0] + ((rnd() * 9) | 0) * 10; const rate = Math.round(1 + rnd() * rnd() * 60); txs.push({ vs, fee: vs * rate }); }
  txs.push({ vs: 2400, fee: 2400 * 9 + 600 }); // the trap: biggest fee, poor rate
  txs.forEach((t, i) => { t.id = i; t.rate = t.fee / t.vs; });
  const pick = new Set(); let tried = { rate: false, big: false };
  const best = (() => { const W = CAP / 10; const dp = new Array(W + 1).fill(0); for (const t of txs) { const w = Math.round(t.vs / 10); for (let c = W; c >= w; c--) dp[c] = Math.max(dp[c], dp[c - w] + t.fee); } return dp[W]; })();
  const greedy = (key) => { const s = new Set(); let used = 0; [...txs].sort((a, b) => b[key] - a[key]).forEach((t) => { if (used + t.vs <= CAP) { s.add(t.id); used += t.vs; } }); return s; };
  const sum = (s, k) => [...s].reduce((a, id) => a + txs[id][k], 0);
  const draw = () => {
    const used = sum(pick, 'vs'), fee = sum(pick, 'fee');
    box.innerHTML = `<div class="pack">${txs.map((t) => `<button class="tx ${pick.has(t.id) ? 'on' : ''}" data-id="${t.id}" aria-pressed="${pick.has(t.id)}"><span class="num">${t.vs} vB</span><b class="num">${t.fee.toLocaleString()} sats</b><span class="num dim">${t.rate.toFixed(1)} sat/vB</span></button>`).join('')}</div>
      <div class="meter"><i style="width:${Math.min(100, used / CAP * 100)}%;background:${used > CAP ? 'var(--alarm)' : 'var(--btc)'}"></i></div>
      <div class="row"><span class="num">${used.toLocaleString()} / ${CAP.toLocaleString()} vB</span><b class="num">${fee.toLocaleString()} sats</b><span class="dim num">best possible ${best.toLocaleString()}</span></div>
      <div class="choices" style="flex-direction:row;flex-wrap:wrap"><button class="btn mini" data-g="big">Biggest fee first</button><button class="btn mini" data-g="rate">Best fee rate first</button><button class="btn mini" data-g="clear">Clear</button></div>
      <p class="note">${used > CAP ? 'Too big: that block would be invalid.' : fee >= best ? 'That\'s the best block possible. Miners can\'t always reach it exactly, but sorting by fee rate gets close.' : tried.big && tried.rate ? 'Sorting by the biggest fee wastes space on one large transaction. Sorting by fee rate is how real block templates are built.' : 'Tap transactions to put them in the block.'}</p>`;
    box.querySelectorAll('[data-id]').forEach((b) => b.onclick = () => { const id = +b.dataset.id; pick.has(id) ? pick.delete(id) : pick.add(id); sfx('tap'); draw(); check(); });
    box.querySelectorAll('[data-g]').forEach((b) => b.onclick = () => { const g = b.dataset.g; pick.clear(); if (g !== 'clear') { greedy(g === 'rate' ? 'rate' : 'fee').forEach((x) => pick.add(x)); tried[g] = true; } sfx('tap'); draw(); check(); });
  };
  const check = () => { const used = sum(pick, 'vs'); if (used <= CAP && ((tried.big && tried.rate) || sum(pick, 'fee') >= best)) unlockGoal('feerate'); };
  draw();
}

// ---- blocks
const POOLS = [['Foundry USA', 29, '/Foundry USA Pool/'], ['AntPool', 18, 'Mined by AntPool'], ['ViaBTC', 12, '/ViaBTC/'], ['F2Pool', 10, '/F2Pool/'], ['SpiderPool', 8, '/SpiderPool/'], ['MARA Pool', 5, '/MARA Pool/'], ['Binance Pool', 3, '/Binance/'], ['SECPOOL', 4, '/SECPOOL/'], ['Luxor', 3, '/LUXOR/'], ['OCEAN', 2, '/OCEAN.XYZ/'], ['Braiins Pool', 2, '/slush/'], ['Unknown', 4, '']];
let myBlocks = [];
try { myBlocks = JSON.parse(localStorage.getItem('pg-blocks') || '[]') || []; } catch (_) { myBlocks = []; }
function rememberBlock(u, won) {
  myBlocks = myBlocks.filter((b) => b.seed === u[35]).slice(-20);
  myBlocks.push({ seed: u[35], h: Math.round(u[6]), sub: u[297], fees: Math.max(0, won - u[297]) });
  try { localStorage.setItem('pg-blocks', JSON.stringify(myBlocks)); } catch (_) {}
}
const subsidyAt = (h) => 50 / 2 ** Math.floor(h / 210000);
function blockAt(h, u) {
  const mine = myBlocks.find((b) => b.h === h && b.seed === u[35]);
  const rnd = mulberry(h * 2654435761 ^ (u[35] | 0));
  let r = rnd() * 100, pool = POOLS[POOLS.length - 1]; for (const p of POOLS) { if (r < p[1]) { pool = p; break; } r -= p[1]; }
  const fees = mine ? mine.fees : u[23] * (0.55 + rnd() * 0.9);
  const tx = 2400 + ((rnd() * 2200) | 0);
  return { h, mine: !!mine, pool: mine ? 'Power Grid (you)' : pool[0], tag: mine ? '/Power Grid/' : pool[2], sub: mine ? mine.sub : subsidyAt(h), fees, tx,
    size: 1.3 + rnd() * 0.9, weight: 3.97 + rnd() * 0.029, nonce: (rnd() * 4294967295) >>> 0, extranonce: Array.from({ length: 8 }, () => ((rnd() * 256) | 0).toString(16).padStart(2, '0')).join('') };
}
const hexOf = (s) => Array.from(new TextEncoder().encode(s)).map((b) => b.toString(16).padStart(2, '0')).join('');
function renderBlocks(body, h) {
  const u = UI(); const tip = Math.round(u[6]);
  const mine = myBlocks.filter((b) => b.seed === u[35]);
  if (h == null) h = mine.length ? mine[mine.length - 1].h : tip;
  const b = blockAt(h, u); const conf = tip - h + 1;
  unlockGoal('block');
  const share = u[12] ? u[11] / u[12] : 0;
  const hb = [h & 255, (h >> 8) & 255, (h >> 16) & 255].map((x) => x.toString(16).padStart(2, '0')).join('');
  const chain = []; for (let k = Math.max(0, h - 4); k <= Math.min(tip, h + 2); k++) chain.push(k);
  let seen = []; try { seen = JSON.parse(localStorage.getItem('pg-seen-blocks') || '[]') || []; } catch (_) {} if (!seen.includes(h)) { seen.push(h); try { localStorage.setItem('pg-seen-blocks', JSON.stringify(seen.slice(-50))); } catch (_) {} }
  if (seen.some((x) => tip - x + 1 >= 6)) unlockGoal('confirm');
  if (b.mine && conf >= 100) unlockGoal('maturity');
  body.innerHTML = `
    <div class="chain" aria-label="The chain around this block">${chain.map((k) => { const x = blockAt(k, u); return `<button class="cblk ${k === h ? 'on' : ''} ${x.mine ? 'me' : ''}" data-h="${k}"><span class="num">${k.toLocaleString()}</span><small>${x.mine ? 'You' : x.pool}</small></button>`; }).join('')}${h + 2 >= tip ? `<div class="cblk next"><span>Next</span><small>being mined</small></div>` : ''}</div>
    <div class="bhead"><div><h3 style="margin:0">Block ${h.toLocaleString()}</h3><span class="dim">${b.mine ? 'Mined by you, solo' : 'Mined by ' + b.pool} · ${conf <= 0 ? 'not mined yet' : conf === 1 ? 'the newest block' : (conf - 1).toLocaleString() + ' blocks ago'}</span></div>
      <span class="conf num">${conf.toLocaleString()} confirmation${conf === 1 ? '' : 's'}</span></div>
    <div class="rows">
      <div class="row"><span>Transactions</span><b class="num">${b.tx.toLocaleString()}</b></div>
      <div class="row"><span>Size</span><b class="num">${b.size.toFixed(2)} MB · ${b.weight.toFixed(3)} MWU</b></div>
      <div class="row"><span>Nonce</span><b class="num">${b.nonce.toLocaleString()}</b></div>
    </div>
    <details class="xp" open><summary><span>Block reward</span><b class="num">${btcf(b.sub + b.fees)}</b></summary>
      <div class="row"><span>Subsidy (new coins)</span><span class="num">${btcf(b.sub)}</span></div>
      <div class="row"><span>Fees from ${(b.tx - 1).toLocaleString()} transactions</span><span class="num">${btcf(b.fees)}</span></div>
      ${!b.mine && !u[293] && !u[469] && share > 0 ? `<div class="row dim"><span>Paid to you by your pool for this block (FPPS)</span><span class="num">${btcs(share * (b.sub + b.fees) * payFactor(u))}</span></div>` : ''}
    </details>
    <details class="xp" id="cb"><summary><span>Coinbase transaction</span><span class="dim">the first transaction</span></summary>
      <p>The block's first transaction has no real input. It creates the new coins and collects the fees, and the miner writes into it.</p>
      <div class="row"><span>Input</span><span class="num">coinbase (no previous output)</span></div>
      <div class="hexline"><code><i class="hl-h" title="Block height (BIP34)">03${hb}</i><i class="hl-t" title="Pool tag">${hexOf(b.tag)}</i><i class="hl-x" title="Extranonce">${b.extranonce}</i></code></div>
      <div class="keyrow"><span><i class="kc hl-h"></i>height ${h.toLocaleString()}</span>${b.tag ? `<span><i class="kc hl-t"></i>"${b.tag}"</span>` : ''}<span><i class="kc hl-x"></i>extranonce</span></div>
      <div class="row"><span>Output 0 · ${b.mine ? 'your address' : 'pool payout'}</span><span class="num">${btcf(b.sub + b.fees)}</span></div>
      <div class="row"><span>Output 1 · OP_RETURN aa21a9ed… (witness commitment)</span><span class="num">₿ 0</span></div>
      ${b.mine ? `<div class="row"><span>Spendable after 100 confirmations</span><span class="num">${conf >= 100 ? 'now' : (100 - conf) + ' blocks to go'}</span></div><div class="meter"><i style="width:${Math.min(100, conf)}%;background:var(--btc)"></i></div>` : ''}
    </details>
    <div class="choices" style="flex-direction:row;flex-wrap:wrap">${b.mine ? '<button class="btn mini" data-a="lab">Replay the search in the hash lab</button>' : ''}${mine.length ? `<button class="btn mini" data-a="mine">Your blocks (${mine.length})</button>` : ''}<button class="btn mini" data-a="tip">Newest block</button></div>`;
  body.querySelectorAll('[data-h]').forEach((x) => x.onclick = () => { sfx('tap'); renderBlocks(body, +x.dataset.h); });
  body.querySelector('#cb').addEventListener('toggle', (e) => { if (e.target.open) unlockGoal('coinbase'); });
  const lab = body.querySelector('[data-a=lab]'); if (lab) lab.onclick = () => openLab({ found: true, amount: b.sub + b.fees });
  const mb = body.querySelector('[data-a=mine]'); if (mb) mb.onclick = () => renderBlocks(body, mine[mine.length - 1].h);
  body.querySelector('[data-a=tip]').onclick = () => renderBlocks(body, tip);
}

// ---- charts (real data from mempool.space, cached for offline)
let chartData = null;
try { chartData = JSON.parse(localStorage.getItem('pg-charts') || 'null'); } catch (_) { chartData = null; }
const getLong = (path) => Promise.race([fetch(MP + path, { cache: 'no-store' }).then((r) => { if (!r.ok) throw r.status; return r.json(); }), new Promise((_, bad) => setTimeout(() => bad('timeout'), 15000))]);
async function fetchCharts() {
  if (chartData && Date.now() - chartData.at < 3600e3) return chartData;
  const [hr, fees, rw, da, tipH] = await Promise.all([getLong('/v1/mining/hashrate/1y'), getLong('/v1/mining/blocks/fees/3m'), getLong('/v1/mining/blocks/rewards/1y'), getLong('/v1/difficulty-adjustment'), getLong('/blocks/tip/height')]);
  const hrAt = (t) => { let best = hr.hashrates[0]; for (const p of hr.hashrates) { if (p.timestamp <= t) best = p; else break; } return best.avgHashrate; };
  chartData = {
    at: Date.now(), tip: +tipH, timeAvg: +da.timeAvg || 600000,
    hashrate: hr.hashrates.map((p) => [p.timestamp * 1000, p.avgHashrate / 1e18]),
    difficulty: hr.difficulty.map((p) => [p.time * 1000, p.difficulty / 1e12]),
    fees: fees.map((p) => [p.timestamp * 1000, p.avgFees / SAT]),
    hashprice: rw.map((p) => [p.timestamp * 1000, p.avgRewards / SAT * 144 * p.USD / (hrAt(p.timestamp) / 1e15)]),
  };
  try { localStorage.setItem('pg-charts', JSON.stringify(chartData)); } catch (_) {}
  return chartData;
}
function renderCharts(body) {
  const u = UI();
  const nextHalv = (Math.floor(u[6] / 210000) + 1) * 210000;
  body.innerHTML = `<p class="lead" id="ch-status">Loading a year of real network data from mempool.space…</p><div id="ch"></div>`;
  unlockGoal('charts');
  const paint = (d, stale) => {
    const left = (Math.floor(d.tip / 210000) + 1) * 210000 - d.tip; const when = new Date(Date.now() + left * 600000), pace = new Date(Date.now() + left * d.timeAvg);
    const mY = (t) => t.toLocaleDateString('en-GB', { month: 'long', year: 'numeric' });
    $('#ch-status').innerHTML = `Real Bitcoin network, from mempool.space${stale ? ` (saved ${ago(d.at)}, offline)` : ''}. Your run is at block ${Math.round(u[6]).toLocaleString()}, with its own simulated future.`;
    const hp = d.hashprice[d.hashprice.length - 1][1];
    $('#ch').innerHTML = `
      <div class="tiles">
        <div class="tile"><span>Next halving (real)</span><b class="num">${mY(when)}</b><small class="num">${left.toLocaleString()} blocks at 10 min each${mY(pace) !== mY(when) ? ` · ${mY(pace)} at today's ${(d.timeAvg / 60000).toFixed(1)}-minute pace` : ''}</small></div>
        <div class="tile"><span>Hashprice today</span><b class="num">${money(hp)}/PH/day</b><small>Your run: ${money(u[22] * 1000)}/PH/day</small></div>
        <div class="tile"><span>Next halving in your run</span><b class="num">${(nextHalv - u[6]).toLocaleString()} blocks</b><small>≈ ${Math.round((nextHalv - u[6]) / 144)} game days</small></div>
      </div>
      ${chartCard('hr', 'Network hashrate', 'EH/s, 7-day average', avg7(d.hashrate), '#1FA8A2', (v) => v.toFixed(0) + ' EH/s', false)}
      ${chartCard('df', 'Difficulty', 'trillion', d.difficulty, '#1FA8A2', (v) => v.toFixed(1) + ' T', true)}
      ${chartCard('hp', 'Hashprice', '$ per PH/s per day, 7-day average', avg7(d.hashprice), '#D9711C', (v) => '$' + v.toFixed(1), false)}
      ${chartCard('fe', 'Fees per block, last 3 months', 'BTC', d.fees, '#D9711C', (v) => '₿ ' + v.toFixed(3), false)}`;
    // draw each chart at its real pixel width so text and strokes aren't stretched
    $('#ch').querySelectorAll('.chart').forEach((c) => { const id = c.dataset.id, svg = c.querySelector('svg'); CH[id].px = Math.max(280, c.clientWidth); svg.setAttribute('viewBox', `0 0 ${CH[id].px} 160`); svg.innerHTML = chartSvg(id); wireChart(c); });
    $('#ch').querySelectorAll('[data-tbl]').forEach((b) => b.onclick = () => { const c = b.closest('.ccard'); c.classList.toggle('tbl'); b.textContent = c.classList.contains('tbl') ? 'Chart' : 'Table'; });
    if (left) unlockGoal('halvingdate');
  };
  if (chartData) paint(chartData, true);
  fetchCharts().then((d) => { if ($('#ch')) paint(d, false); }).catch(() => { if (!chartData && $('#ch-status')) $('#ch-status').textContent = 'The charts need a connection to mempool.space. Everything else works offline.'; });
}
const avg7 = (pts) => pts.map((p, i) => { const w = pts.slice(Math.max(0, i - 6), i + 1); return [p[0], w.reduce((a, q) => a + q[1], 0) / w.length]; });
// One single-series line chart: hairline grid, 3 y ticks, crosshair + tooltip, and a table view.
const CH = {};
function chartCard(id, title, unit, pts, color, fmt, step) {
  CH[id] = { pts, fmt, step, color };
  const last = pts[pts.length - 1];
  const rows = pts.filter((_, i) => i % Math.ceil(pts.length / 12) === 0 || i === pts.length - 1).map((p) => `<tr><td>${new Date(p[0]).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: '2-digit' })}</td><td class="num">${fmt(p[1])}</td></tr>`).join('');
  return `<div class="ccard"><div class="ch-head"><div><b>${title}</b><span class="dim"> · ${unit}</span></div><span class="num">${fmt(last[1])}</span><button class="btn mini" data-tbl>Table</button></div>
    <div class="chart" data-id="${id}"><svg viewBox="0 0 600 160" preserveAspectRatio="none" role="img" aria-label="${title}, last value ${fmt(last[1])}">${chartSvg(id)}</svg><div class="ctip" hidden></div></div>
    <table class="ctable"><tbody>${rows}</tbody></table></div>`;
}
function chartSvg(id) {
  const { pts, color, step, fmt } = CH[id]; const W = CH[id].px || 600, H = 160, L = 64, R = 6, T = 10, B = 22;
  const xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]);
  const x0 = Math.min(...xs), x1 = Math.max(...xs); let y0 = Math.min(...ys), y1 = Math.max(...ys); const pad = (y1 - y0) * 0.08 || 1; y0 -= pad; y1 += pad;
  const X = (x) => L + (x - x0) / (x1 - x0 || 1) * (W - L - R), Y = (y) => T + (1 - (y - y0) / (y1 - y0)) * (H - T - B);
  CH[id].X = X; CH[id].Y = Y; CH[id].x0 = x0; CH[id].x1 = x1; CH[id].W = W; CH[id].L = L; CH[id].R = R;
  let d = ''; pts.forEach((p, i) => { d += i ? (step ? `H${X(p[0]).toFixed(1)}V${Y(p[1]).toFixed(1)}` : `L${X(p[0]).toFixed(1)} ${Y(p[1]).toFixed(1)}`) : `M${X(p[0]).toFixed(1)} ${Y(p[1]).toFixed(1)}`; });
  let grid = ''; for (let k = 0; k < 3; k++) { const v = y0 + (y1 - y0) * (0.15 + k * 0.35); grid += `<line x1="${L}" x2="${W - R}" y1="${Y(v)}" y2="${Y(v)}" class="g"/><text x="${L - 8}" y="${Y(v) + 4}" class="t" text-anchor="end">${fmt(v)}</text>`; }
  const months = []; const dt = new Date(x0); dt.setDate(1); dt.setMonth(dt.getMonth() + 1); const span = (x1 - x0) / 864e5 > 120 ? 3 : 1;
  while (+dt < x1) { months.push(+dt); dt.setMonth(dt.getMonth() + span); }
  const xl = months.map((m) => `<text x="${X(m)}" y="${H - 6}" class="t" text-anchor="middle">${new Date(m).toLocaleDateString('en-GB', { month: 'short' })}</text>`).join('');
  const lp = pts[pts.length - 1];
  return `${grid}<line x1="${L}" x2="${W - R}" y1="${H - B}" y2="${H - B}" class="g"/>${xl}<path d="${d}" fill="none" stroke="${color}" stroke-width="2" vector-effect="non-scaling-stroke" stroke-linejoin="round"/><circle cx="${X(lp[0])}" cy="${Y(lp[1])}" r="4" fill="${color}" stroke="var(--panel-solid)" stroke-width="2" vector-effect="non-scaling-stroke"/><line class="xh" x1="0" x2="0" y1="${T}" y2="${H - B}" visibility="hidden"/>`;
}
function wireChart(el_) {
  const c = CH[el_.dataset.id]; const svg = el_.querySelector('svg'); const tipEl = el_.querySelector('.ctip'); const xh = svg.querySelector('.xh');
  const move = (e) => {
    const r = svg.getBoundingClientRect(); const fx = (e.clientX - r.left) / r.width * c.W;
    const t = c.x0 + (fx - c.L) / (c.W - c.L - c.R) * (c.x1 - c.x0);
    let best = c.pts[0]; for (const p of c.pts) if (Math.abs(p[0] - t) < Math.abs(best[0] - t)) best = p;
    const px = c.X(best[0]); xh.setAttribute('x1', px); xh.setAttribute('x2', px); xh.setAttribute('visibility', 'visible');
    tipEl.hidden = false; tipEl.innerHTML = `<span>${new Date(best[0]).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' })}</span><b class="num">${c.fmt(best[1])}</b>`;
    const left = px / c.W * r.width; tipEl.style.left = Math.min(r.width - tipEl.offsetWidth, Math.max(0, left - tipEl.offsetWidth / 2)) + 'px';
  };
  el_.addEventListener('pointermove', move); el_.addEventListener('pointerdown', move);
  el_.addEventListener('pointerleave', () => { tipEl.hidden = true; xh.setAttribute('visibility', 'hidden'); });
}

// ------------------------------------------------------------------ parts, spares and crew
const PARTS = [['Hashboards', 'hashboard', 30], ['Power supplies', 'power supply', 10], ['Fans', 'fan', 40]];
function partsRow(kind, u) {
  const tot = [30, 10, kind === 13 ? 0 : 40]; const d = [u[506], u[507], u[508]];
  const txt = PARTS.map((p, i) => tot[i] ? `<span class="${d[i] ? 'neg' : ''}">${p[0].replace('Power supplies', 'PSUs')} ${tot[i] - d[i]}/${tot[i]}</span>` : '').filter(Boolean).join(' · ');
  return `<div class="row" style="font-size:12px"><span>Parts ${whyBtn('parts')}</span><b class="parts">${txt}</b></div>`;
}
function openCrew() {
  E.sync_ui(); const u = UI(); const dead = u[488] + u[489] + u[490];
  const step = (a, v, max) => `<span class="stepper"><button class="btn mini" data-st="${a}" data-d="-1" ${v <= 0 ? 'disabled' : ''} aria-label="Fewer">−</button><b class="num">${v}</b><button class="btn mini" data-st="${a}" data-d="1" ${v >= max ? 'disabled' : ''} aria-label="More">+</button></span>`;
  openModal(`<div class="mh"><div><h2>Crew & spares</h2><span class="dim">${fmtDate(runDate(u))}</span></div><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <p class="lead">Machines fail a part at a time: hashboards, power supplies, fans. Technicians swap them from your shelf every morning. Without one, every fix is a contractor call-out. ${whyBtn('parts')}</p>
    <div class="tiles">
      <div class="tile"><span>Failed parts now</span><b class="num">${dead}</b><small>${u[510] ? `in ${u[510]} box${u[510] === 1 ? '' : 'es'}: ${u[488]} boards, ${u[489]} PSUs, ${u[490]} fans` : 'Everything is running'}</small></div>
      <div class="tile"><span>Hashrate lost to them</span><b class="num">${hash(u[511])}</b><small>${u[11] > 0 ? ((u[511] / (u[11] + u[511])) * 100).toFixed(1) + '% of your plant' : '—'}</small></div>
      <div class="tile"><span>Wages</span><b class="num">${money(u[497])}/day</b><small>${money(u[498])} paid so far</small></div>
    </div>
    <h3>Crew</h3>
    <div class="rows">
      <div class="row"><span><b style="color:var(--ink)">Technicians</b><br><small class="dim">$${u[502]}/day each. Each swaps up to ${u[504]} parts a day from the shelf and cleans the 10 dustiest boxes.</small></span>${step('tech', u[480], 6)}</div>
      <div class="row"><span><b style="color:var(--ink)">Guards</b><br><small class="dim">$${u[503]}/day each. Each makes raids 25% less likely and may stop one at the gate.</small></span>${step('guard', u[481], 6)}</div>
    </div>
    <h3>Spares</h3>
    <table class="mtable"><thead><tr><th scope="col">Part</th><th scope="col">On the shelf</th><th scope="col">Failed now</th><th scope="col">Price</th><th scope="col">Keep stocked</th><th scope="col"></th></tr></thead><tbody>
      ${PARTS.map((p, i) => `<tr><th scope="row"><b>${p[0]}</b><small>${i === 0 ? '3 per machine' : i === 1 ? '1 per machine' : '4 per machine, none on hydro'}</small></th><td class="num">${u[482 + i]}</td><td class="num ${u[488 + i] ? 'neg' : ''}">${u[488 + i]}</td><td class="num">$${u[499 + i]}</td><td>${step('t' + i, u[485 + i], 500)}</td><td><button class="btn mini" data-buy="${i}">Buy 5 · $${u[499 + i] * 5}</button></td></tr>`).join('')}
    </tbody></table>
    <p class="note">${u[480] > 0 ? 'Every morning your technicians top the shelf up to the "keep stocked" level at list price, then swap power supplies first (a whole machine each), then boards, then fans.' : 'With no technician, nothing restocks or gets swapped on its own. Tap a box and use Fix parts: a contractor charges $' + u[505] + ' a visit, plus 50% on any part not on your shelf.'}</p>
    ${u[491] + u[492] + u[493] > 0 ? `<div class="rows"><div class="row"><span>Failed so far</span><b class="num">${u[491]} boards · ${u[492]} PSUs · ${u[493]} fans</b></div><div class="row"><span>Swapped by your crew</span><b class="num">${u[494]} · ${u[495]} · ${u[496]}</b></div></div>` : ''}
    ${dead ? '<div class="choices" style="flex-direction:row"><button class="btn" data-a="worst">' + ic('wrench') + 'Go to the worst box</button></div>' : ''}`, true);
  const m = $('#modal'); m.classList.add('wide');
  m.querySelector('[data-x]').onclick = closeModal;
  m.querySelectorAll('[data-st]').forEach((b) => b.onclick = () => {
    const a = b.dataset.st, d = +b.dataset.d;
    if (a === 'tech') { E.set_crew(Math.max(0, u[480] + d), u[481]); if (d > 0) unlockGoal('crew'); }
    else if (a === 'guard') { E.set_crew(u[480], Math.max(0, u[481] + d)); if (d > 0) unlockGoal('guards'); }
    else { const i = +a.slice(1); E.set_spare_target(i, Math.max(0, u[485 + i] + d * (i === 2 ? 5 : 1))); }
    sfx('tap'); openCrew();
  });
  m.querySelectorAll('[data-buy]').forEach((b) => b.onclick = () => { E.buy_spares(+b.dataset.buy, 5); sfx('cash'); E.sync_ui(); drainToasts(UI()); openCrew(); });
  const w = m.querySelector('[data-a=worst]'); if (w) w.onclick = () => { if (E.select_worst_parts()) { E.sync_ui(); closeModal(); openInspector(); } };
}

// ------------------------------------------------------------------ power contracts
const CONTRACTS = [
  { t: 'Fixed price', d: (u) => `$${u[305]}/MWh, every hour. Your supplier carries the risk. In a grid emergency you may be offered money to switch off.` },
  { t: 'Spot price', d: (u, lo, hi) => `About $${Math.round(u[357])}/MWh today on average: $${Math.round(lo)} overnight, $${Math.round(hi)} late afternoon. In an emergency it spikes into the thousands.` },
  { t: 'Fixed + demand response', d: (u) => `$${u[305]}/MWh, with $${u[358]} back on every MWh you draw. When the grid calls, you're switched off automatically, and paid.` },
];
const CAPS = [[0, 'Break-even'], [50, '$50'], [100, '$100'], [250, '$250'], [1e6, 'No limit']];
function openContract() {
  E.sync_ui(); const u = UI(); const c = u[346], left = u[351];
  const hrs = [...u.slice(360, 384)]; const lo = Math.min(...hrs), hi = Math.max(...hrs), now = u[384];
  const owned0 = [10, 11, 12, 13].filter((k) => u[312 + k - 10] > 0); const worst = owned0.length ? owned0[0] : 0;
  const beW = worst ? econ(worst, u).breakeven : 0;
  const fixed = u[305]; const top = Math.max(hi, fixed, beW < hi * 3 ? beW : 0) * 1.15;
  const bars = hrs.map((v, h) => `<i class="${h === now ? 'now' : ''}" style="height:${(v / top * 100).toFixed(1)}%" title="${String(h).padStart(2, '0')}:00 · $${v.toFixed(0)}/MWh"></i>`).join('');
  const owned = [10, 11, 12, 13].filter((k) => u[312 + k - 10] > 0);
  const be = owned.map((k) => `<div class="row"><span>${SHORT[k].replace(/s$/, '')} breaks even at</span><b class="num">$${Math.round(econ(k, u).breakeven)}/MWh</b></div>`).join('');
  const capOn = (v) => v === 0 ? u[348] <= 0 : Math.abs(u[348] - v) < 0.5;
  openModal(`<div class="mh"><div><h2>Power contract</h2><span class="dim">Grid hookup · ${fmtDate(runDate(u))}</span></div><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <p class="lead">Power is most of what mining costs, and how you buy it matters as much as the price. ${whyBtn('contract')}</p>
    ${u[516] ? '' : `<p class="note">${SITES_UI[u[512]].t} has no power market. ${u[512] === 5 ? 'Your power comes from the well, at a fixed cost.' : 'Miners sign a fixed contract with the state utility.'} Spot and demand response are Texas things. ${whyBtn('site')}</p>`}
    <div class="diffs">${CONTRACTS.map((k, i) => `<button class="btn ${i === c ? 'on' : ''}" data-c="${i}" aria-pressed="${i === c}" ${!u[516] && i ? 'disabled' : ''}><span class="px" style="font-size:13px;color:${i === c ? 'var(--volt)' : 'var(--ink)'}">${k.t}</span><small>${k.d(u, lo, hi)}</small></button>`).join('')}</div>
    <p class="note">${left > 0 ? `You're on this contract for ${left} more day${left === 1 ? '' : 's'}. Every change runs at least 30 days.` : 'You can change now. A new contract runs at least 30 days.'}</p>
    <h3>Spot price today <span class="dim" style="font-weight:400">· now $${Math.round(u[347]).toLocaleString()}/MWh, highlighted</span></h3>
    <div class="spotchart" role="img" aria-label="Spot price by hour today, from $${Math.round(lo)} to $${Math.round(hi)} per MWh">
      <div class="bars">${bars}<span class="ref" style="bottom:${(fixed / top * 100).toFixed(1)}%"><em>fixed $${fixed}</em></span>${beW > 0 && beW < top ? `<span class="ref be" style="bottom:${(beW / top * 100).toFixed(1)}%"><em>${SHORT[worst].replace(/s$/, '')} break-even $${Math.round(beW)}</em></span>` : ''}</div>
      <div class="hrs"><span>00</span><span>06</span><span>12</span><span>18</span><span>24</span></div>
    </div>
    ${c === 1 ? `<h3>Switch off when power costs more than</h3>
      <div class="seg" role="radiogroup" aria-label="Price limit">${CAPS.map(([v, t]) => `<button role="radio" data-cap="${v}" aria-checked="${capOn(v)}" aria-selected="${capOn(v)}">${t}</button>`).join('')}</div>
      <p class="note">${u[348] <= 0 ? 'Break-even: each box switches itself off when an hour of power costs more than it mines at today\'s hashprice, and back on when the price falls.' : u[348] >= 1e5 ? 'No limit: everything runs whatever power costs, including through a spike of thousands per MWh.' : `Above $${Math.round(u[348])}/MWh the whole grid feed goes dark, whatever each box earns.`}</p>` : ''}
    ${be ? `<div class="rows">${be}</div>` : ''}
    ${u[386] > 0.05 || u[355] > 0 ? `<h3>On this contract</h3><div class="rows">
      <div class="row"><span>Average paid since day ${u[359] + 1}</span><b class="num">$${u[354].toFixed(1)}/MWh</b></div>
      ${c === 1 ? `<div class="row"><span>Hours boxes sat out a high price</span><b class="num">${Math.round(u[356])}</b></div>` : ''}
      ${u[355] > 0 ? `<div class="row"><span>Demand-response standby credits</span><b class="num">${money(u[355])}</b></div>` : ''}
      ${u[303] > 0 ? `<div class="row"><span>Paid for switching off (all contracts)</span><b class="num">${money(u[303])}</b></div>` : ''}
    </div>` : ''}`, true);
  const m = $('#modal'); m.classList.add('wide');
  m.querySelector('[data-x]').onclick = closeModal;
  m.querySelectorAll('[data-c]').forEach((b) => b.onclick = () => { if (E.set_contract(+b.dataset.c)) { sfx('upgrade'); if (+b.dataset.c === 1) tip('spot'); } E.sync_ui(); drainToasts(UI()); openContract(); });
  m.querySelectorAll('[data-cap]').forEach((b) => b.onclick = () => { E.set_spot_cap(+b.dataset.cap); sfx('tap'); openContract(); });
}

// ------------------------------------------------------------------ ASIC market: machine prices follow hashprice
const SHORT = { 10: 'S19j Pros', 11: 'S19 XPs', 12: 'S21s', 13: 'S21 XP Hydros' };
const listPct = (u) => Math.pow(Math.max(u[321], 0.01), 0.75) * 100; // new machines, % of list price
// The market index is recorded once a game day, per run, so the sheet can chart it.
let mktHist = { id: '', pts: [] };
try { mktHist = JSON.parse(localStorage.getItem('pg-mkt') || 'null') || mktHist; } catch (_) {}
function runDate(u) {
  const sc = scenOf(u[318]); if (sc && HIST[sc.key]) return scenDate(sc, Math.min(u[15], HIST[sc.key].rows.length - 1));
  return new Date(Date.UTC(2026, 10, 13, 12) + (u[6] - 975493) * 6e5); // block 975,493 was mined on 13 Nov 2026
}
function recordMarket(u) {
  const id = `${u[35]}:${u[318]}:${u[34]}`; const last = mktHist.pts[mktHist.pts.length - 1];
  if (mktHist.id !== id || (last && u[15] < last[0])) mktHist = { id, pts: [] };
  else if (last && last[0] === u[15]) return;
  mktHist.pts.push([u[15], +runDate(u), +u[321].toFixed(4)]);
  if (mktHist.pts.length > 800) mktHist.pts = mktHist.pts.filter((_, i) => i % 2 === 0);
}
function saveMarket() { try { localStorage.setItem('pg-mkt', JSON.stringify(mktHist)); } catch (_) {} }
// Same model as game.rs: new = list x index^0.75, used = new x (0.1 + 0.8 x margin at $35/MWh), clamped.
const MSPEC = { 10: [1040, 30.7], 11: [1410, 30.1], 12: [2000, 35], 13: [4730, 56.8] };
function usedAt(kind, idx, list) {
  const [th, kw] = MSPEC[kind]; const margin = 1 - kw * 24 / 1000 * 35 / (th * idx * 0.035);
  return list * Math.pow(idx, 0.75) * Math.min(0.9, Math.max(0.08, 0.1 + 0.8 * margin));
}
let mktModel = 10;
function openMarket() {
  E.sync_ui(); const u = UI(); recordMarket(u);
  const list = (k) => u[322 + k - 10] / Math.pow(Math.max(u[321], 0.01), 0.75);
  const nextList = 7 - (u[15] % 7);
  const rowsHtml = [10, 11, 12, 13].map((k) => {
    const m = k - 10, ec = econ(k, u), n = u[330 + m], locked = u[430 + k] <= 0;
    return `<tr class="${locked ? 'lk' : ''}"><th scope="row"><b>${SHORT[k].replace(/s$/, '')}</b><small>${(MSPEC[k][1] * 1000 / MSPEC[k][0]).toFixed(1)} J/TH</small></th>
      <td class="num">${money(u[322 + m])}</td><td class="num">${n ? money(u[326 + m]) : '—'}</td><td class="num">${n || '0'}</td>
      <td class="num ${ec.profit < 0 ? 'neg' : ''}">${ec.profit < 0 ? '−' : '+'}${money(Math.abs(ec.profit))}</td></tr>`;
  }).join('');
  openModal(`<div class="mh"><div><h2>Machine market</h2><span class="dim">10-packs · ${fmtDate(runDate(u))}</span></div><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <p class="lead">Machine prices follow hashprice. When mining pays, boxes sell out. When it doesn't, miners sell whatever no longer covers its power, and old models crash hardest. ${whyBtn('market')}</p>
    <div class="tiles">
      <div class="tile"><span>Market hashprice</span><b class="num">${money(u[321] * 35)}/PH/day</b><small>Today ${money(u[22] * 1000)}. Prices react over a few days.</small></div>
      <div class="tile"><span>New machines</span><b class="num">${Math.round(listPct(u))}% of list</b><small>New used listings in ${nextList} day${nextList === 1 ? '' : 's'}</small></div>
      <div class="tile"><span>Your machines</span><b class="num">${money(u[320])}</b><small>${u[42] ? `What a broker would pay for your ${u[42]}, after a 12% cut` : 'You don\'t own any yet'}</small></div>
    </div>
    <table class="mtable"><thead><tr><th scope="col">Machine</th><th scope="col">New</th><th scope="col">Used</th><th scope="col">For sale</th><th scope="col">Profit/day at your power</th></tr></thead><tbody>${rowsHtml}</tbody></table>
    <div id="mk-chart"></div>
    <p class="note">Used machines arrive at 45–80% health and dusty. Each health point below 100 costs 0.3% of hashrate, and a service restores it. To buy one, pick the machine in the build bar and switch the bar to Used.</p>
    <div class="choices" style="flex-direction:row;flex-wrap:wrap"><button class="btn" data-a="sellold">${ic('chip')}Sell my least efficient machine</button></div>`, true);
  const m = $('#modal'); m.classList.add('wide');
  m.querySelector('[data-x]').onclick = closeModal;
  const worst = () => { for (const k of [10, 11, 12, 13]) if (u[312 + k - 10] > 0) return k; return 0; };
  const so = m.querySelector('[data-a=sellold]'); const wk = worst();
  if (!wk) so.hidden = true; else so.innerHTML = `${ic('chip')}Sell one ${SHORT[wk].replace(/s$/, '')} to a broker`;
  so.onclick = () => { if (E.select_kind(wk, 0)) { E.sync_ui(); closeModal(); openInspector(); } };
  const chart = () => {
    const box = $('#mk-chart'); if (!box) return;
    const pts = mktHist.pts.map((p) => [p[1], usedAt(mktModel, p[2], list(mktModel))]);
    const seg = `<div class="seg" role="tablist" aria-label="Machine">${[10, 11, 12, 13].map((k) => `<button role="tab" data-k="${k}" aria-selected="${k === mktModel}">${SHORT[k].replace(/s$/, '')}</button>`).join('')}</div>`;
    if (pts.length < 3) { box.innerHTML = `<h3>Used price</h3>${seg}<p class="dim">The chart fills in as days pass. Check back in a few days.</p>`; }
    else {
      box.innerHTML = `<h3>Used price</h3>${seg}` + chartCard('mk', SHORT[mktModel].replace(/s$/, '') + ', used', '10-pack, perfect condition', pts, '#D9711C', (v) => money(v), false);
      const c = box.querySelector('.chart'), svg = c.querySelector('svg'); CH.mk.px = Math.max(280, c.clientWidth); svg.setAttribute('viewBox', `0 0 ${CH.mk.px} 160`); svg.innerHTML = chartSvg('mk'); wireChart(c);
      box.querySelector('[data-tbl]').onclick = (e) => { const cc = e.target.closest('.ccard'); cc.classList.toggle('tbl'); e.target.textContent = cc.classList.contains('tbl') ? 'Chart' : 'Table'; };
    }
    box.querySelectorAll('[data-k]').forEach((b) => b.onclick = () => { mktModel = +b.dataset.k; sfx('tap'); chart(); });
  };
  chart();
}

// ------------------------------------------------------------------ history: replay real moments with real daily data
// Rows come from history.js (real price, difficulty and fees per day). The plant, the machine prices and the
// grid events' exact timing are the game's; everything Bitcoin pays you is what it paid on that day.
const HIST = window.PG_HISTORY || {};
const SCEN = [
  { key: 'uri', id: 1, t: 'Winter Storm Uri', when: 'Texas, February 2021', usd: 30, cash: 40000, max: 10, plant: [10, 36, 10, 0],
    star3: { t: 'Did what Texas miners did', goal: 'switch off for every call the grid makes during the storm.', ok: (u) => u[302] >= 5 },
    brief: 'You run 36 containers of S19-class machines in Texas on a fixed $30/MWh contract. Bitcoin is $33,500 and climbing. A deep freeze is coming, and the grid is about to need every megawatt it can get.',
    goal: 'End February with more cash and bitcoin than you started with. For the third star, switch off for every storm call.',
    real: 'From 14 to 19 February, Winter Storm Uri knocked out about half of Texas\'s power plants. ERCOT ordered rotating outages, about 4.5 million homes and businesses lost power, and the real-time price sat at the $9,000/MWh cap for days. Almost every bitcoin miner in Texas switched off. A megawatt-hour not used was worth far more to the grid than any bitcoin it could mine. Afterwards, flexible loads like miners became a regular part of ERCOT\'s demand-response programmes.',
    game: 'Prices, difficulty and fees are the real daily numbers, and machine prices follow that day\'s hashprice. The calls, the cold and the outage are scripted to the storm\'s dates; the exact hours are the game\'s.' },
  { key: 'china', id: 2, t: 'The great migration', when: 'May to July 2021', usd: 40, cash: 40000, max: 10, plant: [10, 36, 10, 0],
    star3: { t: 'Grew while others went dark', goal: 'finish July above 50 PH/s.', ok: (u) => u[11] >= 50000 },
    brief: 'Your plant is in the US, on $40/MWh power. Bitcoin is near $58,000 and half of the world\'s hashrate is in China. That is about to change. Mining pays so well right now that even $240/MWh diesel makes money.',
    goal: 'End July with more cash and bitcoin than you started with. For the third star, grow past 50 PH/s while the competition is offline.',
    real: 'In May 2021 China\'s State Council called for a crackdown on bitcoin mining, and by late June the big mining provinces had ordered miners to shut down. Roughly half the network\'s hashrate went dark within weeks. On 3 July difficulty fell 27.9%, the biggest drop on record, so every machine still running outside China earned far more per terahash. Container loads of ASICs were shipped to the US, Kazakhstan and Russia. By early 2022 hashrate was back at its old high, and the US had become the largest mining country.',
    game: 'The replay uses the real daily price, difficulty and fees. Machine prices follow hashprice in the game\'s simple model; in 2021 buyers also paid for months of waiting, so real prices ran even higher.' },
  { key: 'bear', id: 3, t: 'Crypto winter', when: 'April to December 2022', usd: 65, cash: 60000, max: 11, plant: [10, 24, 11, 12],
    star3: { t: 'Came out ahead', goal: 'end the year worth more than if you had sold everything for bitcoin on day one.', ok: (u) => u[0] + u[1] * u[2] + u[320] > u[49] * u[2] },
    brief: 'Bitcoin is $46,000 and your 36 containers are printing money. Like many miners in 2021, you bought them with a loan: $28,000 is due on the first of every month. Power is getting expensive too: your contract is $65/MWh.',
    goal: 'Make it to New Year\'s Eve without being repossessed. Every month, $28,000 leaves your account whatever bitcoin is worth.',
    real: '2022 broke a lot of miners. Terra collapsed in May, Celsius froze withdrawals in June and FTX failed in November, and bitcoin fell from about $46,000 to under $16,000. Difficulty kept climbing anyway as machines ordered during the boom arrived, so hashprice hit record lows while power prices rose. Miners who had borrowed against machines or bitcoin were squeezed hardest: Core Scientific, then the biggest public miner, filed for bankruptcy in December. Survivors switched off old machines, sold bitcoin and hardware, and renegotiated power. Used ASIC prices fell more than 80% from their peak.',
    game: 'The real daily price, difficulty and fees drive the replay, and machine prices fall with hashprice, as they really did. The loan is the game\'s, sized to hurt.' },
  { key: 'halving', id: 4, t: 'The fourth halving', when: 'April to June 2024', usd: 45, cash: 50000, max: 12, plant: [10, 24, 12, 10],
    brief: 'You run 24 containers of S19j Pros and 10 of brand-new S21s on $45/MWh power. Bitcoin is near $70,000. On 20 April the subsidy halves from 6.25 to 3.125 BTC.',
    star3: { t: 'Won the efficiency race', goal: 'run at least 20 containers of S21s by the end of June.', ok: (u) => u[314] >= 20 },
    goal: 'Get through the halving and the summer after it with more cash and bitcoin than you started with. For the third star, upgrade to at least 20 S21 containers.',
    real: 'Block 840,000 was mined on 20 April 2024 and cut the subsidy to 3.125 BTC. The same block launched the Runes protocol, and the rush to get in paid about 37.6 BTC in fees, twelve times the new subsidy. Within days fees fell back, and hashprice halved to record lows. Miners switched off many older S19-class machines; those with cheap power and new hardware kept running. Several public miners began turning parts of their sites over to AI computing, which paid more per megawatt.',
    game: 'The halving lands on the real day, and the daily price, difficulty and fees are real, including the fee spike. Machine prices follow hashprice, so S21s get cheaper after the halving. Underclocking the S19j Pros is another way through.' },
];
let histBest = {};
try { histBest = JSON.parse(localStorage.getItem('pg-history') || '{}') || {}; } catch (_) { histBest = {}; }
const scenOf = (id) => SCEN.find((s) => s.id === id);
const scenDate = (sc, day) => { const d = new Date(HIST[sc.key].start + 'T12:00:00Z'); d.setUTCDate(d.getUTCDate() + day); return d; };
const fmtDate = (d) => d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric', timeZone: 'UTC' });
function pushScenRows(sc) {
  const rows = HIST[sc.key].rows; const p = E.scen_buf(rows.length); const f = new Float64Array(mem(), p, rows.length * 5);
  rows.forEach((r, i) => { f[i * 5] = r[0]; f[i * 5 + 1] = r[1] * 1e12; f[i * 5 + 2] = r[2]; f[i * 5 + 3] = r[3]; f[i * 5 + 4] = r[4]; });
}
function scenBriefing(sc) {
  const h = HIST[sc.key]; if (!h) { toast('Missing data', 'The history file did not load.', 'var(--alarm)'); return; }
  const r0 = h.rows[0];
  openModal(`<div class="mh"><div><h2>${sc.t}</h2><span class="dim">${sc.when} · ${h.rows.length} days of real data</span></div><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <p class="lead">${sc.brief}</p>
    <div class="rows">
      <div class="row"><span>Starts</span><b class="num">${fmtDate(scenDate(sc, 0))} · block ${h.height.toLocaleString()}</b></div>
      <div class="row"><span>Bitcoin</span><b class="num">${money(r0[0])}</b></div>
      <div class="row"><span>Difficulty</span><b class="num">${r0[1].toFixed(1)} T</b></div>
      <div class="row"><span>Power contract</span><b class="num">$${sc.usd}/MWh</b></div>
      <div class="row"><span>Cash</span><b class="num">${money(sc.cash)}</b></div>
      <div class="row tot"><span>Goal</span><b>${sc.goal}</b></div>
    </div>
    <p>No random events: just the market as it really was, plus anything the history brings. Your plant can be changed like any other.</p>
    <div class="choices" style="flex-direction:row"><button class="btn btn-p" data-a="go">${ic('play')}Start ${fmtDate(scenDate(sc, 0))}</button><button class="btn" data-a="back">Back</button></div>`, true);
  const m = $('#modal');
  m.querySelector('[data-x]').onclick = m.querySelector('[data-a=back]').onclick = () => newRunScreen();
  m.querySelector('[data-a=go]').onclick = () => startScenario(sc);
}
function startScenario(sc) {
  clearSave(); E.set_site(0); E.new_game(0x51CE0000 + sc.id, 1); pushScenRows(sc);
  const [ka, na, kb, nb] = sc.plant; E.scen_start(sc.id, HIST[sc.key].height, sc.usd, sc.cash, sc.max, ka, na, kb, nb);
  seenTips = { start: 1, hashrate: 1, grow: 1 }; closeModal(); afterStart(); startAudio(); E.sync_ui();
  toast(sc.t, `${fmtDate(scenDate(sc, 0))}. ${sc.goal}`, 'var(--volt)', '', 'book');
}
// A loaded scenario save needs its rows back (they live here, not in the save).
function restoreScenario() { const id = UI()[318]; const sc = id > 0 && scenOf(id); if (sc && HIST[sc.key]) pushScenRows(sc); }
function scenPill(u) {
  const sc = scenOf(u[318]); if (!sc) return null; const n = HIST[sc.key].rows.length; const d = Math.min(u[15], n - 1);
  return { text: `${fmtDate(scenDate(sc, d))} · day ${Math.min(u[15] + 1, n)} of ${n}`, pct: Math.min(100, u[15] / n * 100) };
}
function showDebrief(u, outcome) {
  const sc = scenOf(u[318]); if (!sc) return false;
  // Scored on cash + bitcoin + what a broker would pay for the machines, at that day's market prices.
  const w0 = u[319], w1 = u[0] + u[1] * u[2] + u[320], hold = u[49] * u[2], ch = w1 / w0 - 1, holdCh = hold / w0 - 1;
  const third = sc.star3 ? sc.star3.ok(u) : w1 > hold;
  const stars = outcome === 'done' ? 1 + (w1 > w0) + (w1 > w0 && third) : 0;
  const label = ['Didn\'t make it', 'Survived', 'Made money', sc.star3 ? sc.star3.t : 'Beat holding bitcoin'][stars];
  if (stars > (histBest[sc.key] || 0)) { histBest[sc.key] = stars; try { localStorage.setItem('pg-history', JSON.stringify(histBest)); } catch (_) {} }
  if (outcome === 'done') unlockGoal('h_' + sc.key);
  const pct = (x) => (x >= 0 ? '+' : '−') + Math.abs(x * 100).toFixed(1) + '%';
  openModal(`<div class="mh"><div><h2>${outcome === 'done' ? 'Debrief' : 'Repossessed'}</h2><span class="dim">${sc.t} · ${sc.when}</span></div></div>
    <div class="stars" aria-label="${stars} of 3">${[1, 2, 3].map((k) => `<i class="${k <= stars ? 'on' : ''}"></i>`).join('')}<span class="dim">${label}</span></div>
    <div class="rows">
      <div class="row"><span>Cash, bitcoin and machines at the start</span><b class="num">${money(w0)}</b></div>
      <div class="row"><span>${outcome === 'done' ? 'At the end' : 'When the bank came'}</span><b class="num">${money(w1)} <span class="${ch >= 0 ? '' : 'neg'}">${pct(ch)}</span></b></div>
      <div class="row dim"><span>of which machines</span><b class="num">${money(u[320])}</b></div>
      <div class="row"><span>If you had sold it all for bitcoin on day one</span><b class="num">${money(hold)} <span class="${holdCh >= 0 ? '' : 'neg'}">${pct(holdCh)}</span></b></div>
    </div>
    <p class="note">Bitcoin is valued at the last day's real price, machines at what a broker would pay that day.${sc.star3 ? ` Third star: ${sc.star3.goal}` : ' Third star: beat selling everything for bitcoin.'}</p>
    <h3>What real miners did</h3><p>${sc.real}</p>
    <h3>What this replay simplifies</h3><p>${sc.game}</p>
    <div class="choices" style="flex-direction:row"><button class="btn btn-p" data-a="again">Try again</button><button class="btn" data-a="home">Back to the garage</button></div>`);
  const m = $('#modal');
  m.querySelector('[data-a=again]').onclick = () => { overShown = false; scenBriefing(sc); };
  m.querySelector('[data-a=home]').onclick = () => { overShown = false; newRunScreen(); };
  return true;
}
function historyList() {
  return `<div><span class="lbl">Replay history</span><div class="hist">${SCEN.map((sc) => { const b = histBest[sc.key] || 0;
    return `<button class="hrow" data-h="${sc.key}"><span><b>${sc.t}</b><small>${sc.when}</small></span><span class="stars mini">${[1, 2, 3].map((k) => `<i class="${k <= b ? 'on' : ''}"></i>`).join('')}</span></button>`; }).join('')}</div></div>`;
}

// ------------------------------------------------------------------ live mempool: transactions arrive, sort by fee rate, get mined
// One square is a bundle of about 30 transactions at a similar fee rate. A block takes 120 squares (1 MvB).
function animateMempool(cv, mp) {
  const ctx2 = cv.getContext('2d'); const dpr = Math.max(1, devicePixelRatio || 1);
  const W = cv.clientWidth, H = 230; cv.width = W * dpr; cv.height = H * dpr; ctx2.scale(dpr, dpr);
  const CELL = 12, GAP = 2, CAP = 120, POOLW = Math.max(160, Math.floor((W - 150) / (CELL + GAP)) * (CELL + GAP)), COLS = POOLW / (CELL + GAP);
  const all = mp.blocks;
  // a fee rate drawn from one projected block's percentiles
  const fromBlock = (b) => { const R = b.feeRange; const q = Math.random() * (R.length - 1), i = Math.floor(q); return R[i] * Math.pow(R[Math.min(i + 1, R.length - 1)] / Math.max(R[i], 1e-6), q - i); };
  // newcomers mostly bid for the next couple of blocks, like real arrivals
  const sample = () => { const r = Math.random(); return fromBlock(all[Math.min(all.length - 1, r < 0.5 ? 0 : r < 0.8 ? 1 : 2)]); };
  const pool = []; let id = 0;
  // start with the real (or modelled) queue, block by block, so the top 120 squares really are the next block
  all.slice(0, 3).forEach((b) => { const n = Math.round(CAP * b.blockVSize / 1e6); for (let k = 0; k < n && pool.length < COLS * 9; k++) pool.push({ id: id++, rate: fromBlock(b), x: 0, y: 0, born: -1 }); });
  const chain = []; let t0 = performance.now(), last = t0, nextBlock = 8000, flying = null;
  const arrive = CAP / 8 * (0.85 + Math.min(all.length, 10) * 0.05); // squares per second
  let acc = 0, hover = null;
  const layout = () => {
    pool.sort((a, b) => b.rate - a.rate);
    pool.forEach((s, k) => {
      const tx = (k % COLS) * (CELL + GAP), ty = 26 + Math.floor(k / COLS) * (CELL + GAP);
      if (s.tx !== undefined && ty !== s.ty) s.x = tx; // wrapping to another row: slide vertically, don't fly across
      s.tx = tx; s.ty = ty;
    });
  };
  layout(); pool.forEach((s) => { s.x = s.tx; s.y = s.ty; });
  cv.onpointermove = (e) => { const r = cv.getBoundingClientRect(); const x = e.clientX - r.left, y = e.clientY - r.top; hover = pool.find((s) => x >= s.x && x < s.x + CELL && y >= s.y && y < s.y + CELL) || null; };
  cv.onpointerleave = () => { hover = null; };
  const frame = (now) => {
    if (!document.body.contains(cv)) return;
    const dt = Math.min(0.1, (now - last) / 1000); last = now;
    acc += dt * arrive; while (acc >= 1 && pool.length < COLS * 13) { acc -= 1; pool.push({ id: id++, rate: sample(), x: Math.random() * POOLW, y: -CELL, born: now }); layout(); }
    if (now - t0 > nextBlock && !flying) {
      const take = pool.splice(0, CAP); const rates = take.map((s) => s.rate);
      flying = { squares: take, t: now, min: Math.min(...rates), med: rates[Math.floor(rates.length / 2)] };
      t0 = now; nextBlock = 6000 + Math.random() * 6000; layout();
    }
    for (const s of pool) { s.x += (s.tx - s.x) * Math.min(1, dt * 14); s.y += (s.ty - s.y) * Math.min(1, dt * 14); }
    ctx2.clearRect(0, 0, W, H);
    ctx2.font = '12px Rubik, sans-serif';
    ctx2.fillStyle = '#BDB7A8'; ctx2.fillText(`Mempool · ${pool.length} squares waiting`, 0, 14);
    if (pool.length > CAP) { const lbl = `above the line: next block, ${satvb(pool[CAP - 1].rate)} sat/vB and up`; ctx2.fillStyle = '#F5F0E6'; ctx2.fillText(lbl, Math.max(0, POOLW - ctx2.measureText(lbl).width), 14); }
    for (const s of pool) { ctx2.fillStyle = FEE_RAMP[binOf(s.rate)]; ctx2.fillRect(s.x, s.y, CELL, CELL); }
    // the cut line: the first 120 squares (highest fee rates) fit in the next block; it steps where the block ends mid-row
    if (pool.length > CAP) {
      const STEP = CELL + GAP, r = Math.floor(CAP / COLS), c = CAP % COLS, top = 26 + r * STEP - GAP / 2, bot = top + STEP;
      ctx2.strokeStyle = '#F5F0E6'; ctx2.lineWidth = 2; ctx2.beginPath();
      if (c === 0) { ctx2.moveTo(0, top); ctx2.lineTo(POOLW, top); } else { ctx2.moveTo(0, bot); ctx2.lineTo(c * STEP - GAP / 2, bot); ctx2.lineTo(c * STEP - GAP / 2, top); ctx2.lineTo(POOLW, top); }
      ctx2.stroke(); ctx2.lineWidth = 1;
    }
    // the chain on the right
    const cx = POOLW + 24; ctx2.fillStyle = '#BDB7A8'; ctx2.fillText('Mined', cx, 14);
    chain.slice(-4).forEach((b, k, arr) => { const y = 26 + (arr.length - 1 - k) * 50; ctx2.fillStyle = FEE_RAMP[binOf(b.med)]; ctx2.fillRect(cx, y, 40, 40); ctx2.fillStyle = '#F5F0E6'; ctx2.fillText(`${satvb(b.min)}+ sat/vB`, cx + 48, y + 16); ctx2.fillStyle = '#BDB7A8'; ctx2.fillText(k === arr.length - 1 ? 'just now' : `${(arr.length - 1 - k) * 10} min ago`, cx + 48, y + 32); });
    if (flying) {
      const p = Math.min(1, (now - flying.t) / 700);
      flying.squares.forEach((s, k) => { const tx = cx + (k % 10) * 4, ty = 26 + Math.floor(k / 10) * 3.3; ctx2.fillStyle = FEE_RAMP[binOf(s.rate)]; const x = s.x + (tx - s.x) * p, y = s.y + (ty - s.y) * p, sz = CELL + (4 - CELL) * p; ctx2.fillRect(x, y, sz, sz); });
      if (p >= 1) { chain.push({ min: flying.min, med: flying.med }); flying = null; sfx('tap'); }
    }
    if (hover) { const txt = `~30 txs at ${satvb(hover.rate)} sat/vB`; const w = ctx2.measureText(txt).width + 12; const x = Math.min(W - w, hover.x), y = Math.max(0, hover.y - 22); ctx2.fillStyle = '#0d100d'; ctx2.fillRect(x, y, w, 18); ctx2.fillStyle = '#F5F0E6'; ctx2.fillText(txt, x + 6, y + 13); }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

// ------------------------------------------------------------------ HUD
let shownEvent = -1, overShown = false, goalTick = 0;
function updateUI() {
  const u = UI();
  $('#h-cash .v').textContent = money(u[0]);
  $('#h-cash .v').style.color = u[0] < 0 ? 'var(--alarm)' : 'var(--cash)';
  $('#h-btc .v').textContent = btcf(u[1]);
  const net = u[3]; const nv = $('#h-net .v');
  nv.textContent = (net < 0 ? '▼ ' : '▲ ') + money(net); nv.style.color = net < 0 ? 'var(--alarm)' : 'var(--ink)';
  $('#h-hash .v').textContent = hash(u[11]);
  { const sh = u[12] > 0 ? u[11] / u[12] * 100 : 0; const t = `Your hashrate: ${hash(u[11])}, ${sh >= 0.01 ? sh.toFixed(2) : sh.toPrecision(1)}% of the network's ${hash(u[12])}`; const c = $('#h-hash'); if (c.title !== t) c.title = t; }
  $('#h-pow .v').textContent = `${power(u[4])} / ${power(u[5])}`;
  { const l = u[346] === 1 ? `Power · $${Math.round(u[347]).toLocaleString('en-US')}` : 'Power'; const le = $('#h-pow .lbl'); if (le.textContent !== l) le.textContent = l; }
  $('#h-pow .v').style.color = u[245] > 0.5 ? 'var(--alarm)' : 'var(--volt)';
  $('#h-px .v').textContent = money(u[2]);
  if (running && !u[21]) recordMarket(u);
  $('#h-px .lbl').innerHTML = u[318] ? 'BTC then' : u[241] ? '<i class="live"></i>BTC live' : 'BTC sim';
  const toHalv = u[8], toRet = u[7];
  if (toHalv < 2016) { $('#p-text').innerHTML = `Halving in <b class="num">${toHalv}</b> blocks`; $('#p-bar').style.width = (100 - toHalv / 20.16) + '%'; $('#p-bar').style.background = 'var(--btc)'; }
  const WX = (u[318] ? '' : SITES_UI[u[512]].short + ' · ') + ic(['sun', 'rain', 'storm', 'wind'][u[284]]) + ` ${u[16].toFixed(0)}°`;
  applySite(u);
  { const sb = $('#b-sites'); const hide = !!u[318]; if (sb.hidden !== hide) sb.hidden = hide; if (sb.dataset.n !== String(u[562])) sb.dataset.n = u[562] > 1 ? u[562] : ''; }
  if (toHalv < 2016) $('#p-text').innerHTML = WX + ' · ' + $('#p-text').innerHTML;
  const sp = scenPill(u);
  if (sp) { $('#p-text').innerHTML = `${WX} · ${sp.text}${toHalv < 2016 ? ` · halving in <b class="num">${toHalv}</b>` : ''}`; $('#p-bar').style.width = sp.pct + '%'; $('#p-bar').style.background = 'var(--hash)'; }
  else { $('#p-text').innerHTML = `${WX} · Block ${u[6].toLocaleString()} · retarget in <b class="num">${toRet}</b>${u[241] && u[243] && !u[292] ? ` <span class="dim">(est ${u[243] > 0 ? '+' : ''}${(u[243] * 100).toFixed(1)}%)</span>` : ''}`; $('#p-bar').style.width = (100 - toRet / 20.16) + '%'; $('#p-bar').style.background = 'var(--volt)'; }
  drainToasts(u);
  checkSkip(u);
  if (pendingLab && $('#modal-wrap').hidden) { pendingLab = 0; const was = speed; setSpeed(0); openNetwork('blocks'); const w = setInterval(() => { if ($('#modal-wrap').hidden) { clearInterval(w); setSpeed(was || 1); } }, 300); }
  renderStatus(u);
  if (u[15] >= 365 && !overShown) yearCheck(u);
  renderHoverTip(u);
  ambience(u);
  if (inspOpen) { if (u[80] < 0) closeInspector(); else if (++inspTick % 6 === 0) renderInspector(false); else positionInspector(); }
  if (tool >= 0 && !$('#placebar').hidden) showPlacebar();
  if (u[40] > 0) tip('heat');
  if (++goalTick % 10 === 0) checkGoals(u);
  // events
  const pend = u[18];
  if (pend >= 0 && shownEvent !== pend) { shownEvent = pend; showEvent(pend, u); }
  if (pend < 0) shownEvent = -1;
  if (u[21] > 0 && !overShown) { overShown = true; showOver(u[21]); }
  if (dockDirty) { dockDirty = false; renderDock(); }
}
let dockDirty = true, inspTick = 0, statusHtml = '', pendingLab = 0;
function renderHoverTip(u) {
  const t = $('#hovertip');
  if (!mouseAt || u[260] < 0 || tool >= 0 || u[260] === u[80]) { t.hidden = true; return; }
  const kind = u[261], k = KINDS[kind]; if (!k) { t.hidden = true; return; }
  const temp = u[262], hp = u[263], cool = u[264], off = u[265] > 0, broken = u[266] > 0, fire = u[267] > 0, f = u[268];
  let h = `<b>${k.name}</b>`;
  if (isMiner(kind)) {
    const st = broken ? ['Dead', 'var(--alarm)'] : off ? ['Off', 'var(--muted)'] : fire ? ['ON FIRE', 'var(--alarm)'] : u[269] === 0 || f <= 0 ? ['No power', 'var(--alarm)'] : temp > 80 ? ['Throttled', 'var(--btc)'] : ['Mining', 'var(--hash)'];
    const tc = temp > 85 ? 'var(--alarm)' : temp > 70 ? 'var(--btc)' : 'var(--hash)';
    h += `<span style="color:${st[1]}">${st[0]}</span>`;
    if (!broken && !off) h += `<span>${ic('therm')} <b style="color:${tc}">${temp.toFixed(0)} °C</b>${cool > 0.5 ? ` · <span style="color:var(--hash)">${ic('snow')} −${cool.toFixed(0)} °C</span>` : ' · <span class="dim">no fan air</span>'}${u[276] > 0 ? ` · <span class="dim">${u[276]} packed</span>` : ''}</span>`;
    if (u[290] >= 10) h += `<span style="color:#c9a06a">${ic('dust')} dust ${u[290].toFixed(0)}% (+${(u[290] * 0.15).toFixed(0)} °C)</span>`;
    h += `<span>${ic('heart')} ${hp.toFixed(0)}% · ${hash(u[270])}${f > 0 && f < 0.999 ? ` · <span style="color:var(--btc)">${Math.round(f * 100)}% power</span>` : ''}</span>`;
    if (u[273] > 0) h += `<span class="dim">${ic('shield')} outside the fence</span>`;
  } else if (isPow(kind)) {
    h += `<span>Level ${u[274]}/3 · ${power(u[275])}</span><span class="dim">Tap to upgrade</span>`;
  } else if (kind === 14) {
    h += `<span>${off ? 'Off' : f > 0 ? 'Blowing ' + DIRN[u[277]].split(' ')[0] : 'No power'}</span><span class="dim">Tap to turn it</span>`;
  } else if (kind === 17) { h += `<span>${u[269] ? 'Live' : 'No source on this line'}</span>`; }
  else if (kind === 18) { h += `<span>Level ${u[274]}/3 · ${['', 'wood', 'stone', 'steel'][u[274]]}</span><span class="dim">Tap to upgrade walls</span>`; }
  else if (kind === 19) { h += `<span class="dim">Keeps raiders thinking twice</span>`; }
  t.innerHTML = h; t.hidden = false;
  t.style.left = Math.min(innerWidth - t.offsetWidth - 8, mouseAt[0] + 16) + 'px';
  t.style.top = Math.max(8, mouseAt[1] - t.offsetHeight - 12) + 'px';
}
let hushed = {};
function renderStatus(u) {
  const rows = [];
  if (u[245] > 0.5) rows.push(['alarm', `${ic('bolt')} Short <b>${power(u[245])}</b> · boxes on ${u[246] > 1 ? u[246] + ' networks' : 'that network'} slowed to ${Math.round(u[43] * 100)}%`, '<button class="btn mini" data-s="shed">Switch off worst</button><button class="btn btn-p mini" data-s="up">Upgrade power</button>', 'shortage']);
  if (u[240] > 0) rows.push(['alarm', `${ic('plug')} <b>${u[240]}</b> ${u[240] === 1 ? 'thing has' : 'things have'} no power`, '<button class="btn mini" data-s="grid">Show wiring</button>', 'unpowered']);
  if (u[253] > 0) rows.push(['', `${ic('power')} <b>${u[253]}</b> ${u[253] === 1 ? 'box' : 'boxes'} switched off`, '<button class="btn mini" data-s="wake">Turn all on</button>', 'off']);
  const fire = u[40]; if (fire > 0) rows.push(['warn', `${ic('flame')} <b>${fire}</b> hot ${fire === 1 ? 'box' : 'boxes'} over 80 °C`, '<button class="btn mini" data-s="air">Show airflow</button>', 'heat']);
  if (u[248] > 0 && !u[318] && (u[15] >= 8 || u[254] > 0)) rows.push(['warn', `${ic('shield')} <b>${u[248]}</b> ${u[248] === 1 ? 'box' : 'boxes'} outside a fence`, '<button class="btn mini" data-s="fence">Draw fence</button>', 'exposed']);
  if (u[287] > 0) rows.push([u[286] > 50 ? 'warn' : '', `${ic('dust')} <b>${u[287]}</b> dusty ${u[287] === 1 ? 'machine' : 'machines'} running hot`, `<button class="btn mini" data-s="clean">Clean all ${money(u[288])}</button>`, 'dust']);
  if (u[41] > 0) rows.push(['alarm', `${ic('x')} <b>${u[41]}</b> dead ${u[41] === 1 ? 'box' : 'boxes'}`, '', 'dead']);
  if (u[300] > 0) rows.push([u[301] === 1 ? '' : 'warn', u[301] === 1 ? `${ic('pause')} Curtailed for the grid · <b>${(u[300] / 6).toFixed(1)} h</b> left · ${money(u[316])} when it ends` : `${ic('bolt')} Paying spot price · <b>${(u[300] / 6).toFixed(1)} h</b> left · ${money(u[317])} extra so far`, '', u[301] === 1 ? 'curtail' : 'spot']);
  if (u[346] === 1) {
    const sp = `$${Math.round(u[347]).toLocaleString()}/MWh`;
    if (u[353] > 0) rows.push(['warn', `${ic('pause')} Grid feed off · spot ${sp} is over your $${Math.round(u[348])} limit`, '<button class="btn mini" data-s="contract">Contract</button>', 'spot']);
    else if (u[352] > 0) rows.push(['', `${ic('pause')} <b>${u[352]}</b> ${u[352] === 1 ? 'box sits' : 'boxes sit'} out · spot ${sp} costs more than ${u[352] === 1 ? 'it earns' : 'they earn'}`, '<button class="btn mini" data-s="contract">Contract</button>', 'econoff']);
    else if (u[349] > 0) rows.push(['alarm', `${ic('bolt')} Price spike ${sp} · <b>${(u[349] / 6).toFixed(1)} h</b> left`, '<button class="btn mini" data-s="contract">Set a limit</button>', 'spot']);
  }
  if (u[399] > 0.02 && u[11] > 0) rows.push(['warn', `${ic('warn')} <b>${(u[399] * 100).toFixed(1)}%</b> of your shares rejected: boxes too hot, worn or overclocked`, '<button class="btn mini" data-s="air">Show airflow</button>', 'rejects']);
  { const dead = u[488] + u[489] + u[490]; if (dead > 0 && u[510] > 0) rows.push([u[511] > u[11] * 0.05 ? 'warn' : '', `${ic('wrench')} <b>${u[510]}</b> ${u[510] === 1 ? 'box has' : 'boxes have'} failed parts · ${hash(u[511])} lost`, '<button class="btn mini" data-s="crew">Crew & spares</button>', 'parts']); }
  if (u[514] > 0) rows.push(['warn', `${ic('drop')} Dry season · the state halves miners' power until the rains`, '', 'dry']);
  if (u[512] === 5 && u[513] < 0.9) rows.push([u[513] < 0.6 ? 'warn' : '', `${ic('flame')} The well gives <b>${Math.round(u[513] * 100)}%</b> of what it did`, '', 'flare']);
  if (u[299] > 0) rows.push(['', `${ic('stack')} Fee spike ×${u[298].toFixed(1)} · blocks pay ${btcf(u[297] + u[23])}`, '<button class="btn mini" data-s="mempool">Mempool</button>', 'fees']);
  // an alert can be crossed off: it stays away until its cause clears, and comes back if the trouble returns
  const live = new Set(rows.map((r) => r[3] || r[1]));
  for (const k in hushed) if (!k.startsWith('ms') && !live.has(k)) delete hushed[k];
  const xBtn = (k) => `<button class="ib mini" data-dx="${k.replace(/"/g, '&quot;')}" aria-label="Dismiss">${ic('x')}</button>`;
  const h = rows.filter((r) => !hushed[r[3] || r[1]]).map(([c, t, b, w]) => `<div class="st ${c}"><span>${t}</span>${w ? whyBtn(w) : ''}${b}${xBtn(w || t)}</div>`).join('') + (stepsHtml(u) || milestoneHtml());
  if (h === statusHtml) return; statusHtml = h;
  const box = $('#status'); box.innerHTML = h; box.hidden = !h;
  box.querySelectorAll('[data-dx]').forEach((b) => b.onclick = () => { hushed[b.dataset.dx] = 1; sfx('tap'); statusHtml = null; renderStatus(UI()); });
  box.querySelectorAll('[data-s]').forEach((b) => b.onclick = () => {
    const a = b.dataset.s;
    if (a === 'shed') E.shed();
    if (a === 'up') { if (E.select_upgradable()) { E.sync_ui(); openInspector(); tip('upgrade'); } else toast('Nothing to upgrade', 'Every source is at level 3. Build another one from the Power tab.', 'var(--muted)'); }
    if (a === 'grid') { showView(2); tip('pole'); }
    if (a === 'wake') { E.wake_all(); sfx('power_on'); }
    if (a === 'clean') { E.clean_all(); sfx('clean'); }
    if (a === 'air') showView(1);
    if (a === 'mempool') openNetwork('mempool');
    if (a === 'contract') { openContract(); return; }
    if (a === 'crew') { openCrew(); return; }
    if (a === 'fence') { tab = 'site'; document.querySelectorAll('#tabs [data-tab]').forEach((x) => x.setAttribute('aria-selected', x.dataset.tab === 'site')); setTool(18); }
    if (a === 'steps-x') { steps.off = 1; saveSteps(); }
    sfx('tap'); statusHtml = ''; E.sync_ui(); renderStatus(UI());
  });
}
setInterval(() => { dockDirty = true; }, 1500);

$('#h-pow').onclick = () => { viewMask ^= 2; applyView(); };
$('#h-cash').onclick = () => why('cash');
$('#h-net').onclick = () => why('net');
$('#h-hash').onclick = () => why('hashrate');
$('#h-px').onclick = () => why('price');
$('#pill').onclick = () => openNetwork('mempool');
// ------------------------------------------------------------------ miner economics (what real miners look at)
const SPEC = { 10: [1040, 30.7], 11: [1410, 30.1], 12: [2000, 35], 13: [4730, 56.8] };
function powerPrice(u) { return u[4] > 1 && u[24] > 0 ? u[24] / (u[4] * 24 / 1000) : (u[305] || 35); } // blended $/MWh you pay now
function econ(kind, u) {
  const [th, kw] = SPEC[kind];
  const rev = th * u[22] * payFactor(u) * 0.998;     // $/day after pool fee, stale and base rejects
  const pp = powerPrice(u); const cost = kw * 24 / 1000 * pp;
  const profit = rev - cost;
  return { rev, cost, profit, pp, breakeven: rev / (kw * 24 / 1000), payback: profit > 0 ? u[400 + kind] / profit : Infinity, jth: kw * 1000 / th };
}
// ---- firmware profiles (same tables as data.rs)
const TUNE = { name: ['Low', 'Eco', 'Stock', 'Turbo', 'Max'], th: [0.70, 0.85, 1, 1.15, 1.28], kw: [0.55, 0.76, 1, 1.32, 1.62], heat: [-10, -5, 0, 6, 13] };
function tuneNet(kind, t, u) {
  const [th, kw] = SPEC[kind]; const i = t + 2; const fw = t ? 0.98 : 1; const rej = 0.002 + 0.012 * Math.max(t, 0);
  return th * TUNE.th[i] * fw * (1 - rej) * u[22] * payFactor(u) - kw * TUNE.kw[i] * 24 / 1000 * powerPrice(u);
}
function firmwareRows(kind, u) {
  const cur = u[342], i = cur + 2; const ts = [-2, -1, 0, 1, 2]; const nets = ts.map((t) => tuneNet(kind, t, u));
  const best = ts[nets.indexOf(Math.max(...nets))];
  const sgn = (v) => (v < 0 ? '−' : '+') + (Math.abs(v) < 100 ? '$' + Math.abs(v).toFixed(1) : money(Math.abs(v)));
  const desc = cur === 0 ? 'The maker\'s firmware. No fee.' : `${Math.round(TUNE.th[i] * 100)}% hashrate for ${Math.round(TUNE.kw[i] * 100)}% power, ${Math.abs(TUNE.heat[i])} °C ${cur > 0 ? 'hotter, wears faster' : 'cooler, wears slower'}. Custom firmware keeps 2% of hashrate as its fee.`;
  return `<div class="row"><span>Firmware · net per day now</span>${whyBtn('firmware')}</div>
    <div class="seg fwseg" role="radiogroup" aria-label="Firmware profile">${ts.map((t, x) => `<button role="radio" data-a="tune" data-t="${t}" aria-checked="${t === cur}" aria-selected="${t === cur}"><span>${TUNE.name[x]}</span><small class="num ${nets[x] < 0 ? 'neg' : ''}">${sgn(nets[x])}</small></button>`).join('')}</div>
    <div class="row" style="font-size:12px"><span>${desc}${best !== cur ? ` ${TUNE.name[best + 2]} nets the most right now.` : ''}</span></div>
    ${u[343] > 1 ? `<div class="row" style="padding-top:0"><span></span><button class="btn mini" data-a="tunesame">All ${u[343]} ${SHORT[kind]} on ${TUNE.name[i]}</button></div>` : ''}`;
}
const days = (d) => d === Infinity ? 'never' : d > 730 ? (d / 365).toFixed(1) + ' years' : Math.round(d) + ' days';

// ------------------------------------------------------------------ your bitcoin: sell, and pool vs solo
$('#h-btc').onclick = () => openTreasury();
// Pools (same numbers as data.rs). Fictional pools, real trade-offs.
const PAYPOOLS = [
  { t: 'Big pool nearby', fee: 0.02, ms: 25, pplns: false, d: 'A third of the network, servers in your region. Paid every block.' },
  { t: 'Budget pool overseas', fee: 0.01, ms: 240, pplns: false, d: 'Half the fee, servers on another continent. Paid every block.' },
  { t: 'Small PPLNS pool', fee: 0.005, ms: 60, pplns: true, d: '2% of the network. Pays your cut only when the pool finds a block, about 3 a day.' },
];
const payFactor = (u) => u[293] ? 1 : (1 - u[463]) * (1 - u[398]);
function openTreasury() {
  const u = UI(); const solo = u[293] > 0;
  const blk = u[294];
  const soloLine = blk > 0 ? (blk > 365 ? `about one block every <b>${(blk / 365).toFixed(blk > 3650 ? 0 : 1)} years</b>` : `about one block every <b>${blk.toFixed(blk < 10 ? 1 : 0)} days</b>`) : 'never, with no hashrate';
  openModal(`<div class="mh"><h2>Your bitcoin</h2><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <p>You hold <b style="color:var(--btc)">${btcf(u[1])}</b>, worth <b>${money(u[1] * u[2])}</b>. Selling pays your power bills. Holding gets you through a halving, and the core needs ₿ 21 in reserve.</p>
    <div class="choices" style="flex-direction:row;flex-wrap:wrap">${[25, 50, 100].map((p) => `<button class="btn choice" data-p="${p}" style="flex:1;min-width:110px">${p === 100 ? 'Sell all' : 'Sell ' + p + '%'}<span class="num" style="color:var(--cash)">+${money(u[1] * u[2] * p / 100 * 0.995)}</span></button>`).join('')}</div>
    <h3>How you get paid ${whyBtn('pplns')}</h3>
    <div class="diffs">
      ${PAYPOOLS.map((p, i) => { const on = !solo && u[397] === i; return `<button class="btn ${on ? 'on' : ''}" data-pool="${i}" aria-pressed="${on}"><span class="px" style="font-size:13px;color:${on ? 'var(--volt)' : 'var(--ink)'}">${p.t}</span><small>${p.d} ${p.pplns ? 'PPLNS' : 'FPPS'} · ${(p.fee * 100).toFixed(1)}% fee · ${p.ms} ms away, about ${((0.0005 + p.ms * 0.00008) * 100).toFixed(1)}% stale.</small></button>`; }).join('')}
      <button class="btn ${solo ? 'on' : ''}" data-m="1"><span class="px" style="font-size:13px;color:${solo ? 'var(--volt)' : 'var(--ink)'}">Solo</span><small>No fee, no payouts, until you find a whole block yourself: ${btcf(u[297] + u[23])}. With your hashrate that's ${soloLine}.</small></button>
    </div>
    ${u[11] > 0 ? `<div class="rows">
      <div class="row"><span>Shares accepted</span><b class="num">${((1 - u[398]) * (1 - u[399]) * 100).toFixed(2)}%</b></div>
      ${solo ? '' : `<div class="row"><span>Stale: ${u[464]} ms to the pool ${whyBtn('stale')}</span><b class="num">${(u[398] * 100).toFixed(2)}%</b></div>`}
      <div class="row"><span>Rejected: hot, worn or overclocked boxes ${whyBtn('rejects')}</span><b class="num ${u[399] > 0.02 ? 'neg' : ''}">${(u[399] * 100).toFixed(2)}%</b></div>
      ${!solo && u[469] > 0 && u[462] > 0 ? `<div class="row"><span>Pool luck since you joined · ${u[468]} blocks</span><b class="num ${u[460] < 1 ? 'neg' : ''}">${(u[460] * 100).toFixed(0)}% of expected</b></div>` : ''}
    </div>` : ''}
    <div class="mkt"><span>Your hashrate <b>${hash(u[11])}</b></span><span>Network <b>${(u[12] / 1e6).toFixed(0)} EH/s</b></span><span>Your share <b>${u[12] ? (u[11] / u[12] * 100).toExponential(2) : 0}%</b></span><span>Hashprice <b>${money(u[22] * 1000)}</b>/PH/day</span>${u[295] ? `<span>Blocks found <b>${u[295]}</b></span>` : ''}</div>
    <div class="choices" style="flex-direction:row;flex-wrap:wrap"><button class="btn" data-a="lab">${ic('pick')}How does a block get found? Try it</button><button class="btn" data-a="blk">${ic('cube')}Latest block</button><button class="btn" data-a="mp">${ic('stack')}Mempool</button></div>`, true);
  const m = $('#modal');
  m.querySelectorAll('[data-p]').forEach((b) => b.onclick = () => { E.sell_btc(+b.dataset.p); closeModal(); if (u[1] > 0) unlockGoal('sell'); });
  m.querySelectorAll('[data-m]').forEach((b) => b.onclick = () => { E.set_solo(+b.dataset.m); sfx('tap'); E.sync_ui(); openTreasury(); });
  m.querySelectorAll('[data-pool]').forEach((b) => b.onclick = () => { E.set_pool(+b.dataset.pool); sfx('tap'); E.sync_ui(); openTreasury(); });
  m.querySelector('[data-x]').onclick = closeModal;
  m.querySelector('[data-a=lab]').onclick = () => openLab();
  m.querySelector('[data-a=blk]').onclick = () => openNetwork('blocks');
  m.querySelector('[data-a=mp]').onclick = () => openNetwork('mempool');
}

// ------------------------------------------------------------------ the hash lab: real SHA-256d proof of work, by hand
const K256 = new Uint32Array([0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2]);
function sha256(msg) {
  const l = msg.length, nb = ((l + 9 + 63) >> 6) << 6, buf = new Uint8Array(nb);
  buf.set(msg); buf[l] = 0x80; const bits = l * 8; buf[nb - 4] = bits >>> 24; buf[nb - 3] = bits >>> 16; buf[nb - 2] = bits >>> 8; buf[nb - 1] = bits;
  let h0 = 0x6a09e667, h1 = 0xbb67ae85, h2 = 0x3c6ef372, h3 = 0xa54ff53a, h4 = 0x510e527f, h5 = 0x9b05688c, h6 = 0x1f83d9ab, h7 = 0x5be0cd19;
  const w = new Uint32Array(64);
  for (let o = 0; o < nb; o += 64) {
    for (let i = 0; i < 16; i++) w[i] = (buf[o + i * 4] << 24) | (buf[o + i * 4 + 1] << 16) | (buf[o + i * 4 + 2] << 8) | buf[o + i * 4 + 3];
    for (let i = 16; i < 64; i++) { const a = w[i - 15], b = w[i - 2]; w[i] = (((a >>> 7) | (a << 25)) ^ ((a >>> 18) | (a << 14)) ^ (a >>> 3)) + (((b >>> 17) | (b << 15)) ^ ((b >>> 19) | (b << 13)) ^ (b >>> 10)) + w[i - 7] + w[i - 16]; }
    let a = h0, b = h1, c = h2, d = h3, e = h4, f = h5, g = h6, h = h7;
    for (let i = 0; i < 64; i++) {
      const t1 = (h + (((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7))) + ((e & f) ^ (~e & g)) + K256[i] + w[i]) | 0;
      const t2 = ((((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10))) + ((a & b) ^ (a & c) ^ (b & c))) | 0;
      h = g; g = f; f = e; e = (d + t1) | 0; d = c; c = b; b = a; a = (t1 + t2) | 0;
    }
    h0 = (h0 + a) | 0; h1 = (h1 + b) | 0; h2 = (h2 + c) | 0; h3 = (h3 + d) | 0; h4 = (h4 + e) | 0; h5 = (h5 + f) | 0; h6 = (h6 + g) | 0; h7 = (h7 + h) | 0;
  }
  const out = new Uint8Array(32); [h0, h1, h2, h3, h4, h5, h6, h7].forEach((v, i) => { out[i * 4] = v >>> 24; out[i * 4 + 1] = v >>> 16; out[i * 4 + 2] = v >>> 8; out[i * 4 + 3] = v; });
  return out;
}
const hexRev = (b) => Array.from(b).reverse().map((x) => x.toString(16).padStart(2, '0')).join(''); // Bitcoin shows hashes byte-reversed
const zerosOf = (hex) => { let n = 0; while (n < hex.length && hex[n] === '0') n++; return n; };
function bitsFromDifficulty(d) {
  // target = 0xffff * 2^208 / d, in compact "nBits" form
  const lg = Math.log2(65535) + 208 - Math.log2(d); const bytes = Math.floor(lg / 8) + 1;
  let mant = Math.floor(Math.pow(2, lg - (bytes - 3) * 8)); let exp = bytes; if (mant >= 0x800000) { mant >>>= 8; exp++; }
  return ((exp << 24) | (mant & 0xffffff)) >>> 0;
}
let lab = null;
function openLab(opts = {}) {
  const u = UI(); const height = Math.round(u[6]) + (opts.found ? 0 : 1); const diff = market.diff || 1.48e14;
  const realZeros = Math.floor((256 - (Math.log2(65535) + 208 - Math.log2(diff))) / 4);
  const hdr = new Uint8Array(80); const dv = new DataView(hdr.buffer);
  dv.setUint32(0, 0x20000000, true);
  for (let i = 4; i < 36; i++) hdr[i] = i < 26 ? (Math.random() * 256) | 0 : 0;   // previous block hash (it had to beat the target too, so it ends in zeros)
  for (let i = 36; i < 68; i++) hdr[i] = (Math.random() * 256) | 0;               // merkle root of this block's transactions
  dv.setUint32(68, Math.floor(Date.now() / 1000), true);
  dv.setUint32(72, bitsFromDifficulty(diff), true);
  const field = (a, b) => hexRev(hdr.subarray(a, b));
  lab = { hdr, dv, nonce: (Math.random() * 1e6) | 0, tries: 0, want: opts.found ? 4 : 4, running: false, found: null, t0: 0, rate: 0, best: 0 };
  openModal(`<div class="mh"><h2>${opts.found ? `Block ${height.toLocaleString()} is yours` : 'Mine a block by hand'}</h2><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <p>${opts.found ? `Your machines hashed this header trillions of times a second until one result came in under the target. You won <b style="color:var(--btc)">${btcf(opts.amount)}</b>. Here's the same search, slowed down to browser speed:` : 'A miner takes the block header (80 bytes), runs it through SHA-256 twice, and checks whether the result, read as a number, is below the target. The only part it can change is the <b>nonce</b>, so it just keeps counting. A smaller target means more leading zeros, and more tries.'}</p>
    <div class="hdr">
      <div><span>Version</span><code>20000000</code></div>
      <div><span>Previous block</span><code>${field(4, 36)}</code></div>
      <div><span>Merkle root</span><code>${field(36, 68)}</code></div>
      <div><span>Time</span><code>${new Date(dv.getUint32(68, true) * 1000).toISOString().slice(0, 19).replace('T', ' ')}</code></div>
      <div><span>Target (bits)</span><code>${dv.getUint32(72, true).toString(16)} · today's real difficulty</code></div>
      <div><span>Nonce</span><code id="lab-n" class="hot">0</code></div>
    </div>
    <div class="hashbox"><span class="lbl">SHA-256(SHA-256(header))</span><code id="lab-h">press Mine</code></div>
    <div class="labrow"><span class="lbl">Practice target</span>${[3, 4, 5, 6].map((z) => `<button class="btn mini ${z === lab.want ? 'on' : ''}" data-z="${z}">${z} zeros</button>`).join('')}<span class="dim">Real network today: <b style="color:var(--btc)">${realZeros} zeros</b></span></div>
    <div class="mkt" id="lab-s"><span>Tries <b>0</b></span></div>
    <div class="choices" style="flex-direction:row"><button class="btn btn-p" data-a="go">${ic('pick')}Mine</button><button class="btn" data-a="stop">Stop</button></div>
    <div id="lab-r" class="labres" hidden></div>`, true);
  const m = $('#modal');
  const stop = () => { if (lab) lab.running = false; };
  m.querySelector('[data-x]').onclick = () => { stop(); closeModal(); };
  m.querySelector('[data-a=stop]').onclick = stop;
  m.querySelectorAll('[data-z]').forEach((b) => b.onclick = () => { lab.want = +b.dataset.z; m.querySelectorAll('[data-z]').forEach((x) => x.classList.toggle('on', x === b)); });
  m.querySelector('[data-a=go]').onclick = () => { if (lab.running) return; lab.running = true; lab.found = null; lab.t0 = performance.now(); lab.tries = 0; lab.best = 0; $('#lab-r').hidden = true; requestAnimationFrame(labStep); sfx('tap'); };
  if (opts.found) setTimeout(() => m.querySelector('[data-a=go]').click(), 600);
  function labStep() {
    if (!lab || !lab.running || $('#modal-wrap').hidden) { if (lab) lab.running = false; return; }
    const t = performance.now(); let last = null;
    while (performance.now() - t < 14) {
      for (let k = 0; k < 200; k++) {
        lab.nonce = (lab.nonce + 1) >>> 0; lab.dv.setUint32(76, lab.nonce, true); lab.tries++;
        const h = hexRev(sha256(sha256(lab.hdr))); const z = zerosOf(h);
        if (z > lab.best) lab.best = z;
        last = h;
        if (z >= lab.want) { lab.found = h; break; }
      }
      if (lab.found) break;
    }
    const secs = (performance.now() - lab.t0) / 1000; lab.rate = lab.tries / Math.max(secs, 0.001);
    const h = lab.found || last; const z = zerosOf(h);
    $('#lab-n').textContent = lab.nonce.toLocaleString();
    $('#lab-h').innerHTML = `<i>${h.slice(0, z)}</i>${h.slice(z)}`;
    $('#lab-s').innerHTML = `<span>Tries <b>${lab.tries.toLocaleString()}</b></span><span>Your browser <b>${Math.round(lab.rate).toLocaleString()} hashes/s</b></span><span>Best so far <b>${lab.best} zeros</b></span>`;
    if (lab.found) {
      lab.running = false; sfx('block'); setTimeout(() => unlockGoal('lab'), 1200);
      const realTries = diff * 2 ** 32; const yrs = realTries / lab.rate / 31557600;
      const box = 104e12; // one Antminer S19j Pro
      $('#lab-r').hidden = false;
      $('#lab-r').innerHTML = `<b style="color:var(--cash)">Found it.</b> Nonce ${lab.nonce.toLocaleString()} after ${lab.tries.toLocaleString()} tries. Each extra zero means about 16× more tries.<br>
        At today's real target your browser would need about <b>${yrs > 1e9 ? (yrs / 1e9).toFixed(0) + ' billion' : yrs > 1e6 ? (yrs / 1e6).toFixed(0) + ' million' : Math.round(yrs).toLocaleString()} years</b>. One Antminer S19j Pro (104 trillion hashes/s) would need about <b>${(realTries / box / 31557600).toFixed(0)} years</b> on its own.
        The whole network, at ${(market.netTh / 1e6).toFixed(0)} EH/s, finds one every ~10 minutes. That's why miners pool together.`;
      return;
    }
    requestAnimationFrame(labStep);
  }
}

// ------------------------------------------------------------------ modals
let closableModal = false, prevSpeed = 1;
function openModal(html, closable = false) {
  stopSkip(false);
  closableModal = closable; $('#modal').className = 'card'; $('#modal').innerHTML = html; $('#modal-wrap').hidden = false; $('#why').hidden = true;
  const f = $('#modal').querySelector('button'); if (f) f.focus();
}
function closeModal() { $('#modal-wrap').hidden = true; }
$('#modal-wrap').addEventListener('pointerdown', (e) => { if (e.target.id === 'modal-wrap' && closableModal) closeModal(); });

function showEvent(id, u) {
  sfx('event');
  const roll = u[19], arg = u[20];
  let body = '', choices = [];
  if (id === 0) { body = EVENTS[0].d; choices = [['Rent chillers', 'Keep full hashrate', '−' + money(arg), 'var(--cash)'], ['Throttle to 60%', "Free, but you'll mine less for 3 days", '−40% hash', 'var(--alarm)'], ['Gamble it', 'Roll a d20: 11+ and it passes, under 11 two boxes fry', '50/50', 'var(--btc)']]; }
  else if (id === 6) { body = 'A safety inspector is walking the site with a clipboard.'; choices = [['Pay the fine', 'Clean and simple', '−' + money(arg), 'var(--alarm)'], ['Argue it', `Roll 14+ and they leave. Under 14 costs ${money(arg * 2.5)}`, '35%', 'var(--btc)']]; }
  else if (id === 13) {
    const kw = u[307], rate = arg / Math.max(kw / 1000 * 6, 1e-9), spot = u[305] * u[256];
    const share = Math.min(1, kw / Math.max(u[4], 1)), mined = u[22] / 4 * u[11] * payFactor(u) * share - kw / 1000 * 6 * u[305];
    body = `The grid is short and wholesale prices are spiking. The utility offers <b>${money(rate)}/MWh</b> for every megawatt-hour you don't use over the next 6 hours. You're drawing <b>${power(kw)}</b> from the grid. Mining with that power for 6 hours would make about <b>${money(mined)}</b> after the power bill.`;
    choices = [['Switch off and get paid', 'Grid-fed boxes go dark for 6 hours', '+' + money(arg), 'var(--cash)'], ['Keep mining', `You pay the spot price, about ${money(spot)}/MWh, for 6 hours`, `×${Math.round(u[256])} power`, 'var(--alarm)']];
  }
  else if (id === 9) { body = 'Your hottest miner box caught fire and is offline.'; choices = [['Repair now', 'Back online right away', 'Repair', 'var(--cash)'], ['Leave it', 'Fix it later from the inspector', 'Free', 'var(--muted)']]; }
  else if (id === 20) {
    const m = arg; const pct = Math.round((m - 1) * 100);
    openModal(`<div class="evhead"><div class="die">${roll}</div><div class="evt"><h2>Halving</h2><span class="dim">Halving ${u[14]} · block ${u[6].toLocaleString()}</span></div></div>
      <p>The block reward is now <b style="color:var(--btc)">₿ ${u[9]}</b>. Your power bill didn't change. The market rolled a <b>${roll}</b>: bitcoin ${pct >= 0 ? 'jumps' : 'drops'} <b style="color:${pct >= 0 ? 'var(--cash)' : 'var(--alarm)'}">${pct >= 0 ? '+' : ''}${pct}%</b>.</p>
      <div class="choices"><button class="btn btn-p">Keep mining</button></div>`);
    speak(`Halving. The block reward is now ${u[9]} bitcoin.`);
    $('#modal button').onclick = () => { E.choose(0); closeModal(); tip('halving'); setTimeout(() => tipNudge('Another halving survived', 'If the game showed you why halvings matter, the tip jar takes any amount, even 21 sats.'), 2500); };
    return;
  } else if (id === 21) {
    openModal(`<div class="evhead"><div class="die" id="die">?</div><div class="evt"><h2>Ignite the core</h2><span class="dim">The end of the ladder</span></div></div>
      <p>Everything is in place. One roll decides it: <b>12 or higher</b> and the free energy core holds. Lower, and it collapses and you try again in 30 days.</p>
      <div class="choices"><button class="btn btn-p">Roll the d20</button></div>`);
    $('#modal button').onclick = () => { const d = $('#die'); d.classList.add('rolling'); setTimeout(() => { E.choose(0); const uu = UI(); d.classList.remove('rolling'); d.textContent = uu[19]; setTimeout(closeModal, 1400); }, 900); };
    return;
  }
  const ev = EVENTS[id];
  openModal(`<div class="evhead"><div class="die">${roll}</div><div class="evt"><h2>${ev.t}</h2><span class="dim">Day ${u[15]} · you rolled ${roll}</span></div></div>
    <p>${body} ${whyBtn({ 0: 'heat', 6: 'event', 9: 'heat', 13: 'curtail' }[id] || 'event')}</p><div class="choices">${choices.map((c, i) => `<button class="btn choice" data-c="${i}"><span>${c[0]}<small>${c[1]}</small></span><span class="num" style="color:${c[3]}">${c[2]}</span></button>`).join('')}</div>`);
  $('#modal').querySelectorAll('[data-c]').forEach((b) => b.onclick = () => {
    const c = +b.dataset.c; const gamble = (id === 0 && c === 2) || (id === 6 && c === 1);
    E.choose(c);
    if (id === 13) { tip('curtail'); if (c === 0) sfx('power_off'); }
    if (gamble) {
      const r = UI()[19]; const win = id === 0 ? r >= 11 : r >= 14;
      $('#modal').innerHTML = `<div class="evhead"><div class="die">${r}</div><div><h2 style="color:${win ? 'var(--cash)' : 'var(--alarm)'}">${win ? 'You got away with it' : 'Bad roll'}</h2><p>${win ? 'Nothing lost this time.' : id === 0 ? 'Two miner boxes fried.' : `That cost ${money(arg * 2.5)}.`}</p></div></div><button class="btn btn-p">OK</button>`;
      $('#modal button').onclick = closeModal; sfx(win ? 'coin' : 'bad');
    } else closeModal();
  });
}

// ---- the end of the ladder: a short sequence, then what free energy would really do to mining
function showFinale(u) {
  const f = $('#finale'); const lines = [`Day ${u[15]}. The core holds.`, 'Unlimited power, at $0 a megawatt-hour.', `Your plant mines at ${hash(u[11])}, and every box is pure profit.`, 'For now.'];
  f.innerHTML = `<div class="bloom"></div><div class="flines">${lines.map((t, i) => `<p style="animation-delay:${1.2 + i * 1.6}s">${t}</p>`).join('')}</div><button class="btn btn-p fgo" style="animation-delay:${1.2 + lines.length * 1.6}s">Continue</button>`;
  f.hidden = false; sfx('upgrade');
  const done = () => { f.hidden = true; closingCard(UI()); };
  f.querySelector('.fgo').onclick = done; if (calm) setTimeout(done, 400);
}
function closingCard(u) {
  openModal(`<div class="mh"><div><h2>Why free energy wouldn't make mining free</h2><span class="dim">Seed ${hex(u[35])} · ${SITES_UI[u[512]].t} · ${DIFFS[u[34]][0]} · day ${u[15]}</span></div></div>
    <p class="lead">You won because only you had the core. If everyone did, it would change very little.</p>
    <p>Bitcoin adjusts its difficulty every 2,016 blocks so that blocks keep coming about every ten minutes, however much hashing joins in. Free power would bring a flood of new machines. Difficulty would climb with them, until each machine earned barely more than it cost to buy, house and cool. The power bill would vanish and the profit would vanish with it, competed away by everyone else's free power.</p>
    <p>That's the rule this whole game runs on: mining profits attract hashrate, and hashrate competes the profit away. Miners end up spending roughly what the block reward is worth, whether on power, machines or something else.</p>
    <div class="rows"><div class="row"><span>Bitcoin</span><b class="num">${btcf(u[1])}</b></div><div class="row"><span>Cash</span><b class="num">${money(u[0])}</b></div><div class="row"><span>Sites</span><b class="num">${u[562]}</b></div><div class="row"><span>Halvings survived</span><b class="num">${u[14]}</b></div></div>
    <p class="note">The core was the game's one fantasy. Everything below it on the ladder was as real as we could make it, and the last three rungs say so when they aren't.</p>
    <div class="choices" style="flex-direction:row;flex-wrap:wrap"><button class="btn btn-p" data-a="new">New run</button><button class="btn" data-a="keep">Keep mining</button><button class="btn" data-a="ledger">Open the ledger</button><button class="btn" data-a="tip">${ic('heart')}Support the game</button></div>`);
  const m = $('#modal');
  m.querySelector('[data-a=tip]').onclick = () => openDonate(() => closingCard(u));
  m.querySelector('[data-a=new]').onclick = () => { overShown = false; newRunScreen(); };
  m.querySelector('[data-a=keep]').onclick = () => { E.keep_mining(); overShown = false; closeModal(); setSpeed(1); };
  m.querySelector('[data-a=ledger]').onclick = () => openLedger('core');
}
function showOver(kind) {
  setSpeed(0); clearSave(false); // a finished run is finished: no restoring it from the last autosave
  const u = UI();
  if (u[318] > 0 && showDebrief(u, kind === 3 ? 'done' : 'bust')) return;
  if (kind === 2) { showFinale(u); return; }
  if (kind === 2) {
    openModal(`<h2 style="color:var(--hash)">Free energy</h2><p>The core holds. Unlimited power at $0/MWh. Mining is pure profit now, and the run is won on day <b>${u[15]}</b> on <b>${DIFFS[u[34]][0]}</b>.</p>
      <p>Seed <b class="num">${hex(u[35])}</b> · ${btcf(u[1])} · ${money(u[0])}</p><div class="choices"><button class="btn btn-p">New run</button></div>`);
  } else {
    openModal(`<h2 style="color:var(--alarm)">Repossessed</h2><p>The bills went unpaid too long and the bank took the plant on day <b>${u[15]}</b>. Seed <b class="num">${hex(u[35])}</b>.</p><div class="choices"><button class="btn btn-p">New run</button></div>`);
  }
  $('#modal button').onclick = () => { overShown = false; newRunScreen(); };
}

// ------------------------------------------------------------------ ladder
function openLadder() {
  const u = UI();
  const rungs = LADDER.slice(0, 11); let curTier = 0; rungs.forEach((k, n) => { if (u[430 + k] > 0) curTier = n; });
  const tiers = rungs.map((t, n) => { const k = KINDS[t];
    const unlocked = u[430 + t] > 0, lvl = t === 21 ? u[567] : t === 22 ? u[569] : u[60 + t], cnt = t === 21 ? u[566] : t === 22 ? u[568] : u[70 + t];
    const state = t === 6 && !u[30] && !cnt ? 'skipped' : !unlocked ? 'locked' : lvl >= 3 ? 'maxed' : cnt ? 'built' : 'open';
    return `<div class="tier ${state === 'locked' || state === 'skipped' ? 'locked' : ''} ${n === curTier ? 'cur' : ''}"><span class="thumb">${thumb(k.spr, 64)}</span>
      <span class="lbl" style="color:${state === 'maxed' ? 'var(--hash)' : state === 'locked' || state === 'skipped' ? 'var(--muted)' : 'var(--volt)'}">${String(n + 1).padStart(2, '0')}${k.game ? ' · game' : ''} · ${{ locked: 'Locked', maxed: 'Maxed', built: 'Built', open: 'Can build', skipped: 'No river here: skipped' }[state]}</span>
      <b style="font-size:13px">${k.name}</b><span class="num" style="color:var(--volt)">${power(CAP[t][2])} max</span><span style="color:var(--muted)">${k.desc}</span>
      <span class="dots">${[1, 2, 3].map((q) => `<span class="${lvl >= q ? 'on' : ''}"></span>`).join('')}</span></div>`;
  }).join('');
  const req = [['Survive 2 halvings', u[140]], ['A fusion tokamak at level 3', u[141]], ['Hold ₿ 21 in reserve', u[142]], [`2016 blocks in a row with no power shortage (now ${u[13] >= 2016 ? 2016 : u[13]})`, u[143]]];
  openModal(`<div class="mh"><h2>Power ladder</h2><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <p>Build each source and upgrade it to level 3 to unlock the next. Cheaper power keeps old miners alive through halvings. Everything up to fission exists today; the rungs marked game don't, yet. ${whyBtn('fusion')}</p>
    <div class="ladder">${tiers}</div>
    <div class="card" style="padding:14px;display:flex;gap:14px;flex-wrap:wrap;box-shadow:inset 0 0 0 2px rgba(95,230,224,.5)">
      <span class="thumb" style="width:120px;height:110px;display:flex;align-items:flex-end;justify-content:center;background:#2B3A22;border-radius:10px">${thumb('B_CORE', 108)}</span>
      <div style="flex:1;min-width:220px;display:flex;flex-direction:column;gap:8px"><span class="lbl" style="color:var(--hash)">12 · game · The end of the ladder</span><b class="px" style="color:var(--hash);font-size:18px">Free energy core</b>
      <span style="color:var(--muted);font-size:13px">∞ MW at $0. Costs ${money(u[159])} and one lucky roll. The hardest finish in the game.</span>
      <div class="reqs">${req.map(([t, ok]) => `<div class="req ${ok ? 'ok' : ''}"><i>${ok ? ic('check') : ''}</i><span>${t}</span></div>`).join('')}</div></div></div>`, true);
  $('#modal [data-x]').onclick = closeModal;
}

// ------------------------------------------------------------------ learn / glossary
$('#b-ledger').onclick = () => openLedger();
$('#b-sites').onclick = () => openSites();
updateLedgerBadge();
// ------------------------------------------------------------------ support the game: a Lightning tip jar
// Value for value: the game is free, and paying is a lesson, not a gate. A first Lightning payment
// unlocks a ledger card. The game asks a real wallet for a real invoice, then checks
// the preimage the player pastes back with SHA-256, the same proof every Lightning node relies on.
const TIP_ADDR = 'inertmaid12@walletofsatoshi.com';
const TIP_LNURL = 'https://walletofsatoshi.com/.well-known/lnurlp/inertmaid12';
const TIP_AMOUNTS = [21, 210, 2100, 21000];
let tipInv = null; // { pr, sats, hash }
try { tipInv = JSON.parse(localStorage.getItem('pg-inv') || 'null'); } catch (_) { tipInv = null; }
const BECH = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l';
// The payment hash is tagged field 'p' (value 1) in a BOLT11 invoice: 52 five-bit words, 256 bits.
function invoiceHash(pr) {
  try {
    const s = pr.toLowerCase(), cut = s.lastIndexOf('1'), w = [...s.slice(cut + 1, -6)].map((c) => BECH.indexOf(c));
    for (let i = 7; i < w.length - 104;) {
      const type = w[i], len = w[i + 1] * 32 + w[i + 2];
      if (type === 1 && len === 52) {
        let acc = 0, bits = 0, hex = '';
        for (const v of w.slice(i + 3, i + 3 + 52)) { acc = (acc << 5) | v; bits += 5; while (bits >= 8) { bits -= 8; hex += ((acc >> bits) & 255).toString(16).padStart(2, '0'); } }
        return hex.slice(0, 64);
      }
      i += 3 + len;
    }
  } catch (_) {}
  return '';
}
async function sha256hex(hex) {
  const b = new Uint8Array(hex.match(/../g).map((h) => parseInt(h, 16)));
  const d = new Uint8Array(await crypto.subtle.digest('SHA-256', b));
  return [...d].map((x) => x.toString(16).padStart(2, '0')).join('');
}
async function fetchInvoice(sats, note) {
  const meta = await (await fetch(TIP_LNURL)).json();
  const url = new URL(meta.callback); url.searchParams.set('amount', String(sats * 1000));
  if (note && meta.commentAllowed) url.searchParams.set('comment', note.slice(0, meta.commentAllowed));
  const r = await (await fetch(url)).json();
  if (!r.pr) throw new Error(r.reason || 'No invoice');
  return { pr: r.pr, sats, hash: invoiceHash(r.pr) };
}
const fmtSats = (n) => n.toLocaleString('en-US') + ' sats';
const satsUsd = (n) => { if (!market || !market.price) return ''; const usd = n / 1e8 * market.price; return ` · about ${usd < 0.995 ? Math.max(1, Math.round(usd * 100)) + '¢' : money(usd)}`; };
function paidLightning() {
  try { localStorage.setItem('pg-tipped', String(Date.now())); } catch (_) {}
  tipInv = null; try { localStorage.removeItem('pg-inv'); } catch (_) {}
  unlockGoal('lightning');
  toast('Payment proven', 'The preimage matches the invoice: that\'s a Lightning receipt nobody can fake.', 'var(--btc)', '', 'heart');
}
function openDonate(back) {
  const inv = tipInv;
  const qr = inv ? PGQR.svg(inv.pr.toUpperCase(), 220) : `<img src="donate-qr.png" alt="Lightning QR code for ${TIP_ADDR}" width="220" height="220">`;
  openModal(`<div class="mh"><h2>Support Power Grid</h2><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <p>Power Grid is free to play, with no ads and no tracking. If it taught you something about mining, pay what you want over Lightning.${ledger.lightning ? '' : ' Your first payment adds the Lightning card to your ledger.'}</p>
    <div class="amts" role="group" aria-label="Amount">${TIP_AMOUNTS.map((a) => `<button class="btn ${inv && inv.sats === a ? 'on' : ''}" data-sats="${a}">${a.toLocaleString('en-US')}</button>`).join('')}<input class="codein" type="number" min="1" inputmode="numeric" placeholder="Other" aria-label="Other amount in sats" data-k="custom"><span class="dim">sats</span></div>
    <input class="codein note" maxlength="140" placeholder="Note to the developer (optional)" aria-label="Note" data-k="note">
    <div class="donate">
      <span class="qr">${qr}</span>
      <div class="addr">
        ${inv ? `<small>Invoice for ${fmtSats(inv.sats)}${satsUsd(inv.sats)}</small><b class="num inv">${inv.pr.slice(0, 24)}…</b>`
        : `<small>Lightning address · any amount</small><b class="num">${TIP_ADDR}</b>`}
        <div class="row"><a class="btn btn-p" href="lightning:${inv ? inv.pr : TIP_ADDR}">${ic('bolt')}Open in wallet</a><button class="btn" data-a="copy">${inv ? 'Copy invoice' : 'Copy address'}</button></div>
        <small>Scan with any Lightning wallet, such as Wallet of Satoshi, Phoenix, Strike or Cash App. ${inv ? 'The invoice is good for a few minutes.' : 'Or pick an amount above for an invoice.'}</small>
      </div>
    </div>
    ${inv && inv.hash ? `<div class="proof"><b>Paid? Prove it the Lightning way</b>
      <small>After paying, your wallet holds a <i>preimage</i>: the secret that unlocked this payment, usually under the payment details. Paste it and the game checks it against the invoice.</small>
      <div class="row"><input class="codein" data-k="pre" placeholder="Preimage (64 hex characters)" aria-label="Preimage" spellcheck="false" autocomplete="off"><button class="btn" data-a="check">Check</button></div>
      </div>`
    : ''}`, !back);
  const m = $('#modal');
  m.querySelector('[data-x]').onclick = back || closeModal;
  const make = async (sats) => {
    if (!(sats >= 1)) return;
    m.querySelectorAll('[data-sats]').forEach((b) => b.disabled = true);
    try { tipInv = await fetchInvoice(sats, m.querySelector('[data-k=note]').value.trim()); try { localStorage.setItem('pg-inv', JSON.stringify(tipInv)); } catch (_) {} openDonate(back); }
    catch (_) { m.querySelectorAll('[data-sats]').forEach((b) => b.disabled = false); toast('Couldn\'t reach the wallet', 'No invoice right now. The Lightning address still works: scan it and pick the amount in your wallet.', 'var(--alarm)'); }
  };
  m.querySelectorAll('[data-sats]').forEach((b) => b.onclick = () => make(+b.dataset.sats));
  m.querySelector('[data-k=custom]').onkeydown = (e) => { if (e.key === 'Enter') make(Math.floor(+e.target.value)); };
  m.querySelector('[data-k=custom]').onchange = (e) => make(Math.floor(+e.target.value));
  m.querySelector('[data-a=copy]').onclick = (e) => {
    const b = e.currentTarget, t = inv ? inv.pr : TIP_ADDR, label = b.textContent;
    const done = () => { b.textContent = 'Copied'; setTimeout(() => { b.textContent = label; }, 1500); };
    try { navigator.clipboard.writeText(t).then(done, () => toast('Copy this', t, 'var(--btc)')); } catch (_) { toast('Copy this', t, 'var(--btc)'); }
  };
  const chk = m.querySelector('[data-a=check]');
  if (chk) chk.onclick = async () => {
    const v = m.querySelector('[data-k=pre]').value.trim().toLowerCase();
    if (!/^[0-9a-f]{64}$/.test(v)) { toast('Not a preimage', 'A preimage is 64 characters of 0–9 and a–f.', 'var(--alarm)'); return; }
    let h = ''; try { h = await sha256hex(v); } catch (_) {}
    if (h && h === inv.hash) { paidLightning(); back ? back() : closeModal(); }
    else toast('That doesn\'t match', 'SHA-256 of that preimage isn\'t this invoice\'s payment hash. Check you copied the preimage, not the payment hash.', 'var(--alarm)');
  };
}
// One gentle mention of the tip jar per visit, and never within two weeks of a proven tip. It comes at a random
// moment between 6 and 20 minutes of actual play, or at a high point (a halving survived, a solo block) if that is sooner.
let nudgeDone = false; try { nudgeDone = !!sessionStorage.getItem('pg-nudged'); } catch (_) {}
function tipNudge(title, body) {
  if (nudgeDone) return;
  let last = 0; try { last = +(localStorage.getItem('pg-tipped') || 0); } catch (_) {}
  nudgeDone = true; try { sessionStorage.setItem('pg-nudged', '1'); } catch (_) {}
  if (Date.now() - last < 14 * 864e5) return;
  const t = el('div', { class: 'toast act', style: '--tc:var(--btc)' }, `<span class="ti">${ic('heart')}</span><div><b>${title}</b><span>${body}</span><button class="btn mini">Tip jar</button></div>`);
  t.querySelector('button').onclick = () => { t.remove(); openDonate(); };
  $('#toasts').prepend(t); setTimeout(() => t.remove(), 14000);
}
{ const at = (6 + Math.random() * 14) * 60; let played = 0;
  setInterval(() => {
    if (nudgeDone || !running || !speed || document.hidden || !$('#modal-wrap').hidden || !$('#memo').hidden) return;
    played += 5;
    if (played >= at) tipNudge('Enjoying Power Grid?', 'It\'s free, with no ads and no tracking. If you\'d like to support it, the tip jar takes any amount over Lightning.');
  }, 5000); }
$('#b-donate').onclick = () => openDonate();

$('#b-learn').onclick = () => {
  openModal(`<div class="mh"><h2>Foreman's notes</h2><button class="ib" data-x aria-label="Close">${ic('x')}</button></div><div class="gloss">
  <button class="btn btn-p" data-a="lab" style="align-self:flex-start">${ic('pick')}Try it: mine a block by hand</button>
  <div><b>Hashrate</b> is how many guesses per second your miners make at the next block. Your share of all hashrate is roughly your share of blocks.</div>
  <div><b>Difficulty</b> retargets every 2016 blocks to keep blocks about 10 minutes apart. When more miners join, it goes up and you earn less ₿ per box.</div>
  <div><b>J/TH</b> is joules per terahash: the power a miner burns for its work. Lower is better. When ₿ per day costs more power than it's worth, a box is a space heater.</div>
  <div><b>Halving</b> cuts the block reward in half every 210,000 blocks, about every four years. The next one is at block 1,050,000. Fees matter more after each one.</div>
  <div><b>Hashprice</b> is what one TH/s earns per day: <b class="num">${money(UI()[22] * 1000)}</b> per PH/s right now.</div>
  <div><b>Heat</b>: boxes heat each other when packed together. 70 °C glows orange, 85 °C red, 95 °C can catch fire. Every hour hot costs health. Fans blow a 4-tile lane; R turns them.</div>
  <div><b>Power networks</b>: electricity only flows between things that touch, or through power poles (4-tile reach). Each network has its own supply. Line losses grow with every pole.</div>
  <div><b>Upgrading</b>: tap a power building, then Upgrade. Three levels each. Level 3 unlocks the next source.</div>
  <div><b>Shortages</b>: if a network asks for more than it gets, everything on it slows down and brownouts wear PSUs. Switch off the worst boxes or add power.</div>
  <div><b>Health</b>: boxes wear out, faster when hot. Service them, or sell on the black market for 25–50% of list price depending on health.</div>
  <div><b>Security</b>: raiders walk in from the map edge. Anything they can reach is exposed. Close a wall loop around your plant. Wood walls get breached 40% of the time, stone 15%, steel 3%. Signs make raids rarer.</div>
  <div><b>Weather and dust</b>: rain cools the plant and rinses dust. Storms bring lightning. Dust storms coat everything. Each 10% dust adds 1.5 °C to a box and slows a fan. Clean for $120 each.</div>
  <div><b>Fees</b>: users pay to get their transactions into a block, and whoever mines the block keeps them on top of the subsidy. When the mempool is jammed, fees spike. Each halving makes fees matter more.</div>
  <div><b>Curtailment</b>: when the grid is short, it pays big loads to switch off. Accept a call and your grid-fed boxes go dark for 6 hours for a credit. Decline and you pay the spike price.</div>
  <div><b>Your power contract</b>: the grid price depends on difficulty: $35, $45 or $55 per MWh. Power is most of a miner's running cost, and it decides who survives a halving.</div>
  <div><b>Game prices</b>: machines and power plants cost less than in real life so a run fits in an evening. Hashrates, J/TH, difficulty, rewards, fees and the halving schedule are real.</div>
  <div><b>The late ladder</b>: after fission come deep geothermal, orbital solar and fusion. They're marked game: real research, built nowhere yet. The free energy core is the one pure fantasy.</div>
  <div><b>Where</b>: each run is set somewhere real: Texas, Iceland, Paraguay, Ethiopia, Bhutan or an oil well in North Dakota. Power, weather, rules and risks differ. Only Texas has spot and demand-response contracts.</div>
  <div><b>Parts and crew</b> (Site tab): boards, power supplies and fans fail one at a time. Hire technicians to swap them from your spares, or call a contractor per box. Guards deter raids.</div>
  <div><b>Pools</b> (tap ₿): a big pool nearby, a cheap one overseas with more stale shares, or a small PPLNS pool that pays when it finds blocks. Hot or overclocked boxes get shares rejected.</div>
  <div><b>Greenhouse</b> (Cooling tab): buys the heat of boxes within 2 tiles, up to 200 kW. Worth most when it's cold. Piped boxes run 6 °C cooler.</div>
  <div><b>Power contract</b> (grid hookup or the Power tab): fixed price, spot price with automatic switch-off, or fixed plus demand response. Contracts run at least 30 days.</div>
  <div><b>Firmware</b>: tap a miner to pick a profile, from Low (70% hashrate, 55% power) to Max (128%, 162%). Underclock when margins are thin; overclock when hashprice is high and you can keep the box cool.</div>
  <div><b>Machine market</b>: machine prices follow hashprice. Used boxes are cheaper but worn; brokers buy yours back at the used price, less 12%. Open it from the Miners tab.</div>
  <div><b>Live market</b>: in a live run the price is today's real bitcoin price, and the first retarget follows mempool.space's estimate. Later retargets are projections.</div></div>`, true);
  $('#modal [data-a=lab]').onclick = () => openLab();
  $('#modal [data-x]').onclick = closeModal;
};

// ------------------------------------------------------------------ live market (mempool.space)
const MP = 'https://mempool.space/api';
let market = { price: 73324, netTh: 9.5555e8, height: 975493, remaining: 215, est: -0.0739, fees: 0.03, diff: 1.4836e14, at: 0, src: 'built-in snapshot (13 Nov 2026)' };
try { const c = JSON.parse(localStorage.getItem('pg-market') || 'null'); if (c && c.price) market = c; } catch (_) {}
let marketState = market.at ? 'cached' : 'snapshot', liveRun = true;
const getJ = (path) => Promise.race([fetch(MP + path, { cache: 'no-store' }).then((r) => { if (!r.ok) throw r.status; return r.json(); }), new Promise((_, bad) => setTimeout(() => bad('timeout'), 6000))]);
async function fetchMarket() {
  marketState = 'loading';
  try {
    const [px, da, hr, tip_, rw] = await Promise.all([getJ('/v1/prices'), getJ('/v1/difficulty-adjustment'), getJ('/v1/mining/hashrate/3d'), getJ('/blocks/tip/height'), getJ('/v1/mining/reward-stats/144').catch(() => null)]);
    market = {
      price: +px.USD, netTh: +hr.currentHashrate / 1e12, diff: +hr.currentDifficulty, height: +tip_,
      remaining: +da.remainingBlocks, est: +da.difficultyChange / 100,
      fees: rw ? +rw.totalFee / 144 / 1e8 : market.fees, at: Date.now(), src: 'mempool.space',
    };
    try { localStorage.setItem('pg-market', JSON.stringify(market)); } catch (_) {}
    marketState = 'live';
  } catch (_) { marketState = market.at ? 'cached' : 'snapshot'; }
  pushMarket();
  return marketState;
}
function pushMarket() { E.set_market(market.price, market.netTh, market.height, market.remaining, market.est, market.fees, liveRun ? 1 : 0); }
pushMarket();
setInterval(async () => {
  if (!running || !UI()[241]) return;
  try { const px = await getJ('/v1/prices'); if (px.USD > 0) { E.set_price(+px.USD); market.price = +px.USD; } } catch (_) {}
}, 5 * 60 * 1000);
const ago = (t) => { const m = Math.round((Date.now() - t) / 60000); return m < 2 ? 'just now' : m < 120 ? m + ' min ago' : Math.round(m / 60) + ' h ago'; };

// ------------------------------------------------------------------ menu, saves, new run
const hex = (n) => (n >>> 0).toString(16).toUpperCase().padStart(8, '0').replace(/(.{4})/, '$1-');
// Seed challenges: the same seed, site and difficulty give the same map and the same opening, so a year's result is comparable.
const chCode = (seed, site, diff) => `${(seed >>> 0).toString(16).toUpperCase().padStart(8, '0')}-${site}-${diff}`;
let seedBest = {};
try { seedBest = JSON.parse(localStorage.getItem('pg-seeds') || '{}') || {}; } catch (_) { seedBest = {}; }
function useCode(c) {
  const m = /^\s*([0-9a-f]{4}-?[0-9a-f]{4})-([0-5])-([0-2])\s*$/i.exec(c || ''); if (!m) return false;
  pickSeed = parseInt(m[1].replace('-', ''), 16) >>> 0; pickSite = +m[2]; pickDiff = +m[3];
  try { localStorage.setItem('pg-site', pickSite); } catch (_) {}
  return true;
}
function copyText(t, btn) {
  const ok = () => { if (btn) { btn.textContent = 'Copied'; setTimeout(() => { btn.textContent = 'Copy'; }, 1500); } };
  try { navigator.clipboard.writeText(t).then(ok, () => toast('Your code', t, 'var(--volt)')); } catch (_) { toast('Your code', t, 'var(--volt)'); }
}
let runCode = '';
try { runCode = localStorage.getItem('pg-code') || ''; } catch (_) {}
// a shared link: ...#c=CA20A69A-0-0 opens the new-run screen on that challenge
try { const h = /#c=([^&]+)/.exec(location.hash); if (h) useCode(decodeURIComponent(h[1])); } catch (_) {}
function yearCheck(u) {
  if (u[318] || u[15] < 365 || !runCode) return;
  const k = 'year-' + runCode; if (steps[k]) return; steps[k] = 1; saveSteps();
  const worth = u[0] + u[1] * u[2] + u[565]; const best = seedBest[runCode] || 0;
  if (worth > best) { seedBest[runCode] = Math.round(worth); try { localStorage.setItem('pg-seeds', JSON.stringify(seedBest)); } catch (_) {} }
  openModal(`<div class="mh"><div><h2>One year in</h2><span class="dim">Challenge ${runCode}</span></div></div>
    <div class="rows"><div class="row tot"><span>Cash, bitcoin and machines</span><b class="num">${money(worth)}</b></div>
    <div class="row"><span>Your best on this code before</span><b class="num">${best ? money(best) : 'first try'}</b></div></div>
    <p>Anyone who starts a run with this code gets the same map, site and difficulty. Send it to a friend and compare a year.</p>
    <div class="choices" style="flex-direction:row"><button class="btn btn-p" data-a="copy">Copy code ${runCode}</button><button class="btn" data-a="go">Keep playing</button></div>`, true);
  const m = $('#modal'); m.querySelector('[data-a=copy]').onclick = (e) => copyText(runCode, e.target); m.querySelector('[data-a=go]').onclick = closeModal;
}
function saveGame() {
  if (!running || UI()[21] > 0) return;
  try {
    const n = E.save(); const b = new Uint8Array(mem(), E.save_ptr(), n);
    let s = ''; for (let i = 0; i < n; i++) s += String.fromCharCode(b[i]);
    localStorage.setItem('pg-save', btoa(s));
  } catch (_) {}
  saveMarket();
}
function loadGame() {
  try {
    const s = localStorage.getItem('pg-save'); if (!s) return false;
    const bin = atob(s); const p = E.load_buf(bin.length); const b = new Uint8Array(mem(), p, bin.length);
    for (let i = 0; i < bin.length; i++) b[i] = bin.charCodeAt(i);
    return E.load() === 1;
  } catch (_) { return false; }
}
// Starting over keeps the last save one step back, so an accidental new run is recoverable from the menu.
function clearSave(keepPrev = true) { try { const s = localStorage.getItem('pg-save'); if (s && keepPrev) localStorage.setItem('pg-save-prev', s); if (!keepPrev) localStorage.removeItem('pg-save-prev'); localStorage.removeItem('pg-save'); } catch (_) {} }
function downloadSave(key, name) { try { const s = localStorage.getItem(key); if (!s) return false; const bin = atob(s); const b = new Uint8Array(bin.length); for (let i = 0; i < bin.length; i++) b[i] = bin.charCodeAt(i); el('a', { href: URL.createObjectURL(new Blob([b])), download: name }).click(); return true; } catch (_) { return false; } }
function restorePrev() { try { const s = localStorage.getItem('pg-save-prev'); if (!s) return false; localStorage.setItem('pg-save', s); const ok = loadGame(); if (ok) restoreScenario(); return ok; } catch (_) { return false; } }
setInterval(saveGame, 20000);
document.addEventListener('visibilitychange', () => { if (document.hidden) { saveGame(); FM.suspend(); stopSpeaking(); if (speed) { prevSpeed = speed; setSpeed(0); } } else FM.resume(); });

$('#b-menu').onclick = () => {
  const u = UI();
  openModal(`<div class="mh"><h2>Paused</h2><button class="ib" data-x aria-label="Close">${ic('x')}</button></div>
    <p>Seed <b class="num">${hex(u[35])}</b> · ${DIFFS[u[34]][0]} · day ${u[15]} · halvings ${u[14]}</p>
    <div class="choices"><button class="btn btn-p" data-a="resume">Resume</button><button class="btn" data-a="sound">Music &amp; voice</button><button class="btn" data-a="save">Save now</button><button class="btn" data-a="export">Export save file</button>
    <label class="btn" style="cursor:pointer">Import save file<input type="file" accept=".pgsave" hidden></label><button class="btn" data-a="home">Back to the garage</button><button class="btn btn-d" data-a="new">Abandon run</button></div>`, true);
  const m = $('#modal'); const was = speed; setSpeed(0);
  m.querySelector('[data-x]').onclick = m.querySelector('[data-a=resume]').onclick = () => { closeModal(); setSpeed(was || 1); };
  m.querySelector('[data-a=sound]').onclick = () => openSettings();
  m.querySelector('[data-a=save]').onclick = () => { saveGame(); toast('Saved', 'Saved in this browser.', 'var(--cash)'); };
  m.querySelector('[data-a=home]').onclick = () => { E.center_home(); closeModal(); setSpeed(was || 1); };
  m.querySelector('[data-a=export]').onclick = () => { const n = E.save(); const b = new Uint8Array(mem(), E.save_ptr(), n).slice(); const a = el('a', { href: URL.createObjectURL(new Blob([b])), download: `power-grid-${hex(UI()[35])}-day${UI()[15]}.pgsave` }); a.click(); };
  m.querySelector('input[type=file]').onchange = async (e) => { const f = e.target.files[0]; if (!f) return; const b = new Uint8Array(await f.arrayBuffer()); const p = E.load_buf(b.length); new Uint8Array(mem(), p, b.length).set(b); if (E.load() === 1) { restoreScenario(); closeModal(); afterStart(); toast('Loaded', 'Save file imported.', 'var(--cash)'); } else toast('Bad file', "That isn't a Power Grid save.", 'var(--alarm)'); };
  const ab = m.querySelector('[data-a=new]'); ab.onclick = () => { if (ab.dataset.sure) { clearSave(); newRunScreen(); } else { ab.dataset.sure = 1; ab.textContent = 'Tap again to abandon this run'; } };
};

let pickDiff = 0, pickSeed = (Math.random() * 2 ** 32) >>> 0, pickSite = 0;
try { pickSite = Math.min(5, Math.max(0, +(localStorage.getItem('pg-site') || 0))); } catch (_) {}
function newRunScreen() {
  running = running && false; ['#hud', '#pill', '#speed', '#dock', '#np', '#status'].forEach((s) => $(s).hidden = true); statusHtml = ''; $('#memo').hidden = true; memoResume = false; stopSpeaking(); closeInspector(); setTool(-1);
  const has = !!localStorage.getItem('pg-save');
  E.set_site(pickSite); E.preview_seed(pickSeed); redraw = true; const u = UI();
  const tr = [['River', u[30] ? 'Yes: a dam is possible' : 'None: the ladder skips hydro', u[30] ? 'var(--hash)' : 'var(--alarm)'],
    ['Sunshine', u[31] > 0.95 ? 'Strong' : u[31] > 0.7 ? 'Average' : 'Weak: solar struggles', u[31] > 0.7 ? 'var(--hash)' : 'var(--alarm)'],
    ['Wind', u[32] > 1.0 ? 'Strong' : u[32] > 0.7 ? 'Average' : 'Weak', u[32] > 0.7 ? 'var(--hash)' : 'var(--alarm)'],
    ['Coal seam', u[33] ? 'Yes: cheap coal' : 'None: coal shipped in (+$15/MWh)', u[33] ? 'var(--hash)' : 'var(--alarm)']];
  openModal(`<div style="display:flex;flex-direction:column;gap:4px"><h2 class="px" style="color:var(--volt);font-size:44px;line-height:44px">Power Grid</h2>
    <p>Mine bitcoin. Climb the power ladder. Reach free energy, if you can.</p></div>
    <div style="display:flex;align-items:center;gap:10px;flex-wrap:wrap"><span class="lbl">Map seed</span><span class="tag num" style="font-size:15px">${hex(pickSeed)}</span><button class="btn" data-a="reroll">Reroll</button><span class="dim" style="font-size:12px">Challenge code <b class="num" style="color:var(--ink)">${chCode(pickSeed, pickSite, pickDiff)}</b>${seedBest[chCode(pickSeed, pickSite, pickDiff)] ? ` · your best after a year ${money(seedBest[chCode(pickSeed, pickSite, pickDiff)])}` : ''}</span><button class="btn mini" data-a="copycode">Copy</button><input class="codein" placeholder="Paste a code" aria-label="Challenge code" maxlength="14"><button class="btn mini" data-a="usecode">Use</button></div>
    <div class="traits">${tr.map(([n, t, c]) => `<div class="trait"><b style="color:${c}">${n}</b>${t}</div>`).join('')}</div>
    <div><span class="lbl">Where</span><div class="diffs sites" style="margin-top:6px">${SITES_UI.map((st, i) => `<button class="btn ${i === pickSite ? 'on' : ''}" data-site="${i}" aria-pressed="${i === pickSite}"><span class="px" style="font-size:13px;color:${i === pickSite ? 'var(--volt)' : 'var(--ink)'}">${st.t}</span><small>${st.power} · ${st.climate}</small><small class="risk">${st.risks}</small></button>`).join('')}</div></div>
    <div><span class="lbl">Market</span><div class="diffs" style="margin-top:6px">
      <button class="btn ${liveRun ? 'on' : ''}" data-m="1"><span class="px" style="font-size:13px;color:${liveRun ? 'var(--volt)' : 'var(--ink)'}">Live</span><small>Today's real price, network hashrate, block height and fees. The price keeps following the real one.</small></button>
      <button class="btn ${liveRun ? '' : 'on'}" data-m="0"><span class="px" style="font-size:13px;color:${liveRun ? 'var(--ink)' : 'var(--volt)'}">Simulated</span><small>Starts from the same numbers, then the price wanders on its own with crashes and pumps.</small></button></div>
      <div class="mkt">${marketState === 'loading' ? 'Fetching today\'s numbers…' : `<span><b>${money(market.price)}</b> BTC</span><span><b>${(market.diff / 1e12).toFixed(2)} T</b> difficulty</span><span><b>${(market.netTh / 1e6).toFixed(0)} EH/s</b> network</span><span>block <b>${market.height.toLocaleString()}</b></span><span>retarget in <b>${market.remaining}</b> (est ${market.est > 0 ? '+' : ''}${(market.est * 100).toFixed(1)}%)</span><span class="dim">${marketState === 'live' ? '● live from mempool.space' : marketState === 'cached' ? 'saved from mempool.space ' + ago(market.at) + ' (offline)' : market.src + ' (offline)'}</span>`}</div></div>
    <div><span class="lbl">Difficulty</span><div class="diffs" style="margin-top:6px">${DIFFS.map(([n, d], i) => `<button class="btn ${i === pickDiff ? 'on' : ''}" data-d="${i}"><span class="px" style="font-size:13px;color:${i === pickDiff ? 'var(--volt)' : 'var(--ink)'}">${n}</span><small>${d}</small></button>`).join('')}</div></div>
    <div class="choices" style="flex-direction:row;flex-wrap:wrap"><button class="btn btn-p" data-a="start" style="font-size:16px">${ic('play')}Start run</button>${has ? '<button class="btn" data-a="cont">Continue saved run</button>' : ''}${!has && localStorage.getItem('pg-save-prev') ? '<button class="btn" data-a="prev">Restore previous run</button>' : ''}</div>
    ${historyList()}
    <span style="font-size:12px;color:var(--muted)">Plays offline once loaded. Saves stay in this browser. Sprites: OpenGFX (GPL-2.0), ground textures by rubberduck (CC0), and original art. · <a href="#" data-a="donate" style="color:var(--btc)">Support the game</a> · ${BUILD}</span>`);
  const m = $('#modal');
  m.querySelector('[data-a=donate]').onclick = (e) => { e.preventDefault(); openDonate(newRunScreen); };
  m.querySelector('[data-a=reroll]').onclick = () => { pickSeed = (Math.random() * 2 ** 32) >>> 0; newRunScreen(); };
  m.querySelector('[data-a=copycode]').onclick = (e) => copyText(chCode(pickSeed, pickSite, pickDiff), e.target);
  m.querySelector('[data-a=usecode]').onclick = () => { if (useCode(m.querySelector('.codein').value)) { sfx('tap'); newRunScreen(); } else toast('Not a code', 'A challenge code looks like CA20A69A-0-0: seed, site, difficulty.', 'var(--alarm)'); };
  m.querySelectorAll('[data-h]').forEach((b) => b.onclick = () => { sfx('tap'); scenBriefing(SCEN.find((x) => x.key === b.dataset.h)); });
  m.querySelectorAll('[data-d]').forEach((b) => b.onclick = () => { pickDiff = +b.dataset.d; newRunScreen(); });
  m.querySelectorAll('[data-site]').forEach((b) => b.onclick = () => { pickSite = +b.dataset.site; try { localStorage.setItem('pg-site', pickSite); } catch (_) {} sfx('tap'); newRunScreen(); });
  m.querySelectorAll('[data-m]').forEach((b) => b.onclick = () => { liveRun = b.dataset.m === '1'; pushMarket(); newRunScreen(); });
  m.querySelector('[data-a=start]').onclick = () => { pushMarket(); clearSave(); E.set_site(pickSite); E.new_game(pickSeed, pickDiff); runCode = chCode(pickSeed, pickSite, pickDiff); try { localStorage.setItem('pg-code', runCode); } catch (_) {} seenTips = {}; try { localStorage.removeItem('pg-tips'); } catch (_) {} closeModal(); afterStart(); startAudio(); askTutorial(); };
  const c = m.querySelector('[data-a=cont]'); if (c) c.onclick = () => {
    if (loadGame()) { restoreScenario(); closeModal(); afterStart(); startAudio(); return; }
    // Never throw a run away: park it, explain, and offer the file.
    try { localStorage.setItem('pg-save-bak-' + Date.now(), localStorage.getItem('pg-save')); localStorage.removeItem('pg-save'); } catch (_) {}
    openModal(`<h2 style="color:var(--alarm)">This save won't load</h2><p>It was made by a build this one can't read. It hasn't been deleted: it's parked in this browser, and you can download it to keep.</p>
      <div class="choices"><button class="btn" data-a="dl">Download the save file</button><button class="btn btn-p" data-a="ok">Back</button></div>`);
    const mm = $('#modal'); const key = Object.keys(localStorage).filter((k) => k.startsWith('pg-save-bak-')).sort().pop();
    mm.querySelector('[data-a=dl]').onclick = () => downloadSave(key, 'power-grid-unreadable.pgsave');
    mm.querySelector('[data-a=ok]').onclick = () => newRunScreen();
  };
  const pv = m.querySelector('[data-a=prev]'); if (pv) pv.onclick = () => { if (restorePrev()) { closeModal(); afterStart(); startAudio(); toast('Restored', 'Your previous run is back.', 'var(--cash)'); } else toast('No luck', "That run couldn't be restored.", 'var(--alarm)'); };
}
// Every new run asks once: a guided start, or straight in.
function askTutorial() {
  setSpeed(0);
  openModal(`<h2>How do you want to start?</h2>
    <div class="tutpick">
      <button class="btn btn-p" data-a="tut"><b>Show me around</b><small>The foreman walks you through your first few minutes and explains things as they come up. The game waits while he talks.</small></button>
      <button class="btn" data-a="skip"><b>Skip the tutorial</b><small>No notes, no checklist. You can turn the tutorial back on in Settings.</small></button>
    </div>`);
  const m = $('#modal');
  m.querySelector('[data-a=tut]').onclick = () => { settings.tutorial = true; saveSettings(); steps = {}; saveSteps(); closeModal(); memoResume = true; setTimeout(() => tip('start'), 250); };
  m.querySelector('[data-a=skip]').onclick = () => { settings.tutorial = false; saveSettings(); closeModal(); setSpeed(1); };
}
function afterStart() {
  try { seenTips = JSON.parse(localStorage.getItem('pg-tips') || '{}'); } catch (_) { seenTips = {}; }
  ['#hud', '#pill', '#speed', '#dock'].forEach((s) => $(s).hidden = false);
  hushed = {};
  overShown = false; shownEvent = -1; fit(); setSpeed(1); viewMask = 0; viewAuto = 0; applyView(); renderDock(); running = true;
}

// ------------------------------------------------------------------ go
step(100, 'Ready');
E.new_game(pickSeed, 0); fit(); E.frame(0); imgData.data.set(new Uint8ClampedArray(mem(), E.fb_ptr(), fbW * fbH * 4)); ctx.putImageData(imgData, 0, 0);
$('#boot').remove();
requestAnimationFrame(loop);
newRunScreen();
fetchMarket().then(() => { if (!running && !$('#modal-wrap').hidden && $('#modal [data-m]')) newRunScreen(); });
})();
