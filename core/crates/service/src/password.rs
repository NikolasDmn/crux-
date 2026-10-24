//! argon2id with m = 19 MiB, t = 2, p = 1 (spec 4.1). Hashing is CPU-bound, so callers run it on
//! the blocking pool.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

fn hasher() -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::new(19 * 1024, 2, 1, None).expect("argon2 params"))
}

pub fn hash(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(hasher().hash_password(password.as_bytes(), &salt).map_err(|e| anyhow::anyhow!("hash: {e}"))?.to_string())
}

pub fn verify(password: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|parsed| hasher().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// Verified against when the login name doesn't exist, so a miss costs the same time as a hit.
pub fn dummy_hash() -> &'static str {
    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DUMMY.get_or_init(|| hash("crux-dummy-password-for-timing").expect("dummy hash"))
}

/// A temporary password for an admin reset (no email in a small trial): 16 letters and digits,
/// without look-alikes (0/O, 1/l/I).
pub fn temporary() -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..16).map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char).collect()
}

pub async fn hash_blocking(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || hash(&password)).await?
}

pub async fn verify_blocking(password: String, phc: String) -> bool {
    tokio::task::spawn_blocking(move || verify(&password, &phc)).await.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify() {
        let h = hash("correct horse battery").unwrap();
        assert!(h.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify("correct horse battery", &h));
        assert!(!verify("wrong horse battery", &h));
        assert!(!verify("anything", "not-a-phc-string"));
    }
}
