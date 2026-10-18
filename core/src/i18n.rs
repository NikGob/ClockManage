//! Interface language: Russian or English, one for the whole process.
//!
//! Texts live in one flat JSON file per language, `ui/locales/<lang>.json`, shared by the core,
//! the app, the UI and the phone app: `tr("app_quit")`, `tr!("view_part_of", part = 2, parts = 3)`.
//! `tests/locales.rs` keeps the files and the code in step. The app sets the language from
//! the config (`Config::lang`, "auto" = the Windows UI language) before anything is shown.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    #[default]
    Ru,
    En,
}

impl Lang {
    pub fn key(self) -> &'static str {
        match self {
            Lang::Ru => "ru",
            Lang::En => "en",
        }
    }

    /// Language of a locale name like "ru-RU", "uk", "en-US": Russian for ru/uk/be, else English.
    pub fn from_locale(locale: &str) -> Lang {
        let primary = locale.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        if matches!(primary.as_str(), "ru" | "uk" | "be") {
            Lang::Ru
        } else {
            Lang::En
        }
    }
}

/// The language setting: a fixed one or the system's.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LangPref {
    #[default]
    Auto,
    Ru,
    En,
}

impl LangPref {
    /// `system` = the language the OS speaks, used for "auto".
    pub fn resolve(self, system: Lang) -> Lang {
        match self {
            LangPref::Auto => system,
            LangPref::Ru => Lang::Ru,
            LangPref::En => Lang::En,
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn get() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::Ru
    }
}

pub fn en() -> bool {
    get() == Lang::En
}

/// The texts of every language, one flat JSON file each, shared with the UI and the phone app.
const FILES: [(Lang, &str); 2] = [
    (Lang::Ru, include_str!("../../ui/locales/ru.json")),
    (Lang::En, include_str!("../../ui/locales/en.json")),
];

fn table(lang: Lang) -> &'static HashMap<String, String> {
    static TABLES: [OnceLock<HashMap<String, String>>; 2] = [OnceLock::new(), OnceLock::new()];
    TABLES[lang as usize].get_or_init(|| {
        let src = FILES.iter().find(|(l, _)| *l == lang).map(|(_, s)| *s).unwrap_or("{}");
        serde_json::from_str(src).expect("ui/locales: invalid JSON")
    })
}

/// The text of `key` in the current language (English if it is missing there, the key itself
/// if it is missing everywhere — the tests make sure it never is).
pub fn tr(key: &'static str) -> &'static str {
    tr_in(get(), key)
}

/// The text of `key` in `lang`, whatever the current language is.
pub fn tr_in(lang: Lang, key: &'static str) -> &'static str {
    table(lang).get(key).or_else(|| table(Lang::En).get(key)).map(String::as_str).unwrap_or(key)
}

/// Every language with a texts file.
pub fn langs() -> impl Iterator<Item = Lang> {
    FILES.iter().map(|(l, _)| *l)
}

/// Put `{name}` arguments into a text.
pub fn fill(text: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let arg = after.find('}').and_then(|close| {
            let name = &after[..close];
            args.iter().find(|(n, _)| *n == name).map(|(_, v)| (close, v))
        });
        match arg {
            Some((close, v)) => {
                out.push_str(&v.to_string());
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text of `key` with its `{name}` arguments: `tr!("view_part_of", part = 2, parts = 3)`.
#[macro_export]
macro_rules! tr {
    ($key:literal $(,)?) => {
        $crate::i18n::tr($key).to_string()
    };
    ($key:literal, $($name:ident = $val:expr),+ $(,)?) => {
        $crate::i18n::fill($crate::i18n::tr($key), &[$((stringify!($name), &$val as &dyn ::std::fmt::Display)),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales() {
        assert_eq!(Lang::from_locale("ru-RU"), Lang::Ru);
        assert_eq!(Lang::from_locale("uk_UA"), Lang::Ru);
        assert_eq!(Lang::from_locale("en-US"), Lang::En);
        assert_eq!(Lang::from_locale("sk"), Lang::En);
        assert_eq!(Lang::from_locale(""), Lang::En);
        assert_eq!(LangPref::Auto.resolve(Lang::En), Lang::En);
        assert_eq!(LangPref::Ru.resolve(Lang::En), Lang::Ru);
    }

    #[test]
    fn fills_named_arguments() {
        let s = fill("{a} из {b} · {c} {x}", &[("a", &1), ("b", &"2"), ("c", &String::from("3"))]);
        assert_eq!(s, "1 из 2 · 3 {x}");
        assert_eq!(fill("{\"mon\": \"full\"}", &[]), "{\"mon\": \"full\"}");
    }
}
