// A small QR code encoder (ISO/IEC 18004): alphanumeric or byte mode, error correction L, versions 1-20.
// Enough for Lightning invoices, which are written in upper case so they fit the denser alphanumeric mode.
// Usage: PGQR.svg(text, px) -> an SVG string. No dependencies, no network.
'use strict';
window.PGQR = (function () {
  // Error correction level L: [blocks, total codewords, data codewords] per group, versions 1..20.
  const RS_L = [null,
    [1, 26, 19], [1, 44, 34], [1, 70, 55], [1, 100, 80], [1, 134, 108], [2, 86, 68], [2, 98, 78], [2, 121, 97], [2, 146, 116],
    [2, 86, 68, 2, 87, 69], [4, 101, 81], [2, 116, 92, 2, 117, 93], [4, 133, 107], [3, 145, 115, 1, 146, 116],
    [5, 109, 87, 1, 110, 88], [5, 122, 98, 1, 123, 99], [1, 135, 107, 5, 136, 108], [5, 150, 120, 1, 151, 121],
    [3, 141, 113, 4, 142, 114], [3, 135, 107, 5, 136, 108]];
  const ALIGN = [null, [], [6, 18], [6, 22], [6, 26], [6, 30], [6, 34], [6, 22, 38], [6, 24, 42], [6, 26, 46], [6, 28, 50],
    [6, 30, 54], [6, 32, 58], [6, 34, 62], [6, 26, 46, 66], [6, 26, 48, 70], [6, 26, 50, 74], [6, 30, 54, 78], [6, 30, 56, 82],
    [6, 30, 58, 86], [6, 34, 62, 90]];
  const ALNUM = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:';

  // GF(256) with the QR polynomial 0x11d
  const EXP = new Array(512), LOG = new Array(256);
  for (let i = 0, x = 1; i < 255; i++) { EXP[i] = x; LOG[x] = i; x <<= 1; if (x & 256) x ^= 0x11d; }
  for (let i = 255; i < 512; i++) EXP[i] = EXP[i - 255];
  const mul = (a, b) => (a && b ? EXP[LOG[a] + LOG[b]] : 0);
  function rsGen(n) { let g = [1]; for (let i = 0; i < n; i++) { const ng = new Array(g.length + 1).fill(0); for (let j = 0; j < g.length; j++) { ng[j] ^= g[j]; ng[j + 1] ^= mul(g[j], EXP[i]); } g = ng; } return g; }
  function rsEc(data, n) {
    const g = rsGen(n), r = data.concat(new Array(n).fill(0));
    for (let i = 0; i < data.length; i++) { const c = r[i]; if (c) for (let j = 0; j < g.length; j++) r[i + j] ^= mul(g[j], c); }
    return r.slice(data.length);
  }
  const blocksOf = (v) => { const t = RS_L[v], out = []; for (let i = 0; i < t.length; i += 3) for (let k = 0; k < t[i]; k++) out.push([t[i + 1], t[i + 2]]); return out; };
  const dataCap = (v) => blocksOf(v).reduce((s, b) => s + b[1], 0);

  function encode(text) {
    const alnum = [...text].every((c) => ALNUM.includes(c));
    const bytes = alnum ? null : [...new TextEncoder().encode(text)];
    const n = alnum ? text.length : bytes.length;
    let v = 1;
    for (; v <= 20; v++) {
      const cc = alnum ? (v < 10 ? 9 : 11) : (v < 10 ? 8 : 16);
      const bits = 4 + cc + (alnum ? 11 * (n >> 1) + 6 * (n & 1) : 8 * n);
      if (bits <= dataCap(v) * 8) break;
    }
    if (v > 20) throw new Error('Too long for a QR code');
    const bits = [];
    const put = (val, len) => { for (let i = len - 1; i >= 0; i--) bits.push((val >>> i) & 1); };
    put(alnum ? 2 : 4, 4); put(n, alnum ? (v < 10 ? 9 : 11) : (v < 10 ? 8 : 16));
    if (alnum) { for (let i = 0; i + 1 < n; i += 2) put(ALNUM.indexOf(text[i]) * 45 + ALNUM.indexOf(text[i + 1]), 11); if (n & 1) put(ALNUM.indexOf(text[n - 1]), 6); }
    else for (const b of bytes) put(b, 8);
    const cap = dataCap(v) * 8;
    put(0, Math.min(4, cap - bits.length)); while (bits.length % 8) bits.push(0);
    const data = []; for (let i = 0; i < bits.length; i += 8) data.push(parseInt(bits.slice(i, i + 8).join(''), 2));
    for (let p = 0; data.length < cap / 8; p++) data.push(p % 2 ? 0x11 : 0xec);
    // split into blocks, add error correction, interleave
    const bl = blocksOf(v), dB = [], eB = []; let off = 0;
    for (const [tot, dc] of bl) { const d = data.slice(off, off + dc); off += dc; dB.push(d); eB.push(rsEc(d, tot - dc)); }
    const out = [];
    for (let i = 0; i < Math.max(...dB.map((d) => d.length)); i++) for (const d of dB) if (i < d.length) out.push(d[i]);
    for (let i = 0; i < eB[0].length; i++) for (const e of eB) out.push(e[i]);
    return { v, cw: out };
  }

  function matrix(text) {
    const { v, cw } = encode(text);
    const N = 17 + 4 * v, M = [], F = [];
    for (let i = 0; i < N; i++) { M.push(new Array(N).fill(0)); F.push(new Array(N).fill(false)); }
    const set = (r, c, d) => { M[r][c] = d ? 1 : 0; F[r][c] = true; };
    const finder = (r, c) => { for (let y = -1; y <= 7; y++) for (let x = -1; x <= 7; x++) { const rr = r + y, cc = c + x; if (rr < 0 || cc < 0 || rr >= N || cc >= N) continue; const ring = Math.max(Math.abs(y - 3), Math.abs(x - 3)); set(rr, cc, ring !== 2 && ring !== 4); } };
    finder(0, 0); finder(0, N - 7); finder(N - 7, 0);
    for (let i = 8; i < N - 8; i++) { set(6, i, i % 2 === 0); set(i, 6, i % 2 === 0); }
    const al = ALIGN[v];
    // skip only the three that would sit on a finder; the ones on the timing lines overwrite them
    for (const [ri, r] of al.entries()) for (const [ci, c] of al.entries()) { const last = al.length - 1; if ((ri === 0 && ci === 0) || (ri === 0 && ci === last) || (ri === last && ci === 0)) continue; for (let y = -2; y <= 2; y++) for (let x = -2; x <= 2; x++) set(r + y, c + x, Math.max(Math.abs(y), Math.abs(x)) !== 1); }
    // reserve format and version areas
    for (let i = 0; i < 9; i++) { if (!F[8][i]) set(8, i, 0); if (!F[i][8]) set(i, 8, 0); }
    for (let i = 0; i < 8; i++) { set(8, N - 1 - i, 0); set(N - 1 - i, 8, 0); }
    set(N - 8, 8, 1); // the dark module
    let vbits = 0;
    if (v >= 7) { let rem = v; for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >> 11) * 0x1f25); vbits = (v << 12) | rem; for (let i = 0; i < 18; i++) { const b = (vbits >> i) & 1, a = Math.floor(i / 3), c = N - 11 + (i % 3); set(a, c, b); set(c, a, b); } }
    // data, in the zigzag
    const dbits = []; for (const b of cw) for (let i = 7; i >= 0; i--) dbits.push((b >> i) & 1);
    let k = 0, up = true;
    for (let c = N - 1; c > 0; c -= 2) {
      if (c === 6) c = 5;
      for (let i = 0; i < N; i++) { const r = up ? N - 1 - i : i; for (const cc of [c, c - 1]) if (!F[r][cc]) { M[r][cc] = k < dbits.length ? dbits[k] : 0; k++; } }
      up = !up;
    }
    const MASKS = [(r, c) => (r + c) % 2 === 0, (r) => r % 2 === 0, (r, c) => c % 3 === 0, (r, c) => (r + c) % 3 === 0,
      (r, c) => (Math.floor(r / 2) + Math.floor(c / 3)) % 2 === 0, (r, c) => ((r * c) % 2) + ((r * c) % 3) === 0,
      (r, c) => (((r * c) % 2) + ((r * c) % 3)) % 2 === 0, (r, c) => (((r + c) % 2) + ((r * c) % 3)) % 2 === 0];
    function withMask(m) {
      const X = M.map((row) => row.slice());
      for (let r = 0; r < N; r++) for (let c = 0; c < N; c++) if (!F[r][c] && MASKS[m](r, c)) X[r][c] ^= 1;
      let f = (1 << 3) | m, rem = f; // level L = 01
      for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >> 9) * 0x537);
      f = ((f << 10) | rem) ^ 0x5412;
      for (let i = 0; i < 15; i++) {
        const b = (f >> i) & 1;
        // first copy, wrapped around the top-left finder (X[row][col])
        if (i < 6) X[i][8] = b; else if (i === 6) X[7][8] = b; else if (i === 7) X[8][8] = b; else if (i === 8) X[8][7] = b; else X[8][14 - i] = b;
        // second copy, split between the top-right and bottom-left finders
        if (i < 8) X[8][N - 1 - i] = b; else X[N - 15 + i][8] = b;
      }
      X[N - 8][8] = 1;
      return X;
    }
    function penalty(X) {
      let p = 0;
      for (let pass = 0; pass < 2; pass++) for (let r = 0; r < N; r++) {
        let run = 1;
        for (let c = 1; c <= N; c++) {
          const a = pass ? X[c - 1]?.[r] : X[r][c - 1], b = c < N ? (pass ? X[c][r] : X[r][c]) : -1;
          if (a === b) run++; else { if (run >= 5) p += run - 2; run = 1; }
        }
      }
      for (let r = 0; r < N - 1; r++) for (let c = 0; c < N - 1; c++) { const s = X[r][c] + X[r + 1][c] + X[r][c + 1] + X[r + 1][c + 1]; if (s === 0 || s === 4) p += 3; }
      const pat = [1, 0, 1, 1, 1, 0, 1, 0, 0, 0, 0], pat2 = [0, 0, 0, 0, 1, 0, 1, 1, 1, 0, 1];
      for (let r = 0; r < N; r++) for (let c = 0; c <= N - 11; c++) for (const P of [pat, pat2]) {
        if (P.every((b, i) => X[r][c + i] === b)) p += 40;
        if (P.every((b, i) => X[c + i][r] === b)) p += 40;
      }
      let dark = 0; for (const row of X) for (const b of row) dark += b;
      p += Math.floor(Math.abs(dark * 20 / (N * N) - 10)) * 10;
      return p;
    }
    let best = null, bestP = Infinity;
    for (let m = 0; m < 8; m++) { const X = withMask(m), p = penalty(X); if (p < bestP) { bestP = p; best = X; } }
    return best;
  }

  function svg(text, px = 220) {
    const X = matrix(text), N = X.length, q = 4, S = N + 2 * q;
    let d = '';
    for (let r = 0; r < N; r++) for (let c = 0; c < N; c++) if (X[r][c]) d += `M${c + q} ${r + q}h1v1h-1z`;
    return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${S} ${S}" width="${px}" height="${px}" shape-rendering="crispEdges" role="img"><rect width="${S}" height="${S}" fill="#fff"/><path d="${d}" fill="#000"/></svg>`;
  }
  return { svg, matrix };
})();
