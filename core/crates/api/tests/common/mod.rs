use std::sync::Arc;

use axum::Router;
use crux_api::state::Peers;
use crux_api::AppState;
use crux_testkit::{test_ctx, TestCtx, TestDb};

pub struct Harness {
    pub db: TestDb,
    pub t: TestCtx,
    pub app: Router,
}

/// `None` when no test MySQL is configured (the test then passes as skipped).
pub async fn harness() -> Option<Harness> {
    let db = TestDb::create().await?;
    let t = test_ctx(db.pool.clone());
    let app = crux_api::app(AppState { ctx: t.ctx.clone(), peers: Arc::new(Peers::default()), cookie_secure: false });
    Some(Harness { db, t, app })
}
