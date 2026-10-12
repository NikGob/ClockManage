// Minimal Chrome DevTools Protocol driver: headless Chrome, screenshots and frame capture.
import { spawn } from 'node:child_process';
import { mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';

const CHROME = 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const PORT = 9333;
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function launch(profileDir) {
  rmSync(profileDir, { recursive: true, force: true });
  const proc = spawn(CHROME, [
    '--headless=new', `--remote-debugging-port=${PORT}`, `--user-data-dir=${profileDir}`,
    '--no-first-run', '--no-default-browser-check', '--hide-scrollbars', '--force-color-profile=srgb',
    '--font-render-hinting=none', '--lang=en-US', 'about:blank',
  ], { stdio: 'ignore' });
  let ver;
  for (let i = 0; i < 50 && !ver; i++) {
    try { ver = await (await fetch(`http://127.0.0.1:${PORT}/json/version`)).json(); } catch { await sleep(200); }
  }
  const ws = new WebSocket(ver.webSocketDebuggerUrl);
  await new Promise((r) => ws.addEventListener('open', r, { once: true }));
  let id = 0;
  const pending = new Map();
  const handlers = [];
  ws.addEventListener('message', (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) {
      const { res, rej } = pending.get(m.id);
      pending.delete(m.id);
      m.error ? rej(new Error(JSON.stringify(m.error))) : res(m.result);
    } else if (m.method) handlers.forEach((h) => h(m));
  });
  const send = (method, params = {}, sessionId) => new Promise((res, rej) => {
    const mid = ++id;
    pending.set(mid, { res, rej });
    ws.send(JSON.stringify({ id: mid, method, params, sessionId }));
  });
  const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  const s = (method, params) => send(method, params, sessionId);
  await s('Page.enable');
  await s('Runtime.enable');

  const page = {
    async viewport(width, height, scale = 1) {
      await s('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: scale, mobile: false });
    },
    async scheme(value) {
      await s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value }, { name: 'prefers-reduced-motion', value: 'no-preference' }] });
    },
    async transparent() {
      await s('Emulation.setDefaultBackgroundColorOverride', { color: { r: 0, g: 0, b: 0, a: 0 } });
    },
    async go(url, settle = 1500) {
      const loaded = new Promise((r) => { const h = (m) => { if (m.method === 'Page.loadEventFired') { handlers.splice(handlers.indexOf(h), 1); r(); } }; handlers.push(h); });
      await s('Page.navigate', { url });
      await loaded;
      await sleep(settle);
    },
    async eval(expr) {
      const r = await s('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
      return r.result.value;
    },
    async click(sel) {
      const box = await page.eval(`(() => { const r = document.querySelector(${JSON.stringify(sel)}).getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
      for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) {
        await s('Input.dispatchMouseEvent', { type, x: box[0], y: box[1], button: 'left', clickCount: 1 });
      }
    },
    async shot(path, clip) {
      const r = await s('Page.captureScreenshot', { format: 'png', ...(clip ? { clip: { ...clip, scale: 1 } } : {}), captureBeyondViewport: false });
      writeFileSync(path, Buffer.from(r.data, 'base64'));
    },
    /** Frames for `ms` at about `fps`; `during(t)` runs actions at given times (ms). */
    async record(dir, ms, fps = 15, during = [], clip) {
      rmSync(dir, { recursive: true, force: true });
      mkdirSync(dir, { recursive: true });
      const step = 1000 / fps;
      const t0 = Date.now();
      let n = 0;
      const acts = [...during].sort((a, b) => a[0] - b[0]);
      while (Date.now() - t0 < ms) {
        const t = Date.now() - t0;
        while (acts.length && acts[0][0] <= t) await acts.shift()[1]();
        const r = await s('Page.captureScreenshot', { format: 'png', ...(clip ? { clip: { ...clip, scale: 1 } } : {}) });
        writeFileSync(join(dir, `f${String(n++).padStart(4, '0')}.png`), Buffer.from(r.data, 'base64'));
        const next = t0 + n * step;
        const wait = next - Date.now();
        if (wait > 0) await sleep(wait);
      }
      return { frames: n, fps: n / (ms / 1000) };
    },
    /** Compositor frames for `ms` (JPEG, with their timestamps); `during` = [[ms, action]]. */
    async screencast(dir, ms, during = []) {
      rmSync(dir, { recursive: true, force: true });
      mkdirSync(dir, { recursive: true });
      const list = [];
      const h = (m) => {
        if (m.method !== 'Page.screencastFrame') return;
        const n = list.length;
        writeFileSync(join(dir, `f${String(n).padStart(4, '0')}.jpg`), Buffer.from(m.params.data, 'base64'));
        list.push(m.params.metadata.timestamp);
        s('Page.screencastFrameAck', { sessionId: m.params.sessionId }).catch(() => {});
      };
      handlers.push(h);
      await s('Page.startScreencast', { format: 'jpeg', quality: 92, everyNthFrame: 1 });
      const t0 = Date.now();
      const acts = [...during].sort((a, b) => a[0] - b[0]);
      while (Date.now() - t0 < ms) {
        while (acts.length && acts[0][0] <= Date.now() - t0) await acts.shift()[1]();
        await sleep(10);
      }
      await s('Page.stopScreencast');
      handlers.splice(handlers.indexOf(h), 1);
      writeFileSync(join(dir, 'times.json'), JSON.stringify(list));
      return { frames: list.length, fps: list.length / (ms / 1000) };
    },
    close() { try { ws.close(); } catch { /* */ } proc.kill(); },
  };
  return page;
}
