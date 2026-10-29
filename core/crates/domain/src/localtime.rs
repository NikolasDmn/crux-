//! Local wall-clock times to instants (audit L6). A time inside a DST gap (02:30 on spring-forward
//! day in Amsterdam, midnight in America/Santiago) doesn't exist: it becomes the first valid
//! instant after the gap, the way a wall clock jumps, instead of being read as UTC.

use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

/// The instant of a local wall-clock time: the earlier one when the clocks go back, the first one
/// after the gap when they go forward.
pub fn to_utc(tz: Tz, local: NaiveDateTime) -> DateTime<Utc> {
    if let Some(t) = tz.from_local_datetime(&local).earliest() {
        return t.with_timezone(&Utc);
    }
    // Gaps are whole quarter hours and at most a day long (Samoa skipped 30 December 2011).
    for q in 1..=4 * 25 {
        if let Some(t) = tz.from_local_datetime(&(local + Duration::minutes(15 * q))).earliest() {
            return t.with_timezone(&Utc);
        }
    }
    Utc.from_utc_datetime(&local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap()
    }

    #[test]
    fn gaps_jump_forward_and_overlaps_take_the_earlier() {
        let ams = chrono_tz::Europe::Amsterdam;
        // 29 March 2026, 02:00 -> 03:00 CEST (01:00 UTC).
        assert_eq!(to_utc(ams, at(2026, 3, 29, 2, 30)), Utc.with_ymd_and_hms(2026, 3, 29, 1, 0, 0).unwrap());
        // 25 October 2026, 02:30 happens twice: the first is 00:30 UTC.
        assert_eq!(to_utc(ams, at(2026, 10, 25, 2, 30)), Utc.with_ymd_and_hms(2026, 10, 25, 0, 30, 0).unwrap());
        assert_eq!(to_utc(ams, at(2026, 6, 1, 9, 0)), Utc.with_ymd_and_hms(2026, 6, 1, 7, 0, 0).unwrap());
        // Santiago skips midnight on 6 September 2026 (00:00 -> 01:00, UTC-3 = 04:00 UTC).
        let scl = chrono_tz::America::Santiago;
        assert_eq!(to_utc(scl, at(2026, 9, 6, 0, 0)), Utc.with_ymd_and_hms(2026, 9, 6, 4, 0, 0).unwrap());
    }
}
