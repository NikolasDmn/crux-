//! Membership tiers (backend-spec 4.3). Every limit comes from the `Tier` row, never a constant.

use serde::{Deserialize, Serialize};

use crate::ValidationErrors;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierLimits {
    pub storage_quota_bytes: i64,
    pub max_courses: Option<i32>,
    /// Free-form limits still settling (`Tier.Limits`); read if set, enforced as features land.
    pub extra: Option<serde_json::Value>,
}

/// Signup tier: the invite's tier when it has one, otherwise the default tier.
pub fn signup_tier_id(invite_tier_id: Option<i64>, default_tier_id: i64) -> i64 {
    invite_tier_id.unwrap_or(default_tier_id)
}

/// `MaxCourses` NULL = unlimited.
pub fn may_create_course(limits: &TierLimits, owned_active_courses: i64) -> bool {
    limits.max_courses.is_none_or(|max| owned_active_courses < i64::from(max))
}

pub fn validate_tier(code: &str, name: &str, storage_quota_bytes: i64, max_courses: Option<i32>) -> ValidationErrors {
    let mut errors = ValidationErrors::new();
    let code_ok = (1..=32).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !code_ok {
        errors.push("code", "invalid", "Use 1-32 lowercase letters, digits or '-'.");
    }
    if !(1..=64).contains(&name.trim().chars().count()) {
        errors.push("name", "invalid", "Enter 1-64 characters.");
    }
    if storage_quota_bytes < 0 {
        errors.push("storageQuotaBytes", "invalid", "Must be zero or more.");
    }
    if max_courses.is_some_and(|m| m < 0) {
        errors.push("maxCourses", "invalid", "Must be zero or more, or empty for unlimited.");
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(max_courses: Option<i32>) -> TierLimits {
        TierLimits { storage_quota_bytes: 1 << 30, max_courses, extra: None }
    }

    #[test]
    fn signup_tier_prefers_invite() {
        assert_eq!(signup_tier_id(Some(7), 1), 7);
        assert_eq!(signup_tier_id(None, 1), 1);
    }

    #[test]
    fn course_cap() {
        assert!(may_create_course(&limits(None), 10_000));
        assert!(may_create_course(&limits(Some(3)), 2));
        assert!(!may_create_course(&limits(Some(3)), 3));
        assert!(!may_create_course(&limits(Some(0)), 0));
    }

    #[test]
    fn tier_validation() {
        assert!(validate_tier("free", "Free", 1 << 30, None).is_empty());
        let e = validate_tier("Free Tier", "", -1, Some(-2));
        let fields: Vec<_> = e.0.iter().map(|f| f.field.as_str()).collect();
        assert_eq!(fields, ["code", "name", "storageQuotaBytes", "maxCourses"]);
    }
}
