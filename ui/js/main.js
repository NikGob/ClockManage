import { call, on, installRipples, esc } from './api.js';
import { snack } from './ui.js';
import { applyTheme } from './theme.js';
import { installTooltips, hideTip } from './tooltip.js';
import { icon } from './icons.js';
import { mountToday } from './views/today.js';
import { mountPlan } from './views/plan.js';
import { mountBlock } from './views/block.js';
import { mountLog } from './views/log.js';
import { mountSettings } from './views/settings.js';

const ROUTES = [
  { id: 'today', label: 'Сегодня', icon: 'timer', title: 'Сегодня', mount: mountToday },
  { id: 'plan', label: 'План', icon: 'list', title: 'План дня', mount: mountPlan },
  { id: 'block', label: 'Блокировка', icon: 'lock', title: 'Блокировка', mount: mountBlock },
  { id: 'log', label: 'Журнал', icon: 'history', title: 'Журнал', mount: mountLog },
  { id: 'settings', label: 'Настройки', icon: 'settings', title: 'Настройки', mount: mountSettings },
];

const nav = document.getElementById('nav');
let view = document.getElementById('view');
const title = document.getElementById('screen-title');
const actions = document.getElementById('top-actions');
const banner = document.getElementById('banner');

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
  try { localStorage.setItem('route', r.id); } catch { /* storage may be unavailable */ }
}

function render(s) {
  snapshot = s;
  applyTheme({ seed: s.meta.seed, mode: s.meta.theme_mode, variant: s.meta.variant });
  // Attention badge on "Сегодня" when the timer waits for a click.
  const badge = nav.querySelector('[data-route="today"] .badge');
  if (badge) badge.hidden = s.view.phase.kind !== 'await';
  const problems = [];
  if (!s.meta.admin && s.view.lock.base) problems.push('Программа запущена без прав администратора — сайты и приложения не блокируются. Запусти от имени администратора.');
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
