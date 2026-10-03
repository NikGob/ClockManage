//! Per-day statistics and JSON/CSV export built from saved [`DayState`]s.

use serde::Serialize;

use crate::clock::{self, MIN};
use crate::config::DayKind;
use crate::day::{fmt_day_min, DayState};

#[derive(Debug, Clone, Serialize)]
pub struct BlockStats {
    pub name: String,
    pub planned_min: u32,
    pub actual_min: f64,
    /// `actual_min` rounded DOWN to a quarter of an hour, in hours (165 -> 2.75, 84.6 -> 1.25).
    pub journal_hours: f64,
    pub done: bool,
    pub parts_done: u32,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub pauses: u32,
    pub pause_min: f64,
    /// End-of-block line: what was boring, where the mind wandered.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PauseStats {
    pub start: String,
    pub end: String,
    pub minutes: f64,
    pub during: String,
    pub block: Option<String>,
    pub access_min: f64,
    pub extensions: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmergencyStats {
    pub at: String,
    pub until: String,
    pub minutes: f64,
    pub ended_early: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayEndChangeStats {
    pub at: String,
    pub from: String,
    pub to: String,
    /// The new end is after midnight (night of the next date).
    pub to_next_day: bool,
    pub reason: Option<String>,
    /// ui | mcp
    pub by: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayStats {
    pub date: String,
    pub kind: DayKind,
    pub study_day: bool,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub planned_min: u32,
    pub actual_min: f64,
    pub blocks: Vec<BlockStats>,
    /// Sum of `blocks[].journal_hours`.
    pub journal_total: f64,
    pub pauses_count: usize,
    pub pauses_min: f64,
    pub pauses: Vec<PauseStats>,
    pub pause_access_min: f64,
    pub pause_access_extensions: u32,
    pub emergency_count: usize,
    pub emergencies: Vec<EmergencyStats>,
    pub lunch: Option<String>,
    pub single_timer_min: f64,
    /// One-off shifts of this day's end, oldest first.
    pub day_end_changes: Vec<DayEndChangeStats>,
}

fn m(ms: i64) -> f64 {
    (ms as f64 / MIN as f64 * 10.0).round() / 10.0
}

/// Minutes (as shown, one decimal) -> hours rounded down to 0.25. Integer math on tenths of a
/// minute, so 60.0 is exactly 1.0 and never 0.75 from a float error.
pub fn journal_hours(actual_min: f64) -> f64 {
    let tenths = (actual_min * 10.0).round().max(0.0) as i64;
    (tenths / 150) as f64 * 0.25
}

pub fn day_stats(d: &DayState, tz: i32, now: clock::Ts) -> DayStats {
    let iso = |t: clock::Ts| clock::iso(t, tz);
    let blocks: Vec<BlockStats> = d
        .plan
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let p = &d.progress[i];
            let (n, ms) = d
                .pauses
                .iter()
                .filter(|r| r.block == Some(i))
                .fold((0u32, 0i64), |(n, ms), r| (n + 1, ms + r.end - r.start));
            let actual_min = m(d.block_work_live(i, now));
            BlockStats {
                name: b.name.clone(),
                planned_min: b.minutes,
                actual_min,
                journal_hours: journal_hours(actual_min),
                done: p.completed_at.is_some(),
                parts_done: p.parts_done,
                started_at: p.started_at.map(iso),
                completed_at: p.completed_at.map(iso),
                pauses: n,
                pause_min: m(ms),
                note: p.note.clone(),
            }
        })
        .collect();
    let mut pauses: Vec<PauseStats> = d
        .pauses
        .iter()
        .map(|r| PauseStats {
            start: iso(r.start),
            end: iso(r.end),
            minutes: m(r.end - r.start),
            during: r.what.clone(),
            block: r.block.and_then(|b| d.plan.get(b)).map(|b| b.name.clone()),
            access_min: m(r.access_ms),
            extensions: r.extensions,
        })
        .collect();
    let mut access_ms: i64 = d.pauses.iter().map(|r| r.access_ms).sum();
    let mut extensions: u32 = d.pauses.iter().map(|r| r.extensions).sum();
    if let Some(p) = &d.pause {
        let acc: i64 = p.access.iter().map(|w| (w.until.min(now) - w.from).max(0)).sum();
        access_ms += acc;
        extensions += p.access.len().saturating_sub(1) as u32;
        pauses.push(PauseStats {
            start: iso(p.since),
            end: "идёт".into(),
            minutes: m(now - p.since),
            during: p.what.clone(),
            block: p.block.and_then(|b| d.plan.get(b)).map(|b| b.name.clone()),
            access_min: m(acc),
            extensions: p.access.len().saturating_sub(1) as u32,
        });
    }
    let pauses_ms: i64 = d.pauses.iter().map(|r| r.end - r.start).sum::<i64>() + d.pause.as_ref().map(|p| now - p.since).unwrap_or(0);
    DayStats {
        date: d.date.format("%Y-%m-%d").to_string(),
        kind: d.kind,
        study_day: d.study_day,
        started_at: d.started_at.map(iso),
        completed_at: d.completed_at.map(iso),
        planned_min: d.plan.iter().map(|b| b.minutes).sum(),
        actual_min: m((0..d.plan.len()).map(|i| d.block_work_live(i, now)).sum()),
        journal_total: blocks.iter().map(|b| b.journal_hours).sum(),
        blocks,
        pauses_count: pauses.len(),
        pauses_min: m(pauses_ms),
        pauses,
        pause_access_min: m(access_ms),
        pause_access_extensions: extensions,
        emergency_count: d.emergencies.len(),
        emergencies: d
            .emergencies
            .iter()
            .map(|e| EmergencyStats { at: iso(e.at), until: iso(e.until), minutes: m(e.until - e.at), ended_early: e.ended_early })
            .collect(),
        lunch: d.lunch.as_ref().map(|l| {
            let kind = if !l.with_timer {
                "без таймера"
            } else if l.at_pc {
                "с таймером, за ПК"
            } else {
                "с таймером"
            };
            match l.end {
                Some(e) => format!("{kind}, {}–{}", clock::hm(l.start, tz), clock::hm(e, tz)),
                None => format!("{kind}, с {}", clock::hm(l.start, tz)),
            }
        }),
        single_timer_min: m(d.singles.iter().map(|s| s.work_ms).sum()),
        day_end_changes: d
            .day_end_changes
            .iter()
            .map(|c| DayEndChangeStats {
                at: iso(c.ts),
                from: fmt_day_min(c.from_min),
                to: fmt_day_min(c.to_min),
                to_next_day: c.to_min >= 24 * 60,
                reason: c.reason.clone(),
                by: c.by.clone(),
            })
            .collect(),
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', ';']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// One flat CSV with a row per block, pause, emergency and lunch.
pub fn to_csv(days: &[DayStats]) -> String {
    let mut out = String::from("\u{feff}date,type,name,start,end,planned_min,actual_min,details\n");
    let mut row = |cols: [&str; 8]| {
        out.push_str(&cols.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    };
    for d in days {
        row([&d.date, "day", "", d.started_at.as_deref().unwrap_or(""), d.completed_at.as_deref().unwrap_or(""), &d.planned_min.to_string(), &d.actual_min.to_string(), &format!("journal_h={} pauses={} pause_min={} pause_access_min={} extensions={} emergencies={}", d.journal_total, d.pauses_count, d.pauses_min, d.pause_access_min, d.pause_access_extensions, d.emergency_count)]);
        for b in &d.blocks {
            row([&d.date, "block", &b.name, b.started_at.as_deref().unwrap_or(""), b.completed_at.as_deref().unwrap_or(""), &b.planned_min.to_string(), &b.actual_min.to_string(), &format!("journal_h={} done={} parts={} pauses={} pause_min={}{}", b.journal_hours, b.done, b.parts_done, b.pauses, b.pause_min, b.note.as_ref().map(|n| format!(" note={n}")).unwrap_or_default())]);
        }
        for p in &d.pauses {
            row([&d.date, "pause", p.block.as_deref().unwrap_or(""), &p.start, &p.end, "", &p.minutes.to_string(), &format!("during={} access_min={} extensions={}", p.during, p.access_min, p.extensions)]);
        }
        for e in &d.emergencies {
            row([&d.date, "emergency", "", &e.at, &e.until, "", &e.minutes.to_string(), if e.ended_early { "ended_early" } else { "" }]);
        }
        if let Some(l) = &d.lunch {
            row([&d.date, "lunch", "", "", "", "", "", l]);
        }
        for c in &d.day_end_changes {
            row([&d.date, "day_end_change", "", &c.at, "", "", "", &format!("{}->{} by={} reason={}", c.from, c.to, c.by, c.reason.as_deref().unwrap_or(""))]);
        }
        if d.single_timer_min > 0.0 {
            row([&d.date, "single_timer", "", "", "", "", &d.single_timer_min.to_string(), ""]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn journal_hours_round_down_to_quarters() {
        assert_eq!(journal_hours(165.0), 2.75);
        assert_eq!(journal_hours(84.6), 1.25);
        assert_eq!(journal_hours(60.0), 1.0);
        assert_eq!(journal_hours(59.9), 0.75);
        assert_eq!(journal_hours(14.9), 0.0);
        assert_eq!(journal_hours(0.0), 0.0);
        let mut c = Config::default();
        c.profiles.full.plan = vec![crate::day::PlanBlock::new("A", 165), crate::day::PlanBlock::new("B", 90)];
        let s = 1_790_589_600_000;
        let mut d = DayState::new(s, &c);
        d.start_day(s, &c).unwrap();
        d.progress[0].work_ms = 165 * MIN;
        d.progress[1].work_ms = 84 * MIN + 36_000;
        d.pause(s, &c).unwrap();
        d.phase = crate::day::Phase::Idle;
        d.pause = None;
        let st = day_stats(&d, 180, s);
        assert_eq!(st.blocks[0].journal_hours, 2.75);
        assert_eq!(st.blocks[1].journal_hours, 1.25);
        assert_eq!(st.journal_total, 4.0);
    }

    #[test]
    fn csv_escapes() {
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        let cfg = Config::default();
        let d = DayState::new(0, &cfg);
        let s = day_stats(&d, 180, 0);
        let csv = to_csv(&[s]);
        assert!(csv.lines().count() >= 4);
    }
}
