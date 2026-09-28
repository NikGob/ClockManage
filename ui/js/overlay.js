import { call, on, esc, mmss, installRipples } from './api.js';
import { applyTheme } from './theme.js';
import { doodle, play } from './doodles.js';
import { icon } from './icons.js';

const stage = document.getElementById('stage');
const scrim = document.getElementById('scrim');
let hideTimer = 0;
let stopBoil = () => {};
let current = null;
let lastView = null;

installRipples();

const ART_CLASS = { await: 'ringing', break: 'steaming', access: 'locking' };

function render(p) {
  current = p;
  clearTimeout(hideTimer);
  stopBoil();
  const passive = !!p.passive;
  scrim.className = `scrim ${passive ? 'soft' : 'full'}`;
  stage.classList.toggle('top', passive);
  requestAnimationFrame(() => scrim.classList.add('show'));

  const actions = [];
  if (p.kind === 'await') {
    actions.push(`<button class="btn filled xl interactive" data-act="start">${icon('play')}${esc(p.action || 'Начать')}</button>`);
    actions.push(`<button class="btn text lg interactive" data-act="hide">Скрыть на минуту</button>`);
  } else if (!passive) {
    actions.push(`<button class="btn filled lg interactive" data-act="hide">${icon('check')}Понятно</button>`);
  }

  stage.innerHTML = `
    <section class="card ${passive ? 'passive' : ''}" role="${passive ? 'status' : 'alertdialog'}" aria-labelledby="ov-title">
      <div class="art ${ART_CLASS[p.kind] || ''}">${doodle(p.kind, passive ? 112 : 260)}</div>
      <h1 id="ov-title">${esc(p.title)}</h1>
      ${p.text ? `<p class="sub">${esc(p.text)}</p>` : ''}
      ${p.kind === 'await' ? '<p class="waiting tnum" id="waiting"></p>' : ''}
      ${p.note ? `<p class="note">${esc(p.note)}</p>` : ''}
      ${actions.length ? `<div class="actions">${actions.join('')}</div>` : ''}
      ${p.preview ? '<p class="preview">Предпросмотр — так будет выглядеть оповещение</p>' : ''}
    </section>`;
  const card = stage.firstElementChild;
  requestAnimationFrame(() => card.classList.add('in'));
  stopBoil = play(stage);
  updateWaiting();
  const primary = stage.querySelector('[data-act="start"]') || stage.querySelector('[data-act="hide"]');
  if (primary && !passive) setTimeout(() => primary.focus({ preventScroll: true }), 350);
  if (passive) hideTimer = setTimeout(hide, p.auto_hide_ms || 5000);
}

function hide() {
  clearTimeout(hideTimer);
  stopBoil();
  const card = stage.firstElementChild;
  scrim.classList.remove('show');
  if (card) {
    card.classList.remove('in');
    card.classList.add('out');
  }
  setTimeout(() => {
    stage.innerHTML = '';
    current = null;
    call('hide_overlay');
  }, 230);
}

function updateWaiting() {
  const el = document.getElementById('waiting');
  if (!el || !lastView) return;
  const ms = lastView.phase.kind === 'await' ? lastView.phase.waiting_ms : 0;
  el.textContent = ms > 5000 ? `Ждём ${mmss(ms)} — это время не идёт в учёбу, блокировка включена` : 'Блокировка включена, пока не начнёшь';
}

stage.addEventListener('click', async (e) => {
  const b = e.target.closest('[data-act]');
  if (!b) return;
  if (b.dataset.act === 'start') {
    b.setAttribute('aria-busy', 'true');
    try {
      if (!current?.preview) await call('start_next');
    } catch (err) {
      console.warn(err);
    }
    hide();
  } else {
    hide();
  }
});

window.addEventListener('keydown', (e) => {
  if (!current || current.passive) return;
  if (e.key === 'Escape') hide();
});

on('overlay', (p) => render(p));
on('state', (s) => {
  lastView = s.view;
  applyTheme({ seed: s.meta.seed, mode: s.meta.theme_mode, variant: s.meta.variant });
  updateWaiting();
  // Started from elsewhere (tray, mini window) — the alarm is no longer relevant.
  if (current?.kind === 'await' && !current.preview && !current.demo && s.view.phase.kind !== 'await') hide();
});

(async () => {
  const s = await call('get_state');
  lastView = s.view;
  applyTheme({ seed: s.meta.seed, mode: s.meta.theme_mode, variant: s.meta.variant });
  const p = await call('get_overlay');
  const q = new URLSearchParams(location.search).get('demo');
  if (q) render(await (await import('./mock.js')).overlayDemo(q));
  else if (p && window.__TAURI__) render(p);
})();
