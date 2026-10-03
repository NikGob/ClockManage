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
// The alarm pops up over whatever the user is doing: a click or Enter that was meant for the
// window underneath must not press "Начать часть" the instant it appears.
const ARM_MS = 900;
let armedAt = 0;

installRipples();

const ART_CLASS = { await: 'ringing', wake: 'ringing', break: 'steaming', segment: 'steaming', access: 'locking', nope: 'wagging' };

function render(p) {
  current = p;
  clearTimeout(hideTimer);
  stopBoil();
  const passive = !!p.passive;
  scrim.className = `scrim ${passive ? 'soft' : 'full'}${p.kind === 'wake' ? ' alarm' : ''}`;
  const center = p.kind === 'nope';
  stage.classList.toggle('top', passive && !center);
  requestAnimationFrame(() => scrim.classList.add('show'));

  const actions = [];
  if (p.kind === 'wake') {
    // Only "Встал" stops the alarm: no "hide", no Escape.
    actions.push(`<button class="btn filled xl interactive" data-act="end_segment">${icon('alarm')}${esc(p.action || 'Встал')}</button>`);
  } else if (p.kind === 'segment') {
    actions.push(`<button class="btn filled xl interactive" data-act="end_segment">${icon('check')}${esc(p.action || 'Закончил')}</button>`);
    actions.push('<button class="btn text lg interactive" data-act="hide">Ещё немного</button>');
  } else if (p.kind === 'ask') {
    (p.types || []).forEach((t) => actions.push(`<button class="btn tonal lg interactive" data-act="seg" data-name="${esc(t.name)}">${esc(t.name)} · ${t.minutes} мин</button>`));
    actions.push('<button class="btn text lg interactive" data-act="hide">Перерыв</button>');
  } else if (p.ask_note) {
    actions.push(`<button class="btn filled lg interactive" data-act="note">${icon('check')}Сохранить</button>`);
    actions.push('<button class="btn text lg interactive" data-act="skip">Пропустить</button>');
  } else if (p.kind === 'await') {
    actions.push(`<button class="btn filled xl interactive" data-act="start">${icon('play')}${esc(p.action || 'Начать')}</button>`);
    actions.push(`<button class="btn text lg interactive" data-act="hide">Скрыть на минуту</button>`);
  } else if (!passive) {
    actions.push(`<button class="btn filled lg interactive" data-act="hide">${icon('check')}Понятно</button>`);
  }

  stage.innerHTML = `
    <section class="card ${center ? 'bubble' : passive ? 'passive' : ''}" data-kind="${esc(p.kind)}" role="${passive ? 'status' : 'alertdialog'}" aria-labelledby="ov-title">
      <div class="art ${ART_CLASS[p.kind] || ''}">${doodle(p.kind, center ? 200 : passive ? 112 : 260)}</div>
      <h1 id="ov-title">${esc(p.title)}</h1>
      ${p.text ? `<p class="sub">${esc(p.text)}</p>` : ''}
      ${p.kind === 'await' ? '<p class="waiting tnum" id="waiting"></p>' : ''}
      ${p.note ? `<p class="note">${esc(p.note)}</p>` : ''}
      ${p.ask_note ? `<div class="field note-field"><label for="ov-note">Что было скучно, куда отвлекался? Одна строка, можно пропустить</label>
        <input id="ov-note" maxlength="300" autocomplete="off" spellcheck="true"></div>` : ''}
      ${actions.length ? `<div class="actions">${actions.join('')}</div>` : ''}
      ${p.preview ? '<p class="preview">Предпросмотр — так будет выглядеть оповещение</p>' : ''}
    </section>`;
  const card = stage.firstElementChild;
  [...card.children].forEach((el, i) => el.style.setProperty('--i', i));
  requestAnimationFrame(() => card.classList.add('in'));
  stopBoil = play(stage, { extraMs: { await: 3600, break: 1800 }[p.kind] ?? 1200, jitter: p.kind !== 'nope' });
  updateWaiting();
  armedAt = performance.now() + ARM_MS;
  const primary = stage.querySelector('#ov-note') || stage.querySelector('[data-act="start"]') || stage.querySelector('[data-act="hide"]');
  if (primary && !passive) setTimeout(() => primary.focus({ preventScroll: true }), ARM_MS);
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
  if (performance.now() < armedAt) return;
  if (b.dataset.act === 'end_segment' || b.dataset.act === 'seg') {
    b.setAttribute('aria-busy', 'true');
    try {
      if (!current?.preview && !current?.demo) {
        if (b.dataset.act === 'seg') await call('start_segments', { items: [{ name: b.dataset.name }] });
        else await call('end_segment');
      }
    } catch (err) {
      console.warn(err);
    }
    hide();
  } else if (b.dataset.act === 'note' || b.dataset.act === 'skip') {
    await saveNote(b.dataset.act === 'note' ? document.getElementById('ov-note')?.value : null);
  } else if (b.dataset.act === 'start') {
    b.setAttribute('aria-busy', 'true');
    try {
      if (!current?.preview) await call('start_next', { expect: 'await' });
    } catch (err) {
      console.warn(err);
    }
    hide();
  } else {
    hide();
  }
});

async function saveNote(text) {
  if (!current?.preview && !current?.demo) {
    try {
      await call('set_block_note', { block: current.block, note: text?.trim() ? text : null });
    } catch (err) {
      console.warn(err);
    }
  }
  hide();
}

window.addEventListener('keydown', (e) => {
  if (!current || current.passive) return;
  if (e.key === 'Enter' && e.target.id === 'ov-note' && performance.now() >= armedAt) { e.preventDefault(); saveNote(e.target.value); return; }
  // Escape only hides the card: the line still waits on the "Сегодня" screen.
  if (e.key === 'Escape' && current.kind !== 'wake') hide();
});

on('overlay', (p) => render(p));
on('state', (s) => {
  lastView = s.view;
  applyTheme({ seed: s.meta.seed, mode: s.meta.theme_mode, variant: s.meta.variant });
  updateWaiting();
  // Started from elsewhere (tray, mini window) — the alarm is no longer relevant.
  if (current?.kind === 'await' && !current.preview && !current.demo && s.view.phase.kind !== 'await') hide();
  if (['wake', 'segment'].includes(current?.kind) && !current.preview && !current.demo && s.view.phase.kind !== 'segment') hide();
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
