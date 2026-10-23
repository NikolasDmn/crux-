use std::sync::Mutex;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::rngs::OsRng;
use rand::RngCore;

/// Makes PublicIds, event ids and secret tokens. A trait so tests get predictable values.
pub trait IdGen: Send + Sync {
    /// A new ULID string (26 chars, sortable by time).
    fn ulid(&self) -> String;
    /// A URL-safe random secret with at least 256 bits of entropy (refresh tokens, one-time links).
    fn secret(&self) -> String;
    /// A short human-typeable code (invites): 16 chars, Crockford base32, ~80 bits.
    fn invite_code(&self) -> String;
}

pub struct UlidGen {
    last: Mutex<ulid::Generator>,
}

impl Default for UlidGen {
    fn default() -> Self {
        Self { last: Mutex::new(ulid::Generator::new()) }
    }
}

impl IdGen for UlidGen {
    /// Monotonic within one process, so ids made in the same millisecond still sort.
    fn ulid(&self) -> String {
        let mut g = self.last.lock().unwrap();
        g.generate().unwrap_or_else(|_| ulid::Ulid::new()).to_string()
    }

    fn secret(&self) -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    fn invite_code(&self) -> String {
        const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
        let chars: String = (0..16).map(|_| CROCKFORD[(OsRng.next_u32() % 32) as usize] as char).collect();
        // Grouped for reading aloud: XXXX-XXXX-XXXX-XXXX.
        format!("{}-{}-{}-{}", &chars[0..4], &chars[4..8], &chars[8..12], &chars[12..16])
    }
}

/// Sequential ids for tests: `01TEST00000000000000000001`, ...
pub struct SeqIds(Mutex<u64>);

impl Default for SeqIds {
    fn default() -> Self {
        Self(Mutex::new(0))
    }
}

impl SeqIds {
    fn next(&self) -> u64 {
        let mut n = self.0.lock().unwrap();
        *n += 1;
        *n
    }
}

impl IdGen for SeqIds {
    fn ulid(&self) -> String {
        format!("01TEST{:020}", self.next())
    }

    fn secret(&self) -> String {
        format!("secret-{:020}", self.next())
    }

    fn invite_code(&self) -> String {
        format!("INVITE-{:08}", self.next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulids_are_sortable_and_distinct() {
        let g = UlidGen::default();
        let ids: Vec<String> = (0..100).map(|_| g.ulid()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
        sorted.dedup();
        assert_eq!(sorted.len(), 100);
        assert!(ids.iter().all(|i| i.len() == 26));
    }

    #[test]
    fn secrets_are_long_and_unique() {
        let g = UlidGen::default();
        let (a, b) = (g.secret(), g.secret());
        assert_ne!(a, b);
        assert_eq!(a.len(), 43, "32 bytes, base64url");
    }

    #[test]
    fn invite_codes_are_grouped() {
        let code = UlidGen::default().invite_code();
        assert_eq!(code.len(), 19);
        assert_eq!(code.matches('-').count(), 3);
    }

    #[test]
    fn seq_ids_are_valid_public_ids() {
        assert_eq!(SeqIds::default().ulid().len(), 26);
    }
}
