//! Study clock: all "day" logic runs on a fixed UTC offset (Moscow, UTC+3, no DST).

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, Timelike, Utc};

/// Milliseconds since Unix epoch (UTC).
pub type Ts = i64;

pub const SEC: i64 = 1_000;
pub const MIN: i64 = 60 * SEC;

pub fn now_ts() -> Ts {
    Utc::now().timestamp_millis()
}

fn offset(tz_offset_min: i32) -> FixedOffset {
    FixedOffset::east_opt(tz_offset_min * 60).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap())
}

pub fn local(ts: Ts, tz_offset_min: i32) -> DateTime<FixedOffset> {
    DateTime::<Utc>::from_timestamp_millis(ts).unwrap_or_default().with_timezone(&offset(tz_offset_min))
}

pub fn local_date(ts: Ts, tz_offset_min: i32) -> NaiveDate {
    local(ts, tz_offset_min).date_naive()
}

pub fn minute_of_day(ts: Ts, tz_offset_min: i32) -> u32 {
    let l = local(ts, tz_offset_min);
    l.hour() * 60 + l.minute()
}

/// 0 = Monday.
pub fn weekday_index(ts: Ts, tz_offset_min: i32) -> usize {
    local(ts, tz_offset_min).weekday().num_days_from_monday() as usize
}

/// Timestamp of `minute` on the local date of `ts`.
pub fn at_minute(ts: Ts, tz_offset_min: i32, minute: u32) -> Ts {
    let d = local_date(ts, tz_offset_min);
    let naive = d.and_hms_opt(minute / 60, minute % 60, 0).unwrap_or_default();
    naive.and_utc().timestamp_millis() - tz_offset_min as i64 * MIN
}

pub fn hm(ts: Ts, tz_offset_min: i32) -> String {
    local(ts, tz_offset_min).format("%H:%M").to_string()
}

pub fn iso(ts: Ts, tz_offset_min: i32) -> String {
    local(ts, tz_offset_min).format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moscow_minutes() {
        // 2026-09-28 19:00:00 UTC == 22:00 MSK
        let ts = DateTime::parse_from_rfc3339("2026-09-28T19:00:00Z").unwrap().timestamp_millis();
        assert_eq!(minute_of_day(ts, 180), 22 * 60);
        assert_eq!(at_minute(ts, 180, 22 * 60), ts);
        assert_eq!(weekday_index(ts, 180), 0);
    }
}
