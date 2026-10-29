//! `Idempotency-Key` on create endpoints (spec 3.1): stored 24 h in Redis per user + route. A
//! repeat returns the first response (status, body and its Location / ETag); a repeat while the
//! first is still running is a 409. Audit L3: the key is bound to the request body (another body
//! is a 422), a crashed request frees its key after a minute, and a response too large to store
//! still marks the key used, so a retry never creates a duplicate.

use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use crux_domain::auth::token_hash;
use crux_service::AppError;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::AppState;

const TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// While the first request runs: longer than any handler, and a crash frees the key after it.
const PENDING_TTL: Duration = Duration::from_secs(60);
const PENDING: &str = "pending:";
const MAX_STORED_BODY: usize = 1024 * 1024;
const MAX_REQUEST_BODY: usize = 8 * 1024 * 1024;
/// Response headers a replay repeats.
const KEPT_HEADERS: [&str; 3] = ["location", "etag", "content-type"];

#[derive(Serialize, Deserialize)]
struct Stored {
    status: u16,
    /// None when the response was too large to keep (a replay is then a 409).
    body: Option<String>,
    #[serde(default)]
    headers: Vec<(String, String)>,
    /// Hash of the request body the key was first used with.
    #[serde(default)]
    request: String,
}

pub async fn layer(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if req.method() != Method::POST {
        return next.run(req).await;
    }
    let Some(key) = req.headers().get("idempotency-key").and_then(|v| v.to_str().ok()).map(str::to_string) else {
        return next.run(req).await;
    };
    // Scope by the bearer's subject without a DB hit; anonymous calls aren't idempotent-keyed.
    let now = state.ctx.clock.now().timestamp();
    let sub = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|t| state.ctx.keys.verify_access(t.trim(), now).ok())
        .map(|c| c.sub);
    let Some(sub) = sub else {
        return next.run(req).await;
    };
    if key.is_empty() || key.len() > 128 {
        return ApiError(crux_domain::ValidationErrors::single("Idempotency-Key", "invalid", "Use 1-128 characters.").into())
            .into_response();
    }
    let redis_key = format!("crux:idem:{sub}:{}", &token_hash(&format!("{} {}|{key}", req.method(), req.uri().path()))[..40]);

    // The key belongs to this exact request: hash the body, then hand it on to the handler.
    let (req_parts, req_body) = req.into_parts();
    let Ok(req_bytes) = to_bytes(req_body, MAX_REQUEST_BODY).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let request = token_hash(&String::from_utf8_lossy(&req_bytes));
    let req = Request::from_parts(req_parts, Body::from(req_bytes));

    match state.ctx.kv.set_nx(&redis_key, &format!("{PENDING}{request}"), PENDING_TTL).await {
        Ok(true) => {}
        Ok(false) => return replay(&state, &redis_key, &request).await,
        Err(e) => return ApiError(AppError::from(e)).into_response(),
    }

    let resp = next.run(req).await;
    if !resp.status().is_success() {
        let _ = state.ctx.kv.del(&redis_key).await;
        return resp;
    }
    let (parts, body) = resp.into_parts();
    let bytes = match to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(_) => {
            let _ = state.ctx.kv.del(&redis_key).await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let headers = KEPT_HEADERS
        .iter()
        .filter_map(|h| parts.headers.get(*h).and_then(|v| v.to_str().ok()).map(|v| (h.to_string(), v.to_string())))
        .collect();
    let body = (bytes.len() <= MAX_STORED_BODY).then(|| String::from_utf8_lossy(&bytes).into_owned());
    let stored = Stored { status: parts.status.as_u16(), body, headers, request };
    if let Ok(json) = serde_json::to_string(&stored) {
        let _ = state.ctx.kv.set(&redis_key, &json, TTL).await;
    }
    Response::from_parts(parts, Body::from(bytes))
}

fn reused() -> Response {
    ApiError(crux_domain::ValidationErrors::single("Idempotency-Key", "reused", "This key was used for a different request.").into())
        .into_response()
}

async fn replay(state: &AppState, redis_key: &str, request: &str) -> Response {
    match state.ctx.kv.get(redis_key).await {
        Ok(Some(v)) if v.starts_with(PENDING) => {
            if v != format!("{PENDING}{request}") {
                return reused();
            }
            ApiError(AppError::conflict("The same request is still being processed.")).into_response()
        }
        Ok(Some(v)) => match serde_json::from_str::<Stored>(&v) {
            Ok(stored) if !stored.request.is_empty() && stored.request != request => reused(),
            Ok(Stored { body: None, .. }) => {
                ApiError(AppError::conflict("Already done; the response was too large to repeat.")).into_response()
            }
            Ok(stored) => {
                let mut resp = Response::new(Body::from(stored.body.unwrap_or_default()));
                *resp.status_mut() = StatusCode::from_u16(stored.status).unwrap_or(StatusCode::OK);
                resp.headers_mut().insert(axum::http::header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
                for (k, v) in &stored.headers {
                    if let (Ok(name), Ok(value)) = (HeaderName::try_from(k.as_str()), HeaderValue::from_str(v)) {
                        resp.headers_mut().insert(name, value);
                    }
                }
                resp.headers_mut().insert("idempotent-replay", HeaderValue::from_static("true"));
                resp
            }
            Err(_) => ApiError(AppError::conflict("Duplicate request.")).into_response(),
        },
        // Gone in between (the first failed, or a crashed request's pending marker expired).
        Ok(None) => ApiError(AppError::conflict("Try again.")).into_response(),
        Err(e) => ApiError(AppError::from(e)).into_response(),
    }
}
