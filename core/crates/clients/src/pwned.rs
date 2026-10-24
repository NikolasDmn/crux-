//! Breached-password check via Have I Been Pwned's range API (decided 2026-10-23, Q-auth-3).
//! k-anonymity: only the first 5 hex characters of the password's SHA-1 leave the server; the API
//! returns every matching suffix and we compare locally. `Add-Padding` hides the response size.

use std::time::Duration;

use async_trait::async_trait;
use sha1::{Digest, Sha1};

use crate::ClientError;

#[async_trait]
pub trait BreachCheck: Send + Sync {
    async fn is_breached(&self, password: &str) -> Result<bool, ClientError>;
}

pub struct Hibp {
    http: reqwest::Client,
    base: String,
}

impl Hibp {
    pub fn new(base: &str) -> Self {
        let http =
            reqwest::Client::builder().timeout(Duration::from_secs(2)).user_agent("crux-password-check").build().expect("http client");
        Self { http, base: base.trim_end_matches('/').to_string() }
    }
}

/// (prefix, suffix) of the uppercase hex SHA-1.
pub fn split_hash(password: &str) -> (String, String) {
    let hex = hex_upper(&Sha1::digest(password.as_bytes()));
    (hex[..5].to_string(), hex[5..].to_string())
}

fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// Is `suffix` listed with a count above zero? (Padding rows have count 0.)
pub fn suffix_listed(body: &str, suffix: &str) -> bool {
    body.lines().any(|line| {
        let mut parts = line.trim().split(':');
        matches!((parts.next(), parts.next()), (Some(s), Some(n)) if s.eq_ignore_ascii_case(suffix) && n.trim().parse::<u64>().is_ok_and(|n| n > 0))
    })
}

#[async_trait]
impl BreachCheck for Hibp {
    async fn is_breached(&self, password: &str) -> Result<bool, ClientError> {
        let (prefix, suffix) = split_hash(password);
        let body = self
            .http
            .get(format!("{}/range/{prefix}", self.base))
            .header("Add-Padding", "true")
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(suffix_listed(&body, &suffix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sha1_into_prefix_and_suffix() {
        // SHA-1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8
        assert_eq!(split_hash("password"), ("5BAA6".to_string(), "1E4C9B93F3F0682250B6CF8331B7EE68FD8".to_string()));
    }

    #[test]
    fn matches_listed_suffixes_but_not_padding() {
        let body = "1E4C9B93F3F0682250B6CF8331B7EE68FD8:9545824\r\nAAAA:0\r\nBBBB:3";
        assert!(suffix_listed(body, "1e4c9b93f3f0682250b6cf8331b7ee68fd8"));
        assert!(!suffix_listed(body, "AAAA"), "count 0 is padding");
        assert!(!suffix_listed(body, "CCCC"));
    }
}
