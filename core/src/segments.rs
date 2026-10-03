//! Non-study segments: lunch, a nap, a walk… A countdown on the wall clock, then overrun until
//! the user ends it ("Закончил" / "Встал"). Several can be queued; planned ones (`type: "break"`
//! plan items) start by themselves when the block before them closes.

use crate::clock::{Ts, MIN};
use crate::config::Config;
use crate::day::{BreakKind, DayState, Event, Mode, Phase, QueuedSegment, SegmentRecord};

/// Warn this long before the planned end (not for alarm segments).
const WARN_MS: i64 = 5 * MIN;
/// Repeat the "over time" notice this often.
const OVERRUN_REPEAT_MS: i64 = 5 * MIN;
/// Ask "что сейчас?" this long after a block closed with nothing started since.
pub const WHAT_NOW_MS: i64 = 10 * MIN;

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
        self.log(
            now,
            "segment",
            format!("{} {} мин{}", q.name, q.minutes, if q.alarm { " — будильник в конце" } else { "" }),
        );
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
        r.end = Some(now);
        let (name, planned, actual) = (r.name.clone(), r.planned_ms(), now - r.start);
        if let Some(i) = r.plan_item {
            if let Some(p) = self.progress.get_mut(i) {
                p.completed_at = Some(now);
            }
        }
        let over = actual - planned;
        self.log(
            now,
            "segment_end",
            format!(
                "{name} окончен: {} мин{}",
                (actual + MIN / 2) / MIN,
                if over >= MIN { format!(" (превышение {} мин)", over / MIN) } else { String::new() }
            ),
        );
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
        if let Some(p) = q.plan_item.and_then(|i| self.progress.get_mut(i)) {
            p.completed_at = Some(now);
        }
        self.log(now, "segment_queue", format!("Из очереди убран: {}", q.name));
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
        // the nap starts right away and has no warning, only the alarm
        assert_eq!(d.segment().unwrap().name, "Сон");
        let s1 = s0 + 52 * MIN;
        assert!(d.tick(s1 + 18 * MIN, &c).is_empty());
        assert!(d.tick(s1 + 20 * MIN, &c).iter().any(|e| matches!(e, Event::SegmentEnded { alarm: true, .. })));
        d.end_segment(s1 + 21 * MIN).unwrap();
        assert!(matches!(d.phase, Phase::Await { next: 1, .. }));
        let st = crate::stats::day_stats(&d, 180, s1 + 22 * MIN);
        assert_eq!(st.breaks.len(), 2);
        assert_eq!((st.breaks[0].kind.as_str(), st.breaks[0].planned_min, st.breaks[0].actual_min, st.breaks[0].overrun_min), ("Обед", 45, 52.0, 7.0));
        assert_eq!((st.breaks[1].kind.as_str(), st.breaks[1].overrun_min), ("Сон", 1.0));
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
        assert!(d.set_plan(start() + 87 * MIN, vec![PlanBlock::new("Математика", 45), PlanBlock::new("Экстернат", 60)], true).is_err());
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
        assert!(d.progress[2].started_at.is_none());
        assert!(!d.is_open_break(2));
        let f = d.forecast(start() + 45 * MIN, &c);
        assert_eq!(f.segments_left_ms, 60 * MIN);
        // dropping it from the queue skips it for today
        d.drop_queued(start() + 46 * MIN, 0).unwrap();
        assert!(!d.is_open_break(2));
        assert!(d.is_block_done(2));
        assert_eq!(d.forecast(start() + 46 * MIN, &c).segments_left_ms, 39 * MIN);
        d.end_segment(start() + 85 * MIN).unwrap();
        assert!(matches!(d.phase, Phase::Await { next: 3, .. }));
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
