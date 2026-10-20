// The iPhone side of ClockManage meals: pair with the PC through ntfy.sh, take pushes, show the
// picks, edit the diet. Same sealing as core/src/relay.rs: AES-256-GCM, base64url(nonce ‖ ct).
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

// ---- screens ----
const kindName = (kind) => ({ breakfast: tr('meals_breakfast'), lunch: tr('meals_lunch'), dinner: tr('meals_dinner') }[kind] || tr('meals_food_picks'));
const presets = () => [
  ['brain', tr('meals_max_for_the_brain'), tr('meals_goal_a_clear_steady_mind')],
  ['protein', tr('meals_brain_protein'), tr('meals_goal_plenty_of_protein_for')],
  ['light', tr('meals_brain_light'), tr('meals_goal_lose_weight_slowly_while')],
];

const standalone = () => navigator.standalone === true || matchMedia('(display-mode: standalone)').matches;

function pairScreen(code = '', error = '') {
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

async function picks() {
  try {
    const r = await (await caches.open('cm')).match('picks.json');
    return r ? r.json() : null;
  } catch { return null; }
}

function picksHtml(p) {
  if (!p?.venues?.length) return `<div class="card muted">${tr('pwa_no_picks_yet_they_come_before')}</div>`;
  const price = (d) => { try { return new Intl.NumberFormat(LOCALE, { style: 'currency', currency: d.currency || 'EUR' }).format(d.price / 100); } catch { return String(d.price / 100); } };
  const time = new Date(p.at).toLocaleTimeString(LOCALE, { hour: '2-digit', minute: '2-digit' });
  const title = p.meal ? tr('meals_kind_at', { kind: kindName(p.kind), time: p.meal }) : kindName(p.kind);
  return `<p class="name">${esc(title)}</p><p class="muted">${tr('meals_picked_at_tap_a_dish', { time })}</p>` + p.venues.map((v) => `<div class="card">
      <div class="venue-head"><div class="grow"><div class="name">${esc(v.name)}</div>
        <div class="muted">${tr('meals_km_min', { km: Number(v.km).toLocaleString(LOCALE), eta: esc(v.eta) })}</div></div>
        <a class="btn tonal" href="${esc(v.url)}">${tr('meals_open_in_wolt')}</a></div>
      ${v.dishes.map((d) => `<a class="dish" href="${esc(d.url)}"><span><span class="name">${esc(d.name)}</span>${d.why ? `<br><span class="muted">${esc(d.why)}</span>` : ''}</span><span class="price">${esc(price(d))}</span></a>`).join('')}
    </div>`).join('');
}

async function latestDiet(s) {
  try {
    const key = unb64(s.link.key);
    for (const m of (await poll(s.link.down)).reverse()) {
      try {
        const msg = await open(key, m.sealed);
        if (msg.t === 'diet') return msg;
      } catch { /* not ours */ }
    }
  } catch { /* offline: keep the local copy */ }
  return null;
}

async function mainScreen(s) {
  const step = !standalone()
    ? `<div class="card"><p class="name">${tr('pwa_add_to_the_home_screen')}</p><p>${tr('pwa_notifications_only_work_from_the')}</p></div>`
    : !s.subscribed || Notification.permission !== 'granted'
      ? `<div class="card"><p>${tr('pwa_meal_picks_from_the_pc_come')}</p><div class="row"><button class="btn" id="allow">${tr('pwa_allow_notifications')}</button></div></div>`
      : '';
  const current = await picks();
  const pending = s.pending && Date.now() - s.pending.at < 10 * 60000 && !(current && current.at > s.pending.at) ? s.pending : null;
  app.innerHTML = `<p class="muted">${esc(tr('pwa_connected_to', { pc: s.pc || '' }))}</p>${step}
    <h2>${tr('pwa_order_now')}</h2>
    <div class="card"><div class="row">${['breakfast', 'lunch', 'dinner'].map((k) => `<button class="btn ${pending?.kind === k ? '' : 'tonal'}" data-kind="${k}">${esc(kindName(k))}</button>`).join('')}</div>
      <p class="muted" id="order-note">${pending ? esc(tr('pwa_picking_a_notification_comes_in', { meal: kindName(pending.kind) })) : tr('pwa_the_pc_picks_restaurants_and')}</p></div>
    <h2>${tr('meals_picks')}</h2><div id="picks">${picksHtml(current)}</div>
    <h2>${tr('meals_diet')}</h2>
    <div class="card"><div class="row"><span class="muted">${tr('meals_ready_diets')}</span>${presets().map(([id, name]) => `<button class="btn tonal" data-preset="${id}">${esc(name)}</button>`).join('')}</div>
      <textarea id="diet" rows="8" style="margin-top:10px">${esc(s.diet?.text || '')}</textarea>
      <div class="row"><button class="btn" id="save">${tr('meals_save_the_diet')}</button></div></div>
    <div class="row"><button class="btn text" id="repair">${tr('meals_pair_again')}</button></div>`;
  document.getElementById('allow')?.addEventListener('click', subscribe);
  app.querySelectorAll('[data-kind]').forEach((b) => { b.onclick = async () => {
    try {
      await post(s.link.up, await seal(unb64(s.link.key), { t: 'pick', kind: b.dataset.kind }));
      store({ pending: { kind: b.dataset.kind, at: Date.now() } });
      render();
    } catch (e) { toast(tr('pwa_couldnt_send', { error: e.message })); }
  }; });
  app.querySelectorAll('[data-preset]').forEach((b) => { b.onclick = () => {
    document.getElementById('diet').value = presets().find(([id]) => id === b.dataset.preset)[2];
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
  const fresh = await latestDiet(s);
  const area = document.getElementById('diet');
  if (fresh && fresh.at > (s.diet?.at || 0) && document.activeElement !== area) {
    store({ diet: { text: fresh.text, at: fresh.at } });
    area.value = fresh.text;
  }
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
    navigator.serviceWorker.addEventListener('message', () => { if (load().link) render(); });
  }
  render();
})();
