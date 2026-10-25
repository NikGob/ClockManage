// The iPhone web app laid out as GitHub Pages serves it: pwa/ with ui/locales/ under locales/.
//
//   node scripts/media/pwa-server.mjs            (from the repo root; http://localhost:8767)
//
// A paired web app with a made-up day: put `demoState()` of iphone-day.mjs into localStorage
// `cm` (capture.mjs does) and block ntfy.sh.
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname } from 'node:path';
import { fileURLToPath } from 'node:url';

export const PWA_PORT = 8767;
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.json': 'application/json', '.webmanifest': 'application/manifest+json', '.png': 'image/png' };

export function servePwa(port = PWA_PORT) {
  return createServer((req, res) => {
    const path = new URL(req.url, 'http://localhost').pathname.replace(/\/$/, '/index.html');
    const file = path.startsWith('/locales/') ? `ui${path}` : `pwa${path}`;
    let body;
    try { body = readFileSync(file); } catch { return res.writeHead(404).end(); }
    res.writeHead(200, { 'Content-Type': TYPES[extname(file)] || 'application/octet-stream' }).end(body);
  }).listen(port);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  servePwa();
  console.log(`http://localhost:${PWA_PORT}/`);
}
