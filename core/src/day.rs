//! The study day: plan, timer state machine, pauses, access windows and the raw log.
//!
//! Everything is driven by explicit timestamps so the engine is deterministic and testable.
//! The app calls [`DayState::tick`] a few times per second and reacts to returned [`Event`]s.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::clock::{self, Ts, MIN};
use crate::config::{fmt_hm, Config, DayKind, Timing};
use crate::i18n::tr;
use crate::tr;

/// A remainder shorter than this is merged into the previous work segment (50 min -> one part).
const MERGE_TAIL_MS: i64 = 15 * MIN;
const PAUSE_REMINDER_MS: i64 = 5 * MIN;
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
            tr!("config_segment")
        } else {
            tr!("day_block")
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
    /// Access on pause windows (before 0.5.0): still counted into the pause record.
    #[serde(default)]
    pub access: Vec<AccessWindow>,
    pub reminded_at: Ts,
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

/// "Автоблокировка с 13:00": the block list went on because the study day had not started by
/// then. Kept here from the moment it fires, so a restart, a power cut or a settings change never
/// lifts it: it holds until "Начать день" (then the day's own lock), the day end or the date
/// change. Only emergency access opens it for a while.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoLock {
    /// The time from the profile, minutes after the day's midnight.
    pub planned_min: u32,
    /// When it went on: the planned time, or later when the app was not running then.
    pub at: Ts,
    /// "Начать день" took over.
    #[serde(default)]
    pub started_at: Option<Ts>,
}

impl AutoLock {
    /// "Задержка старта": from the autolock going on to "Начать день".
    pub fn start_delay_ms(&self) -> Option<i64> {
        self.started_at.map(|s| (s - self.at).max(0))
    }
}

/// Warn this long before the autolock goes on.
pub const AUTOLOCK_WARN_MS: i64 = 5 * MIN;

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
    /// The autolock goes on in `left_ms` (five minutes before, once) unless the day starts.
    AutoLockSoon { left_ms: i64, at: Ts },
    /// The autolock went on: the day had not started by its time.
    AutoLockOn { at: Ts },
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
    /// The autolock of this day, once it went on.
    #[serde(default)]
    pub autolock: Option<AutoLock>,
    /// The "lock in 5 minutes" warning was given for the autolock due at this time.
    #[serde(default)]
    pub autolock_warned_for: Option<Ts>,
    /// Events raised by a command (not by the clock), handed out with the next tick.
    #[serde(skip)]
    pub pending_events: Vec<Event>,
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
        "removed" => tr!("day_removed", block = c.block),
        "added" => tr!("day_added_min", block = c.block, to = to),
        "moved" if from != to => tr!("day_moved_min", block = c.block, from = from, to = to),
        "moved" => tr!("day_moved", block = c.block),
        "closed" => tr!("day_closed_as_is_min", block = c.block, from = from, to = to),
        _ => tr!("day_min", block = c.block, from = from, to = to),
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
            autolock: None,
            autolock_warned_for: None,
            pending_events: vec![],
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
        self.log(now, "focus", tr!("day_focus_lock_continues_until", clock = clock::hm(f.until, cfg.tz_offset_min)));
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
                });
            }
            self.log(now, "restore", tr!("day_the_app_was_closed_for_min", min = worked_min(now - saved)));
        }
        // An access window left by an older version never survives a restart.
        if let Some(p) = &mut self.pause {
            if let Some(w) = p.access.last_mut() {
                w.until = w.until.min(now);
            }
        }
        if self.started_at.is_none() {
            self.study_day = self.blocks_by_profile(cfg);
        }
    }

    /// Whether a day not started yet is a study day by its profile. Once the autolock went on
    /// it stays one: turning the profile's lock off only shapes the days ahead.
    pub fn blocks_by_profile(&self, cfg: &Config) -> bool {
        cfg.profile(self.kind).block || self.autolock.is_some()
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
            self.log(end, "day_close", tr("day_day_closed_at_the_date_change"));
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
            // A day saved with a stale `next` (the plan got shorter) has no current block.
            Phase::Break { next, .. } | Phase::Await { next, .. } | Phase::Lunch { next, .. } | Phase::Segment { next, .. } => Some(next).filter(|&n| n < self.plan.len()),
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

    /// The study-day lock (before exemptions like break access or emergency).
    pub fn plan_lock(&self, now: Ts, cfg: &Config) -> bool {
        self.study_day && self.started_at.is_some() && self.completed_at.is_none() && !self.after_day_end(now, cfg)
    }

    pub fn single_lock(&self) -> bool {
        self.mode == Mode::Single
            && self.singles.last().is_some_and(|s| s.cfg.block && s.ended_at.is_none())
            && !matches!(self.phase, Phase::Idle | Phase::Done)
    }

    pub fn base_lock(&self, now: Ts, cfg: &Config) -> bool {
        self.plan_lock(now, cfg) || self.single_lock() || self.focus_until(now).is_some() || self.autolock_on(now, cfg)
    }

    /// When today's autolock goes on, while it still can: the profile has one, the day blocks,
    /// has a block to start and has not started, it is before the day end, and it is not on yet.
    pub fn autolock_due(&self, now: Ts, cfg: &Config) -> Option<Ts> {
        if self.autolock.is_some() || self.started_at.is_some() || self.completed_at.is_some() || !self.study_day {
            return None;
        }
        let at = clock::date_minute(self.date, cfg.tz_offset_min, cfg.autolock_min(self.kind)?);
        let end = self.day_end_at(cfg);
        (at < end && now < end && self.first_open_block().is_some()).then_some(at)
    }

    /// The autolock holds the block list right now: it went on and the day has not started.
    /// Only the record counts, not today's settings: changing them never lifts it.
    pub fn autolock_on(&self, now: Ts, cfg: &Config) -> bool {
        self.autolock.is_some() && self.started_at.is_none() && self.completed_at.is_none() && !self.after_day_end(now, cfg)
    }

    /// Five minutes before the autolock: a warning; at its time: on (and at once to the log).
    fn autolock_tick(&mut self, now: Ts, cfg: &Config) -> Option<Event> {
        let at = self.autolock_due(now, cfg)?;
        if now >= at {
            let planned_min = cfg.autolock_min(self.kind).unwrap_or_default();
            self.autolock = Some(AutoLock { planned_min, at: now, started_at: None });
            let text = if now - at < MIN {
                tr!("day_autolock_on", time = fmt_hm(planned_min))
            } else {
                // Late: the app was not running at its time, or the day got its autolock after it.
                tr!("day_autolock_on_late", time = fmt_hm(planned_min), clock = clock::hm(now, cfg.tz_offset_min))
            };
            self.log(now, "autolock", text);
            return Some(Event::AutoLockOn { at: now });
        }
        if now >= at - AUTOLOCK_WARN_MS && self.autolock_warned_for != Some(at) {
            self.autolock_warned_for = Some(at);
            return Some(Event::AutoLockSoon { left_ms: at - now, at });
        }
        None
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
        // A focus lock is opened by emergency access only: break access, lunch at the PC and
        // segment access are study-day exemptions.
        if focus_until.is_none() {
            // The break between blocks while its timer runs: a paused break stays blocked, and
            // once the timer runs out (waiting for "start") the lock is back.
            if let Phase::Break { brk: BreakKind::Between, dur_ms, elapsed_ms, since: Some(s), .. } = self.phase {
                if cfg.break_access && self.pause.is_none() {
                    return LockState {
                        blocked: false,
                        base: true,
                        reason: "break_access".into(),
                        until: Some(s + (dur_ms - elapsed_ms)),
                        focus_until,
                    };
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
        } else if self.autolock_on(now, cfg) {
            "autolock"
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

    /// A start right now is a night one (`Config::night_start`: until 05:00 and after the day
    /// end by default). One click there is most likely a miss, so only a confirmed start
    /// (`start_day_at_night`) works.
    pub fn night_start(&self, now: Ts, cfg: &Config) -> bool {
        let n = &cfg.night_start;
        n.enabled && (clock::minute_of_day(now, cfg.tz_offset_min) < n.until_min || (n.after_day_end && self.after_day_end(now, cfg)))
    }

    pub fn start_day(&mut self, now: Ts, cfg: &Config) -> Result<(), String> {
        if self.started_at.is_none() && self.night_start(now, cfg) {
            return Err(tr!("day_its_night_the_day_starts_only"));
        }
        self.start_day_at_night(now, cfg)
    }

    /// The start the user confirmed: works at night too.
    pub fn start_day_at_night(&mut self, now: Ts, cfg: &Config) -> Result<(), String> {
        if self.mode != Mode::Plan || self.started_at.is_some() {
            return Err(tr!("day_the_day_has_already_started"));
        }
        let first = self.first_open_block().ok_or(tr("day_todays_plan_is_empty_add_at"))?;
        self.timing = cfg.timing.clone();
        self.study_day = self.blocks_by_profile(cfg);
        self.started_at = Some(now);
        let leading = self.planned_breaks_before(first);
        if leading.is_empty() {
            self.start_work(now, first);
        } else {
            self.queue_planned(cfg, &leading);
            self.start_queued(now, first);
        }
        let msg = if self.plan_lock(now, cfg) { tr("day_day_started_the_lock_is_on") } else { tr("day_day_started_no_lock") };
        self.log(now, "day_start", msg);
        // The autolock hands over to the day's lock; the wait goes to the log ("задержка старта").
        if let Some(a) = self.autolock.as_mut().filter(|a| a.started_at.is_none()) {
            a.started_at = Some(now);
            let min = worked_min(now - a.at);
            self.log(now, "autolock", tr!("day_the_day_started_min_after_the_autolock", min = min));
        }
        Ok(())
    }

    pub fn pause(&mut self, now: Ts, _cfg: &Config) -> Result<(), String> {
        if self.pause.is_some() {
            return Err(tr!("day_already_paused"));
        }
        let what = self.phase_what();
        match &mut self.phase {
            Phase::Work { elapsed_ms, since, .. } | Phase::Break { elapsed_ms, since, .. } => {
                let s = since.take().ok_or(tr("day_the_timer_isnt_running"))?;
                *elapsed_ms += (now - s).max(0);
            }
            _ => return Err(tr!("day_nothing_to_pause_right_now")),
        }
        self.pause = Some(ActivePause { since: now, what, block: self.current_block(), access: vec![], reminded_at: now });
        self.log(now, "pause", tr!("day_pause"));
        Ok(())
    }

    pub fn resume(&mut self, now: Ts) -> Result<(), String> {
        let p = self.pause.take().ok_or(tr("day_the_timer_isnt_paused"))?;
        match &mut self.phase {
            Phase::Work { since, .. } | Phase::Break { since, .. } => *since = Some(now),
            _ => {}
        }
        let since = p.since;
        if self.close_pause(p, now) {
            self.log(now, "resume", tr("day_resumed"));
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
                self.log(now, "break_skip", tr("day_break_ended_early"));
                n
            }
            Phase::Await { next, .. } => *next,
            Phase::Lunch { next, .. } => {
                let n = *next;
                self.finish_lunch_record(now);
                n
            }
            _ => return Err(tr!("day_the_next_part_cant_be_started")),
        };
        let target = if self.mode == Mode::Plan && (self.is_block_done(next) || next >= self.plan.len()) { self.first_open_block() } else { Some(next) };
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
        let u = self.skip_undo.take().ok_or(tr("day_nothing_to_undo"))?;
        let fresh = matches!(self.phase, Phase::Work { block, since: Some(_), .. } if block == u.block) && self.pause.is_none();
        if now - u.at > SKIP_UNDO_MS || !fresh {
            return Err(tr!("day_too_late_to_undo_the_part"));
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
        self.log(now, "break_skip_undo", tr("day_break_skip_undone_the_break_goes"));
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
                tr!("day_youve_already_had_lunch_today")
            } else {
                tr!("day_lunch_can_only_start_during_a")
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
                tr!("day_lunch_min", lunch_min = self.timing.lunch_min, extra = if at_pc { tr("day_at_the_pc_access_open") } else { "" }),
            );
        } else {
            self.phase = Phase::Lunch { since: now, next };
            self.log(now, "lunch", tr("day_lunch_without_a_timer"));
        }
        Ok(())
    }

    pub fn emergency(&mut self, now: Ts, cfg: &Config, phrase: &str) -> Result<Ts, String> {
        if !self.base_lock(now, cfg) {
            return Err(tr!("day_the_lock_isnt_active_right_now"));
        }
        if self.emergencies.iter().any(|e| now < e.until) {
            return Err(tr!("day_emergency_access_is_already_open"));
        }
        if norm_phrase(phrase) != norm_phrase(&cfg.emergency_phrase) {
            return Err(tr!("day_the_phrase_doesnt_match_type_it"));
        }
        let until = now + cfg.emergency_min as i64 * MIN;
        self.emergencies.push(Emergency { at: now, until, ended_early: false, notified: false });
        let n = self.emergencies.len();
        let text = if self.autolock_on(now, cfg) {
            tr!("day_emergency_access_during_the_autolock", emergency_min = cfg.emergency_min, n = n)
        } else {
            tr!("day_emergency_access_for_min", emergency_min = cfg.emergency_min, n = n)
        };
        self.log(now, "emergency", text);
        Ok(until)
    }

    /// Block from now for `minutes`, any day. A running focus lock only moves later: a shorter
    /// call is refused.
    pub fn start_focus(&mut self, now: Ts, cfg: &Config, minutes: u32, reason: Option<&str>, by: &str) -> Result<FocusChange, String> {
        if !(1..=MAX_FOCUS_MIN).contains(&minutes) {
            return Err(tr!("day_a_focus_lock_lasts_1_to", MAX_FOCUS_MIN = MAX_FOCUS_MIN));
        }
        let until = now + minutes as i64 * MIN;
        let hm = |ts: Ts| clock::hm(ts, cfg.tz_offset_min);
        let reason = reason.map(|r| r.trim().chars().take(200).collect::<String>()).filter(|r| !r.is_empty());
        let note = reason.as_ref().map(|r| format!(" ({r})")).unwrap_or_default();
        let from = self.focus_until(now);
        match from {
            Some(cur) if until <= cur => {
                return Err(tr!(
                    "day_a_focus_lock_already_runs_until",
                    cur = hm(cur),
                    cur2 = (cur - now + MIN - 1) / MIN,
                    cur3 = (cur - now) / MIN,
                ));
            }
            Some(cur) => {
                if let Some(f) = self.focus.iter_mut().rev().find(|f| f.until == cur) {
                    f.until = until;
                    if reason.is_some() {
                        f.reason = reason;
                    }
                }
                self.log(now, "focus", tr!("day_focus_lock_extended", cur = hm(cur), until = hm(until), note = note));
            }
            None => {
                self.focus.push(FocusLock { at: now, until, by: by.into(), reason, notified: false });
                self.log(now, "focus", tr!("day_focus_lock_until_min", until = hm(until), minutes = minutes, note = note));
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
        if any {
            self.log(now, "access_end", tr("day_access_closed_manually"));
            Ok(())
        } else {
            Err(tr!("day_access_is_already_closed"))
        }
    }

    pub fn start_single(&mut self, now: Ts, cfg: SingleCfg) -> Result<(), String> {
        if self.mode == Mode::Single {
            return Err(tr!("day_the_single_timer_is_already_running"));
        }
        if !matches!(self.phase, Phase::Idle | Phase::Done) {
            return Err(tr!("day_finish_the_days_plan_first_the"));
        }
        let cfg = SingleCfg { work_min: cfg.work_min.clamp(1, 240), break_min: cfg.break_min.min(120), block: cfg.block };
        self.log(
            now,
            "single_start",
            tr!("day_single_timer_min", work_min = cfg.work_min, extra = if cfg.break_min > 0 { tr!("day_plus_break", min = cfg.break_min) } else { String::new() }),
        );
        self.singles.push(SingleRun { cfg, started_at: now, ended_at: None, work_ms: 0, rounds: 0 });
        self.mode = Mode::Single;
        self.start_work(now, 0);
        Ok(())
    }

    pub fn stop_single(&mut self, now: Ts) -> Result<(), String> {
        if self.mode != Mode::Single {
            return Err(tr!("day_the_single_timer_isnt_running"));
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
        self.log(now, "single_stop", tr("day_single_timer_stopped"));
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
        let name = self.plan.get(block).map(|b| b.name.clone()).ok_or(tr("day_no_such_block"))?;
        let p = &mut self.progress[block];
        if p.started_at.is_none() && p.work_ms == 0 {
            return Err(tr!("day_hasnt_started_yet", name = name));
        }
        let note: Option<String> = note.map(|n| n.trim().chars().take(300).collect::<String>()).filter(|n| !n.is_empty());
        match &note {
            Some(n) => {
                p.note = Some(n.clone());
                p.note_skipped = false;
                let by = if by == "mcp" { tr("day_agent") } else { "" };
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
            return Err(tr!("day_a_block_can_only_be_closed"));
        }
        let i = match name.map(str::trim).filter(|n| !n.is_empty()) {
            // Two blocks of one subject: the open started one is meant.
            Some(n) => {
                let named: Vec<usize> = (0..self.plan.len()).filter(|&i| self.is_study(i) && same_name(&self.plan[i].name, n)).collect();
                *named.iter().find(|&&i| self.check_closable(i).is_ok()).or(named.first()).ok_or(tr!("day_theres_no_block_in_the_plan", n = n))?
            }
            None => self.current_block().ok_or(tr("day_theres_no_current_block_right_now"))?,
        };
        self.check_closable(i)?;
        Ok(i)
    }

    /// A started study block that is still open.
    fn check_closable(&self, i: usize) -> Result<(), String> {
        let Some(b) = self.plan.get(i) else {
            return Err(tr!("day_theres_no_current_block_right_now"));
        };
        let name = &b.name;
        if b.is_break() {
            return Err(tr!("day_is_a_segment_not_a_study", name = name));
        }
        if self.is_block_done(i) {
            return Err(tr!("day_is_already_closed", name = name));
        }
        let p = &self.progress[i];
        if p.started_at.is_none() && p.work_ms == 0 {
            return Err(tr!("day_hasnt_started_if_you_dont_need", name = name));
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
            return Err(tr!(
                "day_less_than_a_minute_worked_on",
                self_v = self.plan[i].name,
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
            tr!(
                "day_block_closed_early_of_min_plan",
                worked = fmt_min(worked),
                extra = if by == "mcp" { tr("day_agent") } else { "" },
                name = name,
                from_min = from_min,
                to_min = to_min,
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
            return Err(tr!("day_thats_not_later_than_the_current"));
        }
        self.move_day_end(now, cfg, new_end, None, "ui").map(|_| ())
    }

    /// Shift today's day end once ("HH:MM", Moscow). "00:00".."02:00" mean the night after.
    /// The config template stays untouched. Returns the logged change (`from == to`: nothing changed).
    pub fn set_day_end(&mut self, now: Ts, cfg: &Config, hm: &str, reason: Option<&str>, by: &str) -> Result<DayEndChange, String> {
        let typed = clock::parse_hm(hm).ok_or(tr("day_time_must_be_hh_mm_e"))?;
        let to = if typed <= NEXT_DAY_UNTIL_MIN { typed + 24 * 60 } else { typed };
        self.move_day_end(now, cfg, to, reason, by)
    }

    /// Move today's day end to `to` minutes after the day's midnight (up to 02:00 of the night
    /// after), later or earlier, as long as it is still ahead of `now`.
    pub fn move_day_end(&mut self, now: Ts, cfg: &Config, to: u32, reason: Option<&str>, by: &str) -> Result<DayEndChange, String> {
        let to_ts = clock::date_minute(self.date, cfg.tz_offset_min, to);
        if to > MAX_DAY_END_MIN {
            return Err(tr!("day_the_day_end_cant_move_past"));
        }
        if to_ts <= now {
            return Err(tr!(
                "day_has_already_passed_the_day_end",
                to = fmt_day_min(to),
                clock = clock::hm(now, cfg.tz_offset_min),
            ));
        }
        let reason = reason.map(|r| r.trim().chars().take(200).collect::<String>()).filter(|r| !r.is_empty());
        let from = self.day_end(cfg);
        // The autolock holds until the day end: an earlier end would lift it.
        if to < from && self.autolock_on(now, cfg) {
            return Err(tr!("day_during_the_autolock_the_day_end"));
        }
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
            tr!(
                "day_day_end",
                from = fmt_day_min(from),
                to = fmt_day_min(to),
                map = change.reason.as_ref().map(|r| format!(" ({r})")).unwrap_or_default(),
                extra = if by == "mcp" { tr("day_agent") } else { "" },
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
        // The autolock is a lock too: no lighter day to slip out of it.
        let locked = locked || self.autolock_on(now, cfg);
        if locked && kind < self.kind {
            return Err(tr!("day_during_the_study_day_the_day", kind = kind.label()));
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
        self.log(now, "kind", tr!("day_day", from = from.label(), kind = kind.label()));
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
            return Err(tr!("day_no_more_than_plan_items_per", MAX_PLAN_BLOCKS = MAX_PLAN_BLOCKS));
        }
        if plan.iter().any(|b| b.minutes == 0) {
            return Err(tr!("day_every_block_needs_a_duration"));
        }
        let started = self.started_at.is_some();
        if started && !plan.iter().any(|b| !b.is_break()) {
            return Err(tr!("day_a_started_days_plan_cant_be"));
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
                    return Err(tr!(
                        "day_has_started_min_worked_it_cant",
                        name = old.name,
                        worked = worked_min(worked),
                    ));
                }
                Some(j) if locked && touched(&self.progress[i]) && !old.is_break() && plan[j].minutes < worked_min(worked) => {
                    return Err(tr!(
                        "day_min_already_worked_it_cant_be",
                        name = old.name,
                        worked = worked_min(worked),
                        worked2 = worked_min(worked),
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
                // No open block left: the old index may point past the new plan, so it goes too.
                if let Phase::Break { next, .. } | Phase::Await { next, .. } | Phase::Lunch { next, .. } | Phase::Segment { next, .. } = &mut self.phase {
                    *next = open.unwrap_or(0);
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
                            self.log(now, "day_done", tr("day_all_blocks_of_the_day_are"));
                        }
                    }
                    _ => {}
                }
            }
        }
        let summary = self.plan.iter().map(|b| tr!("day_name_minutes", name = b.name, minutes = b.minutes)).collect::<Vec<_>>().join(" · ");
        let diff = changes.iter().map(fmt_change).collect::<Vec<_>>().join(", ");
        self.log(now, "plan", if diff.is_empty() { tr!("day_plan", summary = summary) } else { tr!("day_log_plan_with_changes", summary = summary, diff = diff) });
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
                return Err(tr!("day_close_closes_a_study_block_and", name = b.name));
            }
            let i = (0..self.plan.len())
                .find(|&i| !taken.contains(&i) && self.is_study(i) && same_name(&self.plan[i].name, &b.name) && self.check_closable(i).is_ok())
                .ok_or(tr!("day_close_closes_a_started_not_yet", b = b.name.trim()))?;
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
            let res = found.ok_or(tr!("day_is_already_closed", name = name)).and_then(|i| self.finish_block_at(now, cfg, i, by));
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
        let mut ev = std::mem::take(&mut self.pending_events);
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
        ev.extend(self.autolock_tick(now, cfg));
        if !self.day_end_notified && self.started_at.is_some() && self.completed_at.is_none() && self.study_day && self.after_day_end(now, cfg) {
            self.day_end_notified = true;
            let text = match self.focus_until(now) {
                Some(u) => tr!("day_the_study_day_is_over_focus", cfg = fmt_day_min(self.day_end(cfg)), clock = clock::hm(u, cfg.tz_offset_min)),
                None => tr!("day_the_lock_is_off_for_the", cfg = fmt_day_min(self.day_end(cfg))),
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
                (tr!("day_single_timer"), r + 1, 0)
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
                    self.log(end, "lunch_end", tr("day_lunch_over"));
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
                self.log(end, "work_end", tr!("day_single_timer_round_done", round = round));
                ev.push(Event::WorkEnded { block_name: tr!("day_single_timer"), part: round, parts: 0, block_done: false });
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
                self.log(end, "work_end", tr!("day_part_of_done", name = name, part = part, parts = parts));
                ev.push(Event::WorkEnded { block_name: name.clone(), part, parts, block_done });
                if block_done {
                    let (pauses, pause_ms) = self
                        .pauses
                        .iter()
                        .filter(|r| r.block == Some(block))
                        .fold((0u32, 0i64), |(n, ms), r| (n + 1, ms + (r.end - r.start)));
                    self.log(end, "block_done", tr!("day_block_closed_min_of_work", work_ms = work_ms / MIN, name = name));
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
    /// study | single | autolock | focus | emergency | break_access | lunch_at_pc |
    /// segment_access | not_study_day | not_started | completed | day_end
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
        let mut c = Config::study_week();
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
    fn break_access_opens_only_the_break_between_blocks() {
        let mut c = cfg();
        c.break_access = true;
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        // A pause opens nothing.
        d.pause(start() + 5 * MIN, &c).unwrap();
        assert_eq!(d.lock_state(start() + 6 * MIN, &c).reason, "study");
        d.resume(start() + 10 * MIN).unwrap();
        // Neither does the short break between the parts of one block.
        d.tick(start() + 50 * MIN, &c);
        assert!(matches!(d.phase, Phase::Break { brk: BreakKind::Short, .. }));
        assert!(d.lock_state(start() + 51 * MIN, &c).blocked);
        d.tick(start() + 60 * MIN, &c);
        d.start_next(start() + 60 * MIN).unwrap();
        // The block is done: the break between blocks is open until its timer runs out.
        d.tick(start() + 105 * MIN, &c);
        assert!(matches!(d.phase, Phase::Break { brk: BreakKind::Between, .. }));
        let l = d.lock_state(start() + 106 * MIN, &c);
        assert!(!l.blocked);
        assert_eq!(l.reason, "break_access");
        assert_eq!(l.until, Some(start() + 125 * MIN));
        // A paused break is blocked, and the access goes on after resume.
        d.pause(start() + 110 * MIN, &c).unwrap();
        assert!(d.lock_state(start() + 111 * MIN, &c).blocked);
        d.resume(start() + 112 * MIN).unwrap();
        assert_eq!(d.lock_state(start() + 113 * MIN, &c).until, Some(start() + 127 * MIN));
        // The break is over: waiting for "start" is blocked again.
        d.tick(start() + 127 * MIN, &c);
        assert!(matches!(d.phase, Phase::Await { .. }));
        assert!(d.lock_state(start() + 128 * MIN, &c).blocked);

        // Turned off: the break between blocks stays blocked.
        c.break_access = false;
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.tick(start() + 45 * MIN, &c);
        d.tick(start() + 55 * MIN, &c);
        d.start_next(start() + 55 * MIN).unwrap();
        d.tick(start() + 100 * MIN, &c);
        assert!(matches!(d.phase, Phase::Break { brk: BreakKind::Between, .. }));
        assert!(d.lock_state(start() + 101 * MIN, &c).blocked);
    }

    #[test]
    fn night_start_needs_a_confirmation() {
        let c = cfg();
        // 00:05 MSK: one click does not start the day, a confirmed start does.
        let night = t("2026-09-28T21:05:00Z");
        let mut d = DayState::new(night, &c);
        assert!(d.night_start(night, &c));
        assert!(d.start_day(night, &c).is_err());
        assert!(d.started_at.is_none());
        d.start_day_at_night(night, &c).unwrap();
        assert!(d.started_at.is_some());
        // 05:00 is morning; after the day end (22:00) it is night again.
        let d = DayState::new(start(), &c);
        assert!(!d.night_start(t("2026-09-28T02:00:00Z"), &c));
        assert!(!d.night_start(start(), &c));
        assert!(d.night_start(t("2026-09-28T19:00:00Z"), &c));
        // The settings: night until 07:00, not after the day end; or off altogether.
        let mut c = cfg();
        c.night_start.until_min = 7 * 60;
        c.night_start.after_day_end = false;
        assert!(d.night_start(t("2026-09-28T03:30:00Z"), &c));
        assert!(!d.night_start(t("2026-09-28T19:00:00Z"), &c));
        c.night_start.enabled = false;
        assert!(!d.night_start(night, &c));
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
        let ev = r.events.last().unwrap();
        assert_eq!(ev.kind, "restore");
        assert!(ev.text.contains("40 мин"), "{}", ev.text);
    }

    #[test]
    fn single_timer() {
        let mut c = cfg();
        // 13:00 on a day not started: the autolock would hold the list (see `autolock_tests`).
        c.profiles.full.autolock = false;
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
        c.break_access = true;
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 45), PlanBlock::new("Экстернат", 150)];
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.tick(start() + 45 * MIN, &c);
        let p = start() + 46 * MIN;
        assert_eq!(d.lock_state(p, &c).reason, "break_access");
        d.start_focus(p, &c, 60, None, "mcp").unwrap();
        // Break access does not open a focus lock.
        let l = d.lock_state(p + MIN, &c);
        assert!(l.blocked);
        assert_eq!(l.reason, "study");
        assert_eq!(l.focus_until, Some(p + 60 * MIN));

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
        let mut c = Config::study_week();
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

#[cfg(test)]
mod autolock_tests {
    use super::*;
    use chrono::DateTime;

    fn t(s: &str) -> Ts {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }

    // Monday 2026-09-28: 13:00 at UTC+3 is 10:00Z.
    fn at(hm: &str) -> Ts {
        t(&format!("2026-09-28T{hm}:00+03:00"))
    }

    fn cfg() -> Config {
        let mut c = Config::study_week();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 90), PlanBlock::new("Экстернат", 150)];
        c
    }

    fn autolock_events(ev: &[Event]) -> Vec<&Event> {
        ev.iter().filter(|e| matches!(e, Event::AutoLockSoon { .. } | Event::AutoLockOn { .. })).collect()
    }

    #[test]
    fn a_start_before_its_time_leaves_no_trace() {
        let c = cfg();
        let mut d = DayState::new(at("09:00"), &c);
        assert!(!d.lock_state(at("09:00"), &c).blocked);
        assert_eq!(crate::view::build(&d, &c, at("09:00")).autolock.unwrap().time, "13:00");
        d.start_day(at("12:10"), &c).unwrap();
        for hm in ["12:55", "13:00", "14:00"] {
            assert!(autolock_events(&d.tick(at(hm), &c)).is_empty(), "{hm}");
        }
        assert!(d.autolock.is_none());
        assert_eq!(d.lock_state(at("13:30"), &c).reason, "study");
        assert!(!d.events.iter().any(|e| e.kind == "autolock"));
        assert!(crate::stats::day_stats(&d, c.tz_offset_min, at("14:00")).autolock.is_none());
        assert!(crate::view::build(&d, &c, at("13:30")).autolock.is_none());
    }

    #[test]
    fn a_late_start_holds_the_lock_and_logs_the_delay() {
        let c = cfg();
        let mut d = DayState::new(at("09:00"), &c);
        assert!(autolock_events(&d.tick(at("12:54"), &c)).is_empty());
        // Five minutes before: one warning.
        let ev = d.tick(at("12:55"), &c);
        assert_eq!(autolock_events(&ev), [&Event::AutoLockSoon { left_ms: 5 * MIN, at: at("13:00") }]);
        assert!(autolock_events(&d.tick(at("12:57"), &c)).is_empty());
        assert!(!d.lock_state(at("12:59"), &c).blocked);

        let ev = d.tick(at("13:00"), &c);
        assert_eq!(autolock_events(&ev), [&Event::AutoLockOn { at: at("13:00") }]);
        assert!(autolock_events(&d.tick(at("13:01"), &c)).is_empty());
        let l = d.lock_state(at("13:20"), &c);
        assert!(l.blocked && l.base);
        assert_eq!((l.reason.as_str(), l.until), ("autolock", None));
        let log = d.events.iter().find(|e| e.kind == "autolock").unwrap();
        assert_eq!(log.ts, at("13:00"));
        assert!(log.text.contains("13:00"), "{}", log.text);

        // The lock rules hold: the list only grows, no lighter day to slip out.
        let mut fewer = c.clone();
        fewer.blocklist.sites.clear();
        assert!(c.check_update(&fewer, crate::config::EditContext { locked: d.base_lock(at("13:20"), &c) }).is_err());
        assert!(d.set_kind(at("13:20"), &c, DayKind::Off, false).is_err());
        assert!(!crate::view::build(&d, &c, at("13:20")).can.lighter_kind);
        // The plan of a day not started can still be reshaped.
        d.set_plan(at("13:25"), vec![PlanBlock::new("Математика", 45)], false).unwrap();

        // "Начать день" 47 minutes later: the day's own lock takes over, the delay is logged.
        d.start_day(at("13:47"), &c).unwrap();
        assert_eq!(d.lock_state(at("13:48"), &c).reason, "study");
        assert!(!d.autolock_on(at("13:48"), &c));
        let log = d.events.iter().rev().find(|e| e.kind == "autolock").unwrap();
        assert!(log.text.contains("47 мин"), "{}", log.text);
        let v = crate::view::build(&d, &c, at("13:48")).autolock.unwrap();
        assert_eq!((v.on, v.start_delay_min, v.at), (false, Some(47), at("13:00")));
        let s = crate::stats::day_stats(&d, c.tz_offset_min, at("13:48")).autolock.unwrap();
        assert_eq!((s.time.as_str(), s.start_delay_min), ("13:00", Some(47)));

        // Once the plan is done, the lock is off as on any day.
        d.tick(at("14:32"), &c);
        assert!(d.completed_at.is_some());
        assert!(!d.lock_state(at("14:33"), &c).base);
    }

    #[test]
    fn never_on_a_day_off_or_when_off() {
        let c = cfg();
        let sunday = t("2026-10-04T13:30:00+03:00");
        let mut d = DayState::new(t("2026-10-04T09:00:00+03:00"), &c);
        d.set_plan(sunday, vec![PlanBlock::new("Чтение", 30)], false).unwrap();
        assert_eq!(d.kind, DayKind::Off);
        assert!(autolock_events(&d.tick(sunday, &c)).is_empty());
        assert!(!d.lock_state(sunday, &c).base);
        assert!(crate::view::build(&d, &c, sunday).autolock.is_none());

        // Turned off for the profile; or a profile without the lock at all.
        let offs: [fn(&mut Config); 2] = [|c| c.profiles.full.autolock = false, |c| c.profiles.full.block = false];
        for off in offs {
            let mut c = cfg();
            off(&mut c);
            let mut d = DayState::new(at("09:00"), &c);
            assert!(autolock_events(&d.tick(at("12:55"), &c)).is_empty());
            assert!(autolock_events(&d.tick(at("13:00"), &c)).is_empty());
            assert!(!d.lock_state(at("13:30"), &c).blocked);
        }

        // An empty plan has nothing to start: no lock to wait in.
        let mut d = DayState::new(at("09:00"), &cfg());
        d.plan.clear();
        d.progress.clear();
        assert!(autolock_events(&d.tick(at("13:00"), &cfg())).is_empty());
    }

    #[test]
    fn a_restart_or_a_settings_change_keeps_it() {
        let c = cfg();
        let mut d = DayState::new(at("09:00"), &c);
        d.tick(at("13:00"), &c);
        d.saved_at = at("13:00");
        // Power cut: the saved day comes back with the lock.
        let json = serde_json::to_string(&d).unwrap();
        let mut r: DayState = serde_json::from_str(&json).unwrap();
        r.restore(at("13:40"), &c);
        assert!(autolock_events(&r.tick(at("13:40"), &c)).is_empty());
        assert_eq!(r.lock_state(at("13:40"), &c).reason, "autolock");
        // Turning it off (or the lock of the profile) only shapes the days ahead.
        let mut off = c.clone();
        off.profiles.full.autolock = false;
        off.profiles.full.block = false;
        r.restore(at("13:41"), &off);
        assert!(r.lock_state(at("13:41"), &off).blocked);
        // ...and the day started after that still locks.
        r.start_day(at("13:42"), &off).unwrap();
        assert_eq!(r.lock_state(at("13:42"), &off).reason, "study");
        r.restore(at("13:43"), &off);
        assert_eq!(r.lock_state(at("13:43"), &off).reason, "study");

        // The app was not running at 13:00: on as soon as it is, and the log says when.
        let mut d = DayState::new(at("09:00"), &c);
        d.tick(at("12:00"), &c);
        let mut r: DayState = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        r.restore(at("15:10"), &c);
        assert_eq!(autolock_events(&r.tick(at("15:10"), &c)), [&Event::AutoLockOn { at: at("15:10") }]);
        let log = r.events.iter().find(|e| e.kind == "autolock").unwrap();
        assert!(log.text.contains("13:00") && log.text.contains("15:10"), "{}", log.text);

        // Too late: after the day end it never goes on.
        let mut d = DayState::new(at("09:00"), &c);
        assert!(autolock_events(&d.tick(at("22:30"), &c)).is_empty());
        assert!(!d.lock_state(at("22:30"), &c).blocked);

        // Days saved before the autolock existed still load.
        let mut v = serde_json::to_value(&r).unwrap();
        v.as_object_mut().unwrap().remove("autolock");
        v.as_object_mut().unwrap().remove("autolock_warned_for");
        let old: DayState = serde_json::from_value(v).unwrap();
        assert!(old.autolock.is_none());
    }

    #[test]
    fn only_emergency_access_opens_it_and_the_day_end_lifts_it() {
        let c = cfg();
        let mut d = DayState::new(at("09:00"), &c);
        d.tick(at("13:00"), &c);
        let until = d.emergency(at("13:30"), &c, &c.emergency_phrase).unwrap();
        let l = d.lock_state(at("13:31"), &c);
        assert!(!l.blocked && l.base);
        assert_eq!(l.reason, "emergency");
        let log = d.events.iter().rev().find(|e| e.kind == "emergency").unwrap();
        assert!(log.text.contains("автоблокировк"), "{}", log.text);
        assert!(d.tick(until, &c).contains(&Event::EmergencyEnded));
        assert_eq!(d.lock_state(until, &c).reason, "autolock");
        // The phone holds it too, offline as well.
        let p = crate::phone::snapshot(&d, &c, until);
        assert!(p.lock.blocked && p.lock.day_lock);
        // An earlier day end would lift it too: only a later one.
        assert!(d.set_day_end(until, &c, "13:50", None, "mcp").is_err());
        assert!(d.clone().set_day_end(until, &c, "23:00", None, "mcp").is_ok());

        // Never started: the day end lifts it.
        let ten_pm = at("22:00");
        assert!(d.lock_state(ten_pm - 1, &c).blocked);
        let l = d.lock_state(ten_pm, &c);
        assert!(!l.base);
        assert_eq!(l.reason, "not_started");
    }

    #[test]
    fn a_new_time_gets_its_own_warning() {
        let mut c = cfg();
        let mut d = DayState::new(at("09:00"), &c);
        assert!(!autolock_events(&d.tick(at("12:56"), &c)).is_empty());
        c.profiles.full.autolock_min = 14 * 60;
        assert!(autolock_events(&d.tick(at("13:00"), &c)).is_empty());
        assert_eq!(autolock_events(&d.tick(at("13:55"), &c)), [&Event::AutoLockSoon { left_ms: 5 * MIN, at: at("14:00") }]);
        assert!(autolock_events(&d.tick(at("13:56"), &c)).is_empty());
    }
}
