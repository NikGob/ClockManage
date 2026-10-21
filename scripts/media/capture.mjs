// README media: screenshots and screencast frames of the UI on the mock backend.
//
//   cd ui && python -m http.server 8766                  (in another terminal)
//   node scripts/media/capture.mjs <stills> <frames> <chrome-profile> [names,...]
//   python scripts/media/build.py <stills> <frames> docs/media
//
// Needs Google Chrome and ffmpeg; no npm packages (Chrome is driven over CDP).
// The iPhone web app is served by this script itself, laid out as GitHub Pages serves it.
import { launch, sleep } from './cdp.mjs';
import { readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname } from 'node:path';

const [out, frames, profile, only] = process.argv.slice(2);
const B = 'http://localhost:8766';
const PWA = 'http://localhost:8767';
const W = 1280, H = 720;
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.json': 'application/json', '.webmanifest': 'application/manifest+json', '.png': 'image/png' };
const pwa = createServer((req, res) => {
  const path = new URL(req.url, PWA).pathname.replace(/\/$/, '/index.html');
  const file = path.startsWith('/locales/') ? `ui${path}` : `pwa${path}`;
  let body;
  try { body = readFileSync(file); } catch { return res.writeHead(404).end(); }
  res.writeHead(200, { 'Content-Type': TYPES[extname(file)] || 'application/octet-stream' }).end(body);
}).listen(8767);
const p = await launch(profile);
const meta = {};
const want = (name) => !only || only.split(',').includes(name);

async function main(url, settle) {
  await p.go(`${B}/index.html?lang=en&${url}`, settle);
}
async function rec(name, ms, fps, during) {
  if (!want(name)) return;
  meta[name] = await p.screencast(`${frames}/${name}`, ms, during);
  console.log(name, meta[name]);
}
async function shot(name, clip) {
  if (!want(name)) return;
  await p.shot(`${out}/${name}.png`, clip);
  console.log(name);
}

try {
  await p.viewport(W, H, 1);
  await p.scheme('dark');
  await p.go(`${B}/index.html?lang=en`);
  await p.eval(`localStorage.setItem('lang','en'); 1`);

  // Timer: the wave ring and the ticking digits.
  if (want('timer')) { await main('state=work&route=today', 2500); await rec('timer', 6200, 15, [[1800, () => p.click('[data-act="pause"]')], [4000, () => p.click('[data-act="resume"]')]]); }

  // Navigating between screens: the indicator drop and the cascading contents.
  if (want('tour')) {
    await main('state=work&route=today', 2500);
    const nav = (r) => () => p.click(`[data-route="${r}"]`);
    await rec('tour', 8200, 15, [[700, nav('plan')], [2300, nav('block')], [3900, nav('log')], [5500, nav('settings')], [7100, nav('today')]]);
  }

  // Theme: colour and light/dark spread as a circle from the pressed control.
  if (want('theme')) {
    await main('state=work&route=settings', 2500);
    await p.eval(`document.querySelector('[data-seed]').scrollIntoView({ block: 'center' }); 1`);
    await sleep(800);
    const sw = (c) => () => p.click(`[data-seed="${c}"]`);
    const mode = (m) => () => p.click(`[data-seg="mode"] [data-v="${m}"]`);
    await rec('theme', 7600, 15, [[600, sw('#1565C0')], [2000, sw('#C2185B')], [3400, mode('light')], [5000, sw('#E65100')], [6300, sw('#2E7D32')]]);
  }

  // Alerts on top of everything.
  for (const [name, demo, ms] of [['alarm', 'await', 4200], ['block', 'block', 3600], ['nope', 'nope', 3200], ['wake', 'wake', 3200]]) {
    if (!want(name)) continue;
    await p.go(`${B}/overlay.html?lang=en&demo=${demo}`, 0);
    await rec(name, ms, 15);
  }

  // Stills, dark.
  await p.viewport(W, 800, 1);
  if (want('today-dark')) { await main('state=work&route=today', 2500); await shot('today-dark'); }
  if (want('plan')) { await main('state=work&route=plan', 2500); await shot('plan'); }
  if (want('blocking')) { await main('state=work&route=block', 2500); await shot('blocking'); }
  if (want('log')) { await main('state=work&route=log', 2500); await shot('log'); }
  if (want('week')) { await main('state=work&route=log', 2000); await p.click('[data-logview="week"]'); await sleep(1500); await shot('week'); }
  if (want('settings')) { await main('state=work&route=settings', 2500); await shot('settings'); }
  if (want('nap')) { await main('state=segment&prep=1&route=today', 2500); await shot('nap'); }
  if (want('meals')) { await main('state=work&route=meals', 2500); await shot('meals'); }
  // The iPhone web app, paired and installed, with the mock's picks; ntfy.sh is never reached.
  if (want('iphone-picks') || want('iphone-diet')) {
    await main('state=work&route=meals', 1500);
    const picks = await p.eval(`import('./js/mock.js').then(async (m) => JSON.stringify((await m.invoke('meals_state')).picks))`);
    const diet = JSON.parse(readFileSync('ui/locales/en.json', 'utf8')).meals_goal_a_clear_steady_mind;
    await p.cdp('Network.enable');
    await p.cdp('Network.setBlockedURLs', { urls: ['*ntfy.sh*'] });
    await p.cdp('Page.addScriptToEvaluateOnNewDocument', { source: `Object.defineProperty(navigator, 'standalone', { value: true }); if (window.Notification) Object.defineProperty(Notification, 'permission', { get: () => 'granted' });` });
    await p.cdp('Emulation.setDeviceMetricsOverride', { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
    await p.go(`${PWA}/`, 500);
    const state = { link: { up: 'demo', down: 'demo', key: '' }, vapid: '', pc: 'Home PC', subscribed: true, diet: { text: diet, at: 1 } };
    await p.eval(`localStorage.setItem('cm', ${JSON.stringify(JSON.stringify(state))}); caches.open('cm').then((c) => c.put('picks.json', new Response(${JSON.stringify(picks)}))).then(() => 1)`);
    await p.scheme('dark');
    if (want('iphone-picks')) { await p.go(`${PWA}/`, 2000); await shot('iphone-picks'); }
    await p.scheme('light');
    if (want('iphone-diet')) {
      await p.go(`${PWA}/`, 2000);
      await p.eval(`document.querySelector('details').open = true; document.querySelector('details').scrollIntoView({ block: 'end' }); 1`);
      await sleep(600);
      await shot('iphone-diet');
    }
    await p.scheme('dark');
    await p.viewport(W, 800, 1);
  }
  // Stills, light.
  await p.scheme('light');
  if (want('today-light')) { await main('state=break&route=today', 2500); await shot('today-light'); }
  if (want('work-light')) { await main('state=work&route=today', 2500); await shot('work-light'); }
  if (want('idle-light')) { await main('state=idle&route=today', 3000); await shot('idle-light'); }
  // Mini timer, transparent around the pill.
  for (const [name, scheme] of [['mini-dark', 'dark'], ['mini-light', 'light']]) {
    if (!want(name)) continue;
    await p.scheme(scheme);
    await p.viewport(420, 120, 2);
    await p.go(`${B}/mini.html?lang=en&state=work`, 2000);
    await p.eval(`document.documentElement.style.background = 'transparent'; document.body.style.background = 'transparent'; 1`);
    await p.transparent?.();
    const r = await p.eval(`JSON.stringify(document.getElementById('mini').getBoundingClientRect())`);
    const b = JSON.parse(r);
    await shot(name, { x: b.x, y: b.y, width: b.width, height: b.height });
  }
  writeFileSync(`${frames}/meta.json`, JSON.stringify(meta, null, 1));
} finally {
  pwa.close();
  p.close();
  await sleep(300);
}
