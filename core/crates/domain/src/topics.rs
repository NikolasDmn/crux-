//! Event topics core produces (backend-spec 3.3). Payload schemas live in `contracts/events/`.

pub const USER_CREATED: &str = "user.created";
pub const USER_UPDATED: &str = "user.updated";
pub const USER_DELETED: &str = "user.deleted";
pub const USER_TIER_CHANGED: &str = "user.tier_changed";
pub const NOTIFY_REQUESTED: &str = "notify.requested";

/// Every topic core publishes, for the contract tests.
pub const PRODUCED: &[&str] = &[USER_CREATED, USER_UPDATED, USER_DELETED, USER_TIER_CHANGED, NOTIFY_REQUESTED];
