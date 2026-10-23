//! Request id + one structured log line per request (spec 12): request id, user PublicId, route,
//! latency, status. Never tokens, passwords, keys or bodies.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

pub const REQUEST_ID: &str = "x-request-id";

tokio::task_local! {
    static REQUEST_ID_SLOT: String;
}

/// Filled by the auth extractor so the log line can name the user.
#[derive(Clone, Default)]
pub struct LogUser(pub Arc<Mutex<Option<String>>>);

impl LogUser {
    pub fn set(&self, public_id: &str) {
        *self.0.lock().unwrap() = Some(public_id.to_string());
    }
}

pub fn current_request_id() -> Option<String> {
    REQUEST_ID_SLOT.try_with(|id| id.clone()).ok()
}

fn sanitize(id: &str) -> Option<String> {
    let ok = (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ok.then(|| id.to_string())
}

pub async fn layer(mut req: Request, next: Next) -> Response {
    let started = Instant::now();
    let id = req.headers().get(REQUEST_ID).and_then(|v| v.to_str().ok()).and_then(sanitize).unwrap_or_else(new_request_id);
    let route = req.extensions().get::<MatchedPath>().map(|p| p.as_str().to_string()).unwrap_or_else(|| "unmatched".into());
    let method = req.method().clone();
    let user = LogUser::default();
    req.extensions_mut().insert(user.clone());

    let mut resp = REQUEST_ID_SLOT.scope(id.clone(), next.run(req)).await;
    if let Ok(v) = HeaderValue::from_str(&id) {
        resp.headers_mut().insert(REQUEST_ID, v);
    }
    let status = resp.status().as_u16();
    let user = user.0.lock().unwrap().clone().unwrap_or_default();
    let latency_ms = started.elapsed().as_millis() as u64;
    if status >= 500 {
        tracing::error!(request_id = %id, user = %user, %method, route = %route, status, latency_ms, "request");
    } else if route != "/healthz" && route != "/readyz" {
        tracing::info!(request_id = %id, user = %user, %method, route = %route, status, latency_ms, "request");
    }
    resp
}

fn new_request_id() -> String {
    // Unique per process and roughly time-ordered: millisecond timestamp + counter, as hex.
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("{ms:013x}{:011x}", N.fetch_add(1, Ordering::Relaxed))
}
