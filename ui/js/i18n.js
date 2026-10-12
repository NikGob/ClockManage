// Interface language. Texts stay next to the code as pairs: L('Пауза', 'Pause').
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

/** The text of the current language. */
export const L = (ru, eng) => (en ? eng : ru);

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
