// Material 3 dynamic colour: scheme from a seed colour (official material-color-utilities).
import {
  Hct, argbFromHex, hexFromArgb, MaterialDynamicColors as C,
  SchemeFidelity, SchemeTonalSpot, SchemeVibrant,
} from '../vendor/mcu/index.js';

const ROLES = [
  'primary', 'onPrimary', 'primaryContainer', 'onPrimaryContainer', 'inversePrimary',
  'secondary', 'onSecondary', 'secondaryContainer', 'onSecondaryContainer',
  'tertiary', 'onTertiary', 'tertiaryContainer', 'onTertiaryContainer',
  'error', 'onError', 'errorContainer', 'onErrorContainer',
  'surface', 'onSurface', 'surfaceVariant', 'onSurfaceVariant', 'surfaceDim', 'surfaceBright',
  'surfaceContainerLowest', 'surfaceContainerLow', 'surfaceContainer',
  'surfaceContainerHigh', 'surfaceContainerHighest',
  'inverseSurface', 'inverseOnSurface', 'outline', 'outlineVariant', 'scrim', 'shadow',
];

const SURFACES = new Set([
  'surface', 'surfaceDim', 'surfaceBright', 'surfaceVariant', 'surfaceContainerLowest', 'surfaceContainerLow',
  'surfaceContainer', 'surfaceContainerHigh', 'surfaceContainerHighest',
]);
const LIGHT_SHIFT = 3.5;

const VARIANTS = { fidelity: SchemeFidelity, tonal_spot: SchemeTonalSpot, vibrant: SchemeVibrant };

const kebab = (s) => s.replace(/[A-Z]/g, (m) => '-' + m.toLowerCase());

export function schemeVars(seed, dark, variant = 'fidelity') {
  let argb;
  try { argb = argbFromHex(seed); } catch { argb = argbFromHex('#2E7D32'); }
  const Scheme = VARIANTS[variant] || SchemeFidelity;
  const s = new Scheme(Hct.fromInt(argb), dark, 0);
  const out = {};
  for (const r of ROLES) {
    let c = C[r].getArgb(s);
    // Light theme is a touch deeper than stock M3 so surfaces are not paper-white.
    if (!dark && SURFACES.has(r)) {
      const h = Hct.fromInt(c);
      c = Hct.from(s.neutralPalette.hue, Math.max(h.chroma, 2.5), h.tone - LIGHT_SHIFT).toInt();
    }
    out[`--md-sys-color-${kebab(r)}`] = hexFromArgb(c);
  }
  return out;
}

const block = (sel, vars) => `${sel}{${Object.entries(vars).map(([k, v]) => `${k}:${v}`).join(';')}}`;

let current = '';

/** Apply seed/mode/variant to the document. Cheap when nothing changed. */
export function applyTheme({ seed = '#2E7D32', mode = 'system', variant = 'fidelity' } = {}) {
  const key = `${seed}|${mode}|${variant}`;
  if (key === current) return;
  current = key;
  const light = schemeVars(seed, false, variant);
  const dark = schemeVars(seed, true, variant);
  let css;
  if (mode === 'light') css = block(':root', light) + ':root{color-scheme:light}';
  else if (mode === 'dark') css = block(':root', dark) + ':root{color-scheme:dark}';
  else {
    css = block(':root', light) + ':root{color-scheme:light dark}'
      + `@media (prefers-color-scheme: dark){${block(':root', dark)}}`;
  }
  let el = document.getElementById('dynamic-theme');
  if (!el) {
    el = document.createElement('style');
    el.id = 'dynamic-theme';
    document.head.appendChild(el);
  }
  el.textContent = css;
  document.documentElement.dataset.mode = mode;
  syncTitlebar();
}

export function isDark() {
  const mode = document.documentElement.dataset.mode || 'system';
  return mode === 'dark' || (mode === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
}

/** Let the native title bar blend with the window background (Windows 11). */
export function syncTitlebar() {
  const T = window.__TAURI__;
  if (!T) return;
  requestAnimationFrame(() => {
    const cs = getComputedStyle(document.documentElement);
    const bg = cs.getPropertyValue('--titlebar-bg').trim() || cs.getPropertyValue('--md-sys-color-surface').trim();
    const fg = cs.getPropertyValue('--md-sys-color-on-surface').trim();
    T.core.invoke('style_titlebar', { bg, fg, dark: isDark() }).catch(() => {});
  });
}

matchMedia('(prefers-color-scheme: dark)').addEventListener('change', syncTitlebar);
