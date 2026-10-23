//! Response shapes the service returns (camelCase, PublicIds only, ISO times; spec 3.1).

use crux_repo::invites::InviteRow;
use crux_repo::settings::SettingsRow;
use crux_repo::tiers::TierRow;
use crux_repo::users::UserRow;
use serde::Serialize;
use utoipa::ToSchema;

use crate::time::{iso, iso_opt};

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserView {
    pub id: String,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub first_name: String,
    pub last_name: String,
    pub time_zone: String,
    pub locale: String,
    #[schema(example = "User")]
    pub role: String,
    pub tier: String,
    #[schema(example = "Active")]
    pub status: String,
    pub created_at: String,
    pub version: i32,
}

impl From<&UserRow> for UserView {
    fn from(u: &UserRow) -> Self {
        Self {
            id: u.public_id.clone(),
            username: u.username.clone(),
            email: u.email.clone(),
            email_verified: u.email_verified_at.is_some(),
            first_name: u.first_name.clone(),
            last_name: u.last_name.clone(),
            time_zone: u.time_zone.clone(),
            locale: u.locale.clone(),
            role: u.role.clone(),
            tier: u.tier_code.clone(),
            status: u.status.clone(),
            created_at: iso(u.created_at),
            version: u.row_version,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub flash_retention: f64,
    pub flash_new_per_day: i32,
    pub flash_review_limit: i32,
    pub mock_go_easy: bool,
    pub mock_real_layout: bool,
    #[schema(value_type = Object)]
    pub extra: serde_json::Value,
    pub updated_at: String,
    pub version: i32,
}

impl From<&SettingsRow> for SettingsView {
    fn from(s: &SettingsRow) -> Self {
        Self {
            flash_retention: (s.flash_retention * 100.0).round() / 100.0,
            flash_new_per_day: s.flash_new_per_day,
            flash_review_limit: s.flash_review_limit,
            mock_go_easy: s.mock_go_easy,
            mock_real_layout: s.mock_real_layout,
            extra: s.extra.as_ref().map(|j| j.0.clone()).unwrap_or_else(|| serde_json::json!({})),
            updated_at: iso(s.updated_at),
            version: s.row_version,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TierView {
    pub code: String,
    pub name: String,
    pub storage_quota_bytes: i64,
    pub max_courses: Option<i32>,
    #[schema(value_type = Option<Object>)]
    pub limits: Option<serde_json::Value>,
    pub is_default: bool,
    pub is_active: bool,
    pub sort_order: i32,
}

impl From<&TierRow> for TierView {
    fn from(t: &TierRow) -> Self {
        Self {
            code: t.code.clone(),
            name: t.name.clone(),
            storage_quota_bytes: t.storage_quota_bytes,
            max_courses: t.max_courses,
            limits: t.limits.as_ref().map(|j| j.0.clone()),
            is_default: t.is_default,
            is_active: t.is_active,
            sort_order: t.sort_order,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InviteView {
    pub id: String,
    pub email: Option<String>,
    pub tier: Option<String>,
    pub max_uses: i32,
    pub uses: i32,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

impl From<&InviteRow> for InviteView {
    fn from(i: &InviteRow) -> Self {
        Self {
            id: i.public_id.clone().unwrap_or_default(),
            email: i.email.clone(),
            tier: i.tier_code.clone(),
            max_uses: i.max_uses,
            uses: i.uses,
            expires_at: iso_opt(i.expires_at),
            created_at: iso(i.created_at),
            revoked_at: iso_opt(i.revoked_at),
        }
    }
}

/// Returned once, at creation: the only time the plain code exists outside the email/link.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatedInvite {
    #[serde(flatten)]
    pub invite: InviteView,
    pub code: String,
    pub signup_url: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Page<T: ToSchema> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}
