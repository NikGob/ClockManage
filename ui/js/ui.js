// Shared UI helpers: snackbar, dialogs, number formatting.
import { esc } from './api.js';

let snackTimer = 0;

export function snack(text, ms = 3600) {
  const el = document.getElementById('snackbar');
  document.getElementById('snack-text').textContent = text;
  el.classList.add('show');
  clearTimeout(snackTimer);
  snackTimer = setTimeout(() => el.classList.remove('show'), ms);
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
