//! One error type for the service layer; the api crate maps it to `{"error":{code,message,details}}`
//! (spec 3.1).

use crux_domain::ValidationErrors;
use crux_repo::RepoError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("validation failed")]
    Validation(ValidationErrors),
    #[error("not signed in")]
    Unauthenticated,
    /// Also for "exists but not yours" (spec 4.2).
    #[error("not found")]
    NotFound,
    /// `current` is the resource as it is now, for the "changed somewhere else" dialog.
    #[error("{message}")]
    Conflict { message: String, current: Option<serde_json::Value> },
    #[error("invite invalid")]
    InviteInvalid { reason: &'static str },
    /// Not in the spec's code table; see OPEN-QUESTIONS.md.
    #[error("email not verified")]
    EmailUnverified,
    #[error("rate limited")]
    RateLimited { retry_after_secs: u64 },
    /// Spec 3.1: `details.reason` ∈ user_off, feature_off, course_off, material_blocked, no_key, service_unavailable.
    #[error("AI is off here")]
    AiDisabled { reason: &'static str },
    #[error("time is over")]
    MockTimeOver,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn conflict(message: &str) -> Self {
        AppError::Conflict { message: message.into(), current: None }
    }

    /// InnoDB picked this transaction as a deadlock victim (1213, SQLSTATE 40001) and rolled it
    /// back, so running the use case again is safe.
    pub fn is_deadlock(&self) -> bool {
        let AppError::Internal(e) = self else { return false };
        e.chain().any(|c| match c.downcast_ref::<crux_repo::sqlx_reexport::Error>() {
            Some(crux_repo::sqlx_reexport::Error::Database(db)) => db.code().as_deref() == Some("40001"),
            _ => false,
        })
    }
}

impl From<ValidationErrors> for AppError {
    fn from(e: ValidationErrors) -> Self {
        AppError::Validation(e)
    }
}

impl From<RepoError> for AppError {
    fn from(e: RepoError) -> Self {
        match e {
            RepoError::VersionConflict => AppError::conflict("Changed somewhere else."),
            RepoError::Duplicate(key) => AppError::conflict(&format!("Duplicate value ({key}).")),
            RepoError::Db(db) => AppError::Internal(db.into()),
        }
    }
}

impl From<crux_repo::sqlx_reexport::Error> for AppError {
    fn from(e: crux_repo::sqlx_reexport::Error) -> Self {
        AppError::Internal(e.into())
    }
}

impl From<crux_clients::ClientError> for AppError {
    fn from(e: crux_clients::ClientError) -> Self {
        AppError::Internal(anyhow::anyhow!(e))
    }
}
