//! The study day: plan, timer state machine, pauses, access windows and the raw log.
//!
//! Everything is driven by explicit timestamps so the engine is deterministic and testable.
//! The app calls [`DayState::tick`] a few times per second and reacts to returned [`Event`]s.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::clock::{self, Ts, MIN};
use crate::config::{fmt_hm, Config, DayKind, Timing};

/// A remainder shorter than this is merged into the previous work segment (50 min -> one part).
const MERGE_TAIL_MS: i64 = 15 * MIN;
const PAUSE_REMINDER_MS: i64 = 5 * MIN;
const ACCESS_WARNING_MS: i64 = 2 * MIN;
const MAX_PLAN_BLOCKS: usize = 16;
/// A skipped break can be taken back this long (misclick protection).
pub const SKIP_UNDO_MS: i64 = 10 * crate::clock::SEC;
/// Pauses shorter than this are noise (a misclick, pause/resume to check something): they are
/// not recorded and not counted.
pub const MIN_PAUSE_MS: i64 = 10 * crate::clock::SEC;
const MAX_BLOCK_MIN: u32 = 8 * 60;
/// Latest allowed day end: 02:00 of the next day, in minutes after the day's midnight.
pub const MAX_DAY_END_MIN: u32 = 26 * 60;
/// "00:00".."02:00" typed as a day end mean the night after the study day.
const NEXT_DAY_UNTIL_MIN: u32 = 2 * 60;
/// Longest focus lock, counted from now: one call never locks for more than this.
pub const MAX_FOCUS_MIN: u32 = 12 * 60;

/// A plan item is a study block or a planned non-study segment (lunch, nap…).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    #[default]
    Study,
    Break,
}

impl ItemKind {
    pub fn is_study(&self) -> bool {
        *self == ItemKind::Study
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanBlock {
    pub name: String,
    pub minutes: u32,
    /// `"break"` for a planned segment; study blocks leave it out.
    #[serde(default, rename = "type", skip_serializing_if = "ItemKind::is_study")]
    pub kind: ItemKind,
}

impl PlanBlock {
    pub fn new(name: &str, minutes: u32) -> Self {
        Self { name: name.into(), minutes, kind: ItemKind::Study }
    }
    /// A planned segment ("Обед", 45).
    pub fn brk(name: &str, minutes: u32) -> Self {
        Self { name: name.into(), minutes, kind: ItemKind::Break }
    }
    pub fn is_break(&self) -> bool {
        self.kind == ItemKind::Break
    }
    pub fn normalize(&mut self) {
        let name: String = self.name.trim().chars().take(40).collect();
        self.name = if !name.is_empty() {
            name
        } else if self.is_break() {
            "Отрезок".into()
        } else {
            "Блок".into()
        };
        self.minutes = self.minutes.min(if self.is_break() { crate::config::MAX_SEGMENT_MIN } else { MAX_BLOCK_MIN });
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
    /// Closed early by "finish block": done on the minutes actually worked.
    #[serde(default)]
    pub closed: bool,
    /// One line at the end of the block: what was boring, where the mind wandered.
    #[serde(default)]
    pub note: Option<String>,
    /// The note was skipped on purpose (don't ask again).
    #[serde(default)]
    pub note_skipped: bool,
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
    /// Lunch without a timer ("just ate"); blocking stays on. Before 0.3 only.
    Lunch { since: Ts, next: usize },
    /// A non-study segment (`segments[rec]`): wall-clock countdown, then overrun until the user
    /// ends it. `next` is the block whose part starts afterwards.
    Segment { rec: usize, next: usize },
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

/// A segment as it ran (or runs: `end` is `None`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SegmentRecord {
    pub name: String,
    pub planned_min: u32,
    pub alarm: bool,
    #[serde(default)]
    pub open_access: bool,
    pub start: Ts,
    pub end: Option<Ts>,
    /// The plan item it comes from.
    #[serde(default)]
    pub plan_item: Option<usize>,
    #[serde(default)]
    pub warned: bool,
    #[serde(default)]
    pub ended_notified: bool,
    #[serde(default)]
    pub reminded_at: Ts,
    /// Planned preparation before the countdown (a nap: coffee, getting to bed); 0 = none.
    #[serde(default)]
    pub prep_min: u32,
    /// "Лёг": the countdown (and the alarm) runs from here. `None` while getting ready.
    #[serde(default)]
    pub lay_at: Option<Ts>,
    /// No countdown: a stopwatch up until "Закончил". `planned_min` still measures the overrun.
    #[serde(default)]
    pub open_ended: bool,
    /// The "time to lie down" reminder was shown.
    #[serde(default)]
    pub prep_notified: bool,
}

impl SegmentRecord {
    pub fn planned_ms(&self) -> i64 {
        self.planned_min as i64 * MIN
    }
    /// Still getting ready: nothing counts down until "Лёг".
    pub fn in_prep(&self) -> bool {
        self.prep_min > 0 && self.lay_at.is_none() && self.end.is_none()
    }
    /// End of the planned preparation (only a reminder fires there).
    pub fn prep_end(&self) -> Ts {
        self.start + self.prep_min as i64 * MIN
    }
    /// Where the countdown starts: "Лёг", or the start for a segment without preparation.
    pub fn countdown_from(&self) -> Ts {
        self.lay_at.unwrap_or(self.start)
    }
    pub fn planned_end(&self) -> Ts {
        self.countdown_from() + self.planned_ms()
    }
    /// Preparation as it ran (0 without one): up to "Лёг", or to the end if never lay down.
    pub fn prep_ms(&self, now: Ts) -> i64 {
        if self.prep_min == 0 {
            return 0;
        }
        (self.lay_at.or(self.end).unwrap_or(now) - self.start).max(0)
    }
    /// The segment itself, without the preparation (0 if it ended before "Лёг").
    pub fn actual_ms(&self, now: Ts) -> i64 {
        if self.prep_min > 0 && self.lay_at.is_none() {
            return 0;
        }
        (self.end.unwrap_or(now) - self.countdown_from()).max(0)
    }
}

/// A segment waiting its turn (lunch → nap).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueuedSegment {
    pub name: String,
    pub minutes: u32,
    #[serde(default)]
    pub alarm: bool,
    #[serde(default)]
    pub open_access: bool,
    #[serde(default)]
    pub plan_item: Option<usize>,
    /// Preparation before the countdown, from the segment type.
    #[serde(default)]
    pub prep_min: u32,
    /// A stopwatch instead of a countdown.
    #[serde(default)]
    pub open_ended: bool,
    /// Picked by hand and took the place of a planned segment of the same type: dropping it from
    /// the queue gives that planned one back instead of skipping it.
    #[serde(default)]
    pub consumed: bool,
}

impl QueuedSegment {
    /// A segment of the type called `name` (alarm / access / preparation from the settings).
    pub fn of(cfg: &Config, name: &str, minutes: Option<u32>) -> Self {
        let t = cfg.segment_type(name);
        Self {
            name: t.map(|t| t.name.clone()).unwrap_or_else(|| name.trim().chars().take(24).collect()),
            minutes: minutes.or(t.map(|t| t.minutes)).unwrap_or(15).clamp(1, crate::config::MAX_SEGMENT_MIN),
            alarm: t.is_some_and(|t| t.alarm),
            open_access: t.is_some_and(|t| t.open_access),
            plan_item: None,
            prep_min: t.map(|t| t.prep()).unwrap_or(0),
            open_ended: false,
            consumed: false,
        }
    }

    /// "Без времени": a stopwatch, so no alarm and nothing to get ready for.
    pub fn stopwatch(mut self) -> Self {
        self.open_ended = true;
        self.alarm = false;
        self.prep_min = 0;
        self
    }

    /// Time it takes in the day: the preparation and the segment itself.
    pub fn total_ms(&self) -> i64 {
        (self.prep_min + self.minutes) as i64 * MIN
    }
}

/// Everything needed to take a skipped break back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkipUndo {
    pub at: Ts,
    /// The break as it was (a running one keeps counting through the undo window).
    pub phase: Phase,
    pub pause: Option<ActivePause>,
    /// Closing that pause added a record to `pauses` (short ones add none).
    #[serde(default)]
    pub pause_recorded: bool,
    pub block: usize,
    /// The skip started the block for the first time.
    pub first_start: bool,
    pub lunch_end_cleared: bool,
}

/// One-off shift of today's day end (the template in the config stays as is).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DayEndChange {
    pub ts: Ts,
    /// Minutes after the day's midnight; above 1440 means the next night.
    pub from_min: u32,
    pub to_min: u32,
    #[serde(default)]
    pub reason: Option<String>,
    /// "ui" | "mcp"
    pub by: String,
}

/// "Block now for N minutes", outside the study day as well. Only the end can move, and only
/// later; nothing but emergency access opens it early.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FocusLock {
    pub at: Ts,
    pub until: Ts,
    /// "ui" | "mcp"
    pub by: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub notified: bool,
}

/// What `start_focus` did.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FocusChange {
    /// The end before this call, when a focus lock was already on.
    pub from: Option<Ts>,
    pub until: Ts,
    pub extended: bool,
}

/// What `set_plan` changed, block by block.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlanChange {
    pub block: String,
    /// added | removed | shortened | lengthened | moved | closed
    pub change: String,
    pub from_min: Option<u32>,
    pub to_min: Option<u32>,
}

/// What "finish block" did.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FinishedBlock {
    pub block: String,
    pub worked_ms: i64,
    pub from_min: u32,
    pub to_min: u32,
}

/// Does the rest of the plan fit before today's day end?
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Forecast {
    /// Work still to do in the plan.
    pub work_left_ms: i64,
    /// Breaks between the remaining parts and blocks.
    pub breaks_left_ms: i64,
    /// Non-study segments still ahead: the running one, the queue and planned ones not yet taken.
    pub segments_left_ms: i64,
    /// If you go on right now without pauses.
    pub finish_at: Ts,
    pub day_end_at: Ts,
    pub fits: bool,
    /// day_end_at - finish_at: negative when the plan does not fit.
    pub margin_ms: i64,
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
    BlockCompleted { block: usize, block_name: String, work_ms: i64, pauses: u32, pause_ms: i64 },
    DayCompleted { work_ms: i64 },
    BreakEnded { next_name: String, next_part: u32, next_parts: u32, lunch: bool },
    AwaitReminder { waiting_ms: i64, next_name: String, next_part: u32, next_parts: u32 },
    PauseReminder { paused_ms: i64 },
    PauseAccessWarning { left_ms: i64 },
    PauseAccessExpired,
    EmergencyEnded,
    FocusEnded,
    DayEndReached,
    /// Five minutes before the planned end of a segment (not for alarm segments).
    SegmentWarning { name: String, left_ms: i64 },
    /// The planned end of a segment: a notice, or the loud alarm for a nap.
    SegmentEnded { name: String, alarm: bool },
    /// Every 5 minutes after the planned end until the segment is ended.
    SegmentOverrun { name: String, alarm: bool, over_ms: i64 },
    /// The preparation is up ("пора ложиться"), then every 5 minutes until "Лёг". The countdown
    /// never starts by itself.
    SegmentPrepOver { name: String, over_ms: i64 },
    /// A stopwatch segment runs well past its usual length: a quiet nudge, no alarm.
    SegmentLong { name: String, elapsed_ms: i64 },
    /// A block closed 10 minutes ago and nothing started since: "что сейчас?"
    AskWhatNow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayState {
    pub date: NaiveDate,
    /// Kind of today (from the week schedule, or switched on the Today screen).
    #[serde(default)]
    pub kind: DayKind,
    /// "Начать день" turns on blocking today (the profile's setting, pinned at start).
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
    /// Today's day end when it differs from the settings (minutes after the day's midnight,
    /// above 1440 = the night after). Same field the 0.2.x "extend" buttons wrote.
    #[serde(default)]
    pub day_end_min: Option<u32>,
    /// One-off shifts of today's day end, for the log and MCP.
    #[serde(default)]
    pub day_end_changes: Vec<DayEndChange>,
    #[serde(default)]
    pub saved_at: Ts,
    /// The last skipped break, for "Отменить" within [`SKIP_UNDO_MS`].
    #[serde(default)]
    pub skip_undo: Option<SkipUndo>,
    /// Non-study segments of the day, in order (the running one has no end).
    #[serde(default)]
    pub segments: Vec<SegmentRecord>,
    #[serde(default)]
    pub segment_queue: Vec<QueuedSegment>,
    /// "Что сейчас?" was asked for the block closed at this time.
    #[serde(default)]
    pub what_now_for: Option<Ts>,
    /// Focus locks started (or carried over from the day before) on this day.
    #[serde(default)]
    pub focus: Vec<FocusLock>,
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

/// "23:00", or "01:00" for minutes past midnight of the next day.
pub fn fmt_day_min(min: u32) -> String {
    fmt_hm(min % (24 * 60))
}

/// Old plan items (by old index) that moved: everything outside the longest run that kept its
/// order. Moving one item reports that one, not every item it shifted.
fn moved_items(map: &[Option<usize>]) -> Vec<usize> {
    let kept: Vec<(usize, usize)> = map.iter().enumerate().filter_map(|(i, m)| m.map(|j| (i, j))).collect();
    // Longest increasing subsequence of new positions, in old order (n <= 16: quadratic is fine).
    let n = kept.len();
    let (mut len, mut prev) = (vec![1usize; n], vec![usize::MAX; n]);
    for a in 0..n {
        for b in 0..a {
            if kept[b].1 < kept[a].1 && len[b] + 1 > len[a] {
                len[a] = len[b] + 1;
                prev[a] = b;
            }
        }
    }
    let mut stay = vec![false; n];
    let mut k = (0..n).max_by_key(|&a| len[a]);
    while let Some(a) = k {
        stay[a] = true;
        k = (prev[a] != usize::MAX).then_some(prev[a]);
    }
    kept.iter().zip(stay).filter(|(_, s)| !s).map(|((i, _), _)| *i).collect()
}

/// "Математика 90→60 мин", "удалён «Словацкий»", "добавлен «Физика» 45 мин".
pub fn fmt_change(c: &PlanChange) -> String {
    let (from, to) = (c.from_min.unwrap_or(0), c.to_min.unwrap_or(0));
    match c.change.as_str() {
        "removed" => format!("удалён «{}»", c.block),
        "added" => format!("добавлен «{}» {to} мин", c.block),
        "moved" if from != to => format!("«{}» переставлен, {from}→{to} мин", c.block),
        "moved" => format!("«{}» переставлен", c.block),
        "closed" => format!("«{}» закрыт как есть: {from}→{to} мин", c.block),
        _ => format!("{} {from}→{to} мин", c.block),
    }
}

/// Same block / segment name, ignoring case and spaces around ("математика" = "Математика").
/// `eq_ignore_ascii_case` does not fold Cyrillic.
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Whole minutes worked, rounded DOWN. The one rounding for "done_min", the "can't cut below"
/// limit, the phone's "−" floor and "close block": 10 min 40 s is 10 everywhere.
pub fn worked_min(ms: i64) -> u32 {
    (ms.max(0) / MIN) as u32
}

/// Minutes with one decimal: "84,6", "90".
pub fn fmt_min(ms: i64) -> String {
    let tenths = (ms * 10 + MIN / 2) / MIN;
    if tenths % 10 == 0 {
        format!("{}", tenths / 10)
    } else {
        format!("{},{}", tenths / 10, tenths % 10)
    }
}

fn norm_phrase(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl DayState {
    pub fn new(now: Ts, cfg: &Config) -> Self {
        let date = clock::local_date(now, cfg.tz_offset_min);
        let kind = cfg.kind_on(date);
        let plan = cfg.plan_on(date).clone();
        Self {
            date,
            kind,
            study_day: cfg.profile(kind).block,
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
            day_end_min: None,
            day_end_changes: vec![],
            saved_at: now,
            skip_undo: None,
            segments: vec![],
            segment_queue: vec![],
            what_now_for: None,
            focus: vec![],
        }
    }

    /// A new day keeps the focus lock still running on the day before (it outlives midnight).
    pub fn carry_focus(&mut self, prev: &DayState, now: Ts, cfg: &Config) {
        let Some(f) = prev.focus.iter().rev().find(|f| now < f.until) else {
            return;
        };
        // Emergency access opened under the focus lock keeps its full time across midnight.
        if let Some(e) = prev.emergencies.iter().rev().find(|e| now < e.until) {
            if !self.emergencies.iter().any(|x| x.at == e.at) {
                self.emergencies.push(Emergency { at: e.at, until: e.until, ended_early: false, notified: false });
            }
        }
        if self.focus_until(now).is_some_and(|u| u >= f.until) {
            return;
        }
        self.focus.push(FocusLock { at: now, until: f.until, by: f.by.clone(), reason: f.reason.clone(), notified: false });
        self.log(now, "focus", format!("Фокус-блокировка продолжается до {}", clock::hm(f.until, cfg.tz_offset_min)));
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
            self.study_day = cfg.profile(self.kind).block;
        }
    }

    /// 0 = Monday, of the study day itself (after midnight it may still be yesterday's day).
    pub fn weekday(&self) -> usize {
        self.date.weekday().num_days_from_monday() as usize
    }

    /// The day stays current until midnight, or longer while its day end runs into the night.
    pub fn is_live(&self, now: Ts, cfg: &Config) -> bool {
        let today = clock::local_date(now, cfg.tz_offset_min);
        today == self.date || (today > self.date && now < self.day_end_at(cfg))
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
        if let Phase::Segment { rec, .. } = self.phase {
            if let Some(r) = self.segments.get_mut(rec) {
                r.end.get_or_insert(end.max(r.start));
            }
        }
        self.segment_queue.clear();
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
            Phase::Break { next, .. } | Phase::Await { next, .. } | Phase::Lunch { next, .. } | Phase::Segment { next, .. } => Some(next),
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
        self.plan.get(i).filter(|b| !b.is_break()).map(|b| count_segments(b.total_ms(), self.seg_ms())).unwrap_or(0)
    }

    pub fn is_study(&self, i: usize) -> bool {
        self.plan.get(i).is_some_and(|b| !b.is_break())
    }

    /// A planned segment that was not taken yet.
    /// Not started and not waiting in the queue either.
    pub fn is_open_break(&self, i: usize) -> bool {
        self.plan.get(i).is_some_and(|b| b.is_break() && b.minutes > 0)
            && self.progress[i].started_at.is_none()
            && self.progress[i].completed_at.is_none()
            && !self.segment_queue.iter().any(|q| q.plan_item == Some(i))
    }

    /// Planned segments not taken yet that stand before study block `next` in the day's order.
    pub fn planned_breaks_before(&self, next: usize) -> Vec<usize> {
        (0..next.min(self.plan.len())).filter(|&i| self.is_open_break(i)).collect()
    }

    pub fn is_block_done(&self, i: usize) -> bool {
        self.progress.get(i).is_some_and(|p| p.completed_at.is_some())
    }

    /// The first study block not closed yet (planned segments are not blocks).
    pub fn first_open_block(&self) -> Option<usize> {
        (0..self.plan.len()).find(|&i| self.is_study(i) && !self.is_block_done(i) && self.plan[i].minutes > 0)
    }

    /// Today's day end in minutes after the day's midnight (may exceed 24:00).
    pub fn day_end(&self, cfg: &Config) -> u32 {
        self.day_end_min.unwrap_or(cfg.day_end_min)
    }

    pub fn day_end_at(&self, cfg: &Config) -> Ts {
        clock::date_minute(self.date, cfg.tz_offset_min, self.day_end(cfg))
    }

    pub fn after_day_end(&self, now: Ts, cfg: &Config) -> bool {
        now >= self.day_end_at(cfg)
    }

    /// Remaining work + breaks of the plan if you go on right now, against today's day end.
    pub fn forecast(&self, now: Ts, cfg: &Config) -> Forecast {
        let seg = self.seg_ms();
        let short = self.timing.short_break_min as i64 * MIN;
        let between = self.timing.between_blocks_min as i64 * MIN;
        let (mut work, mut breaks, mut segs) = (0i64, 0i64, 0i64);
        let plan_mode = self.mode == Mode::Plan;
        if plan_mode {
            match self.phase {
                Phase::Break { dur_ms, .. } => breaks += (dur_ms - self.phase_elapsed(now)).max(0),
                Phase::Segment { rec, .. } => {
                    if let Some(r) = self.segments.get(rec) {
                        // Getting ready: what is left of it, then the whole segment from "Лёг".
                        segs += if r.in_prep() { (r.prep_end() - now).max(0) + r.planned_ms() } else { (r.planned_end() - now).max(0) };
                    }
                }
                _ => {}
            }
            segs += self.segment_queue.iter().map(QueuedSegment::total_ms).sum::<i64>();
        }
        let mut first = true;
        // Planned segments stand between blocks instead of the usual break; ones after the
        // last block never run (the day closes with it).
        let mut pending = 0i64;
        for i in (0..self.plan.len()).filter(|&i| !self.is_block_done(i) && self.plan[i].minutes > 0) {
            if self.plan[i].is_break() {
                if self.is_open_break(i) {
                    let prep = cfg.segment_type(&self.plan[i].name).map(|t| t.prep()).unwrap_or(0);
                    pending += self.plan[i].total_ms() + prep as i64 * MIN;
                }
                continue;
            }
            let total = self.plan[i].total_ms();
            let mut done = self.progress[i].work_ms;
            let mut parts = 0;
            if let Phase::Work { block, dur_ms, .. } = self.phase {
                if plan_mode && block == i {
                    work += (dur_ms - self.phase_elapsed(now)).max(0);
                    done += dur_ms;
                    parts += 1;
                }
            }
            loop {
                let s = next_segment_ms(total, done, seg);
                if s <= 0 {
                    break;
                }
                work += s;
                done += s;
                parts += 1;
            }
            breaks += (parts - 1).max(0) as i64 * short;
            if !first && pending == 0 {
                breaks += between;
            }
            segs += pending;
            pending = 0;
            first = false;
        }
        let finish_at = now + work + breaks + segs;
        let day_end_at = self.day_end_at(cfg);
        Forecast {
            work_left_ms: work,
            breaks_left_ms: breaks,
            segments_left_ms: segs,
            finish_at,
            day_end_at,
            fits: finish_at <= day_end_at,
            margin_ms: day_end_at - finish_at,
        }
    }

    /// The study-day lock (before exemptions like pause access or emergency).
    pub fn plan_lock(&self, now: Ts, cfg: &Config) -> bool {
        self.study_day && self.started_at.is_some() && self.completed_at.is_none() && !self.after_day_end(now, cfg)
    }

    pub fn single_lock(&self) -> bool {
        self.mode == Mode::Single
            && self.singles.last().is_some_and(|s| s.cfg.block && s.ended_at.is_none())
            && !matches!(self.phase, Phase::Idle | Phase::Done)
    }

    pub fn base_lock(&self, now: Ts, cfg: &Config) -> bool {
        self.plan_lock(now, cfg) || self.single_lock() || self.focus_until(now).is_some()
    }

    /// End of the focus lock running at `now`.
    pub fn focus_until(&self, now: Ts) -> Option<Ts> {
        self.focus.iter().map(|f| f.until).filter(|u| now < *u).max()
    }

    /// Final decision: should blocked sites/apps be blocked right now?
    pub fn lock_state(&self, now: Ts, cfg: &Config) -> LockState {
        let focus_until = self.focus_until(now);
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
            return LockState { blocked: false, base: false, reason: reason.into(), until: None, focus_until };
        }
        if let Some(e) = self.emergencies.iter().rev().find(|e| now < e.until) {
            return LockState { blocked: false, base: true, reason: "emergency".into(), until: Some(e.until), focus_until };
        }
        // A focus lock is opened by emergency access only: pause access, lunch at the PC and
        // segment access are study-day exemptions.
        if focus_until.is_none() {
            if let Some(p) = &self.pause {
                if p.access_active(now) {
                    return LockState { blocked: false, base: true, reason: "pause_access".into(), until: p.access_until(), focus_until };
                }
            }
            if let Phase::Break { brk: BreakKind::Lunch { at_pc: true }, dur_ms, elapsed_ms, since: Some(s), .. } = self.phase {
                return LockState {
                    blocked: false,
                    base: true,
                    reason: "lunch_at_pc".into(),
                    until: Some(s + (dur_ms - elapsed_ms)),
                    focus_until,
                };
            }
            if let Phase::Segment { rec, .. } = self.phase {
                if let Some(r) = self.segments.get(rec).filter(|r| r.open_access && (r.in_prep() || now < r.planned_end())) {
                    // While getting ready the end is not known yet.
                    let until = (!r.in_prep()).then(|| r.planned_end());
                    return LockState { blocked: false, base: true, reason: "segment_access".into(), until, focus_until };
                }
            }
        }
        let reason = if self.single_lock() {
            "single"
        } else if self.plan_lock(now, cfg) {
            "study"
        } else {
            "focus"
        };
        LockState { blocked: true, base: true, reason: reason.into(), until: None, focus_until }
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
        self.study_day = cfg.profile(self.kind).block;
        self.started_at = Some(now);
        let leading = self.planned_breaks_before(first);
        if leading.is_empty() {
            self.start_work(now, first);
        } else {
            self.queue_planned(cfg, &leading);
            self.start_queued(now, first);
        }
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
        if cfg.pause_access && self.base_lock(now, cfg) && self.focus_until(now).is_none() {
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
        let since = p.since;
        if self.close_pause(p, now) {
            self.log(now, "resume", "Продолжение");
        } else if let Some(i) = self.events.iter().rposition(|e| e.kind == "pause" && e.ts == since) {
            // A blink of a pause leaves no trace in the log either.
            self.events.remove(i);
        }
        Ok(())
    }

    pub(crate) fn close_pause_pub(&mut self, p: ActivePause, now: Ts) -> bool {
        self.close_pause(p, now)
    }

    /// Record a finished pause. Returns false for a pause shorter than [`MIN_PAUSE_MS`]
    /// (dropped as noise).
    fn close_pause(&mut self, p: ActivePause, now: Ts) -> bool {
        if now - p.since < MIN_PAUSE_MS {
            return false;
        }
        let access_ms = p.access.iter().map(|w| (w.until.min(now) - w.from).max(0)).sum();
        self.pauses.push(PauseRecord {
            start: p.since,
            end: now,
            what: p.what,
            block: p.block,
            access_ms,
            extensions: p.access.len().saturating_sub(1) as u32,
        });
        true
    }

    /// Extend pause access (the app checks the captcha before calling this).
    pub fn extend_pause_access(&mut self, now: Ts, cfg: &Config) -> Result<Ts, String> {
        if !cfg.pause_access {
            return Err("Доступ на паузе выключен в настройках.".into());
        }
        if !self.base_lock(now, cfg) {
            return Err("Блокировка сейчас не действует.".into());
        }
        if self.focus_until(now).is_some() {
            return Err("Во время фокус-блокировки доступ на паузе не открывается — только аварийный доступ.".into());
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
        let before = (self.phase.clone(), self.pause.clone(), self.lunch.as_ref().is_some_and(|l| l.end.is_none()));
        self.skip_undo = None;
        let pause_recorded = match self.pause.take() {
            Some(p) => self.close_pause(p, now),
            None => false,
        };
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
        let target = if self.mode == Mode::Plan && self.is_block_done(next) { self.first_open_block() } else { Some(next) };
        let Some(b) = target else {
            self.phase = Phase::Done;
            return Ok(());
        };
        let first_start = self.mode == Mode::Plan && self.progress[b].started_at.is_none();
        self.start_work(now, b);
        if let (Phase::Break { .. }, pause, lunch_open) = before {
            let lunch_end_cleared = lunch_open && self.lunch.as_ref().is_some_and(|l| l.end.is_some());
            self.skip_undo = Some(SkipUndo { at: now, phase: before.0, pause, pause_recorded, block: b, first_start, lunch_end_cleared });
        }
        Ok(())
    }

    /// Take a skipped break back within [`SKIP_UNDO_MS`]: the break goes on as if it had never
    /// been skipped (those seconds count as break, not work).
    pub fn undo_skip(&mut self, now: Ts) -> Result<(), String> {
        let u = self.skip_undo.take().ok_or("Отменять нечего.")?;
        let fresh = matches!(self.phase, Phase::Work { block, since: Some(_), .. } if block == u.block) && self.pause.is_none();
        if now - u.at > SKIP_UNDO_MS || !fresh {
            return Err("Поздно отменять — часть уже идёт.".into());
        }
        if u.first_start {
            if let Some(p) = self.progress.get_mut(u.block) {
                p.started_at = None;
            }
        }
        if u.pause_recorded {
            self.pauses.pop();
        }
        self.pause = u.pause;
        if u.lunch_end_cleared {
            if let Some(l) = &mut self.lunch {
                l.end = None;
            }
        }
        self.phase = u.phase;
        self.log(now, "break_skip_undo", "Пропуск перерыва отменён — перерыв продолжается");
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

    /// Block from now for `minutes`, any day. A running focus lock only moves later: a shorter
    /// call is refused.
    pub fn start_focus(&mut self, now: Ts, cfg: &Config, minutes: u32, reason: Option<&str>, by: &str) -> Result<FocusChange, String> {
        if !(1..=MAX_FOCUS_MIN).contains(&minutes) {
            return Err(format!("Фокус-блокировка — от 1 до {MAX_FOCUS_MIN} минут от текущего момента."));
        }
        let until = now + minutes as i64 * MIN;
        let hm = |ts: Ts| clock::hm(ts, cfg.tz_offset_min);
        let reason = reason.map(|r| r.trim().chars().take(200).collect::<String>()).filter(|r| !r.is_empty());
        let note = reason.as_ref().map(|r| format!(" ({r})")).unwrap_or_default();
        let from = self.focus_until(now);
        match from {
            Some(cur) if until <= cur => {
                return Err(format!(
                    "Фокус-блокировка уже идёт до {} (ещё {} мин) — её можно только продлить: minutes должно быть больше {}.",
                    hm(cur),
                    (cur - now + MIN - 1) / MIN,
                    (cur - now) / MIN
                ));
            }
            Some(cur) => {
                if let Some(f) = self.focus.iter_mut().rev().find(|f| f.until == cur) {
                    f.until = until;
                    if reason.is_some() {
                        f.reason = reason;
                    }
                }
                self.log(now, "focus", format!("Фокус-блокировка продлена: {} → {}{note}", hm(cur), hm(until)));
            }
            None => {
                // Pause access does not open a focus lock: close a window still running.
                if let Some(p) = &mut self.pause {
                    if p.access_active(now) {
                        for w in &mut p.access {
                            if w.until > now {
                                w.until = now.max(w.from);
                            }
                        }
                        p.warned = true;
                        p.expired_notified = true;
                    }
                }
                self.focus.push(FocusLock { at: now, until, by: by.into(), reason, notified: false });
                self.log(now, "focus", format!("Фокус-блокировка до {} ({minutes} мин){note}", hm(until)));
            }
        }
        Ok(FocusChange { from, until, extended: from.is_some() })
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

    /// The latest closed block that still waits for its end-of-block line.
    pub fn pending_note(&self) -> Option<usize> {
        (0..self.plan.len())
            .filter(|&i| self.is_study(i) && self.progress[i].completed_at.is_some() && self.progress[i].note.is_none() && !self.progress[i].note_skipped)
            .max_by_key(|&i| self.progress[i].completed_at)
    }

    /// Save the end-of-block line (`None` or blank = skip). Works for any started block.
    pub fn set_block_note(&mut self, now: Ts, block: usize, note: Option<&str>, by: &str) -> Result<(), String> {
        let name = self.plan.get(block).map(|b| b.name.clone()).ok_or("Нет такого блока.")?;
        let p = &mut self.progress[block];
        if p.started_at.is_none() && p.work_ms == 0 {
            return Err(format!("«{name}» ещё не начат."));
        }
        let note: Option<String> = note.map(|n| n.trim().chars().take(300).collect::<String>()).filter(|n| !n.is_empty());
        match &note {
            Some(n) => {
                p.note = Some(n.clone());
                p.note_skipped = false;
                let by = if by == "mcp" { " — агент" } else { "" };
                self.log(now, "block_note", format!("«{name}»: {n}{by}"));
            }
            None => {
                if p.note.is_none() {
                    p.note_skipped = true;
                }
            }
        }
        Ok(())
    }

    /// The block "finish block" would close: `name` (any started, open block) or the current one.
    pub fn finish_target(&self, name: Option<&str>) -> Result<usize, String> {
        if self.mode != Mode::Plan || self.started_at.is_none() {
            return Err("Закрыть блок можно только во время учебного дня.".into());
        }
        let i = match name.map(str::trim).filter(|n| !n.is_empty()) {
            // Two blocks of one subject: the open started one is meant.
            Some(n) => {
                let named: Vec<usize> = (0..self.plan.len()).filter(|&i| self.is_study(i) && same_name(&self.plan[i].name, n)).collect();
                *named.iter().find(|&&i| self.check_closable(i).is_ok()).or(named.first()).ok_or(format!("В плане нет блока «{n}»."))?
            }
            None => self.current_block().ok_or("Сейчас нет текущего блока.")?,
        };
        self.check_closable(i)?;
        Ok(i)
    }

    /// A started study block that is still open.
    fn check_closable(&self, i: usize) -> Result<(), String> {
        let name = &self.plan[i].name;
        if self.plan[i].is_break() {
            return Err(format!("«{name}» — отрезок, а не учебный блок."));
        }
        if self.is_block_done(i) {
            return Err(format!("«{name}» уже закрыт."));
        }
        let p = &self.progress[i];
        if p.started_at.is_none() && p.work_ms == 0 {
            return Err(format!("«{name}» ещё не начат — если он сегодня не нужен, убери его из плана."));
        }
        Ok(())
    }

    /// Close a started block right now on the minutes actually worked: its plan becomes the
    /// fact (whole minutes, rounded down like everywhere) and the day moves on as if the block
    /// had just ended.
    pub fn finish_block(&mut self, now: Ts, cfg: &Config, name: Option<&str>, by: &str) -> Result<(FinishedBlock, Vec<Event>), String> {
        let i = self.finish_target(name)?;
        self.finish_block_at(now, cfg, i, by)
    }

    fn finish_block_at(&mut self, now: Ts, cfg: &Config, i: usize, by: &str) -> Result<(FinishedBlock, Vec<Event>), String> {
        self.check_closable(i)?;
        let worked = self.block_work_live(i, now);
        if worked < MIN {
            return Err(format!(
                "По «{}» отработано меньше минуты — закрывать нечего. Если он сегодня не нужен, убери его из плана: такой блок удаляется.",
                self.plan[i].name
            ));
        }
        let running_here = matches!(self.phase, Phase::Work { block, .. } if block == i);
        let next_here = matches!(self.phase, Phase::Break { next, .. } | Phase::Await { next, .. } | Phase::Lunch { next, .. } if next == i);
        if running_here {
            if let Some(p) = self.pause.take() {
                self.close_pause(p, now);
            }
            let elapsed = self.phase_elapsed(now);
            let p = &mut self.progress[i];
            p.work_ms += elapsed;
            if elapsed > 0 {
                p.parts_done += 1;
            }
        }
        let name = self.plan[i].name.clone();
        let from_min = self.plan[i].minutes;
        let to_min = worked_min(worked).max(1);
        self.plan[i].minutes = to_min;
        let p = &mut self.progress[i];
        p.closed = true;
        p.completed_at = Some(now);
        let mut ev = vec![];
        let (pauses, pause_ms) = self
            .pauses
            .iter()
            .filter(|r| r.block == Some(i))
            .fold((0u32, 0i64), |(n, ms), r| (n + 1, ms + (r.end - r.start)));
        self.log(
            now,
            "block_finish",
            format!(
                "Блок «{name}» закрыт досрочно: {} из {from_min} мин, план {from_min}→{to_min} мин{}",
                fmt_min(worked),
                if by == "mcp" { " — агент" } else { "" }
            ),
        );
        ev.push(Event::BlockCompleted { block: i, block_name: name.clone(), work_ms: worked, pauses, pause_ms });
        let in_segment = matches!(self.phase, Phase::Segment { .. });
        if (running_here || next_here) && !in_segment {
            match self.first_open_block() {
                Some(n) if !running_here && !matches!(self.phase, Phase::Break { .. }) => {
                    if let Phase::Await { next, .. } | Phase::Lunch { next, .. } = &mut self.phase {
                        *next = n;
                    }
                }
                _ => {
                    if let Some(p) = self.pause.take() {
                        self.close_pause(p, now);
                    }
                    self.finish_lunch_record(now);
                    self.after_block(now, cfg, &mut ev);
                }
            }
        } else if in_segment {
            // The segment goes on; afterwards the next open block (or the day end).
            if let (Some(n), Phase::Segment { next, .. }) = (self.first_open_block(), &mut self.phase) {
                *next = n;
            }
        } else if self.first_open_block().is_none() && matches!(self.phase, Phase::Break { .. } | Phase::Await { .. } | Phase::Lunch { .. }) {
            self.phase = Phase::Done;
            self.completed_at = Some(now);
            let total = self.progress.iter().map(|p| p.work_ms).sum();
            ev.push(Event::DayCompleted { work_ms: total });
        }
        Ok((FinishedBlock { block: name, worked_ms: worked, from_min, to_min }, ev))
    }

    /// Quick "+30 мин / +1 ч / …" buttons: move today's day end later.
    /// After the old end has passed this turns the lock back on until the new end.
    pub fn extend_day_end(&mut self, now: Ts, cfg: &Config, new_end: u32) -> Result<(), String> {
        if new_end <= self.day_end(cfg) {
            return Err("Это не позже текущего конца дня.".into());
        }
        self.move_day_end(now, cfg, new_end, None, "ui").map(|_| ())
    }

    /// Shift today's day end once ("HH:MM", Moscow). "00:00".."02:00" mean the night after.
    /// The config template stays untouched. Returns the logged change (`from == to`: nothing changed).
    pub fn set_day_end(&mut self, now: Ts, cfg: &Config, hm: &str, reason: Option<&str>, by: &str) -> Result<DayEndChange, String> {
        let typed = clock::parse_hm(hm).ok_or("Время нужно в формате ЧЧ:ММ, например 23:00.")?;
        let to = if typed <= NEXT_DAY_UNTIL_MIN { typed + 24 * 60 } else { typed };
        self.move_day_end(now, cfg, to, reason, by)
    }

    /// Move today's day end to `to` minutes after the day's midnight (up to 02:00 of the night
    /// after), later or earlier, as long as it is still ahead of `now`.
    pub fn move_day_end(&mut self, now: Ts, cfg: &Config, to: u32, reason: Option<&str>, by: &str) -> Result<DayEndChange, String> {
        let to_ts = clock::date_minute(self.date, cfg.tz_offset_min, to);
        if to > MAX_DAY_END_MIN {
            return Err("Позже 02:00 ночи конец дня сдвинуть нельзя.".into());
        }
        if to_ts <= now {
            return Err(format!(
                "{} уже прошло. Конец дня — позже текущего времени ({}) и не позже 02:00 следующих суток.",
                fmt_day_min(to),
                clock::hm(now, cfg.tz_offset_min)
            ));
        }
        let reason = reason.map(|r| r.trim().chars().take(200).collect::<String>()).filter(|r| !r.is_empty());
        let from = self.day_end(cfg);
        let change = DayEndChange { ts: now, from_min: from, to_min: to, reason, by: by.into() };
        if to == from {
            return Ok(change);
        }
        self.day_end_min = (to != cfg.day_end_min).then_some(to);
        // The "day is over" notice fires again at the new time.
        self.day_end_notified = false;
        self.log(
            now,
            "day_end_change",
            format!(
                "Конец дня: {} → {}{}{}",
                fmt_day_min(from),
                fmt_day_min(to),
                change.reason.as_ref().map(|r| format!(" ({r})")).unwrap_or_default(),
                if by == "mcp" { " — агент" } else { "" }
            ),
        );
        self.day_end_changes.push(change.clone());
        Ok(change)
    }

    /// Switch today's kind. Before the start the plan is replaced by the profile's template;
    /// after it the template is merged in (blocks only grow). While `locked` only a stricter kind
    /// is accepted.
    pub fn set_kind(&mut self, now: Ts, cfg: &Config, kind: DayKind, locked: bool) -> Result<(), String> {
        if kind == self.kind {
            return Ok(());
        }
        if locked && kind < self.kind {
            return Err(format!("Во время учёбы день можно сделать только плотнее, не «{}».", kind.label()));
        }
        let profile = cfg.profile(kind);
        let from = self.kind;
        if self.started_at.is_none() {
            self.set_plan(now, profile.plan.clone(), false)?;
        } else {
            let mut plan = self.plan.clone();
            for b in &profile.plan {
                match plan.iter_mut().find(|p| same_name(&p.name, &b.name) && p.kind == b.kind) {
                    Some(p) => p.minutes = p.minutes.max(b.minutes),
                    None => plan.push(b.clone()),
                }
            }
            if plan != self.plan {
                self.set_plan(now, plan, locked)?;
            }
        }
        self.kind = kind;
        self.study_day = if locked { self.study_day || profile.block } else { profile.block };
        self.log(now, "kind", format!("День: {} → {}", from.label(), kind.label()));
        Ok(())
    }

    /// Replace today's plan. Items are matched to the old plan by name (any case) and type, in any
    /// order. Started blocks keep their recorded work: they can't be removed or renamed (unless
    /// less than a minute was worked), and while `locked` they can't be cut below the whole minutes
    /// already worked. Anything not started, and any segment, can be moved, cut, removed or added.
    pub fn set_plan(&mut self, now: Ts, mut plan: Vec<PlanBlock>, locked: bool) -> Result<Vec<PlanChange>, String> {
        for b in &mut plan {
            b.normalize();
        }
        if plan.len() > MAX_PLAN_BLOCKS {
            return Err(format!("Не больше {MAX_PLAN_BLOCKS} пунктов плана в день."));
        }
        if plan.iter().any(|b| b.minutes == 0) {
            return Err("У каждого блока должна быть длительность.".into());
        }
        let started = self.started_at.is_some();
        if started && !plan.iter().any(|b| !b.is_break()) {
            return Err("План начатого дня не может остаться без учебных блоков.".into());
        }
        let touched = |p: &BlockProgress| p.started_at.is_some() || p.work_ms > 0;

        // Old item -> new item by name (any case) and type, in any order: started items first, so
        // that two blocks of one subject keep their work. Unstarted items and segments move freely.
        let mut map: Vec<Option<usize>> = vec![None; self.plan.len()];
        let mut used = vec![false; plan.len()];
        let order: Vec<usize> = (0..self.plan.len()).filter(|&i| touched(&self.progress[i])).chain((0..self.plan.len()).filter(|&i| !touched(&self.progress[i]))).collect();
        for i in order {
            let old = &self.plan[i];
            if let Some(j) = (0..plan.len()).find(|&j| !used[j] && same_name(&plan[j].name, &old.name) && plan[j].kind == old.kind) {
                map[i] = Some(j);
                used[j] = true;
            }
        }
        for (i, old) in self.plan.iter().enumerate() {
            let worked = self.block_work_live(i, now);
            match map[i] {
                // Work under a minute (a misclick start) is noise: such a block may go. Segments
                // keep their record in the day's segments either way.
                None if old.is_break() || !touched(&self.progress[i]) || worked < MIN => {}
                None => {
                    return Err(format!(
                        "«{}» уже начат (отработано {} мин) — его нельзя удалить или переименовать: отработанное не стирается. Закрыть его на отработанном — пункт с close: true; удалить можно блок, где отработано меньше минуты.",
                        old.name,
                        worked_min(worked)
                    ));
                }
                Some(j) if locked && touched(&self.progress[i]) && !old.is_break() && plan[j].minutes < worked_min(worked) => {
                    return Err(format!(
                        "«{}»: уже отработано {} мин — меньше нельзя (можно ровно {} или закрыть блок как есть: close: true).",
                        old.name,
                        worked_min(worked),
                        worked_min(worked)
                    ));
                }
                _ => {}
            }
        }

        let mut changes = vec![];
        let moved = moved_items(&map);
        for (i, old) in self.plan.iter().enumerate() {
            match map[i] {
                None => changes.push(PlanChange { block: old.name.clone(), change: "removed".into(), from_min: Some(old.minutes), to_min: None }),
                Some(_) if moved.contains(&i) => changes.push(PlanChange {
                    block: old.name.clone(),
                    change: "moved".into(),
                    from_min: Some(old.minutes),
                    to_min: map[i].map(|j| plan[j].minutes),
                }),
                Some(j) if plan[j].minutes != old.minutes => changes.push(PlanChange {
                    block: old.name.clone(),
                    change: if plan[j].minutes < old.minutes { "shortened" } else { "lengthened" }.into(),
                    from_min: Some(old.minutes),
                    to_min: Some(plan[j].minutes),
                }),
                _ => {}
            }
        }
        for (b, _) in plan.iter().zip(&used).filter(|(_, u)| !**u) {
            changes.push(PlanChange { block: b.name.clone(), change: "added".into(), from_min: None, to_min: Some(b.minutes) });
        }

        // Move progress and every stored block index to the new positions.
        let mut progress = vec![BlockProgress::default(); plan.len()];
        for (i, m) in map.iter().enumerate() {
            if let Some(j) = m {
                progress[*j] = self.progress[i].clone();
                // Lengthening a block closed early opens it again.
                if progress[*j].closed && plan[*j].minutes > self.plan[i].minutes {
                    progress[*j].closed = false;
                }
            }
        }
        let remap = |b: Option<usize>| b.and_then(|i| map.get(i).copied().flatten());
        for r in &mut self.pauses {
            r.block = remap(r.block);
        }
        if let Some(p) = &mut self.pause {
            p.block = remap(p.block);
        }
        for r in &mut self.segments {
            r.plan_item = remap(r.plan_item);
        }
        // A planned segment taken out of the plan while it waits in the queue does not run; one
        // picked by hand stays (it only stops standing in for a planned one).
        self.segment_queue.retain(|q| q.consumed || q.plan_item.is_none() || remap(q.plan_item).is_some());
        for q in &mut self.segment_queue {
            q.plan_item = remap(q.plan_item);
            q.consumed &= q.plan_item.is_some();
        }
        if let Some(u) = &mut self.skip_undo {
            match remap(Some(u.block)) {
                Some(j) => u.block = j,
                None => self.skip_undo = None,
            }
        }
        let mut lost_next = false;
        // The running block was removed (it had less than a minute): its seconds go with it.
        let mut lost_work = false;
        if self.mode == Mode::Plan {
            match &mut self.phase {
                Phase::Work { block, .. } => match remap(Some(*block)) {
                    Some(j) => *block = j,
                    None => lost_work = true,
                },
                // A started block stays next; otherwise the next one is simply the first open in the new order.
                Phase::Break { next, .. } | Phase::Await { next, .. } | Phase::Lunch { next, .. } | Phase::Segment { next, .. } => match remap(Some(*next)) {
                    Some(j) if touched(&self.progress[*next]) && !self.plan[*next].is_break() => *next = j,
                    _ => lost_next = true,
                },
                _ => {}
            }
        }
        self.plan = plan;
        self.progress = progress;
        if lost_work {
            if let Some(p) = self.pause.take() {
                self.close_pause(p, now);
            }
            self.phase = match self.first_open_block() {
                Some(next) => Phase::Await { next, since: now, reminded_at: now },
                None => Phase::Done,
            };
        }

        // The running part follows the new block length but never ends before what is already done.
        let current_work = match self.phase {
            Phase::Work { block, .. } if self.mode == Mode::Plan => Some(block),
            _ => None,
        };
        if let Some(b) = current_work {
            let elapsed = self.phase_elapsed(now);
            let d = next_segment_ms(self.plan[b].total_ms(), self.progress[b].work_ms, self.seg_ms()).max(elapsed);
            if let Phase::Work { dur_ms, .. } = &mut self.phase {
                *dur_ms = d;
            }
        }
        // Re-evaluate completion; the running block is closed by the timer itself.
        for i in 0..self.plan.len() {
            // Planned segments are closed by ending the segment, never by work.
            if Some(i) == current_work || self.plan[i].is_break() {
                continue;
            }
            let total = self.plan[i].total_ms();
            let p = &mut self.progress[i];
            if p.completed_at.is_some() && p.work_ms < total && !p.closed {
                p.completed_at = None;
            }
            if p.completed_at.is_none() && p.work_ms >= total && p.started_at.is_some() {
                p.completed_at = Some(now);
            }
        }
        if self.mode == Mode::Plan {
            let open = self.first_open_block();
            if lost_next {
                if let (Some(n), Phase::Break { next, .. } | Phase::Await { next, .. } | Phase::Lunch { next, .. } | Phase::Segment { next, .. }) = (open, &mut self.phase) {
                    *next = n;
                }
            }
            if started {
                match (open, &self.phase) {
                    (Some(next), Phase::Done) => {
                        self.completed_at = None;
                        self.phase = Phase::Await { next, since: now, reminded_at: now };
                    }
                    (Some(_), _) => self.completed_at = None,
                    (None, Phase::Await { .. } | Phase::Break { .. } | Phase::Lunch { .. } | Phase::Done) => {
                        if let Some(p) = self.pause.take() {
                            self.close_pause(p, now);
                        }
                        self.finish_lunch_record(now);
                        self.phase = Phase::Done;
                        if self.completed_at.is_none() {
                            self.completed_at = Some(now);
                            self.log(now, "day_done", "Все блоки дня закрыты (план изменён) — блокировка снята");
                        }
                    }
                    _ => {}
                }
            }
        }
        let summary = self.plan.iter().map(|b| format!("{} {} мин", b.name, b.minutes)).collect::<Vec<_>>().join(" · ");
        let diff = changes.iter().map(fmt_change).collect::<Vec<_>>().join(", ");
        self.log(now, "plan", if diff.is_empty() { format!("План: {summary}") } else { format!("План: {summary} ({diff})") });
        Ok(changes)
    }

    /// [`Self::set_plan`] plus "close as is": `close[j]` marks plan item `j` — a started study block
    /// that is still open — to be closed on the minutes worked (its plan becomes the fact, the
    /// rest of it leaves the plan). A block with less than a minute worked is removed instead.
    /// All or nothing: on any error the day stays as it was.
    pub fn edit_plan(&mut self, now: Ts, cfg: &Config, mut plan: Vec<PlanBlock>, close: &[bool], locked: bool, by: &str) -> Result<(Vec<PlanChange>, Vec<Event>), String> {
        let mut closing: Vec<(String, u32)> = vec![];
        let mut taken: Vec<usize> = vec![];
        let mut keep = vec![true; plan.len()];
        for (j, b) in plan.iter_mut().enumerate() {
            if !close.get(j).copied().unwrap_or(false) {
                continue;
            }
            if b.is_break() {
                return Err(format!("«{}»: close закрывает учебный блок, а это отрезок.", b.name));
            }
            let i = (0..self.plan.len())
                .find(|&i| !taken.contains(&i) && self.is_study(i) && same_name(&self.plan[i].name, &b.name) && self.check_closable(i).is_ok())
                .ok_or(format!("«{}»: close закрывает начатый и ещё не закрытый блок, а такого в плане нет.", b.name.trim()))?;
            taken.push(i);
            let worked = self.block_work_live(i, now);
            if worked < MIN {
                // A misclick start: nothing to keep, the block simply goes.
                keep[j] = false;
                continue;
            }
            // Its length stays until it is closed right after the plan is in place.
            b.minutes = self.plan[i].minutes.max(worked_min(worked));
            closing.push((b.name.clone(), self.plan[i].minutes));
        }
        let plan: Vec<PlanBlock> = plan.into_iter().zip(keep).filter_map(|(b, k)| k.then_some(b)).collect();
        let backup = self.clone();
        let mut changes = self.set_plan(now, plan, locked)?;
        let mut ev = vec![];
        for (name, from_min) in closing {
            let found = (0..self.plan.len()).find(|&i| self.is_study(i) && same_name(&self.plan[i].name, &name) && self.check_closable(i).is_ok());
            let res = found.ok_or(format!("«{name}» уже закрыт.")).and_then(|i| self.finish_block_at(now, cfg, i, by));
            match res {
                Ok((f, e)) => {
                    changes.retain(|c| !(same_name(&c.block, &name) && matches!(c.change.as_str(), "shortened" | "lengthened")));
                    changes.push(PlanChange { block: f.block, change: "closed".into(), from_min: Some(from_min), to_min: Some(f.to_min) });
                    ev.extend(e);
                }
                Err(e) => {
                    *self = backup;
                    return Err(e);
                }
            }
        }
        Ok((changes, ev))
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
        ev.extend(self.segment_events(now, quiet));
        if !quiet {
            ev.extend(self.ask_what_now(now));
        }
        for e in &mut self.emergencies {
            if now >= e.until && !e.notified {
                e.notified = true;
                ev.push(Event::EmergencyEnded);
            }
        }
        for f in &mut self.focus {
            if now >= f.until && !f.notified {
                f.notified = true;
                ev.push(Event::FocusEnded);
            }
        }
        if !self.day_end_notified && self.started_at.is_some() && self.completed_at.is_none() && self.study_day && self.after_day_end(now, cfg) {
            self.day_end_notified = true;
            let text = match self.focus_until(now) {
                Some(u) => format!("{} — учебный день закончился, фокус-блокировка до {}", fmt_day_min(self.day_end(cfg)), clock::hm(u, cfg.tz_offset_min)),
                None => format!("{} — блокировка снята по времени", fmt_day_min(self.day_end(cfg))),
            };
            self.log(now, "day_end", text);
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

    fn advance(&mut self, now: Ts, cfg: &Config, ev: &mut Vec<Event>) -> bool {
        match self.phase.clone() {
            Phase::Work { block, dur_ms, elapsed_ms, since: Some(s) } => {
                let end = s + (dur_ms - elapsed_ms);
                if now < end {
                    return false;
                }
                self.finish_work(end, block, dur_ms, cfg, ev);
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

    fn finish_work(&mut self, end: Ts, block: usize, dur_ms: i64, cfg: &Config, ev: &mut Vec<Event>) {
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
                    ev.push(Event::BlockCompleted { block, block_name: name, work_ms, pauses, pause_ms });
                    self.after_block(end, cfg, ev);
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
    /// study | single | focus | emergency | pause_access | lunch_at_pc | segment_access |
    /// not_study_day | not_started | completed | day_end
    pub reason: String,
    /// End of the current access window: blocked again after it.
    pub until: Option<Ts>,
    /// End of the running focus lock, whatever the reason above.
    pub focus_until: Option<Ts>,
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
        let mut c = Config::default();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 90), PlanBlock::new("Экстернат", 150)];
        c
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
        c.profiles.full.plan = vec![PlanBlock::new("A", 45)];
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
    fn locked_plan_can_shrink_but_keeps_worked_time() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 90), PlanBlock::new("Экстернат", 150), PlanBlock::new("Словацкий", 90)];
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let now = start() + 30 * MIN;
        // not started blocks: cut and remove
        let plan = vec![PlanBlock::new("Математика", 90), PlanBlock::new("Экстернат", 60)];
        let ch = d.set_plan(now, plan, true).unwrap();
        assert_eq!(ch.len(), 2);
        assert!(ch.iter().any(|c| c.change == "shortened" && c.block == "Экстернат" && c.to_min == Some(60)));
        assert!(ch.iter().any(|c| c.change == "removed" && c.block == "Словацкий"));
        // current block: not below the 30 min already worked
        let too_short = vec![PlanBlock::new("Математика", 20), PlanBlock::new("Экстернат", 60)];
        assert!(d.set_plan(now, too_short, true).is_err());
        // started block can't be removed
        assert!(d.set_plan(now, vec![PlanBlock::new("Экстернат", 60)], true).is_err());
        // cut the current block to 40: the running part ends at 40, worked time is kept
        d.set_plan(now, vec![PlanBlock::new("Математика", 40), PlanBlock::new("Экстернат", 60)], true).unwrap();
        assert!(matches!(d.phase, Phase::Work { dur_ms, .. } if dur_ms == 40 * MIN));
        let ev = d.tick(start() + 40 * MIN, &c);
        assert!(ev.iter().any(|e| matches!(e, Event::BlockCompleted { work_ms, .. } if *work_ms == 40 * MIN)));
        assert!(matches!(d.phase, Phase::Break { brk: BreakKind::Between, next: 1, .. }));
        // adding and lengthening still works
        let more = vec![PlanBlock::new("Математика", 40), PlanBlock::new("Физика", 45), PlanBlock::new("Экстернат", 90)];
        d.set_plan(start() + 41 * MIN, more, true).unwrap();
        assert_eq!(d.progress.len(), 3);
        assert_eq!(d.progress[0].work_ms, 40 * MIN);
        assert!(matches!(d.phase, Phase::Break { next: 1, .. }));
    }

    #[test]
    fn removing_the_rest_closes_the_day() {
        let c = cfg();
        // Math is done (90 min), Экстернат not started: break between blocks.
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.tick(start() + 45 * MIN, &c);
        d.tick(start() + 55 * MIN, &c);
        d.start_next(start() + 55 * MIN).unwrap();
        d.tick(start() + 100 * MIN, &c);
        assert!(matches!(d.phase, Phase::Break { brk: BreakKind::Between, next: 1, .. }));
        d.set_plan(start() + 101 * MIN, vec![PlanBlock::new("Математика", 90)], true).unwrap();
        assert_eq!(d.phase, Phase::Done);
        assert!(!d.lock_state(start() + 101 * MIN, &c).blocked);
        assert_eq!(d.progress[0].work_ms, 90 * MIN);
    }

    #[test]
    fn day_end_shift_keeps_lock_until_new_time() {
        let c = cfg();
        let ten_pm = t("2026-09-28T19:00:00Z");
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.pause(start() + MIN, &c).unwrap();
        let ch = d.set_day_end(start() + 2 * MIN, &c, "23:00", Some("сдвиг"), "mcp").unwrap();
        assert_eq!((ch.from_min, ch.to_min), (22 * 60, 23 * 60));
        assert!(d.lock_state(ten_pm, &c).blocked);
        assert!(!d.tick(ten_pm, &c).contains(&Event::DayEndReached));
        assert!(d.lock_state(ten_pm + 59 * MIN, &c).blocked);
        assert!(d.tick(ten_pm + 60 * MIN, &c).contains(&Event::DayEndReached));
        assert!(!d.lock_state(ten_pm + 60 * MIN, &c).blocked);
        // the config template is untouched
        assert_eq!(c.day_end_min, 22 * 60);
        assert_eq!(d.day_end_changes.len(), 1);
        assert!(d.events.iter().any(|e| e.kind == "day_end_change" && e.text.contains("22:00 → 23:00 (сдвиг)")));
    }

    #[test]
    fn day_end_rules() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        // 13:00 now: the past and after 02:00 are refused
        assert!(d.set_day_end(start(), &c, "12:00", None, "ui").is_err());
        assert!(d.set_day_end(start(), &c, "02:30", None, "ui").is_err());
        assert!(d.set_day_end(start(), &c, "25:00", None, "ui").is_err());
        // earlier end is fine as long as it is still ahead
        d.set_day_end(start(), &c, "18:00", None, "ui").unwrap();
        assert!(!d.lock_state(t("2026-09-28T15:00:00Z"), &c).blocked);
        // past midnight: the day stays live and locked until 01:00 of the next date
        d.set_day_end(start(), &c, "01:00", None, "ui").unwrap();
        assert_eq!(d.day_end(&c), 25 * 60);
        let half_past_midnight = t("2026-09-28T21:30:00Z");
        assert!(d.is_live(half_past_midnight, &c));
        assert!(d.lock_state(half_past_midnight, &c).blocked);
        let one_am = t("2026-09-28T22:00:00Z");
        assert!(!d.is_live(one_am, &c));
        assert!(!d.lock_state(one_am, &c).blocked);
        // back to the template value clears the override
        d.set_day_end(start(), &c, "22:00", None, "ui").unwrap();
        assert_eq!(d.day_end_min, None);
        assert_eq!(d.day_end_changes.len(), 3);
    }

    #[test]
    fn day_saved_by_0_2_4_keeps_its_extension() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let mut json: serde_json::Value = serde_json::to_value(&d).unwrap();
        json.as_object_mut().unwrap().remove("day_end_changes");
        json["day_end_min"] = serde_json::json!(23 * 60);
        let d: DayState = serde_json::from_value(json).unwrap();
        assert_eq!(d.day_end(&c), 23 * 60);
        assert!(d.lock_state(t("2026-09-28T19:30:00Z"), &c).blocked);
        assert!(d.day_end_changes.is_empty());
    }

    #[test]
    fn forecast_counts_breaks() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        // 90 + 150 min work; breaks: 1 short in math, 1 between, 3 short in Экстернат
        let f = d.forecast(start(), &c);
        assert_eq!(f.work_left_ms, 240 * MIN);
        assert_eq!(f.breaks_left_ms, (4 * 10 + 20) * MIN);
        assert!(f.fits);
        d.set_day_end(start(), &c, "17:00", None, "ui").unwrap();
        let f = d.forecast(start(), &c);
        assert!(!f.fits);
        assert_eq!(f.margin_ms, (240 - 300) * MIN);
    }

    fn full_with_breaks() -> Vec<PlanBlock> {
        vec![
            PlanBlock::new("Экстернат", 180),
            PlanBlock::brk("Обед", 45),
            PlanBlock::brk("Сон", 20),
            PlanBlock::new("Словацкий", 90),
            PlanBlock::new("Математика", 60),
        ]
    }

    #[test]
    fn next_days_start_with_their_own_plan() {
        let mut c = cfg();
        let sunday = t("2026-10-04T09:00:00Z");
        let monday = t("2026-10-05T06:00:00Z");
        let tuesday = t("2026-10-06T06:00:00Z");
        let sun = DayState::new(sunday, &c);
        // On Sunday the agent sets Monday's template and a one-off plan for Tuesday.
        c.profiles.full.plan = full_with_breaks();
        let tue = NaiveDate::from_ymd_opt(2026, 10, 6).unwrap();
        c.plan_overrides.insert(tue, vec![PlanBlock::new("Словацкий", 120)]);
        let c: Config = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(c.plan_overrides[&tue], vec![PlanBlock::new("Словацкий", 120)]);
        // Sunday is untouched: still a day off with the day-off template.
        assert_eq!((sun.kind, sun.plan.len()), (DayKind::Off, 0));
        assert!(c.profile(DayKind::Off).plan.is_empty());
        let mon = DayState::new(monday, &c);
        assert_eq!(mon.kind, DayKind::Full);
        assert_eq!(mon.plan, full_with_breaks());
        assert_eq!(mon.progress.len(), 5);
        let mut c = c;
        assert!(!c.drop_overrides_through(mon.date));
        let d = DayState::new(tuesday, &c);
        assert_eq!((d.kind, d.plan.clone()), (DayKind::Full, vec![PlanBlock::new("Словацкий", 120)]));
        assert!(c.drop_overrides_through(d.date));
        assert!(c.plan_overrides.is_empty());
    }

    #[test]
    fn late_start_forecast_shows_the_overrun_at_once() {
        let mut c = cfg();
        c.profiles.full.plan = full_with_breaks();
        // Work 330 + breaks 70 (5 short, 1 between) + lunch 45, nap 20 and 10 min to get ready
        // for it = 75: 7 h 55 min; ends 22:00.
        let late = t("2026-09-28T14:00:00Z"); // 17:00 MSK
        let mut d = DayState::new(late, &c);
        let f = d.forecast(late, &c);
        assert_eq!((f.work_left_ms, f.breaks_left_ms, f.segments_left_ms), (330 * MIN, 70 * MIN, 75 * MIN));
        assert!(!f.fits);
        assert_eq!(f.margin_ms, -175 * MIN);
        d.start_day(late, &c).unwrap();
        let f = d.forecast(late, &c);
        assert!(!f.fits);
        assert_eq!(f.margin_ms, -175 * MIN);
        // The same plan started at 13:00 fits with 65 min to spare.
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let f = d.forecast(start(), &c);
        assert!(f.fits);
        assert_eq!(f.margin_ms, 65 * MIN);
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

    #[test]
    fn day_end_moves_only_later_and_relocks() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let ten_pm = t("2026-09-28T19:00:00Z");
        assert!(d.tick(ten_pm, &c).contains(&Event::DayEndReached));
        assert!(!d.lock_state(ten_pm, &c).blocked);
        assert!(d.extend_day_end(ten_pm, &c, 21 * 60).is_err());
        assert!(d.extend_day_end(ten_pm, &c, 26 * 60 + 1).is_err());
        d.extend_day_end(ten_pm, &c, 23 * 60).unwrap();
        assert!(d.lock_state(ten_pm, &c).blocked);
        let eleven = t("2026-09-28T20:00:00Z");
        assert!(d.lock_state(eleven - 1, &c).blocked);
        assert!(d.tick(eleven, &c).contains(&Event::DayEndReached));
        assert!(!d.lock_state(eleven, &c).blocked);
    }

    #[test]
    fn kind_switch_rules() {
        let mut c = cfg();
        c.profiles.light.plan = vec![PlanBlock::new("Математика", 45)];
        c.profiles.off.block = false;
        // Monday is full by default; before the start anything goes and the plan follows.
        let mut d = DayState::new(start(), &c);
        assert_eq!(d.kind, DayKind::Full);
        d.set_kind(start(), &c, DayKind::Light, false).unwrap();
        assert_eq!(d.plan, c.profiles.light.plan);
        d.start_day(start(), &c).unwrap();
        let locked = d.plan_lock(start(), &c);
        assert!(locked);
        // During the lock: not lighter, but heavier merges the template in.
        assert!(d.set_kind(start(), &c, DayKind::Off, locked).is_err());
        d.set_kind(start(), &c, DayKind::Full, locked).unwrap();
        assert_eq!(d.kind, DayKind::Full);
        assert_eq!(d.plan[0], PlanBlock::new("Математика", 90));
        assert_eq!(d.plan[1], PlanBlock::new("Экстернат", 150));
        assert!(d.study_day);
    }

    #[test]
    fn finish_block_closes_on_worked_minutes() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        // 45 min part 1, break, 39.6 min of part 2 -> 84.6 min worked
        d.tick(start() + 45 * MIN, &c);
        d.tick(start() + 55 * MIN, &c);
        d.start_next(start() + 55 * MIN).unwrap();
        let now = start() + 55 * MIN + 39 * MIN + 36 * crate::clock::SEC;
        let (f, ev) = d.finish_block(now, &c, None, "ui").unwrap();
        // Whole minutes, rounded down like done_min: 84,6 -> 84.
        assert_eq!((f.from_min, f.to_min), (90, 84));
        assert_eq!(f.worked_ms, 84 * MIN + 36 * crate::clock::SEC);
        assert!(ev.iter().any(|e| matches!(e, Event::BlockCompleted { .. })));
        assert!(d.is_block_done(0));
        assert_eq!(d.plan[0].minutes, 84);
        assert_eq!(d.progress[0].parts_done, 2);
        assert!(matches!(d.phase, Phase::Break { brk: BreakKind::Between, next: 1, .. }));
        assert!(d.events.iter().any(|e| e.kind == "block_finish" && e.text.contains("84,6 из 90 мин")));
        // a later plan edit does not reopen it; lengthening does
        d.set_plan(now + MIN, vec![PlanBlock::new("Математика", 84), PlanBlock::new("Экстернат", 120)], true).unwrap();
        assert!(d.is_block_done(0));
        d.set_plan(now + MIN, vec![PlanBlock::new("Математика", 100), PlanBlock::new("Экстернат", 120)], true).unwrap();
        assert!(!d.is_block_done(0));
    }

    /// Acceptance (MCP): a block started for 2 seconds is deleted; the day goes on.
    #[test]
    fn block_with_seconds_of_work_can_be_removed() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let now = start() + 2 * crate::clock::SEC;
        let ch = d.set_plan(now, vec![PlanBlock::new("Экстернат", 150)], true).unwrap();
        assert_eq!(ch[0].change, "removed");
        assert_eq!(d.plan.len(), 1);
        // the running part went with it: the timer waits for the next block, still locked
        assert!(matches!(d.phase, Phase::Await { next: 0, .. }));
        assert_eq!(d.progress[0].work_ms, 0);
        assert!(d.lock_state(now, &c).blocked);
        // a minute of work is not noise any more
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let err = d.set_plan(start() + MIN, vec![PlanBlock::new("Экстернат", 150)], true).unwrap_err();
        assert!(err.contains("close: true"), "{err}");
        // renaming is removing + adding: same rule
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.set_plan(start() + 5 * crate::clock::SEC, vec![PlanBlock::new("Алгебра", 90), PlanBlock::new("Экстернат", 150)], true).unwrap();
        assert_eq!(d.plan[0].name, "Алгебра");
    }

    /// Acceptance (MCP): done_min and the "can't cut below" limit are the same number.
    #[test]
    fn cut_limit_is_the_rounded_down_minutes_worked() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let now = start() + 10 * MIN + 40 * crate::clock::SEC;
        let done_min = crate::view::build(&d, &c, now).blocks[0].work_ms / MIN;
        assert_eq!(done_min, 10);
        let err = d.set_plan(now, vec![PlanBlock::new("Математика", 9), PlanBlock::new("Экстернат", 150)], true).unwrap_err();
        assert!(err.contains("уже отработано 10 мин"), "{err}");
        // exactly the minutes worked is fine: the part ends right away and closes the block
        d.set_plan(now, vec![PlanBlock::new("Математика", 10), PlanBlock::new("Экстернат", 150)], true).unwrap();
        let ev = d.tick(now + 200, &c);
        assert!(ev.iter().any(|e| matches!(e, Event::BlockCompleted { .. })));
        // the work is never trimmed: 10 min 40 s stays in the stats
        assert_eq!(d.progress[0].work_ms, 10 * MIN + 40 * crate::clock::SEC);
        assert_eq!(crate::phone::snapshot(&d, &c, now).blocks[0].min_minutes, 10);
    }

    /// Acceptance (MCP): a started block is closed as it is through the plan.
    #[test]
    fn plan_edit_closes_a_started_block_as_is() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 90), PlanBlock::new("Словацкий", 90), PlanBlock::new("Экстернат", 150)];
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let now = start() + 30 * MIN + 59 * crate::clock::SEC;
        // close Математика as is and move Экстернат ahead of Словацкий in one call
        let plan = vec![PlanBlock::new("Математика", 1), PlanBlock::new("Экстернат", 150), PlanBlock::new("Словацкий", 90)];
        let (ch, ev) = d.edit_plan(now, &c, plan, &[true, false, false], true, "mcp").unwrap();
        assert_eq!(d.plan[0].minutes, 30);
        assert!(d.is_block_done(0) && d.progress[0].closed);
        assert_eq!(d.progress[0].work_ms, 30 * MIN + 59 * crate::clock::SEC);
        assert!(ev.iter().any(|e| matches!(e, Event::BlockCompleted { .. })));
        assert!(ch.iter().any(|c| c.change == "closed" && c.from_min == Some(90) && c.to_min == Some(30)));
        // one block moved: only it is reported
        let moved: Vec<_> = ch.iter().filter(|c| c.change == "moved").map(|c| c.block.as_str()).collect();
        assert_eq!(moved.len(), 1);
        assert!(matches!(d.phase, Phase::Break { brk: BreakKind::Between, next: 1, .. }));
        assert_eq!(d.plan[1].name, "Экстернат");
        // close on a block under a minute removes it; on an unstarted one it is refused, all or nothing
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let t = start() + 20 * crate::clock::SEC;
        let before = d.plan.clone();
        let bad = vec![PlanBlock::new("Математика", 90), PlanBlock::new("Словацкий", 90), PlanBlock::new("Экстернат", 100)];
        assert!(d.edit_plan(t, &c, bad, &[false, true, false], true, "mcp").is_err());
        assert_eq!(d.plan, before);
        let plan = vec![PlanBlock::new("Математика", 90), PlanBlock::new("Словацкий", 90), PlanBlock::new("Экстернат", 150)];
        let (ch, _) = d.edit_plan(t, &c, plan, &[true, false, false], true, "mcp").unwrap();
        assert_eq!(d.plan.len(), 2);
        assert!(ch.iter().any(|c| c.change == "removed" && c.block == "Математика"));
    }

    #[test]
    fn started_blocks_and_segments_move_freely() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("A", 45), PlanBlock::new("B", 45), PlanBlock::brk("Обед", 30), PlanBlock::new("C", 45)];
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.tick(start() + 45 * MIN, &c); // A done, B next
        d.start_next(start() + 50 * MIN).unwrap();
        // B is running; put the lunch first, C before B, and A last — case of the name ignored
        let plan = vec![PlanBlock::brk("Обед", 30), PlanBlock::new("C", 45), PlanBlock::new("b", 45), PlanBlock::new("A", 45)];
        d.set_plan(start() + 55 * MIN, plan, true).unwrap();
        assert!(matches!(d.phase, Phase::Work { block: 2, .. }));
        assert_eq!(d.plan[2].name, "b");
        assert!(d.is_block_done(3));
        assert_eq!(d.progress[3].work_ms, 45 * MIN);
        assert!(same_name("Математика", " математика "));
    }

    #[test]
    fn finish_block_rules_and_last_block() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("A", 90)];
        let mut d = DayState::new(start(), &c);
        assert!(d.finish_block(start(), &c, None, "ui").is_err());
        d.start_day(start(), &c).unwrap();
        assert!(d.finish_block(start() + 10 * crate::clock::SEC, &c, None, "ui").is_err());
        assert!(d.finish_block(start() + MIN, &c, Some("Б"), "ui").is_err());
        d.pause(start() + 20 * MIN, &c).unwrap();
        let (f, ev) = d.finish_block(start() + 30 * MIN, &c, Some("a"), "mcp").unwrap();
        assert_eq!(f.to_min, 20);
        assert!(ev.iter().any(|e| matches!(e, Event::DayCompleted { .. })));
        assert_eq!(d.phase, Phase::Done);
        assert!(d.pause.is_none());
        assert!(!d.lock_state(start() + 31 * MIN, &c).blocked);
    }

    #[test]
    fn skipped_break_can_be_taken_back() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.tick(start() + 45 * MIN, &c); // break 45..55
        let skip = start() + 48 * MIN;
        d.start_next(skip).unwrap();
        assert!(d.phase.is_work());
        assert!(crate::view::build(&d, &c, skip + 1000).can.undo_skip);
        d.undo_skip(skip + 8 * crate::clock::SEC).unwrap();
        // the break kept counting through the undo window and ends on time
        assert!(matches!(d.phase, Phase::Break { brk: BreakKind::Short, .. }));
        assert_eq!(d.phase_elapsed(skip + 8 * crate::clock::SEC), 3 * MIN + 8 * crate::clock::SEC);
        assert_eq!(d.progress[0].work_ms, 45 * MIN);
        assert!(d.tick(start() + 55 * MIN, &c).iter().any(|e| matches!(e, Event::BreakEnded { .. })));
        assert!(d.undo_skip(start() + 55 * MIN).is_err());
    }

    #[test]
    fn undo_window_is_ten_seconds_and_restores_first_start() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.tick(start() + 45 * MIN, &c);
        d.tick(start() + 55 * MIN, &c);
        d.start_next(start() + 55 * MIN).unwrap();
        d.tick(start() + 100 * MIN, &c); // block 0 done -> between break, next = 1 (not started)
        d.pause(start() + 101 * MIN, &c).unwrap();
        let skip = start() + 102 * MIN;
        d.start_next(skip).unwrap();
        assert!(d.progress[1].started_at.is_some());
        assert!(d.undo_skip(skip + 11 * crate::clock::SEC).is_err());
        d.start_next(skip + 20 * crate::clock::SEC).ok();
        // a fresh skip from the between break: undo restores the pause and "not started"
        let mut d2 = DayState::new(start(), &c);
        d2.start_day(start(), &c).unwrap();
        d2.tick(start() + 45 * MIN, &c);
        d2.tick(start() + 55 * MIN, &c);
        d2.start_next(start() + 55 * MIN).unwrap();
        d2.tick(start() + 100 * MIN, &c);
        d2.pause(start() + 101 * MIN, &c).unwrap();
        let n = d2.pauses.len();
        d2.start_next(skip).unwrap();
        d2.undo_skip(skip + 5 * crate::clock::SEC).unwrap();
        assert!(d2.progress[1].started_at.is_none());
        assert!(d2.pause.is_some());
        assert_eq!(d2.pauses.len(), n);
        assert!(matches!(d2.phase, Phase::Break { brk: BreakKind::Between, since: None, .. }));
    }

    #[test]
    fn block_note_is_asked_once() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("A", 45), PlanBlock::new("B", 45)];
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        assert_eq!(d.pending_note(), None);
        let ev = d.tick(start() + 45 * MIN, &c);
        assert!(ev.iter().any(|e| matches!(e, Event::BlockCompleted { block: 0, .. })));
        assert_eq!(d.pending_note(), Some(0));
        d.set_block_note(start() + 46 * MIN, 0, Some("  скучно на интегралах, лез в телефон  "), "ui").unwrap();
        assert_eq!(d.progress[0].note.as_deref(), Some("скучно на интегралах, лез в телефон"));
        assert_eq!(d.pending_note(), None);
        assert!(d.set_block_note(start(), 1, Some("x"), "ui").is_err());
        // skipping
        d.start_next(start() + 50 * MIN).unwrap();
        d.tick(start() + 95 * MIN, &c);
        assert_eq!(d.pending_note(), Some(1));
        d.set_block_note(start() + 96 * MIN, 1, None, "ui").unwrap();
        assert_eq!(d.pending_note(), None);
        assert_eq!(d.progress[1].note, None);
    }

    #[test]
    fn blink_pauses_are_not_recorded() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let p = start() + 5 * MIN;
        d.pause(p, &c).unwrap();
        // an ongoing 5 s pause is not in the stats yet
        assert_eq!(crate::stats::day_stats(&d, 180, p + 5000).pauses_count, 0);
        d.resume(p + 9_999).unwrap();
        assert!(d.pauses.is_empty());
        assert!(!d.events.iter().any(|e| e.kind == "pause" || e.kind == "resume"));
        // exactly 10 s counts
        d.pause(p + MIN, &c).unwrap();
        assert_eq!(crate::stats::day_stats(&d, 180, p + MIN + 10_000).pauses_count, 1);
        d.resume(p + MIN + 10_000).unwrap();
        assert_eq!(d.pauses.len(), 1);
        let st = crate::stats::day_stats(&d, 180, p + 2 * MIN);
        assert_eq!(st.pauses_count, 1);
        assert_eq!(st.blocks[0].pauses, 1);
        // the paused time still froze the timer: 5 s of work were not counted
        assert_eq!(d.phase_elapsed(p + MIN + 10_000), 5 * MIN + MIN - 9_999);
    }

    #[test]
    fn undo_after_a_blink_pause_keeps_older_records() {
        let c = cfg();
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.pause(start() + MIN, &c).unwrap();
        d.resume(start() + 3 * MIN).unwrap();
        assert_eq!(d.pauses.len(), 1);
        d.tick(start() + 47 * MIN, &c); // in the break
        d.pause(start() + 48 * MIN, &c).unwrap();
        d.start_next(start() + 48 * MIN + 3_000).unwrap(); // 3 s pause: not recorded
        d.undo_skip(start() + 48 * MIN + 5_000).unwrap();
        assert_eq!(d.pauses.len(), 1);
        assert!(d.pause.is_some());
    }

    #[test]
    fn off_day_has_no_lock() {
        let c = cfg();
        let sunday = t("2026-10-04T10:00:00Z");
        let mut d = DayState::new(sunday, &c);
        assert_eq!(d.kind, DayKind::Off);
        d.set_plan(sunday, vec![PlanBlock::new("Чтение", 30)], false).unwrap();
        d.start_day(sunday, &c).unwrap();
        assert!(!d.lock_state(sunday, &c).blocked);
    }

    #[test]
    fn focus_lock_blocks_any_day_and_only_moves_later() {
        let c = cfg();
        let sunday = t("2026-10-04T10:00:00Z");
        let mut d = DayState::new(sunday, &c);
        assert!(!d.lock_state(sunday, &c).blocked);
        assert!(d.start_focus(sunday, &c, 0, None, "mcp").is_err());
        assert!(d.start_focus(sunday, &c, MAX_FOCUS_MIN + 1, None, "mcp").is_err());

        let ch = d.start_focus(sunday, &c, 90, Some(" ютуб "), "mcp").unwrap();
        assert_eq!(ch, FocusChange { from: None, until: sunday + 90 * MIN, extended: false });
        assert_eq!(d.focus[0].reason.as_deref(), Some("ютуб"));
        let l = d.lock_state(sunday + MIN, &c);
        assert!(l.blocked && l.base);
        assert_eq!(l.reason, "focus");
        assert_eq!((l.until, l.focus_until), (None, Some(sunday + 90 * MIN)));

        // During the focus lock the block list only grows, as during a study day.
        let mut fewer = c.clone();
        fewer.blocklist.sites.retain(|s| s != "x.com");
        assert!(c.check_update(&fewer, crate::config::EditContext { locked: d.base_lock(sunday + MIN, &c) }).is_err());

        // Only later: a shorter or equal end is refused and changes nothing.
        let ten = sunday + 10 * MIN;
        assert!(d.start_focus(ten, &c, 60, None, "mcp").is_err());
        assert!(d.start_focus(ten, &c, 80, None, "mcp").is_err());
        assert_eq!(d.focus_until(ten), Some(sunday + 90 * MIN));
        let ch = d.start_focus(ten, &c, 120, None, "mcp").unwrap();
        assert_eq!(ch, FocusChange { from: Some(sunday + 90 * MIN), until: sunday + 130 * MIN, extended: true });
        assert_eq!(d.focus.len(), 1);
        assert_eq!(d.focus[0].reason.as_deref(), Some("ютуб"));

        assert!(!d.tick(sunday + 129 * MIN, &c).contains(&Event::FocusEnded));
        assert!(d.tick(sunday + 130 * MIN, &c).contains(&Event::FocusEnded));
        assert!(!d.tick(sunday + 131 * MIN, &c).contains(&Event::FocusEnded));
        let l = d.lock_state(sunday + 130 * MIN, &c);
        assert!(!l.blocked && !l.base);
        assert_eq!((l.reason.as_str(), l.focus_until), ("not_study_day", None));
        assert_eq!(d.events.iter().filter(|e| e.kind == "focus").count(), 2);

        // An extension with a reason replaces it; without one keeps it.
        let s = sunday + 200 * MIN;
        d.start_focus(s, &c, 30, None, "mcp").unwrap();
        d.start_focus(s, &c, 40, Some("диплом"), "mcp").unwrap();
        d.start_focus(s, &c, 50, None, "mcp").unwrap();
        assert_eq!(d.focus.last().unwrap().reason.as_deref(), Some("диплом"));
    }

    #[test]
    fn focus_lock_is_opened_by_emergency_only() {
        let mut c = cfg();
        c.pause_access = true;
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let p = start() + 5 * MIN;
        d.pause(p, &c).unwrap();
        assert_eq!(d.lock_state(p + MIN, &c).reason, "pause_access");
        d.start_focus(p + MIN, &c, 60, None, "mcp").unwrap();
        // The pause access window is closed, not left running under the focus lock.
        assert_eq!(d.pause.as_ref().unwrap().access_until(), Some(p + MIN));
        let ev = d.tick(p + 10 * MIN, &c);
        assert!(!ev.contains(&Event::PauseAccessExpired) && !ev.iter().any(|e| matches!(e, Event::PauseAccessWarning { .. })));
        assert!(d.extend_pause_access(p + 10 * MIN, &c).is_err());
        let l = d.lock_state(p + 2 * MIN, &c);
        assert!(l.blocked);
        assert_eq!(l.reason, "study");
        assert_eq!(l.focus_until, Some(p + 61 * MIN));

        d.emergency(p + 3 * MIN, &c, &c.emergency_phrase).unwrap();
        let l = d.lock_state(p + 4 * MIN, &c);
        assert!(!l.blocked);
        assert_eq!(l.reason, "emergency");
        assert!(d.lock_state(p + 3 * MIN + c.emergency_min as i64 * MIN, &c).blocked);

        // Past the day end the focus lock still holds.
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        let ten_pm = t("2026-09-28T19:00:00Z");
        d.start_focus(ten_pm - 30 * MIN, &c, 90, None, "mcp").unwrap();
        let l = d.lock_state(ten_pm + 30 * MIN, &c);
        assert!(l.blocked);
        assert_eq!(l.reason, "focus");
        assert!(!d.lock_state(ten_pm + 60 * MIN, &c).blocked);
    }

    #[test]
    fn focus_lock_outlives_midnight_and_old_days_load() {
        let c = cfg();
        // Sunday 23:00 MSK, until Monday 01:00 MSK.
        let late = t("2026-10-04T20:00:00Z");
        let mut sunday = DayState::new(late, &c);
        sunday.start_focus(late, &c, 120, Some("сон"), "mcp").unwrap();
        // Emergency access 00:25..00:35 MSK, opened on Sunday under the focus lock.
        let sos = t("2026-10-04T21:25:00Z");
        sunday.emergency(sos, &c, &c.emergency_phrase).unwrap();
        let after_midnight = t("2026-10-04T21:30:00Z");
        assert!(!sunday.is_live(after_midnight, &c));
        sunday.finalize(after_midnight);

        let mut monday = DayState::new(after_midnight, &c);
        assert_eq!(monday.kind, DayKind::Full);
        monday.carry_focus(&sunday, after_midnight, &c);
        monday.carry_focus(&sunday, after_midnight, &c);
        assert_eq!(monday.focus.len(), 1);
        assert_eq!(monday.focus[0].reason.as_deref(), Some("сон"));
        // The emergency keeps its time once, then the focus lock is back.
        assert_eq!(monday.emergencies.len(), 1);
        assert_eq!(monday.lock_state(after_midnight, &c).reason, "emergency");
        let sos_end = sos + c.emergency_min as i64 * MIN;
        let l = monday.lock_state(sos_end, &c);
        assert!(l.blocked);
        assert_eq!((l.reason.as_str(), l.focus_until), ("focus", Some(late + 120 * MIN)));
        assert!(monday.tick(late + 120 * MIN, &c).contains(&Event::FocusEnded));

        // Nothing to carry once it is over.
        let mut later = DayState::new(late + 3 * 60 * MIN, &c);
        later.carry_focus(&sunday, late + 3 * 60 * MIN, &c);
        assert!(later.focus.is_empty());

        // Days saved before focus locks existed still load.
        let mut json = serde_json::to_value(&monday).unwrap();
        json.as_object_mut().unwrap().remove("focus");
        let old: DayState = serde_json::from_value(json).unwrap();
        assert!(old.focus.is_empty());
    }
}

#[cfg(test)]
mod lunch_tests {
    use super::*;
    use chrono::DateTime;

    fn t(s: &str) -> Ts {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }

    #[test]
    fn lunch_starts_from_a_paused_break() {
        let mut c = Config::default();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 105)];
        let s = t("2026-09-30T10:00:00Z");
        for with_timer in [true, false] {
            let mut d = DayState::new(s, &c);
            d.start_day(s, &c).unwrap();
            d.tick(s + 46 * MIN, &c);
            d.pause(s + 47 * MIN, &c).unwrap();
            assert!(crate::view::build(&d, &c, s + 48 * MIN).can.lunch);
            d.start_lunch(s + 48 * MIN, with_timer, false).unwrap();
            assert!(d.pause.is_none());
            let v = crate::view::build(&d, &c, s + 49 * MIN);
            if with_timer {
                assert_eq!(v.phase.kind, "lunch_break");
                assert!(v.phase.running);
            } else {
                assert_eq!(v.phase.kind, "lunch");
            }
        }
    }
}
