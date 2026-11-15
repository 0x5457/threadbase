/* Threadbase service worker: installable shell and Web Push notifications. */
const CACHE = "threadbase-shell-v1";

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(["/", "/favicon.svg", "/manifest.webmanifest"]))
      .catch(() => undefined)
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))),
      )
      .then(() => self.clients.claim()),
  );
});

/* Network-first navigation with the app shell as an offline fallback. */
self.addEventListener("fetch", (event) => {
  if (event.request.method !== "GET" || event.request.mode !== "navigate") return;
  event.respondWith(
    fetch(event.request)
      .then((response) => {
        const copy = response.clone();
        caches.open(CACHE).then((cache) => cache.put("/", copy)).catch(() => undefined);
        return response;
      })
      .catch(() => caches.match("/")),
  );
});

self.addEventListener("push", (event) => {
  let payload = {};
  try {
    payload = event.data ? event.data.json() : {};
  } catch {
    payload = { body: event.data ? event.data.text() : "" };
  }
  const url = new URL(payload.url || "/", self.location.origin).href;
  event.waitUntil(
    (async () => {
      const clients = await self.clients.matchAll({
        type: "window",
        includeUncontrolled: true,
      });
      // A visible tab already on this thread is the notification.
      if (clients.some((client) => client.visibilityState === "visible" && client.url === url)) {
        return;
      }
      await self.registration.showNotification(payload.title || "Threadbase", {
        body: payload.body || "",
        icon: "/icon-192.png",
        badge: "/icon-192.png",
        tag: payload.tag || undefined,
        data: { url },
      });
    })(),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = new URL(event.notification.data?.url || "/", self.location.origin).href;
  event.waitUntil(
    (async () => {
      const clients = await self.clients.matchAll({
        type: "window",
        includeUncontrolled: true,
      });
      for (const client of clients) {
        if (client.url === url && "focus" in client) {
          await client.focus();
          return;
        }
      }
      await self.clients.openWindow(url);
    })(),
  );
});
