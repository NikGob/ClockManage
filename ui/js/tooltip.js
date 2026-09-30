// M3 tooltips for anything with `data-tip` (or a native `title`, which is taken over, or an
// icon button's aria-label). `data-tip-title` makes it a rich tooltip with a heading.
// Shown after a short hover or on keyboard focus; once one is open, neighbours show instantly.

let el = null;
let anchor = null;
let timer = 0;
let warmUntil = 0;

const SEL = '[data-tip], [title], .icon-btn[aria-label], .sos[aria-label]';

function tipOf(a) {
  if (a.hasAttribute('title')) {
    if (!a.dataset.tip) a.dataset.tip = a.getAttribute('title');
    a.removeAttribute('title');
  }
  return a.dataset.tip || a.getAttribute('aria-label') || '';
}

function place(a) {
  const r = a.getBoundingClientRect();
  const w = el.offsetWidth;
  const h = el.offsetHeight;
  const gap = 8;
  let top = r.top - h - gap;
  let origin = 'bottom';
  if (top < 8) { top = r.bottom + gap; origin = 'top'; }
  const left = Math.min(Math.max(8, r.left + r.width / 2 - w / 2), innerWidth - w - 8);
  el.style.left = `${Math.round(left)}px`;
  el.style.top = `${Math.round(top)}px`;
  el.style.transformOrigin = `${Math.round(r.left + r.width / 2 - left)}px ${origin}`;
}

function show(a) {
  const text = tipOf(a);
  if (!text || !a.isConnected) return;
  anchor = a;
  const title = a.dataset.tipTitle;
  el.className = `tooltip${title ? ' rich' : ''}`;
  el.textContent = '';
  if (title) {
    const b = document.createElement('b');
    b.textContent = title;
    el.append(b);
  }
  el.append(text);
  a.setAttribute('aria-describedby', 'tooltip');
  place(a);
  requestAnimationFrame(() => el.classList.add('show'));
}

export function hideTip() {
  clearTimeout(timer);
  if (anchor) {
    anchor.removeAttribute('aria-describedby');
    warmUntil = performance.now() + 500;
  }
  anchor = null;
  el?.classList.remove('show');
}

function schedule(a, delay) {
  clearTimeout(timer);
  if (anchor && anchor !== a) hideTip();
  const d = performance.now() < warmUntil ? 0 : delay;
  timer = setTimeout(() => show(a), d);
}

export function installTooltips(root = document) {
  el = document.createElement('div');
  el.id = 'tooltip';
  el.className = 'tooltip';
  el.setAttribute('role', 'tooltip');
  document.body.appendChild(el);
  root.addEventListener('pointerover', (e) => {
    if (e.pointerType === 'touch') return;
    const a = e.target.closest?.(SEL);
    if (a === anchor && a) return;
    if (!a || a.disabled && !a.dataset.tip) { hideTip(); return; }
    tipOf(a);
    schedule(a, a.dataset.tipTitle ? 650 : 450);
  });
  root.addEventListener('pointerout', (e) => {
    const a = e.target.closest?.(SEL);
    if (a && !a.contains(e.relatedTarget)) { clearTimeout(timer); if (a === anchor) hideTip(); }
  });
  root.addEventListener('focusin', (e) => {
    const a = e.target.closest?.(SEL);
    if (a && e.target.matches(':focus-visible')) schedule(a, 250);
  });
  root.addEventListener('focusout', hideTip);
  root.addEventListener('pointerdown', hideTip, true);
  root.addEventListener('keydown', (e) => { if (e.key === 'Escape') hideTip(); });
  addEventListener('scroll', hideTip, true);
  addEventListener('blur', hideTip);
}
