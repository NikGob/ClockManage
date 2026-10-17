//! The English interface. A separate test binary: the language is process-wide, and the unit
//! tests of the crate check the Russian texts.

use clockmanage_core::config::{DayKind, EditContext};
use clockmanage_core::day::PlanBlock;
use clockmanage_core::i18n::{self, Lang};
use clockmanage_core::mcp::{self, McpHost, NightStartRequest, PlanRequest};
use clockmanage_core::{chrono, view, Config, DayState};
use serde_json::{json, Value};

fn en() {
    i18n::set(Lang::En);
}

// Monday 2026-09-28, 13:00 at UTC+3.
fn start() -> i64 {
    chrono::DateTime::parse_from_rfc3339("2026-09-28T10:00:00Z").unwrap().timestamp_millis()
}

#[test]
fn defaults_are_english() {
    en();
    let c = Config::default();
    assert_eq!(c.profiles.full.plan[0].name, "Math");
    assert!(c.segments.iter().any(|s| s.name == "Lunch"));
    assert!(c.emergency_phrase.starts_with("I am consciously"));
    assert_eq!(DayKind::Off.label(), "Day off");
}

#[test]
fn timer_view_and_refusals() {
    en();
    let mut c = Config::default();
    c.profiles.full.plan = vec![PlanBlock::new("Math", 90)];
    let mut d = DayState::new(start(), &c);
    assert_eq!(view::build(&d, &c, start()).phase.title, "Day not started");
    d.start_day(start(), &c).unwrap();
    assert_eq!(view::build(&d, &c, start() + 1000).phase.subtitle, "Part 1 of 2");
    assert_eq!(d.start_day(start(), &c).unwrap_err(), "The day has already started.");

    let mut fewer = c.clone();
    fewer.blocklist.sites.clear();
    let err = c.check_update(&fewer, EditContext { locked: true }).unwrap_err();
    assert_eq!(err, "While the lock is on, sites can only be added.");
}

#[test]
fn autolock_log_is_english() {
    en();
    let mut c = Config::default();
    c.profiles.full.plan = vec![PlanBlock::new("Math", 45)];
    let mut d = DayState::new(start() - 3_600_000, &c);
    d.tick(start(), &c);
    d.start_day(start() + 12 * 60_000, &c).unwrap();
    let log: Vec<&str> = d.events.iter().filter(|e| e.kind == "autolock").map(|e| e.text.as_str()).collect();
    assert_eq!(log, ["Autolock from 13:00: the day hasn’t started — the block list is closed until “Start day”", "The day started 12 min after the autolock (start delay)"]);
}

struct NoHost;

impl McpHost for NoHost {
    fn session_state(&self) -> Value {
        json!({})
    }
    fn day_stats(&self, _: Option<&str>) -> Result<Value, String> {
        unreachable!()
    }
    fn week_stats(&self, _: Option<&str>) -> Result<Value, String> {
        unreachable!()
    }
    fn get_plan(&self, _: Option<&str>) -> Result<Value, String> {
        unreachable!()
    }
    fn set_plan(&self, _: PlanRequest) -> Result<Value, String> {
        unreachable!()
    }
    fn set_template(&self, _: DayKind, _: Vec<PlanBlock>) -> Result<Value, String> {
        unreachable!()
    }
    fn set_week_schedule(&self, _: Vec<(usize, DayKind)>) -> Result<Value, String> {
        unreachable!()
    }
    fn set_day_end(&self, _: &str, _: Option<&str>) -> Result<Value, String> {
        unreachable!()
    }
    fn finish_block(&self, _: Option<&str>, _: Option<&str>) -> Result<Value, String> {
        unreachable!()
    }
    fn set_block_note(&self, _: Option<&str>, _: &str) -> Result<Value, String> {
        unreachable!()
    }
    fn get_blocklist(&self) -> Value {
        json!({})
    }
    fn edit_blocklist(&self, _: bool, _: Vec<String>, _: Vec<String>) -> Result<Value, String> {
        unreachable!()
    }
    fn start_focus_lock(&self, _: u32, _: Option<&str>) -> Result<Value, String> {
        unreachable!()
    }
    fn set_night_start(&self, _: NightStartRequest) -> Result<Value, String> {
        unreachable!()
    }
    fn set_autolock(&self, _: DayKind, _: Option<u32>) -> Result<Value, String> {
        unreachable!()
    }
}

#[test]
fn mcp_speaks_english() {
    en();
    let call = |body: Value| -> Value { serde_json::from_str(&mcp::handle(&body.to_string(), &NoHost).unwrap()).unwrap() };
    let list = call(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}));
    let text = list.to_string();
    assert!(!text.chars().any(|c| ('а'..='я').contains(&c.to_ascii_lowercase()) || ('А'..='Я').contains(&c)), "Cyrillic in tools/list");
    let bad = call(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "start_focus_lock", "arguments": {}}}));
    assert_eq!(bad["result"]["content"][0]["text"], "minutes (or hours) is required: how long to lock for.");
}
