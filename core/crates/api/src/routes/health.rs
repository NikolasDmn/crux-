//! `/healthz` (liveness) and `/readyz` (DB + Redis) per spec 12. Not part of the public API doc.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/healthz", get(healthz)).route("/readyz", get(readyz))
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let db = crux_repo::db::ping(&state.ctx.pool).await.is_ok();
    let redis = state.ctx.kv.ping().await.is_ok();
    let status = if db && redis { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(json!({ "db": db, "redis": redis })))
}
