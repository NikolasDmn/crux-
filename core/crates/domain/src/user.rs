//! Users and their settings: plain types plus patch validation.

use serde::{Deserialize, Serialize};

use crate::{auth, ValidationErrors};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Admin,
}

impl Role {
    pub fn as_db(self) -> &'static str {
        match self {
            Role::User => "User",
            Role::Admin => "Admin",
        }
    }

    pub fn from_db(s: &str) -> Self {
        if s == "Admin" { Role::Admin } else { Role::User }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserStatus {
    Active,
    Disabled,
    PendingDeletion,
}

impl UserStatus {
    pub fn from_db(s: &str) -> Self {
        match s {
            "Disabled" => UserStatus::Disabled,
            "PendingDeletion" => UserStatus::PendingDeletion,
            _ => UserStatus::Active,
        }
    }

    /// Disabled accounts can't log in. PendingDeletion can, so the user can cancel (spec 11).
    pub fn may_log_in(self) -> bool {
        self != UserStatus::Disabled
    }
}

/// `PATCH /me`. Email changes need re-verification (frontend spec, Profile) and arrive later.
#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePatch {
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub time_zone: Option<String>,
}

pub fn validate_profile_patch(p: &ProfilePatch) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    if let Some(u) = &p.username {
        e.merge(auth::validate_username(u));
    }
    if let Some(f) = &p.first_name {
        e.merge(auth::validate_name("firstName", f));
    }
    if let Some(l) = &p.last_name {
        e.merge(auth::validate_name("lastName", l));
    }
    if let Some(tz) = &p.time_zone {
        e.merge(auth::validate_time_zone(tz));
    }
    e
}

/// `PATCH /me/settings` (`UserSettings`).
#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub flash_retention: Option<f64>,
    pub flash_new_per_day: Option<i32>,
    pub flash_review_limit: Option<i32>,
    pub mock_go_easy: Option<bool>,
    pub mock_real_layout: Option<bool>,
    /// Replaces `Extra` wholesale: small UI prefs (`dailyHours`, `multiCoursePerDay`).
    #[schema(value_type = Option<Object>)]
    pub extra: Option<serde_json::Value>,
}

pub fn validate_settings_patch(p: &SettingsPatch) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    // DECIMAL(3,2), CHECK (0 < r < 1)
    if p.flash_retention.is_some_and(|r| !(0.70..=0.99).contains(&r)) {
        e.push("flashRetention", "invalid", "Use a value between 0.70 and 0.99.");
    }
    if p.flash_new_per_day.is_some_and(|n| !(0..=1000).contains(&n)) {
        e.push("flashNewPerDay", "invalid", "Use 0-1000.");
    }
    if p.flash_review_limit.is_some_and(|n| !(0..=10_000).contains(&n)) {
        e.push("flashReviewLimit", "invalid", "Use 0-10000.");
    }
    if let Some(extra) = &p.extra {
        if !extra.is_object() {
            e.push("extra", "invalid", "Must be an object.");
        } else if let Some(h) = extra.get("dailyHours") {
            if !h.as_f64().is_some_and(|h| (0.5..=16.0).contains(&h)) {
                e.push("extra.dailyHours", "invalid", "Use 0.5-16 hours.");
            }
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_and_status_mapping() {
        assert_eq!(Role::from_db("Admin"), Role::Admin);
        assert_eq!(Role::from_db("User").as_db(), "User");
        assert!(UserStatus::from_db("PendingDeletion").may_log_in());
        assert!(!UserStatus::from_db("Disabled").may_log_in());
    }

    #[test]
    fn profile_patch_validation() {
        assert!(validate_profile_patch(&ProfilePatch::default()).is_empty());
        let bad = ProfilePatch { username: Some("x".into()), time_zone: Some("Nowhere".into()), ..Default::default() };
        assert_eq!(validate_profile_patch(&bad).0.len(), 2);
    }

    #[test]
    fn settings_patch_validation() {
        let ok = SettingsPatch { flash_retention: Some(0.9), extra: Some(serde_json::json!({"dailyHours": 4})), ..Default::default() };
        assert!(validate_settings_patch(&ok).is_empty());
        let bad = SettingsPatch {
            flash_retention: Some(1.0),
            flash_new_per_day: Some(-1),
            flash_review_limit: Some(20_000),
            extra: Some(serde_json::json!({"dailyHours": 40})),
            ..Default::default()
        };
        assert_eq!(validate_settings_patch(&bad).0.len(), 4);
        let not_obj = SettingsPatch { extra: Some(serde_json::json!([1])), ..Default::default() };
        assert_eq!(validate_settings_patch(&not_obj).0[0].field, "extra");
    }
}
