//! Error responses: `{"error":{"code","message","details","requestId"}}` (spec 3.1).

use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use crux_service::AppError;
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

use crate::reqlog::current_request_id;

#[derive(Debug)]
pub struct ApiError(pub AppError);

impl<E: Into<AppError>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError(e.into())
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDetail {
    /// One of the codes in backend-spec 3.1.
    pub code: String,
    pub message: String,
    #[schema(value_type = Option<Object>)]
    pub details: Option<serde_json::Value>,
    pub request_id: Option<String>,
}

/// Status and code for each error (spec 3.1 table).
pub fn status_and_code(e: &AppError) -> (StatusCode, &'static str) {
    match e {
        AppError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed"),
        AppError::Unauthenticated => (StatusCode::UNAUTHORIZED, "unauthenticated"),
        AppError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        AppError::Conflict { .. } => (StatusCode::CONFLICT, "conflict"),
        AppError::InviteInvalid { .. } => (StatusCode::FORBIDDEN, "invite_invalid"),
        AppError::EmailUnverified => (StatusCode::FORBIDDEN, "email_unverified"),
        AppError::RateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
        AppError::AiDisabled { .. } => (StatusCode::FORBIDDEN, "ai_disabled"),
        AppError::MockTimeOver => (StatusCode::CONFLICT, "mock_time_over"),
        AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = status_and_code(&self.0);
        let request_id = current_request_id();
        let (message, details) = match &self.0 {
            AppError::Validation(errors) => ("Some fields need attention.".to_string(), Some(json!({ "fields": errors.0 }))),
            AppError::Unauthenticated => ("Sign in to continue.".into(), None),
            AppError::NotFound => ("Not found.".into(), None),
            AppError::Conflict { message, current } => (message.clone(), current.as_ref().map(|c| json!({ "current": c }))),
            AppError::InviteInvalid { reason } => ("This invite can't be used.".into(), Some(json!({ "reason": reason }))),
            AppError::EmailUnverified => ("Verify your email address first.".into(), None),
            AppError::RateLimited { retry_after_secs } => {
                ("Too many attempts. Try again later.".into(), Some(json!({ "retryAfterSecs": retry_after_secs })))
            }
            AppError::AiDisabled { reason } => ("AI is off here.".into(), Some(json!({ "reason": reason }))),
            AppError::MockTimeOver => ("Time is over for this mock.".into(), None),
            AppError::Internal(e) => {
                tracing::error!(error = %e, request_id = request_id.as_deref().unwrap_or(""), "internal error");
                ("Something went wrong.".into(), None)
            }
        };
        let body = ErrorBody { error: ErrorDetail { code: code.into(), message, details, request_id } };
        let mut resp = (status, axum::Json(body)).into_response();
        if let AppError::RateLimited { retry_after_secs } = &self.0 {
            if let Ok(v) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                resp.headers_mut().insert(axum::http::header::RETRY_AFTER, v);
            }
        }
        resp
    }
}
