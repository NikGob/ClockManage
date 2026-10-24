// The iPhone side of ClockManage: pair with the PC through ntfy.sh, show the day (the timer and
// the plan) the PC sends, and while Meals is on take pushes, show the picks, ask for a pick and
// edit the diet. Pages under a bottom bar, like the Android app; nothing here blocks anything.
// Same sealing as core/src/relay.rs: AES-256-GCM, base64url(nonce ‖ ct).
const NTFY = 'https://ntfy.sh';
const PART = 3000;
const MIN = 60000;
// The PC's language once a day message told it (as on Android), the iPhone's before that.
let lang = load().lang || (/^(ru|uk|be)\b/i.test(navigator.language || '') ? 'ru' : 'en');
let LOCALE = lang === 'en' ? 'en-GB' : 'ru-RU';
let texts = {};

function tr(key, args) {
  let s = texts[key] ?? key;
  for (const [k, v] of Object.entries(args || {})) s = s.split(`{${k}}`).join(String(v));
  return s;
}

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const app = document.getElementById('app');
const nav = document.getElementById('nav');

// Material icons (Apache-2.0), the same family as the desktop app.
const ICON = {
  restaurant: 'M11 9H9V2H7v7H5V2H3v7c0 2.12 1.66 3.84 3.75 3.97V22h2.5v-9.03C11.34 12.84 13 11.12 13 9V2h-2v7zm5-3v8h2.5v8H21V2c-2.76 0-5 2.24-5 4z',
  cart: 'M7 18c-1.1 0-1.99.9-1.99 2S5.9 22 7 22s2-.9 2-2-.9-2-2-2zM1 2v2h2l3.6 7.59-1.35 2.45c-.16.28-.25.61-.25.96 0 1.1.9 2 2 2h12v-2H7.42c-.14 0-.25-.11-.25-.25l.03-.12.9-1.63h7.45c.75 0 1.41-.41 1.75-1.03l3.58-6.49A1.003 1.003 0 0020 4H5.21l-.94-2H1zm16 16c-1.1 0-1.99.9-1.99 2s.89 2 1.99 2 2-.9 2-2-.9-2-2-2z',
  coffee: 'M20 3H4v10c0 2.21 1.79 4 4 4h6c2.21 0 4-1.79 4-4v-3h2c1.11 0 2-.9 2-2V5c0-1.11-.89-2-2-2zm0 5h-2V5h2v3zM4 19h16v2H4z',
  sun: 'M6.76 4.84l-1.8-1.79-1.41 1.41 1.79 1.79 1.42-1.41zM4 10.5H1v2h3v-2zm9-9.95h-2V3.5h2V.55zm7.45 3.91l-1.41-1.41-1.79 1.79 1.41 1.41 1.79-1.79zm-3.21 13.7l1.79 1.8 1.41-1.41-1.8-1.79-1.4 1.4zM20 10.5v2h3v-2h-3zm-8-5c-3.31 0-6 2.69-6 6s2.69 6 6 6 6-2.69 6-6-2.69-6-6-6zm-1 16.95h2V19.5h-2v2.95zm-7.45-3.91l1.41 1.41 1.79-1.8-1.41-1.41-1.79 1.8z',
  moon: 'M12 3c-4.97 0-9 4.03-9 9s4.03 9 9 9 9-4.03 9-9c0-.46-.04-.92-.1-1.36-.98 1.37-2.58 2.26-4.4 2.26-2.98 0-5.4-2.42-5.4-5.4 0-1.81.89-3.42 2.26-4.4-.44-.06-.9-.1-1.36-.1z',
  pin: 'M12 2C8.13 2 5 5.13 5 9c0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7zm0 9.5c-1.38 0-2.5-1.12-2.5-2.5s1.12-2.5 2.5-2.5 2.5 1.12 2.5 2.5-1.12 2.5-2.5 2.5z',
  clock: 'M11.99 2C6.47 2 2 6.48 2 12s4.47 10 9.99 10C17.52 22 22 17.52 22 12S17.52 2 11.99 2zM12 20c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8zm.5-13H11v6l5.25 3.15.75-1.23-4.5-2.67z',
  leaf: 'M6.05 8.05c-2.73 2.73-2.73 7.15-.02 9.88 1.47-3.4 4.09-6.24 7.36-7.93-2.77 2.34-4.71 5.61-5.39 9.32 2.6 1.23 5.8.78 7.95-1.37C19.43 14.47 20 4 20 4S9.53 4.57 6.05 8.05z',
  open: 'M19 19H5V5h7V3H5c-1.11 0-2 .9-2 2v14c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2v-7h-2v7zM14 3v2h3.59l-9.83 9.83 1.41 1.41L19 6.41V10h2V3h-7z',
  chev: 'M10 6L8.59 7.41 13.17 12l-4.58 4.59L10 18l6-6z',
  timer: 'M15 1H9v2h6V1zm-4 13h2V8h-2v6zm8.03-6.61l1.42-1.42c-.43-.51-.9-.99-1.41-1.41l-1.42 1.42C16.07 4.74 14.12 4 12 4c-4.97 0-9 4.03-9 9s4.02 9 9 9 9-4.03 9-9c0-2.12-.74-4.07-1.97-5.61zM12 20c-3.87 0-7-3.13-7-7s3.13-7 7-7 7 3.13 7 7-3.13 7-7 7z',
  list: 'M3 13h2v-2H3v2zm0 4h2v-2H3v2zm0-8h2V7H3v2zm4 4h14v-2H7v2zm0 4h14v-2H7v2zM7 7v2h14V7H7z',
  settings: 'M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58a.49.49 0 00.12-.61l-1.92-3.32a.488.488 0 00-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54a.484.484 0 00-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58a.49.49 0 00-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z',
  check: 'M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z',
  bell: 'M12 22c1.1 0 2-.9 2-2h-4c0 1.1.89 2 2 2zm6-6v-5c0-3.07-1.64-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.63 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z',
  computer: 'M20 18c1.1 0 1.99-.9 1.99-2L22 6c0-1.1-.9-2-2-2H4c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2H0v2h24v-2h-4zM4 6h16v10H4V6z',
  refresh: 'M17.65 6.35C16.2 4.9 14.21 4 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08c-.82 2.33-3.04 4-5.65 4-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z',
};
const icon = (name, cls = '') => `<svg viewBox="0 0 24 24" class="${cls}" aria-hidden="true"><path d="${ICON[name]}"/></svg>`;

function toast(text) {
  const el = document.getElementById('toast');
  el.textContent = text;
  el.style.display = 'block';
  clearTimeout(toast.t);
  toast.t = setTimeout(() => { el.style.display = 'none'; }, 4000);
}

// ---- storage ----
function load() {
  try { return JSON.parse(localStorage.getItem('cm') || '{}'); } catch { return {}; }
}
function store(patch) {
  const next = { ...load(), ...patch };
  try { localStorage.setItem('cm', JSON.stringify(next)); } catch { /* private mode */ }
  return next;
}

async function setLang(l) {
  lang = l === 'ru' ? 'ru' : 'en';
  LOCALE = lang === 'en' ? 'en-GB' : 'ru-RU';
  document.documentElement.lang = lang;
  try { texts = await (await fetch(`locales/${lang}.json`)).json(); } catch { /* keys stay visible */ }
}

// ---- crypto ----
const b64 = (bytes) => btoa(String.fromCharCode(...new Uint8Array(bytes))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
function unb64(s) {
  const t = s.replace(/-/g, '+').replace(/_/g, '/');
  return Uint8Array.from(atob(t + '='.repeat((4 - (t.length % 4)) % 4)), (c) => c.charCodeAt(0));
}
const hex = (bytes) => [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, '0')).join('');
const sha256 = (text) => crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
const normalize = (code) => code.replace(/[^0-9a-z]/gi, '').toUpperCase();
const pairTopic = async (code) => `cm${hex(await sha256(`clockmanage-pair:${normalize(code)}`)).slice(0, 32)}`;
const pairKey = async (code) => sha256(`clockmanage-key:${normalize(code)}`);
const aes = (raw) => crypto.subtle.importKey('raw', raw, 'AES-GCM', false, ['encrypt', 'decrypt']);

async function seal(raw, obj) {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, await aes(raw), new TextEncoder().encode(JSON.stringify(obj)));
  const out = new Uint8Array(12 + ct.byteLength);
  out.set(iv);
  out.set(new Uint8Array(ct), 12);
  return b64(out);
}

async function open(raw, sealed) {
  const b = unb64(sealed);
  const plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: b.slice(0, 12) }, await aes(raw), b.slice(12));
  return JSON.parse(new TextDecoder().decode(plain));
}

// ---- ntfy ----
async function post(topic, sealed) {
  const id = hex(crypto.getRandomValues(new Uint8Array(4)));
  const n = Math.ceil(sealed.length / PART);
  for (let i = 0; i < n; i++) {
    const res = await fetch(`${NTFY}/${topic}`, { method: 'POST', body: `${id}:${i}/${n}:${sealed.slice(i * PART, (i + 1) * PART)}` });
    if (!res.ok) throw new Error(`ntfy ${res.status}`);
  }
}

/** Adds one ntfy message (a part `id:i/n:body`) to `groups`; the whole sealed text once every part is in. */
function addPart(groups, m) {
  if (m?.event !== 'message') return null;
  const [id, pos, ...rest] = m.message.split(':');
  const [i, n] = (pos || '').split('/').map(Number);
  if (!rest.length || !(n > 0) || !(i < n)) return null;
  const k = `${id}/${n}`;
  const g = groups.get(k) || { parts: [], time: 0, n };
  g.parts[i] = rest.join(':');
  g.time = Math.max(g.time, m.time);
  groups.set(k, g);
  if (g.parts.filter((p) => p !== undefined).length < n) return null;
  groups.delete(k);
  return { time: g.time, sealed: g.parts.join('') };
}

/** Whole sealed messages of a topic, oldest first: [{ time, sealed }]. */
async function poll(topic) {
  const res = await fetch(`${NTFY}/${topic}/json?poll=1&since=12h`);
  const groups = new Map();
  const out = [];
  for (const l of (await res.text()).split('\n').filter(Boolean)) {
    let m = null;
    try { m = JSON.parse(l); } catch { /* a broken line */ }
    const whole = addPart(groups, m);
    if (whole) out.push(whole);
  }
  return out.sort((a, b) => a.time - b.time);
}

// ---- the day the PC sent ----
// Every time in it is the PC's clock, projected forward ("nobody presses anything"): the page
// walks the timeline by itself, as the Android app does offline.

/** The PC's clock now. Phones and PCs set their clocks from the net: a small gap is noise of
 *  ntfy's whole seconds and is left out. */
function pcNow() {
  const off = load().day?.offset || 0;
  return Date.now() + (Math.abs(off) < 5000 ? 0 : off);
}

const entryAt = (day, now) => day.timeline.find((e) => e.until == null || now < e.until) || day.timeline[day.timeline.length - 1] || null;
const segEnd = (e) => e.from + e.dur_ms;
function remaining(e, now) {
  if (e.paused) return Math.max(0, e.dur_ms - e.elapsed_ms);
  if (e.until != null) return Math.max(0, e.until - now);
  return e.kind === 'segment' ? segEnd(e) - now : 0;
}
function elapsed(e, now) {
  if (e.paused) return e.elapsed_ms;
  if (e.until != null) return Math.min(e.dur_ms, Math.max(0, now - e.from));
  return Math.max(0, now - e.from);
}
/** Work of block `i` at `now`: the running block grows while its part runs. */
function work(e, i, now) {
  const base = e?.blocks_work_ms?.[i] || 0;
  return e && e.kind === 'work' && e.block === i ? base + elapsed(e, now) : base;
}
const isBreak = (b) => b.kind === 'break';

function mmss(ms) {
  const s = Math.ceil(Math.max(0, ms) / 1000);
  const p = (n) => String(n).padStart(2, '0');
  return s >= 3600 ? `${Math.floor(s / 3600)}:${p(Math.floor(s / 60) % 60)}:${p(s % 60)}` : `${p(Math.floor(s / 60))}:${p(s % 60)}`;
}
function dur(ms) {
  const m = Math.floor((Math.max(0, ms) + 30000) / MIN);
  const h = Math.floor(m / 60);
  const r = m % 60;
  return h === 0 ? tr('ui_min', { r }) : r === 0 ? tr('app_h', { h }) : tr('ui_h_min', { h, r });
}
function hours(min) {
  if (min % 60 === 0) return tr('android_h', { v: min / 60 });
  if (min < 60) return tr('ui_minutes_label', { min });
  return tr('android_h_min', { v: Math.floor(min / 60), v2: min % 60 });
}
const clockAt = (ts) => new Date(ts).toLocaleTimeString(LOCALE, { hour: '2-digit', minute: '2-digit' });

// ---- the PC's colours ----
const ROLES = {
  surface: '--surface', 'on-surface': '--on-surface', 'on-surface-variant': '--muted', 'surface-container': '--card', 'surface-container-high': '--card-high',
  'surface-container-highest': '--card-highest', primary: '--primary', 'on-primary': '--on-primary', 'primary-container': '--container',
  'on-primary-container': '--on-container', 'secondary-container': '--secondary', 'on-secondary-container': '--on-secondary',
  'tertiary-container': '--tertiary', 'on-tertiary-container': '--on-tertiary', error: '--error', outline: '--outline', 'outline-variant': '--outline-variant',
};
const COLOR = /^(#[0-9a-f]{3,8}|rgba?\([\d.,\s%]+\))$/i;

/** The PC's scheme, as the Android app takes it: light, dark, or both following the iPhone. */
function applyTheme(theme) {
  let el = document.getElementById('pc-theme');
  if (!theme?.light || !theme?.dark) { el?.remove(); return; }
  const vars = (roles) => Object.entries(ROLES).filter(([r]) => COLOR.test(String(roles[r] || '').trim())).map(([r, v]) => `${v}: ${roles[r].trim()};`).join(' ');
  const css = theme.mode === 'dark' ? `:root { ${vars(theme.dark)} color-scheme: dark; }`
    : theme.mode === 'light' ? `:root { ${vars(theme.light)} color-scheme: light; }`
      : `:root { ${vars(theme.light)} } @media (prefers-color-scheme: dark) { :root { ${vars(theme.dark)} } }`;
  if (!el) {
    el = document.createElement('style');
    el.id = 'pc-theme';
    document.head.append(el);
  }
  if (el.textContent !== css) el.textContent = css;
}

// ---- the ring ----

/** The timer in a wavy ring, like the Android app's Dial: the part done waves while it runs. */
class Dial {
  constructor(el) {
    this.el = el;
    this.canvas = el.querySelector('canvas');
    this.ctx = this.canvas.getContext('2d');
    this.target = -1;
    this.shown = 0;
    this.amp = 0;
    this.targetAmp = 0;
    this.phase = 0;
    this.last = 0;
    this.arc = '#000';
    this.track = '#000';
    this.calm = matchMedia('(prefers-reduced-motion: reduce)').matches;
  }

  progress(fraction, running) {
    this.target = fraction;
    this.targetAmp = running && fraction >= 0 ? 1 : 0;
  }

  /** The arc's colour and the track's (drawn faint). */
  colors(arc, track) {
    this.arc = arc;
    this.track = track;
  }

  draw(t) {
    const dt = this.last ? Math.min(64, t - this.last) : 16;
    this.last = t;
    const k = this.calm ? 1 : 1 - 0.001 ** (dt / 600);
    const goal = Math.min(1, Math.max(0, this.target));
    this.shown += (goal - this.shown) * (Math.abs(goal - this.shown) > 0.5 ? 1 : k);
    this.amp += ((this.calm ? 0 : this.targetAmp) - this.amp) * k;
    if (!this.calm) this.phase += dt * 0.0022;

    const css = this.canvas.clientWidth;
    if (!css) return;
    const ratio = window.devicePixelRatio || 1;
    if (this.canvas.width !== Math.round(css * ratio)) {
      this.canvas.width = Math.round(css * ratio);
      this.canvas.height = Math.round(css * ratio);
    }
    const c = this.ctx;
    const size = this.canvas.width;
    const cx = size / 2;
    const stroke = size * 0.035;
    const r = size / 2 - stroke;
    c.clearRect(0, 0, size, size);
    c.lineWidth = stroke;
    c.lineCap = 'round';
    c.lineJoin = 'round';

    // The track: flat, after the part done, with a gap on both sides.
    const p = this.target < 0 ? 0 : this.shown;
    const gap = p > 0.001 && p < 0.999 ? 0.09 : 0;
    const a0 = p * 2 * Math.PI + gap;
    const a1 = 2 * Math.PI - gap;
    if (a1 - a0 > 0.01) {
      c.strokeStyle = this.track;
      c.globalAlpha = 0.14;
      c.beginPath();
      c.arc(cx, cx, r, a0 - Math.PI / 2, a1 - Math.PI / 2);
      c.stroke();
      c.globalAlpha = 1;
    }
    // The part done: a wave tapered at both ends, so the round caps stay on the track.
    if (p > 0.001) {
      const end = p * 2 * Math.PI;
      const steps = Math.max(8, Math.ceil(end / (Math.PI / 90)));
      const waveH = (r * 1.9 / 88) * this.amp;
      c.strokeStyle = this.arc;
      c.beginPath();
      for (let i = 0; i <= steps; i++) {
        const a = (i / steps) * end;
        const taper = Math.min(1, a / 0.35, (end - a) / 0.35);
        const rr = r + waveH * taper * Math.sin(a * 22 - this.phase * 6);
        const x = cx + rr * Math.sin(a);
        const y = cx - rr * Math.cos(a);
        if (i === 0) c.moveTo(x, y); else c.lineTo(x, y);
      }
      c.stroke();
    }
  }
}

// ---- pages ----
const standalone = () => navigator.standalone === true || matchMedia('(display-mode: standalone)').matches;
const kindName = (kind) => ({ breakfast: tr('meals_breakfast'), lunch: tr('meals_lunch'), dinner: tr('meals_dinner') }[kind] || tr('meals_food_picks'));
const KIND_ICON = { breakfast: 'coffee', lunch: 'sun', dinner: 'moon' };
const presets = () => [
  ['brain', tr('meals_max_for_the_brain'), tr('meals_goal_a_clear_steady_mind')],
  ['protein', tr('meals_brain_protein'), tr('meals_goal_plenty_of_protein_for')],
  ['light', tr('meals_brain_light'), tr('meals_goal_lose_weight_slowly_while')],
];

/** The pages under the bar: Meals only while Meals is on on the PC. A PC that sends no day (older
 *  than the pages) paired the iPhone from Meals, so it has Meals on. */
function tabs(s) {
  const meals = s.day ? s.day.meals === true : true;
  return [
    ['today', 'timer', tr('main_today')],
    ['plan', 'list', tr('main_plan')],
    ...(meals ? [['meals', 'restaurant', tr('main_meals')]] : []),
    ['settings', 'settings', tr('main_settings')],
  ];
}

let shell = null; // { ids, pages: { id: element }, dial, track }
let tab = load().tab || 'today';
let picksNow = null;

/** Builds the bar and the pages (again when the tabs or the language change). */
function build(s) {
  const list = tabs(s);
  const ids = list.map(([id]) => id);
  if (!ids.includes(tab)) tab = 'today';
  app.innerHTML = `<div id="pages">${ids.map((id) => `<section class="page" data-page="${id}"></section>`).join('')}</div>`;
  nav.innerHTML = list.map(([id, ic, label]) => `<button data-tab="${id}" aria-label="${esc(label)}"><span class="pill">${icon(ic)}</span><span>${esc(label)}</span></button>`).join('');
  nav.hidden = false;
  const pages = Object.fromEntries(ids.map((id) => [id, app.querySelector(`[data-page="${id}"]`)]));
  pages.today.innerHTML = `<h1>${tr('main_today')}</h1><div class="status" id="status"><i></i><span></span></div>
    <div class="hero" id="hero"><div class="dial" id="dial"><canvas></canvas><div class="face"><div class="state"></div><div class="time"></div></div></div>
      <div class="title"></div><div class="sub"></div></div>`;
  shell = { ids, key: `${ids.join()}|${lang}`, pages, dial: new Dial(pages.today.querySelector('#dial')), track: app.querySelector('#pages'), shownKind: null, planKey: '' };
  nav.querySelectorAll('[data-tab]').forEach((b) => { b.onclick = () => select(b.dataset.tab); });
  swipe(shell.track);
  select(tab, true);
  renderAll(s);
}

function select(id, first = false) {
  const changed = id !== tab;
  tab = id;
  store({ tab });
  const i = shell.ids.indexOf(id);
  shell.track.style.transform = `translateX(${-i * 100}%)`;
  nav.querySelectorAll('[data-tab]').forEach((b) => {
    const on = b.dataset.tab === id;
    b.classList.toggle('on', on);
    b.setAttribute('aria-current', on ? 'page' : 'false');
    if (on && changed && !first) {
      b.classList.remove('grow');
      void b.offsetWidth; // restart the animation
      b.classList.add('grow');
    }
  });
}

/** A horizontal swipe moves to the next or the previous page; scrolling up and down stays the page's. */
function swipe(track) {
  let x0 = 0, y0 = 0, t0 = 0, dx = 0, way = null;
  track.addEventListener('touchstart', (e) => {
    if (e.touches.length !== 1) return;
    x0 = e.touches[0].clientX; y0 = e.touches[0].clientY; t0 = Date.now(); dx = 0; way = null;
  }, { passive: true });
  track.addEventListener('touchmove', (e) => {
    const ddx = e.touches[0].clientX - x0;
    const ddy = e.touches[0].clientY - y0;
    if (!way && Math.hypot(ddx, ddy) > 10) way = Math.abs(ddx) > Math.abs(ddy) * 1.2 && !e.target.closest('textarea, input') ? 'x' : 'y';
    if (way !== 'x') return;
    const i = shell.ids.indexOf(tab);
    // Past the first or the last page it only gives a little.
    dx = (i === 0 && ddx > 0) || (i === shell.ids.length - 1 && ddx < 0) ? ddx / 4 : ddx;
    track.classList.add('drag');
    track.style.transform = `translateX(calc(${-i * 100}% + ${dx}px))`;
  }, { passive: true });
  const end = () => {
    if (way !== 'x') return;
    way = null;
    track.classList.remove('drag');
    const i = shell.ids.indexOf(tab);
    const fast = Math.abs(dx) / Math.max(1, Date.now() - t0) > 0.5;
    const go = Math.abs(dx) > track.clientWidth * 0.25 || (fast && Math.abs(dx) > 30) ? (dx < 0 ? 1 : -1) : 0;
    const next = shell.ids[Math.min(shell.ids.length - 1, Math.max(0, i + go))];
    if (next !== tab) select(next); else select(tab, true);
  };
  track.addEventListener('touchend', end);
  track.addEventListener('touchcancel', end);
}

function renderAll(s) {
  applyTheme(s.day?.theme);
  shell.shownKind = null;
  shell.planKey = '';
  tick(s);
  renderMeals(s);
  renderSettings(s);
}

// Today and Plan move every second; the rest only when something comes.
function tick(s = load()) {
  if (!shell) return;
  renderToday(s);
  renderPlan(s);
}

function statusLine(s) {
  const el = shell.pages.today.querySelector('#status');
  const at = s.day?.at;
  el.classList.toggle('old', !at || Date.now() - at > 11 * 3600000);
  el.querySelector('span').textContent = at ? tr('pwa_updated_at', { time: clockAt(at), pc: s.day.pc || s.pc || '' }) : tr('pwa_connected_to', { pc: s.pc || '' });
}

/** The hero's colour by the phase, as on Android: waiting — primary, a break or a segment — tertiary, done — secondary. */
function paintHero(hero, e) {
  const cls = { await: 'await', break: 'rest', lunch_break: 'rest', segment: 'rest', done: 'done' }[e?.kind] || '';
  hero.className = `hero ${cls}`;
  // The colours the card is going to (its own colour is still on the way while it fades).
  const css = getComputedStyle(hero);
  const fg = css.getPropertyValue('--fg').trim();
  const arc = e?.paused ? css.getPropertyValue('--outline').trim() : cls ? fg : css.getPropertyValue('--primary').trim();
  shell.dial.colors(arc, fg);
}

function renderToday(s) {
  const page = shell.pages.today;
  const hero = page.querySelector('#hero');
  statusLine(s);
  const day = s.day?.day;
  const face = page.querySelector('.face');
  const [stateEl, timeEl] = [face.querySelector('.state'), face.querySelector('.time')];
  if (!day) {
    if (shell.shownKind !== 'none') { paintHero(hero, null); shell.shownKind = 'none'; }
    stateEl.textContent = '';
    timeEl.textContent = '';
    shell.dial.progress(-1, false);
    hero.querySelector('.title').textContent = tr('android_main_waiting_for_data_from_the_pc');
    hero.querySelector('.sub').textContent = tr('pwa_the_timer_and_the_plan_show');
    return;
  }
  const now = pcNow();
  const e = entryAt(day, now);
  const kind = `${e?.kind}${e?.paused}`;
  if (kind !== shell.shownKind) {
    paintHero(hero, e);
    if (shell.shownKind && shell.shownKind !== 'none') {
      shell.dial.el.classList.remove('spring');
      void shell.dial.el.offsetWidth;
      shell.dial.el.classList.add('spring');
    }
    shell.shownKind = kind;
  }
  let fraction = -1, running = false, scale = 1, over = false, state, time;
  switch (e?.kind) {
    case 'work': case 'break': case 'lunch_break':
      state = e.paused ? tr('today_paused') : e.kind === 'work' ? tr('log_work') : tr('log_break');
      time = mmss(remaining(e, now));
      fraction = elapsed(e, now) / Math.max(1, e.dur_ms);
      running = !e.paused;
      break;
    case 'await': state = tr('pwa_waiting_for_you'); time = mmss(elapsed(e, now)); scale = 0.8; break;
    case 'segment': {
      const left = remaining(e, now);
      if (e.stopwatch) {
        state = left < 0 ? tr('today_untimed_longer_than_usual') : tr('mini_untimed');
        time = mmss(elapsed(e, now));
      } else {
        state = e.prep ? (left < 0 ? tr('mini_time_to_lie_down') : tr('mini_getting_ready')) : left < 0 ? tr('mini_over') : e.alarm ? tr('today_nap_alarm') : tr('today_segment');
        time = left < 0 ? `+${mmss(-left)}` : mmss(left);
        over = left < 0;
        if (!e.prep) {
          fraction = (now - e.from) / Math.max(1, e.dur_ms);
          running = !over;
        }
      }
      break;
    }
    case 'lunch': state = tr('log_lunch'); time = mmss(elapsed(e, now)); scale = 0.8; break;
    case 'done': state = tr('today_done'); time = '✓'; fraction = 1; scale = 0.9; break;
    default:
      state = tr('today_planned');
      time = dur(day.blocks.filter((b) => !isBreak(b)).reduce((n, b) => n + b.minutes, 0) * MIN);
      scale = 0.5;
  }
  stateEl.textContent = state;
  timeEl.textContent = time;
  // An hour and more ("1:05:00") takes a smaller size so it stays inside the ring.
  const w = shell.dial.el.clientWidth || 300;
  timeEl.style.fontSize = `${w * 0.22 * (time.length > 6 ? scale * 0.78 : scale)}px`;
  timeEl.classList.toggle('over', over);
  shell.dial.progress(fraction, running);
  hero.querySelector('.title').textContent = e?.title || '';
  hero.querySelector('.sub').textContent = e?.subtitle || '';
}

/** A plan row's mark, as on the PC: done ✓, current ●, a segment ☕, still to do ○. */
function mark(b) {
  if (b.done) return `<span class="mark ok">${icon('check')}</span>`;
  if (isBreak(b)) return `<span class="mark">${icon('coffee')}</span>`;
  return '<span class="mark"><i></i></span>';
}

function renderPlan(s) {
  const page = shell.pages.plan;
  const day = s.day?.day;
  if (!day) {
    if (shell.planKey !== 'none') {
      shell.planKey = 'none';
      page.innerHTML = `<h1>${tr('main_day_plan')}</h1><div class="note">${esc(tr('pwa_the_timer_and_the_plan_show'))}</div>`;
    }
    return;
  }
  const now = pcNow();
  const e = entryAt(day, now);
  const current = e?.block ?? -1;
  const key = `${day.version}|${current}|${lang}`;
  const studyMin = day.blocks.filter((b) => !isBreak(b)).reduce((n, b) => n + b.minutes, 0);
  if (shell.planKey !== key) {
    shell.planKey = key;
    const rows = day.blocks.map((b, i) => {
      const nowRow = i === current && !b.done;
      return `<div class="rowi${nowRow ? ' now' : ''}${isBreak(b) ? ' seg' : ''}" data-i="${i}">${mark(b)}
        <div class="grow"><div class="name">${esc(b.name)}</div><div class="meta"></div>${isBreak(b) ? '' : '<div class="bar"><i></i></div>'}</div></div>`;
    }).join('');
    page.innerHTML = `<h1>${tr('main_day_plan')}</h1>
      ${studyMin > 0 ? '<div class="total"></div><div class="bar"><i></i></div>' : ''}
      <div class="rows">${rows || `<div class="empty">${esc(tr('pwa_todays_plan_is_empty_make'))}</div>`}</div>`;
  }
  const total = page.querySelector('.total');
  if (total) {
    const done = day.blocks.reduce((n, _, i) => n + work(e, i, now), 0);
    total.textContent = tr('android_main_of_of_study', { work: dur(done), study_min: hours(studyMin) });
    page.querySelector('.total + .bar i').style.width = `${Math.min(100, (done / Math.max(1, studyMin * MIN)) * 100)}%`;
  }
  page.querySelectorAll('.rowi').forEach((row) => {
    const i = Number(row.dataset.i);
    const b = day.blocks[i];
    const meta = row.querySelector('.meta');
    if (isBreak(b)) {
      meta.textContent = tr('android_main_min', { minutes: b.minutes }) + (b.done ? tr('plan_done') : b.started ? tr('stats_running') : tr('android_main_segment_not_study'));
      return;
    }
    const w = work(e, i, now);
    meta.textContent = tr('android_main_of_min', { w: Math.floor(w / MIN), minutes: b.minutes }) + (b.done ? tr('plan_suffix_done') : '');
    row.querySelector('.bar i').style.width = `${Math.min(100, (w / Math.max(1, b.minutes * MIN)) * 100)}%`;
  });
}

// ---- Meals ----

/** A price in minor units of its currency (ISO 4217: cents, kopecks; yen have none), the same
 *  as `money` in ui/js/views/meals.js: 1390 EUR → 13,90 €, 54900 RUB → 549 ₽, 980 JPY → ¥980. */
function money(minor, currency) {
  const plain = () => (minor / 100).toLocaleString(LOCALE) + (currency ? ` ${currency}` : '');
  if (!currency) return plain();
  for (const currencyDisplay of ['narrowSymbol', 'symbol']) {
    try {
      const digits = new Intl.NumberFormat('en', { style: 'currency', currency }).resolvedOptions().maximumFractionDigits;
      const v = minor / 10 ** digits;
      const shown = Number.isInteger(v) ? 0 : digits;
      return new Intl.NumberFormat(LOCALE, { style: 'currency', currency, currencyDisplay, minimumFractionDigits: shown, maximumFractionDigits: shown }).format(v);
    } catch { /* an old Safari without narrowSymbol, or an unknown code */ }
  }
  return plain();
}

async function cachedPicks() {
  try {
    const r = await (await caches.open('cm')).match('picks.json');
    return r ? r.json() : null;
  } catch { return null; }
}

async function keepPicks(p) {
  try {
    await (await caches.open('cm')).put('picks.json', new Response(JSON.stringify(p), { headers: { 'Content-Type': 'application/json' } }));
  } catch { /* storage may be unavailable */ }
}

function venueHtml(v, service) {
  const sum = v.dishes.reduce((n, d) => n + (d.price || 0), 0);
  return `<article class="venue">
      ${v.image ? `<div class="banner" style="background-image:url('${esc(v.image)}')"></div>` : ''}
      <div class="vhead"><div class="grow"><div class="vname">${esc(v.name)}</div>
        <div class="meta-pills"><span class="pill2">${icon('pin')}${esc(tr('meals_km_away', { km: Number(v.km).toLocaleString(LOCALE) }))}</span>${v.eta ? `<span class="pill2">${icon('clock')}${esc(tr('meals_eta_min', { eta: v.eta }))}</span>` : ''}</div></div>
        <a class="open" href="${esc(v.url)}" aria-label="${esc(tr('meals_open_in_service', { service }))}">${esc(service)}${icon('open')}</a></div>
      ${v.dishes.map((d) => `<a class="item" href="${esc(d.url)}">${d.image ? `<img class="thumb" src="${esc(d.image)}" alt="" loading="lazy">` : ''}<div class="grow"><div class="iname">${esc(d.name)}</div>${d.why ? `<div class="why">${icon('leaf')}<span>${esc(d.why)}</span></div>` : ''}</div>
        <span class="price">${esc(money(d.price, d.currency))}</span>${icon('chev', 'chev')}</a>`).join('')}
      ${v.dishes.length > 1 ? `<div class="sum"><span>${tr('meals_total')}</span><b>${esc(money(sum, v.dishes[0].currency))}</b></div>` : ''}
    </article>`;
}

function picksHtml(p) {
  if (!p?.venues?.length) return `<h2>${tr('meals_picks')}</h2><div class="card muted">${tr('pwa_no_picks_yet_they_come_before')}</div>`;
  const time = clockAt(p.at);
  const title = p.meal ? tr('meals_kind_at', { kind: kindName(p.kind), time: p.meal }) : kindName(p.kind);
  const food = p.venues.filter((v) => !v.store);
  const shops = p.venues.filter((v) => v.store);
  // Picks made before the PC named their service are all from Wolt (a brand name: not translated).
  const service = p.service || 'Wolt';
  return `<div class="picks-head"><h2>${esc(title)}</h2><span class="muted">${esc(time)}</span></div>
    <p class="muted" style="margin:0 4px">${esc(tr('meals_picked_at_tap_a_dish', { time, service }))}</p>
    ${food.length ? `<h3>${icon('restaurant')}${tr('meals_restaurants')}</h3>${food.map((v) => venueHtml(v, service)).join('')}` : ''}
    ${shops.length ? `<h3>${icon('cart')}${tr('meals_from_stores')}</h3>${shops.map((v) => venueHtml(v, service)).join('')}` : ''}`;
}

/** Add to the Home Screen first, then allow notifications: the picks and the PC's news come as pushes. */
function notifyStep(s, why) {
  if (!standalone()) return `<div class="card"><p><b>${tr('pwa_add_to_the_home_screen')}</b></p><p class="muted">${tr('pwa_notifications_only_work_from_the')}</p></div>`;
  if (!s.subscribed || Notification.permission !== 'granted') return `<div class="card"><p>${why}</p><div class="row"><button class="btn" data-allow>${icon('bell')}${tr('pwa_allow_notifications')}</button></div></div>`;
  return '';
}

function renderMeals(s) {
  const page = shell.pages.meals;
  if (!page) return;
  // Don't redraw under the user's fingers while they edit the diet.
  if (page.querySelector('details[open]')) return;
  const current = picksNow;
  const pending = s.pending && Date.now() - s.pending.at < 10 * MIN && !(current && current.at > s.pending.at) ? s.pending : null;
  const diet = s.diet?.text || '';
  const preset = presets().find(([, , text]) => text === diet);
  page.innerHTML = `<h1>${tr('main_meals')}</h1>${notifyStep(s, tr('pwa_meal_picks_from_the_pc_come'))}
    <h2>${tr('pwa_order_now')}</h2>
    <div class="meals">${['breakfast', 'lunch', 'dinner'].map((k) => `<button class="meal ${pending?.kind === k ? 'on' : ''}" data-kind="${k}">${icon(KIND_ICON[k])}${esc(kindName(k))}</button>`).join('')}</div>
    <p class="muted hint">${pending ? esc(tr('pwa_picking_a_notification_comes_in', { meal: kindName(pending.kind) })) : tr('pwa_the_pc_picks_restaurants_and')}</p>
    ${picksHtml(current)}
    <h2>${tr('meals_diet')}</h2>
    <details class="card"><summary><span>${preset ? esc(preset[1]) : tr('pwa_your_own_diet')}</span>${icon('chev', 'chev')}</summary>
      <div class="chips">${presets().map(([id, name, text]) => `<button class="chip ${text === diet ? 'on' : ''}" data-preset="${id}">${esc(name)}</button>`).join('')}</div>
      <textarea id="diet" rows="10">${esc(diet)}</textarea>
      <div class="row"><button class="btn" id="save">${tr('meals_save_the_diet')}</button></div></details>`;
  page.querySelectorAll('[data-kind]').forEach((b) => { b.onclick = async () => {
    try {
      await post(s.link.up, await seal(unb64(s.link.key), { t: 'pick', kind: b.dataset.kind }));
      renderMeals(store({ pending: { kind: b.dataset.kind, at: Date.now() } }));
    } catch (e) { toast(tr('pwa_couldnt_send', { error: e.message })); }
  }; });
  page.querySelectorAll('[data-preset]').forEach((b) => { b.onclick = () => {
    page.querySelector('#diet').value = presets().find(([id]) => id === b.dataset.preset)[2];
    page.querySelectorAll('[data-preset]').forEach((c) => c.classList.toggle('on', c === b));
    toast(tr('meals_press_save_the_diet_to_keep'));
  }; });
  page.querySelector('#save').onclick = async () => {
    const text = page.querySelector('#diet').value.trim();
    try {
      await post(s.link.up, await seal(unb64(s.link.key), { t: 'diet', text, at: Date.now() }));
      store({ diet: { text, at: Date.now() } });
      toast(tr('pwa_sent_to_the_pc_it_applies'));
    } catch (e) { toast(tr('pwa_couldnt_send', { error: e.message })); }
  };
  page.querySelector('[data-allow]')?.addEventListener('click', subscribe);
}

// ---- Settings ----

function renderSettings(s) {
  const page = shell.pages.settings;
  const on = standalone() && s.subscribed && window.Notification?.permission === 'granted';
  const updated = s.day?.at ? tr('pwa_updated_at', { time: clockAt(s.day.at), pc: s.day.pc || s.pc || '' }) : tr('android_main_waiting_for_data_from_the_pc');
  page.innerHTML = `<h1>${tr('main_settings')}</h1>
    <div class="card">
      <div class="setting"><span class="badge">${icon('computer')}</span>
        <div class="grow"><div class="t">${esc(tr('pwa_connected_to', { pc: s.pc || '' }))}</div><div class="muted">${esc(updated)}</div></div></div>
      <div class="setting"><span class="badge">${icon('bell')}</span>
        <div class="grow"><div class="t">${tr('pwa_notifications')}</div>${on ? '' : `<div class="muted">${standalone() ? tr('pwa_meal_picks_from_the_pc_come') : tr('pwa_notifications_only_work_from_the')}</div>`}</div>
        ${on ? `<span class="chipon">${tr('pwa_notifications_are_on')}</span>` : standalone() ? `<button class="btn tonal" data-allow>${tr('pwa_allow_notifications')}</button>` : ''}</div>
    </div>
    <div class="row"><button class="btn text" id="repair">${icon('refresh')}${tr('meals_pair_again')}</button></div>`;
  page.querySelector('[data-allow]')?.addEventListener('click', subscribe);
  page.querySelector('#repair').onclick = () => {
    live?.close();
    localStorage.removeItem('cm');
    shell = null;
    pairScreen();
  };
}

// ---- pairing ----

function pairScreen(code = '', error = '') {
  nav.hidden = true;
  app.innerHTML = `<div class="single"><div class="brand"><img src="apple-touch-icon.png" alt=""><h1>ClockManage</h1></div>
    <h2>${tr('pwa_connect_to_clockmanage')}</h2>
    <div class="card"><p>${tr('pwa_on_the_pc_open_clockmanage_settings')}</p>
      <input id="code" class="code" value="${esc(code)}" placeholder="ABCD-EFGH" autocomplete="off" autocapitalize="characters" aria-label="${tr('pwa_pairing_code')}">
      <div class="row"><button class="btn" id="connect">${tr('pwa_connect')}</button></div>
      ${error ? `<p class="error">${esc(error)}</p>` : ''}</div></div>`;
  document.getElementById('connect').onclick = () => connect(document.getElementById('code').value);
}

async function connect(code) {
  if (!normalize(code)) return;
  app.querySelector('#connect').textContent = tr('pwa_waiting_for_the_pc');
  try {
    const topic = await pairTopic(code);
    const key = await pairKey(code);
    for (let tries = 0; tries < 20; tries++) {
      for (const m of (await poll(topic)).reverse()) {
        try {
          const link = await open(key, m.sealed);
          store({ link: { up: link.up, down: link.down, key: link.key }, vapid: link.vapid, pc: link.pc, subscribed: false });
          return render();
        } catch { /* someone else's message */ }
      }
      await new Promise((r) => setTimeout(r, 3000));
    }
    pairScreen(code, tr('pwa_the_code_didnt_work_check_it'));
  } catch (e) {
    pairScreen(code, tr('pwa_couldnt_send', { error: e.message }));
  }
}

async function subscribe() {
  // The permission prompt must come straight from the tap.
  const perm = await Notification.requestPermission();
  if (perm !== 'granted') return toast(tr('pwa_notifications_are_off_turn_them'));
  try {
    const s = load();
    const reg = await navigator.serviceWorker.ready;
    const sub = (await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: unb64(s.vapid) })).toJSON();
    await post(s.link.up, await seal(unb64(s.link.key), { t: 'sub', sub: { endpoint: sub.endpoint, p256dh: sub.keys.p256dh, auth: sub.keys.auth } }));
    const next = store({ subscribed: true });
    renderMeals(next);
    renderSettings(next);
  } catch (e) {
    toast(tr('pwa_couldnt_send', { error: e.message }));
  }
}

// ---- what the PC sends ----

/** Applies one message from the PC (newer ones win); true when something to show changed. */
async function take(msg, time) {
  const s = load();
  const at = time * 1000;
  if (msg.t === 'day' && msg.day?.timeline && at >= (s.day?.at || 0)) {
    store({ day: { day: msg.day, meals: msg.meals === true, pc: msg.pc || s.pc, theme: msg.theme || null, at, offset: msg.day.server_now - at } });
    if (msg.day.lang && msg.day.lang !== lang) {
      await setLang(msg.day.lang);
      store({ lang });
    }
    return true;
  }
  if (msg.t === 'diet' && msg.at > (s.diet?.at || 0)) {
    store({ diet: { text: msg.text, at: msg.at } });
    return true;
  }
  // A push holds 4 KB: the relay has the whole picks of the same run.
  if (msg.t === 'picks' && msg.picks && msg.picks.at >= (picksNow?.at || 0) && JSON.stringify(msg.picks) !== JSON.stringify(picksNow)) {
    picksNow = msg.picks;
    await keepPicks(msg.picks);
    return true;
  }
  return false;
}

/** Shows what changed: the pages are built anew when the tabs or the language changed. */
function refresh() {
  const s = load();
  if (!s.link) return;
  if (!shell || shell.key !== `${tabs(s).map(([id]) => id).join()}|${lang}`) build(s);
  else renderAll(s);
}

/** Everything the PC sent in the last 12 hours (ntfy keeps that long). */
async function catchUp() {
  const s = load();
  if (!s.link) return;
  let changed = false;
  try {
    const key = unb64(s.link.key);
    for (const m of await poll(s.link.down)) {
      try { changed = (await take(await open(key, m.sealed), m.time)) || changed; } catch { /* not ours */ }
    }
  } catch { /* offline: keep the local copies */ }
  if (changed) refresh();
}

/** While the page is open new messages come at once. */
let live = null;
function listen() {
  const s = load();
  live?.close();
  live = null;
  if (!s.link || document.hidden || !window.EventSource) return;
  const key = unb64(s.link.key);
  const groups = new Map();
  live = new EventSource(`${NTFY}/${s.link.down}/sse`);
  live.onmessage = async (ev) => {
    let m = null;
    try { m = JSON.parse(ev.data); } catch { return; }
    const whole = addPart(groups, m);
    if (!whole) return;
    try { if (await take(await open(key, whole.sealed), whole.time)) refresh(); } catch { /* not ours */ }
  };
}

// The ring waves and the time runs while Today shows.
let lastTick = 0;
function frame(t) {
  if (shell && !document.hidden) {
    if (tab === 'today') shell.dial.draw(t);
    if (t - lastTick > 500) { lastTick = t; tick(); }
  }
  requestAnimationFrame(frame);
}

async function render() {
  const s = load();
  const code = new URLSearchParams(location.search).get('pair');
  if (!s.link) {
    pairScreen(code || '');
    if (code) connect(code);
    return;
  }
  picksNow = await cachedPicks();
  refresh();
  catchUp();
  listen();
}

(async () => {
  await setLang(lang);
  if ('serviceWorker' in navigator) {
    navigator.serviceWorker.register('sw.js').catch(() => {});
    // Any push (picks or a failure) ends the wait for a pick.
    navigator.serviceWorker.addEventListener('message', async () => {
      if (!load().link) return;
      store({ pending: null });
      picksNow = await cachedPicks();
      refresh();
    });
  }
  document.addEventListener('visibilitychange', () => {
    if (!load().link) return;
    if (document.hidden) { live?.close(); live = null; return; }
    catchUp();
    listen();
  });
  render();
  requestAnimationFrame(frame);
})();
