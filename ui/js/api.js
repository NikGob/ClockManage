// Thin bridge to the Rust side. Outside Tauri (plain browser preview) a mock backend is used.
const T = window.__TAURI__;
let mock = null;

async function getMock() {
  if (!mock) mock = await import('./mock.js');
  return mock;
}

export const inTauri = !!T;

export async function call(cmd, args = {}) {
  if (T) return T.core.invoke(cmd, args);
  return (await getMock()).invoke(cmd, args);
}

export async function on(event, cb) {
  if (T) return T.event.listen(event, (e) => cb(e.payload));
  return (await getMock()).listen(event, cb);
}

export function startDrag(e) {
  if (!T || e.button !== 0) return;
  if (e.target.closest('button, input, a, [data-nodrag]')) return;
  T.window.getCurrentWindow().startDragging();
}

// ---- formatting ----
export function mmss(ms) {
  const s = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const p = (n) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${p(m)}:${p(sec)}` : `${p(m)}:${p(sec)}`;
}

export function dur(ms) {
  const m = Math.round(Math.max(0, ms) / 60000);
  const h = Math.floor(m / 60);
  const r = m % 60;
  if (h === 0) return `${r} мин`;
  return r ? `${h} ч ${r} мин` : `${h} ч`;
}

export function minutes(min) {
  return dur(min * 60000);
}

export function hm(ts) {
  const d = new Date(ts);
  return d.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
}

export function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function spawnRipple(el, x, y) {
  const r = el.getBoundingClientRect();
  const size = Math.hypot(Math.max(x, r.width - x), Math.max(y, r.height - y)) * 2;
  const dot = document.createElement('span');
  dot.className = 'ripple';
  dot.style.cssText = `width:${size}px;height:${size}px;left:${x - size / 2}px;top:${y - size / 2}px`;
  el.appendChild(dot);
  const born = performance.now();
  let released = false;
  let grown = false;
  const fade = () => {
    if (!released || !grown || dot.classList.contains('out')) return;
    dot.classList.add('out');
    setTimeout(() => dot.remove(), 380);
  };
  dot.addEventListener('animationend', () => { grown = true; fade(); });
  return () => {
    released = true;
    // Always show at least a short ripple even for a quick tap.
    setTimeout(fade, Math.max(0, 120 - (performance.now() - born)));
  };
}

function hop(el) {
  if (!el.classList.contains('btn')) return;
  el.classList.remove('released');
  void el.offsetWidth;
  el.classList.add('released');
  setTimeout(() => el.classList.remove('released'), 450);
}

/** Material ripple (pointer + keyboard) and the release "hop" on any `.interactive` element. */
export function installRipples(root = document) {
  root.addEventListener('pointerdown', (e) => {
    const el = e.target.closest('.interactive');
    if (!el || el.disabled || e.button > 0) return;
    const r = el.getBoundingClientRect();
    const release = spawnRipple(el, e.clientX - r.left, e.clientY - r.top);
    const up = () => {
      release();
      hop(el);
      el.removeEventListener('pointerleave', cancel);
    };
    const cancel = () => {
      release();
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointerup', up, { once: true });
    el.addEventListener('pointerleave', cancel, { once: true });
  });
  root.addEventListener('keydown', (e) => {
    if ((e.key !== 'Enter' && e.key !== ' ') || e.repeat) return;
    const el = e.target.closest?.('.interactive');
    if (!el || el.disabled || el !== e.target) return;
    const r = el.getBoundingClientRect();
    const release = spawnRipple(el, r.width / 2, r.height / 2);
    el.addEventListener('keyup', () => { release(); hop(el); }, { once: true });
  });
}
