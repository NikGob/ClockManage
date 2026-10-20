// The iPhone side of ClockManage meals: pair with the PC through ntfy.sh, take pushes, show the
// picks, ask for a pick, edit the diet. Same sealing as core/src/relay.rs: AES-256-GCM,
// base64url(nonce ‖ ct).
const NTFY = 'https://ntfy.sh';
const PART = 3000;
const lang = /^(ru|uk|be)\b/i.test(navigator.language || '') ? 'ru' : 'en';
const LOCALE = lang === 'en' ? 'en-GB' : 'ru-RU';
let texts = {};

function tr(key, args) {
  let s = texts[key] ?? key;
  for (const [k, v] of Object.entries(args || {})) s = s.split(`{${k}}`).join(String(v));
  return s;
}

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const app = document.getElementById('app');

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

/** Whole sealed messages of a topic, oldest first: [{ time, sealed }]. */
async function poll(topic) {
  const res = await fetch(`${NTFY}/${topic}/json?poll=1&since=12h`);
  const lines = (await res.text()).split('\n').filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return null; } });
  const groups = new Map();
  for (const m of lines) {
    if (m?.event !== 'message') continue;
    const [id, pos, ...rest] = m.message.split(':');
    const [i, n] = (pos || '').split('/').map(Number);
    if (!rest.length || !(n > 0) || !(i < n)) continue;
    const g = groups.get(`${id}/${n}`) || { parts: [], time: 0, n };
    g.parts[i] = rest.join(':');
    g.time = Math.max(g.time, m.time);
    groups.set(`${id}/${n}`, g);
  }
  return [...groups.values()].filter((g) => g.parts.filter((p) => p !== undefined).length === g.n)
    .map((g) => ({ time: g.time, sealed: g.parts.join('') })).sort((a, b) => a.time - b.time);
}

// ---- texts built at render time ----
const kindName = (kind) => ({ breakfast: tr('meals_breakfast'), lunch: tr('meals_lunch'), dinner: tr('meals_dinner') }[kind] || tr('meals_food_picks'));
const KIND_ICON = { breakfast: 'coffee', lunch: 'sun', dinner: 'moon' };
const presets = () => [
  ['brain', tr('meals_max_for_the_brain'), tr('meals_goal_a_clear_steady_mind')],
  ['protein', tr('meals_brain_protein'), tr('meals_goal_plenty_of_protein_for')],
  ['light', tr('meals_brain_light'), tr('meals_goal_lose_weight_slowly_while')],
];

function money(cents, currency) {
  const whole = cents % 100 === 0;
  try {
    return new Intl.NumberFormat(LOCALE, { style: 'currency', currency: currency || 'EUR', currencyDisplay: 'narrowSymbol', minimumFractionDigits: whole ? 0 : 2, maximumFractionDigits: whole ? 0 : 2 }).format(cents / 100);
  } catch { return (cents / 100).toLocaleString(LOCALE); }
}

// ---- screens ----
const standalone = () => navigator.standalone === true || matchMedia('(display-mode: standalone)').matches;

function showStatus(pc) {
  const el = document.getElementById('status');
  el.hidden = !pc;
  el.querySelector('span').textContent = pc ? tr('pwa_connected_to', { pc }) : '';
}

function pairScreen(code = '', error = '') {
  showStatus('');
  app.innerHTML = `<h2>${tr('pwa_connect_to_clockmanage')}</h2>
    <div class="card"><p>${tr('pwa_on_the_pc_open_clockmanage_meals')}</p>
      <input id="code" class="code" value="${esc(code)}" placeholder="ABCD-EFGH" autocomplete="off" autocapitalize="characters" aria-label="${tr('pwa_pairing_code')}">
      <div class="row"><button class="btn" id="connect">${tr('pwa_connect')}</button></div>
      ${error ? `<p class="error">${esc(error)}</p>` : ''}</div>`;
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
    store({ subscribed: true });
    render();
  } catch (e) {
    toast(tr('pwa_couldnt_send', { error: e.message }));
  }
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

function venueHtml(v) {
  const sum = v.dishes.reduce((n, d) => n + (d.price || 0), 0);
  return `<article class="venue">
      <div class="vhead"><div class="grow"><div class="vname">${esc(v.name)}</div>
        <div class="meta"><span class="pill">${icon('pin')}${esc(tr('meals_km_away', { km: Number(v.km).toLocaleString(LOCALE) }))}</span>${v.eta ? `<span class="pill">${icon('clock')}${esc(tr('meals_eta_min', { eta: v.eta }))}</span>` : ''}</div></div>
        <a class="open" href="${esc(v.url)}" aria-label="${tr('meals_open_in_wolt')}">Wolt${icon('open')}</a></div>
      ${v.dishes.map((d) => `<a class="item" href="${esc(d.url)}"><div class="grow"><div class="iname">${esc(d.name)}</div>${d.why ? `<div class="why">${icon('leaf')}<span>${esc(d.why)}</span></div>` : ''}</div>
        <span class="price">${esc(money(d.price, d.currency))}</span>${icon('chev', 'chev')}</a>`).join('')}
      ${v.dishes.length > 1 ? `<div class="total"><span>${tr('meals_total')}</span><b>${esc(money(sum, v.dishes[0].currency))}</b></div>` : ''}
    </article>`;
}

function picksHtml(p) {
  if (!p?.venues?.length) return `<h2>${tr('meals_picks')}</h2><div class="card muted">${tr('pwa_no_picks_yet_they_come_before')}</div>`;
  const time = new Date(p.at).toLocaleTimeString(LOCALE, { hour: '2-digit', minute: '2-digit' });
  const title = p.meal ? tr('meals_kind_at', { kind: kindName(p.kind), time: p.meal }) : kindName(p.kind);
  const food = p.venues.filter((v) => !v.store);
  const shops = p.venues.filter((v) => v.store);
  return `<div class="picks-head"><h2>${esc(title)}</h2><span class="muted">${esc(time)}</span></div>
    <p class="muted" style="margin:0 4px">${tr('meals_picked_at_tap_a_dish', { time })}</p>
    ${food.length ? `<h3>${icon('restaurant')}${tr('meals_restaurants')}</h3>${food.map(venueHtml).join('')}` : ''}
    ${shops.length ? `<h3>${icon('cart')}${tr('meals_from_stores')}</h3>${shops.map(venueHtml).join('')}` : ''}`;
}

/** The newest diet and picks the PC sent through the relay. */
async function fromPc(s) {
  const out = {};
  try {
    const key = unb64(s.link.key);
    for (const m of (await poll(s.link.down)).reverse()) {
      try {
        const msg = await open(key, m.sealed);
        if (msg.t === 'diet' && !out.diet) out.diet = msg;
        if (msg.t === 'picks' && !out.picks) out.picks = msg.picks;
      } catch { /* not ours */ }
    }
  } catch { /* offline: keep the local copies */ }
  return out;
}

async function mainScreen(s, refresh = true) {
  showStatus(s.pc || '');
  const step = !standalone()
    ? `<div class="card"><p><b>${tr('pwa_add_to_the_home_screen')}</b></p><p>${tr('pwa_notifications_only_work_from_the')}</p></div>`
    : !s.subscribed || Notification.permission !== 'granted'
      ? `<div class="card"><p>${tr('pwa_meal_picks_from_the_pc_come')}</p><div class="row"><button class="btn" id="allow">${tr('pwa_allow_notifications')}</button></div></div>`
      : '';
  const current = await cachedPicks();
  const pending = s.pending && Date.now() - s.pending.at < 10 * 60000 && !(current && current.at > s.pending.at) ? s.pending : null;
  const diet = s.diet?.text || '';
  const preset = presets().find(([, , text]) => text === diet);
  app.innerHTML = `${step}
    <h2>${tr('pwa_order_now')}</h2>
    <div class="meals">${['breakfast', 'lunch', 'dinner'].map((k) => `<button class="meal ${pending?.kind === k ? 'on' : ''}" data-kind="${k}">${icon(KIND_ICON[k])}${esc(kindName(k))}</button>`).join('')}</div>
    <p class="muted note">${pending ? esc(tr('pwa_picking_a_notification_comes_in', { meal: kindName(pending.kind) })) : tr('pwa_the_pc_picks_restaurants_and')}</p>
    <div id="picks">${picksHtml(current)}</div>
    <h2>${tr('meals_diet')}</h2>
    <details class="card"><summary><span>${preset ? esc(preset[1]) : tr('pwa_your_own_diet')}</span>${icon('chev', 'chev')}</summary>
      <div class="chips">${presets().map(([id, name, text]) => `<button class="chip ${text === diet ? 'on' : ''}" data-preset="${id}">${esc(name)}</button>`).join('')}</div>
      <textarea id="diet" rows="10">${esc(diet)}</textarea>
      <div class="row"><button class="btn" id="save">${tr('meals_save_the_diet')}</button></div></details>
    <div class="row"><button class="btn text" id="repair">${tr('meals_pair_again')}</button></div>`;
  document.getElementById('allow')?.addEventListener('click', subscribe);
  app.querySelectorAll('[data-kind]').forEach((b) => { b.onclick = async () => {
    try {
      await post(s.link.up, await seal(unb64(s.link.key), { t: 'pick', kind: b.dataset.kind }));
      store({ pending: { kind: b.dataset.kind, at: Date.now() } });
      mainScreen(load(), false);
    } catch (e) { toast(tr('pwa_couldnt_send', { error: e.message })); }
  }; });
  app.querySelectorAll('[data-preset]').forEach((b) => { b.onclick = () => {
    document.getElementById('diet').value = presets().find(([id]) => id === b.dataset.preset)[2];
    app.querySelectorAll('[data-preset]').forEach((c) => c.classList.toggle('on', c === b));
    toast(tr('meals_press_save_the_diet_to_keep'));
  }; });
  document.getElementById('repair').onclick = () => { localStorage.removeItem('cm'); pairScreen(); };
  document.getElementById('save').onclick = async () => {
    const text = document.getElementById('diet').value.trim();
    try {
      await post(s.link.up, await seal(unb64(s.link.key), { t: 'diet', text, at: Date.now() }));
      store({ diet: { text, at: Date.now() } });
      toast(tr('pwa_sent_to_the_pc_it_applies'));
    } catch (e) { toast(tr('pwa_couldnt_send', { error: e.message })); }
  };
  if (!refresh) return;
  const fresh = await fromPc(s);
  let again = false;
  if (fresh.diet && fresh.diet.at > (s.diet?.at || 0)) {
    store({ diet: { text: fresh.diet.text, at: fresh.diet.at } });
    again = true;
  }
  // A push holds 4 KB: the relay has the whole picks of the same run.
  if (fresh.picks && fresh.picks.at >= (current?.at || 0) && JSON.stringify(fresh.picks) !== JSON.stringify(current)) {
    await keepPicks(fresh.picks);
    again = true;
  }
  // Don't redraw under the user's fingers while they edit the diet.
  if (again && !app.querySelector('details[open]')) mainScreen(load(), false);
}

async function render() {
  const s = load();
  const code = new URLSearchParams(location.search).get('pair');
  if (!s.link) {
    pairScreen(code || '');
    if (code) connect(code);
    return;
  }
  await mainScreen(s);
}

(async () => {
  try { texts = await (await fetch(`locales/${lang}.json`)).json(); } catch { /* keys stay visible */ }
  if ('serviceWorker' in navigator) {
    navigator.serviceWorker.register('sw.js').catch(() => {});
    // Any push (picks or a failure) ends the wait for a pick.
    navigator.serviceWorker.addEventListener('message', () => { if (load().link) { store({ pending: null }); render(); } });
  }
  document.addEventListener('visibilitychange', () => { if (!document.hidden && load().link) render(); });
  render();
})();
