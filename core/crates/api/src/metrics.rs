//! Prometheus metrics (spec 12): request counts and latency per route, plus queue depths read at
//! scrape time. Served on `/internal/metrics` (the gateway never exposes `/internal/*`).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;

use crate::AppState;

#[derive(Default, Clone, Copy)]
struct Series {
    count: u64,
    sum_secs: f64,
}

static REQUESTS: Mutex<BTreeMap<(String, String, u16), Series>> = Mutex::new(BTreeMap::new());

/// Called by the request log layer for every request.
pub fn record(method: &str, route: &str, status: u16, elapsed: Duration) {
    let mut m = REQUESTS.lock().unwrap();
    let s = m.entry((method.to_string(), route.to_string(), status)).or_default();
    s.count += 1;
    s.sum_secs += elapsed.as_secs_f64();
}

fn esc(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

async fn gauge(pool: &crux_repo::MySqlPool, sql: &str) -> i64 {
    sqlx_scalar(pool, sql).await.unwrap_or(-1)
}

async fn sqlx_scalar(pool: &crux_repo::MySqlPool, sql: &str) -> anyhow::Result<i64> {
    let mut conn = pool.acquire().await?;
    Ok(crux_repo::db::scalar_i64(&mut conn, sql).await?)
}

pub async fn handler(State(s): State<AppState>) -> impl IntoResponse {
    let mut out = String::new();
    out.push_str("# TYPE crux_http_requests_total counter\n# TYPE crux_http_request_duration_seconds summary\n");
    for ((method, route, status), v) in REQUESTS.lock().unwrap().iter() {
        let labels = format!("service=\"core\",method=\"{}\",route=\"{}\",status=\"{status}\"", esc(method), esc(route));
        out.push_str(&format!("crux_http_requests_total{{{labels}}} {}\n", v.count));
        out.push_str(&format!("crux_http_request_duration_seconds_sum{{{labels}}} {:.6}\n", v.sum_secs));
        out.push_str(&format!("crux_http_request_duration_seconds_count{{{labels}}} {}\n", v.count));
    }
    let gauges = [
        ("crux_outbox_unpublished", "SELECT COUNT(*) FROM Outbox WHERE PublishedAt IS NULL"),
        ("crux_exports_queued", "SELECT COUNT(*) FROM AccountExport WHERE Status IN ('Queued','Building')"),
        ("crux_deletions_pending", "SELECT COUNT(*) FROM AccountDeletion WHERE Status IN ('Pending','Purging')"),
        ("crux_users_total", "SELECT COUNT(*) FROM Users"),
    ];
    for (name, sql) in gauges {
        out.push_str(&format!("# TYPE {name} gauge\n{name}{{service=\"core\"}} {}\n", gauge(&s.ctx.pool, sql).await));
    }
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], out)
}
