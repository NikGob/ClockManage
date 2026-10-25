// A made-up day for the iPhone web app's pictures, shaped as the PC sends it (`publish_day` in
// src-tauri/src/iphone.rs): Math done, Spanish in its second part, then lunch and Thesis.
const MIN = 60000;

export function demoState(lang = 'en', now = Date.now(), { meals = true, diet = '' } = {}) {
  const L = (ru, en) => (lang === 'ru' ? ru : en);
  const from = now - 12 * MIN - 25000;
  const blocks = [
    { name: L('Математика', 'Math'), minutes: 90, done: true, started: true, parts: 2, parts_done: 2, min_minutes: 1, kind: 'study' },
    { name: L('Словацкий', 'Spanish'), minutes: 90, done: false, started: true, parts: 2, parts_done: 1, min_minutes: 1, kind: 'study' },
    { name: L('Обед', 'Lunch'), minutes: 45, done: false, started: false, parts: 1, parts_done: 0, min_minutes: 1, kind: 'break' },
    { name: L('Экстернат', 'Thesis'), minutes: 150, done: false, started: false, parts: 4, parts_done: 0, min_minutes: 1, kind: 'study' },
  ];
  const entry = (o) => ({ paused: false, elapsed_ms: 0, alarm: false, prep: false, stopwatch: false, block: null, subtitle: '', ...o });
  const timeline = [
    entry({ from, until: from + 45 * MIN, kind: 'work', title: L('Словацкий', 'Spanish'), subtitle: L('часть 2 из 2', 'part 2 of 2'), block: 1, dur_ms: 45 * MIN, blocks_work_ms: [90 * MIN, 45 * MIN, 0, 0] }),
    entry({ from: from + 45 * MIN, until: null, kind: 'await', title: L('Обед', 'Lunch'), subtitle: L('после «Словацкого»', 'after Spanish'), block: 2, dur_ms: 0, blocks_work_ms: [90 * MIN, 90 * MIN, 0, 0] }),
  ];
  const day = { version: 'demo', server_now: now, lang, date: '', kind: 'full', study_day: true, started: true, completed: false, day_end: '22:00', day_end_at: now + 8 * 60 * MIN, done_at: null, blocks, timeline };
  return {
    link: { up: 'cmdemoup', down: 'cmdemodown', key: '' }, vapid: '', pc: 'Home PC', subscribed: true, lang, tab: 'today',
    diet: { text: diet, at: 1 },
    day: { day, meals, pc: 'Home PC', theme: null, at: now - 40000, offset: 0 },
  };
}
