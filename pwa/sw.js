// Pushes from the PC: show the notification at once (iOS revokes silent pushes) and keep the picks
// for the page.
self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()));

self.addEventListener('push', (e) => {
  let msg = {};
  try { msg = e.data ? e.data.json() : {}; } catch { msg = { title: 'ClockManage', body: e.data?.text() || '' }; }
  e.waitUntil((async () => {
    if (msg.picks) {
      const cache = await caches.open('cm');
      await cache.put('picks.json', new Response(JSON.stringify(msg.picks), { headers: { 'Content-Type': 'application/json' } }));
    }
    await self.registration.showNotification(msg.title || 'ClockManage', { body: msg.body || '', icon: 'icon-192.png', tag: 'meals' });
    for (const c of await self.clients.matchAll({ type: 'window' })) c.postMessage('picks');
  })());
});

self.addEventListener('notificationclick', (e) => {
  e.notification.close();
  e.waitUntil((async () => {
    const open = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
    if (open.length) return open[0].focus();
    return self.clients.openWindow('./');
  })());
});
