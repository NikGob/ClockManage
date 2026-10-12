//! Interface language: Russian or English, one for the whole process.
//!
//! Texts stay next to the code as pairs — `tr!("Пауза", "Paused")`, `pick("Обед", "Lunch")` —
//! so a new string can't miss its translation. The app sets the language from the config
//! (`Config::lang`, "auto" = the Windows UI language) before anything is shown.

use std::sync::atomic::{AtomicU8, Ordering};

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

/// The text of the current language.
pub fn pick<'a>(ru: &'a str, en: &'a str) -> &'a str {
    if self::en() {
        en
    } else {
        ru
    }
}

/// `format!` in the current language: `tr!("{n} мин", "{n} min")`, `tr!("Часть {} из {}", "Part {} of {}", a, b)`.
/// Both formats take the same arguments.
#[macro_export]
macro_rules! tr {
    ($ru:literal, $en:literal $(,)?) => {
        if $crate::i18n::en() { format!($en) } else { format!($ru) }
    };
    ($ru:literal, $en:literal, $($arg:expr),+ $(,)?) => {
        if $crate::i18n::en() { format!($en, $($arg),+) } else { format!($ru, $($arg),+) }
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
}
