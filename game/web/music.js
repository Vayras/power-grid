// Foreman FM: an endless, procedurally generated lo-fi radio built on Web Audio.
// Every song gets a random key, tempo, jazzy progression, groove and title. No samples, no network.
'use strict';
window.ForemanFM = (function () {
  const W1 = ['Coolant', 'Hashrate', 'Night Shift', 'Halving', 'Retarget', 'Mempool', 'Block 840k', 'Turbine', 'Dusk', 'Static', 'Rust', 'Ember', 'Grid', 'Fan Hum', "Foreman's", 'Immersion', 'Low Fee', 'Cold Wallet', 'Brownout', 'Sats'];
  const W2 = ['Lullaby', 'Rain', 'Drift', 'Blues', 'Tape', 'Season', 'Walk', 'Glow', 'Hours', 'Dreams', 'Haze', 'Loop', 'Shift', 'Breeze', 'Nap', 'Hum'];
  // chords as [semitone offset from key root, quality]
  const Q = { maj7: [0, 4, 7, 11], m7: [0, 3, 7, 10], dom7: [0, 4, 7, 10], m9: [0, 3, 7, 10, 14], maj9: [0, 4, 7, 11, 14], m11: [0, 3, 7, 10, 17] };
  const PROGS = [
    [[2, 'm9'], [7, 'dom7'], [0, 'maj9'], [9, 'm7']],
    [[0, 'maj7'], [9, 'm9'], [2, 'm7'], [7, 'dom7']],
    [[5, 'maj9'], [4, 'm7'], [2, 'm9'], [0, 'maj7']],
    [[0, 'm9'], [5, 'm9'], [10, 'dom7'], [3, 'maj7']],
    [[0, 'maj7'], [5, 'maj9'], [4, 'm7'], [9, 'm11']],
    [[9, 'm9'], [5, 'maj7'], [0, 'maj9'], [7, 'dom7']],
    [[0, 'm11'], [8, 'maj7'], [3, 'maj9'], [10, 'dom7']],
  ];
  const KICKS = [[0, 2.5], [0, 1.75, 2.5], [0, 0.75, 2.5, 3.25], [0, 2, 2.75]];

  let ctx = null, master, musicBus, duckGain, filter, comp, verb, verbSend, lfo, lfoGain, crackle = null, crackleGain;
  let on = false, vol = 0.55, timer = null, song = null, nextTime = 0, step = 0, onSong = () => {};
  const rnd = (a, b) => a + Math.random() * (b - a);
  const pick = (a) => a[Math.floor(Math.random() * a.length)];
  const mtof = (m) => 440 * Math.pow(2, (m - 69) / 12);

  function build(useCtx) {
    ctx = useCtx || new (window.AudioContext || window.webkitAudioContext)();
    master = ctx.createGain(); master.gain.value = vol;
    duckGain = ctx.createGain(); duckGain.gain.value = 1;
    comp = ctx.createDynamicsCompressor(); comp.threshold.value = -18; comp.ratio.value = 3; comp.attack.value = 0.02; comp.release.value = 0.3;
    filter = ctx.createBiquadFilter(); filter.type = 'lowpass'; filter.frequency.value = 5200; filter.Q.value = 0.5;
    const sat = ctx.createWaveShaper(); const curve = new Float32Array(1024);
    for (let i = 0; i < 1024; i++) { const x = i / 512 - 1; curve[i] = Math.tanh(1.6 * x) / Math.tanh(1.6); }
    sat.curve = curve;
    musicBus = ctx.createGain(); musicBus.gain.value = 0.8;
    musicBus.connect(filter); filter.connect(sat); sat.connect(comp); comp.connect(duckGain); duckGain.connect(master); master.connect(ctx.destination);
    // reverb from a generated impulse
    verb = ctx.createConvolver(); const len = ctx.sampleRate * 2.4; const ir = ctx.createBuffer(2, len, ctx.sampleRate);
    for (let c = 0; c < 2; c++) { const d = ir.getChannelData(c); for (let i = 0; i < len; i++) d[i] = (Math.random() * 2 - 1) * Math.pow(1 - i / len, 3.2); }
    verb.buffer = ir; verbSend = ctx.createGain(); verbSend.gain.value = 0.32; verbSend.connect(verb); verb.connect(filter);
    // tape wow: one slow LFO bends every oscillator a few cents
    lfo = ctx.createOscillator(); lfo.frequency.value = 0.45; lfoGain = ctx.createGain(); lfoGain.gain.value = 7; lfo.connect(lfoGain); lfo.start();
    // vinyl crackle loop
    const cl = ctx.sampleRate * 4; const cb = ctx.createBuffer(1, cl, ctx.sampleRate); const cd = cb.getChannelData(0);
    let lp = 0;
    for (let i = 0; i < cl; i++) { lp = lp * 0.96 + (Math.random() * 2 - 1) * 0.04; cd[i] = lp * 0.5 + (Math.random() < 0.0009 ? (Math.random() * 2 - 1) * 0.9 : 0); }
    crackle = ctx.createBufferSource(); crackle.buffer = cb; crackle.loop = true;
    crackleGain = ctx.createGain(); crackleGain.gain.value = 0.05;
    const hp = ctx.createBiquadFilter(); hp.type = 'highpass'; hp.frequency.value = 900;
    crackle.connect(hp); hp.connect(crackleGain); crackleGain.connect(comp); crackle.start();
  }

  function newSong() {
    const minorish = Math.random() < 0.45;
    const prog = pick(PROGS);
    song = {
      title: pick(W1) + ' ' + pick(W2),
      bpm: Math.round(rnd(68, 86)), key: 48 + Math.floor(Math.random() * 8), prog,
      swing: rnd(0.12, 0.24), kick: pick(KICKS), bars: pick([40, 48, 56]),
      pent: minorish ? [0, 3, 5, 7, 10] : [0, 2, 4, 7, 9], melodyP: rnd(0.12, 0.28), hatP: rnd(0.75, 1), cutoff: rnd(3200, 5600),
      epTone: rnd(0.6, 1.4),
    };
    step = 0;
    filter.frequency.setTargetAtTime(song.cutoff, ctx.currentTime, 2);
    onSong(song.title, song.bpm);
  }

  // ---------------------------------------------------------------- instruments
  function env(g, t, a, peak, d, sustain, r, end) {
    g.gain.setValueAtTime(0.0001, t); g.gain.exponentialRampToValueAtTime(peak, t + a);
    g.gain.exponentialRampToValueAtTime(Math.max(0.0001, peak * sustain), t + a + d);
    g.gain.setValueAtTime(Math.max(0.0001, peak * sustain), end); g.gain.exponentialRampToValueAtTime(0.0001, end + r);
  }
  function ep(midi, t, dur, vel) { // FM electric piano
    const car = ctx.createOscillator(), mod = ctx.createOscillator(), mg = ctx.createGain(), g = ctx.createGain(), lp = ctx.createBiquadFilter();
    const f = mtof(midi);
    car.type = 'sine'; car.frequency.value = f; mod.type = 'sine'; mod.frequency.value = f * 1.0;
    mg.gain.setValueAtTime(f * 1.4 * song.epTone * vel, t); mg.gain.exponentialRampToValueAtTime(f * 0.15, t + 1.2);
    mod.connect(mg); mg.connect(car.frequency);
    lfoGain.connect(car.detune);
    lp.type = 'lowpass'; lp.frequency.value = 2200;
    car.connect(lp); lp.connect(g); g.connect(musicBus); g.connect(verbSend);
    env(g, t, 0.008, 0.11 * vel, 0.9, 0.35, 0.5, t + dur);
    car.start(t); mod.start(t); car.stop(t + dur + 0.6); mod.stop(t + dur + 0.6);
  }
  function bass(midi, t, dur, vel) {
    const o = ctx.createOscillator(), o2 = ctx.createOscillator(), g = ctx.createGain(), lp = ctx.createBiquadFilter();
    o.type = 'triangle'; o.frequency.value = mtof(midi); o2.type = 'sine'; o2.frequency.value = mtof(midi - 12);
    lp.type = 'lowpass'; lp.frequency.value = 380;
    o.connect(lp); o2.connect(lp); lp.connect(g); g.connect(musicBus);
    env(g, t, 0.01, 0.32 * vel, 0.25, 0.6, 0.12, t + dur);
    o.start(t); o2.start(t); o.stop(t + dur + 0.2); o2.stop(t + dur + 0.2);
  }
  function lead(midi, t, dur, vel) {
    const o = ctx.createOscillator(), g = ctx.createGain(), lp = ctx.createBiquadFilter();
    o.type = 'triangle'; o.frequency.value = mtof(midi); lfoGain.connect(o.detune);
    lp.type = 'lowpass'; lp.frequency.value = 1500;
    o.connect(lp); lp.connect(g); g.connect(musicBus); g.connect(verbSend);
    env(g, t, 0.03, 0.06 * vel, 0.3, 0.5, 0.4, t + dur);
    o.start(t); o.stop(t + dur + 0.5);
  }
  let noiseBuf = null;
  function noise() {
    if (!noiseBuf) { noiseBuf = ctx.createBuffer(1, ctx.sampleRate, ctx.sampleRate); const d = noiseBuf.getChannelData(0); for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1; }
    const s = ctx.createBufferSource(); s.buffer = noiseBuf; return s;
  }
  function kick(t, vel) {
    const o = ctx.createOscillator(), g = ctx.createGain();
    o.frequency.setValueAtTime(115, t); o.frequency.exponentialRampToValueAtTime(42, t + 0.12);
    g.gain.setValueAtTime(0.75 * vel, t); g.gain.exponentialRampToValueAtTime(0.0001, t + 0.35);
    o.connect(g); g.connect(musicBus); o.start(t); o.stop(t + 0.4);
  }
  function snare(t, vel) {
    const n = noise(), bp = ctx.createBiquadFilter(), g = ctx.createGain();
    bp.type = 'bandpass'; bp.frequency.value = 1700; bp.Q.value = 0.7;
    g.gain.setValueAtTime(0.32 * vel, t); g.gain.exponentialRampToValueAtTime(0.0001, t + 0.2);
    n.connect(bp); bp.connect(g); g.connect(musicBus); g.connect(verbSend); n.start(t, Math.random() * 0.5); n.stop(t + 0.25);
    const o = ctx.createOscillator(), og = ctx.createGain(); o.frequency.value = 185;
    og.gain.setValueAtTime(0.16 * vel, t); og.gain.exponentialRampToValueAtTime(0.0001, t + 0.09);
    o.connect(og); og.connect(musicBus); o.start(t); o.stop(t + 0.1);
  }
  function hat(t, vel, open) {
    const n = noise(), hp = ctx.createBiquadFilter(), g = ctx.createGain();
    hp.type = 'highpass'; hp.frequency.value = 7200;
    const len = open ? 0.22 : 0.045;
    g.gain.setValueAtTime(0.09 * vel, t); g.gain.exponentialRampToValueAtTime(0.0001, t + len);
    n.connect(hp); hp.connect(g); g.connect(musicBus); n.start(t, Math.random() * 0.5); n.stop(t + len + 0.02);
  }

  // ---------------------------------------------------------------- sequencer (16th-note steps)
  function section(bar) {
    const b = song.bars;
    if (bar < 4) return 'intro';
    if (bar >= b - 4) return 'outro';
    if (bar >= Math.floor(b / 2) - 2 && bar < Math.floor(b / 2) + 2) return 'break';
    return 'main';
  }
  function schedule(s, t) {
    const sixteenth = 60 / song.bpm / 4;
    const bar = Math.floor(s / 16), pos = s % 16, beat = pos / 4;
    const sec = section(bar);
    const ch = song.prog[bar % song.prog.length];
    const root = song.key + ch[0];
    const tones = Q[ch[1]];
    const swingT = (pos % 2 === 1) ? song.swing * sixteenth * 2 : 0;
    const tt = t + swingT;
    const drums = sec === 'main';
    // chords
    if (pos === 0 || (pos === 8 && Math.random() < 0.35)) {
      const vel = pos === 0 ? rnd(0.8, 1) : rnd(0.45, 0.65);
      const dur = pos === 0 ? sixteenth * (Math.random() < 0.5 ? 14 : 7) : sixteenth * 6;
      tones.forEach((iv, k) => { if (k === 0 && tones.length > 4) return; ep(root + 12 + iv, tt + k * 0.012 + rnd(0, 0.008), dur, vel * rnd(0.85, 1)); });
    }
    // bass
    if (sec !== 'intro' && sec !== 'outro' && (pos === 0 || pos === 10 || (pos === 6 && Math.random() < 0.3))) {
      bass(root - 12 + (pos === 10 && Math.random() < 0.4 ? 7 : 0), tt, sixteenth * (pos === 0 ? 5 : 3), pos === 0 ? 1 : 0.75);
    }
    // drums
    if (drums) {
      if (song.kick.some((k) => Math.abs(k - beat) < 0.01)) kick(tt, pos === 0 ? 1 : 0.8);
      if (pos === 4 || pos === 12) snare(tt + rnd(0, 0.012), rnd(0.85, 1));
      if (pos === 15 && Math.random() < 0.15) snare(tt, 0.35);
      if (pos % 2 === 0 && Math.random() < song.hatP) hat(tt, pos % 4 === 0 ? 0.9 : 0.55, pos === 14 && Math.random() < 0.2);
    } else if (sec === 'break' && pos % 4 === 0) hat(tt, 0.3, false);
    // melody
    if ((sec === 'main' || sec === 'break') && pos % 2 === 0 && Math.random() < song.melodyP) {
      const n = song.key + 24 + pick(song.pent) + (Math.random() < 0.3 ? 12 : 0);
      lead(n, tt, sixteenth * pick([2, 3, 4, 6]), rnd(0.6, 1));
    }
    // outro fade
    if (sec === 'outro' && pos === 0) musicBus.gain.setTargetAtTime(Math.max(0.05, 0.8 * (song.bars - bar - 1) / 4), t, 0.8);
    if (bar === 0 && pos === 0) musicBus.gain.setTargetAtTime(0.8, t, 0.5);
  }
  function tick() {
    if (!song) newSong();
    while (nextTime < ctx.currentTime + 0.25) {
      schedule(step, nextTime);
      nextTime += 60 / song.bpm / 4;
      step++;
      if (step >= song.bars * 16) newSong();
    }
  }

  return {
    start() {
      if (!ctx) build();
      if (ctx.state === 'suspended') ctx.resume();
      if (on) return; on = true;
      if (!song) newSong(); else onSong(song.title, song.bpm);
      nextTime = ctx.currentTime + 0.1;
      timer = setInterval(tick, 40);
    },
    stop() { on = false; clearInterval(timer); timer = null; if (ctx) ctx.suspend(); },
    skip() { if (!ctx) return; musicBus.gain.setTargetAtTime(0.0001, ctx.currentTime, 0.15); setTimeout(() => { newSong(); nextTime = ctx.currentTime + 0.15; }, 450); },
    setVolume(v) { vol = v; if (master) master.gain.setTargetAtTime(v, ctx.currentTime, 0.05); },
    duck(d) { if (duckGain) duckGain.gain.setTargetAtTime(d ? 0.3 : 1, ctx.currentTime, 0.25); },
    suspend() { if (ctx && on) ctx.suspend(); },
    resume() { if (ctx && on) ctx.resume(); },
    get playing() { return on; },
    get title() { return song ? song.title : ''; },
    onSong(fn) { onSong = fn; },
    // Offline render of the first `sec` seconds of a fresh song (for previews/tests). Call before start().
    async renderPreview(sec) {
      const off = new OfflineAudioContext(2, Math.round(44100 * sec), 44100);
      build(off); newSong();
      nextTime = 0.05; step = 0;
      const title = song.title;
      while (nextTime < sec) { schedule(step, nextTime); nextTime += 60 / song.bpm / 4; step++; }
      const buf = await off.startRendering();
      ctx = null; song = null;
      return { buf, title };
    },
  };
})();
