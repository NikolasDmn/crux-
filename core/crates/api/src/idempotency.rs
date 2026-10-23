//! `Idempotency-Key` on create endpoints (spec 3.1): stored 24 h in Redis per user + route. A
//! repeat returns the first response; a repeat while the first is still running is a 409.

use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use crux_domain::auth::token_hash;
use crux_service::AppError;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::AppState;

const TTL: Duration = Duration::from_secs(24 * 60 * 60);
const PENDING: &str = "pending";
const MAX_STORED_BODY: usize = 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Stored {
    status: u16,
    body: String,
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
        return ApiError(crux_domain::ValidationErrors::single("Idempotency-Key", "invalid", "Use 1-128 characters.").into()).into_response();
    }
    let redis_key = format!("crux:idem:{sub}:{}", &token_hash(&format!("{} {}|{key}", req.method(), req.uri().path()))[..40]);

    match state.ctx.kv.set_nx(&redis_key, PENDING, TTL).await {
        Ok(true) => {}
        Ok(false) => return replay(&state, &redis_key).await,
        Err(e) => return ApiError(AppError::from(e)).into_response(),
    }

    let resp = next.run(req).await;
    if !resp.status().is_success() {
        let _ = state.ctx.kv.del(&redis_key).await;
        return resp;
    }
    let (parts, body) = resp.into_parts();
    let bytes = match to_bytes(body, MAX_STORED_BODY).await {
        Ok(b) => b,
        Err(_) => {
            let _ = state.ctx.kv.del(&redis_key).await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let stored = Stored { status: parts.status.as_u16(), body: String::from_utf8_lossy(&bytes).into_owned() };
    if let Ok(json) = serde_json::to_string(&stored) {
        let _ = state.ctx.kv.set(&redis_key, &json, TTL).await;
    }
    Response::from_parts(parts, Body::from(bytes))
}

async fn replay(state: &AppState, redis_key: &str) -> Response {
    match state.ctx.kv.get(redis_key).await {
        Ok(Some(v)) if v != PENDING => match serde_json::from_str::<Stored>(&v) {
            Ok(stored) => {
                let mut resp = Response::new(Body::from(stored.body));
                *resp.status_mut() = StatusCode::from_u16(stored.status).unwrap_or(StatusCode::OK);
                resp.headers_mut().insert(axum::http::header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
                resp.headers_mut().insert("idempotent-replay", HeaderValue::from_static("true"));
                resp
            }
            Err(_) => ApiError(AppError::conflict("Duplicate request.")).into_response(),
        },
        Ok(_) => ApiError(AppError::conflict("The same request is still being processed.")).into_response(),
        Err(e) => ApiError(AppError::from(e)).into_response(),
    }
}
