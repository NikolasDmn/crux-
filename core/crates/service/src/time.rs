//! API time format (spec 3.1): ISO-8601 UTC with milliseconds and `Z`.

use chrono::{DateTime, SecondsFormat, Utc};

pub fn iso(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn iso_opt(t: Option<DateTime<Utc>>) -> Option<String> {
    t.map(iso)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn formats_with_millis_and_z() {
        let t = Utc.with_ymd_and_hms(2026, 10, 23, 9, 5, 0).unwrap();
        assert_eq!(iso(t), "2026-10-23T09:05:00.000Z");
    }
}
