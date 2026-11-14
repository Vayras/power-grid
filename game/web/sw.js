// Offline cache: the game works without a connection after the first load.
const CACHE = 'power-grid-v29';
const CORE = ['./', 'index.html', 'style.css', 'game.js', 'music.js', 'history.js', 'game.wasm', 'atlas.png', 'sprites.json', 'manifest.webmanifest', 'icon-192.png', 'icon-512.png', 'donate-qr.png', 'qr.js', 'audio/rain-light.m4a', 'audio/rain-heavy.m4a', 'audio/wind.m4a'];
self.addEventListener('install', (e) => { e.waitUntil(caches.open(CACHE).then((c) => c.addAll(CORE)).then(() => self.skipWaiting())); });
self.addEventListener('activate', (e) => { e.waitUntil(caches.keys().then((ks) => Promise.all(ks.filter((k) => k !== CACHE).map((k) => caches.delete(k)))).then(() => self.clients.claim())); });
// Network first, so a new version always wins; the cache is only the offline fallback.
self.addEventListener('fetch', (e) => {
  if (e.request.method !== 'GET' || !e.request.url.startsWith(self.location.origin)) return;
  e.respondWith(fetch(e.request).then((res) => {
    if (res.ok) { const copy = res.clone(); caches.open(CACHE).then((c) => c.put(e.request, copy)); }
    return res;
  }).catch(() => caches.match(e.request, { ignoreSearch: true })));
});
