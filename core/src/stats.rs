//! Per-day statistics and JSON/CSV export built from saved [`DayState`]s.

use serde::Serialize;

use crate::clock::{self, MIN};
use crate::config::DayKind;
use crate::day::{fmt_day_min, DayState, MIN_PAUSE_MS};

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

/// A non-study segment as it ran: lunch, a nap, a walk…
#[derive(Debug, Clone, Serialize)]
pub struct BreakStats {
    /// The segment type ("Обед", "Сон", "Прогулка" or your own).
    #[serde(rename = "type")]
    pub kind: String,
    pub planned_min: u32,
    /// The segment itself; for a nap — the sleep from "Лёг", without the preparation.
    pub actual_min: f64,
    /// Minutes over the planned length (0 when it ended in time).
    pub overrun_min: f64,
    /// Start of the segment (of the preparation, when it had one).
    pub start: String,
    /// `None` while it is still running.
    pub end: Option<String>,
    /// Preparation as it ran (coffee, getting to bed); `None` for a segment without one.
    pub prep_min: Option<f64>,
    /// Preparation planned by the segment type.
    pub prep_planned_min: Option<u32>,
    /// "Лёг": where the countdown started (`None`: no preparation, or ended before lying down).
    pub lay_at: Option<String>,
    /// "Без времени": counted with a stopwatch, no countdown.
    pub stopwatch: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmergencyStats {
    pub at: String,
    pub until: String,
    pub minutes: f64,
    pub ended_early: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusStats {
    pub at: String,
    pub until: String,
    pub minutes: f64,
    pub reason: Option<String>,
    /// ui | mcp
    pub by: String,
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
    /// Non-study segments (lunch, nap, walk…) in order. They are not study time.
    pub breaks: Vec<BreakStats>,
    pub single_timer_min: f64,
    /// One-off shifts of this day's end, oldest first.
    pub day_end_changes: Vec<DayEndChangeStats>,
    /// Focus locks of this day; one carried over from the day before starts when this day began.
    pub focus_locks: Vec<FocusStats>,
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
        .filter(|(_, b)| !b.is_break())
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
    // A pause shorter than MIN_PAUSE_MS may still turn out to be a blink: not counted yet.
    let open_pause = d.pause.as_ref().filter(|p| now - p.since >= MIN_PAUSE_MS);
    if let Some(p) = open_pause {
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
    let pauses_ms: i64 = d.pauses.iter().map(|r| r.end - r.start).sum::<i64>() + open_pause.map(|p| now - p.since).unwrap_or(0);
    DayStats {
        date: d.date.format("%Y-%m-%d").to_string(),
        kind: d.kind,
        study_day: d.study_day,
        started_at: d.started_at.map(iso),
        completed_at: d.completed_at.map(iso),
        planned_min: d.plan.iter().filter(|b| !b.is_break()).map(|b| b.minutes).sum(),
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
        breaks: breaks(d, now, tz),
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
        focus_locks: d
            .focus
            .iter()
            .map(|f| FocusStats { at: iso(f.at), until: iso(f.until), minutes: m(f.until - f.at), reason: f.reason.clone(), by: f.by.clone() })
            .collect(),
    }
}

/// One subject over a week: journal hours per day, Monday first.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WeekSubject {
    pub name: String,
    pub hours: [f64; 7],
    pub total: f64,
}

/// A week of journal hours by subject and day (blocks of the same name are added up).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WeekStats {
    /// Monday of the week, YYYY-MM-DD.
    pub week_start: String,
    /// The 7 dates, Monday..Sunday.
    pub days: Vec<String>,
    pub subjects: Vec<WeekSubject>,
    pub day_totals: [f64; 7],
    /// Sum of all journal hours of the week.
    pub total: f64,
    /// Actual study minutes of the week (not rounded).
    pub actual_min: f64,
}

/// Monday of the week that contains `date`.
pub fn week_monday(date: chrono::NaiveDate) -> chrono::NaiveDate {
    use chrono::Datelike;
    date - chrono::Duration::days(date.weekday().num_days_from_monday() as i64)
}

/// Build the week starting at `monday` from the stats of whatever days of it have a log.
pub fn week_stats(monday: chrono::NaiveDate, days: &[DayStats]) -> WeekStats {
    let dates: Vec<String> = (0..7).map(|i| (monday + chrono::Duration::days(i)).format("%Y-%m-%d").to_string()).collect();
    let mut subjects: Vec<WeekSubject> = vec![];
    let mut actual = 0.0;
    for d in days {
        let Some(col) = dates.iter().position(|x| *x == d.date) else { continue };
        actual += d.actual_min;
        for b in &d.blocks {
            let key = b.name.trim().to_lowercase();
            let i = match subjects.iter().position(|s| s.name.to_lowercase() == key) {
                Some(i) => i,
                None => {
                    subjects.push(WeekSubject { name: b.name.trim().to_string(), hours: [0.0; 7], total: 0.0 });
                    subjects.len() - 1
                }
            };
            subjects[i].hours[col] += b.journal_hours;
            subjects[i].total += b.journal_hours;
        }
    }
    // Planned but never worked all week: not a journal line.
    subjects.retain(|s| s.total > 0.0);
    let mut day_totals = [0.0; 7];
    for s in &subjects {
        for (t, h) in day_totals.iter_mut().zip(s.hours) {
            *t += h;
        }
    }
    WeekStats {
        week_start: dates[0].clone(),
        days: dates,
        total: day_totals.iter().sum(),
        day_totals,
        subjects,
        actual_min: (actual * 10.0).round() / 10.0,
    }
}

/// The week as tab-separated text: pastes straight into a spreadsheet or a journal.
pub fn week_tsv(w: &WeekStats) -> String {
    let num = |h: f64| if h == 0.0 { String::new() } else { format!("{h}").replace('.', ",") };
    let mut out = String::from("Предмет");
    for d in &w.days {
        out.push('\t');
        out.push_str(&d[8..10]);
        out.push('.');
        out.push_str(&d[5..7]);
    }
    out.push_str("\tИтого\n");
    for s in &w.subjects {
        out.push_str(&s.name);
        for h in s.hours {
            out.push('\t');
            out.push_str(&num(h));
        }
        out.push('\t');
        out.push_str(&num(s.total));
        out.push('\n');
    }
    out.push_str("Итого");
    for h in w.day_totals {
        out.push('\t');
        out.push_str(&num(h));
    }
    out.push('\t');
    out.push_str(&num(w.total));
    out.push('\n');
    out
}

fn breaks(d: &DayState, now: clock::Ts, tz: i32) -> Vec<BreakStats> {
    let iso = |t: clock::Ts| clock::iso(t, tz);
    // A lunch from before 0.3 shows up as an "Обед" segment.
    let legacy = d.lunch.iter().map(|l| {
        let end = l.end.unwrap_or(now);
        let planned = if l.with_timer { d.timing.lunch_min } else { 0 };
        BreakStats {
            kind: "Обед".into(),
            planned_min: planned,
            actual_min: m(end - l.start),
            overrun_min: if l.with_timer { m((end - l.start - planned as i64 * MIN).max(0)) } else { 0.0 },
            start: iso(l.start),
            end: l.end.map(iso),
            prep_min: None,
            prep_planned_min: None,
            lay_at: None,
            stopwatch: !l.with_timer,
        }
    });
    let segs = d.segments.iter().map(|s| {
        let actual = s.actual_ms(now);
        let prep = s.prep_min > 0;
        BreakStats {
            kind: s.name.clone(),
            planned_min: s.planned_min,
            actual_min: m(actual),
            // Ended on the preparation: nothing ran over.
            overrun_min: if prep && s.lay_at.is_none() { 0.0 } else { m((actual - s.planned_ms()).max(0)) },
            start: iso(s.start),
            end: s.end.map(iso),
            prep_min: prep.then(|| m(s.prep_ms(now))),
            prep_planned_min: prep.then_some(s.prep_min),
            lay_at: s.lay_at.map(iso),
            stopwatch: s.open_ended,
        }
    });
    legacy.chain(segs).collect()
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', ';']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// One flat CSV with a row per block, pause, emergency and segment.
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
        for b in &d.breaks {
            let mut details = format!("overrun_min={}", b.overrun_min);
            if let Some(p) = b.prep_min {
                details.push_str(&format!(" prep_min={p} lay_at={}", b.lay_at.as_deref().unwrap_or("")));
            }
            if b.stopwatch {
                details.push_str(" stopwatch");
            }
            row([&d.date, "break", &b.kind, &b.start, b.end.as_deref().unwrap_or(""), &b.planned_min.to_string(), &b.actual_min.to_string(), &details]);
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
    fn week_adds_up_subjects_by_day() {
        let mut c = Config::default();
        let mon = 1_790_589_600_000; // Monday 2026-09-28
        let mut day = |ts: i64, blocks: &[(&str, i64)]| {
            c.profiles.full.plan = blocks.iter().map(|(n, m)| crate::day::PlanBlock::new(n, (*m as u32).max(1))).collect();
            let mut d = DayState::new(ts, &c);
            for (i, (_, m)) in blocks.iter().enumerate() {
                d.progress[i].work_ms = m * MIN;
            }
            day_stats(&d, 180, ts)
        };
        let days = vec![
            day(mon, &[("Математика", 165), ("Словацкий", 84)]),
            day(mon + 2 * 86_400_000, &[("математика", 60), ("Экстернат", 0)]),
            day(mon + 9 * 86_400_000, &[("Математика", 600)]), // next week: ignored
        ];
        let w = week_stats(week_monday(chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()), &days);
        assert_eq!(w.week_start, "2026-09-28");
        assert_eq!(w.subjects.len(), 2); // Экстернат never worked
        assert_eq!(w.subjects[0].name, "Математика");
        assert_eq!(w.subjects[0].hours, [2.75, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(w.subjects[0].total, 3.75);
        assert_eq!(w.subjects[1].total, 1.25);
        assert_eq!(w.day_totals[0], 4.0);
        assert_eq!(w.total, 5.0);
        assert_eq!(w.actual_min, 309.0);
        let tsv = week_tsv(&w);
        assert!(tsv.starts_with("Предмет\t28.09\t29.09"));
        assert!(tsv.contains("Математика\t2,75\t\t1\t"));
        assert!(tsv.trim_end().ends_with("Итого\t4\t\t1\t\t\t\t\t5"));
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
