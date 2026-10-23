//! `/internal/v1/*` (spec 3.2): only on the internal Docker network; the gateway answers 404.
//! Listed in core's OpenAPI under the tag `internal` in a later milestone, once the routes that
//! carry data (access checks, apply) exist.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::ApiResult;
use crate::extract::{bearer, JsonBody};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/internal/v1/jwks", get(jwks)).route("/internal/v1/service-token", post(service_token))
}

/// Public keys for verifying access and service tokens. Public data; cache for a few minutes.
async fn jwks(State(state): State<AppState>) -> ([(axum::http::HeaderName, &'static str); 1], Json<serde_json::Value>) {
    ([(axum::http::header::CACHE_CONTROL, "public, max-age=300")], Json(state.ctx.keys.jwks()))
}

#[derive(Deserialize)]
struct ServiceTokenInput {
    /// The calling service.
    service: String,
    /// The service it wants to call.
    audience: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceTokenOutput {
    token: String,
    expires_in: i64,
}

async fn service_token(
    State(state): State<AppState>,
    req: axum::http::request::Parts,
    JsonBody(input): JsonBody<ServiceTokenInput>,
) -> ApiResult<(StatusCode, Json<ServiceTokenOutput>)> {
    let secret = bearer(&req).unwrap_or("");
    let token = crux_service::auth::issue_service_token(&state.ctx, secret, &input.service, &input.audience)?;
    Ok((StatusCode::OK, Json(ServiceTokenOutput { token, expires_in: crux_service::jwt::SERVICE_TOKEN_TTL_SECS })))
}
