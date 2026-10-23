//! Pagination (spec 3.1): `?limit=50&cursor=…` → `{items, nextCursor}`, limit ≤ 200.
//! Cursors are opaque base64url of the last row's internal sort key; never an `Id` in the clear.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 200;

pub fn clamp_limit(limit: Option<u32>) -> u32 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Keyset cursor over a descending/ascending unique i64 key.
pub fn encode_cursor(key: i64) -> String {
    URL_SAFE_NO_PAD.encode(format!("k:{key}"))
}

pub fn decode_cursor(cursor: &str) -> Option<i64> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let s = String::from_utf8(bytes).ok()?;
    s.strip_prefix("k:")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_is_clamped() {
        assert_eq!(clamp_limit(None), 50);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(500)), 200);
        assert_eq!(clamp_limit(Some(20)), 20);
    }

    #[test]
    fn cursor_round_trip_and_garbage() {
        assert_eq!(decode_cursor(&encode_cursor(42)), Some(42));
        assert_eq!(decode_cursor("not-a-cursor"), None);
        assert_eq!(decode_cursor(&URL_SAFE_NO_PAD.encode("x:1")), None);
    }
}
