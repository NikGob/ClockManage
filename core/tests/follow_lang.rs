//! The untouched defaults follow the interface language. A separate test binary: the language
//! is process-wide, and this test switches it.

use clockmanage_core::config::DayKind;
use clockmanage_core::i18n::{self, Lang};
use clockmanage_core::Config;

#[test]
fn untouched_defaults_follow_the_language() {
    // A config written by a Russian first run (or an older version), opened in English.
    i18n::set(Lang::Ru);
    let mut c = Config::default();
    c.profiles.light.plan[1].name = "Физика".into();
    c.segments[2].name = "Душ".into();
    i18n::set(Lang::En);
    assert!(c.follow_lang());
    let names = |k: DayKind, c: &Config| c.profiles.get(k).plan.iter().map(|b| b.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(DayKind::Full, &c), ["Math", "Languages", "Project"]);
    // What the user renamed is their data.
    assert_eq!(names(DayKind::Light, &c), ["Math", "Физика"]);
    assert_eq!(c.segments.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Lunch", "Nap", "Душ"]);
    assert!(c.emergency_phrase.starts_with("I am consciously"));
    assert!(!c.follow_lang());

    // Back to Russian: the same defaults come back.
    i18n::set(Lang::Ru);
    assert!(c.follow_lang());
    assert_eq!(c.segments[0].name, "Обед");
    assert_eq!(names(DayKind::Full, &c)[0], "Математика");

    // A name already taken is not doubled: "Lunch" stays when the user has an "Обед" of their own.
    i18n::set(Lang::En);
    let mut c = Config::default();
    c.segments.push(clockmanage_core::config::SegmentType { name: "Обед".into(), ..Default::default() });
    c.follow_lang();
    assert_eq!(c.segments.iter().filter(|t| t.name == "Lunch").count(), 1);
}
