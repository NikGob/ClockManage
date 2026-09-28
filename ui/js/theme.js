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

const VARIANTS = { fidelity: SchemeFidelity, tonal_spot: SchemeTonalSpot, vibrant: SchemeVibrant };

const kebab = (s) => s.replace(/[A-Z]/g, (m) => '-' + m.toLowerCase());

export function schemeVars(seed, dark, variant = 'fidelity') {
  let argb;
  try { argb = argbFromHex(seed); } catch { argb = argbFromHex('#2E7D32'); }
  const Scheme = VARIANTS[variant] || SchemeFidelity;
  const s = new Scheme(Hct.fromInt(argb), dark, 0);
  const out = {};
  for (const r of ROLES) out[`--md-sys-color-${kebab(r)}`] = hexFromArgb(C[r].getArgb(s));
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
}
