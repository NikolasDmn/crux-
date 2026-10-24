//! `/internal/v1/*` (spec 3.2): only on the internal Docker network; the gateway answers 404.
//! Listed in core's OpenAPI under the tag `internal` in a later milestone, once the routes that
//! carry data (access checks, apply) exist.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::ApiResult;
use crate::extract::{bearer, JsonBody, Service};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/internal/v1/jwks", get(jwks))
        .route("/internal/v1/service-token", post(service_token))
        .route("/internal/v1/access", get(access))
        .route("/internal/v1/access/file-ref", get(file_ref_access))
        .route("/internal/v1/ai/materials", get(ai_materials))
        .route("/internal/v1/context/user", post(ai_context))
        .route("/internal/v1/mocks/{id}/open", get(mock_open))
}

#[derive(Deserialize)]
struct AccessQuery {
    user: String,
    course: String,
}

/// Spec 4.2: the other services ask core for a user's role on a course (cache ≤ 30 s).
async fn access(
    State(state): State<AppState>,
    _svc: Service,
    Query(q): Query<AccessQuery>,
) -> ApiResult<Json<crux_service::catalog::AccessAnswer>> {
    Ok(Json(crux_service::catalog::access(&state.ctx, &q.user, &q.course).await?))
}

#[derive(Deserialize)]
struct FileRefQuery {
    user: String,
    #[serde(rename = "ref")]
    file_ref: String,
}

/// Spec 6.1 step 7: may this user download the file because a course they read references it?
async fn file_ref_access(
    State(state): State<AppState>,
    _svc: Service,
    Query(q): Query<FileRefQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let allowed = crux_service::catalog::file_ref_access(&state.ctx, &q.user, &q.file_ref).await?;
    Ok(Json(serde_json::json!({ "allowed": allowed })))
}

#[derive(Deserialize)]
struct AiMaterialsQuery {
    user: String,
    refs: String,
}

/// Spec 7.1 gate: AiAllowed / excluded / file of each material, for materials the user can read.
async fn ai_materials(
    State(state): State<AppState>,
    _svc: Service,
    Query(q): Query<AiMaterialsQuery>,
) -> ApiResult<Json<Vec<crux_service::ai::AiMaterial>>> {
    let refs: Vec<String> = q.refs.split(',').filter(|r| !r.is_empty()).map(String::from).collect();
    Ok(Json(crux_service::ai::materials(&state.ctx, &q.user, &refs).await?))
}

/// Spec 7.4 block 6: what the AI context builder may put in a prompt for this user and course.
async fn ai_context(
    State(state): State<AppState>,
    _svc: Service,
    JsonBody(req): JsonBody<crux_service::ai::ContextRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(crux_service::ai::context(&state.ctx, req).await?))
}

#[derive(Deserialize)]
struct UserQuery {
    user: String,
}

/// Spec 15 `open_mock`: may this user open the mock? (404 if not.)
async fn mock_open(
    State(state): State<AppState>,
    _svc: Service,
    axum::extract::Path(id): axum::extract::Path<String>,
    Query(q): Query<UserQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let status = crux_service::mocks::open_for(&state.ctx, &q.user, &id).await?;
    Ok(Json(serde_json::json!({ "status": status })))
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
