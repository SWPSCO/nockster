// Nockster PWA service worker.
// Strategy:
//   - navigations  -> network-first (always get the latest app shell when online),
//                     fall back to the cached shell offline.
//   - hashed assets (/assets/*, content-hashed + immutable) -> cache-first.
//   - other same-origin GETs -> stale-while-revalidate.
//   - /updates/* (OTA index/bundle/firmware) and cross-origin (Nockblocks API,
//     GitHub, etc.) -> bypass the SW entirely (freshness / no CORS interference).
// Bump CACHE to force old caches out on the next activate.
const CACHE = 'nockster-v1';

self.addEventListener('install', () => {
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    const keys = await caches.keys();
    await Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)));
    await self.clients.claim();
  })());
});

self.addEventListener('fetch', (event) => {
  const req = event.request;
  if (req.method !== 'GET') return;

  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return; // cross-origin: let the network handle it
  if (url.pathname.startsWith('/updates/')) return; // OTA must always be fresh

  // App shell / navigations: network-first, offline fallback to cached index.
  if (req.mode === 'navigate') {
    event.respondWith((async () => {
      try {
        const net = await fetch(req);
        const cache = await caches.open(CACHE);
        cache.put('/index.html', net.clone());
        return net;
      } catch {
        const cache = await caches.open(CACHE);
        const cached = await cache.match('/index.html');
        return cached || Response.error();
      }
    })());
    return;
  }

  // Immutable hashed build assets (incl. the wasm): cache-first.
  if (url.pathname.startsWith('/assets/')) {
    event.respondWith((async () => {
      const cache = await caches.open(CACHE);
      const hit = await cache.match(req);
      if (hit) return hit;
      const net = await fetch(req);
      if (net.ok) cache.put(req, net.clone());
      return net;
    })());
    return;
  }

  // Everything else same-origin (icons, manifest, ...): stale-while-revalidate.
  event.respondWith((async () => {
    const cache = await caches.open(CACHE);
    const hit = await cache.match(req);
    const network = fetch(req)
      .then((net) => {
        if (net.ok) cache.put(req, net.clone());
        return net;
      })
      .catch(() => hit);
    return hit || network;
  })());
});
