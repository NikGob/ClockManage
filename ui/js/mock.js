// Browser-only mock of the Rust backend, used for design previews (`?state=work`).
// Never loaded inside the Tauri app.
import { lang } from './i18n.js';

// Preview data in the preview's language (not part of the app's texts).
const L = (ru, en) => (lang === 'en' ? en : ru);
const q = new URLSearchParams(location.search);
let STATE = q.get('state') || 'work';
// `?then=lunch_break&at=2000`: switch the phase after a while (to test what a click does
// when the buttons change under the cursor).
if (q.get('then')) setTimeout(() => { STATE = q.get('then'); }, Number(q.get('at') || 2000));
const t0 = Date.now();
const MIN = 60000;
let update = q.get('update') || 'latest';

const cfg = {
  week: ['full', 'full', 'light', 'full', 'full', 'off', 'off'], day_end_min: 1320, tz_offset_min: 180,
  night_start: { enabled: true, until_min: 300, after_day_end: true, hold_sec: 2 }, lang: q.get('lang') || 'auto',
  profiles: {
    full: { plan: [{ name: L('Математика', 'Math'), minutes: 90 }, { name: L('Словацкий', 'Spanish'), minutes: 90 }, { name: L('Экстернат', 'Thesis'), minutes: 150 }], block: true, autolock: true, autolock_min: 780 },
    light: { plan: [{ name: L('Математика', 'Math'), minutes: 60 }, { name: L('Словацкий', 'Spanish'), minutes: 60 }], block: true, autolock: true, autolock_min: 780 },
    off: { plan: [], block: false, autolock: false, autolock_min: 780 },
  },
  timing: { work_segment_min: 45, short_break_min: 10, between_blocks_min: 20, lunch_min: 45 },
  blocklist: { sites: ['web.telegram.org', 'x.com', 'twitter.com', 'twitch.tv', 'discord.com', 'youtube.com/shorts'], apps: ['Telegram.exe', 'Discord.exe'] },
  break_access: q.get('ba') === '1',
  emergency_phrase: L('Я осознанно прерываю учебный день, понимаю что это попадёт в лог, и через десять минут вернусь к работе', 'I am consciously interrupting my study day, I understand this goes to the log, and I will be back to work in ten minutes'),
  emergency_min: 10, reminder_sec: 60, sound: true, overlay: true, restart_firefox: true, autostart: true, auto_update: true, mcp_enabled: true, mcp_port: 47213,
  segments: [{ name: L('Обед', 'Lunch'), minutes: 45, alarm: false, open_access: false, prep_min: 0 }, { name: L('Сон', 'Nap'), minutes: 20, alarm: true, open_access: false, prep_min: 10 }, { name: L('Прогулка', 'Walk'), minutes: 15, alarm: false, open_access: false, prep_min: 0 }],
  phone: { enabled: q.get('phone') !== '0', port: 47811, apps: ['org.telegram.messenger', 'com.discord'], devices: [] },
  meals: {
    diet: L('Без сахара и жареного. Белок в каждом приёме пищи, овощи — половина тарелки. Обед до 700 ккал, ужин до 500.', 'No sugar, nothing fried. Protein at every meal, vegetables half the plate. Lunch up to 700 kcal, dinner up to 500.'),
    address: 'Mannerheimintie 16, Helsinki', lat: 60.1699, lon: 24.9384, max_km: 3, times: [780, 1140], lead_min: 30,
    agent_cmd: 'claude -p --mcp-config "{mcp_config}" --strict-mcp-config --allowedTools "mcp__clockmanage__get_diet mcp__clockmanage__wolt_search mcp__clockmanage__wolt_menu mcp__clockmanage__set_meal_options"',
    pwa_url: 'https://danilt2000.github.io/ClockManage/', vapid_key: '', link: q.get('paired') === '0' ? null : { key: '', up: '', down: '' }, subscription: null, relay_seen: 0,
  },
  appearance: { seed: q.get('seed') || '#2E7D32', mode: q.get('mode') || 'system', variant: q.get('variant') || 'fidelity', mini_contrast: q.get('contrast') === '1' },
};
let kind = q.get('kind') || 'full';
const W = 'https://wolt.com/en/fin/helsinki/restaurant/';
const mealPicks = {
  at: t0 - 4 * MIN, meal: '13:00', kind: 'lunch',
  venues: [
    { name: 'Fafa’s Kamppi', url: `${W}fafas-kamppi`, km: 0.6, eta: '20-30', dishes: [
      { name: L('Боул с курицей', 'Chicken bowl'), price: 1390, currency: 'EUR', url: `${W}fafas-kamppi/itemid-1`, why: L('Белок и овощи, без жарки', 'Protein and vegetables, nothing fried') },
      { name: L('Хумус с овощами', 'Hummus with vegetables'), price: 650, currency: 'EUR', url: `${W}fafas-kamppi/itemid-2`, why: L('Клетчатка, мало сахара', 'Fibre, little sugar') },
    ] },
    { name: 'Poke Bowl Helsinki', url: `${W}poke-bowl`, km: 1.1, eta: '25-35', dishes: [
      { name: L('Поке с лососем', 'Salmon poke'), price: 1590, currency: 'EUR', url: `${W}poke-bowl/itemid-3`, why: L('Рыба и рис, соус отдельно', 'Fish and rice, sauce on the side') },
    ] },
    { name: 'Green Kitchen', url: `${W}green-kitchen`, km: 2.4, eta: '30-40', dishes: [
      { name: L('Салат с тофу', 'Tofu salad'), price: 1290, currency: 'EUR', url: `${W}green-kitchen/itemid-4`, why: L('Лёгкий ужин до 500 ккал', 'A light dinner under 500 kcal') },
      { name: L('Суп из чечевицы', 'Lentil soup'), price: 890, currency: 'EUR', url: `${W}green-kitchen/itemid-5`, why: L('Белок без мяса', 'Protein without meat') },
    ] },
  ],
};
let mealsPairing = null;
const MOCK_QR = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 29 29"><rect width="29" height="29" fill="#fff"/>${Array.from({ length: 841 }, (_, i) => ((i * 7919) % 13 < 6 ? `<rect x="${i % 29}" y="${Math.floor(i / 29)}" width="1" height="1"/>` : '')).join('')}</svg>`;
let phonePin = null;
let noteDone = false;
let dayEnd = 1320;

function blocks(now) {
  const el = now - t0;
  const b = [
    { name: L('Математика', 'Math'), minutes: 90, work_ms: 90 * MIN, parts: 2, parts_done: 2, done: true, current: false, started: true },
    { name: L('Словацкий', 'Spanish'), minutes: 90, work_ms: 45 * MIN, parts: 2, parts_done: 1, done: false, current: true, started: true },
    { name: L('Обед', 'Lunch'), minutes: 45, work_ms: 0, parts: 0, parts_done: 0, done: false, current: false, started: false, kind: 'break' },
    { name: L('Экстернат', 'Thesis'), minutes: 150, work_ms: 0, parts: 4, parts_done: 0, done: false, current: false, started: false },
  ];
  if (STATE === 'work') b[1].work_ms = 45 * MIN + 13 * MIN + el;
  if (STATE === 'idle') b.forEach((x) => { x.work_ms = 0; x.parts_done = 0; x.done = false; x.current = false; x.started = false; });
  // A planned segment is not study: it has no work time.
  if (STATE === 'done') b.forEach((x) => { x.work_ms = x.kind === 'break' ? 0 : x.minutes * MIN; x.done = true; x.current = false; });
  return b;
}

function phase(now) {
  const el = now - t0;
  switch (STATE) {
    case 'work': { const dur = 45 * MIN, e = 13 * MIN + el; return { kind: 'work', title: L('Словацкий', 'Spanish'), subtitle: L('Часть 2 из 2', 'Part 2 of 2'), block: 1, dur_ms: dur, elapsed_ms: e, remaining_ms: dur - e, running: true, paused: false, waiting_ms: 0 }; }
    case 'paused': { const dur = 45 * MIN, e = 21 * MIN; return { kind: 'work', title: L('Словацкий', 'Spanish'), subtitle: L('Часть 2 из 2', 'Part 2 of 2'), block: 1, dur_ms: dur, elapsed_ms: e, remaining_ms: dur - e, running: false, paused: true, waiting_ms: 0 }; }
    case 'lunch_break': { const dur = 45 * MIN, e = el; return { kind: 'lunch_break', title: L('Обед', 'Lunch'), subtitle: L('Дальше: Словацкий, часть 2 из 2', 'Next: Spanish, part 2 of 2'), block: 1, dur_ms: dur, elapsed_ms: e, remaining_ms: dur - e, running: true, paused: false, waiting_ms: 0 }; }
    case 'break': {
      // ?ba=1 shows the break between blocks (the one access in breaks opens).
      if (cfg.break_access) { const dur = 20 * MIN, e = 13 * MIN + el; return { kind: 'break', title: L('Перерыв между блоками', 'Break between blocks'), subtitle: L('Дальше: Экстернат, часть 1 из 4', 'Next: Thesis, part 1 of 4'), block: 3, dur_ms: dur, elapsed_ms: e, remaining_ms: dur - e, running: true, paused: false, waiting_ms: 0 }; }
      const dur = 10 * MIN, e = 3 * MIN + el; return { kind: 'break', title: L('Перерыв', 'Break'), subtitle: L('Дальше: Словацкий, часть 2 из 2', 'Next: Spanish, part 2 of 2'), block: 1, dur_ms: dur, elapsed_ms: e, remaining_ms: dur - e, running: true, paused: false, waiting_ms: 0 };
    }
    case 'segment': {
      // ?prep=1 — getting ready for the nap; ?sw=1 — lunch without a timer.
      const prep = !!q.get('prep'), sw = !!q.get('sw');
      const dur = prep ? 10 * MIN : sw ? 45 * MIN : 20 * MIN, e = (q.get('over') ? dur + 7 * MIN : 6 * MIN) + el;
      const sub = prep ? L('Подготовка · потом сон 20 мин — нажми «Лёг», когда ляжешь', 'Getting ready · then nap 20 min — press “In bed” when you lie down') : L('Дальше: Словацкий, часть 2 из 2', 'Next: Spanish, part 2 of 2');
      return { kind: 'segment', title: sw ? L('Обед', 'Lunch') : L('Сон', 'Nap'), subtitle: sub, block: 1, dur_ms: dur, elapsed_ms: e, remaining_ms: dur - e, running: true, paused: false, waiting_ms: 0, alarm: !sw, prep, stopwatch: sw, queue: q.get('queue') ? [L('Прогулка 15 мин', 'Walk 15 min')] : [] };
    }
    case 'await': return { kind: 'await', title: L('Перерыв окончен', 'Break over'), subtitle: L('Словацкий · часть 2 из 2', 'Spanish · part 2 of 2'), block: 1, dur_ms: 0, elapsed_ms: 0, remaining_ms: 0, running: false, paused: false, waiting_ms: 74000 + el };
    case 'done': return { kind: 'done', title: L('День закрыт', 'Day closed'), subtitle: L('Все блоки отсижены', 'All blocks done'), block: null, dur_ms: 0, elapsed_ms: 0, remaining_ms: 0, running: false, paused: false, waiting_ms: 0 };
    default: return { kind: 'idle', title: L('День не начат', 'Day not started'), subtitle: '', block: null, dur_ms: 0, elapsed_ms: 0, remaining_ms: 0, running: false, paused: false, waiting_ms: 0 };
  }
}

// Day end: midnight of the mock day is "today" in local time — good enough for previews.
const midnight = new Date(); midnight.setHours(0, 0, 0, 0);
const dayEndAt = () => midnight.getTime() + dayEnd * MIN;

function forecast(now, b) {
  const work = b.reduce((a, x) => a + Math.max(0, x.minutes * MIN - x.work_ms), 0);
  const breaks = work > 0 ? 60 * MIN : 0;
  const finish = now + work + breaks;
  return { work_left_ms: work, breaks_left_ms: breaks, finish_at: finish, day_end_at: dayEndAt(), fits: finish <= dayEndAt(), margin_ms: dayEndAt() - finish };
}

function snapshot() {
  const now = Date.now();
  const b = blocks(now);
  const p = phase(now);
  const started = STATE !== 'idle';
  const paused = STATE === 'paused';
  // `?ba=1`: access in the break between blocks (state=break).
  const access = STATE === 'break' && cfg.break_access;
  // `?focus=1`: a focus lock (start_focus_lock) runs for 95 more minutes.
  const focus_until = q.get('focus') === '1' ? t0 + 95 * MIN : null;
  // `?autolock=1` (state=idle): the day hasn't started by 13:00, the autolock holds the list.
  const autolockOn = STATE === 'idle' && q.get('autolock') === '1' && kind !== 'off';
  const prof = cfg.profiles[kind];
  const autolock = autolockOn ? { time: '13:00', at: t0 - 25 * MIN, on: true, start_delay_min: null }
    : STATE === 'idle' && kind !== 'off' && prof.autolock ? { time: `${String(Math.floor(prof.autolock_min / 60)).padStart(2, '0')}:${String(prof.autolock_min % 60).padStart(2, '0')}`, at: midnight.getTime() + prof.autolock_min * MIN, on: false, start_delay_min: null }
      : null;
  const lock = focus_until && (STATE === 'idle' || STATE === 'done') ? { blocked: true, base: true, reason: 'focus', until: null, focus_until }
    : autolockOn ? { blocked: true, base: true, reason: 'autolock', until: null, focus_until }
    : STATE === 'idle' ? { blocked: false, base: false, reason: 'not_started', until: null, focus_until }
      : STATE === 'done' ? { blocked: false, base: false, reason: 'completed', until: null, focus_until }
        : access && !focus_until ? { blocked: false, base: true, reason: 'break_access', until: t0 + 7 * MIN, focus_until }
          : { blocked: true, base: true, reason: 'study', until: null, focus_until };
  return {
    view: {
      now, date: '2026-09-28', weekday: 0, study_day: kind !== 'off', kind, started, completed: STATE === 'done', after_day_end: false, night_hold_sec: cfg.night_start.hold_sec, mode: 'plan',
      day_end: `${String(Math.floor(dayEnd / 60) % 24).padStart(2, '0')}:${String(dayEnd % 60).padStart(2, '0')}`, day_end_min: dayEnd, now_min: 17 * 60 + 5, day_end_base: '22:00',
      day_end_at: dayEndAt(), day_end_next_day: dayEnd >= 1440, day_end_changed: dayEnd !== 1320, forecast: forecast(now, b),
      phase: p, blocks: b, planned_ms: 330 * MIN, work_ms: b.reduce((a, x) => a + x.work_ms, 0), lock, autolock,
      pause: paused ? { since: t0 - 4 * MIN, paused_ms: now - t0 + 4 * MIN } : null,
      emergency_count: 0, lunch_used: false, single: null, undo_until: null,
      pending_note: q.get('note') && !noteDone ? { block: 0, name: L('Математика', 'Math') } : null,
      can: {
        start_day: STATE === 'idle', night_start: STATE === 'idle' && q.has('night'), pause: ['work', 'break'].includes(STATE), resume: paused, start_next: ['await', 'break', 'lunch_break'].includes(STATE),
        lunch: ['await', 'break'].includes(STATE), single: ['idle', 'done'].includes(STATE), stop_single: false,
        emergency: lock.base && !access, end_access: false, edit_break_access: !lock.base,
        extend_day_end: kind !== 'off' && STATE !== 'done' && dayEnd < 1560, lighter_kind: !lock.base, set_day_end: true,
        finish_block: ['work', 'paused', 'break', 'await'].includes(STATE),
        segment: ['break', 'await', 'segment'].includes(STATE), end_segment: STATE === 'segment',
        lay_down: STATE === 'segment' && !!q.get('prep'), segment_mode: STATE === 'segment' && !!q.get('sw'),
      },
    },
    meta: {
      admin: q.get('admin') !== '0', version: '0.5.0', mcp: { enabled: true, port: 47213, running: true, url: 'http://127.0.0.1:47213/mcp', error: null },
      phone: { enabled: cfg.phone.enabled, running: cfg.phone.enabled, port: 47811, address: '192.168.1.42', pc_name: 'DESKTOP-PC', error: null, pin: phonePin, pin_until: phonePin ? t0 + 2 * MIN : null,
        apk: { available: true, code: 42, name: '0.5.0 (58)', size: 812345 },
        devices: [{ id: 'a1b2c3', name: 'Pixel 8', paired_at: t0 - 86400000, last_seen: Date.now() - 2 * MIN, blocker: true }] },
      blocker_error: null, blocking_applied: lock.blocked, sound: true, overlay: true, break_access: cfg.break_access,
      emergency_min: 10, lunch_min: 45, segments: cfg.segments, seed: cfg.appearance.seed, theme_mode: cfg.appearance.mode, variant: cfg.appearance.variant,
      mini_contrast: cfg.appearance.mini_contrast,
      data_dir: q.get('os') === 'linux' ? '/home/you/.local/share/com.nikgob.clockmanage' : 'C:\\Users\\you\\AppData\\Roaming\\com.nikgob.clockmanage',
      tz_offset_min: cfg.tz_offset_min,
      // ?lang=en previews the English interface; "auto" follows the browser.
      lang: cfg.lang === 'auto' ? (/^(ru|uk|be)/i.test(navigator.language) ? 'ru' : 'en') : cfg.lang,
      // ?update=ready previews a downloaded update (also available, checking, downloading, error).
      update: { state: update, version: ['available', 'downloading', 'ready'].includes(update) ? '0.5.1' : null, checked_at: t0 - 20 * MIN, error: update === 'error' ? 'error sending request' : null, by_itself: q.get('os') !== 'linux' || q.get('admin') === '0' },
    },
  };
}

const listeners = {};
export function listen(ev, cb) {
  (listeners[ev] ||= []).push(cb);
  if (ev === 'state') setInterval(() => cb(snapshot()), 1000);
  return () => {};
}

const days = [
  { date: '2026-09-28', planned_min: 330, actual_min: 148, blocks_done: 1, blocks: 3, pauses: 2, pauses_min: 11, emergencies: 0, pause_access_min: 0, study_day: true, started: true },
  { date: '2026-09-25', planned_min: 300, actual_min: 300, blocks_done: 3, blocks: 3, pauses: 1, pauses_min: 6, emergencies: 1, pause_access_min: 10, study_day: true, started: true },
  { date: '2026-09-24', planned_min: 330, actual_min: 262, blocks_done: 2, blocks: 3, pauses: 4, pauses_min: 31, emergencies: 0, pause_access_min: 0, study_day: true, started: true },
];

export async function invoke(cmd, args) {
  switch (cmd) {
    case 'get_state': return snapshot();
    case 'get_config': return structuredClone(cfg);
    case 'save_config': Object.assign(cfg, args.cfg); return structuredClone(cfg);
    case 'get_overlay': return null;
    case 'set_day_kind': kind = args.kind; return null;
    case 'extend_day_end': dayEnd = args.minutes; return null;
    case 'list_days': return q.get('empty') ? [] : days;
    case 'set_day_end': {
      const [h, m] = args.time.split(':').map(Number);
      const fmt = (x) => `${String(Math.floor(x / 60) % 24).padStart(2, '0')}:${String(x % 60).padStart(2, '0')}`;
      const old = dayEnd;
      dayEnd = h * 60 + m <= 120 ? h * 60 + m + 1440 : h * 60 + m;
      setTimeout(() => (listeners.agent || []).forEach((cb) => cb({ title: L('Агент изменил конец дня', 'The agent moved the day end'), text: `${fmt(old)} → ${fmt(dayEnd)} (${L('превью', 'preview')})` })), 1500);
      return { ok: true, changed: old !== dayEnd, old: fmt(old), new: fmt(dayEnd), new_is_next_day: dayEnd >= 1440 };
    }
    case 'start_day': STATE = 'work'; return null;
    case 'pause': STATE = 'paused'; return null;
    case 'resume': STATE = 'work'; return null;
    case 'start_next': STATE = 'work'; return null;
    case 'start_segments': STATE = 'segment'; return null;
    case 'end_segment': STATE = 'await'; return null;
    case 'lay_down': q.delete('prep'); return null;
    case 'set_segment_mode': if (args?.stopwatch) q.set('sw', '1'); else q.delete('sw'); return null;
    case 'undo_skip': STATE = 'break'; return null;
    case 'week_stats': return {
      week: { week_start: '2026-09-28', days: ['2026-09-28', '2026-09-29', '2026-09-30', '2026-10-01', '2026-10-02', '2026-10-03', '2026-10-04'],
        subjects: [{ name: L('Математика', 'Math'), hours: [1.5, 1.5, 0.75, 1.5, 1.25, 0, 0], total: 6.5 }, { name: L('Словацкий', 'Spanish'), hours: [0.75, 1.5, 1, 0, 1.5, 0, 0], total: 4.75 }, { name: L('Экстернат', 'Thesis'), hours: [0, 2.25, 0, 2.5, 1.75, 0, 0], total: 6.5 }],
        day_totals: [2.25, 5.25, 1.75, 4, 4.5, 0, 0], total: 17.75, actual_min: 1104.6 },
      tsv: L('Предмет\t28.09\n', 'Subject\t28.09\n') };
    case 'set_block_note': noteDone = true; return null;
    case 'check_update': update = 'checking'; setTimeout(() => { update = 'latest'; }, 1500); return null;
    case 'install_update': location.reload(); return null;
    case 'drop_queued': return null;
    case 'finish_block': return { ok: true, block: args.name };
    case 'meals_state': return { picks: q.get('picks') === '0' ? null : mealPicks, running: false, error: null, paired: q.get('paired') !== '0', pairing: mealsPairing };
    case 'meals_pick_now': return null;
    case 'meals_pair': mealsPairing = { code: 'K7QM-4XPT', url: `${cfg.meals.pwa_url}?pair=K7QM4XPT`, qr: MOCK_QR, until: Date.now() + 10 * MIN }; return mealsPairing;
    case 'meals_unpair': mealsPairing = null; cfg.meals.link = null; return null;
    case 'meals_geocode': return { lat: 60.1699, lon: 24.9384, label: args.address };
    case 'open_url': return null;
    case 'phone_pin': phonePin = '482913'; return { pin: phonePin, until: Date.now() + 2 * MIN };
    case 'captcha_new': return { id: 1, problems: ['47 × 8', '512 + 389', '742 − 118 × 4'], wait_ms: 15000 };
    case 'day_stats': return {
      date: args.date, study_day: true, started_at: '2026-09-28T13:12:00+03:00', completed_at: null, planned_min: 330, actual_min: 148,
      blocks: [
        { name: L('Математика', 'Math'), planned_min: 90, actual_min: 90, journal_hours: 1.5, done: true, parts_done: 2, pauses: 1, pause_min: 6, note: L('скучно на интегралах, лез в телефон', 'integrals got boring, checked my phone') },
        { name: L('Словацкий', 'Spanish'), planned_min: 90, actual_min: 58, journal_hours: 0.75, done: false, parts_done: 1, pauses: 1, pause_min: 5 },
        { name: L('Экстернат', 'Thesis'), planned_min: 150, actual_min: 0, journal_hours: 0, done: false, parts_done: 0, pauses: 0, pause_min: 0 },
      ],
      journal_total: 2.25, pauses_count: 2, pauses_min: 11,
      pauses: [
        { start: '2026-09-28T13:40:00+03:00', end: '2026-09-28T13:46:00+03:00', minutes: 6, during: 'work', block: L('Математика', 'Math'), access_min: 0, extensions: 0 },
        { start: '2026-09-28T15:02:00+03:00', end: '2026-09-28T15:07:00+03:00', minutes: 5, during: 'break', block: L('Словацкий', 'Spanish'), access_min: 0, extensions: 0 },
      ],
      pause_access_min: 0, pause_access_extensions: 0, emergency_count: 0, emergencies: [], autolock: { time: '13:00', at: '2026-09-28T13:00:00+03:00', day_started_at: '2026-09-28T13:12:00+03:00', start_delay_min: 12 },
      breaks: [{ type: L('Обед', 'Lunch'), planned_min: 45, actual_min: 52, overrun_min: 7, start: '2026-09-28T14:40:00+03:00', end: '2026-09-28T15:32:00+03:00' }, { type: L('Сон', 'Nap'), planned_min: 20, actual_min: 20.5, overrun_min: 0.5, start: '2026-09-28T15:32:00+03:00', end: '2026-09-28T15:52:30+03:00' }], single_timer_min: 0,
    };
    default: console.log('[mock]', cmd, args); return null;
  }
}

export function overlayDemo(kind) {
  const d = {
    await: { kind: 'await', passive: false, title: L('Перерыв окончен', 'Break over'), text: L('Словацкий · часть 2 из 2', 'Spanish · part 2 of 2'), action: L('Начать часть 2', 'Start part 2') },
    break: { kind: 'break', passive: true, auto_hide_ms: 600000, title: L('Перерыв', 'Break'), text: L('Математика: часть 1 из 2 готова. Перерыв 10 мин.', 'Math: part 1 of 2 done. Break 10 min.') },
    block: { kind: 'block', passive: false, title: L('«Математика» закрыт', '“Math” closed'), text: L('1 ч 30 мин работы · пауз: 1 (6 мин)', '1 h 30 min of work · pauses: 1 (6 min)'), ask_note: true, block: 0 },
    day: { kind: 'day', passive: false, title: L('День закрыт', 'Day closed'), text: L('5 ч 30 мин учёбы. Блокировка снята.', '5 h 30 min of study. The lock is off.') },
    wake: { kind: 'wake', passive: false, force: true, title: L('Вставай!', 'Wake up!'), text: L('Сон окончен 5 мин назад', 'Nap ended 5 min ago'), action: L('Встал', 'I’m up') },
    segment: { kind: 'segment', passive: false, title: L('Обед окончен', 'Lunch over'), text: L('Дальше: Словацкий, часть 2 из 2', 'Next: Spanish, part 2 of 2'), action: L('Закончил обед', 'Done with lunch') },
    prep: { kind: 'prep', passive: false, title: L('Пора ложиться', 'Time to lie down'), text: L('Подготовка окончена. Сон и будильник пойдут от «Лёг».', 'Time’s up for getting ready. The nap and the alarm start from “In bed”.'), action: L('Лёг', 'In bed') },
    ask: { kind: 'ask', passive: false, title: L('Что сейчас?', 'What now?'), text: L('Блок закрыт 10 минут назад. Запусти таймер того, чем занят:', 'A block closed 10 minutes ago. Start a timer for what you’re doing:'), types: [{ name: L('Обед', 'Lunch'), minutes: 45 }, { name: L('Сон', 'Nap'), minutes: 20 }, { name: L('Прогулка', 'Walk'), minutes: 15 }] },
    nope: { kind: 'nope', passive: true, auto_hide_ms: 600000, title: L('Не-не-не', 'Nope'), text: L('Telegram — после учёбы', 'Telegram — after studying') },
  }[kind];
  return { ...d, demo: true };
}
