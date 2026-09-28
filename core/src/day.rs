//! The study day: plan, timer state machine, pauses, access windows and the raw log.
//!
//! Everything is driven by explicit timestamps so the engine is deterministic and testable.
//! The app calls [`DayState::tick`] a few times per second and reacts to returned [`Event`]s.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::clock::{self, Ts, MIN};
use crate::config::{Config, Timing};

/// A remainder shorter than this is merged into the previous work segment (50 min -> one part).
const MERGE_TAIL_MS: i64 = 15 * MIN;
const PAUSE_REMINDER_MS: i64 = 5 * MIN;
const ACCESS_WARNING_MS: i64 = 2 * MIN;
const MAX_PLAN_BLOCKS: usize = 12;
const MAX_BLOCK_MIN: u32 = 8 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanBlock {
    pub name: String,
    pub minutes: u32,
}

impl PlanBlock {
    pub fn new(name: &str, minutes: u32) -> Self {
        Self { name: name.into(), minutes }
    }
    pub fn normalize(&mut self) {
        let name: String = self.name.trim().chars().take(40).collect();
        self.name = if name.is_empty() { "Блок".into() } else { name };
        self.minutes = self.minutes.min(MAX_BLOCK_MIN);
    }
    pub fn total_ms(&self) -> i64 {
        self.minutes as i64 * MIN
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BlockProgress {
    pub work_ms: i64,
    pub parts_done: u32,
    pub started_at: Option<Ts>,
    pub completed_at: Option<Ts>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Plan,
    Single,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BreakKind {
    Short,
    Between,
    Lunch { at_pc: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Phase {
    /// Nothing is running.
    Idle,
    /// Work segment of `block` (plan index, or 0 in single mode).
    Work { block: usize, dur_ms: i64, elapsed_ms: i64, since: Option<Ts> },
    /// Break; `next` is the block whose part starts afterwards.
    Break { brk: BreakKind, dur_ms: i64, elapsed_ms: i64, since: Option<Ts>, next: usize },
    /// Break is over, waiting for the user to press "start".
    Await { next: usize, since: Ts, reminded_at: Ts },
    /// Lunch without a timer ("just ate"); blocking stays on.
    Lunch { since: Ts, next: usize },
    /// All blocks of the plan are done.
    Done,
}

impl Phase {
    fn running_since(&self) -> Option<Ts> {
        match self {
            Phase::Work { since, .. } | Phase::Break { since, .. } => *since,
            _ => None,
        }
    }
    pub fn is_work(&self) -> bool {
        matches!(self, Phase::Work { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SingleCfg {
    pub work_min: u32,
    /// 0 = no break, next round starts on button press.
    pub break_min: u32,
    /// Enforce the block list while the single timer runs.
    pub block: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SingleRun {
    pub cfg: SingleCfg,
    pub started_at: Ts,
    pub ended_at: Option<Ts>,
    pub work_ms: i64,
    pub rounds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccessWindow {
    pub from: Ts,
    pub until: Ts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivePause {
    pub since: Ts,
    pub what: String,
    pub block: Option<usize>,
    pub access: Vec<AccessWindow>,
    pub reminded_at: Ts,
    pub warned: bool,
    pub expired_notified: bool,
}

impl ActivePause {
    pub fn access_until(&self) -> Option<Ts> {
        self.access.last().map(|w| w.until)
    }
    pub fn access_active(&self, now: Ts) -> bool {
        self.access_until().is_some_and(|u| now < u)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PauseRecord {
    pub start: Ts,
    pub end: Ts,
    /// "work" | "break" | "lunch"
    pub what: String,
    pub block: Option<usize>,
    pub access_ms: i64,
    pub extensions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Emergency {
    pub at: Ts,
    pub until: Ts,
    #[serde(default)]
    pub ended_early: bool,
    #[serde(default)]
    pub notified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LunchRecord {
    pub with_timer: bool,
    pub at_pc: bool,
    pub start: Ts,
    pub end: Option<Ts>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogEvent {
    pub ts: Ts,
    pub kind: String,
    pub text: String,
}

/// Something the app should react to (sound, notification, overlay).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    WorkEnded { block_name: String, part: u32, parts: u32, block_done: bool },
    BlockCompleted { block_name: String, work_ms: i64, pauses: u32, pause_ms: i64 },
    DayCompleted { work_ms: i64 },
    BreakEnded { next_name: String, next_part: u32, next_parts: u32, lunch: bool },
    AwaitReminder { waiting_ms: i64, next_name: String, next_part: u32, next_parts: u32 },
    PauseReminder { paused_ms: i64 },
    PauseAccessWarning { left_ms: i64 },
    PauseAccessExpired,
    EmergencyEnded,
    DayEndReached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayState {
    pub date: NaiveDate,
    pub study_day: bool,
    pub plan: Vec<PlanBlock>,
    pub progress: Vec<BlockProgress>,
    pub timing: Timing,
    pub started_at: Option<Ts>,
    pub completed_at: Option<Ts>,
    pub mode: Mode,
    pub phase: Phase,
    pub pause: Option<ActivePause>,
    #[serde(default)]
    pub pauses: Vec<PauseRecord>,
    #[serde(default)]
    pub emergencies: Vec<Emergency>,
    #[serde(default)]
    pub lunch: Option<LunchRecord>,
    #[serde(default)]
    pub singles: Vec<SingleRun>,
    #[serde(default)]
    pub events: Vec<LogEvent>,
    #[serde(default)]
    pub day_end_notified: bool,
    #[serde(default)]
    pub saved_at: Ts,
}

pub fn next_segment_ms(total_ms: i64, done_ms: i64, seg_ms: i64) -> i64 {
    let rem = total_ms - done_ms;
    if rem <= 0 {
        return 0;
    }
    let tail = rem - seg_ms;
    if tail > 0 && tail < MERGE_TAIL_MS {
        rem
    } else {
        rem.min(seg_ms)
    }
}

pub fn count_segments(total_ms: i64, seg_ms: i64) -> u32 {
    let mut done = 0;
    let mut n = 0;
    loop {
        let s = next_segment_ms(total_ms, done, seg_ms);
        if s <= 0 {
            return n;
        }
        done += s;
        n += 1;
    }
}

fn norm_phrase(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl DayState {
    pub fn new(now: Ts, cfg: &Config) -> Self {
        let plan = cfg.plan_template.clone();
        Self {
            date: clock::local_date(now, cfg.tz_offset_min),
            study_day: cfg.study_days[clock::weekday_index(now, cfg.tz_offset_min)],
            progress: vec![BlockProgress::default(); plan.len()],
            plan,
            timing: cfg.timing.clone(),
            started_at: None,
            completed_at: None,
            mode: Mode::Plan,
            phase: Phase::Idle,
            pause: None,
            pauses: vec![],
            emergencies: vec![],
            lunch: None,
            singles: vec![],
            events: vec![],
            day_end_notified: false,
            saved_at: now,
        }
    }

    /// State loaded from disk after the app was closed: a running timer becomes paused
    /// (time while the app was not running is never counted).
    pub fn restore(&mut self, now: Ts, cfg: &Config) {
        let saved = self.saved_at.max(self.phase.running_since().unwrap_or(0));
        let was_running = self.phase.running_since().is_some();
        if was_running {
            let saved = saved.min(now);
            match &mut self.phase {
                Phase::Work { elapsed_ms, since, dur_ms, .. } | Phase::Break { elapsed_ms, since, dur_ms, .. } => {
                    if let Some(s) = *since {
                        *elapsed_ms = (*elapsed_ms + (saved - s).max(0)).min(*dur_ms);
                    }
                    *since = None;
                }
                _ => {}
            }
            if self.pause.is_none() {
                let what = self.phase_what();
                self.pause = Some(ActivePause {
                    since: saved,
                    what,
                    block: self.current_block(),
                    access: vec![],
                    reminded_at: now,
                    warned: false,
                    expired_notified: true,
                });
            }
            self.log(now, "restore", "Приложение было закрыто — таймер поставлен на паузу");
        }
        // Access windows never survive a restart.
        if let Some(p) = &mut self.pause {
            if let Some(w) = p.access.last_mut() {
                w.until = w.until.min(now);
            }
        }
        if self.started_at.is_none() {
            self.study_day = cfg.study_days[clock::weekday_index(now, cfg.tz_offset_min)];
        }
    }

    /// Close the day for good (date rollover): partial work is credited, open pause is closed.
    pub fn finalize(&mut self, end: Ts) {
        if self.mode == Mode::Single {
            let _ = self.stop_single(end);
        }
        if let Some(p) = self.pause.take() {
            self.close_pause(p, end);
        }
        if let Phase::Work { block, .. } = self.phase {
            let partial = self.phase_elapsed(end);
            if let Some(p) = self.progress.get_mut(block) {
                p.work_ms += partial;
            }
        }
        if let Some(l) = &mut self.lunch {
            l.end.get_or_insert(end);
        }
        if !matches!(self.phase, Phase::Idle | Phase::Done) {
            self.phase = Phase::Idle;
            self.log(end, "day_close", "День закрыт при смене даты");
        }
        self.saved_at = end;
    }

    pub fn log(&mut self, ts: Ts, kind: &str, text: impl Into<String>) {
        self.events.push(LogEvent { ts, kind: kind.into(), text: text.into() });
    }

    // ---------- derived facts ----------

    pub fn seg_ms(&self) -> i64 {
        self.timing.work_segment_min as i64 * MIN
    }

    pub fn current_block(&self) -> Option<usize> {
        if self.mode != Mode::Plan {
            return None;
        }
        match self.phase {
            Phase::Work { block, .. } => Some(block),
            Phase::Break { next, .. } | Phase::Await { next, .. } | Phase::Lunch { next, .. } => Some(next),
            _ => None,
        }
    }

    fn phase_what(&self) -> String {
        match &self.phase {
            Phase::Work { .. } => "work".into(),
            Phase::Break { brk: BreakKind::Lunch { .. }, .. } => "lunch".into(),
            _ => "break".into(),
        }
    }

    /// Live elapsed ms of the running work/break phase.
    pub fn phase_elapsed(&self, now: Ts) -> i64 {
        match &self.phase {
            Phase::Work { elapsed_ms, since, dur_ms, .. } | Phase::Break { elapsed_ms, since, dur_ms, .. } => {
                (elapsed_ms + since.map(|s| (now - s).max(0)).unwrap_or(0)).min(*dur_ms)
            }
            _ => 0,
        }
    }

    /// Work ms of a plan block including the running segment.
    pub fn block_work_live(&self, i: usize, now: Ts) -> i64 {
        let base = self.progress.get(i).map(|p| p.work_ms).unwrap_or(0);
        match self.phase {
            Phase::Work { block, .. } if self.mode == Mode::Plan && block == i => base + self.phase_elapsed(now),
            _ => base,
        }
    }

    pub fn block_parts(&self, i: usize) -> u32 {
        self.plan.get(i).map(|b| count_segments(b.total_ms(), self.seg_ms())).unwrap_or(0)
    }

    pub fn is_block_done(&self, i: usize) -> bool {
        self.progress.get(i).is_some_and(|p| p.completed_at.is_some())
    }

    fn first_open_block(&self) -> Option<usize> {
        (0..self.plan.len()).find(|&i| !self.is_block_done(i) && self.plan[i].minutes > 0)
    }

    pub fn after_day_end(&self, now: Ts, cfg: &Config) -> bool {
        clock::local_date(now, cfg.tz_offset_min) != self.date
            || clock::minute_of_day(now, cfg.tz_offset_min) >= cfg.day_end_min
    }

    /// The study-day lock (before exemptions like pause access or emergency).
    pub fn plan_lock(&self, now: Ts, cfg: &Config) -> bool {
        self.study_day && self.started_at.is_some() && self.completed_at.is_none() && !self.after_day_end(now, cfg)
    }

    fn single_lock(&self) -> bool {
        self.mode == Mode::Single
            && self.singles.last().is_some_and(|s| s.cfg.block && s.ended_at.is_none())
            && !matches!(self.phase, Phase::Idle | Phase::Done)
    }

    pub fn base_lock(&self, now: Ts, cfg: &Config) -> bool {
        self.plan_lock(now, cfg) || self.single_lock()
    }

    /// Final decision: should blocked sites/apps be blocked right now?
    pub fn lock_state(&self, now: Ts, cfg: &Config) -> LockState {
        if !self.base_lock(now, cfg) {
            let reason = if self.single_lock() {
                "single"
            } else if !self.study_day {
                "not_study_day"
            } else if self.started_at.is_none() {
                "not_started"
            } else if self.completed_at.is_some() {
                "completed"
            } else {
                "day_end"
            };
            return LockState { blocked: false, base: false, reason: reason.into(), until: None };
        }
        if let Some(e) = self.emergencies.iter().rev().find(|e| now < e.until) {
            return LockState { blocked: false, base: true, reason: "emergency".into(), until: Some(e.until) };
        }
        if let Some(p) = &self.pause {
            if p.access_active(now) {
                return LockState { blocked: false, base: true, reason: "pause_access".into(), until: p.access_until() };
            }
        }
        if let Phase::Break { brk: BreakKind::Lunch { at_pc: true }, dur_ms, elapsed_ms, since: Some(s), .. } = self.phase {
            return LockState {
                blocked: false,
                base: true,
                reason: "lunch_at_pc".into(),
                until: Some(s + (dur_ms - elapsed_ms)),
            };
        }
        LockState { blocked: true, base: true, reason: if self.single_lock() { "single" } else { "study" }.into(), until: None }
    }

    // ---------- commands ----------

    fn start_work(&mut self, now: Ts, block: usize) {
        let dur_ms = match self.mode {
            Mode::Plan => {
                let p = &mut self.progress[block];
                if p.started_at.is_none() {
                    p.started_at = Some(now);
                }
                next_segment_ms(self.plan[block].total_ms(), p.work_ms, self.seg_ms())
            }
            Mode::Single => self.singles.last().map(|s| s.cfg.work_min as i64 * MIN).unwrap_or(0),
        };
        self.phase = Phase::Work { block, dur_ms, elapsed_ms: 0, since: Some(now) };
    }

    pub fn start_day(&mut self, now: Ts, cfg: &Config) -> Result<(), String> {
        if self.mode != Mode::Plan || self.started_at.is_some() {
            return Err("День уже начат.".into());
        }
        let first = self.first_open_block().ok_or("План на сегодня пустой — добавь хотя бы один блок.")?;
        self.timing = cfg.timing.clone();
        self.study_day = cfg.study_days[clock::weekday_index(now, cfg.tz_offset_min)];
        self.started_at = Some(now);
        self.start_work(now, first);
        let msg = if self.plan_lock(now, cfg) { "День начат, блокировка включена" } else { "День начат (без блокировки)" };
        self.log(now, "day_start", msg);
        Ok(())
    }

    pub fn pause(&mut self, now: Ts, cfg: &Config) -> Result<(), String> {
        if self.pause.is_some() {
            return Err("Уже на паузе.".into());
        }
        let what = self.phase_what();
        match &mut self.phase {
            Phase::Work { elapsed_ms, since, .. } | Phase::Break { elapsed_ms, since, .. } => {
                let s = since.take().ok_or("Таймер не идёт.")?;
                *elapsed_ms += (now - s).max(0);
            }
            _ => return Err("Сейчас нечего ставить на паузу.".into()),
        }
        let mut access = vec![];
        if cfg.pause_access && self.base_lock(now, cfg) {
            access.push(AccessWindow { from: now, until: now + cfg.pause_access_min as i64 * MIN });
        }
        let granted = !access.is_empty();
        self.pause = Some(ActivePause {
            since: now,
            what,
            block: self.current_block(),
            access,
            reminded_at: now,
            warned: false,
            expired_notified: false,
        });
        self.log(
            now,
            "pause",
            if granted { format!("Пауза, доступ открыт на {} мин", cfg.pause_access_min) } else { "Пауза".into() },
        );
        Ok(())
    }

    pub fn resume(&mut self, now: Ts) -> Result<(), String> {
        let p = self.pause.take().ok_or("Таймер не на паузе.")?;
        match &mut self.phase {
            Phase::Work { since, .. } | Phase::Break { since, .. } => *since = Some(now),
            _ => {}
        }
        self.close_pause(p, now);
        self.log(now, "resume", "Продолжение");
        Ok(())
    }

    fn close_pause(&mut self, p: ActivePause, now: Ts) {
        let access_ms = p.access.iter().map(|w| (w.until.min(now) - w.from).max(0)).sum();
        self.pauses.push(PauseRecord {
            start: p.since,
            end: now,
            what: p.what,
            block: p.block,
            access_ms,
            extensions: p.access.len().saturating_sub(1) as u32,
        });
    }

    /// Extend pause access (the app checks the captcha before calling this).
    pub fn extend_pause_access(&mut self, now: Ts, cfg: &Config) -> Result<Ts, String> {
        if !cfg.pause_access {
            return Err("Доступ на паузе выключен в настройках.".into());
        }
        if !self.base_lock(now, cfg) {
            return Err("Блокировка сейчас не действует.".into());
        }
        let p = self.pause.as_mut().ok_or("Продлить можно только во время паузы.")?;
        let from = p.access_until().map(|u| u.max(now)).unwrap_or(now);
        let until = from + cfg.pause_access_min as i64 * MIN;
        p.access.push(AccessWindow { from, until });
        p.warned = false;
        p.expired_notified = false;
        let n = p.access.len() - 1;
        self.log(now, "pause_extend", format!("Доступ на паузе продлён (#{n}) до {}", clock::hm(until, cfg.tz_offset_min)));
        Ok(until)
    }

    /// Start the next part from a break / waiting / lunch state.
    pub fn start_next(&mut self, now: Ts) -> Result<(), String> {
        if self.pause.is_some() {
            if let Some(p) = self.pause.take() {
                self.close_pause(p, now);
            }
        }
        let next = match &self.phase {
            Phase::Break { next, brk, .. } => {
                let n = *next;
                if matches!(brk, BreakKind::Lunch { .. }) {
                    self.finish_lunch_record(now);
                }
                self.log(now, "break_skip", "Перерыв завершён досрочно");
                n
            }
            Phase::Await { next, .. } => *next,
            Phase::Lunch { next, .. } => {
                let n = *next;
                self.finish_lunch_record(now);
                n
            }
            _ => return Err("Сейчас нельзя начать следующую часть.".into()),
        };
        if self.mode == Mode::Plan && self.is_block_done(next) {
            return match self.first_open_block() {
                Some(b) => {
                    self.start_work(now, b);
                    Ok(())
                }
                None => {
                    self.phase = Phase::Done;
                    Ok(())
                }
            };
        }
        self.start_work(now, next);
        Ok(())
    }

    fn finish_lunch_record(&mut self, now: Ts) {
        if let Some(l) = &mut self.lunch {
            if l.end.is_none() {
                l.end = Some(now);
            }
        }
    }

    pub fn can_lunch(&self) -> bool {
        self.mode == Mode::Plan
            && self.lunch.is_none()
            && self.started_at.is_some()
            && matches!(self.phase, Phase::Break { brk: BreakKind::Short | BreakKind::Between, .. } | Phase::Await { .. })
    }

    pub fn start_lunch(&mut self, now: Ts, with_timer: bool, at_pc: bool) -> Result<(), String> {
        if !self.can_lunch() {
            return Err(if self.lunch.is_some() {
                "Обед сегодня уже был.".into()
            } else {
                "Обед можно начать только в перерыве или перед следующей частью.".into()
            });
        }
        if let Some(p) = self.pause.take() {
            self.close_pause(p, now);
        }
        let next = self.current_block().unwrap_or(0);
        let at_pc = with_timer && at_pc;
        self.lunch = Some(LunchRecord { with_timer, at_pc, start: now, end: None });
        if with_timer {
            let dur_ms = self.timing.lunch_min as i64 * MIN;
            self.phase = Phase::Break { brk: BreakKind::Lunch { at_pc }, dur_ms, elapsed_ms: 0, since: Some(now), next };
            self.log(
                now,
                "lunch",
                format!("Обед {} мин{}", self.timing.lunch_min, if at_pc { ", за ПК — доступ открыт" } else { "" }),
            );
        } else {
            self.phase = Phase::Lunch { since: now, next };
            self.log(now, "lunch", "Обед без таймера");
        }
        Ok(())
    }

    pub fn emergency(&mut self, now: Ts, cfg: &Config, phrase: &str) -> Result<Ts, String> {
        if !self.base_lock(now, cfg) {
            return Err("Блокировка сейчас не действует.".into());
        }
        if self.emergencies.iter().any(|e| now < e.until) {
            return Err("Аварийный доступ уже открыт.".into());
        }
        if norm_phrase(phrase) != norm_phrase(&cfg.emergency_phrase) {
            return Err("Фраза не совпадает. Введи её точно, символ в символ.".into());
        }
        let until = now + cfg.emergency_min as i64 * MIN;
        self.emergencies.push(Emergency { at: now, until, ended_early: false, notified: false });
        let n = self.emergencies.len();
        self.log(now, "emergency", format!("Аварийный доступ #{n} на {} мин", cfg.emergency_min));
        Ok(until)
    }

    pub fn end_access_early(&mut self, now: Ts) -> Result<(), String> {
        let mut any = false;
        if let Some(e) = self.emergencies.iter_mut().rev().find(|e| now < e.until) {
            e.until = now;
            e.ended_early = true;
            e.notified = true;
            any = true;
        }
        if let Some(p) = &mut self.pause {
            if let Some(w) = p.access.last_mut() {
                if now < w.until {
                    w.until = now;
                    p.expired_notified = true;
                    any = true;
                }
            }
        }
        if any {
            self.log(now, "access_end", "Доступ закрыт вручную");
            Ok(())
        } else {
            Err("Доступ и так закрыт.".into())
        }
    }

    pub fn start_single(&mut self, now: Ts, cfg: SingleCfg) -> Result<(), String> {
        if self.mode == Mode::Single {
            return Err("Одиночный таймер уже идёт.".into());
        }
        if !matches!(self.phase, Phase::Idle | Phase::Done) {
            return Err("Сначала закончи план дня — одиночный таймер доступен, когда день не идёт.".into());
        }
        let cfg = SingleCfg { work_min: cfg.work_min.clamp(1, 240), break_min: cfg.break_min.min(120), block: cfg.block };
        self.log(
            now,
            "single_start",
            format!("Одиночный таймер {} мин{}", cfg.work_min, if cfg.break_min > 0 { format!(" + перерыв {}", cfg.break_min) } else { String::new() }),
        );
        self.singles.push(SingleRun { cfg, started_at: now, ended_at: None, work_ms: 0, rounds: 0 });
        self.mode = Mode::Single;
        self.start_work(now, 0);
        Ok(())
    }

    pub fn stop_single(&mut self, now: Ts) -> Result<(), String> {
        if self.mode != Mode::Single {
            return Err("Одиночный таймер не запущен.".into());
        }
        if let Some(p) = self.pause.take() {
            self.close_pause(p, now);
        }
        let partial = if let Phase::Work { .. } = self.phase { self.phase_elapsed(now) } else { 0 };
        if let Some(s) = self.singles.last_mut() {
            s.work_ms += partial;
            s.ended_at = Some(now);
        }
        self.mode = Mode::Plan;
        self.phase = if self.completed_at.is_some() { Phase::Done } else { Phase::Idle };
        self.log(now, "single_stop", "Одиночный таймер остановлен");
        Ok(())
    }

    /// Replace today's plan. While `locked`, blocks can only be added or extended.
    pub fn set_plan(&mut self, now: Ts, mut plan: Vec<PlanBlock>, locked: bool) -> Result<(), String> {
        for b in &mut plan {
            b.normalize();
        }
        if plan.len() > MAX_PLAN_BLOCKS {
            return Err(format!("Не больше {MAX_PLAN_BLOCKS} блоков в день."));
        }
        if plan.iter().any(|b| b.minutes == 0) {
            return Err("У каждого блока должна быть длительность.".into());
        }
        let started = self.started_at.is_some();
        if locked {
            if plan.len() < self.plan.len() {
                return Err("Во время блокировки блоки можно только добавлять.".into());
            }
            for (i, old) in self.plan.iter().enumerate() {
                if plan[i].minutes < old.minutes {
                    return Err(format!("«{}»: во время блокировки время можно только увеличить.", old.name));
                }
                if self.progress[i].started_at.is_some() && plan[i].name != old.name {
                    return Err(format!("«{}» уже начат — переименовать нельзя.", old.name));
                }
            }
        } else if started {
            // Blocks with recorded work must stay where they are so the log stays truthful.
            for (i, p) in self.progress.iter().enumerate() {
                if p.work_ms > 0 && plan.get(i).map(|b| &b.name) != Some(&self.plan[i].name) {
                    return Err(format!("«{}» уже начат — его нельзя удалить или переставить.", self.plan[i].name));
                }
            }
        }
        if !started && plan.is_empty() {
            // allowed: empty plan before the day starts
        } else if started && plan.is_empty() {
            return Err("План начатого дня не может быть пустым.".into());
        }
        let mut progress = self.progress.clone();
        progress.resize(plan.len(), BlockProgress::default());
        self.plan = plan;
        self.progress = progress;
        // Re-evaluate completion after the change.
        for i in 0..self.plan.len() {
            let total = self.plan[i].total_ms();
            let p = &mut self.progress[i];
            if p.completed_at.is_some() && p.work_ms < total {
                p.completed_at = None;
            }
            if p.completed_at.is_none() && p.work_ms >= total && p.started_at.is_some() {
                p.completed_at = Some(now);
            }
        }
        let current_valid = match self.phase {
            Phase::Work { block, .. } if self.mode == Mode::Plan => block < self.plan.len(),
            Phase::Break { next, .. } | Phase::Await { next, .. } | Phase::Lunch { next, .. } if self.mode == Mode::Plan => {
                next < self.plan.len()
            }
            _ => true,
        };
        if !current_valid {
            self.phase = match self.first_open_block() {
                Some(next) => Phase::Await { next, since: now, reminded_at: now },
                None => Phase::Done,
            };
        }
        if started && self.mode == Mode::Plan {
            match (self.first_open_block(), &self.phase) {
                (Some(next), Phase::Done) => {
                    self.completed_at = None;
                    self.phase = Phase::Await { next, since: now, reminded_at: now };
                }
                (Some(_), _) => self.completed_at = None,
                (None, Phase::Await { .. }) => {
                    self.phase = Phase::Done;
                    self.completed_at.get_or_insert(now);
                }
                _ => {}
            }
        }
        self.log(now, "plan", format!("План: {}", self.plan.iter().map(|b| format!("{} {} мин", b.name, b.minutes)).collect::<Vec<_>>().join(" · ")));
        Ok(())
    }

    // ---------- time ----------

    pub fn tick(&mut self, now: Ts, cfg: &Config) -> Vec<Event> {
        let mut ev = vec![];
        for _ in 0..64 {
            if !self.advance(now, cfg, &mut ev) {
                break;
            }
        }
        // After the day ends nobody should be woken up by a forgotten plan timer.
        let quiet = self.mode == Mode::Plan && self.after_day_end(now, cfg);
        if let Phase::Await { since, reminded_at, next } = self.phase {
            if !quiet && now - reminded_at >= cfg.reminder_sec as i64 * 1000 {
                self.phase = Phase::Await { since, reminded_at: now, next };
                let (next_name, next_part, next_parts) = self.part_info(next);
                ev.push(Event::AwaitReminder { waiting_ms: now - since, next_name, next_part, next_parts });
            }
        }
        if let Some(p) = &mut self.pause {
            if let Some(until) = p.access_until() {
                if now < until && until - now <= ACCESS_WARNING_MS && !p.warned {
                    p.warned = true;
                    ev.push(Event::PauseAccessWarning { left_ms: until - now });
                }
                if now >= until && !p.expired_notified {
                    p.expired_notified = true;
                    p.reminded_at = now;
                    ev.push(Event::PauseAccessExpired);
                }
            }
            if !quiet && now - p.reminded_at >= PAUSE_REMINDER_MS {
                p.reminded_at = now;
                ev.push(Event::PauseReminder { paused_ms: now - p.since });
            }
        }
        for e in &mut self.emergencies {
            if now >= e.until && !e.notified {
                e.notified = true;
                ev.push(Event::EmergencyEnded);
            }
        }
        if !self.day_end_notified && self.started_at.is_some() && self.completed_at.is_none() && self.study_day && self.after_day_end(now, cfg) {
            self.day_end_notified = true;
            self.log(now, "day_end", format!("{} — блокировка снята по времени", crate::config::fmt_hm(cfg.day_end_min)));
            ev.push(Event::DayEndReached);
        }
        ev
    }

    /// (name, part number, parts) of the part that starts next in `block`.
    pub fn part_info(&self, block: usize) -> (String, u32, u32) {
        match self.mode {
            Mode::Plan => {
                let name = self.plan.get(block).map(|b| b.name.clone()).unwrap_or_default();
                let parts = self.block_parts(block);
                let done = self.progress.get(block).map(|p| p.parts_done).unwrap_or(0);
                (name, (done + 1).min(parts.max(1)), parts)
            }
            Mode::Single => {
                let r = self.singles.last().map(|s| s.rounds).unwrap_or(0);
                ("Одиночный таймер".into(), r + 1, 0)
            }
        }
    }

    fn advance(&mut self, now: Ts, _cfg: &Config, ev: &mut Vec<Event>) -> bool {
        match self.phase.clone() {
            Phase::Work { block, dur_ms, elapsed_ms, since: Some(s) } => {
                let end = s + (dur_ms - elapsed_ms);
                if now < end {
                    return false;
                }
                self.finish_work(end, block, dur_ms, ev);
                true
            }
            Phase::Break { brk, dur_ms, elapsed_ms, since: Some(s), next } => {
                let end = s + (dur_ms - elapsed_ms);
                if now < end {
                    return false;
                }
                let lunch = matches!(brk, BreakKind::Lunch { .. });
                if lunch {
                    self.finish_lunch_record(end);
                    self.log(end, "lunch_end", "Обед окончен");
                }
                self.phase = Phase::Await { next, since: end, reminded_at: now };
                let (next_name, next_part, next_parts) = self.part_info(next);
                ev.push(Event::BreakEnded { next_name, next_part, next_parts, lunch });
                true
            }
            _ => false,
        }
    }

    fn finish_work(&mut self, end: Ts, block: usize, dur_ms: i64, ev: &mut Vec<Event>) {
        match self.mode {
            Mode::Single => {
                let (brk_min, round) = {
                    let s = self.singles.last_mut().expect("single run");
                    s.work_ms += dur_ms;
                    s.rounds += 1;
                    (s.cfg.break_min, s.rounds)
                };
                self.log(end, "work_end", format!("Одиночный таймер: круг {round} закончен"));
                ev.push(Event::WorkEnded { block_name: "Одиночный таймер".into(), part: round, parts: 0, block_done: false });
                self.phase = if brk_min > 0 {
                    Phase::Break { brk: BreakKind::Short, dur_ms: brk_min as i64 * MIN, elapsed_ms: 0, since: Some(end), next: 0 }
                } else {
                    Phase::Await { next: 0, since: end, reminded_at: end }
                };
            }
            Mode::Plan => {
                let parts = self.block_parts(block);
                let total = self.plan[block].total_ms();
                let name = self.plan[block].name.clone();
                let p = &mut self.progress[block];
                p.work_ms += dur_ms;
                p.parts_done += 1;
                let part = p.parts_done;
                let block_done = p.work_ms >= total;
                if block_done {
                    p.completed_at = Some(end);
                }
                let work_ms = p.work_ms;
                self.log(end, "work_end", format!("{name}: часть {part} из {parts} закончена"));
                ev.push(Event::WorkEnded { block_name: name.clone(), part, parts, block_done });
                if block_done {
                    let (pauses, pause_ms) = self
                        .pauses
                        .iter()
                        .filter(|r| r.block == Some(block))
                        .fold((0u32, 0i64), |(n, ms), r| (n + 1, ms + (r.end - r.start)));
                    self.log(end, "block_done", format!("Блок «{name}» закрыт: {} мин работы", work_ms / MIN));
                    ev.push(Event::BlockCompleted { block_name: name, work_ms, pauses, pause_ms });
                    match self.first_open_block() {
                        Some(next) => {
                            let dur_ms = self.timing.between_blocks_min as i64 * MIN;
                            self.phase = Phase::Break { brk: BreakKind::Between, dur_ms, elapsed_ms: 0, since: Some(end), next };
                        }
                        None => {
                            self.phase = Phase::Done;
                            self.completed_at = Some(end);
                            let total: i64 = self.progress.iter().map(|p| p.work_ms).sum();
                            self.log(end, "day_done", "Все блоки дня закрыты — блокировка снята");
                            ev.push(Event::DayCompleted { work_ms: total });
                        }
                    }
                } else {
                    let dur_ms = self.timing.short_break_min as i64 * MIN;
                    self.phase = Phase::Break { brk: BreakKind::Short, dur_ms, elapsed_ms: 0, since: Some(end), next: block };
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LockState {
    /// Block list is enforced right now.
    pub blocked: bool,
    /// The day/single lock is in force (possibly suspended by an access window).
    pub base: bool,
    /// study | single | emergency | pause_access | lunch_at_pc | not_study_day | not_started | completed | day_end
    pub reason: String,
    pub until: Option<Ts>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn t(s: &str) -> Ts {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }

    // Monday 2026-09-28, 13:00 MSK
    fn start() -> Ts {
        t("2026-09-28T10:00:00Z")
    }

    fn cfg() -> Config {
        Config {
            plan_template: vec![PlanBlock::new("Математика", 90), PlanBlock::new("Экстернат", 150)],
            ..Config::default()
        }
    }

    #[test]
    fn segments() {
        let s = 45 * MIN;
        assert_eq!(count_segments(90 * MIN, s), 2);
        assert_eq!(count_segments(150 * MIN, s), 4);
        assert_eq!(count_segments(50 * MIN, s), 1);
        assert_eq!(next_segment_ms(150 * MIN, 135 * MIN, s), 15 * MIN);
        assert_eq!(next_segment_ms(50 * MIN, 0, s), 50 * MIN);
    }

    #[test]
    fn full_block_cycle_waits_after_break() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        assert!(d.lock_state(start(), &c).blocked);

        let ev = d.tick(start() + 45 * MIN, &c);
        assert!(matches!(ev[0], Event::WorkEnded { part: 1, parts: 2, block_done: false, .. }));
        assert!(matches!(d.phase, Phase::Break { brk: BreakKind::Short, .. }));

        let ev = d.tick(start() + 55 * MIN, &c);
        assert!(matches!(ev[0], Event::BreakEnded { next_part: 2, .. }));
        assert!(matches!(d.phase, Phase::Await { .. }));
        // still blocked while waiting
        assert!(d.lock_state(start() + 56 * MIN, &c).blocked);
        // loud reminder each minute
        let ev = d.tick(start() + 56 * MIN, &c);
        assert!(matches!(ev[0], Event::AwaitReminder { .. }));

        // waiting time is not work time
        d.start_next(start() + 60 * MIN).unwrap();
        let ev = d.tick(start() + 105 * MIN, &c);
        assert!(ev.iter().any(|e| matches!(e, Event::BlockCompleted { work_ms, .. } if *work_ms == 90 * MIN)));
        assert!(matches!(d.phase, Phase::Break { brk: BreakKind::Between, next: 1, .. }));
    }

    #[test]
    fn pause_freezes_time_and_resume_is_manual() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.pause(start() + 10 * MIN, &c).unwrap();
        d.tick(start() + 200 * MIN, &c);
        assert!(d.phase.is_work());
        assert_eq!(d.phase_elapsed(start() + 200 * MIN), 10 * MIN);
        d.resume(start() + 200 * MIN).unwrap();
        assert_eq!(d.pauses[0].end - d.pauses[0].start, 190 * MIN);
        d.tick(start() + 235 * MIN, &c);
        assert!(matches!(d.phase, Phase::Break { .. }));
    }

    #[test]
    fn pause_access_expires_and_extends() {
        let mut c = cfg();
        c.pause_access = true;
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let p = start() + 5 * MIN;
        d.pause(p, &c).unwrap();
        assert!(!d.lock_state(p + MIN, &c).blocked);
        let ev = d.tick(p + 10 * MIN, &c);
        assert!(ev.contains(&Event::PauseAccessExpired));
        assert!(d.lock_state(p + 10 * MIN, &c).blocked);
        d.extend_pause_access(p + 12 * MIN, &c).unwrap();
        assert!(!d.lock_state(p + 13 * MIN, &c).blocked);
        d.resume(p + 15 * MIN).unwrap();
        assert!(d.lock_state(p + 15 * MIN, &c).blocked);
        assert_eq!(d.pauses[0].access_ms, 13 * MIN);
        assert_eq!(d.pauses[0].extensions, 1);
    }

    #[test]
    fn day_done_unlocks_and_22_unlocks() {
        let mut c = cfg();
        c.plan_template = vec![PlanBlock::new("A", 45)];
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let ev = d.tick(start() + 45 * MIN, &c);
        assert!(ev.iter().any(|e| matches!(e, Event::DayCompleted { .. })));
        assert!(!d.lock_state(start() + 46 * MIN, &c).blocked);

        let mut d = DayState::new(start(), &cfg());
        d.start_day(start(), &c).unwrap();
        d.pause(start() + MIN, &c).unwrap();
        let ten_pm = t("2026-09-28T19:00:00Z");
        assert!(d.lock_state(ten_pm - 1, &c).blocked);
        assert!(!d.lock_state(ten_pm, &c).blocked);
        assert!(d.tick(ten_pm, &c).contains(&Event::DayEndReached));
    }

    #[test]
    fn no_reminders_after_day_end() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.tick(start() + 55 * MIN, &c);
        assert!(matches!(d.phase, Phase::Await { .. }));
        let late = t("2026-09-28T19:30:00Z");
        let ev = d.tick(late, &c);
        assert!(!ev.iter().any(|e| matches!(e, Event::AwaitReminder { .. })));
    }

    #[test]
    fn emergency_needs_phrase() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        assert!(d.emergency(start(), &c, "хочу").is_err());
        let until = d.emergency(start(), &c, &format!("  {}  ", c.emergency_phrase)).unwrap();
        assert!(!d.lock_state(start() + MIN, &c).blocked);
        assert!(d.lock_state(until, &c).blocked);
        assert!(d.tick(until, &c).contains(&Event::EmergencyEnded));
    }

    #[test]
    fn locked_plan_only_grows() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let mut shorter = d.plan.clone();
        shorter[1].minutes = 60;
        assert!(d.set_plan(start(), shorter, true).is_err());
        let mut longer = d.plan.clone();
        longer.push(PlanBlock::new("Словацкий", 90));
        assert!(d.set_plan(start(), longer, true).is_ok());
        assert_eq!(d.progress.len(), 3);
    }

    #[test]
    fn lunch_at_pc_opens_access() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.tick(start() + 45 * MIN, &c);
        d.start_lunch(start() + 46 * MIN, true, true).unwrap();
        assert!(!d.lock_state(start() + 50 * MIN, &c).blocked);
        let ev = d.tick(start() + 91 * MIN, &c);
        assert!(matches!(ev[0], Event::BreakEnded { lunch: true, .. }));
        assert!(d.lock_state(start() + 92 * MIN, &c).blocked);
        assert!(d.start_lunch(start() + 92 * MIN, false, false).is_err());
    }

    #[test]
    fn restore_pauses_running_timer() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.saved_at = start() + 20 * MIN;
        let json = serde_json::to_string(&d).unwrap();
        let mut r: DayState = serde_json::from_str(&json).unwrap();
        r.restore(start() + 60 * MIN, &c);
        assert!(r.pause.is_some());
        assert_eq!(r.phase_elapsed(start() + 60 * MIN), 20 * MIN);
        assert!(r.lock_state(start() + 60 * MIN, &c).blocked);
    }

    #[test]
    fn single_timer() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_single(start(), SingleCfg { work_min: 25, break_min: 5, block: true }).unwrap();
        assert!(d.lock_state(start(), &c).blocked);
        d.tick(start() + 25 * MIN, &c);
        assert!(matches!(d.phase, Phase::Break { .. }));
        d.tick(start() + 30 * MIN, &c);
        assert!(matches!(d.phase, Phase::Await { .. }));
        d.stop_single(start() + 31 * MIN).unwrap();
        assert_eq!(d.singles[0].work_ms, 25 * MIN);
        assert!(!d.lock_state(start() + 31 * MIN, &c).blocked);
    }
}
