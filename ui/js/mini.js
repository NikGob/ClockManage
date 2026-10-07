import { call, on, mmss, startDrag, installRipples } from './api.js';
import { applyTheme } from './theme.js';
import { icon, morphIcon } from './icons.js';

const el = (id) => document.getElementById(id);
const root = el('mini');
installRipples();
el('open').innerHTML = icon('open');
el('close').innerHTML = icon('close');

let prevTime = '';
// The single button changes meaning (pause → start the next part): clicks that keep coming
// right after it changed are ignored until they stop for a moment.
let btnSig = '';
let shieldUntil = 0;

function setTime(text) {
  const t = el('time');
  if (text === prevTime) return;
  // Animate only the glyphs that changed.
  if (prevTime.length === text.length && /\d/.test(text)) {
    t.innerHTML = [...text].map((c, i) => `<span class="${c !== prevTime[i] ? 'tick' : ''}">${c}</span>`).join('');
  } else {
    t.textContent = text;
  }
  prevTime = text;
}

function render(s) {
  const v = s.view;
  const p = v.phase;
  applyTheme({ seed: s.meta.seed, mode: s.meta.theme_mode, variant: s.meta.variant });
  root.dataset.kind = p.kind;
  root.dataset.contrast = String(!!s.meta.mini_contrast);
  root.dataset.paused = String(!!p.paused);
  let frac = 0;
  let label = p.title;
  let btn = null;
  const b = el('primary');
  switch (p.kind) {
    case 'work':
    case 'break':
    case 'lunch_break':
      frac = p.dur_ms ? p.elapsed_ms / p.dur_ms : 0;
      setTime(mmss(p.remaining_ms));
      {
        const part = p.subtitle.match(/(\d+) из (\d+)/);
        const short = part ? `${part[1]}/${part[2]}` : '';
        label = p.kind === 'work' ? `${p.title}${short ? ' · ' + short : ''}` : p.title;
        if (p.paused) label = `Пауза · ${label}`;
      }
      btn = p.paused || !v.can.pause ? (v.can.resume ? ['play', 'Продолжить'] : null) : ['pause', 'Пауза'];
      break;
    case 'segment': {
      const over = p.remaining_ms < 0;
      if (p.stopwatch) {
        frac = p.dur_ms ? Math.min(1, p.elapsed_ms / p.dur_ms) : 0;
        setTime(mmss(p.elapsed_ms));
        label = `${p.title} · без времени`;
        btn = ['check', `Закончил ${p.title.toLowerCase()}`];
        break;
      }
      frac = over ? 1 : p.elapsed_ms / p.dur_ms;
      setTime(over ? `+${mmss(-p.remaining_ms)}` : mmss(p.remaining_ms));
      if (p.prep) {
        label = over ? `${p.title} · пора ложиться` : `${p.title} · подготовка`;
        btn = ['bed', 'Лёг'];
        break;
      }
      label = over ? `${p.title} · превышено` : p.title;
      btn = p.alarm ? ['alarm', 'Встал'] : ['check', `Закончил ${p.title.toLowerCase()}`];
      break;
    }
    case 'await':
      setTime(p.subtitle.includes('часть') ? `Часть ${p.subtitle.match(/часть (\d+)/)?.[1] ?? ''}` : 'Дальше');
      label = `Перерыв окончен · ${mmss(p.waiting_ms)}`;
      btn = ['play', 'Начать следующую часть'];
      frac = 1;
      break;
    case 'lunch':
      setTime(mmss(p.elapsed_ms));
      label = 'Обед · нажми, когда поешь';
      btn = ['play', 'Пообедал — начать'];
      break;
    case 'done':
      setTime('Готово');
      label = `${Math.round(v.work_ms / 60000)} мин учёбы · блокировка снята`;
      break;
    default:
      setTime(v.can.start_day ? 'Старт' : '--:--');
      label = v.can.start_day ? 'Начать учебный день' : p.title;
      btn = v.can.start_day ? ['play', 'Начать день'] : null;
  }
  el('bar').style.strokeDashoffset = String(100 - Math.min(1, Math.max(0, frac)) * 100);
  el('label').textContent = label;
  el('label').title = label;
  b.hidden = !btn;
  const sig = btn ? btn[1] : '';
  if (btnSig && sig !== btnSig) shieldUntil = performance.now() + 900;
  btnSig = sig;
  if (btn) {
    if (!morphIcon(b, btn[0])) b.innerHTML = icon(btn[0]);
    b.setAttribute('aria-label', btn[1]);
    b.title = btn[1];
  }
}

root.addEventListener('mousedown', startDrag);
el('primary').addEventListener('click', () => {
  if (performance.now() < shieldUntil) { shieldUntil = performance.now() + 700; return; }
  call('primary').catch(console.warn);
});
el('open').addEventListener('click', () => call('show_main', { route: 'today' }));
el('close').addEventListener('click', () => call('toggle_mini'));
root.addEventListener('dblclick', (e) => { if (!e.target.closest('button')) call('show_main', { route: 'today' }); });

on('state', render);
call('get_state').then(render);
