//! Read model for the UI, the mini window, the tray and MCP.

use serde::Serialize;

use crate::clock::{Ts, MIN};
use crate::config::{fmt_hm, Config, DayKind};
use crate::day::{fmt_day_min, BreakKind, DayState, Forecast, LockState, Mode, Phase, MAX_DAY_END_MIN};

#[derive(Debug, Clone, Serialize, Default)]
pub struct PhaseView {
    /// idle | work | break | lunch_break | await | lunch | segment | done
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
    /// Segment: its end is a loud alarm (a nap). Never rings while `prep`.
    pub alarm: bool,
    /// Segment: what is queued after it ("Сон 20 мин").
    pub queue: Vec<String>,
    /// Segment: still getting ready — waits for "Лёг"; `dur_ms` / `remaining_ms` are the
    /// preparation (negative remaining = the preparation is over, time to lie down).
    pub prep: bool,
    /// Segment: "без времени" — a stopwatch; `elapsed_ms` counts up, `remaining_ms` < 0 = over
    /// the usual length.
    pub stopwatch: bool,
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
    pub note: Option<String>,
    /// study | break (a planned segment: no parts, no work)
    pub kind: crate::day::ItemKind,
    /// A planned segment waiting in the queue (taken, will run next).
    pub queued: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingNote {
    pub block: usize,
    pub name: String,
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
    /// Today's day end can still move later (quick "+30 мин / +1 ч" buttons).
    pub extend_day_end: bool,
    /// Today's day end can be set to any time (later or earlier) for today.
    pub set_day_end: bool,
    /// Today may switch to a lighter kind (outside of the lock).
    pub lighter_kind: bool,
    /// The current block was started and can be closed on the minutes worked.
    pub finish_block: bool,
    /// A break was just skipped: "Отменить" works until `undo_until`.
    pub undo_skip: bool,
    /// "Отрезок": start a segment now, or queue one behind the running segment.
    pub segment: bool,
    /// "Закончил" / "Встал" (on the preparation: "не буду спать").
    pub end_segment: bool,
    /// "Лёг": the running segment is still getting ready.
    pub lay_down: bool,
    /// The running segment may switch between a countdown and a stopwatch.
    pub segment_mode: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub now: Ts,
    pub date: String,
    pub weekday: usize,
    pub study_day: bool,
    pub kind: DayKind,
    pub started: bool,
    pub completed: bool,
    pub after_day_end: bool,
    /// Today's day end ("23:00"); may differ from the template after a one-off shift.
    pub day_end: String,
    /// Effective day end and the current time, minutes after the day's own midnight
    /// (both may run past 1440 when the day end was moved into the night).
    pub day_end_min: u32,
    pub now_min: u32,
    /// Day end from the settings (without today's shift).
    pub day_end_base: String,
    pub day_end_at: Ts,
    pub day_end_next_day: bool,
    pub day_end_changed: bool,
    /// Does the rest of the plan fit before today's day end?
    pub forecast: Forecast,
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
    /// End of the "undo skipped break" window.
    pub undo_until: Option<Ts>,
    /// A closed block waits for its end-of-block line.
    pub pending_note: Option<PendingNote>,
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
            ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
            }
        }
        Phase::Segment { rec, next } => {
            let r = &d.segments[*rec];
            let (name, part, parts) = d.part_info(*next);
            let queue: Vec<String> = d
                .segment_queue
                .iter()
                .map(|q| if q.open_ended { format!("{} без времени", q.name) } else { format!("{} {} мин", q.name, q.minutes) })
                .collect();
            let prep = r.in_prep();
            let (dur_ms, elapsed) = if prep { (r.prep_min as i64 * MIN, now - r.start) } else { (r.planned_ms(), now - r.countdown_from()) };
            PhaseView {
                kind: "segment".into(),
                title: r.name.clone(),
                subtitle: if prep {
                    format!("Подготовка · потом {} {} мин — нажми «Лёг», когда ляжешь", r.name.to_lowercase(), r.planned_min)
                } else if !queue.is_empty() {
                    format!("Потом: {}", queue.join(" → "))
                } else if d.first_open_block().is_some() && parts > 0 {
                    format!("Дальше: {name}, часть {part} из {parts}")
                } else {
                    "Учебные блоки на сегодня закрыты".into()
                },
                block: current,
                dur_ms,
                elapsed_ms: elapsed,
                // Negative = over time.
                remaining_ms: dur_ms - elapsed,
                running: true,
                paused: false,
                waiting_ms: 0,
                alarm: r.alarm,
                queue,
                prep,
                stopwatch: r.open_ended,
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
            ..Default::default()
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
            note: d.progress[i].note.clone(),
            kind: b.kind,
            queued: d.segment_queue.iter().any(|q| q.plan_item == Some(i)),
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
    let day_end = d.day_end(cfg);
    let can = Can {
        start_day: d.mode == Mode::Plan && d.started_at.is_none() && d.first_open_block().is_some(),
        segment: d.can_start_segment(),
        end_segment: matches!(d.phase, Phase::Segment { .. }),
        lay_down: d.segment().is_some_and(|r| r.in_prep()),
        segment_mode: d.segment().is_some_and(|r| !r.in_prep() && !r.alarm),
        pause: running && !lunch_break,
        resume: paused,
        start_next: matches!(d.phase, Phase::Await { .. } | Phase::Break { .. } | Phase::Lunch { .. }),
        lunch: d.can_lunch(),
        single: d.mode == Mode::Plan && matches!(d.phase, Phase::Idle | Phase::Done),
        stop_single: d.mode == Mode::Single,
        emergency: lock.base && !access_open,
        extend_access: cfg.pause_access && paused && lock.base && lock.focus_until.is_none(),
        end_access: access_open,
        edit_pause_access: !lock.base,
        extend_day_end: d.study_day
            && d.mode == Mode::Plan
            && d.completed_at.is_none()
            && d.is_live(now, cfg)
            && day_end < MAX_DAY_END_MIN,
        lighter_kind: !d.plan_lock(now, cfg),
        set_day_end: d.mode == Mode::Plan && d.is_live(now, cfg),
        undo_skip: d.skip_undo.as_ref().is_some_and(|u| now - u.at <= crate::day::SKIP_UNDO_MS)
            && matches!(d.phase, Phase::Work { since: Some(_), .. })
            && d.pause.is_none(),
        finish_block: d.finish_target(None).is_ok_and(|i| d.block_work_live(i, now) >= MIN),
    };

    View {
        now,
        date: d.date.format("%Y-%m-%d").to_string(),
        weekday: d.weekday(),
        study_day: d.study_day,
        kind: d.kind,
        started: d.started_at.is_some(),
        completed: d.completed_at.is_some(),
        after_day_end,
        day_end: fmt_day_min(day_end),
        day_end_min: day_end,
        now_min: ((now - d.day_end_at(cfg)) / MIN + day_end as i64).max(0) as u32,
        day_end_base: fmt_hm(cfg.day_end_min),
        day_end_at: d.day_end_at(cfg),
        day_end_next_day: day_end >= 24 * 60,
        day_end_changed: day_end != cfg.day_end_min,
        forecast: d.forecast(now, cfg),
        mode: d.mode,
        planned_ms: d.plan.iter().filter(|b| !b.is_break()).map(|b| b.total_ms()).sum(),
        work_ms: blocks.iter().map(|b| b.work_ms).sum(),
        phase,
        blocks,
        lock,
        pause,
        emergency_count: d.emergencies.len(),
        lunch_used: d.lunch.is_some(),
        single: if d.mode == Mode::Single { d.singles.last().cloned() } else { None },
        pending_note: d.pending_note().map(|i| PendingNote { block: i, name: d.plan[i].name.clone() }),
        undo_until: d.skip_undo.as_ref().map(|u| u.at + crate::day::SKIP_UNDO_MS).filter(|_| can.undo_skip),
        can,
    }
}
