//! Non-study segments: lunch, a nap, a walk… A countdown on the wall clock, then overrun until
//! the user ends it ("Закончил" / "Встал"). Several can be queued; planned ones (`type: "break"`
//! plan items) start by themselves when the block before them closes.

use crate::clock::{Ts, MIN};
use crate::config::Config;
use crate::day::{fmt_min, same_name, BreakKind, DayState, Event, Mode, Phase, QueuedSegment, SegmentRecord};

/// Warn this long before the planned end (not for alarm segments).
const WARN_MS: i64 = 5 * MIN;
/// Repeat the "over time" notice this often.
const OVERRUN_REPEAT_MS: i64 = 5 * MIN;
/// Ask "что сейчас?" this long after a block closed with nothing started since.
pub const WHAT_NOW_MS: i64 = 10 * MIN;
/// A stopwatch segment ("без времени") nudges this long after its usual length, then as often.
const STOPWATCH_NUDGE_MS: i64 = 30 * MIN;

impl DayState {
    /// A segment can start in a break or while waiting for the next part, and more can be queued
    /// while one runs.
    pub fn can_start_segment(&self) -> bool {
        self.mode == Mode::Plan
            && self.started_at.is_some()
            && self.completed_at.is_none()
            && matches!(
                self.phase,
                Phase::Break { brk: BreakKind::Short | BreakKind::Between, .. } | Phase::Await { .. } | Phase::Segment { .. }
            )
    }

    /// The running segment.
    pub fn segment(&self) -> Option<&SegmentRecord> {
        match self.phase {
            Phase::Segment { rec, .. } => self.segments.get(rec),
            _ => None,
        }
    }

    /// Start segments now (from a break / waiting) or queue them behind the running one.
    pub fn start_segments(&mut self, now: Ts, items: Vec<QueuedSegment>) -> Result<(), String> {
        if items.is_empty() {
            return Err("Выбери хотя бы один отрезок.".into());
        }
        if !self.can_start_segment() {
            return Err("Отрезок ставится в перерыве или между блоками.".into());
        }
        if items.iter().any(|q| q.name.trim().is_empty() || q.minutes == 0) {
            return Err("У отрезка должны быть название и длительность.".into());
        }
        let mut items = items;
        self.take_planned(now, &mut items);
        if matches!(self.phase, Phase::Segment { .. }) {
            let names = items.iter().map(|q| format!("{} {} мин", q.name, q.minutes)).collect::<Vec<_>>().join(" → ");
            self.segment_queue.extend(items);
            self.log(now, "segment_queue", format!("В очередь: {names}"));
            return Ok(());
        }
        if let Some(p) = self.pause.take() {
            self.close_pause_pub(p, now);
        }
        self.skip_undo = None;
        let next = self.current_block().or_else(|| self.first_open_block()).unwrap_or(0);
        self.segment_queue = items;
        self.start_queued(now, next);
        Ok(())
    }

    /// A segment picked by hand takes the place of the nearest planned segment of the same type
    /// that has not run yet: that one counts as taken and does not start again later. A second
    /// lunch only happens when the plan has a second one (or there is none left to take).
    fn take_planned(&mut self, now: Ts, items: &mut [QueuedSegment]) {
        for k in 0..items.len() {
            if items[k].plan_item.is_some() {
                continue;
            }
            let picked: Vec<usize> = items[..k].iter().filter_map(|q| q.plan_item).collect();
            let found = (0..self.plan.len()).find(|&i| self.is_open_break(i) && !picked.contains(&i) && same_name(&self.plan[i].name, &items[k].name));
            if let Some(i) = found {
                items[k].plan_item = Some(i);
                items[k].consumed = true;
                let name = self.plan[i].name.clone();
                self.log(now, "segment_plan", format!("«{name}» взят раньше плана — запланированный не запустится второй раз"));
            }
        }
    }

    /// Queue planned segments (plan items) in their order; they count as taken from now on.
    pub fn queue_planned(&mut self, cfg: &Config, items: &[usize]) {
        for &i in items {
            let b = &self.plan[i];
            let mut q = QueuedSegment::of(cfg, &b.name, Some(b.minutes));
            q.name = b.name.clone();
            q.plan_item = Some(i);
            self.segment_queue.push(q);
        }
    }

    /// Start the first queued segment. Returns false when the queue is empty.
    pub fn start_queued(&mut self, now: Ts, next: usize) -> bool {
        if self.segment_queue.is_empty() {
            return false;
        }
        let q = self.segment_queue.remove(0);
        if let Some(i) = q.plan_item {
            if let Some(p) = self.progress.get_mut(i) {
                p.started_at = Some(now);
            }
        }
        let how = if q.open_ended {
            " — без времени, секундомер".to_string()
        } else if q.prep_min > 0 {
            format!(" — сначала подготовка {} мин, отсчёт{} от «Лёг»", q.prep_min, if q.alarm { " и будильник" } else { "" })
        } else if q.alarm {
            " — будильник в конце".to_string()
        } else {
            String::new()
        };
        self.log(now, "segment", format!("{} {} мин{how}", q.name, q.minutes));
        self.segments.push(SegmentRecord {
            name: q.name,
            planned_min: q.minutes,
            alarm: q.alarm,
            open_access: q.open_access,
            start: now,
            end: None,
            plan_item: q.plan_item,
            warned: false,
            ended_notified: false,
            reminded_at: now,
            prep_min: q.prep_min,
            lay_at: None,
            open_ended: q.open_ended,
            prep_notified: false,
        });
        self.phase = Phase::Segment { rec: self.segments.len() - 1, next };
        true
    }

    /// "Закончил" / "Встал": end the running segment; the next queued one starts at once,
    /// otherwise the timer waits for the next part (or the day is over).
    pub fn end_segment(&mut self, now: Ts) -> Result<(), String> {
        let Phase::Segment { rec, next } = self.phase else {
            return Err("Сейчас не идёт отрезок.".into());
        };
        let r = &mut self.segments[rec];
        let never_lay = r.in_prep();
        r.end = Some(now);
        let (name, planned, actual, prep) = (r.name.clone(), r.planned_ms(), r.actual_ms(now), r.prep_ms(now));
        let had_prep = r.prep_min > 0;
        if let Some(i) = r.plan_item {
            if let Some(p) = self.progress.get_mut(i) {
                p.completed_at = Some(now);
            }
        }
        let over = actual - planned;
        let prep = if had_prep { format!(", подготовка {} мин", fmt_min(prep)) } else { String::new() };
        let text = if never_lay {
            format!("{name} отменён на подготовке{prep}")
        } else {
            format!(
                "{name} окончен: {} мин{}{prep}",
                fmt_min(actual),
                if over >= MIN { format!(" (превышение {} мин)", over / MIN) } else { String::new() }
            )
        };
        self.log(now, "segment_end", text);
        if self.start_queued(now, next) {
            return Ok(());
        }
        match self.first_open_block() {
            Some(n) => self.phase = Phase::Await { next: n, since: now, reminded_at: now },
            None => {
                self.phase = Phase::Done;
                if self.completed_at.is_none() {
                    self.completed_at = Some(now);
                    self.log(now, "day_done", "Все блоки дня закрыты — блокировка снята");
                }
            }
        }
        Ok(())
    }

    /// Take a segment out of the queue.
    pub fn drop_queued(&mut self, now: Ts, index: usize) -> Result<(), String> {
        if index >= self.segment_queue.len() {
            return Err("В очереди нет такого отрезка.".into());
        }
        let q = self.segment_queue.remove(index);
        // A planned one dropped from the queue is skipped for today, not moved to a later spot.
        // One picked by hand only gives its planned place back.
        if !q.consumed {
            if let Some(p) = q.plan_item.and_then(|i| self.progress.get_mut(i)) {
                p.completed_at = Some(now);
            }
        }
        self.log(now, "segment_queue", format!("Из очереди убран: {}", q.name));
        Ok(())
    }

    /// "Лёг": the preparation is over, the countdown (and the alarm) starts now.
    pub fn lay_down(&mut self, now: Ts) -> Result<(), String> {
        let Phase::Segment { rec, .. } = self.phase else {
            return Err("Сейчас не идёт отрезок.".into());
        };
        let r = &mut self.segments[rec];
        if !r.in_prep() {
            return Err("Отсчёт уже идёт.".into());
        }
        r.lay_at = Some(now);
        r.warned = false;
        r.ended_notified = false;
        r.reminded_at = now;
        let text = format!("{}: лёг после {} мин подготовки — {} через {} мин", r.name, fmt_min(now - r.start), if r.alarm { "будильник" } else { "конец" }, r.planned_min);
        self.log(now, "segment_lay", text);
        Ok(())
    }

    /// Switch the running segment between a countdown and a stopwatch ("без времени"). A planned
    /// segment starts by itself with a countdown; this is how it goes without one.
    pub fn set_segment_mode(&mut self, now: Ts, open_ended: bool) -> Result<(), String> {
        let Phase::Segment { rec, .. } = self.phase else {
            return Err("Сейчас не идёт отрезок.".into());
        };
        let r = &mut self.segments[rec];
        if r.in_prep() {
            return Err("Сначала «Лёг» — на подготовке отсчёта ещё нет.".into());
        }
        if r.alarm && open_ended {
            return Err(format!("У «{}» в конце будильник — без времени его не поставить.", r.name));
        }
        if r.open_ended == open_ended {
            return Ok(());
        }
        r.open_ended = open_ended;
        // Back to a countdown: notices go from here (an end already passed is said once).
        r.warned = now >= r.planned_end() - WARN_MS;
        r.ended_notified = false;
        r.reminded_at = now;
        let name = r.name.clone();
        self.log(now, "segment_mode", format!("{name}: {}", if open_ended { "без времени, секундомер" } else { "снова по таймеру" }));
        Ok(())
    }

    /// What comes after a closed block: planned segments standing before the next block, the
    /// break between blocks, or the end of the day.
    pub fn after_block(&mut self, now: Ts, cfg: &Config, ev: &mut Vec<Event>) {
        match self.first_open_block() {
            Some(next) => {
                let planned = self.planned_breaks_before(next);
                if planned.is_empty() {
                    let dur_ms = self.timing.between_blocks_min as i64 * MIN;
                    self.phase = Phase::Break { brk: BreakKind::Between, dur_ms, elapsed_ms: 0, since: Some(now), next };
                } else {
                    self.queue_planned(cfg, &planned);
                    self.start_queued(now, next);
                }
            }
            None => {
                self.phase = Phase::Done;
                self.completed_at = Some(now);
                self.segment_queue.clear();
                let total: i64 = self.progress.iter().map(|p| p.work_ms).sum();
                self.log(now, "day_done", "Все блоки дня закрыты — блокировка снята");
                ev.push(Event::DayCompleted { work_ms: total });
            }
        }
    }

    /// Warning / end / overrun notices of the running segment. Overrun reminders keep quiet
    /// after the day end, the nap alarm never does.
    pub(crate) fn segment_events(&mut self, now: Ts, quiet: bool) -> Vec<Event> {
        let mut ev = vec![];
        let Phase::Segment { rec, .. } = self.phase else { return ev };
        let Some(r) = self.segments.get_mut(rec) else { return ev };
        if r.in_prep() {
            // Getting ready: only "пора ложиться", never the countdown or the alarm by itself.
            let end = r.prep_end();
            if now >= end && (!r.prep_notified || (now - r.reminded_at >= OVERRUN_REPEAT_MS && !quiet)) {
                r.prep_notified = true;
                r.reminded_at = now;
                ev.push(Event::SegmentPrepOver { name: r.name.clone(), over_ms: now - end });
            }
            return ev;
        }
        if r.open_ended {
            // A stopwatch: no countdown notices, only a quiet nudge well past the usual length.
            let late = r.planned_end() + STOPWATCH_NUDGE_MS;
            if now >= late && now - r.reminded_at >= STOPWATCH_NUDGE_MS && !quiet {
                r.reminded_at = now;
                ev.push(Event::SegmentLong { name: r.name.clone(), elapsed_ms: now - r.start });
            }
            return ev;
        }
        let end = r.planned_end();
        if !r.alarm && !r.warned && r.planned_ms() > WARN_MS && now >= end - WARN_MS && now < end {
            r.warned = true;
            ev.push(Event::SegmentWarning { name: r.name.clone(), left_ms: end - now });
        }
        if now >= end {
            if !r.ended_notified {
                r.ended_notified = true;
                r.warned = true;
                r.reminded_at = now;
                ev.push(Event::SegmentEnded { name: r.name.clone(), alarm: r.alarm });
            } else if now - r.reminded_at >= OVERRUN_REPEAT_MS && (r.alarm || !quiet) {
                r.reminded_at = now;
                ev.push(Event::SegmentOverrun { name: r.name.clone(), alarm: r.alarm, over_ms: now - end });
            }
        }
        ev
    }

    /// 10 minutes after a block closed, nothing started since (no next part, no segment):
    /// ask once what is going on.
    pub(crate) fn ask_what_now(&mut self, now: Ts) -> Option<Event> {
        if self.mode != Mode::Plan || self.completed_at.is_some() {
            return None;
        }
        if !matches!(self.phase, Phase::Break { brk: BreakKind::Between, .. } | Phase::Await { .. }) {
            return None;
        }
        let closed = (0..self.plan.len())
            .filter(|&i| self.is_study(i))
            .filter_map(|i| self.progress[i].completed_at)
            .max()?;
        let segment_since = self.segments.iter().any(|s| s.start >= closed);
        if now - closed < WHAT_NOW_MS || self.what_now_for == Some(closed) || segment_since {
            return None;
        }
        self.what_now_for = Some(closed);
        Some(Event::AskWhatNow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::SEC;
    use crate::day::{ItemKind, PlanBlock};
    use chrono::DateTime;

    fn t(s: &str) -> Ts {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }
    fn start() -> Ts {
        t("2026-09-28T10:00:00Z")
    }
    fn cfg() -> Config {
        let mut c = Config::default();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 45), PlanBlock::new("Экстернат", 45)];
        c
    }
    /// Day started, first block done at +45: between break, next = 1.
    fn after_first(c: &Config) -> DayState {
        let mut d = DayState::new(start(), c);
        d.start_day(start(), c).unwrap();
        d.tick(start() + 45 * MIN, c);
        d
    }

    #[test]
    fn lunch_then_nap_run_as_a_queue() {
        let c = cfg();
        let mut d = after_first(&c);
        let s0 = start() + 50 * MIN;
        d.start_segments(s0, vec![QueuedSegment::of(&c, "обед", None), QueuedSegment::of(&c, "Сон", None)]).unwrap();
        assert!(matches!(d.phase, Phase::Segment { rec: 0, next: 1 }));
        assert_eq!(d.segment().unwrap().name, "Обед");
        assert_eq!(d.segment_queue.len(), 1);
        // still blocked, like a break; no pause during a segment
        assert!(d.lock_state(s0 + MIN, &c).blocked);
        assert!(d.pause(s0 + MIN, &c).is_err());
        // 5 minutes before the end, at the end, then every 5 minutes with the overrun
        assert!(d.tick(s0 + 40 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentWarning { left_ms, .. } if *left_ms == 5 * MIN)));
        assert!(d.tick(s0 + 45 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentEnded { alarm: false, .. })));
        assert!(d.tick(s0 + 49 * MIN, &c).is_empty());
        assert!(d.tick(s0 + 50 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentOverrun { over_ms, .. } if *over_ms == 5 * MIN)));
        // the segment never ends by itself
        assert!(matches!(d.phase, Phase::Segment { rec: 0, .. }));
        d.end_segment(s0 + 52 * MIN).unwrap();
        // the nap starts with getting ready: nothing counts down until "Лёг"
        assert_eq!(d.segment().unwrap().name, "Сон");
        assert!(d.segment().unwrap().in_prep());
        let s1 = s0 + 52 * MIN;
        assert!(d.tick(s1 + 6 * MIN, &c).is_empty());
        let lay = s1 + 7 * MIN;
        d.lay_down(lay).unwrap();
        assert!(d.lay_down(lay + 1).is_err());
        // then no warning, only the alarm — 20 minutes from "Лёг"
        assert!(d.tick(lay + 18 * MIN, &c).is_empty());
        assert!(d.tick(lay + 20 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentEnded { alarm: true, .. })));
        d.end_segment(lay + 21 * MIN).unwrap();
        assert!(matches!(d.phase, Phase::Await { next: 1, .. }));
        let st = crate::stats::day_stats(&d, 180, lay + 22 * MIN);
        assert_eq!(st.breaks.len(), 2);
        assert_eq!((st.breaks[0].kind.as_str(), st.breaks[0].planned_min, st.breaks[0].actual_min, st.breaks[0].overrun_min), ("Обед", 45, 52.0, 7.0));
        assert_eq!(st.breaks[0].prep_min, None);
        // the nap: preparation and sleep apart
        let nap = &st.breaks[1];
        assert_eq!((nap.kind.as_str(), nap.prep_min, nap.prep_planned_min, nap.actual_min, nap.overrun_min), ("Сон", Some(7.0), Some(10), 21.0, 1.0));
        assert!(nap.lay_at.is_some());
        // not study time
        assert_eq!(st.actual_min, 45.0);
        assert_eq!(st.journal_total, 0.75);
    }

    #[test]
    fn planned_break_starts_when_the_block_before_closes() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 45), PlanBlock::brk("Обед", 40), PlanBlock::new("Экстернат", 45)];
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        // forecast: 90 work + 40 lunch (instead of the 20-min break between blocks)
        let f = d.forecast(start(), &c);
        assert_eq!((f.work_left_ms, f.breaks_left_ms, f.segments_left_ms), (90 * MIN, 0, 40 * MIN));
        assert_eq!(f.finish_at, start() + 130 * MIN);
        d.tick(start() + 45 * MIN, &c);
        assert!(matches!(d.phase, Phase::Segment { next: 2, .. }));
        assert_eq!(d.segment().unwrap().plan_item, Some(1));
        let f = d.forecast(start() + 50 * MIN, &c);
        assert_eq!(f.segments_left_ms, 35 * MIN);
        d.end_segment(start() + 85 * MIN).unwrap();
        assert!(d.is_block_done(1));
        assert!(matches!(d.phase, Phase::Await { next: 2, .. }));
        // planned segments are not study: stats and the plan total leave them out
        let st = crate::stats::day_stats(&d, 180, start() + 86 * MIN);
        assert_eq!(st.blocks.len(), 2);
        assert_eq!(st.planned_min, 90);
        assert_eq!(st.breaks[0].planned_min, 40);
        // a plan edit keeps the taken segment and its link
        d.set_plan(start() + 87 * MIN, vec![PlanBlock::new("Математика", 45), PlanBlock::brk("Обед", 40), PlanBlock::new("Экстернат", 60)], true).unwrap();
        assert!(d.is_block_done(1));
        assert_eq!(d.plan[1].kind, ItemKind::Break);
        // a segment that already ran may leave the plan: its record stays in the day
        d.set_plan(start() + 87 * MIN, vec![PlanBlock::new("Математика", 45), PlanBlock::new("Экстернат", 60)], true).unwrap();
        assert_eq!(d.plan.len(), 2);
        assert_eq!(d.segments[0].plan_item, None);
        assert_eq!(crate::stats::day_stats(&d, 180, start() + 88 * MIN).breaks[0].actual_min, 40.0);
    }

    #[test]
    fn what_now_is_asked_once_ten_minutes_after_a_block() {
        let mut c = cfg();
        c.timing.between_blocks_min = 30;
        let mut d = after_first(&c);
        assert!(d.tick(start() + 54 * MIN, &c).is_empty());
        assert!(d.tick(start() + 55 * MIN, &c).contains(&Event::AskWhatNow));
        assert!(!d.tick(start() + 56 * MIN, &c).contains(&Event::AskWhatNow));
        // a segment started before the 10 minutes: no question
        let mut d = after_first(&c);
        d.start_segments(start() + 50 * MIN, vec![QueuedSegment::of(&c, "Прогулка", None)]).unwrap();
        d.end_segment(start() + 52 * MIN).unwrap();
        assert!(!d.tick(start() + 60 * MIN, &c).contains(&Event::AskWhatNow));
    }

    #[test]
    fn segment_with_open_access_and_queue_edits() {
        let mut c = cfg();
        c.segments[0].open_access = true;
        let mut d = after_first(&c);
        let s0 = start() + 46 * MIN;
        d.start_segments(s0, vec![QueuedSegment::of(&c, "Обед", Some(30))]).unwrap();
        assert_eq!(d.lock_state(s0 + MIN, &c).reason, "segment_access");
        assert!(d.lock_state(s0 + 30 * MIN, &c).blocked);
        d.start_segments(s0 + MIN, vec![QueuedSegment::of(&c, "Свой", Some(7))]).unwrap();
        assert_eq!(d.segment_queue[0].minutes, 7);
        let f = d.forecast(s0 + 10 * MIN, &c);
        assert_eq!(f.segments_left_ms, 20 * MIN + 7 * MIN);
        d.drop_queued(s0 + 2 * MIN, 0).unwrap();
        assert!(d.segment_queue.is_empty());
        d.end_segment(s0 + 3 * SEC).unwrap();
        assert!(matches!(d.phase, Phase::Await { .. }));
        assert!(d.end_segment(s0 + 4 * SEC).is_err());
        // not during work
        d.start_next(s0 + 5 * SEC).unwrap();
        assert!(d.start_segments(s0 + 6 * SEC, vec![QueuedSegment::of(&c, "Обед", None)]).is_err());
    }

    #[test]
    fn queued_planned_segment_is_not_shown_as_taken() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 45), PlanBlock::brk("Обед", 40), PlanBlock::brk("Сон", 20), PlanBlock::new("Экстернат", 45)];
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.tick(start() + 45 * MIN, &c);
        assert_eq!(d.segment().unwrap().name, "Обед");
        // "Сон" waits in the queue: not taken, not open, counted once in the forecast
        // (with its 10 minutes to get ready)
        assert!(d.progress[2].started_at.is_none());
        assert!(!d.is_open_break(2));
        let f = d.forecast(start() + 45 * MIN, &c);
        assert_eq!(f.segments_left_ms, 70 * MIN);
        // dropping it from the queue skips it for today
        d.drop_queued(start() + 46 * MIN, 0).unwrap();
        assert!(!d.is_open_break(2));
        assert!(d.is_block_done(2));
        assert_eq!(d.forecast(start() + 46 * MIN, &c).segments_left_ms, 39 * MIN);
        d.end_segment(start() + 85 * MIN).unwrap();
        assert!(matches!(d.phase, Phase::Await { next: 3, .. }));
    }

    /// Acceptance: the block before "Сон 20" closes -> getting ready -> "Лёг" -> 20 min -> alarm;
    /// preparation and sleep apart in the stats.
    #[test]
    fn nap_waits_for_lay_down_and_rings_from_it() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 45), PlanBlock::brk("Сон", 20), PlanBlock::new("Экстернат", 45)];
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        // 45 work + 10 to get ready + 20 nap + 45 work
        assert_eq!(d.forecast(start(), &c).finish_at, start() + 120 * MIN);
        let closed = start() + 45 * MIN;
        d.tick(closed, &c);
        let r = d.segment().unwrap();
        assert!(r.in_prep() && r.alarm && r.plan_item == Some(1));
        let v = crate::view::build(&d, &c, closed + MIN);
        assert!(v.can.lay_down && v.phase.prep && !v.can.segment_mode);
        assert_eq!((v.phase.dur_ms, v.phase.remaining_ms), (10 * MIN, 9 * MIN));
        // the nap does not count down while getting ready: the forecast holds all 20 min of it
        assert_eq!(d.forecast(closed + 4 * MIN, &c).segments_left_ms, 6 * MIN + 20 * MIN);
        // the phone gets the preparation, never the alarm, while getting ready
        let (tl, _) = crate::phone::timeline(&d, &c, closed + MIN);
        assert!(tl[0].prep && !tl[0].alarm);
        assert_eq!((tl[0].from, tl[0].dur_ms, tl[0].until), (closed, 10 * MIN, None));
        // preparation over: a reminder, not the countdown — and again every 5 minutes
        assert!(d.tick(closed + 9 * MIN, &c).is_empty());
        assert!(d.tick(closed + 10 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentPrepOver { over_ms: 0, .. })));
        assert!(d.tick(closed + 14 * MIN, &c).is_empty());
        assert!(d.tick(closed + 15 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentPrepOver { .. })));
        assert!(d.segment().unwrap().in_prep());
        assert!(!d.tick(closed + 40 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentEnded { .. })));
        // "Лёг" at +13: the alarm 20 minutes later, not 20 minutes after the block
        let lay = closed + 13 * MIN;
        d.lay_down(lay).unwrap();
        let v = crate::view::build(&d, &c, lay + 5 * MIN);
        assert!(!v.phase.prep && !v.can.lay_down);
        assert_eq!(v.phase.remaining_ms, 15 * MIN);
        let (tl, _) = crate::phone::timeline(&d, &c, lay + MIN);
        assert!(tl[0].alarm && !tl[0].prep);
        assert_eq!(tl[0].from + tl[0].dur_ms, lay + 20 * MIN);
        assert!(d.tick(lay + 19 * MIN, &c).is_empty());
        assert!(d.tick(lay + 20 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentEnded { alarm: true, .. })));
        d.end_segment(lay + 22 * MIN).unwrap();
        assert!(matches!(d.phase, Phase::Await { next: 2, .. }));
        let nap = &crate::stats::day_stats(&d, 180, lay + 23 * MIN).breaks[0];
        assert_eq!((nap.prep_min, nap.actual_min, nap.overrun_min, nap.planned_min), (Some(13.0), 22.0, 2.0, 20));
        assert!(d.events.iter().any(|e| e.kind == "segment_lay"));
    }

    #[test]
    fn nap_skipped_on_the_preparation_counts_no_sleep() {
        let c = cfg();
        let mut d = after_first(&c);
        let s0 = start() + 46 * MIN;
        d.start_segments(s0, vec![QueuedSegment::of(&c, "Сон", None)]).unwrap();
        // "не буду спать": ended before "Лёг"
        d.end_segment(s0 + 4 * MIN).unwrap();
        let nap = &crate::stats::day_stats(&d, 180, s0 + 5 * MIN).breaks[0];
        assert_eq!((nap.prep_min, nap.actual_min, nap.overrun_min), (Some(4.0), 0.0, 0.0));
        assert!(nap.lay_at.is_none());
        assert!(d.events.iter().any(|e| e.text.contains("отменён на подготовке")));
        // a type set to 0 minutes of preparation counts down at once
        let mut c = cfg();
        c.segments[1].prep_min = Some(0);
        let mut d = after_first(&c);
        d.start_segments(s0, vec![QueuedSegment::of(&c, "Сон", None)]).unwrap();
        assert!(!d.segment().unwrap().in_prep());
        assert!(d.tick(s0 + 20 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentEnded { alarm: true, .. })));
    }

    /// Acceptance: lunch taken before its place in the plan -> the planned one does not start.
    #[test]
    fn early_lunch_takes_the_place_of_the_planned_one() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 45), PlanBlock::new("Экстернат", 45), PlanBlock::brk("Обед", 40), PlanBlock::new("Физика", 45)];
        let mut d = after_first(&c);
        let before = d.forecast(start() + 46 * MIN, &c);
        // the between break after Математика: lunch now, ahead of its place
        let s0 = start() + 46 * MIN;
        d.start_segments(s0, vec![QueuedSegment::of(&c, "обед", None)]).unwrap();
        assert_eq!(d.segment().unwrap().plan_item, Some(2));
        assert!(!d.is_open_break(2));
        assert!(crate::view::build(&d, &c, s0).blocks[2].started);
        // the forecast drops the planned 40 and counts the running 45 at once
        let f = d.forecast(s0, &c);
        assert_eq!(f.segments_left_ms, 45 * MIN);
        assert_eq!(before.segments_left_ms, 40 * MIN);
        d.end_segment(s0 + 45 * MIN).unwrap();
        assert!(d.is_block_done(2));
        d.start_next(s0 + 46 * MIN).unwrap();
        // Экстернат closes: the usual break between blocks, no second lunch
        d.tick(s0 + 91 * MIN, &c);
        assert!(matches!(d.phase, Phase::Break { brk: BreakKind::Between, next: 3, .. }));
        assert_eq!(d.segments.len(), 1);
        assert!(d.events.iter().any(|e| e.kind == "segment_plan"));
        // a second lunch by hand is just an extra one: nothing planned is left to take
        d.start_segments(s0 + 92 * MIN, vec![QueuedSegment::of(&c, "Обед", Some(10))]).unwrap();
        assert_eq!(d.segment().unwrap().plan_item, None);
    }

    #[test]
    fn early_lunch_dropped_from_the_queue_gives_the_planned_one_back() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 45), PlanBlock::new("Экстернат", 45), PlanBlock::brk("Обед", 40)];
        let mut d = after_first(&c);
        let s0 = start() + 46 * MIN;
        d.start_segments(s0, vec![QueuedSegment::of(&c, "Прогулка", None), QueuedSegment::of(&c, "Обед", None)]).unwrap();
        assert_eq!(d.segment_queue[0].plan_item, Some(2));
        assert!(!d.is_open_break(2));
        d.drop_queued(s0 + MIN, 0).unwrap();
        assert!(d.is_open_break(2));
        assert!(!d.is_block_done(2));
    }

    /// Acceptance: lunch without a timer — a stopwatch, the fact in the stats, the forecast live.
    #[test]
    fn lunch_without_a_timer_is_a_stopwatch() {
        let c = cfg();
        let mut d = after_first(&c);
        let s0 = start() + 46 * MIN;
        d.start_segments(s0, vec![QueuedSegment::of(&c, "Обед", None).stopwatch()]).unwrap();
        let v = crate::view::build(&d, &c, s0 + 10 * MIN);
        assert!(v.phase.stopwatch && !v.phase.alarm && v.can.segment_mode);
        assert_eq!(v.phase.elapsed_ms, 10 * MIN);
        // no warning, no end, no overrun alarm at the usual 45 minutes
        for m in [40, 45, 50, 74] {
            assert!(d.tick(s0 + m * MIN, &c).is_empty(), "minute {m}");
        }
        // only a quiet nudge half an hour past the usual length
        assert!(d.tick(s0 + 75 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentLong { elapsed_ms, .. } if *elapsed_ms == 75 * MIN)));
        // the forecast moves on with the clock once the usual length is over
        let f1 = d.forecast(s0 + 50 * MIN, &c);
        let f2 = d.forecast(s0 + 60 * MIN, &c);
        assert_eq!(f1.segments_left_ms, 0);
        assert_eq!(f2.finish_at - f1.finish_at, 10 * MIN);
        assert_eq!(d.forecast(s0 + 20 * MIN, &c).segments_left_ms, 25 * MIN);
        d.end_segment(s0 + 80 * MIN).unwrap();
        let b = &crate::stats::day_stats(&d, 180, s0 + 81 * MIN).breaks[0];
        assert_eq!((b.planned_min, b.actual_min, b.overrun_min, b.stopwatch), (45, 80.0, 35.0, true));
    }

    #[test]
    fn running_segment_switches_between_timer_and_stopwatch() {
        let mut c = cfg();
        c.profiles.full.plan = vec![PlanBlock::new("Математика", 45), PlanBlock::brk("Обед", 40), PlanBlock::new("Экстернат", 45)];
        let mut d = DayState::new(start(), &c);
        d.start_day(start(), &c).unwrap();
        d.tick(start() + 45 * MIN, &c);
        // the planned lunch started by itself with a countdown: go without one
        d.set_segment_mode(start() + 46 * MIN, true).unwrap();
        assert!(d.tick(start() + 85 * MIN, &c).is_empty());
        d.set_segment_mode(start() + 86 * MIN, false).unwrap();
        // back on the timer, already over: said once, then the usual reminders
        assert!(d.tick(start() + 86 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentEnded { .. })));
        // a nap keeps its alarm
        let mut d = after_first(&cfg());
        d.start_segments(start() + 46 * MIN, vec![QueuedSegment::of(&c, "Сон", None)]).unwrap();
        assert!(d.set_segment_mode(start() + 47 * MIN, true).is_err());
        d.lay_down(start() + 48 * MIN).unwrap();
        assert!(d.set_segment_mode(start() + 49 * MIN, true).is_err());
        // "Сон без времени" picked at the start: no alarm, nothing to get ready for
        let q = QueuedSegment::of(&c, "Сон", None).stopwatch();
        assert!(!q.alarm && q.prep_min == 0 && q.open_ended);
    }

    #[test]
    fn plan_json_marks_breaks() {
        let p = vec![PlanBlock::new("A", 45), PlanBlock::brk("Обед", 45)];
        let j = serde_json::to_string(&p).unwrap();
        assert_eq!(j, r#"[{"name":"A","minutes":45},{"name":"Обед","minutes":45,"type":"break"}]"#);
        let back: Vec<PlanBlock> = serde_json::from_str(&j).unwrap();
        assert_eq!(back, p);
    }
}
