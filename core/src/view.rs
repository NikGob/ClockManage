//! Read model for the UI, the mini window, the tray and MCP.

use serde::Serialize;

use crate::clock::{self, Ts};
use crate::config::{fmt_hm, Config};
use crate::day::{BreakKind, DayState, LockState, Mode, Phase};

#[derive(Debug, Clone, Serialize)]
pub struct PhaseView {
    /// idle | work | break | lunch_break | await | lunch | done
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub block: Option<usize>,
    pub dur_ms: i64,
    pub elapsed_ms: i64,
    pub remaining_ms: i64,
    pub running: bool,
    pub paused: bool,
    /// When waiting: how long already.
    pub waiting_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockView {
    pub name: String,
    pub minutes: u32,
    pub work_ms: i64,
    pub parts: u32,
    pub parts_done: u32,
    pub done: bool,
    pub current: bool,
    pub started: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PauseView {
    pub since: Ts,
    pub paused_ms: i64,
    pub access_enabled: bool,
    pub access_until: Option<Ts>,
    pub access_left_ms: i64,
    pub extensions: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Can {
    pub start_day: bool,
    /// Kinds today can be raised to (stricter than now).
    pub raise: Vec<crate::config::DayKind>,
    pub pause: bool,
    pub resume: bool,
    pub start_next: bool,
    pub lunch: bool,
    pub single: bool,
    pub stop_single: bool,
    pub emergency: bool,
    pub extend_access: bool,
    pub end_access: bool,
    pub edit_pause_access: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub now: Ts,
    pub date: String,
    pub weekday: usize,
    pub study_day: bool,
    /// full | light | off
    pub day_kind: crate::config::DayKind,
    /// Kind by the schedule, when today was raised by hand.
    pub raised_from: Option<crate::config::DayKind>,
    pub started: bool,
    pub completed: bool,
    pub after_day_end: bool,
    pub day_end: String,
    pub mode: Mode,
    pub phase: PhaseView,
    pub blocks: Vec<BlockView>,
    pub planned_ms: i64,
    pub work_ms: i64,
    pub lock: LockState,
    pub pause: Option<PauseView>,
    pub emergency_count: usize,
    pub lunch_used: bool,
    pub single: Option<crate::day::SingleRun>,
    pub can: Can,
}

pub fn build(d: &DayState, cfg: &Config, now: Ts) -> View {
    let lock = d.lock_state(now, cfg);
    let current = d.current_block();
    let paused = d.pause.is_some();
    let elapsed = d.phase_elapsed(now);
    let phase = match &d.phase {
        Phase::Idle => PhaseView {
            kind: "idle".into(),
            title: if d.started_at.is_some() { "Пауза дня".into() } else { "День не начат".into() },
            subtitle: String::new(),
            block: None,
            dur_ms: 0,
            elapsed_ms: 0,
            remaining_ms: 0,
            running: false,
            paused: false,
            waiting_ms: 0,
        },
        Phase::Work { block, dur_ms, since, .. } => {
            let (name, _, parts) = d.part_info(*block);
            let part = match d.mode {
                Mode::Plan => d.progress[*block].parts_done + 1,
                Mode::Single => d.singles.last().map(|s| s.rounds + 1).unwrap_or(1),
            };
            PhaseView {
                kind: "work".into(),
                title: name,
                subtitle: if parts > 0 { format!("Часть {part} из {parts}") } else { format!("Круг {part}") },
                block: current,
                dur_ms: *dur_ms,
                elapsed_ms: elapsed,
                remaining_ms: dur_ms - elapsed,
                running: since.is_some(),
                paused,
                waiting_ms: 0,
            }
        }
        Phase::Break { brk, dur_ms, since, next, .. } => {
            let (name, part, parts) = d.part_info(*next);
            let (kind, title) = match brk {
                BreakKind::Short => ("break", "Перерыв"),
                BreakKind::Between => ("break", "Перерыв между блоками"),
                BreakKind::Lunch { at_pc: true } => ("lunch_break", "Обед за ПК"),
                BreakKind::Lunch { at_pc: false } => ("lunch_break", "Обед"),
            };
            PhaseView {
                kind: kind.into(),
                title: title.into(),
                subtitle: if parts > 0 { format!("Дальше: {name}, часть {part} из {parts}") } else { format!("Дальше: круг {part}") },
                block: current,
                dur_ms: *dur_ms,
                elapsed_ms: elapsed,
                remaining_ms: dur_ms - elapsed,
                running: since.is_some(),
                paused,
                waiting_ms: 0,
            }
        }
        Phase::Await { next, since, .. } => {
            let (name, part, parts) = d.part_info(*next);
            PhaseView {
                kind: "await".into(),
                title: "Перерыв окончен".into(),
                subtitle: if parts > 0 { format!("{name} · часть {part} из {parts}") } else { format!("Круг {part}") },
                block: current,
                dur_ms: 0,
                elapsed_ms: 0,
                remaining_ms: 0,
                running: false,
                paused: false,
                waiting_ms: now - since,
            }
        }
        Phase::Lunch { since, next } => {
            let (name, part, parts) = d.part_info(*next);
            PhaseView {
                kind: "lunch".into(),
                title: "Обед".into(),
                subtitle: format!("Потом: {name}, часть {part} из {parts}"),
                block: current,
                dur_ms: 0,
                elapsed_ms: now - since,
                remaining_ms: 0,
                running: false,
                paused: false,
                waiting_ms: now - since,
            }
        }
        Phase::Done => PhaseView {
            kind: "done".into(),
            title: "День закрыт".into(),
            subtitle: "Все блоки отсижены".into(),
            block: None,
            dur_ms: 0,
            elapsed_ms: 0,
            remaining_ms: 0,
            running: false,
            paused: false,
            waiting_ms: 0,
        },
    };

    let blocks: Vec<BlockView> = d
        .plan
        .iter()
        .enumerate()
        .map(|(i, b)| BlockView {
            name: b.name.clone(),
            minutes: b.minutes,
            work_ms: d.block_work_live(i, now),
            parts: d.block_parts(i),
            parts_done: d.progress[i].parts_done,
            done: d.is_block_done(i),
            current: d.mode == Mode::Plan && current == Some(i),
            started: d.progress[i].started_at.is_some(),
        })
        .collect();

    let pause = d.pause.as_ref().map(|p| {
        let until = p.access_until();
        PauseView {
            since: p.since,
            paused_ms: now - p.since,
            access_enabled: cfg.pause_access,
            access_until: until,
            access_left_ms: until.map(|u| (u - now).max(0)).unwrap_or(0),
            extensions: p.access.len().saturating_sub(1) as u32,
        }
    });

    let running = d.phase_elapsed(now) >= 0 && matches!(d.phase, Phase::Work { since: Some(_), .. } | Phase::Break { since: Some(_), .. });
    let lunch_break = matches!(d.phase, Phase::Break { brk: BreakKind::Lunch { .. }, .. });
    let access_open = matches!(lock.reason.as_str(), "emergency" | "pause_access");
    let after_day_end = d.after_day_end(now, cfg);
    let can = Can {
        start_day: d.mode == Mode::Plan
            && d.started_at.is_none()
            && d.kind() != crate::config::DayKind::Off
            && d.plan.iter().any(|b| b.minutes > 0),
        raise: if d.started_at.is_some() && d.after_day_end(now, cfg) {
            vec![]
        } else {
            [crate::config::DayKind::Light, crate::config::DayKind::Full].into_iter().filter(|k| *k > d.kind()).collect()
        },
        pause: running && !lunch_break,
        resume: paused,
        start_next: matches!(d.phase, Phase::Await { .. } | Phase::Break { .. } | Phase::Lunch { .. }),
        lunch: d.can_lunch(),
        single: d.mode == Mode::Plan && matches!(d.phase, Phase::Idle | Phase::Done),
        stop_single: d.mode == Mode::Single,
        emergency: lock.base && !access_open,
        extend_access: cfg.pause_access && paused && lock.base,
        end_access: access_open,
        edit_pause_access: !lock.base,
    };

    View {
        now,
        date: d.date.format("%Y-%m-%d").to_string(),
        weekday: clock::weekday_index(now, cfg.tz_offset_min),
        study_day: d.study_day,
        day_kind: d.kind(),
        raised_from: d.raised_from,
        started: d.started_at.is_some(),
        completed: d.completed_at.is_some(),
        after_day_end,
        day_end: fmt_hm(d.day_end_min(cfg)),
        mode: d.mode,
        planned_ms: d.plan.iter().map(|b| b.total_ms()).sum(),
        work_ms: blocks.iter().map(|b| b.work_ms).sum(),
        phase,
        blocks,
        lock,
        pause,
        emergency_count: d.emergencies.len(),
        lunch_used: d.lunch.is_some(),
        single: if d.mode == Mode::Single { d.singles.last().cloned() } else { None },
        can,
    }
}
