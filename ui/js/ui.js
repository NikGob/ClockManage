// Shared UI helpers: snackbar, dialogs, number formatting.
import { esc } from './api.js';

let snackTimer = 0;
let snackCount = 0;

/**
 * Bottom message. `action` = { label, run } adds a button (an "undo"); the bar then stays for
 * the whole `ms`, and the label counts the seconds down.
 */
export function snack(text, ms = 3600, action = null) {
  const el = document.getElementById('snackbar');
  document.getElementById('snack-text').textContent = text;
  const btn = document.getElementById('snack-act');
  clearInterval(snackCount);
  if (btn) {
    btn.hidden = !action;
    btn.onclick = null;
    if (action) {
      const until = Date.now() + ms;
      const label = () => { btn.textContent = `${action.label} · ${Math.max(0, Math.ceil((until - Date.now()) / 1000))}`; };
      label();
      snackCount = setInterval(label, 250);
      btn.onclick = () => {
        clearInterval(snackCount);
        el.classList.remove('show');
        action.run();
      };
    }
  }
  // A popover lives in the top layer: re-opening it puts it above any modal dialog, so an
  // error raised from inside a dialog is actually seen (it used to hide under the backdrop).
  if (el.showPopover) {
    try {
      if (el.matches(':popover-open')) el.hidePopover();
      el.classList.remove('show');
      el.showPopover();
      void el.offsetWidth;
    } catch { /* not supported: plain fixed element */ }
  }
  el.classList.add('show');
  clearTimeout(snackTimer);
  snackTimer = setTimeout(() => { el.classList.remove('show'); clearInterval(snackCount); }, ms);
}

/** Wrap a command so failures surface as a snackbar and buttons show pending state. */
export async function run(fn, btn) {
  if (btn) btn.setAttribute('aria-busy', 'true');
  try {
    return await fn();
  } catch (e) {
    snack(typeof e === 'string' ? e : e?.message || String(e));
    return undefined;
  } finally {
    if (btn) btn.removeAttribute('aria-busy');
  }
}

/**
 * Open an M3 dialog. `build(dlg, close)` fills it; resolves with the value passed to close().
 * Focus returns to the element that opened it.
 */
export function dialog(html, build) {
  return new Promise((resolve) => {
    const opener = document.activeElement;
    const d = document.createElement('dialog');
    d.className = 'm3';
    d.innerHTML = `<form method="dialog" class="dlg">${html}</form>`;
    document.body.appendChild(d);
    let done = false;
    const close = (value) => {
      if (done) return;
      done = true;
      d.classList.add('closing');
      setTimeout(() => {
        d.close();
        d.remove();
        if (opener && opener.focus) opener.focus({ preventScroll: true });
        resolve(value);
      }, 150);
    };
    d.addEventListener('cancel', (e) => { e.preventDefault(); close(undefined); });
    d.addEventListener('click', (e) => { if (e.target === d) close(undefined); });
    d.querySelectorAll('[data-close]').forEach((b) => b.addEventListener('click', (e) => { e.preventDefault(); close(undefined); }));
    d.querySelector('form').addEventListener('submit', (e) => e.preventDefault());
    build?.(d, close);
    d.showModal();
    const first = d.querySelector('[autofocus]') || d.querySelector('input, textarea, button:not([data-close])');
    first?.focus();
  });
}

/** Yes/no M3 dialog. Resolves true when confirmed. */
export function ask(title, text, ok = 'Да', cancel = 'Отмена') {
  return dialog(`<h2>${esc(title)}</h2><p class="body-m muted">${esc(text)}</p>
    <div class="actions"><button class="btn text interactive" data-close>${esc(cancel)}</button><button class="btn filled interactive" data-ok autofocus>${esc(ok)}</button></div>`,
  (d, close) => d.querySelector('[data-ok]').addEventListener('click', () => close(true))).then((v) => v === true);
}

/**
 * Press-and-hold confirmation: `onDone` runs only after holding `btn` for `ms` (pointer or
 * Space/Enter). A `.fillbar` child shows the progress. A plain click does nothing.
 */
export function holdButton(btn, ms, onDone) {
  const bar = btn.querySelector('.fillbar');
  let start = 0;
  let raf = 0;
  const stop = () => {
    cancelAnimationFrame(raf);
    start = 0;
    if (bar) { bar.style.transition = 'transform 200ms'; bar.style.transform = 'scaleX(0)'; }
  };
  const frame = () => {
    const t = Math.min(1, (Date.now() - start) / ms);
    if (bar) { bar.style.transition = 'none'; bar.style.transform = `scaleX(${t})`; }
    if (t < 1) { raf = requestAnimationFrame(frame); return; }
    stop();
    onDone();
  };
  const begin = (e) => {
    if (btn.disabled || start) return;
    e.preventDefault();
    start = Date.now();
    raf = requestAnimationFrame(frame);
  };
  btn.addEventListener('pointerdown', begin);
  btn.addEventListener('keydown', (e) => { if ((e.key === ' ' || e.key === 'Enter') && !e.repeat) begin(e); });
  ['pointerup', 'pointerleave', 'keyup', 'blur'].forEach((ev) => btn.addEventListener(ev, () => { if (start) stop(); }));
  btn.addEventListener('click', (e) => e.preventDefault());
}

export function hoursLabel(min) {
  const h = min / 60;
  if (min % 60 === 0) return `${h} ч`;
  if (min < 60) return `${min} мин`;
  return `${String(Math.round(h * 100) / 100).replace('.', ',')} ч`;
}

export function partsPreview(min, seg) {
  const parts = [];
  let done = 0;
  const merge = 15;
  while (done < min) {
    const rem = min - done;
    const tail = rem - seg;
    const s = tail > 0 && tail < merge ? rem : Math.min(seg, rem);
    parts.push(s);
    done += s;
  }
  return parts;
}

export const WEEKDAYS = ['Пн', 'Вт', 'Ср', 'Чт', 'Пт', 'Сб', 'Вс'];
export const WEEKDAYS_FULL = ['понедельник', 'вторник', 'среда', 'четверг', 'пятница', 'суббота', 'воскресенье'];

export function dateLabel(iso) {
  const [y, m, d] = iso.split('-').map(Number);
  const dt = new Date(y, m - 1, d);
  const wd = WEEKDAYS_FULL[(dt.getDay() + 6) % 7];
  return `${dt.toLocaleDateString('ru-RU', { day: 'numeric', month: 'long' })} · ${wd}`;
}

export { esc };

/**
 * M3 menu anchored to `anchor`. items: [{ value, label, sub?, icon?, selected?, disabled?, tip? }]
 * (or the string '-' for a divider). Resolves with the chosen value, or undefined when dismissed.
 */
export function menu(anchor, items, { title = '', note = '' } = {}) {
  return new Promise((resolve) => {
    // A second click on the anchor closes its menu (toggle).
    if (anchor._menuClose) { anchor._menuClose(); resolve(undefined); return; }
    document.querySelector('.menu:not(.closing)')?._close?.();
    const m = document.createElement('div');
    m.className = 'menu';
    m.setAttribute('role', 'menu');
    m.innerHTML = (title ? `<div class="menu-title">${esc(title)}</div>` : '')
      + items.map((it, i) => (it === '-' ? '<div class="menu-div" role="separator"></div>' : `
        <button class="menu-item interactive" role="menuitemradio" aria-checked="${!!it.selected}" data-i="${i}"
          ${it.disabled ? 'aria-disabled="true"' : ''} ${it.tip ? `data-tip="${esc(it.tip)}"` : ''} style="--n:${i}">
          ${it.icon ? `<span class="lead">${it.icon}</span>` : ''}
          <span class="txt"><span class="l">${esc(it.label)}</span>${it.sub ? `<span class="s">${esc(it.sub)}</span>` : ''}</span>
          ${it.selected ? '<span class="trail"><svg class="ck" viewBox="0 0 24 24" aria-hidden="true"><path pathLength="1" d="M5 12.5l4.5 4.5L19 7.5"/></svg></span>' : ''}
        </button>`)).join('')
      + (note ? `<div class="menu-note">${esc(note)}</div>` : '');
    document.body.appendChild(m);
    const r = anchor.getBoundingClientRect();
    const w = m.offsetWidth;
    const h = m.offsetHeight;
    const below = r.bottom + 4 + h < innerHeight - 8 || r.top - 4 - h < 8;
    m.style.left = `${Math.round(Math.min(Math.max(8, r.left), innerWidth - w - 8))}px`;
    m.style.top = `${Math.round(below ? r.bottom + 4 : r.top - 4 - h)}px`;
    m.style.transformOrigin = below ? 'top left' : 'bottom left';
    requestAnimationFrame(() => m.classList.add('open'));
    anchor.setAttribute('aria-expanded', 'true');

    let done = false;
    const close = (value) => {
      if (done) return;
      done = true;
      anchor._menuClose = null;
      anchor.removeAttribute('aria-expanded');
      document.removeEventListener('pointerdown', outside, true);
      removeEventListener('blur', dismiss);
      removeEventListener('resize', dismiss);
      m.classList.remove('open');
      m.classList.add('closing');
      setTimeout(() => m.remove(), 160);
      if (value === undefined) anchor.focus?.({ preventScroll: true });
      resolve(value);
    };
    const dismiss = () => close(undefined);
    const outside = (e) => { if (!m.contains(e.target) && !anchor.contains(e.target)) close(undefined); };
    m._close = dismiss;
    anchor._menuClose = dismiss;
    document.addEventListener('pointerdown', outside, true);
    addEventListener('blur', dismiss);
    addEventListener('resize', dismiss);

    const buttons = [...m.querySelectorAll('.menu-item')];
    m.addEventListener('click', (e) => {
      const b = e.target.closest('.menu-item');
      if (!b || b.getAttribute('aria-disabled') === 'true') return;
      b.classList.add('chosen');
      setTimeout(() => close(items[Number(b.dataset.i)].value), 120);
    });
    m.addEventListener('keydown', (e) => {
      const i = buttons.indexOf(document.activeElement);
      if (e.key === 'Escape') { e.preventDefault(); close(undefined); }
      else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const d = e.key === 'ArrowDown' ? 1 : -1;
        buttons[(i + d + buttons.length) % buttons.length]?.focus();
      } else if (e.key === 'Tab') close(undefined);
    });
    (buttons.find((b) => b.getAttribute('aria-checked') === 'true') || buttons[0])?.focus({ preventScroll: true });
  });
}

export const KINDS = [
  { id: 'full', label: 'Полный', icon: 'bolt', hint: 'Полный учебный день' },
  { id: 'light', label: 'Лёгкий', icon: 'eco', hint: 'Облегчённый день — свой план' },
  { id: 'off', label: 'Выходной', icon: 'weekend', hint: 'Отдых — по умолчанию без блокировки' },
];
export const KIND_RANK = { off: 0, light: 1, full: 2 };
export const kindLabel = (id) => KINDS.find((k) => k.id === id)?.label || id;

export function hm(min) {
  return `${String(Math.floor(min / 60)).padStart(2, '0')}:${String(min % 60).padStart(2, '0')}`;
}

/** "Математика 1,5 ч · Словацкий 1 ч" / "5 ч 30 мин" summary of a plan. */
export function planSummary(plan) {
  if (!plan?.length) return 'Плана нет';
  const total = plan.reduce((a, b) => a + b.minutes, 0);
  const h = Math.floor(total / 60);
  const m = total % 60;
  return `${h ? `${h} ч` : ''}${h && m ? ' ' : ''}${m ? `${m} мин` : ''} · ${plan.map((b) => b.name).join(', ')}`;
}
