//! Integration-test helpers (spec 13: integration tests on real MySQL).
//!
//! Set `CRUX_TEST_MYSQL_URL=mysql://crux_test:crux-test@127.0.0.1:3306` (the compose MySQL has
//! that user). Each test gets its own `crux_test_core_<n>` database, migrated from empty, and drops
//! it at the end. Without the variable, DB tests print "skipped" and pass, so `cargo test` works
//! on a laptop with no Docker; CI always sets it.

use std::collections::HashSet;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use chrono::{TimeZone, Utc};
use crux_clients::clock::ManualClock;
use crux_clients::files::MemoryFiles;
use crux_clients::ids::UlidGen;
use crux_clients::kv::MemoryKv;
use crux_repo::MySqlPool;
use crux_service::ctx::Config;
use crux_service::jwt::JwtKeys;
use crux_service::Ctx;
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::Executor;
use tower::ServiceExt;

pub const ENV: &str = "CRUX_TEST_MYSQL_URL";

/// Returns `None` (and says why) when no test database is configured.
pub fn test_mysql_url() -> Option<String> {
    match std::env::var(ENV) {
        Ok(url) if !url.is_empty() => Some(url),
        _ => {
            eprintln!("skipped: set {ENV} to run DB integration tests");
            None
        }
    }
}

pub struct TestDb {
    pub pool: MySqlPool,
    admin_url: String,
    name: String,
}

impl TestDb {
    /// `let Some(db) = TestDb::create().await else { return };`
    pub async fn create() -> Option<Self> {
        use crux_clients::ids::IdGen;
        let base = test_mysql_url()?;
        let name = format!("crux_test_core_{}", UlidGen::default().ulid().to_lowercase());
        let admin = crux_repo::db::connect(&base, 1).await.expect("connect to test MySQL");
        admin
            .execute(format!("CREATE DATABASE `{name}` CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci").as_str())
            .await
            .expect("create test database");
        admin.close().await;
        let pool = crux_repo::db::connect(&format!("{}/{name}", base.trim_end_matches('/')), 5).await.expect("connect test db");
        crux_repo::db::migrate(&pool).await.expect("migrations run from empty");
        Some(Self { pool, admin_url: base, name })
    }

    pub async fn drop_db(self) {
        self.pool.close().await;
        if let Ok(admin) = crux_repo::db::connect(&self.admin_url, 1).await {
            let _ = admin.execute(format!("DROP DATABASE `{}`", self.name).as_str()).await;
            admin.close().await;
        }
    }
}

pub struct TestCtx {
    pub ctx: Ctx,
    pub clock: Arc<ManualClock>,
    pub files: Arc<MemoryFiles>,
}

/// A Ctx on the test database with a hand-moved clock, in-memory Redis and a fresh signing key.
pub fn test_ctx(pool: MySqlPool) -> TestCtx {
    let clock = Arc::new(ManualClock::new(Utc.with_ymd_and_hms(2026, 10, 23, 12, 0, 0).unwrap()));
    let files = Arc::new(MemoryFiles::default());
    let breached: HashSet<String> = ["password123456".to_string()].into();
    let ctx = Ctx {
        pool,
        clock: clock.clone(),
        ids: Arc::new(UlidGen::default()),
        kv: Arc::new(MemoryKv::default()),
        keys: Arc::new(JwtKeys::from_pkcs8_pem(&JwtKeys::generate_pem()).unwrap()),
        files: files.clone(),
        breach_check: None,
        breached_passwords: Arc::new(breached),
        config: Arc::new(Config { app_base_url: "http://crux.test".into(), service_secret: "test-service-secret".into() }),
    };
    TestCtx { ctx, clock, files }
}

/// Creates an invite straight in the DB and returns its code.
pub async fn seed_invite(ctx: &Ctx, max_uses: i32) -> String {
    use crux_domain::auth::{normalize_invite_code, token_hash};
    let code = ctx.ids.invite_code();
    let mut conn = ctx.pool.acquire().await.unwrap();
    crux_repo::invites::insert(
        &mut conn,
        &crux_repo::invites::NewInvite {
            public_id: &ctx.ids.ulid(),
            code_hash: &token_hash(&normalize_invite_code(&code)),
            email: None,
            invited_by: None,
            tier_id: None,
            max_uses,
            expires_at: None,
        },
    )
    .await
    .unwrap();
    code
}

/// A response with its JSON body (Null when empty) and headers.
pub struct Resp {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub json: Value,
}

impl Resp {
    pub fn code(&self) -> &str {
        self.json["error"]["code"].as_str().unwrap_or("")
    }
}

/// One request through the router, no network.
pub async fn call(app: &Router, method: &str, path: &str, token: Option<&str>, body: Option<Value>, headers: &[(&str, &str)]) -> Resp {
    let mut req = Request::builder().method(method).uri(path).header("x-forwarded-for", "203.0.113.7");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())).unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    Resp { status, headers, json }
}

/// Signs up `name` with a fresh invite. Returns (access token, user JSON, refresh cookie value).
pub async fn signup(app: &Router, ctx: &Ctx, name: &str) -> (String, Value, String) {
    let code = seed_invite(ctx, 1).await;
    let r = call(
        app,
        "POST",
        "/auth/signup",
        None,
        Some(serde_json::json!({
            "inviteCode": code, "email": format!("{name}@example.com"), "username": name,
            "firstName": name, "lastName": "Test", "password": "correct horse battery", "timeZone": "Europe/Amsterdam"
        })),
        &[],
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "signup {name}: {}", r.json);
    let cookie = refresh_cookie(&r.headers).expect("refresh cookie set");
    (r.json["accessToken"].as_str().unwrap().to_string(), r.json["user"].clone(), cookie)
}

pub fn refresh_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|c| c.strip_prefix("crux_refresh=").map(|rest| rest.split(';').next().unwrap_or("").to_string()))
        .filter(|v| !v.is_empty())
}

/// Marks a user's email verified directly (the email round trip is tested on its own).
pub async fn verify_email(ctx: &Ctx, user_ref: &str) {
    sqlx::query("UPDATE Users SET EmailVerifiedAt = UTC_TIMESTAMP() WHERE PublicId = ?").bind(user_ref).execute(&ctx.pool).await.unwrap();
}

pub async fn make_admin(ctx: &Ctx, user_ref: &str) {
    sqlx::query("UPDATE Users SET Role = 'Admin' WHERE PublicId = ?").bind(user_ref).execute(&ctx.pool).await.unwrap();
}

/// Outbox topics in insert order.
pub async fn outbox_topics(pool: &MySqlPool) -> Vec<String> {
    sqlx::query_scalar("SELECT Topic FROM Outbox ORDER BY Id").fetch_all(pool).await.unwrap()
}
