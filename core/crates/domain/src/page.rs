//! Pagination (spec 3.1): `?limit=50&cursor=…` → `{items, nextCursor}`, limit ≤ 200.
//! Cursors are the last row's internal sort key run through a keyed permutation (audit L5: a
//! readable id showed row counts and growth). The key comes from the service secret at startup
//! (`set_cursor_key`); tests use a fixed one. A tampered cursor is just another position.

use std::sync::OnceLock;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

static CURSOR_KEY: OnceLock<Vec<u8>> = OnceLock::new();

/// Call once at startup, before serving.
pub fn set_cursor_key(secret: &str) {
    let _ = CURSOR_KEY.set(Sha256::digest(format!("crux-cursor|{secret}").as_bytes()).to_vec());
}

fn cursor_key() -> &'static [u8] {
    CURSOR_KEY.get_or_init(|| Sha256::digest(b"crux-cursor|dev").to_vec())
}

/// A 4-round Feistel network over the two 32-bit halves: a permutation of all i64s.
fn round(r: u8, half: u32) -> u32 {
    let mut h = Sha256::new();
    h.update(cursor_key());
    h.update([r]);
    h.update(half.to_be_bytes());
    let d = h.finalize();
    u32::from_be_bytes([d[0], d[1], d[2], d[3]])
}

fn permute(v: u64) -> u64 {
    let (mut l, mut r) = ((v >> 32) as u32, v as u32);
    for i in 0..4 {
        (l, r) = (r, l ^ round(i, r));
    }
    (u64::from(l) << 32) | u64::from(r)
}

fn unpermute(v: u64) -> u64 {
    let (mut l, mut r) = ((v >> 32) as u32, v as u32);
    for i in (0..4).rev() {
        (l, r) = (r ^ round(i, l), l);
    }
    (u64::from(l) << 32) | u64::from(r)
}

pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 200;

pub fn clamp_limit(limit: Option<u32>) -> u32 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Keyset cursor over a descending/ascending unique i64 key.
pub fn encode_cursor(key: i64) -> String {
    URL_SAFE_NO_PAD.encode(permute(key as u64).to_be_bytes())
}

pub fn decode_cursor(cursor: &str) -> Option<i64> {
    let bytes: [u8; 8] = URL_SAFE_NO_PAD.decode(cursor).ok()?.try_into().ok()?;
    Some(unpermute(u64::from_be_bytes(bytes)) as i64)
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
        for k in [0, 1, 2, 41, 42, i64::MAX, -7] {
            assert_eq!(decode_cursor(&encode_cursor(k)), Some(k));
        }
        // Audit L5: nothing readable, and neighbours don't look alike.
        let (a, b) = (encode_cursor(1000), encode_cursor(1001));
        assert!(!URL_SAFE_NO_PAD.decode(&a).unwrap().ends_with(&1000u64.to_be_bytes()));
        assert_ne!(a[..6], b[..6]);
    }
}
