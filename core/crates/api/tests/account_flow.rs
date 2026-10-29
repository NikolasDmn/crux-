//! Milestone 9: account export and deletion (spec 11, 9.4).

mod common;

use axum::http::StatusCode;
use chrono::Duration;
use crux_clients::bus::{Delivery, Envelope};
use crux_jobs::consumer::process;
use crux_testkit::{call, outbox_topics, signup};
use serde_json::json;

fn purged(n: u32, user_ref: &str, service: &str) -> Delivery {
    Delivery {
        stream_id: format!("{n}-0"),
        topic: "user.purged".into(),
        envelope: Envelope {
            event_id: format!("01PURGED00000000000000000{n}"),
            topic: "user.purged".into(),
            schema_version: 1,
            occurred_at: "2026-11-10T00:00:00.000Z".into(),
            producer: service.into(),
            aggregate_ref: None,
            payload: json!({ "userRef": user_ref, "service": service }),
        },
    }
}

async fn users_with(h: &common::Harness, public_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM Users WHERE PublicId = ?").bind(public_id).fetch_one(&h.db.pool).await.unwrap()
}

#[tokio::test]
async fn export_then_delete_with_course_transfer() {
    let Some(h) = common::harness().await else { return };
    let (t, me, _) = signup(&h.app, &h.t.ctx, "dana").await;
    let me = me["id"].as_str().unwrap().to_string();
    let (bt, bob, _) = signup(&h.app, &h.t.ctx, "bob").await;
    let bob = bob["id"].as_str().unwrap().to_string();
    let shared = call(&h.app, "POST", "/courses", Some(&t), Some(json!({"name": "Shared", "code": "SH"})), &[]).await.json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let solo = call(&h.app, "POST", "/courses", Some(&t), Some(json!({"name": "Solo", "code": "SO"})), &[]).await.json["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(&h.app, "POST", &format!("/courses/{shared}/members"), Some(&t), Some(json!({"usernameOrEmail": "bob", "role": "Editor"})), &[])
        .await;
    call(&h.app, "POST", &format!("/course-invitations/{shared}/accept"), Some(&bt), None, &[]).await;
    call(&h.app, "POST", "/slips", Some(&t), Some(json!({"courseId": solo, "type": "Sign", "description": "minus"})), &[]).await;

    // Export: queued, built by the job, parts from every service, no secrets.
    let e = call(&h.app, "POST", "/account/export", Some(&t), None, &[]).await;
    assert_eq!((e.status, e.json["status"].as_str()), (StatusCode::ACCEPTED, Some("Queued")));
    assert_eq!(call(&h.app, "POST", "/account/export", Some(&t), None, &[]).await.json["id"], e.json["id"], "one at a time");
    *h.t.exports.ai_down.lock().unwrap() = true;
    assert_eq!(crux_service::account::run_exports(&h.t.ctx).await.unwrap(), 1);
    let list = call(&h.app, "GET", "/account/exports", Some(&t), None, &[]).await.json;
    assert_eq!(list[0]["status"], "Ready", "{list}");
    assert!(list[0]["fileId"].is_string() && list[0]["expiresAt"].is_string());
    let (_, parts) = h.t.exports.built.lock().unwrap()[0].clone();
    assert_eq!(parts["core"]["Users"][0]["Username"], "dana");
    assert!(parts["core"]["Users"][0].get("PasswordHash").is_none(), "never secrets");
    assert!(parts["core"].get("UserSession").is_none());
    assert_eq!(parts["core"]["Slip"].as_array().unwrap().len(), 1);
    assert_eq!(parts["core"]["Course"].as_array().unwrap().len(), 2);
    assert_eq!(parts["ai"]["unavailable"], true, "ai off: the export says so");
    assert!(outbox_topics(&h.db.pool).await.contains(&"notify.requested".to_string()));

    // Deletion needs the password and a decision for the shared course.
    let r = call(&h.app, "GET", "/account/deletion", Some(&t), None, &[]).await.json;
    assert_eq!((r["status"].as_str(), r["sharedCourses"][0]["members"][0]["id"].as_str()), (Some("None"), Some(bob.as_str())));
    let bad = call(&h.app, "POST", "/account/delete", Some(&t), Some(json!({"password": "nope nope nope"})), &[]).await;
    assert_eq!(bad.json["error"]["details"]["fields"][0]["code"], "wrong");
    let missing = call(&h.app, "POST", "/account/delete", Some(&t), Some(json!({"password": "correct horse battery"})), &[]).await;
    assert_eq!(missing.json["error"]["details"]["fields"][0]["code"], "required");
    let body = json!({"password": "correct horse battery", "courses": [{"courseId": shared, "action": "transfer", "toUserId": bob}]});
    let ok = call(&h.app, "POST", "/account/delete", Some(&t), Some(body.clone()), &[]).await;
    assert_eq!(ok.json["status"], "Pending", "{}", ok.json);
    assert_eq!(call(&h.app, "GET", "/me", Some(&t), None, &[]).await.status, StatusCode::OK, "can still sign in to cancel");
    let c = call(&h.app, "POST", "/account/delete/cancel", Some(&t), None, &[]).await;
    assert_eq!(c.json["status"], "Cancelled");
    call(&h.app, "POST", "/account/delete", Some(&t), Some(body), &[]).await;

    // 14 days later: the job transfers / deletes, emits user.deleted; acks complete it.
    h.t.clock.advance(Duration::days(13));
    assert_eq!(crux_service::account::run_deletions(&h.t.ctx).await.unwrap(), 0, "grace period");
    h.t.clock.advance(Duration::days(2));
    assert_eq!(crux_service::account::run_deletions(&h.t.ctx).await.unwrap(), 1);
    let topics = outbox_topics(&h.db.pool).await;
    assert!(topics.contains(&"user.deleted".to_string()) && topics.contains(&"course.deleted".to_string()));
    let handler = crux_jobs::files::FileEvents;
    process(&h.db.pool, &handler, &purged(1, &me, "files")).await.unwrap();
    process(&h.db.pool, &handler, &purged(2, &me, "notify")).await.unwrap();
    assert_eq!(users_with(&h, &me).await, 1, "waits for every service");
    process(&h.db.pool, &handler, &purged(3, &me, "ai")).await.unwrap();
    assert_eq!(users_with(&h, &me).await, 0);
    let login = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "bob", "password": "correct horse battery"})), &[]).await;
    let bt = login.json["accessToken"].as_str().unwrap().to_string();
    let c = call(&h.app, "GET", &format!("/courses/{shared}"), Some(&bt), None, &[]).await.json;
    assert_eq!((c["myRole"].as_str(), c["owner"]["id"].as_str()), (Some("Owner"), Some(bob.as_str())), "{c}");
    let status: String = sqlx::query_scalar("SELECT CAST(Status AS CHAR) FROM AccountDeletion WHERE UserRef = ?")
        .bind(&me)
        .fetch_one(&h.db.pool)
        .await
        .unwrap();
    assert_eq!(status, "Done");
    h.db.drop_db().await;
}

#[tokio::test]
async fn deletion_rechecks_transfers_locks_the_account_and_times_out() {
    let Some(h) = common::harness().await else { return };
    let (t, me, _) = signup(&h.app, &h.t.ctx, "dana").await;
    let me = me["id"].as_str().unwrap().to_string();
    let (bt, bob, _) = signup(&h.app, &h.t.ctx, "bob").await;
    let bob = bob["id"].as_str().unwrap().to_string();
    let shared = call(&h.app, "POST", "/courses", Some(&t), Some(json!({"name": "Shared", "code": "SH"})), &[]).await.json["id"]
        .as_str()
        .unwrap()
        .to_string();
    call(&h.app, "POST", &format!("/courses/{shared}/members"), Some(&t), Some(json!({"usernameOrEmail": "bob", "role": "Editor"})), &[])
        .await;
    call(&h.app, "POST", &format!("/course-invitations/{shared}/accept"), Some(&bt), None, &[]).await;
    let body = json!({"password": "correct horse battery", "courses": [{"courseId": shared, "action": "transfer", "toUserId": bob}]});
    assert_eq!(call(&h.app, "POST", "/account/delete", Some(&t), Some(body), &[]).await.json["status"], "Pending");
    // Bob leaves during the grace period: the course goes with Dana's account instead.
    let left = call(&h.app, "DELETE", &format!("/courses/{shared}/members/{bob}"), Some(&bt), None, &[]).await;
    assert_eq!(left.status, StatusCode::NO_CONTENT, "{}", left.json);

    h.t.clock.advance(Duration::days(15));
    assert_eq!(crux_service::account::run_deletions(&h.t.ctx).await.unwrap(), 1);
    let again = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "bob", "password": "correct horse battery"})), &[]).await;
    let bt = again.json["accessToken"].as_str().unwrap().to_string();
    assert_eq!(call(&h.app, "GET", &format!("/courses/{shared}"), Some(&bt), None, &[]).await.status, StatusCode::NOT_FOUND);
    // Audit M3: while the services purge, the account can't be used.
    assert_eq!(call(&h.app, "GET", "/me", Some(&t), None, &[]).await.status, StatusCode::UNAUTHORIZED);
    let login = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "dana", "password": "correct horse battery"})), &[]).await;
    assert_ne!(login.status, StatusCode::OK);

    // ai isn't deployed: it never acks, and the deletion finishes after the timeout.
    let handler = crux_jobs::files::FileEvents;
    process(&h.db.pool, &handler, &purged(1, &me, "files")).await.unwrap();
    process(&h.db.pool, &handler, &purged(2, &me, "notify")).await.unwrap();
    h.t.clock.advance(Duration::days(2));
    crux_service::account::run_deletions(&h.t.ctx).await.unwrap();
    assert_eq!(users_with(&h, &me).await, 1, "still waiting");
    h.t.clock.advance(Duration::days(2));
    assert_eq!(crux_service::account::run_deletions(&h.t.ctx).await.unwrap(), 1);
    assert_eq!(users_with(&h, &me).await, 0);
    h.db.drop_db().await;
}

/// Audit M4: every core table is exported, skipped as a secret store, or declared not personal.
#[tokio::test]
async fn export_covers_every_table() {
    let Some(h) = common::harness().await else { return };
    let (_, me, _) = signup(&h.app, &h.t.ctx, "dana").await;
    let id: i64 =
        sqlx::query_scalar("SELECT Id FROM Users WHERE PublicId = ?").bind(me["id"].as_str().unwrap()).fetch_one(&h.db.pool).await.unwrap();
    let mut conn = h.db.pool.acquire().await.unwrap();
    let part = crux_service::account::core_part(&mut conn, id).await.unwrap();
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT CAST(TABLE_NAME AS CHAR) FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE()")
            .fetch_all(&mut *conn)
            .await
            .unwrap();
    let missing: Vec<&String> = tables
        .iter()
        .filter(|t| !t.starts_with('_'))
        .filter(|t| part.get(t.as_str()).is_none())
        .filter(|t| !crux_repo::account::SKIP_TABLES.contains(&t.as_str()))
        .filter(|t| !crux_service::account::NOT_PERSONAL.contains(&t.as_str()))
        .collect();
    assert!(missing.is_empty(), "export these, or list them as not personal: {missing:?}");
    drop(conn);
    h.db.drop_db().await;
}
