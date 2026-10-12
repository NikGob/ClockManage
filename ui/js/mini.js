import { call, on, mmss, startDrag, installRipples } from './api.js';
import { applyTheme } from './theme.js';
import { icon, morphIcon } from './icons.js';
import { L, followLang } from './i18n.js';

const el = (id) => document.getElementById(id);
const root = el('mini');
installRipples();
document.title = L('ClockManage — мини-таймер', 'ClockManage — mini timer');
for (const [id, ru, en] of [['open', 'Открыть приложение', 'Open the app'], ['close', 'Скрыть мини-таймер', 'Hide the mini timer']]) {
  el(id).setAttribute('aria-label', L(ru, en));
  el(id).title = L(ru, en);
}
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
  followLang(s.meta.lang);
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
        const part = p.subtitle.match(/(\d+) (?:из|of) (\d+)/);
        const short = part ? `${part[1]}/${part[2]}` : '';
        label = p.kind === 'work' ? `${p.title}${short ? ' · ' + short : ''}` : p.title;
        if (p.paused) label = `${L('Пауза', 'Paused')} · ${label}`;
      }
      btn = p.paused || !v.can.pause ? (v.can.resume ? ['play', L('Продолжить', 'Resume')] : null) : ['pause', L('Пауза', 'Pause')];
      break;
    case 'segment': {
      const over = p.remaining_ms < 0;
      if (p.stopwatch) {
        frac = p.dur_ms ? Math.min(1, p.elapsed_ms / p.dur_ms) : 0;
        setTime(mmss(p.elapsed_ms));
        label = `${p.title} · ${L('без времени', 'untimed')}`;
        btn = ['check', L(`Закончил ${p.title.toLowerCase()}`, `Done with ${p.title.toLowerCase()}`)];
        break;
      }
      frac = over ? 1 : p.elapsed_ms / p.dur_ms;
      setTime(over ? `+${mmss(-p.remaining_ms)}` : mmss(p.remaining_ms));
      if (p.prep) {
        label = `${p.title} · ${over ? L('пора ложиться', 'time to lie down') : L('подготовка', 'getting ready')}`;
        btn = ['bed', L('Лёг', 'In bed')];
        break;
      }
      label = over ? `${p.title} · ${L('превышено', 'over')}` : p.title;
      btn = p.alarm ? ['alarm', L('Встал', 'I’m up')] : ['check', L(`Закончил ${p.title.toLowerCase()}`, `Done with ${p.title.toLowerCase()}`)];
      break;
    }
    case 'await':
      {
        const n = p.subtitle.match(/(?:часть|part) (\d+)/i)?.[1];
        setTime(n ? `${L('Часть', 'Part')} ${n}` : L('Дальше', 'Next'));
      }
      label = `${L('Перерыв окончен', 'Break over')} · ${mmss(p.waiting_ms)}`;
      btn = ['play', L('Начать следующую часть', 'Start the next part')];
      frac = 1;
      break;
    case 'lunch':
      setTime(mmss(p.elapsed_ms));
      label = L('Обед · нажми, когда поешь', 'Lunch · press when you’ve eaten');
      btn = ['play', L('Пообедал — начать', 'Done eating — start')];
      break;
    case 'done':
      setTime(L('Готово', 'Done'));
      label = L(`${Math.round(v.work_ms / 60000)} мин учёбы · блокировка снята`, `${Math.round(v.work_ms / 60000)} min of study · lock off`);
      break;
    default:
      setTime(v.can.start_day ? L('Старт', 'Start') : '--:--');
      label = v.can.start_day ? L('Начать учебный день', 'Start the study day') : p.title;
      btn = v.can.start_day ? ['play', L('Начать день', 'Start day')] : null;
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
