// README media: screenshots and screencast frames of the UI on the mock backend.
//
//   cd ui && python -m http.server 8766                  (in another terminal)
//   node scripts/media/capture.mjs <stills> <frames> <chrome-profile> [names,...]
//   python scripts/media/build.py <stills> <frames> docs/media
//
// Needs Google Chrome and ffmpeg; no npm packages (Chrome is driven over CDP).
import { launch, sleep } from './cdp.mjs';
import { writeFileSync } from 'node:fs';

const [out, frames, profile, only] = process.argv.slice(2);
const B = 'http://localhost:8766';
const W = 1280, H = 720;
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
  p.close();
  await sleep(300);
}
