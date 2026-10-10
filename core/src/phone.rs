//! What the Android app gets over Wi-Fi: a snapshot of the day plus a projected timeline.
//!
//! The phone may lose the PC at any moment (Wi-Fi, PC asleep). It keeps counting from the
//! timeline: the engine is deterministic, so the PC simulates the day forward ("nobody presses
//! anything") and the phone just walks the entries by its own clock. Every value in the snapshot
//! is an absolute timestamp, so the snapshot only changes when the day really changes — that is
//! what the long poll waits for.

use serde::Serialize;

use crate::clock::{Ts, MIN};
use crate::config::Config;
use crate::day::{worked_min, DayState, Mode, Phase};
use crate::view;

/// How far ahead the timeline is simulated.
const HORIZON_MS: i64 = 16 * 60 * MIN;
const MAX_ENTRIES: usize = 32;

/// One phase of the projected day.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Entry {
    /// For a running work/break: its virtual start (elapsed = now - from).
    /// For a paused phase: the pause start. For waiting: since when.
    pub from: Ts,
    /// When this phase ends by itself; `None`: it waits for the user.
    pub until: Option<Ts>,
    /// idle | work | break | lunch_break | await | lunch | segment | done
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub block: Option<usize>,
    pub paused: bool,
    /// Frozen elapsed time while paused (otherwise `now - from`).
    pub elapsed_ms: i64,
    pub dur_ms: i64,
    /// Work of every plan block at `from`; the running block adds `now - from`.
    pub blocks_work_ms: Vec<i64>,
    /// Segment whose end is a loud alarm (a nap): the phone rings at `from + dur_ms`.
    /// Always false while getting ready (`prep`): the alarm counts from "Лёг".
    pub alarm: bool,
    /// Segment still getting ready: `from + dur_ms` is the end of the preparation — a reminder
    /// to lie down, not the alarm. Waits for "Лёг".
    pub prep: bool,
    /// Segment "без времени": a stopwatch from `from`, no countdown and no end notices.
    pub stopwatch: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PhoneBlock {
    pub name: String,
    pub minutes: u32,
    pub done: bool,
    pub started: bool,
    pub parts: u32,
    pub parts_done: u32,
    /// The "−" button may not go below this (during the lock: the time already worked).
    pub min_minutes: u32,
    /// study | break
    pub kind: crate::day::ItemKind,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PhoneLock {
    /// The day lock is in force (it may be suspended by an access window).
    pub base: bool,
    pub blocked: bool,
    pub reason: String,
    /// End of the current access window: blocked again after it.
    pub until: Option<Ts>,
    /// End of the focus lock: blocked until then, past the day end and the plan too.
    pub focus_until: Option<Ts>,
    /// The study-day or single-timer lock alone, without the focus lock.
    pub day_lock: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PhoneCan {
    pub start_day: bool,
    pub pause: bool,
    pub resume: bool,
    pub start_next: bool,
    pub edit_plan: bool,
    /// "Закончил" / "Встал" for the running segment.
    pub end_segment: bool,
    /// "Лёг": the running segment is getting ready.
    pub lay_down: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PhoneSnapshot {
    /// Changes only when the day changes (the long poll waits for a new one).
    pub version: String,
    pub server_now: Ts,
    pub date: String,
    pub kind: crate::config::DayKind,
    pub study_day: bool,
    pub started: bool,
    pub completed: bool,
    pub mode: Mode,
    pub day_end: String,
    pub day_end_at: Ts,
    pub lock: PhoneLock,
    /// The plan closes by itself at this time if nobody touches anything (the lock lifts).
    pub done_at: Option<Ts>,
    pub blocks: Vec<PhoneBlock>,
    pub timeline: Vec<Entry>,
    pub can: PhoneCan,
    /// Blocked on the phone during the lock.
    pub sites: Vec<String>,
    pub apps: Vec<String>,
}

fn entry(d: &DayState, cfg: &Config, t: Ts) -> Entry {
    let v = view::build(d, cfg, t).phase;
    let blocks_work_ms = d.progress.iter().map(|p| p.work_ms).collect();
    let (from, until, paused, elapsed_ms, dur_ms) = match &d.phase {
        Phase::Work { dur_ms, elapsed_ms, since: Some(s), .. } | Phase::Break { dur_ms, elapsed_ms, since: Some(s), .. } => {
            let from = s - elapsed_ms;
            (from, Some(from + dur_ms), false, 0, *dur_ms)
        }
        Phase::Work { dur_ms, elapsed_ms, since: None, .. } | Phase::Break { dur_ms, elapsed_ms, since: None, .. } => {
            (d.pause.as_ref().map(|p| p.since).unwrap_or(t), None, true, *elapsed_ms, *dur_ms)
        }
        Phase::Await { since, .. } | Phase::Lunch { since, .. } => (*since, None, false, 0, 0),
        // Waits for the user, but has a planned end: the phone counts down to `from + dur_ms`
        // (the preparation while getting ready, then the segment from "Лёг").
        Phase::Segment { rec, .. } => (d.segments[*rec].countdown_from(), None, false, 0, v.dur_ms),
        Phase::Done => (d.completed_at.unwrap_or(0), None, false, 0, 0),
        Phase::Idle => (0, None, false, 0, 0),
    };
    Entry {
        alarm: v.alarm && !v.prep && !v.stopwatch,
        prep: v.prep,
        stopwatch: v.stopwatch,
        from,
        until,
        kind: v.kind,
        title: v.title,
        subtitle: v.subtitle,
        block: v.block,
        paused,
        elapsed_ms,
        dur_ms,
        blocks_work_ms,
    }
}

/// Walk the day forward from `now` as if nobody pressed anything.
pub fn timeline(d: &DayState, cfg: &Config, now: Ts) -> (Vec<Entry>, Option<Ts>) {
    let mut sim = d.clone();
    let mut out = vec![];
    let mut done_at = None;
    let mut t = now;
    while out.len() < MAX_ENTRIES {
        let e = entry(&sim, cfg, t);
        let until = e.until;
        out.push(e);
        let Some(end) = until else { break };
        if end - now > HORIZON_MS {
            break;
        }
        t = end.max(t);
        sim.tick(t, cfg);
        if d.completed_at.is_none() && done_at.is_none() {
            done_at = sim.completed_at;
        }
    }
    (out, done_at)
}

/// FNV-1a: a cheap fingerprint of the snapshot, nothing cryptographic.
fn fingerprint(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

pub fn snapshot(d: &DayState, cfg: &Config, now: Ts) -> PhoneSnapshot {
    let v = view::build(d, cfg, now);
    let locked = d.plan_lock(now, cfg);
    let (timeline, done_at) = timeline(d, cfg, now);
    let blocks = d
        .plan
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let p = &d.progress[i];
            let touched = p.started_at.is_some() || p.work_ms > 0;
            PhoneBlock {
                name: b.name.clone(),
                minutes: b.minutes,
                done: d.is_block_done(i),
                started: p.started_at.is_some(),
                parts: d.block_parts(i),
                parts_done: p.parts_done,
                kind: b.kind,
                // The same floor as everywhere: whole minutes worked, rounded down.
                min_minutes: if locked && touched && !b.is_break() { worked_min(d.block_work_live(i, now)).max(1) } else { 1 },
            }
        })
        .collect();
    let mut s = PhoneSnapshot {
        version: String::new(),
        server_now: now,
        date: v.date,
        kind: v.kind,
        study_day: v.study_day,
        started: v.started,
        completed: v.completed,
        mode: v.mode,
        day_end: v.day_end,
        day_end_at: v.day_end_at,
        lock: PhoneLock { base: v.lock.base, blocked: v.lock.blocked, reason: v.lock.reason, until: v.lock.until, focus_until: v.lock.focus_until, day_lock: locked || d.single_lock() },
        done_at,
        blocks,
        timeline,
        can: PhoneCan {
            start_day: v.can.start_day,
            pause: v.can.pause,
            resume: v.can.resume,
            start_next: matches!(v.phase.kind.as_str(), "await" | "lunch"),
            edit_plan: d.mode == Mode::Plan,
            end_segment: v.can.end_segment,
            lay_down: v.can.lay_down,
        },
        sites: cfg.blocklist.sites.clone(),
        apps: cfg.phone.apps.clone(),
    };
    // `min_minutes` follows the running block minute by minute: leave it out of the fingerprint,
    // or the long poll would wake every minute for nothing.
    let mut stable = s.clone();
    stable.server_now = 0;
    stable.blocks.iter_mut().for_each(|b| b.min_minutes = 0);
    s.version = fingerprint(&serde_json::to_string(&stable).unwrap_or_default());
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::day::PlanBlock;
    use chrono::DateTime;

    fn t(s: &str) -> Ts {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }

    fn cfg() -> Config {
        let mut c = Config::default();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 90), PlanBlock::new("Экстернат", 45)];
        c
    }

    #[test]
    fn timeline_runs_until_the_user_is_needed() {
        let c = cfg();
        let s = t("2026-09-28T10:00:00Z");
        let mut d = crate::DayState::new(s, &c);
        d.start_day(s, &c).unwrap();
        let (tl, done_at) = timeline(&d, &c, s + 5 * MIN);
        let kinds: Vec<_> = tl.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["work", "break", "await"]);
        assert_eq!(tl[0].from, s);
        assert_eq!(tl[0].until, Some(s + 45 * MIN));
        assert_eq!(tl[1].until, Some(s + 55 * MIN));
        assert_eq!(tl[2].until, None);
        assert_eq!(tl[2].from, s + 55 * MIN);
        assert_eq!(done_at, None);
        // the simulation never touches the real day
        assert!(d.phase.is_work());
    }

    #[test]
    fn last_part_projects_the_day_end() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("A", 30)];
        let s = t("2026-09-28T10:00:00Z");
        let mut d = crate::DayState::new(s, &c);
        d.start_day(s, &c).unwrap();
        let (tl, done_at) = timeline(&d, &c, s);
        assert_eq!(tl.last().unwrap().kind, "done");
        assert_eq!(done_at, Some(s + 30 * MIN));
    }

    #[test]
    fn version_is_stable_while_nothing_happens() {
        let c = cfg();
        let s = t("2026-09-28T10:00:00Z");
        let mut d = crate::DayState::new(s, &c);
        d.start_day(s, &c).unwrap();
        let a = snapshot(&d, &c, s + MIN);
        let b = snapshot(&d, &c, s + 3 * MIN);
        assert_eq!(a.version, b.version);
        d.pause(s + 4 * MIN, &c).unwrap();
        let p = snapshot(&d, &c, s + 5 * MIN);
        assert_ne!(p.version, a.version);
        assert!(p.timeline[0].paused);
        assert_eq!(p.timeline[0].elapsed_ms, 4 * MIN);
        assert_eq!(snapshot(&d, &c, s + 9 * MIN).version, p.version);
    }

    #[test]
    fn running_block_minimum_follows_worked_time() {
        let c = cfg();
        let s = t("2026-09-28T10:00:00Z");
        let mut d = crate::DayState::new(s, &c);
        d.start_day(s, &c).unwrap();
        // 20 min and a bit: 20, like done_min and the "can't cut below" limit.
        let snap = snapshot(&d, &c, s + 20 * MIN + 40_000);
        assert_eq!(snap.blocks[0].min_minutes, 20);
        assert_eq!(snap.blocks[1].min_minutes, 1);
        assert!(snap.lock.blocked);
    }
}
