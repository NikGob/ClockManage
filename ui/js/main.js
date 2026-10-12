import { call, on, installRipples, esc } from './api.js';
import { snack, stagger, staggerFirstRender } from './ui.js';
import { applyTheme } from './theme.js';
import { installTooltips, hideTip } from './tooltip.js';
import { icon } from './icons.js';
import { mountToday } from './views/today.js';
import { mountPlan } from './views/plan.js';
import { mountBlock } from './views/block.js';
import { mountLog } from './views/log.js';
import { mountSettings } from './views/settings.js';
import { L, followLang } from './i18n.js';

const ROUTES = [
  { id: 'today', label: L('Сегодня', 'Today'), icon: 'timer', title: L('Сегодня', 'Today'), mount: mountToday },
  { id: 'plan', label: L('План', 'Plan'), icon: 'list', title: L('План дня', 'Day plan'), mount: mountPlan },
  { id: 'block', label: L('Блокировка', 'Blocking'), icon: 'lock', title: L('Блокировка', 'Blocking'), mount: mountBlock },
  { id: 'log', label: L('Журнал', 'Log'), icon: 'history', title: L('Журнал', 'Log'), mount: mountLog },
  { id: 'settings', label: L('Настройки', 'Settings'), icon: 'settings', title: L('Настройки', 'Settings'), mount: mountSettings },
];

const nav = document.getElementById('nav');
let view = document.getElementById('view');
const title = document.getElementById('screen-title');
const actions = document.getElementById('top-actions');
const banner = document.getElementById('banner');
nav.setAttribute('aria-label', L('Разделы', 'Sections'));

let current = null;
let screen = null;
let snapshot = null;
const ctx = {
  navigate: (id) => go(id),
  setActions: (html) => { actions.innerHTML = html; },
  onAction: null,
};

installRipples();
installTooltips();

nav.innerHTML = ROUTES.map((r) => `
  <button class="dest" data-route="${r.id}" aria-label="${r.label}">
    <span class="pill interactive">${icon(r.icon)}<span class="badge" hidden></span></span><span>${r.label}</span>
  </button>`).join('')
;

// One shared indicator that slides (and stretches like a drop) between destinations.
const ind = document.createElement('span');
ind.className = 'nav-ind';
nav.prepend(ind);
let indAt = null;
function moveIndicator(animate) {
  const pill = nav.querySelector('[aria-current="page"] .pill');
  if (!pill) return;
  const n = nav.getBoundingClientRect();
  const r = pill.getBoundingClientRect();
  const to = { x: r.left - n.left, y: r.top - n.top, w: r.width, h: r.height };
  const css = (b) => ({ transform: `translate(${b.x}px, ${b.y}px)`, width: `${b.w}px`, height: `${b.h}px` });
  Object.assign(ind.style, css(to));
  const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (animate && indAt && !reduce) {
    const a = indAt;
    // Midway the drop spans both destinations, then snaps onto the new one.
    const x0 = Math.min(a.x, to.x), y0 = Math.min(a.y, to.y);
    const mid = { x: x0, y: y0, w: Math.max(a.x + a.w, to.x + to.w) - x0, h: Math.max(a.y + a.h, to.y + to.h) - y0 };
    ind.animate([css(a), { ...css(mid), offset: 0.4 }, css(to)], { duration: 460, easing: 'cubic-bezier(.2, 0, 0, 1)' });
  }
  indAt = to;
}
addEventListener('resize', () => moveIndicator(false));

nav.addEventListener('click', (e) => {
  const b = e.target.closest('[data-route]');
  if (b) go(b.dataset.route);
});
nav.addEventListener('keydown', (e) => {
  if (!['ArrowDown', 'ArrowUp', 'ArrowLeft', 'ArrowRight'].includes(e.key)) return;
  const items = [...nav.querySelectorAll('[data-route]')];
  const i = items.indexOf(document.activeElement);
  if (i < 0) return;
  const d = e.key === 'ArrowDown' || e.key === 'ArrowRight' ? 1 : -1;
  items[(i + d + items.length) % items.length].focus();
  e.preventDefault();
});
actions.addEventListener('click', (e) => ctx.onAction?.(e));

function go(id) {
  const r = ROUTES.find((x) => x.id === id) || ROUTES[0];
  if (current === r.id) return;
  current = r.id;
  nav.querySelectorAll('[data-route]').forEach((b) => {
    if (b.dataset.route === r.id) b.setAttribute('aria-current', 'page');
    else b.removeAttribute('aria-current');
  });
  moveIndicator(true);
  title.textContent = r.title;
  hideTip();
  actions.innerHTML = '';
  ctx.onAction = null;
  // A fresh container per screen: listeners of the previous screen go away with it
  // (reusing one element stacked a click handler per visit — one click opened N dialogs).
  const fresh = view.cloneNode(false);
  view.replaceWith(fresh);
  view = fresh;
  view.classList.add('enter');
  screen = r.mount(view, ctx);
  screen.show?.();
  if (snapshot) screen.update(snapshot);
  // Contents rise in a cascade: what is there now, and the first render of screens that load
  // their data first (caught on insertion, before it is ever painted).
  // A screen drawn right away is done; only an empty one waits for its first render
  // (otherwise its regular re-renders would be animated again).
  if (!stagger(view)) staggerFirstRender(view);
  try { localStorage.setItem('route', r.id); } catch { /* storage may be unavailable */ }
}

function render(s) {
  followLang(s.meta.lang);
  snapshot = s;
  applyTheme({ seed: s.meta.seed, mode: s.meta.theme_mode, variant: s.meta.variant });
  // Attention badge on "Сегодня" when the timer waits for a click.
  const badge = nav.querySelector('[data-route="today"] .badge');
  if (badge) badge.hidden = s.view.phase.kind !== 'await';
  const problems = [];
  if (!s.meta.admin && s.view.lock.base) problems.push(L('Программа запущена без прав администратора — сайты и приложения не блокируются. Запусти от имени администратора.', 'ClockManage runs without administrator rights — sites and apps are not blocked. Run it as administrator.'));
  if (s.meta.blocker_error) problems.push(s.meta.blocker_error);
  banner.hidden = !problems.length;
  if (problems.length) banner.innerHTML = `${icon('warning')}<span>${esc(problems.join(' '))}</span>`;
  document.title = s.view.phase.kind === 'work' || s.view.phase.kind === 'break'
    ? `${s.view.phase.title} — ClockManage` : 'ClockManage';
  screen?.update(s);
}

on('state', render);
on('config', () => screen?.config?.());
on('navigate', (id) => go(id));
// Changes made by the agent through MCP, so it is clear they were not mine.
on('agent', (p) => snack(`${p.title}: ${p.text}`, 7000));

(async () => {
  let start = 'today';
  try { start = new URLSearchParams(location.search).get('route') || localStorage.getItem('route') || 'today'; } catch { /* ignore */ }
  go(start);
  render(await call('get_state'));
})();
