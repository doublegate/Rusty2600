// Rusty2600 PWA service worker (`[v2.9.0]` "Full Circle").
//
// Makes the wasm demo installable + usable offline after a first visit. Trunk hashes the
// `.wasm`/`.js` glue filenames per build (see `web/index.html`'s comment on why the Trunk
// project pins those paths), so a fixed precache manifest would go stale on every rebuild;
// instead this uses a runtime cache-first-then-revalidate strategy over same-origin GETs. The
// app shell (HTML, wasm, JS glue, CSS, icons, manifest) is cached the first time it loads, so a
// later offline visit is served from the cache. ROMs are loaded by the user from local disk
// (a `<input type=file>`/native file picker) and never fetched over the network, so nothing
// proprietary is ever cached here. Matches RustyNES's own `sw.js` convention.
//
// Bump CACHE_NAME to evict a previous deploy's shell.

"use strict";

const CACHE_NAME = "rusty2600-shell-v1";

// Activate as soon as possible — don't wait for existing tabs to close before taking over.
self.addEventListener("install", () => {
    self.skipWaiting();
});

// Drop any cache from a previous CACHE_NAME so a new deploy isn't served a stale wasm binary
// alongside fresh JS glue (a mismatched pair would be a hard boot failure, not a soft one).
self.addEventListener("activate", (event) => {
    event.waitUntil(
        (async () => {
            const keys = await caches.keys();
            await Promise.all(
                keys.filter((k) => k !== CACHE_NAME).map((k) => caches.delete(k))
            );
            await self.clients.claim();
        })()
    );
});

// The cache key for a request. Navigation requests (the app shell's `index.html`) may carry a
// `?settings=…` share-link query (`crate::share_link`, `[v2.9.0]`) that varies per link — keying
// the cache by the full URL would (a) duplicate the whole shell once per distinct share link and
// (b) make a freshly-opened share link miss the cache and fail offline. So navigations are
// normalized to their pathname (query + hash stripped); every `?settings=…` URL resolves to the
// one cached shell, and the wasm side reads the query itself once the page has actually loaded.
// Sub-resources (wasm/JS/icons/manifest) keep their full URL as the cache key.
function cacheKey(request) {
    if (request.mode === "navigate") {
        const url = new URL(request.url);
        url.search = "";
        url.hash = "";
        return new Request(url.toString(), { method: "GET" });
    }
    return request;
}

// Cache-first for same-origin GETs, with a background revalidate so a stale cached entry is
// refreshed for next time without blocking this response. Falls back to (and populates the
// cache from) the network on a cache miss. Cross-origin and non-GET requests pass straight
// through untouched.
self.addEventListener("fetch", (event) => {
    const request = event.request;
    if (request.method !== "GET") {
        return;
    }
    const url = new URL(request.url);
    if (url.origin !== self.location.origin) {
        return;
    }

    const key = cacheKey(request);
    event.respondWith(
        (async () => {
            const cache = await caches.open(CACHE_NAME);
            const cached = await cache.match(key);
            if (cached) {
                event.waitUntil(
                    fetch(request)
                        .then((response) => {
                            if (response && response.ok) {
                                cache.put(key, response.clone());
                            }
                        })
                        .catch(() => {
                            // Offline: keep serving the cached copy, nothing to refresh with.
                        })
                );
                return cached;
            }
            try {
                const response = await fetch(request);
                if (response && response.ok) {
                    cache.put(key, response.clone());
                }
                return response;
            } catch (err) {
                // Offline + not yet cached: nothing this worker can do about it.
                return Response.error();
            }
        })()
    );
});
