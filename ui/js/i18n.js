// Interface language. Texts live in one flat JSON file per language, ui/locales/<lang>.json,
// shared with the Rust side and the phone app: tr('app_quit'), tr('settings_downloading', { version: '0.5.0' }).
// The backend decides the language (Settings → System → Language, "auto" = Windows); every
// window starts with the last one it saw and reloads once when the state says otherwise.

const KEY = 'lang';

function initial() {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved === 'ru' || saved === 'en') return saved;
  } catch { /* storage may be unavailable */ }
  return /^(ru|uk|be)\b/i.test(navigator.language || '') ? 'ru' : 'en';
}

export const lang = initial();
document.documentElement.lang = lang;

export const en = lang === 'en';

async function load(name) {
  try {
    return await (await fetch(new URL(`../locales/${name}.json`, import.meta.url))).json();
  } catch {
    return {};
  }
}
// English is the fallback for a key missing in the current language.
const [texts, fallback] = await Promise.all([load(lang), en ? null : load('en')]);

/** The text of `key` in the current language, with its `{name}` arguments filled in. */
export function tr(key, args) {
  const text = texts[key] ?? fallback?.[key] ?? key;
  return args ? text.replace(/\{(\w+)\}/g, (m, name) => (name in args ? String(args[name]) : m)) : text;
}

/** Locale for Intl / toLocale*String. */
export const LOCALE = en ? 'en-GB' : 'ru-RU';

/** Follow the backend's language: remember it and reload this window if it differs. */
export function followLang(next) {
  if (next !== 'ru' && next !== 'en' || next === lang) return;
  // Reload only when the next load is sure to start in `next`: without storage it would loop.
  try {
    localStorage.setItem(KEY, next);
    if (localStorage.getItem(KEY) === next) location.reload();
  } catch { /* storage unavailable: stay in the current language */ }
}

/** Decimal comma in Russian, point in English: 1,5 / 1.5. */
export const dec = (n) => (en ? String(n) : String(n).replace('.', ','));
