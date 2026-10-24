//! Event topics core produces (backend-spec 3.3). Payload schemas live in `contracts/events/`.

pub const USER_CREATED: &str = "user.created";
pub const USER_UPDATED: &str = "user.updated";
pub const USER_DELETED: &str = "user.deleted";
pub const USER_TIER_CHANGED: &str = "user.tier_changed";
pub const NOTIFY_REQUESTED: &str = "notify.requested";
pub const COURSE_CREATED: &str = "course.created";
pub const COURSE_UPDATED: &str = "course.updated";
pub const COURSE_ARCHIVED: &str = "course.archived";
pub const COURSE_DELETED: &str = "course.deleted";
pub const COURSE_MEMBER_CHANGED: &str = "course.member_changed";
pub const MATERIAL_CREATED: &str = "material.created";
pub const MATERIAL_UPDATED: &str = "material.updated";
pub const MATERIAL_PURGE_REQUESTED: &str = "material.purge_requested";
pub const MATERIAL_PURGE_CANCELLED: &str = "material.purge_cancelled";

/// Topics core consumes from files (storage saver, spec 6.2).
pub const FILE_PURGE_SCHEDULED: &str = "file.purge_scheduled";
pub const FILE_PURGED: &str = "file.purged";
pub const FILE_RESTORED: &str = "file.restored";

/// Every topic core publishes, for the contract tests.
pub const PRODUCED: &[&str] = &[
    USER_CREATED,
    USER_UPDATED,
    USER_DELETED,
    USER_TIER_CHANGED,
    NOTIFY_REQUESTED,
    COURSE_CREATED,
    COURSE_UPDATED,
    COURSE_ARCHIVED,
    COURSE_DELETED,
    COURSE_MEMBER_CHANGED,
    MATERIAL_CREATED,
    MATERIAL_UPDATED,
    MATERIAL_PURGE_REQUESTED,
    MATERIAL_PURGE_CANCELLED,
];
