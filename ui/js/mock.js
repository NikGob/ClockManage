// Browser-only mock of the Rust backend, used for design previews (`?state=work`).
// Never loaded inside the Tauri app.
const q = new URLSearchParams(location.search);
let STATE = q.get('state') || 'work';
// `?then=lunch_break&at=2000`: switch the phase after a while (to test what a click does
// when the buttons change under the cursor).
if (q.get('then')) setTimeout(() => { STATE = q.get('then'); }, Number(q.get('at') || 2000));
const t0 = Date.now();
const MIN = 60000;

const cfg = {
  week: ['full', 'full', 'light', 'full', 'full', 'off', 'off'], day_end_min: 1320, tz_offset_min: 180,
  profiles: {
    full: { plan: [{ name: 'Математика', minutes: 90 }, { name: 'Словацкий', minutes: 90 }, { name: 'Экстернат', minutes: 150 }], block: true },
    light: { plan: [{ name: 'Математика', minutes: 60 }, { name: 'Словацкий', minutes: 60 }], block: true },
    off: { plan: [], block: false },
  },
  timing: { work_segment_min: 45, short_break_min: 10, between_blocks_min: 20, lunch_min: 45 },
  blocklist: { sites: ['web.telegram.org', 'x.com', 'twitter.com', 'twitch.tv', 'discord.com', 'youtube.com/shorts'], apps: ['Telegram.exe', 'Discord.exe'] },
  pause_access: q.get('pa') === '1', pause_access_min: 10,
  emergency_phrase: 'Я осознанно прерываю учебный день, понимаю что это попадёт в лог, и через десять минут вернусь к работе',
  emergency_min: 10, reminder_sec: 60, sound: true, overlay: true, restart_firefox: true, autostart: true, mcp_enabled: true, mcp_port: 47213,
  segments: [{ name: 'Обед', minutes: 45, alarm: false, open_access: false, prep_min: 0 }, { name: 'Сон', minutes: 20, alarm: true, open_access: false, prep_min: 10 }, { name: 'Прогулка', minutes: 15, alarm: false, open_access: false, prep_min: 0 }],
  phone: { enabled: q.get('phone') !== '0', port: 47811, apps: ['org.telegram.messenger', 'com.discord'], devices: [] },
  appearance: { seed: q.get('seed') || '#2E7D32', mode: q.get('mode') || 'system', variant: q.get('variant') || 'fidelity', mini_contrast: q.get('contrast') === '1' },
};
let kind = q.get('kind') || 'full';
let phonePin = null;
let noteDone = false;
let dayEnd = 1320;

function blocks(now) {
  const el = now - t0;
  const b = [
    { name: 'Математика', minutes: 90, work_ms: 90 * MIN, parts: 2, parts_done: 2, done: true, current: false, started: true },
    { name: 'Словацкий', minutes: 90, work_ms: 45 * MIN, parts: 2, parts_done: 1, done: false, current: true, started: true },
    { name: 'Обед', minutes: 45, work_ms: 0, parts: 0, parts_done: 0, done: false, current: false, started: false, kind: 'break' },
    { name: 'Экстернат', minutes: 150, work_ms: 0, parts: 4, parts_done: 0, done: false, current: false, started: false },
  ];
  if (STATE === 'work') b[1].work_ms = 45 * MIN + 13 * MIN + el;
  if (STATE === 'idle') b.forEach((x) => { x.work_ms = 0; x.parts_done = 0; x.done = false; x.current = false; x.started = false; });
  if (STATE === 'done') b.forEach((x) => { x.work_ms = x.minutes * MIN; x.done = true; x.current = false; });
  return b;
}

function phase(now) {
  const el = now - t0;
  switch (STATE) {
    case 'work': { const dur = 45 * MIN, e = 13 * MIN + el; return { kind: 'work', title: 'Словацкий', subtitle: 'Часть 2 из 2', block: 1, dur_ms: dur, elapsed_ms: e, remaining_ms: dur - e, running: true, paused: false, waiting_ms: 0 }; }
    case 'paused': { const dur = 45 * MIN, e = 21 * MIN; return { kind: 'work', title: 'Словацкий', subtitle: 'Часть 2 из 2', block: 1, dur_ms: dur, elapsed_ms: e, remaining_ms: dur - e, running: false, paused: true, waiting_ms: 0 }; }
    case 'lunch_break': { const dur = 45 * MIN, e = el; return { kind: 'lunch_break', title: 'Обед', subtitle: 'Дальше: Словацкий, часть 2 из 2', block: 1, dur_ms: dur, elapsed_ms: e, remaining_ms: dur - e, running: true, paused: false, waiting_ms: 0 }; }
    case 'break': { const dur = 10 * MIN, e = 3 * MIN + el; return { kind: 'break', title: 'Перерыв', subtitle: 'Дальше: Словацкий, часть 2 из 2', block: 1, dur_ms: dur, elapsed_ms: e, remaining_ms: dur - e, running: true, paused: false, waiting_ms: 0 }; }
    case 'segment': {
      // ?prep=1 — getting ready for the nap; ?sw=1 — lunch without a timer.
      const prep = !!q.get('prep'), sw = !!q.get('sw');
      const dur = prep ? 10 * MIN : sw ? 45 * MIN : 20 * MIN, e = (q.get('over') ? dur + 7 * MIN : 6 * MIN) + el;
      const sub = prep ? 'Подготовка · потом сон 20 мин — нажми «Лёг», когда ляжешь' : 'Дальше: Словацкий, часть 2 из 2';
      return { kind: 'segment', title: sw ? 'Обед' : 'Сон', subtitle: sub, block: 1, dur_ms: dur, elapsed_ms: e, remaining_ms: dur - e, running: true, paused: false, waiting_ms: 0, alarm: !sw, prep, stopwatch: sw, queue: q.get('queue') ? ['Прогулка 15 мин'] : [] };
    }
    case 'await': return { kind: 'await', title: 'Перерыв окончен', subtitle: 'Словацкий · часть 2 из 2', block: 1, dur_ms: 0, elapsed_ms: 0, remaining_ms: 0, running: false, paused: false, waiting_ms: 74000 + el };
    case 'done': return { kind: 'done', title: 'День закрыт', subtitle: 'Все блоки отсижены', block: null, dur_ms: 0, elapsed_ms: 0, remaining_ms: 0, running: false, paused: false, waiting_ms: 0 };
    default: return { kind: 'idle', title: 'День не начат', subtitle: '', block: null, dur_ms: 0, elapsed_ms: 0, remaining_ms: 0, running: false, paused: false, waiting_ms: 0 };
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
  const access = paused && cfg.pause_access;
  const lock = STATE === 'idle' ? { blocked: false, base: false, reason: 'not_started', until: null }
    : STATE === 'done' ? { blocked: false, base: false, reason: 'completed', until: null }
      : access ? { blocked: false, base: true, reason: 'pause_access', until: t0 + 7 * MIN }
        : { blocked: true, base: true, reason: 'study', until: null };
  return {
    view: {
      now, date: '2026-09-28', weekday: 0, study_day: kind !== 'off', kind, started, completed: STATE === 'done', after_day_end: false, mode: 'plan',
      day_end: `${String(Math.floor(dayEnd / 60) % 24).padStart(2, '0')}:${String(dayEnd % 60).padStart(2, '0')}`, day_end_min: dayEnd, now_min: 17 * 60 + 5, day_end_base: '22:00',
      day_end_at: dayEndAt(), day_end_next_day: dayEnd >= 1440, day_end_changed: dayEnd !== 1320, forecast: forecast(now, b),
      phase: p, blocks: b, planned_ms: 330 * MIN, work_ms: b.reduce((a, x) => a + x.work_ms, 0), lock,
      pause: paused ? { since: t0 - 4 * MIN, paused_ms: now - t0 + 4 * MIN, access_enabled: cfg.pause_access, access_until: access ? t0 + 7 * MIN : null, access_left_ms: access ? t0 + 7 * MIN - now : 0, extensions: 0 } : null,
      emergency_count: 0, lunch_used: false, single: null, undo_until: null,
      pending_note: q.get('note') && !noteDone ? { block: 0, name: 'Математика' } : null,
      can: {
        start_day: STATE === 'idle', pause: ['work', 'break'].includes(STATE), resume: paused, start_next: ['await', 'break', 'lunch_break'].includes(STATE),
        lunch: ['await', 'break'].includes(STATE), single: ['idle', 'done'].includes(STATE), stop_single: false,
        emergency: lock.base && !access, extend_access: access, end_access: access, edit_pause_access: !lock.base,
        extend_day_end: kind !== 'off' && STATE !== 'done' && dayEnd < 1560, lighter_kind: !lock.base, set_day_end: true,
        finish_block: ['work', 'paused', 'break', 'await'].includes(STATE),
        segment: ['break', 'await', 'segment'].includes(STATE), end_segment: STATE === 'segment',
        lay_down: STATE === 'segment' && !!q.get('prep'), segment_mode: STATE === 'segment' && !!q.get('sw'),
      },
    },
    meta: {
      admin: true, version: '0.1.0', mcp: { enabled: true, port: 47213, running: true, url: 'http://127.0.0.1:47213/mcp', error: null },
      phone: { enabled: cfg.phone.enabled, running: cfg.phone.enabled, port: 47811, address: '192.168.1.42', pc_name: 'NIK-PC', error: null, pin: phonePin, pin_until: phonePin ? t0 + 2 * MIN : null,
        apk: { available: true, code: 42, name: '0.4.0 (42)', size: 812345 },
        devices: [{ id: 'a1b2c3', name: 'Pixel 8', paired_at: t0 - 86400000, last_seen: Date.now() - 2 * MIN, blocker: true }] },
      blocker_error: null, blocking_applied: lock.blocked, sound: true, overlay: true, pause_access: cfg.pause_access, pause_access_min: 10,
      emergency_min: 10, lunch_min: 45, segments: cfg.segments, seed: cfg.appearance.seed, theme_mode: cfg.appearance.mode, variant: cfg.appearance.variant,
      mini_contrast: cfg.appearance.mini_contrast,
      data_dir: 'C:\\Users\\nik\\AppData\\Roaming\\com.nikgob.clockmanage',
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
      setTimeout(() => (listeners.agent || []).forEach((cb) => cb({ title: 'Агент изменил конец дня', text: `${fmt(old)} → ${fmt(dayEnd)} (превью)` })), 1500);
      return { ok: true, changed: old !== dayEnd, old: fmt(old), new: fmt(dayEnd), new_is_next_day: dayEnd >= 1440 };
    }
    case 'start_next': STATE = 'work'; return null;
    case 'start_segments': STATE = 'segment'; return null;
    case 'end_segment': STATE = 'await'; return null;
    case 'lay_down': q.delete('prep'); return null;
    case 'set_segment_mode': if (args?.stopwatch) q.set('sw', '1'); else q.delete('sw'); return null;
    case 'undo_skip': STATE = 'break'; return null;
    case 'week_stats': return {
      week: { week_start: '2026-09-28', days: ['2026-09-28', '2026-09-29', '2026-09-30', '2026-10-01', '2026-10-02', '2026-10-03', '2026-10-04'],
        subjects: [{ name: 'Математика', hours: [1.5, 1.5, 0.75, 1.5, 1.25, 0, 0], total: 6.5 }, { name: 'Словацкий', hours: [0.75, 1.5, 1, 0, 1.5, 0, 0], total: 4.75 }, { name: 'Экстернат', hours: [0, 2.25, 0, 2.5, 1.75, 0, 0], total: 6.5 }],
        day_totals: [2.25, 5.25, 1.75, 4, 4.5, 0, 0], total: 17.75, actual_min: 1104.6 },
      tsv: 'Предмет\t28.09\n' };
    case 'set_block_note': noteDone = true; return null;
    case 'drop_queued': return null;
    case 'finish_block': return { ok: true, block: args.name };
    case 'phone_pin': phonePin = '482913'; return { pin: phonePin, until: Date.now() + 2 * MIN };
    case 'captcha_new': return { id: 1, problems: ['47 × 8', '512 + 389', '742 − 118 × 4'], wait_ms: 15000 };
    case 'day_stats': return {
      date: args.date, study_day: true, started_at: '2026-09-28T13:12:00+03:00', completed_at: null, planned_min: 330, actual_min: 148,
      blocks: [
        { name: 'Математика', planned_min: 90, actual_min: 90, journal_hours: 1.5, done: true, parts_done: 2, pauses: 1, pause_min: 6, note: 'скучно на интегралах, лез в телефон' },
        { name: 'Словацкий', planned_min: 90, actual_min: 58, journal_hours: 0.75, done: false, parts_done: 1, pauses: 1, pause_min: 5 },
        { name: 'Экстернат', planned_min: 150, actual_min: 0, journal_hours: 0, done: false, parts_done: 0, pauses: 0, pause_min: 0 },
      ],
      journal_total: 2.25, pauses_count: 2, pauses_min: 11,
      pauses: [
        { start: '2026-09-28T13:40:00+03:00', end: '2026-09-28T13:46:00+03:00', minutes: 6, during: 'work', block: 'Математика', access_min: 0, extensions: 0 },
        { start: '2026-09-28T15:02:00+03:00', end: '2026-09-28T15:07:00+03:00', minutes: 5, during: 'break', block: 'Словацкий', access_min: 0, extensions: 0 },
      ],
      pause_access_min: 0, pause_access_extensions: 0, emergency_count: 0, emergencies: [], breaks: [{ type: 'Обед', planned_min: 45, actual_min: 52, overrun_min: 7, start: '2026-09-28T14:40:00+03:00', end: '2026-09-28T15:32:00+03:00' }, { type: 'Сон', planned_min: 20, actual_min: 20.5, overrun_min: 0.5, start: '2026-09-28T15:32:00+03:00', end: '2026-09-28T15:52:30+03:00' }], single_timer_min: 0,
    };
    default: console.log('[mock]', cmd, args); return null;
  }
}

export function overlayDemo(kind) {
  const d = {
    await: { kind: 'await', passive: false, title: 'Перерыв окончен', text: 'Словацкий · часть 2 из 2', action: 'Начать часть 2' },
    break: { kind: 'break', passive: true, auto_hide_ms: 600000, title: 'Перерыв', text: 'Математика: часть 1 из 2 готова. Перерыв 10 мин.' },
    block: { kind: 'block', passive: false, title: '«Математика» закрыт', text: '1 ч 30 мин работы · пауз: 1 (6 мин)', ask_note: true, block: 0 },
    day: { kind: 'day', passive: false, title: 'День закрыт', text: '5 ч 30 мин учёбы. Блокировка снята.' },
    wake: { kind: 'wake', passive: false, force: true, title: 'Вставай!', text: 'Сон окончен 5 мин назад', action: 'Встал' },
    segment: { kind: 'segment', passive: false, title: 'Обед окончен', text: 'Дальше: Словацкий, часть 2 из 2', action: 'Закончил обед' },
    prep: { kind: 'prep', passive: false, title: 'Пора ложиться', text: 'Подготовка окончена. Сон и будильник пойдут от «Лёг».', action: 'Лёг' },
    ask: { kind: 'ask', passive: false, title: 'Что сейчас?', text: 'Блок закрыт 10 минут назад. Запусти таймер того, чем занят:', types: [{ name: 'Обед', minutes: 45 }, { name: 'Сон', minutes: 20 }, { name: 'Прогулка', minutes: 15 }] },
    nope: { kind: 'nope', passive: true, auto_hide_ms: 600000, title: 'Не-не-не', text: 'Telegram — после учёбы' },
    access: { kind: 'access', passive: true, auto_hide_ms: 600000, title: 'Доступ закрыт', text: 'Блокировка снова включена' },
  }[kind];
  return { ...d, demo: true };
}
