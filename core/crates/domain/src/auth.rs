//! Signup, login and token rules (backend-spec 4.1). Pure: the service layer loads rows, calls
//! these, and writes the result.

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use crate::ValidationErrors;

pub const ACCESS_TOKEN_TTL: Duration = Duration::minutes(15);
/// Not fixed by the spec; see OPEN-QUESTIONS.md.
pub const REFRESH_TOKEN_TTL: Duration = Duration::days(30);
pub const VERIFY_EMAIL_TTL: Duration = Duration::hours(48);
pub const RESET_PASSWORD_TTL: Duration = Duration::hours(1);
pub const SSE_TICKET_TTL_SECS: u64 = 60;
pub const PASSWORD_MIN_CHARS: usize = 10;
/// argon2 has its own upper bound; this keeps hashing time bounded.
pub const PASSWORD_MAX_CHARS: usize = 256;

/// sha256 as lowercase hex: how invite codes, refresh tokens and one-time tokens are stored.
pub fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Invite codes are typed by hand: ignore case and surrounding space before hashing.
pub fn normalize_invite_code(code: &str) -> String {
    code.trim().to_uppercase()
}

/// Emails and usernames compare case-insensitively (the DB collation does the same).
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

pub fn validate_password(password: &str, breached: &HashSet<String>) -> ValidationErrors {
    let mut errors = ValidationErrors::new();
    let chars = password.chars().count();
    if chars < PASSWORD_MIN_CHARS {
        errors.push("password", "too_short", "Use at least 10 characters.");
    } else if chars > PASSWORD_MAX_CHARS {
        errors.push("password", "too_long", "Use at most 256 characters.");
    } else if breached.contains(&password.to_lowercase()) {
        errors.push("password", "breached", "This password appears in known data breaches. Pick another one.");
    }
    errors
}

/// Username: 3-32 chars of letters, digits, `_`, `.`, `-`, starting with a letter or digit.
/// The spec doesn't define this; see OPEN-QUESTIONS.md.
pub fn validate_username(username: &str) -> ValidationErrors {
    let mut errors = ValidationErrors::new();
    let len = username.chars().count();
    let charset_ok = username.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    let starts_ok = username.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    if !(3..=32).contains(&len) || !charset_ok || !starts_ok {
        errors.push(
            "username",
            "invalid",
            "Use 3-32 letters, digits, '_', '.' or '-', starting with a letter or digit.",
        );
    }
    errors
}

/// Deliberately simple: one `@`, a non-empty local part, a dotted domain, no spaces. The verify
/// email is the real check.
pub fn validate_email(email: &str) -> ValidationErrors {
    let mut errors = ValidationErrors::new();
    let email = email.trim();
    let ok = email.len() <= 255
        && !email.contains(char::is_whitespace)
        && match email.split_once('@') {
            Some((local, domain)) => {
                !local.is_empty() && !domain.contains('@') && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
            }
            None => false,
        };
    if !ok {
        errors.push("email", "invalid", "Enter a valid email address.");
    }
    errors
}

pub fn validate_time_zone(tz: &str) -> ValidationErrors {
    if tz.parse::<chrono_tz::Tz>().is_ok() {
        ValidationErrors::new()
    } else {
        ValidationErrors::single("timeZone", "invalid", "Unknown time zone.")
    }
}

pub fn validate_name(field: &str, value: &str) -> ValidationErrors {
    let len = value.trim().chars().count();
    if (1..=255).contains(&len) {
        ValidationErrors::new()
    } else {
        ValidationErrors::single(field, "invalid", "Enter 1-255 characters.")
    }
}

/// What an invite row looks like to the rule.
#[derive(Debug, Clone)]
pub struct InviteState {
    pub email: Option<String>,
    pub max_uses: i32,
    pub uses: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InviteRejection {
    #[error("invite revoked")]
    Revoked,
    #[error("invite expired")]
    Expired,
    #[error("invite used up")]
    UsedUp,
    #[error("invite is for another email address")]
    WrongEmail,
}

/// Spec 4.1: checked for RevokedAt, ExpiresAt, Uses < MaxUses, and the email if the invite has one.
pub fn check_invite(invite: &InviteState, signup_email: &str, now: DateTime<Utc>) -> Result<(), InviteRejection> {
    if invite.revoked_at.is_some() {
        return Err(InviteRejection::Revoked);
    }
    if invite.expires_at.is_some_and(|at| at <= now) {
        return Err(InviteRejection::Expired);
    }
    if invite.uses >= invite.max_uses {
        return Err(InviteRejection::UsedUp);
    }
    if let Some(email) = &invite.email {
        if normalize_email(email) != normalize_email(signup_email) {
            return Err(InviteRejection::WrongEmail);
        }
    }
    Ok(())
}

/// What a refresh-token lookup found.
#[derive(Debug, Clone)]
pub struct SessionState {
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshDecision {
    /// Rotate: revoke this session and issue a new one.
    Rotate,
    /// The cookie is past its life; ask the user to log in.
    Expired,
    /// A rotated (revoked) token came back: someone kept a copy. Revoke every session of the user.
    Reused,
}

/// Spec 4.1: the refresh token rotates on every use; reusing an old one revokes every session.
pub fn decide_refresh(session: &SessionState, now: DateTime<Utc>) -> RefreshDecision {
    if session.revoked_at.is_some() {
        RefreshDecision::Reused
    } else if session.expires_at <= now {
        RefreshDecision::Expired
    } else {
        RefreshDecision::Rotate
    }
}

/// One-time tokens (verify email, reset password).
#[derive(Debug, Clone)]
pub struct OneTimeTokenState {
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

pub fn one_time_token_usable(token: &OneTimeTokenState, now: DateTime<Utc>) -> bool {
    token.used_at.is_none() && token.expires_at > now
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 23, 12, 0, 0).unwrap()
    }

    fn invite() -> InviteState {
        InviteState { email: None, max_uses: 1, uses: 0, expires_at: None, revoked_at: None }
    }

    #[test]
    fn token_hash_is_sha256_hex() {
        assert_eq!(token_hash("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn password_rules() {
        let breached: HashSet<String> = ["password123".to_string()].into();
        assert!(!validate_password("short", &breached).is_empty());
        assert!(!validate_password("Password123", &breached).is_empty(), "breached check ignores case");
        assert!(validate_password("correct horse battery", &breached).is_empty());
        assert!(!validate_password(&"x".repeat(257), &breached).is_empty());
        assert_eq!(validate_password("ééééééééé", &breached).0[0].code, "too_short", "counts chars, not bytes");
    }

    #[test]
    fn username_rules() {
        assert!(validate_username("nick").is_empty());
        assert!(validate_username("n.k-1_x").is_empty());
        assert!(!validate_username("ni").is_empty());
        assert!(!validate_username("_nick").is_empty());
        assert!(!validate_username("nick name").is_empty());
        assert!(!validate_username(&"a".repeat(33)).is_empty());
    }

    #[test]
    fn email_rules() {
        assert!(validate_email("nick@example.com").is_empty());
        assert!(validate_email("  nick@example.com ").is_empty());
        for bad in ["nick", "nick@", "@example.com", "nick@example", "ni ck@example.com", "a@b@c.com", "a@.com"] {
            assert!(!validate_email(bad).is_empty(), "{bad} should be rejected");
        }
    }

    #[test]
    fn time_zone_and_names() {
        assert!(validate_time_zone("Europe/Amsterdam").is_empty());
        assert!(!validate_time_zone("Mars/Base").is_empty());
        assert!(validate_name("firstName", "Nick").is_empty());
        assert!(!validate_name("firstName", "   ").is_empty());
    }

    #[test]
    fn invite_accepts_fresh_invite() {
        assert_eq!(check_invite(&invite(), "a@b.nl", now()), Ok(()));
    }

    #[test]
    fn invite_rejections_in_order() {
        let revoked = InviteState { revoked_at: Some(now()), ..invite() };
        assert_eq!(check_invite(&revoked, "a@b.nl", now()), Err(InviteRejection::Revoked));

        let expired = InviteState { expires_at: Some(now()), ..invite() };
        assert_eq!(check_invite(&expired, "a@b.nl", now()), Err(InviteRejection::Expired));

        let used = InviteState { uses: 1, ..invite() };
        assert_eq!(check_invite(&used, "a@b.nl", now()), Err(InviteRejection::UsedUp));

        let multi = InviteState { uses: 4, max_uses: 5, ..invite() };
        assert_eq!(check_invite(&multi, "a@b.nl", now()), Ok(()));
    }

    #[test]
    fn invite_email_matches_case_insensitively() {
        let bound = InviteState { email: Some("Nick@Example.com".into()), ..invite() };
        assert_eq!(check_invite(&bound, "nick@example.COM", now()), Ok(()));
        assert_eq!(check_invite(&bound, "other@example.com", now()), Err(InviteRejection::WrongEmail));
    }

    #[test]
    fn refresh_decisions() {
        let live = SessionState { expires_at: now() + Duration::days(1), revoked_at: None };
        assert_eq!(decide_refresh(&live, now()), RefreshDecision::Rotate);

        let expired = SessionState { expires_at: now(), revoked_at: None };
        assert_eq!(decide_refresh(&expired, now()), RefreshDecision::Expired);

        let rotated = SessionState { revoked_at: Some(now() - Duration::minutes(1)), ..live };
        assert_eq!(decide_refresh(&rotated, now()), RefreshDecision::Reused, "reuse wins over expiry");
    }

    #[test]
    fn one_time_tokens() {
        let fresh = OneTimeTokenState { expires_at: now() + Duration::minutes(5), used_at: None };
        assert!(one_time_token_usable(&fresh, now()));
        assert!(!one_time_token_usable(&OneTimeTokenState { used_at: Some(now()), ..fresh.clone() }, now()));
        assert!(!one_time_token_usable(&OneTimeTokenState { expires_at: now(), ..fresh }, now()));
    }
}
