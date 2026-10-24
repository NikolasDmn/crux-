//! Deadlock retry: InnoDB rolls the victim's transaction back and expects the client to run it
//! again (MySQL 1213). Each use case is one transaction (spec 2.1) and cross-service effects go
//! through the outbox inside it, so the whole request is replayed, up to 3 attempts.
//!
//! Sits inside the idempotency layer, so a stored response is the final one.

use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http::StatusCode;

/// Set on a response whose error was a deadlock (see `error.rs`).
#[derive(Clone, Copy)]
pub struct Deadlocked;

const ATTEMPTS: u32 = 3;
/// Matches axum's default `Json` body limit; core takes no larger bodies.
const MAX_BODY: usize = 2 * 1024 * 1024;

pub async fn layer(req: Request, next: Next) -> Response {
    let (parts, body) = req.into_parts();
    let bytes: Bytes = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(b) => b,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let mut attempt = 1;
    loop {
        let mut again = Request::builder().method(parts.method.clone()).uri(parts.uri.clone()).version(parts.version);
        *again.headers_mut().expect("fresh builder") = parts.headers.clone();
        *again.extensions_mut().expect("fresh builder") = parts.extensions.clone();
        let resp = next.clone().run(again.body(Body::from(bytes.clone())).expect("parts from a valid request")).await;
        if resp.extensions().get::<Deadlocked>().is_none() || attempt == ATTEMPTS {
            if attempt > 1 {
                tracing::warn!(attempt, status = resp.status().as_u16(), "deadlock retry finished");
            }
            return resp;
        }
        // Jittered back-off (10-40 ms, then 20-80 ms) so the two victims don't collide again.
        let jitter = u64::from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() % 30);
        tokio::time::sleep(Duration::from_millis((10 + jitter) * u64::from(attempt))).await;
        attempt += 1;
    }
}
